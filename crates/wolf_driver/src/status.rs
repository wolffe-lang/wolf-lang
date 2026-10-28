//! Driver-owned progress rendering. The compiler only sends observational events.
use std::collections::BTreeMap;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};
use wolf_span::progress::{self, Event};

#[derive(Clone, Default)]
pub enum Timings {
    #[default]
    None,
    Text,
    Json(PathBuf),
}
#[derive(Clone, Default)]
pub struct Options {
    pub quiet: bool,
    pub timings: Timings,
    pub command: String,
    reports: Arc<Mutex<Vec<serde_json::Value>>>,
}
impl Options {
    pub fn command(command: &str) -> Self {
        Self {
            command: command.into(),
            ..Self::default()
        }
    }
    pub fn flag(&mut self, arg: &str) -> Result<bool, String> {
        match arg {
            "-q" | "--quiet" => self.quiet = true,
            "--timings" => self.timings = Timings::Text,
            _ => {
                if let Some(path) = arg.strip_prefix("--timings=json:") {
                    if path.is_empty() {
                        return Err("--timings=json:<file> needs a path".into());
                    }
                    self.timings = Timings::Json(path.into());
                } else if arg.starts_with("--timings=") {
                    return Err("expected --timings or --timings=json:<file>".into());
                } else {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}

/// Drain tool output as it arrives: diagnostics remain on their original streams and
/// reset the same idle clock as driver diagnostics, even during a long link.
pub fn run_tool(command: &mut std::process::Command) -> io::Result<std::process::ExitStatus> {
    let current = CURRENT
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(Weak::upgrade);
    let active =
        current.is_some_and(|s| !s.state.lock().unwrap_or_else(|e| e.into_inner()).view.quiet);
    if !active {
        return command.status();
    }
    use std::io::Read;
    use std::process::Stdio;
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    std::thread::scope(|scope| {
        let out = scope.spawn(move || -> io::Result<()> {
            let mut buffer = [0u8; 8192];
            loop {
                let n = stdout.read(&mut buffer)?;
                if n == 0 {
                    return Ok(());
                }
                output(|| io::stdout().lock().write_all(&buffer[..n]))?;
            }
        });
        let err = scope.spawn(move || -> io::Result<()> {
            let mut buffer = [0u8; 8192];
            loop {
                let n = stderr.read(&mut buffer)?;
                if n == 0 {
                    return Ok(());
                }
                output(|| io::stderr().lock().write_all(&buffer[..n]))?;
            }
        });
        let status = child.wait();
        let out = out
            .join()
            .map_err(|_| io::Error::other("tool stdout reader panicked"))?;
        let err = err
            .join()
            .map_err(|_| io::Error::other("tool stderr reader panicked"))?;
        out?;
        err?;
        status
    })
}

static CURRENT: Mutex<Option<Weak<Shared>>> = Mutex::new(None);
static TEST_OPTIONS: OnceLock<Options> = OnceLock::new();
pub fn set_test_options(options: Options) {
    let _ = TEST_OPTIONS.set(options);
}
pub fn test_options() -> Option<&'static Options> {
    TEST_OPTIONS.get()
}

/// Serialize diagnostic writes with the live line, and reset the idle deadline.
/// With no compilation active, this is precisely the original write.
pub fn output<T>(write: impl FnOnce() -> T) -> T {
    let current = CURRENT
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(Weak::upgrade);
    if let Some(shared) = current {
        let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
        let clear = state.view.output(shared.start.elapsed());
        render(&clear);
        write()
    } else {
        write()
    }
}

struct Active {
    scope: &'static str,
    unit: String,
    phase: &'static str,
    detail: String,
    start: Duration,
}
struct State {
    view: View,
    active: BTreeMap<u64, Active>,
    rows: BTreeMap<(&'static str, String, &'static str, String), (u64, Duration)>,
    stopped: bool,
}
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    start: Instant,
}

/// Pure clock-driven state machine: the worker and fake-clock tests share every decision.
struct View {
    tty: bool,
    quiet: bool,
    after: Duration,
    next_heartbeat: Duration,
    last_heartbeat: Option<Duration>,
    visible: bool,
    done: usize,
    total: usize,
    reused: usize,
    command: String,
    tier: String,
}
impl View {
    fn new(tty: bool, dumb: bool, quiet: bool, after: Duration, command: &str, tier: &str) -> Self {
        Self {
            tty: tty && !dumb,
            quiet,
            after,
            next_heartbeat: after,
            last_heartbeat: None,
            visible: false,
            done: 0,
            total: 0,
            reused: 0,
            command: command.into(),
            tier: tier.into(),
        }
    }
    fn output(&mut self, now: Duration) -> String {
        self.next_heartbeat = now + self.after;
        self.clear()
    }
    fn clear(&mut self) -> String {
        if std::mem::take(&mut self.visible) {
            "\r\x1b[2K".into()
        } else {
            String::new()
        }
    }
    fn tick(
        &mut self,
        now: Duration,
        unit: &str,
        phase: &str,
        detail: &str,
        entered: Duration,
    ) -> String {
        if self.quiet {
            return String::new();
        }
        let unit = safe(unit);
        let phase = safe(phase);
        if self.tty {
            self.visible = true;
            let fill = self
                .done
                .saturating_mul(10)
                .checked_div(self.total)
                .unwrap_or(0)
                .min(10);
            let bar = format!("{}{}", "=".repeat(fill), " ".repeat(10 - fill));
            let detail = if detail.is_empty() {
                String::new()
            } else {
                format!(" ({})", safe(detail))
            };
            format!(
                "\r\x1b[2KBuilding [{bar}] {}/{} · {unit} · {} {phase}{detail} {}s ({} reused)",
                self.done,
                self.total,
                self.tier,
                now.saturating_sub(entered).as_secs(),
                self.reused
            )
        } else {
            if now < self.next_heartbeat {
                return String::new();
            }
            self.last_heartbeat = Some(now);
            self.next_heartbeat = now + Duration::from_secs(60);
            format!(
                "wolf {}: still in {phase} of {unit} ({} s)\n",
                self.command,
                now.as_secs()
            )
        }
    }
    fn finish(&mut self, now: Duration, success: bool) -> String {
        let mut text = self.clear();
        if !self.quiet && self.last_heartbeat.is_some() {
            text.push_str(&format!(
                "wolf {}: finished {} in {:.3}s{}\n",
                self.command,
                self.tier,
                now.as_secs_f64(),
                if success { "" } else { " (failed)" }
            ));
        }
        text
    }
}
fn safe(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .map(|c| if c.is_ascii() { c } else { '?' })
        .take(48)
        .collect()
}
fn render(text: &str) {
    if text.is_empty() {
        return;
    }
    let live = text.starts_with("\r\x1b[2KBuilding");
    terminal::visible(live);
    let text = if live {
        let prefix = "\r\x1b[2K";
        format!(
            "{prefix}{}",
            text[prefix.len()..]
                .chars()
                .take(terminal::columns().saturating_sub(3))
                .collect::<String>()
        )
    } else {
        text.to_string()
    };
    let mut stderr = io::stderr().lock();
    let _ = stderr.write_all(text.as_bytes());
    let _ = stderr.flush();
}

pub struct Reporter {
    shared: Arc<Shared>,
    worker: Option<std::thread::JoinHandle<()>>,
    options: Options,
    terminal: terminal::Guard,
    finished: bool,
}
impl Reporter {
    pub fn start(options: &Options, tier: &str) -> io::Result<Self> {
        let after = std::env::var("WOLF_STATUS_AFTER")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v >= 0.0 && *v <= 86400.0)
            .map(Duration::from_secs_f64)
            .unwrap_or(Duration::from_secs(30));
        let view = View::new(
            io::stderr().is_terminal(),
            std::env::var("TERM").as_deref() == Ok("dumb"),
            options.quiet,
            after,
            &options.command,
            tier,
        );
        let terminal = terminal::Guard::new(view.tty && !view.quiet);
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                view,
                active: BTreeMap::new(),
                rows: BTreeMap::new(),
                stopped: false,
            }),
            wake: Condvar::new(),
            start: Instant::now(),
        });
        let callback = Arc::clone(&shared);
        let collect = !matches!(options.timings, Timings::None);
        progress::listen(Some(Arc::new(move |event| {
            let now = callback.start.elapsed();
            let mut s = callback.state.lock().unwrap_or_else(|e| e.into_inner());
            match event {
                Event::Begin {
                    id,
                    scope,
                    unit,
                    phase,
                    detail,
                } => {
                    s.active.insert(
                        id,
                        Active {
                            scope,
                            unit,
                            phase,
                            detail,
                            start: now,
                        },
                    );
                }
                Event::End { id, elapsed } => {
                    if let Some(a) = s.active.remove(&id)
                        && collect
                    {
                        let row = s
                            .rows
                            .entry((a.scope, a.unit, a.phase, a.detail))
                            .or_default();
                        row.0 += 1;
                        row.1 += elapsed;
                    }
                }
                Event::Units(n) => {
                    s.view.total = n;
                    s.view.done = 0;
                    s.view.reused = 0;
                }
                Event::UnitDone { reused } => {
                    s.view.done += 1;
                    s.view.reused += usize::from(reused);
                }
            }
        })));
        *CURRENT.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::downgrade(&shared));
        let worker = if options.quiet {
            None
        } else {
            let callback = Arc::clone(&shared);
            match std::thread::Builder::new()
                .name("wolf-build-status".into())
                .spawn(move || {
                    let mut s = callback.state.lock().unwrap_or_else(|e| e.into_inner());
                    while !s.stopped {
                        let now = callback.start.elapsed();
                        let current = s.active.iter().max_by_key(|(id, a)| {
                            (
                                match a.scope {
                                    "pass" => 2,
                                    "unit" => 1,
                                    _ => 0,
                                },
                                *id,
                            )
                        });
                        let (unit, phase, detail, entered) = current
                            .map(|(_, a)| (a.unit.clone(), a.phase, a.detail.clone(), a.start))
                            .unwrap_or((
                                "package".into(),
                                "preparing",
                                String::new(),
                                Duration::ZERO,
                            ));
                        let text = s.view.tick(now, &unit, phase, &detail, entered);
                        render(&text);
                        s = callback
                            .wake
                            .wait_timeout(s, Duration::from_millis(100))
                            .unwrap_or_else(|e| e.into_inner())
                            .0;
                    }
                }) {
                Ok(worker) => Some(worker),
                Err(error) => {
                    progress::listen(None);
                    *CURRENT.lock().unwrap_or_else(|e| e.into_inner()) = None;
                    return Err(error);
                }
            }
        };
        Ok(Self {
            shared,
            worker,
            options: options.clone(),
            terminal,
            finished: false,
        })
    }
    pub fn finish(&mut self, success: bool) -> io::Result<()> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;
        progress::listen(None);
        {
            let mut s = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
            s.stopped = true;
            self.shared.wake.notify_all();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        *CURRENT.lock().unwrap_or_else(|e| e.into_inner()) = None;
        let mut s = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        let elapsed = self.shared.start.elapsed();
        render(&s.view.finish(elapsed, success));
        self.terminal.clear();
        let rows: Vec<_> = s.rows.iter().map(|((scope, unit, phase, detail), (count, seconds))| serde_json::json!({"scope": scope, "unit": unit, "phase": phase, "detail": detail, "count": count, "seconds": seconds.as_secs_f64()})).collect();
        match &self.options.timings {
            Timings::None => {}
            Timings::Text if !self.options.quiet => {
                let mut stderr = io::stderr().lock();
                writeln!(
                    stderr,
                    "wolf {} timings: {} {:.3}s (elapsed contributions overlap; package/unit/pass scopes are not additive)",
                    self.options.command,
                    s.view.tier,
                    elapsed.as_secs_f64()
                )?;
                writeln!(stderr, "scope\tunit\tphase\tdetail\tcount\tseconds")?;
                for ((scope, unit, phase, detail), (count, seconds)) in &s.rows {
                    writeln!(
                        stderr,
                        "{scope}\t{unit}\t{phase}\t{detail}\t{count}\t{:.6}",
                        seconds.as_secs_f64()
                    )?;
                }
            }
            Timings::Text => {}
            Timings::Json(path) => {
                let mut reports = self
                    .options
                    .reports
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                reports.push(serde_json::json!({"command": self.options.command, "tier": s.view.tier, "success": success, "seconds": elapsed.as_secs_f64(), "units": s.view.total, "reused": s.view.reused, "rows": rows}));
                let text = serde_json::to_vec_pretty(
                    &serde_json::json!({"schema": "wolf-build-timings/1", "builds": *reports}),
                )?;
                std::fs::write(path, text)?;
            }
        }
        Ok(())
    }
}
impl Drop for Reporter {
    fn drop(&mut self) {
        if let Err(error) = self.finish(false) {
            std::eprintln!("wolf {}: timings: {error}", self.options.command);
            std::process::exit(2);
        }
    }
}

#[cfg(unix)]
mod terminal {
    use std::sync::atomic::{AtomicBool, Ordering};
    static VISIBLE: AtomicBool = AtomicBool::new(false);
    pub fn visible(visible: bool) {
        VISIBLE.store(visible, Ordering::Relaxed);
    }
    pub fn columns() -> usize {
        unsafe {
            let mut size: libc::winsize = std::mem::zeroed();
            if libc::ioctl(2, libc::TIOCGWINSZ, &mut size) == 0 && size.ws_col > 0 {
                return usize::from(size.ws_col);
            }
        }
        80
    }
    extern "C" fn clear_at_exit() {
        if VISIBLE.swap(false, Ordering::Relaxed) {
            // Async-signal-safe; no Rust IO, allocation, lock, or unwinding.
            unsafe {
                libc::write(2, b"\r\x1b[2K".as_ptr().cast(), 5);
            }
        }
    }
    extern "C" fn interrupted(signal: libc::c_int) {
        clear_at_exit();
        // SA_RESETHAND has already restored default; re-deliver after returning.
        unsafe {
            libc::raise(signal);
        }
    }
    pub struct Guard {
        saved: Vec<(libc::c_int, libc::sigaction)>,
    }
    impl Guard {
        pub fn new(enabled: bool) -> Self {
            static REGISTER: std::sync::Once = std::sync::Once::new();
            let mut saved = Vec::new();
            if enabled {
                REGISTER.call_once(|| unsafe {
                    libc::atexit(clear_at_exit);
                });
                for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGQUIT] {
                    unsafe {
                        let mut old: libc::sigaction = std::mem::zeroed();
                        if libc::sigaction(signal, std::ptr::null(), &mut old) == 0
                            && old.sa_sigaction == libc::SIG_DFL
                        {
                            let mut action: libc::sigaction = std::mem::zeroed();
                            action.sa_sigaction = interrupted as *const () as usize;
                            action.sa_flags = libc::SA_RESETHAND;
                            libc::sigemptyset(&mut action.sa_mask);
                            if libc::sigaction(signal, &action, std::ptr::null_mut()) == 0 {
                                saved.push((signal, old));
                            }
                        }
                    }
                }
            }
            Self { saved }
        }
        pub fn clear(&mut self) {
            clear_at_exit();
            for (signal, old) in self.saved.drain(..) {
                unsafe {
                    libc::sigaction(signal, &old, std::ptr::null_mut());
                }
            }
        }
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            self.clear();
        }
    }
}
#[cfg(windows)]
mod terminal {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    static VISIBLE: AtomicBool = AtomicBool::new(false);
    static ACTIVE: AtomicBool = AtomicBool::new(false);
    static OLD_MODE: AtomicU32 = AtomicU32::new(0);
    #[repr(C)]
    struct Coord {
        x: i16,
        y: i16,
    }
    #[repr(C)]
    struct Rect {
        left: i16,
        top: i16,
        right: i16,
        bottom: i16,
    }
    #[repr(C)]
    struct Info {
        size: Coord,
        cursor: Coord,
        attributes: u16,
        window: Rect,
        maximum: Coord,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "GetStdHandle"]
        fn handle(which: u32) -> *mut c_void;
        #[link_name = "WriteFile"]
        fn write(
            handle: *mut c_void,
            data: *const u8,
            size: u32,
            written: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
        #[link_name = "GetConsoleMode"]
        fn get_mode(handle: *mut c_void, mode: *mut u32) -> i32;
        #[link_name = "SetConsoleMode"]
        fn set_mode(handle: *mut c_void, mode: u32) -> i32;
        #[link_name = "SetConsoleCtrlHandler"]
        fn ctrl_handler(handler: Option<extern "system" fn(u32) -> i32>, add: i32) -> i32;
        #[link_name = "GetConsoleScreenBufferInfo"]
        fn screen_info(handle: *mut c_void, info: *mut Info) -> i32;
    }
    unsafe extern "C" {
        fn atexit(callback: extern "C" fn()) -> i32;
    }
    fn stderr() -> *mut c_void {
        unsafe { handle((-12i32) as u32) }
    }
    pub fn visible(visible: bool) {
        VISIBLE.store(visible, Ordering::Relaxed);
    }
    pub fn columns() -> usize {
        unsafe {
            let mut info: Info = std::mem::zeroed();
            if screen_info(stderr(), &mut info) != 0 {
                return (i32::from(info.window.right) - i32::from(info.window.left) + 1).max(1)
                    as usize;
            }
        }
        80
    }
    extern "C" fn clear_at_exit() {
        if VISIBLE.swap(false, Ordering::Relaxed) {
            let mut written = 0;
            unsafe {
                write(
                    stderr(),
                    b"\r\x1b[2K".as_ptr(),
                    5,
                    &mut written,
                    std::ptr::null_mut(),
                );
            }
        }
        if ACTIVE.swap(false, Ordering::Relaxed) {
            unsafe {
                set_mode(stderr(), OLD_MODE.load(Ordering::Relaxed));
            }
        }
    }
    extern "system" fn interrupted(_: u32) -> i32 {
        clear_at_exit();
        0
    }
    pub struct Guard {
        installed: bool,
    }
    impl Guard {
        pub fn new(enabled: bool) -> Self {
            static REGISTER: std::sync::Once = std::sync::Once::new();
            let mut installed = false;
            if enabled {
                REGISTER.call_once(|| unsafe {
                    atexit(clear_at_exit);
                });
                let mut old = 0;
                unsafe {
                    if get_mode(stderr(), &mut old) != 0 && set_mode(stderr(), old | 4) != 0 {
                        OLD_MODE.store(old, Ordering::Relaxed);
                        ACTIVE.store(true, Ordering::Relaxed);
                        installed = ctrl_handler(Some(interrupted), 1) != 0;
                    }
                }
            }
            Self { installed }
        }
        pub fn clear(&mut self) {
            clear_at_exit();
            if self.installed {
                unsafe {
                    ctrl_handler(Some(interrupted), 0);
                }
                self.installed = false;
            }
        }
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            self.clear();
        }
    }
}
#[cfg(not(any(unix, windows)))]
mod terminal {
    pub fn visible(_: bool) {}
    pub fn columns() -> usize {
        80
    }
    pub struct Guard;
    impl Guard {
        pub fn new(_: bool) -> Self {
            Self
        }
        pub fn clear(&mut self) {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn view(tty: bool, dumb: bool, quiet: bool) -> View {
        View::new(
            tty,
            dumb,
            quiet,
            Duration::from_secs(30),
            "build",
            "release",
        )
    }
    fn tick(v: &mut View, seconds: u64) -> String {
        v.tick(
            Duration::from_secs(seconds),
            "yew",
            "midend/licm",
            "f",
            Duration::ZERO,
        )
    }
    #[test]
    fn non_tty_is_silent_until_threshold_then_every_sixty_seconds() {
        let mut v = view(false, false, false);
        assert_eq!(tick(&mut v, 29), "");
        assert_eq!(
            tick(&mut v, 30),
            "wolf build: still in midend/licm of yew (30 s)\n"
        );
        assert_eq!(tick(&mut v, 89), "");
        assert!(!tick(&mut v, 90).is_empty());
        assert_eq!(
            v.finish(Duration::from_secs(91), true),
            "wolf build: finished release in 91.000s\n"
        );
    }
    #[test]
    fn fast_build_and_output_reset_preserve_silence() {
        let mut v = view(false, false, false);
        assert_eq!(tick(&mut v, 1), "");
        assert_eq!(v.finish(Duration::from_secs(2), false), "");
        v.output(Duration::from_secs(20));
        assert_eq!(tick(&mut v, 49), "");
        assert!(!tick(&mut v, 50).is_empty());
        v.output(Duration::from_secs(80));
        assert_eq!(tick(&mut v, 109), "");
        assert!(!tick(&mut v, 110).is_empty());
    }
    #[test]
    fn tty_rewrites_and_clears_on_output_and_failure() {
        let mut v = view(true, false, false);
        v.total = 22;
        v.done = 14;
        v.reused = 2;
        let text = tick(&mut v, 151);
        assert!(text.starts_with("\r\x1b[2KBuilding [======    ] 14/22"));
        assert!(text.contains("151s (2 reused)"));
        assert_eq!(v.output(Duration::from_secs(152)), "\r\x1b[2K");
        assert!(!tick(&mut v, 153).is_empty());
        assert_eq!(v.finish(Duration::from_secs(154), false), "\r\x1b[2K");
    }
    #[test]
    fn dumb_uses_no_terminal_controls_and_quiet_silences_both_modes() {
        let mut dumb = view(true, true, false);
        assert_eq!(tick(&mut dumb, 1), "");
        assert!(!tick(&mut dumb, 30).contains('\x1b'));
        for tty in [false, true] {
            let mut quiet = view(tty, false, true);
            assert_eq!(tick(&mut quiet, 1000), "");
            assert_eq!(quiet.finish(Duration::from_secs(1001), true), "");
        }
    }
    #[test]
    fn override_and_option_validation() {
        let mut v = View::new(false, false, false, Duration::from_secs(1), "run", "native");
        assert_eq!(tick(&mut v, 0), "");
        assert!(tick(&mut v, 1).starts_with("wolf run:"));
        let mut options = Options::command("build");
        assert!(options.flag("--timings=json:").is_err());
        assert!(options.flag("--timings=bad").is_err());
        assert!(options.flag("--quiet").unwrap());
        assert!(options.flag("--timings").unwrap());
        assert!(!options.flag("--release").unwrap());
    }
}
