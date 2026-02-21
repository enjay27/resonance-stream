use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let root_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let root_path = Path::new(&root_dir);

    // Adjust this path to where your `lib/WinDivert` is located relative to `src-tauri`
    let lib_dir = root_path.parent().unwrap().join("lib");
    let wd_path = lib_dir.join("WinDivert").join("x64");

    // 1. Link against WinDivert.lib
    println!("cargo:rustc-link-search=native={}", wd_path.display());
    println!("cargo:rustc-link-lib=WinDivert");

    // 2. Setup paths
    let dll_src = wd_path.join("WinDivert.dll");
    let sys_src = wd_path.join("WinDivert64.sys");

    let bin_dir = root_path.join("bin");
    if !bin_dir.exists() {
        let _ = fs::create_dir(&bin_dir);
    }

    // 3. Copy to src-tauri/bin (for bundling resources)
    if dll_src.exists() {
        let _ = fs::copy(&dll_src, bin_dir.join("WinDivert.dll"));
    } else {
        println!("cargo:warning=WinDivert.dll not found in lib folder. Run setup_libs.bat!");
    }

    if sys_src.exists() {
        let _ = fs::copy(&sys_src, bin_dir.join("WinDivert64.sys"));
    }

    // 4. Copy to target profile dir (for dev runtime)
    if let Ok(profile) = env::var("PROFILE") {
        let target_profile_dir = root_path.join("target").join(&profile);
        // Ensure target dir exists
        if target_profile_dir.exists() {
             if dll_src.exists() {
                let _ = fs::copy(&dll_src, target_profile_dir.join("WinDivert.dll"));
             }
             if sys_src.exists() {
                let _ = fs::copy(&sys_src, target_profile_dir.join("WinDivert64.sys"));
             }
        }
    }

    // Standard Tauri build
    if cfg!(debug_assertions) {
        println!("DEV BUILD");
        tauri_build::build();
    } else {
        let mut windows = tauri_build::WindowsAttributes::new();
        windows = windows.app_manifest(include_str!("app.manifest"));

        tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
            .expect("failed to run build script");
    };
}