//! Asks a llama-server (or a stand-in) at `<url>` to translate one line with the app's own client, and prints
//! the result: `ready` / `not ready` for `/health`, then the translation or the error. The test of the stand-in
//! server (`runbook/tests/test_llama_stub.py`) runs it, so the stand-in is proven against the real client.
//!
//!     cargo run -q -p resonance-llama --example translate_once -- http://127.0.0.1:8080 "こんにちは"

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [url, text] = args.as_slice() else {
        eprintln!("usage: translate_once <url> <japanese text>");
        return ExitCode::from(2);
    };
    let client = resonance_llama::client();
    println!(
        "{}",
        if resonance_llama::health_ok(&client, url) {
            "ready"
        } else {
            "not ready"
        }
    );
    match resonance_llama::translate_text(&client, url, text) {
        Ok(korean) => {
            println!("{korean}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            println!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
