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
    monkeypatch.setattr(capture_spike, "BURST_WAIT_S", 15)
    monkeypatch.setattr(capture_spike, "RESTART_WAIT_S", 6)  # a dead restart is waited for this long, not 90 s
    monkeypatch.setattr(capture_spike, "IDLE_WAIT_S", 12)  # the stand-in watchdog trips every ~3.3 s and its log throttle holds 6 s
    monkeypatch.setenv("FAKE_APP_WATCHDOG", "0.3,3,6")
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


DEEP = ("CS-loopback-burst", "CS-loopback-bytes", "CS-watchdog", "CS-log-dedup", "CS-vpn-hint", "CS-restart-nodup",
        "CS-loopback-sniffer-restart", "CS-loopback-after-restart")


def test_the_burst_idle_and_restart_rows_pass_on_a_capture_that_keeps_its_promises(env, tmp_path):
    rec, got = run(env, tmp_path)
    assert rec.summary()["fail"] == 0, rec.report()
    for check in DEEP:
        assert got[check].status == "pass", rec.report()
    assert "500 of 500" in got["CS-loopback-burst"].evidence and "20 of 20" in got["CS-loopback-bytes"].evidence


@pytest.mark.parametrize("bug, failing", [
    ("no-watchdog", {"CS-watchdog", "CS-log-dedup"}),
    ("no-throttle", {"CS-log-dedup"}),
    ("vpn-mismatch", {"CS-vpn-hint"}),
    ("no-remember", {"CS-restart-nodup"}),
    ("restart-dead", {"CS-loopback-sniffer-restart"}),
])
def test_each_broken_promise_of_the_deep_rows_is_caught(env, tmp_path, monkeypatch, bug, failing):
    monkeypatch.setenv("FAKE_APP_SNIFF_BUG", bug)
    monkeypatch.setenv("FAKE_APP_VPN", "vEthernet (stand-in)")
    rec, got = run(env, tmp_path)
    caught = {c for c in DEEP if c in got and got[c].status == "fail"}
    assert failing <= caught, f"{bug}: wanted {failing} among the failures, got {caught}\n{rec.report()}"


def test_a_burst_has_a_unique_line_per_frame_and_the_sizes_asked_for(tmp_path):
    frames = capture_spike.burst_frames(12, "burst", 1000, tmp_path)
    assert [f["text"] for f in frames] == [f"burst {n}" for n in range(1, 13)]
    assert len({f["hex"] for f in frames}) == 12


def test_the_server_can_cut_a_stream_into_one_byte_segments(env):
    server = capture_spike.FrameServer("127.0.0.1")
    try:
        got = bytearray()
        server.client.settimeout(3)
        server.send(b"hello world", chunk=1)
        # the client of the server drains in a thread of its own, so listen as a second client instead
        extra = socket.create_connection(("127.0.0.1", 5003), timeout=3)
        import time as _t

        _t.sleep(0.3)
        server.send(b"abcdef", chunk=1)
        while len(got) < 6:
            got += extra.recv(16)
        assert bytes(got) == b"abcdef"
        extra.close()
    finally:
        server.close()


def test_a_slow_starting_app_is_waited_for_after_the_restart(env, tmp_path, monkeypatch):
    """Real app, hosted runner: a start with the sniffer on takes ~20 s to be ready, and the bridge holds a command until then; the
    restart row asked for the chat log with the default 10 s and died on `no ack`."""
    monkeypatch.setenv("FAKE_APP_READY_DELAY", "11.5")
    rec, got = run(env, tmp_path)
    assert got["CS-restart-nodup"].status == "pass", rec.report()
    assert rec.summary()["fail"] == 0, rec.report()


def test_an_app_that_never_answers_after_the_restart_fails_the_row_with_its_log(env, tmp_path, monkeypatch):
    """Real app, hosted runner (run 37430504474): the second start bound its sniffer, then the first command got no ack in 90 s.
    A traceback says nothing about why; the row must fail on its own and carry the second run's app log and the topics it published."""
    monkeypatch.setenv("FAKE_APP_READY_DELAY", "30")
    monkeypatch.setattr(capture_spike, "READY_WAIT_S", 2)
    rec, got = run(env, tmp_path)
    row = got["CS-restart-nodup"]
    assert row.status == "fail", rec.report()
    assert "no ack for get-chat-history" in row.evidence, row.evidence
    assert "app log" in row.evidence and "topics" in row.evidence, row.evidence
    assert "CS-lan-error" not in got and "CS-loopback-error" not in got, rec.report()
