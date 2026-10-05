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
        lines.append({"topic": "rs/app/event/chat-message-update", "message": {"seq": seq, "t_ms": t, "name": "chat-message-update", "payload": payload}})
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
