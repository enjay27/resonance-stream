//! Raw IPv4/TCP packet → chat-message pipeline (reassembly, parsing, dedup, blocking).

mod message_processor;
mod pipeline;
mod stream_tracker;

pub use self::pipeline::{ChatPipeline, PipelineAction};
