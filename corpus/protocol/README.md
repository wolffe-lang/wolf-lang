# Protocol fixtures ([proto.harness.fixtures])

Canned observation records every conforming schema validator must
accept/reject exactly as named: `valid.json`, `with-extensions.json`,
and `with-warnings.json` (the s67 additive `warnings` array,
[proto.record.warn]) validate; `wrong-version.json` and
`missing-field.json` (no `phase_reached`) are rejected. Exercised by
xtask unit tests here and by wolf-interp's is00 harness independently.
