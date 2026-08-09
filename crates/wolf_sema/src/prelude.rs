//! The std/prelude stub tables (s12).
//!
//! Until the real standard library lands (s05 surface, s51 packaging),
//! resolution needs *names only*: the prelude makes a small ambient set
//! resolvable without imports (D31), and a tiny `std` module tree backs
//! `use std.…`. No types, no signatures — those arrive with the actual
//! std. These tables are deliberately one obvious const each, so the
//! whole stub inventory is reviewable at a glance.

/// Ambient prelude names (D31): resolvable in every file with no import.
///
/// The trailing group are provisional stand-ins that the s02 corpus
/// programs call as if they were std (`worker()`, `acquire()`, …); they
/// retire the moment the real std surface (s05) replaces them — keeping
/// the corpus resolving cleanly is what the stub is *for* this sprint.
pub const PRELUDE: &[&str] = &[
    // io
    "print",
    "print_raw",
    // collections & sync (report 07, D13–D16 surface)
    "List",
    "Map",
    "Pool",
    "Mutex",
    "channel",
    // small helpers the corpus leans on
    "min",
    "zip",
    // comptime reflection (D29)
    "reflect",
    // provisional corpus stand-ins (retire with s05's real std surface)
    "acquire",
    "release",
    "build_config",
    "build_batch",
    "worker",
    "sleeper",
];

/// Built-in type names, resolvable everywhere in type (and expression)
/// position. Closed set for now; the real inventory is spec 02's.
pub const BUILTIN_TYPES: &[&str] = &[
    "bool", "str", "byte", "int", "uint", "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64",
    "f32", "f64", "wrapping",
];

/// One stub std module: its dotted path and its item names.
#[derive(Clone, Copy, Debug)]
pub struct StdModule {
    pub path: &'static [&'static str],
    pub items: &'static [&'static str],
}

/// The `std` stub tree behind `use std.…` — names only, no types.
pub const STD_MODULES: &[StdModule] = &[
    StdModule {
        path: &["std"],
        items: &["fs"],
    },
    StdModule {
        path: &["std", "fs"],
        items: &["read_text"],
    },
];

/// Find a std module by exact dotted path segments.
pub fn std_module(path: &[&str]) -> Option<usize> {
    STD_MODULES.iter().position(|m| m.path == path)
}

/// Is `name` an ambient prelude name?
pub fn in_prelude(name: &str) -> bool {
    PRELUDE.contains(&name)
}

/// Is `name` a built-in type name?
pub fn is_builtin_type(name: &str) -> bool {
    BUILTIN_TYPES.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn std_lookup_by_segments() {
        assert_eq!(std_module(&["std"]), Some(0));
        let fs = std_module(&["std", "fs"]).expect("std.fs registered");
        assert!(STD_MODULES[fs].items.contains(&"read_text"));
        assert_eq!(std_module(&["std", "net"]), None);
    }

    #[test]
    fn prelude_and_builtins_answer() {
        assert!(in_prelude("print"));
        assert!(!in_prelude("frobnicate"));
        assert!(is_builtin_type("i32"));
        assert!(is_builtin_type("wrapping"));
        assert!(!is_builtin_type("List")); // List is prelude, not builtin
    }
}
