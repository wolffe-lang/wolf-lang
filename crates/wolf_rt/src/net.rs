//! The s39 net entry points — blocking TCP v0, the runtime half of the
//! `net_*` builtin tier.
//!
//! Every operation here carries the `net` capability (I13's tagging
//! discipline, same ledger as the fs tier's `fs` tag; enforcement/audit
//! UX is s40+s51). The comptime sandbox refuses the whole family
//! categorically (D33: `wolf add` must never mean arbitrary code talks
//! to the network with the builder's credentials).
//!
//! # Posture (X6; s35 reactor-routed on the native linux runtime)
//!
//! v0 shipped this module *blocking-syscall-shaped*: plain `std::net`
//! calls on the calling thread. s35 keeps every v0 signature and
//! routes the parking calls — `accept`, `read`, `write` — through the
//! io reactor on linux (the native runtime's platform floor, the same
//! gate as the task layer): readiness is awaited in the reactor
//! first (a runtime-owned park — blocking compensation applies, kill
//! teardown reaches it, deadlines compose), then the syscall runs
//! without blocking. The completion-arrival decision appended its
//! `io.arrive` kind to spec/07 `[sched.point.set]` per
//! `[sched.stable]` (the reservation v0 recorded here, activated in
//! reactor.rs). Off-linux this module keeps the v0 blocking path (the
//! kqueue/IOCP port sprints widen); the CHECKED lane
//! (`wolf_mem::ubcheck`'s net tier) keeps its v0 blocking path
//! everywhere — the checked machine single-steps under a budget and
//! owes no scheduling; its io story joins the simulated reactor in
//! s36 against the reactor's seam.
//!
//! `connect` remains the one blocking syscall: routing it through the
//! reactor needs a raw nonblocking-socket floor (EINPROGRESS +
//! writable + `SO_ERROR`), which lands with the port sprints.
//! [`NetTable::connect_timeout`] covers dial deadlines meanwhile via
//! std's own poll — a recorded delta, not a reactor route.
//!
//! # Deadlines (the `timeout` row, reachable)
//!
//! [`NetTable::set_deadline`] arms a per-socket budget applied to
//! each subsequent parking call; a fired deadline resolves as the
//! `timeout` row (v0 declared the tag with no way to reach it — the
//! reactor's timer wheel makes it real). linux-only, like the route
//! itself: elsewhere the call is an honest `io` refusal, never a
//! silently-inert deadline.
//!
//! # Error rows (D30)
//!
//! Errors are payload rows, never traps or sentinels. The v0 tag
//! vocabulary is `{refused, timeout, closed, io}` (rule 3 of the
//! wolf-std taxonomy: one tag per actionable response — anything
//! outside a builtin's declared row coarsens to `io`). [`err_tag`] is
//! the single mapping table; the checked executor
//! (`wolf_mem::ubcheck::net_err_tag`) mirrors it by hand — this crate
//! may depend on nothing in the compiler (D15) — and the driver's
//! `net_parity` test pins the two against each other, exactly the
//! fmt-shim precedent.
//!
//! A forged, foreign, or already-closed local fd is the `io` row,
//! never a trap: a bad handle is a checkable condition, not a
//! contract violation. The peer *finishing* (orderly FIN on read,
//! reset on write) is `closed` — an outcome, the socket analogue of
//! the fs tier's `eof`.

use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};

/// The v0 row-tag mapping: `io::ErrorKind` → net row tag. One table,
/// mirrored by `wolf_mem::ubcheck::net_err_tag`, pinned by the
/// driver's `net_parity` test.
pub fn err_tag(kind: std::io::ErrorKind) -> &'static str {
    use std::io::ErrorKind as K;
    match kind {
        K::ConnectionRefused => "refused",
        K::TimedOut | K::WouldBlock => "timeout",
        K::ConnectionReset | K::ConnectionAborted | K::BrokenPipe | K::NotConnected => "closed",
        _ => "io",
    }
}

/// One open socket in the runtime's table.
#[derive(Debug)]
pub enum Sock {
    Listener(TcpListener),
    Stream(TcpStream),
}

/// One socket table slot: the socket plus its armed deadline budget.
#[derive(Debug)]
struct Entry {
    sock: Sock,
    /// Per-op deadline budget ([`NetTable::set_deadline`]; honored by
    /// the linux reactor route).
    deadline: Option<std::time::Duration>,
}

/// The process-local socket table: index = the `int` fd wolf code
/// holds; `None` after close (drop closes — double close is `io`).
/// Deliberately NOT the OS fd: wolf handles are dense small ints into
/// this table, so a forged handle can never alias a foreign OS fd.
#[derive(Debug, Default)]
pub struct NetTable {
    socks: Vec<Option<Entry>>,
}

/// A net operation's failure: the row tag it raises.
pub type NetErr = &'static str;

impl NetTable {
    pub fn new() -> NetTable {
        NetTable::default()
    }

    fn push(&mut self, s: Sock) -> i64 {
        let fd = self.socks.len() as i64;
        self.socks.push(Some(Entry {
            sock: s,
            deadline: None,
        }));
        fd
    }

    fn entry(&mut self, fd: i64) -> Option<&mut Entry> {
        usize::try_from(fd)
            .ok()
            .and_then(|i| self.socks.get_mut(i))
            .and_then(Option::as_mut)
    }

    fn get(&mut self, fd: i64) -> Option<&mut Sock> {
        self.entry(fd).map(|e| &mut e.sock)
    }

    /// Park in the reactor until `fd` (a stream when `want_stream`,
    /// else a listener) is ready for `interest`, or its deadline
    /// budget fires (`timeout`). The net flavor of the wait is
    /// kill-only — see reactor.rs's cancellation section.
    #[cfg(target_os = "linux")]
    fn wait_ready(
        &mut self,
        fd: i64,
        want_stream: bool,
        interest: crate::reactor::Interest,
    ) -> Result<(), NetErr> {
        use std::os::fd::AsRawFd as _;
        let Some(e) = self.entry(fd) else {
            return Err("io");
        };
        let raw = match (&e.sock, want_stream) {
            (Sock::Stream(s), true) => s.as_raw_fd(),
            (Sock::Listener(l), false) => l.as_raw_fd(),
            _ => return Err("io"),
        };
        let deadline = e.deadline.map(|d| std::time::Instant::now() + d);
        match crate::reactor::wait_fd_net(raw, interest, deadline) {
            crate::reactor::IoWait::Ready => Ok(()),
            crate::reactor::IoWait::TimedOut => Err("timeout"),
            // A killed proc on a non-unwindable frame: the result is
            // moot but must be a row (the compiled teardown branch is
            // codegen's — pool.rs's honest s34 refusal, unchanged).
            crate::reactor::IoWait::Cancelled => Err("io"),
        }
    }

    /// Arm (`millis > 0`) or clear (`millis <= 0`) this socket's
    /// deadline budget: every subsequent parking call (`accept`,
    /// `read`, `write`) resolves as the `timeout` row when readiness
    /// does not arrive within the budget. linux (the reactor route);
    /// elsewhere an honest `io` refusal — never an inert deadline.
    pub fn set_deadline(&mut self, fd: i64, millis: i64) -> Result<(), NetErr> {
        #[cfg(target_os = "linux")]
        {
            let Some(e) = self.entry(fd) else {
                return Err("io");
            };
            e.deadline = u64::try_from(millis)
                .ok()
                .filter(|&m| m > 0)
                .map(std::time::Duration::from_millis);
            Ok(())
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (fd, millis);
            Err("io")
        }
    }

    /// Bind + listen. `addr` is `"host:port"`; port 0 asks the OS for
    /// an ephemeral port (the corpus discipline: loopback + port 0,
    /// never a fixed port, never an external host).
    pub fn listen(&mut self, addr: &str) -> Result<i64, NetErr> {
        match TcpListener::bind(addr) {
            Ok(l) => Ok(self.push(Sock::Listener(l))),
            Err(e) => Err(err_tag(e.kind())),
        }
    }

    /// The local port a listener (or stream) is bound to — how a
    /// port-0 bind learns what it got.
    pub fn port(&mut self, fd: i64) -> Result<i64, NetErr> {
        let addr = match self.get(fd) {
            Some(Sock::Listener(l)) => l.local_addr(),
            Some(Sock::Stream(s)) => s.local_addr(),
            None => return Err("io"),
        };
        addr.map(|a| i64::from(a.port()))
            .map_err(|e| err_tag(e.kind()))
    }

    /// Park until one connection arrives (reactor-routed on linux;
    /// the deadline budget resolves `timeout`); returns the stream's
    /// fd.
    pub fn accept(&mut self, fd: i64) -> Result<i64, NetErr> {
        #[cfg(target_os = "linux")]
        self.wait_ready(fd, false, crate::reactor::Interest::Read)?;
        let accepted = match self.get(fd) {
            Some(Sock::Listener(l)) => l.accept(),
            Some(Sock::Stream(_)) => return Err("io"),
            None => return Err("io"),
        };
        match accepted {
            Ok((s, _peer)) => Ok(self.push(Sock::Stream(s))),
            Err(e) => Err(err_tag(e.kind())),
        }
    }

    /// Dial `addr` (blocking connect — see the module doc: connect's
    /// reactor route awaits the raw-socket floor).
    pub fn connect(&mut self, addr: &str) -> Result<i64, NetErr> {
        match TcpStream::connect(addr) {
            Ok(s) => Ok(self.push(Sock::Stream(s))),
            Err(e) => Err(err_tag(e.kind())),
        }
    }

    /// Dial with a deadline: the `timeout` row when the handshake
    /// does not complete in `millis`. The wait sits in std's own
    /// poll (recorded delta — not a reactor route); `millis <= 0`
    /// behaves as [`NetTable::connect`].
    pub fn connect_timeout(&mut self, addr: &str, millis: i64) -> Result<i64, NetErr> {
        use std::net::ToSocketAddrs as _;
        let Some(m) = u64::try_from(millis).ok().filter(|&m| m > 0) else {
            return self.connect(addr);
        };
        let Ok(mut addrs) = addr.to_socket_addrs() else {
            return Err("io");
        };
        let Some(sa) = addrs.next() else {
            return Err("io");
        };
        match TcpStream::connect_timeout(&sa, std::time::Duration::from_millis(m)) {
            Ok(s) => Ok(self.push(Sock::Stream(s))),
            Err(e) => Err(err_tag(e.kind())),
        }
    }

    /// Park until some bytes arrive (at most `max`; reactor-routed on
    /// linux — the deadline budget resolves `timeout`); the peer's
    /// orderly close is the `closed` row, the socket `eof`.
    pub fn read(&mut self, fd: i64, max: i64) -> Result<Vec<u8>, NetErr> {
        if !matches!(self.get(fd), Some(Sock::Stream(_))) {
            return Err("io");
        }
        if max <= 0 {
            return Ok(Vec::new());
        }
        #[cfg(target_os = "linux")]
        self.wait_ready(fd, true, crate::reactor::Interest::Read)?;
        let Some(Sock::Stream(s)) = self.get(fd) else {
            return Err("io");
        };
        let mut buf = vec![0u8; (max as u64).min(1 << 20) as usize];
        match s.read(&mut buf) {
            Ok(0) => Err("closed"),
            Ok(n) => {
                buf.truncate(n);
                Ok(buf)
            }
            Err(e) => Err(err_tag(e.kind())),
        }
    }

    /// Write the whole buffer (send space awaited through the
    /// reactor on linux — the deadline budget resolves `timeout` at
    /// admission; the whole-buffer drain then rides the syscall, the
    /// short-write completion loop being io_uring-parity work).
    pub fn write(&mut self, fd: i64, bytes: &[u8]) -> Result<(), NetErr> {
        #[cfg(target_os = "linux")]
        self.wait_ready(fd, true, crate::reactor::Interest::Write)?;
        let Some(Sock::Stream(s)) = self.get(fd) else {
            return Err("io");
        };
        s.write_all(bytes).map_err(|e| err_tag(e.kind()))
    }

    /// Close a socket; drop closes. Double close (or a forged fd) is
    /// the `io` row.
    pub fn close(&mut self, fd: i64) -> Result<(), NetErr> {
        match usize::try_from(fd).ok().and_then(|i| self.socks.get_mut(i)) {
            Some(slot @ Some(_)) => {
                *slot = None;
                Ok(())
            }
            _ => Err("io"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Loopback echo, port 0, single table — the whole v0 surface in
    /// one deterministic sequence (connect completes against the
    /// backlog before accept runs; TCP loopback semantics).
    #[test]
    fn loopback_echo_roundtrip() {
        let mut t = NetTable::new();
        let srv = t.listen("127.0.0.1:0").expect("listen");
        let port = t.port(srv).expect("port");
        let cli = t.connect(&format!("127.0.0.1:{port}")).expect("connect");
        t.write(cli, b"ping").expect("write");
        let conn = t.accept(srv).expect("accept");
        assert_eq!(t.read(conn, 16).expect("read"), b"ping");
        t.write(conn, b"pong").expect("reply");
        assert_eq!(t.read(cli, 16).expect("read reply"), b"pong");
        t.close(cli).expect("close cli");
        t.close(conn).expect("close conn");
        t.close(srv).expect("close srv");
    }

    /// Dialing a port that was just released is the `refused` row —
    /// a handleable outcome, never a trap.
    #[test]
    fn refused_is_a_row() {
        let mut t = NetTable::new();
        let srv = t.listen("127.0.0.1:0").expect("listen");
        let port = t.port(srv).expect("port");
        t.close(srv).expect("close");
        assert_eq!(t.connect(&format!("127.0.0.1:{port}")), Err("refused"));
    }

    /// The peer's orderly close is `closed` (the socket `eof`), and a
    /// forged or double-closed fd is `io`.
    #[test]
    fn closed_and_forged_fd_rows() {
        let mut t = NetTable::new();
        let srv = t.listen("127.0.0.1:0").expect("listen");
        let port = t.port(srv).expect("port");
        let cli = t.connect(&format!("127.0.0.1:{port}")).expect("connect");
        let conn = t.accept(srv).expect("accept");
        t.close(conn).expect("close server side");
        assert_eq!(t.read(cli, 16), Err("closed"));
        assert_eq!(t.read(9999, 16), Err("io"));
        t.close(cli).expect("close");
        assert_eq!(t.close(cli), Err("io"));
        assert_eq!(t.write(cli, b"x"), Err("io"));
    }

    /// s35: the loopback echo under armed deadlines — the reactor
    /// route end to end (readiness awaited for accept/read/write;
    /// generous budgets never fire, the roundtrip is byte-identical
    /// to the v0 path).
    #[cfg(target_os = "linux")]
    #[test]
    fn echo_under_deadline() {
        let mut t = NetTable::new();
        let srv = t.listen("127.0.0.1:0").expect("listen");
        let port = t.port(srv).expect("port");
        t.set_deadline(srv, 5_000).expect("srv deadline");
        let cli = t
            .connect_timeout(&format!("127.0.0.1:{port}"), 5_000)
            .expect("connect");
        t.set_deadline(cli, 5_000).expect("cli deadline");
        t.write(cli, b"ping").expect("write");
        let conn = t.accept(srv).expect("accept");
        t.set_deadline(conn, 5_000).expect("conn deadline");
        assert_eq!(t.read(conn, 16).expect("read"), b"ping");
        t.write(conn, b"pong").expect("reply");
        assert_eq!(t.read(cli, 16).expect("read reply"), b"pong");
        t.close(cli).expect("close cli");
        t.close(conn).expect("close conn");
        t.close(srv).expect("close srv");
    }

    /// s35: the `timeout` row is REACHABLE (v0 declared the tag with
    /// no way to reach it; the reactor's timer wheel makes it real):
    /// an idle accept and an idle read fire their budgets, and the
    /// same sockets still work once readiness truly arrives.
    #[cfg(target_os = "linux")]
    #[test]
    fn timeout_row_reachable() {
        let mut t = NetTable::new();
        let srv = t.listen("127.0.0.1:0").expect("listen");
        let port = t.port(srv).expect("port");
        // Nothing pending: the accept budget fires.
        t.set_deadline(srv, 40).expect("arm");
        assert_eq!(t.accept(srv), Err("timeout"));
        // A pending connection resolves the same listener.
        let cli = t.connect(&format!("127.0.0.1:{port}")).expect("connect");
        t.set_deadline(srv, 5_000).expect("rearm");
        let conn = t.accept(srv).expect("accept after connect");
        // No data: the read budget fires; data resolves it.
        t.set_deadline(conn, 40).expect("arm read");
        assert_eq!(t.read(conn, 16), Err("timeout"));
        t.write(cli, b"ping").expect("write");
        t.set_deadline(conn, 5_000).expect("rearm read");
        assert_eq!(t.read(conn, 16).expect("read"), b"ping");
        // Clearing the budget restores the indefinite wait contract
        // (proved by arming, clearing, and reading ready data).
        t.write(cli, b"more").expect("write more");
        t.set_deadline(conn, 40).expect("arm");
        t.set_deadline(conn, 0).expect("clear");
        assert_eq!(t.read(conn, 16).expect("read"), b"more");
        // A forged fd cannot arm a deadline.
        assert_eq!(t.set_deadline(9999, 40), Err("io"));
    }

    /// Accepting on a stream (or reading on a listener) is `io` —
    /// wrong-kind handles are checkable conditions.
    #[test]
    fn wrong_kind_is_io() {
        let mut t = NetTable::new();
        let srv = t.listen("127.0.0.1:0").expect("listen");
        let port = t.port(srv).expect("port");
        let cli = t.connect(&format!("127.0.0.1:{port}")).expect("connect");
        assert_eq!(t.accept(cli), Err("io"));
        assert_eq!(t.read(srv, 4), Err("io"));
    }
}
