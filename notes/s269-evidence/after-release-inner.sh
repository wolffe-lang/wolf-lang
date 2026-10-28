set -euo pipefail
lane=/home/mfwolffe/lanes/s269
cd "$lane/yew"
export WOLF_RT_LIB="$lane/baseline/libwolf_rt.a"
printf 's269 after-release %s\n' "$(date -u +%FT%TZ)"
"$lane/candidate/wolf" build --release --no-cache --deny-warnings --timings src/yew.lu -o target/release/yew
cmp "$lane/baseline/yew-release" target/release/yew
sha256sum "$lane/baseline/yew-release" target/release/yew
printf 's269 release observer produces byte-identical yew binary\n'
