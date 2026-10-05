import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from tools import dryrun  # noqa: E402

NB = Path(__file__).resolve().parent.parent / "notebooks" / "w1-feed.ipynb"


def test_a_green_run_and_a_healthy_feed(monkeypatch):
    report = dryrun.execute(NB, "pass")
    assert "pass 8 / fail 0 / skip 0" in report
    assert "K3-3 Release feed workflow run is green | pass | https://github.com/enjay27/resonance-stream/actions/runs/900002" in report


def test_a_red_run_is_a_failure_with_its_url(monkeypatch):
    monkeypatch.setenv("W1_FIXTURE_RUNS", "gh_runs_failed.json")
    report = dryrun.execute(NB, "pass")
    assert "K3-3 Release feed workflow run is green | fail | https://github.com/enjay27/resonance-stream/actions/runs/900004" in report


def test_a_run_that_never_finishes_times_out_as_a_failure(monkeypatch):
    monkeypatch.setenv("W1_FIXTURE_RUNS", "gh_runs_running.json")
    report = dryrun.execute(NB, "pass")
    assert "still running after 600 s" in report


def test_committed_without_outputs():
    nb = json.loads(NB.read_text(encoding="utf-8"))
    assert all(not c.get("outputs") for c in nb["cells"] if c["cell_type"] == "code")
