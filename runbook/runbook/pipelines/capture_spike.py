"""Pipeline `capture-spike`: does the app's raw-socket sniffer see traffic that this same PC sends?

The game's chat reaches the app as TCP segments from port 5003, read with `SIO_RCVALL` on one adapter's address. To test
the capture end to end with no game, this PC plays the game server: a TCP server on port 5003 sends real chat frames (built by
`resonance_core::capture::synth`) to a client on the same PC. The open question (see `.memory/sessions/2026-10-06-automation-plan.md`):
Windows may deliver a PC's traffic to itself internally, without it ever passing the adapter the sniffer listens on. So it is tried
two ways, each a row:

  lan       the server and client use this PC's LAN address (the one the default route uses), the sniffer listens on it
  loopback  both use 127.0.0.1, the sniffer listens on 127.0.0.1

For each: the app starts with the sniffer ON and `network_interface` set in its config, the frames of the sample are sent, and the
`packet-event`s are counted. The rows say how many of the lines arrived and what the sniffer said about itself.

Needs an exe with a firewall rule of its own (the app does not start the sniffer without it). The rule is not made here unless asked
(`--add-firewall-rule`: a rule for this copy only, any remote address, removed at the end): by default a missing rule is a `skip` that says so.
"""
from __future__ import annotations

import json
import socket
import subprocess
import threading
import time
from pathlib import Path

from runbook import bridge, common, mockfeed, updater
from runbook.common import Recorder

ROOT = Path(__file__).resolve().parents[3]
SAMPLE = ROOT / "crates" / "core" / "testdata" / "replay-sample.jsonl"
PORT = 5003
MAX_GAP_S = 0.3  # the sample's pauses are for reading; the sniffer does not need them
GRACE_S = 6  # after the last frame, how long to wait for the last chat
BIND_WAIT_S = 90  # how long the sniffer may take to say it listens (the window must load first)


def cargo_example(*args: str) -> str:
    """Output of `cargo run -q -p resonance-core --example synth_frames -- ARGS`."""
    done = subprocess.run(["cargo", "run", "-q", "-p", "resonance-core", "--example", "synth_frames", "--", *args],
                          cwd=ROOT, capture_output=True, text=True, encoding="utf-8", timeout=600)
    if done.returncode != 0:
        raise RuntimeError(f"synth_frames {' '.join(args)} failed: {done.stderr.strip()[-300:]}")
    return done.stdout


def sample_frames(sample: Path = SAMPLE) -> list[dict]:
    """The chat lines to send, those whose text the app shows as it is: `{delay_ms, hex, text, nickname, ...}`."""
    lines = [json.loads(line) for line in cargo_example("frames", str(sample)).splitlines() if line.strip()]
    return [line for line in lines if not line["text"].startswith("emojiPic=") and "<sprite=" not in line["text"]]


def lan_address() -> str | None:
    """The address the OS would use to reach the internet (what the app's route rule picks); None when offline."""
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as probe:
            probe.connect(("8.8.8.8", 80))
            return probe.getsockname()[0]
    except OSError:
        return None


class FrameServer:
    """Plays the game server: listens on (ip, 5003), and sends frames to everyone connected -- the client this class
    opens itself, and anything else that connects (the stand-in app, in a dry run)."""

    def __init__(self, ip: str, port: int = PORT) -> None:
        self.ip, self.port = ip, port
        self.listener = socket.socket()
        self.listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.listener.bind((ip, port))
        self.listener.listen(5)
        self.conns: list[socket.socket] = []
        self._lock = threading.Lock()
        threading.Thread(target=self._accept, daemon=True).start()
        self.client = socket.create_connection((ip, port), timeout=10)
        threading.Thread(target=self._drain, daemon=True).start()

    def _accept(self) -> None:
        while True:
            try:
                conn, _ = self.listener.accept()
            except OSError:
                return
            with self._lock:
                self.conns.append(conn)

    def _drain(self) -> None:
        try:
            while self.client.recv(4096):
                pass
        except OSError:
            pass

    def send(self, data: bytes) -> int:
        with self._lock:
            conns = list(self.conns)
        sent = 0
        for conn in conns:
            try:
                conn.sendall(data)
                sent += 1
            except OSError:
                pass
        return sent

    def close(self) -> None:
        for sock in [self.client, self.listener, *self.conns]:
            try:
                sock.shutdown(socket.SHUT_RDWR)  # wakes `accept` on POSIX, where close() alone leaves the port listening
            except OSError:
                pass
            try:
                sock.close()
            except OSError:
                pass


# --- the firewall rule the app wants for its exe (resonance_core::sniffer_net::rule_name_for) ---------------
def rule_name(exe: Path) -> str:
    return cargo_example("rule-name", str(exe)).strip()


def rule_exists(name: str) -> bool:
    if common.dry_run():
        return True
    done = subprocess.run(["netsh", "advfirewall", "firewall", "show", "rule", f"name={name}"], capture_output=True)
    return done.returncode == 0


def add_rule(name: str, exe: Path) -> bool:
    done = subprocess.run(["netsh", "advfirewall", "firewall", "add", "rule", f"name={name}", "dir=in", "action=allow",
                           "protocol=TCP", "remoteport=5003", f"program={exe}", "enable=yes", "profile=any"], capture_output=True)
    return done.returncode == 0


def delete_rule(name: str) -> None:
    subprocess.run(["netsh", "advfirewall", "firewall", "delete", "rule", f"name={name}"], capture_output=True)


class CaptureSpike:
    def __init__(self, rec: Recorder, exe: str | Path, runs: Path | None = None, add_firewall_rule: bool = False,
                 variants: tuple[str, ...] = ("lan", "loopback"), sample: Path = SAMPLE) -> None:
        self.rec = rec
        self.exe = Path(exe)
        self.runs = Path(runs or common.RUNS) / "capture-spike"
        self.add_firewall_rule = add_firewall_rule
        self.variants = variants
        self.sample = sample
        self._serves: list[bridge.Serve] = []
        self._servers: list[FrameServer] = []
        self._rules: list[str] = []

    def run(self, key_path: str | None = None, password: str | None = None) -> None:
        """(`key_path` and `password` are for the pipelines that sign; this one ignores them.)"""
        frames = sample_frames(self.sample)
        for variant in self.variants:
            ip = "127.0.0.1" if variant == "loopback" else lan_address()
            check = f"CS-{variant}"
            if ip is None:
                self.rec.record(check, f"the sniffer sees the chat sent from this PC ({variant})", "skip", "no network route: this PC is offline")
                continue
            with updater.step(self.rec, check, self.runs, strays=self.runs, stop=self._stop):
                self.round(variant, ip, frames)

    def _stop(self, folder: str | Path) -> list[int]:
        for server in self._servers:
            server.close()
        self._servers.clear()
        for serve in self._serves:
            serve.stop()
        self._serves.clear()
        for name in self._rules:
            delete_rule(name)
        self._rules.clear()
        return mockfeed.stop_copies(folder)

    def firewall(self, exe: Path, check: str) -> bool:
        """The rule the app looks for exists (made here when asked); False: the round cannot run."""
        name = rule_name(exe)
        if rule_exists(name):
            return True
        if not self.add_firewall_rule:
            self.rec.record(check, "the sniffer sees the chat sent from this PC", "skip",
                            f"this copy has no firewall rule ({name}), and the app does not start the sniffer without it. Run again with "
                            "--add-firewall-rule (a rule for this copy only, removed at the end), or give it its rule by hand")
            return False
        if not add_rule(name, exe):
            self.rec.auto(f"{check}-firewall", "the firewall rule for this copy was made", False, f"netsh failed for {name} (not elevated?)")
            return False
        self._rules.append(name)
        return True

    def round(self, variant: str, ip: str, frames: list[dict]) -> None:
        check = f"CS-{variant}"
        exe = updater.fresh_copy(self.exe, self.runs, variant)
        if not self.firewall(exe, check):
            return
        data = exe.parent / "data"
        (data / "config").mkdir(parents=True, exist_ok=True)
        (data / "config" / "config.json").write_text(json.dumps({"init_done": True, "network_interface": ip, "use_translation": False}), encoding="utf-8")
        events = bridge.Serve(exe.parent / "events.jsonl")
        self._serves.append(events)
        args = mockfeed.flag_args(data, exe.parent / "status.json", log_file=exe.parent / "app.log", fresh=False, capture=True,
                                  extra=("--no-update-check", "--bridge-url", events.url))
        mockfeed.start_app(exe, args)
        if not bridge.wait_started(events, exe.parent / "app.log", label=f"the app ({variant})"):
            self.rec.auto(f"{check}-start", "the copy started and connected to the bridge", False, "no app-started event in 120 s")
            return
        try:
            events.expect("sniffer-state", {"payload.state": {"regex": "^(Pending|Active)$"}}, timeout=BIND_WAIT_S)
            self.rec.auto(f"{check}-bind", f"the sniffer listens on {ip}", True)
        except RuntimeError:
            self.rec.auto(f"{check}-bind", f"the sniffer listens on {ip}", False, self.said(events))
            return
        server = FrameServer(ip)
        self._servers.append(server)
        time.sleep(1)
        wanted = [f["text"] for f in frames]
        print(f"  sending {len(frames)} chat frames from port {PORT} on {ip} ...", flush=True)
        for frame in frames:
            time.sleep(min(frame["delay_ms"] / 1000, MAX_GAP_S))
            server.send(bytes.fromhex(frame["hex"]))
        time.sleep(GRACE_S)
        seen = [m["message"]["payload"]["message"] for m in events.events() if m["topic"] == "rs/app/event/packet-event"]
        got = [text for text in wanted if text in seen]
        self.rec.auto(check, f"the sniffer saw the chat this PC sent on {ip}", got == wanted,
                      f"{len(got)} of {len(wanted)} lines arrived ({len(seen)} packet-events); {self.said(events)}")
        try:
            events.send("quit", timeout=15)
        except RuntimeError:
            pass

    @staticmethod
    def said(events: bridge.Serve) -> str:
        """What the sniffer said about itself, for the evidence column."""
        said = []
        for m in events.events():
            payload = m["message"].get("payload") if isinstance(m["message"], dict) else None
            if m["topic"] == "rs/app/event/system-event" and payload and payload.get("source") == "Sniffer":
                said.append(str(payload.get("message")))
            if m["topic"] == "rs/app/event/sniffer-state" and payload:
                said.append(f"[{payload.get('state')}] {payload.get('message')}")
        return "sniffer: " + (" | ".join(said[-6:]) or "(nothing)")
