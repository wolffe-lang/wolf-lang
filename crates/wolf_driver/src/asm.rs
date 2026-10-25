//! Linked assembly (kw05, spec/04 `[abi.asm.link]`, K2 = C): the root
//! manifest's `asm` sources, read for their roster, assembled for the
//! build's target, and handed to the link or placed beside the object.
//!
//! An explicit `--assembler` / `WOLF_AS` uses the as-shaped protocol.
//! Otherwise the assembler is the C driver the build already trusts for that
//! target: on the freestanding target, the release tier's clang
//! (`WOLF_CLANG`, else `clang`) with `--target=x86_64-unknown-none-elf`,
//! which assembles ELF x86-64 from any host; on a hosted target, the
//! link's own C driver (`CC`, else `cc`). Nothing is searched for and
//! nothing runs that the manifest did not name (D33).

use std::path::{Path, PathBuf};

use wolf_backend::target::Target;

/// The listed sources of one build, with their text read once.
pub struct Listed {
    /// (as the manifest spells it, resolved path).
    pub sources: Vec<(String, PathBuf)>,
    /// The roster: every `.globl` name the sources make visible
    /// (`[abi.asm.roster]`), in listing order.
    pub roster: Vec<String>,
}

impl Listed {
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    pub fn spelled(&self) -> Vec<String> {
        self.sources.iter().map(|(s, _)| s.clone()).collect()
    }
}

/// Read the root manifest's `asm` sources. A listed file that does not
/// exist (or cannot be read) is refused naming it — never dropped.
pub fn listed(project: Option<&wolf_pkg::Project>) -> Result<Listed, String> {
    let mut out = Listed {
        sources: Vec::new(),
        roster: Vec::new(),
    };
    let Some(project) = project else {
        return Ok(out);
    };
    for src in &project.asm {
        let text = std::fs::read_to_string(&src.path).map_err(|e| {
            format!(
                "wolf.pkg lists the assembly source `{}`, which cannot be read ({}: {e})",
                src.spelled,
                src.path.display()
            )
        })?;
        for name in wolf_pkg::asm::globals(&text) {
            if !out.roster.contains(&name) {
                out.roster.push(name);
            }
        }
        out.sources.push((src.spelled.clone(), src.path.clone()));
    }
    Ok(out)
}

/// The roster as the checked machine names it: on Apple targets a C
/// name carries one leading underscore in assembly (`_kw_seven` is wolf's
/// `kw_seven`), so it is stripped there.
pub fn roster_for_host(listed: &Listed) -> Vec<String> {
    listed
        .roster
        .iter()
        .map(|n| {
            if cfg!(target_os = "macos") {
                n.strip_prefix('_').unwrap_or(n).to_string()
            } else {
                n.clone()
            }
        })
        .collect()
}

/// The object name a listed source takes beside `out`: `K.o` gets
/// `K.asm-<stem>.o` (`[abi.asm.link]`).
pub fn beside(out: &Path, spelled: &str) -> PathBuf {
    let stem = Path::new(spelled)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "asm".to_string());
    out.with_extension(format!("asm-{stem}.o"))
}

/// Assemble `src` into `obj` for `target`. Errors name the source and
/// the command; a freestanding object that is not ELF x86-64 is refused
/// (an assembler that ignored `--target` must not reach a kernel link).
pub fn assemble(
    spelled: &str,
    src: &Path,
    obj: &Path,
    target: Target,
    assembler: Option<&Path>,
) -> Result<(), String> {
    if cfg!(windows) && !target.is_freestanding() {
        return Err(format!(
            "assembly source `{spelled}`: a hosted windows build has no assembler step yet \
             (the link is a COFF linker, not a C driver); target x86_64-unknown-none \
             assembles with clang or an explicit assembler from any host"
        ));
    }
    let selected = selection(assembler, std::env::var_os("WOLF_AS"));
    let mut command = if let Some(prog) = &selected {
        let mut command = std::process::Command::new(prog);
        if target.is_freestanding() || cfg!(target_arch = "x86_64") {
            command.arg("--64");
        }
        command.arg("-o").arg(obj).arg(src);
        command
    } else {
        let mut command = if target.is_freestanding() {
            let clang = std::env::var_os("WOLF_CLANG")
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "clang".into());
            let mut command = std::process::Command::new(clang);
            command.arg("--target=x86_64-unknown-none-elf");
            command
        } else {
            std::process::Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()))
        };
        command.arg("-c").arg(src).arg("-o").arg(obj);
        command
    };
    // A successful no-output tool must not validate a previous build's object.
    match std::fs::remove_file(obj) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(format!(
                "assembly source `{spelled}`: remove {}: {e}",
                obj.display()
            ));
        }
    }
    let run = command
        .output()
        .map_err(|e| format!("assembly source `{spelled}`: cannot run {command:?} ({e})"))?;
    if !run.status.success() {
        return Err(format!(
            "assembly source `{spelled}` did not assemble ({command:?}, {}):\n{}",
            run.status,
            String::from_utf8_lossy(&run.stderr).trim_end()
        ));
    }
    // Preserve the freestanding guard on every host. An explicitly selected
    // assembler on Linux must also produce the host's ELF relocatable object.
    if target.is_freestanding() || (selected.is_some() && cfg!(target_os = "linux")) {
        use object::Object;
        let bytes = std::fs::read(obj)
            .map_err(|e| format!("assembly source `{spelled}`: read {}: {e}", obj.display()))?;
        let arch = if target.is_freestanding() || cfg!(target_arch = "x86_64") {
            object::Architecture::X86_64
        } else {
            object::Architecture::Aarch64
        };
        let valid = object::File::parse(&*bytes).is_ok_and(|file| {
            file.format() == object::BinaryFormat::Elf
                && file.is_64()
                && file.is_little_endian()
                && file.architecture() == arch
                && file.kind() == object::ObjectKind::Relocatable
        });
        if !valid {
            let expected = if arch == object::Architecture::X86_64 {
                "ELF x86-64"
            } else {
                "ELF AArch64"
            };
            return Err(format!(
                "assembly source `{spelled}`: {command:?} did not produce an {expected} object \
                 for target {target:?}"
            ));
        }
    }
    Ok(())
}

/// Selection is pure so precedence tests never mutate the process environment.
fn selection(cli: Option<&Path>, env: Option<std::ffi::OsString>) -> Option<PathBuf> {
    cli.map(Path::to_path_buf)
        .or_else(|| env.filter(|value| !value.is_empty()).map(PathBuf::from))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembler_cli_over_environment_over_default() {
        assert_eq!(
            selection(Some(Path::new("cli tool")), Some("env tool".into())),
            Some("cli tool".into())
        );
        assert_eq!(
            selection(None, Some("env tool".into())),
            Some("env tool".into())
        );
        assert_eq!(selection(None, Some("".into())), None);
        assert_eq!(selection(None, None), None);
    }
}

/// The roster of the `wolf.pkg` beside `entry`, for the checked
/// machine's refusal text (`[abi.asm.machines]`). conform-run's record
/// is reproducible from the file and the flags alone, so the manifest
/// decides nothing here: a manifest that is absent, malformed or lists
/// an unreadable source only leaves a call into assembly refused under
/// the C membrane's name — the same `unsupported` verdict.
pub fn roster_beside(entry: &Path) -> Vec<String> {
    let dir = entry.parent().unwrap_or(Path::new("."));
    let path = dir.join("wolf.pkg");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    if !wolf_pkg::is_manifest(&text) {
        return Vec::new();
    }
    let mut sm = wolf_span::SourceMap::new();
    let file = sm.intern(&path);
    let (Some(m), _) = wolf_pkg::manifest::parse(file, &text) else {
        return Vec::new();
    };
    let mut roster = Vec::new();
    for (spelled, _) in &m.asm {
        if let Ok(src) = std::fs::read_to_string(dir.join(spelled)) {
            roster.extend(wolf_pkg::asm::globals(&src));
        }
    }
    roster_for_host(&Listed {
        sources: Vec::new(),
        roster,
    })
}
