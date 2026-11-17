//! COFF keeps its existing native behavior; it must not silently claim the
//! ELF/Mach-O layout guarantee or accept an unsupported cold request.
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
        String::from_utf8_lossy(&built.stderr).contains("layout guarantee does not apply to COFF")
    );
    let run = Command::new(&exe).output().unwrap();
    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "17");
    let rejected = root.join("cold.exe");
    let built = Command::new(env!("CARGO_BIN_EXE_wolf"))
        .arg("build")
        .arg(&source)
        .arg("-o")
        .arg(&rejected)
        .args(["--cold-function", "helper"])
        .output()
        .unwrap();
    assert!(!built.status.success());
    assert!(
        String::from_utf8_lossy(&built.stderr)
            .contains("cold function placement on COFF is not supported")
    );
    assert!(!rejected.exists());
}
