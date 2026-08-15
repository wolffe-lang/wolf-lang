//! The declarative C recipe in `wolf.pkg` (s46, c10).
//!
//! This is where D33 meets C interop. Every other ecosystem answers
//! "where is this library's header?" by running something; wolf answers
//! it by reading a declaration. These tests pin that the declaration is
//! expressive enough to be useful, and that every way of smuggling an
//! execution back in is refused **at the manifest** rather than on a
//! consumer's machine.

use std::path::Path;

use wolf_pkg::manifest::{self, Manifest};

fn parse(text: &str) -> (Option<Manifest>, Vec<String>) {
    let mut sm = wolf_span::SourceMap::new();
    let file = sm.intern(Path::new("wolf.pkg"));
    let (m, diags) = manifest::parse(file, text);
    let msgs = diags.iter().map(|d| d.message.clone()).collect();
    (m, msgs)
}

const HEAD: &str = "pkg {\n    name: \"acme/app\",\n    version: \"0.1.0\",\n";

#[test]
fn a_full_recipe_parses() {
    let (m, diags) = parse(&format!(
        "{HEAD}    capabilities: [ffi],
    c: {{
        zstd: {{
            headers: [\"zstd.h\"],
            include: [\"vendor/zstd/lib\"],
            define: {{ ZSTD_STATIC_LINKING_ONLY: \"1\", ZSTD_LEGACY: 0 }},
            cflags: [\"-std=c11\"],
            link: [\"zstd\"],
            sysroot: system,
        }},
        libc: {{
            headers: [\"stdlib.h\", \"string.h\"],
        }},
    }},
}}\n"
    ));
    assert!(diags.is_empty(), "{diags:?}");
    let m = m.expect("parses");
    assert_eq!(m.c.len(), 2);

    let libc = m.c.iter().find(|r| r.name == "libc").expect("libc recipe");
    assert_eq!(libc.headers, vec!["stdlib.h", "string.h"]);
    assert_eq!(
        libc.sysroot,
        wolf_cimport::recipe::Sysroot::Bundled,
        "the default is the bundled per-target headers, which is what \
         cross-compilation needs"
    );

    let zstd = m.c.iter().find(|r| r.name == "zstd").expect("zstd recipe");
    assert_eq!(zstd.include, vec!["vendor/zstd/lib"]);
    assert_eq!(zstd.link, vec!["zstd"]);
    assert_eq!(zstd.sysroot, wolf_cimport::recipe::Sysroot::System);
    assert_eq!(
        zstd.define
            .get("ZSTD_STATIC_LINKING_ONLY")
            .map(String::as_str),
        Some("1")
    );
    assert_eq!(
        zstd.define.get("ZSTD_LEGACY").map(String::as_str),
        Some("0"),
        "an integer define is written out as its text"
    );
}

/// D33's whole point, at the C seam: a recipe cannot ask for a program
/// to run, however it is spelled.
#[test]
fn recipe_keys_that_run_programs_are_refused() {
    for key in [
        "pkg_config",
        "command",
        "exec",
        "configure",
        "shell",
        "probe",
    ] {
        let (m, diags) = parse(&format!(
            "{HEAD}    c: {{ lib: {{ headers: [\"x.h\"], {key}: \"anything\" }} }},\n}}\n"
        ));
        assert!(m.is_none(), "`{key}` should refuse the manifest");
        assert!(
            diags.iter().any(|d| d.contains("no build scripts, ever")),
            "`{key}`: {diags:?}"
        );
    }
}

/// The manifest-wide D33 gate still applies inside a recipe.
#[test]
fn a_build_hook_inside_a_recipe_is_still_refused() {
    let (m, diags) = parse(&format!(
        "{HEAD}    c: {{ lib: {{ headers: [\"x.h\"], build: \"./configure\" }} }},\n}}\n"
    ));
    assert!(m.is_none());
    assert!(diags.iter().any(|d| d.contains("build time")), "{diags:?}");
}

/// An include path only its author has would make the build
/// unreproducible on any other machine, so it is refused where it is
/// written rather than where it breaks.
#[test]
fn machine_specific_include_paths_are_refused() {
    for bad in ["/usr/include", "../../elsewhere", "$HOME/inc"] {
        let (m, diags) = parse(&format!(
            "{HEAD}    c: {{ lib: {{ headers: [\"x.h\"], include: [\"{bad}\"] }} }},\n}}\n"
        ));
        assert!(m.is_none(), "`{bad}` should refuse the manifest");
        assert!(
            diags.iter().any(|d| d.contains("include")),
            "`{bad}`: {diags:?}"
        );
    }
}

/// `-I` and `-D` through `cflags` would dodge the checks that make the
/// other two safe.
#[test]
fn include_and_define_cannot_be_smuggled_through_cflags() {
    let (m, diags) = parse(&format!(
        "{HEAD}    c: {{ lib: {{ headers: [\"x.h\"], cflags: [\"-I/usr/include\"] }} }},\n}}\n"
    ));
    assert!(m.is_none());
    assert!(
        diags.iter().any(|d| d.contains("`include` and `define`")),
        "{diags:?}"
    );
}

/// A sysroot is a named choice between two things, not a path to go
/// searching.
#[test]
fn sysroot_is_a_named_choice() {
    let (m, diags) = parse(&format!(
        "{HEAD}    c: {{ lib: {{ headers: [\"x.h\"], sysroot: \"/opt/cross\" }} }},\n}}\n"
    ));
    assert!(m.is_none());
    assert!(
        diags
            .iter()
            .any(|d| d.contains("bundled") && d.contains("not a path")),
        "{diags:?}"
    );
}

#[test]
fn an_unknown_recipe_field_is_refused() {
    let (m, diags) = parse(&format!(
        "{HEAD}    c: {{ lib: {{ headers: [\"x.h\"], libdirs: [\"lib\"] }} }},\n}}\n"
    ));
    assert!(m.is_none());
    assert!(
        diags.iter().any(|d| d.contains("unknown C recipe field")),
        "{diags:?}"
    );
}

#[test]
fn a_recipe_with_no_headers_is_refused() {
    let (m, diags) = parse(&format!(
        "{HEAD}    c: {{ lib: {{ link: [\"z\"] }} }},\n}}\n"
    ));
    assert!(m.is_none());
    assert!(diags.iter().any(|d| d.contains("headers")), "{diags:?}");
}

/// `import c` carries the `ffi` capability. Imported C is the one
/// import that leaves wolf's world entirely, so opening one is a
/// declared, diffable act.
#[test]
fn import_c_carries_the_ffi_capability() {
    use wolf_pkg::audit::{C_IMPORT_TARGET, import_cap};
    use wolf_pkg::manifest::Cap;

    assert_eq!(import_cap(C_IMPORT_TARGET), Some(Cap::Ffi));
    // The std facades keep their own capabilities.
    assert_eq!(import_cap("std.net.tcp"), Some(Cap::Net));
    assert_eq!(import_cap("std.fs"), Some(Cap::Fs));
    assert_eq!(import_cap("std.env"), Some(Cap::Env));
    // And an ordinary module carries nothing.
    assert_eq!(import_cap("std.list"), None);
    assert_eq!(import_cap("app.util"), None);
}
