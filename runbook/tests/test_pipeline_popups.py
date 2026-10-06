"""Dry run of the `popups` pipeline against tests/fake_app.py, a stand-in that models the two popup windows as understood (made hidden after
start, the X hides, the place is remembered, they follow the pin, closing the main window ends the app). It proves the commands and the judging;
real windows are the real run's."""
import stat
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from runbook import bridge, common, mockfeed  # noqa: E402
from runbook.common import Recorder  # noqa: E402
from runbook.pipelines.popups import Popups  # noqa: E402

HERE = Path(__file__).resolve().parent
pytestmark = [
    pytest.mark.skipif(sys.platform == "win32", reason="the stand-in app is a shell script"),
    pytest.mark.skipif(bridge.node_path() is None or not (bridge.BRIDGE_DIR / "node_modules" / "aedes").is_dir(),
                       reason="needs node and `npm ci` in runbook/bridge"),
]


@pytest.fixture(autouse=True)
def env(monkeypatch, tmp_path):
    monkeypatch.setattr(common, "RUNS", tmp_path)
    monkeypatch.setenv("RUNBOOK_DRYRUN", "1")
    exe = tmp_path / "local" / "resonance-stream.exe"
    exe.parent.mkdir()
    exe.write_text('#!/bin/sh\nFAKE_APP_EXE="$0" exec python3 "$FAKE_APP_PY" "$@"\n', encoding="utf-8")
    exe.chmod(exe.stat().st_mode | stat.S_IXUSR)
    monkeypatch.setenv("FAKE_APP_PY", str(HERE / "fake_app.py"))
    monkeypatch.setenv("FAKE_APP_LIFETIME", "120")
    yield exe


def run(exe, tmp_path):
    rec = Recorder("popups")
    Popups(rec, exe, tmp_path).run()
    assert [p for p in mockfeed.posix_processes() if str(tmp_path) in p["Path"]] == [], "no app may be left running"
    return rec, {r.check: r for r in rec.rows}


CHECKS = ("PP-prewarm", "PP-open", "PP-hide", "PP-pin", "PP-exit", "PP-place")


def test_popup_windows_that_follow_the_rules_pass_every_row(env, tmp_path):
    rec, got = run(env, tmp_path)
    assert rec.summary()["fail"] == 0, rec.report()
    for check in CHECKS:
        assert got[check].status == "pass", rec.report()


@pytest.mark.parametrize("bug, failing", [
    ("second-window", {"PP-open"}),
    ("no-restore", {"PP-place"}),
    ("pin-ignored", {"PP-pin"}),
])
def test_each_broken_rule_is_caught(env, tmp_path, monkeypatch, bug, failing):
    monkeypatch.setenv("FAKE_APP_POPUP_BUG", bug)
    rec, got = run(env, tmp_path)
    caught = {c for c in CHECKS if c in got and got[c].status == "fail"}
    assert failing <= caught, f"{bug}: wanted {failing} among the failures, got {caught}\n{rec.report()}"
