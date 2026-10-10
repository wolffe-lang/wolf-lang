//! s221 (wolf-lang#619, ruling #55) — a C declaration of the package's
//! own charges the `ffi` capability, exactly as `import c` does.
//!
//! Each fixture under `tests/fixtures/pkg/ffi/<case>/` is a package that
//! declares no capability. Until s221 a bodyless `extern "c" fn getpid`
//! called inside `unsafe` built and ran in a `caps=[]` package, `wolf
//! audit` printed `effective: []` and `wolf audit --ci` exited 0, while
//! the same call through `import c` was E1504.
//!
//! The witnesses, red at trunk `ac0ac498` (every refused build here
//! built, every audit exited 0) and green here:
//! - a `caps=[]` root calling `getpid` (#619's program), refused E1504 on
//!   both compiling tiers, naming the declaration and its first call;
//! - a `caps=[]` dependency doing the same, charged to the dependency;
//! - an `extern "c" let` (a link-time symbol, `[abi.link.extern]`);
//! - a declaration nothing calls, still charged, as an unused `import c`;
//! - pax's shape: a freestanding kernel package (an `asm` roster,
//!   standalone `kmain_*.lu` entries built for `x86_64-unknown-none`),
//!   whose C declarations sit in a module the entries use, refused per
//!   entry and in the audit;
//! - and the honest cases: `ffi` declared builds (hosted, and the kernel's
//!   objects), `import c` reports exactly what it reported at trunk, and
//!   wolf code C calls (`extern "c" fn` with a body, `export fn`) charges
//!   nothing.
//!
//! Every refusal is a front-end stop (before codegen, before the
//! assembler), so the refusal rows run on every host. The honest hosted
//! rows build: a build that stops with any `error[` fails the test, and
//! only a diagnostic-free exit 2 (no linker) skips, loudly — exit 2 alone
//! is never a skip (wolf-lang#550). The kernel's objects assemble its
//! roster, so that row is unix-only and never skips (asm_link.rs's rule).

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

/// Copy one ffi fixture into a fresh tmp dir named for the test (`tag`),
/// so two tests staging one fixture never share a directory; the
/// fixtures stay pristine.
fn stage(case: &str, tag: &str) -> PathBuf {
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pkg/ffi")
        .join(case);
    let dest = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("cap_ffi_{tag}"));
    let _ = std::fs::remove_dir_all(&dest);
    copy_tree(&src, &dest);
    dest
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("mkdir");
    for entry in std::fs::read_dir(from).expect("read fixture dir").flatten() {
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            copy_tree(&src, &dst);
        } else {
            std::fs::copy(&src, &dst).expect("copy fixture file");
        }
    }
}

/// Give a staged manifest `capabilities: [ffi]`, its closing brace kept.
fn declare_ffi(manifest: &Path) {
    let text = std::fs::read_to_string(manifest).expect("read manifest");
    let at = text.rfind('}').expect("a manifest closes");
    let declared = format!("{}    capabilities: [ffi]\n}}\n", &text[..at]);
    std::fs::write(manifest, declared).expect("write manifest");
}

/// Run `wolf` in `dir` with WOLF_STD scrubbed (no fixture needs std).
fn run_wolf(dir: &Path, args: &[&str]) -> Output {
    Command::new(wolf())
        .args(args)
        .current_dir(dir)
        .env_remove("WOLF_STD")
        .output()
        .expect("wolf runs")
}

/// The rendered text with its line wrapping undone: a note wraps at the
/// terminal width, so a phrase is matched on the flattened text.
fn flat(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn exe(dir: &Path) -> PathBuf {
    dir.join(if cfg!(windows) { "app.exe" } else { "app.out" })
}

/// `wolf build app/main.lu`, on the default tier or `--release`.
fn build(dir: &Path, release: bool) -> Output {
    let out = exe(dir);
    let mut args = vec!["build", "app/main.lu", "-o", out.to_str().unwrap()];
    if release {
        args.insert(1, "--release");
    }
    run_wolf(dir, &args)
}

/// A front-end refusal: exit 2 and an E1504 headline containing `head`.
fn assert_refused(out: &Output, head: &str) -> String {
    let err = stderr(out);
    assert_eq!(out.status.code(), Some(2), "stderr:\n{err}");
    assert!(
        err.lines()
            .any(|l| l.starts_with("error[E1504]:") && l.contains(head)),
        "expected an E1504 headline containing {head:?}:\n{err}"
    );
    err
}

/// An honest build: no diagnostic error of any kind. Exit 0, or a
/// diagnostic-free environment refusal (no linker), skipped loudly.
/// `true` when it built.
fn assert_builds(out: &Output, what: &str) -> bool {
    let err = stderr(out);
    assert!(
        !err.lines().any(|l| l.starts_with("error[")),
        "{what} must build with no error:\n{err}"
    );
    if out.status.code() == Some(0) {
        return true;
    }
    if lane_exit::environment_refusal(out, what) {
        eprintln!("SKIP {what}: {}", err.trim());
        return false;
    }
    panic!("{what}: exit {:?}\n{err}", out.status.code());
}

fn audit(dir: &Path, pkg: &str, ci: bool) -> Output {
    let mut args = vec!["audit", "--dir", pkg];
    if ci {
        args.insert(1, "--ci");
    }
    run_wolf(dir, &args)
}

/// The audit derives `ffi` from the code with no build and no ledger,
/// gives `reason` for `who` (its report line ending in `more`, the count
/// of further sites), and `--ci` refuses; without `--ci` the same report
/// exits 0.
fn assert_audit_undeclared(dir: &Path, pkg: &str, who: &str, reason: &str, more: &str) {
    let out = audit(dir, pkg, true);
    assert_eq!(out.status.code(), Some(1), "stderr:\n{}", stderr(&out));
    let report = stdout(&out);
    assert!(report.contains("effective: [ffi]\n"), "{report}");
    assert!(
        report.contains(&format!("  ffi: {who} — UNDECLARED: {reason}{more}\n")),
        "expected the reason {reason:?}:\n{report}"
    );
    let short = who.trim_end_matches(" (root)");
    assert!(
        report.contains(&format!(
            "wolf audit: `{short}` reaches capability `ffi` without declaring it ({reason})"
        )),
        "{report}"
    );
    assert!(
        stderr(&out).contains("undeclared capability use — refusing (--ci)"),
        "{}",
        stderr(&out)
    );
    assert!(
        !dir.join(pkg).join("wolf.sum").exists(),
        "the audit wrote a ledger"
    );
    let out = audit(dir, pkg, false);
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    assert_eq!(stdout(&out), report);
}

#[test]
fn issue_619_a_caps_free_package_calling_getpid_is_e1504_on_both_tiers() {
    let dir = stage("root", "root");
    assert_audit_undeclared(
        &dir,
        "app",
        "demo/app (root)",
        "declares `extern \"c\" fn getpid` at app/main.lu:4:15, \
         first called at app/main.lu:7:22",
        "",
    );
    for release in [false, true] {
        let err = assert_refused(
            &build(&dir, release),
            "this package declares `extern \"c\" fn getpid` but does not declare the `ffi` \
             capability",
        );
        assert!(
            err.contains("`getpid` is declared here: it reaches the `ffi` capability"),
            "the declaration is shown:\n{err}"
        );
        assert!(
            err.contains("first called here"),
            "the call is shown:\n{err}"
        );
        assert!(
            flat(&err).contains(
                "declares the C function `getpid` at app/main.lu:4:15 (first called at \
                 app/main.lu:7:22)"
            ),
            "{err}"
        );
        assert!(flat(&err).contains("ruling #55"), "{err}");
        assert!(!exe(&dir).exists(), "a refused build wrote a program");
    }
}

#[test]
fn a_caps_free_dependency_declaring_a_c_function_is_charged_for_it() {
    let dir = stage("dep", "dep");
    assert_audit_undeclared(
        &dir,
        "app",
        "pad",
        "declares `extern \"c\" fn getpid` at pkg://pad/pad.lu:3:15, \
         first called at pkg://pad/pad.lu:6:22",
        "",
    );
    let err = assert_refused(
        &build(&dir, false),
        "dependency `pad` declares `extern \"c\" fn getpid` but does not declare the `ffi` \
         capability",
    );
    assert!(err.contains(" ::: pkg://pad/pad.lu:3:15"), "{err}");
    assert!(err.contains("first called here"), "{err}");
    // The root declares nothing and reaches nothing itself.
    assert!(
        !err.contains("this package declares"),
        "the root was charged for its dependency's declaration:\n{err}"
    );
}

#[test]
fn an_extern_c_let_is_charged_as_a_c_declaration() {
    let dir = stage("symbol", "symbol");
    assert_audit_undeclared(
        &dir,
        "app",
        "demo/app (root)",
        "declares `extern \"c\" let environ` at app/main.lu:3:16, \
         first used at app/main.lu:6:26",
        "",
    );
    let err = assert_refused(
        &build(&dir, false),
        "this package declares `extern \"c\" let environ` but does not declare the `ffi` \
         capability",
    );
    assert!(
        flat(&err).contains("the link-time symbol `environ`"),
        "{err}"
    );
    assert!(err.contains("first used here"), "{err}");
}

#[test]
fn a_c_declaration_nothing_calls_is_still_charged() {
    let dir = stage("unused", "unused");
    assert_audit_undeclared(
        &dir,
        "app",
        "demo/app (root)",
        "declares `extern \"c\" fn getpid` at app/main.lu:3:15, never called in this package",
        "",
    );
    let err = assert_refused(
        &build(&dir, false),
        "this package declares `extern \"c\" fn getpid`",
    );
    assert!(
        flat(&err).contains("(never called in this package)"),
        "{err}"
    );
    assert!(!err.contains("first called here"), "{err}");
}

#[test]
fn declaring_ffi_builds_and_the_audit_gives_the_reason() {
    let dir = stage("root", "root_declared");
    declare_ffi(&dir.join("app/wolf.pkg"));
    let out = audit(&dir, "app", true);
    assert_eq!(out.status.code(), Some(0), "stderr:\n{}", stderr(&out));
    let report = stdout(&out);
    assert!(
        report.contains("demo/app 0.1.0 (root) caps=[ffi]\n"),
        "{report}"
    );
    assert!(report.contains("effective: [ffi]\n"), "{report}");
    assert!(
        report.contains(
            "  ffi: demo/app (root) — declared; declares `extern \"c\" fn getpid` at \
             app/main.lu:4:15, first called at app/main.lu:7:22\n"
        ),
        "{report}"
    );
    // getpid is POSIX: the program links and runs where the C library
    // has it.
    if cfg!(unix) {
        for release in [false, true] {
            let what = if release { "root --release" } else { "root" };
            if assert_builds(&build(&dir, release), what) {
                let ran = Command::new(exe(&dir)).output().expect("the program runs");
                assert_eq!(stdout(&ran).trim_end(), "pid true", "{what}");
            }
        }
    }
}

#[test]
fn import_c_is_charged_exactly_as_before() {
    // The control: the headline, the reason and the audit line are
    // trunk's, byte for byte.
    let dir = stage("importc", "importc");
    let out = audit(&dir, "app", true);
    assert_eq!(out.status.code(), Some(1), "stderr:\n{}", stderr(&out));
    assert!(
        stdout(&out).contains(
            "  ffi: demo/app (root) — UNDECLARED: imports `import c` at app/main.lu:2:1\n"
        ),
        "{}",
        stdout(&out)
    );
    let err = assert_refused(
        &build(&dir, false),
        "this package uses `import c` but does not declare the `ffi` capability",
    );
    assert!(
        flat(&err).contains("the root module imports `import c`. Add `ffi`"),
        "{err}"
    );
    // Declared, it builds where `cc` links the C library's malloc.
    declare_ffi(&dir.join("app/wolf.pkg"));
    if cfg!(unix) {
        assert_builds(&build(&dir, false), "importc declared");
    } else {
        let out = audit(&dir, "app", true);
        assert_eq!(out.status.code(), Some(0), "stdout:\n{}", stdout(&out));
    }
}

#[test]
fn wolf_code_c_calls_charges_nothing() {
    let dir = stage("body", "body");
    let out = audit(&dir, "app", true);
    assert_eq!(out.status.code(), Some(0), "stdout:\n{}", stdout(&out));
    assert!(stdout(&out).contains("effective: []\n"), "{}", stdout(&out));
    if assert_builds(&build(&dir, false), "body") {
        let ran = Command::new(exe(&dir)).output().expect("the program runs");
        assert_eq!(stdout(&ran).trim_end(), "42 42");
    }
}

/// Build one kernel entry for the freestanding target, as pax's
/// tools/build-kernel does. A build of several modules writes one
/// object per module beside `-o K.o` (`K.root.o`, `K.port.o`) and the
/// assembled roster as `K.asm-io.o` ([abi.asm.link]); the entry's own
/// module's object is returned.
fn build_entry(dir: &Path, entry: &str) -> (Output, PathBuf) {
    let obj = dir.join(format!("{entry}.o"));
    let root = dir.join(format!("{entry}.root.o"));
    let out = run_wolf(
        dir,
        &[
            "build",
            &format!("kern/{entry}.lu"),
            "--target",
            "x86_64-unknown-none",
            "--emit=obj",
            "-o",
            obj.to_str().unwrap(),
        ],
    );
    (out, root)
}

#[test]
fn a_freestanding_kernel_is_charged_per_entry_and_in_the_audit() {
    // pax's shape: the C declaration lives in `port`, which the entries
    // use; `kmain_b` adds a link-time symbol of its own.
    let dir = stage("kernel", "kernel");
    assert_audit_undeclared(
        &dir,
        "kern",
        "demo/kern (root)",
        "declares `extern \"c\" fn k_outb` at kern/port/port.lu:4:15, \
         first called at kern/port/port.lu:9:14",
        " (+1 more)",
    );
    let (out, obj) = build_entry(&dir, "kmain_a");
    let err = assert_refused(
        &out,
        "this package declares `extern \"c\" fn k_outb` but does not declare the `ffi` capability",
    );
    assert!(!obj.exists(), "a refused build wrote an object:\n{err}");
    let (out, _) = build_entry(&dir, "kmain_b");
    let err = assert_refused(&out, "but does not declare the `ffi` capability");
    assert!(
        flat(&err).contains("1 more site in the same package reaches `ffi`"),
        "kmain_b reaches ffi twice (its own symbol and port's function):\n{err}"
    );
}

/// The declared kernel assembles its roster for the target: unix only,
/// as asm_link.rs's assembling rows, and never a skip.
#[cfg(unix)]
#[test]
fn a_freestanding_kernel_that_declares_ffi_builds_its_objects() {
    let dir = stage("kernel", "kernel_declared");
    declare_ffi(&dir.join("kern/wolf.pkg"));
    let out = audit(&dir, "kern", true);
    assert_eq!(out.status.code(), Some(0), "stdout:\n{}", stdout(&out));
    assert!(
        stdout(&out).contains("effective: [ffi]\n"),
        "{}",
        stdout(&out)
    );
    for entry in ["kmain_a", "kmain_b"] {
        let (out, obj) = build_entry(&dir, entry);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{entry} must build:\n{}",
            stderr(&out)
        );
        assert!(obj.is_file(), "{entry}: no object at {}", obj.display());
        let asm = dir.join(format!("{entry}.asm-io.o"));
        assert!(asm.is_file(), "{entry}: the roster was not assembled");
    }
}
