"""K3: run the release-feed check on GitHub, and read the feed the way the app does."""
from __future__ import annotations

import json
import re
from datetime import datetime, timedelta, timezone
from typing import Callable


def _parse_time(stamp: str) -> datetime:
    return datetime.fromisoformat(stamp.replace("Z", "+00:00"))


def pick_dispatched_run(runs_json: str, since: str) -> dict | None:
    """The newest `workflow_dispatch` run created at or after `since` (minus 10 s of clock skew).

    Scheduled runs and older runs are somebody else's; a run list that is not JSON is an error
    (gh printed a login problem), not 'no runs'.
    """
    runs_json = re.sub(r"\x1b\[[0-9;]*m", "", runs_json).strip()  # colour codes gh may still print
    if not runs_json:
        return None
    try:
        runs = json.loads(runs_json)
    except json.JSONDecodeError as e:
        raise ValueError(f"not the JSON `gh run list` prints: {runs_json[:120]!r}") from e
    floor = _parse_time(since) - timedelta(seconds=10)
    mine = [r for r in runs if r.get("event") == "workflow_dispatch" and _parse_time(r["createdAt"]) >= floor]
    return max(mine, key=lambda r: r["createdAt"]) if mine else None


def run_state(run: dict) -> tuple[str, str]:
    """('running' | 'pass' | 'fail', the run's URL)."""
    url = run.get("url", "")
    if run.get("status") != "completed":
        return "running", url
    return ("pass" if run.get("conclusion") == "success" else "fail"), url


class FakeClock:
    """A clock for tests: sleeping just advances it."""

    def __init__(self) -> None:
        self.t = 0.0
        self.slept: list[float] = []

    def now(self) -> float:
        return self.t

    def sleep(self, seconds: float) -> None:
        self.slept.append(seconds)
        self.t += seconds


def poll(fetch: Callable[[], tuple[str, str] | None], sleep: Callable[[float], None],
         now: Callable[[], float], timeout: float, interval: float) -> tuple[str, str]:
    """Call `fetch` until it says pass or fail. None (the run is not listed yet) counts as running."""
    start = now()
    while True:
        state = fetch()
        if state is not None and state[0] != "running":
            return state
        if now() - start + interval > timeout:
            return "fail", f"still running after {timeout:g} s"
        sleep(interval)


def check_feed_json(text: str, slug: str, expected_version: str | None = None) -> list[tuple[str, bool, str]]:
    """What the app needs from latest.json: version, an exe URL on this repo's releases, a signature, notes."""
    try:
        data = json.loads(text)
    except json.JSONDecodeError:
        return [("latest.json is JSON", False, text.strip()[:80])]
    if not isinstance(data, dict):
        return [("latest.json is a JSON object", False, type(data).__name__)]
    out = []
    version = str(data.get("version") or "").lstrip("v")
    out.append(("version is present", bool(version), version))
    if expected_version is not None:
        want = expected_version.lstrip("v")
        out.append((f"version is {want}", version == want, f"feed says {version!r}"))
    url = str(data.get("url") or "")
    prefix = f"https://github.com/{slug}/releases/download/"
    out.append(("url is this repository's release asset", url.startswith(prefix) and url.endswith(".exe"), url))
    out.append(("signature is present", bool(str(data.get("signature") or "").strip()), ""))
    notes = str(data.get("notes") or "")
    out.append(("notes are present and in Korean", bool(re.search("[가-힣]", notes)),
                notes.splitlines()[0] if notes else "no notes"))
    return out

