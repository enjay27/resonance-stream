//! Which chat messages a view shows. Pure functions, so they are tested on
//! the host (`cargo test -p resonance-stream-ui`) without a browser.

use crate::ui_types::ChatMessage;
use std::collections::{HashMap, VecDeque};

pub const ALL_TAB: &str = "전체";
pub const CUSTOM_TAB: &str = "커스텀";

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

impl Tab {
    /// Key of the tab's message list in [`TabViews`] (and in `tab_limits`).
    pub fn view_key(self) -> Option<&'static str> {
        match self {
            Tab::All => Some(ALL_TAB),
            Tab::Custom => Some(CUSTOM_TAB),
            Tab::System => None,
            Tab::Channel(channel) => Some(channel),
        }
    }
}

/// How many messages a tab keeps. Channel tabs have their own limit
/// (right-click menu; unset: WORLD 200, others 1000). The all-tab and the
/// custom tab have no input of their own: the all-tab holds as many as all
/// channel limits together (2000 when none is set), the custom tab as many
/// as its selected channels together.
pub fn tab_limit(limits: &HashMap<String, usize>, key: &str, custom_filters: &[String]) -> usize {
    let own = |key: &str| {
        limits
            .get(key)
            .copied()
            .unwrap_or(if key == "WORLD" { 200 } else { 1000 })
    };
    let limit = match key {
        ALL_TAB => {
            let sum: usize = limits
                .iter()
                .filter(|(k, _)| !matches!(k.as_str(), ALL_TAB | CUSTOM_TAB | "SYSTEM"))
                .map(|(_, v)| *v)
                .sum();
            if sum == 0 {
                2000
            } else {
                sum
            }
        }
        CUSTOM_TAB => custom_filters.iter().map(|channel| own(channel)).sum(),
        _ => own(key),
    };
    limit.max(1)
}

/// Per-tab message lists (pids, oldest first), each capped by its own limit,
/// so a busy world chat cannot push party or guild messages out.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TabViews {
    views: HashMap<String, VecDeque<u64>>,
    /// How many views hold each pid; a pid no view holds can be dropped.
    refs: HashMap<u64, usize>,
}

impl TabViews {
    /// Files a new message under the all-tab, its channel and (when
    /// selected) the custom tab. Returns the pids no tab holds any more.
    pub fn add(
        &mut self,
        pid: u64,
        channel: &str,
        custom_filters: &[String],
        limits: &HashMap<String, usize>,
    ) -> Vec<u64> {
        let mut keys = vec![ALL_TAB, channel];
        if custom_filters.iter().any(|c| c == channel) {
            keys.push(CUSTOM_TAB);
        }
        let mut dropped = Vec::new();
        for key in keys {
            let limit = tab_limit(limits, key, custom_filters);
            let view = self.views.entry(key.to_string()).or_default();
            view.push_back(pid);
            *self.refs.entry(pid).or_insert(0) += 1;
            while view.len() > limit {
                let Some(old) = view.pop_front() else { break };
                if let Some(count) = self.refs.get_mut(&old) {
                    *count -= 1;
                    if *count == 0 {
                        self.refs.remove(&old);
                        dropped.push(old);
                    }
                }
            }
        }
        dropped
    }

    /// The tab's pids, oldest first.
    pub fn pids<'a>(&'a self, key: &str) -> impl DoubleEndedIterator<Item = u64> + 'a {
        self.views.get(key).into_iter().flatten().copied()
    }

    pub fn len(&self, key: &str) -> usize {
        self.views.get(key).map_or(0, VecDeque::len)
    }

    pub fn clear(&mut self) {
        self.views.clear();
        self.refs.clear();
    }
}

/// The chat messages the UI holds: each by pid, plus the per-tab lists.
/// A message is kept exactly as long as some tab lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatStore<T> {
    messages: HashMap<u64, T>,
    views: TabViews,
}

impl<T> Default for ChatStore<T> {
    fn default() -> Self {
        Self {
            messages: HashMap::new(),
            views: TabViews::default(),
        }
    }
}

impl<T: Clone> ChatStore<T> {
    pub fn add(
        &mut self,
        pid: u64,
        channel: &str,
        message: T,
        custom_filters: &[String],
        limits: &HashMap<String, usize>,
    ) {
        self.messages.insert(pid, message);
        for gone in self.views.add(pid, channel, custom_filters, limits) {
            self.messages.remove(&gone);
        }
    }

    pub fn get(&self, pid: u64) -> Option<&T> {
        self.messages.get(&pid)
    }

    /// Merges the backend's history (`(pid, channel, message)`) under the
    /// messages that arrived live while it was being fetched, instead of
    /// replacing them. Pids give the order (the backend hands them out in
    /// arrival order); a pid held both ways keeps the live copy, which
    /// events may already have updated.
    pub fn merge_history(
        &mut self,
        history: Vec<(u64, String, T)>,
        channel_of: impl Fn(&T) -> String,
        custom_filters: &[String],
        limits: &HashMap<String, usize>,
    ) {
        let live = std::mem::take(self);
        let mut rows: Vec<(u64, String, T)> = history
            .into_iter()
            .filter(|(pid, _, _)| !live.messages.contains_key(pid))
            .collect();
        rows.extend(
            live.messages
                .into_iter()
                .map(|(pid, m)| (pid, channel_of(&m), m)),
        );
        rows.sort_unstable_by_key(|(pid, _, _)| *pid);
        for (pid, channel, message) in rows {
            self.add(pid, &channel, message, custom_filters, limits);
        }
    }

    /// The tab's messages, oldest first.
    pub fn tab<'a>(&'a self, key: &str) -> impl DoubleEndedIterator<Item = T> + 'a {
        self.views
            .pids(key)
            .filter_map(move |pid| self.messages.get(&pid).cloned())
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Every message held, in arrival (pid) order.
    pub fn all(&self) -> Vec<T> {
        let mut pids: Vec<_> = self.messages.keys().copied().collect();
        pids.sort_unstable();
        pids.iter().map(|pid| self.messages[pid].clone()).collect()
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.views.clear();
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

/// A message that neither pings (keyword alert) nor counts as unread: a
/// blocked sender, or WORLD chat below the minimum sender level.
pub fn is_muted(m: &ChatMessage, min_level: u64) -> bool {
    m.is_blocked || (m.channel == "WORLD" && m.level < min_level)
}

/// The newest `limit` items accepted by `keep`, oldest first -- walking
/// from the newest end, so a short page of a long list stops early.
pub fn newest_matching<T>(
    items: impl DoubleEndedIterator<Item = T>,
    limit: usize,
    mut keep: impl FnMut(&T) -> bool,
) -> Vec<T> {
    let mut page: Vec<T> = items.rev().filter(|v| keep(v)).take(limit).collect();
    page.reverse();
    page
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

    use std::collections::BTreeMap;

    #[test]
    fn newest_matching_returns_the_last_page_in_order() {
        let log: BTreeMap<u64, u64> = (1..=10).map(|pid| (pid, pid)).collect();
        assert_eq!(
            newest_matching(log.values().copied(), 3, |_| true),
            [8, 9, 10]
        );
        assert_eq!(
            newest_matching(log.values().copied(), 2, |v| v % 2 == 1),
            [7, 9]
        );
        assert_eq!(
            newest_matching(log.values().copied(), 50, |v| *v > 8),
            [9, 10]
        );
    }

    #[test]
    fn newest_matching_stops_early() {
        let log: BTreeMap<u64, u64> = (1..=1000).map(|pid| (pid, pid)).collect();
        let mut looked_at = 0;
        newest_matching(log.values().copied(), 5, |_| {
            looked_at += 1;
            true
        });
        assert_eq!(looked_at, 5);
    }

    fn limits(pairs: &[(&str, usize)]) -> HashMap<String, usize> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn tab_limits_and_their_defaults() {
        let l = limits(&[("PARTY", 50), ("LOCAL", 30), ("전체", 999), ("커스텀", 999)]);
        let custom = vec!["PARTY".to_string(), "WORLD".to_string()];
        assert_eq!(tab_limit(&l, "PARTY", &custom), 50);
        assert_eq!(tab_limit(&l, "WORLD", &custom), 200); // unset: menu default
        assert_eq!(tab_limit(&l, "GUILD", &custom), 1000);
        // All-tab: every set channel limit together; its own entry is ignored.
        assert_eq!(tab_limit(&l, ALL_TAB, &custom), 80);
        assert_eq!(tab_limit(&HashMap::new(), ALL_TAB, &custom), 2000);
        // Custom: its selected channels together.
        assert_eq!(tab_limit(&l, CUSTOM_TAB, &custom), 250);
        assert_eq!(tab_limit(&l, CUSTOM_TAB, &[]), 1); // never 0: keeps the newest
    }

    #[test]
    fn views_file_each_message_under_its_tabs() {
        let mut v = TabViews::default();
        let custom = vec!["GUILD".to_string()];
        let l = HashMap::new();
        v.add(1, "WORLD", &custom, &l);
        v.add(2, "GUILD", &custom, &l);
        assert_eq!(v.pids(ALL_TAB).collect::<Vec<_>>(), [1, 2]);
        assert_eq!(v.pids("WORLD").collect::<Vec<_>>(), [1]);
        assert_eq!(v.pids("GUILD").collect::<Vec<_>>(), [2]);
        assert_eq!(v.pids(CUSTOM_TAB).collect::<Vec<_>>(), [2]);
        assert_eq!(v.pids("PARTY").count(), 0);
    }

    #[test]
    fn busy_world_chat_does_not_evict_guild_messages() {
        let mut v = TabViews::default();
        let l = limits(&[("WORLD", 3), ("GUILD", 2)]); // all-tab: 5
        v.add(1, "GUILD", &[], &l);
        let mut dropped = Vec::new();
        for pid in 2..=20 {
            dropped.extend(v.add(pid, "WORLD", &[], &l));
        }
        assert_eq!(v.pids("WORLD").collect::<Vec<_>>(), [18, 19, 20]);
        assert_eq!(v.len(ALL_TAB), 5);
        // The guild message left the all-tab but its own tab still holds it.
        assert_eq!(v.pids("GUILD").collect::<Vec<_>>(), [1]);
        assert!(!dropped.contains(&1));
        // World messages no tab holds are reported, each once.
        assert_eq!(dropped, (2..=15).collect::<Vec<_>>());
    }

    #[test]
    fn a_pid_is_dropped_only_when_its_last_view_lets_go() {
        let mut v = TabViews::default();
        let l = limits(&[("GUILD", 1), ("PARTY", 1)]); // all-tab: 2
        assert!(v.add(1, "GUILD", &[], &l).is_empty());
        assert!(v.add(2, "PARTY", &[], &l).is_empty());
        // 3 pushes 1 out of the all-tab (limit 2) and out of GUILD (limit 1):
        // now no view holds 1.
        let dropped = v.add(3, "GUILD", &[], &l);
        assert_eq!(dropped, [1]);
        v.clear();
        assert_eq!(v.len(ALL_TAB), 0);
    }

    #[test]
    fn chat_store_keeps_a_message_while_a_tab_lists_it() {
        let mut store = ChatStore::default();
        let l = limits(&[("WORLD", 2)]); // all-tab: 2
        store.add(1, "GUILD", "g1", &[], &l);
        store.add(2, "WORLD", "w2", &[], &l);
        store.add(3, "WORLD", "w3", &[], &l);
        store.add(4, "WORLD", "w4", &[], &l);
        // w2 left both WORLD and the all-tab; g1 left the all-tab but GUILD keeps it.
        assert_eq!(store.get(2), None);
        assert_eq!(store.get(1), Some(&"g1"));
        assert_eq!(store.len(), 3);
        assert_eq!(store.tab(ALL_TAB).collect::<Vec<_>>(), ["w3", "w4"]);
        assert_eq!(store.tab("GUILD").collect::<Vec<_>>(), ["g1"]);
        assert_eq!(store.all(), ["g1", "w3", "w4"]);
        store.clear();
        assert_eq!(store.len(), 0);
        assert_eq!(store.tab(ALL_TAB).count(), 0);
    }

    #[test]
    fn blocked_and_low_level_world_senders_are_muted() {
        let mut m = msg("WORLD", 5, "a", "hi");
        assert!(!is_muted(&m, 5));
        assert!(is_muted(&m, 6));
        m.channel = "GUILD".into();
        assert!(!is_muted(&m, 6)); // the level rule is WORLD only
        m.is_blocked = true;
        assert!(is_muted(&m, 0));
    }

    type Row = (String, &'static str);
    fn row(channel: &str, tag: &'static str) -> Row {
        (channel.to_string(), tag)
    }
    fn channel_of(r: &Row) -> String {
        r.0.clone()
    }

    #[test]
    fn history_merges_under_messages_that_arrived_while_it_loaded() {
        // Regression (A3): hydration replaced the store with the history,
        // dropping messages that arrived (live) during the fetch.
        let (filters, limits) = (vec![], HashMap::new());
        let mut store = ChatStore::default();
        store.add(7, "WORLD", row("WORLD", "live"), &filters, &limits);
        let history = vec![
            (3, "PARTY".to_string(), row("PARTY", "old-a")),
            (5, "WORLD".to_string(), row("WORLD", "old-b")),
        ];
        store.merge_history(history, channel_of, &filters, &limits);
        let tags: Vec<_> = store.tab(ALL_TAB).map(|r| r.1).collect();
        assert_eq!(tags, ["old-a", "old-b", "live"]);
        assert_eq!(store.tab("WORLD").count(), 2);
    }

    #[test]
    fn a_message_both_live_and_in_history_keeps_the_live_copy() {
        let (filters, limits) = (vec![], HashMap::new());
        let mut store = ChatStore::default();
        store.add(5, "WORLD", row("WORLD", "live"), &filters, &limits);
        let history = vec![(5, "WORLD".to_string(), row("WORLD", "snapshot"))];
        store.merge_history(history, channel_of, &filters, &limits);
        assert_eq!(store.len(), 1);
        assert_eq!(store.get(5).unwrap().1, "live");
    }

    #[test]
    fn merged_history_still_obeys_the_tab_limits() {
        let filters = vec![];
        let limits = limits(&[("WORLD", 2)]);
        let mut store = ChatStore::default();
        store.add(9, "WORLD", row("WORLD", "live"), &filters, &limits);
        let history = (1..=4)
            .map(|pid| (pid, "WORLD".to_string(), row("WORLD", "old")))
            .collect();
        store.merge_history(history, channel_of, &filters, &limits);
        let pids: Vec<_> = store.views.pids("WORLD").collect();
        assert_eq!(pids, [4, 9]);
    }
}
