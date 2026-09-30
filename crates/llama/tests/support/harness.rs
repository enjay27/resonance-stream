//! What the app does between the two servers, without Tauri: the sniffer
//! worker's pipeline and dispatch (`sniffer::dispatch_pipeline_actions`) and
//! the translator worker's job (`translator::process_translation_job`).
//! Keep it in step with those two when they change.

use super::llama_server::MockLlama;
use reqwest::blocking::Client;
use resonance_core::capture::{ChatPipeline, PipelineAction};
use resonance_core::text::{
    contains_japanese, convert_to_romaji, preprocess_text, translate_masked, Dictionary,
    TranslationCache, TRANSLATION_CACHE_SIZE,
};
use resonance_llama::translate_text;
use resonance_types::ChatMessage;
use std::collections::{HashMap, HashSet};

/// The capture side: raw packets in, chat rows out.
pub struct Capture {
    pub pipeline: ChatPipeline,
    pub blocked: HashSet<u64>,
    next_pid: u64,
    /// Updated rows for newly blocked users (`UpdateBlockedMessage`).
    pub blocked_updates: Vec<ChatMessage>,
    /// Japanese nickname -> romaji, as the sniffer fills it.
    pub nicknames: HashMap<String, String>,
}

impl Capture {
    pub fn new() -> Self {
        Self {
            pipeline: ChatPipeline::new(),
            blocked: HashSet::new(),
            next_pid: 0,
            blocked_updates: Vec::new(),
            nicknames: HashMap::new(),
        }
    }

    /// Every new row the packets produce, in order.
    pub fn feed(&mut self, packets: &[Vec<u8>]) -> Vec<ChatMessage> {
        let mut rows = Vec::new();
        for packet in packets {
            let blocked = &self.blocked;
            let next_pid = &mut self.next_pid;
            let actions = self.pipeline.feed_network_packet(
                packet,
                |uid| blocked.contains(&uid),
                || {
                    *next_pid += 1;
                    *next_pid
                },
                || {},
            );
            for action in actions {
                match action {
                    PipelineAction::EmitNewMessage(mut chat) => {
                        if contains_japanese(&chat.nickname) {
                            chat.nickname_romaji = Some(
                                self.nicknames
                                    .entry(chat.nickname.clone())
                                    .or_insert_with(|| convert_to_romaji(&chat.nickname))
                                    .clone(),
                            );
                        }
                        rows.push(chat);
                    }
                    PipelineAction::UpdateBlockedMessage(chat) => self.blocked_updates.push(chat),
                }
            }
        }
        rows
    }

    pub fn texts(&mut self, packets: &[Vec<u8>]) -> Vec<String> {
        self.feed(packets).into_iter().map(|c| c.message).collect()
    }
}

/// The translator side, talking to a llama-server at `url`.
pub struct Translator {
    pub client: Client,
    pub url: String,
    pub dictionary: Dictionary,
    pub cache: TranslationCache,
}

impl Translator {
    pub fn new(llama: &MockLlama, client: Client) -> Self {
        Self {
            client,
            url: llama.url.clone(),
            dictionary: Dictionary::default(),
            cache: TranslationCache::new(TRANSLATION_CACHE_SIZE),
        }
    }

    /// `process_translation_job` without the ledger, archive and emit.
    pub fn translate(
        &mut self,
        chat: &ChatMessage,
        nicknames: &HashMap<String, String>,
    ) -> Result<String, String> {
        let shield = preprocess_text(&chat.message, &self.dictionary, Some(nicknames));
        let (client, url) = (&self.client, &self.url);
        translate_masked(&shield, &mut self.cache, |masked| {
            translate_text(client, url, masked)
        })
    }
}

/// Both ends wired together: rows with the translation of every Japanese
/// one (the app only queues Japanese rows for the translator).
pub struct Harness {
    pub capture: Capture,
    pub translator: Translator,
}

impl Harness {
    pub fn new(llama: &MockLlama) -> Self {
        Self {
            capture: Capture::new(),
            translator: Translator::new(llama, resonance_llama::client()),
        }
    }

    pub fn run(&mut self, packets: &[Vec<u8>]) -> Vec<ChatMessage> {
        let mut rows = self.capture.feed(packets);
        for row in &mut rows {
            if contains_japanese(&row.message) {
                row.translated = self.translator.translate(row, &self.capture.nicknames).ok();
            }
        }
        rows
    }
}
