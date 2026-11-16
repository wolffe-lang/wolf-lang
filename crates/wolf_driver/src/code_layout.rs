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

pub fn prepare(module: &mut Module, package: &wolf_sema::Package) -> Result<(), String> {
    if cfg!(windows) {
        return Err("stable function layout on COFF is not supported; object and executable emission are refused rather than silently using cluster order".into());
    }
    let homes = wolf_wir::midend::summary::Homes::from_package(package, module);
    for id in module.funcs.keys() {
        let f = &mut module.funcs[id];
        f.code_home = Some(homes.home_of(&f.name).to_string());
    }
    Ok(())
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

/// Arguments are passed separately, never interpolated into a shell command.
/// The same policy reaches the default C driver and an explicitly selected one.
pub fn link_args(
    module: &Module,
    shim: Option<FuncId>,
    dir: &Path,
    objects: &[(String, Vec<u8>)],
) -> Result<Vec<OsString>, String> {
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
        std::fs::write(&path,
            "SECTIONS { .wolf.cold : ALIGN(4096) { *(SORT_BY_NAME(.wolf.cold.*)) } } INSERT AFTER .text;\n")
            .map_err(|e| e.to_string())?;
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
    use object::{Object, ObjectSymbol};
    let bytes = std::fs::read(exe).map_err(|e| e.to_string())?;
    let file = object::File::parse(&*bytes).map_err(|e| e.to_string())?;
    let wanted = order(module, shim);
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
