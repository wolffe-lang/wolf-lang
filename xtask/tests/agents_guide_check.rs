//! wolf-lang#1147: `docs/agents-guide-check.sh` must pass with the release
//! archives it is documented to run against, in each of the three ways it is
//! configured: with no std variable, with `WOLF_STD`, and with `LUPIN_STD`.
//!
//! The checker is run for real against a one-fence guide with stand-in
//! machines that mimic what the released ones do about the std: the stand-in
//! `wolf` accepts `--std-root` only for a directory named `std` (a parent
//! fails with E0403, as the real one does), and the stand-in `lupin` answers
//! `unsupported` when it has neither `--std-root` nor `LUPIN_STD`. Each
//! stand-in records the arguments it was given, so a failing row names the
//! argument that was wrong.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const GUIDE: &str = "# guide\n\n```wolf\nfn main() {}\n```\n";

const WOLF: &str = r#"#!/bin/sh
echo "$*" >> "$STUB_LOG/wolf.args"
case "$1" in
--version) echo "wolf 0.0.0 (stand-in)"; exit 0 ;;
fmt) exit 0 ;;
build)
    std=""
    while [ $# -gt 0 ]; do
        [ "$1" = "--std-root" ] && std="$2"
        shift
    done
    if [ -n "$std" ] && [ "$(basename "$std")" != std ]; then
        echo "error[E0403]: $std is not a std directory"
        exit 1
    fi
    printf '#!/bin/sh\nexit 0\n' > prog
    chmod +x prog
    exit 0 ;;
esac
exit 2
"#;

const LUPIN: &str = r#"#!/bin/sh
echo "$*" >> "$STUB_LOG/lupin.args"
case "$1" in
--version) echo "lupin 0.0.0 (stand-in)"; exit 0 ;;
esac
std=""
while [ $# -gt 0 ]; do
    [ "$1" = "--std-root" ] && std="$2"
    shift
done
if [ -n "$std" ] && [ "$(basename "$std")" != std ]; then
    echo '{"verdict":"fail(E0403)"}'
elif [ -z "$std" ] && [ -z "${LUPIN_STD:-}" ]; then
    echo '{"verdict":"unsupported"}'
else
    echo '{"verdict":"exit(0)"}'
fi
"#;

fn script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/agents-guide-check.sh")
}

fn write_exec(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// A scratch archive: `wolf` with `std/` beside it, and `lupin` with no std.
struct Rig {
    root: PathBuf,
}

impl Rig {
    fn new(case: &str) -> Rig {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("agents_guide_check")
            .join(case);
        let _ = fs::remove_dir_all(&root);
        for d in ["wolf-archive/std", "lupin-archive", "elsewhere/std", "log"] {
            fs::create_dir_all(root.join(d)).unwrap();
        }
        write_exec(&root.join("wolf-archive/wolf"), WOLF);
        write_exec(&root.join("lupin-archive/lupin"), LUPIN);
        fs::write(root.join("guide.md"), GUIDE).unwrap();
        Rig { root }
    }

    fn run(&self, env: &[(&str, PathBuf)]) -> Output {
        let mut c = Command::new("sh");
        c.arg(script())
            .arg(self.root.join("guide.md"))
            .env_remove("WOLF_STD")
            .env_remove("LUPIN_STD")
            .env("WOLF", self.root.join("wolf-archive/wolf"))
            .env("LUPIN", self.root.join("lupin-archive/lupin"))
            .env("STUB_LOG", self.root.join("log"));
        for (k, v) in env {
            c.env(k, v);
        }
        c.output().expect("sh runs")
    }

    fn args(&self, which: &str) -> String {
        fs::read_to_string(self.root.join("log").join(which)).unwrap_or_default()
    }
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

fn passes(o: &Output) {
    assert!(o.status.success(), "the checker failed:\n{}", text(o));
    assert!(text(o).contains("1 passed, 0 failed"), "{}", text(o));
}

#[test]
fn no_std_variable_gives_lupin_the_std_beside_wolf() {
    let r = Rig::new("defaults");
    let o = r.run(&[]);
    let want = format!("--std-root {}", r.root.join("wolf-archive/std").display());
    assert!(
        r.args("lupin.args").contains(&want),
        "lupin std-root argument: expected {want}, got:\n{}",
        r.args("lupin.args")
    );
    assert!(
        !r.args("wolf.args").contains("--std-root"),
        "wolf takes its bundled std:\n{}",
        r.args("wolf.args")
    );
    passes(&o);
}

#[test]
fn wolf_std_names_the_std_directory_and_goes_to_both_machines() {
    let r = Rig::new("wolf_std");
    let std = r.root.join("elsewhere/std");
    let o = r.run(&[("WOLF_STD", std.clone())]);
    let want = format!("--std-root {}", std.display());
    assert!(
        r.args("wolf.args").contains(&want),
        "wolf std-root argument: expected {want}, got:\n{}",
        r.args("wolf.args")
    );
    assert!(
        r.args("lupin.args").contains(&want),
        "lupin std-root argument: expected {want}, got:\n{}",
        r.args("lupin.args")
    );
    passes(&o);
}

#[test]
fn lupin_std_alone_is_enough_and_is_not_overridden() {
    let r = Rig::new("lupin_std");
    let o = r.run(&[("LUPIN_STD", r.root.join("elsewhere/std"))]);
    passes(&o);
    assert!(
        !r.args("lupin.args").contains("--std-root"),
        "lupin reads LUPIN_STD itself:\n{}",
        r.args("lupin.args")
    );
}

#[test]
fn no_std_anywhere_is_a_setup_error_naming_both_variables() {
    let r = Rig::new("no_std");
    fs::remove_dir_all(r.root.join("wolf-archive/std")).unwrap();
    let o = r.run(&[]);
    assert_eq!(o.status.code(), Some(2), "{}", text(&o));
    let t = text(&o);
    assert!(
        t.contains("WOLF_STD") && t.contains("LUPIN_STD"),
        "the message names both variables:\n{t}"
    );
}
