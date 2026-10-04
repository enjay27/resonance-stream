//! The app's side of the test flags (`resonance_core::test_env` reads and
//! resolves them). They are honoured only when [`GATE_OPEN`]: a debug build, or
//! a build with the `test-env` feature. A normal release never reads the command
//! line or the `RESONANCE_TEST_*` variables, so nothing here can move its data.

use parking_lot::Mutex;
use resonance_core::test_env::{self as core_env, AppDirs, StatusReport, TestEnv, UpdateState};
use std::path::Path;
use std::sync::OnceLock;
use tauri::AppHandle;

/// Are the test flags honoured in this build?
pub const GATE_OPEN: bool = cfg!(any(debug_assertions, feature = "test-env"));

/// What a test run set up. Absent in a normal run.
struct Active {
    env: TestEnv,
    /// The folders under `--data-dir`, when there is one.
    dirs: Option<AppDirs>,
    /// The JSON `--status-file` holds, kept so one field can change.
    status: Mutex<StatusReport>,
}

static ACTIVE: OnceLock<Active> = OnceLock::new();

fn exit_with(message: &str) -> ! {
    eprintln!("resonance-stream: {message}");
    std::process::exit(2)
}

/// Reads the test flags and applies the ones that must act before the app
/// starts (`--fresh`, the WebView2 folder, `--print-env`). Call first in `run`.
pub fn init() {
    if !GATE_OPEN {
        return;
    }
    let env = core_env::parse(std::env::args().skip(1), |name| std::env::var(name).ok())
        .unwrap_or_else(|e| exit_with(&e.to_string()));
    if env == TestEnv::default() {
        return;
    }
    // `resolve_dirs` only needs defaults when there is no --data-dir.
    let dirs = env
        .data_dir
        .as_ref()
        .map(|_| core_env::resolve_dirs(Path::new(""), Path::new(""), &env));

    let report = StatusReport {
        version: env!("CARGO_PKG_VERSION").to_string(),
        exe: std::env::current_exe()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
        pid: std::process::id(),
        ready: false,
        config_dir: dirs
            .as_ref()
            .map(|d| d.config.to_string_lossy().into_owned()),
        data_dir: dirs.as_ref().map(|d| d.data.to_string_lossy().into_owned()),
        flags: env.set_flags().into_iter().map(String::from).collect(),
        update: UpdateState::None,
    };

    // No side effects: print what was understood, and stop.
    if env.print_env {
        println!("{}", report.to_json());
        if let Some(path) = &env.status_file {
            if let Err(e) = report.write(path) {
                exit_with(&format!("cannot write {}: {e}", path.display()));
            }
        }
        std::process::exit(0);
    }

    if let Some(dirs) = &dirs {
        if env.fresh {
            dirs.reset()
                .unwrap_or_else(|e| exit_with(&format!("--fresh: {e}")));
        }
        if let Some(webview) = &dirs.webview {
            if let Err(e) = std::fs::create_dir_all(webview) {
                exit_with(&format!("cannot create {}: {e}", webview.display()));
            }
            // Read by WebView2 when the first window is made; nothing else is
            // running yet, so changing the environment is safe.
            std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", webview);
        }
    }

    let active = Active {
        env,
        dirs,
        status: Mutex::new(report),
    };
    write_status(&active);
    let _ = ACTIVE.set(active);
}

fn write_status(active: &Active) {
    if let Some(path) = &active.env.status_file {
        if let Err(e) = active.status.lock().write(path) {
            log::warn!("[TestEnv] Could not write {}: {e}", path.display());
        }
    }
}

/// Is this flag set? Always false in a normal run.
fn flag(set: impl Fn(&TestEnv) -> bool) -> bool {
    ACTIVE.get().is_some_and(|active| set(&active.env))
}

/// `--no-capture`: no sniffer, no firewall check.
pub fn no_capture() -> bool {
    flag(|env| env.no_capture)
}

/// `--no-translator`: no llama-server.
pub fn no_translator() -> bool {
    flag(|env| env.no_translator)
}

/// `--no-update-check`: the update check finds nothing, without asking the network.
pub fn no_update_check() -> bool {
    flag(|env| env.no_update_check)
}

/// `--no-popups`: the popup windows are not made ahead of time.
pub fn no_popups() -> bool {
    flag(|env| env.no_popups)
}

/// `--no-window-state`: the window size and place are neither restored nor saved.
pub fn no_window_state() -> bool {
    flag(|env| env.no_window_state)
}

/// What `config.json` held as `init_done` when the app started.
static STORED_INIT_DONE: OnceLock<bool> = OnceLock::new();

/// `init_done` for this run, given what the file says (see `--assume-setup-done`).
pub fn init_done_for_run(stored: bool) -> bool {
    let _ = STORED_INIT_DONE.set(stored);
    ACTIVE
        .get()
        .map_or(stored, |active| active.env.init_done_for_run(stored))
}

/// The `init_done` to write to `config.json`, given the one the app holds.
pub fn init_done_for_disk(current: bool) -> bool {
    match (ACTIVE.get(), STORED_INIT_DONE.get()) {
        (Some(active), Some(&stored)) => active.env.init_done_for_disk(current, stored),
        _ => current,
    }
}

/// The folders under `--data-dir`, or `None` for Tauri's own.
pub fn dirs() -> Option<&'static AppDirs> {
    ACTIVE.get().and_then(|active| active.dirs.as_ref())
}

/// `--log-file`: where the log is also written.
pub fn log_file() -> Option<&'static Path> {
    ACTIVE
        .get()
        .and_then(|active| active.env.log_file.as_deref())
}

/// The flags to start the app with again after an update is applied, so the
/// new process runs in the same test environment.
pub fn restart_args() -> Vec<String> {
    ACTIVE
        .get()
        .map(|active| active.env.restart_args())
        .unwrap_or_default()
}

/// The app has finished starting: tells the status file, with the folders as
/// they resolved.
pub fn mark_ready(app: &AppHandle) {
    let Some(active) = ACTIVE.get() else { return };
    {
        let mut status = active.status.lock();
        let text = |dir: tauri::Result<std::path::PathBuf>| {
            dir.ok().map(|d| d.to_string_lossy().into_owned())
        };
        status.config_dir = text(crate::app_dirs::config(app));
        status.data_dir = text(crate::app_dirs::data(app));
        status.ready = true;
    }
    write_status(active);
}

/// Records where the app's own update stands (`--status-file`'s `update`).
pub fn report_update(state: UpdateState) {
    let Some(active) = ACTIVE.get() else { return };
    active.status.lock().update = state;
    write_status(active);
}
