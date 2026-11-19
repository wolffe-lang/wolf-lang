# ll02 prediction: the map-index doubling row under load

Committed on branch `ll02` before any change to the row. Base: trunk `a892fb61`.

## The row and its red

`map_index_lanes::native_string_index_doubling_costs_at_most_two_point_five` builds
`tests/fixtures/map_index/main.lu`, runs it at 50,000 and at 100,000 keys three times each,
and asserts median(100k) / median(50k) <= 2.5. It times each child by wall clock, polled
every millisecond. It failed once on hosted CI at ratio 2.7291 (run 44471799116: 50k
`[0.080, 0.058, 0.056]`, 100k `[0.189, 0.159, 0.114]`). The fastest pair there is 0.114 /
0.056 = 2.04: the code was linear, and two of the three 100k samples were slowed by the host.
A median of three absorbs one slow sample per size, never two.

## What the row guards

Indexing a string-keyed map costs time linear in the number of keys. A linear cost doubles
when the keys double (ratio 2); a quadratic cost quadruples (ratio 4). The bound 2.5 sits
between the two and stays.

## The change, predicted

1. Time the child's own CPU (user plus system, from `wait4`'s rusage), not the wall clock.
   Time a child spends off the CPU waiting for a busy host is no longer counted.
2. Take seven interleaved pairs (50k, 100k, 50k, 100k, ...) and compare the minimum of each
   size. Host noise only adds time, so the minimum is the best estimate of the code's cost.
3. Keep the bound 2.5 and the two sizes. Assert the 50k minimum is at least 5 ms of CPU, so
   the ratio is never taken over timer grain.
4. The same-run C row in the same file keeps wall-clock medians; it is not this change.

## Rows, predicted

- Old row, quiet two-CPU slot: green nearly always.
- Old row, same slot beside a bursty CPU hog: red in a clear share of runs (predicted: at
  least one run in ten).
- New row, quiet: green every run, ratio near 2.0-2.2.
- New row, beside the same hog: green every run of a loop at least as long as the old row's.
- Plant (one hunk in `wolf_rt`'s `key_hash`: every key hashes to slot 0, so each probe walks
  the table; answers stay right, cost goes quadratic): the new row red by name with a ratio
  near 4, quiet and beside the hog. The old row is red on it too.
- If the new row is ever red beside the hog on linear code, the fix is not done; the note
  will say what was seen and what changed.
