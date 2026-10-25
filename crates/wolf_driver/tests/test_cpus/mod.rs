//! wolf-lang#925 (s266): the CPUs a gate pins its programs to.
//!
//! The process and signal gates run each program again under `taskset`
//! on a few CPUs (wolf-lang#570: kasumi's sixteen hid a pool starvation a
//! four-CPU runner showed), and each spelled the set: `taskset -c 0-3`,
//! `taskset -c 0`, an absolute `/usr/bin/taskset`. On a shared host a
//! lane owns a BLOCK of CPUs (kasumi 4-7, hasu 6-11), and an explicit
//! `taskset -c 0-3` escapes the lane's own affinity into another lane's
//! block — rootless podman delegates no cpuset there, so the container
//! does not stop it (s263 measured it on hasu).
//!
//! So every request goes through here. The allocation is
//! `WOLF_TEST_CPUS` (a `taskset -c` list: `4-7`, `6,8-10`, `0-15:2`) or,
//! when it is unset, the CPUs this process may run on (linux's
//! `sched_getaffinity`; numbering is never assumed to start at zero).
//! A request for `n` CPUs takes the allocation's first `n`, and the
//! label names the list actually used (`taskset -c 4-7`, not `0-3`).
//! `taskset` itself comes from `PATH` (NixOS keeps it outside
//! `/usr/bin`). A request that cannot fit an allocation the operator
//! NAMED (`WOLF_TEST_CPUS`) fails the test by name; with no allocation
//! named, a host smaller than the request skips the pinned rows loudly,
//! as the gates did before (R-s266-4).
//!
//! Included by path (`mod test_cpus;`) in each gate file under
//! `crates/wolf_driver/tests/` that pins; `test_cpus_helper.rs` tests it.

#![allow(dead_code)] // each gate uses its own part

use std::path::PathBuf;
use std::process::Command;

/// The variable that names a lane's allocation.
pub const VAR: &str = "WOLF_TEST_CPUS";

/// A `taskset -c` CPU list (`N`, `N-M`, `N-M:S`, comma-joined) as the
/// sorted, de-duplicated CPUs it names.
pub fn parse_list(list: &str) -> Result<Vec<usize>, String> {
    let mut cpus = Vec::new();
    for part in list.trim().split(',') {
        let bad = || format!("{VAR}: {list:?} is not a CPU list (`4-7`, `6,8-10`, `0-15:2`)");
        let (range, step) = match part.split_once(':') {
            Some((r, s)) => (r, s.parse::<usize>().map_err(|_| bad())?),
            None => (part, 1),
        };
        let (lo, hi) = match range.split_once('-') {
            Some((a, b)) => (
                a.parse::<usize>().map_err(|_| bad())?,
                b.parse::<usize>().map_err(|_| bad())?,
            ),
            None => {
                let n = range.parse::<usize>().map_err(|_| bad())?;
                (n, n)
            }
        };
        if step == 0 || hi < lo || hi >= 4096 {
            return Err(bad());
        }
        cpus.extend((lo..=hi).step_by(step));
    }
    cpus.sort_unstable();
    cpus.dedup();
    if cpus.is_empty() {
        return Err(format!("{VAR}: {list:?} names no CPU"));
    }
    Ok(cpus)
}

/// CPUs as the shortest `taskset -c` list: runs as `N-M`, the rest
/// comma-joined (`[4, 5, 6, 7]` is `4-7`, `[4, 6]` is `4,6`).
pub fn render(cpus: &[usize]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < cpus.len() {
        let mut j = i;
        while j + 1 < cpus.len() && cpus[j + 1] == cpus[j] + 1 {
            j += 1;
        }
        out.push(if j > i {
            format!("{}-{}", cpus[i], cpus[j])
        } else {
            cpus[i].to_string()
        });
        i = j + 1;
    }
    out.join(",")
}

/// The CPUs this process may run on, ascending.
#[cfg(target_os = "linux")]
fn affinity() -> Vec<usize> {
    // SAFETY: a zeroed set filled by the kernel for this thread.
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        if libc::sched_getaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &mut set) != 0 {
            return Vec::new();
        }
        (0..libc::CPU_SETSIZE as usize)
            .filter(|&c| libc::CPU_ISSET(c, &set))
            .collect()
    }
}

#[cfg(not(target_os = "linux"))]
fn affinity() -> Vec<usize> {
    let n = std::thread::available_parallelism().map_or(1, |n| n.get());
    (0..n).collect()
}

/// Where the allocation came from, for a message.
pub fn source() -> String {
    match std::env::var(VAR) {
        Ok(v) => format!("{VAR}={v}"),
        Err(_) => "this process's affinity".to_string(),
    }
}

/// The allocation: `WOLF_TEST_CPUS`, or the process's own affinity.
/// A malformed `WOLF_TEST_CPUS`, or one naming a CPU this process may
/// not run on (linux), fails the test by name.
pub fn allocation() -> Vec<usize> {
    match std::env::var(VAR) {
        Ok(v) => {
            let named = parse_list(&v).unwrap_or_else(|e| panic!("{e}"));
            let own = affinity();
            if cfg!(target_os = "linux") && !own.is_empty() {
                let outside: Vec<usize> =
                    named.iter().copied().filter(|c| !own.contains(c)).collect();
                assert!(
                    outside.is_empty(),
                    "{VAR}={v} names CPU(s) {} this process may not run on (its affinity is {})",
                    render(&outside),
                    render(&own)
                );
            }
            named
        }
        Err(_) => affinity(),
    }
}

/// The first `n` CPUs of `alloc` as a `taskset -c` list, or why not.
pub fn take(alloc: &[usize], n: usize) -> Result<String, String> {
    if n == 0 || alloc.len() < n {
        return Err(format!(
            "{n} CPU(s) asked of an allocation of {} ({})",
            alloc.len(),
            render(alloc)
        ));
    }
    Ok(render(&alloc[..n]))
}

/// The first `n` CPUs of the allocation. `None` when they do not fit:
/// a test FAILS when `WOLF_TEST_CPUS` named the allocation, and skips
/// loudly (naming `test`) when the host itself is smaller.
pub fn subset(n: usize, test: &str) -> Option<String> {
    match take(&allocation(), n) {
        Ok(list) => Some(list),
        Err(why) => {
            assert!(
                std::env::var_os(VAR).is_none(),
                "{test}: {why} from {} — the pinned rows cannot run (wolf-lang#925)",
                source()
            );
            eprintln!(
                "SKIP: {test}: {why} from {} — the pinned rows did not run",
                source()
            );
            None
        }
    }
}

/// `taskset` from `PATH`, linux only.
pub fn taskset() -> Option<PathBuf> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .map(|d| d.join("taskset"))
        .find(|p| p.is_file())
}

/// A pin: `taskset -c <list>` from `PATH`, the list the allocation's.
#[derive(Debug, Clone)]
pub struct Pin {
    pub taskset: PathBuf,
    pub list: String,
    /// How many CPUs the list holds.
    pub n: usize,
}

impl Pin {
    /// `taskset -c <list>`: the label every pinned row carries.
    pub fn label(&self) -> String {
        format!("taskset -c {}", self.list)
    }

    /// The argv prefix: the `taskset` path, `-c`, the list.
    pub fn argv(&self) -> Vec<String> {
        vec![
            self.taskset.display().to_string(),
            "-c".to_string(),
            self.list.clone(),
        ]
    }

    /// A command that runs `program` under the pin.
    pub fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut c = Command::new(&self.taskset);
        c.arg("-c").arg(&self.list).arg(program);
        c
    }

    /// The pin as a shell word list for a command line run elsewhere (an
    /// inner container): `taskset` by name there, the same list.
    pub fn shell(&self, program: &str) -> String {
        format!("taskset -c {} {program}", self.list)
    }
}

/// `n` CPUs of the allocation under `taskset` from `PATH`: `None` on a
/// host with no `taskset` (not linux, or not on `PATH`) or one smaller
/// than `n` with no allocation named (both loud), and a failure by name
/// when `WOLF_TEST_CPUS` cannot fit `n`.
pub fn pin(n: usize, test: &str) -> Option<Pin> {
    let Some(taskset) = taskset() else {
        if cfg!(target_os = "linux") {
            eprintln!("SKIP: {test}: no taskset on PATH — the pinned rows did not run");
        }
        return None;
    };
    let list = subset(n, test)?;
    Some(Pin { taskset, list, n })
}

/// As [`pin`], for a gate whose linux host must pin and never skips: no
/// `taskset` on `PATH`, or an allocation smaller than `n`, fails the
/// test by name.
pub fn pin_required(n: usize, test: &str) -> Pin {
    let taskset = taskset().unwrap_or_else(|| {
        panic!(
            "{test}: `taskset` (util-linux) is part of this gate's linux host and is not on PATH"
        )
    });
    let list = take(&allocation(), n).unwrap_or_else(|why| {
        panic!(
            "{test}: {why} from {} — this gate pins and never skips (wolf-lang#925)",
            source()
        )
    });
    Pin { taskset, list, n }
}
