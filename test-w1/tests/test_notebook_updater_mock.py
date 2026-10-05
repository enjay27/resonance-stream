"""Dry run of w1-updater-mock.ipynb against tests/fake_app.py (a stand-in for a test-env exe).

This proves the cells run, wait on the status file, read the folders and judge what they should.
It is not a Windows run and proves nothing about the real app."""
import json
import os
import stat
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from tools import dryrun  # noqa: E402
from w1 import mockfeed  # noqa: E402

HERE = Path(__file__).resolve().parent
NB = HERE.parent / "notebooks" / "w1-updater-mock.ipynb"
pytestmark = pytest.mark.skipif(sys.platform == "win32", reason="the stand-in app is a shell script")


@pytest.fixture(autouse=True)
def env(monkeypatch, tmp_path):
    from w1 import common
    monkeypatch.setattr(common, "RUNS", tmp_path)
    monkeypatch.setenv("W1_RUNS_DIR", str(tmp_path))
    exe = tmp_path / "local" / "resonance-stream.exe"
    exe.parent.mkdir()
    exe.write_text('#!/bin/sh\nFAKE_APP_EXE="$0" exec python3 "$FAKE_APP_PY" "$@"\n', encoding="utf-8")
    exe.chmod(exe.stat().st_mode | stat.S_IXUSR)
    monkeypatch.setenv("FAKE_APP_PY", str(HERE / "fake_app.py"))
    monkeypatch.setenv("FAKE_APP_SIGNED_FILE", str(tmp_path / "mock" / "new" / "update.exe"))
    monkeypatch.setenv("FAKE_APP_RESTART_AFTER", "4")
    monkeypatch.setenv("FAKE_APP_STALL_AFTER", "2")
    monkeypatch.setenv("FAKE_APP_LIFETIME", "90")
    monkeypatch.setenv("APPDATA", str(tmp_path / "appdata"))
    (tmp_path / "appdata" / "com.enjay.bpsr.resonance-stream").mkdir(parents=True)
    (tmp_path / "appdata" / "com.enjay.bpsr.resonance-stream" / "config.json").write_text('{"init_done": true}')
    monkeypatch.setenv("W1_TEXT_LOCAL_EXE", str(exe))
    monkeypatch.setenv("W1_TEXT_NEW_VERSION", "0.6.0")  # what the signing fixture says
    monkeypatch.setenv("W1_TEXT_BACKUP_KEY", r"C:\keys\backup.key")
    monkeypatch.setenv("W1_TEXT_BACKUP_PW", "not-a-real-password")
    yield


def rows(report: str) -> dict[str, str]:
    """check id -> 'status | evidence' for every table row of the report."""
    out = {}
    for line in report.splitlines():
        if line.startswith("| ") and not line.startswith("| check") and not line.startswith("|---"):
            cell, status, *evidence = [c.strip() for c in line.strip("|").split("|")]
            out[cell.split(" ")[0]] = f"{status} | {' '.join(evidence)}"
    return out


def running_copies(tmp_path):
    """Stand-in apps of this test that are still alive."""
    return [p for p in mockfeed.posix_processes() if str(tmp_path) in p["Path"]]


def test_every_check_passes_when_the_app_behaves(tmp_path):
    report = dryrun.execute(NB, "pass,pass")
    assert "fail 0" in report and "skip 0" in report, report
    got = rows(report)
    for check in ("M0", "M1-1", "M1-2", "M1-3", "M8", "M9", "M10", "M11", "M12", "M3-1", "M3-2", "M3-3", "M3-4",
                  "M4-1", "M4-2", "M4-3", "M4-4", "M5-2", "M5-3", "M6-2", "M6-3", "M7-2", "M7-3", "M-iso"):
        assert got[check].startswith("pass"), (check, got.get(check), report)
    assert running_copies(tmp_path) == [], "every check closes its app: nothing may be left running"


def test_a_bug_that_accepts_any_download_is_caught(monkeypatch):
    monkeypatch.setenv("FAKE_APP_SKIP_VERIFY", "1")
    report = dryrun.execute(NB, "pass,pass")
    got = rows(report)
    assert got["M5-2"].startswith("fail"), report  # the flipped file was accepted
    assert got["M-iso"].startswith("pass")


def test_an_app_that_touches_the_real_config_is_caught(monkeypatch):
    monkeypatch.setenv("FAKE_APP_TOUCH_REAL", "1")
    report = dryrun.execute(NB, "pass,pass")
    assert rows(report)["M-iso"].startswith("fail"), report


def test_an_exe_that_ignores_the_flags_stops_the_notebook(monkeypatch, tmp_path):
    ignorer = tmp_path / "ignorer.exe"
    ignorer.write_text("#!/bin/sh\nsleep 60\n", encoding="utf-8")
    ignorer.chmod(0o755)
    monkeypatch.setenv("W1_TEXT_LOCAL_EXE", str(ignorer))
    with pytest.raises(Exception) as e:
        dryrun.execute(NB, "pass,pass")
    assert "does not take the test flags" in str(e.value)


def test_a_check_that_raises_is_a_failed_row_the_notebook_goes_on_and_the_app_is_closed(monkeypatch, tmp_path):
    monkeypatch.setenv("FAKE_APP_BAD_STATUS", "1")  # begin() reads status["pid"]: KeyError inside the check
    report = dryrun.execute(NB, "pass,pass")
    got = rows(report)
    assert got["M8-error"].startswith("fail"), report
    assert "KeyError" in got["M8-error"]
    assert "M-iso" in got and got["M-iso"].startswith("pass"), "the notebook went on to the end"
    assert running_copies(tmp_path) == []


def test_every_cell_that_starts_a_copy_is_guarded():
    import nbformat

    nb = nbformat.read(str(NB), as_version=4)
    starting = [c.source for c in nb.cells if c.cell_type == "code" and "begin(" in c.source and "def begin" not in c.source]
    assert len(starting) >= 3
    for source in starting:
        assert "guard(" in source, source[:100]
