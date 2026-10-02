//! Names of the sniffer's Windows Firewall rules.
//!
//! The rule that lets the raw socket receive game traffic is bound to one exe
//! (`program=<path>`). The dev exe (`target/debug`) and the installed exe are
//! different programs, so each gets its own rule: switching between them must
//! not delete the other's rule. The name carries a short hash of the exe path
//! -- ASCII, so `netsh` output and exit codes behave the same on any locale
//! and for any user name in the path.

/// The name every rule starts with; also the name of the single rule older
/// versions created for whichever exe ran setup last.
pub const LEGACY_RULE_NAME: &str = "Resonance Stream (Packet Sniffing)";

/// The rule name for the exe at `exe_path`. Windows paths ignore case and
/// accept either slash, so those do not change the name.
pub fn rule_name_for(exe_path: &str) -> String {
    // FNV-1a over the normalised path: tiny, and the same on every build.
    let hash = exe_path
        .to_lowercase()
        .replace('\\', "/")
        .bytes()
        .fold(0x811c_9dc5_u32, |h, b| {
            (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
        });
    format!("{LEGACY_RULE_NAME} [{hash:08x}]")
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEV: &str = r"C:\Users\kade\resonance-stream\target\debug\resonance-stream.exe";
    const RELEASE: &str = r"C:\Program Files\Resonance Stream\resonance-stream.exe";

    #[test]
    fn name_starts_with_the_legacy_name_and_ends_in_eight_hex_digits() {
        let name = rule_name_for(DEV);
        let hash = name
            .strip_prefix(LEGACY_RULE_NAME)
            .and_then(|rest| rest.strip_prefix(" ["))
            .and_then(|rest| rest.strip_suffix(']'))
            .expect("`<legacy name> [<hash>]`");
        assert_eq!(hash.len(), 8);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn name_is_stable() {
        assert_eq!(rule_name_for(DEV), rule_name_for(DEV));
    }

    #[test]
    fn dev_and_release_exes_get_different_names() {
        assert_ne!(rule_name_for(DEV), rule_name_for(RELEASE));
    }

    #[test]
    fn case_and_slash_style_do_not_change_the_name() {
        assert_eq!(
            rule_name_for(DEV),
            rule_name_for(&DEV.to_uppercase().replace('\\', "/"))
        );
    }

    #[test]
    fn name_is_ascii_even_for_a_korean_path() {
        let name = rule_name_for(r"C:\Users\케이드\resonance-stream.exe");
        assert!(name.is_ascii());
        assert_ne!(name, rule_name_for(RELEASE));
    }

    #[test]
    fn name_differs_from_the_legacy_name() {
        assert_ne!(rule_name_for(DEV), LEGACY_RULE_NAME);
    }
}
