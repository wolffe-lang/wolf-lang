# Protocol fixtures ([proto.harness.fixtures])

Canned observation records every conforming schema validator must
accept/reject exactly as named: `valid.json`, `with-extensions.json`,
`with-warnings.json` (the s67 additive `warnings` array,
[proto.record.warn]), `clean-stop.json` (a full-ladder `pass` at a
lane's deepest rung, [proto.record.pass]) and `with-trap-message.json`
(the s169 additive `trap_message` plus the trap site in both
spellings, [proto.record.trap]) validate; `wrong-version.json` and
`missing-field.json` (no `phase_reached`) are rejected. Exercised by
xtask unit tests here and by wolf-interp's is00 harness independently.
