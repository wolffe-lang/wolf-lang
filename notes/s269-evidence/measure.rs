// Temporary baseline-only instrumentation; never part of the fix.
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
static OUTPUT: OnceLock<Mutex<std::fs::File>> = OnceLock::new();
pub struct Timer { unit: String, phase: String, started: Instant }
pub fn start(unit: impl Into<String>, phase: impl Into<String>) -> Timer {
    Timer { unit: unit.into(), phase: phase.into(), started: Instant::now() }
}
impl Drop for Timer {
    fn drop(&mut self) {
        let seconds = self.started.elapsed().as_secs_f64();
        let Some(path) = std::env::var_os("S269_MEASURE") else { return; };
        let output = OUTPUT.get_or_init(|| Mutex::new(std::fs::OpenOptions::new().create(true).append(true).open(path).expect("measurement output")));
        let mut output = output.lock().unwrap_or_else(|e| e.into_inner());
        writeln!(output, "{}\t{}\t{seconds:.9}", self.unit.replace(['\t', '\n'], " "), self.phase).expect("measurement record");
    }
}
