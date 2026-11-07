# r43: the thirty-eighth release, wolf 0.2.38 with lupin 0.1.60

wolf 0.2.38 was released from `95ae0827` (wolf-lang#1067, release run 42354798018), paired with lupin 0.1.60 from `17bd5223` (wolf-interp#348, release run 42328498687) and the bundled std at wolf-std `d4a4d4ca`. The release note is `docs/releases/0.2.38.md`, and the prediction is `docs/releases/0.2.38-prediction.txt`. The doors moved in homebrew-wolf#49. This note records the measurement that came after the tag: s283's DWARF rows against the release commit.

## Why the DWARF rows were measured again

0.2.38's headline is DWARF v1 (s283): source types in a debugger and debug-only call frames. s283's rows route a missing debugger, dump tool or pinned oracle through `tests/support/oracle_gate.rs`. That gate prints a named SKIP unless `WOLF_DWARF_REQUIRE=1` is set. The release gauntlet did not set it, and its image had no `llvm-dwarfdump`, so 22 of these rows printed SKIP there: 17 value, boundary and layout rows, the recursive-type gap row, and 4 frame-range rows. No workflow sets the variable either (wolf-lang#1069). So those rows had not been measured at this release.

They were measured after the tag, at the release commit `95ae0827`, in a fresh tree with an empty target. `WOLF_DWARF_REQUIRE=1` was set, the tools matched the pins `notes/s283-dwarf-oracles.json` names, and the published lupin 0.1.60 archive was the paired oracle. Each run first checks that the variable takes, with s283's own gate control: with the dump tool removed, `dwarf_v1_map_layout_matches_runtime` must fail by name when the variable is set, and print one SKIP and pass when it is unset.

## The measurement

| | Linux x86-64 | macOS arm64 |
| --- | --- | --- |
| tools | GNU gdb 15.1 (`Ubuntu 15.1-1ubuntu1~24.04.1`), LLVM dwarfdump 21.1.8 | lldb-2100.0.16.4, Apple LLVM 21.0.0 dwarfdump (SHA-256 93be00ae…, the pinned digest) |
| gate control | required: `FAIL dwarf-v1-map-runtime-layout` (0 passed, 1 failed); unrequired: one SKIP, passes | the same |
| `debug_v1` | 36 passed, 0 failed | 36 passed, 0 failed |
| `debug_frame_steps` | 19 passed, 0 failed | 19 passed, 0 failed |
| verdict lines | 78 PASS, 0 FAIL, 0 SKIP, 1 KNOWN-GAP | 70 PASS, 0 FAIL, 5 SKIP, 4 KNOWN-GAP |
| the 22 rows the gauntlet skipped | 21 PASS, 1 KNOWN-GAP | 20 PASS, 1 KNOWN-GAP, 1 SKIP (platform scope, below) |
| record | SHA-256 a26c88696cbc90d97f16408bd4f9ef70986fc179408c8d6e584d515a8dd05b64, `RECORD-END worst=0 steps=6`, verified by the org recorder | SHA-256 c2d70bd85417813619a0892f20bbffca2c3815ab57e4371414b0128f6a18f030, `RECORD-END worst=1 steps=7` |

The 21 Linux passes among the 22:
- the four frame-range rows, `dwarf-frame-{big-frame,pressure,variadic,wolf}-fde-ranges`;
- the ten value encodings: bool, byte, char, f32, f64, i8, i16, i32, i64 and int;
- the five unsigned boundary encodings: u8, u16, u32, u64 and uint;
- `dwarf-v1-list-runtime-layout` and `dwarf-v1-map-runtime-layout`.

The known gaps are s283's named ones. Each is an assertion that fails if the presentation changes, and none is counted as a pass:
- `dwarf-recursive-nominal`, on both hosts (`type=Node cut=opaque-recursive-declaration`);
- `lldb-variant-part`, three times on macOS: the payload enum, the error union and the optional, which LLDB presents as `{}` under the C compile unit.

**Five rows are platform-scoped on macOS:** `dwarf-frame-variadic-chain-steps`, `dwarf-frame-variadic-core-steps`, `dwarf-frame-variadic-entry-steps`, `dwarf-frame-variadic-vector-steps` and `dwarf-frame-variadic-fde-ranges`. Each says `reason=owned variadic ABI supports x86-64 Linux only, not this host`. That is the documented scope of wolf's variadic ABI, not a missing tool, and all five PASS in the Linux record.

The macOS record's last step is a summary that fails on any SKIP line, so it ends 1 on these five. It is accepted as is. The 0.2.38 DWARF holds on both hosts within its documented scope.

On Linux, two earlier runs of the same measurement were voided for faults in that summary step, and the rows were green in both. The first parser read a verdict only at the start of a line, while cargo prints it after `test <name> ... `. The second did not map the recursive gap's SKIP name (`dwarf-recursive-nominal-gap`) to its verdict line (`KNOWN-GAP dwarf-recursive-nominal`). The record above is the third run, at a fresh target.

## Known reds and follow-ups

- wolf-lang#1051: one race test fails intermittently on some Linux hosts, as it did at 0.2.37. glibc's malloc reads the CPU count from `/sys/devices/system/cpu/online` on the lowest free descriptor once a program runs more threads than its arena bound. The cause and the measurement are on the issue, and the fix is planned.
- wolf-lang#1057: the nightly property sweep's cascade bound on one mutated corpus file; it affects diagnostics on invalid source only.
- wolf-lang#1068: asm_link's hosted row builds only the debug runtime, so a release-profile test run on a fresh target fails it.
- wolf-lang#1069: no workflow sets `WOLF_DWARF_REQUIRE`, the gap this note closes by hand for 0.2.38.
- wolf-std#125: the std's lupin ledger lags 0.1.60 on seven rows and two doc lines; the release note names them.
- sniff's re-pin to 0.2.38 is unblocked. Its Linux DWARF rows pass against the published 0.2.38 and 0.1.60.
