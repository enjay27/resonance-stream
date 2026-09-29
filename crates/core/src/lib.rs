//! Platform-independent logic of Resonance Stream: turning captured TCP
//! payloads into chat messages, and preparing text for / cleaning text from
//! the translation model. No Tauri, no Windows APIs — builds and tests on any OS.

pub mod capture;
pub mod protocol;
pub mod text;
