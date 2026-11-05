//! A standard-tool byte witness, not an LLDB formatter. The SDK's dwarfdump
//! supplies the DIE graph; LLDB's plain memory-read command supplies the bytes.
//! No offsets, discriminant numbers, payload sizes or payload types are baked
//! into the witness. Only the source program and its oracle text are fixtures.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

fn pins() -> serde_json::Value {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../notes/s283-dwarf-oracles.json"
    )))
    .expect("oracle manifest")
}

#[derive(Clone, Debug)]
struct Die {
    offset: u64,
    parent: Option<u64>,
    tag: String,
    attrs: BTreeMap<String, String>,
}

fn parse(text: &str) -> Result<BTreeMap<u64, Die>, String> {
    let mut entries = BTreeMap::new();
    let mut stack: Vec<(usize, u64)> = Vec::new();
    for line in text.lines() {
        if let Some((left, right)) = line.split_once(':')
            && let Some(hex) = left.strip_prefix("0x")
            && let Ok(offset) = u64::from_str_radix(hex, 16)
        {
            let depth = right.len() - right.trim_start().len();
            while stack.last().is_some_and(|(old, _)| *old >= depth) {
                stack.pop();
            }
            let tag = right.trim();
            if tag == "NULL" {
                continue;
            }
            if !tag.starts_with("DW_TAG_") {
                return Err(format!("unknown DIE line: {line}"));
            }
            let die = Die {
                offset,
                parent: stack.last().map(|(_, id)| *id),
                tag: tag.to_owned(),
                attrs: BTreeMap::new(),
            };
            if entries.insert(offset, die).is_some() {
                return Err("duplicate DIE offset".into());
            }
            stack.push((depth, offset));
        } else if let Some(attr) = line.trim().strip_prefix("DW_AT_") {
            let (name, value) = attr.split_once('(').ok_or("attribute without value")?;
            let value = value
                .strip_suffix(')')
                .ok_or("unterminated attribute value")?;
            let id = stack.last().ok_or("attribute outside DIE")?.1;
            entries
                .get_mut(&id)
                .ok_or("missing attribute owner")?
                .attrs
                .insert(name.trim().to_owned(), value.to_owned());
        }
    }
    Ok(entries)
}

impl Die {
    fn attr(&self, key: &str) -> Result<&str, String> {
        self.attrs
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| format!("{} at {:#x} lacks DW_AT_{key}", self.tag, self.offset))
    }
    fn number(&self, key: &str) -> Result<u64, String> {
        let token = self
            .attr(key)?
            .split_whitespace()
            .next()
            .ok_or("empty integer")?;
        if let Some(hex) = token.strip_prefix("0x") {
            u64::from_str_radix(hex, 16).map_err(|e| e.to_string())
        } else {
            token.parse::<u64>().map_err(|e| e.to_string())
        }
    }
    fn name(&self) -> Option<&str> {
        self.attrs.get("name")?.strip_prefix('"')?.strip_suffix('"')
    }
}

struct Schema {
    tool: PathBuf,
    artifact: PathBuf,
    dir: PathBuf,
    dies: BTreeMap<u64, Die>,
}

fn output(cmd: &mut Command, dir: &Path, label: &str) -> Result<String, String> {
    let out = cmd.output().map_err(|e| format!("{cmd:?}: {e}"))?;
    std::fs::write(dir.join(format!("{label}.stdout.txt")), &out.stdout)
        .map_err(|e| e.to_string())?;
    std::fs::write(dir.join(format!("{label}.stderr.txt")), &out.stderr)
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "{label}: {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    String::from_utf8(out.stdout).map_err(|e| e.to_string())
}

impl Schema {
    fn load(&mut self, id: u64) -> Result<Die, String> {
        if let Some(die) = self.dies.get(&id) {
            return Ok(die.clone());
        }
        let text = output(
            Command::new(&self.tool)
                .arg(format!("--debug-info={id:#x}"))
                .arg("--show-children")
                .arg(&self.artifact),
            &self.dir,
            &format!("die-{id:x}"),
        )?;
        self.dies.extend(parse(&text)?);
        self.dies
            .get(&id)
            .cloned()
            .ok_or_else(|| format!("unresolved DIE reference {id:#x}"))
    }
    fn children(&self, id: u64, tag: &str) -> Vec<Die> {
        self.dies
            .values()
            .filter(|d| d.parent == Some(id) && d.tag == tag)
            .cloned()
            .collect()
    }
}

fn one(mut dies: Vec<Die>, what: &str) -> Result<Die, String> {
    if dies.len() != 1 {
        return Err(format!("expected one {what}, got {}", dies.len()));
    }
    Ok(dies.remove(0))
}

fn integer(bytes: &[u8], offset: u64, size: u64, signed: bool) -> Result<i128, String> {
    if size == 0 || size > 8 {
        return Err(format!("unsupported integer width {size}"));
    }
    let end = offset.checked_add(size).ok_or("field extent overflow")?;
    let data = bytes
        .get(
            usize::try_from(offset).map_err(|e| e.to_string())?
                ..usize::try_from(end).map_err(|e| e.to_string())?,
        )
        .ok_or("field lies outside variable bytes")?;
    let mut value = 0u64;
    for (i, byte) in data.iter().enumerate() {
        value |= u64::from(*byte) << (i * 8);
    }
    if signed && data.last().unwrap() & 0x80 != 0 {
        Ok(i128::from(value) - (1i128 << (size * 8)))
    } else {
        Ok(i128::from(value))
    }
}

fn active_value(schema: &mut Schema, root: &Die, bytes: &[u8]) -> Result<String, String> {
    if bytes.len() as u64 != root.number("byte_size")? {
        return Err("LLDB byte count disagrees with DWARF type size".into());
    }
    let part = one(
        schema.children(root.offset, "DW_TAG_variant_part"),
        "variant part",
    )?;
    let discr = schema.load(part.number("discr")?)?;
    if discr.parent != Some(part.offset) || discr.tag != "DW_TAG_member" {
        return Err("discriminant is not a member of its variant part".into());
    }
    let tag_type = schema.load(discr.number("type")?)?;
    let signed = match tag_type.attr("encoding")? {
        "DW_ATE_signed" => true,
        "DW_ATE_unsigned" => false,
        other => return Err(format!("unsupported discriminant encoding {other}")),
    };
    let tag = integer(
        bytes,
        discr.number("data_member_location")?,
        tag_type.number("byte_size")?,
        signed,
    )?;
    let selected: Vec<_> = schema
        .children(part.offset, "DW_TAG_variant")
        .into_iter()
        .filter(|v| {
            v.attr("discr_value")
                .ok()
                .and_then(|n| n.parse::<i128>().ok())
                == Some(tag)
        })
        .collect();
    let selected = one(selected, &format!("active variant for discriminator {tag}"))?;
    let view = one(
        schema.children(selected.offset, "DW_TAG_member"),
        "active payload view",
    )?;
    let name = view.name().ok_or("variant view has no name")?.to_owned();
    let payload = schema.load(view.number("type")?)?;
    let fields = schema.children(payload.offset, "DW_TAG_member");
    if fields.is_empty() {
        return Ok(format!("{name}\n"));
    }
    let field = one(fields, "active integer payload")?;
    let ty = schema.load(field.number("type")?)?;
    let signed = match ty.attr("encoding")? {
        "DW_ATE_signed" => true,
        "DW_ATE_unsigned" => false,
        other => return Err(format!("unsupported fixture payload encoding {other}")),
    };
    let offset = view
        .number("data_member_location")?
        .checked_add(field.number("data_member_location")?)
        .ok_or("payload offset overflow")?;
    let value = integer(bytes, offset, ty.number("byte_size")?, signed)?;
    Ok(format!("{name} {value}\n"))
}

/// A successful assertion of the presentation limitation emits KNOWN-GAP, not
/// PASS. A separately named byte row runs only when the paired oracle is given.
pub fn check(row: &str, exe: &Path, expected_type: &str) {
    let byte_row = format!("{row}-bytes");
    if let Err(error) = check_inner(&byte_row, exe, expected_type) {
        eprintln!("FAIL {byte_row}");
        panic!("{byte_row}: {error}");
    }
}

fn check_inner(row: &str, exe: &Path, expected_type: &str) -> Result<(), String> {
    let dir = exe.parent().ok_or("fixture has no directory")?;
    let tool = match std::env::var_os("LLVM_DWARFDUMP") {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(
            output(
                Command::new("xcrun").args(["--find", "dwarfdump"]),
                dir,
                "sdk-tool",
            )?
            .trim(),
        ),
    };
    let pin = pins();
    let version = output(Command::new(&tool).arg("--version"), dir, "sdk-version")?;
    if !version.contains(
        pin["macos_aarch64"]["sdk_dwarfdump_version"]
            .as_str()
            .ok_or("SDK version pin")?,
    ) {
        return Err(format!(
            "SDK dwarfdump differs from pinned oracle: {version}"
        ));
    }
    let version = output(Command::new("lldb").arg("--version"), dir, "lldb-version")?;
    if !version.contains(
        pin["macos_aarch64"]["lldb_version"]
            .as_str()
            .ok_or("LLDB version pin")?,
    ) {
        return Err(format!("LLDB differs from known-gap oracle: {version}"));
    }
    let artifact = exe
        .with_extension("dSYM")
        .join("Contents/Resources/DWARF")
        .join(exe.file_name().ok_or("fixture name")?);
    let text = output(
        Command::new(&tool)
            .args(["--debug-info", "--name=inspect", "--show-children"])
            .arg(&artifact),
        dir,
        "inspect-die",
    )?;
    let dies = parse(&text)?;
    let param = one(
        dies.values()
            .filter(|d| {
                d.tag == "DW_TAG_formal_parameter"
                    && d.name() == Some("value")
                    && d.parent.and_then(|id| dies.get(&id)).is_some_and(|parent| {
                        parent.name() == Some("inspect")
                            && parent
                                .attrs
                                .get("decl_file")
                                .is_some_and(|file| file.contains("probe.lu"))
                    })
            })
            .cloned()
            .collect(),
        "source parameter",
    )?;
    let mut schema = Schema {
        tool,
        artifact,
        dir: dir.to_owned(),
        dies,
    };
    let root = schema.load(param.number("type")?)?;
    if root.name() != Some(expected_type) {
        return Err(format!("wrong source type: {:?}", root.name()));
    }
    let size = root.number("byte_size")?;
    if size == 0 || size > 4096 {
        return Err(format!("unbounded fixture byte read: {size}"));
    }
    let bytes = dir.join("value.bin");
    if bytes.exists() {
        std::fs::remove_file(&bytes).map_err(|e| e.to_string())?;
    }
    let memory = format!(
        "memory read --binary --outfile {} --size 1 --count {size} `&value`",
        serde_json::to_string(&bytes).map_err(|e| e.to_string())?
    );
    let text = output(
        Command::new("lldb")
            .args([
                "--no-lldbinit",
                "--batch",
                "-o",
                "breakpoint set --name inspect",
                "-o",
                "run",
                "-o",
                "frame variable value",
                "-o",
                &memory,
                "-o",
                "continue",
            ])
            .arg(exe),
        dir,
        "lldb-variant-bytes",
    )?;
    let observed = text
        .lines()
        .skip_while(|line| line.trim() != "(lldb) frame variable value")
        .skip(1)
        .take_while(|line| !line.starts_with("(lldb)"))
        .collect::<Vec<_>>()
        .join("\n");
    let gap = format!("({expected_type}) value = {{}}");
    if observed != gap {
        return Err(format!(
            "KNOWN-GAP lldb-variant-part changed: expected {gap:?}, observed {observed:?}; reassess native variant rendering"
        ));
    }
    println!("KNOWN-GAP lldb-variant-part type={expected_type} text={gap}");
    let bytes = std::fs::read(&bytes).map_err(|e| e.to_string())?;
    let actual = active_value(&mut schema, &root, &bytes)?;
    if reference(row, dir, &actual)? {
        println!("PASS {row} variant-value={}", actual.trim_end());
    }
    Ok(())
}

pub fn reference(row: &str, dir: &Path, actual: &str) -> Result<bool, String> {
    let Some(lupin) = std::env::var_os("LUPIN") else {
        if std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_some() {
            return Err("required pinned lupin is absent".into());
        }
        eprintln!("SKIP {row} reason=set LUPIN to the paired 0.1.59 oracle");
        return Ok(false);
    };
    let record = output(
        Command::new(lupin)
            .arg("conform-run")
            .arg(dir.join("probe.lu"))
            .arg("--json"),
        dir,
        "lupin-variant",
    )?;
    let record: serde_json::Value = serde_json::from_str(&record).map_err(|e| e.to_string())?;
    if record["impl_version"].as_str() != pins()["lupin"]["version"].as_str()
        || record["verdict"] != "exit(0)"
    {
        return Err(format!(
            "paired oracle did not execute this fixture: {record}"
        ));
    }
    if record["stdout_inline"].as_str() != Some(actual) {
        return Err(format!(
            "DWARF/debugger value {actual:?} disagrees with lupin: {record}"
        ));
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tree_and_reference_parse_are_not_name_or_line_echoes() {
        let text = "0x00000010: DW_TAG_structure_type\n              DW_AT_name (\"Choice\")\n              DW_AT_byte_size (24)\n0x00000020:   DW_TAG_variant_part\n                DW_AT_discr (0x00000030)\n0x00000030:     DW_TAG_member\n                  DW_AT_data_member_location (0)\n0x00000040:     DW_TAG_variant\n                  DW_AT_discr_value (0)\n0x00000050:       DW_TAG_member\n                    DW_AT_name (\"Number\")\n                    DW_AT_type (0x00000080 \"Choice::Number\")\n";
        let dies = parse(text).unwrap();
        assert_eq!(dies[&0x30].parent, Some(0x20));
        assert_eq!(dies[&0x50].parent, Some(0x40));
        assert_eq!(dies[&0x50].number("type").unwrap(), 0x80);
        assert!(
            parse("(lldb) frame variable value\n(Choice) value = {}\n")
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn a_wrong_discriminant_offset_cannot_pass_the_byte_witness() {
        let text = "0x10: DW_TAG_structure_type\n DW_AT_name (\"Choice\")\n DW_AT_byte_size (24)\n0x20:   DW_TAG_variant_part\n DW_AT_discr (0x30)\n0x30:     DW_TAG_member\n DW_AT_type (0x80)\n DW_AT_data_member_location (0)\n0x40:     DW_TAG_variant\n DW_AT_discr_value (0)\n0x50:       DW_TAG_member\n DW_AT_name (\"Number\")\n DW_AT_type (0x90)\n DW_AT_data_member_location (0)\n0x80: DW_TAG_base_type\n DW_AT_name (\"int\")\n DW_AT_byte_size (8)\n DW_AT_encoding (DW_ATE_signed)\n0x90: DW_TAG_structure_type\n DW_AT_name (\"Choice::Number\")\n DW_AT_byte_size (24)\n0xa0:   DW_TAG_member\n DW_AT_name (\"_0\")\n DW_AT_type (0x80)\n DW_AT_data_member_location (8)\n";
        let dies = parse(text).unwrap();
        let root = dies[&0x10].clone();
        let mut schema = Schema {
            tool: PathBuf::new(),
            artifact: PathBuf::new(),
            dir: PathBuf::new(),
            dies,
        };
        let mut bytes = [0u8; 24];
        bytes[8..16].copy_from_slice(&73i64.to_le_bytes());
        assert_eq!(
            active_value(&mut schema, &root, &bytes).unwrap(),
            "Number 73\n"
        );
        schema
            .dies
            .get_mut(&0x30)
            .unwrap()
            .attrs
            .insert("data_member_location".into(), "8".into());
        assert!(
            active_value(&mut schema, &root, &bytes)
                .unwrap_err()
                .contains("discriminator 73")
        );
    }

    #[test]
    fn byte_reads_are_bounded_and_signed_by_the_dwarf_type() {
        assert_eq!(integer(&[255], 0, 1, false).unwrap(), 255);
        assert_eq!(integer(&[255], 0, 1, true).unwrap(), -1);
        assert!(integer(&[0; 8], 8, 8, true).is_err());
        assert!(integer(&[0; 8], u64::MAX, 8, false).is_err());
        assert!(integer(&[0; 8], 0, 0, false).is_err());
    }
}
