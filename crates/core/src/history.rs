//! Backend chat history: the newest messages of each channel, keyed by pid.

use chrono::{Days, NaiveDate};
use resonance_types::{Channel, ChatMessage};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::{Path, PathBuf};

/// Daily chat log file for `date` (YYYY-MM-DD) inside the chat_logs folder.
pub fn chat_log_file_name(date: &str) -> String {
    format!("{}.jsonl", date)
}

/// Training-pair file of one channel (tab) in the app data folder. Anything a
/// file name cannot hold becomes `_`, so a channel name is never a path.
pub fn dataset_file_name(channel: &str) -> String {
    let safe: String = channel
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let safe = if safe.trim().is_empty() {
        "unknown".to_string()
    } else {
        safe
    };
    format!("dataset_{}.jsonl", safe)
}

/// Daily chat logs in `dir` that `keep_days` no longer covers: today's file
/// and those of the `keep_days - 1` days before it stay. 0 keeps every file.
/// A file whose name is not a `YYYY-MM-DD.jsonl` date is never touched.
pub fn expired_chat_logs(dir: &Path, today: NaiveDate, keep_days: u32) -> Vec<PathBuf> {
    let Some(oldest_kept) = (keep_days > 0)
        .then(|| today.checked_sub_days(Days::new(u64::from(keep_days) - 1)))
        .flatten()
    else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut expired: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .filter(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(|stem| NaiveDate::parse_from_str(stem, "%Y-%m-%d").ok())
                .is_some_and(|day| day < oldest_kept)
        })
        .collect();
    expired.sort();
    expired
}

/// Deletes `expired_chat_logs`; returns how many files were removed.
pub fn remove_expired_chat_logs(dir: &Path, today: NaiveDate, keep_days: u32) -> usize {
    expired_chat_logs(dir, today, keep_days)
        .iter()
        .filter(|path| std::fs::remove_file(path).is_ok())
        .count()
}

/// How many messages of each channel the backend keeps and reloads: the
/// numbers of the UI's channel tabs (src/chat_view.rs `tab_limit`; unset:
/// WORLD 200, others 1000). Per channel, so a busy WORLD chat cannot push
/// GUILD or PARTY messages out.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChannelLimits(HashMap<Channel, usize>);

impl ChannelLimits {
    /// `tab_limits` from the config: the entries named after a channel. The
    /// all-tab and custom-tab entries, and names that are no channel, are ignored.
    pub fn new(tab_limits: &HashMap<String, usize>) -> Self {
        Self(
            Channel::ALL
                .into_iter()
                .filter_map(|channel| Some((channel, *tab_limits.get(channel.as_str())?)))
                .collect(),
        )
    }

    /// Messages `channel` keeps; at least 1, like the UI.
    pub fn of(&self, channel: Channel) -> usize {
        self.0
            .get(&channel)
            .copied()
            .unwrap_or(if channel == Channel::World { 200 } else { 1000 })
            .max(1)
    }
}

/// Most log lines a start-up reload examines (see [`load_recent_within`]).
pub const MAX_SCAN_LINES: usize = 50_000;

/// The newest messages saved in `dir` (one JSON `ChatMessage` per line, one
/// `.jsonl` file per day), up to each channel's limit, oldest first. Reading
/// stops once every channel with a limit of its own is full. A message saved
/// twice (untranslated, then again once a catch-up translated it) comes back
/// once, as its newest line. Pids are
/// renumbered 1..=n in that order: saved pids come from earlier runs and may
/// collide, while new messages must sort after the loaded ones.
pub fn load_recent(dir: &Path, limits: &ChannelLimits) -> Vec<ChatMessage> {
    load_recent_within(dir, limits, MAX_SCAN_LINES)
}

/// Sets the blocked flag of each restored message from the block list as it is now. A row carries the flag it
/// had when it was saved, which is stale both ways: a sender blocked since then must come back hidden, and one
/// unblocked since then must not stay hidden.
pub fn apply_block_list(messages: &mut [ChatMessage], is_blocked: impl Fn(u64) -> bool) {
    for message in messages {
        message.is_blocked = is_blocked(message.uid);
    }
}

/// [`load_recent`] examining at most `max_lines` log lines (newest first), so
/// the start-up cost does not grow with the age of the log folder: a channel
/// that never fills (LOCAL, PARTY) would otherwise read every line ever saved.
pub fn load_recent_within(
    dir: &Path,
    limits: &ChannelLimits,
    max_lines: usize,
) -> Vec<ChatMessage> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<_> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .collect();
    files.sort(); // YYYY-MM-DD names: sorted by day

    // Newest day first, newest line first, until every channel is full.
    let mut newest_first = Vec::new();
    let mut counts: HashMap<Channel, usize> = HashMap::new();
    let mut seen = std::collections::HashSet::new();
    let all_full = |counts: &HashMap<Channel, usize>| {
        Channel::ALL
            .into_iter()
            .all(|c| counts.get(&c).copied().unwrap_or(0) >= limits.of(c))
    };
    let mut scanned = 0usize;
    'files: for file in files.iter().rev() {
        if all_full(&counts) || scanned >= max_lines {
            break;
        }
        let Ok(content) = std::fs::read_to_string(file) else {
            continue;
        };
        for line in content.lines().rev() {
            if all_full(&counts) {
                break;
            }
            if scanned >= max_lines {
                break 'files;
            }
            scanned += 1;
            let Ok(message) = serde_json::from_str::<ChatMessage>(line) else {
                continue;
            };
            // Same identity as the capture's duplicate check; messages
            // without one (no timestamp, no sequence id) are all kept.
            let identity = (message.uid, message.timestamp, message.sequence_id);
            if (message.timestamp != 0 || message.sequence_id != 0) && !seen.insert(identity) {
                continue;
            }
            let count = counts.entry(message.channel).or_insert(0);
            if *count < limits.of(message.channel) {
                *count += 1;
                newest_first.push(message);
            }
        }
    }

    newest_first.reverse();
    for (pid, message) in (1..).zip(newest_first.iter_mut()) {
        message.pid = pid;
    }
    newest_first
}

/// Backend chat history, capped per channel (`ChannelLimits`). Pids come
/// from one increasing counter, so ordering by pid is arrival order.
#[derive(Debug, Default)]
pub struct ChatHistory {
    messages: BTreeMap<u64, ChatMessage>,
    /// Pids of each channel, oldest first.
    by_channel: HashMap<Channel, VecDeque<u64>>,
    limits: ChannelLimits,
}

impl ChatHistory {
    pub fn new(limits: ChannelLimits) -> Self {
        Self {
            messages: BTreeMap::new(),
            by_channel: HashMap::new(),
            limits,
        }
    }

    /// Changes the limits, dropping the oldest messages of channels that shrank.
    pub fn set_limits(&mut self, limits: ChannelLimits) {
        self.limits = limits;
        let channels: Vec<Channel> = self.by_channel.keys().copied().collect();
        for channel in channels {
            self.evict(channel);
        }
    }

    /// Stores a message, dropping the oldest ones of its channel beyond the
    /// channel's limit.
    pub fn push(&mut self, message: ChatMessage) {
        let (pid, channel) = (message.pid, message.channel);
        if self.messages.insert(pid, message).is_none() {
            self.by_channel.entry(channel).or_default().push_back(pid);
        }
        self.evict(channel);
    }

    /// For in-place updates (translation, blocked flag); the channel must
    /// not be changed through it.
    pub fn get(&self, pid: u64) -> Option<&ChatMessage> {
        self.messages.get(&pid)
    }

    pub fn get_mut(&mut self, pid: u64) -> Option<&mut ChatMessage> {
        self.messages.get_mut(&pid)
    }

    /// Oldest first.
    pub fn values(&self) -> impl Iterator<Item = &ChatMessage> {
        self.messages.values()
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut ChatMessage> {
        self.messages.values_mut()
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.by_channel.clear();
    }

    /// The newest message of a channel always stays.
    fn evict(&mut self, channel: Channel) {
        let limit = self.limits.of(channel);
        let Some(pids) = self.by_channel.get_mut(&channel) else {
            return;
        };
        while pids.len() > limit {
            if let Some(oldest) = pids.pop_front() {
                self.messages.remove(&oldest);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(pid: u64) -> ChatMessage {
        on("GUILD", pid)
    }

    fn on(channel: &str, pid: u64) -> ChatMessage {
        ChatMessage {
            pid,
            channel: Channel::from_name(channel),
            message: format!("m{pid}"),
            ..Default::default()
        }
    }

    fn limits(pairs: &[(&str, usize)]) -> ChannelLimits {
        ChannelLimits::new(&pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect())
    }

    fn pids(h: &ChatHistory) -> Vec<u64> {
        h.values().map(|m| m.pid).collect()
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("rs-history-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn line(pid: u64, text: &str) -> String {
        line_on("GUILD", pid, text)
    }

    fn line_on(channel: &str, pid: u64, text: &str) -> String {
        serde_json::to_string(&ChatMessage {
            pid,
            channel: Channel::from_name(channel),
            message: text.into(),
            ..Default::default()
        })
        .unwrap()
    }

    #[test]
    fn restored_rows_follow_the_block_list_as_it_is_now_not_as_it_was_when_saved() {
        // Saved while sender 1 was not blocked, and while sender 2 was.
        let mut rows = vec![
            ChatMessage {
                pid: 1,
                uid: 1,
                is_blocked: false,
                ..Default::default()
            },
            ChatMessage {
                pid: 2,
                uid: 2,
                is_blocked: true,
                ..Default::default()
            },
            ChatMessage {
                pid: 3,
                uid: 3,
                is_blocked: false,
                ..Default::default()
            },
            ChatMessage {
                pid: 4,
                uid: 0, // the player's own line: no sender id
                ..Default::default()
            },
        ];
        // Since then: sender 1 was blocked, sender 2 was unblocked.
        apply_block_list(&mut rows, |uid| uid == 1);
        let flags: Vec<bool> = rows.iter().map(|m| m.is_blocked).collect();
        assert_eq!(flags, [true, false, false, false]);
    }

    #[test]
    fn dataset_files_are_one_per_channel_with_safe_names() {
        assert_eq!(dataset_file_name("GUILD"), "dataset_GUILD.jsonl");
        assert_eq!(dataset_file_name("길드 채팅"), "dataset_길드 채팅.jsonl");
        // A channel name is never a path.
        assert_eq!(dataset_file_name("../a\\b:c"), "dataset_.._a_b_c.jsonl");
        assert_eq!(dataset_file_name(""), "dataset_unknown.jsonl");
    }

    #[test]
    fn load_recent_takes_the_newest_across_days() {
        let dir = temp_dir("recent");
        std::fs::write(
            dir.join(chat_log_file_name("2026-09-28")),
            [line(7, "a"), line(8, "b"), line(9, "c")].join("\n"),
        )
        .unwrap();
        std::fs::write(
            dir.join(chat_log_file_name("2026-09-29")),
            [line(1, "d"), "not json".to_string(), line(2, "e")].join("\n") + "\n",
        )
        .unwrap();
        std::fs::write(dir.join("notes.txt"), line(5, "ignored")).unwrap();

        let got = load_recent(&dir, &limits(&[("GUILD", 4)]));
        let texts: Vec<_> = got.iter().map(|m| m.message.as_str()).collect();
        assert_eq!(texts, ["b", "c", "d", "e"]); // oldest first, bad line skipped
        let pids: Vec<_> = got.iter().map(|m| m.pid).collect();
        assert_eq!(pids, [1, 2, 3, 4]); // renumbered: old pids collided (1, 2 vs 7..9)
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_recent_stops_after_its_line_budget() {
        // P1: with no retention, every launch used to read every log ever
        // written. The scan now ends after `max_lines`, newest first.
        let dir = temp_dir("budget");
        std::fs::write(
            dir.join(chat_log_file_name("2026-09-28")),
            [line_on("GUILD", 1, "g1"), line_on("GUILD", 2, "g2")].join("\n"),
        )
        .unwrap();
        let world: Vec<_> = (0..10)
            .map(|i| line_on("WORLD", i, &format!("w{i}")))
            .collect();
        std::fs::write(dir.join(chat_log_file_name("2026-09-29")), world.join("\n")).unwrap();

        let limits = limits(&[("WORLD", 100), ("GUILD", 5)]);
        let got = load_recent_within(&dir, &limits, 10);
        let texts: Vec<_> = got.iter().map(|m| m.message.as_str()).collect();
        assert_eq!(texts.len(), 10, "only the newest 10 lines are examined");
        assert!(texts.iter().all(|t| t.starts_with('w')), "{texts:?}");

        // A budget that covers everything finds the old GUILD lines too.
        assert_eq!(load_recent_within(&dir, &limits, 12).len(), 12);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_recent_fills_each_channel_on_its_own() {
        // A busy WORLD day after a GUILD day: GUILD still comes back.
        let dir = temp_dir("per-channel");
        std::fs::write(
            dir.join(chat_log_file_name("2026-09-28")),
            [line_on("GUILD", 1, "g1"), line_on("GUILD", 2, "g2")].join("\n"),
        )
        .unwrap();
        let world: Vec<_> = (0..10)
            .map(|i| line_on("WORLD", i, &format!("w{i}")))
            .collect();
        std::fs::write(dir.join(chat_log_file_name("2026-09-29")), world.join("\n")).unwrap();

        let got = load_recent(&dir, &limits(&[("WORLD", 3), ("GUILD", 5)]));
        let texts: Vec<_> = got.iter().map(|m| m.message.as_str()).collect();
        assert_eq!(texts, ["g1", "g2", "w7", "w8", "w9"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_recent_without_logs_is_empty() {
        let dir = temp_dir("none");
        assert!(load_recent(&dir.join("missing"), &limits(&[])).is_empty());
        assert!(load_recent(&dir, &limits(&[])).is_empty());
        // No limits configured: the defaults still apply, nothing is lost.
        std::fs::write(
            dir.join(chat_log_file_name("2026-09-29")),
            line_on("WORLD", 1, "w"),
        )
        .unwrap();
        assert_eq!(load_recent(&dir, &limits(&[])).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn channel_limits_ignore_tab_entries_and_default_like_the_ui() {
        let l = limits(&[("전체", 5), ("커스텀", 5), ("GUILD", 0), ("PARTY", 30)]);
        assert_eq!(l.of(Channel::Party), 30);
        assert_eq!(l.of(Channel::Guild), 1); // at least 1
        assert_eq!(l.of(Channel::World), 200);
        assert_eq!(l.of(Channel::Local), 1000);
        // Names that are no channel (the all-tab, a made-up one) limit nothing.
        let ignored = limits(&[("전체", 1), ("TRADE", 1)]);
        assert_eq!(ignored, limits(&[]));
    }

    #[test]
    fn keeps_the_newest_up_to_the_limit() {
        let mut h = ChatHistory::new(limits(&[("GUILD", 3)]));
        for pid in 1..=5 {
            h.push(msg(pid));
        }
        assert_eq!(pids(&h), [3, 4, 5]);
    }

    #[test]
    fn a_busy_channel_does_not_evict_another() {
        let mut h = ChatHistory::new(limits(&[("WORLD", 2), ("GUILD", 2)]));
        h.push(on("GUILD", 1));
        for pid in 2..=10 {
            h.push(on("WORLD", pid));
        }
        assert_eq!(pids(&h), [1, 9, 10]);
    }

    #[test]
    fn shrinking_the_limit_drops_the_oldest() {
        let mut h = ChatHistory::new(limits(&[("GUILD", 10)]));
        for pid in 1..=5 {
            h.push(msg(pid));
        }
        h.set_limits(limits(&[("GUILD", 2)]));
        assert_eq!(pids(&h), [4, 5]);
    }

    #[test]
    fn a_zero_limit_still_keeps_the_latest_message() {
        let mut h = ChatHistory::new(limits(&[("GUILD", 0)]));
        h.push(msg(1));
        h.push(msg(2));
        assert_eq!(pids(&h), [2]);
    }

    #[test]
    fn pushing_a_pid_again_replaces_it_once() {
        let mut h = ChatHistory::new(limits(&[("GUILD", 2)]));
        h.push(msg(1));
        h.push(msg(1));
        h.push(msg(2));
        assert_eq!(pids(&h), [1, 2]);
    }

    #[test]
    fn get_mut_updates_in_place() {
        let mut h = ChatHistory::new(limits(&[("GUILD", 3)]));
        h.push(msg(1));
        h.get_mut(1).unwrap().translated = Some("번역".into());
        assert_eq!(
            h.values().next().unwrap().translated.as_deref(),
            Some("번역")
        );
        assert!(h.get_mut(99).is_none());
    }

    #[test]
    fn load_recent_keeps_the_newest_line_of_a_message_saved_twice() {
        let dir = temp_dir("twice");
        let saved = |translated: Option<&str>, seq: u64| {
            serde_json::to_string(&ChatMessage {
                uid: 9,
                timestamp: 1000,
                sequence_id: seq,
                channel: Channel::Guild,
                message: "こんにちは".into(),
                translated: translated.map(Into::into),
                ..Default::default()
            })
            .unwrap()
        };
        std::fs::write(
            dir.join(chat_log_file_name("2026-09-30")),
            [
                saved(None, 1),         // archived untranslated
                saved(None, 2),         // another message
                saved(Some("안녕"), 1), // the catch-up's translated copy
                line_on("GUILD", 0, "no identity"),
                line_on("GUILD", 0, "no identity"),
            ]
            .join("\n"),
        )
        .unwrap();
        let got = load_recent(&dir, &limits(&[]));
        let seqs: Vec<_> = got
            .iter()
            .map(|m| (m.sequence_id, m.translated.clone()))
            .collect();
        assert_eq!(
            seqs,
            [(2, None), (1, Some("안녕".into())), (0, None), (0, None)]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn log_dir_with(name: &str, files: &[&str]) -> std::path::PathBuf {
        let dir = temp_dir(name);
        for file in files {
            std::fs::write(dir.join(file), "").unwrap();
        }
        dir
    }

    fn names(paths: &[std::path::PathBuf]) -> Vec<String> {
        paths
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn retention_keeps_today_and_the_days_before_it() {
        let dir = log_dir_with(
            "retention",
            &[
                "2026-09-27.jsonl",
                "2026-09-28.jsonl",
                "2026-09-29.jsonl",
                "2026-09-30.jsonl",
                "notes.jsonl",    // not a date: never touched
                "2026-01-01.txt", // not a log
            ],
        );
        let today = day("2026-09-30");
        assert_eq!(
            names(&expired_chat_logs(&dir, today, 2)),
            ["2026-09-27.jsonl", "2026-09-28.jsonl"]
        );
        assert_eq!(names(&expired_chat_logs(&dir, today, 1)).len(), 3); // today only
        assert!(expired_chat_logs(&dir, today, 4).is_empty());
        assert!(expired_chat_logs(&dir, today, 0).is_empty()); // 0 = keep all
        assert!(expired_chat_logs(&dir.join("missing"), today, 1).is_empty());

        assert_eq!(remove_expired_chat_logs(&dir, today, 2), 2);
        assert!(!dir.join("2026-09-28.jsonl").exists());
        assert!(dir.join("2026-09-29.jsonl").exists());
        assert!(dir.join("notes.jsonl").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn retention_crosses_month_and_year_boundaries() {
        let dir = log_dir_with("retention-year", &["2025-12-31.jsonl", "2026-01-01.jsonl"]);
        assert_eq!(
            names(&expired_chat_logs(&dir, day("2026-01-02"), 2)),
            ["2025-12-31.jsonl"]
        );
        // A huge setting cannot underflow the calendar.
        assert!(expired_chat_logs(&dir, day("2026-01-02"), u32::MAX).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
