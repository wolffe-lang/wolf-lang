//! COFF keeps its existing native behavior; it must not claim the ELF/Mach-O
//! layout guarantee or accept an unsupported cold request. An ordinary build
//! stays silent on stderr; `--verbose` notes the missing guarantee, and a cold
//! request is refused by name whatever the verbosity.
#![cfg(windows)]

use std::path::Path;
use std::process::Command;

#[test]
fn ordinary_coff_still_runs_and_cold_requests_are_refused_by_name() {
    let status = Command::new(env!("CARGO"))
        .args(["build", "-p", "wolf_rt"])
        .status()
        .unwrap();
    assert!(status.success());
    let root =
        Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("coff-layout-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("main.lu");
    std::fs::write(
        &source,
        "fn helper() -> int { 17 }\nfn main() { let n = helper(); print(\"{n}\") }\n",
    )
    .unwrap();
    let exe = root.join("ordinary.exe");
    let built = Command::new(env!("CARGO_BIN_EXE_wolf"))
        .arg("build")
        .arg(&source)
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    assert!(
        !String::from_utf8_lossy(&built.stderr).contains("layout guarantee"),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let run = Command::new(&exe).output().unwrap();
    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "17");
    // The object path runs through the same placement decision: silent.
    let object = Command::new(env!("CARGO_BIN_EXE_wolf"))
        .arg("build")
        .arg(&source)
        .arg("--emit=obj")
        .arg("-o")
        .arg(root.join("ordinary.obj"))
        .output()
        .unwrap();
    assert!(object.status.success());
    assert_eq!(
        String::from_utf8_lossy(&object.stderr),
        "",
        "an ordinary build is silent"
    );
    let verbose = Command::new(env!("CARGO_BIN_EXE_wolf"))
        .arg("build")
        .arg(&source)
        .arg("-o")
        .arg(root.join("verbose.exe"))
        .arg("--verbose")
        .output()
        .unwrap();
    assert!(verbose.status.success());
    assert!(
        String::from_utf8_lossy(&verbose.stderr)
            .contains("wolf build: note: the stable layout guarantee does not apply to COFF"),
        "{}",
        String::from_utf8_lossy(&verbose.stderr)
    );
    for verbose in [false, true] {
        let rejected = root.join("cold.exe");
        let mut command = Command::new(env!("CARGO_BIN_EXE_wolf"));
        command
            .arg("build")
            .arg(&source)
            .arg("-o")
            .arg(&rejected)
            .args(["--cold-function", "helper"]);
        if verbose {
            command.arg("--verbose");
        }
        let built = command.output().unwrap();
        assert!(!built.status.success());
        assert!(
            String::from_utf8_lossy(&built.stderr)
                .contains("cold function placement on COFF is not supported"),
            "verbose={verbose}: {}",
            String::from_utf8_lossy(&built.stderr)
        );
        assert!(!rejected.exists());
    }
}
