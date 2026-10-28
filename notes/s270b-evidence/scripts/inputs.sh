#!/usr/bin/env bash
# s270b: copy the two release archives (by copy, from s270's verified inputs), check
# them against the digests GitHub reports, unpack them under s270b's own inputs.
set -euo pipefail
base=/home/mfwolffe/lanes/s270b
mkdir -p "$base/inputs/lupin" "$base/inputs/wolf035"
cp /home/mfwolffe/lanes/s270/inputs/lupin/lupin-0.1.58-x86_64-unknown-linux-gnu.tar.gz "$base/inputs/lupin/"
cp /home/mfwolffe/lanes/s270/inputs/wolf/wolf-0.2.35-x86_64-unknown-linux-gnu.tar.gz "$base/inputs/wolf035/"
cd "$base/inputs"
echo "34f52792bb59bc7565e0edc1235cd47d9a6ad13d5e6528993627f2e699b62193  lupin/lupin-0.1.58-x86_64-unknown-linux-gnu.tar.gz" | sha256sum -c -
echo "34f52792bb59bc7565e0edc1235cd47d9a6ad13d5e6528993627f2e699b62193  lupin/lupin-0.1.58-x86_64-unknown-linux-gnu.tar.gz" | sha256sum -c -
echo "4cc4045bbbb605c07835bc4f32685953c784eeef4cd3139a48b457dfc90dad1a  wolf035/wolf-0.2.35-x86_64-unknown-linux-gnu.tar.gz" | sha256sum -c -
echo "4cc4045bbbb605c07835bc4f32685953c784eeef4cd3139a48b457dfc90dad1a  wolf035/wolf-0.2.35-x86_64-unknown-linux-gnu.tar.gz" | sha256sum -c -
tar -xzf lupin/lupin-0.1.58-x86_64-unknown-linux-gnu.tar.gz -C lupin
tar -xzf wolf035/wolf-0.2.35-x86_64-unknown-linux-gnu.tar.gz -C wolf035
ls lupin/* wolf035/*
cp /home/mfwolffe/lanes/r41/scripts/taskset-shim.c "$base/scripts/taskset-shim.c"
sha256sum "$base/scripts/taskset-shim.c" /home/mfwolffe/lanes/r41/scripts/taskset-shim.c
