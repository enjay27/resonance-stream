"""Dry run of the `persistence` pipeline against tests/fake_app.py, a stand-in that archives chat, saves the block list and reloads both
the way the backend is understood to. It proves the steps and the judgements; whether the real app keeps its promises is the real run."""
import stat
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from runbook import bridge, common, mockfeed  # noqa: E402
from runbook.common import Recorder  # noqa: E402
from runbook.pipelines import persistence  # noqa: E402
from runbook.pipelines.persistence import Persistence  # noqa: E402

HERE = Path(__file__).resolve().parent
pytestmark = [
    pytest.mark.skipif(sys.platform == "win32", reason="the stand-in app is a shell script"),
    pytest.mark.skipif(bridge.node_path() is None or not (bridge.BRIDGE_DIR / "node_modules" / "aedes").is_dir(),
                       reason="needs node and `npm ci` in runbook/bridge"),
]


@pytest.fixture(autouse=True)
def env(monkeypatch, tmp_path):
    monkeypatch.setattr(common, "RUNS", tmp_path)
    monkeypatch.setattr(persistence, "GRACE_S", 0.3)
    monkeypatch.setenv("RUNBOOK_DRYRUN", "1")
    exe = tmp_path / "local" / "resonance-stream.exe"
    exe.parent.mkdir()
    exe.write_text('#!/bin/sh\nFAKE_APP_EXE="$0" exec python3 "$FAKE_APP_PY" "$@"\n', encoding="utf-8")
    exe.chmod(exe.stat().st_mode | stat.S_IXUSR)
    monkeypatch.setenv("FAKE_APP_PY", str(HERE / "fake_app.py"))
    monkeypatch.setenv("FAKE_APP_LIFETIME", "90")
    monkeypatch.setenv("FAKE_APP_VERSION", persistence.workspace_version() or "0.0.0")
    monkeypatch.setenv("FAKE_APP_LEAD_IN_MS", "50")
    yield exe


def run(exe, tmp_path):
    rec = Recorder("persistence")
    Persistence(rec, exe, tmp_path).run()
    assert [p for p in mockfeed.posix_processes() if str(tmp_path) in p["Path"]] == [], "no app may be left running"
    return rec, {r.check: r for r in rec.rows}


CHECKS = ("CP-archive", "CP-config", "CP-reload", "CP-block-reload", "CP-pid", "CP-version",
          "CP-retention", "CP-retention-served", "CP-busy-world",
          "CP-fav-save", "CP-fav-event", "CP-fav-block", "CP-fav-config", "CP-fav-reload")


def test_an_app_that_keeps_its_promises_passes_every_row(env, tmp_path):
    rec, got = run(env, tmp_path)
    assert rec.summary()["fail"] == 0, rec.report()
    for check in CHECKS:
        assert got[check].status == "pass", rec.report()


@pytest.mark.parametrize("bug, failing", [
    ("no-archive", {"CP-archive", "CP-reload"}),
    ("world-archived", {"CP-archive"}),
    ("no-config", {"CP-config", "CP-pid"}),
    ("no-reload", {"CP-reload"}),
    ("pid-restart", {"CP-pid"}),
    ("unflagged-reload", {"CP-block-reload"}),
    ("no-retention", {"CP-retention", "CP-retention-served"}),
    ("retention-takes-all", {"CP-retention"}),
    ("global-limit", {"CP-busy-world"}),
    ("fav-not-saved", {"CP-fav-config", "CP-fav-reload"}),
    ("fav-renumbered", {"CP-fav-save", "CP-fav-reload"}),
    ("fav-clobbered", {"CP-fav-block", "CP-fav-config", "CP-fav-reload"}),
    ("fav-silent", {"CP-fav-event"}),
])
def test_each_broken_promise_is_caught(env, tmp_path, monkeypatch, bug, failing):
    monkeypatch.setenv("FAKE_APP_PERSIST_BUG", bug)
    rec, got = run(env, tmp_path)
    caught = {c for c in CHECKS if c in got and got[c].status == "fail"}
    assert failing <= caught, f"{bug}: wanted {failing} among the failures, got {caught}\n{rec.report()}"


def test_a_stale_exe_is_told_by_its_version(env, tmp_path, monkeypatch):
    monkeypatch.setenv("FAKE_APP_VERSION", "0.0.1")
    rec, got = run(env, tmp_path)
    assert got["CP-version"].status == "fail", rec.report()
    assert persistence.workspace_version() in got["CP-version"].evidence, got["CP-version"].evidence
