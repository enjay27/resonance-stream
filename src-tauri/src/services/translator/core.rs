use std::sync::atomic::{AtomicU16, Ordering};

/// Port tried first for llama-server; another is picked if it is taken.
pub const PREFERRED_SERVER_PORT: u16 = 8080;
static SERVER_PORT: AtomicU16 = AtomicU16::new(PREFERRED_SERVER_PORT);

/// Base URL of the running llama-server (the port is chosen at launch), or the stand-in a test run
/// named with `--llama-url`.
pub fn server_url() -> String {
    if let Some(url) = crate::test_env::llama_url() {
        return url.to_string();
    }
    format!("http://127.0.0.1:{}", SERVER_PORT.load(Ordering::Relaxed))
}

pub(crate) fn set_server_port(port: u16) {
    SERVER_PORT.store(port, Ordering::Relaxed);
}
