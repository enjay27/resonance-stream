//! resonance-llama against a mock llama-server: what the server can answer,
//! and what the client makes of it.

mod support;

use reqwest::blocking::Client;
use resonance_core::text::output_token_limit;
use resonance_llama::{client, client_with, health_ok, translate_text, REQUEST_TIMEOUT};
use std::time::Duration;
use support::llama_server::{MockLlama, Reply};

#[test]
fn a_reply_is_trimmed() {
    let llama = MockLlama::start();
    llama.content("  116 정찰 우측 은나포 \n");
    assert_eq!(
        translate_text(&Client::new(), &llama.url, "116　偵察右　銀なぽ").as_deref(),
        Ok("116 정찰 우측 은나포")
    );
}

#[test]
fn the_request_is_a_completion_with_the_prompt_and_an_output_limit() {
    let llama = MockLlama::start();
    llama.content("안녕");
    translate_text(&Client::new(), &llama.url, "[P0]こんにちは").unwrap();

    let requests = llama.completions();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "POST");
    let body = requests[0].json();
    let prompt = body["prompt"].as_str().unwrap();
    assert!(prompt.starts_with("<bos><start_of_turn>user\n"));
    assert!(prompt.ends_with("[P0]こんにちは<end_of_turn>\n<start_of_turn>model\n"));
    assert_eq!(body["n_predict"], output_token_limit("[P0]こんにちは"));
    assert_eq!(llama.prompted_lines(), ["[P0]こんにちは"]);
}

#[test]
fn unicode_in_and_out_survives_the_round_trip() {
    let llama = MockLlama::start();
    llama.content("좋아요 👍 「확인」");
    assert_eq!(
        translate_text(&Client::new(), &llama.url, "いいね👍「確認」").as_deref(),
        Ok("좋아요 👍 「확인」")
    );
    assert_eq!(llama.prompted_lines(), ["いいね👍「確認」"]);
}

#[test]
fn one_client_serves_many_requests_in_a_row() {
    let llama = MockLlama::start();
    llama.content("하나").content("둘").content("셋");
    let client = Client::new();
    let got: Vec<_> = ["一", "二", "三"]
        .iter()
        .map(|t| translate_text(&client, &llama.url, t).unwrap())
        .collect();
    assert_eq!(got, ["하나", "둘", "셋"]);
}

#[test]
fn an_error_status_is_an_error() {
    // llama-server's error body carries no `content`.
    for status in [500, 503] {
        let llama = MockLlama::start();
        llama.reply([Reply::Status(status)]);
        let err = translate_text(&Client::new(), &llama.url, "テスト").unwrap_err();
        assert!(err.contains("no content"), "{status}: {err}");
    }
}

#[test]
fn a_reply_that_is_not_json_is_an_error() {
    let llama = MockLlama::start();
    llama.reply([Reply::Body("<html>Bad Gateway</html>".into())]);
    let err = translate_text(&Client::new(), &llama.url, "テスト").unwrap_err();
    assert!(err.contains("unreadable"), "{err}");
}

#[test]
fn a_reply_without_string_content_is_an_error() {
    for body in [
        r#"{"stop":true}"#,
        r#"{"content":null}"#,
        r#"{"content":42}"#,
        "[]",
    ] {
        let llama = MockLlama::start();
        llama.reply([Reply::Body(body.into())]);
        let err = translate_text(&Client::new(), &llama.url, "テスト").unwrap_err();
        assert!(err.contains("no content"), "{body}: {err}");
    }
}

#[test]
fn a_reply_cut_off_mid_body_is_an_error() {
    let llama = MockLlama::start();
    llama.reply([Reply::CutOff]);
    assert!(translate_text(&Client::new(), &llama.url, "テスト").is_err());
}

#[test]
fn a_connection_closed_without_a_reply_is_an_error() {
    let llama = MockLlama::start();
    llama.reply([Reply::Close]);
    let err = translate_text(&Client::new(), &llama.url, "テスト").unwrap_err();
    assert!(err.contains("connection error"), "{err}");
}

#[test]
fn a_server_that_is_not_running_is_an_error() {
    // Bind and drop: nothing listens on that port any more.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let err = translate_text(
        &Client::new(),
        &format!("http://127.0.0.1:{port}"),
        "テスト",
    )
    .unwrap_err();
    assert!(err.contains("connection error"), "{err}");
}

#[test]
fn a_slow_reply_still_arrives() {
    let llama = MockLlama::start();
    llama.reply([Reply::Slow(Duration::from_millis(300), "늦었다".into())]);
    assert_eq!(
        translate_text(&Client::new(), &llama.url, "遅い").as_deref(),
        Ok("늦었다")
    );
}

#[test]
fn health_is_not_ok_while_the_model_loads() {
    let llama = MockLlama::start();
    llama.health([503, 503]);
    let client = Client::new();
    assert!(!health_ok(&client, &llama.url));
    assert!(!health_ok(&client, &llama.url));
    assert!(health_ok(&client, &llama.url));
}

#[test]
fn health_is_not_ok_without_a_server() {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    assert!(!health_ok(
        &Client::new(),
        &format!("http://127.0.0.1:{port}")
    ));
}

/// Runs `f` on its own thread; `None` if it has not returned after `limit`.
fn within<T: Send + 'static>(limit: Duration, f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(limit).ok()
}

#[test]
fn a_hung_server_is_an_error_once_the_timeout_passes() {
    // llama-server can accept a request and never answer (a stuck GPU
    // driver). Without a timeout the translator worker waits forever and
    // the supervisor never sees the failures that would restart it.
    let llama = MockLlama::start();
    llama.reply([Reply::Hang]);
    let url = llama.url.clone();
    let client = client_with(Duration::from_millis(300));
    let got = within(Duration::from_secs(5), move || {
        translate_text(&client, &url, "テスト")
    })
    .expect("translate_text returned within 5 s");
    assert!(got.is_err());
}

#[test]
fn a_hung_health_check_is_not_ok_once_the_timeout_passes() {
    let llama = MockLlama::start();
    llama.health([0]); // any status the mock does not know: hang
    let url = llama.url.clone();
    let client = client_with(Duration::from_millis(300));
    let healthy = within(Duration::from_secs(5), move || health_ok(&client, &url))
        .expect("health_ok returned within 5 s");
    assert!(!healthy);
}

#[test]
fn an_empty_reply_is_an_error() {
    // An empty translation would show as a blank row and settle the line;
    // as an error the line stays owed and is retried.
    for text in ["", "   ", "\n"] {
        let llama = MockLlama::start();
        llama.content(text);
        let err = translate_text(&Client::new(), &llama.url, "テスト").unwrap_err();
        assert!(err.contains("empty"), "{text:?}: {err}");
    }
}

#[test]
fn a_hung_server_fails_jobs_while_their_lines_are_still_fresh() {
    // Several failures in a row restart the server; they must come before
    // the queued lines go stale.
    let hung_after = REQUEST_TIMEOUT * resonance_core::workers::HUNG_AFTER_FAILURES;
    assert!(REQUEST_TIMEOUT < resonance_core::workers::MAX_TRANSLATION_WAIT);
    assert!(hung_after <= Duration::from_secs(90), "{hung_after:?}");
}

#[test]
fn the_translator_client_talks_to_the_server() {
    let llama = MockLlama::start();
    llama.content("됐다");
    let client = client();
    assert!(health_ok(&client, &llama.url));
    assert_eq!(
        translate_text(&client, &llama.url, "できた").as_deref(),
        Ok("됐다")
    );
}
