use crate::protocol::types::ChatMessage;
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};

pub enum ProcessAction {
    IgnoreDuplicate,
    EmitNewMessage,
    UpdateBlockedMessage,
}

pub struct MessageProcessor {
    pub dedup_cache: HashSet<u64>,
}

impl MessageProcessor {
    pub fn new() -> Self {
        Self {
            dedup_cache: HashSet::new(),
        }
    }

    /// Pure logic: Determines what to do with a chat message without mutating global state.
    /// Takes blocked_users as a reference so it always has the latest UI state.
    pub fn process(
        &mut self,
        chat: &mut ChatMessage,
        blocked_users: &HashMap<u64, String>,
    ) -> ProcessAction {

        // 1. GENERATE DETERMINISTIC PID
        let mut hasher = DefaultHasher::new();
        (chat.uid, chat.timestamp, chat.sequence_id).hash(&mut hasher);
        chat.pid = hasher.finish() & 0x1FFFFFFFFFFFFF;

        // 2. Check if Blocked
        if blocked_users.contains_key(&chat.uid) {
            chat.is_blocked = true;
        }

        // 3. Deduplication Check using the new PID
        if self.dedup_cache.contains(&chat.pid) {
            if chat.is_blocked {
                return ProcessAction::UpdateBlockedMessage;
            }
            return ProcessAction::IgnoreDuplicate;
        }

        ProcessAction::EmitNewMessage
    }

    /// Registers a successfully emitted message into the duplicate cache
    pub fn commit_new_message(&mut self, chat: &ChatMessage) {
        self.dedup_cache.insert(chat.pid);

        // Prevent memory leaks over an 8-hour gaming session!
        if self.dedup_cache.len() > 2000 {
            self.dedup_cache.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::types::ChatMessage;

    #[test]
    fn test_message_processor_deduplication() {
        let mut processor = MessageProcessor::new();
        let blocked = HashMap::new(); // Empty blocked list for this test

        let mut msg1 = ChatMessage {
            uid: 100,
            timestamp: 5000,
            sequence_id: 1,
            pid: 1,
            ..Default::default()
        };
        let mut msg2 = ChatMessage {
            uid: 100,
            timestamp: 5000,
            sequence_id: 1,
            pid: 2,
            ..Default::default()
        }; // Exact duplicate signature

        // First message should be evaluated as new
        match processor.process(&mut msg1, &blocked) {
            ProcessAction::EmitNewMessage => processor.commit_new_message(&msg1),
            _ => panic!("Expected new message"),
        }

        // Second message should be ignored
        match processor.process(&mut msg2, &blocked) {
            ProcessAction::IgnoreDuplicate => {} // Success!
            _ => panic!("Expected duplicate to be ignored"),
        }
    }

    #[test]
    fn test_message_processor_blocking() {
        let mut processor = MessageProcessor::new();

        let mut blocked = HashMap::new();
        blocked.insert(999, "Spammer".to_string());

        let mut msg = ChatMessage {
            uid: 999,
            message: "Buy gold!".to_string(),
            ..Default::default()
        };

        // Should evaluate as new, but automatically flag the mutable chat reference as blocked
        match processor.process(&mut msg, &blocked) {
            ProcessAction::EmitNewMessage => {
                assert_eq!(msg.is_blocked, true);
            }
            _ => panic!("Expected new message"),
        }
    }
}
