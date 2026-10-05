import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from runbook import replay  # noqa: E402

SAMPLE = Path(__file__).resolve().parents[2] / "crates" / "core" / "testdata" / "replay-sample.jsonl"


def test_the_shipped_sample_has_eight_lines_over_about_ten_seconds():
    entries = replay.parse_sample(SAMPLE.read_text(encoding="utf-8"))
    assert len(entries) == 8
    assert 10 < replay.total_seconds(entries) < 15
    assert set(replay.by_channel(entries)) == {"WORLD", "GUILD", "PARTY", "LOCAL", "BEGINNER"}


def test_a_line_without_text_is_named():
    with pytest.raises(ValueError, match="line 2"):
        replay.parse_sample('{"text": "a"}\n{"channel": "WORLD"}')


LOG_OK = """[2026-10-05T08:00:00Z INFO] [Replay] Replaying 8 chat lines from C:\\x.jsonl
[2026-10-05T08:00:12Z INFO] [Replay] Replay finished
"""


def test_a_good_log():
    r = replay.log_report(LOG_OK, 8)
    assert r["count_ok"] and r["finished"] and not r["problems"]


def test_a_log_that_never_finished_or_complained():
    r = replay.log_report("[t ERROR] [Replay] line 3: unknown field\n", 8)
    assert r["started"] is None and not r["finished"] and len(r["problems"]) == 1
