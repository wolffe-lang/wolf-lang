//! s149 (wolf-lang#289, #290): what one accepted connection and one
//! served file COST THE KERNEL on linux, counted the way lobo's ws27
//! counted them — `strace` over a child that does exactly the two
//! things a static-file server does per request, and assertions on
//! the calls that appear against the descriptors they were made on.
//!
//! Why a trace and not a flag read: the crate tests beside this one
//! (`net::tests::an_accepted_stream_arrives_non_blocking_and_close_on_exec`,
//! `nodelay_is_paid_at_the_write_that_could_be_delayed`) prove the
//! POSTURE from the kernel's own answer on every unix host, and that
//! is the part a program can depend on. What they cannot see is the
//! number of calls it took to get there, which is the whole content
//! of #290 — `accept4(SOCK_NONBLOCK|SOCK_CLOEXEC)` is one call where
//! `accept4(SOCK_CLOEXEC)` + `ioctl(FIONBIO)` was two — so the count
//! needs the tracer. It is linux-only for the same reason the saving
//! is: macOS has no `accept4`, and no unprivileged tracer either
//! (dtruss wants SIP down).
//!
//! `harness = false`: the child is this binary re-executed with one
//! argument, and the trace must hold that child's calls and nothing
//! else.
//!
//! The four claims, each against the fd the call names:
//!
//!   accept        exactly one `accept4`, carrying BOTH `SOCK_NONBLOCK`
//!                 and `SOCK_CLOEXEC`, and NO `accept(`.
//!   posture       no `ioctl(FIONBIO)` and no `fcntl(F_SETFL)` on the
//!                 accepted fd, ever: the posture rode the accept.
//!   nodelay       no `setsockopt(TCP_NODELAY)` on the accepted fd
//!                 before its first write — the connection-per-request
//!                 shape pays none at all — and exactly one before the
//!                 second write, which is the write Nagle could hold.
//!   open          the read-non-blocking open (mode 5) is ONE `openat`
//!                 carrying `O_NONBLOCK`, with NO path stat before it;
//!                 the classification is a metadata call on the fd.

use std::io::Write as _;
use std::path::PathBuf;
use std::process::Command;

use wolf_rt::fs::{__wolf_rt_fs_close, __wolf_rt_fs_fstat, __wolf_rt_fs_open, fs_mode};
use wolf_rt::net::NetTable;

/// The unique basename the fs half opens, so its calls are picked out
/// of a trace that also holds the fixture's own write.
const FIXTURE: &str = "s149-syscall-shape.txt";

/// The syscalls the assertions read. Everything else is noise this
/// filter keeps out of the trace, which is what makes the counts
/// readable rather than approximate.
const TRACED: &str = "accept,accept4,ioctl,fcntl,setsockopt,openat,statx,newfstatat,\
                      fstat,write,writev,send,sendto,close";

/// The calls a WRITE on a TCP stream can appear as. std writes a
/// socket with `send(…, MSG_NOSIGNAL)` on linux — which strace renders
/// as `sendto(fd, …, NULL, 0)` — and `writev` for a gather, so a
/// witness that looked only for `write(` found neither. Measured, not
/// assumed: the first cut of this test asserted `write(` and the linux
/// job answered with one `setsockopt` and no writes at all.
const WRITES: [&str; 4] = ["sendto(", "send(", "write(", "writev("];

/// Is this traced line a write on a stream?
fn is_write(l: &str) -> bool {
    WRITES.iter().any(|w| l.starts_with(w))
}

pub fn main() {
    if std::env::args().any(|a| a == "--child") {
        child();
        return;
    }
    let Some(strace) = which("strace") else {
        if std::env::var_os("WOLF_SYSCALL_REQUIRE_STRACE").is_some() {
            panic!(
                "syscall_shape: WOLF_SYSCALL_REQUIRE_STRACE is set and there is no \
                 `strace` on PATH — the linux job is the instrument for #289/#290 \
                 and may not answer a missing tracer with a green skip"
            );
        }
        eprintln!(
            "syscall_shape: SKIPPED — no `strace` on PATH. This is the counting \
             witness for wolf-lang#289/#290; on CI the job installs one and sets \
             WOLF_SYSCALL_REQUIRE_STRACE, which turns this skip into a failure."
        );
        return;
    };
    let dir = std::env::temp_dir().join(format!("wolf-s149-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let log = dir.join("trace.txt");
    let me = std::env::current_exe().expect("current_exe");
    // NOT `-f`: the reactor's thread makes none of the calls this
    // asserts on, and following it is the one way a line of the
    // trace could arrive split across an `<unfinished ...>` pair.
    let st = Command::new(strace)
        .arg("-e")
        .arg(format!("trace={TRACED}"))
        .arg("-o")
        .arg(&log)
        .arg(&me)
        .arg("--child")
        .env("WOLF_S149_DIR", &dir)
        .status()
        .expect("spawn strace");
    assert!(st.success(), "the traced child failed: {st}");
    let trace = std::fs::read_to_string(&log).expect("trace");
    let lines: Vec<String> = trace.lines().map(strip_pid).collect();
    check(&lines);
    let _ = std::fs::remove_dir_all(&dir);
    // The shape, printed on the way past: a run that passes still
    // says what it counted, so the CI log is the record rather than
    // a green tick over an unread file.
    eprintln!("syscall_shape: the linux count holds (#289, #290). The trace:");
    for l in lines.iter().filter(|l| {
        l.starts_with("accept")
            || l.starts_with("setsockopt(")
            || (l.starts_with("openat(") && l.contains(FIXTURE))
            || l.contains("FIONBIO")
            || (first_arg(l).is_some() && is_write(l))
    }) {
        eprintln!("  {l}");
    }
}

/// The child: one accepted connection written to twice, then one file
/// opened non-blocking and classified off its handle. Nothing else —
/// every call in the trace is one of these two shapes or the fixture's.
fn child() {
    let dir = PathBuf::from(std::env::var_os("WOLF_S149_DIR").expect("WOLF_S149_DIR"));
    let mut t = NetTable::new();
    let l = t.listen("127.0.0.1:0").expect("listen");
    let port = t.port(l).expect("port");
    let cli = t.connect(&format!("127.0.0.1:{port}")).expect("dial");
    let conn = t.accept(l).expect("accept");
    t.write(conn, b"one").expect("first write");
    t.write(conn, b"two").expect("second write");
    let mut got = Vec::new();
    while got.len() < 6 {
        got.extend(t.read(cli, 16).expect("read"));
    }
    assert_eq!(got, b"onetwo");

    // The sockets stay OPEN across the fs half on purpose: a closed
    // descriptor's number comes straight back on the next open, and
    // the assertions below read the trace BY descriptor. Closing
    // first would let the fixture inherit the accepted stream's
    // number and every `write(conn, …)` claim would be reading the
    // wrong lines.
    let path = dir.join(FIXTURE);
    let mut f = std::fs::File::create(&path).expect("fixture");
    f.write_all(b"12345").expect("fixture bytes");
    drop(f);
    let p = path.display().to_string();
    let mut out = [0i64; 2];
    // SAFETY: a live str pair (a Rust `String`'s bytes) and an out
    // slot of the width the entry contracts.
    unsafe {
        let fd = __wolf_rt_fs_open(p.as_ptr() as i64, p.len() as i64, fs_mode::READ_NONBLOCK);
        assert!(fd >= 0, "mode 5 open");
        assert_eq!(
            __wolf_rt_fs_fstat(fd, out.as_mut_ptr() as i64),
            0,
            "fstat on the handle"
        );
        assert_eq!(__wolf_rt_fs_close(fd), 0, "close");
    }
    for fd in [conn, cli, l] {
        t.close(fd).expect("close");
    }
}

fn check(lines: &[String]) {
    // --- the accept ---------------------------------------------------
    let accepts: Vec<&String> = lines.iter().filter(|l| l.starts_with("accept4(")).collect();
    assert_eq!(
        accepts.len(),
        1,
        "exactly one accept4 — the child takes one connection\n{}",
        lines.join("\n")
    );
    assert!(
        !lines.iter().any(|l| l.starts_with("accept(")),
        "no bare accept(2): the host has accept4 and the runtime uses it"
    );
    let a = accepts[0];
    assert!(
        a.contains("SOCK_NONBLOCK"),
        "accept4 carries SOCK_NONBLOCK — this is the ioctl that vanishes (#290): {a}"
    );
    assert!(
        a.contains("SOCK_CLOEXEC"),
        "accept4 still carries SOCK_CLOEXEC — nobody's child inherits a connection: {a}"
    );
    let conn = ret_fd(a).expect("accept4's returned fd");

    // --- the posture, and the option ----------------------------------
    let on_conn = |l: &String| first_arg(l) == Some(conn);
    assert!(
        !lines
            .iter()
            .any(|l| on_conn(l) && l.starts_with("ioctl(") && l.contains("FIONBIO")),
        "no ioctl(FIONBIO) on the accepted stream: the posture rode the accept\n{}",
        lines.join("\n")
    );
    assert!(
        !lines
            .iter()
            .any(|l| on_conn(l) && l.starts_with("fcntl(") && l.contains("F_SETFL")),
        "and no fcntl(F_SETFL) either"
    );
    let steps: Vec<&String> = lines
        .iter()
        .filter(|l| {
            on_conn(l)
                && (is_write(l) || (l.starts_with("setsockopt(") && l.contains("TCP_NODELAY")))
        })
        .collect();
    assert_eq!(
        steps.len(),
        3,
        "two writes and exactly one TCP_NODELAY between them: {steps:?}\n{}",
        lines.join("\n")
    );
    assert!(
        is_write(steps[0]),
        "the FIRST write is paid for by nothing — no unacknowledged data exists, so \
         Nagle cannot hold it, so a connection-per-request server sets no option at \
         all (#290): {steps:?}"
    );
    assert!(
        steps[1].starts_with("setsockopt("),
        "the option is set BEFORE the second write, which is the one Nagle could sit \
         on: {steps:?}"
    );
    assert!(is_write(steps[2]), "then the second write: {steps:?}");

    // --- the open ------------------------------------------------------
    let opens: Vec<&String> = lines
        .iter()
        .filter(|l| l.starts_with("openat(") && l.contains(FIXTURE) && l.contains("O_RDONLY"))
        .collect();
    assert_eq!(opens.len(), 1, "one read open of the fixture: {opens:?}");
    assert!(
        opens[0].contains("O_NONBLOCK"),
        "mode 5 carries O_NONBLOCK into the open (#289): {}",
        opens[0]
    );
    assert!(
        !lines.iter().any(|l| {
            (l.starts_with("statx(") || l.starts_with("newfstatat(")) && l.contains(FIXTURE)
        }),
        "NO path stat: the whole point is that the classification comes off the \
         handle, not off a second walk of the name (#289)\n{}",
        lines.join("\n")
    );
    let file = ret_fd(opens[0]).expect("openat's returned fd");
    assert!(
        lines.iter().any(|l| {
            first_arg(l) == Some(file)
                && (l.starts_with("statx(")
                    || l.starts_with("fstat(")
                    || l.starts_with("newfstatat("))
        }),
        "and exactly the metadata call on the fd that replaces it\n{}",
        lines.join("\n")
    );
}

/// The tracer, found the way a shell finds it: the first `strace` on
/// `PATH`. Never a hard-coded `/usr/bin/strace` — a runner image is
/// free to put it elsewhere, and a wrong absolute path would read as
/// "no tracer" and skip.
fn which(bin: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|d| d.join(bin))
            .find(|c| c.is_file())
    })
}

/// `[pid 1234] call(...)` -> `call(...)`; every other line unchanged.
fn strip_pid(l: &str) -> String {
    match l.strip_prefix("[pid ") {
        Some(rest) => rest
            .split_once("] ")
            .map_or_else(|| l.to_string(), |(_, c)| c.to_string()),
        None => l.trim_start().to_string(),
    }
}

/// The descriptor a traced line ANSWERED (`… ) = 7`).
fn ret_fd(l: &str) -> Option<i64> {
    l.rsplit_once(" = ")?.1.trim().parse().ok()
}

/// The descriptor a traced line was made ON (`call(7, …`).
fn first_arg(l: &str) -> Option<i64> {
    let (_, args) = l.split_once('(')?;
    let head = args.split([',', ')']).next()?;
    head.trim().parse().ok()
}
