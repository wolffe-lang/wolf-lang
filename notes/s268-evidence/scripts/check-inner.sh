set -euo pipefail
cargo fmt --all
cargo test -p wolf_codegen_clif --test dense_match -- --nocapture
cargo test -p wolf_wir --test token_inline -- --nocapture
cargo test -p wolf_driver --test dense_match_lanes -- --nocapture
cargo build -p wolf_driver
lane=/home/mfwolffe/lanes/s268
mkdir -p "$lane/after"
for tier in native release; do
  flags=()
  if [ "$tier" = release ]; then flags=(--release); fi
  for bench in dense-match token-leaf; do
    target/debug/wolf build --no-cache "bench/dispatch/$bench/main.lu" "${flags[@]}" -o "$lane/after/$bench-$tier"
    for run in 1 2 3; do
      echo "tier=$tier bench=$bench run=$run"
      "$lane/after/$bench-$tier"
    done
  done
done
