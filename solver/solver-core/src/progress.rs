//! Shared atomic progress + cancel beacon for LAHC. An `Arc<ProgressBeacon>`
//! is written by the solver thread and read by an external observer (in
//! production: the FastAPI request handler via the PyO3 `ProgressHandle`
//! wrapper). All atomics use `Relaxed` ordering; the only invariant we need
//! is "every read returns some value the loop wrote", which `Relaxed`
//! guarantees on every platform we target.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Lock-free progress + cancel channel. Construct with [`ProgressBeacon::new`].
#[derive(Debug, Default)]
pub struct ProgressBeacon {
    iter: AtomicU64,
    placement_count: AtomicU64,
    best_score: AtomicU64,
    feasible: AtomicBool,
    cancel_requested: AtomicBool,
}

impl ProgressBeacon {
    /// Construct an empty beacon wrapped in an `Arc` for shared ownership
    /// across the solver thread and external observers.
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Write `iter` (monotonic), `placement_count`, `best_score`,
    /// `feasible`. Called from inside the LAHC loop.
    pub fn record(&self, iter: u64, placement_count: u64, best_score: u64, feasible: bool) {
        self.iter.store(iter, Ordering::Relaxed);
        self.placement_count
            .store(placement_count, Ordering::Relaxed);
        self.best_score.store(best_score, Ordering::Relaxed);
        self.feasible.store(feasible, Ordering::Relaxed);
    }

    /// Returns `true` once an external observer has requested cancellation.
    pub fn cancel_requested(&self) -> bool {
        self.cancel_requested.load(Ordering::Relaxed)
    }

    /// Sets the cancel flag. Idempotent.
    pub fn request_cancel(&self) {
        self.cancel_requested.store(true, Ordering::Relaxed);
    }

    /// Snapshot accessor: the most recently written iteration counter.
    pub fn iter_snapshot(&self) -> u64 {
        self.iter.load(Ordering::Relaxed)
    }

    /// Snapshot accessor: the most recently written placement count.
    pub fn placement_count_snapshot(&self) -> u64 {
        self.placement_count.load(Ordering::Relaxed)
    }

    /// Snapshot accessor: the most recently written best canonical score.
    pub fn best_score_snapshot(&self) -> u64 {
        self.best_score.load(Ordering::Relaxed)
    }

    /// Snapshot accessor: whether the running incumbent was feasible at the
    /// last write.
    pub fn feasible_snapshot(&self) -> bool {
        self.feasible.load(Ordering::Relaxed)
    }
}
