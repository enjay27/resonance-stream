//! Keeps a line that repeats from filling the system log.
//!
//! The backend keeps only the last 200 log lines, so a message written every
//! few seconds pushes the useful ones out. [`LogThrottle`] lets the first
//! occurrence through, drops the identical ones for a window, and then lets
//! the next one through with a count of what it dropped.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// How long identical lines are held back after one was written.
pub const REPEAT_WINDOW: Duration = Duration::from_secs(60);

/// Distinct lines tracked at once; past this the memory is cleared, which at
/// worst lets a few repeats through.
const MAX_TRACKED: usize = 256;

#[derive(Debug)]
struct Seen {
    /// When this line was last written.
    written: Instant,
    /// Identical lines dropped since then.
    dropped: u32,
}

#[derive(Debug)]
pub struct LogThrottle {
    window: Duration,
    seen: HashMap<(String, String), Seen>,
}

impl Default for LogThrottle {
    fn default() -> Self {
        Self::new(REPEAT_WINDOW)
    }
}

impl LogThrottle {
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            seen: HashMap::new(),
        }
    }

    /// The text to write for this line, or `None` to drop it. A line that
    /// follows dropped ones carries their count.
    pub fn check(&mut self, source: &str, message: &str, now: Instant) -> Option<String> {
        // Forget lines that expired with nothing dropped, so the map holds
        // about the distinct lines of the last window. One that dropped some
        // stays until its next occurrence reports the count.
        let window = self.window;
        self.seen.retain(|_, seen| {
            seen.dropped > 0 || now.saturating_duration_since(seen.written) < window
        });
        if self.seen.len() > MAX_TRACKED {
            self.seen.clear();
        }

        let key = (source.to_string(), message.to_string());
        match self.seen.get_mut(&key) {
            Some(seen) if now.saturating_duration_since(seen.written) < window => {
                seen.dropped += 1;
                None
            }
            Some(seen) => {
                let dropped = std::mem::take(&mut seen.dropped);
                seen.written = now;
                Some(match dropped {
                    0 => message.to_string(),
                    n => format!("{message} (repeated {n} more times)"),
                })
            }
            None => {
                self.seen.insert(
                    key,
                    Seen {
                        written: now,
                        dropped: 0,
                    },
                );
                Some(message.to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Duration = Duration::from_secs(60);

    fn secs(base: Instant, s: u64) -> Instant {
        base + Duration::from_secs(s)
    }

    #[test]
    fn the_first_line_goes_through_unchanged() {
        let mut throttle = LogThrottle::new(WINDOW);
        let out = throttle.check("Sniffer", "No traffic", Instant::now());
        assert_eq!(out.as_deref(), Some("No traffic"));
    }

    #[test]
    fn an_identical_line_inside_the_window_is_dropped() {
        let t0 = Instant::now();
        let mut throttle = LogThrottle::new(WINDOW);
        throttle.check("Sniffer", "No traffic", t0);
        assert_eq!(throttle.check("Sniffer", "No traffic", secs(t0, 15)), None);
        assert_eq!(throttle.check("Sniffer", "No traffic", secs(t0, 59)), None);
    }

    #[test]
    fn the_first_line_after_the_window_says_how_many_were_dropped() {
        let t0 = Instant::now();
        let mut throttle = LogThrottle::new(WINDOW);
        throttle.check("Sniffer", "No traffic", t0);
        throttle.check("Sniffer", "No traffic", secs(t0, 15));
        throttle.check("Sniffer", "No traffic", secs(t0, 30));
        let out = throttle.check("Sniffer", "No traffic", secs(t0, 60));
        assert_eq!(out.as_deref(), Some("No traffic (repeated 2 more times)"));
    }

    #[test]
    fn a_line_after_the_window_with_nothing_dropped_is_plain() {
        let t0 = Instant::now();
        let mut throttle = LogThrottle::new(WINDOW);
        throttle.check("Sniffer", "No traffic", t0);
        let out = throttle.check("Sniffer", "No traffic", secs(t0, 120));
        assert_eq!(out.as_deref(), Some("No traffic"));
    }

    #[test]
    fn the_window_restarts_when_a_line_is_written() {
        let t0 = Instant::now();
        let mut throttle = LogThrottle::new(WINDOW);
        throttle.check("Sniffer", "No traffic", t0);
        assert!(throttle
            .check("Sniffer", "No traffic", secs(t0, 60))
            .is_some());
        // 60 s after t0 it was written again, so 90 s is inside the new window
        assert_eq!(throttle.check("Sniffer", "No traffic", secs(t0, 90)), None);
        assert!(throttle
            .check("Sniffer", "No traffic", secs(t0, 120))
            .is_some());
    }

    #[test]
    fn a_different_message_or_source_is_not_held_back() {
        let t0 = Instant::now();
        let mut throttle = LogThrottle::new(WINDOW);
        throttle.check("Sniffer", "No traffic", t0);
        assert!(throttle.check("Sniffer", "Other", secs(t0, 1)).is_some());
        assert!(throttle
            .check("Translator", "No traffic", secs(t0, 1))
            .is_some());
    }

    #[test]
    fn a_line_still_counts_as_a_repeat_after_another_line_came_between() {
        let t0 = Instant::now();
        let mut throttle = LogThrottle::new(WINDOW);
        throttle.check("Sniffer", "No traffic", t0);
        throttle.check("Sniffer", "Other", secs(t0, 5));
        assert_eq!(throttle.check("Sniffer", "No traffic", secs(t0, 10)), None);
    }

    #[test]
    fn lines_not_seen_for_a_window_are_forgotten() {
        let t0 = Instant::now();
        let mut throttle = LogThrottle::new(WINDOW);
        for n in 0..100 {
            throttle.check("Sniffer", &format!("line {n}"), t0);
        }
        throttle.check("Sniffer", "later", secs(t0, 61));
        assert_eq!(throttle.seen.len(), 1);
    }

    #[test]
    fn the_memory_stays_bounded_for_lines_that_each_dropped_one() {
        let t0 = Instant::now();
        let mut throttle = LogThrottle::new(WINDOW);
        for n in 0..1000 {
            let line = format!("line {n}");
            throttle.check("Sniffer", &line, t0);
            throttle.check("Sniffer", &line, t0);
        }
        assert!(throttle.seen.len() <= MAX_TRACKED + 1);
    }
}
