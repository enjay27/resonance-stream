use std::collections::HashMap;
use crate::protocol::types::ChatMessage;

pub enum ProcessAction {
    IgnoreDuplicate,
    EmitNewMessage,
    UpdateBlockedMessage,
}

pub struct MessageProcessor {
    pub dedup_cache: HashMap<(u64, u64, u64), u64>,
}

impl MessageProcessor {
    pub fn new() -> Self {
        Self { dedup_cache: HashMap::new() }
    }

    /// Pure logic: Determines what to do with a chat message without mutating global state.
    /// Takes blocked_users as a reference so it always has the latest UI state.
    pub fn process(&self, chat: &mut ChatMessage, blocked_users: &HashMap<u64, String>) -> ProcessAction {
        if blocked_users.contains_key(&chat.uid) {
            chat.is_blocked = true;
        }

        let signature = (chat.uid, chat.timestamp, chat.sequence_id);

        if let Some(&existing_pid) = self.dedup_cache.get(&signature) {
            if chat.is_blocked {
                chat.pid = existing_pid; // Carry over the original PID so the UI updates the correct row
                return ProcessAction::UpdateBlockedMessage;
            }
            return ProcessAction::IgnoreDuplicate;
        }

        ProcessAction::EmitNewMessage
    }

    /// Registers a successfully emitted message into the duplicate cache
    pub fn commit_new_message(&mut self, chat: &ChatMessage) {
        let signature = (chat.uid, chat.timestamp, chat.sequence_id);
        self.dedup_cache.insert(signature, chat.pid);
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

        let mut msg1 = ChatMessage { uid: 100, timestamp: 5000, sequence_id: 1, pid: 1, ..Default::default() };
        let mut msg2 = ChatMessage { uid: 100, timestamp: 5000, sequence_id: 1, pid: 2, ..Default::default() }; // Exact duplicate signature

        // First message should be evaluated as new
        match processor.process(&mut msg1, &blocked) {
            ProcessAction::EmitNewMessage => processor.commit_new_message(&msg1),
            _ => panic!("Expected new message"),
        }

        // Second message should be ignored
        match processor.process(&mut msg2, &blocked) {
            ProcessAction::IgnoreDuplicate => {}, // Success!
            _ => panic!("Expected duplicate to be ignored"),
        }
    }

    #[test]
    fn test_message_processor_blocking() {
        let processor = MessageProcessor::new();

        let mut blocked = HashMap::new();
        blocked.insert(999, "Spammer".to_string());

        let mut msg = ChatMessage { uid: 999, message: "Buy gold!".to_string(), ..Default::default() };

        // Should evaluate as new, but automatically flag the mutable chat reference as blocked
        match processor.process(&mut msg, &blocked) {
            ProcessAction::EmitNewMessage => {
                assert_eq!(msg.is_blocked, true);
            },
            _ => panic!("Expected new message"),
        }
    }
}