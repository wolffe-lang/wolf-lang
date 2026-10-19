s248 (wolf-lang#799; yew `docs/MAP.md` §5 G10) — `wolf test`'s discovery.

- `pkg/`: yw00's witness, verbatim (yew `notes/yw00-evidence/witness/test-discovery/pkg/`
  at `e9ea99b`). Two test files of one shape; at 0.2.32 and 0.2.33 `wolf test pkg`
  ran `a_test.lu::test_in_a` and `z_test.lu::main`, because discovery read the first
  file of the package built for each test file (`lib.lu` sorts before `z_test.lu`).
- `shared/`: a `test_*` function in a NON-test file of the directory. A test file's
  tests are its own `test_*` (ruling asked R6, the recommendation implemented): the
  shared file's `test_shape` is a helper `m_test.lu` calls, never a test of its own.
  At 0.2.33 discovery read `b.lu` (it sorts first) and ran `test_shape` as
  `m_test.lu`'s only test.
