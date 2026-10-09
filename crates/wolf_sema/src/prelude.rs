//! The std/prelude stub tables (s12).
//!
//! Until the real standard library lands (s05 surface, s51 packaging),
//! resolution needs *names only*: the prelude makes a small ambient set
//! resolvable without imports (D31), and a tiny `std` module tree backs
//! `use std.…`. No types, no signatures — those arrive with the actual
//! std. These tables are deliberately one obvious const each, so the
//! whole stub inventory is reviewable at a glance.
//!
//! s212 (ruling #39 = B, wolf-lang#586, wolf-lsp#32): each row of the
//! two ambient tables carries its [`Kind`] and the spec anchor that
//! defines it, and `wolf prelude --json` publishes exactly these rows
//! ([`names`]). The resolver, the W0304 lint and the published list
//! read ONE table, so a name cannot be added to the language without
//! appearing in the list, and the list cannot name what the checker
//! does not resolve. `spec/prelude.json` is the committed copy (beside
//! `anchors.json`, so a consumer that vendors `spec/` has it at every
//! pin); `cargo xtask prelude-diff` diffs it between two tags.

/// What an ambient name is, as the published list spells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A built-in scalar type ([`BUILTIN_TYPES`]): `int`, `str`, …
    BuiltinType,
    /// A prelude type name that takes (or may take) type arguments and
    /// is written in type position: `List`, `Map`, `Scope`, `range`, …
    Type,
    /// A callable: the host builtins `[os.host.sigs]` types and the
    /// output helpers.
    Function,
    /// A comptime intrinsic or primitive (`[conf.resolve.ambient]`'s
    /// exception): `assert` and the D33 allowlist.
    Intrinsic,
    /// An s02 corpus stand-in ([`PRELUDE_PROVISIONAL`]), not language.
    Provisional,
}

impl Kind {
    /// The published spelling (`wolf-prelude/0`'s `kind` field).
    pub const fn as_str(self) -> &'static str {
        match self {
            Kind::BuiltinType => "builtin_type",
            Kind::Type => "type",
            Kind::Function => "function",
            Kind::Intrinsic => "intrinsic",
            Kind::Provisional => "provisional",
        }
    }
}

/// One ambient name: its spelling, its kind, and the spec anchor whose
/// clause defines it — `None` where no clause does (a spec debt the
/// published list states rather than papers over with a near miss).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ambient {
    pub name: &'static str,
    pub kind: Kind,
    pub anchor: Option<&'static str>,
}

/// The anchor of every host builtin: its signature, row included, is
/// `[os.host.sigs]`'s table line (held equal to `host_builtin_sig` by
/// `tests/host_table_spec.rs`).
pub const HOST_SIGS_ANCHOR: &str = "os.host.sigs";

const fn host(name: &'static str) -> Ambient {
    Ambient {
        name,
        kind: Kind::Function,
        anchor: Some(HOST_SIGS_ANCHOR),
    }
}

const fn fun(name: &'static str, anchor: Option<&'static str>) -> Ambient {
    Ambient {
        name,
        kind: Kind::Function,
        anchor,
    }
}

const fn ty(name: &'static str, anchor: Option<&'static str>) -> Ambient {
    Ambient {
        name,
        kind: Kind::Type,
        anchor,
    }
}

const fn intrinsic(name: &'static str, anchor: Option<&'static str>) -> Ambient {
    Ambient {
        name,
        kind: Kind::Intrinsic,
        anchor,
    }
}

const fn stand_in(name: &'static str) -> Ambient {
    Ambient {
        name,
        kind: Kind::Provisional,
        anchor: None,
    }
}

const fn prim(name: &'static str, anchor: Option<&'static str>) -> Ambient {
    Ambient {
        name,
        kind: Kind::BuiltinType,
        anchor,
    }
}

/// Ambient prelude names (D31): resolvable in every file with no import.
///
/// The trailing group are provisional stand-ins that the s02 corpus
/// programs call as if they were std (`worker()`, `acquire()`, …); they
/// retire the moment the real std surface (s05) replaces them — keeping
/// the corpus resolving cleanly is what the stub is *for* this sprint.
pub const PRELUDE: &[Ambient] = &[
    // io (s38: stderr writers mirror print; `read_line` is the stdin
    // read — checked lane only until native str materialization lands)
    fun("print", None),
    fun("print_raw", None),
    fun("eprint", None),
    fun("eprint_raw", None),
    host("read_line"),
    // collections & sync (report 07, D13–D16 surface)
    ty("List", Some("type.list.lit")),
    ty("Map", Some("type.map")),
    ty("Pool", Some("mem.shared.handle")),
    ty("Mutex", Some("conc.mm.hb.mutex")),
    ty("channel", Some("conc.chan.type")),
    // kw11 (`[conc.mm.fence]`, K5 = A): the fence builtin. Its order
    // operand's `Order` is no prelude name: an order operand is a mark,
    // not an expression (`[conc.mm.atomic.order]`).
    fun("fence", Some("conc.mm.fence")),
    // the concurrency handle types (s170, wolf-lang#316; BACKLOG B21
    // rules the spelling): `Scope` is `scope name { … }`'s handle and
    // `Proc[T]` is `spawn proc f(…)`'s, T being the completion value
    // the join collects (wolf-lang#110). Named here — not admitted as
    // the lowercase keywords in type position the way `region` is —
    // so `[conc.task.scope]`'s handle-as-parameter is writable:
    // `fn f(s: Scope)`, `fn g(p: Proc[int])`. `sig.rs` types them.
    ty("Scope", Some("conc.proc.handle")),
    ty("Proc", Some("conc.proc.handle")),
    // s158 (`[type.range]`, wolf-lang#24): `range[int]` / `range[char]`
    // — the type of `a..b`. A prelude NAME, like `List`, not a
    // `BUILTIN_TYPES` prim: it takes an argument.
    ty("range", Some("type.range.name")),
    // small helpers the corpus leans on
    fun("min", None),
    fun("zip", None),
    // comptime reflection intrinsics (D29, s16 — the explicit
    // allowlist; `reflect` is the s02 corpus spelling of `typeinfo`)
    intrinsic("reflect", None),
    intrinsic("typeinfo", None),
    intrinsic("typebuild", None),
    intrinsic("implements", None),
    intrinsic("size_of", Some("abi.layout.query")),
    // kw08 (`[abi.layout.query]`): the other two layout queries.
    intrinsic("align_of", Some("abi.layout.query")),
    intrinsic("offset_of", Some("abi.layout.query")),
    // assertions ([conf.trap.map]: the one user-raised trap; at
    // comptime a failure is E0710)
    intrinsic("assert", Some("conf.trap.assert")),
    // ambient host surfaces (s16 sandbox probes; callable at runtime,
    // categorically refused at comptime — D33). Retire with s05's
    // real std surface like the stand-ins below.
    host("read_text"),
    host("net_fetch"),
    host("env_var"),
    host("clock_ms"),
    host("random_seed"),
    // the fs builtin tier (s38, D30 payload-less rows at v0; I13: all
    // tagged `fs` in the sandbox table). std.fs (stdc02) DELEGATES to
    // these — the builtin spelling is not the std API.
    host("fs_read_text"),
    host("fs_write_text"),
    host("fs_open"),
    host("fs_create"),
    host("fs_read"),
    host("fs_write"),
    host("fs_close"),
    host("fs_remove"),
    host("fs_exists"),
    // the s90 additions (wolf-lang#51/#52): the same `fs` capability,
    // the same D30 rows. `fs_open_mode` gives the family the open
    // MODE it never had (so `append_text` appends instead of
    // read-concat-writing); the `_bytes`/`_chunk` pairs make
    // `copy_file`/`move_file` byte operations; `fs_read_dir` is the
    // listing std shipped without; `fs_rename` promises the move but
    // NOT atomicity (see `wolf_rt::fs`), and the create/metadata
    // entries let a writer make the directory it is about to write
    // into and ask WHAT exists.
    host("fs_open_mode"),
    host("fs_read_bytes"),
    host("fs_write_bytes"),
    host("fs_read_chunk"),
    host("fs_write_chunk"),
    host("fs_read_dir"),
    host("fs_create_dir"),
    host("fs_create_dir_all"),
    host("fs_remove_dir"),
    host("fs_remove_dir_all"),
    host("fs_rename"),
    host("fs_is_file"),
    host("fs_is_dir"),
    host("fs_size"),
    host("fs_modified_ms"),
    // s142 (#261): the stat on an open handle, `[os.fs.fstat]`.
    host("fs_fstat"),
    // s218 (#625, #626): the full stat record by path, following and
    // not (`[os.fs.stat]`), the link's target (`[os.fs.readlink]`) and
    // the unsorted typed listing (`[os.fs.readdir]`).
    host("fs_stat"),
    host("fs_lstat"),
    host("fs_read_link"),
    host("fs_read_dir_entries"),
    // s199 (#426): the handle's offset — `[os.fs.seek]`,
    // `[os.fs.tell]`, `[os.fs.read_at]`.
    host("fs_seek"),
    host("fs_tell"),
    host("fs_read_at"),
    // s200 (#417): the fused chunk copy, `[os.fs.copy]`.
    host("fs_copy_chunk"),
    // the net builtin tier (s39, blocking TCP v0 on the checked lane;
    // D30 rows {refused, timeout, closed, io}; I13: all tagged `net`
    // in the sandbox table). std.net (stdc02+) DELEGATES to these —
    // the builtin spelling is not the std API; the s35 reactor owns
    // the real async story.
    host("net_listen"),
    host("net_port"),
    host("net_accept"),
    host("net_connect"),
    host("net_read"),
    host("net_write"),
    // s115 (#137): the byte twins — `List[int]` in/out, no UTF-8 gate,
    // so a server carries a binary body (`fs_read_bytes`/`fs_write_bytes`
    // for the socket). The str variants stay for text.
    host("net_read_bytes"),
    host("net_write_bytes"),
    // s141 (#254): the gathered write — every part of a
    // `List[List[byte]]` in one syscall, `net_write`'s rows —
    // `[os.net.writev]`; and the one socket option a stream takes
    // after acquisition, `[os.net.nodelay]`.
    host("net_writev"),
    host("net_writev_head"),
    host("net_nodelay"),
    // s136 (#227): the unix-domain pair, `[os.net.unix]`.
    host("net_listen_unix"),
    host("net_connect_unix"),
    // s137 (#234/#235): listener options (`reuse_port`, backlog —
    // `[os.net.listen.opts]`) and the adopt half of descriptor
    // inheritance (`[os.proc.inherit]`).
    host("net_listen_with"),
    host("net_adopt_listener"),
    // s137 (#127): readiness over a set, `[os.net.wait]` — the
    // primitive a spawn-free serving loop blocks on.
    host("net_wait"),
    host("net_close"),
    // s106 (#45's builtin half): the per-socket deadline budget that
    // makes the `timeout` tag reachable — declared with the family
    // since s39, armable since the s35 reactor, crossed here.
    host("net_deadline"),
    // the os/env builtin tier (s40, capability-first per I13: the env
    // family + `os_cwd` carry `env`, the process trio + `os_exit`
    // carry `exec` in the sandbox table). std.os/std.env/std.process
    // (stdc02+) DELEGATE to these — the builtin spelling is not the
    // std API. `os_spawn` is argv-array ONLY: no shell-string spawn
    // exists anywhere, by construction (injection off the table).
    host("env_args"),
    host("env_get"),
    host("env_set"),
    // s225 (wolf-lang#534, `[os.env.unset]`): the removal `env_set`
    // lacked — `env`-tagged with the family it completes.
    host("env_unset"),
    host("env_vars"),
    host("os_cwd"),
    // s90 (wolf-lang#69): the running executable's path. `env`-tagged
    // with `os_cwd` — it READS process context and changes nothing —
    // even though its reason to exist is std.process's rig, which
    // spawns the test binary as its own child.
    host("os_exe"),
    // s137 (#233, `[os.cpus]`): the schedulable core count — a
    // machine-state read, `env`-tagged with `os_cwd`/`os_exe`.
    host("os_cpus"),
    host("os_exit"),
    host("os_spawn"),
    // s137 (#235, `[os.proc.inherit]`): the spawn that hands
    // descriptors to its child — `exec`-tagged with the trio.
    host("os_spawn_with"),
    // s215 (pelt's H2): a spawn with a descriptor map (`exec`), and
    // the three calls a shell's plumbing needs beside it — a pipe, the
    // working directory's write half, and whether a handle is a
    // terminal (`io`: they act on this process and start nothing).
    host("os_spawn_fds"),
    host("os_pipe"),
    host("os_chdir"),
    host("os_isatty"),
    // s225 (wolf-lang#534, `[os.proc.exec]`): replace the running
    // program — argv, an explicit environment and a descriptor map;
    // `exec`-tagged with the spawn family it ends.
    host("os_exec"),
    host("os_wait"),
    host("os_kill"),
    // signal RECEPTION (s114, wolf-lang#126): the receive side of the
    // process-signal world (`os_kill` is send-to-child). By MEANING —
    // a bitmask set of reload/terminate/quit/upgrade, mapped to
    // platform signals at the runtime boundary. `os_signal_raise` is
    // the send-to-self companion (the deterministic loopback). `exec`-
    // tagged with the process family; comptime-refused.
    host("os_signal_listen"),
    host("os_signal_wait"),
    host("os_signal_raise"),
    // s219 (wolf-lang#622, pelt's H3): what a shell needs to survive
    // Ctrl-C and run jobs — a meaning's disposition (ignore, default),
    // the non-blocking take of a queued meaning, a spawn that places
    // its child in a process group with the terminal, the child's pid
    // and its status by signal number, and the terminal's foreground
    // group and mode.
    host("os_signal_ignore"),
    host("os_signal_default"),
    host("os_signal_poll"),
    host("os_spawn_job"),
    host("os_proc_pid"),
    host("os_wait_status"),
    host("os_pgid"),
    host("os_term_foreground"),
    host("os_term_set_foreground"),
    host("os_term_mode"),
    host("os_term_set_mode"),
    // the OS random source (s118, wolf-lang#143): `os_random(n)` mints
    // n bytes of OS-provided entropy as a `List[int]` — the ONLY
    // sanctioned source of cryptographic material ([os.random.sole]).
    // No error row: failure TRAPS ([os.random.trap], never a
    // fallback). `random_seed` above stays what it is — a typed
    // PRNG-seed convenience, NOT an entropy API (the issue's own
    // ruling); nothing lowers it and nothing here changes that.
    host("os_random"),
    // s200 (#407, `[os.fs.error]`): the host's number for the task's most
    // recent fallible fs-family call, and its text. Beside the row,
    // never on it. PURE (no capability): they read what the program's
    // own calls left, no ambient surface.
    host("os_error"),
    host("os_error_text"),
    // the time builtin tier (s40, determinism-first per X12: every
    // entry is Clock-tagged and comptime-refused; the ms-integer
    // spellings are the builtin ABI — Instant/SystemTime/Duration
    // typing is the std facade's, where unit confusion becomes a
    // compile error). Virtualization under --schedules/--replay rides
    // the s36 clock-hook seam as it widens.
    host("time_now_ms"),
    host("time_unix_ms"),
    host("time_sleep_ms"),
    // the json builtin tier (s40, nursery-first per D31: std.x.json
    // wraps these as its query kernels). PURE — no capability tag, no
    // sandbox category; the comptime engine still refuses them (no
    // json evaluator in the D33 allowlist at v0).
    host("json_valid"),
    host("json_get"),
    host("json_type"),
    host("json_len"),
    // the str-construction border (s81, wolf-lang#58). `s.bytes()` is a
    // byte VIEW (s77) and there was no byte SOURCE: nothing anywhere
    // turned bytes back into a `str`, which is what blocked wolf-std's
    // `bytes.to_str`. This is the ONLY entry that builds a `str` from
    // arbitrary numbers, and it VALIDATES — its failure is the `utf8`
    // row, never a trap and never a cast. PURE (no capability, no
    // sandbox category), like the json family.
    host("str_from_utf8"),
    // s200 (#411, `[mem.list.bytes]`): the bulk byte scan over a
    // `List[byte]` — `str`'s `find` and `count` for bytes, lowered to
    // the runtime's vectorised loops. PURE (no capability).
    host("bytes_find"),
    host("bytes_count"),
    // the region accounting queries (s131, wolf-lang#187): the ledger
    // wolf_rt already keeps, surfaced. `region_bytes(r)` answers a
    // named region's charge; `live_region_bytes()` the process-wide
    // live-region total. PURE (no capability, no sandbox category) —
    // they read the memory model's own state, not an ambient OS
    // surface; the comptime engine refuses them with the rest (no
    // region machine in the D33 sandbox).
    host("region_bytes"),
    host("live_region_bytes"),
    // provisional corpus stand-ins (retire with s05's real std surface)
    stand_in("acquire"),
    stand_in("release"),
    stand_in("build_config"),
    stand_in("build_batch"),
    stand_in("worker"),
    stand_in("sleeper"),
];

/// Built-in type names, resolvable everywhere in type (and expression)
/// position. Closed set for now; the real inventory is spec 02's.
pub const BUILTIN_TYPES: &[Ambient] = &[
    prim("bool", None),
    prim("str", Some("type.str")),
    prim("byte", Some("type.byte")),
    prim("char", Some("type.char")),
    prim("int", None),
    prim("uint", None),
    prim("i8", None),
    prim("i16", None),
    prim("i32", None),
    prim("i64", None),
    prim("u8", None),
    prim("u16", None),
    prim("u32", None),
    prim("u64", None),
    prim("f32", None),
    prim("f64", None),
    prim("wrapping", None),
    // s213 (wolf-lang#572, ruling owed): the bottom type, written as a
    // fn's return type — the fn never returns.
    prim("never", Some("type.fn.never")),
];

/// `[type.method.home]` (s166, wolf-lang#390) — the home module of each
/// std data type, as `std.<name>`, beside the builtin method names
/// `[type.method.resolve]` step (1) answers on that type. The loader's
/// pre-scan ([`crate::graph`]) loads a home module only when a method
/// call names one of its `pub fn`s that is NOT in this list, so a
/// program whose method calls are all builtins never loads std for them.
pub const HOME_MODULES: &[(&str, &[&str])] = &[
    (
        "list",
        &[
            "push", "pop", "get", "first", "last", "is_empty", "count", "clear", "par",
        ],
    ),
    ("map", &["count", "is_empty", "clear", "pairs"]),
    (
        "str",
        &[
            "is_empty",
            "get",
            "bytes",
            "chars",
            "starts_with",
            "ends_with",
            "contains",
            "find",
            "rfind",
            "count",
            "split",
            "words",
            "lines",
            "trim",
            "trim_start",
            "trim_end",
            "lower",
            "upper",
            "strip_prefix",
            "strip_suffix",
            "repeat",
            "replace",
            "to_int",
        ],
    ),
    ("range", &[]),
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
    PRELUDE.iter().any(|a| a.name == name)
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

/// Prelude names that exist ONLY in type position (s158): `range` is
/// the type of `a..b` (`[type.range]`) and has no expression form at
/// all — there is no `range(…)` constructor, the values are spelled
/// `a..b`. A declaration named `range` therefore shadows nothing a
/// program can spell, and W0304 exempts it.
///
/// This is the line between it and `List`: `List[int]()` IS an
/// expression, so a module that declares its own `List` really does
/// sever itself from the container, and the lint is right to say so.
/// `var range = true` severs nothing, and two of the corpus's own
/// `os_random` witnesses spell it (the word is too ordinary to tax).
pub const PRELUDE_TYPE_ONLY: &[&str] = &["range"];

/// Would declaring `name` silently shadow an ambient name worth
/// keeping (W0304)? True for every prelude name except the
/// provisional stand-ins and the type-position-only names, and for
/// every built-in type name.
pub fn shadow_hazard(name: &str) -> bool {
    (in_prelude(name) && !PRELUDE_PROVISIONAL.contains(&name) && !PRELUDE_TYPE_ONLY.contains(&name))
        || (is_builtin_type(name) && !BUILTIN_TYPE_ONLY.contains(&name))
}

/// Built-in type names with no expression form (s213): `never` is
/// written only as a fn's return type (`[type.fn.never]`), so a
/// binding named `never` shadows nothing a program can spell — W0304
/// exempts it, as it exempts `range` (wolf-std's tests bind a local
/// `never`).
pub const BUILTIN_TYPE_ONLY: &[&str] = &["never"];

/// Is `name` a built-in type name?
pub fn is_builtin_type(name: &str) -> bool {
    BUILTIN_TYPES.iter().any(|a| a.name == name)
}

/// One published row of the ambient set (s212): a table row plus
/// whether a declaration of that name draws W0304.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublishedName {
    pub name: &'static str,
    pub kind: Kind,
    pub anchor: Option<&'static str>,
    /// [`shadow_hazard`]'s answer: a module that declares this name
    /// draws W0304 (fatal under `--deny-warnings`).
    pub w0304: bool,
}

/// Every name a program can use without an import, in table order —
/// [`PRELUDE`]'s rows, then [`BUILTIN_TYPES`]'s. This is what `wolf
/// prelude` prints; it reads the rows resolution reads, so it cannot
/// drift from them.
pub fn names() -> Vec<PublishedName> {
    PRELUDE
        .iter()
        .chain(BUILTIN_TYPES)
        .map(|a| PublishedName {
            name: a.name,
            kind: a.kind,
            anchor: a.anchor,
            w0304: shadow_hazard(a.name),
        })
        .collect()
}

/// One builtin mark: a row tag the host builtin table declares, and the
/// builtins (in [`PRELUDE`] order) whose rows carry it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mark {
    pub name: String,
    pub by: Vec<&'static str>,
}

/// The builtin marks, sorted by name: every tag of every row that
/// `host_builtin_sig` gives a prelude name — the checker's own rows, so
/// a builtin that gains a tag publishes it. Wolf has no builtin enum;
/// these payload-free tags are the only builtin variant spellings.
pub fn marks() -> Vec<Mark> {
    use crate::types::{TyKind, TypeTable};
    let mut t = TypeTable::new();
    let mut by: std::collections::BTreeMap<String, Vec<&'static str>> = Default::default();
    for a in PRELUDE {
        let Some((_, ret)) = crate::check::host_builtin_sig(&mut t, a.name) else {
            continue;
        };
        let TyKind::ErrUnion(_, row) = t.kind(ret) else {
            continue;
        };
        if let TyKind::Row { tags, .. } = t.kind(*row) {
            for (tag, _) in tags {
                by.entry(tag.clone()).or_default().push(a.name);
            }
        }
    }
    by.into_iter().map(|(name, by)| Mark { name, by }).collect()
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

    /// s170 (wolf-lang#316, B21): the two conc handle types are
    /// PRELUDE names, not builtin scalars — resolvable with no import
    /// and elaborated by `sig.rs`, exactly as `channel`/`List` are.
    /// Shadowing one is a hazard (they are not provisional stand-ins).
    #[test]
    fn conc_handle_types_are_prelude_names() {
        assert!(in_prelude("Scope"));
        assert!(in_prelude("Proc"));
        assert!(!is_builtin_type("Scope"));
        assert!(!is_builtin_type("Proc"));
        assert!(shadow_hazard("Scope"));
        assert!(shadow_hazard("Proc"));
    }

    /// s158 (`[type.range]`): `range` resolves as a type name, and
    /// shadowing it is NOT a W0304 hazard — it has no expression form,
    /// so a `var range = true` severs nothing. `List` is the contrast:
    /// `List[int]()` is an expression, and shadowing it really does
    /// cut the module off from the container.
    #[test]
    fn range_is_a_type_position_prelude_name() {
        assert!(in_prelude("range"));
        assert!(!is_builtin_type("range"));
        assert!(!shadow_hazard("range"));
        assert!(shadow_hazard("List"));
        assert!(shadow_hazard("print"));
        assert!(!shadow_hazard("worker")); // provisional stand-in
    }

    /// s212: the stand-in list W0304 exempts and the rows of kind
    /// `Provisional` are one set, in one order.
    #[test]
    fn provisional_rows_are_the_exempt_stand_ins() {
        let rows: Vec<&str> = PRELUDE
            .iter()
            .filter(|a| a.kind == Kind::Provisional)
            .map(|a| a.name)
            .collect();
        assert_eq!(rows, PRELUDE_PROVISIONAL);
    }

    /// s212: a function's anchor is `[os.host.sigs]` exactly when the
    /// checker types it from the host table — the anchor is not a
    /// guess about which clause a builtin lives under.
    #[test]
    fn host_anchor_iff_host_signature() {
        let mut t = crate::types::TypeTable::new();
        for a in PRELUDE {
            let typed = crate::check::host_builtin_sig(&mut t, a.name).is_some();
            let anchored = a.anchor == Some(HOST_SIGS_ANCHOR);
            assert_eq!(typed, anchored, "{}", a.name);
            if typed {
                assert_eq!(a.kind, Kind::Function, "{}", a.name);
            }
        }
    }

    /// s212: no name is published twice, and every published row
    /// answers the resolver's own two questions.
    #[test]
    fn names_are_unique_and_resolvable() {
        let all = names();
        let mut seen = std::collections::BTreeSet::new();
        for n in &all {
            assert!(seen.insert(n.name), "{} published twice", n.name);
            let builtin = n.kind == Kind::BuiltinType;
            assert_eq!(is_builtin_type(n.name), builtin, "{}", n.name);
            assert_eq!(in_prelude(n.name), !builtin, "{}", n.name);
            assert_eq!(n.w0304, shadow_hazard(n.name), "{}", n.name);
        }
        assert_eq!(all.len(), PRELUDE.len() + BUILTIN_TYPES.len());
    }

    /// s212: every mark is carried by at least one builtin, and every
    /// carrier is a host builtin.
    #[test]
    fn marks_come_from_host_rows() {
        let ms = marks();
        assert!(!ms.is_empty());
        for m in &ms {
            assert!(!m.by.is_empty(), "{}", m.name);
            for b in &m.by {
                let row = PRELUDE
                    .iter()
                    .find(|a| a.name == *b)
                    .expect("a prelude row");
                assert_eq!(row.anchor, Some(HOST_SIGS_ANCHOR), "{b}");
            }
        }
        assert!(ms.windows(2).all(|w| w[0].name < w[1].name), "sorted");
    }
}
