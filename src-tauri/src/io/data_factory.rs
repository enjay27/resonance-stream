use crate::{inject_system_message, ChatMessage, SystemLogLevel};
use chrono::Local;
use crossbeam_channel::{unbounded, Receiver, Sender};
use resonance_core::history::{chat_log_file_name, dataset_file_name};
use resonance_types::Channel;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

pub struct DataFactoryJob {
    /// The message as shown, with its translation when it has one.
    pub chat: ChatMessage,
}

/// The archive lives in the app data folder -- the one "앱 데이터 폴더 열기"
/// opens: one training-pair file per channel (tab), plus the daily chat logs.
fn data_dir(app: &AppHandle) -> std::io::Result<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// The training-pair files, opened on a channel's first message and kept open
/// for the worker's life.
struct Datasets {
    dir: PathBuf,
    files: HashMap<Channel, BufWriter<File>>,
}

impl Datasets {
    fn write(&mut self, channel: Channel, line: &str) -> std::io::Result<()> {
        if !self.files.contains_key(&channel) {
            let path = self.dir.join(dataset_file_name(channel.as_str()));
            let file = OpenOptions::new().create(true).append(true).open(path)?;
            self.files.insert(channel, BufWriter::new(file));
        }
        match self.files.get_mut(&channel) {
            Some(writer) => writeln!(writer, "{}", line),
            None => Ok(()),
        }
    }

    fn flush(&mut self) {
        for writer in self.files.values_mut() {
            let _ = writer.flush();
        }
    }
}

/// Is `channel` archived? (Channels can be switched off per tab, right-click
/// menu; the translator path asks too, so a translated message of a
/// switched-off channel is not written either.)
pub fn archives_channel(app: &AppHandle, channel: Channel) -> bool {
    app.try_state::<crate::AppState>().is_none_or(|state| {
        !state
            .config
            .read()
            .archive_ignored_channels
            .iter()
            .any(|c| c == channel.as_str())
    })
}

/// Daily chat logs (full messages), reloaded as history on the next start.
pub fn chat_logs_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .expect("Failed to resolve AppData directory")
        .join("chat_logs")
}

/// Deletes the daily chat logs the retention setting no longer keeps (0 keeps
/// all). Runs at start-up, when the setting is saved and when the day changes.
pub fn prune_chat_logs(app: &AppHandle) {
    let keep_days = crate::config::current_config(app).chat_log_retention_days;
    let removed = resonance_core::history::remove_expired_chat_logs(
        &chat_logs_dir(app),
        Local::now().date_naive(),
        keep_days,
    );
    if removed > 0 {
        log::info!(
            "[DataFactory] Removed {} chat log file(s) older than {} day(s)",
            removed,
            keep_days
        );
    }
}

/// Appends to today's chat log, switching files when the date changes.
struct DailyLog {
    dir: PathBuf,
    day: String,
    writer: Option<BufWriter<File>>,
}

impl DailyLog {
    fn write(&mut self, chat: &ChatMessage) -> std::io::Result<()> {
        let today = Local::now().format("%Y-%m-%d").to_string();
        if self.writer.is_none() || self.day != today {
            self.flush();
            std::fs::create_dir_all(&self.dir)?;
            let path = self.dir.join(chat_log_file_name(&today));
            let file = OpenOptions::new().create(true).append(true).open(path)?;
            self.writer = Some(BufWriter::new(file));
            self.day = today;
        }
        let line = serde_json::to_string(chat).map_err(std::io::Error::other)?;
        match self.writer.as_mut() {
            Some(writer) => writeln!(writer, "{}", line),
            None => Ok(()),
        }
    }

    fn flush(&mut self) {
        if let Some(writer) = self.writer.as_mut() {
            let _ = writer.flush();
        }
    }
}

pub fn start_data_factory_worker(app: AppHandle) -> Sender<DataFactoryJob> {
    let (tx, rx): (Sender<DataFactoryJob>, Receiver<DataFactoryJob>) = unbounded();

    thread::spawn(move || {
        let dir = match data_dir(&app) {
            Ok(dir) => dir,
            Err(e) => {
                inject_system_message(
                    &app,
                    SystemLogLevel::Error,
                    "DataFactory",
                    format!("Cannot open the chat archive: {}", e),
                );
                return;
            }
        };
        inject_system_message(
            &app,
            SystemLogLevel::Info,
            "DataFactory",
            format!("Archiving chat to {}", dir.display()),
        );

        // Two archives: the training pairs (dataset_<channel>.jsonl) and the
        // full daily chat logs. Files stay open for the worker's life and are
        // flushed whenever the queue runs dry, so a burst is one write and
        // nothing waits long on disk.
        let mut datasets = Datasets {
            dir: dir.clone(),
            files: HashMap::new(),
        };
        let mut daily = DailyLog {
            dir: chat_logs_dir(&app),
            day: String::new(),
            writer: None,
        };
        let mut failing = false; // one report per streak of failures
        while let Ok(job) = rx.recv() {
            let chat = job.chat;
            let entry = serde_json::json!({
                "pid": chat.pid,
                "original": chat.message,
                "translated": chat.translated, // Some("text") or null
                "timestamp": now_ms()
            });
            let dataset_result = datasets.write(chat.channel, &entry.to_string());
            let day_before = daily.day.clone();
            let daily_result = daily.write(&chat);
            if !day_before.is_empty() && daily.day != day_before {
                prune_chat_logs(&app); // a new day: yesterday's cut-off moved
            }
            match dataset_result.and(daily_result) {
                Ok(()) => failing = false,
                Err(e) => {
                    log::warn!("[DataFactory] Archive write failed: {}", e);
                    if !failing {
                        failing = true;
                        inject_system_message(
                            &app,
                            SystemLogLevel::Error,
                            "DataFactory",
                            format!("Chat archive write failed: {}", e),
                        );
                    }
                }
            }
            if rx.is_empty() {
                datasets.flush();
                daily.flush();
            }
        }
        datasets.flush();
        daily.flush();
    });

    tx
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
