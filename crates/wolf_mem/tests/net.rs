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

/// Statically clean ladder, then checked execution. Panics on refusal.
fn run(src: &str) -> ubcheck::RunOutcome {
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
/// gate. `net_write_bytes`/`net_read_bytes` over `List[int]`.
#[test]
fn byte_roundtrip_carries_binary() {
    assert_stdout(
        "fn main() -> !int {\n\
             let srv = net_listen(\"127.0.0.1:0\")?\n\
             let port = net_port(srv)?\n\
             let cli = net_connect(\"127.0.0.1:{port}\")?\n\
             var payload = List[int]()\n\
             (mut payload).push(255)\n\
             (mut payload).push(0)\n\
             (mut payload).push(128)\n\
             (mut payload).push(65)\n\
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

/// s115 (#137): a `List[int]` element outside `0..=255` is the
/// `invalid` row (the `fs_write_bytes` refusal over the socket),
/// handleable with `else`, and nothing is sent.
#[test]
fn write_bytes_out_of_range_is_the_invalid_row() {
    assert_stdout(
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
        "invalid\n",
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
