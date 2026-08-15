//! The s40 native os/env runtime — argv, environment, cwd, exit.
//!
//! Semantics are `ubcheck.rs`'s `os_builtin`, entry for entry, with
//! the one DOCUMENTED lane asymmetry: the checked machine's `env_set`
//! writes a machine-local overlay (it runs inside a threaded test
//! host where `setenv` is unsound), while this runtime writes the
//! compiled program's own real environment — the program owns its
//! process. Everything else mirrors: `env_vars` is sorted `K=V` with
//! non-UTF-8 entries skipped, `env_get` rows are `missing`/`utf8`,
//! `env_set` rejects `=`/NUL/empty names as `invalid`, argv drops the
//! program name.
//!
//! The process trio (`os_spawn`/`os_wait`/`os_kill`) is CHECKED-LANE
//! ONLY at s40 (lowering refuses honestly, the s39 net posture): its
//! native rung needs List[str] argv unpacking plus a child table, and
//! lands with the std.process facade work.
//!
//! Error codes per entry (lowering maps them to row tags):
//! `env_get`: 0 ok, 1 missing, 2 utf8. `env_set`: 0 ok, 1 invalid.
//! `os_cwd`: 0 ok, 1 io.

use crate::str::{ambient_copy, view, write_pair};

/// Copy `s` into the ambient region and push it as a `{ptr, len}`
/// element of the 16-byte-element list at `hdr`.
fn push_str(hdr: *mut crate::list::ListHdr, s: &str) {
    let p = ambient_copy(s.as_bytes());
    let pair = [p as i64, s.len() as i64];
    crate::list::push_raw(hdr, pair.as_ptr().cast());
}

/// `env_args() -> List[str]` — the program's arguments, program name
/// dropped (argv[0] is the binary's path, not the program's input).
/// Non-UTF-8 arguments are skipped (unreachable through the str
/// tier).
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_env_args() -> i64 {
    let hdr = crate::list::new_list(16);
    for a in std::env::args().skip(1) {
        push_str(hdr, &a);
    }
    hdr as i64
}

/// `env_get(name) -> str ! {missing, utf8}`.
///
/// # Safety
///
/// A valid str pair; `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_env_get(np: i64, nl: i64, out: i64) -> i64 {
    let name = unsafe { view(np, nl) };
    match std::env::var(name) {
        Ok(v) => {
            let p = ambient_copy(v.as_bytes());
            unsafe { write_pair(out, p as i64, v.len() as i64) };
            0
        }
        Err(std::env::VarError::NotPresent) => 1,
        Err(std::env::VarError::NotUnicode(_)) => 2,
    }
}

/// `env_set(name, value) -> () ! {invalid}` — writes the process's
/// real environment (see the module doc's lane-asymmetry note).
///
/// # Safety
///
/// Both pairs must be valid str pairs. The write itself follows the
/// platform `setenv` contract: sound while no other thread reads the
/// environment concurrently — tasks that race `env_get` against
/// `env_set` are a program-owned data race, and the checked lane's
/// overlay is the racefree reference the std facade may later adopt
/// runtime-wide.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_env_set(np: i64, nl: i64, vp: i64, vl: i64) -> i64 {
    let (name, value) = unsafe { (view(np, nl), view(vp, vl)) };
    if name.is_empty() || name.contains('=') || name.contains('\0') || value.contains('\0') {
        return 1;
    }
    // SAFETY: name/value validated above; concurrency posture is the
    // caller's per the fn-level contract.
    unsafe { std::env::set_var(name, value) };
    0
}

/// `env_vars() -> List[str]` — `K=V` lines, SORTED (determinism over
/// environ order), non-UTF-8 entries skipped.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_env_vars() -> i64 {
    let mut vars: Vec<String> = std::env::vars_os()
        .filter_map(|(k, v)| {
            Some(format!(
                "{}={}",
                k.into_string().ok()?,
                v.into_string().ok()?
            ))
        })
        .collect();
    vars.sort();
    let hdr = crate::list::new_list(16);
    for kv in &vars {
        push_str(hdr, kv);
    }
    hdr as i64
}

/// `os_cwd() -> str ! {io}` — a non-UTF-8 cwd is `io` (unreachable
/// through the str tier, the fs coarsening rule).
///
/// # Safety
///
/// `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_cwd(out: i64) -> i64 {
    match std::env::current_dir() {
        Err(_) => 1,
        Ok(p) => match p.to_str() {
            None => 1,
            Some(s) => {
                let cp = ambient_copy(s.as_bytes());
                unsafe { write_pair(out, cp as i64, s.len() as i64) };
                0
            }
        },
    }
}

/// `os_exit(code)` — immediate termination, code masked to the
/// process range exactly as the checked lane masks it
/// (`rem_euclid(256)`); defers do NOT run (the documented contract).
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_exit(code: i64) -> ! {
    // `main` never returns through here, so the compiler's dump before
    // `ret` cannot fire; this is the s45 counterpart. No-op in a
    // normal build.
    crate::prof::dump_on_exit();
    std::process::exit(code.rem_euclid(256) as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_roundtrip_and_rows() {
        let name = format!("WOLF_RT_OS_TEST_{}", std::process::id());
        let (np, nl) = (name.as_ptr() as i64, name.len() as i64);
        let mut out = [0i64; 2];
        let o = out.as_mut_ptr() as i64;
        unsafe {
            // Absent: the missing code, never a trap.
            assert_eq!(__wolf_rt_env_get(np, nl, o), 1);
            let (vp, vl) = ("den".as_ptr() as i64, 3);
            assert_eq!(__wolf_rt_env_set(np, nl, vp, vl), 0);
            assert_eq!(__wolf_rt_env_get(np, nl, o), 0);
            assert_eq!(view(out[0], out[1]), "den");
            // Invalid names are the invalid code.
            let bad = "A=B";
            assert_eq!(
                __wolf_rt_env_set(bad.as_ptr() as i64, bad.len() as i64, vp, vl),
                1
            );
            assert_eq!(__wolf_rt_env_set(np, 0, vp, vl), 1); // empty
            // SAFETY: test-local name; no concurrent env readers care.
            std::env::remove_var(&name);
        }
    }

    #[test]
    fn vars_are_sorted_kv_lines() {
        let hdr = __wolf_rt_env_vars();
        let n = unsafe { crate::list::__wolf_rt_list_len(hdr) };
        let mut prev = String::new();
        for i in 0..n {
            let mut pair = [0i64; 2];
            let rc = unsafe { crate::list::__wolf_rt_list_read(hdr, i, pair.as_mut_ptr() as i64) };
            assert_eq!(rc, 1, "in-bounds read");
            let kv = unsafe { view(pair[0], pair[1]) };
            assert!(kv.contains('='), "K=V shape: {kv}");
            assert!(prev.as_str() <= kv, "sorted: {prev} <= {kv}");
            prev = kv.to_string();
        }
    }

    #[test]
    fn cwd_is_a_str() {
        let mut out = [0i64; 2];
        assert_eq!(unsafe { __wolf_rt_os_cwd(out.as_mut_ptr() as i64) }, 0);
        assert!(out[1] > 0);
    }
}
