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
    /// s137 (#235, `[os.proc.inherit]`): the OS descriptors this table
    /// ADOPTED through [`NetTable::adopt_listener`]. An adopted fd is
    /// owned by exactly one slot; adopting the same number twice would
    /// give two slots one descriptor (a double close waiting to
    /// happen), so a second adopt of a live one is `io`. Entries leave
    /// with the slot's close.
    #[cfg(unix)]
    adopted: Vec<std::os::fd::RawFd>,
}

/// s137 (#234): the `listen(2)` queue hint a `backlog <= 0` asks for —
/// std's own number, so `net_listen_with(addr, false, 0)` is
/// `net_listen(addr)` call for call.
#[cfg(unix)]
const DEFAULT_BACKLOG: i64 = 128;

/// s137 (#234, `[os.net.listen.opts]`): bind + listen a TCP socket by
/// hand so the options land BEFORE the bind — `SO_REUSEPORT` after
/// `bind` is too late, which is why `std::net::TcpListener::bind` (which
/// sets `SO_REUSEADDR` only) cannot serve this call. The socket is
/// close-on-exec like every std socket (the #235 posture: a child
/// inherits nothing it was not handed), `SO_REUSEADDR` is set as std
/// sets it, `SO_REUSEPORT` when asked, and the backlog is the hint the
/// caller gave (the host clamps it to its `somaxconn`).
#[cfg(unix)]
fn bind_with(addr: &str, reuse_port: bool, backlog: i64) -> std::io::Result<TcpListener> {
    use std::net::{SocketAddr, ToSocketAddrs as _};
    use std::os::fd::FromRawFd as _;
    let sa = addr
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    let family = match sa {
        SocketAddr::V4(_) => libc::AF_INET,
        SocketAddr::V6(_) => libc::AF_INET6,
    };
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    let ty = libc::SOCK_STREAM | libc::SOCK_CLOEXEC;
    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    let ty = libc::SOCK_STREAM;
    // SAFETY: plain socket creation; the result is checked below.
    let fd = unsafe { libc::socket(family, ty, 0) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // Owned from here: every early return below closes it.
    // SAFETY: `fd` is a fresh, open stream socket nothing else owns.
    let l = unsafe { TcpListener::from_raw_fd(fd) };
    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    {
        // SAFETY: valid fd; FD_CLOEXEC is the std posture on this host.
        if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    let one: libc::c_int = 1;
    let optlen = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
    // SAFETY: valid fd, a live c_int and its true length.
    if unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_REUSEADDR,
            (&raw const one).cast(),
            optlen,
        )
    } < 0
    {
        return Err(std::io::Error::last_os_error());
    }
    if reuse_port {
        // SAFETY: as above.
        if unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_REUSEPORT,
                (&raw const one).cast(),
                optlen,
            )
        } < 0
        {
            return Err(std::io::Error::last_os_error());
        }
    }
    // SAFETY: a zeroed sockaddr_storage is a valid, oversized buffer
    // for either family; the length passed is the family's own.
    let mut storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
    let len: libc::socklen_t = match sa {
        SocketAddr::V4(v4) => {
            // SAFETY: sockaddr_in fits inside sockaddr_storage.
            let sin = unsafe { &mut *(&raw mut storage).cast::<libc::sockaddr_in>() };
            sin.sin_family = libc::AF_INET as libc::sa_family_t;
            sin.sin_port = v4.port().to_be();
            sin.sin_addr = libc::in_addr {
                s_addr: u32::from_ne_bytes(v4.ip().octets()),
            };
            std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t
        }
        SocketAddr::V6(v6) => {
            // SAFETY: sockaddr_in6 fits inside sockaddr_storage.
            let sin6 = unsafe { &mut *(&raw mut storage).cast::<libc::sockaddr_in6>() };
            sin6.sin6_family = libc::AF_INET6 as libc::sa_family_t;
            sin6.sin6_port = v6.port().to_be();
            sin6.sin6_flowinfo = v6.flowinfo();
            sin6.sin6_addr = libc::in6_addr {
                s6_addr: v6.ip().octets(),
            };
            sin6.sin6_scope_id = v6.scope_id();
            std::mem::size_of::<libc::sockaddr_in6>() as libc::socklen_t
        }
    };
    // SAFETY: valid fd, a live sockaddr and its length.
    if unsafe { libc::bind(fd, (&raw const storage).cast(), len) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let queue = if backlog > 0 {
        i32::try_from(backlog).unwrap_or(i32::MAX)
    } else {
        DEFAULT_BACKLOG as i32
    };
    // SAFETY: valid, bound fd.
    if unsafe { libc::listen(fd, queue) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(l)
}

/// s137 (#235, `[os.proc.inherit]`): what an OS descriptor handed to
/// this process IS — a listening stream socket of a family the table
/// serves, or not ours to adopt. Three questions, each answered by the
/// kernel rather than trusted from the caller: `SO_TYPE` (a stream
/// socket at all — `ENOTSOCK`/`EBADF` say no), `getpeername` (it has
/// NO peer — a connected stream is not a listener; `ENOTCONN` is the
/// answer wanted) with `SO_ACCEPTCONN` beside it where the kernel
/// keeps a listening flag (linux, freebsd — macOS answers
/// `ENOPROTOOPT` to that option, measured, so there a bound socket
/// nobody called `listen` on adopts and its first accept is `io`),
/// and `getsockname`'s family.
#[cfg(unix)]
fn inherited_listener_family(raw: std::os::fd::RawFd) -> Option<libc::c_int> {
    let mut ty: libc::c_int = 0;
    let mut len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
    // SAFETY: a live c_int and its length; the fd is the caller's
    // number, and a bad one answers an error, never a fault.
    let rc = unsafe {
        libc::getsockopt(
            raw,
            libc::SOL_SOCKET,
            libc::SO_TYPE,
            (&raw mut ty).cast(),
            &mut len,
        )
    };
    if rc != 0 || ty != libc::SOCK_STREAM {
        return None;
    }
    // SAFETY: a zeroed sockaddr_storage is a valid receive buffer.
    let mut peer: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
    let mut plen = std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;
    // SAFETY: valid buffer and length.
    let rc = unsafe { libc::getpeername(raw, (&raw mut peer).cast(), &mut plen) };
    if rc == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ENOTCONN) {
        return None;
    }
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    {
        let mut accepting: libc::c_int = 0;
        let mut len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
        // SAFETY: as above.
        let rc = unsafe {
            libc::getsockopt(
                raw,
                libc::SOL_SOCKET,
                libc::SO_ACCEPTCONN,
                (&raw mut accepting).cast(),
                &mut len,
            )
        };
        if rc != 0 || accepting == 0 {
            return None;
        }
    }
    // SAFETY: a zeroed sockaddr_storage is a valid receive buffer.
    let mut storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
    let mut slen = std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;
    // SAFETY: valid buffer and length.
    let rc = unsafe { libc::getsockname(raw, (&raw mut storage).cast(), &mut slen) };
    if rc != 0 {
        return None;
    }
    Some(libc::c_int::from(storage.ss_family))
}

/// A net operation's failure: the row tag it raises.
pub type NetErr = &'static str;

impl NetTable {
    /// `const` so the shim tier's process table ([`NET`]) can live in a
    /// `static Mutex` without lazy-init machinery (the fs `FILES`
    /// precedent).
    pub const fn new() -> NetTable {
        NetTable {
            socks: Vec::new(),
            #[cfg(unix)]
            adopted: Vec::new(),
        }
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

    /// `net_listen_with(addr, reuse_port, backlog)` (s137, #234,
    /// `[os.net.listen.opts]`): [`NetTable::listen`] with the two
    /// listener options a prefork server needs. `reuse_port` sets
    /// `SO_REUSEPORT` before the bind, so N processes (or N listeners
    /// in one) share the port — what the kernel then DOES with the
    /// group is the host's, measured and named in the clause (linux
    /// distributes accepts by 4-tuple hash; macOS hands every SYN to
    /// the newest bound socket and falls to a survivor when it
    /// closes; windows refuses by name — `SO_REUSEADDR` there lets any
    /// process hijack the port and promises nothing about delivery,
    /// which is not this option). `backlog` is the `listen(2)` queue
    /// hint (`<= 0`: the runtime's default; the host clamps to its
    /// `somaxconn`). Rows: an address another socket holds without
    /// the option is `exists` (the "in use" row #234 asked for, in
    /// the vocabulary s136 already had), a privileged port `denied`,
    /// the host `unsupported` by name, the rest `io`.
    pub fn listen_with(
        &mut self,
        addr: &str,
        reuse_port: bool,
        backlog: i64,
    ) -> Result<i64, NetErr> {
        #[cfg(unix)]
        {
            match bind_with(addr, reuse_port, backlog) {
                Ok(l) => Ok(self.push(Sock::Listener(l))),
                Err(e) => Err(err_tag(e.kind())),
            }
        }
        #[cfg(not(unix))]
        {
            if reuse_port {
                return Err("unsupported");
            }
            // The backlog hint is the runtime's default on this host
            // at this pin (std's bind; named in docs/platforms.md).
            let _ = backlog;
            match TcpListener::bind(addr) {
                Ok(l) => Ok(self.push(Sock::Listener(l))),
                Err(e) => Err(err_tag(e.kind())),
            }
        }
    }

    /// `net_adopt_listener(fd)` (s137, #235, `[os.proc.inherit]`): take
    /// an OS descriptor this process was HANDED — by a parent's
    /// `os_spawn_with(.., inherit)`, numbered from 3 in the child in
    /// the order given — as a listener in this table. The kernel is
    /// asked what the number is ([`inherited_listener_family`]): a
    /// listening TCP socket becomes a [`Sock::Listener`], a listening
    /// `AF_UNIX` one a [`Sock::UnixListener`] whose path the child
    /// does NOT own (no unlink at close — the parent bound it), and
    /// anything else — not a socket, a connected stream, a foreign
    /// family, a number this table already adopted — is `io`, never a
    /// trap and never a wrapped stranger. The adopted descriptor is
    /// marked close-on-exec: this process's own children inherit only
    /// what it hands them. Windows: `unsupported`, by name (a `SOCKET`
    /// is not a small stable number; the serving rung is named in
    /// docs/platforms.md).
    pub fn adopt_listener(&mut self, fd: i64) -> Result<i64, NetErr> {
        #[cfg(unix)]
        {
            use std::os::fd::FromRawFd as _;
            let Ok(raw) = std::os::fd::RawFd::try_from(fd) else {
                return Err("io");
            };
            if raw < 0 || self.adopted.contains(&raw) {
                return Err("io");
            }
            let Some(family) = inherited_listener_family(raw) else {
                return Err("io");
            };
            let sock = match family {
                libc::AF_INET | libc::AF_INET6 => {
                    // SAFETY: the kernel just confirmed `raw` is a
                    // listening stream socket of the internet family;
                    // this table becomes its one owner.
                    Sock::Listener(unsafe { TcpListener::from_raw_fd(raw) })
                }
                libc::AF_UNIX => {
                    // SAFETY: as above, for the unix family.
                    Sock::UnixListener(
                        unsafe { UnixListener::from_raw_fd(raw) },
                        std::path::PathBuf::new(),
                    )
                }
                _ => return Err("io"),
            };
            // SAFETY: valid fd we now own.
            unsafe { libc::fcntl(raw, libc::F_SETFD, libc::FD_CLOEXEC) };
            self.adopted.push(raw);
            Ok(self.push(sock))
        }
        #[cfg(not(unix))]
        {
            let _ = fd;
            Err("unsupported")
        }
    }

    /// The OS descriptor behind a live handle — the spawn side's map
    /// for `os_spawn_with`'s inherit set (s137, #235). Any live socket
    /// qualifies (a listener is what a prefork master hands over; a
    /// stream is what an upgrade might); a forged or closed handle is
    /// `None`, which the caller answers as `io` BEFORE any child is
    /// spawned.
    #[cfg(unix)]
    pub(crate) fn raw_fd_of(&mut self, fd: i64) -> Option<std::os::fd::RawFd> {
        use std::os::fd::AsRawFd as _;
        match self.get(fd)? {
            Sock::Listener(l) => Some(l.as_raw_fd()),
            Sock::Stream(s) => Some(s.as_raw_fd()),
            Sock::UnixListener(l, _) => Some(l.as_raw_fd()),
            Sock::UnixStream(s) => Some(s.as_raw_fd()),
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
                {
                    // s137: an adopted descriptor leaves the adopted
                    // set with its slot (the number may come back
                    // through a later adopt, legitimately).
                    if let Some(e) = &gone {
                        use std::os::fd::AsRawFd as _;
                        let raw = match &e.sock {
                            Sock::Listener(l) => l.as_raw_fd(),
                            Sock::Stream(s) => s.as_raw_fd(),
                            Sock::UnixListener(l, _) => l.as_raw_fd(),
                            Sock::UnixStream(s) => s.as_raw_fd(),
                        };
                        self.adopted.retain(|&a| a != raw);
                    }
                    if let Some(Entry {
                        sock: Sock::UnixListener(l, path),
                        ..
                    }) = gone
                    {
                        drop(l);
                        // An EMPTY path is an adopted listener (s137,
                        // `[os.proc.inherit]`): the parent bound the
                        // file, the parent removes it.
                        if !path.as_os_str().is_empty() {
                            let _ = std::fs::remove_file(path);
                        }
                    }
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

/// `net_listen_with(addr, reuse_port, backlog) -> int ! {unsupported,
/// exists, denied, io}` (s137, #234, `[os.net.listen.opts]`) — the fd
/// (>= 0), or `-code`. `reuse` is the bool as an i64 (nonzero = set).
/// See [`NetTable::listen_with`] for the rows and the per-host posture.
///
/// # Safety
///
/// A valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_net_listen_with(
    ap: i64,
    al: i64,
    reuse: i64,
    backlog: i64,
) -> i64 {
    let addr = unsafe { view(ap, al) };
    match tbl().listen_with(addr, reuse != 0, backlog) {
        Ok(fd) => fd,
        Err(t) => -code_of_tag(t),
    }
}

/// `net_adopt_listener(fd) -> int ! {unsupported, io}` (s137, #235,
/// `[os.proc.inherit]`) — the table handle (>= 0) for an inherited OS
/// descriptor, or `-code`. See [`NetTable::adopt_listener`].
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_net_adopt_listener(fd: i64) -> i64 {
    match tbl().adopt_listener(fd) {
        Ok(h) => h,
        Err(t) => -code_of_tag(t),
    }
}

/// The OS descriptors behind a set of live handles, in order — the
/// spawn shim's map for `os_spawn_with`'s inherit set (s137, #235).
/// `None` when any handle is forged or closed: nothing is spawned on a
/// bad set.
#[cfg(unix)]
pub(crate) fn raw_fds_of(handles: &[i64]) -> Option<Vec<std::os::fd::RawFd>> {
    let mut t = tbl();
    handles.iter().map(|&h| t.raw_fd_of(h)).collect()
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

    // ---------------- s137: listener options and adoption (#234/#235) --

    /// s137 (#234, `[os.net.listen.opts]`): the option rows. Without
    /// `reuse_port` a second bind of a held address is `exists` — the
    /// "in use" row #234 asked for, spelled with the tag s136 already
    /// gave the vocabulary (lowering coarsens it to `io` for the plain
    /// `net_listen`, whose row is `{io}`); a group needs the option on
    /// EVERY member, so a reuse bind against a plain listener is
    /// `exists` too. With the option on both, the second bind
    /// succeeds and answers the same port. `(addr, false, 0)` is
    /// `net_listen(addr)` call for call, and a backlog hint of 1 still
    /// listens.
    #[cfg(unix)]
    #[test]
    fn listen_with_rows_and_reuse_port_shares_the_port() {
        let mut t = NetTable::new();
        let a = t.listen_with("127.0.0.1:0", false, 16).expect("bind");
        let port = t.port(a).expect("port");
        let addr = format!("127.0.0.1:{port}");
        assert_eq!(t.listen_with(&addr, false, 0), Err("exists"));
        assert_eq!(
            t.listen(&addr),
            Err("exists"),
            "the table's tag; lowering coarsens"
        );
        assert_eq!(
            t.listen_with(&addr, true, 0),
            Err("exists"),
            "the group needs both"
        );
        t.close(a).expect("close");
        let a = t.listen_with("127.0.0.1:0", true, 16).expect("bind a");
        let port = t.port(a).expect("port");
        let addr = format!("127.0.0.1:{port}");
        let b = t
            .listen_with(&addr, true, 16)
            .expect("bind b: the port is shared");
        assert_eq!(t.port(b).expect("port b"), port);
        t.close(a).expect("close a");
        t.close(b).expect("close b");
        // The default shape echoes exactly like `listen`.
        let srv = t
            .listen_with("127.0.0.1:0", false, 0)
            .expect("default shape");
        let port = t.port(srv).expect("port");
        let cli = t.connect(&format!("127.0.0.1:{port}")).expect("connect");
        t.write(cli, b"ping").expect("write");
        let conn = t.accept(srv).expect("accept");
        assert_eq!(t.read(conn, 16).expect("read"), b"ping");
        for fd in [cli, conn, srv] {
            t.close(fd).expect("close");
        }
        let one = t
            .listen_with("127.0.0.1:0", false, 1)
            .expect("backlog hint 1 listens");
        t.close(one).expect("close");
        // A privileged port is `denied` — unless the test runs as root,
        // in which case it binds (both are the truth about the host).
        match t.listen_with("127.0.0.1:1", false, 0) {
            Err("denied") => {}
            Ok(fd) => t.close(fd).expect("close"),
            other => panic!("port 1: {other:?}"),
        }
    }

    /// s137 (#234): what the kernel DOES with a `SO_REUSEPORT` group,
    /// MEASURED per host and pinned here so `[os.net.listen.opts]` and
    /// docs/platforms.md can never drift from the runner. Two hands on
    /// one port; each dial is accepted by whichever hand the kernel
    /// chose (the other's accept times out). linux distributes by
    /// 4-tuple hash — both hands accept. macOS hands EVERY SYN to the
    /// newest bound socket (0 to the older one — measured 0/64 by the
    /// s137 probe, in one process and across two) and, once that
    /// socket closes, to the survivor: the port is shared, accepts are
    /// not distributed, and failover is gap-free. Both hosts: every
    /// dial is accepted by SOME hand, and the survivor takes every
    /// dial after the newest closes.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn reuse_port_group_delivery_is_the_hosts_measured_posture() {
        let mut t = NetTable::new();
        let a = t.listen_with("127.0.0.1:0", true, 64).expect("bind a");
        let port = t.port(a).expect("port");
        let addr = format!("127.0.0.1:{port}");
        let b = t.listen_with(&addr, true, 64).expect("bind b");
        t.set_deadline(a, 60).expect("arm a");
        t.set_deadline(b, 60).expect("arm b");
        let mut keep = Vec::new();
        let (mut na, mut nb) = (0u32, 0u32);
        for _ in 0..24 {
            keep.push(std::net::TcpStream::connect(&addr).expect("dial"));
            match t.accept(a) {
                Ok(c) => {
                    na += 1;
                    t.close(c).expect("close");
                }
                Err("timeout") => {
                    let c = t.accept(b).expect("the other hand holds it");
                    nb += 1;
                    t.close(c).expect("close");
                }
                Err(e) => panic!("accept a: {e}"),
            }
        }
        assert_eq!(na + nb, 24, "every dial is accepted by one hand: {na}/{nb}");
        #[cfg(target_os = "linux")]
        assert!(
            na > 0 && nb > 0,
            "linux distributes a reuseport group's accepts — measured {na}/{nb}"
        );
        #[cfg(target_os = "macos")]
        assert_eq!(
            (na, nb),
            (0, 24),
            "macOS hands every SYN to the newest bound socket — measured older/newest {na}/{nb}; \
             if this moved, [os.net.listen.opts] and docs/platforms.md move with it"
        );
        // Failover: the newest closes, the survivor takes every dial.
        t.close(b).expect("close b");
        for _ in 0..8 {
            keep.push(std::net::TcpStream::connect(&addr).expect("dial after close"));
            let c = t.accept(a).expect("the survivor accepts");
            t.close(c).expect("close");
        }
        t.close(a).expect("close a");
    }

    /// s137 (#235, `[os.proc.inherit]`): adoption takes a handed
    /// descriptor — modelled here as a `dup` of a real listener's fd,
    /// which is exactly what a spawned child's 3 is — and the handle is
    /// the same listener (same port, accepts the dial, reads). One
    /// owner per number: a second adopt of a live number is `io`, and
    /// a close releases it for a fresh dup. Strangers are `io`, never
    /// a trap: a pipe end, a CONNECTED stream (a socket, but not
    /// listening), a negative or absurd number. The adopted fd is
    /// close-on-exec afterwards. A unix-domain listener adopts too,
    /// and its close leaves the socket file: the parent bound it.
    #[cfg(unix)]
    #[test]
    fn adopt_listener_takes_a_handed_descriptor_and_refuses_strangers() {
        use std::os::fd::AsRawFd as _;
        let mut t = NetTable::new();
        let parent = std::net::TcpListener::bind("127.0.0.1:0").expect("parent bind");
        let port = parent.local_addr().expect("addr").port();
        // SAFETY: a live listener fd; the dup is ours to hand over.
        let raw = unsafe { libc::dup(parent.as_raw_fd()) };
        assert!(raw >= 0);
        // Clear close-on-exec on the dup (a handed 3 has none) so the
        // adopt's own flag-setting is observable below.
        // SAFETY: our own fd.
        unsafe { libc::fcntl(raw, libc::F_SETFD, 0) };
        let l = t.adopt_listener(i64::from(raw)).expect("adopt");
        assert_eq!(
            t.port(l).expect("port"),
            i64::from(port),
            "the adopted handle is the same listener"
        );
        // SAFETY: our own fd.
        let flags = unsafe { libc::fcntl(raw, libc::F_GETFD) };
        assert!(
            flags & libc::FD_CLOEXEC != 0,
            "an adopted descriptor is close-on-exec"
        );
        let mut cli = std::net::TcpStream::connect(("127.0.0.1", port)).expect("dial");
        let conn = t.accept(l).expect("accept through the adopted handle");
        cli.write_all(b"hi").expect("send");
        assert_eq!(t.read(conn, 8).expect("read"), b"hi");
        assert_eq!(t.adopt_listener(i64::from(raw)), Err("io"), "one owner");
        t.close(conn).expect("close conn");
        t.close(l).expect("close l");
        // SAFETY: as above.
        let raw2 = unsafe { libc::dup(parent.as_raw_fd()) };
        let l2 = t
            .adopt_listener(i64::from(raw2))
            .expect("a fresh dup adopts again");
        t.close(l2).expect("close l2");
        // Strangers.
        let mut fds = [0i32; 2];
        // SAFETY: plain pipe creation; both ends are then marked
        // close-on-exec, because `os::tests` spawns children in this
        // same binary and witnesses that a child holds EXACTLY the
        // descriptors it was handed — a stray inheritable end sitting
        // at the next free number would falsify it.
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        // SAFETY: our own fds.
        unsafe {
            libc::fcntl(fds[0], libc::F_SETFD, libc::FD_CLOEXEC);
            libc::fcntl(fds[1], libc::F_SETFD, libc::FD_CLOEXEC);
        }
        assert_eq!(t.adopt_listener(i64::from(fds[0])), Err("io"), "a pipe");
        // SAFETY: our own fds.
        unsafe {
            libc::close(fds[0]);
            libc::close(fds[1]);
        }
        // SAFETY: a live stream fd; close-on-exec for the reason
        // above (this binary spawns children).
        let stream_raw = unsafe { libc::dup(cli.as_raw_fd()) };
        // SAFETY: our own fd.
        unsafe { libc::fcntl(stream_raw, libc::F_SETFD, libc::FD_CLOEXEC) };
        assert_eq!(
            t.adopt_listener(i64::from(stream_raw)),
            Err("io"),
            "a connected stream is not a listener"
        );
        // SAFETY: our own fd.
        unsafe { libc::close(stream_raw) };
        assert_eq!(t.adopt_listener(-1), Err("io"));
        assert_eq!(t.adopt_listener(1 << 40), Err("io"));
        assert_eq!(t.adopt_listener(99_999), Err("io"));
        // The unix family adopts; close leaves the file to its binder.
        let dir = std::env::temp_dir().join(format!("wolf-s137-adopt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("l.sock");
        let ul = UnixListener::bind(&path).expect("unix bind");
        // SAFETY: a live listener fd.
        let uraw = unsafe { libc::dup(ul.as_raw_fd()) };
        let uh = t.adopt_listener(i64::from(uraw)).expect("unix adopt");
        assert_eq!(t.port(uh), Err("io"), "no port on a unix listener");
        t.close(uh).expect("close");
        assert!(
            path.exists(),
            "the adopter does not unlink the parent's path"
        );
        drop(ul);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// s137: the two surfaces through the shim boundary — the fd or
    /// the negated code, the fs_open convention.
    #[cfg(unix)]
    #[test]
    fn shim_listen_with_and_adopt_codes() {
        let (ap, al) = pair_of("127.0.0.1:0");
        let a = unsafe { __wolf_rt_net_listen_with(ap, al, 1, 8) };
        assert!(a >= 0, "bind: {a}");
        let port = __wolf_rt_net_port(a);
        let addr = format!("127.0.0.1:{port}");
        let (cp, cl) = pair_of(&addr);
        let b = unsafe { __wolf_rt_net_listen_with(cp, cl, 1, 8) };
        assert!(b >= 0, "the group's second bind: {b}");
        assert_eq!(__wolf_rt_net_close(a), net_code::OK);
        assert_eq!(__wolf_rt_net_close(b), net_code::OK);
        let plain = unsafe { __wolf_rt_net_listen_with(ap, al, 0, 0) };
        assert!(plain >= 0);
        let port = __wolf_rt_net_port(plain);
        let addr = format!("127.0.0.1:{port}");
        let (cp, cl) = pair_of(&addr);
        assert_eq!(
            unsafe { __wolf_rt_net_listen_with(cp, cl, 0, 0) },
            -net_code::EXISTS,
            "in use, by name"
        );
        assert_eq!(__wolf_rt_net_close(plain), net_code::OK);
        assert_eq!(__wolf_rt_net_adopt_listener(-1), -net_code::IO);
        assert_eq!(__wolf_rt_net_adopt_listener(99_999), -net_code::IO);
        // A real handed descriptor through the shim.
        use std::os::fd::AsRawFd as _;
        let parent = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        // SAFETY: a live listener fd.
        let raw = unsafe { libc::dup(parent.as_raw_fd()) };
        let h = __wolf_rt_net_adopt_listener(i64::from(raw));
        assert!(h >= 0, "adopt: {h}");
        assert_eq!(
            __wolf_rt_net_port(h),
            i64::from(parent.local_addr().unwrap().port())
        );
        assert_eq!(__wolf_rt_net_close(h), net_code::OK);
    }

    /// s137 on windows: `reuse_port` and adoption refuse BY NAME (the
    /// `unsupported` code, never a bare `io`), the option-less shape
    /// serves with `exists` for a held address, and the runner MEASURES
    /// what `SO_REUSEADDR` would have meant: two ws2_32 sockets both
    /// carrying it bind one port, and the kernel's delivery to the pair
    /// is pinned so docs/platforms.md states the runner's answer, not
    /// a manual's. (`SO_REUSEADDR` there lets any process take a bound
    /// port — nginx's `reuseport` is not that — which is why the option
    /// is refused rather than aliased.)
    #[cfg(windows)]
    #[test]
    fn shim_listen_opts_refuse_by_name_on_windows_and_measure_so_reuseaddr() {
        let (ap, al) = pair_of("127.0.0.1:0");
        assert_eq!(
            unsafe { __wolf_rt_net_listen_with(ap, al, 1, 0) },
            -net_code::UNSUPPORTED
        );
        assert_eq!(__wolf_rt_net_adopt_listener(3), -net_code::UNSUPPORTED);
        let l = unsafe { __wolf_rt_net_listen_with(ap, al, 0, 16) };
        assert!(l >= 0, "the option-less shape serves: {l}");
        let port = __wolf_rt_net_port(l);
        let addr = format!("127.0.0.1:{port}");
        let (cp, cl) = pair_of(&addr);
        assert_eq!(
            unsafe { __wolf_rt_net_listen_with(cp, cl, 0, 0) },
            -net_code::EXISTS,
            "in use, by name"
        );
        assert_eq!(__wolf_rt_net_close(l), net_code::OK);

        // THE MEASUREMENT.
        let _wake = std::net::UdpSocket::bind("127.0.0.1:0").expect("winsock up");
        #[repr(C)]
        struct SockAddrIn {
            family: u16,
            port: u16,
            addr: u32,
            zero: [u8; 8],
        }
        #[link(name = "ws2_32")]
        unsafe extern "system" {
            fn socket(af: i32, ty: i32, proto: i32) -> usize;
            fn setsockopt(s: usize, level: i32, name: i32, val: *const u8, len: i32) -> i32;
            fn bind(s: usize, name: *const SockAddrIn, namelen: i32) -> i32;
            fn listen(s: usize, backlog: i32) -> i32;
            fn getsockname(s: usize, name: *mut SockAddrIn, namelen: *mut i32) -> i32;
            fn accept(s: usize, addr: *mut u8, len: *mut i32) -> usize;
            fn closesocket(s: usize) -> i32;
            fn ioctlsocket(s: usize, cmd: i32, argp: *mut u32) -> i32;
            fn WSAGetLastError() -> i32;
        }
        const AF_INET: i32 = 2;
        const SOCK_STREAM: i32 = 1;
        const SOL_SOCKET: i32 = 0xffff;
        const SO_REUSEADDR: i32 = 0x0004;
        const INVALID_SOCKET: usize = usize::MAX;
        const FIONBIO: i32 = -2_147_195_266; // 0x8004667e
        let mk = |port: u16| -> Result<usize, i32> {
            // SAFETY: plain winsock calls on a socket we own; every
            // result is checked.
            unsafe {
                let s = socket(AF_INET, SOCK_STREAM, 0);
                if s == INVALID_SOCKET {
                    return Err(WSAGetLastError());
                }
                let one: i32 = 1;
                if setsockopt(s, SOL_SOCKET, SO_REUSEADDR, (&raw const one).cast(), 4) != 0 {
                    let e = WSAGetLastError();
                    closesocket(s);
                    return Err(e);
                }
                let sa = SockAddrIn {
                    family: AF_INET as u16,
                    port: port.to_be(),
                    addr: u32::from_ne_bytes([127, 0, 0, 1]),
                    zero: [0; 8],
                };
                if bind(s, &sa, std::mem::size_of::<SockAddrIn>() as i32) != 0 {
                    let e = WSAGetLastError();
                    closesocket(s);
                    return Err(e);
                }
                if listen(s, 16) != 0 {
                    let e = WSAGetLastError();
                    closesocket(s);
                    return Err(e);
                }
                let mut nb: u32 = 1;
                ioctlsocket(s, FIONBIO, &mut nb);
                Ok(s)
            }
        };
        let a = mk(0).expect("first SO_REUSEADDR bind");
        let mut sa = SockAddrIn {
            family: 0,
            port: 0,
            addr: 0,
            zero: [0; 8],
        };
        let mut len = std::mem::size_of::<SockAddrIn>() as i32;
        // SAFETY: a live socket and a correctly sized out struct.
        assert_eq!(unsafe { getsockname(a, &mut sa, &mut len) }, 0);
        let port = u16::from_be(sa.port);
        let b = mk(port);
        let b = match b {
            Ok(b) => b,
            Err(e) => panic!(
                "MEASURED: a second SO_REUSEADDR bind of a held port was refused with WSA error {e} \
                 — the runner's answer; docs/platforms.md states the other one"
            ),
        };
        let mut keep = Vec::new();
        let (mut na, mut nb) = (0u32, 0u32);
        for _ in 0..16 {
            keep.push(
                std::net::TcpStream::connect(("127.0.0.1", port)).expect("dial the shared port"),
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
            // SAFETY: live nonblocking listeners; a WOULDBLOCK answers
            // INVALID_SOCKET, which is the "not here" branch.
            unsafe {
                let x = accept(a, std::ptr::null_mut(), std::ptr::null_mut());
                if x != INVALID_SOCKET {
                    closesocket(x);
                    na += 1;
                    continue;
                }
                let y = accept(b, std::ptr::null_mut(), std::ptr::null_mut());
                if y != INVALID_SOCKET {
                    closesocket(y);
                    nb += 1;
                }
            }
        }
        // SAFETY: our own sockets.
        unsafe {
            closesocket(a);
            closesocket(b);
        }
        assert_eq!(
            (na, nb),
            (16, 0),
            "MEASURED on the windows runner: SO_REUSEADDR pair delivery, first/second bound = \
             {na}/{nb} of 16 dials — every dial to the FIRST socket bound, none to the second. \
             That is the sentence docs/platforms.md states, and it is the reason `reuse_port` is \
             refused by name rather than aliased: SO_REUSEADDR lets the second bind SUCCEED and \
             then gives it nothing, which is the worst of both answers for a prefork worker. A \
             different split is the runner's answer and the sentence moves to it."
        );
    }

    /// s137 on windows: what a listener's `SOCKET` marked
    /// `HANDLE_FLAG_INHERIT` is worth across `std::process::Command`,
    /// MEASURED on the runner rather than assumed. This binary
    /// re-executes itself with the handle value named in the
    /// environment and asks the child what that number is: 42 = the
    /// same listener (its local port matches), 7 = a different socket,
    /// 8 = not a usable socket in the child at all.
    ///
    /// **The runner answers 8.** Marking the handle inheritable is not
    /// enough: `Command` on this host publishes only its stdio handles
    /// to the child, so the socket does not arrive. That is a stronger
    /// reason for the by-name refusal than the one #235 was filed
    /// with — the gap is not just that a `SOCKET` has no small stable
    /// number to name by position, it is that the descriptor does not
    /// cross at all through the spawn the runtime performs. If this
    /// number ever moves, docs/platforms.md's windows sentence and
    /// `[os.proc.inherit]`'s host posture move with it.
    #[cfg(windows)]
    #[test]
    fn windows_socket_handle_inheritance_measured() {
        use std::os::windows::io::{AsRawSocket as _, FromRawSocket as _};
        if let Ok(v) = std::env::var("WOLF_S137_INHERIT_CHILD") {
            let (h, port) = v.split_once(':').expect("handle:port");
            let h: u64 = h.parse().expect("handle");
            // SAFETY: the parent handed this SOCKET; if it is not a
            // socket in this process the call below answers an error,
            // never a fault.
            let l = unsafe { std::net::TcpListener::from_raw_socket(h) };
            let code = match l.local_addr() {
                Ok(a) if a.port().to_string() == port => 42,
                Ok(_) => 7,
                Err(_) => 8,
            };
            std::process::exit(code);
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn SetHandleInformation(h: usize, mask: u32, flags: u32) -> i32;
        }
        const HANDLE_FLAG_INHERIT: u32 = 1;
        let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = l.local_addr().unwrap().port();
        // SAFETY: a live socket handle we own.
        let ok = unsafe {
            SetHandleInformation(
                l.as_raw_socket() as usize,
                HANDLE_FLAG_INHERIT,
                HANDLE_FLAG_INHERIT,
            )
        };
        assert_ne!(
            ok, 0,
            "SetHandleInformation(HANDLE_FLAG_INHERIT) on a socket"
        );
        let status = std::process::Command::new(std::env::current_exe().expect("exe"))
            .args([
                "--exact",
                "net::tests::windows_socket_handle_inheritance_measured",
                "--nocapture",
            ])
            .env(
                "WOLF_S137_INHERIT_CHILD",
                format!("{}:{port}", l.as_raw_socket()),
            )
            .status()
            .expect("re-exec");
        assert_eq!(
            status.code(),
            Some(8),
            "MEASURED on the windows runner: 8 — a listener's SOCKET marked \
             HANDLE_FLAG_INHERIT does NOT arrive usable in a child spawned by \
             std::process::Command (42 = the same listener; 7 = a different socket; \
             8 = not a usable socket in the child; anything else = the harness). \
             docs/platforms.md's inheritance sentence is this number"
        );
    }
}
