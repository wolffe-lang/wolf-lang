//! `wolf` — the single toolchain binary (D34).
//!
//! Eventual surface: build run test bench fmt doc lsp dbg add vendor audit
//! publish fix toolchain. v0 grows at s31 (`wolf build|run`); today the
//! binary anchors the crate graph's top and serves the differential
//! protocol's stub (`conform-run`, spec/06 [proto.invoke]) so the harness
//! and Track 2 target a running contract from day one.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version") => println!("wolf 0.0.1 (pre-alpha)"),
        Some("conform-run") => conform_run(&args[1..]),
        _ => {
            eprintln!("wolf: pre-alpha scaffold; `wolf build|run` lands at sprint s31");
            std::process::exit(2);
        }
    }
}

/// Stub observation record ([proto.record]): no phase is implemented yet,
/// so every program is `unsupported` at `phase_reached: none`. The record
/// is honest — the conservatism ledger shows exactly what the compiler
/// cannot do yet, and the schema/harness are exercised for real.
fn conform_run(args: &[String]) {
    let mut file = None;
    for a in args {
        if a == "--json" || a.starts_with("--phase=") || a.starts_with("--seed=") {
            continue; // accepted per [proto.invoke.cli]; stub ignores them
        }
        if a.starts_with("--") {
            eprintln!("wolf conform-run: unknown flag `{a}`");
            std::process::exit(2);
        }
        file = Some(a.clone());
    }
    let Some(file) = file else {
        eprintln!("usage: wolf conform-run <file.lu> [--phase=<p>] [--seed=N] [--json]");
        std::process::exit(2);
    };
    if !std::path::Path::new(&file).is_file() {
        eprintln!("wolf conform-run: no such file: {file}");
        std::process::exit(2);
    }
    // Hand-built JSON: the schema is tiny, stable, and versioned; a JSON
    // dependency in the driver is not yet warranted.
    println!(
        "{{\"protocol\":1,\"impl\":\"wolfc\",\"impl_version\":\"{}\",\"commit\":\"{}\",\
         \"file\":\"{}\",\"phase_reached\":\"none\",\"seeded\":false,\
         \"diagnostics\":[],\"verdict\":\"unsupported\",\
         \"stdout_sha256\":null,\"stdout_inline\":null}}",
        env!("CARGO_PKG_VERSION"),
        option_env!("WOLF_COMMIT").unwrap_or("unknown"),
        file.replace('\\', "/"),
    );
}
