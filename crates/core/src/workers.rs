//! Decisions about the app's background workers, kept free of Tauri so they
//! can be tested on any OS.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

/// What to do with a worker after a settings change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerChange {
    Keep,
    Start,
    Stop,
    Restart,
}

/// The settings that decide whether and how the translator runs.
#[derive(Debug, Clone, Copy)]
pub struct TranslatorSettings<'a> {
    pub enabled: bool,
    pub compute_mode: &'a str,
    pub tier: &'a str,
}

/// Exactly one change for the translator, given old and new settings.
pub fn translator_change(old: TranslatorSettings, new: TranslatorSettings) -> WorkerChange {
    match (old.enabled, new.enabled) {
        (false, true) => WorkerChange::Start,
        (true, false) => WorkerChange::Stop,
        (true, true) if old.compute_mode != new.compute_mode || old.tier != new.tier => {
            WorkerChange::Restart
        }
        _ => WorkerChange::Keep,
    }
}

/// A translation that waited longer than this is skipped: by then its chat
/// row has scrolled away, and translating it only delays the newer ones.
pub const MAX_TRANSLATION_WAIT: Duration = Duration::from_secs(60);

/// A port for the local translation server: `preferred` when it is free,
/// otherwise one the OS picks. 8080 is a common development port, so the
/// server must not assume it.
pub fn pick_local_port(preferred: u16) -> u16 {
    let bind = |port: u16| std::net::TcpListener::bind(("127.0.0.1", port));
    bind(preferred)
        .or_else(|_| bind(0))
        .and_then(|listener| listener.local_addr())
        .map(|addr| addr.port())
        .unwrap_or(preferred)
}

pub fn translation_is_stale(queued_at: Instant, now: Instant) -> bool {
    now.saturating_duration_since(queued_at) > MAX_TRANSLATION_WAIT
}

/// Most pids the ledger holds; the oldest go first. Only the newest few are
/// ever caught up, so this only bounds memory while translation stays off.
pub const LEDGER_CAPACITY: usize = 5000;

/// The translation ledger: the Japanese chat messages of this run (it starts
/// empty when the app opens) that are still owed a translation. A message
/// leaves it only when its translation succeeds -- or when a catch-up passes
/// it over as too old -- so one that failed, went stale in the queue or
/// arrived while the translator was off is still owed at the next start.
#[derive(Debug, Default)]
pub struct TranslationLedger {
    owed: BTreeSet<u64>,
}

impl TranslationLedger {
    /// A new Japanese message, owed until translated.
    pub fn record(&mut self, pid: u64) {
        self.owed.insert(pid);
        while self.owed.len() > LEDGER_CAPACITY {
            self.owed.pop_first();
        }
    }

    /// Translated (or gone): no longer owed. Returns whether it was owed.
    pub fn settle(&mut self, pid: u64) -> bool {
        self.owed.remove(&pid)
    }

    pub fn is_owed(&self, pid: u64) -> bool {
        self.owed.contains(&pid)
    }

    pub fn len(&self) -> usize {
        self.owed.len()
    }

    pub fn is_empty(&self) -> bool {
        self.owed.is_empty()
    }

    /// At a translator start: the newest `limit` owed pids, oldest first.
    /// They stay owed until settled; the older ones are passed over and
    /// dropped (0 catches up nothing and drops everything).
    pub fn catch_up(&mut self, limit: usize) -> Vec<u64> {
        let keep_from = self.owed.len().saturating_sub(limit);
        let newest = match self.owed.iter().nth(keep_from).copied() {
            Some(first_kept) => self.owed.split_off(&first_kept),
            None => BTreeSet::new(),
        };
        self.owed = newest;
        self.owed.iter().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(enabled: bool, mode: &'static str, tier: &'static str) -> TranslatorSettings<'static> {
        TranslatorSettings {
            enabled,
            compute_mode: mode,
            tier,
        }
    }

    #[test]
    fn turning_translation_on_starts_exactly_one_worker() {
        // Regression (review B4): save_config started two.
        assert_eq!(
            translator_change(s(false, "cpu", "middle"), s(true, "cpu", "middle")),
            WorkerChange::Start
        );
    }

    #[test]
    fn translator_transitions() {
        use WorkerChange::*;
        let on = s(true, "gpu", "high");
        assert_eq!(translator_change(on, s(false, "gpu", "high")), Stop);
        assert_eq!(translator_change(on, on), Keep);
        assert_eq!(translator_change(on, s(true, "gpu", "low")), Restart);
        assert_eq!(translator_change(on, s(true, "cpu", "high")), Restart);
        // Spec changes while off do not start anything.
        assert_eq!(
            translator_change(s(false, "cpu", "low"), s(false, "gpu", "high")),
            Keep
        );
    }

    #[test]
    fn picks_the_preferred_port_when_free() {
        // Ask the OS for a free port, release it, then prefer it.
        let free = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        assert_eq!(pick_local_port(free), free);
    }

    #[test]
    fn avoids_a_busy_preferred_port() {
        let busy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let busy_port = busy.local_addr().unwrap().port();
        let picked = pick_local_port(busy_port);
        assert_ne!(picked, busy_port);
        assert_ne!(picked, 0);
    }

    #[test]
    fn staleness() {
        let t0 = Instant::now();
        assert!(!translation_is_stale(t0, t0));
        assert!(!translation_is_stale(t0, t0 + MAX_TRANSLATION_WAIT));
        assert!(translation_is_stale(
            t0,
            t0 + MAX_TRANSLATION_WAIT + Duration::from_millis(1)
        ));
    }

    #[test]
    fn ledger_owes_a_message_until_it_is_translated() {
        let mut ledger = TranslationLedger::default();
        assert!(ledger.is_empty());
        ledger.record(5);
        ledger.record(7);
        assert!(ledger.is_owed(5));
        assert!(ledger.settle(5));
        assert!(!ledger.settle(5)); // a second translation of it is not owed
        assert!(!ledger.is_owed(5));
        assert_eq!(ledger.len(), 1);
    }

    #[test]
    fn a_failed_message_is_still_owed_after_later_ones_succeed() {
        let mut ledger = TranslationLedger::default();
        for pid in [1, 2, 3] {
            ledger.record(pid);
        }
        ledger.settle(3); // 2 failed, 3 succeeded
        ledger.settle(1);
        assert_eq!(ledger.catch_up(10), [2]);
    }

    #[test]
    fn catch_up_takes_the_newest_and_drops_the_rest() {
        let mut ledger = TranslationLedger::default();
        for pid in [10, 11, 12, 13, 14] {
            ledger.record(pid);
        }
        assert_eq!(ledger.catch_up(2), [13, 14]);
        assert_eq!(ledger.len(), 2); // still owed until translated
        assert!(!ledger.is_owed(12)); // passed over for good
        assert_eq!(ledger.catch_up(5), [13, 14]);
        assert!(ledger.catch_up(0).is_empty());
        assert!(ledger.is_empty());
    }

    #[test]
    fn ledger_keeps_only_the_newest_up_to_its_capacity() {
        let mut ledger = TranslationLedger::default();
        for pid in 0..(LEDGER_CAPACITY as u64 + 3) {
            ledger.record(pid);
        }
        assert_eq!(ledger.len(), LEDGER_CAPACITY);
        assert!(!ledger.is_owed(2));
        assert!(ledger.is_owed(3));
    }
}
