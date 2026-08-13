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
    // io (s38: stderr writers mirror print; `read_line` is the stdin
    // read — checked lane only until native str materialization lands)
    "print",
    "print_raw",
    "eprint",
    "eprint_raw",
    "read_line",
    // collections & sync (report 07, D13–D16 surface)
    "List",
    "Map",
    "Pool",
    "Mutex",
    "channel",
    // small helpers the corpus leans on
    "min",
    "zip",
    // comptime reflection intrinsics (D29, s16 — the explicit
    // allowlist; `reflect` is the s02 corpus spelling of `typeinfo`)
    "reflect",
    "typeinfo",
    "typebuild",
    "implements",
    "size_of",
    // assertions ([conf.trap.map]: the one user-raised trap; at
    // comptime a failure is E0710)
    "assert",
    // ambient host surfaces (s16 sandbox probes; callable at runtime,
    // categorically refused at comptime — D33). Retire with s05's
    // real std surface like the stand-ins below.
    "read_text",
    "net_fetch",
    "env_var",
    "clock_ms",
    "random_seed",
    // the fs builtin tier (s38, D30 payload-less rows at v0; I13: all
    // tagged `fs` in the sandbox table). std.fs (stdc02) DELEGATES to
    // these — the builtin spelling is not the std API.
    "fs_read_text",
    "fs_write_text",
    "fs_open",
    "fs_create",
    "fs_read",
    "fs_write",
    "fs_close",
    "fs_remove",
    "fs_exists",
    // the net builtin tier (s39, blocking TCP v0 on the checked lane;
    // D30 rows {refused, timeout, closed, io}; I13: all tagged `net`
    // in the sandbox table). std.net (stdc02+) DELEGATES to these —
    // the builtin spelling is not the std API; the s35 reactor owns
    // the real async story.
    "net_listen",
    "net_port",
    "net_accept",
    "net_connect",
    "net_read",
    "net_write",
    "net_close",
    // the os/env builtin tier (s40, capability-first per I13: the env
    // family + `os_cwd` carry `env`, the process trio + `os_exit`
    // carry `exec` in the sandbox table). std.os/std.env/std.process
    // (stdc02+) DELEGATE to these — the builtin spelling is not the
    // std API. `os_spawn` is argv-array ONLY: no shell-string spawn
    // exists anywhere, by construction (injection off the table).
    "env_args",
    "env_get",
    "env_set",
    "env_vars",
    "os_cwd",
    "os_exit",
    "os_spawn",
    "os_wait",
    "os_kill",
    // the time builtin tier (s40, determinism-first per X12: every
    // entry is Clock-tagged and comptime-refused; the ms-integer
    // spellings are the builtin ABI — Instant/SystemTime/Duration
    // typing is the std facade's, where unit confusion becomes a
    // compile error). Virtualization under --schedules/--replay rides
    // the s36 clock-hook seam as it widens.
    "time_now_ms",
    "time_unix_ms",
    "time_sleep_ms",
    // the json builtin tier (s40, nursery-first per D31: std.x.json
    // wraps these as its query kernels). PURE — no capability tag, no
    // sandbox category; the comptime engine still refuses them (no
    // json evaluator in the D33 allowlist at v0).
    "json_valid",
    "json_get",
    "json_type",
    "json_len",
    // the str-construction border (s81, wolf-lang#58). `s.bytes()` is a
    // byte VIEW (s77) and there was no byte SOURCE: nothing anywhere
    // turned bytes back into a `str`, which is what blocked wolf-std's
    // `bytes.to_str`. This is the ONLY entry that builds a `str` from
    // arbitrary numbers, and it VALIDATES — its failure is the `utf8`
    // row, never a trap and never a cast. PURE (no capability, no
    // sandbox category), like the json family.
    "str_from_utf8",
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
///
/// Subordinated by F-0001: this table answers only when no std root is
/// configured (`--std-root` / `WOLF_STD`). With a root, `use std.X`
/// loads `<std-root>/X/` through the ordinary module machinery
/// ([`crate::graph`]) and this table is never consulted.
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

/// The provisional corpus stand-ins at the tail of [`PRELUDE`]: names
/// that exist precisely so early corpus programs can define them, and
/// that retire with the real std surface. Shadowing one is expected,
/// not hazardous, so the W0304 lint exempts them.
pub const PRELUDE_PROVISIONAL: &[&str] = &[
    "acquire",
    "release",
    "build_config",
    "build_batch",
    "worker",
    "sleeper",
];

/// Would declaring `name` silently shadow an ambient name worth
/// keeping (W0304)? True for every prelude name except the
/// provisional stand-ins, and for every built-in type name.
pub fn shadow_hazard(name: &str) -> bool {
    (in_prelude(name) && !PRELUDE_PROVISIONAL.contains(&name)) || is_builtin_type(name)
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
