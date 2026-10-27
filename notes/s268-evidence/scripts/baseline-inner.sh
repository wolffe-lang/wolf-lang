set -euo pipefail
lane=/home/mfwolffe/lanes/s268
cargo build -p wolf_driver -p wolf_rt
cp target/debug/wolf "$lane/baseline/wolf"
cp target/debug/libwolf_rt.a "$lane/baseline/libwolf_rt.a"
sha256sum "$lane/baseline/wolf" "$lane/baseline/libwolf_rt.a"
for tier in native release; do
  flags=()
  if [ "$tier" = release ]; then flags=(--release); fi
  for bench in dense-match token-leaf; do
    target/debug/wolf build "bench/dispatch/$bench/main.lu" "${flags[@]}" -o "$lane/baseline/$bench-$tier"
    for run in 1 2 3; do
      echo "tier=$tier bench=$bench run=$run"
      "$lane/baseline/$bench-$tier"
    done
  done
done
