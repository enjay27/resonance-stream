//! Backend chat history: the last `limit` messages, keyed by pid.

use resonance_types::ChatMessage;
use std::collections::BTreeMap;
use std::path::Path;

/// Daily chat log file for `date` (YYYY-MM-DD) inside the chat_logs folder.
pub fn chat_log_file_name(date: &str) -> String {
    format!("{}.jsonl", date)
}

/// The newest `limit` messages saved in `dir` (one JSON `ChatMessage` per
/// line, one `.jsonl` file per day), oldest first. Pids are renumbered
/// 1..=n in that order: saved pids come from earlier runs and may collide,
/// while new messages must sort after the loaded ones.
pub fn load_recent(dir: &Path, limit: usize) -> Vec<ChatMessage> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<_> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .collect();
    files.sort(); // YYYY-MM-DD names: sorted by day

    // Newest day first, newest line first, until `limit` messages.
    let mut newest_first = Vec::new();
    for file in files.iter().rev() {
        if newest_first.len() >= limit {
            break;
        }
        let Ok(content) = std::fs::read_to_string(file) else {
            continue;
        };
        for line in content.lines().rev() {
            if newest_first.len() >= limit {
                break;
            }
            if let Ok(message) = serde_json::from_str::<ChatMessage>(line) {
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

/// Pids come from one increasing counter, so ordering by pid is arrival
/// order, and dropping the oldest message is `pop_first` (O(log n)) instead
/// of shifting every entry of an insertion-ordered map.
#[derive(Debug, Default)]
pub struct ChatHistory {
    messages: BTreeMap<u64, ChatMessage>,
    limit: usize,
}

impl ChatHistory {
    pub fn new(limit: usize) -> Self {
        Self {
            messages: BTreeMap::new(),
            limit,
        }
    }

    /// Changes the limit, dropping the oldest messages if it shrank.
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
        self.evict();
    }

    /// Stores a message, dropping the oldest ones beyond the limit.
    pub fn push(&mut self, message: ChatMessage) {
        self.messages.insert(message.pid, message);
        self.evict();
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
    }

    /// The newest message always stays, even with a limit of 0.
    fn evict(&mut self) {
        while self.messages.len() > self.limit.max(1) {
            self.messages.pop_first();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(pid: u64) -> ChatMessage {
        ChatMessage {
            pid,
            message: format!("m{pid}"),
            ..Default::default()
        }
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
        serde_json::to_string(&ChatMessage {
            pid,
            message: text.into(),
            ..Default::default()
        })
        .unwrap()
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

        let got = load_recent(&dir, 4);
        let texts: Vec<_> = got.iter().map(|m| m.message.as_str()).collect();
        assert_eq!(texts, ["b", "c", "d", "e"]); // oldest first, bad line skipped
        let pids: Vec<_> = got.iter().map(|m| m.pid).collect();
        assert_eq!(pids, [1, 2, 3, 4]); // renumbered: old pids collided (1, 2 vs 7..9)
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_recent_without_logs_is_empty() {
        let dir = temp_dir("none");
        assert!(load_recent(&dir.join("missing"), 10).is_empty());
        assert!(load_recent(&dir, 0).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keeps_the_newest_up_to_the_limit() {
        let mut h = ChatHistory::new(3);
        for pid in 1..=5 {
            h.push(msg(pid));
        }
        assert_eq!(pids(&h), [3, 4, 5]);
    }

    #[test]
    fn shrinking_the_limit_drops_the_oldest() {
        let mut h = ChatHistory::new(10);
        for pid in 1..=5 {
            h.push(msg(pid));
        }
        h.set_limit(2);
        assert_eq!(pids(&h), [4, 5]);
    }

    #[test]
    fn a_zero_limit_still_keeps_the_latest_message() {
        // Matches the old behaviour: evict first, then insert.
        let mut h = ChatHistory::new(0);
        h.push(msg(1));
        h.push(msg(2));
        assert_eq!(pids(&h), [2]);
    }

    #[test]
    fn get_mut_updates_in_place() {
        let mut h = ChatHistory::new(3);
        h.push(msg(1));
        h.get_mut(1).unwrap().translated = Some("번역".into());
        assert_eq!(
            h.values().next().unwrap().translated.as_deref(),
            Some("번역")
        );
        assert!(h.get_mut(99).is_none());
    }
}
