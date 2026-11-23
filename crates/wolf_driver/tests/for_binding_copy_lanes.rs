//! wolf-lang#1169 — a `for` binding holds its own value, on every machine.
//!
//! `[gram.pat.let]`: a name a `for` header binds holds its own value, never
//! a place in the walked list. Every compiling machine bound the element
//! itself, so a store through the binding, or through anything moved,
//! built, lent, handed or returned from it, wrote the list. The fix binds a
//! deep copy unless one predicate (`wolf_mem::forcopy`) shows the body only
//! reads the binding; the checked machine's `eval_for` and the lowering's
//! `lower_for_list` both ask it.
//!
//! Why a driver test beside the corpus rows: `cargo xtask corpus` runs every
//! entry on the native lane only, and this gate asks the checked machine,
//! native, release and lupin. It fails in both directions if a machine
//! stops asking the predicate:
//!
//! - a machine that never copies fails the rows: each prints what the list
//!   held after a store through the binding, and the header is the spec's
//!   answer (lupin's, where one runs);
//! - a machine that always copies fails the ledger row: a read-only walk
//!   inside a region charges that region nothing, where a copy an
//!   iteration charges the ledger (`[mem.region.account.1]`), and the
//!   escaping storing walk beside it does charge. A walk that only writes
//!   through its binding charges nothing either: its copy is made in a
//!   region of its own and freed at the end of each iteration, which the
//!   native peak-memory row holds by name.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// The corpus rows: the seven stores, the eight escapes, and the copy of an
/// enum the checked machine's deep copy used to share.
const ROWS: [&str; 16] = [
    "for_binding_store",
    "for_binding_store_nested",
    "for_binding_store_read",
    "for_binding_store_map",
    "for_binding_store_field",
    "for_binding_store_tuple",
    "for_binding_store_arm",
    "for_binding_let_move",
    "for_binding_elem_move",
    "for_binding_struct_init",
    "for_binding_lend",
    "for_binding_var_lend",
    "for_binding_take_arg",
    "for_binding_outer_assign",
    "for_binding_return_mut",
    "copy_enum_payload",
];

const MACHINES: [&str; 3] = ["--checked", "--native", "--release"];

struct Obs {
    verdict: String,
    stdout: String,
}

fn parse_obs(bytes: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value =
        serde_json::from_slice(bytes).unwrap_or_else(|e| panic!("{what} record parses: {e}"));
    Obs {
        verdict: rec["verdict"].as_str().unwrap_or("").to_string(),
        stdout: rec["stdout_inline"].as_str().unwrap_or("").to_string(),
    }
}

/// One machine's observation of `entry`; `None` where the host cannot run
/// a compiled tier (a loud skip, refused under `WOLF_TEST_NATIVE_REQUIRE`).
fn lane(entry: &Path, flag: &str) -> Option<Obs> {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(entry)
        .arg(flag)
        .arg("--json")
        .output()
        .expect("wolf runs");
    if lane_exit::environment_refusal(&out, &format!("wolf {flag}")) && flag != "--checked" {
        assert!(
            std::env::var_os("WOLF_TEST_NATIVE_REQUIRE").is_none(),
            "required tier {flag} unavailable: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        eprintln!(
            "SKIP: environment cannot run the {flag} lane: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "conform-run {flag} failed on {}: {}",
        entry.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    Some(parse_obs(&out.stdout, "the observation"))
}

/// The sibling lupin, found as the other lane gates find it: `LUPIN`
/// first, then a `wolf-interp` checkout beside an ancestor.
fn sibling_lupin() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("LUPIN") {
        let p = PathBuf::from(p);
        return p.is_file().then_some(p);
    }
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    while dir.pop() {
        let candidate = dir.join("../wolf-interp/target/release/lupin");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn lupin(entry: &Path) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found"
        );
        eprintln!(
            "SKIP: no sibling lupin on this box (set LUPIN); the machines still answer the header"
        );
        return None;
    };
    let out = Command::new(&lupin)
        .arg("conform-run")
        .arg(entry)
        .arg("--json")
        .output()
        .expect("lupin runs");
    Some(parse_obs(&out.stdout, "lupin's observation"))
}

/// The `stdout="…"` of a row's `run(exit=0, …)` header, unescaped.
fn expected(src: &str) -> String {
    let line = src
        .lines()
        .find(|l| l.starts_with("//! check: run(exit=0, stdout=\""))
        .expect("a run(exit=0, stdout=…) header");
    let body = line
        .trim_start_matches("//! check: run(exit=0, stdout=\"")
        .trim_end_matches("\")");
    body.replace("\\n", "\n").replace("\\\"", "\"")
}

fn corpus(row: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/memory")
        .join(format!("{row}.lu"))
}

#[test]
fn every_row_answers_its_header_on_every_machine() {
    let mut wrong = Vec::new();
    for row in ROWS {
        let entry = corpus(row);
        let src = std::fs::read_to_string(&entry).expect("the row exists");
        let want = expected(&src);
        for m in MACHINES {
            let Some(o) = lane(&entry, m) else {
                continue;
            };
            if o.verdict != "exit(0)" || o.stdout != want {
                wrong.push(format!(
                    "{row} {m}: {} {:?}, the header {:?}",
                    o.verdict, o.stdout, want
                ));
            }
        }
        if let Some(o) = lupin(&entry)
            && (o.verdict != "exit(0)" || o.stdout != want)
        {
            wrong.push(format!(
                "{row} lupin: {} {:?}, the header {:?}",
                o.verdict, o.stdout, want
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// What each kind of walk charges the region it runs in. One that only
/// reads binds the element itself and charges nothing. One that only writes
/// through its binding binds a copy made in a region of its own, freed at
/// the end of every iteration, so it charges the walk's region nothing
/// either. One whose written binding escapes (`last = row`) binds a copy that
/// must outlive the iteration: it charges, and the list is left alone.
/// lupin's ledger does not charge its copies, so this row is the compiling
/// machines' only.
const LEDGER: &str = "fn main() -> !int {
    let rows = [[1, 2], [3, 4], [5, 6]]
    let probe = region()
    in probe {
        var total = 0
        var last = [0]
        let before = region_bytes(probe)
        for row in rows {
            total += row[0]
        }
        let read = region_bytes(probe) - before
        let mid = region_bytes(probe)
        for row in rows {
            row[0] = 0
        }
        let local = region_bytes(probe) - mid
        let late = region_bytes(probe)
        for row in rows {
            row[0] = 0
            last = row
        }
        let kept = region_bytes(probe) > late
        print(\"read={read} local={local} kept={kept} rows={rows[0][0]} last={last[0]} total={total}\")
    }
    0
}
";

#[test]
fn a_walk_that_only_reads_charges_its_region_nothing() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("for_binding_copy_lanes")
        .join("ledger");
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let entry = dir.join("main.lu");
    std::fs::write(&entry, LEDGER).expect("program written");
    let want = "read=0 local=0 kept=true rows=1 last=0 total=9\n";
    let mut wrong = Vec::new();
    for m in MACHINES {
        let Some(o) = lane(&entry, m) else {
            continue;
        };
        eprintln!("REPORT ledger {m}: {} {:?}", o.verdict, o.stdout);
        if o.verdict != "exit(0)" || o.stdout != want {
            wrong.push(format!(
                "ledger {m}: {} {:?}, want {want:?}",
                o.verdict, o.stdout
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The hot case by name (a per-file index or worktree walk): a walk that
/// builds a struct from each element's parts and pushes it binds the element
/// itself. `push` copies the pushed struct whole, so walk A charges its region
/// exactly what walk B charges, which pushes the element itself (a plain
/// argument of `push`, read at every commit). Each walk builds its own entries
/// in its own region, so no value crosses a region.
const PER_FILE: &str = "struct Entry {
    path: List[byte],
    id: List[int],
}

fn entries() -> List[Entry] {
    var es = List[Entry]()
    for i in 0..40 {
        (mut es).push(Entry { path: [1 as byte, 2 as byte, 3 as byte], id: [i, i + 1] })
    }
    es
}

fn main() -> !int {
    let a = region()
    let b = region()
    var ca = 0
    var cb = 0
    var na = 0
    var nb = 0
    in a {
        let es = entries()
        let before = region_bytes(a)
        var out = List[Entry]()
        for e in es {
            (mut out).push(Entry { path: e.path, id: e.id })
        }
        ca = region_bytes(a) - before
        na = out.len
    }
    in b {
        let es = entries()
        let before = region_bytes(b)
        var out = List[Entry]()
        for e in es {
            (mut out).push(e)
        }
        cb = region_bytes(b) - before
        nb = out.len
    }
    print(\"same={ca == cb} charged={ca > 0} items={na} {nb}\")
    0
}
";

#[test]
fn a_per_file_walk_that_pushes_a_struct_charges_what_pushing_the_element_does() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("for_binding_copy_lanes")
        .join("per_file");
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let entry = dir.join("main.lu");
    std::fs::write(&entry, PER_FILE).expect("program written");
    let want = "same=true charged=true items=40 40\n";
    let mut wrong = Vec::new();
    for m in MACHINES {
        let Some(o) = lane(&entry, m) else {
            continue;
        };
        eprintln!("REPORT per-file {m}: {} {:?}", o.verdict, o.stdout);
        if o.verdict != "exit(0)" || o.stdout != want {
            wrong.push(format!(
                "per-file {m}: {} {:?}, want {want:?}",
                o.verdict, o.stdout
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// `src` built on one tier; `None` where the host cannot build it (a loud
/// skip, refused under `WOLF_TEST_NATIVE_REQUIRE`).
#[cfg(unix)]
fn build(dir: &Path, name: &str, src: &str, release: bool) -> Option<PathBuf> {
    let at = dir.join(name);
    std::fs::create_dir_all(&at).expect("program dir");
    let entry = at.join("main.lu");
    std::fs::write(&entry, src).expect("program written");
    let exe = at.join(if release { "a-release" } else { "a-native" });
    let mut cmd = Command::new(wolf());
    cmd.arg("build")
        .arg(&entry)
        .arg("-o")
        .arg(&exe)
        .arg("--no-cache");
    if release {
        cmd.arg("--release");
    }
    let out = cmd.output().expect("wolf build runs");
    if lane_exit::environment_refusal(&out, "wolf build") {
        assert!(
            std::env::var_os("WOLF_TEST_NATIVE_REQUIRE").is_none(),
            "required tier unavailable: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        eprintln!("SKIP: {name}: this host cannot build the tier");
        return None;
    }
    assert!(
        out.status.success(),
        "wolf build {name} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Some(exe)
}

/// Reap `child` with `wait4(2)`: its raw status and its peak resident set
/// in bytes (`ru_maxrss`: kibibytes on linux, bytes on macOS).
#[cfg(unix)]
fn reap_with_peak(child: std::process::Child) -> (libc::c_int, u64) {
    let pid = child.id() as libc::pid_t;
    let mut status: libc::c_int = 0;
    // SAFETY: a zeroed `rusage` is a valid out-parameter; `wait4` fills it
    // for the child this test spawned and has not waited for.
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: plain FFI on a pid this process spawned.
    let reaped = unsafe { libc::wait4(pid, &mut status, 0, &mut ru) };
    assert_eq!(reaped, pid, "wait4 reaps exactly the child");
    let maxrss = ru.ru_maxrss as u64;
    let bytes = if cfg!(target_os = "macos") {
        maxrss
    } else {
        maxrss * 1024
    };
    (status, bytes)
}

/// The peak resident set of a run of `exe`, in bytes, and its stdout.
#[cfg(unix)]
fn peak(exe: &Path) -> (String, u64) {
    use std::io::Read as _;
    let mut child = Command::new(exe)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn the witness");
    let mut stdout = String::new();
    child
        .stdout
        .take()
        .expect("piped stdout")
        .read_to_string(&mut stdout)
        .expect("read stdout");
    let (status, bytes) = reap_with_peak(child);
    assert!(
        libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0,
        "{} did not exit 0 (status {status})",
        exe.display()
    );
    (stdout, bytes)
}

/// A walk that only writes through its binding over 1,000 lists of 1,000
/// ints, `passes` times: every iteration binds a copy of an 8 KB list.
#[cfg(unix)]
fn write_only(passes: u32) -> String {
    format!(
        "fn main() -> int {{
    var rows = List[List[int]]()
    for i in 0..1000 {{
        var row = List[int]()
        for j in 0..1000 {{
            (mut row).push(j)
        }}
        (mut rows).push(take row)
    }}
    var total = 0
    for pass in 0..{passes} {{
        for row in rows {{
            row[0] = pass
            total += row[1]
        }}
    }}
    print(\"{{total}} {{rows[0][0]}}\")
    0
}}
"
    )
}

/// The write-only copy is freed at the end of its iteration: six more
/// passes (6,000 more copies of 8 KB, 48 MB if they were kept) leave the
/// peak resident set where it was, within 16 MiB, natively and on release.
///
/// Unix only: the peak is the child's `ru_maxrss` from `wait4(2)`, which
/// Windows does not have. The ledger row holds the same free on every host.
#[cfg(unix)]
#[test]
fn a_walk_that_only_writes_keeps_no_copy_past_its_iteration() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("for_binding_copy_lanes")
        .join("write_only");
    std::fs::create_dir_all(&dir).expect("scratch dir");
    const MIB: u64 = 1 << 20;
    let mut wrong = Vec::new();
    for release in [false, true] {
        let tier = if release { "release" } else { "native" };
        let (Some(two), Some(eight)) = (
            build(&dir, &format!("{tier}-2"), &write_only(2), release),
            build(&dir, &format!("{tier}-8"), &write_only(8), release),
        ) else {
            continue;
        };
        let (o2, p2) = peak(&two);
        let (o8, p8) = peak(&eight);
        let grew = p8 as i64 - p2 as i64;
        eprintln!(
            "REPORT write-only {tier}: peak {p2} B at 2 passes, {p8} B at 8 ({grew:+} B); stdout {:?} {:?}",
            o2.trim(),
            o8.trim()
        );
        if o2.trim() != "2000 0" || o8.trim() != "8000 0" {
            wrong.push(format!(
                "{tier}: stdout {:?} {:?}, want \"2000 0\" \"8000 0\"",
                o2.trim(),
                o8.trim()
            ));
        }
        if grew > (16 * MIB) as i64 {
            wrong.push(format!(
                "{tier}: the peak grew {grew} B from 2 to 8 passes: write-only copies are kept past their iteration"
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// A walk that hands its binding to a pool's `init` leaves the list alone:
/// `init` keeps what it is handed as it is, so the binding it is handed must
/// be the loop's own copy. Writing the pool's slot writes that copy.
const POOL: &str = "fn main() -> !int {
    let rows = [[1, 2], [3, 4]]
    var pool = Pool[List[int]]()
    var hs = List[handle List[int]]()
    for _ in 0..2 {
        (mut hs).push((mut pool).reserve())
    }
    var i = 0
    for row in rows {
        (mut pool).init(hs[i], row)
        i += 1
    }
    pool[hs[0]][0] = 9
    print(\"rows={rows[0][0]} pool={pool[hs[0]][0]}\")
    0
}
";

#[test]
fn a_walk_that_hands_its_binding_to_a_pool_leaves_the_list_alone() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("for_binding_copy_lanes")
        .join("pool");
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let entry = dir.join("main.lu");
    std::fs::write(&entry, POOL).expect("program written");
    let want = "rows=1 pool=9\n";
    let mut wrong = Vec::new();
    for m in MACHINES {
        let Some(o) = lane(&entry, m) else {
            continue;
        };
        eprintln!("REPORT pool {m}: {} {:?}", o.verdict, o.stdout);
        if o.verdict != "exit(0)" || o.stdout != want {
            wrong.push(format!(
                "pool {m}: {} {:?}, want {want:?}",
                o.verdict, o.stdout
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
