import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from w1 import interface  # noqa: E402

FIX = Path(__file__).resolve().parent / "fixtures"
RUST = Path(__file__).resolve().parents[2] / "crates" / "core" / "src" / "sniffer_net.rs"


def text(name):
    return (FIX / name).read_text(encoding="utf-8")


def adapters(name):
    return interface.parse_adapters(text(name))


def statuses(checks):
    return {title: status for title, status, _ in checks}


# --- the route table -------------------------------------------------------------------------
def test_default_routes_are_the_0_0_0_0_rows():
    routes = interface.parse_default_routes(text("route_print_two_adapters.txt"))
    assert [(r.gateway, r.interface_ip, r.metric) for r in routes] == [
        ("192.168.0.1", "192.168.0.23", 25), ("192.168.0.1", "192.168.0.57", 50)]


def test_the_default_route_is_the_lowest_metric():
    routes = interface.parse_default_routes(text("route_print_vpn.txt"))
    assert interface.default_route_ip(routes) == "10.8.0.2"


def test_no_default_route_is_none():
    assert interface.default_route_ip(interface.parse_default_routes("nothing here")) is None


# --- adapters --------------------------------------------------------------------------------
def test_parse_adapters_reads_the_powershell_json():
    a = adapters("adapters_two.json")
    assert [x.ip for x in a] == ["192.168.0.23", "192.168.0.57", "127.0.0.1"]
    assert a[0].alias == "이더넷" and not a[0].virtual


def test_one_adapter_as_a_bare_object_is_a_list_of_one():
    assert len(interface.parse_adapters('{"Alias":"a","IP":"1.2.3.4","Description":"d","Status":"Up","Virtual":false}')) == 1


def test_adapter_garbage_is_an_error():
    with pytest.raises(ValueError):
        interface.parse_adapters("The term 'Get-NetAdapter' is not recognized")


# --- the app's keyword list, read from the Rust source so it cannot drift ---------------------
def test_the_keywords_come_from_the_rust_source():
    words = interface.virtual_keywords(RUST.read_text(encoding="utf-8"))
    assert "TAP" in words and "WireGuard" in words and "vEthernet" in words
    assert len(words) == 13


def test_a_name_with_a_keyword_is_virtual_whatever_the_case():
    words = ["TAP", "WireGuard"]
    assert interface.looks_virtual("tap-windows adapter v9", words)
    assert interface.looks_virtual("My WIREGUARD tunnel", words)
    assert not interface.looks_virtual("Realtek PCIe GbE Family Controller", words)


# --- the app's own log line --------------------------------------------------------------------
def test_parse_the_log_line():
    line = "[12:00:01] Sniffer: Auto-Targeting Network Interface: 192.168.0.23 (default route)"
    assert interface.parse_log_line(line) == ("192.168.0.23", "default route")
    assert interface.parse_log_line("Auto-Targeting Network Interface: 10.0.0.5 (first physical adapter)") == ("10.0.0.5", "first physical adapter")


def test_a_line_that_is_not_the_log_line_is_an_error():
    with pytest.raises(ValueError):
        interface.parse_log_line("Sniffer: Binding to 192.168.0.23")


# --- scenario 1: two live adapters ------------------------------------------------------------
def test_two_adapters_all_good():
    checks = interface.check_default_route(
        ("192.168.0.23", "default route"),
        interface.parse_default_routes(text("route_print_two_adapters.txt")),
        adapters("adapters_two.json"), interface.virtual_keywords(RUST.read_text(encoding="utf-8")))
    assert set(statuses(checks).values()) == {"pass"}, checks


def test_two_adapters_but_the_app_took_the_list_rule_is_a_failure():
    checks = interface.check_default_route(
        ("192.168.0.23", "first physical adapter"),
        interface.parse_default_routes(text("route_print_two_adapters.txt")),
        adapters("adapters_two.json"), ["TAP"])
    assert "fail" in statuses(checks).values()


def test_two_adapters_but_the_app_picked_the_other_adapter_is_a_failure():
    checks = interface.check_default_route(
        ("192.168.0.57", "default route"),
        interface.parse_default_routes(text("route_print_two_adapters.txt")),
        adapters("adapters_two.json"), ["TAP"])
    assert any(s == "fail" and "route" in t for t, s, _ in checks), checks


def test_an_ip_that_is_not_on_this_machine_is_a_failure():
    checks = interface.check_default_route(
        ("203.0.113.9", "default route"),
        interface.parse_default_routes(text("route_print_two_adapters.txt")),
        adapters("adapters_two.json"), ["TAP"])
    assert any(s == "fail" for _, s, _ in checks)


# --- scenario 2: a full-tunnel VPN ------------------------------------------------------------
KW = ["TAP", "OpenVPN", "WireGuard"]


def vpn_checks(logged, adapters_file="adapters_vpn.json", route="route_print_vpn.txt"):
    return interface.check_vpn(
        logged, interface.parse_default_routes(text(route)), adapters(adapters_file), KW)


def test_vpn_the_physical_adapter_is_picked_by_the_list_rule():
    assert set(statuses(vpn_checks(("192.168.0.23", "first physical adapter"))).values()) == {"pass"}


def test_vpn_picking_the_vpn_adapter_is_a_failure():
    checks = vpn_checks(("10.8.0.2", "default route"))
    assert "fail" in statuses(checks).values()


def test_vpn_not_the_default_route_voids_the_scenario():
    # the VPN is up but does not carry the default route: nothing here says anything
    checks = vpn_checks(("192.168.0.23", "default route"), route="route_print_two_adapters.txt")
    assert statuses(checks)["precondition: the default route runs through a virtual adapter"] == "skip"


def test_vpn_adapter_the_app_does_not_know_is_flagged():
    checks = vpn_checks(("10.8.0.2", "default route"), adapters_file="adapters_unknown_vpn.json")
    flagged = [t for t, s, _ in checks if s == "fail" and "keyword" in t]
    assert flagged, checks
