set -euo pipefail
lane=/home/mfwolffe/lanes/s269
cd "$lane/yew"
export WOLF_RT_LIB="$lane/baseline/libwolf_rt.a"
printf 's269 after-native %s\n' "$(date -u +%FT%TZ)"
"$lane/candidate/wolf" build --no-cache --deny-warnings --timings src/yew.lu -o target/native/yew
cmp "$lane/baseline/yew-native" target/native/yew
sha256sum "$lane/baseline/yew-native" target/native/yew
printf 's269 native observer produces byte-identical yew binary\n'
