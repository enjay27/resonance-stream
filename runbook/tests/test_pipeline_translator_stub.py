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
    monkeypatch.setattr(translator_stub, "SLOW_LOADING_S", 3.0)
    monkeypatch.setattr(translator_stub, "SLOW_DELAY_S", 0.6)
    monkeypatch.setattr(translator_stub, "WAIT_S", 8.0)
    monkeypatch.setattr(translator_stub, "HANG_MARGIN_S", 4.0)
    monkeypatch.setattr(translator_stub, "REQUEST_TIMEOUT_S", 1.5)  # the app waits 30 s for a reply, its stand-in here 1 s
    monkeypatch.setenv("FAKE_APP_REQUEST_TIMEOUT", "1")
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


CHECKS = ("TS-ready", "TS-log-quiet", "TS-translate", "TS-prompt", "TS-restart", "TS-catchup", "TS-reload",
          "TS-later-wait", "TS-later-catchup", "TS-live-first", "TS-hang",
          "TS-dict-before", "TS-dict-sync", "TS-dict-local", "TS-dict-bad", "TS-dict-auto-off", "TS-dict-auto-on")


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
    ("no-limit", {"TS-later-catchup"}),
    ("live-starves", {"TS-live-first"}),
    ("poll-spam", {"TS-log-quiet"}),
    ("hang-stalls", {"TS-hang"}),
    ("reload-twice", {"TS-reload"}),
])
def test_each_broken_rule_is_caught(env, tmp_path, monkeypatch, bug, failing):
    monkeypatch.setenv("FAKE_APP_TR_BUG", bug)
    rec, got = run(env, tmp_path)
    caught = {c for c in CHECKS if c in got and got[c].status == "fail"}
    assert failing <= caught, f"{bug}: wanted {failing} among the failures, got {caught}\n{rec.report()}"


@pytest.mark.parametrize("bug, failing", [
    ("sync-not-installed", {"TS-dict-sync"}),
    ("local-needs-restart", {"TS-dict-local"}),
    ("bad-saved", {"TS-dict-bad"}),
    ("auto-always", {"TS-dict-auto-off"}),
])
def test_each_broken_dictionary_rule_is_caught(env, tmp_path, monkeypatch, bug, failing):
    monkeypatch.setenv("FAKE_APP_DICT_BUG", bug)
    rec, got = run(env, tmp_path)
    caught = {c for c in CHECKS if c in got and got[c].status == "fail"}
    assert failing <= caught, f"{bug}: wanted {failing} among the failures, got {caught}\n{rec.report()}"
