//! The line in the settings that says which dictionary is in use: the version last synced, the
//! revision of the signed metadata it came from, and whether the file still is that one. Pure and
//! host-tested; the backend's `get_dictionary_status` supplies the numbers.

use crate::ui_types::{DictionaryState, DictionaryStatus};

/// What the settings show next to the sync button, e.g. `v1.0.8 · 서명 리비전 3 · 게시본과 같음`.
pub fn status_line(status: &DictionaryStatus) -> String {
    let version = status.version.trim();
    // `0.0.0` is what the metadata file starts with: no sync has happened.
    let never_synced = version.is_empty() || version == "0.0.0";
    let mut parts: Vec<String> = Vec::new();
    if never_synced {
        parts.push("아직 동기화한 적 없음".into());
        if status.state == DictionaryState::Modified {
            parts.push("직접 수정됨".into());
        }
    } else {
        parts.push(format!("v{version}"));
        if status.revision > 0 {
            parts.push(format!("서명 리비전 {}", status.revision));
        }
        parts.push(
            match status.state {
                DictionaryState::Same => "게시본과 같음",
                DictionaryState::Modified => "직접 수정됨",
                DictionaryState::Unknown => "확인 불가",
            }
            .into(),
        );
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(version: &str, revision: u64, state: DictionaryState) -> DictionaryStatus {
        DictionaryStatus {
            version: version.into(),
            revision,
            state,
        }
    }

    #[test]
    fn a_synced_dictionary_names_its_version_revision_and_state() {
        assert_eq!(
            status_line(&status("1.0.8", 3, DictionaryState::Same)),
            "v1.0.8 · 서명 리비전 3 · 게시본과 같음"
        );
        assert_eq!(
            status_line(&status("1.0.8", 3, DictionaryState::Modified)),
            "v1.0.8 · 서명 리비전 3 · 직접 수정됨"
        );
    }

    #[test]
    fn an_unknown_revision_is_left_out_not_shown_as_zero() {
        assert_eq!(
            status_line(&status("1.0.6", 0, DictionaryState::Unknown)),
            "v1.0.6 · 확인 불가"
        );
    }

    #[test]
    fn a_dictionary_that_was_never_synced_says_so() {
        for version in ["", "0.0.0", "  "] {
            assert_eq!(
                status_line(&status(version, 0, DictionaryState::Unknown)),
                "아직 동기화한 적 없음"
            );
        }
        // Edited before any sync: still never synced, and it does not claim a version.
        assert_eq!(
            status_line(&status("0.0.0", 0, DictionaryState::Modified)),
            "아직 동기화한 적 없음 · 직접 수정됨"
        );
    }
}
