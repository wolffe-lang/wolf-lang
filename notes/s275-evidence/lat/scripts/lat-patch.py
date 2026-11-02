import sys
p = "crates/wolf_rt/src/signal.rs"
s = open(p).read()
old = """            let t0 = std::time::Instant::now();
            assert_eq!(wait_fd(r, meaning::USER), meaning::USER);
            if t0.elapsed() > Duration::from_millis(50) {
                slow += 1;
            }"""
new = """            let t0 = std::time::Instant::now();
            assert_eq!(wait_fd(r, meaning::USER), meaning::USER);
            let took = t0.elapsed();
            // s275 measurement only (never pushed): each wait's latency.
            if std::env::var_os("WOLF_S275_LAT").is_some() {
                eprintln!("S275_LAT us={}", took.as_micros());
            }
            if took > Duration::from_millis(50) {
                slow += 1;
            }"""
assert s.count(old) == 1, "pattern"
open(p, "w").write(s.replace(old, new))
