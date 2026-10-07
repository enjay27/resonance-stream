//! The AI server runs elevated, from a folder the user can write to (review W-2). Its zip is
//! pinned by SHA-256 at download, but the files are only checked here: right before the app
//! starts the server, every file must be the one the pinned zip held, and no other program
//! or library may sit beside it (Windows loads a DLL from the exe's own folder first).

use crate::download::sha256_file;
use std::collections::HashMap;
use std::path::Path;

/// One file of the pinned zip, by the name the app gives it (the zip is extracted flat).
pub struct Pin {
    pub name: &'static str,
    pub sha256: &'static str,
}

/// What is wrong with the folder. Names are as they are on disk (`Unexpected`) or as pinned.
#[derive(Debug, PartialEq, Eq)]
pub enum Problem {
    Missing(String),
    Changed(String),
    /// A file that exists but could not be read, so nobody can vouch for it.
    Unreadable(String),
    /// A program or library that is not part of the pinned zip.
    Unexpected(String),
}

/// Checks `dir` against `pins`: every pinned file is there with its hash, and no other `.exe`
/// or `.dll` sits beside them. Names compare case-insensitively, as Windows does. An empty
/// answer means the folder may be started.
pub fn verify_dir(dir: &Path, pins: &[Pin]) -> Vec<Problem> {
    verify_dir_with(dir, pins, sha256_file)
}

/// [`verify_dir`] with the hashing swapped out, so a file that cannot be read can be tested.
fn verify_dir_with(
    dir: &Path,
    pins: &[Pin],
    hash: impl Fn(&Path) -> std::io::Result<String>,
) -> Vec<Problem> {
    let mut on_disk: HashMap<String, String> = HashMap::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|t| !t.is_dir()) {
                let name = entry.file_name().to_string_lossy().into_owned();
                on_disk.insert(name.to_lowercase(), name);
            }
        }
    }
    let mut problems = Vec::new();
    for pin in pins {
        match on_disk.remove(&pin.name.to_lowercase()) {
            None => problems.push(Problem::Missing(pin.name.to_string())),
            Some(actual) => match hash(&dir.join(&actual)) {
                Ok(hash) if hash == pin.sha256 => {}
                Ok(_) => problems.push(Problem::Changed(pin.name.to_string())),
                Err(_) => problems.push(Problem::Unreadable(pin.name.to_string())),
            },
        }
    }
    let mut unexpected: Vec<String> = on_disk
        .into_values()
        .filter(|name| {
            let lower = name.to_lowercase();
            lower.ends_with(".exe") || lower.ends_with(".dll")
        })
        .collect();
    unexpected.sort();
    problems.extend(unexpected.into_iter().map(Problem::Unexpected));
    problems
}

/// The first few problems as one line for the log and the translator state.
pub fn summary(problems: &[Problem]) -> String {
    const SHOWN: usize = 3;
    let mut parts: Vec<String> = problems
        .iter()
        .take(SHOWN)
        .map(|p| match p {
            Problem::Missing(n) => format!("missing: {n}"),
            Problem::Changed(n) => format!("changed: {n}"),
            Problem::Unreadable(n) => format!("unreadable: {n}"),
            Problem::Unexpected(n) => format!("not part of the AI engine: {n}"),
        })
        .collect();
    if problems.len() > SHOWN {
        parts.push(format!("and {} more", problems.len() - SHOWN));
    }
    parts.join("; ")
}

/// The files of `AI_SERVER_ZIP_URL` (src-tauri `downloader/server.rs`), from the zip whose
/// SHA-256 is pinned there. Regenerate with `.github/scripts/ai-server-pins.py <zip>`; change
/// the table, the URL and that hash together.
// Zip sha256: 8144cf0a765f6a69c8bf62acb70e9d224bcfef28b3ffd2260fe4cf5b8bde20dc
pub const AI_SERVER_PINS: &[Pin] = &[
    Pin {
        name: "ggml-base.dll",
        sha256: "82eb6423e67f5f6061beab01d8118ed6bcaf049f9d9a366e6592b0283f260b67",
    },
    Pin {
        name: "ggml-cpu-alderlake.dll",
        sha256: "2117a7a3d03a46422cc242900496f12cb8a255ea414d50d72f40b4764802a0fe",
    },
    Pin {
        name: "ggml-cpu-cannonlake.dll",
        sha256: "813f3a7d59bdacd654fb9c2f75dda4a9194d52644753c115f8c2bf1b3dcc7ced",
    },
    Pin {
        name: "ggml-cpu-cascadelake.dll",
        sha256: "281f560bd3fbcd508ee473e64a1fa4967ad57254835f68ef276d1348f7818ab2",
    },
    Pin {
        name: "ggml-cpu-cooperlake.dll",
        sha256: "62e9ddc1d48f00cc59d9ab73ceb255d76b52a7ddbb8dda15912e92622ea17d59",
    },
    Pin {
        name: "ggml-cpu-haswell.dll",
        sha256: "81ffc28ad276950c5d75edfdeef9f4ea7116e1d29f55e9aafdd500758baa0c46",
    },
    Pin {
        name: "ggml-cpu-icelake.dll",
        sha256: "29929b7164ff650c4d4d12bac176633963170e36430794999831d03f9f107f33",
    },
    Pin {
        name: "ggml-cpu-ivybridge.dll",
        sha256: "b9de7a9006f2a216cedb3e6f62508169b9e633418c0cb283ab9e454f1c22f6a7",
    },
    Pin {
        name: "ggml-cpu-piledriver.dll",
        sha256: "98b546b7df21aae82e2a170395e6c40a8aab79dc3cc9958a32e474a82e57601c",
    },
    Pin {
        name: "ggml-cpu-sandybridge.dll",
        sha256: "2480366757f50adc29614c32ddcc70e8599ce31799ae18f6666d0c72cf922432",
    },
    Pin {
        name: "ggml-cpu-sapphirerapids.dll",
        sha256: "64bfd2dae817f4adccfbfe4511ad4aabfe10170913988ed25ac37611584101a1",
    },
    Pin {
        name: "ggml-cpu-skylakex.dll",
        sha256: "626850b6c5e336e87adb73e9692e653d799201fa39410b5e20bd7b7d56f21d15",
    },
    Pin {
        name: "ggml-cpu-sse42.dll",
        sha256: "941b4b7d75e81919e3737ce211956ebcc2a8b93f63a9d38e86d1df0dda4fb42f",
    },
    Pin {
        name: "ggml-cpu-x64.dll",
        sha256: "fae466e3881ac221229a07cee56d069ea85dffd5c19e100e50e89de623f64ac4",
    },
    Pin {
        name: "ggml-cpu-zen4.dll",
        sha256: "3db88952370ea447f1ef2844d816ff05eecae15969837848308782cd2e0455ee",
    },
    Pin {
        name: "ggml-rpc.dll",
        sha256: "6301706f2dbe3dc392a69999d61f8d8155014e39aaf61978080ab2e8f9566360",
    },
    Pin {
        name: "ggml-vulkan.dll",
        sha256: "7e67f90f53885bb5661ce4573d048dee9ac80445ffefb49d793f524da49137e2",
    },
    Pin {
        name: "ggml.dll",
        sha256: "00c7cd52893935d60b99f57a67f06175df95114e3b86371aa240408f1f795578",
    },
    Pin {
        name: "libomp140.x86_64.dll",
        sha256: "9ab1cb787e52b2a36133899c01c1db1f876067d1471cd224ead85f40a8152b99",
    },
    Pin {
        name: "llama-server.exe",
        sha256: "cf47cb082d5811763d61acc9e6274f196d1d93e6dd5793c92d994977e696eb85",
    },
    Pin {
        name: "llama.dll",
        sha256: "cefbaf3d2712948075b6e3991013e966e27abe8e6d1fbf042db59642c618289b",
    },
    Pin {
        name: "mtmd.dll",
        sha256: "1008af46d25d0d430e8058f9d7c1119ef91036e450aa28bf07c9f1fcc009e94e",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO_SHA256: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
    const PINS: &[Pin] = &[
        Pin {
            name: "llama-server.exe",
            sha256: HELLO_SHA256,
        },
        Pin {
            name: "ggml.dll",
            sha256: HELLO_SHA256,
        },
    ];

    fn folder(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("rs-pins-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for file in ["llama-server.exe", "ggml.dll"] {
            std::fs::write(dir.join(file), b"hello").unwrap();
        }
        dir
    }

    #[test]
    fn a_folder_of_the_pinned_files_passes() {
        let dir = folder("ok");
        assert_eq!(verify_dir(&dir, PINS), vec![]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_changed_file_is_named() {
        let dir = folder("changed");
        std::fs::write(dir.join("ggml.dll"), b"hellO").unwrap();
        assert_eq!(
            verify_dir(&dir, PINS),
            vec![Problem::Changed("ggml.dll".into())]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_file_is_named() {
        let dir = folder("missing");
        std::fs::remove_file(dir.join("ggml.dll")).unwrap();
        assert_eq!(
            verify_dir(&dir, PINS),
            vec![Problem::Missing("ggml.dll".into())]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_folder_means_every_file_is_missing() {
        let dir = std::env::temp_dir().join(format!("rs-pins-nowhere-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            verify_dir(&dir, PINS),
            vec![
                Problem::Missing("llama-server.exe".into()),
                Problem::Missing("ggml.dll".into())
            ]
        );
    }

    #[test]
    fn a_program_or_library_nobody_pinned_is_refused() {
        // A DLL the server could load from its own folder is as dangerous as a changed one.
        let dir = folder("extra");
        std::fs::write(dir.join("version.dll"), b"x").unwrap();
        std::fs::write(dir.join("Helper.EXE"), b"x").unwrap();
        assert_eq!(
            verify_dir(&dir, PINS),
            vec![
                Problem::Unexpected("Helper.EXE".into()),
                Problem::Unexpected("version.dll".into())
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn files_that_cannot_run_are_left_alone() {
        let dir = folder("harmless");
        std::fs::write(dir.join("llama-server.log"), b"log").unwrap();
        std::fs::write(dir.join("server_temp.zip"), b"zip").unwrap();
        std::fs::create_dir_all(dir.join("sub.dll")).unwrap(); // a folder, not a library
        assert_eq!(verify_dir(&dir, PINS), vec![]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn names_compare_the_way_windows_does() {
        let dir = folder("case");
        std::fs::rename(dir.join("ggml.dll"), dir.join("GGML.DLL")).unwrap();
        assert_eq!(verify_dir(&dir, PINS), vec![]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_where_a_file_belongs_is_a_missing_file() {
        let dir = folder("folder-in-place");
        std::fs::remove_file(dir.join("ggml.dll")).unwrap();
        std::fs::create_dir_all(dir.join("ggml.dll")).unwrap();
        assert_eq!(
            verify_dir(&dir, PINS),
            vec![Problem::Missing("ggml.dll".into())]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_cannot_be_read_is_not_vouched_for() {
        let dir = folder("unreadable");
        let locked = |path: &std::path::Path| {
            if path.ends_with("ggml.dll") {
                Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
            } else {
                sha256_file(path)
            }
        };
        assert_eq!(
            verify_dir_with(&dir, PINS, locked),
            vec![Problem::Unreadable("ggml.dll".into())]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_message_names_a_few_files_and_counts_the_rest() {
        let one = vec![Problem::Changed("ggml.dll".into())];
        assert_eq!(summary(&one), "changed: ggml.dll");
        let many: Vec<Problem> = (1..=5)
            .map(|i| Problem::Missing(format!("f{i}.dll")))
            .collect();
        assert_eq!(
            summary(&many),
            "missing: f1.dll; missing: f2.dll; missing: f3.dll; and 2 more"
        );
        assert_eq!(
            summary(&[Problem::Unreadable("a.dll".into())]),
            "unreadable: a.dll"
        );
        let mixed = vec![
            Problem::Unexpected("a.dll".into()),
            Problem::Changed("b.dll".into()),
        ];
        assert_eq!(
            summary(&mixed),
            "not part of the AI engine: a.dll; changed: b.dll"
        );
    }

    #[test]
    fn the_pinned_table_is_the_whole_engine() {
        // 21 libraries and the server, from the pinned zip (.github/scripts/ai-server-pins.py).
        assert_eq!(AI_SERVER_PINS.len(), 22);
        assert!(AI_SERVER_PINS.iter().any(|p| p.name == "llama-server.exe"));
        let mut names: Vec<_> = AI_SERVER_PINS
            .iter()
            .map(|p| p.name.to_lowercase())
            .collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 22, "no name twice");
        for pin in AI_SERVER_PINS {
            assert_eq!(pin.sha256.len(), 64, "{}", pin.name);
            assert!(
                pin.sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "{}",
                pin.name
            );
            let ext = pin.name.rsplit('.').next().unwrap();
            assert!(ext == "dll" || ext == "exe", "{}", pin.name);
        }
    }
}
