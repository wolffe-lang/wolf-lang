//! Optional observational compiler events. No listener means no allocation or clock read.
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub enum Event {
    Begin {
        id: u64,
        scope: &'static str,
        unit: String,
        phase: &'static str,
        detail: String,
    },
    End {
        id: u64,
        elapsed: Duration,
    },
    Units(usize),
    UnitDone {
        reused: bool,
    },
}

type Listener = Arc<dyn Fn(Event) + Send + Sync>;
static ENABLED: AtomicBool = AtomicBool::new(false);
static LISTENER: RwLock<Option<Listener>> = RwLock::new(None);
static NEXT: AtomicU64 = AtomicU64::new(1);

/// A driver owns the listener for one compilation; library entry points never install one.
pub fn listen(listener: Option<Listener>) {
    let enabled = listener.is_some();
    *LISTENER.write().unwrap_or_else(|e| e.into_inner()) = listener;
    ENABLED.store(enabled, Ordering::Release);
}

fn listener() -> Option<Listener> {
    if !ENABLED.load(Ordering::Acquire) {
        return None;
    }
    LISTENER.read().unwrap_or_else(|e| e.into_inner()).clone()
}

pub fn event(event: Event) {
    if let Some(listener) = listener() {
        listener(event);
    }
}

/// Elapsed contributions can overlap (parallel bodies, nested phases); scope is explicit.
pub struct Span(Option<(Listener, u64, Instant)>);
impl Drop for Span {
    fn drop(&mut self) {
        if let Some((listener, id, start)) = &self.0 {
            listener(Event::End {
                id: *id,
                elapsed: start.elapsed(),
            });
        }
    }
}

pub fn span(scope: &'static str, unit: &str, phase: &'static str, detail: &str) -> Span {
    let Some(listener) = listener() else {
        return Span(None);
    };
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    listener(Event::Begin {
        id,
        scope,
        unit: if unit.is_empty() {
            "root".into()
        } else {
            unit.into()
        },
        phase,
        detail: detail.into(),
    });
    Span(Some((listener, id, Instant::now())))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disabled_span_has_no_timer() {
        assert!(span("unit", "root", "parse", "").0.is_none());
    }
}
