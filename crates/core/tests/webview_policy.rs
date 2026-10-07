//! The webview's Content-Security-Policy (`src-tauri/tauri.conf.json`), pinned.
//!
//! A wrong policy does not fail a build: it blanks the window (the app starts by `fetch`-ing its own
//! wasm file, and an inline module script starts it), and a window cannot be run from Linux. So the
//! rules a policy must keep are written down here and checked on every OS.
//!
//! Tauri adds to `script-src` the SHA-256 of every inline `<script>` in the built HTML at compile time
//! (`tauri-codegen`, `inject_script_hashes`), so the config must NOT carry that hash.

use serde_json::Value;
use std::collections::BTreeMap;

const CONFIG: &str = include_str!("../../../src-tauri/tauri.conf.json");

fn config() -> Value {
    serde_json::from_str(CONFIG).expect("tauri.conf.json is JSON")
}

/// `{ directive: [sources] }` of the `csp` string, or None when there is no policy.
fn policy() -> Option<BTreeMap<String, Vec<String>>> {
    let value = config();
    let text = value["app"]["security"]["csp"].as_str()?.to_string();
    Some(
        text.split(';')
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .map(|d| {
                let mut parts = d.split_whitespace().map(String::from);
                (parts.next().unwrap(), parts.collect())
            })
            .collect(),
    )
}

fn sources<'a>(policy: &'a BTreeMap<String, Vec<String>>, directive: &str) -> &'a [String] {
    policy
        .get(directive)
        .unwrap_or_else(|| panic!("the policy has no `{directive}`"))
}

#[test]
fn the_webview_has_a_content_security_policy() {
    assert!(
        policy().is_some(),
        "app.security.csp is null: no policy at all"
    );
}

#[test]
fn everything_not_named_comes_from_the_app_itself() {
    let policy = policy().unwrap();
    assert_eq!(sources(&policy, "default-src"), ["'self'"]);
}

#[test]
fn scripts_are_the_apps_own_files_and_wasm_may_run_but_nothing_is_evaluated() {
    let policy = policy().unwrap();
    let script = sources(&policy, "script-src");
    assert!(script.contains(&"'self'".to_string()), "{script:?}");
    // The app is wasm: without this the window stays blank.
    assert!(
        script.contains(&"'wasm-unsafe-eval'".to_string()),
        "{script:?}"
    );
    for forbidden in [
        "'unsafe-eval'",
        "'unsafe-inline'",
        "*",
        "data:",
        "blob:",
        "http:",
        "https:",
    ] {
        assert!(
            !script.contains(&forbidden.to_string()),
            "script-src has {forbidden}"
        );
    }
    // Tauri adds the hash of the inline start-up script itself; one written here would go stale.
    assert!(
        !script
            .iter()
            .any(|s| s.starts_with("'sha256-") || s.starts_with("'nonce-")),
        "{script:?}"
    );
}

#[test]
fn the_app_may_fetch_its_own_files_and_talk_to_the_backend_and_nothing_else() {
    let policy = policy().unwrap();
    let connect = sources(&policy, "connect-src");
    // The start-up `fetch` of the wasm file is a connect-src request: without 'self' the window is blank
    // (found by running the page in Chromium with a policy that left it out).
    assert!(connect.contains(&"'self'".to_string()), "{connect:?}");
    // Tauri's IPC on Windows.
    assert!(connect.contains(&"ipc:".to_string()), "{connect:?}");
    assert!(
        connect.contains(&"http://ipc.localhost".to_string()),
        "{connect:?}"
    );
    // No other host: the backend does all the network calls.
    let others: Vec<_> = connect
        .iter()
        .filter(|s| !["'self'", "ipc:", "http://ipc.localhost"].contains(&s.as_str()))
        .collect();
    assert!(others.is_empty(), "connect-src also allows {others:?}");
}

#[test]
fn styles_may_be_inline_because_the_ui_sets_style_attributes() {
    // Dropping 'unsafe-inline' blocks every `style="..."` the views set (six at once in the settings).
    let policy = policy().unwrap();
    let style = sources(&policy, "style-src");
    assert!(style.contains(&"'self'".to_string()), "{style:?}");
    assert!(style.contains(&"'unsafe-inline'".to_string()), "{style:?}");
}

#[test]
fn no_directive_opens_the_webview_to_the_internet() {
    let policy = policy().unwrap();
    for (directive, list) in &policy {
        for source in list {
            let remote = source == "*"
                || source == "http:"
                || source == "https:"
                || source == "ws:"
                || source == "wss:"
                || source.starts_with("https://")
                || (source.starts_with("http://") && source != "http://ipc.localhost");
            assert!(!remote, "{directive} allows {source}");
        }
    }
}

#[test]
fn plugins_frames_a_base_tag_and_form_posts_are_closed() {
    let policy = policy().unwrap();
    assert_eq!(sources(&policy, "object-src"), ["'none'"]);
    assert_eq!(sources(&policy, "base-uri"), ["'none'"]);
    assert_eq!(sources(&policy, "form-action"), ["'none'"]);
    assert_eq!(sources(&policy, "frame-ancestors"), ["'none'"]);
}

#[test]
fn the_ui_still_finds_tauri_where_it_looks_for_it() {
    // The ui calls `window.__TAURI__` (no bundler import): the policy change must not touch this.
    assert_eq!(config()["app"]["withGlobalTauri"], Value::Bool(true));
}
