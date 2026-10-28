"""s270 bisect knob (never committed): WOLF_MIDEND_SKIP=a,b,... skips mid-end
passes and whole-program phases by name. Applied to the hasu clone, the
binary copied out, then `git checkout --` restores both files."""
import pathlib, sys

root = pathlib.Path(sys.argv[1])
mod = root / "crates/wolf_wir/src/midend/mod.rs"
wp = root / "crates/wolf_wir/src/midend/wp.rs"
s = mod.read_text()
helper = '''
pub(crate) fn s270_skipped(name: &str) -> bool {
    std::env::var("WOLF_MIDEND_SKIP")
        .map(|v| v.split(',').any(|s| s == name))
        .unwrap_or(false)
}

/// One function's full pass sequence'''
assert s.count("\n/// One function's full pass sequence") == 1
s = s.replace("\n/// One function's full pass sequence", helper, 1)
pairs = [
    ("    simplify::run(m, fid, ve, th, stats)?;\n    inline::run(m, fid, ve, th, stats, scope)?;\n    simplify::run(m, fid, ve, th, stats)?;\n",
     "    if !s270_skipped(\"simplify\") { simplify::run(m, fid, ve, th, stats)?; }\n    if !s270_skipped(\"inline\") { inline::run(m, fid, ve, th, stats, scope)?; }\n    if !s270_skipped(\"simplify\") { simplify::run(m, fid, ve, th, stats)?; }\n"),
    ("    memopt::run(m, fid, ve, stats)?;\n    rangeopt::run(m, fid, ve, th, stats)?;\n    sink::run(m, fid, ve, th, stats)?;\n    coalesce::run(m, fid, ve, th, stats)?;\n    let hoisted = licm::run(m, fid, ve, stats)?;\n",
     "    if !s270_skipped(\"memopt\") { memopt::run(m, fid, ve, stats)?; }\n    if !s270_skipped(\"rangeopt\") { rangeopt::run(m, fid, ve, th, stats)?; }\n    if !s270_skipped(\"sink\") { sink::run(m, fid, ve, th, stats)?; }\n    if !s270_skipped(\"coalesce\") { coalesce::run(m, fid, ve, th, stats)?; }\n    let hoisted = if s270_skipped(\"licm\") { false } else { licm::run(m, fid, ve, stats)? };\n"),
    ("    hdrprom::run(m, fid, ve, th, stats)?;\n    loopcse::run(m, fid, ve, stats)?;\n    simplify::run(m, fid, ve, th, stats)?;\n    Ok(())\n",
     "    if !s270_skipped(\"hdrprom\") { hdrprom::run(m, fid, ve, th, stats)?; }\n    if !s270_skipped(\"loopcse\") { loopcse::run(m, fid, ve, stats)?; }\n    if !s270_skipped(\"simplify\") { simplify::run(m, fid, ve, th, stats)?; }\n    Ok(())\n"),
]
for a, b in pairs:
    assert s.count(a) == 1, a
    s = s.replace(a, b, 1)
mod.write_text(s)
w = wp.read_text()
wpairs = [
    ("    let dstats = dedup::dedup(m);\n",
     "    let dstats = if super::s270_skipped(\"dedup\") { Default::default() } else { dedup::dedup(m) };\n"),
    ("    let (minted, ipr_facts) = interproc::mint(m, &ipr, digest_prefix);\n",
     "    let (minted, ipr_facts) = if super::s270_skipped(\"interproc\") { (Vec::new(), 0) } else { interproc::mint(m, &ipr, digest_prefix) };\n"),
    ("    for c in &clusters {\n        let visible = cluster::visible(c);\n",
     "    for c in clusters.iter().filter(|_| !super::s270_skipped(\"crossinline\")) {\n        let visible = cluster::visible(c);\n"),
]
for a, b in wpairs:
    assert w.count(a) == 1, a
    w = w.replace(a, b, 1)
wp.write_text(w)
print("knob applied")
