"""Dry run of the `updater-mock` pipeline against tests/fake_app.py (a stand-in for a test-env exe that speaks to the
bridge's MQTT broker). It proves the steps start the app, drive it with commands, wait on its events and judge what they
should. It is not a Windows run and proves nothing about the real app."""
import os
import stat
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from runbook import bridge, common, mockfeed  # noqa: E402
from runbook.common import Recorder  # noqa: E402
from runbook.pipelines.updater_mock import UpdaterMock  # noqa: E402

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
    monkeypatch.setenv("FAKE_APP_SIGNED_FILE", str(tmp_path / "mock" / "new" / "update.exe"))
    monkeypatch.setenv("FAKE_APP_STALL_AFTER", "2")
    monkeypatch.setenv("FAKE_APP_LIFETIME", "120")
    monkeypatch.setenv("APPDATA", str(tmp_path / "appdata"))
    config = tmp_path / "appdata" / "com.enjay.bpsr.resonance-stream"
    config.mkdir(parents=True)
    (config / "config.json").write_text('{"init_done": true}')
    yield exe


def run_pipeline(exe, tmp_path):
    rec = Recorder("updater-mock")
    UpdaterMock(rec, exe, tmp_path, new_version="0.6.0").run(key_path=r"C:\keys\backup.key", password="not-a-real-password")
    return rec, {row.check: row for row in rec.rows}


def running_copies(tmp_path):
    return [p for p in mockfeed.posix_processes() if str(tmp_path) in p["Path"]]


def test_every_check_passes_when_the_app_behaves(env, tmp_path):
    rec, got = run_pipeline(env, tmp_path)
    assert rec.summary()["fail"] == 0, rec.report()
    for check in ("M0", "M1-1", "M1-2", "M1-3", "M8", "M9", "M10", "M11", "M12", "M3-1", "M3-2", "M3-3", "M3-4",
                  "M4-1", "M4-2", "M4-3", "M4-refused", "M4-4", "M5-2", "M5-3", "M6-2", "M6-3", "M7-2", "M7-3", "M-iso"):
        assert got[check].status == "pass", (check, got.get(check), rec.report())
    assert got["M3-ui"].status == "skip", "what the dialog looked like is not automated"
    assert running_copies(tmp_path) == [], "every step closes its app: nothing may be left running"


def test_the_signature_is_kept_so_the_password_is_asked_once_per_exe(env, tmp_path):
    run_pipeline(env, tmp_path)
    rec = Recorder("again")
    # no key, no password: the kept signature is used
    pipeline = UpdaterMock(rec, env, tmp_path, new_version="0.6.0")
    assert pipeline.smoke() and pipeline.prepare_release(None, None)
    assert "kept from an earlier run" in {r.check: r for r in rec.rows}["M1-1"].evidence


def test_a_bug_that_accepts_any_download_is_caught(env, tmp_path, monkeypatch):
    monkeypatch.setenv("FAKE_APP_SKIP_VERIFY", "1")
    rec, got = run_pipeline(env, tmp_path)
    assert got["M5-2"].status == "fail", rec.report()  # the flipped file was accepted
    assert got["M4-refused"].status == "fail", rec.report()  # ...and so was the tampered one
    assert got["M-iso"].status == "pass"


def test_an_app_that_touches_the_real_config_is_caught(env, tmp_path, monkeypatch):
    monkeypatch.setenv("FAKE_APP_TOUCH_REAL", "1")
    rec, got = run_pipeline(env, tmp_path)
    assert got["M-iso"].status == "fail", rec.report()


def test_an_exe_that_ignores_the_flags_stops_the_pipeline(env, tmp_path):
    ignorer = tmp_path / "ignorer.exe"
    ignorer.write_text("#!/bin/sh\nsleep 60\n", encoding="utf-8")
    ignorer.chmod(0o755)
    rec, got = run_pipeline(ignorer, tmp_path)
    assert got["M0"].status == "fail" and "M1-1" not in got, rec.report()


def test_a_step_that_raises_is_a_failed_row_the_pipeline_goes_on_and_the_app_is_closed(env, tmp_path, monkeypatch):
    original = UpdaterMock.begin

    def broken(self, label, server):
        if label == "M8-app":
            raise KeyError("pid")
        return original(self, label, server)

    monkeypatch.setattr(UpdaterMock, "begin", broken)
    rec, got = run_pipeline(env, tmp_path)
    assert got["M8-error"].status == "fail" and "KeyError" in got["M8-error"].evidence, rec.report()
    assert got["M-iso"].status == "pass", "the pipeline went on to the end"
    assert running_copies(tmp_path) == []


def test_the_notebook_is_a_thin_wrapper_around_the_pipeline():
    import ast

    import nbformat

    nb = nbformat.read(str(HERE.parent / "notebooks" / "updater-mock.ipynb"), as_version=4)
    code = [c.source for c in nb.cells if c.cell_type == "code"]
    for source in code:
        ast.parse(source)
    assert any("UpdaterMock(" in s and ".run(" in s for s in code)
    assert not any("input(" in s or "rec.manual" in s for s in code), "no prompt of its own"
