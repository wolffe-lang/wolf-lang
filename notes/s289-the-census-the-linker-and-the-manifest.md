# s289: the diagnostics sweep, second change: every prelude stub, the lld a build picks, and a manifest `wolf add` reads back

The second of the sweep's three changes (`notes/s289-prediction.md`), from the first's head
(`notes/s289-the-diagnostics-sweep.md`). It covers three items:

- **The prelude census (wolf-lang#708, #1083).** Every ambient name used as a value
  (`let f = NAME`), called (`NAME()`) or as a receiver (`NAME.member()`) answered "a std/prelude
  stub without a signature" wherever the checker had no typing behind it: 431 of 687 such uses at
  the base. After the first change's stand-ins, 413 were left. Each now answers by its class:
  - a prelude type (`List`, `Map`, `Pool`, `Mutex`, `channel`, `Scope`, `Proc`, `range`) is a
    `type`-kinded value, as a builtin type is (D29). Calling one bare is E0406 naming the type and
    how its values are made (`List[int]()`, `channel[int](n)`, `a..b`, …);
  - a builtin function or intrinsic used as a value is declined as "a builtin function used as a
    value (call it by name)". lupin runs these, so this is the compiler's limit, named;
  - `min` and `zip`, the two helpers no clause types yet, are declined as such. The third change
    gives `min` a signature on every tier.
- **wolf-lang#1082:** on Linux, `wolf build` names the `ld.lld` it picked from PATH and its version,
  once, as a note, with the override (`WOLF_LD=cc` or `--linker cc`). `wolf build --help`, from
  which `wolf.1` is made, documents the selection. The choice is unchanged.
- **wolf-lang#175:** `wolf add` and `wolf init` write every manifest string through one quoting
  function, and the reader cooks exactly what wolf's lexer accepts. The manifest the add writes
  reads back, for every character a TOML basic string escapes, wolf's braces, and 2,000 fuzzed
  values per field.

## Forbidden

Footprint: module

The diff is `wolf_sema` (the checker's census answers and E0406 for a bare type call, the prelude
table's kind lookup, the census test), `wolf_driver` (the lld note, the build help, `wolf init`'s
quoting, and two tests), `wolf_pkg` (the manifest's quoting and reading, and its round-trip test),
the changelog and notes. No spec, codegen, runtime, CI, pin or toolchain change, and no corpus row,
so the lowering ledger does not move. Clean room as in the first change. No label, merge, tag,
release, issue closed, and no attribution trailer. The plant is pushed without a PR. Nothing ran
under `/tmp` or `/var/folders`, and no commit was made on a Linux host.

What existing programs mean, by decision:
- **The census:** a program that used a prelude type as a value, or called one bare, was declined
  and is now a value or E0406. Every other answer is still a refusal, now named. No corpus entry
  moves (the first change's corpus scan: none meets the stub but `errors.lu`, which that change
  answered).
- **#1082:** a Linux build with an `ld.lld` on PATH prints one more line on stderr.
- **#175:** a manifest written before this change reads as it did, except a string that held a
  `\xNN`, a `\u{…}` or a brace pair, which the reader now cooks as the lexer does. Such a string
  was refused by the lexer already, apart from the brace pairs, which no writer of wolf's
  produced.

## Inputs, verified

As the first change's (`notes/s289-the-diagnostics-sweep.md`), at its head `67bd054f`. The rows
and the checks ran on the second arm64 macOS machine. The census and the manifest rows are
host-free. The lld rows need Linux and an `ld.lld` on PATH, so they skip by name on macOS. The
Linux gauntlet's rig sets `WOLF_LLD_NOTE_REQUIRE=1`, which turns a missing lld there into a red.

## Prediction

`notes/s289-prediction.md` §3, "Second PR", held, with one correction. The round trip first named
the package with the raw fuzzed value, and an empty name is the schema's E1502, not a lexical
failure. The row now names the package as `wolf add` does, `local/<directory>` (`bf9340de`).

| row | at the rows' commit | at the fix |
|---|---|---|
| `wolf_sema` `prelude_census`: none of 687 uses answers the stub | 413 do (red) | 0 |
| `prelude_census`: each class in its own words | red at the first type | green |
| `wolf_pkg` `manifest_round_trip`: named values, fuzz, the reader | 3 red (E0101, E1501, E0109) | green |
| `pkg`: `add_writes_a_manifest_that_reads_back` (`dep\q`, `dep{1}`, `dep"q`, `dep\x`) | red, rolled back | green, the program runs |
| `lld_note`: the help names lld, `-fuse-ld=lld`, `WOLF_LD=cc` | red | green |
| `lld_note`: a Linux build notes the lld once, with its path and version | SKIP on macOS | Linux record |
| `lld_note`: `WOLF_LD=cc`, an object build: no note | SKIP on macOS | Linux record |

## Rulings

As recommended in the prediction and ruled:
- **R-s289-6:** the census answers by class (above).
- **R-s289-7's interim:** `min` and `zip` are declined by name. The third change types `min`.
- **R-s289-8:** the lld note, once per `wolf build`, only for a binary, never on macOS or
  Windows; the help documents it.
- **R-s289-9:** one quoting function for every manifest writer. The reader cooks the lexer's
  escape set and leaves a raw string raw.

## For the lupin mirror

Nothing here needs lupin: the census's answers are the compiler's own refusals of names lupin
runs, and the lld note and the manifest are the driver's.

## Evidence index

`notes/s289-evidence/`:
- `m2-red-53c81254-air.log`: the rows red at their commit, each by name.
- `m2-checks-934e36ac-air.log`: at the fix, all green:
  - every row; `wolf_pkg`'s and `wolf_sema`'s suites;
  - fmt and clippy;
  - the corpus: 0 bad, the fail-pin ledger unchanged at 443;
  - the catalog unchanged.
- `m2-rebase-map.txt`: the logged commits and the commits they are now, with the same patches.

The Linux record of this head and the plant's record are cited in the PR.

## Done-when

Commits:
- The rows: `4646f08c` (the manifest round trip), `f884b5b4` (`wolf add`'s), `3d42b032` (the lld
  note), `3fe71190` (the census).
- The fix:
  - `f75bad66`, `4cdbe333` (the quoting and the reader);
  - `27ae6243`, `af80621d` (the note and the help);
  - `22d9d262`, `d280aebd` (the census's answers).
- `0656ab85`, `190f786b`: formatting. `bf9340de`: the round trip names the package as `wolf add`
  does.
- `3c7f311d`: the changelog. `3eda92f6`: the README's linker paragraph. This note and its evidence.
- `ab56e962`: the first change's evidence rig (`rig/air-at.sh`) named the second Mac's ssh host;
  it now writes `SECOND_MAC` in its place, as the sanitizer does for the other hosts.

Test targets:
- [x] the rows red at their commit, by name (`m2-red-53c81254-air.log`)
- [x] the census: no ambient name in any of three uses answers the stub; each class in its words
- [x] the manifest: the named values and 2,000 fuzzed values per field round-trip; `wolf add`
      with a backslash, a brace or a quote in the dependency's path, and the program runs
- [x] `wolf build --help` documents lld and its override
- [x] fmt, clippy, the `wolf_pkg` and `wolf_sema` suites, the corpus `0 bad`, the catalog unchanged
- [ ] the lld note's Linux rows, and the gauntlet (`cargo xtask ci`, every step's `END`) at this
      head on Linux x86-64
- [ ] the plant red by name at `a_build_that_picks_lld_from_path_says_which_once`, its controls
      green
- [x] README read: its linker paragraph said the default link path was unchanged and named no
      lld; refreshed in a README-only commit (`3eda92f6`)
