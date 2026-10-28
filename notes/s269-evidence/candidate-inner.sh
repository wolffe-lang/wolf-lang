set -euo pipefail
lane=/home/mfwolffe/lanes/s269
cargo build --locked --release -p wolf_driver -p wolf_rt -j4
mkdir -p "$lane/candidate"
cp target/release/wolf target/release/libwolf_rt.a "$lane/candidate/"
cp "$lane/yew/target/native/yew" "$lane/baseline/yew-native"
cp "$lane/yew/target/release/yew" "$lane/baseline/yew-release"
sha256sum "$lane/candidate/wolf" "$lane/candidate/libwolf_rt.a" "$lane/baseline/yew-native" "$lane/baseline/yew-release"
sha256sum "$lane/candidate/wolf" "$lane/candidate/libwolf_rt.a"
