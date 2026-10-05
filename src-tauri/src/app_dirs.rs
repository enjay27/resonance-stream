//! Where the app keeps its files. Every folder the app writes to is asked for
//! here and nowhere else, so a test run can later move all of them at once
//! (`--data-dir`, see `.memory/roadmap/test-run-parameters.md`).
//!
//! Both return what Tauri resolves, unless a test run asked for `--data-dir`.

use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// `config.json` and the metadata file (Tauri's `app_config_dir`).
pub fn config(app: &AppHandle) -> tauri::Result<PathBuf> {
    match crate::test_env::dirs() {
        Some(dirs) => Ok(dirs.config.clone()),
        None => app.path().app_config_dir(),
    }
}

/// Model, server, chat logs, dictionary, raw captures (Tauri's `app_data_dir`).
pub fn data(app: &AppHandle) -> tauri::Result<PathBuf> {
    match crate::test_env::dirs() {
        Some(dirs) => Ok(dirs.data.clone()),
        None => app.path().app_data_dir(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("read src dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    /// The choke point only works if nothing goes around it: a call site that
    /// asks Tauri directly would keep writing to the real folder in a test run.
    #[test]
    fn no_code_outside_this_module_asks_tauri_for_a_folder() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        rust_files(&src, &mut files);
        assert!(files.len() > 10, "found the source files");

        let mut offenders = Vec::new();
        for file in files {
            if file.file_name().is_some_and(|n| n == "app_dirs.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&file).expect("read source file");
            for (number, line) in text.lines().enumerate() {
                let code = line.trim_start();
                if code.starts_with("//") {
                    continue;
                }
                if code.contains("app_data_dir(") || code.contains("app_config_dir(") {
                    offenders.push(format!("{}:{}", file.display(), number + 1));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "use crate::app_dirs::{{config, data}} instead:\n{}",
            offenders.join("\n")
        );
    }
}
