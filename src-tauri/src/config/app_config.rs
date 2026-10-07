use crate::{inject_system_message, AppState, SystemLogLevel, TranslatorState};
use parking_lot::Mutex;
use resonance_core::download::{keep_bad_copy, read_text_retrying, write_atomic};
use resonance_core::favorites_migration::migrate_favorites;
use resonance_core::history::ChannelLimits;
use resonance_core::workers::{translator_change, TranslatorSettings, WorkerChange};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

pub use resonance_types::AppConfig;

fn get_config_path(app: &AppHandle) -> PathBuf {
    let config_dir = crate::app_dirs::config(app).expect("Could not resolve app config dir");

    // Ensure the directory exists (e.g., create 'com.bpsr.translator' folder)
    if !config_dir.exists() {
        let _ = fs::create_dir_all(&config_dir);
    }

    config_dir.join("config.json")
}

/// A `config.json` as text -> the config. A file written before the favorites
/// tabs had ids (names instead) is converted first, so every tab and favorite
/// survives; a file already in the new shape is read as it is.
fn parse_config(content: &str) -> serde_json::Result<AppConfig> {
    let mut json: serde_json::Value = serde_json::from_str(content)?;
    migrate_favorites(&mut json);
    serde_json::from_value(json)
}

/// Reads `config.json`, writing the defaults first if it does not exist yet.
/// Only start-up (and early callers before the state exists) read the file.
pub fn read_config_file(app: &AppHandle) -> AppConfig {
    let path = get_config_path(app);

    if !path.exists() {
        // Create default if missing
        let default_config = AppConfig::default();
        if let Ok(json) = serde_json::to_string_pretty(&default_config) {
            let _ = write_atomic(&path, json.as_bytes());
        }
        return default_config;
    }

    // A lock held for a moment (an antivirus scan) is waited out; a file that stays unreadable,
    // or does not parse, is kept as `config.json.bad` -- the next save would replace it -- and
    // the user is told once the app state exists (`take_load_notice`).
    match read_text_retrying(&path, 3, Duration::from_millis(250)) {
        Ok(content) => parse_config(&content)
            .unwrap_or_else(|e| defaults_instead(&path, &format!("it could not be parsed ({e})"))),
        Err(e) => defaults_instead(&path, &format!("it could not be read ({e})")),
    }
}

/// What the start-up read found wrong with `config.json`, for the system log (set while the
/// app state does not exist yet, so it cannot be logged there and then).
static LOAD_NOTICE: Mutex<Option<String>> = Mutex::new(None);

/// The note about a `config.json` that was not used, once.
pub fn take_load_notice() -> Option<String> {
    LOAD_NOTICE.lock().take()
}

/// Defaults in place of a `config.json` that cannot be used: the file is copied to
/// `config.json.bad` first, and the notice says where it is -- or that even the copy failed.
fn defaults_instead(path: &std::path::Path, why: &str) -> AppConfig {
    let notice = match keep_bad_copy(path) {
        Ok(backup) => format!(
            "config.json was not used: {why}. The defaults are in use and the file was kept as {}.",
            backup.display()
        ),
        Err(e) => format!(
            "config.json was not used: {why}. The defaults are in use, and the file could not be \
             copied either ({e}): saving a setting will replace it."
        ),
    };
    log::error!("{notice}");
    *LOAD_NOTICE.lock() = Some(notice);
    AppConfig::default()
}

fn translator_settings(c: &AppConfig) -> TranslatorSettings {
    TranslatorSettings {
        enabled: c.use_translation,
        compute_mode: c.compute_mode,
        tier: c.tier,
    }
}

/// The live config: the in-memory copy once the app state exists.
pub fn current_config(app: &AppHandle) -> AppConfig {
    match app.try_state::<AppState>() {
        Some(state) => state.config.read().clone(),
        None => read_config_file(app),
    }
}

#[tauri::command]
pub fn load_config(app: AppHandle) -> AppConfig {
    current_config(&app)
}

/// async: writes the file and may start or stop workers -- not on the main thread.
///
/// `Err`: the file could not be written. The setting is in use for this run all the same (the
/// ui already shows it), and the ui tells the user it will not survive a restart.
#[tauri::command(async)]
pub fn save_config(
    app: AppHandle,
    state: State<'_, AppState>,
    config: AppConfig,
) -> Result<(), String> {
    // One save at a time: two overlapping saves would each compare against
    // the same old config and start (or stop) the same worker twice.
    let _saving = state.config_lock.lock();
    let config = config.keeping_favorites_of(&state.config.read());
    apply_config(&app, &state, config)
}

/// Replaces the favorites only -- every other setting is left as it is, so the
/// favorites popup can never overwrite the main window's newer settings --
/// then tells every window (`favorites-changed`). The global shortcuts follow
/// (`apply_config`). async: writes the file.
#[tauri::command(async)]
pub fn save_favorites(
    app: AppHandle,
    state: State<'_, AppState>,
    favorites: resonance_types::FavoritesState,
) {
    modify_config(&app, &state, |config| {
        config.favorite_messages = favorites.messages;
        config.favorite_tabs = favorites.tabs;
    });
    let _ = app.emit("favorites-changed", state.config.read().favorites());
}

/// Read-modify-write of the live config under the save lock, so a change
/// made from the backend (block list) cannot be lost to a concurrent save.
pub fn modify_config(
    app: &AppHandle,
    state: &State<'_, AppState>,
    change: impl FnOnce(&mut AppConfig),
) {
    let _saving = state.config_lock.lock();
    let mut config = state.config.read().clone();
    change(&mut config);
    // No ui call is waiting for this answer: a failed write goes to the system log.
    if let Err(e) = apply_config(app, state, config) {
        inject_system_message(app, SystemLogLevel::Error, "Settings", e);
    }
}

/// Puts `config` in use and writes it to disk. A failed write does not stop the change from
/// taking effect, but it is reported.
fn apply_config(
    app: &AppHandle,
    state: &State<'_, AppState>,
    config: AppConfig,
) -> Result<(), String> {
    let app = app.clone();
    let old_config = state.config.read().clone();

    let path = get_config_path(&app);
    let mut on_disk = config.clone();
    on_disk.init_done = crate::test_env::init_done_for_disk(config.init_done);
    let written = serde_json::to_string_pretty(&on_disk)
        .map_err(|e| e.to_string())
        .and_then(|json| write_atomic(&path, json.as_bytes()).map_err(|e| e.to_string()))
        .map_err(|e| format!("config.json was not saved ({e}): this setting is in use now but will not survive a restart."));
    if let Err(e) = &written {
        log::error!("{e}");
    }
    *state.config.write() = config.clone();
    state
        .chat_history
        .lock()
        .set_limits(ChannelLimits::new(&config.tab_limits));

    if old_config.favorite_messages != config.favorite_messages {
        state.shortcuts.lock().favorites = config.favorite_messages.clone();
        crate::shortcut::apply_global_shortcuts(&app);
    }

    // --- MANAGE THE SNIFFER THREAD (NETWORK ADAPTER CHANGE) ---
    if old_config.network_interface != config.network_interface {
        // Drop the old Sender (Instantly kills the socket and watchdog threads)
        *state.sniffer_tx.lock() = None;

        // Restart the sniffer bound to the newly selected interface
        if config.init_done {
            inject_system_message(
                &app,
                SystemLogLevel::Info,
                "Sniffer",
                "Network adapter changed. Restarting sniffer...",
            );
            let tx = crate::services::sniffer::start_sniffer_worker(app.clone());
            *state.sniffer_tx.lock() = Some(tx);
        }
    }

    // --- MANAGE THE AI WORKER THREAD --- (exactly one change per save)
    match translator_change(
        translator_settings(&old_config),
        translator_settings(&config),
    ) {
        WorkerChange::Keep => {}
        change @ (WorkerChange::Start | WorkerChange::Restart) => {
            if change == WorkerChange::Restart {
                // Drop the old sender to break the current thread's loop
                *state.translator_tx.lock() = None;
                inject_system_message(
                    &app,
                    SystemLogLevel::Info,
                    "Translator",
                    "Applying new AI Engine specifications...",
                );
            }
            let model_path = crate::get_model_path(&app);
            let tx = crate::services::translator::start_translator_worker(app.clone(), model_path);
            *state.translator_tx.lock() = Some(tx);
        }
        WorkerChange::Stop => {
            // Drop the Sender (Kills the thread and frees VRAM)
            *state.translator_tx.lock() = None;
            crate::services::translator::retire_translator_workers();
            inject_system_message(
                &app,
                SystemLogLevel::Info,
                "Translator",
                "AI Translation Disabled. Server stopped and VRAM cleared.",
            );
            crate::services::translator::emit_translator_state(
                &app,
                TranslatorState::Off,
                "AI Translation Disabled.",
            );
        }
    }

    if old_config.raw_capture != config.raw_capture {
        crate::services::sniffer::set_raw_capture(config.raw_capture);
    }

    if old_config.chat_log_retention_days != config.chat_log_retention_days {
        crate::io::prune_chat_logs(&app);
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;
    use resonance_types::default_favorite_messages;

    #[test]
    fn a_config_with_tab_names_is_converted_when_read() {
        let config = parse_config(
            r#"{"init_done": true,
                "favorite_tabs": ["레이드", "던전"],
                "favorite_messages": [
                    {"text": "a", "tab": ""},
                    {"text": "b", "shortcut": "Alt+F1", "tab": "던전"}]}"#,
        )
        .unwrap();
        assert!(config.init_done, "the rest of the file is read as before");
        let tabs: Vec<_> = config
            .favorite_tabs
            .iter()
            .map(|t| (t.id, t.name.as_str()))
            .collect();
        assert_eq!(tabs, [(1, "레이드"), (2, "던전")]);
        let filed: Vec<_> = config.favorite_messages.iter().map(|f| f.tab).collect();
        assert_eq!(filed, [0, 2]);
        assert_eq!(config.favorite_messages[1].shortcut, "Alt+F1");
    }

    #[test]
    fn a_config_in_the_new_shape_is_read_as_it_is_and_one_without_favorites_gets_the_defaults() {
        let json = r#"{"favorite_tabs": [{"id": 4, "name": "레이드"}],
                       "favorite_messages": [{"text": "a", "tab": 4}]}"#;
        let config = parse_config(json).unwrap();
        assert_eq!(config.favorite_tabs[0].id, 4);
        assert_eq!(config.favorite_messages[0].tab, 4);
        assert_eq!(
            parse_config(&serde_json::to_string(&config).unwrap())
                .unwrap()
                .favorites(),
            config.favorites(),
            "saving and reading again changes nothing"
        );

        let bare = parse_config(r#"{"init_done": true}"#).unwrap();
        assert_eq!(bare.favorite_messages, default_favorite_messages());
        assert!(bare.favorite_tabs.is_empty());
        assert!(parse_config("not json").is_err());
    }
}
