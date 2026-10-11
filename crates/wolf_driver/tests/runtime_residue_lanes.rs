//! s228 (wolf-lang#661, #662, #374) — the runtime residue, on
//! every machine that runs it.
//!
//! s228 changed where three things live in the native runtime: a
//! closed handle's table storage is reused, a monitor's channel is
//! dropped once its delivery is received, and `net_read`'s bytes land
//! in the region they were read in. What a program can observe of the
//! first two must not move, and the third must end up agreeing with
//! lupin:
//!
//! - **handles** (#661): the numbers `fs_open` and the net calls
//!   answer keep going up whatever was closed, and a closed handle
//!   answers `io` across ten thousand opens that reuse its storage —
//!   never the bytes of the handle that reused it. Every machine owes
//!   the same line (handles are printed as offsets from the first, so
//!   the line does not depend on what the harness had open).
//! - **procs** (#662): a join and a monitor long after the exit still
//!   answer (`[conc.proc.join]`), and a monitor whose delivery has been
//!   received is an empty channel, not a dead one. Native, release and
//!   lupin; the checked machine records `unsupported` (C1) and must
//!   say so for that reason.
//! - **the trap pool** (#662): more contained traps than the pool has
//!   slots (eight per core) used to leave every slot holding a parked
//!   thread, and the process stopped answering. Native and release,
//!   run directly under a deadline so a hang is a failure here and not
//!   a stuck job.
//! - **`net_read`** (#374): `corpus/memory/region_net_read_charged.lu`
//!   on the machines `cargo xtask corpus` does not run it on (that
//!   lane is native only).

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

#[derive(Debug)]
struct Obs {
    verdict: String,
    stdout: String,
    version: String,
    unsupported: String,
}

fn parse_obs(bytes: &[u8], stderr: &[u8], what: &str) -> Obs {
    let rec: serde_json::Value = serde_json::from_slice(bytes).unwrap_or_else(|e| {
        panic!(
            "{what} record parses: {e}; stderr: {}",
            String::from_utf8_lossy(stderr)
        )
    });
    let s = |k: &str| rec[k].as_str().unwrap_or("").to_string();
    Obs {
        verdict: s("verdict"),
        stdout: s("stdout_inline"),
        version: s("impl_version"),
        unsupported: rec["x-unsupported-construct"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| String::from_utf8_lossy(stderr).into_owned()),
    }
}

/// One wolfgang machine. `None`: the host cannot run the lane (the s59
/// skip pattern, printed), never a silent pass.
fn lane(entry: &Path, flags: &[&str]) -> Option<Obs> {
    let what = format!("wolf {}", flags.join(" "));
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(entry)
        .args(flags)
        .arg("--json")
        // The program's own directory: lupin serves the fs tier inside
        // its working directory only (`[os.fs.path.domain]`), so every
        // machine runs where the program's data file is.
        .current_dir(entry.parent().expect("an entry has a directory"))
        .output()
        .expect("wolf runs");
    if flags != ["--checked"] && lane_exit::environment_refusal(&out, &what) {
        eprintln!(
            "SKIP: environment cannot run the {} lane: {}",
            flags.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    Some(parse_obs(&out.stdout, &out.stderr, &what))
}

/// The sibling lupin, found as `pairing.rs` finds it.
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

fn lupin_says(entry: &Path) -> Option<Obs> {
    let Some(lupin) = sibling_lupin() else {
        assert!(
            std::env::var_os("WOLF_PAIRING_REQUIRE_SIBLING").is_none(),
            "WOLF_PAIRING_REQUIRE_SIBLING is set and no sibling lupin was found \
             (LUPIN={}) — the oracle leg of s228's lanes did not run",
            std::env::var("LUPIN").unwrap_or_else(|_| "<unset>".into())
        );
        eprintln!(
            "SKIP: no sibling lupin on this box (set LUPIN) — the compiled machines are \
             still held to the expected answer; only the oracle leg is absent"
        );
        return None;
    };
    let out = Command::new(&lupin)
        .arg("conform-run")
        .arg(entry)
        .arg("--json")
        .current_dir(entry.parent().expect("an entry has a directory"))
        .output()
        .expect("lupin runs");
    Some(parse_obs(&out.stdout, &out.stderr, "lupin"))
}

/// Write `src` as `main.lu` in a fresh directory of its own.
fn program(name: &str, src: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("s228-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let p = dir.join("main.lu");
    std::fs::write(&p, src).expect("write program");
    p
}

/// What the checked machine owes: the same bytes, or `unsupported`
/// for the one recorded reason.
enum Checked {
    Runs,
    Unsupported(&'static str),
}

fn every_machine(name: &str, src: &str, want: &str, checked: Checked) {
    let entry = program(name, src);
    // What the handles program opens, beside it.
    std::fs::write(entry.with_file_name("data.txt"), b"wolf").expect("write data file");
    for flags in [&["--native"][..], &["--native", "--release"][..]] {
        let Some(obs) = lane(&entry, flags) else {
            continue;
        };
        assert_eq!(
            (obs.verdict.as_str(), obs.stdout.as_str()),
            ("exit(0)", want),
            "{name}: wolf {} ({})",
            flags.join(" "),
            obs.version
        );
    }
    let obs = lane(&entry, &["--checked"]).expect("the checked machine always answers");
    match checked {
        Checked::Runs => assert_eq!(
            (obs.verdict.as_str(), obs.stdout.as_str()),
            ("exit(0)", want),
            "{name}: wolf --checked ({})",
            obs.version
        ),
        Checked::Unsupported(reason) if obs.verdict == "unsupported" => {
            assert!(
                obs.unsupported.contains(reason),
                "{name}: checked is unsupported for a reason other than the recorded one: {}",
                obs.unsupported
            );
            eprintln!(
                "REPORT: {name} checked unsupported: {}",
                obs.unsupported.trim()
            );
        }
        Checked::Unsupported(_) => assert_eq!(
            (obs.verdict.as_str(), obs.stdout.as_str()),
            ("exit(0)", want),
            "{name}: checked now runs this shape and must answer the same bytes"
        ),
    }
    if let Some(obs) = lupin_says(&entry) {
        assert_eq!(
            (obs.verdict.as_str(), obs.stdout.as_str()),
            ("exit(0)", want),
            "{name}: lupin {} ({})",
            obs.version,
            obs.unsupported.trim()
        );
    }
}

// ------------------------------------------------------------ handles --

/// `data.txt` is a four-byte file the harness writes beside the
/// program.
const HANDLES: &str = r#"fn main() -> !int {
    let p = "data.txt"
    let a = fs_open(p)?
    let b = fs_open(p)?
    fs_close(a)?
    let c = fs_open(p)?
    var closes = 0
    var reads = 0
    var last = c
    var i = 0
    while i < 10000 {
        let f = fs_open(p)?
        last = f
        fs_close(a) else |_| {
            closes = closes + 1
        }
        let t = fs_read(a, 16) else |_| {
            reads = reads + 1
            ""
        }
        if t.len > 0 {
            reads = reads - 1000000
        }
        fs_close(f)?
        i = i + 1
    }
    let live = fs_read(b, 16)?
    print("files +{b - a} +{c - a} | stale closes {closes} reads {reads} | last +{last - a} | live {live}")
    fs_close(b)?
    fs_close(c)?

    let srv = net_listen("127.0.0.1:0")?
    let port = net_port(srv)?
    let cli = net_connect("127.0.0.1:{port}")?
    let conn = net_accept(srv)?
    net_close(conn)?
    net_close(cli)?
    var stale = 0
    var lastc = conn
    i = 0
    while i < 200 {
        let c2 = net_connect("127.0.0.1:{port}")?
        let s2 = net_accept(srv)?
        lastc = s2
        net_write(c2, "ping")?
        let got = net_read(conn, 16) else |_| {
            stale = stale + 1
            ""
        }
        if got.len > 0 {
            stale = stale - 1000000
        }
        net_close(conn) else |_| {
            stale = stale + 1
        }
        let mine = net_read(s2, 16)?
        if mine != "ping" {
            stale = stale - 1000000
        }
        net_close(s2)?
        net_close(c2)?
        i = i + 1
    }
    print("socks +{cli - srv} +{conn - srv} | stale {stale} | last +{lastc - srv}")
    net_close(srv)?
    0
}
"#;

#[test]
fn handle_numbers_and_use_after_close_agree_on_every_machine() {
    every_machine(
        "handles",
        HANDLES,
        "files +1 +2 | stale closes 10000 reads 10000 | last +10002 | live wolf\n\
         socks +1 +2 | stale 400 | last +402\n",
        Checked::Runs,
    );
}

// -------------------------------------------------------------- procs --

const LATE: &str = r#"fn work(n: int) -> int ! {io} {
    if n < 0 {
        return io
    }
    n + 1
}

fn exit_class(w: Proc[int]) -> int {
    let m = w.monitor()
    var cls = 3
    select {
        exit(reason) from m => {
            if reason.is_normal() {
                cls = 0
            } else {
                cls = 1
            }
        },
        timeout(30.s) => {
            cls = 4
        },
    }
    cls
}

fn main() -> int {
    let first = spawn proc work(6)
    let m = first.monitor()
    var got = 9
    select {
        exit(reason) from m => {
            if reason.is_normal() {
                got = 0
            }
        },
        timeout(30.s) => {
            got = 4
        },
    }
    var again = 9
    select {
        exit(reason) from m => {
            again = 1
        },
        timeout(20.ms) => {
            again = 0
        },
    }
    var bad = 0
    var i = 0
    while i < 200 {
        region pass {
            let w = spawn proc work(i)
            bad = bad + exit_class(w)
            let v = w.join() else |_| { -1 }
            if v != i + 1 {
                bad = bad + 1
            }
        }
        i = i + 1
    }
    let late = first.join() else |_| { -1 }
    let twice = first.join() else |_| { -1 }
    let late_cls = exit_class(first)
    print("first {got} spent-monitor {again} | 200 procs bad {bad} | late join {late} {twice} monitor {late_cls}")
    0
}
"#;

#[test]
fn a_late_join_and_a_spent_monitor_answer_as_before() {
    every_machine(
        "late",
        LATE,
        "first 0 spent-monitor 0 | 200 procs bad 0 | late join 7 7 monitor 0\n",
        Checked::Unsupported("structured concurrency in checked execution"),
    );
}

// ------------------------------------------------------ the trap pool --

const WEDGE: &str = r#"fn boom(n: int) -> int ! {io} {
    var xs = List[int]()
    (mut xs).push(n)
    xs[n + 1]
}

fn fine(n: int) -> int ! {io} {
    n + 1
}

fn main() -> int {
    let cpus = os_cpus() else |_| { 64 }
    let traps = cpus * 8 + 40
    var faults = 0
    var i = 0
    while i < traps {
        let w = spawn proc boom(i)
        let v = w.join() else |_| { -1 }
        if v == -1 {
            faults = faults + 1
        }
        i = i + 1
    }
    let w = spawn proc fine(41)
    let v = w.join() else |_| { -1 }
    print("contained {faults == traps} | then a proc answered {v}")
    0
}
"#;

/// Build `entry` on `tier` and run it under a deadline. `None`: the
/// host cannot build the lane (printed).
fn run_built(entry: &Path, release: bool, deadline: Duration) -> Option<(Option<i32>, String)> {
    let tier = if release { "release" } else { "native" };
    let exe = entry.with_file_name(format!("wedge-{tier}{}", std::env::consts::EXE_SUFFIX));
    let mut build = Command::new(wolf());
    build.arg("build");
    if release {
        build.arg("--release");
    }
    let out = build
        .arg(entry)
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("wolf runs");
    // Exit 2 is also what a COMPILE error exits with (wolf-lang#550):
    // a program that does not compile is this test's failure, never a
    // host that cannot run the lane.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("error["),
        "wolf build ({tier}): the witness does not compile: {stderr}"
    );
    if lane_exit::environment_refusal(&out, &format!("wolf build ({tier})")) {
        eprintln!(
            "SKIP: environment cannot build the {tier} lane: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "wolf build ({tier}) failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut child = Command::new(&exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the built program starts");
    let started = Instant::now();
    let status = loop {
        if let Some(st) = child.try_wait().expect("wait") {
            break Some(st);
        }
        if started.elapsed() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let mut stdout = String::new();
    if let Some(mut o) = child.stdout.take() {
        use std::io::Read as _;
        let _ = o.read_to_string(&mut stdout);
    }
    Some((status.and_then(|s| s.code()), stdout))
}

#[test]
fn contained_traps_past_the_pool_size_do_not_stop_the_process() {
    let entry = program("wedge", WEDGE);
    for release in [false, true] {
        let Some((code, stdout)) = run_built(&entry, release, Duration::from_secs(120)) else {
            continue;
        };
        assert_eq!(
            (code, stdout.as_str()),
            (Some(0), "contained true | then a proc answered 42\n"),
            "release={release}: after 8 x cores + 40 contained traps the next proc was never \
             run — every pool slot holds a thread parked by a trap (wolf-lang#662; `None` is \
             the 120 s deadline)"
        );
    }
}

// ----------------------------------------------------------- net_read --

#[test]
fn a_net_read_is_charged_to_its_region_on_every_machine() {
    every_machine(
        "net-read-charged",
        include_str!("../../../corpus/memory/region_net_read_charged.lu"),
        "charged true | reclaimed true | got 64000",
        Checked::Runs,
    );
}
