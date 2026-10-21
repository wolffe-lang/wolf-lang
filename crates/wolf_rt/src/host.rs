//! s253 (wolf-lang#819, `[os.uname]`): which OS and which architecture
//! the program runs on — `os_name() -> str` and `os_arch() -> str`, no
//! row (ruling asked, `[os.process.calls.note]` R5, the recommendation
//! implemented).
//!
//! The words are fixed by the clause. On a unix host they are what the
//! KERNEL answers (`uname(2)`), not what the program was built for, so a
//! linux binary on PAX's Linux-ABI kernel names PAX: the `sysname`
//! lowercased (`linux`, `freebsd`, `pax`), Darwin's as `macos`; the
//! `machine` lowercased, with macOS's and the BSDs' spellings of the two
//! tier-1 architectures as linux's (`arm64` is `aarch64`, `amd64` is
//! `x86_64`). Windows has no `uname`: `windows`, and the architecture the
//! program was built for (a process under emulation runs as its build's).
//! A kernel that answers nothing (no tier-1 host does) answers the
//! build's words, `std::env::consts`'s, which are the same vocabulary.
//! Read once a process: a kernel's name and machine do not change while
//! it runs.

use std::sync::OnceLock;

use crate::str::{ambient_copy, write_pair};

/// `(os, arch)`, read at the first ask.
fn host() -> &'static (String, String) {
    static HOST: OnceLock<(String, String)> = OnceLock::new();
    HOST.get_or_init(|| match uname() {
        Some((sysname, machine)) => (os_word(&sysname), arch_word(&machine)),
        None => (
            os_word(std::env::consts::OS),
            arch_word(std::env::consts::ARCH),
        ),
    })
}

/// The kernel's `sysname` and `machine`, or `None` where there is no
/// `uname` (windows) or it failed.
#[cfg(unix)]
fn uname() -> Option<(String, String)> {
    // SAFETY: `uname` fills a zeroed struct it is handed; every field is
    // a NUL-terminated array the read below stops inside.
    let u = unsafe {
        let mut u: libc::utsname = std::mem::zeroed();
        if libc::uname(&mut u) != 0 {
            return None;
        }
        u
    };
    let field = |f: &[libc::c_char]| -> String {
        let bytes: Vec<u8> = f.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
        String::from_utf8_lossy(&bytes).into_owned()
    };
    let (s, m) = (field(&u.sysname), field(&u.machine));
    if s.is_empty() || m.is_empty() {
        return None;
    }
    Some((s, m))
}

#[cfg(not(unix))]
fn uname() -> Option<(String, String)> {
    None
}

/// The OS word for a kernel's `sysname` (or a build's `consts::OS`):
/// lowercased, Darwin's `macos`.
pub fn os_word(sysname: &str) -> String {
    let lower = sysname.to_ascii_lowercase();
    match lower.as_str() {
        "darwin" => "macos".to_string(),
        _ => lower,
    }
}

/// The architecture word for a kernel's `machine` (or a build's
/// `consts::ARCH`): lowercased, `arm64` as `aarch64`, `amd64` as `x86_64`.
pub fn arch_word(machine: &str) -> String {
    let lower = machine.to_ascii_lowercase();
    match lower.as_str() {
        "arm64" => "aarch64".to_string(),
        "amd64" => "x86_64".to_string(),
        _ => lower,
    }
}

/// The OS word this process answers.
pub fn name() -> &'static str {
    // s253 PLANT (never merge): the word the kernel answers is dropped.
    let _ = host();
    "unknown"
}

/// The architecture word this process answers.
pub fn arch() -> &'static str {
    // s253 PLANT (never merge): the word the kernel answers is dropped.
    "unknown"
}

/// `os_name() -> str` (`[os.uname]`), the pair through `out`.
///
/// # Safety
///
/// `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_name(out: i64) {
    let s = name();
    let p = ambient_copy(s.as_bytes());
    unsafe { write_pair(out, p as i64, s.len() as i64) };
}

/// `os_arch() -> str` (`[os.uname]`), the pair through `out`.
///
/// # Safety
///
/// `out` must address 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_arch(out: i64) {
    let s = arch();
    let p = ambient_copy(s.as_bytes());
    unsafe { write_pair(out, p as i64, s.len() as i64) };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The vocabulary: each tier-1 kernel's spelling and the build's.
    #[test]
    fn the_words_are_the_clauses() {
        for (sysname, want) in [
            ("Linux", "linux"),
            ("Darwin", "macos"),
            ("FreeBSD", "freebsd"),
            ("PAX", "pax"),
            ("linux", "linux"),
            ("macos", "macos"),
            ("windows", "windows"),
        ] {
            assert_eq!(os_word(sysname), want, "{sysname}");
        }
        for (machine, want) in [
            ("x86_64", "x86_64"),
            ("aarch64", "aarch64"),
            ("arm64", "aarch64"),
            ("amd64", "x86_64"),
        ] {
            assert_eq!(arch_word(machine), want, "{machine}");
        }
    }

    /// This host's answer is its kernel's (on unix) and one of the
    /// vocabulary's on every tier-1 host the suite runs on.
    #[test]
    fn this_host_answers_its_kernel() {
        #[cfg(target_os = "linux")]
        assert_eq!(name(), "linux");
        #[cfg(target_os = "macos")]
        assert_eq!(name(), "macos");
        #[cfg(windows)]
        assert_eq!(name(), "windows");
        #[cfg(target_arch = "x86_64")]
        assert_eq!(arch(), "x86_64");
        #[cfg(target_arch = "aarch64")]
        assert_eq!(arch(), "aarch64");
        #[cfg(unix)]
        {
            let (s, m) = uname().expect("uname answers");
            assert_eq!(name(), os_word(&s));
            assert_eq!(arch(), arch_word(&m));
        }
    }
}
