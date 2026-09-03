//! The s39 net entry points — blocking TCP v0, the runtime half of the
//! `net_*` builtin tier.
//!
//! Every operation here carries the `net` capability (I13's tagging
//! discipline, same ledger as the fs tier's `fs` tag; enforcement/audit
//! UX is s40+s51). The comptime sandbox refuses the whole family
//! categorically (D33: `wolf add` must never mean arbitrary code talks
//! to the network with the builder's credentials).
//!
//! # Posture (X6; s35 reactor-routed on the native linux, macOS, and windows runtimes)
//!
//! v0 shipped this module *blocking-syscall-shaped*: plain `std::net`
//! calls on the calling thread. s35 keeps every v0 signature and
//! routes the parking calls — `accept`, `read`, `write` — through the
//! io reactor on linux, macOS, and windows (the native runtime's
//! platform floor, the same gate as the task layer — epoll, kqueue
//! since s59, `WSAPoll` since s60b): readiness is awaited in the reactor
//! first (a runtime-owned park — blocking compensation applies, kill
//! teardown reaches it, deadlines compose), then the syscall runs
//! without blocking. The completion-arrival decision appended its
//! `io.arrive` kind to spec/07 `[sched.point.set]` per
//! `[sched.stable]` (the reservation v0 recorded here, activated in
//! reactor.rs). Off the ported hosts this module keeps the v0
//! blocking path (the IOCP port sprint widens); the CHECKED lane
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
//! reactor's timer wheel makes it real). Reactor hosts only (linux,
//! macOS, windows), like the route itself: elsewhere the call is an
//! honest `io` refusal, never a silently-inert deadline.
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
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::Mutex;

use crate::fs::{byte_elems, write_bytes_list};
use crate::str::{ambient_copy, view, write_pair};

/// The v0 row-tag mapping: `io::ErrorKind` → net row tag. One table,
/// mirrored by `wolf_mem::ubcheck::net_err_tag`, pinned by the
/// driver's `net_parity` test.
pub fn err_tag(kind: std::io::ErrorKind) -> &'static str {
    use std::io::ErrorKind as K;
    match kind {
        K::ConnectionRefused => "refused",
        K::TimedOut | K::WouldBlock => "timeout",
        K::ConnectionReset | K::ConnectionAborted | K::BrokenPipe | K::NotConnected => "closed",
        // s136 (#227): the path rows of the unix-domain family — a
        // missing directory or socket file, a permission the caller
        // lacks, a path already bound. Only the builtins that declare
        // them see them; everywhere else lowering (and the checked
        // machine's `coarse`) folds them to `io` as before.
        K::NotFound => "not_found",
        K::PermissionDenied => "denied",
        K::AddrInUse => "exists",
        _ => "io",
    }
}

/// One open socket in the runtime's table.
#[derive(Debug)]
pub enum Sock {
    Listener(TcpListener),
    Stream(TcpStream),
    /// s136 (#227, `[os.net.unix]`): an `AF_UNIX` listener and the
    /// path it bound. The runtime created the socket file, so the
    /// runtime removes it: `close` unlinks the path (the cleanup
    /// posture the clause states). Unix hosts only — windows refuses
    /// the family by name (`unsupported`; see `listen_unix`).
    #[cfg(unix)]
    UnixListener(UnixListener, std::path::PathBuf),
    /// An `AF_UNIX` stream: accepted from a [`Sock::UnixListener`] or
    /// dialed by `connect_unix`. Reads, writes, deadlines and close
    /// are the TCP stream's, call for call.
    #[cfg(unix)]
    UnixStream(UnixStream),
}

impl Sock {
    /// A stream of either family (the `read`/`write`/`deadline`
    /// receivers); a listener is not one.
    fn is_stream(&self) -> bool {
        match self {
            Sock::Stream(_) => true,
            #[cfg(unix)]
            Sock::UnixStream(_) => true,
            _ => false,
        }
    }

    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Sock::Stream(s) => s.read(buf),
            #[cfg(unix)]
            Sock::UnixStream(s) => s.read(buf),
            _ => Err(std::io::Error::from(std::io::ErrorKind::Other)),
        }
    }

    fn write_all(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        match self {
            Sock::Stream(s) => s.write_all(bytes),
            #[cfg(unix)]
            Sock::UnixStream(s) => s.write_all(bytes),
            _ => Err(std::io::Error::from(std::io::ErrorKind::Other)),
        }
    }

    /// One accepted connection of the listener's own family.
    fn accept(&self) -> std::io::Result<Sock> {
        match self {
            Sock::Listener(l) => l.accept().map(|(s, _)| Sock::Stream(s)),
            #[cfg(unix)]
            Sock::UnixListener(l, _) => l.accept().map(|(s, _)| Sock::UnixStream(s)),
            _ => Err(std::io::Error::from(std::io::ErrorKind::Other)),
        }
    }

    /// The raw OS handle for a readiness park: the stream's when
    /// `want_stream`, else the listener's; `None` for the wrong kind.
    /// Reactor hosts only (the module is gated the same way; a tier-2
    /// host without a reactor parks in the syscall itself).
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn raw(&self, want_stream: bool) -> Option<crate::reactor::RawFd> {
        use std::os::fd::AsRawFd as _;
        match (self, want_stream) {
            (Sock::Stream(s), true) => Some(s.as_raw_fd()),
            (Sock::Listener(l), false) => Some(l.as_raw_fd()),
            (Sock::UnixStream(s), true) => Some(s.as_raw_fd()),
            (Sock::UnixListener(l, _), false) => Some(l.as_raw_fd()),
            _ => None,
        }
    }

    #[cfg(windows)]
    fn raw(&self, want_stream: bool) -> Option<crate::reactor::RawFd> {
        use std::os::windows::io::AsRawSocket as _;
        match (self, want_stream) {
            (Sock::Stream(s), true) => Some(s.as_raw_socket()),
            (Sock::Listener(l), false) => Some(l.as_raw_socket()),
            _ => None,
        }
    }
}

/// One socket table slot: the socket plus its armed deadline budget.
#[derive(Debug)]
struct Entry {
    sock: Sock,
    /// Per-op deadline budget ([`NetTable::set_deadline`]; honored by
    /// the reactor route — linux and macOS since s59).
    #[cfg_attr(
        not(any(target_os = "linux", target_os = "macos", target_os = "windows")),
        allow(dead_code)
    )]
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
    /// `const` so the shim tier's process table ([`NET`]) can live in a
    /// `static Mutex` without lazy-init machinery (the fs `FILES`
    /// precedent).
    pub const fn new() -> NetTable {
        NetTable { socks: Vec::new() }
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

    /// The park parameters of a readiness wait — the raw OS fd (a
    /// stream when `want_stream`, else a listener; the wrong kind or a
    /// tombstone is `io`) and the socket's armed deadline as an
    /// absolute instant. Split out of [`NetTable::wait_ready`] so the
    /// SHIM tier (one process table behind a `Mutex`) can snapshot
    /// these under a short lock and park with the lock RELEASED — a
    /// blocked accept holding the table would deadlock the connect
    /// that resolves it.
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    fn park_spec(
        &mut self,
        fd: i64,
        want_stream: bool,
    ) -> Result<(crate::reactor::RawFd, Option<std::time::Instant>), NetErr> {
        let Some(e) = self.entry(fd) else {
            return Err("io");
        };
        let Some(raw) = e.sock.raw(want_stream) else {
            return Err("io");
        };
        Ok((raw, e.deadline.map(|d| std::time::Instant::now() + d)))
    }

    /// Park in the reactor until `fd` (a stream when `want_stream`,
    /// else a listener) is ready for `interest`, or its deadline
    /// budget fires (`timeout`). The net flavor of the wait is
    /// kill-only — see reactor.rs's cancellation section.
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    fn wait_ready(
        &mut self,
        fd: i64,
        want_stream: bool,
        interest: crate::reactor::Interest,
    ) -> Result<(), NetErr> {
        let (raw, deadline) = self.park_spec(fd, want_stream)?;
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
    /// does not arrive within the budget. Reactor hosts (linux +
    /// macOS); elsewhere an honest `io` refusal — never an inert
    /// deadline.
    pub fn set_deadline(&mut self, fd: i64, millis: i64) -> Result<(), NetErr> {
        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
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
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
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

    /// `net_listen_unix(path)` (s136, #227): bind + listen an
    /// `AF_UNIX` stream socket at `path`. The path must not exist —
    /// a stale socket file is `exists`, the operator's to remove (a
    /// program that owns the path removes it first with `fs_remove`);
    /// a missing directory is `not_found`, a permission the caller
    /// lacks is `denied`. The runtime remembers the path and unlinks
    /// it at [`NetTable::close`]. Windows: `unsupported`, by name —
    /// `AF_UNIX` exists there since Win10 1803, but `std::net` has no
    /// unix-domain surface on that host and this runtime carries no
    /// winsock binding beyond `WSAPoll` (D15); the serving rung is
    /// named, not silently `io`.
    pub fn listen_unix(&mut self, path: &str) -> Result<i64, NetErr> {
        #[cfg(unix)]
        {
            match UnixListener::bind(path) {
                Ok(l) => Ok(self.push(Sock::UnixListener(l, std::path::PathBuf::from(path)))),
                Err(e) => Err(err_tag(e.kind())),
            }
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            Err("unsupported")
        }
    }

    /// `net_connect_unix(path)` (s136, #227): dial the `AF_UNIX`
    /// listener at `path`. No socket file is `not_found`; a file
    /// nobody listens on (a stale one) is `refused`; a permission the
    /// caller lacks is `denied`. Windows: `unsupported`, by name.
    pub fn connect_unix(&mut self, path: &str) -> Result<i64, NetErr> {
        #[cfg(unix)]
        {
            match UnixStream::connect(path) {
                Ok(s) => Ok(self.push(Sock::UnixStream(s))),
                Err(e) => Err(err_tag(e.kind())),
            }
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            Err("unsupported")
        }
    }

    /// The local port a listener (or stream) is bound to — how a
    /// port-0 bind learns what it got.
    pub fn port(&mut self, fd: i64) -> Result<i64, NetErr> {
        let addr = match self.get(fd) {
            Some(Sock::Listener(l)) => l.local_addr(),
            Some(Sock::Stream(s)) => s.local_addr(),
            // A unix-domain socket has no port: `io`, the wrong-kind
            // answer.
            _ => return Err("io"),
        };
        addr.map(|a| i64::from(a.port()))
            .map_err(|e| err_tag(e.kind()))
    }

    /// Park until one connection arrives (reactor-routed on linux;
    /// the deadline budget resolves `timeout`); returns the stream's
    /// fd.
    pub fn accept(&mut self, fd: i64) -> Result<i64, NetErr> {
        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        self.wait_ready(fd, false, crate::reactor::Interest::Read)?;
        self.accept_ready(fd)
    }

    /// [`NetTable::accept`]'s syscall half — readiness already awaited
    /// (or the platform's honest v0 posture: the syscall itself
    /// blocks). The shim tier calls this under the table lock AFTER
    /// parking with the lock released.
    fn accept_ready(&mut self, fd: i64) -> Result<i64, NetErr> {
        let accepted = match self.get(fd) {
            Some(l) if !l.is_stream() => l.accept(),
            _ => return Err("io"),
        };
        match accepted {
            Ok(s) => Ok(self.push(s)),
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
        if !self.get(fd).is_some_and(|s| s.is_stream()) {
            return Err("io");
        }
        if max <= 0 {
            return Ok(Vec::new());
        }
        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        self.wait_ready(fd, true, crate::reactor::Interest::Read)?;
        self.read_ready(fd, max)
    }

    /// [`NetTable::read`]'s syscall half (see [`NetTable::accept_ready`]
    /// for the split's reason). `max` is already known positive.
    fn read_ready(&mut self, fd: i64, max: i64) -> Result<Vec<u8>, NetErr> {
        let Some(s) = self.get(fd).filter(|s| s.is_stream()) else {
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
        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        self.wait_ready(fd, true, crate::reactor::Interest::Write)?;
        self.write_ready(fd, bytes)
    }

    /// [`NetTable::write`]'s syscall half (see
    /// [`NetTable::accept_ready`] for the split's reason).
    fn write_ready(&mut self, fd: i64, bytes: &[u8]) -> Result<(), NetErr> {
        let Some(s) = self.get(fd).filter(|s| s.is_stream()) else {
            return Err("io");
        };
        s.write_all(bytes).map_err(|e| err_tag(e.kind()))
    }

    /// Close a socket; drop closes. Double close (or a forged fd) is
    /// the `io` row. A unix-domain LISTENER's path is unlinked here
    /// (s136, `[os.net.unix]`): the runtime bound it, the runtime
    /// removes it, so a clean shutdown leaves no stale socket file
    /// for the next bind to refuse as `exists`. A stream's close never
    /// touches the path.
    pub fn close(&mut self, fd: i64) -> Result<(), NetErr> {
        match usize::try_from(fd).ok().and_then(|i| self.socks.get_mut(i)) {
            Some(slot @ Some(_)) => {
                let gone = slot.take();
                #[cfg(unix)]
                if let Some(Entry {
                    sock: Sock::UnixListener(l, path),
                    ..
                }) = gone
                {
                    drop(l);
                    let _ = std::fs::remove_file(path);
                }
                #[cfg(not(unix))]
                drop(gone);
                Ok(())
            }
            _ => Err("io"),
        }
    }
}

// ----------------------------- s106: the native shim tier (#118) --
//
// The fs crossing pattern (s40/s90), applied to the table that was
// waiting: every entry returns a small ERROR CODE ([`net_code`]);
// lowering maps codes to the module's interned row tags (coarsening
// undeclared tags to `io`, exactly the checked executor's `coarse`)
// and builds the `!T` value — the runtime never traps and never sees
// a tag name. Text results materialize in the ambient region and
// return as `{ptr, len}` pairs through caller out slots, the fs
// shims' shape symbol for symbol.
//
// LOCK DISCIPLINE (the one place this family may not be fs-verbatim):
// the process table is one `Mutex<NetTable>`, and accept/read/write
// PARK — under native tasks a blocked accept holds its thread until
// the connect that resolves it runs on another. Holding the table
// across the park would deadlock exactly that pair, so the parking
// shims snapshot the park parameters under a short lock
// ([`NetTable::park_spec`]), wait in the reactor with the lock
// RELEASED, and relock for the syscall half (`*_ready`). The window
// between readiness and syscall is the same one `std::net` callers
// live with; the corpus discipline (one logical owner per socket)
// keeps it moot. Off-linux the shims keep the v0 blocking-syscall
// posture — the syscall itself blocks under the lock, the honest
// mirror of the checked lane's own path (native codegen is
// linux-gated at this tier anyway).

/// Error codes of the net family (lowering maps them to row tags,
/// coarsening any the call's row does not declare to `io` — the
/// checked lane's `coarse`, compile-time-dispatched). `UTF8` is the
/// read shim's own decode verdict, never [`err_tag`]'s.
pub mod net_code {
    pub const OK: i64 = 0;
    pub const REFUSED: i64 = 1;
    pub const TIMEOUT: i64 = 2;
    pub const CLOSED: i64 = 3;
    pub const UTF8: i64 = 4;
    pub const IO: i64 = 5;
    /// `net_write_bytes`'s own verdict: a list that is not a
    /// `List[byte]` — the wrong element width, an FFI caller's shape
    /// only since s136 typed the argument (the fs `fs_write_bytes`
    /// `INVALID`, mirrored). Never [`err_tag`]'s — a bad list is
    /// refused before any syscall.
    pub const INVALID: i64 = 6;
    /// s136 (#227): the unix-domain family on a host whose runtime
    /// does not serve it (windows at this pin) — refused BY NAME.
    pub const UNSUPPORTED: i64 = 7;
    /// s136: the unix-domain path rows (`[os.net.unix]`).
    pub const NOT_FOUND: i64 = 8;
    pub const DENIED: i64 = 9;
    pub const EXISTS: i64 = 10;
}

/// The process-wide socket table behind the shim family — the fs
/// `FILES` precedent, holding the [`NetTable`] the unit tests pin.
static NET: Mutex<NetTable> = Mutex::new(NetTable::new());

fn tbl() -> std::sync::MutexGuard<'static, NetTable> {
    NET.lock().unwrap_or_else(|p| p.into_inner())
}

/// A row tag as its wire code ([`net_code`]).
fn code_of_tag(tag: NetErr) -> i64 {
    match tag {
        "refused" => net_code::REFUSED,
        "timeout" => net_code::TIMEOUT,
        "closed" => net_code::CLOSED,
        "unsupported" => net_code::UNSUPPORTED,
        "not_found" => net_code::NOT_FOUND,
        "denied" => net_code::DENIED,
        "exists" => net_code::EXISTS,
        _ => net_code::IO,
    }
}

/// Await readiness with the table lock RELEASED (see the lock
/// discipline note above). Off-linux this is a no-op: the syscall
/// half blocks by itself, the v0 posture.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn wait_unlocked(
    fd: i64,
    want_stream: bool,
    interest: crate::reactor::Interest,
) -> Result<(), NetErr> {
    let (raw, deadline) = tbl().park_spec(fd, want_stream)?;
    match crate::reactor::wait_fd_net(raw, interest, deadline) {
        crate::reactor::IoWait::Ready => Ok(()),
        crate::reactor::IoWait::TimedOut => Err("timeout"),
        crate::reactor::IoWait::Cancelled => Err("io"),
    }
}

/// `net_listen(addr) -> int ! {io}` — the fd (>= 0), or `-code` on
/// failure (the `fs_open` convention: one i64 return carries both).
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_net_listen(ap: i64, al: i64) -> i64 {
    let addr = unsafe { view(ap, al) };
    match tbl().listen(addr) {
        Ok(fd) => fd,
        Err(t) => -code_of_tag(t),
    }
}

/// `net_listen_unix(path) -> int ! {unsupported, exists, not_found,
/// denied, io}` (s136, #227, `[os.net.unix]`) — the fd (>= 0), or
/// `-code`. See [`NetTable::listen_unix`] for the rows and the
/// per-host posture.
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_net_listen_unix(pp: i64, pl: i64) -> i64 {
    let path = unsafe { view(pp, pl) };
    match tbl().listen_unix(path) {
        Ok(fd) => fd,
        Err(t) => -code_of_tag(t),
    }
}

/// `net_connect_unix(path) -> int ! {unsupported, refused, not_found,
/// denied, io}` (s136, #227) — the stream's fd, or `-code`. See
/// [`NetTable::connect_unix`].
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_net_connect_unix(pp: i64, pl: i64) -> i64 {
    let path = unsafe { view(pp, pl) };
    match tbl().connect_unix(path) {
        Ok(fd) => fd,
        Err(t) => -code_of_tag(t),
    }
}

/// `net_port(fd) -> int ! {io}` — the bound local port (>= 0), or
/// `-code`.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_net_port(fd: i64) -> i64 {
    match tbl().port(fd) {
        Ok(p) => p,
        Err(t) => -code_of_tag(t),
    }
}

/// `net_accept(fd) -> int ! {timeout, io}` — parks until a connection
/// arrives (or the armed deadline fires); the stream's fd, or `-code`.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_net_accept(fd: i64) -> i64 {
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    if let Err(t) = wait_unlocked(fd, false, crate::reactor::Interest::Read) {
        return -code_of_tag(t);
    }
    match tbl().accept_ready(fd) {
        Ok(nfd) => nfd,
        Err(t) => -code_of_tag(t),
    }
}

/// `net_connect(addr) -> int ! {refused, timeout, io}` — the blocking
/// dial (the module doc's recorded delta); the stream's fd, or
/// `-code`.
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_net_connect(ap: i64, al: i64) -> i64 {
    let addr = unsafe { view(ap, al) };
    match tbl().connect(addr) {
        Ok(fd) => fd,
        Err(t) => -code_of_tag(t),
    }
}

/// `net_read(fd, max) -> str ! {closed, timeout, utf8, io}` — parks
/// until bytes arrive (at most `max`, clamped to 1 MiB); the peer's
/// orderly close is `CLOSED`, a non-UTF-8 arrival is `UTF8`.
///
/// # Safety
///
/// `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_net_read(fd: i64, max: i64, out: i64) -> i64 {
    {
        // The no-park fast paths, under one short lock: wrong-kind or
        // forged fd is `io` whatever `max` says (the #40 ordering),
        // and `max <= 0` is the empty str with no wait owed.
        let mut t = tbl();
        if !t.get(fd).is_some_and(|s| s.is_stream()) {
            return net_code::IO;
        }
        if max <= 0 {
            let p = ambient_copy(b"");
            unsafe { write_pair(out, p as i64, 0) };
            return net_code::OK;
        }
    }
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    if let Err(t) = wait_unlocked(fd, true, crate::reactor::Interest::Read) {
        return code_of_tag(t);
    }
    match tbl().read_ready(fd, max) {
        Err(t) => code_of_tag(t),
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(s) => {
                let p = ambient_copy(s.as_bytes());
                unsafe { write_pair(out, p as i64, s.len() as i64) };
                net_code::OK
            }
            Err(_) => net_code::UTF8,
        },
    }
}

/// `net_write(fd, s) -> () ! {closed, io}` — send space awaited at
/// admission (a fired deadline surfaces as `TIMEOUT`, which the row
/// coarsens to `io`); then the whole buffer drains.
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_net_write(fd: i64, sp: i64, sl: i64) -> i64 {
    let s = unsafe { view(sp, sl) };
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    if let Err(t) = wait_unlocked(fd, true, crate::reactor::Interest::Write) {
        return code_of_tag(t);
    }
    match tbl().write_ready(fd, s.as_bytes()) {
        Ok(()) => net_code::OK,
        Err(t) => code_of_tag(t),
    }
}

/// `net_read_bytes(fd, max) -> List[byte] ! {closed, timeout, io}` —
/// the byte twin of [`__wolf_rt_net_read`] (s115, #137): parks until
/// bytes arrive (at most `max`, clamped to 1 MiB); no `UTF8` verdict,
/// bytes are bytes, so a binary body finally survives the crossing.
/// The `fs_read_bytes` shape, family for family.
///
/// # Safety
///
/// `out` must address 8 writable bytes (the list header word).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_net_read_bytes(fd: i64, max: i64, out: i64) -> i64 {
    {
        // The no-park fast paths (mirror [`__wolf_rt_net_read`]):
        // wrong-kind or forged fd is `io` whatever `max` says, and
        // `max <= 0` is the empty list with no wait owed.
        let mut t = tbl();
        if !t.get(fd).is_some_and(|s| s.is_stream()) {
            return net_code::IO;
        }
        if max <= 0 {
            unsafe { write_bytes_list(out, b"") };
            return net_code::OK;
        }
    }
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    if let Err(t) = wait_unlocked(fd, true, crate::reactor::Interest::Read) {
        return code_of_tag(t);
    }
    match tbl().read_ready(fd, max) {
        Err(t) => code_of_tag(t),
        Ok(bytes) => {
            unsafe { write_bytes_list(out, &bytes) };
            net_code::OK
        }
    }
}

/// `net_write_bytes(fd, bytes) -> () ! {closed, invalid, io}` — the
/// byte twin of [`__wolf_rt_net_write`] (s115, #137): `bytes` is a
/// `List[byte]` (s136); a list of the wrong element width is `INVALID`
/// and nothing is sent (the `fs_write_bytes` refusal, mirrored);
/// otherwise the whole buffer drains with no UTF-8 gate.
///
/// # Safety
///
/// `hdr` must be a live `List[byte]` header.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_net_write_bytes(fd: i64, hdr: i64) -> i64 {
    let Some(bytes) = (unsafe { byte_elems(hdr) }) else {
        return net_code::INVALID;
    };
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    if let Err(t) = wait_unlocked(fd, true, crate::reactor::Interest::Write) {
        return code_of_tag(t);
    }
    match tbl().write_ready(fd, &bytes) {
        Ok(()) => net_code::OK,
        Err(t) => code_of_tag(t),
    }
}

/// `net_close(fd) -> () ! {io}` — tombstones the slot; double close
/// (or a forged fd) is `io`.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_net_close(fd: i64) -> i64 {
    match tbl().close(fd) {
        Ok(()) => net_code::OK,
        Err(t) => code_of_tag(t),
    }
}

/// `net_deadline(fd, ms) -> () ! {io}` — arm (`ms > 0`) or clear
/// (`ms <= 0`) the socket's deadline budget ([`NetTable::set_deadline`];
/// s106 arms what s35 built — wolf-lang#45's builtin half). Off-linux
/// the honest `io` refusal, never a silently-inert deadline.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_net_deadline(fd: i64, ms: i64) -> i64 {
    match tbl().set_deadline(fd, ms) {
        Ok(()) => net_code::OK,
        Err(t) => code_of_tag(t),
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

    /// A loopback port nothing will answer on — the refusal probes'
    /// target (#205).
    ///
    /// The old shape asked the kernel for an EPHEMERAL port
    /// (`listen("127.0.0.1:0")`), read it, closed it, and dialed —
    /// betting that nothing took the port in between. Under `cargo
    /// test`'s full parallelism the rest of the suite (and every other
    /// test binary cargo runs beside it) is asking that same kernel
    /// for ephemeral ports the whole time, and the bet lost on trunk:
    /// a full `cargo xtask ci` red at the `test` step while
    /// `shim_refused_code` passed 3/3 in isolation. A gate whose
    /// verdict depends on the host's port churn teaches the house to
    /// re-run instead of to read.
    ///
    /// A port from OUTSIDE the host's ephemeral range cannot be handed
    /// to anyone by `bind(0)` — kernels auto-assign only from the high
    /// range (49152.. on macOS and windows, 32768.. on linux) — so
    /// binding one to prove it is free and closing it leaves a port
    /// that STAYS quiet, and the dial after it is a refusal rather
    /// than a coin flip. A clean listener close leaves no TIME_WAIT
    /// (nothing was ever accepted), so the port is immediately dead.
    ///
    /// Holding a socket bound-but-never-listening was the other
    /// candidate and is not portable: macOS drops the SYN on such a
    /// socket instead of answering RST, so the dial hangs to its
    /// deadline instead of refusing (measured here before this shape
    /// was chosen).
    fn quiet_loopback_port() -> u16 {
        // Spread concurrent test binaries apart by pid so they rarely
        // probe the same port; correctness does not depend on it,
        // since a port nobody listens on refuses every dialer.
        let start = u16::try_from(std::process::id() % 8_000).unwrap_or(0);
        for i in 0..256u16 {
            let port = 20_000 + (start + i) % 8_000;
            if let Ok(l) = std::net::TcpListener::bind(("127.0.0.1", port)) {
                drop(l);
                return port;
            }
        }
        panic!("no free loopback port in 20000..28000 for the refusal probe");
    }

    /// Dialing a port nothing answers on is the `refused` row — a
    /// handleable outcome, never a trap.
    #[test]
    fn refused_is_a_row() {
        let mut t = NetTable::new();
        let port = quiet_loopback_port();
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
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
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
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
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

    // ---------------- s106: the shim tier over one process table --

    fn pair_of(s: &str) -> (i64, i64) {
        (s.as_ptr() as i64, s.len() as i64)
    }

    /// A `List[byte]` header, the shape a compiled byte argument has.
    fn bytes_list(bs: &[u8]) -> i64 {
        let hdr = crate::list::new_list(1);
        for b in bs {
            crate::list::push_raw(hdr, core::ptr::from_ref(b));
        }
        hdr as i64
    }

    /// A `List[int]` — the wrong width for the byte write (s136).
    fn int_list(bs: &[i64]) -> i64 {
        let hdr = crate::list::new_list(8);
        for &b in bs {
            crate::list::push_int(hdr, b);
        }
        hdr as i64
    }

    fn list_u8(hdr: i64) -> Vec<u8> {
        let n = unsafe { crate::list::__wolf_rt_list_len(hdr) };
        (0..n)
            .map(|i| {
                let mut cell = [0u8; 1];
                let rc =
                    unsafe { crate::list::__wolf_rt_list_read(hdr, i, cell.as_mut_ptr() as i64) };
                assert_eq!(rc, 1);
                cell[0]
            })
            .collect()
    }

    /// The corpus echo roundtrip through the extern surface: codes
    /// out, str pairs through the out slot, fd handles process-global
    /// — the fs shim tests' shape.
    #[test]
    fn shim_echo_roundtrip_and_rows() {
        let (ap, al) = pair_of("127.0.0.1:0");
        let srv = unsafe { __wolf_rt_net_listen(ap, al) };
        assert!(srv >= 0);
        let port = __wolf_rt_net_port(srv);
        assert!(port > 0);
        let addr = format!("127.0.0.1:{port}");
        let (cp, cl) = pair_of(&addr);
        let cli = unsafe { __wolf_rt_net_connect(cp, cl) };
        assert!(cli >= 0);
        let (mp, ml) = pair_of("ping");
        assert_eq!(unsafe { __wolf_rt_net_write(cli, mp, ml) }, net_code::OK);
        let conn = __wolf_rt_net_accept(srv);
        assert!(conn >= 0);
        let mut out = [0i64; 2];
        let o = out.as_mut_ptr() as i64;
        assert_eq!(unsafe { __wolf_rt_net_read(conn, 16, o) }, net_code::OK);
        assert_eq!(unsafe { view(out[0], out[1]) }, "ping");
        // `max <= 0` is the empty str, no wait owed.
        assert_eq!(unsafe { __wolf_rt_net_read(conn, 0, o) }, net_code::OK);
        assert_eq!(out[1], 0);
        assert_eq!(__wolf_rt_net_close(cli), net_code::OK);
        // The peer's finish is CLOSED; double close is IO.
        assert_eq!(unsafe { __wolf_rt_net_read(conn, 16, o) }, net_code::CLOSED);
        assert_eq!(__wolf_rt_net_close(cli), net_code::IO);
        assert_eq!(__wolf_rt_net_close(conn), net_code::OK);
        assert_eq!(__wolf_rt_net_close(srv), net_code::OK);
        // A forged fd is IO on every entry, never a trap.
        assert_eq!(__wolf_rt_net_accept(99_999), -net_code::IO);
        assert_eq!(__wolf_rt_net_port(99_999), -net_code::IO);
        assert_eq!(unsafe { __wolf_rt_net_read(99_999, 4, o) }, net_code::IO);
        assert_eq!(__wolf_rt_net_deadline(99_999, 50), net_code::IO);
    }

    /// Dialing a quiet port through the shim is the REFUSED code —
    /// `corpus/net/refused_row.lu`'s native half.
    ///
    /// #121 hardened this with a bounded retry over freshly-released
    /// EPHEMERAL ports; #205 is that retry's bill — the race it
    /// tolerated reddened a trunk gauntlet. The target comes from
    /// [`quiet_loopback_port`] now, so one dial is the whole story and
    /// anything but REFUSED is a real failure. Test-only — the corpus
    /// witness is unaffected.
    #[test]
    fn shim_refused_code() {
        let port = quiet_loopback_port();
        let addr = format!("127.0.0.1:{port}");
        let (cp, cl) = pair_of(&addr);
        let rc = unsafe { __wolf_rt_net_connect(cp, cl) };
        assert_eq!(
            rc,
            -net_code::REFUSED,
            "dial of the quiet port {port} answered {rc}, not REFUSED"
        );
    }

    /// A non-UTF-8 arrival is the UTF8 code (the read shim's own
    /// decode verdict, mirroring the checked lane's).
    #[test]
    fn shim_invalid_utf8_is_the_utf8_code() {
        let (ap, al) = pair_of("127.0.0.1:0");
        let srv = unsafe { __wolf_rt_net_listen(ap, al) };
        let port = __wolf_rt_net_port(srv);
        let mut raw = std::net::TcpStream::connect(format!("127.0.0.1:{port}")).expect("dial");
        let conn = __wolf_rt_net_accept(srv);
        raw.write_all(&[0xff, 0xfe]).expect("send");
        let mut out = [0i64; 2];
        let o = out.as_mut_ptr() as i64;
        assert_eq!(unsafe { __wolf_rt_net_read(conn, 16, o) }, net_code::UTF8);
        assert_eq!(__wolf_rt_net_close(conn), net_code::OK);
        assert_eq!(__wolf_rt_net_close(srv), net_code::OK);
    }

    /// s115/#137: the byte path carries what the str path mangles — a
    /// 0xFF, an embedded NUL, and a lone continuation byte (a split
    /// codepoint the UTF-8 gate would reject). Round-trips byte-equal
    /// through the shim boundary; a list that is not a `List[byte]` is
    /// `INVALID` and nothing is sent.
    #[test]
    fn shim_byte_roundtrip_and_invalid() {
        let (ap, al) = pair_of("127.0.0.1:0");
        let srv = unsafe { __wolf_rt_net_listen(ap, al) };
        let port = __wolf_rt_net_port(srv);
        let addr = format!("127.0.0.1:{port}");
        let (cp, cl) = pair_of(&addr);
        let cli = unsafe { __wolf_rt_net_connect(cp, cl) };
        let conn = __wolf_rt_net_accept(srv);
        // Bytes no UTF-8 reader can hold.
        let payload = [0xff, 0x00, 0x80, 0x41];
        assert_eq!(
            unsafe { __wolf_rt_net_write_bytes(cli, bytes_list(&payload)) },
            net_code::OK
        );
        let mut out = [0i64; 1];
        let o = out.as_mut_ptr() as i64;
        assert_eq!(
            unsafe { __wolf_rt_net_read_bytes(conn, 16, o) },
            net_code::OK
        );
        assert_eq!(list_u8(out[0]), payload.to_vec());
        // The str read would have refused the very same bytes.
        let (mp, ml) = pair_of("more");
        assert_eq!(unsafe { __wolf_rt_net_write(cli, mp, ml) }, net_code::OK);
        // `max <= 0` is the empty list, no wait owed.
        assert_eq!(
            unsafe { __wolf_rt_net_read_bytes(conn, 0, o) },
            net_code::OK
        );
        assert_eq!(list_u8(out[0]), Vec::<u8>::new());
        // A list that is not a `List[byte]` is INVALID; nothing is sent.
        assert_eq!(
            unsafe { __wolf_rt_net_write_bytes(cli, int_list(&[256])) },
            net_code::INVALID
        );
        // The pending "more" still arrives, proving the refusal sent
        // nothing ahead of it.
        assert_eq!(
            unsafe { __wolf_rt_net_read_bytes(conn, 16, o) },
            net_code::OK
        );
        assert_eq!(list_u8(out[0]), b"more".to_vec());
        // A forged fd is IO, never a trap.
        assert_eq!(
            unsafe { __wolf_rt_net_read_bytes(99_999, 4, o) },
            net_code::IO
        );
        assert_eq!(
            unsafe { __wolf_rt_net_write_bytes(99_999, bytes_list(&[1])) },
            net_code::IO
        );
        for fd in [cli, conn, srv] {
            assert_eq!(__wolf_rt_net_close(fd), net_code::OK);
        }
    }

    /// s136 (#227, `[os.net.unix]`): the unix-domain family through the
    /// extern surface on a unix host — an echo over a socket PATH, the
    /// accepted stream reading and writing like a TCP one, `net_port`
    /// answering `io` for a socket that has no port, and the cleanup
    /// posture: the listener's close unlinks the path. Rows by name:
    /// a stale path is `exists` at bind, no path is `not_found` at
    /// dial, a listener nobody accepts on still connects (the
    /// backlog), a path with no listener is `refused`.
    #[cfg(unix)]
    #[test]
    fn shim_unix_echo_rows_and_cleanup() {
        let dir = std::env::temp_dir().join(format!("wolf-s136-unix-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("echo.sock");
        let ps = path.display().to_string();
        let (pp, pl) = pair_of(&ps);
        let srv = unsafe { __wolf_rt_net_listen_unix(pp, pl) };
        assert!(srv >= 0, "bind: {srv}");
        assert!(
            path.exists(),
            "the socket file exists while the listener lives"
        );
        // A second bind on the same path is `exists`, by name.
        assert_eq!(
            unsafe { __wolf_rt_net_listen_unix(pp, pl) },
            -net_code::EXISTS
        );
        // A unix socket has no port.
        assert_eq!(__wolf_rt_net_port(srv), -net_code::IO);
        let cli = unsafe { __wolf_rt_net_connect_unix(pp, pl) };
        assert!(cli >= 0, "dial: {cli}");
        let (mp, ml) = pair_of("ping");
        assert_eq!(unsafe { __wolf_rt_net_write(cli, mp, ml) }, net_code::OK);
        let conn = __wolf_rt_net_accept(srv);
        assert!(conn >= 0, "accept: {conn}");
        let mut out = [0i64; 2];
        let o = out.as_mut_ptr() as i64;
        assert_eq!(unsafe { __wolf_rt_net_read(conn, 16, o) }, net_code::OK);
        assert_eq!(unsafe { view(out[0], out[1]) }, "ping");
        let payload = [0xff, 0x00, 0x80, 0x41];
        assert_eq!(
            unsafe { __wolf_rt_net_write_bytes(conn, bytes_list(&payload)) },
            net_code::OK
        );
        let mut hdr = [0i64; 1];
        let h = hdr.as_mut_ptr() as i64;
        assert_eq!(
            unsafe { __wolf_rt_net_read_bytes(cli, 16, h) },
            net_code::OK
        );
        assert_eq!(list_u8(hdr[0]), payload.to_vec());
        // Deadlines arm on a unix stream like a TCP one, and fire.
        assert_eq!(__wolf_rt_net_deadline(cli, 50), net_code::OK);
        assert_eq!(unsafe { __wolf_rt_net_read(cli, 16, o) }, net_code::TIMEOUT);
        assert_eq!(__wolf_rt_net_close(cli), net_code::OK);
        assert_eq!(__wolf_rt_net_close(conn), net_code::OK);
        // The listener's close unlinks the path — the cleanup posture.
        assert_eq!(__wolf_rt_net_close(srv), net_code::OK);
        assert!(!path.exists(), "close unlinked the socket file");
        // No path: `not_found`. A path nobody listens on: `refused`.
        assert_eq!(
            unsafe { __wolf_rt_net_connect_unix(pp, pl) },
            -net_code::NOT_FOUND
        );
        std::fs::write(&path, b"").unwrap();
        let stale = unsafe { __wolf_rt_net_connect_unix(pp, pl) };
        assert!(
            stale == -net_code::REFUSED || stale == -net_code::IO,
            "a plain file at the path is refused (or io on a host that spells ENOTSOCK): {stale}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// s136 (#227): on windows the family refuses BY NAME — the
    /// `unsupported` code, never a bare `io` — because `std::net` has
    /// no unix-domain surface there and the runtime carries no winsock
    /// binding for it (D15). The kernel's own answer is measured
    /// beside it: `socket(AF_UNIX, SOCK_STREAM, 0)` through ws2_32,
    /// which the runner's Windows Server (Win10 1803+) is expected to
    /// serve — the measurement the serving rung will be sized from.
    #[cfg(windows)]
    #[test]
    fn shim_unix_refuses_by_name_and_measures_af_unix() {
        let (pp, pl) = pair_of("C:/wolf-s136-probe.sock");
        assert_eq!(
            unsafe { __wolf_rt_net_listen_unix(pp, pl) },
            -net_code::UNSUPPORTED
        );
        assert_eq!(
            unsafe { __wolf_rt_net_connect_unix(pp, pl) },
            -net_code::UNSUPPORTED
        );
        // Winsock is initialized by std's first socket use.
        let _wake = std::net::UdpSocket::bind("127.0.0.1:0").expect("winsock up");
        #[link(name = "ws2_32")]
        unsafe extern "system" {
            fn socket(af: i32, ty: i32, proto: i32) -> usize;
            fn closesocket(s: usize) -> i32;
            fn WSAGetLastError() -> i32;
        }
        const AF_UNIX: i32 = 1;
        const SOCK_STREAM: i32 = 1;
        const INVALID_SOCKET: usize = usize::MAX;
        let s = unsafe { socket(AF_UNIX, SOCK_STREAM, 0) };
        if s == INVALID_SOCKET {
            let err = unsafe { WSAGetLastError() };
            panic!(
                "AF_UNIX probe: socket() refused with WSA error {err} — this runner's kernel has no AF_UNIX"
            );
        }
        unsafe { closesocket(s) };
    }

    /// s106: the TIMEOUT code crosses the shim boundary — the armed
    /// deadline fires with the table lock RELEASED (a parked read must
    /// not hold the table; see the shim tier's lock discipline), and
    /// clearing restores the indefinite wait.
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    #[test]
    fn shim_deadline_times_out_and_clears() {
        let (ap, al) = pair_of("127.0.0.1:0");
        let srv = unsafe { __wolf_rt_net_listen(ap, al) };
        let port = __wolf_rt_net_port(srv);
        let addr = format!("127.0.0.1:{port}");
        let (cp, cl) = pair_of(&addr);
        let cli = unsafe { __wolf_rt_net_connect(cp, cl) };
        let conn = __wolf_rt_net_accept(srv);
        let mut out = [0i64; 2];
        let o = out.as_mut_ptr() as i64;
        // No data: the read budget fires as the TIMEOUT code.
        assert_eq!(__wolf_rt_net_deadline(cli, 40), net_code::OK);
        assert_eq!(unsafe { __wolf_rt_net_read(cli, 16, o) }, net_code::TIMEOUT);
        // Data resolves the same socket after a clear.
        let (mp, ml) = pair_of("pong");
        assert_eq!(unsafe { __wolf_rt_net_write(conn, mp, ml) }, net_code::OK);
        assert_eq!(__wolf_rt_net_deadline(cli, 0), net_code::OK);
        assert_eq!(unsafe { __wolf_rt_net_read(cli, 16, o) }, net_code::OK);
        assert_eq!(unsafe { view(out[0], out[1]) }, "pong");
        for fd in [cli, conn, srv] {
            assert_eq!(__wolf_rt_net_close(fd), net_code::OK);
        }
    }
}
