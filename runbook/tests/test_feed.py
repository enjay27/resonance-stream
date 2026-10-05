import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from runbook import feed  # noqa: E402

FIX = Path(__file__).resolve().parent / "fixtures"


def text(name):
    return (FIX / name).read_text(encoding="utf-8")


SINCE = "2026-10-04T06:19:50Z"


def test_the_dispatched_run_is_the_newest_one_started_after_we_asked():
    run = feed.pick_dispatched_run(text("gh_runs_success.json"), SINCE)
    assert run["databaseId"] == 900002


def test_a_scheduled_run_is_not_ours():
    runs = json.dumps([r for r in json.loads(text("gh_runs_success.json")) if r["event"] == "schedule"])
    assert feed.pick_dispatched_run(runs, SINCE) is None


def test_a_run_from_before_we_asked_is_not_ours():
    assert feed.pick_dispatched_run(text("gh_runs_success.json"), "2026-10-04T06:21:00Z") is None


def test_no_runs_listed_is_none():
    assert feed.pick_dispatched_run("[]", SINCE) is None
    assert feed.pick_dispatched_run("", SINCE) is None


def test_garbage_is_an_error():
    with pytest.raises(ValueError):
        feed.pick_dispatched_run("gh: not logged in", SINCE)


@pytest.mark.parametrize("name,state", [
    ("gh_runs_success.json", "pass"), ("gh_runs_running.json", "running"), ("gh_runs_failed.json", "fail")])
def test_run_state(name, state):
    run = feed.pick_dispatched_run(text(name), SINCE)
    assert feed.run_state(run)[0] == state


def test_a_cancelled_run_is_a_failure_not_a_pass():
    assert feed.run_state({"status": "completed", "conclusion": "cancelled", "url": "u"})[0] == "fail"


def test_the_evidence_is_the_run_url():
    run = feed.pick_dispatched_run(text("gh_runs_success.json"), SINCE)
    assert feed.run_state(run)[1].endswith("/actions/runs/900002")


# --- polling: a fake clock, nothing sleeps --------------------------------------------------------
def test_poll_returns_as_soon_as_the_state_is_final():
    states = iter([("running", ""), ("running", ""), ("pass", "u")])
    clock = feed.FakeClock()
    assert feed.poll(lambda: next(states), clock.sleep, clock.now, timeout=600, interval=10) == ("pass", "u")
    assert clock.slept == [10, 10]


def test_poll_gives_up_at_the_timeout():
    clock = feed.FakeClock()
    state, evidence = feed.poll(lambda: ("running", ""), clock.sleep, clock.now, timeout=30, interval=10)
    assert state == "fail" and "30" in evidence


def test_poll_treats_no_run_yet_as_running():
    states = iter([None, ("pass", "u")])
    clock = feed.FakeClock()
    assert feed.poll(lambda: next(states), clock.sleep, clock.now, timeout=600, interval=5)[0] == "pass"


# --- the feed the app reads ------------------------------------------------------------------------
def check(feed_text, **kw):
    return feed.check_feed_json(feed_text, "enjay27/resonance-stream", **kw)


def test_the_real_shaped_feed_passes_every_check():
    assert all(ok for _, ok, _ in check(text("real_latest_json_v0.6.1.json"))), check(text("real_latest_json_v0.6.1.json"))


def test_the_expected_version_is_checked():
    assert any(not ok for _, ok, _ in check(text("real_latest_json_v0.6.1.json"), expected_version="0.6.2"))
    assert all(ok for _, ok, _ in check(text("real_latest_json_v0.6.1.json"), expected_version="v0.6.1"))


@pytest.mark.parametrize("field", ["version", "url", "signature", "notes"])
def test_a_missing_field_fails(field):
    data = json.loads(text("real_latest_json_v0.6.1.json"))
    del data[field]
    assert any(not ok for _, ok, _ in check(json.dumps(data)))


def test_a_url_on_another_host_fails():
    data = json.loads(text("real_latest_json_v0.6.1.json"))
    data["url"] = "https://evil.example/Resonance-Stream-v0.6.1.exe"
    assert any(not ok and "url" in t for t, ok, _ in check(json.dumps(data)))


def test_notes_without_korean_fail():
    data = json.loads(text("real_latest_json_v0.6.1.json"))
    data["notes"] = "fixed the download"
    assert any(not ok and "notes" in t for t, ok, _ in check(json.dumps(data)))


def test_not_json_is_one_failed_check_not_a_crash():
    checks = check("<html>Not Found</html>")
    assert len(checks) == 1 and not checks[0][1]


def test_the_real_published_feed_passes_every_check():
    checks = check(text("real_latest_json_v0.6.1.json"), expected_version="0.6.1")
    assert all(ok for _, ok, _ in checks), checks
