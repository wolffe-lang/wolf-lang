//! Function placement policy shared by all of the driver's link paths.
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::Path;
use wolf_backend::code_layout::{VERSION, key};
use wolf_wir::{FuncId, Module};

pub fn select_cold(module: &mut Module, selected: &BTreeSet<String>) -> Result<(), String> {
    for name in selected {
        let ids: Vec<_> = module
            .funcs
            .iter()
            .filter(|(_, f)| &f.name == name)
            .map(|(id, _)| id)
            .collect();
        let [id] = ids.as_slice() else {
            return Err(format!(
                "--cold-function `{name}` must match exactly one qualified function (matched {})",
                ids.len()
            ));
        };
        let f = &mut module.funcs[*id];
        if f.section.is_some() {
            return Err(format!(
                "--cold-function `{name}` conflicts with its explicit section"
            ));
        }
        f.cold = true;
    }
    Ok(())
}

pub fn cache_tag(selected: &BTreeSet<String>) -> String {
    // Length prefixes make selections unambiguous, including unusual names.
    let mut tag = VERSION.to_string();
    for name in selected {
        tag.push_str(&format!(" cold:{}:{name}", name.len()));
    }
    tag
}

pub fn prepare(
    module: &mut Module,
    package: &wolf_sema::Package,
    cold_requested: bool,
) -> Result<bool, String> {
    if cfg!(windows) {
        if cold_requested {
            return Err(
                "cold function placement on COFF is not supported (`--cold-function`)".into(),
            );
        }
        // Keep the established Windows backend and linker path. The driver
        // explicitly says that this target has no new placement guarantee.
        return Ok(false);
    }
    let homes = wolf_wir::midend::summary::Homes::from_package(package, module);
    for id in module.funcs.keys() {
        let f = &mut module.funcs[id];
        f.code_home = Some(homes.home_of(&f.name).to_string());
    }
    Ok(true)
}

fn symbol(module: &Module, id: FuncId, shim: Option<FuncId>) -> String {
    let f = &module.funcs[id];
    if Some(id) == shim {
        "main".into()
    } else if f.export {
        f.name.clone()
    } else {
        wolf_backend::mangle(module, &f.name, f.sig)
    }
}

/// Maps emitted symbols, including native variadic cores, to stable keys.
fn order(module: &Module, shim: Option<FuncId>) -> BTreeMap<String, (bool, String)> {
    let mut result = BTreeMap::new();
    for (id, f) in module.funcs.iter() {
        if f.section.is_some() {
            continue;
        }
        let Some(home) = &f.code_home else {
            continue;
        };
        let symbol = symbol(module, id, shim);
        result.insert(symbol.clone(), (f.cold, key(home, &f.name, &symbol)));
        let core = format!("_W.va.core.{symbol}");
        result.insert(core.clone(), (f.cold, key(home, &f.name, &core)));
    }
    result
}

fn elf_page_size() -> Result<u64, String> {
    #[cfg(unix)]
    {
        // Hosted links target this host; freestanding builds use their own script.
        let bytes = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        let bytes = u64::try_from(bytes).map_err(|_| "cannot determine ELF page size")?;
        if !bytes.is_power_of_two() {
            return Err("ELF page size is not a positive power of two".into());
        }
        Ok(bytes)
    }
    #[cfg(not(unix))]
    Err("ELF page isolation is unavailable on this host".into())
}

fn cold_script(page: u64) -> String {
    // Reserve the entire last page too: subsequent .init/.fini/.plt output
    // sections must not make a cold page resident merely during startup.
    format!(
        "SECTIONS {{ .wolf.cold : ALIGN({page}) {{ *(SORT_BY_NAME(.wolf.cold.*)) . = ALIGN({page}); }} }} INSERT AFTER .text;\n"
    )
}

/// Arguments are passed separately, never interpolated into a shell command.
/// The same policy reaches the default C driver and an explicitly selected one.
pub fn link_args(
    module: &Module,
    shim: Option<FuncId>,
    dir: &Path,
    objects: &[(String, Vec<u8>)],
) -> Result<Vec<OsString>, String> {
    if module.funcs.values().all(|f| f.code_home.is_none()) {
        return Ok(Vec::new());
    }
    if cfg!(target_os = "macos") {
        use object::{Object, ObjectSymbol};
        let wanted = order(module, shim);
        let mut placed = Vec::new();
        for (_, bytes) in objects {
            let file = object::File::parse(&**bytes).map_err(|e| e.to_string())?;
            for s in file.symbols().filter(|s| s.is_definition()) {
                let Ok(raw) = s.name() else {
                    continue;
                };
                let name = raw.strip_prefix('_').unwrap_or(raw);
                if let Some(k) = wanted.get(name) {
                    placed.push((k.clone(), raw.to_string()));
                }
            }
        }
        placed.sort();
        let path = dir.join("functions.order");
        let text = placed
            .into_iter()
            .map(|(_, n)| format!("{n}\n"))
            .collect::<String>();
        std::fs::write(&path, text).map_err(|e| e.to_string())?;
        return Ok(vec![
            "-Xlinker".into(),
            "-order_file".into(),
            "-Xlinker".into(),
            path.into_os_string(),
        ]);
    }
    if cfg!(windows) {
        return Err("stable function layout on COFF is not supported".into());
    }
    // An explicit SORT clause is required: lld's synthesized default script
    // can retain LLVM cluster order even with --sort-section=name. Collect
    // Wolf inputs before the default .text wildcard can consume them.
    let hot = dir.join("hot.ld");
    std::fs::write(
        &hot,
        "SECTIONS { .text.wolf : { *(SORT_BY_NAME(.text.wolf.*)) } } INSERT BEFORE .text;\n",
    )
    .map_err(|e| e.to_string())?;
    let mut args = vec![
        OsString::from("-Wl,--sort-section=name"),
        "-Xlinker".into(),
        "-T".into(),
        "-Xlinker".into(),
        hot.into_os_string(),
    ];
    if module.funcs.values().any(|f| f.cold) {
        let path = dir.join("cold.ld");
        std::fs::write(&path, cold_script(elf_page_size()?)).map_err(|e| e.to_string())?;
        args.extend([
            "-Xlinker".into(),
            "-T".into(),
            "-Xlinker".into(),
            path.into_os_string(),
        ]);
    }
    Ok(args)
}

/// Refuse a linker which accepted the arguments but did not honor placement.
/// Missing symbols may have been eliminated; compare the surviving definitions.
pub fn verify(exe: &Path, module: &Module, shim: Option<FuncId>) -> Result<(), String> {
    use object::{Object, ObjectSection, ObjectSymbol};
    let wanted = order(module, shim);
    if wanted.is_empty() {
        return Ok(());
    }
    let bytes = std::fs::read(exe).map_err(|e| e.to_string())?;
    let file = object::File::parse(&*bytes).map_err(|e| e.to_string())?;
    let mut placed = Vec::new();
    for s in file.symbols().filter(|s| s.is_definition()) {
        let Ok(raw) = s.name() else {
            continue;
        };
        let name = if file.format() == object::BinaryFormat::MachO {
            raw.strip_prefix('_').unwrap_or(raw)
        } else {
            raw
        };
        if let Some(k) = wanted.get(name) {
            placed.push((k.clone(), s.address(), raw.to_string()));
        }
    }
    placed.sort();
    if !wanted.is_empty() && placed.is_empty() {
        return Err("linker output has no function symbols to validate stable layout".into());
    }
    for pair in placed.windows(2) {
        if pair[0].1 > pair[1].1 {
            return Err(format!(
                "linker did not honor stable function layout: `{}` is after `{}`",
                pair[0].2, pair[1].2
            ));
        }
    }
    if file.format() == object::BinaryFormat::Elf && placed.iter().any(|((cold, _), _, _)| *cold) {
        let bad = |message: &str| format!("linker did not honor cold page isolation: {message}");
        let cold = file
            .section_by_name(".wolf.cold")
            .ok_or_else(|| bad("missing .wolf.cold section"))?;
        let page = elf_page_size()?;
        if !matches!(cold.flags(), object::SectionFlags::Elf { sh_flags } if sh_flags & 4 != 0) {
            return Err(bad("cold section is not executable"));
        }
        let start = cold.address();
        let end = start
            .checked_add(cold.size())
            .ok_or_else(|| bad("cold range overflow"))?;
        if cold.size() == 0 || start % page != 0 || end % page != 0 {
            return Err(bad("cold section must own whole pages"));
        }
        for section in file.sections() {
            if section.index() == cold.index() || section.size() == 0 {
                continue;
            }
            let executable = matches!(section.flags(), object::SectionFlags::Elf { sh_flags } if sh_flags & 4 != 0);
            if !executable {
                continue;
            }
            let section_end = section
                .address()
                .checked_add(section.size())
                .ok_or_else(|| bad("section range overflow"))?;
            if section.address() / page <= (end - 1) / page
                && (section_end - 1) / page >= start / page
            {
                return Err(bad(&format!(
                    ".wolf.cold shares a page with {}",
                    section.name().unwrap_or("an executable section")
                )));
            }
        }
        for ((is_cold, _), address, name) in &placed {
            if *is_cold && !(*address >= start && *address < end) {
                return Err(bad(&format!("`{name}` is outside .wolf.cold")));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cold_output_reserves_both_page_boundaries() {
        for page in [4096, 16384, 65536] {
            let script = cold_script(page);
            assert_eq!(script.matches(&format!("ALIGN({page})")).count(), 2);
            assert!(script.contains(&format!(". = ALIGN({page});")));
        }
    }

    #[test]
    fn cold_selection_is_exact_and_cache_order_independent() {
        let src = "fn @a(i64) -> i64 {\nb0(%0: i64):\n  ret %0\n}\n";
        let mut module = wolf_wir::parse_module(src).unwrap();
        assert!(
            select_cold(&mut module, &BTreeSet::from(["missing".into()]))
                .unwrap_err()
                .contains("matched 0")
        );
        select_cold(&mut module, &BTreeSet::from(["a".into()])).unwrap();
        assert!(module.funcs.values().next().unwrap().cold);
        assert_ne!(
            cache_tag(&BTreeSet::new()),
            cache_tag(&BTreeSet::from(["a".into()]))
        );
        assert_eq!(
            cache_tag(&BTreeSet::from(["a".into(), "b".into()])),
            cache_tag(&BTreeSet::from(["b".into(), "a".into()]))
        );
    }
}
