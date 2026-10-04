//! kw07 (K3 = B, STATUS #31): `[mem.unsafe.volatile.2]` proved in the
//! object. Each `p.read_volatile()` / `p.write_volatile(v)` is exactly
//! one access of the pointee's width, never elided, split, merged or
//! reordered against another volatile access — on the native tier
//! (Cranelift, no mid-end) and the release tier (the WIR mid-end, then
//! LLVM at `-O2`), for the hosted target and for `x86_64-unknown-none`
//! (kw04).
//!
//! The functions are `tests/fixtures/volatile/vol.lu`'s `export fn`s
//! (kw02: unmangled, kept by `--release`), built beside `main.lu`
//! (hosted) or `kmain.lu` (freestanding) — D32 makes a directory one
//! module. For each object the gate disassembles every function and
//! counts its MEMORY ACCESSES: instructions with a memory operand whose
//! base register is not `rsp`/`rbp`/`rip` (the frame and the constant
//! pool are not the pointer). Then, per function:
//!
//! - `vr_T` / `vw_T` (the eight fixed-width integers and `byte`): one
//!   load / one store, of `T`'s width;
//! - `vw_twice` / `vr_twice`: two stores / two loads of 4 bytes, where
//!   the release tier's ordinary-access controls (`ord_w_twice`,
//!   `ord_r_twice`) have ONE — the merge the optimizer makes when it may;
//! - `vr_unused`: the load whose value is dropped is still there;
//! - `vw_order`: three stores with immediates 0x11, 0x22, 0x33, in that
//!   order;
//! - `vw_loop` (a hundred stores to one address): a store inside a
//!   backward branch's range, where the release control `ord_w_loop`
//!   has no loop at all and one store — the fold;
//! - `vr_poll` (`while p.read_volatile() == 0`): a load inside the loop,
//!   where the release control `ord_r_poll` reads once, before it.
//!
//! The controls make the loop and merge rows mean something: a row is
//! only evidence if the ordinary spelling of the same shape IS folded
//! on the same tier, so the gate asserts that too.
//!
//! At trunk 8e36bc1a every build here failed (`unsupported — this
//! raw-pointer operation`), so every row was red. A failed build FAILS
//! the gate; there is no skip in this file (wolf-lang#550). The
//! disassembly needs an x86-64 linux host's `objdump` and compiles only
//! there; the object rows run on every host.

// The disassembly half compiles only on the x86-64 linux host; its
// helpers are unused elsewhere (the macOS and windows clippy lanes).
#![cfg_attr(
    not(all(target_os = "linux", target_arch = "x86_64")),
    allow(dead_code, unused_imports)
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FREESTANDING: &str = "x86_64-unknown-none";

/// The volatile pointees and their widths in bytes.
const WIDTHS: [(&str, u32); 9] = [
    ("u8", 1),
    ("u16", 2),
    ("u32", 4),
    ("u64", 8),
    ("i8", 1),
    ("i16", 2),
    ("i32", 4),
    ("i64", 8),
    ("byte", 1),
];

/// Every `export fn` in `vol.lu`.
fn exported() -> Vec<String> {
    let mut v: Vec<String> = WIDTHS
        .iter()
        .flat_map(|(t, _)| [format!("vr_{t}"), format!("vw_{t}")])
        .collect();
    for f in [
        "vw_twice",
        "ord_w_twice",
        "vr_twice",
        "ord_r_twice",
        "vr_unused",
        "vw_order",
        "vw_loop",
        "ord_w_loop",
        "vr_poll",
        "ord_r_poll",
    ] {
        v.push(f.to_string());
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
        .join("tests/fixtures/volatile")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

/// `vol.lu` plus the entry file, alone in a scratch directory.
fn staged(case: &str, entry: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("volatile_disasm")
        .join(case);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    for f in ["vol.lu", entry] {
        std::fs::copy(fixture(f), dir.join(f)).expect("copy fixture");
    }
    dir
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Target {
    Hosted,
    Freestanding,
}

/// Build the object on `tier` (`native` | `release`). Any failure fails
/// the gate: this build has no host it may be skipped on.
fn build_obj(target: Target, tier: &str, test: &str) -> PathBuf {
    // One scratch directory per (test, target, tier): tests run in
    // parallel and `staged` starts from an empty directory.
    let (case, entry) = match target {
        Target::Hosted => (format!("{test}_hosted_{tier}"), "main.lu"),
        Target::Freestanding => (format!("{test}_free_{tier}"), "kmain.lu"),
    };
    let dir = staged(&case, entry);
    let obj = dir.join("vol.o");
    let o = obj.to_str().unwrap().to_string();
    let mut args = vec!["build", "vol.lu", "--emit=obj", "-o", &o];
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

/// The defined global symbols of an object.
fn globals(obj: &Path) -> Vec<String> {
    use object::{Object, ObjectSymbol};
    let bytes = std::fs::read(obj).expect("read object");
    let file = object::File::parse(&*bytes).expect("parse object");
    file.symbols()
        .filter(|s| s.is_global() && !s.is_undefined())
        .filter_map(|s| s.name().ok().map(str::to_string))
        .collect()
}

/// Every host: the freestanding objects build on both tiers and define
/// every export under its own name (the native tier emits x86-64 from
/// any host; the disassembly below is the x86-64 linux host's).
#[test]
fn every_volatile_function_compiles_for_the_freestanding_target() {
    for tier in ["native", "release"] {
        let obj = build_obj(Target::Freestanding, tier, "exports");
        let g = globals(&obj);
        let missing: Vec<String> = exported().into_iter().filter(|f| !g.contains(f)).collect();
        assert!(
            missing.is_empty(),
            "{tier}: exports missing from the object: {missing:?} (globals {g:?})"
        );
    }
}

/// One instruction: its offset, the mnemonic and the operand text
/// (objdump's Intel syntax).
#[derive(Clone, Debug)]
struct Insn {
    at: u64,
    mnemonic: String,
    ops: String,
}

/// A memory access through a pointer (not the frame, not `rip`).
#[derive(Clone, Debug, PartialEq)]
struct Access {
    at: u64,
    store: bool,
    width: u32,
    /// The instruction, for the failure message.
    text: String,
}

impl Insn {
    /// The access this instruction makes through a pointer register,
    /// if any. A store is a `mov` whose destination (first operand) is
    /// the memory operand; any other memory operand is a load (`mov`,
    /// `movzx`, `movsx`, `cmp`, `add` with a memory source…).
    fn access(&self) -> Option<Access> {
        // A multi-byte `nop` (`data16 cs nop WORD PTR [rax+rax*1]`,
        // loop alignment) names a memory operand and touches nothing.
        if self.mnemonic.contains("nop") || self.ops.contains("nop") {
            return None;
        }
        let (width, rest) = [
            ("BYTE PTR [", 1u32),
            ("WORD PTR [", 2),
            ("DWORD PTR [", 4),
            ("QWORD PTR [", 8),
        ]
        .iter()
        .filter_map(|(k, w)| {
            // `DWORD PTR` contains `WORD PTR`; find the longest match.
            let i = self.ops.find(k)?;
            if *k == "WORD PTR [" && i > 0 && self.ops[..i].ends_with('D') {
                return None;
            }
            if *k == "WORD PTR [" && i > 0 && self.ops[..i].ends_with('Q') {
                return None;
            }
            Some((*w, &self.ops[i + k.len()..]))
        })
        .next()?;
        let base = rest.split([']', '+', '-', '*']).next().unwrap_or("").trim();
        if matches!(base, "rsp" | "rbp" | "rip") || base.is_empty() {
            return None;
        }
        let store = self.mnemonic == "mov"
            && self
                .ops
                .split(',')
                .next()
                .is_some_and(|d| d.contains("PTR ["));
        Some(Access {
            at: self.at,
            store,
            width,
            text: format!("{} {}", self.mnemonic, self.ops),
        })
    }

    /// `Some(target)` when this is a jump to a lower address — a loop's
    /// back edge.
    fn back_edge(&self) -> Option<u64> {
        if !self.mnemonic.starts_with('j') {
            return None;
        }
        let target = u64::from_str_radix(self.ops.split_whitespace().next()?, 16).ok()?;
        (target <= self.at).then_some(target)
    }
}

/// One function's code: its instructions, cut where its constant pool
/// begins (Cranelift places a function's constants after its code,
/// inside the symbol's range, where they disassemble as junk — the
/// first `rip`-relative reference into the function's own range marks
/// where the data starts).
#[derive(Debug)]
struct Func {
    insns: Vec<Insn>,
}

impl Func {
    fn accesses(&self) -> Vec<Access> {
        self.insns.iter().filter_map(Insn::access).collect()
    }

    fn loops(&self) -> Vec<(u64, u64)> {
        self.insns
            .iter()
            .filter_map(|i| i.back_edge().map(|t| (t, i.at)))
            .collect()
    }

    fn in_loop(&self, at: u64) -> bool {
        self.loops().iter().any(|&(lo, hi)| (lo..=hi).contains(&at))
    }

    fn listing(&self) -> String {
        self.insns
            .iter()
            .map(|i| format!("  {:x}: {} {}", i.at, i.mnemonic, i.ops))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Every defined symbol's (address, size) in an object: the bound of a
/// function's code (objdump prints the alignment padding after a
/// function under its name, and zero padding decodes as
/// `add BYTE PTR [rax],al`).
fn extents(obj: &Path) -> BTreeMap<String, (u64, u64)> {
    use object::{Object, ObjectSymbol};
    let bytes = std::fs::read(obj).expect("read object");
    let file = object::File::parse(&*bytes).expect("parse object");
    file.symbols()
        .filter(|s| !s.is_undefined() && s.size() > 0)
        .filter_map(|s| Some((s.name().ok()?.to_string(), (s.address(), s.size()))))
        .collect()
}

/// Parse `objdump -d -M intel --no-show-raw-insn` into functions, each
/// bounded by its symbol's size when `extents` names it.
fn parse_objdump(dis: &str, extents: &BTreeMap<String, (u64, u64)>) -> BTreeMap<String, Func> {
    let mut out: BTreeMap<String, Vec<Insn>> = BTreeMap::new();
    let mut cur: Option<String> = None;
    for line in dis.lines() {
        if let Some(name) = line
            .strip_suffix(">:")
            .and_then(|l| l.split_once(" <"))
            .map(|(_, n)| n.to_string())
        {
            out.entry(name.clone()).or_default();
            cur = Some(name);
            continue;
        }
        let Some(name) = &cur else { continue };
        let Some((addr, rest)) = line.split_once(":\t") else {
            continue;
        };
        let Ok(at) = u64::from_str_radix(addr.trim(), 16) else {
            continue;
        };
        let rest = rest.trim();
        let (mnemonic, ops) = match rest.split_once(char::is_whitespace) {
            Some((m, o)) => (m.to_string(), o.trim().to_string()),
            None => (rest.to_string(), String::new()),
        };
        out.get_mut(name).unwrap().push(Insn { at, mnemonic, ops });
    }
    out.into_iter()
        .map(|(name, insns)| {
            let insns: Vec<Insn> = match extents.get(&name) {
                Some(&(at, size)) => insns
                    .into_iter()
                    .filter(|i| i.at >= at && i.at < at + size)
                    .collect(),
                None => insns,
            };
            let lo = insns.first().map_or(0, |i| i.at);
            let hi = insns.last().map_or(0, |i| i.at);
            // `# 3d8 <vw_loop+0x88>` — a rip-relative target in range.
            let pool = insns
                .iter()
                .filter_map(|i| {
                    let (_, c) = i.ops.split_once("# ")?;
                    let t = u64::from_str_radix(c.split_whitespace().next()?, 16).ok()?;
                    (i.ops.contains("rip") && t > lo && t <= hi).then_some(t)
                })
                .min();
            let insns = match pool {
                Some(p) => insns.into_iter().filter(|i| i.at < p).collect(),
                None => insns,
            };
            (name, Func { insns })
        })
        .collect()
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod linux_x86_64 {
    use super::*;

    fn disassemble(obj: &Path) -> BTreeMap<String, Func> {
        let out: Output = Command::new("objdump")
            .args(["-d", "-M", "intel", "--no-show-raw-insn"])
            .arg(obj)
            .output()
            .unwrap_or_else(|e| panic!("`objdump` is part of this gate's host (no skip): {e}"));
        assert!(out.status.success(), "objdump: {}", text(&out.stderr));
        let funcs = parse_objdump(&text(&out.stdout), &extents(obj));
        for f in exported() {
            assert!(
                funcs.contains_key(&f),
                "{}: no `{f}` in the disassembly",
                obj.display()
            );
        }
        funcs
    }

    /// One function's accesses must be exactly `want` (store?, width),
    /// in order.
    fn assert_accesses(
        funcs: &BTreeMap<String, Func>,
        name: &str,
        want: &[(bool, u32)],
        why: &str,
        failures: &mut Vec<String>,
    ) {
        let f = &funcs[name];
        let got: Vec<(bool, u32)> = f.accesses().iter().map(|a| (a.store, a.width)).collect();
        if got != want {
            failures.push(format!(
                "{name}: want {want:?} (store?, width) — {why}; got {got:?}\n{}",
                f.listing()
            ));
        }
    }

    /// Every volatile row on one object.
    fn volatile_rows(funcs: &BTreeMap<String, Func>, failures: &mut Vec<String>) {
        for (t, w) in WIDTHS {
            assert_accesses(
                funcs,
                &format!("vr_{t}"),
                &[(false, w)],
                "one load of the width",
                failures,
            );
            assert_accesses(
                funcs,
                &format!("vw_{t}"),
                &[(true, w)],
                "one store of the width",
                failures,
            );
        }
        assert_accesses(
            funcs,
            "vw_twice",
            &[(true, 4), (true, 4)],
            "two writes of one value are two stores (never merged)",
            failures,
        );
        assert_accesses(
            funcs,
            "vr_twice",
            &[(false, 4), (false, 4)],
            "two reads are two loads (never CSE'd)",
            failures,
        );
        assert_accesses(
            funcs,
            "vr_unused",
            &[(false, 4)],
            "a read whose value is dropped still reads (never elided)",
            failures,
        );
        // Order: the three stores' immediates in program order.
        let order: Vec<String> = funcs["vw_order"]
            .accesses()
            .iter()
            .map(|a| a.text.rsplit(',').next().unwrap_or("").trim().to_string())
            .collect();
        if order != ["0x11", "0x22", "0x33"] {
            failures.push(format!(
                "vw_order: want stores of 0x11, 0x22, 0x33 in that order; got {order:?}\n{}",
                funcs["vw_order"].listing()
            ));
        }
        // The loop keeps a store per iteration.
        let lp = &funcs["vw_loop"];
        let stores: Vec<Access> = lp.accesses().into_iter().filter(|a| a.store).collect();
        if stores.is_empty()
            || !stores.iter().all(|a| a.width == 4 && lp.in_loop(a.at))
            || lp.accesses().iter().any(|a| !a.store)
        {
            failures.push(format!(
                "vw_loop: want 4-byte stores, every one inside the loop, and no load; got \
                 {stores:?}, loops {:?}\n{}",
                lp.loops(),
                lp.listing()
            ));
        }
        // The poll reads on every iteration.
        let pl = &funcs["vr_poll"];
        let loads: Vec<Access> = pl.accesses();
        if loads.is_empty()
            || !loads
                .iter()
                .all(|a| !a.store && a.width == 4 && pl.in_loop(a.at))
        {
            failures.push(format!(
                "vr_poll: want 4-byte loads, every one inside the loop; got {loads:?}, loops \
                 {:?}\n{}",
                pl.loops(),
                pl.listing()
            ));
        }
    }

    /// The release tier's controls: the ordinary spelling of each shape
    /// IS folded, so the volatile rows beside them are evidence.
    fn release_controls(funcs: &BTreeMap<String, Func>, failures: &mut Vec<String>) {
        assert_accesses(
            funcs,
            "ord_w_twice",
            &[(true, 4)],
            "the control: two ordinary stores of one value merge",
            failures,
        );
        assert_accesses(
            funcs,
            "ord_r_twice",
            &[(false, 4)],
            "the control: two ordinary reads CSE",
            failures,
        );
        let lp = &funcs["ord_w_loop"];
        if !lp.loops().is_empty() || lp.accesses().len() != 1 {
            failures.push(format!(
                "ord_w_loop: the control must fold to one store and no loop; got {:?}, loops \
                 {:?}\n{}",
                lp.accesses(),
                lp.loops(),
                lp.listing()
            ));
        }
        let pl = &funcs["ord_r_poll"];
        let acc = pl.accesses();
        if acc.len() != 1 || pl.in_loop(acc[0].at) {
            failures.push(format!(
                "ord_r_poll: the control's one load must be hoisted out of the loop; got \
                 {acc:?}, loops {:?}\n{}",
                pl.loops(),
                pl.listing()
            ));
        }
    }

    fn gate(target: Target, tier: &str) {
        let obj = build_obj(target, tier, "disasm");
        let funcs = disassemble(&obj);
        let mut failures = Vec::new();
        volatile_rows(&funcs, &mut failures);
        if tier == "release" {
            release_controls(&funcs, &mut failures);
        }
        assert!(
            failures.is_empty(),
            "[mem.unsafe.volatile.2] on {target:?} {tier} ({}): {} row(s) red:\n{}",
            obj.display(),
            failures.len(),
            failures.join("\n\n")
        );
    }

    #[test]
    fn native_hosted_one_access_per_call() {
        gate(Target::Hosted, "native");
    }

    #[test]
    fn release_hosted_one_access_per_call_and_the_controls_fold() {
        gate(Target::Hosted, "release");
    }

    #[test]
    fn native_freestanding_one_access_per_call() {
        gate(Target::Freestanding, "native");
    }

    #[test]
    fn release_freestanding_one_access_per_call_and_the_controls_fold() {
        gate(Target::Freestanding, "release");
    }
}

/// The parser and the access classifier on objdump's own text, so the
/// gate's instrument is itself tested on every host.
#[test]
fn the_instrument_reads_objdump_intel_syntax() {
    let dis = "\
0000000000000050 <vw_twice>:
  50:\tmov    DWORD PTR [rdi],0x7
  56:\tmov    QWORD PTR [rsp],rdi
  5a:\tmovzx  eax,WORD PTR [rdi]
  5e:\tcmp    DWORD PTR [rdi],0x0
  62:\tmov    rsi,QWORD PTR [rip+0x10]        # 70 <vw_twice+0x20>
  66:\tjne    5a <vw_twice+0xa>
  67:\tdata16 cs nop WORD PTR [rax+rax*1+0x0]
  68:\tmov    BYTE PTR [rbp-0x8],al
  6c:\tret
  70:\tadd    BYTE PTR [rax],al
  74:\tadd    BYTE PTR [rax],al
";
    // The symbol ends before the padding at 0x74.
    let ext: BTreeMap<String, (u64, u64)> = [("vw_twice".to_string(), (0x50, 0x24))].into();
    let funcs = parse_objdump(dis, &ext);
    let f = &funcs["vw_twice"];
    let acc: Vec<(u64, bool, u32)> = f
        .accesses()
        .iter()
        .map(|a| (a.at, a.store, a.width))
        .collect();
    // The frame store, the rip load and the pool junk at 0x70 are not
    // accesses through the pointer.
    assert_eq!(
        acc,
        vec![(0x50, true, 4), (0x5a, false, 2), (0x5e, false, 4)]
    );
    assert_eq!(f.loops(), vec![(0x5a, 0x66)]);
    assert!(f.in_loop(0x5e) && !f.in_loop(0x50));
}
