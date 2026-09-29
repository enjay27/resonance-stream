//! Decisions about the app's background workers, kept free of Tauri so they
//! can be tested on any OS.

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

pub fn translation_is_stale(queued_at: Instant, now: Instant) -> bool {
    now.saturating_duration_since(queued_at) > MAX_TRANSLATION_WAIT
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
    fn staleness() {
        let t0 = Instant::now();
        assert!(!translation_is_stale(t0, t0));
        assert!(!translation_is_stale(t0, t0 + MAX_TRANSLATION_WAIT));
        assert!(translation_is_stale(
            t0,
            t0 + MAX_TRANSLATION_WAIT + Duration::from_millis(1)
        ));
    }
}
