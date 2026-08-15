//! Locating, spawning and talking to an importer worker.
//!
//! # Why the worker is a separate program (the D33 firewall)
//!
//! D17 names libclang as the v0 bootstrap. D33 says wolf runs no build
//! scripts, ever — and the `clang-sys`/`bindgen` route into libclang is
//! a build script whose job is to go looking around the host for a
//! shared library. Linking libclang into the compiler would put a
//! probe-the-host build script inside the very binary that sells "your
//! dependencies cannot run code on your machine".
//!
//! The two are reconciled by the boundary the contract already draws:
//! **the worker is a separate executable and the compiler never links
//! it.** Whatever a worker needs to be built — libclang, a build
//! script, a different language — is that worker's business, on the
//! other side of a process boundary, exactly as `wolf_codegen_llvm`
//! already treats the system LLVM (a named toolchain requirement, never
//! a build script). Nothing in this crate depends on clang, and the
//! conformance suite runs against the *interface*, so the day c15's
//! embedded frontend arrives it drops in here unchanged.
//!
//! # How a worker is found
//!
//! In order, first hit wins:
//!
//! 1. `$WOLF_CIMPORT_WORKER` — an explicit path. Set by CI and by
//!    anyone pinning a worker.
//! 2. `wolf-cimport-worker` next to the running `wolf` binary — how a
//!    toolchain ships one.
//! 3. `wolf-cimport-worker` on `PATH`.
//!
//! There is no fourth step, and in particular there is no "look around
//! the host for something clang-shaped". A missing worker is an honest
//! refusal naming all three places, not a silent fallback to a guess.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::artifact::Artifact;
use crate::cache::{Cache, ImportRequest};
use crate::encode;
use crate::protocol::{Request, Response};

/// The conventional worker program name.
pub const WORKER_NAME: &str = "wolf-cimport-worker";

/// The environment variable that pins a worker explicitly.
pub const WORKER_ENV: &str = "WOLF_CIMPORT_WORKER";

/// Why an import could not happen at all. Distinct from a *refused
/// declaration*, which is a fact recorded in a successful artifact.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ImportError {
    /// No worker anywhere. Carries the places that were looked.
    NoWorker { looked: Vec<String> },
    /// The worker could not be started.
    Spawn { program: String, err: String },
    /// The worker broke the protocol.
    Protocol(String),
    /// The worker answered `err`.
    Worker(String),
    /// The worker's artifact did not decode.
    Artifact(String),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::NoWorker { looked } => {
                write!(
                    f,
                    "no C header importer worker found. `import c` needs the \
                     `{WORKER_NAME}` program, which the compiler runs as a separate \
                     process and never links. Looked at: {}",
                    looked.join(", ")
                )
            }
            ImportError::Spawn { program, err } => {
                write!(f, "could not run the importer worker `{program}`: {err}")
            }
            ImportError::Protocol(m) => write!(f, "the importer worker broke the protocol: {m}"),
            ImportError::Worker(m) => write!(f, "the importer worker refused the import: {m}"),
            ImportError::Artifact(m) => {
                write!(f, "the importer worker's artifact did not decode: {m}")
            }
        }
    }
}

/// A located worker program.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Worker {
    pub program: PathBuf,
    /// Where it was found — reported so a surprising worker is a
    /// visible fact rather than a mystery.
    pub found_via: &'static str,
}

impl Worker {
    /// Find a worker. `self_exe` is the running `wolf` binary's path,
    /// when known (the driver passes `std::env::current_exe()`).
    pub fn find(self_exe: Option<&Path>) -> Result<Worker, ImportError> {
        let pinned = std::env::var(WORKER_ENV).ok().filter(|p| !p.is_empty());
        Worker::find_with(pinned.as_deref(), self_exe)
    }

    /// [`Worker::find`] with the pin passed in rather than read from the
    /// environment. Environment variables are process-global, so a test
    /// that set one would change what every *other* test in the binary
    /// sees; this is the seam that lets the search be tested without
    /// that.
    pub fn find_with(pinned: Option<&str>, self_exe: Option<&Path>) -> Result<Worker, ImportError> {
        let mut looked = Vec::new();

        looked.push(format!("${WORKER_ENV}"));
        if let Some(p) = pinned.filter(|p| !p.is_empty()) {
            let path = PathBuf::from(p);
            if path.is_file() {
                return Ok(Worker {
                    program: path,
                    found_via: WORKER_ENV,
                });
            }
            // An explicitly pinned worker that is not there is an
            // error worth naming precisely: silently falling through
            // to PATH would run a different program than was asked for.
            return Err(ImportError::Spawn {
                program: p.to_string(),
                err: format!("${WORKER_ENV} points at something that is not a file"),
            });
        }

        if let Some(exe) = self_exe
            && let Some(dir) = exe.parent()
        {
            let cand = dir.join(exe_name());
            looked.push(cand.display().to_string());
            if cand.is_file() {
                return Ok(Worker {
                    program: cand,
                    found_via: "the toolchain directory",
                });
            }
        }

        looked.push(format!("`{WORKER_NAME}` on PATH"));
        if let Some(p) = which(&exe_name()) {
            return Ok(Worker {
                program: p,
                found_via: "PATH",
            });
        }

        Err(ImportError::NoWorker { looked })
    }

    /// The worker's identity for the cache key. The path is
    /// deliberately *not* part of it — two toolchains with the same
    /// worker version should share cache entries — but the version is,
    /// so a worker upgrade re-imports.
    pub fn identity(&self) -> String {
        match self.version() {
            Some(v) => v,
            // A worker that will not say what it is gets keyed by path:
            // pessimistic, and never wrong in the dangerous direction.
            None => format!("unversioned:{}", self.program.display()),
        }
    }

    /// [`Worker::identity`], memoized on disk against the program's
    /// path, size and mtime.
    ///
    /// This exists so a **hot build spawns nothing at all**. Asking the
    /// worker its version is a process, and a `--version` on every
    /// import would mean the D7 promise ("an incremental build
    /// re-imports nothing") still paid a process per header set. A
    /// worker that is rebuilt or replaced changes size or mtime, so the
    /// memo re-probes exactly when it must.
    pub fn identity_cached(&self, cache: &Cache) -> String {
        let Some(stamp) = self.stamp() else {
            return self.identity();
        };
        let memo = cache
            .root()
            .join("workers")
            .join(format!("{}.id", blake3::hash(stamp.as_bytes()).to_hex()));
        if let Ok(v) = std::fs::read_to_string(&memo) {
            let v = v.trim().to_string();
            if !v.is_empty() {
                return v;
            }
        }
        let id = self.identity();
        if let Some(dir) = memo.parent()
            && std::fs::create_dir_all(dir).is_ok()
        {
            // Best effort: a read-only cache costs a `--version` per
            // build, never a wrong answer.
            let _ = std::fs::write(&memo, &id);
        }
        id
    }

    /// A cheap fingerprint of the program file: path, size, mtime.
    fn stamp(&self) -> Option<String> {
        let m = std::fs::metadata(&self.program).ok()?;
        let mtime = m
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_nanos();
        Some(format!("{}|{}|{mtime}", self.program.display(), m.len()))
    }

    /// Ask the worker for its version string (`--version`, one line).
    fn version(&self) -> Option<String> {
        let out = Command::new(&self.program).arg("--version").output().ok()?;
        if !out.status.success() {
            return None;
        }
        let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!v.is_empty()).then_some(v)
    }

    /// Run one request/response exchange.
    pub fn ask(&self, req: &Request) -> Result<Response, ImportError> {
        let mut child = Command::new(&self.program)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ImportError::Spawn {
                program: self.program.display().to_string(),
                err: e.to_string(),
            })?;

        let wire = req.render();
        {
            let stdin = child.stdin.as_mut().ok_or_else(|| ImportError::Spawn {
                program: self.program.display().to_string(),
                err: "no stdin pipe".to_string(),
            })?;
            stdin
                .write_all(wire.as_bytes())
                .map_err(|e| ImportError::Protocol(format!("writing the request: {e}")))?;
        }
        // Dropping stdin closes it, which is the worker's EOF.
        drop(child.stdin.take());

        let mut out = Vec::new();
        if let Some(mut so) = child.stdout.take() {
            so.read_to_end(&mut out)
                .map_err(|e| ImportError::Protocol(format!("reading the answer: {e}")))?;
        }
        let mut errbuf = String::new();
        if let Some(mut se) = child.stderr.take() {
            let _ = se.read_to_string(&mut errbuf);
        }
        let status = child
            .wait()
            .map_err(|e| ImportError::Protocol(format!("waiting for the worker: {e}")))?;

        if out.is_empty() {
            let why = if errbuf.trim().is_empty() {
                format!("it exited {status} without saying anything")
            } else {
                format!("it exited {status}: {}", errbuf.trim())
            };
            return Err(ImportError::Protocol(why));
        }
        Response::parse(&out).map_err(ImportError::Protocol)
    }
}

/// The result of an import, and whether it came from the cache.
#[derive(Clone, PartialEq, Debug)]
pub struct Imported {
    pub artifact: Artifact,
    pub key: String,
    /// `true` when no worker process was spawned. The D7 discipline
    /// this sprint is held to: a second build with unchanged inputs
    /// must not start a worker, and a test asserts exactly this.
    pub from_cache: bool,
}

/// Import a header set, consulting the cache first.
///
/// A cache hit **spawns nothing**: no worker process runs, which is the
/// D7 discipline this sprint is held to.
///
/// It does not yet mean a cached build needs no worker *installed*. The
/// key includes the importer's identity — deliberately, so a bootstrap
/// worker's answers are never inherited by its replacement — and
/// obtaining that identity means asking the worker unless the caller
/// already knows it. `importer_hint` is the way to already know it; the
/// place that should supply it is the lockfile, which would let a
/// machine with no C toolchain build from vendored artifacts. That
/// plumbing is not in this sprint, and pretending otherwise would be
/// the kind of "works, but" claim this crate exists to avoid.
pub fn import(
    req: &ImportRequest,
    cache: &Cache,
    self_exe: Option<&Path>,
    importer_hint: Option<&str>,
) -> Result<Imported, ImportError> {
    // The key needs the worker's identity, which normally means asking
    // it. `importer_hint` lets a caller that already knows (the
    // conformance runner, a lockfile) skip the probe entirely.
    let (identity, worker) = match importer_hint {
        Some(h) => (h.to_string(), None),
        None => {
            let w = Worker::find(self_exe)?;
            // Memoized: a hot build must not spawn even a `--version`.
            let id = w.identity_cached(cache);
            (id, Some(w))
        }
    };
    let key = req.key(&identity);

    if let Some(bytes) = cache.get(&key) {
        // A corrupt or stale-format entry falls through and re-imports.
        // Artifacts are reproducible; there is never a reason to reason
        // about a damaged cache instead of rebuilding it.
        if let Ok(artifact) = encode::decode(&bytes) {
            return Ok(Imported {
                artifact,
                key,
                from_cache: true,
            });
        }
    }

    let worker = match worker {
        Some(w) => w,
        None => Worker::find(self_exe)?,
    };
    let resp = worker.ask(&Request::Import(req.clone()))?;
    let bytes = match resp {
        Response::Artifact(b) => b,
        Response::Err(m) => return Err(ImportError::Worker(m)),
        Response::Tokens(_) => {
            return Err(ImportError::Protocol(
                "answered an import with macro tokens".to_string(),
            ));
        }
    };
    let artifact = encode::decode(&bytes).map_err(|e| ImportError::Artifact(e.to_string()))?;

    // A best-effort cache write: a read-only or full cache directory
    // makes builds slower, never wrong.
    let _ = cache.put(&key, &bytes);

    Ok(Imported {
        artifact,
        key,
        from_cache: false,
    })
}

fn exe_name() -> String {
    if cfg!(windows) {
        format!("{WORKER_NAME}.exe")
    } else {
        WORKER_NAME.to_string()
    }
}

/// A `which` that does not shell out (and so has no unixisms and no
/// quoting hazards).
fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(name))
        .find(|c| c.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit;

    /// The refusal a user meets when no worker is installed has to say
    /// where to put one. "not found" with no next step is how a feature
    /// becomes folklore.
    #[test]
    fn a_missing_worker_names_every_place_it_looked() {
        let e = ImportError::NoWorker {
            looked: vec![
                "$WOLF_CIMPORT_WORKER".into(),
                "/opt/wolf/bin/wolf-cimport-worker".into(),
                "`wolf-cimport-worker` on PATH".into(),
            ],
        };
        let text = e.to_string();
        assert!(text.contains(WORKER_ENV), "{text}");
        assert!(text.contains("PATH"), "{text}");
        assert!(
            text.contains("never links"),
            "the refusal should say why the worker is a separate process: {text}"
        );
    }

    /// A cached import must not need a worker at all — the property
    /// that makes vendored artifacts work on a machine with no C
    /// toolchain, and the D7 no-work-on-rebuild discipline.
    #[test]
    fn a_cached_import_spawns_nothing() {
        let dir = std::env::temp_dir().join(format!("wolf-cimport-w-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cache = Cache::new(&dir);
        let req = testkit::sample_request();
        let key = req.key("test-worker 1");
        cache
            .put(&key, &crate::encode::encode(&testkit::sample_artifact()))
            .expect("writes");

        // No worker exists under this name, and none is needed: if the
        // cache were consulted after locating a worker, this would fail
        // with NoWorker.
        let got = import(&req, &cache, None, Some("test-worker 1")).expect("cache hit");
        assert!(got.from_cache);
        assert_eq!(got.key, key);
        assert_eq!(got.artifact, testkit::sample_artifact());

        std::fs::remove_dir_all(&dir).expect("cleans up");
    }

    /// A cache entry written by a different worker must not be handed
    /// to this one.
    #[test]
    fn a_different_worker_does_not_inherit_cached_answers() {
        let dir = std::env::temp_dir().join(format!("wolf-cimport-w2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cache = Cache::new(&dir);
        let req = testkit::sample_request();
        cache
            .put(
                &req.key("libclang-worker 1"),
                &crate::encode::encode(&testkit::sample_artifact()),
            )
            .expect("writes");

        // Asking as c15's frontend must miss, and therefore try to find
        // a worker (which is absent here) rather than return the
        // bootstrap worker's answer.
        let e = import(&req, &cache, None, Some("wolf-c-frontend 1")).expect_err("must miss");
        assert!(matches!(e, ImportError::NoWorker { .. }), "{e:?}");

        std::fs::remove_dir_all(&dir).expect("cleans up");
    }

    /// A worker pinned explicitly that is not there must be an error,
    /// never a quiet fallback to a different program: running something
    /// other than the worker that was asked for is worse than running
    /// nothing.
    #[test]
    fn a_pinned_worker_that_is_missing_does_not_fall_through() {
        let e =
            Worker::find_with(Some("/definitely/not/here/worker"), None).expect_err("must refuse");
        match e {
            ImportError::Spawn { program, err } => {
                assert_eq!(program, "/definitely/not/here/worker");
                assert!(err.contains(WORKER_ENV), "{err}");
            }
            other => panic!("expected a spawn error, got {other:?}"),
        }
    }

    /// An empty pin is the same as no pin — an unset variable that some
    /// shell exported as `""` must not become a refusal.
    #[test]
    fn an_empty_pin_falls_through_to_the_ordinary_search() {
        // No `self_exe` and (in the test binary) no worker on PATH, so
        // the ordinary search runs to its end and reports NoWorker
        // rather than treating "" as a pinned path.
        match Worker::find_with(Some(""), None) {
            Err(ImportError::NoWorker { looked }) => {
                assert!(looked.iter().any(|l| l.contains("PATH")), "{looked:?}");
            }
            // A machine that genuinely has one on PATH is also a pass:
            // what must not happen is a Spawn error about "".
            Ok(_) => {}
            Err(other) => panic!("an empty pin must not be treated as a path: {other:?}"),
        }
    }
}
