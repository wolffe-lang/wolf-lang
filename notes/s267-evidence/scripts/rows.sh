#!/bin/bash
# s267: the static-entry rows at the checkout's head, in ctr-u. $1 names the
# table directory under evidence/; $2 is a musl libwolf_rt.a to link (default:
# a debug build of this checkout's wolf_rt for x86_64-unknown-linux-musl).
set -u
L=/home/mfwolffe/lanes/s267
cd $L/wolf-lang
echo "rows head=$(git rev-parse HEAD) host=$(hostname) start=$(date -u +%FT%TZ)"
grep -E 'Cpus_allowed_list|SigBlk|SigIgn|NoNewPrivs' /proc/self/status
printf 'pid 1: '; grep '^Uid:' /proc/1/status
ld.lld --version; ld.bfd --version | head -1; clang --version | head -1; cc --version | head -1
cargo build --locked -p wolf_rt || exit 3
rt=${2:-}
if [ -z "$rt" ]; then
  cargo build --locked -p wolf_rt --target x86_64-unknown-linux-musl || exit 3
  rt=$CARGO_TARGET_DIR/x86_64-unknown-linux-musl/debug/libwolf_rt.a
fi
sha256sum "$rt"; sha256sum "$rt"
export WOLF_STATIC_ENTRY_SYSROOT=$L/wolf-lang/target/static-entry-bundle/sysroot
export WOLF_STATIC_ENTRY_MUSL_RT=$rt WOLF_STATIC_ENTRY_REQUIRE=1 WOLF_STATIC_ENTRY_TABLE=$L/evidence/$1
rm -rf "$WOLF_STATIC_ENTRY_TABLE"
cargo test --locked -p wolf_driver --test static_entry -- --nocapture --test-threads=1
rc=$?
echo "rows exit=$rc end=$(date -u +%FT%TZ)"
exit $rc
