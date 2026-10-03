//! How the app-update dialog shows a download that has not reported progress
//! yet, and how it tells a cancelled download from a failed one.

/// Where to get the app by hand when the in-app update does not work.
pub const RELEASES_URL: &str = "https://github.com/enjay27/resonance-stream/releases/latest";

/// The bar's value: `None` (an indeterminate, moving bar) until the first
/// percent is reported -- 0% could just as well be a connection that never
/// opens.
pub fn bar_value(percent: u8) -> Option<u8> {
    (percent > 0).then(|| percent.min(100))
}

/// The text under the bar.
pub fn bar_label(percent: u8) -> String {
    match bar_value(percent) {
        Some(p) => format!("{p}%"),
        None => "연결 중...".to_string(),
    }
}

/// Did the user cancel, as opposed to the download failing?
pub fn is_cancelled(error: &str) -> bool {
    error.trim() == resonance_types::UPDATE_CANCELLED
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_progress_yet_is_an_indeterminate_bar() {
        assert_eq!(bar_value(0), None);
        assert_eq!(bar_label(0), "연결 중...");
    }

    #[test]
    fn reported_progress_is_a_plain_percentage() {
        assert_eq!(bar_value(1), Some(1));
        assert_eq!(bar_label(1), "1%");
        assert_eq!(bar_value(57), Some(57));
        assert_eq!(bar_label(57), "57%");
        assert_eq!(bar_value(100), Some(100));
        assert_eq!(bar_label(100), "100%");
    }

    #[test]
    fn a_percentage_past_100_is_clamped() {
        assert_eq!(bar_value(250), Some(100));
        assert_eq!(bar_label(250), "100%");
    }

    #[test]
    fn only_the_cancel_reason_counts_as_cancelled() {
        assert!(is_cancelled(resonance_types::UPDATE_CANCELLED));
        assert!(is_cancelled(&format!(
            "  {}\n",
            resonance_types::UPDATE_CANCELLED
        )));
        assert!(!is_cancelled(
            "No data received for 30 s; the connection looks stuck."
        ));
        assert!(!is_cancelled(""));
    }

    #[test]
    fn the_manual_download_page_is_this_repos_latest_release() {
        assert_eq!(
            RELEASES_URL,
            "https://github.com/enjay27/resonance-stream/releases/latest"
        );
    }
}
