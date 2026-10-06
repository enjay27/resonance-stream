"""Dry run of the `translator-stub` pipeline against tests/fake_app.py, a stand-in app that models the translator worker as understood
(start, health wait, jobs, three failures restart, catch-up) and talks to the real stand-in server (`runbook/llama_stub.py`)."""
import stat
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from runbook import bridge, common, mockfeed  # noqa: E402
from runbook.common import Recorder  # noqa: E402
from runbook.pipelines import translator_stub  # noqa: E402
from runbook.pipelines.translator_stub import TranslatorStub  # noqa: E402

HERE = Path(__file__).resolve().parent
pytestmark = [
    pytest.mark.skipif(sys.platform == "win32", reason="the stand-in app is a shell script"),
    pytest.mark.skipif(bridge.node_path() is None or not (bridge.BRIDGE_DIR / "node_modules" / "aedes").is_dir(),
                       reason="needs node and `npm ci` in runbook/bridge"),
]


@pytest.fixture(autouse=True)
def env(monkeypatch, tmp_path):
    monkeypatch.setattr(common, "RUNS", tmp_path)
    monkeypatch.setattr(translator_stub, "LOADING_S", 1.0)
    monkeypatch.setenv("RUNBOOK_DRYRUN", "1")
    exe = tmp_path / "local" / "resonance-stream.exe"
    exe.parent.mkdir()
    exe.write_text('#!/bin/sh\nFAKE_APP_EXE="$0" exec python3 "$FAKE_APP_PY" "$@"\n', encoding="utf-8")
    exe.chmod(exe.stat().st_mode | stat.S_IXUSR)
    monkeypatch.setenv("FAKE_APP_PY", str(HERE / "fake_app.py"))
    monkeypatch.setenv("FAKE_APP_LIFETIME", "180")
    monkeypatch.setenv("FAKE_APP_LEAD_IN_MS", "50")
    yield exe


def run(exe, tmp_path):
    rec = Recorder("translator-stub")
    TranslatorStub(rec, exe, tmp_path).run()
    assert [p for p in mockfeed.posix_processes() if str(tmp_path) in p["Path"]] == [], "no app may be left running"
    return rec, {r.check: r for r in rec.rows}


CHECKS = ("TS-ready", "TS-translate", "TS-prompt", "TS-restart", "TS-catchup")


def test_a_translator_that_follows_the_rules_passes_every_row(env, tmp_path):
    rec, got = run(env, tmp_path)
    assert rec.summary()["fail"] == 0, rec.report()
    for check in CHECKS:
        assert got[check].status == "pass", rec.report()
    assert "literal <bos> in the prompt text: 1" in got["TS-prompt"].evidence


@pytest.mark.parametrize("bug, failing", [
    ("no-catchup", {"TS-catchup"}),
    ("no-restart", {"TS-restart"}),
    ("translates-english", {"TS-translate"}),
])
def test_each_broken_rule_is_caught(env, tmp_path, monkeypatch, bug, failing):
    monkeypatch.setenv("FAKE_APP_TR_BUG", bug)
    rec, got = run(env, tmp_path)
    caught = {c for c in CHECKS if c in got and got[c].status == "fail"}
    assert failing <= caught, f"{bug}: wanted {failing} among the failures, got {caught}\n{rec.report()}"
