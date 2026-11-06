# s283 — DWARF v1: source types and debug-only call frames

The debug tier now describes wolf's source types to a debugger, not just its scalars. Structures, tuples, `str`, `List[T]` and `Map[K, V]` (as their runtime headers), payload-free enums, payload enums, error unions and optionals all get DWARF type entries. Aggregate bindings are spilled where a debugger can read them. An explicit `-g` also writes non-allocated `.debug_frame`, so a debugger can unwind every instruction of every wolf function, return sequences included, without relying on frame pointers.

Footprint: cross-cutting

The change touches source debug metadata, native codegen, the runtime's layout witnesses, the ABI chapter and census tooling.

## Forbidden

No other compiler, debugger, DWARF library or decoder implementation was read. The format comes from DWARF 5, the processor ABIs, and the Intel and Arm instruction references. The only dependency used is gimli's write API, through its public documentation. The return-sequence and prologue decoders are this repository's own, written from the instruction encodings.

The change adds no runtime unwinding, personality routines, landing pads or new language features. The compile unit stays `DW_LANG_C11` for debugger compatibility. It does not emit:

- a Rust-language compile unit;
- an always-live payload union;
- a dynamic-count array type;
- location lists;
- inlined frames;
- release-tier DWARF.

## Inputs, verified

- **Base.** Development started at trunk `e2757a7bd702d3b21f06396291010d2d1cb6101d`. The final base is the trunk head after the two preceding compiler changes; it is named in the records below once they exist.
- **Prediction.** `notes/s283-dwarf-v1-prediction.txt` was committed before any implementation (`921a93bd`).
- **Oracles.** `notes/s283-dwarf-oracles.json` records lupin 0.1.59's tag, commit, spec pin, and archive and executable digests for both platforms (each checked twice). It also records the macOS SDK dwarfdump and lldb versions and digests. The interpreter rows compare against the release `crates/wolf_driver/PAIRING` names, not a second copy of its version.
  - Linux: GNU gdb 15.1 (Ubuntu 24.04 toolchain image), LLVM dwarfdump 21.1.8.
  - macOS arm64: lldb-2100.0.16.4, Apple LLVM 21.0.0 dwarfdump.
- **Sources.** DWARF 5 sections 2, 5, 6.4 and 7; the System V AMD64 psABI (registers and frames); the AArch64 DWARF ABI; Intel SDM volume 2 (ADD, SUB, MOV, PUSH, POP, LEAVE, RET); and the Arm A64 instruction descriptions (ADD, SUB, LDP, STP, LDR, STR, RET, and the PAC hints).
- **Layouts.** Sizes and offsets come from the compiler's native layout and the runtime's own headers. `wolf_rt`'s Rust-only `DEBUG_LAYOUT` constants derive each header's size and field offsets with `size_of` and `offset_of`, and the debug rows compare the emitted DIEs with them. They add no C ABI symbol, and the freestanding runtime compiles the same text.
- **Map.** After the preceding change, Map's header is 88 bytes: the nine existing words, then `index` at 72 (a pointer to a named opaque map index) and `index_cap` at 80 (`int`). The allocator's rounded 96-byte charge is not the type's size.

## Prediction

Source names, signedness, fields, variants and pointees must survive canonical WIR's machine-shape erasure. They are carried on the debug bindings, through a private layout interner, so canonical type ids don't move. Aggregate bindings need real addressable spills, not just new type DIEs. Borrowed byte views need their actual pointer and length pieces, without adding executable WIR instructions for metadata.

The native tier keeps frame pointers, so a backtrace that works proves nothing about CFI. The CFI is held to instruction-level witnesses instead. Backend unwind facts describe prologues, and the owned decoder proves each return sequence's effects against them. A sequence it cannot prove is reported as `CFI_GAP` and left out of FDE coverage, never described with a guessed rule.

Size was predicted as under 10% growth on scalar-only inputs, hundreds to a few thousand bytes per aggregate-heavy compile unit, and roughly 24 to 128 frame bytes per function plus CIEs. The measurements below replace those guesses; the corpus is not scalar-only, and its growth is larger.

## Evidence index

### Rows

| File | What it holds |
|---|---|
| `crates/wolf_driver/tests/debug_v1.rs` | gdb and lldb values for every type row; every scalar's emitted name, encoding and width; typed container contents; packed element strides; borrowed values; runtime-layout ties; paired-interpreter values; the named gap rows |
| `crates/wolf_driver/tests/support/dwarf_variant_bytes.rs` | On macOS, the SDK dwarfdump's DIEs give each discriminant and payload's location, size and type. LLDB's plain `memory read --binary` gives the bytes of the same variable. The selected variant and value must agree with lupin. This is a byte witness, not a formatter |
| `crates/wolf_driver/tests/debug_frame_steps.rs` | From machine entry through return, a backtrace at every executed owned instruction: stable caller suffixes, nested and tail calls, the synthetic `main`, both variadic paths (AL=0 and AL>0), and a pressure chain whose `middle` restores callee-saved registers before `mov rsp, rbp`. Also the FDE ranges against the symbols, and an x86-64 coverage row that runs on any host through the freestanding target |
| `crates/wolf_driver/tests/support/oracle_gate.rs` | With `WOLF_DWARF_REQUIRE=1`, every lane take fails a row whose debugger, dump tool or pinned oracle is missing. Elsewhere such a row prints a SKIP with its reason, never a pass (R-s283-6) |
| `crates/wolf_backend/src/frame_decode.rs`, `crates/wolf_codegen_clif/src/debug_frame.rs` | Bounded decoding of whole ranges and of return sequences; the prologue distance confirmed by the save facts; rule changes at instruction ends; refusal of malformed and unknown forms; save-address disagreement; return-derived CFA consistency |
| `crates/wolf_wir/src/midend/compact.rs` | Split debug pieces survive value renumbering, and disappear if a required piece is removed |
| `crates/wolf_driver/examples/dwarf_sections.rs`, `ci/dwarf-census.py` | Per-section sizes and digests; byte-and-relocation determinism; dwarfdump digests and the emitted vocabulary. A refused compile is reported as not measured. A head-only refusal, an inconsistent repeat or any `CFI_GAP` fails the census |

### Red before green

| Head | Rows | Record |
|---|---|---|
| `b9e4c072` | all 10 original source-type rows red by name | `notes/s283-evidence/linux/red-b9e4c072-source-types.log` |
| `71474842` | all 5 unsigned boundary rows red by name, with no compile-error skip | `notes/s283-evidence/linux/red-71474842-unsigned.log` |
| `c7e8710f` | the per-instruction witness red at `inspect`'s return (a prologue-only FDE dropped its caller), plus the FDE ranges | `notes/s283-evidence/linux/red-c7e8710f-frame-steps.excerpt.log` |
| `e48afd83` | `dwarf-frame-x64-teardown-coverage` red: `CFI_GAP middle 228..252: unknown SP before register restore` | `notes/s283-evidence/macos/red-e48afd83-x64-teardown.log` |
| `c78972fd` | the corpus census before the teardown fix: 2145 uncovered return ranges in 756 measured programs, and 22 across the 31 type fixtures | `notes/s283-evidence/linux/census-c78972fd-before-teardown-fix.json` |

These are development controls. The final plant is separate (below).

### The teardown fix (R-s283-3's count row)

At `c78972fd`, nearly every x86-64 function that saves a callee-saved register had an uncovered return range. A typical sequence, from `middle` above:

```
mov rbx, [rsp+0x20]; mov r12, [rsp+0x28]; mov r13, [rsp+0x30]
add rsp, 0x40; mov rsp, rbp; pop rbp; ret
```

Deriving SP backwards from `ret` stops at `mov rsp, rbp`, so the SP-relative restores before it could not be checked. The producer reports no stack allocation under a frame-pointer CFA, so the unwind facts alone do not give SP either. The fix decodes the prologue's own bytes to find SP's distance below the CFA, and accepts that distance only if the facts confirm it:

- the frame base sits where the CFA rule puts it;
- every decoded save of a register the facts place in memory addresses that same slot.

A fixed frame keeps SP there until the return sequence. Each restore is then checked against its save fact; a wrong distance fails that check and leaves the range uncovered, never a wrong rule. A return sequence the backward derivation already covers never consults the prologue, so arm64, whose return sequences restore by post-index loads, is unchanged.

### Census and determinism

These numbers are measured at the development head; the final census is pending. `ci/dwarf-census.py` covers every positive corpus entry (781 candidates, `//! check: run(`) and the 31 type fixtures. It compiles each to objects five times: trunk once, then the head twice without `-g` and twice with it.

| Section | trunk | head | head `-g` |
|---|---|---|---|
| `.debug_info` | 188,382 | 296,252 | 312,355 |
| `.debug_abbrev` | 47,555 | 69,250 | 79,078 |
| `.debug_line` | 200,491 | 201,431 | 204,548 |
| `.debug_frame` | — | — | 225,920 |
| total | 436,428 | 566,933 (+29.9%) | 821,901 (+88.3%) |

That table is Linux x86-64 at `c78972fd`: 756 programs measured, 25 not measured, 0 failures. Every head object, with and without `-g`, was byte-and-relocation identical across its two builds (789 objects each). The 25 not measured are inputs that trunk refuses too when staged standalone; each is named in the census JSON. The largest growth was `memory/read_lend_frame.lu`, from 800 to 5,767 bytes, which is type information for its lent aggregates.

### Explicit presentation and fidelity gaps

| Gap | Asserted observation | Independent value coverage |
|---|---|---|
| `KNOWN-GAP lldb-variant-part`, payload enum | `(Choice) value = {}` | SDK DIE-directed bytes select `Number 73`; lupin agrees |
| `KNOWN-GAP lldb-variant-part`, error union | `(int ! {Bad(int)}) value = {}` | SDK DIE-directed bytes select `Bad 61`; lupin agrees |
| `KNOWN-GAP lldb-variant-part`, optional | `(int ! {none}) value = {}` | SDK DIE-directed bytes select `none`; lupin agrees |
| `KNOWN-GAP dwarf-recursive-nominal` | a named incomplete `opaque recursive Node` declaration | complete recursive DIE cycles are not claimed |

The gap assertions fail if the presentation changes, and they are never counted as rendering passes. gdb renders the same variant parts on Linux, and those rows pass on its values. Byte integers are shown numerically, by a widening that preserves the emitted signedness, never by a forced unsigned reinterpretation.

### The reader contract for sniff

This is what wolf emits, as observed in the census dumps. The line-table header's opcode-length legend is not counted as emitted vocabulary.

| Surface | What a reader must handle |
|---|---|
| Units and sections | DWARF32, CU version 4, little endian, 8-byte addresses. `.debug_info`, `.debug_abbrev`, `.debug_line`, and `.debug_frame` only under `-g`. Mach-O uses `__DWARF,__debug_*`, and linked programs carry a dSYM |
| Forms | `DW_FORM_addr`, `string`, `udata`, `sdata`, `ref4`, `sec_offset`, `exprloc`, `flag` |
| Tags | `compile_unit`, `subprogram`, `formal_parameter`, `variable`, `base_type`, `pointer_type`, `reference_type`, `structure_type`, `member`, `enumeration_type`, `enumerator`, `variant_part`, `variant` |
| Attributes | `producer`, `name`, `comp_dir`, `language`, `stmt_list`, `low_pc` and a constant `high_pc`, `decl_file`, `decl_line`, `frame_base`, `type`, `location`, `byte_size`, `encoding`, `data_member_location` (a constant), `declaration`, `artificial`, `const_value` (signed), `discr`, `discr_value` (signed) |
| Locations | `frame_base` is `DW_OP_reg6` (x86-64) or `DW_OP_reg29` (arm64). Variables are `DW_OP_fbreg` whole-function spills, not location lists |
| Scalars | each scalar has its own wolf name and byte size, with signed, unsigned, boolean, float, unsigned-char or UTF encoding. Never infer signedness from width |
| Structures and tuples | members carry constant offsets; tuple fields are `_0`, `_1`, … |
| Indirection and headers | typed pointers and references. Opaque runtime objects are named incomplete declarations. Data pointers are typed, and count and stride are separate fields, never a fabricated dynamic array |
| Variants | a structure owns a `variant_part` whose `discr` refers to its discriminant member. Each `variant`'s `discr_value` selects its payload members; a variant without one is the default. Interpret only the active view |
| CFI | CIE and FDEs in `.debug_frame` only, never `.eh_frame`: CIE version 1, absolute initial addresses and CIE offsets. Observed: `DW_CFA_def_cfa`, `offset`, `same_value`, `advance_loc`, `advance_loc1`, `advance_loc2`, `nop`. The writer can also emit `val_offset` (a register held at a CFA offset) and `negate_ra_state` (signed arm64 return addresses); no census input exercised either |
| Registers | x86-64: RSP 7, RBP 6, return address 16. arm64: SP 31, FP 29, LR 30. These are 64-bit values even where a dump spells them WSP, W29 or W30 |
| Line programs | observed in the type fixtures: `DW_LNS_copy`, `advance_pc`, `set_column`, `negate_stmt`, `const_add_pc`, `set_prologue_end`, `DW_LNE_set_address`, `end_sequence`, and special opcodes. A reader still needs every standard opcode the header declares |
| Coverage | one function may have several FDE ranges. A range a debugger finds no FDE for needs its fallback; never extend the preceding rule over it |
| Presentation | LLDB under this C compile unit does not present variant parts. sniff renders the active variant itself from the DWARF; it must not imitate LLDB's empty presentation |

The fixtures' dwarfdump digests are listed in `notes/s283-evidence/fixture-digests.txt`, produced by the final census: `llvm-dwarfdump --verbose --debug-info --debug-abbrev --debug-line --debug-frame object.o`, run in each object's directory. Each digest also covers the compile directory and entry path the objects record.

### Rulings

- **R-s283-1** (accepted): wolf has no fixed arrays, so none are described. List and Map are described as their native headers.
- **R-s283-2** (accepted): call-frame information goes only in `.debug_frame`, under `-g`. `nounwind` is kept. The spec's sentence on unwind tables is narrowed, without changing its anchor id, to forbid runtime unwinding, personality routines and landing pads only.
- **R-s283-3** (accepted): epilogue rules come from an owned minimal decoder over the emitted bytes. An undecodable return sequence leaves its range uncovered, and its count row must be zero. The census now fails on any `CFI_GAP`, and the teardown fix brings the count to zero (measured below).
- **R-s283-4** (accepted): variant parts stay standard under the C11 compile unit. LLDB's empty rendering is the named gap `lldb-variant-part`, with a byte witness against lupin.
- **R-s283-5** (accepted): Map is described as its real 88-byte header, and a row ties the DWARF size and offsets to the runtime's layout.
- **R-s283-6** (recommended, implemented): the DWARF rows' dependence on host tools. The options were:
  - (a) fail whenever a tool is missing or a version differs from the pins, which turns any CI runner whose image moves red;
  - (b) a named SKIP unless `WOLF_DWARF_REQUIRE=1`, which every lane take and gauntlet here sets;
  - (c) pin the tools in CI.

  The recommendation is (b), which follows the repository's existing `WOLF_*_REQUIRE_*` convention. A skip is never a measurement, and the merge records come from takes that require the tools. On macOS the frame-range row now finds the SDK's dwarfdump by itself, so it is measured without an environment variable.

### Final records (pending the rebase)

- The final census (Linux corpus and fixtures; macOS corpus and fixtures) with zero `CFI_GAP`, its totals, and the fixture digests.
- The full `cargo xtask ci` gauntlet at the final head; its `corpus`, `conformance` and `differ-self` steps are the differential and corpus records.
- One final-head plant: one file, one hunk, the variant discriminant's member offset moved from 0 to 8, red by name at the macOS byte row `dwarf-v1-payload-enum-bytes`.

## Done-when

- [x] Prediction committed before implementation; the clean-room sources are kept.
- [x] Source types and aggregate and split debug locations implemented.
- [x] Debug-only CFI, fail-closed return-sequence coverage, and instruction-level witnesses.
- [ ] The return-sequence count row at zero on x86-64 across the census (the fix-head census).
- [x] Named LLDB and recursive gaps kept separate from value passes.
- [x] lupin 0.1.59 values paired; scalar encodings and runtime-layout ties checked on Linux and macOS.
- [x] Anchor regeneration checked in both directions (724 keys; none added, removed or re-owned).
- [ ] Rebase once after the two preceding compiler changes; Map at its 88-byte header with its tie row.
- [ ] The final census, digests and determinism; the macOS retake; the full gauntlet.
- [ ] The final-head discriminant-offset plant and its record.
- [ ] Final head pushed, PR opened with this note, run id read once, one lane audit.
