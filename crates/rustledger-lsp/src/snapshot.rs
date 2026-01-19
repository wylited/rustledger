//! Immutable world snapshot for request handling.
//!
//! Each LSP request receives an immutable snapshot of the world state.
//! This allows requests to be processed concurrently without locks.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Global revision counter for cancellation detection.
static REVISION: AtomicU64 = AtomicU64::new(0);

/// Bump the global revision counter.
/// Called when the world state changes (e.g., document edit).
pub fn bump_revision() -> u64 {
    REVISION.fetch_add(1, Ordering::SeqCst) + 1
}

/// Get the current revision.
pub fn current_revision() -> u64 {
    REVISION.load(Ordering::SeqCst)
}

/// An immutable snapshot of the world state.
///
/// Snapshots capture the revision at creation time, allowing
/// handlers to detect if they should cancel (revision changed).
#[derive(Debug)]
pub struct Snapshot {
    /// The revision at snapshot creation time.
    revision: u64,
    /// Parsed directives (TODO: replace with actual data)
    _data: Arc<()>,
}

impl Snapshot {
    /// Create a new snapshot at the current revision.
    pub fn new() -> Self {
        Self {
            revision: current_revision(),
            _data: Arc::new(()),
        }
    }

    /// Check if this snapshot is still current (not cancelled).
    pub fn is_current(&self) -> bool {
        self.revision == current_revision()
    }

    /// Check if this snapshot has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        !self.is_current()
    }
}

impl Default for Snapshot {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_cancellation() {
        let snap = Snapshot::new();
        assert!(snap.is_current());

        bump_revision();
        assert!(snap.is_cancelled());
    }
}
