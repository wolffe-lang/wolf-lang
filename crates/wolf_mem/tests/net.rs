//! s39 — the net builtin tier under checked execution: blocking TCP
//! v0 (`net_listen`/`net_port`/`net_accept`/`net_connect`/`net_read`/
//! `net_write`/`net_close`). Errors are D30 payload rows, never traps:
//! a dead port is `refused`, the peer's finish is `closed` (the socket
//! `eof`), a text-decode failure is `utf8`, a forged or wrong-kind fd
//! is `io` — each one handleable with `else`/`match`.
//!
//! Discipline (the no-external-network law): every test is loopback
//! (`127.0.0.1`) and port 0 — the OS assigns an ephemeral port and the
//! program learns it through `net_port`. Nothing here ever names a
//! fixed port or a foreign host. The checked machine performs REAL
//! host operations; only the comptime sandbox refuses them (D33).
//!
//! These are the run-expectation twins for `corpus/net/*.lu` (the
//! corpus pins `mem`: native lowering refuses the tier honestly at
//! s39).

use wolf_mem::ubcheck::{self, Budget, Verdict};
use wolf_sema::{AliasTable, MemoryLoader, resolve_package_with, typecheck_package_with};

/// Every program here binds LOOPBACK EPHEMERAL ports, and the host's
/// ephemeral port space is one shared resource for the whole test
/// binary. `refused_row_handles_and_propagates` depends on the one
/// thing that space cannot promise under concurrency: that the port it
/// just closed is still dead a microsecond later. It is not — macOS
/// recycles a freed ephemeral port quickly, so a sibling test's bind
/// can land on it and the "dead port" answers a connection (measured:
/// 3 failures in 40 16-thread runs once s137 added two more
/// ephemeral-binding programs to the file).
///
/// So the execution of a checked program is SERIAL here. Every entry
/// below takes this lock for the whole run, which is exactly the
/// scope that matters: no two programs in this binary hold or release
/// a port at the same time. The file runs in hundredths of a second;
/// there is nothing to win by overlapping it.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|p| p.into_inner())
}

/// Statically clean ladder, then checked execution. Panics on refusal.
fn run(src: &str) -> ubcheck::RunOutcome {
    let _serial = serial();
    let mut ml = MemoryLoader::new("net");
    ml.add_file(&[], "main.lu", src);
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    assert!(
        res.diagnostics
            .iter()
            .all(|d| d.severity != wolf_diag::Severity::Error),
        "input resolves clean: {:?}",
        res.diagnostics
    );
    let tc = typecheck_package_with(&res.package, true);
    assert!(
        tc.not_yet.is_empty(),
        "input typechecks fully: {:?}",
        tc.not_yet
    );
    assert!(
        !tc.has_errors(),
        "input typechecks clean: {:?}",
        tc.diagnostics
    );
    let mem = wolf_mem::check_package(&res.package, &tc);
    assert!(
        mem.not_yet.is_empty(),
        "input stays inside the mem surface: {:?}",
        mem.not_yet
    );
    ubcheck::run_checked(&res.package, &tc, Budget::default())
        .expect("the program is within the executable surface")
}

fn assert_stdout(src: &str, expected: &str) {
    let out = run(src);
    match out.verdict {
        Verdict::Exit(0) => {}
        other => panic!("expected exit(0), got {other:?} (stdout: {:?})", out.stdout),
    }
    assert_eq!(out.stdout, expected, "stdout");
}

// ------------------------------------------------------ happy path --

/// The corpus/net/echo_roundtrip.lu twin: listen, dial, echo, close —
/// deterministic because a loopback connect completes in the backlog
/// before accept runs.
#[test]
fn echo_roundtrip_over_loopback() {
    assert_stdout(
        "fn main() -> !int {\n\
             let srv = net_listen(\"127.0.0.1:0\")?\n\
             let port = net_port(srv)?\n\
             let cli = net_connect(\"127.0.0.1:{port}\")?\n\
             net_write(cli, \"ping\")?\n\
             let conn = net_accept(srv)?\n\
             let msg = net_read(conn, 16)?\n\
             print(\"got: {msg}\")\n\
             net_write(conn, \"pong\")?\n\
             let reply = net_read(cli, 16)?\n\
             print(\"reply: {reply}\")\n\
             net_close(cli)?\n\
             net_close(conn)?\n\
             net_close(srv)?\n\
             0\n\
         }\n",
        "got: ping\nreply: pong\n",
    );
}

/// s115 (#137): the corpus/net/byte_roundtrip.lu twin — the byte
/// path carries bytes the str path mangles (a 0xFF, an embedded NUL,
/// a lone continuation byte), round-tripping BYTE-EQUAL with no UTF-8
/// gate. `net_write_bytes`/`net_read_bytes` over `List[byte]` (s136;
/// `List[int]` from s115 to s135).
#[test]
fn byte_roundtrip_carries_binary() {
    assert_stdout(
        "fn main() -> !int {\n\
             let srv = net_listen(\"127.0.0.1:0\")?\n\
             let port = net_port(srv)?\n\
             let cli = net_connect(\"127.0.0.1:{port}\")?\n\
             var payload = List[byte]()\n\
             (mut payload).push(255 as byte)\n\
             (mut payload).push(0 as byte)\n\
             (mut payload).push(128 as byte)\n\
             (mut payload).push(65 as byte)\n\
             net_write_bytes(cli, payload)?\n\
             let conn = net_accept(srv)?\n\
             let got = net_read_bytes(conn, 16)?\n\
             var same = got.len == payload.len\n\
             var i = 0\n\
             for b in got {\n\
                 if b != payload[i] { same = false }\n\
                 i = i + 1\n\
             }\n\
             print(\"len={got.len} equal={same}\")\n\
             net_close(cli)?\n\
             net_close(conn)?\n\
             net_close(srv)?\n\
             0\n\
         }\n",
        "len=4 equal=true\n",
    );
}

/// s115 (#137) declared `invalid` for a `List[int]` element outside
/// `0..=255`; since s136 (#231) `net_write_bytes` takes `List[byte]`,
/// so the out-of-range element is unconstructible and a `List[int]`
/// at the call is the E0401 mismatch — the row stays declared (the
/// vocabulary is stable; an FFI caller's wrong-width list still earns
/// it) and is unreachable from typed code.
#[test]
fn write_bytes_takes_a_byte_list_since_s136() {
    let mut ml = MemoryLoader::new("net");
    ml.add_file(
        &[],
        "main.lu",
        "fn main() -> !int {\n\
             let srv = net_listen(\"127.0.0.1:0\")?\n\
             let port = net_port(srv)?\n\
             let cli = net_connect(\"127.0.0.1:{port}\")?\n\
             var bad = List[int]()\n\
             (mut bad).push(256)\n\
             net_write_bytes(cli, bad) else |_| print(\"invalid\")\n\
             net_close(cli)?\n\
             net_close(srv)?\n\
             0\n\
         }\n",
    );
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    let tc = typecheck_package_with(&res.package, true);
    let codes: Vec<&str> = tc.diagnostics.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(
        codes,
        vec!["E0401"],
        "a List[int] is not the byte carrier: {codes:?}"
    );
}

// ------------------------------------------------------------ rows --

/// The corpus/net/refused_row.lu twin: a dead ephemeral port is the
/// `refused` row — `else` handles it, `?` makes it the documented
/// process outcome (tag on stdout, exit 1).
#[test]
fn refused_row_handles_and_propagates() {
    let out = run("fn main() -> !int {\n\
             let srv = net_listen(\"127.0.0.1:0\")?\n\
             let port = net_port(srv)?\n\
             net_close(srv)?\n\
             let sock = net_connect(\"127.0.0.1:{port}\") else |_| 0\n\
             print(\"handled: {sock}\")\n\
             let strict = net_connect(\"127.0.0.1:{port}\")?\n\
             print(\"unreachable: {strict}\")\n\
             0\n\
         }\n");
    assert!(
        matches!(out.verdict, Verdict::Exit(1)),
        "propagated row exits 1"
    );
    assert_eq!(out.stdout, "handled: 0\nerror: refused\n");
}

/// The peer's orderly close is the `closed` row (the socket `eof`),
/// an outcome — never a trap.
#[test]
fn peer_close_is_the_closed_row() {
    assert_stdout(
        "fn main() -> !int {\n\
             let srv = net_listen(\"127.0.0.1:0\")?\n\
             let port = net_port(srv)?\n\
             let cli = net_connect(\"127.0.0.1:{port}\")?\n\
             let conn = net_accept(srv)?\n\
             net_close(conn)?\n\
             let got = net_read(cli, 16) else |e| match e {\n\
                 closed => \"the-closed-row\",\n\
                 _ => \"another-row\",\n\
             }\n\
             print(\"read: {got}\")\n\
             0\n\
         }\n",
        "read: the-closed-row\n",
    );
}

/// A forged fd, a double close, and a wrong-kind handle are all the
/// `io` row: a bad handle is a checkable condition, not a contract
/// violation.
#[test]
fn forged_and_closed_fds_are_the_io_row() {
    assert_stdout(
        "fn main() -> !int {\n\
             net_write(99, \"nope\") else |_| print(\"forged: io\")\n\
             let srv = net_listen(\"127.0.0.1:0\")?\n\
             net_close(srv)?\n\
             net_close(srv) else |_| print(\"double close: io\")\n\
             let a = net_accept(srv) else |e| match e {\n\
                 io => 1,\n\
                 _ => 2,\n\
             }\n\
             print(\"closed accept: {a}\")\n\
             0\n\
         }\n",
        "forged: io\ndouble close: io\nclosed accept: 1\n",
    );
}

/// Bytes that do not decode are the `utf8` row. The foreign peer is a
/// raw Rust socket: the wolf program publishes its ephemeral port
/// through a scratch file, the helper thread dials it and writes
/// invalid UTF-8, and the blocking `net_accept`/`net_read` pair sees
/// it — the one test where the peer is not the program itself.
#[test]
fn invalid_utf8_from_a_foreign_peer_is_the_utf8_row() {
    use std::io::Write as _;
    let dir = std::env::temp_dir().join(format!("wolf-s39-net-utf8-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let port_file = dir.join("port.txt");
    let port_lit = port_file.display().to_string().replace('\\', "/");

    let helper = {
        let port_file = port_file.clone();
        std::thread::spawn(move || {
            // Poll for the published port (the wolf side writes it
            // right after listen; generous cap, deterministic exit).
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            let port: u16 = loop {
                if let Ok(s) = std::fs::read_to_string(&port_file)
                    && let Ok(p) = s.trim().parse()
                {
                    break p;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "wolf side never published its port"
                );
                std::thread::sleep(std::time::Duration::from_millis(5));
            };
            let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).expect("dial wolf");
            s.write_all(&[0xff, 0xfe, 0xfd])
                .expect("send invalid utf-8");
        })
    };

    let out = run(&format!(
        "fn main() -> !int {{\n\
             let srv = net_listen(\"127.0.0.1:0\")?\n\
             let port = net_port(srv)?\n\
             fs_write_text(\"{port_lit}\", \"{{port}}\")?\n\
             let conn = net_accept(srv)?\n\
             let got = net_read(conn, 16) else |e| match e {{\n\
                 utf8 => \"the-utf8-row\",\n\
                 _ => \"another-row\",\n\
             }}\n\
             print(\"read: {{got}}\")\n\
             0\n\
         }}\n",
    ));
    helper.join().expect("helper thread");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(matches!(out.verdict, Verdict::Exit(0)), "{:?}", out.verdict);
    assert_eq!(out.stdout, "read: the-utf8-row\n");
}

// ------------------------------- s106: the deadline (`net_deadline`) --

/// The corpus timeout-witness twin (`corpus/net/read_deadline.lu`): a
/// read against a deliberately-silent peer fires its armed budget as
/// the `timeout` row — declared since s39, reachable since s106 —
/// and `?` makes it the documented process outcome (tag on stdout,
/// exit 1).
#[test]
fn read_deadline_against_a_silent_peer_is_the_timeout_row() {
    let out = run("fn main() -> !int {\n\
             let srv = net_listen(\"127.0.0.1:0\")?\n\
             let port = net_port(srv)?\n\
             let cli = net_connect(\"127.0.0.1:{port}\")?\n\
             let conn = net_accept(srv)?\n\
             net_deadline(cli, 40)?\n\
             let msg = net_read(cli, 16)?\n\
             print(\"unreachable: {msg}\")\n\
             net_close(conn)?\n\
             0\n\
         }\n");
    assert!(
        matches!(out.verdict, Verdict::Exit(1)),
        "propagated row exits 1, got {:?} (stdout {:?})",
        out.verdict,
        out.stdout
    );
    assert_eq!(out.stdout, "error: timeout\n");
}

/// wolf-lang#224 — a peer that closed over UNREAD receive data reset
/// the connection; on macOS `setsockopt(SO_RCVTIMEO)` answers EINVAL
/// on the reset socket while the bytes it wrote before closing are
/// still readable there. The deadline must ARM (native's reactor never
/// asks the kernel and reports armed) and the reads must end in the
/// ordinary `closed` row — never `io` at the deadline. Whether the
/// buffered reply is delivered before the reset is the kernel's
/// (Windows discards it), so the test reads to the row and pins only
/// the row. The corpus twin is `net/peer_close_after_serve.lu`.
#[test]
fn deadline_after_a_reset_close_arms_and_the_reply_is_readable() {
    assert_stdout(
        "fn main() -> !int {\n\
             let l = net_listen(\"127.0.0.1:0\")?\n\
             let p = net_port(l)?\n\
             let cli = net_connect(\"127.0.0.1:{p}\")?\n\
             net_write(cli, \"GET / HTTP/1.1\\r\\nHost: x\\r\\n\\r\\n\")?\n\
             let conn = net_accept(l)?\n\
             net_write(conn, \"HTTP/1.1 200 OK\\r\\nContent-Length: 5\\r\\n\\r\\nhello\")?\n\
             net_close(conn)?\n\
             net_deadline(cli, 1000)?\n\
             var end = \"data\"\n\
             var more = true\n\
             while more {\n\
                 let piece = net_read(cli, 4096) else |e| match e {\n\
                     closed => { end = \"closed\"\n more = false\n \"\" },\n\
                     timeout => { end = \"timeout\"\n more = false\n \"\" },\n\
                     utf8 => { end = \"utf8\"\n more = false\n \"\" },\n\
                     io => { end = \"io\"\n more = false\n \"\" },\n\
                 }\n\
                 if piece.len == 0 { more = false }\n\
             }\n\
             print(\"armed {end}\")\n\
             net_close(cli)?\n\
             net_close(l)?\n\
             0\n\
         }\n",
        "armed closed\n",
    );
}

/// An armed budget bounds `accept` too (the side-table emulation),
/// clearing (`ms <= 0`) restores the indefinite-wait contract, and a
/// fired deadline does not poison the socket: readiness that truly
/// arrives still resolves.
#[test]
fn accept_deadline_fires_clears_and_recovers() {
    assert_stdout(
        "fn main() -> !int {\n\
             let srv = net_listen(\"127.0.0.1:0\")?\n\
             let port = net_port(srv)?\n\
             net_deadline(srv, 40)?\n\
             let idle = net_accept(srv) else |e| match e {\n\
                 timeout => -1,\n\
                 _ => -2,\n\
             }\n\
             print(\"idle accept: {idle}\")\n\
             let cli = net_connect(\"127.0.0.1:{port}\")?\n\
             net_deadline(srv, 0)?\n\
             let conn = net_accept(srv)?\n\
             net_write(cli, \"woke\")?\n\
             net_deadline(conn, 5000)?\n\
             let msg = net_read(conn, 16)?\n\
             print(\"got: {msg}\")\n\
             net_close(cli)?\n\
             net_close(conn)?\n\
             net_close(srv)?\n\
             0\n\
         }\n",
        "idle accept: -1\ngot: woke\n",
    );
}

/// Arming a deadline on a forged or closed fd is the `io` row — the
/// same checkable-condition discipline as every other entry.
#[test]
fn deadline_on_a_forged_fd_is_the_io_row() {
    assert_stdout(
        "fn main() -> !int {\n\
             net_deadline(99, 40) else |_| print(\"forged: io\")\n\
             let srv = net_listen(\"127.0.0.1:0\")?\n\
             net_close(srv)?\n\
             net_deadline(srv, 40) else |_| print(\"closed: io\")\n\
             0\n\
         }\n",
        "forged: io\nclosed: io\n",
    );
}

// ------------------------------------------------- read edge cases --

/// `net_read` with a non-positive max is an empty string, not a row —
/// mirroring `fs_read`'s shape.
#[test]
fn read_zero_max_is_empty() {
    assert_stdout(
        "fn main() -> !int {\n\
             let srv = net_listen(\"127.0.0.1:0\")?\n\
             let port = net_port(srv)?\n\
             let cli = net_connect(\"127.0.0.1:{port}\")?\n\
             let conn = net_accept(srv)?\n\
             let empty = net_read(conn, 0)?\n\
             print(\"len: {empty.len}\")\n\
             net_close(cli)?\n\
             0\n\
         }\n",
        "len: 0\n",
    );
}

/// s136 (#227, `[os.net.unix]`): the unix-domain family on the checked
/// machine — an echo over a socket PATH, `net_port` on it the `io`
/// row, the rows by name (`exists` for a second bind, `not_found` for
/// a dial with no file, `refused` for a file nobody listens on), and
/// the cleanup posture: the listener's close unlinks the path. The
/// twin of `corpus/net/unix_echo.lu`.
#[cfg(unix)]
#[test]
fn unix_echo_rows_and_cleanup() {
    let dir = std::env::temp_dir().join(format!("wolf-s136-ub-unix-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("echo.sock").display().to_string();
    let src = format!(
        r#"fn main() -> !int {{
    let path = "{path}"
    let srv = net_listen_unix(path)?
    let again = net_listen_unix(path) else |e| match e {{
        exists => -1,
        _ => -2,
    }}
    let port = net_port(srv) else |_| -1
    let cli = net_connect_unix(path)?
    net_write(cli, "ping")?
    let conn = net_accept(srv)?
    let got = net_read(conn, 16)?
    net_write(conn, "pong {{got}}")?
    let reply = net_read(cli, 16)?
    net_close(cli)?
    net_close(conn)?
    net_close(srv)?
    let gone = !fs_exists(path)
    let none = net_connect_unix(path) else |e| match e {{
        not_found => -1,
        _ => -2,
    }}
    fs_write_text(path, "")?
    let stale = net_connect_unix(path) else |e| match e {{
        refused => -1,
        io => -3,
        _ => -2,
    }}
    fs_remove(path)?
    print("{{again}} {{port}} {{got}} {{reply}} {{gone}} {{none}} {{stale}}")
    0
}}
"#
    );
    let out = run(&src);
    match out.verdict {
        Verdict::Exit(0) => {}
        other => panic!("expected exit(0), got {other:?} (stdout: {:?})", out.stdout),
    }
    let line = out.stdout.trim_end();
    assert!(
        line == "-1 -1 ping pong ping true -1 -1" || line == "-1 -1 ping pong ping true -1 -3",
        "rows by name and the path unlinked at close: {line:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// s136 (#227): a host the machine does not serve refuses BY NAME —
/// the `unsupported` row, handleable, never a bare `io` and never a
/// trap.
#[cfg(not(unix))]
#[test]
fn unix_family_refuses_by_name() {
    assert_stdout(
        r#"fn main() -> !int {
    let l = net_listen_unix("C:/wolf-s136-probe.sock") else |e| match e {
        unsupported => -1,
        _ => -2,
    }
    let c = net_connect_unix("C:/wolf-s136-probe.sock") else |e| match e {
        unsupported => -1,
        _ => -2,
    }
    print("{l} {c}")
    0
}
"#,
        "-1 -1\n",
    );
}

// ---------------- s137: the listener a prefork server shares --------

/// The ladder without the "must execute" assertion — the twin of
/// [`run`] for the two calls the checked machine REFUSES BY NAME
/// (s137, `[os.proc.inherit]`). Answers the refused construct.
fn run_or_refusal(src: &str) -> Result<ubcheck::RunOutcome, String> {
    let _serial = serial();
    let mut ml = MemoryLoader::new("net");
    ml.add_file(&[], "main.lu", src);
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    let tc = typecheck_package_with(&res.package, true);
    assert!(
        tc.not_yet.is_empty() && !tc.has_errors(),
        "input typechecks"
    );
    let mem = wolf_mem::check_package(&res.package, &tc);
    assert!(mem.not_yet.is_empty(), "input stays inside the mem surface");
    ubcheck::run_checked(&res.package, &tc, Budget::default())
        .map_err(|nyc| nyc.construct.to_string())
}

/// s137 (#234, `[os.net.listen.opts]`): the listener options on the
/// checked machine — the option-less shape is `net_listen` call for
/// call, a held address is `exists` BY NAME (the "in use" row #234
/// asked for), a `reuse_port` group holds one port with both hands and
/// a hand without the option cannot join it. The twin of
/// `corpus/net/reuse_port.lu`. The group's port comes from a plain
/// ephemeral bind that is then closed, so both hands name the address
/// explicitly — the shape a prefork master actually writes, and the
/// one the corpus witness performs too.
#[cfg(unix)]
#[test]
fn listen_with_options_and_rows() {
    assert_stdout(
        "fn main() -> !int {\n\
         let probe = net_listen(\"127.0.0.1:0\")?\n\
         let p = net_port(probe)?\n\
         net_close(probe)?\n\
         let addr = \"127.0.0.1:{p}\"\n\
         let a = net_listen_with(addr, true, 16)?\n\
         let b = net_listen_with(addr, true, 16)?\n\
         let same = net_port(b)? == p\n\
         var named = false\n\
         let x = net_listen_with(addr, false, 0) else |e| match e {\n\
         exists => { named = true\n0 - 1 },\n\
         _ => 0 - 1,\n\
         }\n\
         net_close(a)?\n\
         net_close(b)?\n\
         let plain = net_listen_with(\"127.0.0.1:0\", false, 0)?\n\
         let q = net_port(plain)?\n\
         let cli = net_connect(\"127.0.0.1:{q}\")?\n\
         net_write(cli, \"ping\")?\n\
         let conn = net_accept(plain)?\n\
         let got = net_read(conn, 16)?\n\
         net_close(cli)?\n\
         net_close(conn)?\n\
         net_close(plain)?\n\
         print(\"{same} {named} {x} {got}\")\n\
         0\n\
         }\n",
        "true true -1 ping\n",
    );
}

/// s137 (#235, `[os.proc.inherit]`): the checked machine runs no
/// descriptor handoff and says so BY NAME, with the CONSTRUCT named —
/// never a bare `unsupported`, never a trap (the s134 records rule; a
/// conform-run record carries the same string in
/// `x-unsupported-construct`). An EMPTY inherit set is `os_spawn` with
/// the program named apart from its arguments, and is served.
#[test]
fn adoption_and_an_inherit_set_refuse_with_the_construct_named() {
    let adopt = run_or_refusal(
        "fn main() -> !int {\n\
         let l = net_adopt_listener(3) else |_| 0 - 1\n\
         print(\"{l}\")\n\
         0\n\
         }\n",
    );
    assert_eq!(
        adopt.err().as_deref(),
        Some("listener adoption in checked execution")
    );
    let inherit = run_or_refusal(
        "fn main() -> !int {\n\
         let srv = net_listen(\"127.0.0.1:0\")?\n\
         let argv = List[str]()\n\
         let fds = List[int]()\n\
         (mut fds).push(srv)\n\
         let h = os_spawn_with(\"/bin/sh\", argv, fds) else |_| 0 - 1\n\
         print(\"{h}\")\n\
         0\n\
         }\n",
    );
    assert_eq!(
        inherit.err().as_deref(),
        Some("fd inheritance across os_spawn_with in checked execution")
    );
    // The empty set is served: a program that does not exist is the
    // `not_found` row, exactly as `os_spawn` answers it.
    assert_stdout(
        "fn main() -> !int {\n\
         let argv = List[str]()\n\
         let fds = List[int]()\n\
         let h = os_spawn_with(\"wolf-s137-no-such-program\", argv, fds) else |e| match e {\n\
         not_found => 0 - 1,\n\
         _ => 0 - 2,\n\
         }\n\
         print(\"{h}\")\n\
         0\n\
         }\n",
        "-1\n",
    );
}
