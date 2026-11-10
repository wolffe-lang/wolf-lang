use std::os::unix::process::CommandExt;
fn main() {
    let mut c = std::process::Command::new("true");
    c.setsid(true);
    let _ = c.status();
}
