//! Decisions about the app's background workers, kept free of Tauri so they
//! can be tested on any OS.

use resonance_types::{ComputeMode, Tier};
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
pub struct TranslatorSettings {
    pub enabled: bool,
    pub compute_mode: ComputeMode,
    pub tier: Tier,
}

/// How many model layers llama-server puts on the GPU (`-ngl`): none on the
/// CPU, by VRAM tier on the GPU.
pub fn gpu_layers(compute_mode: ComputeMode, tier: Tier) -> u32 {
    match (compute_mode, tier) {
        (ComputeMode::Cpu, _) => 0,
        (ComputeMode::Gpu, Tier::Low) => 12,
        (ComputeMode::Gpu, Tier::Middle) => 24,
        (ComputeMode::Gpu, Tier::High) => 32,
        (ComputeMode::Gpu, Tier::VeryHigh) => 99,
    }
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

/// How long a starting llama-server may take to answer `/health`. Loading
/// a model from a cold disk can take a while; a server that dies is noticed
/// at once (the wait checks the process), so a generous limit costs nothing.
pub const SERVER_START_TIMEOUT: Duration = Duration::from_secs(90);

/// First wait before restarting a failed server; doubled for each restart
/// within `RESTART_WINDOW`.
pub const RESTART_BACKOFF: Duration = Duration::from_secs(2);
/// Restarts allowed within `RESTART_WINDOW` before giving up.
pub const MAX_RESTARTS: usize = 3;
pub const RESTART_WINDOW: Duration = Duration::from_secs(10 * 60);
/// Failed jobs in a row, with the process still alive, that mark the server
/// as hung.
pub const HUNG_AFTER_FAILURES: u32 = 3;

/// What the translator worker does after a job or a server check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupervisorAction {
    Continue,
    /// Kill the server, wait this long, start a new one.
    Restart(Duration),
    /// Too many restarts: leave translation stopped until the user restarts it.
    GiveUp,
}

/// Decides when the translator's llama-server is restarted: when its process
/// exited, or when it stopped answering while alive (hung). Restarts back
/// off, and stop after `MAX_RESTARTS` within `RESTART_WINDOW`.
#[derive(Debug, Default)]
pub struct ServerSupervisor {
    restarts: Vec<Instant>,
    failures_in_a_row: u32,
}

impl ServerSupervisor {
    pub fn on_job_ok(&mut self) {
        self.failures_in_a_row = 0;
    }

    /// A job failed while the server process is still running.
    pub fn on_job_failed(&mut self, now: Instant) -> SupervisorAction {
        self.failures_in_a_row += 1;
        if self.failures_in_a_row < HUNG_AFTER_FAILURES {
            return SupervisorAction::Continue;
        }
        self.restart(now)
    }

    /// The server process is gone.
    pub fn on_server_exited(&mut self, now: Instant) -> SupervisorAction {
        self.restart(now)
    }

    fn restart(&mut self, now: Instant) -> SupervisorAction {
        self.failures_in_a_row = 0;
        self.restarts
            .retain(|at| now.saturating_duration_since(*at) <= RESTART_WINDOW);
        if self.restarts.len() >= MAX_RESTARTS {
            return SupervisorAction::GiveUp;
        }
        let backoff = RESTART_BACKOFF * 2u32.pow(self.restarts.len() as u32);
        self.restarts.push(now);
        SupervisorAction::Restart(backoff)
    }
}

/// Longest log line `log_tail` keeps (then `…`).
pub const LOG_TAIL_LINE_CHARS: usize = 200;

/// The last `n` non-empty lines of a server log, joined by " | ", for an
/// error message: why llama-server exited is usually its last word.
pub fn log_tail(log: &str, n: usize) -> String {
    let mut lines: Vec<String> = log
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .rev()
        .take(n)
        .map(|line| {
            if line.chars().count() > LOG_TAIL_LINE_CHARS {
                let cut: String = line.chars().take(LOG_TAIL_LINE_CHARS).collect();
                cut + "…"
            } else {
                line.to_string()
            }
        })
        .collect();
    lines.reverse();
    lines.join(" | ")
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

    use ComputeMode::{Cpu, Gpu};
    use Tier::{High, Low, Middle, VeryHigh};

    fn s(enabled: bool, compute_mode: ComputeMode, tier: Tier) -> TranslatorSettings {
        TranslatorSettings {
            enabled,
            compute_mode,
            tier,
        }
    }

    #[test]
    fn the_gpu_takes_layers_by_tier_and_the_cpu_none() {
        // The numbers llama-server's `-ngl` has always been given.
        assert_eq!(
            [Low, Middle, High, VeryHigh].map(|tier| gpu_layers(Gpu, tier)),
            [12, 24, 32, 99]
        );
        for tier in Tier::ALL {
            assert_eq!(gpu_layers(Cpu, *tier), 0, "{tier:?}");
        }
    }

    #[test]
    fn turning_translation_on_starts_exactly_one_worker() {
        // Regression (review B4): save_config started two.
        assert_eq!(
            translator_change(s(false, Cpu, Middle), s(true, Cpu, Middle)),
            WorkerChange::Start
        );
    }

    #[test]
    fn translator_transitions() {
        use WorkerChange::*;
        let on = s(true, Gpu, High);
        assert_eq!(translator_change(on, s(false, Gpu, High)), Stop);
        assert_eq!(translator_change(on, on), Keep);
        assert_eq!(translator_change(on, s(true, Gpu, Low)), Restart);
        assert_eq!(translator_change(on, s(true, Cpu, High)), Restart);
        // Spec changes while off do not start anything.
        assert_eq!(
            translator_change(s(false, Cpu, Low), s(false, Gpu, High)),
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

    fn t0() -> Instant {
        Instant::now()
    }

    #[test]
    fn a_server_that_exited_is_restarted_with_growing_backoff() {
        // Regression (A2): a crashed llama-server was never noticed; every
        // later job failed silently while the badge said "Active".
        let mut sup = ServerSupervisor::default();
        let now = t0();
        assert_eq!(
            sup.on_server_exited(now),
            SupervisorAction::Restart(RESTART_BACKOFF)
        );
        assert_eq!(
            sup.on_server_exited(now + Duration::from_secs(10)),
            SupervisorAction::Restart(RESTART_BACKOFF * 2)
        );
    }

    #[test]
    fn a_server_that_keeps_crashing_is_given_up_on() {
        let mut sup = ServerSupervisor::default();
        let now = t0();
        for i in 0..MAX_RESTARTS {
            let at = now + Duration::from_secs(i as u64);
            assert!(matches!(
                sup.on_server_exited(at),
                SupervisorAction::Restart(_)
            ));
        }
        assert_eq!(
            sup.on_server_exited(now + Duration::from_secs(60)),
            SupervisorAction::GiveUp
        );
    }

    #[test]
    fn crashes_long_ago_do_not_count() {
        let mut sup = ServerSupervisor::default();
        let now = t0();
        for _ in 0..MAX_RESTARTS {
            sup.on_server_exited(now);
        }
        let later = now + RESTART_WINDOW + Duration::from_secs(1);
        assert_eq!(
            sup.on_server_exited(later),
            SupervisorAction::Restart(RESTART_BACKOFF)
        );
    }

    #[test]
    fn a_live_server_is_restarted_only_after_several_failures_in_a_row() {
        let mut sup = ServerSupervisor::default();
        let now = t0();
        for _ in 1..HUNG_AFTER_FAILURES {
            assert_eq!(sup.on_job_failed(now), SupervisorAction::Continue);
        }
        sup.on_job_ok();
        for _ in 1..HUNG_AFTER_FAILURES {
            assert_eq!(sup.on_job_failed(now), SupervisorAction::Continue);
        }
        assert_eq!(
            sup.on_job_failed(now),
            SupervisorAction::Restart(RESTART_BACKOFF)
        );
        // ...and the count starts over for the new server.
        assert_eq!(sup.on_job_failed(now), SupervisorAction::Continue);
    }

    #[test]
    fn the_last_lines_of_a_server_log_explain_its_exit() {
        let log = "load model\n\nggml_vulkan: out of memory\r\nfailed to load model\n\n";
        assert_eq!(
            log_tail(log, 2),
            "ggml_vulkan: out of memory | failed to load model"
        );
        assert_eq!(log_tail("", 3), "");
        assert_eq!(log_tail("one", 3), "one");
    }

    #[test]
    fn a_long_log_line_is_cut() {
        let tail = log_tail(&"x".repeat(1000), 1);
        assert!(tail.chars().count() <= LOG_TAIL_LINE_CHARS + 1);
    }
}
