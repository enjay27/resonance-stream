//! Backend chat history: the last `limit` messages, keyed by pid.

use resonance_types::ChatMessage;
use std::collections::BTreeMap;

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
