import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from runbook import firewall  # noqa: E402

FIX = Path(__file__).resolve().parent / "fixtures"
DEV = r"C:\dev\resonance-stream\target\debug\resonance-stream.exe"
RELEASE = r"C:\Program Files\Resonance Stream\resonance-stream.exe"


def load(name):
    return firewall.parse_rules((FIX / name).read_text(encoding="utf-8"))


# rule_name_for must be the Rust function's twin: these values come from compiling it.
@pytest.mark.parametrize("path,name", [
    (RELEASE, "Resonance Stream (Packet Sniffing) [8835dbcf]"),
    (r"C:\Users\케이드\resonance-stream.exe", "Resonance Stream (Packet Sniffing) [0c95ee3f]"),
    (DEV, "Resonance Stream (Packet Sniffing) [d34dd972]"),
    ("", "Resonance Stream (Packet Sniffing) [811c9dc5]"),
])
def test_rule_name_matches_the_rust_function(path, name):
    assert firewall.rule_name_for(path) == name


def test_case_and_slashes_do_not_change_the_name():
    assert firewall.rule_name_for(DEV.upper().replace("\\", "/")) == firewall.rule_name_for(DEV)


def test_parse_two_rules():
    rules = load("firewall_two_rules.json")
    assert [r.program for r in rules] == [DEV, RELEASE]
    assert rules[0].enabled and rules[0].action == "Allow"


def test_powershell_prints_one_rule_as_an_object_not_a_list():
    assert len(load("firewall_one_rule.json")) == 1


def test_no_output_is_no_rules():
    assert load("firewall_none.json") == []
    assert firewall.parse_rules("   \n") == []


def test_garbage_is_an_error_not_no_rules():
    with pytest.raises(ValueError):
        firewall.parse_rules("Get-NetFirewallRule : not recognized")


def test_two_exes_two_rules_all_pass():
    checks = firewall.check_rules(load("firewall_two_rules.json"), [DEV, RELEASE])
    assert all(ok for _, ok, _ in checks), checks


def test_the_legacy_shared_rule_is_a_failure():
    checks = firewall.check_rules(load("firewall_legacy_left.json"), [DEV, RELEASE])
    bad = [title for title, ok, _ in checks if not ok]
    assert any("old shared rule" in t for t in bad), checks


def test_an_exe_without_its_rule_is_a_failure():
    checks = firewall.check_rules(load("firewall_one_rule.json"), [DEV, RELEASE])
    bad = [title for title, ok, _ in checks if not ok]
    assert any(DEV in t for t in bad), checks


def test_the_rule_must_allow_inbound_for_that_exe_and_be_enabled():
    rules = load("firewall_two_rules.json")
    rules[0].enabled = False
    checks = firewall.check_rules(rules, [DEV, RELEASE])
    assert any(not ok for _, ok, _ in checks)


def test_a_rule_whose_name_hashes_another_path_is_a_failure():
    rules = load("firewall_two_rules.json")
    rules[0].program = r"C:\somewhere\else.exe"  # the name still says DEV's hash
    checks = firewall.check_rules(rules, [DEV, RELEASE])
    assert any(not ok for _, ok, _ in checks)


def test_the_powershell_command_filters_on_the_prefix():
    cmd = firewall.POWERSHELL_COMMAND
    assert "Resonance Stream (Packet Sniffing)*" in cmd and "ConvertTo-Json" in cmd
