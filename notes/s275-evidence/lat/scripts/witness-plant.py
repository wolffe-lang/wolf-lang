# s275: the local witness plant for R-s275-6 (measurement only, never pushed).
p = "crates/wolf_rt/src/signal.rs"
s = open(p).read()
old = """    pub fn wait_fds(handles: &[i64], want: i64, deadline_ms: i64) -> Result<(i64, i64), i64> {
"""
new = """    pub fn wait_fds(handles: &[i64], want: i64, deadline_ms: i64) -> Result<(i64, i64), i64> {
        // WITNESS PLANT (s275, never pushed): WOLF_S275_PLANT_POLL_MS=N makes
        // the wait answer no sooner than N ms after it began, as a fallback
        // timer (N = 100) or a reinstated poll period (N = 30) would.
        let plant_t0 = std::time::Instant::now();
        let plant_floor = std::env::var("WOLF_S275_PLANT_POLL_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(std::time::Duration::from_millis);
"""
assert s.count(old) == 1; s = s.replace(old, new)
old = """        drop(files);
        out
    }
"""
new = """        drop(files);
        if let Some(floor) = plant_floor {
            let el = plant_t0.elapsed();
            if el < floor {
                std::thread::sleep(floor - el);
            }
        }
        out
    }
"""
assert s.count(old) == 1; s = s.replace(old, new)
old = """            let t0 = std::time::Instant::now();
            assert_eq!(wait_fd(r, meaning::USER), meaning::USER);
            took.push(t0.elapsed());"""
new = """            let t0 = std::time::Instant::now();
            // WITNESS PLANT (s275, never pushed): WOLF_S275_PLANT_STALLS=k
            // stalls the first k rounds 200 ms inside the timed interval.
            let stalls: usize = std::env::var("WOLF_S275_PLANT_STALLS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            if took.len() < stalls {
                std::thread::sleep(Duration::from_millis(200));
            }
            assert_eq!(wait_fd(r, meaning::USER), meaning::USER);
            took.push(t0.elapsed());"""
assert s.count(old) == 1; s = s.replace(old, new)
open(p, "w").write(s)
