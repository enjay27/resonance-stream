"""Dry run of the `window-restore` pipeline (K18) against tests/fake_app.py, a stand-in that grows, saves and restores a
window rect the way window.rs and the window-state plugin are understood to. It proves the steps drive the commands and judge the
sizes; it proves nothing about a real window or the real plugin."""
import ast
import stat
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from runbook import bridge, common, mockfeed  # noqa: E402
from runbook.common import Recorder  # noqa: E402
from runbook.pipelines.window_restore import WindowRestore  # noqa: E402

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
    monkeypatch.setenv("FAKE_APP_LIFETIME", "90")
    yield exe


def run(exe, tmp_path):
    rec = Recorder("window-restore")
    WindowRestore(rec, exe, tmp_path).run()
    assert [p for p in mockfeed.posix_processes() if str(tmp_path) in p["Path"]] == [], "no app may be left running"
    return rec, {r.check: r for r in rec.rows}


def test_both_ways_out_bring_the_old_size_back(env, tmp_path):
    rec, got = run(env, tmp_path)
    assert rec.summary()["fail"] == 0, rec.report()
    for check in ("K18-quit", "K18-close"):
        assert got[check].status == "pass", rec.report()
        assert "800x600" in got[check].evidence and "1200x900" in got[check].evidence


def test_a_quit_that_saves_the_grown_window_is_caught(env, tmp_path, monkeypatch):
    monkeypatch.setenv("FAKE_APP_K18_BUG", "quit")  # the tray's path forgets to restore
    rec, got = run(env, tmp_path)
    assert got["K18-quit"].status == "fail" and "1200x900" in got["K18-quit"].evidence, rec.report()
    assert got["K18-close"].status == "pass", rec.report()


def test_a_close_that_saves_the_grown_window_is_caught(env, tmp_path, monkeypatch):
    monkeypatch.setenv("FAKE_APP_K18_BUG", "close")
    rec, got = run(env, tmp_path)
    assert got["K18-close"].status == "fail" and got["K18-quit"].status == "pass", rec.report()


def test_a_window_that_is_already_as_big_as_the_screen_is_a_skip(env, tmp_path, monkeypatch):
    monkeypatch.setenv("FAKE_APP_RECT", "6000x4000")
    rec, got = run(env, tmp_path)
    assert got["K18-quit-grow"].status == "skip" and rec.summary()["fail"] == 0, rec.report()


def test_the_notebook_is_a_thin_wrapper_around_the_pipeline():
    import nbformat

    path = HERE.parent / "notebooks" / "window-restore.ipynb"
    nb = nbformat.read(str(path), as_version=4)
    code = [c.source for c in nb.cells if c.cell_type == "code"]
    for source in code:
        ast.parse(source)
    assert any("WindowRestore(" in s and ".run(" in s for s in code)
    assert not any("input(" in s or "rec.manual" in s or "ask_text" in s for s in code)
    assert all(not c.get("outputs") for c in nb.cells if c.cell_type == "code")
