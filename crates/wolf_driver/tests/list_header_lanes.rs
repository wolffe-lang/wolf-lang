//! wolf-lang#1177 (part 1) — an empty list costs a 32-byte header.
//!
//! A `List`'s value is one pointer to a header in the ambient region. The
//! header was 40 bytes (`data`, `len`, `cap`, the element size, the region
//! it was born in), charged 48 by the native arena's 16-byte rounding. The
//! element size is the compiled code's to pass (it knows every list's
//! element type), so the header drops it: 32 bytes, charged 32. Every empty
//! list, every copy and every reserve pays 16 bytes less.
//!
//! The witness makes 1,000 of each and prints its regions' `region_bytes`.
//! The native and release tiers charge the native arena; the checked
//! machine keeps its own ledger, which the header never reached, so its
//! line is unchanged and asserted beside them.
//!
//! The renamed runtime entries keep an object built against one header
//! from linking against the other: an old object calls names the new
//! runtime does not define, and a new object reads only the header words
//! that did not move (`data`, `len`, `cap`) and reaches the one that did,
//! the region, through a renamed entry. The two link rows below, unix only.

mod lane_exit;

use std::path::{Path, PathBuf};
use std::process::Command;

fn wolf() -> &'static str {
    env!("CARGO_BIN_EXE_wolf")
}

const ISSUE: &str = "wolf-lang#1177";

/// One lane's stdout, or `None` where the host cannot run it (the s59
/// skip pattern; the checked lane always runs).
fn lane(entry: &Path, flag: &str) -> Option<String> {
    let out = Command::new(wolf())
        .arg("conform-run")
        .arg(entry)
        .arg(flag)
        .arg("--json")
        .output()
        .expect("wolf runs");
    if lane_exit::environment_refusal(&out, &format!("wolf {flag}")) && flag != "--checked" {
        eprintln!(
            "SKIP: environment cannot run the {flag} lane: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    assert!(
        out.status.success(),
        "conform-run {flag} failed on {}: {}",
        entry.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    let rec: serde_json::Value = serde_json::from_slice(&out.stdout).expect("the record parses");
    assert_eq!(
        rec["verdict"].as_str(),
        Some("exit(0)"),
        "{flag} ({ISSUE}): {rec}"
    );
    Some(
        rec["stdout_inline"]
            .as_str()
            .unwrap_or("")
            .trim_end()
            .to_string(),
    )
}

const WITNESS: &str = r#"
fn main() -> !int {
    let r = region()
    var keep = 0
    in r {
        var i = 0
        while i < 1000 {
            let a = List[int]()
            keep += a.len
            i += 1
        }
    }
    let e = region_bytes(r)
    let r2 = region()
    let src = List[int]()
    in r2 {
        var i = 0
        while i < 1000 {
            let c = copy src
            keep += c.len
            i += 1
        }
    }
    let ce = region_bytes(r2)
    var b16 = List[byte]()
    for k in 0..16 { (mut b16).push(k as byte) }
    let r3 = region()
    in r3 {
        var i = 0
        while i < 1000 {
            let c = copy b16
            keep += c.len
            i += 1
        }
    }
    let c16 = region_bytes(r3)
    var i20 = List[int]()
    for k in 0..20 { (mut i20).push(k) }
    let r4 = region()
    in r4 {
        var i = 0
        while i < 1000 {
            let c = copy i20
            keep += c.len
            i += 1
        }
    }
    let c20 = region_bytes(r4)
    let r5 = region()
    in r5 {
        var i = 0
        while i < 1000 {
            var a = List[byte]()
            (mut a).reserve(16)
            keep += a.len
            i += 1
        }
    }
    let rv = region_bytes(r5)
    print("empty_new={e} empty_copy={ce} copy16b={c16} copy20i={c20} reserve16={rv} keep={keep}")
    0
}
"#;

/// 1,000 of each: an empty `List[int]`, a copy of an empty list, a copy of
/// a 16-byte `List[byte]`, a copy of a 20-int list, and a `reserve(16)`.
const NATIVE: &str =
    "empty_new=32000 empty_copy=32000 copy16b=48000 copy20i=192000 reserve16=48000 keep=36000";
const CHECKED: &str =
    "empty_new=0 empty_copy=0 copy16b=16000 copy20i=320000 reserve16=0 keep=36000";

#[test]
fn an_empty_list_and_each_copy_and_reserve_charge_a_32_byte_header() {
    // Its own directory: the link rows run beside it in the same parent.
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("list_header_lanes")
        .join("charge");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let entry = dir.join("main.lu");
    std::fs::write(&entry, WITNESS).expect("write the witness");
    let checked = lane(&entry, "--checked").expect("the checked lane always runs");
    assert_eq!(
        checked, CHECKED,
        "--checked ({ISSUE}): its ledger never held the header"
    );
    for flag in ["--native", "--release"] {
        let Some(line) = lane(&entry, flag) else {
            continue;
        };
        assert_eq!(line, NATIVE, "{flag} ({ISSUE}): bytes per 1,000 operations");
    }
}

/// The runtime archive beside `wolf`, built once per test binary.
#[cfg(unix)]
fn rt_staticlib() -> PathBuf {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let status = Command::new(env!("CARGO"))
            .args(["build", "-p", "wolf_rt"])
            .status()
            .expect("cargo builds wolf_rt");
        assert!(status.success(), "wolf_rt staticlib build failed");
    });
    let rt = Path::new(wolf())
        .parent()
        .expect("wolf has a directory")
        .join("libwolf_rt.a");
    assert!(rt.is_file(), "libwolf_rt.a beside wolf: {}", rt.display());
    rt
}

#[cfg(unix)]
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("list_header_lanes")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

/// An object compiled against the 40-byte header calls the old entry
/// names; the new runtime does not define them, so it cannot link.
#[cfg(unix)]
#[test]
fn an_object_built_for_the_old_header_does_not_link() {
    let dir = scratch("old_object");
    let c = dir.join("old.c");
    std::fs::write(
        &c,
        "void __wolf_rt_list_push(long hdr, long elem);\n\
         int main(void) { __wolf_rt_list_push(0, 0); return 0; }\n",
    )
    .expect("write the C caller");
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let mut link = Command::new(&cc);
    link.arg("-o")
        .arg(dir.join("old"))
        .arg(&c)
        .arg(rt_staticlib());
    if cfg!(target_os = "macos") {
        link.args(["-lpthread", "-lm"]);
    } else {
        link.args(["-lpthread", "-ldl", "-lm"]);
    }
    let out = link.output().expect("the C driver runs");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "an object calling __wolf_rt_list_push linked against the new runtime ({ISSUE})"
    );
    assert!(
        stderr.contains("__wolf_rt_list_push"),
        "the link fails on the old entry's name, not something else: {stderr}"
    );
}

/// The other direction: an object whose code depends on the moved word
/// names an entry only the new runtime defines. The program copies a value
/// into an outer list from inside a region block, an aimed copy
/// (`[mem.region.outer-write]`), into a list the runtime made, so it calls
/// no list entry but the one that enters the list's region.
#[cfg(unix)]
#[test]
fn an_object_that_enters_a_lists_region_names_the_renamed_entry() {
    use object::{Object, ObjectSymbol};
    let dir = scratch("enter");
    let src = dir.join("main.lu");
    std::fs::write(
        &src,
        "fn main() -> int {\n    var xs = env_args()\n    let r = region()\n    in r {\n        let s = \"aimed\"\n        xs[0] = copy s\n    }\n    xs.len - 1\n}\n",
    )
    .expect("write the program");
    let obj = dir.join("prog.o");
    let out = Command::new(wolf())
        .arg("build")
        .arg(&src)
        .arg("--emit=obj")
        .arg("-o")
        .arg(&obj)
        .output()
        .expect("wolf runs");
    assert!(
        out.status.success(),
        "wolf build --emit=obj: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Every object the build wrote (a program may be written as more than
    // one), each read for the undefined symbols it names.
    let mut names = Vec::new();
    for e in std::fs::read_dir(&dir).expect("read the object dir") {
        let path = e.expect("a directory entry").path();
        if path.extension().is_none_or(|x| x != "o") {
            continue;
        }
        let bytes = std::fs::read(&path).expect("read the object");
        let file = object::File::parse(&*bytes).expect("parse the object");
        names.extend(
            file.symbols()
                .filter(|s| s.is_undefined())
                .filter_map(|s| s.name().ok().map(str::to_string)),
        );
    }
    assert!(
        !names.is_empty(),
        "no object or no undefined symbol under {}",
        dir.display()
    );
    let named = |want: &str| names.iter().any(|n| n.trim_start_matches('_') == want);
    assert!(
        named("wolf_rt_list32_enter"),
        "the aimed copy does not enter the list's region by a renamed entry ({ISSUE}): {names:?}"
    );
}
