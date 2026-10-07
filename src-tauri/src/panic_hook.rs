//! A panic leaves a line in `<data>/logs/panic.log`. Release builds are `panic = "abort"`, so
//! the process ends at once and nothing else says why (review W-5). The text and the capped
//! file are `resonance_core::crash_log`; this is only the hook.

use resonance_core::crash_log::{append_capped, panic_report};
use std::panic::PanicHookInfo;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// A panic log past this size is moved to `panic.log.1`.
const MAX_BYTES: u64 = 256 * 1024;

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Installs the hook, chained before the previous one (the default one still prints to stderr).
/// Call it first thing: a panic before `set_log_dir` is only printed.
pub fn install() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(path) = LOG_PATH.get() {
            let report = report_for(info);
            // Nothing to do if the disk refuses: the process is going down anyway.
            let _ = append_capped(path, &report, MAX_BYTES);
        }
        previous(info);
    }));
}

/// Where panics are written from now on: `<data>/logs/panic.log`.
pub fn set_log_dir(data_dir: &std::path::Path) {
    let _ = LOG_PATH.set(data_dir.join("logs").join("panic.log"));
}

fn report_for(info: &PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    let message = payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "(not text)".to_string());
    let location = info
        .location()
        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    panic_report(
        now,
        env!("CARGO_PKG_VERSION"),
        std::thread::current().name().unwrap_or("<unnamed>"),
        location.as_deref(),
        &message,
    )
}
