//! s228 — the soak: a million cycles of each thing a server does for
//! ever, and resident memory that stays where it was.
//!
//! ws57 (lobo#66) measured what a wolf server still grew by once its
//! own retention was gone: 4 bytes per `fs_open`, 56 per socket, 277
//! per spawned and monitored proc — all in the runtime's tables, none
//! in any region. wolf-lang#661 and #662 are those three numbers, and
//! this is the gate that keeps them fixed: one program, built on the
//! release tier and run as a real process, doing
//!
//! - `CYCLES` × `fs_open` + `fs_close`,
//! - `CYCLES` × `net_connect_unix` + `net_accept` + both closes,
//! - `CYCLES` × `spawn proc` + `monitor` + the exit received, each in
//!   its own `region` — once with procs that all end alike, once with
//!   procs that each end with a value of their own,
//!
//! with the process's RSS read from outside, by the harness, after the
//! first tenth of each leg (allocator and pool warm) and at its end.
//! The program stops at each mark until the harness has read it, so a
//! reading is of a process standing still.
//!
//! # The bounds, stated
//!
//! | leg | growth allowed over 900,000 cycles | trunk `a027b643` (hasu) |
//! |---|---|---|
//! | files | 1 MiB | 3.4 MiB (3.9 B a cycle) |
//! | connections | 1 MiB | 96 MiB (112 B a cycle) |
//! | procs, ending alike | 2 MiB | 237 MiB (277 B a proc) |
//! | procs, a value each | 2 MiB + 12 B a proc (12.3 MiB) | 237 MiB |
//!
//! Files, connections and procs that end alike are FLAT. Procs that
//! each end with their own value are not, and the bound says so: a
//! finished proc's reason word has to stay, because a `Proc[T]` is a
//! copyable id with no lifetime and `[conc.proc.join]` lets it be
//! joined after the exit any number of times. The runtime keeps that
//! word and nothing else (`wolf_rt::task::proc`'s `ExitWords`), one
//! for a whole block of procs when they ended alike and 8 bytes each
//! when they did not; the last bound catches anything beyond the word.
//!
//! `WOLF_SOAK_CYCLES` overrides the count (the bounds scale with it);
//! the default is the million the contract names. Unix only: the
//! connection leg needs a socket family with no ports to exhaust, and
//! windows has no `net_listen_unix` (`[os.net.unix]`) — skipped there
//! by name.

#[cfg(unix)]
mod lane_exit;

#[cfg(unix)]
mod soak {
    use super::lane_exit;
    use std::io::{BufRead as _, BufReader, Write as _};
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::time::Duration;

    fn wolf() -> &'static str {
        env!("CARGO_BIN_EXE_wolf")
    }

    const DEFAULT_CYCLES: u64 = 1_000_000;

    const SOAK: &str = r#"fn work(n: int) -> int ! {io} {
    if n < 0 {
        return io
    }
    n + 1
}

fn once(n: int) -> int {
    let w = spawn proc work(n)
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
        timeout(600.s) => {
            cls = 4
        },
    }
    cls
}

// One mark: say where we are, then stand still until the harness has
// read this process's memory and answers with a line.
fn mark(leg: str, at: int, check: int) {
    print("mark {leg} {at} {check}")
    let _go = read_line() else |_| { "" }
}

fn main() -> int {
    let args = env_args()
    let cycles = args[0].to_int() else |_| { 1000 }
    let warm = cycles / 10

    var i = 0
    var bad = 0
    var last = 0
    while i < cycles {
        if i == warm {
            mark("files", i, last)
        }
        let fd = fs_open("data.txt") else |_| { -1 }
        if fd < 0 {
            bad = bad + 1
        } else {
            last = fd
            fs_close(fd) else |_| {
                bad = bad + 1
            }
        }
        i = i + 1
    }
    mark("files", i, last)

    let l = net_listen_unix("soak.sock") else |_| {
        return 2
    }
    i = 0
    last = 0
    while i < cycles {
        if i == warm {
            mark("conns", i, last)
        }
        let c = net_connect_unix("soak.sock") else |_| { -1 }
        let s = net_accept(l) else |_| { -1 }
        if c < 0 || s < 0 {
            bad = bad + 1
        }
        if s >= 0 {
            last = s
            net_close(s) else |_| {
                bad = bad + 1
            }
        }
        if c >= 0 {
            net_close(c) else |_| {
                bad = bad + 1
            }
        }
        i = i + 1
    }
    mark("conns", i, last)
    net_close(l) else |_| {
        bad = bad + 1
    }

    // Procs that all end alike (a server's per-request procs)...
    i = 0
    while i < cycles {
        if i == warm {
            mark("procs", i, live_region_bytes())
        }
        region pass {
            bad = bad + once(0)
        }
        i = i + 1
    }
    mark("procs", i, live_region_bytes())

    // ...and procs that each end with a value of their own.
    i = 0
    while i < cycles {
        if i == warm {
            mark("procs-own", i, live_region_bytes())
        }
        region pass {
            bad = bad + once(i)
        }
        i = i + 1
    }
    mark("procs-own", i, live_region_bytes())
    print("done bad {bad}")
    bad
}
"#;

    /// The child's resident set, in kB: `/proc/<pid>/status` where
    /// there is one, `ps` elsewhere (macOS).
    fn rss_kb(pid: u32) -> u64 {
        if let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status"))
            && let Some(line) = status.lines().find(|l| l.starts_with("VmRSS:"))
        {
            return line
                .split_whitespace()
                .nth(1)
                .and_then(|n| n.parse().ok())
                .expect("VmRSS is a number of kB");
        }
        let out = Command::new("ps")
            .args(["-o", "rss=", "-p", &pid.to_string()])
            .output()
            .expect("ps runs");
        String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .unwrap_or_else(|_| panic!("ps answered no rss for {pid}: {out:?}"))
    }

    #[test]
    fn a_million_cycles_leave_resident_memory_flat() {
        let cycles: u64 = match std::env::var("WOLF_SOAK_CYCLES") {
            Ok(v) => v.parse().expect("WOLF_SOAK_CYCLES is a count"),
            Err(_) => DEFAULT_CYCLES,
        };
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("s228-soak");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let entry = dir.join("main.lu");
        std::fs::write(&entry, SOAK).expect("write program");
        std::fs::write(dir.join("data.txt"), b"wolf").expect("write data file");
        let exe = dir.join("soak");

        let out = Command::new(wolf())
            .arg("build")
            .arg("--release")
            .arg(&entry)
            .arg("-o")
            .arg(&exe)
            .output()
            .expect("wolf runs");
        // Exit 2 is also a compile error's exit (wolf-lang#550): never
        // read one as a host that cannot run the lane.
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !stderr.contains("error["),
            "the soak program does not compile: {stderr}"
        );
        if lane_exit::environment_refusal(&out, "wolf build --release") {
            eprintln!(
                "SKIP: environment cannot build the release lane: {}",
                stderr.trim()
            );
            return;
        }
        assert!(out.status.success(), "wolf build --release: {stderr}");

        let mut child = Command::new(&exe)
            .arg(cycles.to_string())
            .current_dir(&dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("the soak starts");
        let pid = child.id();
        let mut go = child.stdin.take().expect("piped stdin");
        let lines = BufReader::new(child.stdout.take().expect("piped stdout"));
        // A reader thread, so a program that stops answering is a
        // failure after a deadline and not a job that never ends.
        let (tx, rx) = mpsc::channel::<String>();
        std::thread::spawn(move || {
            for line in lines.lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let deadline = Duration::from_secs(900);
        let next = || match rx.recv_timeout(deadline) {
            Ok(line) => line,
            Err(e) => {
                let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
                panic!("the soak stopped answering for {deadline:?} ({e})");
            }
        };

        let warm = cycles / 10;
        let run = cycles - warm;
        // leg, flat allowance in kB, bytes allowed per cycle.
        let legs = [
            ("files", 1024u64, 0u64),
            ("conns", 1024, 0),
            ("procs", 2048, 0),
            ("procs-own", 2048, 12),
        ];
        let mut report = Vec::new();
        for (leg, flat_kb, per_cycle) in legs {
            let mut rss = [0u64; 2];
            for (k, want_at) in [warm, cycles].into_iter().enumerate() {
                let line = next();
                let words: Vec<&str> = line.split_whitespace().collect();
                assert_eq!(
                    (words[0], words[1], words[2].parse::<u64>().ok()),
                    ("mark", leg, Some(want_at)),
                    "the soak answered `{line}` where mark {leg} {want_at} was due"
                );
                rss[k] = rss_kb(pid);
                eprintln!(
                    "SOAK {leg} at {want_at}: rss {} kB (check {})",
                    rss[k], words[3]
                );
                writeln!(go).expect("answer the mark");
                go.flush().expect("flush");
            }
            let grew_kb = rss[1].saturating_sub(rss[0]);
            let allowed_kb = flat_kb + per_cycle * run / 1024;
            report.push((leg, grew_kb, allowed_kb));
        }
        let done = next();
        drop(go);
        let status = child.wait().expect("the soak exits");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            (done.as_str(), status.code()),
            ("done bad 0", Some(0)),
            "a cycle failed inside the soak"
        );
        for (leg, grew_kb, allowed_kb) in &report {
            eprintln!(
                "SOAK {leg}: +{grew_kb} kB over {run} cycles ({:.2} B a cycle), {allowed_kb} kB allowed",
                *grew_kb as f64 * 1024.0 / run as f64
            );
        }
        let over: Vec<String> = report
            .iter()
            .filter(|(_, grew, allowed)| grew > allowed)
            .map(|(leg, grew, allowed)| {
                format!(
                    "{leg}: resident memory grew {grew} kB over {run} cycles ({:.1} B a cycle), \
                     {allowed} kB allowed",
                    *grew as f64 * 1024.0 / run as f64
                )
            })
            .collect();
        assert!(
            over.is_empty(),
            "the runtime keeps memory per cycle (wolf-lang#661, #662):\n{}",
            over.join("\n")
        );
    }
}

#[cfg(not(unix))]
#[test]
fn a_million_cycles_leave_resident_memory_flat() {
    eprintln!(
        "SKIP: the soak's connection leg needs unix-domain sockets, which this host's \
         runtime refuses by name ([os.net.unix]); the residue is gated here by \
         wolf_rt's handle_residue allocator witness"
    );
}
