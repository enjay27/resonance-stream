"""Dry run of the `interface` pipeline against tests/fake_app.py (a stand-in that says what the sniffer would say) and the
recorded `route print` / adapter fixtures. It proves the pipeline starts the app, reads the line from the bridge and judges it;
it proves nothing about Windows, the VPN or the real sniffer."""
import ast
import stat
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from runbook import bridge, common, mockfeed  # noqa: E402
from runbook.common import Recorder  # noqa: E402
from runbook.pipelines.interface import InterfacePick  # noqa: E402

HERE = Path(__file__).resolve().parent
pytestmark = [
    pytest.mark.skipif(sys.platform == "win32", reason="the stand-in app is a shell script"),
    pytest.mark.skipif(bridge.node_path() is None or not (bridge.BRIDGE_DIR / "node_modules" / "aedes").is_dir(),
                       reason="needs node and `npm ci` in runbook/bridge"),
]
ROUTE = "Auto-Targeting Network Interface: 192.168.0.23 (default route)"
LIST = "Auto-Targeting Network Interface: 192.168.0.23 (first physical adapter)"
NONE = "NETWORK_ERROR: Could not find a valid local IPv4 network interface."


@pytest.fixture(autouse=True)
def env(monkeypatch, tmp_path):
    monkeypatch.setattr(common, "RUNS", tmp_path)
    monkeypatch.setenv("RUNBOOK_DRYRUN", "1")
    exe = tmp_path / "local" / "resonance-stream.exe"
    exe.parent.mkdir()
    exe.write_text('#!/bin/sh\nFAKE_APP_EXE="$0" exec python3 "$FAKE_APP_PY" "$@"\n', encoding="utf-8")
    exe.chmod(exe.stat().st_mode | stat.S_IXUSR)
    monkeypatch.setenv("FAKE_APP_PY", str(HERE / "fake_app.py"))
    monkeypatch.setenv("FAKE_APP_LIFETIME", "90")
    yield exe


def run(exe, tmp_path, situation, line, monkeypatch):
    if line is not None:
        monkeypatch.setenv("FAKE_APP_IFACE_LINE", line)
    rec = Recorder("interface")
    InterfacePick(rec, exe, tmp_path, dry_situation=situation).run()
    assert mockfeed.posix_processes() == [] or all(str(tmp_path) not in p["Path"] for p in mockfeed.posix_processes())
    return rec, rec.report()


def statuses(rec, check):
    return [r.status for r in rec.rows if r.check == check]


def test_two_adapters_and_the_default_route_rule_pass(env, tmp_path, monkeypatch):
    rec, report = run(env, tmp_path, "two", ROUTE, monkeypatch)
    assert rec.summary()["fail"] == 0, report
    assert statuses(rec, "K4-1").count("pass") == 4 and "skip" in statuses(rec, "K4-1")  # 4 checks, "chat captured" skipped
    assert statuses(rec, "K4-2") == ["skip"] and statuses(rec, "K4-3") == ["skip"]


def test_two_adapters_with_the_list_rule_fails(env, tmp_path, monkeypatch):
    rec, report = run(env, tmp_path, "two", LIST, monkeypatch)
    assert rec.summary()["fail"] >= 1, report


def test_a_vpn_default_route_passes_when_the_app_takes_the_physical_adapter(env, tmp_path, monkeypatch):
    rec, report = run(env, tmp_path, "vpn", LIST, monkeypatch)
    assert rec.summary()["fail"] == 0, report
    assert statuses(rec, "K4-2").count("pass") >= 3 and statuses(rec, "K4-1") == ["skip"]


def test_the_app_picking_the_vpn_adapter_is_reported(env, tmp_path, monkeypatch):
    rec, report = run(env, tmp_path, "vpn", "Auto-Targeting Network Interface: 10.8.0.2 (default route)", monkeypatch)
    assert rec.summary()["fail"] >= 2, report  # the rule and the address


def test_offline_the_app_stays_up_and_says_there_is_no_interface(env, tmp_path, monkeypatch):
    rec, report = run(env, tmp_path, "offline", NONE, monkeypatch)
    assert rec.summary()["fail"] == 0, report
    assert statuses(rec, "K4-3").count("pass") == 2


def test_an_app_that_never_says_which_adapter_is_a_failed_row(env, tmp_path, monkeypatch, request):
    monkeypatch.setattr(InterfacePick, "sniffer_line", staticmethod(lambda events: None))  # the 60 s wait, skipped
    rec, report = run(env, tmp_path, "two", None, monkeypatch)
    assert "K4-line" in {r.check for r in rec.rows if r.status == "fail"}, report


def test_one_adapter_is_nothing_to_conclude(env, tmp_path, monkeypatch):
    rec, report = run(env, tmp_path, "single", ROUTE, monkeypatch)
    assert rec.summary()["fail"] == 0 and statuses(rec, "K4-1") == ["skip"], report


def test_the_notebook_is_a_thin_wrapper_around_the_pipeline():
    import nbformat

    nb = nbformat.read(str(HERE.parent / "notebooks" / "interface.ipynb"), as_version=4)
    code = [c.source for c in nb.cells if c.cell_type == "code"]
    for source in code:
        ast.parse(source)
    assert any("InterfacePick(" in s and ".run(" in s for s in code)
    assert not any("input(" in s or "rec.manual" in s or "ask_text" in s for s in code), "nothing to paste or answer"
    assert all(not c.get("outputs") for c in nb.cells if c.cell_type == "code"), "committed without outputs"
