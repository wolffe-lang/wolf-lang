//! `wolf c-import` — drive the C header importer (s46, c10).
//!
//! Two jobs. It imports a header set and prints the artifact's dump
//! (`--dump`), which is the form the conformance suite snapshots and
//! the form a human reads to find out **what the compiler actually
//! believes about a header** — including, especially, what it refused.
//! And it is the escape hatch for diagnosing an import without a wolf
//! program around it.
//!
//! The verb never links a C frontend. It locates an importer worker
//! (see `wolf_cimport::worker` for the search order and why the
//! boundary is a process), or reports honestly that there is none.

use std::path::PathBuf;

use wolf_cimport::cache::{Cache, ImportRequest};
use wolf_cimport::worker::{self, Worker};

struct Cli {
    headers: Vec<String>,
    include: Vec<String>,
    defines: Vec<(String, String)>,
    cflags: Vec<String>,
    target: String,
    sysroot: Option<String>,
    dump: bool,
    no_cache: bool,
    /// Print only the refusals — the `wolf audit` view of an import.
    refusals_only: bool,
}

fn usage() -> ! {
    eprintln!(
        "usage: wolf c-import [options] <header.h>…\n\
         \n\
         options:\n\
         \x20 --dump              print the artifact (the reviewable form)\n\
         \x20 --refusals          print only what the importer refused\n\
         \x20 -I <dir>            add an include directory (repeatable, order matters)\n\
         \x20 -D <name>[=<value>] define a macro (repeatable)\n\
         \x20 --cflag <flag>      pass a flag to the importer (repeatable)\n\
         \x20 --target <triple>   import for this target (default: the host)\n\
         \x20 --sysroot <id>      the sysroot identity to key the cache on\n\
         \x20 --no-cache          import even if a cached artifact exists\n\
         \n\
         The importer runs as a separate program (`{}`); the compiler\n\
         never links a C frontend.",
        worker::WORKER_NAME
    );
    std::process::exit(2)
}

fn parse_cli(args: &[String]) -> Cli {
    let mut c = Cli {
        headers: Vec::new(),
        include: Vec::new(),
        defines: Vec::new(),
        cflags: Vec::new(),
        target: default_target(),
        sysroot: None,
        dump: false,
        no_cache: false,
        refusals_only: false,
    };
    let mut i = 0;
    let next = |i: &mut usize, what: &str| -> String {
        *i += 1;
        match args.get(*i) {
            Some(v) => v.clone(),
            None => {
                eprintln!("wolf c-import: `{what}` needs a value");
                std::process::exit(2);
            }
        }
    };
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "--dump" => c.dump = true,
            "--refusals" => c.refusals_only = true,
            "--no-cache" => c.no_cache = true,
            "--help" | "-h" => usage(),
            "-I" => c.include.push(next(&mut i, "-I")),
            "-D" => c.defines.push(split_define(&next(&mut i, "-D"))),
            "--cflag" => c.cflags.push(next(&mut i, "--cflag")),
            "--target" => c.target = next(&mut i, "--target"),
            "--sysroot" => c.sysroot = Some(next(&mut i, "--sysroot")),
            _ if a.starts_with("-I") => c.include.push(a[2..].to_string()),
            _ if a.starts_with("-D") => c.defines.push(split_define(&a[2..])),
            _ if a.starts_with("--target=") => c.target = a[9..].to_string(),
            _ if a.starts_with('-') && a.len() > 1 => {
                eprintln!("wolf c-import: unknown flag `{a}`");
                std::process::exit(2);
            }
            _ => c.headers.push(a.clone()),
        }
        i += 1;
    }
    if c.headers.is_empty() {
        usage();
    }
    c
}

fn split_define(s: &str) -> (String, String) {
    match s.split_once('=') {
        Some((k, v)) => (k.to_string(), v.to_string()),
        None => (s.to_string(), String::new()),
    }
}

/// The host triple, in the spellings the importer parameterizes on.
fn default_target() -> String {
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86_64"
    };
    if cfg!(target_os = "macos") {
        return format!("{arch}-apple-darwin");
    }
    if cfg!(target_os = "windows") {
        return format!("{arch}-pc-windows-msvc");
    }
    if cfg!(target_os = "freebsd") {
        return format!("{arch}-unknown-freebsd");
    }
    format!("{arch}-unknown-linux-gnu")
}

pub fn c_import(args: &[String]) {
    let cli = parse_cli(args);

    let req = ImportRequest {
        headers: cli.headers.clone(),
        defines: cli.defines.clone(),
        cflags: cli.cflags.clone(),
        include_paths: cli.include.clone(),
        target: cli.target.clone(),
        sysroot: cli.sysroot.clone(),
    };

    let cache_root = match cache_root() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("wolf c-import: {e}");
            std::process::exit(2);
        }
    };
    let cache = Cache::new(&cache_root);

    // `--no-cache` still writes: it is "do the work again", the
    // determinism oracle, not "forget everything".
    let self_exe = std::env::current_exe().ok();
    let result = if cli.no_cache {
        match Worker::find(self_exe.as_deref()) {
            Ok(w) => {
                let id = w.identity();
                let key = req.key(&id);
                match w.ask(&wolf_cimport::Request::Import(req.clone())) {
                    Ok(wolf_cimport::Response::Artifact(bytes)) => {
                        let _ = cache.put(&key, &bytes);
                        wolf_cimport::decode(&bytes)
                            .map(|artifact| worker::Imported {
                                artifact,
                                key,
                                from_cache: false,
                            })
                            .map_err(|e| worker::ImportError::Artifact(e.to_string()))
                    }
                    Ok(wolf_cimport::Response::Err(m)) => Err(worker::ImportError::Worker(m)),
                    Ok(_) => Err(worker::ImportError::Protocol(
                        "answered an import with macro tokens".to_string(),
                    )),
                    Err(e) => Err(e),
                }
            }
            Err(e) => Err(e),
        }
    } else {
        worker::import(&req, &cache, self_exe.as_deref(), None)
    };

    let imported = match result {
        Ok(i) => i,
        Err(e) => {
            eprintln!("wolf c-import: {e}");
            std::process::exit(1);
        }
    };

    let a = &imported.artifact;
    if cli.dump {
        print!("{}", wolf_cimport::dump(a));
        return;
    }

    let (ok, refused) = a.tally();
    if cli.refusals_only {
        for (what, demotion, r) in a.refusals() {
            println!("{what}: {} -> {demotion}", r.tag());
            println!("  {}", r.headline());
            println!("  {}", r.note());
        }
        if refused == 0 {
            println!("(nothing refused — {ok} declarations imported whole)");
        }
        return;
    }

    println!(
        "{} header(s) for {}: {ok} imported, {refused} refused",
        a.headers.len(),
        a.target.triple
    );
    println!("importer: {}", a.importer);
    println!(
        "cache: {} ({})",
        imported.key,
        if imported.from_cache {
            "hit — no worker ran"
        } else {
            "miss — imported now"
        }
    );
    if refused > 0 {
        println!("run `wolf c-import --refusals …` to see what was refused, and why");
    }
}

/// The cimport cache lives under the global cache root, beside the
/// package store — `wolf_pkg` owns that resolution, so ask it.
fn cache_root() -> Result<PathBuf, String> {
    wolf_pkg::source::cache_root().map(|r| r.join("cimport"))
}
