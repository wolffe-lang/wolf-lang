//! Inspect only the producer's .debug_* sections. Used by ci/dwarf-census.py.
//! A comparison checks bytes AND canonical relocation targets, not file mtimes.
use object::{Object, ObjectSection, ObjectSymbol};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Reloc {
    offset: u64,
    kind: String,
    encoding: String,
    size: u8,
    target: String,
    addend: i64,
    implicit: bool,
}

#[derive(Debug, PartialEq, Eq)]
struct Section {
    bytes: Vec<u8>,
    relocs: Vec<Reloc>,
}

fn sections(path: &Path) -> Result<BTreeMap<String, Section>, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let object = object::File::parse(&*bytes).map_err(|e| e.to_string())?;
    let mut result = BTreeMap::new();
    for section in object.sections() {
        let name = section.name().map_err(|e| e.to_string())?;
        let name = if let Some(tail) = name.strip_prefix("__debug_") {
            format!(".debug_{tail}")
        } else {
            name.to_owned()
        };
        if !name.starts_with(".debug_") {
            continue;
        }
        let mut relocs = Vec::new();
        for (offset, relocation) in section.relocations() {
            let target = match relocation.target() {
                object::RelocationTarget::Symbol(index) => {
                    let symbol = object.symbol_by_index(index).map_err(|e| e.to_string())?;
                    format!("symbol:{}", symbol.name().map_err(|e| e.to_string())?)
                }
                object::RelocationTarget::Section(index) => {
                    let section = object.section_by_index(index).map_err(|e| e.to_string())?;
                    format!("section:{}", section.name().map_err(|e| e.to_string())?)
                }
                object::RelocationTarget::Absolute => "absolute".into(),
                _ => return Err("unrecognized debug relocation target".into()),
            };
            relocs.push(Reloc {
                offset,
                kind: format!("{:?}", relocation.kind()),
                encoding: format!("{:?}", relocation.encoding()),
                size: relocation.size(),
                target,
                addend: relocation.addend(),
                implicit: relocation.has_implicit_addend(),
            });
        }
        relocs.sort();
        result.insert(
            name,
            Section {
                bytes: section.data().map_err(|e| e.to_string())?.to_vec(),
                relocs,
            },
        );
    }
    Ok(result)
}

fn run() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let first = args
        .next()
        .ok_or("usage: dwarf_sections OBJECT [COMPARE_OBJECT]")?;
    let second = args.next();
    if args.next().is_some() {
        return Err("too many arguments".into());
    }
    let first = sections(Path::new(&first))?;
    if first.is_empty() {
        return Err("object has no .debug_* sections".into());
    }
    let mut total = 0usize;
    let mut encoded = BTreeMap::new();
    for (name, section) in &first {
        total += section.bytes.len();
        encoded.insert(name, json!({
            "bytes": section.bytes.len(), "blake3": blake3::hash(&section.bytes).to_hex().to_string(),
            "relocations": section.relocs.iter().map(|r| json!({"offset":r.offset,"kind":r.kind,"encoding":r.encoding,"size":r.size,"target":r.target,"addend":r.addend,"implicit":r.implicit})).collect::<Vec<_>>(),
        }));
    }
    let equal = match second {
        Some(path) => Some(first == sections(Path::new(&path))?),
        None => None,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"debug_bytes":total,"sections":encoded,"byte_and_relocation_identical":equal})
        )
        .map_err(|e| e.to_string())?
    );
    if equal == Some(false) {
        return Err("FAIL dwarf-byte-determinism".into());
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("dwarf_sections: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn relocation_changes_are_not_byte_identity() {
        let first = Section {
            bytes: vec![0; 8],
            relocs: vec![Reloc {
                offset: 0,
                kind: "Absolute".into(),
                encoding: "Generic".into(),
                size: 64,
                target: "symbol:left".into(),
                addend: 0,
                implicit: false,
            }],
        };
        let mut second = Section {
            bytes: first.bytes.clone(),
            relocs: first.relocs.clone(),
        };
        second.relocs[0].target = "symbol:right".into();
        assert_ne!(first, second);
    }
}
