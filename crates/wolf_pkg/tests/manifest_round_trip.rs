//! wolf-lang#175: what `wolf add` and `wolf init` write into a manifest,
//! the manifest's own reader gives back. The manifest is wolf syntax, read
//! by wolf's lexer, so a value is held to every character a TOML basic
//! string must escape (the C0 controls, `\b \t \n \f \r` among them, DEL,
//! `"` and `\`), to wolf's own interpolation braces, and to a seeded fuzz
//! of every field the writers fill: a path, a git URL and its tag, a
//! registry package and its minimum, and the package's own name.
//!
//! Before the fix the writers spliced each value between quotes as it was:
//! a Windows `file://D:\a\…` URL wrote `\a`, which the lexer refuses
//! (E0101), and a `{` opened an interpolation (E1501).

use std::path::Path;

use wolf_pkg::manifest::{self, DepSource};
use wolf_pkg::version::Version;

/// Parse `text` as a manifest; the diagnostics rendered when it fails.
fn parse(text: &str) -> Result<manifest::Manifest, String> {
    let mut sm = wolf_span::SourceMap::new();
    let file = sm.intern(Path::new("wolf.pkg"));
    let (m, diags) = manifest::parse(file, text);
    m.ok_or_else(|| {
        let codes: Vec<String> = diags
            .iter()
            .map(|d| format!("{} {}", d.code, d.message))
            .collect();
        format!("{codes:?}\n--- manifest ---\n{text}")
    })
}

/// A fresh manifest (the one `wolf add` creates) with `source` added.
fn added(name: &str, source: &DepSource) -> Result<manifest::Manifest, String> {
    let fresh = manifest::minimal_manifest(name);
    let m = parse(&fresh)?;
    parse(&manifest::insert_dep(&fresh, &m, "dep", source))
}

/// The three sources, each carrying `v` in every string it has (a
/// registry minimum is a version, never free text).
fn sources(v: &str) -> Vec<DepSource> {
    vec![
        DepSource::Path {
            path: v.to_string(),
        },
        DepSource::Git {
            url: v.to_string(),
            tag: v.to_string(),
        },
        DepSource::Registry {
            pkg: v.to_string(),
            major: 1,
            min: Version::parse("1.2.0").expect("a version"),
        },
    ]
}

/// The package's own name as `wolf add` writes it, `local/<directory>`:
/// never empty, which the schema refuses (E1502), whatever the directory.
fn round_trips(v: &str) {
    let name = format!("local/{v}");
    for source in sources(v) {
        let m = added(&name, &source).unwrap_or_else(|e| panic!("{v:?}: does not read back: {e}"));
        assert_eq!(m.name, name, "the name {v:?}");
        assert_eq!(m.deps.len(), 1, "{v:?}");
        assert_eq!(m.deps[0].source, source, "the source {v:?}");
    }
}

/// Each character a TOML basic string escapes, wolf's braces, and the
/// shapes the issue and its neighbours met.
#[test]
fn every_escaped_character_round_trips() {
    let named = [
        "\u{8}",
        "\t",
        "\n",
        "\u{c}",
        "\r",
        "\"",
        "\\",
        "\0",
        "\u{1}",
        "\u{1b}",
        "\u{1f}",
        "\u{7f}",
        "\u{85}",
        "{",
        "}",
        "{{",
        "}}",
        "{x}",
        "\\q",
        "\\x",
        "\\x41",
        "\\u{41}",
        "file://D:\\a\\wolf-lang\\util-repo",
        "C:\\Users\\me\\util",
        "../dep\\q",
        "../dep{1}",
        "../dep\"q",
        "é and 🐺",
        "",
    ];
    for v in named {
        round_trips(v);
    }
}

/// xorshift64*: a fixed, dependency-free sequence, so a failure names a
/// value that reproduces.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// 2,000 values per field: every ASCII character, the C1 controls, and a
/// few multi-byte ones, at lengths 0 to 40.
#[test]
fn fuzzed_values_round_trip() {
    let mut pool: Vec<char> = (0u8..=127).map(char::from).collect();
    pool.extend(['\u{80}', '\u{9f}', 'é', 'ß', '→', '\u{2028}', '🐺']);
    let mut rng = Rng(0x5289_0175_d1a6_c0de);
    for _ in 0..2000 {
        let len = rng.below(41) as usize;
        let v: String = (0..len)
            .map(|_| pool[rng.below(pool.len() as u64) as usize])
            .collect();
        round_trips(&v);
    }
}

/// The reader cooks what the lexer lets through: wolf's code point
/// escapes, the brace pairs, and a raw string as written.
#[test]
fn the_reader_cooks_what_the_lexer_accepts() {
    let text = "pkg {\n    name:    \"a{{b}}\\x41\\u{1F43A}\",\n    version: \"0.1.0\",\n    \
                edition: \"1\",\n    deps: {\n        d: { path: r\"C:\\a\\n\" },\n    },\n}\n";
    let m = parse(text).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(m.name, "a{b}A🐺");
    assert_eq!(
        m.deps[0].source,
        DepSource::Path {
            path: "C:\\a\\n".to_string()
        }
    );
}
