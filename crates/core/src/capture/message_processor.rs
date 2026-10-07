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

pub struct MessageProcessor {
    dedup_cache: HashMap<Signature, u64>,
    order: VecDeque<Signature>,
    /// The same words from a second game client (see [`MessageProcessor::content_key`]): key -> pid.
    content_cache: HashMap<u64, u64>,
    content_order: VecDeque<u64>,
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
            content_cache: HashMap::with_capacity(capacity),
            content_order: VecDeque::with_capacity(capacity),
            capacity: capacity.max(1),
        }
    }

    /// Makes room for at least `capacity` signatures (never shrinks).
    pub fn grow_to(&mut self, capacity: usize) {
        self.capacity = self.capacity.max(capacity);
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

    /// Key for the same message sent by a second game client: its own sequence id differs, but the
    /// sender, the words and the server's send time (Unix seconds) do not, so the three together
    /// name one message whenever it arrives. `None` without a send time: nothing then says two
    /// messages are the same one, and the same text twice is legitimate ("Me" messages have no
    /// uid or timestamp either).
    fn content_key(chat: &ChatMessage) -> Option<u64> {
        use std::hash::{DefaultHasher, Hash, Hasher};
        if chat.timestamp == 0 {
            return None;
        }
        let mut hasher = DefaultHasher::new();
        chat.uid.hash(&mut hasher);
        chat.message.hash(&mut hasher);
        chat.timestamp.hash(&mut hasher);
        Some(hasher.finish())
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

        // Seen before: by its own signature, or as the same words from a second client.
        let seen = self
            .dedup_cache
            .get(&signature)
            .or_else(|| Self::content_key(chat).and_then(|key| self.content_cache.get(&key)));
        if let Some(&existing_pid) = seen {
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
        if let Some(key) = Self::content_key(chat) {
            if self.content_cache.insert(key, chat.pid).is_none() {
                self.content_order.push_back(key);
            }
            while self.content_order.len() > self.capacity {
                if let Some(oldest) = self.content_order.pop_front() {
                    self.content_cache.remove(&oldest);
                }
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
    // --- the same message from a second game client (review R-3) -----------------------------
    // The pipeline's `(uid, timestamp, sequence_id)` key cannot catch it: the sequence id is the
    // client's own. The server's send time is in Unix seconds, so the same uid, text and
    // timestamp is the same message whenever it arrives -- no clock needed.

    fn said(uid: u64, text: &str, timestamp: u64, sequence_id: u64) -> ChatMessage {
        ChatMessage {
            uid,
            message: text.into(),
            timestamp,
            sequence_id,
            ..Default::default()
        }
    }

    fn emitted(processor: &mut MessageProcessor, mut chat: ChatMessage, pid: u64) -> bool {
        match processor.process(&mut chat, &|_| false) {
            ProcessAction::EmitNewMessage => {
                chat.pid = pid;
                processor.commit_new_message(&chat);
                true
            }
            _ => false,
        }
    }

    #[test]
    fn a_second_clients_copy_of_a_message_is_ignored() {
        let mut p = MessageProcessor::new();
        assert!(emitted(&mut p, said(7, "hello", 1_772_343_736, 11), 1));
        // Same uid, text and send time; the other client's own sequence id.
        assert!(!emitted(&mut p, said(7, "hello", 1_772_343_736, 94), 2));
    }

    #[test]
    fn a_late_copy_is_ignored_too_however_much_came_between() {
        let mut p = MessageProcessor::new();
        assert!(emitted(&mut p, said(7, "hello", 100, 1), 1));
        for n in 0..50u64 {
            assert!(emitted(&mut p, said(8, "other", 101 + n, 100 + n), 2 + n));
        }
        assert!(!emitted(&mut p, said(7, "hello", 100, 999), 60));
    }

    #[test]
    fn the_same_words_at_another_time_or_by_another_sender_are_new() {
        let mut p = MessageProcessor::new();
        assert!(emitted(&mut p, said(7, "hello", 100, 1), 1));
        assert!(
            emitted(&mut p, said(7, "hello", 101, 2), 2),
            "a later send of the same words"
        );
        assert!(
            emitted(&mut p, said(8, "hello", 100, 3), 3),
            "another sender"
        );
        assert!(
            emitted(&mut p, said(7, "hello!", 100, 4), 4),
            "other words in the same second"
        );
    }

    #[test]
    fn a_message_with_no_send_time_is_not_matched_by_its_words() {
        // Nothing says two such messages are the same one: only the sequence id can.
        let mut p = MessageProcessor::new();
        assert!(emitted(&mut p, said(7, "hello", 0, 1), 1));
        assert!(emitted(&mut p, said(7, "hello", 0, 2), 2));
        assert!(
            !emitted(&mut p, said(7, "hello", 0, 2), 3),
            "the same sequence id still is"
        );
    }

    #[test]
    fn the_same_me_text_twice_is_still_two_messages() {
        let mut p = MessageProcessor::new();
        assert!(emitted(&mut p, me("ok"), 1));
        assert!(emitted(&mut p, me("ok"), 2));
    }

    #[test]
    fn the_words_are_forgotten_with_the_oldest_signatures() {
        let mut p = MessageProcessor::with_capacity(2);
        assert!(emitted(&mut p, said(7, "hello", 100, 1), 1));
        assert!(emitted(&mut p, said(8, "a", 101, 2), 2));
        assert!(emitted(&mut p, said(8, "b", 102, 3), 3));
        assert!(
            emitted(&mut p, said(7, "hello", 100, 50), 4),
            "past the memory, it is new again"
        );
    }

    #[test]
    fn a_blocked_senders_second_copy_updates_the_row_it_already_has() {
        let mut p = MessageProcessor::new();
        let mut first = said(7, "spam", 100, 1);
        assert!(matches!(
            p.process(&mut first, &|uid| uid == 7),
            ProcessAction::EmitNewMessage
        ));
        first.pid = 5;
        p.commit_new_message(&first);
        let mut copy = said(7, "spam", 100, 90);
        match p.process(&mut copy, &|uid| uid == 7) {
            ProcessAction::UpdateBlockedMessage => assert_eq!(copy.pid, 5),
            _ => panic!("expected an update of pid 5"),
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
        // Different words each: the same words from one sender at one time would be one message
        // from a second client, and this test is about the signatures.
        let msg = |seq| ChatMessage {
            uid: 7,
            timestamp: 1,
            sequence_id: seq,
            pid: seq,
            message: format!("m{seq}"),
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
