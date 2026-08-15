//! The declarative C recipe — how headers are located, without running
//! anything.
//!
//! This is D33's load-bearing piece. Every other language's answer to
//! "where is `stdlib.h`?" is *run something and ask*: a configure
//! script, `pkg-config`, a `build.rs` that shells out. Wolf has no
//! build scripts, ever, so the answer has to be **declared** in
//! `wolf.pkg` and resolved by the compiler:
//!
//! ```text
//! pkg {
//!     name: "acme/app",
//!     capabilities: [ffi],
//!
//!     c: {
//!         libc: {
//!             headers: ["stdlib.h", "string.h"],
//!             sysroot: bundled,
//!         },
//!         zstd: {
//!             headers: ["zstd.h"],
//!             include: ["vendor/zstd/lib"],
//!             define:  { ZSTD_STATIC_LINKING_ONLY: "1" },
//!             cflags:  ["-std=c11"],
//!             link:    ["zstd"],
//!         },
//!     },
//! }
//! ```
//!
//! Three rules make this safe to resolve on someone else's machine, and
//! all three are enforced here rather than trusted:
//!
//! 1. **Include paths are package-relative.** An absolute path, or one
//!    that climbs out of the package with `..`, is refused: it would
//!    make the build depend on a layout only its author has, which is
//!    the failure mode declarative manifests exist to prevent.
//! 2. **Nothing in a recipe names a program.** `pkg-config`, response
//!    files (`@args`), compiler plugins — anything whose effect is
//!    "and then run this" — is refused by name.
//! 3. **The sysroot is a choice between two named things**, not a
//!    path to probe: the bundled per-target headers (s47, which is
//!    also what makes cross-compilation work) or the host's system
//!    headers, said out loud.
//!
//! What a recipe does *not* do is discover anything. If a library is
//! not where the manifest says it is, the build fails with a message
//! naming the recipe entry — it does not go looking.

use std::collections::BTreeMap;

use crate::cache::ImportRequest;

/// Where the C headers for a recipe come from.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Sysroot {
    /// The per-target header bundle the toolchain ships (s47). The
    /// default, because it is the one that behaves the same on every
    /// machine and cross-compiles.
    #[default]
    Bundled,
    /// The host's own system headers. Reproducible only as far as the
    /// host is, and said out loud for exactly that reason.
    System,
}

impl Sysroot {
    pub fn tag(self) -> &'static str {
        match self {
            Sysroot::Bundled => "bundled",
            Sysroot::System => "system",
        }
    }

    pub fn parse(s: &str) -> Option<Sysroot> {
        match s {
            "bundled" => Some(Sysroot::Bundled),
            "system" => Some(Sysroot::System),
            _ => None,
        }
    }
}

/// One named C dependency's recipe.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Recipe {
    /// The recipe's name in the manifest (`libc`, `zstd`).
    pub name: String,
    /// Headers to `#include`, in order — C's include order is
    /// semantics, not a set.
    pub headers: Vec<String>,
    /// Package-relative include directories, in order.
    pub include: Vec<String>,
    /// `-D` defines.
    pub define: BTreeMap<String, String>,
    /// Additional cflags, in order.
    pub cflags: Vec<String>,
    /// Libraries to link (`-lzstd`), without the `lib` prefix.
    pub link: Vec<String>,
    pub sysroot: Sysroot,
}

/// A recipe field that asks for something to happen, refused by name.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RecipeError {
    /// Which recipe.
    pub recipe: String,
    /// Which field.
    pub field: &'static str,
    /// The offending value.
    pub value: String,
    pub why: String,
    pub note: String,
}

/// Keys inside a `c: { }` recipe that ask for a program to run. Refused
/// unconditionally, in the same spirit as the manifest's global
/// build-hook refusal (E1503) — a recipe is data.
pub const FORBIDDEN_RECIPE_KEYS: &[&str] = &[
    "pkg_config",
    "pkgconfig",
    "pkg-config",
    "command",
    "exec",
    "run",
    "probe",
    "discover",
    "configure",
    "shell",
];

/// Flag shapes whose effect is "and then run this".
fn forbidden_cflag(flag: &str) -> Option<&'static str> {
    if flag.starts_with('@') {
        // A response file: the flags are somewhere else, so the
        // manifest no longer says what the build does.
        return Some("a response file moves the real flags out of the manifest");
    }
    // Prefix match, not equality: these take their argument glued on
    // (`-B/opt/tools`), separated by `=`, or as the next word, and all
    // three spellings mean the same thing.
    for bad in [
        "-fplugin",
        "-Xclang",
        "-B",
        "-specs",
        "--sysroot",
        "-isysroot",
    ] {
        if flag.starts_with(bad) {
            return Some("it loads or redirects a toolchain component");
        }
    }
    if flag.starts_with("-I") || flag.starts_with("-D") {
        return Some("use the `include` and `define` fields, which are checked");
    }
    None
}

impl Recipe {
    /// Check a recipe. Every violation is reported, not just the first:
    /// a manifest author fixing one path at a time is a bad afternoon.
    pub fn check(&self) -> Vec<RecipeError> {
        let mut errs = Vec::new();
        let err = |field: &'static str, value: &str, why: &str, note: &str| RecipeError {
            recipe: self.name.clone(),
            field,
            value: value.to_string(),
            why: why.to_string(),
            note: note.to_string(),
        };

        if self.headers.is_empty() {
            errs.push(err(
                "headers",
                "",
                "this C recipe names no headers",
                "a recipe exists to be imported from; add `headers: [\"…\"]`.",
            ));
        }

        for inc in &self.include {
            if let Some(why) = bad_include(inc) {
                errs.push(err(
                    "include",
                    inc,
                    why,
                    "include paths are relative to the package root, so the build \
                     means the same thing on a machine that is not yours.",
                ));
            }
        }

        for flag in &self.cflags {
            if let Some(why) = forbidden_cflag(flag) {
                errs.push(err(
                    "cflags",
                    flag,
                    why,
                    "a recipe is data: it describes a translation unit, it does not \
                     arrange for anything to be executed (D33).",
                ));
            }
        }

        for k in self.define.keys() {
            if k.is_empty() || !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                errs.push(err(
                    "define",
                    k,
                    "this is not a C macro name",
                    "a define's name is an identifier; anything else is the \
                     preprocessor being asked to parse a command line.",
                ));
            }
        }

        for l in &self.link {
            if l.is_empty()
                || l.starts_with('-')
                || l.contains('/')
                || l.contains('\\')
                || l.contains("..")
            {
                errs.push(err(
                    "link",
                    l,
                    "this is not a library name",
                    "`link` takes library names (`zstd` for `-lzstd`), not paths \
                     and not linker flags.",
                ));
            }
        }

        errs
    }

    /// Turn a checked recipe into an import request.
    ///
    /// `package_root` is the directory include paths resolve against;
    /// `sysroot_id` is the *identity* of the sysroot for the cache key
    /// (s47's bundle hash, or the system sysroot's path).
    pub fn to_request(
        &self,
        package_root: &str,
        target: &str,
        sysroot_id: Option<&str>,
    ) -> ImportRequest {
        let sep = if package_root.ends_with('/') { "" } else { "/" };
        ImportRequest {
            headers: self.headers.clone(),
            defines: self
                .define
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            cflags: self.cflags.clone(),
            include_paths: self
                .include
                .iter()
                .map(|i| format!("{package_root}{sep}{i}"))
                .collect(),
            target: target.to_string(),
            sysroot: sysroot_id.map(str::to_string),
        }
    }
}

/// Why an include path is not acceptable, if it is not.
fn bad_include(p: &str) -> Option<&'static str> {
    if p.is_empty() {
        return Some("an include path cannot be empty");
    }
    if p.starts_with('/') || p.starts_with('\\') {
        return Some("this is an absolute path");
    }
    // `C:\…` and `C:/…`.
    let b = p.as_bytes();
    if b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic() {
        return Some("this is an absolute path");
    }
    if p.split(['/', '\\']).any(|seg| seg == "..") {
        return Some("this climbs out of the package with `..`");
    }
    if p.contains('$') || p.contains('%') {
        return Some("this interpolates an environment variable");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_recipe() -> Recipe {
        Recipe {
            name: "zstd".into(),
            headers: vec!["zstd.h".into()],
            include: vec!["vendor/zstd/lib".into()],
            define: BTreeMap::from([("ZSTD_STATIC_LINKING_ONLY".to_string(), "1".to_string())]),
            cflags: vec!["-std=c11".into()],
            link: vec!["zstd".into()],
            sysroot: Sysroot::Bundled,
        }
    }

    #[test]
    fn a_well_formed_recipe_passes() {
        assert_eq!(ok_recipe().check(), Vec::new());
    }

    /// The rule that makes a manifest portable: a path only its author
    /// has is refused at the manifest, not at someone else's build.
    #[test]
    fn absolute_and_escaping_include_paths_are_refused() {
        for bad in [
            "/usr/include",
            "C:\\include",
            "../../elsewhere",
            "$HOME/include",
            "",
        ] {
            let mut r = ok_recipe();
            r.include = vec![bad.to_string()];
            let errs = r.check();
            assert_eq!(errs.len(), 1, "`{bad}` should be refused");
            assert_eq!(errs[0].field, "include");
            assert!(!errs[0].note.is_empty());
        }
    }

    /// D33 in one test: nothing in a recipe may arrange for a program
    /// to run.
    #[test]
    fn flags_that_run_things_are_refused_by_name() {
        for bad in [
            "@response.txt",
            "-fplugin=./evil.so",
            "-Xclang",
            "-B/opt/tools",
            "--sysroot=/elsewhere",
            "-specs=/x",
        ] {
            let mut r = ok_recipe();
            r.cflags = vec![bad.to_string()];
            let errs = r.check();
            assert_eq!(errs.len(), 1, "`{bad}` should be refused");
            assert_eq!(errs[0].field, "cflags");
            assert!(errs[0].note.contains("D33"), "{}", errs[0].note);
        }
    }

    /// `-I` and `-D` smuggled through `cflags` would dodge the include
    /// path check, so they are refused with a pointer at the fields
    /// that *are* checked.
    #[test]
    fn include_and_define_cannot_be_smuggled_through_cflags() {
        let mut r = ok_recipe();
        r.cflags = vec!["-I/usr/include".into(), "-DFOO=1".into()];
        let errs = r.check();
        assert_eq!(errs.len(), 2);
        for e in &errs {
            assert!(e.why.contains("`include` and `define`"), "{}", e.why);
        }
    }

    #[test]
    fn link_takes_names_not_paths_or_flags() {
        for bad in ["-lzstd", "/usr/lib/libz.a", "../libz", ""] {
            let mut r = ok_recipe();
            r.link = vec![bad.to_string()];
            assert_eq!(r.check().len(), 1, "`{bad}` should be refused");
        }
    }

    #[test]
    fn a_recipe_with_no_headers_is_pointless_and_says_so() {
        let mut r = ok_recipe();
        r.headers.clear();
        let errs = r.check();
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].field, "headers");
    }

    /// Every violation is reported at once.
    #[test]
    fn all_violations_are_reported_together() {
        let mut r = ok_recipe();
        r.include = vec!["/abs".into()];
        r.cflags = vec!["@resp".into()];
        r.link = vec!["-lbad".into()];
        assert_eq!(r.check().len(), 3);
    }

    /// Include paths reach the worker resolved against the package
    /// root, and the request keeps their order.
    #[test]
    fn to_request_resolves_include_paths_against_the_package() {
        let req = ok_recipe().to_request("/home/x/proj", "x86_64-unknown-linux-gnu", Some("b3:aa"));
        assert_eq!(req.include_paths, vec!["/home/x/proj/vendor/zstd/lib"]);
        assert_eq!(req.headers, vec!["zstd.h"]);
        assert_eq!(
            req.defines,
            vec![("ZSTD_STATIC_LINKING_ONLY".to_string(), "1".to_string())]
        );
        assert_eq!(req.sysroot.as_deref(), Some("b3:aa"));
    }

    #[test]
    fn sysroot_is_a_named_choice_not_a_path() {
        assert_eq!(Sysroot::parse("bundled"), Some(Sysroot::Bundled));
        assert_eq!(Sysroot::parse("system"), Some(Sysroot::System));
        assert_eq!(Sysroot::parse("/opt/sysroot"), None);
        assert_eq!(Sysroot::default(), Sysroot::Bundled);
    }

    #[test]
    fn the_forbidden_key_list_covers_the_obvious_ways_to_ask_for_a_script() {
        for k in ["pkg_config", "command", "exec", "configure", "shell"] {
            assert!(FORBIDDEN_RECIPE_KEYS.contains(&k), "{k} should be refused");
        }
    }
}
