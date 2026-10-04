import os
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from w1 import common  # noqa: E402


@pytest.mark.parametrize("text,expected", [
    ("pass", ("pass", "")), ("p", ("pass", "")), ("Y", ("pass", "")), ("ok fine", ("pass", "fine")),
    ("fail: bar stuck at 0%", ("fail", "bar stuck at 0%")), ("f", ("fail", "")), ("no", ("fail", "")),
    ("skip", ("skip", "")), ("s: no VPN here", ("skip", "no VPN here")),
    ("", ("skip", "")), ("   ", ("skip", "")),
    ("pasd", ("skip", "pasd")),  # a typo is never a pass
])
def test_parse_answer(text, expected):
    assert common.parse_answer(text) == expected


def test_a_note_keeps_everything_after_the_first_separator():
    assert common.parse_answer("fail: a: b") == ("fail", "a: b")


def test_the_recorder_refuses_a_made_up_status():
    with pytest.raises(ValueError):
        common.Recorder("j").record("1", "t", "maybe")


def test_auto_is_pass_or_fail_only():
    rec = common.Recorder("j")
    assert rec.auto("1", "a", True).status == "pass"
    assert rec.auto("2", "b", False, "why").status == "fail"
    assert rec.summary() == {"pass": 1, "fail": 1, "skip": 0}


def test_manual_reads_answers_from_the_queue(monkeypatch, capsys):
    monkeypatch.setenv("W1_ANSWERS", "pass,fail: broke")
    rec = common.Recorder("j")
    assert rec.manual("A", "first", "do x", "y").status == "pass"
    second = rec.manual("B", "second", "do z", "w")
    assert (second.status, second.evidence) == ("fail", "broke")
    assert os.environ["W1_ANSWERS"] == ""


def test_an_empty_queue_is_an_error_not_a_silent_skip(monkeypatch):
    monkeypatch.setenv("W1_ANSWERS", "")
    with pytest.raises(RuntimeError):
        common.Recorder("j").manual("A", "t", "s", "e")


def test_the_report_lists_every_row_and_escapes_pipes(monkeypatch):
    monkeypatch.setenv("W1_DRYRUN", "1")
    rec = common.Recorder("w1-x")
    rec.record("1", "one", "pass", "a|b")
    rec.record("2", "two", "skip")
    text = rec.report()
    assert "W1 report: w1-x" in text
    assert "pass 1 / fail 0 / skip 1" in text
    assert "a\\|b" in text
    assert "| 2 two | skip |  |" in text


def test_a_dry_run_report_saves_nothing(monkeypatch, tmp_path):
    monkeypatch.setenv("W1_DRYRUN", "1")
    monkeypatch.setattr(common, "RUNS", tmp_path)
    common.Recorder("j").report()
    assert list(tmp_path.iterdir()) == []


def test_capture_returns_the_fixture_in_a_dry_run(monkeypatch):
    monkeypatch.setenv("W1_DRYRUN", "1")
    code, text = common.capture(["netsh"], fixture="hello.txt")
    assert (code, text.strip()) == (0, "hello")


def test_capture_without_a_fixture_in_a_dry_run_is_an_error(monkeypatch):
    monkeypatch.setenv("W1_DRYRUN", "1")
    with pytest.raises(RuntimeError):
        common.capture(["netsh"])


def test_capture_runs_a_real_command_outside_a_dry_run(monkeypatch):
    monkeypatch.delenv("W1_DRYRUN", raising=False)
    code, text = common.capture([sys.executable, "-c", "print('hi')"])
    assert (code, text.strip()) == (0, "hi")


def test_a_missing_program_is_code_127(monkeypatch):
    monkeypatch.delenv("W1_DRYRUN", raising=False)
    assert common.capture(["definitely-not-a-program-xyz"])[0] == 127
