import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from runbook import bridge  # noqa: E402

SAMPLE = Path(__file__).resolve().parents[2] / "crates" / "core" / "testdata" / "replay-sample.jsonl"

needs_node = pytest.mark.skipif(
    bridge.node_path() is None or not (bridge.BRIDGE_DIR / "node_modules" / "aedes").is_dir(),
    reason="needs node and `npm ci` in runbook/bridge")


def _recording(path: Path, messages: list[str], levels=None) -> None:
    """What the app publishes for the shipped sample, one record per line."""
    entries = [json.loads(l) for l in SAMPLE.read_text(encoding="utf-8").splitlines() if l.strip() and not l.startswith("#")]
    t = 1000
    seq = 0
    lines = []
    for i, e in enumerate(entries):
        t += e.get("delay_ms", 0)
        payload = {"pid": i, "channel": e["channel"], "nickname": e["nickname"], "level": e["level"], "message": messages[i]}
        lines.append({"topic": "rs/app/event/packet-event", "message": {"seq": seq, "t_ms": t, "name": "packet-event", "payload": payload}})
        seq += 1
    lines.append({"topic": "rs/app/event/system-event", "message": {"seq": seq, "t_ms": t, "name": "system-event",
                  "payload": {"level": "info", "source": "Replay", "message": "Replay finished"}}})
    path.write_text("\n".join(json.dumps(l, ensure_ascii=False) for l in lines) + "\n", encoding="utf-8")


def _expected_messages() -> list[str]:
    import re
    entries = [json.loads(l) for l in SAMPLE.read_text(encoding="utf-8").splitlines() if l.strip() and not l.startswith("#")]
    return [re.sub(r"emojiPic=\d+", "[스티커]", re.sub(r"<sprite=\d+>", "[이모지]", e["text"])) for e in entries]


@needs_node
def test_a_faithful_recording_of_the_shipped_sample_passes(tmp_path):
    log = tmp_path / "events.jsonl"
    _recording(log, _expected_messages())
    result = bridge.verify("replay-chat", log, SAMPLE)
    assert result["ok"], result
    assert [c["id"] for c in result["checks"]] == ["B1", "B2", "B3", "B4", "B5"]


@needs_node
def test_a_wrong_text_fails_and_says_which_line(tmp_path):
    log = tmp_path / "events.jsonl"
    messages = _expected_messages()
    messages[3] = "回復お願いします！<sprite=3>"  # the backend did not replace the emote
    _recording(log, messages)
    result = bridge.verify("replay-chat", log, SAMPLE)
    b2 = next(c for c in result["checks"] if c["id"] == "B2")
    assert not result["ok"] and not b2["ok"] and "line 4" in b2["detail"]


@needs_node
def test_the_broker_starts_on_a_free_port_and_stops(tmp_path):
    with bridge.Serve(tmp_path / "events.jsonl") as b:
        assert b.port > 0 and b.url == f"mqtt://127.0.0.1:{b.port}"
    assert b._proc.poll() is not None


@needs_node
def test_a_command_nobody_answers_is_an_error(tmp_path):
    with bridge.Serve(tmp_path / "events.jsonl") as b:
        with pytest.raises(RuntimeError, match="no ack"):
            b.send("ping", timeout=0.3)


def test_an_unknown_scenario_is_an_error(tmp_path):
    if bridge.node_path() is None:
        pytest.skip("needs node")
    with pytest.raises(RuntimeError, match="unknown scenario"):
        bridge.verify("nope", tmp_path / "x", SAMPLE)


def _publish(port: int, events: list[tuple[str, dict]]) -> None:
    """A stand-in app for the test: publishes `events` ((name, payload) pairs) to the broker from Node."""
    import subprocess
    script = (
        "import mqtt from 'mqtt';"
        f"const c = await mqtt.connectAsync('mqtt://127.0.0.1:{port}');"
        f"const evs = {json.dumps(events)};"
        "for (const [i, [name, payload]] of evs.entries())"
        "  await c.publishAsync(`rs/app/event/${name}`, JSON.stringify({seq: i, t_ms: Date.now(), name, payload}), {qos: 1});"
        "await c.endAsync();"
    )
    subprocess.run(["node", "--input-type=module", "-e", script], cwd=bridge.BRIDGE_DIR, check=True, timeout=30)


@needs_node
def test_expect_waits_for_an_event_that_matches(tmp_path):
    with bridge.Serve(tmp_path / "events.jsonl") as b:
        _publish(b.port, [("update-state", {"state": "available:0.6.9"}), ("update-state", {"state": "downloading"})])
        found = b.expect("update-state", {"payload.state": "downloading"}, timeout=3)
        assert found["seq"] == 1 and found["payload"]["state"] == "downloading"
        with pytest.raises(RuntimeError, match="timed out"):
            b.expect("update-state", {"payload.state": {"regex": "^error"}}, timeout=0.3)


@needs_node
def test_expect_sequence_wants_the_order(tmp_path):
    with bridge.Serve(tmp_path / "events.jsonl") as b:
        _publish(b.port, [("update-state", {"state": s}) for s in ("available:0.6.9", "downloading", "downloaded")])
        steps = [("update-state", {"payload.state": {"regex": "^" + s}}) for s in ("available", "downloading", "downloaded")]
        assert [m["seq"] for m in b.expect_sequence(steps, timeout=3)] == [0, 1, 2]
        with pytest.raises(RuntimeError, match="step 2"):
            b.expect_sequence([steps[2], steps[1]], timeout=0.3)


@needs_node
def test_events_reads_what_was_recorded(tmp_path):
    with bridge.Serve(tmp_path / "events.jsonl") as b:
        _publish(b.port, [("sniffer-state", {"running": True})])
        b.expect("sniffer-state", timeout=3)
        recorded = b.events()
        assert [(r["topic"], r["message"]["payload"]) for r in recorded] == [("rs/app/event/sniffer-state", {"running": True})]


@needs_node
def test_wait_started_gives_the_event_when_the_app_connects(tmp_path, capsys):
    with bridge.Serve(tmp_path / "events.jsonl") as b:
        _publish(b.port, [("app-started", {"pid": 7, "version": "0.6.1", "exe": "x"})])
        got = bridge.wait_started(b, tmp_path / "app.log", timeout=10, label="copy")
        assert got["pid"] == 7
    assert "waiting" in capsys.readouterr().out


@needs_node
def test_wait_started_says_what_it_waits_for_and_shows_the_log_when_nothing_comes(tmp_path, capsys):
    log = tmp_path / "app.log"
    log.write_text("starting\nsomething went wrong\n", encoding="utf-8")
    with bridge.Serve(tmp_path / "events.jsonl") as b:
        assert bridge.wait_started(b, log, timeout=2, label="copy", beat=1) is None
    out = capsys.readouterr().out
    assert "waiting for copy" in out and "still waiting" in out
    assert "something went wrong" in out, "the app's own log is shown"


def test_the_bridge_packages_are_installed_before_a_pipeline_starts(monkeypatch):
    from runbook import run

    calls = []
    monkeypatch.setattr(bridge, "ensure_installed", lambda *a, **k: calls.append("npm ci"))
    monkeypatch.setattr(run.common, "require_windows_admin", lambda rec: False)  # stop right after the set-up
    import tempfile
    exe = Path(tempfile.mkdtemp()) / "x.exe"
    exe.write_text("x")
    run.main(["window-restore", "--exe", str(exe)])
    assert calls == ["npm ci"]


def test_the_broker_and_verify_are_read_as_utf8_not_the_machine_codepage(monkeypatch, tmp_path):
    """On Windows `text=True` alone reads the broker's Japanese text as cp1252 (the `translator-stub` run died with
    `UnicodeDecodeError: 'charmap' codec can't decode byte 0x81`), so every child process says its encoding."""
    seen = {}

    class FakeProc:
        stdout = type("O", (), {"readline": lambda self: '{"port": 1}\n'})()
        stderr = None
        stdin = None

        def poll(self):
            return 0

    def popen(*args, **kwargs):
        seen["popen"] = kwargs
        return FakeProc()

    def run(*args, **kwargs):
        seen["run"] = kwargs
        return type("R", (), {"returncode": 0, "stdout": '{"ok": true, "checks": []}', "stderr": ""})()

    monkeypatch.setattr(bridge.subprocess, "Popen", popen)
    monkeypatch.setattr(bridge.subprocess, "run", run)
    monkeypatch.setattr(bridge, "node_path", lambda: "node")
    bridge.Serve(tmp_path / "out.jsonl")
    bridge.verify("replay-chat", tmp_path / "log", tmp_path / "sample")
    for call in ("popen", "run"):
        assert seen[call].get("encoding") == "utf-8", call
        assert seen[call].get("errors") == "replace", call
