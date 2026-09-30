use resonance_types::ChatMessage;
use std::collections::{HashMap, VecDeque};

/// Signatures remembered for duplicate detection. Duplicates arrive close
/// together (retransmits, a second client), so the most recent few thousand
/// are plenty; older ones are forgotten.
pub const DEFAULT_DEDUP_CAPACITY: usize = 4096;

type Signature = (u64, u64, u64);

pub enum ProcessAction {
    IgnoreDuplicate,
    EmitNewMessage,
    UpdateBlockedMessage,
}

/// Key for the short-window duplicate check in the app (a second game client
/// sending the same message), or `None` for a message with no identity: "Me"
/// messages have no uid or timestamp, and the same text twice is legitimate.
pub fn fingerprint(chat: &ChatMessage) -> Option<u64> {
    use std::hash::{DefaultHasher, Hash, Hasher};
    if chat.uid == 0 && chat.timestamp == 0 {
        return None;
    }
    let mut hasher = DefaultHasher::new();
    chat.uid.hash(&mut hasher);
    chat.message.hash(&mut hasher);
    chat.timestamp.hash(&mut hasher);
    Some(hasher.finish())
}

pub struct MessageProcessor {
    dedup_cache: HashMap<Signature, u64>,
    order: VecDeque<Signature>,
    capacity: usize,
}

impl MessageProcessor {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_DEDUP_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            dedup_cache: HashMap::with_capacity(capacity),
            order: VecDeque::with_capacity(capacity),
            capacity: capacity.max(1),
        }
    }

    /// Number of remembered signatures.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.dedup_cache.len()
    }

    /// The duplicate-detection key, or `None` when the message carries no
    /// identity (field-4 "Me" messages have no timestamp or sequence id, so
    /// every one of them would share one key).
    fn signature(chat: &ChatMessage) -> Option<Signature> {
        if chat.timestamp == 0 && chat.sequence_id == 0 {
            return None;
        }
        Some((chat.uid, chat.timestamp, chat.sequence_id))
    }

    /// Pure logic: Determines what to do with a chat message without mutating global state.
    /// `is_blocked` is asked once per message, so it always sees the latest block list.
    pub fn process(
        &self,
        chat: &mut ChatMessage,
        is_blocked: &dyn Fn(u64) -> bool,
    ) -> ProcessAction {
        if is_blocked(chat.uid) {
            chat.is_blocked = true;
        }

        let Some(signature) = Self::signature(chat) else {
            return ProcessAction::EmitNewMessage;
        };

        if let Some(&existing_pid) = self.dedup_cache.get(&signature) {
            if chat.is_blocked {
                chat.pid = existing_pid; // Carry over the original PID so the UI updates the correct row
                return ProcessAction::UpdateBlockedMessage;
            }
            return ProcessAction::IgnoreDuplicate;
        }

        ProcessAction::EmitNewMessage
    }

    /// Registers a successfully emitted message into the duplicate cache,
    /// forgetting the oldest signature once the cache is full.
    pub fn commit_new_message(&mut self, chat: &ChatMessage) {
        let Some(signature) = Self::signature(chat) else {
            return;
        };
        if self.dedup_cache.insert(signature, chat.pid).is_none() {
            self.order.push_back(signature);
        }
        while self.order.len() > self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.dedup_cache.remove(&oldest);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use resonance_types::ChatMessage;

    #[test]
    fn test_message_processor_deduplication() {
        let mut processor = MessageProcessor::new();
        let blocked: HashMap<u64, String> = HashMap::new(); // Empty blocked list for this test

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
        match processor.process(&mut msg1, &|uid| blocked.contains_key(&uid)) {
            ProcessAction::EmitNewMessage => processor.commit_new_message(&msg1),
            _ => panic!("Expected new message"),
        }

        // Second message should be ignored
        match processor.process(&mut msg2, &|uid| blocked.contains_key(&uid)) {
            ProcessAction::IgnoreDuplicate => {} // Success!
            _ => panic!("Expected duplicate to be ignored"),
        }
    }

    #[test]
    fn test_message_processor_blocking() {
        let processor = MessageProcessor::new();

        let mut blocked: HashMap<u64, String> = HashMap::new();
        blocked.insert(999, "Spammer".to_string());

        let mut msg = ChatMessage {
            uid: 999,
            message: "Buy gold!".to_string(),
            ..Default::default()
        };

        // Should evaluate as new, but automatically flag the mutable chat reference as blocked
        match processor.process(&mut msg, &|uid| blocked.contains_key(&uid)) {
            ProcessAction::EmitNewMessage => {
                assert_eq!(msg.is_blocked, true);
            }
            _ => panic!("Expected new message"),
        }
    }
    fn me(text: &str) -> ChatMessage {
        // Field-4 ("Me") messages carry no uid, timestamp or sequence id.
        ChatMessage {
            nickname: "Me".into(),
            message: text.into(),
            ..Default::default()
        }
    }

    #[test]
    fn fingerprint_ignores_identityless_messages_and_tells_others_apart() {
        // Regression (N9): the same "Me" text twice in 2 s was dropped.
        assert_eq!(fingerprint(&me("ok")), None);
        let a = ChatMessage {
            uid: 1,
            timestamp: 5,
            message: "hi".into(),
            ..Default::default()
        };
        let b = ChatMessage {
            message: "yo".into(),
            ..a.clone()
        };
        assert_eq!(fingerprint(&a), fingerprint(&a.clone()));
        assert_ne!(fingerprint(&a), fingerprint(&b));
    }

    #[test]
    fn messages_without_identity_are_never_deduplicated() {
        // Regression (review B3): all "Me" messages share the key (0, 0, 0).
        let mut processor = MessageProcessor::new();
        let blocked: HashMap<u64, String> = HashMap::new();
        for text in ["one", "two", "three"] {
            let mut msg = me(text);
            match processor.process(&mut msg, &|uid| blocked.contains_key(&uid)) {
                ProcessAction::EmitNewMessage => processor.commit_new_message(&msg),
                _ => panic!("'{text}' was dropped as a duplicate"),
            }
        }
    }

    #[test]
    fn dedup_cache_is_bounded_and_forgets_the_oldest() {
        let mut processor = MessageProcessor::with_capacity(3);
        let blocked: HashMap<u64, String> = HashMap::new();
        let msg = |seq| ChatMessage {
            uid: 7,
            timestamp: 1,
            sequence_id: seq,
            pid: seq,
            ..Default::default()
        };
        for seq in 1..=4 {
            processor.commit_new_message(&msg(seq));
        }
        assert_eq!(processor.len(), 3);
        // seq 1 was evicted, so it is new again; seq 4 is still a duplicate.
        assert!(matches!(
            processor.process(&mut msg(1), &|uid| blocked.contains_key(&uid)),
            ProcessAction::EmitNewMessage
        ));
        assert!(matches!(
            processor.process(&mut msg(4), &|uid| blocked.contains_key(&uid)),
            ProcessAction::IgnoreDuplicate
        ));
    }
}
