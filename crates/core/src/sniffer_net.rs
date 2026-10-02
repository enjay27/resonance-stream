//! Sniffer network setup that needs no Windows API: the names of its Windows
//! Firewall rules, and which network adapter the raw socket binds to.
//!
//! ## Firewall rule names
//!
//! The rule that lets the raw socket receive game traffic is bound to one exe
//! (`program=<path>`). The dev exe (`target/debug`) and the installed exe are
//! different programs, so each gets its own rule: switching between them must
//! not delete the other's rule. The name carries a short hash of the exe path
//! -- ASCII, so `netsh` output and exit codes behave the same on any locale
//! and for any user name in the path.

use std::net::Ipv4Addr;

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

/// Adapter names that mark a virtual adapter or VPN (matched case-insensitively
/// as a substring). The game's packets are not seen on these.
pub const VIRTUAL_ADAPTER_KEYWORDS: [&str; 13] = [
    "Loopback",
    "vEthernet",
    "TAP",
    "Tailscale",
    "WireGuard",
    "OpenVPN",
    "Radmin",
    "Hamachi",
    "ZeroTier",
    "VMware",
    "VirtualBox",
    "WSL",
    "Npcap",
];

/// Which rule chose the adapter -- the app logs it, so a wrong pick can be
/// told apart from a wrong rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickRule {
    /// The adapter the OS routes outgoing traffic through.
    Route,
    /// The first physical adapter in the OS's list (the route was unknown or
    /// unusable).
    ListOrder,
}

/// The adapter to bind the raw socket to, and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterfacePick {
    pub ip: Ipv4Addr,
    pub rule: PickRule,
}

/// Picks the adapter for the raw socket from the machine's IPv4 adapters
/// (`name`, `ip`, in the OS's order). `route_ip` is the source address the OS
/// would use to reach the internet, when it could be asked.
///
/// The game's traffic leaves by the routed adapter, so that one wins -- unless
/// it is unusable (loopback / link-local), not among `candidates`, or a
/// virtual adapter. The last case is a full-tunnel VPN: the route says "VPN",
/// but the packets are seen on the physical adapter, as before this rule
/// existed. Then, and when there is no route, the first physical adapter in
/// list order is taken.
pub fn pick_interface(
    candidates: &[(String, Ipv4Addr)],
    route_ip: Option<Ipv4Addr>,
) -> Option<InterfacePick> {
    let usable = |ip: &Ipv4Addr| !ip.is_loopback() && !ip.is_link_local();

    let routed = route_ip.filter(usable).and_then(|route| {
        candidates
            .iter()
            .find(|(_, ip)| *ip == route)
            .filter(|(name, _)| !is_virtual_adapter(name))
    });
    if let Some((_, ip)) = routed {
        return Some(InterfacePick {
            ip: *ip,
            rule: PickRule::Route,
        });
    }

    candidates
        .iter()
        .find(|(name, ip)| !is_virtual_adapter(name) && usable(ip))
        .map(|(_, ip)| InterfacePick {
            ip: *ip,
            rule: PickRule::ListOrder,
        })
}

fn is_virtual_adapter(name: &str) -> bool {
    let name = name.to_lowercase();
    VIRTUAL_ADAPTER_KEYWORDS
        .iter()
        .any(|keyword| name.contains(&keyword.to_lowercase()))
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

    fn adapters(list: &[(&str, [u8; 4])]) -> Vec<(String, Ipv4Addr)> {
        list.iter()
            .map(|(name, ip)| ((*name).to_string(), Ipv4Addr::from(*ip)))
            .collect()
    }

    fn pick(list: &[(&str, [u8; 4])], route: Option<[u8; 4]>) -> Option<InterfacePick> {
        pick_interface(&adapters(list), route.map(Ipv4Addr::from))
    }

    fn picked(ip: [u8; 4], rule: PickRule) -> Option<InterfacePick> {
        Some(InterfacePick {
            ip: Ipv4Addr::from(ip),
            rule,
        })
    }

    #[test]
    fn the_routed_adapter_wins_over_list_order() {
        let list = [("Ethernet", [192, 168, 0, 5]), ("Wi-Fi", [10, 0, 0, 7])];
        assert_eq!(
            pick(&list, Some([10, 0, 0, 7])),
            picked([10, 0, 0, 7], PickRule::Route)
        );
    }

    #[test]
    fn a_route_that_is_not_an_adapter_falls_back_to_list_order() {
        let list = [("Ethernet", [192, 168, 0, 5]), ("Wi-Fi", [10, 0, 0, 7])];
        assert_eq!(
            pick(&list, Some([203, 0, 113, 9])),
            picked([192, 168, 0, 5], PickRule::ListOrder)
        );
    }

    #[test]
    fn without_a_route_the_first_physical_adapter_is_taken() {
        let list = [("Ethernet", [192, 168, 0, 5]), ("Wi-Fi", [10, 0, 0, 7])];
        assert_eq!(
            pick(&list, None),
            picked([192, 168, 0, 5], PickRule::ListOrder)
        );
    }

    #[test]
    fn a_route_through_a_virtual_adapter_is_not_trusted() {
        // Full-tunnel VPN: the route is the VPN, the packets are on Ethernet.
        let list = [
            ("Tailscale", [100, 64, 0, 2]),
            ("Ethernet", [192, 168, 0, 5]),
        ];
        assert_eq!(
            pick(&list, Some([100, 64, 0, 2])),
            picked([192, 168, 0, 5], PickRule::ListOrder)
        );
    }

    #[test]
    fn a_loopback_or_link_local_route_is_not_trusted() {
        let list = [
            ("Loopback Pseudo-Interface 1", [127, 0, 0, 1]),
            ("Ethernet", [192, 168, 0, 5]),
        ];
        assert_eq!(
            pick(&list, Some([127, 0, 0, 1])),
            picked([192, 168, 0, 5], PickRule::ListOrder)
        );
        let list = [
            ("Ethernet 2", [169, 254, 1, 1]),
            ("Ethernet", [192, 168, 0, 5]),
        ];
        assert_eq!(
            pick(&list, Some([169, 254, 1, 1])),
            picked([192, 168, 0, 5], PickRule::ListOrder)
        );
    }

    #[test]
    fn virtual_adapters_are_skipped_whatever_the_case() {
        let list = [
            ("vethernet (WSL)", [172, 20, 0, 1]),
            ("VMware Network Adapter VMnet8", [192, 168, 40, 1]),
            ("Ethernet", [192, 168, 0, 5]),
        ];
        assert_eq!(
            pick(&list, None),
            picked([192, 168, 0, 5], PickRule::ListOrder)
        );
    }

    #[test]
    fn loopback_and_link_local_addresses_are_never_picked_from_the_list() {
        let list = [
            ("Ethernet 2", [169, 254, 1, 1]),
            ("Local", [127, 0, 0, 1]),
            ("Ethernet", [192, 168, 0, 5]),
        ];
        assert_eq!(
            pick(&list, None),
            picked([192, 168, 0, 5], PickRule::ListOrder)
        );
    }

    #[test]
    fn nothing_usable_picks_nothing() {
        assert_eq!(pick(&[], Some([10, 0, 0, 7])), None);
        let list = [("Tailscale", [100, 64, 0, 2]), ("Local", [127, 0, 0, 1])];
        assert_eq!(pick(&list, Some([100, 64, 0, 2])), None);
    }
}
