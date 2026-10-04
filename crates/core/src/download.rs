//! Download and update checks, kept free of Tauri and networking so they can
//! be tested on any OS: URL policy, integrity checks, progress throttling
//! and version comparison.

use crate::test_env::is_local_http_url;
use sha2::{Digest, Sha256};
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

/// Only HTTPS downloads: the app runs as Administrator and executes what it
/// downloads (the updater, the llama server), so a plain-HTTP URL could be
/// swapped in transit.
pub fn check_download_url(url: &str) -> Result<(), String> {
    check_download_url_allowing(url, false)
}

/// [`check_download_url`], except that with `allow_local_http` a plain `http://`
/// URL to this machine passes too. Only a test run that points the update feed
/// at a local mock server asks for it; the downloaded exe must still carry a
/// valid signature.
pub fn check_download_url_allowing(url: &str, allow_local_http: bool) -> Result<(), String> {
    let is_https = url
        .get(..8)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("https://"));
    if (is_https && url.len() > 8) || (allow_local_http && is_local_http_url(url)) {
        Ok(())
    } else {
        Err(format!(
            "Refusing to download from a non-HTTPS URL: {:?}",
            url
        ))
    }
}

/// Checks a finished download: its size against `Content-Length` (when the
/// server sent one) and its SHA-256 against `expected_sha256` (when known).
pub struct DownloadCheck {
    hasher: Sha256,
    received: u64,
    expected_len: Option<u64>,
    expected_sha256: Option<String>,
}

impl DownloadCheck {
    pub fn new(expected_len: Option<u64>, expected_sha256: Option<&str>) -> Self {
        Self {
            hasher: Sha256::new(),
            received: 0,
            expected_len,
            expected_sha256: expected_sha256.map(|h| h.trim().to_ascii_lowercase()),
        }
    }

    pub fn update(&mut self, chunk: &[u8]) {
        self.hasher.update(chunk);
        self.received += chunk.len() as u64;
    }

    pub fn received(&self) -> u64 {
        self.received
    }

    pub fn finish(self) -> Result<(), String> {
        if let Some(expected) = self.expected_len {
            if self.received != expected {
                return Err(format!(
                    "Download incomplete: received {} of {} bytes",
                    self.received, expected
                ));
            }
        }
        if let Some(expected) = self.expected_sha256 {
            let actual = hex(&self.hasher.finalize());
            if actual != expected {
                return Err(format!(
                    "Download corrupted: SHA-256 is {}, expected {}",
                    actual, expected
                ));
            }
        }
        Ok(())
    }
}

/// Turns byte counts into whole percents, reporting only changes, so a
/// multi-GB download emits ~100 progress events instead of one per chunk.
#[derive(Debug, Default)]
pub struct ProgressThrottle {
    last: Option<u8>,
}

impl ProgressThrottle {
    /// `Some(percent)` when it changed since the last report; `None` when it
    /// did not, or when the total size is unknown.
    pub fn update(&mut self, downloaded: u64, total: u64) -> Option<u8> {
        if total == 0 {
            return None;
        }
        let percent = (u128::from(downloaded) * 100 / u128::from(total)).min(100) as u8;
        if self.last == Some(percent) {
            return None;
        }
        self.last = Some(percent);
        Some(percent)
    }
}

/// How long a download may take to reach its server. Past this the host is
/// not answering (blocked, offline, a dead proxy), and waiting longer only
/// leaves the progress bar at 0%.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// How long a download may go without a single byte arriving, before the
/// response headers and between chunks. Not a limit on the whole download: a
/// multi-GB model on a slow link may take an hour as long as it keeps moving.
pub const STALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Tells a stuck download from a slow one: it is stuck when no data has
/// arrived for `limit`. The clock is passed in, so this is tested without
/// waiting.
#[derive(Debug, Clone)]
pub struct StallWatch {
    limit: Duration,
    last_data: Instant,
}

impl StallWatch {
    pub fn new(limit: Duration, now: Instant) -> Self {
        Self {
            limit,
            last_data: now,
        }
    }

    pub fn limit(&self) -> Duration {
        self.limit
    }

    /// A chunk of `bytes` arrived at `now`. An empty one is not progress: a
    /// server that only trickles empty chunks is still stuck.
    pub fn data_arrived(&mut self, bytes: usize, now: Instant) {
        if bytes > 0 {
            self.last_data = now;
        }
    }

    /// How much longer the next chunk may take; zero once stalled.
    pub fn remaining(&self, now: Instant) -> Duration {
        self.limit
            .saturating_sub(now.saturating_duration_since(self.last_data))
    }

    pub fn is_stalled(&self, now: Instant) -> bool {
        self.remaining(now).is_zero()
    }
}

/// The reason shown to the user (and logged) when a download stalls.
pub fn stall_error(limit: Duration) -> String {
    format!(
        "No data received for {} s; the connection looks stuck.",
        limit.as_secs()
    )
}

/// SHA-256 (lowercase hex) of a file, read in large chunks -- the model is
/// several GB. Run it off the async runtime.
pub fn sha256_file(path: &Path) -> io::Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut buffer = vec![0u8; 8 * 1024 * 1024];
    let mut hasher = Sha256::new();
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Moves `from` over `to`, retrying while `to` is still locked (on Windows a
/// file stays locked for a moment after the process that mapped it exits).
pub fn replace_file(from: &Path, to: &Path, attempts: u32, pause: Duration) -> io::Result<()> {
    let mut last_err = None;
    for attempt in 0..attempts.max(1) {
        if attempt > 0 {
            std::thread::sleep(pause);
        }
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.expect("at least one attempt"))
}

/// Writes `contents` to `path` so a crash mid-write leaves the old file, not
/// a torn one: the data goes to `<path>.tmp` first and is renamed over `path`.
pub fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    std::fs::write(&tmp, contents)?;
    replace_file(&tmp, path, 5, Duration::from_millis(50)).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// Is `remote` a newer version than `current`? Versions that both parse as
/// semver ("0.4.0", "v0.5.1") are compared; anything else falls back to
/// "different means newer", as before.
pub fn is_newer_version(remote: &str, current: &str) -> bool {
    let parse = |v: &str| semver::Version::parse(v.trim().trim_start_matches('v')).ok();
    match (parse(remote), parse(current)) {
        (Some(remote), Some(current)) => remote > current,
        _ => remote != current,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO_SHA256: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

    #[test]
    fn only_https_urls_are_downloaded() {
        assert!(check_download_url("https://github.com/x/y.zip").is_ok());
        assert!(check_download_url("HTTPS://example.com/a").is_ok());
        assert!(check_download_url("http://example.com/a").is_err());
        assert!(check_download_url("file:///C:/evil.exe").is_err());
        assert!(check_download_url("").is_err());
    }

    #[test]
    fn a_test_run_may_download_from_this_machine_over_http() {
        let local = "http://127.0.0.1:8099/update.exe";
        assert!(check_download_url(local).is_err());
        assert!(check_download_url_allowing(local, false).is_err());
        assert!(check_download_url_allowing(local, true).is_ok());
        // https works either way; no other plain http does.
        assert!(check_download_url_allowing("https://example.com/a", false).is_ok());
        assert!(check_download_url_allowing("https://example.com/a", true).is_ok());
        for url in [
            "http://example.com/a",
            "http://127.0.0.1.evil.example/a",
            "http://127.0.0.1@evil.example/a",
            "file:///C:/evil.exe",
        ] {
            assert!(check_download_url_allowing(url, true).is_err(), "{url}");
        }
    }

    #[test]
    fn a_matching_download_passes() {
        let mut check = DownloadCheck::new(Some(5), Some(HELLO_SHA256));
        check.update(b"hel");
        check.update(b"lo");
        assert_eq!(check.received(), 5);
        assert!(check.finish().is_ok());
    }

    #[test]
    fn hash_comparison_ignores_case_and_whitespace() {
        let upper = format!(" {} ", HELLO_SHA256.to_uppercase());
        let mut check = DownloadCheck::new(None, Some(&upper));
        check.update(b"hello");
        assert!(check.finish().is_ok());
    }

    #[test]
    fn a_truncated_download_fails() {
        let mut check = DownloadCheck::new(Some(10), None);
        check.update(b"hello");
        assert!(check.finish().unwrap_err().contains("5 of 10"));
    }

    #[test]
    fn a_tampered_download_fails() {
        let mut check = DownloadCheck::new(None, Some(HELLO_SHA256));
        check.update(b"hellO");
        assert!(check.finish().unwrap_err().contains("SHA-256"));
    }

    #[test]
    fn without_expectations_anything_passes() {
        let mut check = DownloadCheck::new(None, None);
        check.update(b"whatever");
        assert!(check.finish().is_ok());
    }

    #[test]
    fn progress_reports_only_changes() {
        let mut p = ProgressThrottle::default();
        assert_eq!(p.update(0, 1000), Some(0));
        assert_eq!(p.update(5, 1000), None); // still 0 %
        assert_eq!(p.update(10, 1000), Some(1));
        assert_eq!(p.update(1000, 1000), Some(100));
        assert_eq!(p.update(1000, 1000), None);
        assert_eq!(p.update(10, 0), None); // unknown total
    }

    #[test]
    fn progress_never_exceeds_100() {
        let mut p = ProgressThrottle::default();
        assert_eq!(p.update(2000, 1000), Some(100));
        assert_eq!(p.update(u64::MAX, u64::MAX), None); // still 100, no overflow
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("rs-download-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn sha256_file_hashes_the_contents() {
        let dir = temp_dir("sha");
        let path = dir.join("hello.bin");
        std::fs::write(&path, b"hello").unwrap();
        assert_eq!(sha256_file(&path).unwrap(), HELLO_SHA256);
        assert!(sha256_file(&dir.join("missing")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_atomic_replaces_and_leaves_no_temp_file() {
        let dir = temp_dir("atomic");
        let path = dir.join("config.json");
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        assert!(!dir.join("config.json.tmp").exists());
        assert!(write_atomic(&dir.join("no/such/dir/x"), b"x").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn replace_file_overwrites_the_target() {
        let dir = temp_dir("replace");
        let (new, old) = (dir.join("model.gguf.new"), dir.join("model.gguf"));
        std::fs::write(&new, b"new").unwrap();
        std::fs::write(&old, b"old").unwrap();
        replace_file(&new, &old, 3, Duration::ZERO).unwrap();
        assert_eq!(std::fs::read(&old).unwrap(), b"new");
        assert!(!new.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn replace_file_gives_up_after_its_attempts() {
        let dir = temp_dir("giveup");
        let err = replace_file(&dir.join("missing"), &dir.join("x"), 2, Duration::ZERO);
        assert!(err.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- stall detection -------------------------------------------------

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn a_fresh_watch_allows_the_whole_limit() {
        let t0 = Instant::now();
        let watch = StallWatch::new(secs(30), t0);
        assert_eq!(watch.remaining(t0), secs(30));
        assert!(!watch.is_stalled(t0));
    }

    #[test]
    fn the_wait_counts_down_and_ends_at_the_limit() {
        let t0 = Instant::now();
        let watch = StallWatch::new(secs(30), t0);
        assert_eq!(watch.remaining(t0 + secs(10)), secs(20));
        assert!(!watch.is_stalled(t0 + secs(29)));
        assert!(watch.is_stalled(t0 + secs(30)));
        assert_eq!(watch.remaining(t0 + secs(300)), Duration::ZERO);
    }

    #[test]
    fn data_restarts_the_wait() {
        let t0 = Instant::now();
        let mut watch = StallWatch::new(secs(30), t0);
        watch.data_arrived(1024, t0 + secs(25));
        assert_eq!(watch.remaining(t0 + secs(25)), secs(30));
        assert!(!watch.is_stalled(t0 + secs(54)));
        assert!(watch.is_stalled(t0 + secs(55)));
    }

    #[test]
    fn an_empty_chunk_is_not_progress() {
        let t0 = Instant::now();
        let mut watch = StallWatch::new(secs(30), t0);
        watch.data_arrived(0, t0 + secs(25));
        assert!(watch.is_stalled(t0 + secs(30)));
    }

    #[test]
    fn a_slow_but_steady_download_never_stalls() {
        let t0 = Instant::now();
        let mut watch = StallWatch::new(secs(30), t0);
        for i in 1..=100 {
            let now = t0 + secs(29 * i);
            assert!(!watch.is_stalled(now), "chunk {i}");
            watch.data_arrived(1, now);
        }
    }

    #[test]
    fn a_clock_that_steps_back_does_not_panic() {
        let t0 = Instant::now() + secs(100);
        let watch = StallWatch::new(secs(30), t0);
        assert_eq!(watch.remaining(t0 - secs(50)), secs(30));
        assert!(!watch.is_stalled(t0 - secs(50)));
    }

    #[test]
    fn a_watch_reports_its_limit() {
        assert_eq!(StallWatch::new(secs(12), Instant::now()).limit(), secs(12));
    }

    #[test]
    fn the_stall_message_names_the_wait() {
        let message = stall_error(secs(30));
        assert!(message.contains("30 s"), "{message}");
        assert!(message.to_lowercase().contains("no data"), "{message}");
    }

    #[test]
    fn connecting_gives_up_before_a_stalled_transfer_would() {
        assert!(CONNECT_TIMEOUT <= STALL_TIMEOUT);
        assert!(STALL_TIMEOUT >= secs(10), "a slow link must not be cut off");
    }

    #[test]
    fn version_comparison() {
        // Regression (review R10): an older remote version used to count as an update.
        assert!(!is_newer_version("0.3.9", "0.4.0"));
        assert!(!is_newer_version("0.4.0", "0.4.0"));
        assert!(is_newer_version("0.4.1", "0.4.0"));
        assert!(is_newer_version("v1.0.0", "0.4.0"));
        assert!(is_newer_version("0.10.0", "0.9.0")); // numeric, not string, order
                                                      // Not semver on either side: different means newer, as before.
        assert!(is_newer_version("2026-09", "2026-08"));
        assert!(!is_newer_version("2026-09", "2026-09"));
        assert!(is_newer_version("model-b", ""));
    }
}
