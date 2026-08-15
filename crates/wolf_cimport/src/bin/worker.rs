//! `wolf-cimport-worker` — the reference importer worker.
//!
//! Reads one request from stdin, writes one answer to stdout, exits.
//! `--version` prints the identity the compiler folds into the cache
//! key, which is how a worker upgrade invalidates cached artifacts
//! instead of being trusted by its successor.
//!
//! This program is deliberately separate from the `wolf` binary. See
//! `wolf_cimport`'s crate docs: the process boundary is what lets a C
//! importer exist without a build script inside the compiler (D33).

use std::io::{Read, Write};

use wolf_cimport::protocol::{Request, Response};
use wolf_cimport::refworker::{DiskHeaders, REFERENCE_WORKER_ID, serve};

/// Set to a file path to have the worker append one line **per
/// invocation**, including `--version` probes.
///
/// This exists for one test: the D7 promise is that a rebuild with
/// unchanged inputs spawns no worker process, and the only way to
/// assert "no process ran" without guessing is to have the process say
/// so when it does. It is inert unless the variable is set.
const TRACE_ENV: &str = "WOLF_CIMPORT_WORKER_TRACE";

fn trace(what: &str) {
    let Ok(path) = std::env::var(TRACE_ENV) else {
        return;
    };
    if path.is_empty() {
        return;
    }
    use std::io::Write as _;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(f, "{what}");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    trace(args.first().map(String::as_str).unwrap_or("import"));
    match args.first().map(String::as_str) {
        Some("--version") => {
            println!("{REFERENCE_WORKER_ID}");
            return;
        }
        Some("--help") | Some("-h") => {
            println!(
                "usage: wolf-cimport-worker [--version]\n\
                 \n\
                 Speaks the wolf c-import protocol on stdin/stdout. It is run by\n\
                 the compiler, not by you."
            );
            return;
        }
        Some(other) => {
            eprintln!("wolf-cimport-worker: unknown argument `{other}`");
            std::process::exit(2);
        }
        None => {}
    }

    let mut input = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut input) {
        emit(&Response::Err(format!("could not read the request: {e}")));
        std::process::exit(1);
    }

    let resp = match Request::parse(&input) {
        Ok(req) => serve(&req, &DiskHeaders),
        Err(e) => Response::Err(e),
    };
    let failed = matches!(resp, Response::Err(_));
    emit(&resp);
    if failed {
        std::process::exit(1);
    }
}

fn emit(r: &Response) {
    let bytes = r.render();
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(&bytes);
    let _ = out.flush();
}
