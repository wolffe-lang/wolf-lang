import pathlib
import re
import sys

root = pathlib.Path(sys.argv[1])

def patch(rel, old, new, count=1):
    p = root / rel
    text = p.read_text()
    assert text.count(old) == count, (rel, text.count(old), old[:100])
    p.write_text(text.replace(old, new))

def guard(rel, name, unit, phase):
    p = root / rel
    text = p.read_text()
    pat = r'(?:pub(?:\([^)]*\))? )?fn ' + name + r'\([\s\S]*?\)\s*(?:->[^\n]+)?\{\n'
    matches = list(re.finditer(pat, text))
    assert len(matches) == 1, (rel, name, len(matches))
    at = matches[0].end()
    text = text[:at] + f'    let _s269_timer = wolf_span::s269_measure::start({unit}, "{phase}");\n' + text[at:]
    p.write_text(text)

p = root / 'crates/wolf_span/src/lib.rs'
p.write_text(p.read_text() + '\n#[path = "s269_measure.rs"]\npub mod s269_measure;\n')
(root / 'crates/wolf_span/src/s269_measure.rs').write_bytes(pathlib.Path(sys.argv[2]).read_bytes())

patch('crates/wolf_sema/src/graph.rs',
      '        // Parse every file of the module.\n',
      '        let _s269_parse = wolf_span::s269_measure::start(path.join("."), "lex/parse");\n        // Parse every file of the module.\n')
patch('crates/wolf_sema/src/graph.rs',
      '        self.modules[idx].files = file_indices.clone();',
      '        drop(_s269_parse);\n        self.modules[idx].files = file_indices.clone();')
patch('crates/wolf_sema/src/resolve.rs',
      '    let package = load_package(loader, aliases)?;',
      '    let _s269_load = wolf_span::s269_measure::start("@wall", "load/lex/parse");\n    let package = load_package(loader, aliases)?;\n    drop(_s269_load);\n    let _s269_resolve = wolf_span::s269_measure::start("@wall", "resolve");')
guard('crates/wolf_sema/src/resolve.rs', 'resolve_module', 'pkg.modules[module].dotted()', 'resolve')
guard('crates/wolf_sema/src/check.rs', 'check_body', 'pkg.modules[body.module].dotted()', 'typecheck')
guard('crates/wolf_mem/src/lib.rs', 'lower_body', 'pkg.modules[body.module].dotted()', 'mem/lower')
patch('crates/wolf_mem/src/lib.rs',
      '    for ((outcome, lowered), joined) in lowered_all.into_iter().zip(must_join) {',
      '    for ((outcome, lowered), joined) in lowered_all.into_iter().zip(must_join) {\n        let _s269_mem = wolf_span::s269_measure::start(pkg.modules[outcome.body.module].dotted(), "mem/check");')
guard('crates/wolf_wir/src/lower.rs', 'lower_body', 'pkg.modules[body.module].dotted()', 'WIR lowering')

p = root / 'crates/wolf_wir/src/midend/mod.rs'
text = p.read_text()
a = text.index('fn optimize_one(')
b = text.index('\n/// The functions', a)
fragment = text[a:b]
needle = ') -> Result<(), VerifyError> {\n'
fragment = fragment.replace(needle, needle + '    let s269_unit = scope.homes.map(|h| h.home_of(&m.funcs[fid].name)).unwrap_or("@package").to_string();\n')
lines = fragment.splitlines(True)
new = []
for line in lines:
    match = re.match(r'(\s*)(?:let hoisted = )?(\w+)::(run|revisit)\(', line)
    if match:
        indent, phase, _ = match.groups()
        new += [f'{indent}let _s269_pass = wolf_span::s269_measure::start(s269_unit.clone(), "midend/{phase}");\n', line, f'{indent}drop(_s269_pass);\n']
    else:
        new.append(line)
text = text[:a] + ''.join(new) + text[b:]
p.write_text(text)

p = root / 'crates/wolf_driver/src/main.rs'
text = p.read_text()
a = text.index('fn compile_native(')
b = text.index('\n/// `--codegen-report`', a)
f = text[a:b]
f = f.replace('    let mut sm = wolf_span::SourceMap::new();', '    let _s269_total = wolf_span::s269_measure::start("@wall", "total");\n    let mut _s269_phase = wolf_span::s269_measure::start("@wall", "load+resolve");\n    let mut sm = wolf_span::SourceMap::new();', 1)
for needle, phase in [
    ('    let tc = wolf_sema::typecheck_package(&res);', 'typecheck'),
    ('    let mem = wolf_mem::check_package(&res.package, &tc);', 'mem'),
    ('    let build = wolf_wir::lower_package(&res.package, &tc);', 'WIR lowering'),
    ('    let mid_stats = ', 'midend'),
    ('    // Backend: the driver drives the TRAIT;', 'codegen preparation'),
    ('    let codegen_start = ', 'codegen'),
    ('    link_objects(\n', 'link'),
]:
    assert f.count(needle) == 1, needle
    f = f.replace(needle, f'    drop(_s269_phase);\n    _s269_phase = wolf_span::s269_measure::start("@wall", "{phase}");\n    eprintln!("s269 measuring: {phase}");\n' + needle)
f = f.replace('    let acquire = |u: &ModUnit| -> UnitResult {', '    let acquire = |u: &ModUnit| -> UnitResult {\n        let _s269_unit = wolf_span::s269_measure::start(u.name.clone(), "codegen");')
text = text[:a] + f + text[b:]
p.write_text(text)
print('s269 temporary baseline instrumentation installed')
