"""Dry run of the `capture-spike` pipeline against tests/fake_app.py, whose stand-in "sniffer" is a TCP client of the port-5003 server
the pipeline starts. It proves the pipeline builds real frames, sends them, counts the `packet-event`s and judges them; whether
Windows lets a raw socket see a PC's own traffic is exactly what it cannot say (that is the spike)."""
import socket
import stat
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from runbook import bridge, common, mockfeed  # noqa: E402
from runbook.common import Recorder  # noqa: E402
from runbook.pipelines import capture_spike  # noqa: E402
from runbook.pipelines.capture_spike import CaptureSpike  # noqa: E402

HERE = Path(__file__).resolve().parent
pytestmark = [
    pytest.mark.skipif(sys.platform == "win32", reason="the stand-in app is a shell script"),
    pytest.mark.skipif(bridge.node_path() is None or not (bridge.BRIDGE_DIR / "node_modules" / "aedes").is_dir(),
                       reason="needs node and `npm ci` in runbook/bridge"),
]


def port_5003_is_free() -> bool:
    with socket.socket() as s:
        s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        try:
            s.bind(("127.0.0.1", 5003))
        except OSError:
            return False
    return True


@pytest.fixture(autouse=True)
def env(monkeypatch, tmp_path):
    if not port_5003_is_free():
        pytest.skip("port 5003 is in use on this machine")
    monkeypatch.setattr(common, "RUNS", tmp_path)
    monkeypatch.setattr(capture_spike, "MAX_GAP_S", 0.02)
    monkeypatch.setattr(capture_spike, "GRACE_S", 1.5)
    monkeypatch.setenv("RUNBOOK_DRYRUN", "1")
    exe = tmp_path / "local" / "resonance-stream.exe"
    exe.parent.mkdir()
    exe.write_text('#!/bin/sh\nFAKE_APP_EXE="$0" exec python3 "$FAKE_APP_PY" "$@"\n', encoding="utf-8")
    exe.chmod(exe.stat().st_mode | stat.S_IXUSR)
    monkeypatch.setenv("FAKE_APP_PY", str(HERE / "fake_app.py"))
    monkeypatch.setenv("FAKE_APP_LIFETIME", "90")
    yield exe


def run(exe, tmp_path, variants=("loopback",)):
    rec = Recorder("capture-spike")
    CaptureSpike(rec, exe, tmp_path, variants=variants).run()
    assert [p for p in mockfeed.posix_processes() if str(tmp_path) in p["Path"]] == [], "no app may be left running"
    return rec, {r.check: r for r in rec.rows}


def test_chat_sent_to_port_5003_arrives_as_packet_events(env, tmp_path):
    rec, got = run(env, tmp_path)
    assert rec.summary()["fail"] == 0, rec.report()
    assert got["CS-loopback-bind"].status == "pass"
    assert got["CS-loopback"].status == "pass", rec.report()
    assert "6 of 6 lines arrived" in got["CS-loopback"].evidence  # the sample's 8 lines minus the two with emotes


def test_a_capture_that_loses_chat_is_caught(env, tmp_path, monkeypatch):
    monkeypatch.setenv("FAKE_APP_SNIFF_BUG", "drop-after-2")
    rec, got = run(env, tmp_path)
    assert got["CS-loopback"].status == "fail" and "2 of 6 lines arrived" in got["CS-loopback"].evidence, rec.report()


def test_an_app_whose_sniffer_never_starts_fails_the_bind_row_and_goes_no_further(env, tmp_path, monkeypatch):
    monkeypatch.setenv("FAKE_APP_SNIFF_BUG", "never")
    monkeypatch.setattr(capture_spike, "BIND_WAIT_S", 3)
    rec, got = run(env, tmp_path)
    assert got["CS-loopback-bind"].status == "fail", rec.report()
    assert "CS-loopback" not in got, "nothing is sent to a sniffer that is not listening"


def test_a_variant_with_no_network_route_is_a_skip_not_a_failure(env, tmp_path, monkeypatch):
    monkeypatch.setattr(capture_spike, "lan_address", lambda: None)
    rec, got = run(env, tmp_path, variants=("lan",))
    assert got["CS-lan"].status == "skip" and "offline" in got["CS-lan"].evidence, rec.report()


def test_the_frames_the_example_builds_are_the_samples_chat_lines():
    frames = capture_spike.sample_frames()
    assert [f["nickname"] for f in frames][:2] == ["ミナト", "Kenji"]
    assert all(bytes.fromhex(f["hex"])[4:6] == b"\x00\x02" for f in frames)
    assert not any("emojiPic" in f["text"] or "<sprite=" in f["text"] for f in frames)


def test_a_copy_without_its_firewall_rule_is_a_skip_that_says_how(env, tmp_path, monkeypatch):
    monkeypatch.setattr(capture_spike, "rule_exists", lambda name: False)
    rec, got = run(env, tmp_path)
    assert got["CS-loopback"].status == "skip" and "--add-firewall-rule" in got["CS-loopback"].evidence, rec.report()
