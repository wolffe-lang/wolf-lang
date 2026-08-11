//! The s39 net entry points — blocking TCP v0, the runtime half of the
//! `net_*` builtin tier.
//!
//! Every operation here carries the `net` capability (I13's tagging
//! discipline, same ledger as the fs tier's `fs` tag; enforcement/audit
//! UX is s40+s51). The comptime sandbox refuses the whole family
//! categorically (D33: `wolf add` must never mean arbitrary code talks
//! to the network with the builder's credentials).
//!
//! # Posture (X6, scoped to v0)
//!
//! v0 is *blocking-syscall-shaped*: plain `std::net` calls on the
//! calling thread, no reactor, no task parking. The s35 completion
//! reactor owns the real async story — idle connections holding no
//! task, deadlines composed through `select` — and inherits this
//! module as its syscall floor. Because v0 never makes a scheduling
//! decision (the calling thread simply blocks in the kernel), it adds
//! **no schedule points**: spec/07 `[sched.point.set]` is untouched.
//! When s35 turns these into completion submissions, the
//! completion-arrival decision appends its own kind there per
//! `[sched.stable]` — appended then, by the sprint that makes it real,
//! never renumbered.
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

/// The process-local socket table: index = the `int` fd wolf code
/// holds; `None` after close (drop closes — double close is `io`).
/// Deliberately NOT the OS fd: wolf handles are dense small ints into
/// this table, so a forged handle can never alias a foreign OS fd.
#[derive(Debug, Default)]
pub struct NetTable {
    socks: Vec<Option<Sock>>,
}

/// A net operation's failure: the row tag it raises.
pub type NetErr = &'static str;

impl NetTable {
    pub fn new() -> NetTable {
        NetTable::default()
    }

    fn push(&mut self, s: Sock) -> i64 {
        let fd = self.socks.len() as i64;
        self.socks.push(Some(s));
        fd
    }

    fn get(&mut self, fd: i64) -> Option<&mut Sock> {
        usize::try_from(fd)
            .ok()
            .and_then(|i| self.socks.get_mut(i))
            .and_then(Option::as_mut)
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

    /// Block until one connection arrives; returns the stream's fd.
    pub fn accept(&mut self, fd: i64) -> Result<i64, NetErr> {
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

    /// Dial `addr` (blocking connect).
    pub fn connect(&mut self, addr: &str) -> Result<i64, NetErr> {
        match TcpStream::connect(addr) {
            Ok(s) => Ok(self.push(Sock::Stream(s))),
            Err(e) => Err(err_tag(e.kind())),
        }
    }

    /// Block until some bytes arrive (at most `max`); the peer's
    /// orderly close is the `closed` row, the socket `eof`.
    pub fn read(&mut self, fd: i64, max: i64) -> Result<Vec<u8>, NetErr> {
        let Some(Sock::Stream(s)) = self.get(fd) else {
            return Err("io");
        };
        if max <= 0 {
            return Ok(Vec::new());
        }
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

    /// Write the whole buffer (blocking).
    pub fn write(&mut self, fd: i64, bytes: &[u8]) -> Result<(), NetErr> {
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
