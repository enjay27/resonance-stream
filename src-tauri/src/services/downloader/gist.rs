use crate::inject_system_message;
use crate::protocol::types::SystemLogLevel;
use parking_lot::Mutex;
use resonance_core::download::{
    is_newer_version, BodyCap, BodyTooLarge, CONNECT_TIMEOUT, REMOTE_CALL_TIMEOUT,
};
use resonance_core::test_env::UpdateState;
use resonance_core::text::Dictionary;
use resonance_core::update_feed::{parse_feed_allowing, UpdateFeed};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

const METADATA_URL: &str =
    "https://gist.githubusercontent.com/enjay27/4066e54b9c2ac6c923bf967e6d9a06c5/raw/metadata.json";
/// The update feed of the newest stable release (`release.yml` publishes it);
/// the app learns about its own updates here, not from the gist.
const FEED_URL: &str =
    "https://github.com/enjay27/resonance-stream/releases/latest/download/latest.json";
/// A feed is a few lines of JSON plus release notes.
const FEED_MAX_BYTES: usize = 256 * 1024;
/// The metadata names three versions, a model and its hash. Generous: not measured, the host is
/// out of reach of the session that set it.
const METADATA_MAX_BYTES: usize = 64 * 1024;
/// The custom dictionary: a list of words. Generous (4 MiB), well above any list a person keeps.
const DICT_MAX_BYTES: usize = 4 * 1024 * 1024;
const DICT_URL: &str = "https://gist.githubusercontent.com/enjay27/4066e54b9c2ac6c923bf967e6d9a06c5/raw/custom_dict.json";

// --- 1. Structs matching the unified Gist JSON: shared with the UI ---
pub use resonance_types::{GistMetadata, RemoteDictionary, UpdateCheckResult, VersionInfo};

/// A client for the small calls (feed, metadata, dictionary): it gives up connecting, and gives up
/// on the whole call, instead of waiting on a stalled host while the start-up shows "checking"
/// (review W-8). The downloads have their own, longer rules (`fetch.rs`).
fn remote_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REMOTE_CALL_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())
}

/// The body of `res`, up to `limit` bytes: a longer one is refused at once when the host announces
/// its length, and as it arrives otherwise, so a host that never stops cannot fill the memory.
async fn read_capped(mut res: reqwest::Response, limit: usize) -> Result<Vec<u8>, String> {
    if res.content_length().is_some_and(|n| n > limit as u64) {
        return Err(BodyTooLarge { limit }.to_string());
    }
    let mut cap = BodyCap::new(limit);
    let mut body = Vec::new();
    while let Some(chunk) = res.chunk().await.map_err(|e| e.to_string())? {
        cap.add(chunk.len()).map_err(|e| e.to_string())?;
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// The release the last update check announced. The app updater downloads
/// and verifies exactly this one: its url, its version and its signature.
static LAST_FEED: Mutex<Option<UpdateFeed>> = Mutex::new(None);

/// The release the last update check announced, if there was one.
pub fn announced_update() -> Option<UpdateFeed> {
    LAST_FEED.lock().clone()
}

/// Reads the release feed. `Ok(None)`: no stable release exists yet (404).
async fn fetch_update_feed() -> Result<Option<UpdateFeed>, String> {
    let res = remote_client()?
        .get(crate::test_env::feed_url().unwrap_or(FEED_URL))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;
    if res.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !res.status().is_success() {
        return Err(format!("Update feed returned: {}", res.status()));
    }
    let body = read_capped(res, FEED_MAX_BYTES)
        .await
        .map_err(|e| format!("Update feed: {e}"))?;
    let text = String::from_utf8(body).map_err(|_| "The update feed is not text".to_string())?;
    parse_feed_allowing(&text, crate::test_env::allow_local_http()).map(Some)
}

// --- 2. The Single Unified Fetch Command ---
#[tauri::command]
pub async fn check_all_updates(app: AppHandle) -> Result<UpdateCheckResult, String> {
    if crate::test_env::no_update_check() {
        // Nothing is asked of the network and nothing is announced.
        return Ok(UpdateCheckResult {
            app_update_available: false,
            model_update_available: false,
            dict_update_available: false,
            remote_data: GistMetadata {
                revision: 0,
                app: VersionInfo::default(),
                model: VersionInfo::default(),
                dictionary: RemoteDictionary {
                    version: String::new(),
                    updated_at: String::new(),
                    sha256: String::new(),
                },
            },
        });
    }
    let response = remote_client()?
        .get(crate::test_env::metadata_url().unwrap_or(METADATA_URL))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;
    let body = read_capped(response, METADATA_MAX_BYTES)
        .await
        .map_err(|e| format!("Metadata: {e}"))?;
    let mut remote_data: GistMetadata =
        serde_json::from_slice(&body).map_err(|e| format!("JSON parsing error: {}", e))?;

    let metadata = crate::config::load_metadata(&app);
    let current_app_version = app.package_info().version.to_string();

    // App Check: from the release feed, not the gist (whose `app` entry only
    // serves copies that predate this). A feed that cannot be read is no
    // update -- never a reason to fail the model / dictionary check.
    let mut feed_error = None;
    let feed = match fetch_update_feed().await {
        Ok(feed) => feed,
        Err(e) => {
            log::warn!("[Updater] No app update info: {}", e);
            feed_error = Some(e);
            None
        }
    };
    *LAST_FEED.lock() = feed.clone();
    remote_data.app = feed
        .as_ref()
        .map(|feed| VersionInfo {
            latest_version: feed.version.clone(),
            download_url: feed.url.clone(),
            release_notes: feed.notes.clone(),
            sha256: String::new(),
        })
        .unwrap_or_default();

    // Only a newer version is an update (a stale feed must not offer a
    // downgrade).
    let mut app_update_available = feed
        .as_ref()
        .is_some_and(|feed| is_newer_version(&feed.version, &current_app_version));
    if let Some(ignored) = &metadata.ignored_app_version {
        if ignored == &remote_data.app.latest_version {
            app_update_available = false;
        }
    }

    crate::test_env::report_update(UpdateState::after_check(
        feed_error.as_deref(),
        feed.as_ref()
            .filter(|_| app_update_available)
            .map(|feed| feed.version.as_str()),
    ));

    // Model Check
    let mut model_update_available =
        remote_data.model.latest_version != metadata.current_model_version;
    if let Some(ignored) = &metadata.ignored_model_version {
        if ignored == &remote_data.model.latest_version {
            model_update_available = false;
        }
    }

    // Dictionary Check
    let dict_update_available = remote_data.dictionary.version != metadata.current_dict_version;

    Ok(UpdateCheckResult {
        app_update_available,
        model_update_available,
        dict_update_available,
        remote_data,
    })
}

/// %APPDATA%/<bundle id>/custom_dict.json
pub fn dictionary_path(app: &AppHandle) -> PathBuf {
    crate::app_dirs::data(app)
        .expect("Failed to resolve AppData directory")
        .join("custom_dict.json")
}

/// Makes `dict` the one the translator uses from its next job on.
fn install_dictionary(app: &AppHandle, dict: Dictionary) {
    if let Some(state) = app.try_state::<crate::AppState>() {
        *state.dictionary.write() = Arc::new(dict);
    }
}

#[tauri::command]
pub async fn sync_dictionary(app: AppHandle, version: String) -> Result<String, String> {
    // 1. Resolve Local Path
    let dict_path = dictionary_path(&app);

    // 2. Fetch from Remote
    let response = remote_client()?
        .get(crate::test_env::dictionary_url().unwrap_or(DICT_URL))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let body = read_capped(response, DICT_MAX_BYTES)
        .await
        .map_err(|e| format!("Dictionary: {e}"))?;
    let json_content =
        String::from_utf8(body).map_err(|_| "The dictionary is not text".to_string())?;

    // Validate before saving: a dictionary that does not parse is rejected
    let dict = Dictionary::from_json_str(&json_content)
        .map_err(|e| format!("Invalid dictionary received from Gist: {}", e))?;

    // 3. Save Locally
    fs::create_dir_all(dict_path.parent().unwrap()).map_err(|e| e.to_string())?;
    resonance_core::download::write_atomic(&dict_path, json_content.as_bytes())
        .map_err(|e| e.to_string())?;
    install_dictionary(&app, dict);

    inject_system_message(
        &app,
        SystemLogLevel::Success,
        "ModelManager",
        "Dictionary saved to AppData.",
    );

    inject_system_message(
        &app,
        SystemLogLevel::Success,
        "Translator",
        "Dictionary successfully synchronized.",
    );
    println!(
        "Dictionary successfully synchronized. version {:?}",
        version
    );

    let mut metadata = crate::config::load_metadata(&app);
    metadata.current_dict_version = version;
    crate::config::save_metadata(&app, &metadata);

    Ok("Dictionary updated and reloaded!".to_string())
}

#[tauri::command]
pub fn get_dict_version(app: tauri::AppHandle) -> String {
    let metadata = crate::config::load_metadata(&app);
    metadata.current_dict_version
}

#[tauri::command]
pub fn get_local_dictionary(app: tauri::AppHandle) -> Result<String, String> {
    let dict_path = dictionary_path(&app);

    if !dict_path.exists() {
        return Ok("{}".to_string());
    }

    std::fs::read_to_string(&dict_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_local_dictionary(app: tauri::AppHandle, content: String) -> Result<(), String> {
    let dict = Dictionary::from_json_str(&content)?;
    resonance_core::download::write_atomic(&dictionary_path(&app), content.as_bytes())
        .map_err(|e| e.to_string())?;
    install_dictionary(&app, dict);
    Ok(())
}

#[tauri::command]
pub fn ignore_update(app: AppHandle, target: String, version: String) {
    let mut metadata = crate::config::load_metadata(&app);
    if target == "app" {
        metadata.ignored_app_version = Some(version);
    } else if target == "model" {
        metadata.ignored_model_version = Some(version);
    }
    crate::config::save_metadata(&app, &metadata);
}
