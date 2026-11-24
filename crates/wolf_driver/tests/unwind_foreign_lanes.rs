//! A foreign exception that meets a wolf frame ends the process, with the
//! hosted Linux link carrying GCC's unwinder statically (`-static-libgcc`).
//!
//! Wolf frames carry no unwind tables (`[abi.native.nounwind]`), and the
//! runtime's own Rust panics are caught inside the runtime. So the one place
//! two unwinders could meet in a wolf process is a foreign library that
//! throws: its unwinder is the shared `libgcc_s` (which `libstdc++` brings
//! in), while the executable now carries its own copy. The thrower's
//! unwinder walks up, finds no table for the wolf frame, stops, and C++
//! terminates the process; the executable's copy is never asked. This row
//! holds that outcome: a wolf program calls a C++ function that throws
//! through `extern "c"`, built by `wolf build` itself with `CC` set to a
//! wrapper that adds the thrower library to the driver's own link line, and
//! ends by SIGABRT with libstdc++'s terminate message, never running past the
//! call. It also reads that the driver's line carried `-static-libgcc`, so
//! the row runs the link a wolf program gets.
//!
//! Linux with glibc only (the flag's cfg). Without a C++ compiler the row
//! skips loudly, or fails where `WOLF_FOREIGN_UNWIND_REQUIRE` is set (the
//! Linux CI job sets it).
#![cfg(all(target_os = "linux", target_env = "gnu"))]

mod lane_exit;

use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

fn fixture(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/unwind_foreign")
        .join(name);
    assert!(p.is_file(), "fixture missing: {}", p.display());
    p
}

/// The host's C++ compiler, `g++` first.
fn cxx() -> Option<&'static str> {
    ["g++", "clang++"].into_iter().find(|c| {
        Command::new(c)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

#[test]
fn a_foreign_throw_through_a_wolf_frame_ends_the_process_with_the_static_unwinder() {
    let Some(cxx) = cxx() else {
        assert!(
            std::env::var_os("WOLF_FOREIGN_UNWIND_REQUIRE").is_none(),
            "WOLF_FOREIGN_UNWIND_REQUIRE is set and there is no g++ or clang++: the row may \
             not answer with a skip"
        );
        eprintln!("SKIP: no g++ or clang++ on this host: the foreign-unwinder row cannot build");
        return;
    };
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("unwind_foreign");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");

    let lib = dir.join("libthrow.so");
    let out = Command::new(cxx)
        .args(["-O2", "-fPIC", "-shared", "-o"])
        .arg(&lib)
        .arg(fixture("thrower.cc"))
        .output()
        .expect("the C++ compiler runs");
    assert!(
        out.status.success(),
        "{cxx} -shared: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // The driver's own link line, with the thrower library added and the
    // line kept for the assertion below.
    let log = dir.join("cc-argv");
    let wrapper = dir.join("cc-with-thrower");
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    std::fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{log}'\nexec {cc} \"$@\" -L'{d}' -lthrow \
             -lstdc++ -Wl,-rpath,'{d}'\n",
            log = log.display(),
            d = dir.display()
        ),
    )
    .expect("write the wrapper");
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755))
        .expect("the wrapper is executable");

    let exe = dir.join("prog");
    let built = Command::new(wolf())
        .env("CC", &wrapper)
        .arg("build")
        .arg(fixture("prog.lu"))
        .arg("--no-cache")
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("wolf runs");
    match built.status.code() {
        Some(0) => {}
        Some(2) if lane_exit::environment_refusal(&built, "wolf build") => {
            eprintln!(
                "SKIP: environment cannot link native binaries: {}",
                String::from_utf8_lossy(&built.stderr).trim()
            );
            return;
        }
        other => panic!(
            "wolf build failed (exit {other:?}): {}",
            String::from_utf8_lossy(&built.stderr)
        ),
    }
    let argv = std::fs::read_to_string(&log).expect("the wrapper logged the driver's line");
    assert!(
        argv.contains("-static-libgcc"),
        "the driver's link line carries GCC's unwinder statically: {argv}"
    );

    let run = Command::new(&exe).output().expect("the program runs");
    let stderr = String::from_utf8_lossy(&run.stderr);
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert_eq!(
        run.status.signal(),
        Some(libc::SIGABRT),
        "a throw that meets a wolf frame ends the process by SIGABRT; status {:?}, stderr: \
         {stderr}",
        run.status
    );
    assert!(
        stderr.contains("terminate called after throwing an instance of 'int'"),
        "C++ terminates on the throw it could not unwind: {stderr}"
    );
    assert!(
        !stdout.contains("after"),
        "the program ran past the throwing call: {stdout}"
    );
}
