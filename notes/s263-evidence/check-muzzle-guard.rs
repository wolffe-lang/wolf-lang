#[path = "../../crates/wolf_backend/tests/support/variadic_guard.rs"]
mod variadic_guard;
fn main() {
    let mut total = 0;
    for path in std::env::args().skip(1) {
        let text = std::fs::read_to_string(&path).expect("read objdump");
        let count = variadic_guard::check(&text).unwrap_or_else(|e| panic!("{path}: {e}"));
        println!("{path}: {count} guarded XMM saves; no unguarded vector state");
        total += count;
    }
    println!("total guarded XMM saves: {total}; objdump proof, not QEMU fault execution");
}
