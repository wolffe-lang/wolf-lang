//! kw11 (K5 = A, STATUS #31): `[conc.mm.atomic.raw.3]` and
//! `[conc.mm.fence]` proved in the object. Each atomic operation is its
//! atomic instruction(s) of the pointee's width, in place, on the native
//! tier (Cranelift: every order as `seq_cst`) and the release tier (the
//! WIR mid-end, then LLVM with the order as written), for the hosted
//! target and for `x86_64-unknown-none` (kw04), on x86-64 (the linux
//! host) and aarch64 (the macOS host).
//!
//! The functions are `tests/fixtures/atomic/atom.lu`'s `export fn`s
//! (kw02: unmangled, kept by `--release`): `a_<op>_<T>_<order>` is one
//! operation through the pointer argument, `f_<order>` one fence, and
//! `spin_*` the scheduler's two loops. For each object the gate
//! disassembles every function and reduces it to a SIGNATURE: its atomic
//! instructions (`lock`-prefixed, `xchg` with memory, `mfence`; on
//! aarch64 the acquire/release, exclusive and LSE forms and `dmb`) and its
//! plain loads and stores through a pointer register (not the frame),
//! each with its width (x86-64) or its width-suffixed mnemonic
//! (aarch64), and `@L` when it sits inside a backward branch's range.
//! Every row's signature must be exactly the one [`expect`] names —
//! which is the measured lowering, stated as the clause's promise: one
//! operation, its width, its order's instruction, never split, elided or
//! moved out of a loop. The whole table is printed (`REPORT`) on every
//! run.
//!
//! On every host the objects must build and import nothing an atomic
//! could need (`__atomic_*`, `__sync_*`, `__aarch64_*` outline helpers,
//! a `wolf.` intrinsic name): the instructions are in place.
//!
//! At trunk a3465f87 every build here failed (exit 2: four E0301
//! "nothing named `fence` is in scope"; without the fences, `unsupported`
//! "this raw-pointer operation"), so every row was red. A failed build FAILS the
//! gate; there is no skip in this file (wolf-lang#550).

#![cfg_attr(
    not(any(
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64")
    )),
    allow(dead_code, unused_imports)
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

const FREESTANDING: &str = "x86_64-unknown-none";

const TYPES: [(&str, u32); 8] = [
    ("u8", 1),
    ("u16", 2),
    ("u32", 4),
    ("u64", 8),
    ("i8", 1),
    ("i16", 2),
    ("i32", 4),
    ("i64", 8),
];
const ORDERS: [&str; 5] = ["relaxed", "acquire", "release", "acq_rel", "seq_cst"];
const RMW: [&str; 6] = ["swap", "add", "sub", "and", "or", "xor"];
const CAS: [(&str, &str); 9] = [
    ("relaxed", "relaxed"),
    ("acquire", "relaxed"),
    ("acquire", "acquire"),
    ("release", "relaxed"),
    ("acq_rel", "relaxed"),
    ("acq_rel", "acquire"),
    ("seq_cst", "relaxed"),
    ("seq_cst", "acquire"),
    ("seq_cst", "seq_cst"),
];

/// One row: a fixture function and what it is.
#[derive(Clone, Debug)]
struct Row {
    name: String,
    /// `load`, `store`, a read-modify-write, `cas`, `fence`, or `spin_*`.
    op: String,
    width: u32,
    order: String,
    failure: Option<String>,
}

fn row(op: &str, t: &str, width: u32, order: &str, failure: Option<&str>) -> Row {
    let mut name = format!("a_{op}_{t}_{order}");
    if let Some(f) = failure {
        name = format!("{name}_{f}");
    }
    Row {
        name,
        op: op.to_string(),
        width,
        order: order.to_string(),
        failure: failure.map(str::to_string),
    }
}

/// Every row of `atom.lu`, in its generator's order.
fn rows() -> Vec<Row> {
    let mut v = Vec::new();
    for (t, w) in TYPES {
        for op in ["load", "store"].into_iter().chain(RMW) {
            v.push(row(op, t, w, "seq_cst", None));
        }
        v.push(row("cas", t, w, "seq_cst", Some("seq_cst")));
    }
    for o in ["relaxed", "acquire"] {
        v.push(row("load", "u64", 8, o, None));
    }
    for o in ["relaxed", "release"] {
        v.push(row("store", "u64", 8, o, None));
    }
    for op in RMW {
        for o in &ORDERS[..4] {
            v.push(row(op, "u64", 8, o, None));
        }
    }
    for (s, f) in &CAS[..8] {
        v.push(row("cas", "u64", 8, s, Some(f)));
    }
    for o in ["acquire", "release", "acq_rel", "seq_cst"] {
        v.push(Row {
            name: format!("f_{o}"),
            op: "fence".into(),
            width: 0,
            order: o.into(),
            failure: None,
        });
    }
    for s in ["spin_acquire", "spin_lock", "spin_unlock"] {
        v.push(Row {
            name: s.into(),
            op: s.into(),
            width: 4,
            order: String::new(),
            failure: None,
        });
    }
    v
}

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/atomic")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Target {
    Hosted,
    Freestanding,
}

/// Build the object on `tier` (`native` | `release`) in its own scratch
/// directory. Any failure fails the gate: there is no host this build
/// may be skipped on.
fn build_obj(target: Target, tier: &str, test: &str) -> PathBuf {
    let (case, entry) = match target {
        Target::Hosted => (format!("{test}_hosted_{tier}"), "main.lu"),
        Target::Freestanding => (format!("{test}_free_{tier}"), "kmain.lu"),
    };
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("atomic_disasm")
        .join(case);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    for f in ["atom.lu", entry] {
        std::fs::copy(fixture(f), dir.join(f)).expect("copy fixture");
    }
    let obj = dir.join("atom.o");
    let o = obj.to_str().unwrap().to_string();
    let mut args = vec!["build", "atom.lu", "--emit=obj", "-o", &o];
    if target == Target::Freestanding {
        args.extend(["--target", FREESTANDING]);
    }
    if tier == "release" {
        args.push("--release");
    }
    let out = Command::new(wolf())
        .current_dir(&dir)
        .args(&args)
        .output()
        .expect("wolf runs");
    assert!(
        out.status.success() && obj.is_file(),
        "wolf {} ({target:?}, {tier}) must build an object (exit {:?}) — a refusal or a \
         compile error is the gate failing, never a skip:\n{}",
        args.join(" "),
        out.status.code(),
        text(&out.stderr)
    );
    obj
}

/// The object's defined globals (leading `_` of Mach-O stripped) and its
/// undefined symbols.
fn symbols(obj: &Path) -> (Vec<String>, Vec<String>) {
    use object::{Object, ObjectSymbol};
    let bytes = std::fs::read(obj).expect("read object");
    let file = object::File::parse(&*bytes).expect("parse object");
    let strip = |s: &str| {
        if file.format() == object::BinaryFormat::MachO {
            s.strip_prefix('_').unwrap_or(s).to_string()
        } else {
            s.to_string()
        }
    };
    let mut defined = Vec::new();
    let mut undefined = Vec::new();
    for s in file.symbols() {
        let Ok(n) = s.name() else { continue };
        if s.is_undefined() {
            undefined.push(strip(n));
        } else if s.is_global() {
            defined.push(strip(n));
        }
    }
    (defined, undefined)
}

fn check_object(obj: &Path, what: &str) {
    let (defined, undefined) = symbols(obj);
    let missing: Vec<String> = rows()
        .into_iter()
        .map(|r| r.name)
        .filter(|n| !defined.contains(n))
        .collect();
    assert!(
        missing.is_empty(),
        "{what}: exports missing from the object: {missing:?}"
    );
    let helpers: Vec<&String> = undefined
        .iter()
        .filter(|u| {
            u.contains("atomic")
                || u.starts_with("__sync")
                || u.starts_with("__aarch64_")
                || u.starts_with("wolf.")
        })
        .collect();
    assert!(
        helpers.is_empty(),
        "{what}: an atomic must be its instruction in place, never a call; the object \
         imports {helpers:?} (all imports {undefined:?})"
    );
}

/// Every host: the freestanding objects build on both tiers (the native
/// tier emits x86-64 from any host; the release tier's freestanding
/// object is x86-64 too), each defining every row and importing no
/// atomic helper.
#[test]
fn every_atomic_function_compiles_in_place_freestanding() {
    for tier in ["native", "release"] {
        let f = build_obj(Target::Freestanding, tier, "objects");
        check_object(&f, &format!("freestanding {tier}"));
    }
}

/// The two hosts whose hosted objects this gate disassembles: the hosted
/// objects build on both tiers, define every row and import no atomic
/// helper (windows' release tier is dark by design, s60c).
#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64")
))]
#[test]
fn every_atomic_function_compiles_in_place_hosted() {
    for tier in ["native", "release"] {
        let h = build_obj(Target::Hosted, tier, "objects");
        check_object(&h, &format!("hosted {tier}"));
    }
}

/// One instruction: offset, mnemonic (a `lock` prefix kept as part of
/// it) and operand text.
#[derive(Clone, Debug)]
struct Insn {
    at: u64,
    mnemonic: String,
    ops: String,
}

/// Parse `objdump -d --no-show-raw-insn` (GNU or LLVM) into functions,
/// keyed by symbol with Mach-O's leading `_` stripped.
fn parse_objdump(dis: &str) -> BTreeMap<String, Vec<Insn>> {
    let mut out: BTreeMap<String, Vec<Insn>> = BTreeMap::new();
    let mut cur: Option<String> = None;
    for line in dis.lines() {
        let t = line.trim();
        if let Some(name) = t
            .strip_suffix(">:")
            .and_then(|l| l.split_once(" <"))
            .map(|(_, n)| n.strip_prefix('_').unwrap_or(n).to_string())
        {
            out.entry(name.clone()).or_default();
            cur = Some(name);
            continue;
        }
        let Some(name) = &cur else { continue };
        let Some((addr, rest)) = t.split_once(':') else {
            continue;
        };
        let Ok(at) = u64::from_str_radix(addr.trim(), 16) else {
            continue;
        };
        let rest = rest.trim();
        if rest.is_empty() {
            continue;
        }
        let (mut mnemonic, mut ops) = match rest.split_once(char::is_whitespace) {
            Some((m, o)) => (m.to_string(), o.trim().to_string()),
            None => (rest.to_string(), String::new()),
        };
        if mnemonic == "lock" {
            let (m, o) = ops
                .split_once(char::is_whitespace)
                .map(|(m, o)| (m.to_string(), o.trim().to_string()))
                .unwrap_or((ops.clone(), String::new()));
            mnemonic = format!("lock {m}");
            ops = o;
        }
        out.get_mut(name).unwrap().push(Insn { at, mnemonic, ops });
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Arch {
    X86_64,
    Aarch64,
}

/// The backward branches' ranges: a loop is `target..=branch`.
fn loops(insns: &[Insn]) -> Vec<(u64, u64)> {
    insns
        .iter()
        .filter(|i| {
            i.mnemonic.starts_with('j')
                || i.mnemonic == "b"
                || i.mnemonic.starts_with("b.")
                || i.mnemonic.starts_with("cb")
                || i.mnemonic.starts_with("tb")
        })
        .filter_map(|i| {
            let target = i
                .ops
                .split(',')
                .next_back()?
                .split_whitespace()
                .next()?
                .trim_start_matches("0x");
            let t = u64::from_str_radix(target, 16).ok()?;
            (t <= i.at).then_some((t, i.at))
        })
        .collect()
}

/// A function's signature (see the module doc).
fn signature(arch: Arch, insns: &[Insn]) -> String {
    let ls = loops(insns);
    let in_loop = |at: u64| ls.iter().any(|&(lo, hi)| (lo..=hi).contains(&at));
    let mut toks = Vec::new();
    for i in insns {
        let tok = match arch {
            Arch::X86_64 => x86_token(i),
            Arch::Aarch64 => a64_token(i),
        };
        if let Some(t) = tok {
            toks.push(if in_loop(i.at) { format!("{t}@L") } else { t });
        }
    }
    toks.join(" ")
}

fn x86_width(ops: &str) -> Option<u32> {
    [
        ("QWORD PTR", 8u32),
        ("DWORD PTR", 4),
        ("BYTE PTR", 1),
        ("WORD PTR", 2),
    ]
    .iter()
    .find(|(k, _)| ops.contains(k))
    .map(|&(_, w)| w)
}

fn x86_token(i: &Insn) -> Option<String> {
    let m = i.mnemonic.as_str();
    if m.contains("nop") {
        return None;
    }
    if m == "mfence" || m == "lfence" || m == "sfence" {
        return Some(m.to_string());
    }
    let w = x86_width(&i.ops);
    if m.starts_with("lock ") {
        return Some(format!("{m}:{}", w.unwrap_or(0)));
    }
    // `xchg ax,ax` is a two-byte nop (alignment padding).
    let w = w?;
    let base = i.ops.split_once('[')?.1;
    let reg = base.split([']', '+', '-', '*']).next().unwrap_or("").trim();
    if matches!(reg, "rsp" | "rbp" | "rip") || reg.is_empty() {
        return None;
    }
    if m == "xchg" {
        return Some(format!("xchg:{w}"));
    }
    if m.starts_with("mov") {
        let store = i.ops.split(',').next().is_some_and(|d| d.contains(" PTR "));
        return Some(format!("{}:{w}", if store { "st" } else { "ld" }));
    }
    None
}

fn a64_token(i: &Insn) -> Option<String> {
    let m = i.mnemonic.as_str();
    if m == "dmb" {
        return Some(format!("dmb {}", i.ops.trim()));
    }
    const ATOMIC: [&str; 12] = [
        "ldar", "stlr", "ldaxr", "stlxr", "ldxr", "stxr", "ldapr", "cas", "swp", "ldadd", "ldclr",
        "ldset",
    ];
    if ATOMIC.iter().any(|a| m.starts_with(a)) || m.starts_with("ldeor") {
        return Some(m.to_string());
    }
    // A plain load or store through a pointer register (not the stack
    // or the frame record).
    if matches!(
        m,
        "ldr" | "ldrb" | "ldrh" | "ldrsb" | "ldrsh" | "ldrsw" | "str" | "strb" | "strh"
    ) {
        let base = i.ops.split_once('[')?.1;
        let reg = base.split([',', ']']).next().unwrap_or("").trim();
        if matches!(reg, "sp" | "x29" | "fp") {
            return None;
        }
        return Some(m.to_string());
    }
    None
}

/// The measured lowering every row must keep (`[conc.mm.atomic.raw.3]`).
fn expect(arch: Arch, tier: &str, r: &Row) -> String {
    let native = tier == "native";
    let w = r.width;
    match arch {
        Arch::X86_64 => match r.op.as_str() {
            "load" => format!("ld:{w}"),
            "store" if native => format!("st:{w} mfence"),
            "store" if r.order == "seq_cst" => format!("xchg:{w}"),
            "store" => format!("st:{w}"),
            "swap" => format!("xchg:{w}"),
            "add" | "sub" => format!("lock xadd:{w}"),
            "and" | "or" | "xor" => format!("ld:{w} lock cmpxchg:{w}@L"),
            "cas" => format!("lock cmpxchg:{w}"),
            "fence" if native => "mfence".into(),
            "fence" if r.order == "seq_cst" => "lock or:4".into(),
            "fence" => String::new(),
            "spin_acquire" if native => "ld:4@L ld:4@L".into(),
            "spin_acquire" => "ld:4@L ld:4".into(),
            "spin_lock" => "lock cmpxchg:4@L".into(),
            "spin_unlock" if native => "st:4 mfence".into(),
            "spin_unlock" => "st:4".into(),
            other => panic!("no x86-64 expectation for {other}"),
        },
        Arch::Aarch64 => {
            let s = match w {
                1 => "b",
                2 => "h",
                _ => "",
            };
            if native {
                return match r.op.as_str() {
                    "load" => format!("ldar{s}"),
                    "store" => format!("stlr{s}"),
                    "fence" => "dmb ish".into(),
                    "spin_acquire" => "ldar@L ldar".into(),
                    "spin_lock" => "ldaxr@L stlxr@L".into(),
                    "spin_unlock" => "stlr".into(),
                    _ => format!("ldaxr{s}@L stlxr{s}@L"),
                };
            }
            // LLVM with LSE and RCpc (Apple's default CPU): the order is
            // the instruction's suffix.
            let o = |order: &str| match order {
                "relaxed" => "",
                "acquire" => "a",
                "release" => "l",
                _ => "al",
            };
            match r.op.as_str() {
                "load" => match r.order.as_str() {
                    "relaxed" => format!("ldr{s}"),
                    "acquire" => format!("ldapr{s}"),
                    _ => format!("ldar{s}"),
                },
                "store" if r.order == "relaxed" => format!("str{s}"),
                "store" => format!("stlr{s}"),
                "swap" => format!("swp{}{s}", o(&r.order)),
                "add" | "sub" => format!("ldadd{}{s}", o(&r.order)),
                "and" => format!("ldclr{}{s}", o(&r.order)),
                "or" => format!("ldset{}{s}", o(&r.order)),
                "xor" => format!("ldeor{}{s}", o(&r.order)),
                "cas" => {
                    // LLVM merges the pair: an acquiring failure under a
                    // releasing success makes `al`.
                    let merged = match (r.order.as_str(), r.failure.as_deref()) {
                        ("release", Some("acquire")) => "acq_rel",
                        (s, _) => s,
                    };
                    format!("cas{}{s}", o(merged))
                }
                "fence" if r.order == "acquire" => "dmb ishld".into(),
                "fence" => "dmb ish".into(),
                "spin_acquire" => "ldapr@L ldr".into(),
                "spin_lock" => "casa@L".into(),
                "spin_unlock" => "stlr".into(),
                other => panic!("no aarch64 expectation for {other}"),
            }
        }
    }
}

/// Disassemble `obj`, reduce every row to its signature, print the table
/// (`REPORT`), and fail with every row that differs from [`expect`].
fn assert_rows(arch: Arch, obj: &Path, tier: &str, what: &str) {
    let mut cmd = Command::new("objdump");
    cmd.args(["-d", "--no-show-raw-insn"]);
    if arch == Arch::X86_64 {
        cmd.args(["-M", "intel"]);
    }
    let out = cmd
        .arg(obj)
        .output()
        .unwrap_or_else(|e| panic!("`objdump` is part of this gate's host (no skip): {e}"));
    assert!(out.status.success(), "objdump: {}", text(&out.stderr));
    let funcs = parse_objdump(&text(&out.stdout));
    let mut failures = Vec::new();
    for r in rows() {
        let Some(insns) = funcs.get(&r.name) else {
            failures.push(format!("{}: not in the disassembly", r.name));
            continue;
        };
        let got = signature(arch, insns);
        let want = expect(arch, tier, &r);
        eprintln!("REPORT {what} {:<28} {got}", r.name);
        if got != want {
            let listing: Vec<String> = insns
                .iter()
                .map(|i| format!("    {:x}: {} {}", i.at, i.mnemonic, i.ops))
                .collect();
            failures.push(format!(
                "{}: want `{want}`, got `{got}`\n{}",
                r.name,
                listing.join("\n")
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{what}: {} of {} rows differ:\n{}",
        failures.len(),
        rows().len(),
        failures.join("\n")
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod linux_x86_64 {
    use super::*;

    #[test]
    fn x86_64_native_hosted() {
        let o = build_obj(Target::Hosted, "native", "x86n");
        assert_rows(Arch::X86_64, &o, "native", "x86-64 native hosted");
    }

    #[test]
    fn x86_64_release_hosted() {
        let o = build_obj(Target::Hosted, "release", "x86r");
        assert_rows(Arch::X86_64, &o, "release", "x86-64 release hosted");
    }

    #[test]
    fn x86_64_native_freestanding() {
        let o = build_obj(Target::Freestanding, "native", "x86n");
        assert_rows(Arch::X86_64, &o, "native", "x86-64 native freestanding");
    }

    #[test]
    fn x86_64_release_freestanding() {
        let o = build_obj(Target::Freestanding, "release", "x86r");
        assert_rows(Arch::X86_64, &o, "release", "x86-64 release freestanding");
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod macos_aarch64 {
    use super::*;

    #[test]
    fn aarch64_native_hosted() {
        let o = build_obj(Target::Hosted, "native", "a64n");
        assert_rows(Arch::Aarch64, &o, "native", "aarch64 native hosted");
    }

    #[test]
    fn aarch64_release_hosted() {
        let o = build_obj(Target::Hosted, "release", "a64r");
        assert_rows(Arch::Aarch64, &o, "release", "aarch64 release hosted");
    }
}

#[test]
fn the_row_table_matches_the_fixture() {
    // The fixture is generated with the table; a row added to one and not
    // the other would make a row silently dark.
    let src = std::fs::read_to_string(fixture("atom.lu")).expect("read fixture");
    let defined: Vec<&str> = src
        .lines()
        .filter_map(|l| l.strip_prefix("export fn "))
        .filter_map(|l| l.split('(').next())
        .collect();
    let names: Vec<String> = rows().into_iter().map(|r| r.name).collect();
    assert_eq!(
        defined, names,
        "atom.lu's exports and the gate's rows, in order"
    );
}
