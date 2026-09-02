//! The OS thread count of this process — the no-background-threads
//! assertions' instrument (no_spawn, no_io), one arm per tier-1 host
//! so the claim has teeth on each: `/proc/self/task` on linux,
//! `proc_pidinfo(PROC_PIDTASKINFO)` on macOS (s59), a toolhelp thread
//! snapshot on windows (s60b). Never a guess: a host without an arm
//! does not compile these tests.

#[cfg(target_os = "linux")]
pub fn os_thread_count() -> usize {
    std::fs::read_dir("/proc/self/task").map_or(1, |d| d.count())
}

/// macOS: the task's thread count straight from the kernel (no /proc
/// here).
#[cfg(target_os = "macos")]
pub fn os_thread_count() -> usize {
    // SAFETY: zeroed out-struct of the exact size the call contracts.
    unsafe {
        let mut ti: libc::proc_taskinfo = std::mem::zeroed();
        let sz = size_of::<libc::proc_taskinfo>() as i32;
        let n = libc::proc_pidinfo(
            std::process::id() as i32,
            libc::PROC_PIDTASKINFO,
            0,
            (&raw mut ti).cast(),
            sz,
        );
        if n == sz {
            ti.pti_threadnum as usize
        } else {
            1
        }
    }
}

/// windows: `CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD)` walked for
/// the entries owned by this process — the kernel's own list, the
/// toolhelp shape every process explorer reads. Declared directly
/// (the runtime's no-`windows-sys` posture, kept in its tests).
#[cfg(windows)]
pub fn os_thread_count() -> usize {
    use core::ffi::c_void;

    #[repr(C)]
    struct ThreadEntry32 {
        dw_size: u32,
        cnt_usage: u32,
        th32_thread_id: u32,
        th32_owner_process_id: u32,
        tp_base_pri: i32,
        tp_delta_pri: i32,
        dw_flags: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> *mut c_void;
        fn Thread32First(snapshot: *mut c_void, entry: *mut ThreadEntry32) -> i32;
        fn Thread32Next(snapshot: *mut c_void, entry: *mut ThreadEntry32) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }

    const TH32CS_SNAPTHREAD: u32 = 0x4;
    let me = std::process::id();
    // SAFETY: a fresh snapshot handle, a zeroed entry with its size
    // set as the API contracts, closed once.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snap as isize == -1 {
            return 1;
        }
        let mut e: ThreadEntry32 = std::mem::zeroed();
        e.dw_size = size_of::<ThreadEntry32>() as u32;
        let mut n = 0;
        let mut ok = Thread32First(snap, &mut e);
        while ok != 0 {
            if e.th32_owner_process_id == me {
                n += 1;
            }
            e.dw_size = size_of::<ThreadEntry32>() as u32;
            ok = Thread32Next(snap, &mut e);
        }
        CloseHandle(snap);
        n.max(1)
    }
}
