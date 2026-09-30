//! s162 — #146's stale half, from source: the release tier's overlap
//! guard must be built from the len CURRENT at the loop entry.
//!
//! `shape` grows `x` by in-place `push` and then writes `x[i]` from
//! `c[i]` twice around the store. The indexed loop versions with an
//! overlap guard; at trunk 30731a6 the guard's extent for `x` was the
//! len load ABOVE the first push (it dominates the guard, so no
//! verifier objects), and an extent built from a stale len passes the
//! guard on a pair that overlaps.
//!
//! s187 (wolf-lang#477): s162's witness no longer compiled. It moved a
//! `read` parameter (`var x = x0`) and wrote through it, which the
//! memory checker has refused at `mem` (E1014 ×4, E1002) on every lane
//! since, so this gate compared `fail(E1014)` with `fail(E1014)` and
//! measured nothing. The witness is respelled in today's rules
//! (`var x = copy x0`, called as `shape(c, c)`): it compiles and runs,
//! and the native verdict is asserted to be `exit(0)` with the known
//! sum before any comparison, so a refusal can never pass as agreement.
//!
//! The respelling cannot make the wrong answer visible, and that is why
//! the second test exists. s162's witness relied on `copy` of a List
//! being the operand itself on the native tiers (#384), so the two
//! lists shared one buffer and the stale guard let the noalias loop run
//! over it (release `sum 64`, native `sum 72`). #384 is fixed and the
//! move checker forbids two live Lists sharing a buffer (#93), so from
//! safe source the pair never overlaps and the fast and slow loops
//! answer alike whatever the guard says. With #146 planted back
//! (`overlap_pairs` taking the first len load, `len_is_current`
//! dropped), the behavioural test stays green — measured, s187. The
//! structural test reads the release mid-end's module
//! (`WOLF_MIDEND_DUMP=1`) and asserts the property #146 broke: every
//! overlap guard in `@shape` builds its extent from a len load laid out
//! after the last in-place `push`. That test goes red under the plant.
//!
//! Skips follow the s59 posture: environment exit-2 and the release
//! tier's named host refusal are loud skips, never verdicts; an ICE
//! fails (wolf-lang#471).

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

const SRC: &str = "fn shape(x0: List[int], c: List[int]) -> List[int] {\n\
\x20   var x = copy x0\n\
\x20   (mut x).push(0)\n\
\x20   var i: int = 1\n\
\x20   while i < 8 {\n\
\x20       (mut x).push(i)\n\
\x20       i = i + 1\n\
\x20   }\n\
\x20   i = 0\n\
\x20   while i < 8 {\n\
\x20       x[i] = c[i] + 1\n\
\x20       x[i] = x[i] + c[i]\n\
\x20       i = i + 1\n\
\x20   }\n\
\x20   x\n\
}\n\
\n\
fn total(xs: List[int]) -> int {\n\
\x20   var s: int = 0\n\
\x20   for v in xs {\n\
\x20       s = s + v\n\
\x20   }\n\
\x20   s\n\
}\n\
\n\
fn main() -> int {\n\
\x20   var c = List[int]()\n\
\x20   var i: int = 0\n\
\x20   while i < 8 {\n\
\x20       (mut c).push(i)\n\
\x20       i = i + 1\n\
\x20   }\n\
\x20   let out = shape(c, c)\n\
\x20   print(\"sum {total(out)}\")\n\
\x20   0\n\
}\n";

/// `x` is `c` (0..7) copied and grown by 0..7; the loop rewrites its
/// first eight to `2*c[i] + 1`: 64 + 28.
const WANT: &str = "sum 92\n";

/// A private directory per test (they run in parallel, and one
/// directory is one module, D32).
fn fixture(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("release_overlap_guard")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("prog.lu"), SRC).expect("write witness");
    dir
}

fn ensure_rt_staticlib() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let status = Command::new(env!("CARGO"))
            .args(["build", "-p", "wolf_rt"])
            .status()
            .expect("cargo builds wolf_rt");
        assert!(status.success(), "wolf_rt staticlib build failed");
    });
}

/// One conform-run lane: (verdict, stdout), `None` on a loud skip.
fn lane(src: &Path, flag: &str) -> Option<(String, String)> {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(src)
        .args([flag, "--json"])
        .output()
        .expect("wolf runs");
    if lane_exit::environment_refusal(&out, &format!("wolf {flag}")) {
        eprintln!(
            "SKIP: environment cannot run {flag}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "conform-run {flag} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rec: serde_json::Value = serde_json::from_slice(&out.stdout).expect("record parses");
    if rec["verdict"] == "unsupported"
        && String::from_utf8_lossy(&out.stderr).contains("release tier targets")
    {
        eprintln!("SKIP: the release tier refuses this host");
        return None;
    }
    Some((
        rec["verdict"].as_str().unwrap_or("").to_string(),
        rec["stdout_inline"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| rec["stdout_sha256"].as_str().unwrap_or("").to_string()),
    ))
}

#[test]
fn release_overlap_guard_agrees_with_native_on_a_grown_list() {
    ensure_rt_staticlib();
    let dir = fixture("agree");
    let src = dir.join("prog.lu");
    let Some(native) = lane(&src, "--native") else {
        return;
    };
    // wolf-lang#477: a refusal on both lanes is agreement about nothing.
    // The witness must run, and native must print the known sum.
    assert_eq!(
        native,
        ("exit(0)".to_string(), WANT.to_string()),
        "the #146 witness must compile and run on the native tier before \
         its release answer means anything (wolf-lang#477)"
    );
    let Some(release) = lane(&src, "--release") else {
        return;
    };
    assert_eq!(
        release, native,
        "wolf-lang#146: the release tier's versioned loop disagrees with the \
         native tier — an overlap guard built from a stale len lets the fast \
         path run over storage the guard should have seen overlap"
    );
}

/// The release mid-end's module as `WOLF_MIDEND_DUMP=1` prints it, or
/// `None` on a loud skip (an environment refusal, or a host the release
/// tier refuses by name).
fn release_module_dump(src: &Path, out: &Path) -> Option<String> {
    let run = Command::new(wolf())
        .arg("build")
        .arg(src)
        .arg("--release")
        .arg("-o")
        .arg(out)
        .env("WOLF_MIDEND_DUMP", "1")
        .output()
        .expect("wolf runs");
    let stderr = String::from_utf8_lossy(&run.stderr).to_string();
    if lane_exit::environment_refusal(&run, "wolf build --release") {
        eprintln!(
            "SKIP: environment cannot build the release tier: {}",
            stderr.trim()
        );
        return None;
    }
    if !run.status.success() && stderr.contains("release tier targets") {
        eprintln!("SKIP: the release tier refuses this host");
        return None;
    }
    assert!(
        run.status.success(),
        "wolf build --release failed on the #146 witness: {stderr}"
    );
    Some(stderr)
}

/// The lines of `fn @name`'s body in a printed WIR module.
fn function_body<'a>(module: &'a str, name: &str) -> Vec<&'a str> {
    let open = format!("fn @{name}(");
    let mut lines = module.lines().skip_while(|l| !l.starts_with(&open));
    assert!(
        lines.next().is_some(),
        "no `fn @{name}` in the release module dump"
    );
    lines.take_while(|l| !l.starts_with('}')).collect()
}

/// The instruction that defines `%v` in `body`: (its line index, the
/// right-hand side after `=`).
fn def_of<'a>(body: &[&'a str], v: &str) -> (usize, &'a str) {
    let bare = format!("{v} = ");
    let typed = format!("{v}: ");
    body.iter()
        .enumerate()
        .find_map(|(i, l)| {
            let l = l.trim_start();
            if l.starts_with(&bare) || (l.starts_with(&typed) && l.contains(" = ")) {
                Some((i, l.split_once(" = ").expect("a definition").1))
            } else {
                None
            }
        })
        .unwrap_or_else(|| panic!("no definition of {v} in @shape"))
}

/// The operands of an instruction's right-hand side: `op %a, %b, 8`.
fn operands(rhs: &str) -> Vec<&str> {
    rhs.split_once(' ')
        .map(|(_, rest)| rest.split(", ").map(str::trim).collect())
        .unwrap_or_default()
}

/// wolf-lang#146, asserted on what the release mid-end builds: every
/// overlap guard in `@shape` (`fact noalias %a %b : guard %g`, `%g` the
/// `bor` of two `icmp.ule` over the pair's extents) builds each extent
/// end `ptr.off %base, %len, 8` from a len laid out AFTER the last
/// in-place grow (`call @__wolf_rt_list_push`) — the len current at the
/// loop, never the load above the first push. Seen red with #146
/// planted back (s187, wolf-lang#477): the extent read the load in the
/// entry block.
#[test]
fn the_overlap_guard_extent_reads_the_len_current_at_the_loop() {
    ensure_rt_staticlib();
    let dir = fixture("extent");
    let src = dir.join("prog.lu");
    let Some(module) = release_module_dump(&src, &dir.join("prog.bin")) else {
        return;
    };
    let body = function_body(&module, "shape");
    let last_push = body
        .iter()
        .rposition(|l| l.contains("call @__wolf_rt_list_push("))
        .expect("@shape grows its list in place: a push call in the dump");
    let guards: Vec<&str> = body
        .iter()
        .filter_map(|l| {
            let l = l.trim_start();
            let rest = l.strip_prefix("fact noalias ")?;
            Some(rest.split_once(" : guard ")?.1.trim())
        })
        .collect();
    assert!(
        !guards.is_empty(),
        "the release mid-end minted no overlap guard in @shape — the #146 \
         witness no longer reaches the versioner, and this gate measures \
         nothing (wolf-lang#477)"
    );
    let mut extents = 0;
    for g in guards {
        let (_, bor) = def_of(&body, g);
        assert!(bor.starts_with("bor "), "guard {g} is `{bor}`, not a bor");
        for cmp in operands(bor) {
            let (_, ule) = def_of(&body, cmp);
            assert!(
                ule.starts_with("icmp.ule "),
                "guard operand {cmp} is `{ule}`"
            );
            let end = operands(ule)[0];
            let (_, off) = def_of(&body, end);
            let ops = operands(off);
            assert!(
                off.starts_with("ptr.off ") && ops.len() == 3,
                "extent end {end} is `{off}`"
            );
            let len = ops[1];
            let (at, load) = def_of(&body, len);
            assert!(
                load.starts_with("load.i64 "),
                "the extent's len {len} is `{load}`, not a header load"
            );
            assert!(
                at > last_push,
                "wolf-lang#146: overlap guard {g}'s extent end {end} = `{off}` \
                 reads the len {len} = `{load}` (line {at} of @shape), laid out \
                 above the last in-place push (line {last_push}) — a stale \
                 len, which passes the guard on a pair that overlaps"
            );
            extents += 1;
        }
    }
    assert!(extents >= 2, "no extent was checked");
}
