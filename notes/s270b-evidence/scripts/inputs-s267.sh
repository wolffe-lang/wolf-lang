#!/usr/bin/env bash
# s270b: the s267 musl link bundle (static-entry sysroot), by copy from s270's verified
# inputs, its digest read twice against the one r41 banked, unpacked under s270b's inputs.
set -euo pipefail
base=/home/mfwolffe/lanes/s270b
mkdir -p "$base/inputs/s267/unpacked"
cp /home/mfwolffe/lanes/s270/inputs/s267/ld02-musl-link-bundle.tar.gz "$base/inputs/s267/"
cd "$base/inputs/s267"
echo "dd09895a13d3ea4056db73d3e974f025a732391c38044094ea6fc8ed69fb54cc  ld02-musl-link-bundle.tar.gz" | sha256sum -c -
echo "dd09895a13d3ea4056db73d3e974f025a732391c38044094ea6fc8ed69fb54cc  ld02-musl-link-bundle.tar.gz" | sha256sum -c -
tar -xzf ld02-musl-link-bundle.tar.gz -C unpacked
ls unpacked
diff -r -q unpacked /home/mfwolffe/lanes/s270/inputs/s267/unpacked && echo "unpacked tree identical to s270's"
