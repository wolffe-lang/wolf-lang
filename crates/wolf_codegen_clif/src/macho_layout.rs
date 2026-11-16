//! Rename homogeneous Mach-O text after the native driver has split hot and
//! cold definitions into separate objects. No bytes, addresses or relocations
//! move: only fixed-width section names change. Symbol subsections let ld64's
//! order file arrange the hot functions across objects.
use std::collections::BTreeMap;
use wolf_backend::BackendError;

pub(crate) fn place(bytes: &mut [u8], placed: &[(String, String)]) -> Result<(), BackendError> {
    use object::{Object, ObjectSymbol};
    let fail = |message: String| BackendError::Internal(message);
    if bytes.len() < 32 || bytes[..4] != [0xcf, 0xfa, 0xed, 0xfe] {
        return Err(BackendError::Unsupported(
            "stable function layout requires ELF64 or little-endian Mach-O64".into(),
        ));
    }
    let mut names = BTreeMap::new();
    {
        let file = object::File::parse(&*bytes).map_err(|e| fail(e.to_string()))?;
        for (symbol, name) in placed {
            if name != "__text" && name != "__wolf_cold" {
                return Err(fail(format!("unexpected Mach-O layout section `{name}`")));
            }
            let prefixed = format!("_{symbol}");
            let sym = file
                .symbols()
                .find(|s| s.name() == Ok(symbol.as_str()) || s.name() == Ok(prefixed.as_str()))
                .ok_or_else(|| fail(format!("Mach-O object lost `{symbol}`")))?;
            let section = sym
                .section_index()
                .ok_or_else(|| fail(format!("`{symbol}` has no section")))?;
            if let Some(previous) = names.insert(section.0, name.clone()) {
                if previous != *name {
                    return Err(fail(
                        "native Mach-O hot and cold definitions must be split before emission"
                            .into(),
                    ));
                }
            }
        }
    }
    let read = |b: &[u8], offset: usize| -> Result<u32, BackendError> {
        let word = b
            .get(offset..offset + 4)
            .ok_or_else(|| fail("truncated Mach-O header".into()))?;
        Ok(u32::from_le_bytes(word.try_into().unwrap()))
    };
    let commands = read(bytes, 16)?;
    let mut at = 32usize;
    let mut index = 0usize;
    for _ in 0..commands {
        let command = read(bytes, at)?;
        let size = read(bytes, at + 4)? as usize;
        let end = at
            .checked_add(size)
            .filter(|&end| end <= bytes.len())
            .ok_or_else(|| fail("invalid Mach-O command size".into()))?;
        if size < 8 {
            return Err(fail("short Mach-O command".into()));
        }
        if command == 0x19 {
            // LC_SEGMENT_64, section_64 records follow.
            if size < 72 {
                return Err(fail("short Mach-O segment".into()));
            }
            let count = read(bytes, at + 64)? as usize;
            if count > (size - 72) / 80 {
                return Err(fail("short Mach-O section table".into()));
            }
            for n in 0..count {
                index += 1; // Mach-O n_sect is one-based.
                if let Some(name) = names.remove(&index) {
                    let start = at + 72 + n * 80;
                    bytes[start..start + 16].fill(0);
                    bytes[start..start + name.len()].copy_from_slice(name.as_bytes());
                }
            }
        }
        at = end;
    }
    if !names.is_empty() {
        return Err(fail("Mach-O placement lost a section header".into()));
    }
    let flags = read(bytes, 24)? | 0x2000; // MH_SUBSECTIONS_VIA_SYMBOLS.
    bytes[24..28].copy_from_slice(&flags.to_le_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cranelift_object::object::write::{Object, Symbol, SymbolSection};
    use cranelift_object::object::{
        Architecture, BinaryFormat, Endianness, SectionKind, SymbolFlags, SymbolKind, SymbolScope,
    };

    fn object() -> Vec<u8> {
        let mut object = Object::new(
            BinaryFormat::MachO,
            Architecture::X86_64,
            Endianness::Little,
        );
        object.set_subsections_via_symbols();
        let section = object.add_section(b"__TEXT".to_vec(), b"__text".to_vec(), SectionKind::Text);
        for name in ["a", "b"] {
            let symbol = object.add_symbol(Symbol {
                name: name.as_bytes().to_vec(),
                value: 0,
                size: 0,
                kind: SymbolKind::Text,
                scope: SymbolScope::Dynamic,
                weak: false,
                section: SymbolSection::Undefined,
                flags: SymbolFlags::None,
            });
            object.add_symbol_data(symbol, section, &[0xc3], 1);
        }
        object.write().unwrap()
    }

    #[test]
    fn cold_section_rename_preserves_symbols_and_instruction_bytes() {
        use object::{Object as _, ObjectSection, ObjectSymbol};
        let mut bytes = object();
        let old = bytes.clone();
        place(
            &mut bytes,
            &[
                ("a".into(), "__wolf_cold".into()),
                ("b".into(), "__wolf_cold".into()),
            ],
        )
        .unwrap();
        let before = object::File::parse(&*old).unwrap();
        let after = object::File::parse(&*bytes).unwrap();
        assert_eq!(
            before.section_by_name("__text").unwrap().data().unwrap(),
            after
                .section_by_name("__wolf_cold")
                .unwrap()
                .data()
                .unwrap()
        );
        let values = |f: &object::File<'_>| {
            f.symbols()
                .map(|s| (s.name().unwrap().to_string(), s.address()))
                .collect::<Vec<_>>()
        };
        assert_eq!(values(&before), values(&after));
    }

    #[test]
    fn mixed_temperature_in_one_section_is_rejected_not_misplaced() {
        let error = place(
            &mut object(),
            &[
                ("a".into(), "__text".into()),
                ("b".into(), "__wolf_cold".into()),
            ],
        )
        .unwrap_err();
        assert!(error.to_string().contains("must be split"));
    }
}
