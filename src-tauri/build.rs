use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let target_dir = Path::new(&manifest_dir).join("target").join(profile);

    // List of WinDivert files now located in src-tauri/
    let files = [
        "WinDivert.dll",
        "WinDivert64.sys",
        "WinDivert.lib",
        "resources/models.json"
    ];

    for file in files {
        let src = Path::new(&manifest_dir).join(file);

        // Copy directly to target root (e.g., target/debug/WinDivert.dll)
        let file_name = Path::new(file).file_name().unwrap();
        let dest = target_dir.join(file_name);

        if src.exists() {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).ok();
            }
            let _ = fs::copy(&src, &dest);
        }
    }

    // --- 2. Existing Manifest Logic ---
    let windows = tauri_build::WindowsAttributes::new()
        .app_manifest(include_str!("app.manifest"));

    tauri_build::try_build(
        tauri_build::Attributes::new().windows_attributes(windows)
    )
        .expect("failed to run build script");
}