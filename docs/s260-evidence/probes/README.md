# s260 probes

The programs and scripts behind `../*.log`, run on kasumi (linux
x86-64): `run.sh <wolf> <label> <group...>` builds each program in its
own directory on the host (`taskset -c 4-7`), `rows.sh <label> <wolf>
<group...>` runs every build, the checked machine and lupin 0.1.55 inside
an `archlinux:base-devel` container on cpus 0–3 (`exec.sh`, `SigBlk` and
`SigIgn` printed first). The groups: `copy/` (here: the read-only sink and
`/dev/full` by handle; t06's witness is
`crates/wolf_driver/tests/fixtures/copy_write/witness.lu`), `spawn/`
(here: every spawn of the family; t06's witness is its `682` input),
`spawn2/` (the fixtures under `fixtures/spawn_noexec/`), `kill/` (the
fixtures under `fixtures/kill_park/`, without their headers). The
first control run (`v034-rows.log`) carried two malformed kill probes —
`signal_wait_fds` waited on an empty set and `select_helper` had a
timeout arm the checked machine and lupin fire at once — and
`v034-rows-k2.log` is their corrected run.
