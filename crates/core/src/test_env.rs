//! Parameters for a test run of the app: where it keeps its data, which
//! background work it skips, and where it reads its update feed from.
//!
//! This module only *parses and resolves*: it decides what the flags mean and
//! needs no Tauri, so every rule is tested on any OS. Whether the app honours
//! them at all (debug builds and the `test-env` feature only) is decided in the
//! app crate -- a normal release never calls [`parse`].
//!
//! Every flag has an environment variable, `RESONANCE_TEST_<FLAG>` with the
//! dashes as underscores (`--no-capture` -> `RESONANCE_TEST_NO_CAPTURE`). A flag
//! on the command line wins over its variable.

use std::fmt;
use std::io;
use std::path::{Component, Path, PathBuf};

/// Prefix of the environment variable of every flag.
pub const ENV_PREFIX: &str = "RESONANCE_TEST_";

/// The folders `--data-dir` makes: the three places the app writes.
const CONFIG_SUBDIR: &str = "config";
const DATA_SUBDIR: &str = "data";
const WEBVIEW_SUBDIR: &str = "webview";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// On or off; takes no value on the command line.
    Switch,
    /// Takes one value (`--flag value` or `--flag=value`).
    Value,
}

/// Every flag, in the order `TestEnv::set_flags` lists them.
const FLAGS: [(&str, Kind); 13] = [
    ("data-dir", Kind::Value),
    ("fresh", Kind::Switch),
    ("assume-setup-done", Kind::Switch),
    ("no-capture", Kind::Switch),
    ("no-translator", Kind::Switch),
    ("no-update-check", Kind::Switch),
    ("no-popups", Kind::Switch),
    ("no-window-state", Kind::Switch),
    ("feed-url", Kind::Value),
    ("metadata-url", Kind::Value),
    ("status-file", Kind::Value),
    ("log-file", Kind::Value),
    ("print-env", Kind::Switch),
];

/// What a test run asked for. `TestEnv::default()` is a normal run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TestEnv {
    /// Root of everything the app writes (see [`resolve_dirs`]).
    pub data_dir: Option<PathBuf>,
    /// Empty `data_dir` first. Only valid together with `data_dir`.
    pub fresh: bool,
    /// Behave as if setup were finished, without writing that down.
    pub assume_setup_done: bool,
    pub no_capture: bool,
    pub no_translator: bool,
    pub no_update_check: bool,
    pub no_popups: bool,
    pub no_window_state: bool,
    /// Read the update feed from here instead of the release feed.
    pub feed_url: Option<String>,
    /// Read the gist metadata from here instead of the public gist.
    pub metadata_url: Option<String>,
    /// Write a JSON status file (start, ready, update state) here.
    pub status_file: Option<PathBuf>,
    /// Also write the log here (a release exe has no console).
    pub log_file: Option<PathBuf>,
    /// Print the resolved settings as JSON and exit.
    pub print_env: bool,
}

impl TestEnv {
    /// The flags that are set, by name, in a fixed order -- what the status
    /// file and `--print-env` report as "what was asked for".
    pub fn set_flags(&self) -> Vec<&'static str> {
        let set = [
            self.data_dir.is_some(),
            self.fresh,
            self.assume_setup_done,
            self.no_capture,
            self.no_translator,
            self.no_update_check,
            self.no_popups,
            self.no_window_state,
            self.feed_url.is_some(),
            self.metadata_url.is_some(),
            self.status_file.is_some(),
            self.log_file.is_some(),
            self.print_env,
        ];
        FLAGS
            .iter()
            .zip(set)
            .filter_map(|((name, _), on)| on.then_some(*name))
            .collect()
    }

    fn set_switch(&mut self, name: &str, on: bool) {
        match name {
            "fresh" => self.fresh = on,
            "assume-setup-done" => self.assume_setup_done = on,
            "no-capture" => self.no_capture = on,
            "no-translator" => self.no_translator = on,
            "no-update-check" => self.no_update_check = on,
            "no-popups" => self.no_popups = on,
            "no-window-state" => self.no_window_state = on,
            "print-env" => self.print_env = on,
            _ => unreachable!("not a switch: {name}"),
        }
    }

    fn set_value(&mut self, name: &str, value: String) -> Result<(), TestEnvError> {
        match name {
            "data-dir" => self.data_dir = Some(PathBuf::from(value)),
            "status-file" => self.status_file = Some(PathBuf::from(value)),
            "log-file" => self.log_file = Some(PathBuf::from(value)),
            "feed-url" | "metadata-url" => {
                if !is_test_url_allowed(&value) {
                    return Err(TestEnvError::BadUrl {
                        flag: name.to_string(),
                        url: value,
                    });
                }
                if name == "feed-url" {
                    self.feed_url = Some(value);
                } else {
                    self.metadata_url = Some(value);
                }
            }
            _ => unreachable!("not a value flag: {name}"),
        }
        Ok(())
    }
}

/// Why the parameters were refused. The app prints it and exits: a test run
/// that silently ignored a typo would test the wrong thing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestEnvError {
    /// `--something` that is not a flag.
    UnknownFlag(String),
    /// A word that is not a flag at all.
    UnexpectedArg(String),
    /// A value flag at the end of the line, before another flag, or empty.
    MissingValue(String),
    /// `--fresh=1`: switches take no value (set `RESONANCE_TEST_FRESH=0` to turn one off).
    UnexpectedValue(String),
    /// An environment variable of a switch that is not yes/no.
    BadSwitchEnv { var: String, value: String },
    /// `--fresh` without `--data-dir` would empty the real data folders.
    FreshNeedsDataDir,
    /// A feed / metadata URL that is neither `https` nor local `http`.
    BadUrl { flag: String, url: String },
}

impl fmt::Display for TestEnvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownFlag(flag) => write!(f, "unknown test flag --{flag}"),
            Self::UnexpectedArg(arg) => write!(f, "unexpected argument {arg:?}"),
            Self::MissingValue(flag) => write!(f, "--{flag} needs a value"),
            Self::UnexpectedValue(flag) => write!(f, "--{flag} takes no value"),
            Self::BadSwitchEnv { var, value } => {
                write!(
                    f,
                    "{var}={value:?} is not one of 1/0, true/false, yes/no, on/off"
                )
            }
            Self::FreshNeedsDataDir => {
                write!(
                    f,
                    "--fresh needs --data-dir (it would empty the real data folders)"
                )
            }
            Self::BadUrl { flag, url } => write!(
                f,
                "--{flag} {url:?}: only https:// or http://127.0.0.1 / localhost / [::1]"
            ),
        }
    }
}

impl std::error::Error for TestEnvError {}

fn env_name(flag: &str) -> String {
    format!("{ENV_PREFIX}{}", flag.to_uppercase().replace('-', "_"))
}

fn parse_switch_env(var: &str, value: &str) -> Result<bool, TestEnvError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "" | "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(TestEnvError::BadSwitchEnv {
            var: var.to_string(),
            value: value.to_string(),
        }),
    }
}

/// Reads the test parameters from the command line (without the program name)
/// and the environment (`env` looks one variable up).
pub fn parse<I, S>(args: I, env: impl Fn(&str) -> Option<String>) -> Result<TestEnv, TestEnvError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut out = TestEnv::default();

    for (name, kind) in FLAGS {
        let var = env_name(name);
        let Some(value) = env(&var) else { continue };
        match kind {
            Kind::Switch => out.set_switch(name, parse_switch_env(&var, &value)?),
            // An empty variable is "not set", as in every shell.
            Kind::Value if value.is_empty() => {}
            Kind::Value => out.set_value(name, value)?,
        }
    }

    let args: Vec<String> = args.into_iter().map(|a| a.as_ref().to_string()).collect();
    let mut args = args.into_iter().peekable();
    while let Some(arg) = args.next() {
        let Some(flag) = arg.strip_prefix("--") else {
            return Err(TestEnvError::UnexpectedArg(arg));
        };
        let (name, inline) = match flag.split_once('=') {
            Some((name, value)) => (name, Some(value.to_string())),
            None => (flag, None),
        };
        let Some(&(name, kind)) = FLAGS.iter().find(|(known, _)| *known == name) else {
            return Err(TestEnvError::UnknownFlag(name.to_string()));
        };
        match kind {
            Kind::Switch => {
                if inline.is_some() {
                    return Err(TestEnvError::UnexpectedValue(name.to_string()));
                }
                out.set_switch(name, true);
            }
            Kind::Value => {
                let value = match inline {
                    Some(value) => value,
                    None => match args.peek() {
                        Some(next) if !next.starts_with("--") => args.next().unwrap_or_default(),
                        _ => String::new(),
                    },
                };
                if value.is_empty() {
                    return Err(TestEnvError::MissingValue(name.to_string()));
                }
                out.set_value(name, value)?;
            }
        }
    }

    if out.fresh && out.data_dir.is_none() {
        return Err(TestEnvError::FreshNeedsDataDir);
    }
    Ok(out)
}

/// Whether `url` may be a test feed / metadata source: `https://host...`, or
/// plain `http` to this machine only (`127.0.0.1`, `localhost`, `[::1]`), so a
/// test can run against a local mock server without ever sending an update
/// check over the network unencrypted.
pub fn is_test_url_allowed(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once("://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    // `http://127.0.0.1@evil.example/` is a request to evil.example.
    if authority.contains('@') {
        return false;
    }
    let Some((host, port)) = host_and_port(authority) else {
        return false;
    };
    if !port.is_none_or(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())) {
        return false;
    }
    match scheme.to_ascii_lowercase().as_str() {
        "https" => true,
        "http" => {
            let host = host.to_ascii_lowercase();
            matches!(host.as_str(), "127.0.0.1" | "localhost" | "[::1]")
        }
        _ => false,
    }
}

/// `host`, `host:port`, `[v6]` or `[v6]:port` -> (host, port). `None` when the
/// host is empty or the brackets do not match.
fn host_and_port(authority: &str) -> Option<(&str, Option<&str>)> {
    let (host, port) = if authority.starts_with('[') {
        let end = authority.find(']')? + 1;
        let (host, rest) = authority.split_at(end);
        match rest {
            "" => (host, None),
            _ => (host, Some(rest.strip_prefix(':')?)),
        }
    } else {
        match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        }
    };
    (!host.is_empty() && host != "[]").then_some((host, port))
}

/// The folders the app writes to in this run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppDirs {
    /// The `--data-dir` these were made from; `None` in a normal run.
    pub root: Option<PathBuf>,
    /// `config.json`, metadata (Tauri's `app_config_dir`).
    pub config: PathBuf,
    /// Model, server, chat logs, dictionary, raw capture (Tauri's `app_data_dir`).
    pub data: PathBuf,
    /// WebView2's user data folder to set (`WEBVIEW2_USER_DATA_FOLDER`);
    /// `None` leaves the default under `%LOCALAPPDATA%`.
    pub webview: Option<PathBuf>,
}

/// The folders for this run: the app's own defaults, or the three folders
/// under `--data-dir`. With no flag the defaults come back unchanged.
pub fn resolve_dirs(default_config: &Path, default_data: &Path, env: &TestEnv) -> AppDirs {
    match &env.data_dir {
        Some(root) => AppDirs {
            root: Some(root.clone()),
            config: root.join(CONFIG_SUBDIR),
            data: root.join(DATA_SUBDIR),
            webview: Some(root.join(WEBVIEW_SUBDIR)),
        },
        None => AppDirs {
            root: None,
            config: default_config.to_path_buf(),
            data: default_data.to_path_buf(),
            webview: None,
        },
    }
}

impl AppDirs {
    /// `--fresh`: empties the three folders under the root and creates them
    /// again. Nothing else in the root is touched, and a root that looks like a
    /// drive or a top-level folder (`C:\`, `/`, `/home`) or has a `..` is
    /// refused, so a wrong `--data-dir` cannot cost more than the test's own
    /// folders. A run without a root has nothing to reset: that is an error.
    pub fn reset(&self) -> io::Result<()> {
        let invalid = |msg: &str| io::Error::new(io::ErrorKind::InvalidInput, msg.to_string());
        let root = self
            .root
            .as_deref()
            .ok_or_else(|| invalid("no --data-dir: nothing to reset"))?;
        if root.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(invalid("--data-dir must not contain .."));
        }
        // Drive/root + at least two names: `C:\w1\run1`, `/tmp/run1`.
        if root.components().count() < 3 {
            return Err(invalid("--data-dir is too close to the top of a drive"));
        }
        for dir in [Some(&self.config), Some(&self.data), self.webview.as_ref()]
            .into_iter()
            .flatten()
        {
            match std::fs::remove_dir_all(dir) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn run(args: &[&str], vars: &[(&str, &str)]) -> Result<TestEnv, TestEnvError> {
        let vars: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        parse(args.iter().copied(), |k| vars.get(k).cloned())
    }

    fn ok(args: &[&str]) -> TestEnv {
        run(args, &[]).expect("parses")
    }

    #[test]
    fn no_flags_is_a_normal_run() {
        let env = ok(&[]);
        assert_eq!(env, TestEnv::default());
        assert!(env.set_flags().is_empty());
    }

    #[test]
    fn a_value_flag_takes_the_next_word_or_an_equals_sign() {
        assert_eq!(
            ok(&["--data-dir", "C:\\w1\\run1"]).data_dir,
            Some("C:\\w1\\run1".into())
        );
        assert_eq!(
            ok(&["--data-dir=/tmp/run1"]).data_dir,
            Some("/tmp/run1".into())
        );
        // A value may itself contain `=`.
        assert_eq!(
            ok(&["--log-file=/tmp/a=b.log"]).log_file,
            Some("/tmp/a=b.log".into())
        );
    }

    #[test]
    fn every_switch_turns_on() {
        let env = ok(&[
            "--assume-setup-done",
            "--no-capture",
            "--no-translator",
            "--no-update-check",
            "--no-popups",
            "--no-window-state",
            "--print-env",
        ]);
        assert!(env.assume_setup_done);
        assert!(env.no_capture);
        assert!(env.no_translator);
        assert!(env.no_update_check);
        assert!(env.no_popups);
        assert!(env.no_window_state);
        assert!(env.print_env);
        assert!(!env.fresh);
    }

    #[test]
    fn the_other_value_flags_are_read() {
        let env = ok(&[
            "--status-file",
            "/tmp/status.json",
            "--log-file",
            "/tmp/app.log",
            "--feed-url",
            "https://example.com/latest.json",
            "--metadata-url=http://127.0.0.1:8099/meta.json",
        ]);
        assert_eq!(env.status_file, Some("/tmp/status.json".into()));
        assert_eq!(env.log_file, Some("/tmp/app.log".into()));
        assert_eq!(
            env.feed_url.as_deref(),
            Some("https://example.com/latest.json")
        );
        assert_eq!(
            env.metadata_url.as_deref(),
            Some("http://127.0.0.1:8099/meta.json")
        );
    }

    #[test]
    fn an_unknown_flag_or_a_stray_word_is_an_error() {
        assert_eq!(
            run(&["--no-captur"], &[]),
            Err(TestEnvError::UnknownFlag("no-captur".into()))
        );
        assert_eq!(
            run(&["--bogus=1"], &[]),
            Err(TestEnvError::UnknownFlag("bogus".into()))
        );
        assert_eq!(
            run(&["data-dir"], &[]),
            Err(TestEnvError::UnexpectedArg("data-dir".into()))
        );
    }

    #[test]
    fn a_value_flag_without_a_value_is_an_error() {
        let missing = |flag: &str| Err(TestEnvError::MissingValue(flag.into()));
        assert_eq!(run(&["--data-dir"], &[]), missing("data-dir"));
        // The next word is a flag, not the value.
        assert_eq!(run(&["--data-dir", "--fresh"], &[]), missing("data-dir"));
        assert_eq!(run(&["--log-file="], &[]), missing("log-file"));
    }

    #[test]
    fn a_switch_given_a_value_is_an_error() {
        assert_eq!(
            run(&["--no-capture=1"], &[]),
            Err(TestEnvError::UnexpectedValue("no-capture".into()))
        );
    }

    #[test]
    fn the_environment_sets_flags() {
        let env = run(
            &[],
            &[
                ("RESONANCE_TEST_DATA_DIR", "/tmp/run1"),
                ("RESONANCE_TEST_NO_CAPTURE", "1"),
                ("RESONANCE_TEST_ASSUME_SETUP_DONE", "TRUE"),
                ("RESONANCE_TEST_NO_POPUPS", "0"),
                ("RESONANCE_TEST_FEED_URL", "http://localhost:9/f.json"),
            ],
        )
        .expect("parses");
        assert_eq!(env.data_dir, Some("/tmp/run1".into()));
        assert!(env.no_capture);
        assert!(env.assume_setup_done);
        assert!(!env.no_popups);
        assert_eq!(env.feed_url.as_deref(), Some("http://localhost:9/f.json"));
    }

    #[test]
    fn an_empty_variable_is_not_set() {
        let env = run(
            &[],
            &[
                ("RESONANCE_TEST_DATA_DIR", ""),
                ("RESONANCE_TEST_NO_CAPTURE", ""),
            ],
        )
        .expect("parses");
        assert_eq!(env, TestEnv::default());
    }

    #[test]
    fn a_switch_variable_that_is_not_yes_or_no_is_an_error() {
        assert_eq!(
            run(&[], &[("RESONANCE_TEST_NO_CAPTURE", "maybe")]),
            Err(TestEnvError::BadSwitchEnv {
                var: "RESONANCE_TEST_NO_CAPTURE".into(),
                value: "maybe".into()
            })
        );
    }

    #[test]
    fn a_flag_wins_over_its_variable() {
        let env = run(
            &["--data-dir", "/tmp/from-flag", "--no-capture"],
            &[
                ("RESONANCE_TEST_DATA_DIR", "/tmp/from-env"),
                ("RESONANCE_TEST_NO_CAPTURE", "0"),
            ],
        )
        .expect("parses");
        assert_eq!(env.data_dir, Some("/tmp/from-flag".into()));
        assert!(env.no_capture);
    }

    #[test]
    fn fresh_needs_a_data_dir_from_either_source() {
        assert_eq!(run(&["--fresh"], &[]), Err(TestEnvError::FreshNeedsDataDir));
        assert_eq!(
            run(&[], &[("RESONANCE_TEST_FRESH", "1")]),
            Err(TestEnvError::FreshNeedsDataDir)
        );
        assert!(ok(&["--fresh", "--data-dir", "/tmp/run1"]).fresh);
        let env = run(&["--fresh"], &[("RESONANCE_TEST_DATA_DIR", "/tmp/run1")]).expect("parses");
        assert!(env.fresh);
    }

    #[test]
    fn a_bad_feed_url_is_an_error_from_a_flag_and_from_the_variable() {
        let bad = Err(TestEnvError::BadUrl {
            flag: "feed-url".into(),
            url: "http://example.com/latest.json".into(),
        });
        assert_eq!(
            run(&["--feed-url", "http://example.com/latest.json"], &[]),
            bad
        );
        assert_eq!(
            run(
                &[],
                &[("RESONANCE_TEST_FEED_URL", "http://example.com/latest.json")]
            ),
            bad
        );
    }

    #[test]
    fn set_flags_lists_what_is_set_in_a_fixed_order() {
        let env = ok(&[
            "--print-env",
            "--no-capture",
            "--data-dir",
            "/tmp/run1",
            "--fresh",
        ]);
        assert_eq!(
            env.set_flags(),
            ["data-dir", "fresh", "no-capture", "print-env"]
        );
    }

    #[test]
    fn https_and_local_http_urls_are_allowed() {
        for url in [
            "https://example.com/latest.json",
            "HTTPS://example.com",
            "https://example.com:8443/a?b=c#d",
            "http://127.0.0.1/feed",
            "http://127.0.0.1:8099/feed.json",
            "http://localhost:8099",
            "http://LOCALHOST/feed",
            "http://[::1]:8099/feed",
            "http://[::1]/feed",
        ] {
            assert!(is_test_url_allowed(url), "{url} should be allowed");
        }
    }

    #[test]
    fn other_urls_are_refused() {
        for url in [
            "",
            "example.com/feed",
            "https://",
            "https:///feed",
            "http://example.com/feed",
            "http://127.0.0.1.evil.example/feed",
            "http://localhost.evil.example/feed",
            "http://127.0.0.1@evil.example/feed",
            "https://user:pw@example.com/feed",
            "http://127.0.0.1:/feed",
            "http://127.0.0.1:80x/feed",
            "http://[::1/feed",
            "http://[]/feed",
            "ftp://127.0.0.1/feed",
            "file:///C:/feed.json",
        ] {
            assert!(!is_test_url_allowed(url), "{url} should be refused");
        }
    }

    #[test]
    fn without_a_data_dir_the_defaults_come_back_unchanged() {
        let dirs = resolve_dirs(Path::new("/def/config"), Path::new("/def/data"), &ok(&[]));
        assert_eq!(
            dirs,
            AppDirs {
                root: None,
                config: "/def/config".into(),
                data: "/def/data".into(),
                webview: None,
            }
        );
    }

    #[test]
    fn a_data_dir_holds_config_data_and_webview() {
        let env = ok(&["--data-dir", "/tmp/run1"]);
        let dirs = resolve_dirs(Path::new("/def/config"), Path::new("/def/data"), &env);
        let root = PathBuf::from("/tmp/run1");
        assert_eq!(dirs.root, Some(root.clone()));
        assert_eq!(dirs.config, root.join("config"));
        assert_eq!(dirs.data, root.join("data"));
        assert_eq!(dirs.webview, Some(root.join("webview")));
    }

    /// A folder of its own under the OS temp dir, empty.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("resonance-test-env-{}", std::process::id()))
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    fn dirs_for(root: &Path) -> AppDirs {
        let env = TestEnv {
            data_dir: Some(root.to_path_buf()),
            ..TestEnv::default()
        };
        resolve_dirs(Path::new("/def/config"), Path::new("/def/data"), &env)
    }

    #[test]
    fn reset_empties_the_three_folders_and_leaves_the_rest_of_the_root() {
        let root = scratch("reset-keeps-rest");
        let dirs = dirs_for(&root);
        for dir in [&dirs.config, &dirs.data] {
            std::fs::create_dir_all(dir.join("sub")).unwrap();
            std::fs::write(dir.join("sub").join("old.txt"), "old").unwrap();
        }
        std::fs::write(root.join("keep.txt"), "keep").unwrap();

        dirs.reset().expect("reset");

        for dir in [Some(&dirs.config), Some(&dirs.data), dirs.webview.as_ref()]
            .into_iter()
            .flatten()
        {
            assert!(dir.is_dir(), "{dir:?} exists");
            assert_eq!(
                std::fs::read_dir(dir).unwrap().count(),
                0,
                "{dir:?} is empty"
            );
        }
        assert_eq!(
            std::fs::read_to_string(root.join("keep.txt")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn reset_creates_the_folders_of_a_new_root() {
        let root = scratch("reset-new").join("not-yet");
        let dirs = dirs_for(&root);
        dirs.reset().expect("reset");
        assert!(dirs.config.is_dir() && dirs.data.is_dir());
        assert!(dirs.webview.as_ref().is_some_and(|w| w.is_dir()));
    }

    #[test]
    fn reset_refuses_a_root_near_the_top_or_with_parent_steps() {
        for root in ["/", "/home", "C:\\", "/tmp/../etc/x"] {
            let dirs = dirs_for(Path::new(root));
            assert!(dirs.reset().is_err(), "{root} should be refused");
        }
    }

    #[test]
    fn reset_without_a_data_dir_is_an_error() {
        let dirs = resolve_dirs(Path::new("/def/config"), Path::new("/def/data"), &ok(&[]));
        assert!(dirs.reset().is_err());
    }
}
