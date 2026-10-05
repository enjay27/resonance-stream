"""Helpers for the `replay-chat` notebook: what `--replay-chat <file>` should do with a sample file,
and what its `--log-file` says about it. Pure text work, so it runs on any OS."""
from __future__ import annotations

import json
import re
from pathlib import Path

LEAD_IN_S = 2.0  # src-tauri/src/services/sniffer/replay.rs: wait before the first line


def parse_sample(text: str) -> list[dict]:
    """The chat lines of a replay file (JSON Lines; blank lines and `#` lines are skipped)."""
    lines = []
    for number, raw in enumerate(text.splitlines(), 1):
        raw = raw.strip()
        if not raw or raw.startswith("#"):
            continue
        try:
            entry = json.loads(raw)
        except json.JSONDecodeError as e:
            raise ValueError(f"line {number}: not JSON ({e.msg})") from e
        if not isinstance(entry, dict) or "text" not in entry:
            raise ValueError(f"line {number}: no \"text\"")
        lines.append(entry)
    return lines


def total_seconds(entries: list[dict]) -> float:
    """How long the replay should take: the lead-in plus every `delay_ms`."""
    return LEAD_IN_S + sum(e.get("delay_ms", 0) for e in entries) / 1000


def by_channel(entries: list[dict]) -> dict[str, list[str]]:
    out: dict[str, list[str]] = {}
    for e in entries:
        out.setdefault(e.get("channel", "WORLD"), []).append(e["text"])
    return out


def log_report(log_text: str, expected_lines: int) -> dict:
    """What the app's log says: did the replay start (with how many lines), finish, and complain."""
    replay = [ln for ln in log_text.splitlines() if "[Replay]" in ln]
    started = None
    for ln in replay:
        m = re.search(r"Replaying (\d+) chat lines", ln)
        if m:
            started = int(m.group(1))
    return {
        "started": started,
        "count_ok": started == expected_lines,
        "finished": any("Replay finished" in ln for ln in replay),
        "problems": [ln for ln in replay if re.search(r"\b(ERROR|WARN)\b", ln)],
        "replay_lines": replay,
    }
