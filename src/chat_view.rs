//! Which chat messages a view shows. Pure functions, so they are tested on
//! the host (`cargo test -p resonance-stream-ui`) without a browser.

use crate::ui_types::ChatMessage;
use std::collections::BTreeMap;

/// What a tab shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    All,
    Custom,
    System,
    Channel(&'static str),
}

impl Tab {
    pub fn from_label(label: &str) -> Self {
        match label {
            "전체" => Tab::All,
            "커스텀" => Tab::Custom,
            "시스템" => Tab::System,
            "로컬" => Tab::Channel("LOCAL"),
            "파티" => Tab::Channel("PARTY"),
            "길드" => Tab::Channel("GUILD"),
            _ => Tab::Channel("WORLD"),
        }
    }

    /// Does a message on `channel` belong to this tab? (No level or search
    /// filtering: this decides unread badges.)
    pub fn shows_channel(self, channel: &str, custom_filters: &[String]) -> bool {
        match self {
            Tab::All => true,
            Tab::Custom => custom_filters.iter().any(|c| c == channel),
            Tab::System => false,
            Tab::Channel(key) => channel == key,
        }
    }
}

/// The chat list's filter: tab, minimum sender level (world chat only) and
/// search text (nickname or message, case-insensitive).
pub struct ChatFilter<'a> {
    tab: Tab,
    custom_filters: &'a [String],
    min_level: u64,
    search_lower: String,
}

impl<'a> ChatFilter<'a> {
    pub fn new(tab: Tab, custom_filters: &'a [String], min_level: u64, search: &str) -> Self {
        Self {
            tab,
            custom_filters,
            min_level,
            search_lower: search.to_lowercase(),
        }
    }

    pub fn matches(&self, m: &ChatMessage) -> bool {
        if !self.tab.shows_channel(&m.channel, self.custom_filters) {
            return false;
        }
        if m.channel == "WORLD" && m.level < self.min_level {
            return false;
        }
        self.search_lower.is_empty()
            || m.nickname.to_lowercase().contains(&self.search_lower)
            || m.message.to_lowercase().contains(&self.search_lower)
    }
}

/// The newest `limit` entries accepted by `keep`, oldest first -- walking
/// from the newest end, so a short page of a long log stops early.
pub fn newest_matching<T: Copy>(
    log: &BTreeMap<u64, T>,
    limit: usize,
    mut keep: impl FnMut(&T) -> bool,
) -> Vec<T> {
    let mut page: Vec<T> = log
        .values()
        .rev()
        .filter(|v| keep(v))
        .take(limit)
        .copied()
        .collect();
    page.reverse();
    page
}

/// Inserts under `key` and drops the oldest entries beyond `limit` (the
/// newest one always stays). Keys are pids, so the oldest is the first.
pub fn push_bounded<T>(log: &mut BTreeMap<u64, T>, key: u64, value: T, limit: usize) {
    log.insert(key, value);
    while log.len() > limit.max(1) {
        log.pop_first();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(channel: &str, level: u64, nickname: &str, message: &str) -> ChatMessage {
        ChatMessage {
            channel: channel.into(),
            level,
            nickname: nickname.into(),
            message: message.into(),
            ..Default::default()
        }
    }

    #[test]
    fn tab_labels() {
        assert_eq!(Tab::from_label("전체"), Tab::All);
        assert_eq!(Tab::from_label("커스텀"), Tab::Custom);
        assert_eq!(Tab::from_label("시스템"), Tab::System);
        assert_eq!(Tab::from_label("로컬"), Tab::Channel("LOCAL"));
        assert_eq!(Tab::from_label("파티"), Tab::Channel("PARTY"));
        assert_eq!(Tab::from_label("길드"), Tab::Channel("GUILD"));
        assert_eq!(Tab::from_label("월드"), Tab::Channel("WORLD"));
        assert_eq!(Tab::from_label("anything else"), Tab::Channel("WORLD"));
    }

    #[test]
    fn tab_membership() {
        let custom = vec!["GUILD".to_string()];
        assert!(Tab::All.shows_channel("PARTY", &custom));
        assert!(!Tab::System.shows_channel("PARTY", &custom));
        assert!(Tab::Custom.shows_channel("GUILD", &custom));
        assert!(!Tab::Custom.shows_channel("WORLD", &custom));
        assert!(Tab::Channel("PARTY").shows_channel("PARTY", &custom));
        assert!(!Tab::Channel("PARTY").shows_channel("GUILD", &custom));
    }

    #[test]
    fn level_filter_applies_to_world_chat_only() {
        let f = ChatFilter::new(Tab::All, &[], 10, "");
        assert!(!f.matches(&msg("WORLD", 5, "a", "hi")));
        assert!(f.matches(&msg("WORLD", 10, "a", "hi")));
        assert!(f.matches(&msg("PARTY", 1, "a", "hi")));
    }

    #[test]
    fn search_is_case_insensitive_on_nickname_or_message() {
        let f = ChatFilter::new(Tab::All, &[], 0, "BoB");
        assert!(f.matches(&msg("WORLD", 1, "bobby", "x")));
        assert!(f.matches(&msg("WORLD", 1, "x", "hello BOB")));
        assert!(!f.matches(&msg("WORLD", 1, "alice", "hello")));
    }

    #[test]
    fn system_tab_shows_no_chat() {
        let f = ChatFilter::new(Tab::System, &[], 0, "");
        assert!(!f.matches(&msg("WORLD", 1, "a", "b")));
    }

    #[test]
    fn newest_matching_returns_the_last_page_in_order() {
        let log: BTreeMap<u64, u64> = (1..=10).map(|pid| (pid, pid)).collect();
        assert_eq!(newest_matching(&log, 3, |_| true), [8, 9, 10]);
        assert_eq!(newest_matching(&log, 2, |v| v % 2 == 1), [7, 9]);
        assert_eq!(newest_matching(&log, 50, |v| *v > 8), [9, 10]);
    }

    #[test]
    fn newest_matching_stops_early() {
        let log: BTreeMap<u64, u64> = (1..=1000).map(|pid| (pid, pid)).collect();
        let mut looked_at = 0;
        newest_matching(&log, 5, |_| {
            looked_at += 1;
            true
        });
        assert_eq!(looked_at, 5);
    }

    #[test]
    fn push_bounded_keeps_the_newest() {
        let mut log = BTreeMap::new();
        for pid in 1..=5 {
            push_bounded(&mut log, pid, pid, 3);
        }
        assert_eq!(log.keys().copied().collect::<Vec<_>>(), [3, 4, 5]);
        // A smaller limit trims fully on the next insert.
        push_bounded(&mut log, 6, 6, 1);
        assert_eq!(log.keys().copied().collect::<Vec<_>>(), [6]);
        push_bounded(&mut log, 7, 7, 0);
        assert_eq!(log.keys().copied().collect::<Vec<_>>(), [7]);
    }
}
