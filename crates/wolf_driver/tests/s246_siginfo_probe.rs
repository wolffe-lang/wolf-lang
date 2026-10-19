//! s246 PROBE (never merged): what this host's kernel puts in a SIGSEGV's
//! siginfo when a process SENDS it (kill, raise) and when a real fault
//! raises it — measured black-box in a child process the test starts
//! (the test binary itself, re-run with S246_PROBE set), on CI's
//! throwaway runner, with cores off.

use std::process::Command;

extern "C" fn handler(_sig: libc::c_int, info: *mut libc::siginfo_t, _ctx: *mut libc::c_void) {
    // SAFETY: reading the kernel's siginfo; formatting into a stack
    // buffer and one write(2), then _exit.
    unsafe {
        let code = (*info).si_code;
        #[cfg(target_os = "macos")]
        let (pid, addr) = ((*info).si_pid, (*info).si_addr as usize);
        #[cfg(not(target_os = "macos"))]
        let (pid, addr) = ((*info).si_pid(), (*info).si_addr() as usize);
        let mut buf = [0u8; 128];
        let s = format_args!("");
        let _ = s;
        let text = format!("si_code={code} (0x{code:x}) si_pid={pid} si_addr=0x{addr:x}\n");
        let n = text.len().min(buf.len());
        buf[..n].copy_from_slice(&text.as_bytes()[..n]);
        libc::write(2, buf.as_ptr().cast(), n);
        libc::_exit(0);
    }
}

fn install() {
    // SAFETY: a SA_SIGINFO handler on SIGSEGV and SIGBUS for this child.
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = handler as extern "C" fn(libc::c_int, *mut libc::siginfo_t, *mut libc::c_void) as usize;
        sa.sa_flags = libc::SA_SIGINFO;
        libc::sigemptyset(&mut sa.sa_mask);
        libc::sigaction(libc::SIGSEGV, &sa, std::ptr::null_mut());
        libc::sigaction(libc::SIGBUS, &sa, std::ptr::null_mut());
    }
}

#[test]
fn probe() {
    match std::env::var("S246_PROBE").as_deref() {
        Ok("kill") => {
            install();
            // SAFETY: a process-directed SIGSEGV to this child.
            unsafe { libc::kill(libc::getpid(), libc::SIGSEGV) };
            std::thread::sleep(std::time::Duration::from_secs(1));
            eprintln!("no handler ran");
            std::process::exit(3);
        }
        Ok("raise") => {
            install();
            // SAFETY: a thread-directed SIGSEGV to this child.
            unsafe { libc::raise(libc::SIGSEGV) };
            eprintln!("no handler ran");
            std::process::exit(3);
        }
        Ok("fault") => {
            install();
            // SAFETY: a deliberate write to address 16 in this child, to
            // measure a real fault's siginfo; the handler exits.
            unsafe { std::ptr::write_volatile(16 as *mut u8, 1) };
            eprintln!("no fault");
            std::process::exit(3);
        }
        _ => {}
    }
    let me = std::env::current_exe().unwrap();
    for mode in ["kill", "raise", "fault"] {
        let out = Command::new("sh")
            .arg("-c")
            .arg("ulimit -c 0; exec \"$0\" probe --exact --nocapture --test-threads=1")
            .arg(&me)
            .env("S246_PROBE", mode)
            .output()
            .unwrap();
        use std::io::Write as _;
        let mut err = std::io::stderr().lock();
        let text = String::from_utf8_lossy(&out.stderr);
        let line = text.lines().find(|l| l.contains("si_code") || l.contains("no ")).unwrap_or("?");
        let _ = writeln!(err, "REPORT siginfo {mode}: {line} (exit {:?})", out.status);
    }
}
