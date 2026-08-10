//! The host/snapshot split and the cancellation architecture (contract
//! clause 3 — frozen for s57).
//!
//! Shape (rust-analyzer's `AnalysisHost`/`Analysis`, report 09):
//! [`QueryHost::apply_change`] cancels every outstanding [`Snapshot`]
//! and blocks until they drop; readers observe the mark at checkpoints
//! and unwind with a private sentinel; the unwind is caught at exactly
//! one layer (the public query entries in `queries.rs`, via
//! [`Snapshot::guard`]) and converted to `Err(Cancelled)`.

use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, Weak};
use std::time::Duration;

use crate::overlay::OverlayStore;
use crate::queries::PackageAnalysis;

/// A query was cancelled — by a write ([`QueryHost::apply_change`]) or
/// by the client ([`CancelToken::cancel`]). Also the (private-by-
/// convention) unwind payload; see the crate docs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the query was cancelled")
    }
}

impl std::error::Error for Cancelled {}

/// One overlay mutation (contract clause 1). Full-text only: composing
/// incremental editor deltas into text is the transport shim's job
/// (it owns position encodings; this crate never sees them).
#[derive(Debug, Clone)]
pub enum Change {
    /// A buffer opened: `path` now reads as `text` regardless of disk.
    Open { path: PathBuf, text: Vec<u8> },
    /// An open buffer changed (unsaved edits included).
    Edit { path: PathBuf, text: Vec<u8> },
    /// The buffer closed: `path` reads from disk again.
    Close { path: PathBuf },
}

/// Per-snapshot control block: the cancellation mark, shared with
/// [`CancelToken`]s and the host's live-snapshot registry.
#[derive(Debug, Default)]
struct SnapshotCtl {
    cancelled: AtomicBool,
}

/// Registry of outstanding snapshots + the condvar `apply_change`
/// blocks on until they all drop.
#[derive(Debug, Default)]
struct LiveSnapshots {
    inner: Mutex<Vec<Weak<SnapshotCtl>>>,
    emptied: Condvar,
}

impl LiveSnapshots {
    fn register(&self, ctl: &Arc<SnapshotCtl>) {
        self.lock().push(Arc::downgrade(ctl));
    }

    fn unregister(&self, ctl: &Arc<SnapshotCtl>) {
        let mut live = self.lock();
        live.retain(|w| w.upgrade().is_some_and(|a| !Arc::ptr_eq(&a, ctl)));
        if live.is_empty() {
            self.emptied.notify_all();
        }
    }

    /// Mark every live snapshot cancelled, then block until each has
    /// dropped (writer priority — the s57-frozen semantics).
    fn cancel_all_and_wait(&self) {
        let mut live = self.lock();
        for w in live.iter() {
            if let Some(ctl) = w.upgrade() {
                ctl.cancelled.store(true, Ordering::SeqCst);
            }
        }
        while !live.is_empty() {
            live = self.emptied.wait(live).unwrap_or_else(|e| e.into_inner());
        }
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Weak<SnapshotCtl>>> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Mutable host state, guarded by one mutex so `snapshot()` cannot
/// observe a write in progress.
#[derive(Debug, Default)]
struct HostState {
    overlays: OverlayStore,
    revision: u64,
}

pub(crate) type AnalysisCache = Mutex<HashMap<PathBuf, Arc<PackageAnalysis>>>;

/// The one writer (contract clause 3). Owns the overlay store and the
/// per-revision analysis memo; hands out read-only [`Snapshot`]s.
#[derive(Default)]
pub struct QueryHost {
    state: Mutex<HostState>,
    live: Arc<LiveSnapshots>,
    /// Analysis memo for the *current* revision, keyed by entry path.
    /// Cleared inside `apply_change`, strictly after every old
    /// snapshot has dropped — entries therefore never cross revisions.
    cache: Arc<AnalysisCache>,
}

impl QueryHost {
    pub fn new() -> QueryHost {
        QueryHost::default()
    }

    /// Apply one overlay change: cancel and drain every outstanding
    /// snapshot, then mutate. Blocks the caller (the write path is the
    /// slow path, deliberately). Must not be called from a thread that
    /// holds a [`Snapshot`] — that would deadlock, and the transport
    /// shim's main loop never does.
    pub fn apply_change(&self, change: Change) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        self.live.cancel_all_and_wait();
        match change {
            Change::Open { path, text } | Change::Edit { path, text } => {
                state.overlays.set(path, text);
            }
            Change::Close { path } => state.overlays.remove(&path),
        }
        state.revision += 1;
        self.cache.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    /// Freeze the current state for reading. Cheap; any number may be
    /// outstanding across threads.
    pub fn snapshot(&self) -> Snapshot {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let ctl = Arc::new(SnapshotCtl::default());
        self.live.register(&ctl);
        Snapshot {
            ctl,
            live: Arc::clone(&self.live),
            overlays: state.overlays.clone(),
            revision: state.revision,
            cache: Arc::clone(&self.cache),
        }
    }

    /// The overlay revision (bumps on every change). Observability only.
    pub fn revision(&self) -> u64 {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .revision
    }
}

/// A handle that cancels one snapshot's in-flight queries — the
/// `$/cancelRequest` hook. Cloneable, thread-safe, fire-and-forget.
#[derive(Clone)]
pub struct CancelToken {
    ctl: Arc<SnapshotCtl>,
}

impl CancelToken {
    pub fn cancel(&self) {
        self.ctl.cancelled.store(true, Ordering::SeqCst);
    }
}

/// A frozen read view (contract clause 2). Queries live in
/// `queries.rs`; every public one is wrapped in [`Snapshot::guard`].
pub struct Snapshot {
    ctl: Arc<SnapshotCtl>,
    live: Arc<LiveSnapshots>,
    pub(crate) overlays: OverlayStore,
    pub(crate) revision: u64,
    pub(crate) cache: Arc<AnalysisCache>,
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        self.live.unregister(&self.ctl);
    }
}

impl Snapshot {
    /// A token that cancels this snapshot's queries from another thread.
    pub fn cancel_token(&self) -> CancelToken {
        CancelToken {
            ctl: Arc::clone(&self.ctl),
        }
    }

    /// The revision this snapshot was taken at.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Unwind-if-cancelled — the read-side half of clause 3. Called at
    /// every phase boundary inside queries.
    pub(crate) fn checkpoint(&self) {
        if self.ctl.cancelled.load(Ordering::SeqCst) {
            std::panic::panic_any(Cancelled);
        }
    }

    /// First checkpoint of every query: honors [`crate::TEST_SLOW_ENV`]
    /// by sleeping in cancellable slices (tests only; see the const).
    pub(crate) fn begin(&self) {
        self.checkpoint();
        if let Ok(ms) = std::env::var(crate::TEST_SLOW_ENV)
            && let Ok(total) = ms.parse::<u64>()
        {
            let mut slept = 0;
            while slept < total {
                self.checkpoint();
                let slice = 5.min(total - slept);
                std::thread::sleep(Duration::from_millis(slice));
                slept += slice;
            }
            self.checkpoint();
        }
    }

    /// THE one catch layer (clause 3): runs `f`, converting the
    /// cancellation unwind — and nothing else — into `Err(Cancelled)`.
    /// Any other panic resumes: a compiler bug must stay loud.
    pub(crate) fn guard<T>(&self, f: impl FnOnce() -> T) -> Result<T, Cancelled> {
        match std::panic::catch_unwind(AssertUnwindSafe(f)) {
            Ok(v) => Ok(v),
            Err(payload) => {
                if payload.is::<Cancelled>() {
                    Err(Cancelled)
                } else {
                    std::panic::resume_unwind(payload)
                }
            }
        }
    }

    /// The text of `path` as queries see it: overlay first, disk second
    /// (contract clause 1). `None` when neither has it.
    pub fn file_text(&self, path: &Path) -> Option<Arc<Vec<u8>>> {
        self.overlays.read(path)
    }
}
