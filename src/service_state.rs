//! Orders service states (sniffer, translator) that reach the UI two ways:
//! as events, and as the `get_service_states` snapshot asked for once the
//! listeners are up. Whichever carries the larger `seq` wins, so a snapshot
//! that an event overtook does not put the badge back to an older state.

use std::sync::atomic::{AtomicU64, Ordering};

/// The largest `seq` seen for one service.
pub struct SeqGate(AtomicU64);

impl SeqGate {
    pub const fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    /// Whether a state with this `seq` is newer than the last one taken
    /// (and records it). `seq` 0 carries no order and is always taken.
    pub fn accept(&self, seq: u64) -> bool {
        seq == 0 || self.0.fetch_max(seq, Ordering::SeqCst) < seq
    }
}

impl Default for SeqGate {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_newer_state_is_taken_and_an_older_one_dropped() {
        let gate = SeqGate::new();
        assert!(gate.accept(2));
        assert!(!gate.accept(1), "older snapshot after a newer event");
        assert!(!gate.accept(2), "the same state twice");
        assert!(gate.accept(3));
    }

    #[test]
    fn a_state_without_seq_is_always_taken() {
        let gate = SeqGate::new();
        assert!(gate.accept(5));
        assert!(gate.accept(0));
    }
}
