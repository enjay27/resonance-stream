"""Dry run of the `download-integrity` pipeline against tests/fake_app.py, a stand-in that follows the model download's rules as the backend
is understood to. It proves the pipeline drives the command, reads the disk and judges the outcomes; the real app's answer is the real run's."""
import stat
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from runbook import bridge, common, mockfeed  # noqa: E402
from runbook.common import Recorder  # noqa: E402
from runbook.pipelines.download_integrity import DownloadIntegrity  # noqa: E402

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
    rec = Recorder("download-integrity")
    DownloadIntegrity(rec, exe, tmp_path).run()
    assert [p for p in mockfeed.posix_processes() if str(tmp_path) in p["Path"]] == [], "no app may be left running"
    return rec, {r.check: r for r in rec.rows}


CHECKS = ("DI-good", "DI-skip", "DI-hash", "DI-cut", "DI-404", "DI-nohash", "DI-https")


def test_a_download_that_follows_the_rules_passes_every_row(env, tmp_path):
    rec, got = run(env, tmp_path)
    assert rec.summary()["fail"] == 0, rec.report()
    for check in CHECKS:
        assert got[check].status == "pass", rec.report()


@pytest.mark.parametrize("bug, failing", [
    ("no-verify", {"DI-hash"}),
    ("keeps-part", {"DI-hash", "DI-cut"}),
    ("overwrites-on-fail", {"DI-hash"}),
    ("accepts-http", {"DI-https"}),
])
def test_each_broken_rule_is_caught(env, tmp_path, monkeypatch, bug, failing):
    monkeypatch.setenv("FAKE_APP_DL_BUG", bug)
    rec, got = run(env, tmp_path)
    caught = {c for c in CHECKS if c in got and got[c].status == "fail"}
    assert failing <= caught, f"{bug}: wanted {failing} among the failures, got {caught}\n{rec.report()}"
