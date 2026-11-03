#!/usr/bin/env bash
# s278: the C black box on linux, inside r41-toolchain as the lane uid (1000,
# no capabilities: not privileged for chroot), pinned to the block.
set -u
source /lane/rig/env.sh
echo "== uname $(uname -srm); id $(id); $(grep -E 'Cpus_allowed_list|CapEff|SigBlk|SigIgn' /proc/self/status | tr '\n' ' ')"
mkdir -p /lane/tmp/probe && cd /lane/tmp/probe || exit 1
cc --version | head -1
cc -O1 -Wall -o probe /lane/rig/probe.c || exit 1
cp probe /lane/records/probe-linux-x86_64
sha256sum probe /lane/rig/probe.c
./probe
echo "@status $?"
