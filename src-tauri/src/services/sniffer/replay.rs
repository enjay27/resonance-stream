//! `--replay-chat <file>`: chat lines fed in as if the sniffer had captured them,
//! so the chat list, the translator and the archive can be tried without the game.
//! The file format and its checks are `resonance_core::replay`; each line goes
//! through `ChatPipeline::feed_chat` (the rules a captured message gets) and then
//! `dispatch_pipeline_actions` (what the sniffer does with it).

use crate::inject_system_message;
use crate::protocol::types::SystemLogLevel;
use crate::AppState;
use resonance_core::capture::ChatPipeline;
use resonance_core::replay::{parse_replay, ReplayEntry};
use std::sync::atomic::Ordering;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

/// Time for the window to load its history before the first line arrives.
const LEAD_IN: Duration = Duration::from_secs(2);

/// Starts the replay when `--replay-chat` was given; does nothing otherwise.
pub fn start(app: AppHandle) {
    if let Some(path) = crate::test_env::replay_chat() {
        start_file(app, path);
    }
}

/// Replays the chat lines of `path` (`--replay-chat`, or the bridge's
/// `replay-chat` command).
pub fn start_file(app: AppHandle, path: &std::path::Path) {
    let say = |level: SystemLogLevel, text: String| {
        inject_system_message(&app, level, "Replay", text);
    };
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) => {
            return say(
                SystemLogLevel::Error,
                format!("Cannot read {}: {e}", path.display()),
            )
        }
    };
    let entries = match parse_replay(&text) {
        Ok(entries) if entries.is_empty() => {
            return say(
                SystemLogLevel::Warning,
                format!("{} has no chat lines to replay", path.display()),
            )
        }
        Ok(entries) => entries,
        Err(e) => return say(SystemLogLevel::Error, format!("{}: {e}", path.display())),
    };
    say(
        SystemLogLevel::Info,
        format!(
            "Replaying {} chat lines from {}",
            entries.len(),
            path.display()
        ),
    );
    let app = app.clone();
    std::thread::spawn(move || run(app, entries));
}

fn run(app: AppHandle, entries: Vec<ReplayEntry>) {
    std::thread::sleep(LEAD_IN);
    let state = app.state::<AppState>();
    let mut pipeline = ChatPipeline::new();
    for (index, entry) in entries.iter().enumerate() {
        std::thread::sleep(Duration::from_millis(entry.delay_ms));
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let action = pipeline.feed_chat(
            entry.into_chat(index, now),
            |uid| state.blocked_users.lock().contains_key(&uid),
            || state.next_pid.fetch_add(1, Ordering::SeqCst),
        );
        if let Some(action) = action {
            super::dispatch_pipeline_actions(&app, vec![action]);
        }
    }
    inject_system_message(&app, SystemLogLevel::Info, "Replay", "Replay finished");
}
