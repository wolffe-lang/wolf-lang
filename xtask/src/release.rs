//! The release's musl runtime archive and its read-back (s266,
//! wolf-lang#935): the pieces of `cargo xtask dist-rt` and `cargo xtask
//! release-readback` that are pure, so they are tested without a
//! release.
//!
//! **Why the archive exists.** A release carried `libwolf_rt.a` for the
//! GNU target only, so a static x86-64 link against musl (ligature's CI,
//! anything outside PAX's ISO) had no runtime to link: the GNU archive
//! against musl fails on `open64`/`fstat64` and the unwinder. PAX's
//! `tools/mkwolf` builds the musl runtime for its dev image, and its
//! recipe is the oracle for the flags here: rust as `rust-toolchain.toml`
//! pins it, `cargo build --release --locked` of the same three packages
//! (`wolf_driver`, `wolf_rt`, `wolf_cimport`: one invocation, so the
//! dependency features unify exactly as the image's do), `+crt-static`,
//! `relocation-model=static`, symbols stripped, no build id, and the
//! source, target and registry paths remapped, so the bytes do not
//! depend on where the build ran.

/// The one target the release builds a second runtime for.
pub const MUSL_TARGET: &str = "x86_64-unknown-linux-musl";

/// The marker that opens the release body's musl section, so a re-run
/// of the job replaces the section instead of appending a second one.
pub const NOTES_MARKER: &str = "<!-- wolf-rt-musl -->";

/// Linux's large-file names, which the GNU archive references and a
/// musl libc (since 1.2.4) does not define: a musl runtime that still
/// asks for one is the GNU archive under another name.
pub const GLIBC_ONLY_SYMBOLS: [&str; 4] = ["open64", "fstat64", "stat64", "lseek64"];

/// The archive's file and directory name, without `.tar.gz`.
pub fn rt_archive_stem(version: &str) -> String {
    format!("wolf-rt-{version}-{MUSL_TARGET}")
}

/// mkwolf's `RUSTFLAGS`, with this checkout's three paths. rustc applies
/// the LAST matching `--remap-path-prefix`, so a target directory inside
/// the source tree still maps to `/target`, as mkwolf's sibling
/// directories do.
pub fn musl_rustflags(src: &str, target_dir: &str, registry: &str) -> String {
    format!(
        "-C target-feature=+crt-static -C relocation-model=static -C strip=symbols \
         -C link-arg=-Wl,--build-id=none --remap-path-prefix={src}=/wolf-lang \
         --remap-path-prefix={target_dir}=/target --remap-path-prefix={registry}=/cargo"
    )
}

/// The release body's musl section: what the archive is, how it was
/// built, and one digest line per artifact in the shape
/// [`parse_digests`] reads.
pub fn rt_notes(
    version: &str,
    rust: &str,
    archive_digest: &str,
    lib_digest: &str,
    lib_bytes: u64,
) -> String {
    let stem = rt_archive_stem(version);
    format!(
        "{NOTES_MARKER}\n### The musl runtime ({MUSL_TARGET})\n\n\
         `{stem}.tar.gz` carries `libwolf_rt.a` for `{MUSL_TARGET}`, the runtime a \
         static link against musl needs, with `LICENSE` and the runtime's \
         `LICENSE-EXCEPTION` (wolf-lang#935). It is built as PAX's `tools/mkwolf` builds \
         its dev image's: rust {rust}, `--release --locked`, `+crt-static`, \
         `relocation-model=static`, symbols stripped, the source, target and registry \
         paths remapped. `libwolf_rt_none.a`, the freestanding runtime, is the one every \
         host archive carries.\n\n\
         - `{stem}.tar.gz` sha256 `{archive_digest}`\n\
         - `libwolf_rt.a` sha256 `{lib_digest}` ({lib_bytes} bytes)\n"
    )
}

/// Every `- `NAME` sha256 `HEX`` line of a release body, in order.
pub fn parse_digests(notes: &str) -> Vec<(String, String)> {
    notes
        .lines()
        .filter_map(|l| {
            let rest = l.trim().strip_prefix("- `")?;
            let (name, rest) = rest.split_once('`')?;
            let rest = rest.trim_start().strip_prefix("sha256 `")?;
            let (hex, _) = rest.split_once('`')?;
            let ok = hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit());
            (ok && !name.is_empty()).then(|| (name.to_string(), hex.to_ascii_lowercase()))
        })
        .collect()
}

/// One file read back: its name and the digest of each of two reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadBack {
    pub name: String,
    pub first: String,
    pub second: String,
}

/// The read-back's verdict (r40 item 3's shape, now with the musl
/// archive): every file read twice to the same digest; exactly four host
/// archives `wolf-{version}-*.tar.gz`; the musl archive present, its
/// digest line in the notes and equal to its bytes; the `libwolf_rt.a`
/// inside it (`lib`, read twice) equal to its own line; and every other
/// digest line in the notes equal to the file it names. `Ok` is the
/// table the job prints; `Err` names every failure.
pub fn readback(
    version: &str,
    notes: &str,
    files: &[ReadBack],
    lib: Option<&ReadBack>,
) -> Result<Vec<String>, Vec<String>> {
    let mut bad = Vec::new();
    let mut table = Vec::new();
    for f in files.iter().chain(lib) {
        if f.first != f.second {
            bad.push(format!(
                "{}: two reads disagree ({} then {})",
                f.name, f.first, f.second
            ));
        }
        table.push(format!("{}  {}", f.first, f.name));
    }
    let host_prefix = format!("wolf-{version}-");
    let hosts = files
        .iter()
        .filter(|f| f.name.starts_with(&host_prefix) && f.name.ends_with(".tar.gz"))
        .count();
    if hosts != 4 {
        bad.push(format!(
            "{hosts} host archive(s) `{host_prefix}*.tar.gz`, wanted 4"
        ));
    }
    let lines = parse_digests(notes);
    let line_for = |name: &str| lines.iter().find(|(n, _)| n == name).map(|(_, h)| h);
    let musl = format!("{}.tar.gz", rt_archive_stem(version));
    match files.iter().find(|f| f.name == musl) {
        None => bad.push(format!("the musl runtime archive `{musl}` is missing")),
        Some(f) => match line_for(&musl) {
            None => bad.push(format!(
                "the notes carry no digest line for `{musl}` (`- `{musl}` sha256 `…``)"
            )),
            Some(h) if *h != f.first => bad.push(format!(
                "`{musl}`: the notes say {h}, the bytes are {}",
                f.first
            )),
            Some(_) => {}
        },
    }
    match (lib, line_for("libwolf_rt.a")) {
        (None, _) => bad.push(format!("`{musl}` holds no `libwolf_rt.a`")),
        (Some(_), None) => {
            bad.push("the notes carry no digest line for the musl `libwolf_rt.a`".to_string())
        }
        (Some(l), Some(h)) if *h != l.first => bad.push(format!(
            "the musl `libwolf_rt.a`: the notes say {h}, the bytes are {}",
            l.first
        )),
        _ => {}
    }
    for (name, hex) in &lines {
        if name == "libwolf_rt.a" {
            continue;
        }
        match files.iter().find(|f| &f.name == name) {
            None => bad.push(format!(
                "the notes name `{name}`, which the release does not carry"
            )),
            Some(f) if &f.first != hex => bad.push(format!(
                "`{name}`: the notes say {hex}, the bytes are {}",
                f.first
            )),
            Some(_) => {}
        }
    }
    if bad.is_empty() { Ok(table) } else { Err(bad) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const C: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

    fn rb(name: &str, d: &str) -> ReadBack {
        ReadBack {
            name: name.to_string(),
            first: d.to_string(),
            second: d.to_string(),
        }
    }

    fn release(version: &str) -> Vec<ReadBack> {
        let mut v: Vec<ReadBack> = [
            "x86_64-unknown-linux-gnu",
            "aarch64-unknown-linux-gnu",
            "aarch64-apple-darwin",
            "x86_64-pc-windows-msvc",
        ]
        .iter()
        .map(|h| rb(&format!("wolf-{version}-{h}.tar.gz"), C))
        .collect();
        v.push(rb(&format!("{}.tar.gz", rt_archive_stem(version)), A));
        v
    }

    #[test]
    fn the_notes_section_parses_back_to_its_two_digests() {
        let n = rt_notes("0.2.36", "1.97.1", A, B, 45_363_160);
        assert!(n.starts_with(NOTES_MARKER));
        assert_eq!(
            parse_digests(&n),
            vec![
                (
                    "wolf-rt-0.2.36-x86_64-unknown-linux-musl.tar.gz".to_string(),
                    A.to_string()
                ),
                ("libwolf_rt.a".to_string(), B.to_string()),
            ]
        );
        assert!(n.contains("(45363160 bytes)"));
    }

    #[test]
    fn a_digest_line_needs_sixty_four_hex_digits() {
        let notes = "- `x` sha256 `abc`\n- `y` sha256 `zz`\n* `z` sha256 `{A}`\nprose `q` sha256\n";
        assert!(parse_digests(notes).is_empty());
        let upper = format!("  - `x.tar.gz` sha256 `{}` (1 bytes)", A.to_uppercase());
        assert_eq!(parse_digests(&upper), vec![("x.tar.gz".into(), A.into())]);
    }

    #[test]
    fn a_whole_release_reads_back() {
        let notes = format!("changes\n\n{}", rt_notes("0.2.36", "1.97.1", A, B, 1));
        let table = readback(
            "0.2.36",
            &notes,
            &release("0.2.36"),
            Some(&rb("libwolf_rt.a", B)),
        )
        .expect("green");
        assert_eq!(table.len(), 6);
    }

    #[test]
    fn a_missing_digest_line_is_red_by_name() {
        // The plant's shape: the section without its archive line.
        let notes = rt_notes("0.2.36", "1.97.1", A, B, 1).replace(
            "- `wolf-rt-0.2.36-x86_64-unknown-linux-musl.tar.gz` sha256",
            "- `wolf-rt-0.2.36-x86_64-unknown-linux-musl.tar.gz` sha-256",
        );
        let e = readback(
            "0.2.36",
            &notes,
            &release("0.2.36"),
            Some(&rb("libwolf_rt.a", B)),
        )
        .unwrap_err();
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("no digest line for `wolf-rt-0.2.36"), "{e:?}");
    }

    #[test]
    fn every_disagreement_is_named() {
        let notes = rt_notes("0.2.36", "1.97.1", B, A, 1);
        let mut files = release("0.2.36");
        files.remove(0);
        files[0].second = B.to_string();
        let e = readback("0.2.36", &notes, &files, Some(&rb("libwolf_rt.a", B))).unwrap_err();
        let all = e.join("\n");
        assert!(all.contains("two reads disagree"), "{all}");
        assert!(all.contains("3 host archive(s)"), "{all}");
        assert!(all.contains("the notes say bbbb"), "{all}");
        assert!(
            all.contains("the musl `libwolf_rt.a`: the notes say aaaa"),
            "{all}"
        );
        let e = readback("0.2.36", &notes, &release("0.2.36")[..4], None).unwrap_err();
        let all = e.join("\n");
        assert!(all.contains("is missing"), "{all}");
        assert!(all.contains("holds no `libwolf_rt.a`"), "{all}");
        assert!(all.contains("which the release does not carry"), "{all}");
    }

    #[test]
    fn the_flags_are_mkwolfs_with_the_target_remap_last_but_one() {
        let f = musl_rustflags("/w/src", "/w/src/target/dist-rt", "/home/u/.cargo");
        for want in [
            "-C target-feature=+crt-static",
            "-C relocation-model=static",
            "-C strip=symbols",
            "-C link-arg=-Wl,--build-id=none",
        ] {
            assert!(f.contains(want), "{f}");
        }
        let src = f.find("--remap-path-prefix=/w/src=/wolf-lang").unwrap();
        let tgt = f
            .find("--remap-path-prefix=/w/src/target/dist-rt=/target")
            .unwrap();
        let reg = f.find("--remap-path-prefix=/home/u/.cargo=/cargo").unwrap();
        assert!(src < tgt && tgt < reg, "{f}");
    }
}
