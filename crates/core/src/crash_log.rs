//! What is written when the app panics, and where a wire timestamp is read as a date.
//!
//! Release builds are `panic = "abort"`: a panic ends the process at once, so without a hook
//! nothing says why the app vanished (review W-5). The hook itself lives in the app; the text
//! and the size-capped file are here, tested on every OS.

use chrono::{DateTime, Datelike, Timelike, Utc};
use std::io::Write;
use std::path::{Path, PathBuf};

/// A Unix timestamp as a date, or `None` when it is outside what a date can hold. The chat
/// timestamp is a raw varint from the wire, so any value can arrive.
pub fn unix_to_utc(seconds: u64) -> Option<DateTime<Utc>> {
    i64::try_from(seconds)
        .ok()
        .and_then(|s| DateTime::from_timestamp(s, 0))
}

/// `2023-11-14 22:13:20` (chrono's `format` is not enabled in this crate).
pub fn format_utc(t: &DateTime<Utc>) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        t.year(),
        t.month(),
        t.day(),
        t.hour(),
        t.minute(),
        t.second()
    )
}

/// One panic as text: when, which version, which thread, where, and the message.
pub fn panic_report(
    unix_seconds: u64,
    version: &str,
    thread: &str,
    location: Option<&str>,
    message: &str,
) -> String {
    let time = unix_to_utc(unix_seconds)
        .map(|t| format!("{} UTC", format_utc(&t)))
        .unwrap_or_else(|| "unknown time".to_string());
    format!(
        "[{time}] panic in thread '{thread}' (v{version}) at {}: {message}\n",
        location.unwrap_or("unknown location")
    )
}

/// Appends `text` to `path` (its folder is created). A file already at `max_bytes` or more is
/// first moved to `<path>.1`, replacing an older backup, so the log never grows without bound.
pub fn append_capped(path: &Path, text: &str, max_bytes: u64) -> std::io::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    if std::fs::metadata(path).is_ok_and(|m| m.len() >= max_bytes) {
        let mut backup = path.as_os_str().to_owned();
        backup.push(".1");
        let backup = PathBuf::from(backup);
        let _ = std::fs::remove_file(&backup);
        std::fs::rename(path, &backup)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("rs-crash-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_wire_timestamp_in_range_is_a_date() {
        let dt = unix_to_utc(1_700_000_000).unwrap();
        assert_eq!(format_utc(&dt), "2023-11-14 22:13:20");
    }

    #[test]
    fn a_wire_timestamp_out_of_range_is_not_a_date() {
        // A raw varint from the packet: Export used to `unwrap` this and take the app down.
        assert!(unix_to_utc(u64::MAX).is_none());
        assert!(unix_to_utc(1 << 62).is_none());
    }

    #[test]
    fn a_report_names_the_time_version_thread_place_and_message() {
        let report = panic_report(
            1_700_000_000,
            "0.6.1",
            "worker-1",
            Some("src/x.rs:10:5"),
            "boom",
        );
        assert!(
            report.starts_with("[2023-11-14 22:13:20 UTC] panic in thread 'worker-1' (v0.6.1)"),
            "{report}"
        );
        assert!(report.contains("src/x.rs:10:5"), "{report}");
        assert!(report.contains("boom"), "{report}");
        assert!(report.ends_with('\n'));
    }

    #[test]
    fn a_report_survives_an_unknown_place_and_time() {
        let report = panic_report(u64::MAX, "0.6.1", "main", None, "boom");
        assert!(report.contains("unknown time"), "{report}");
        assert!(report.contains("unknown location"), "{report}");
    }

    #[test]
    fn reports_are_appended_one_after_the_other() {
        let dir = temp_dir("append");
        let path = dir.join("logs").join("panic.log");
        append_capped(&path, "one\n", 1000).unwrap();
        append_capped(&path, "two\n", 1000).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "one\ntwo\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_full_log_moves_aside_and_the_new_report_starts_a_fresh_file() {
        let dir = temp_dir("cap");
        let path = dir.join("panic.log");
        append_capped(&path, "0123456789", 10).unwrap();
        append_capped(&path, "new\n", 10).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new\n");
        assert_eq!(
            std::fs::read_to_string(dir.join("panic.log.1")).unwrap(),
            "0123456789"
        );
        // A second rotation replaces the old backup instead of failing.
        append_capped(&path, "0123456789", 10).unwrap();
        append_capped(&path, "newer\n", 10).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("panic.log.1")).unwrap(),
            "new\n0123456789"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
