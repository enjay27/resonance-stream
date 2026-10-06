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

On the same running app, per variant, a burst and a stream cut into one-byte segments:

  CS-<variant>-burst   500 chat frames written at once (many frames in a segment, frames split across segments) all arrive, each once
  CS-<variant>-bytes   20 frames written one byte per TCP segment all arrive, each once

And, on the first variant only (they take minutes of silence and a restart):

  CS-watchdog        once the frames stop, the sniffer says so: its state turns Error with the red badge's text
  CS-log-dedup       over a hundred seconds of silence the system log has "No game traffic for 15s" twice, not at every trip: the
                     second carries "(repeated N more times)"
  CS-vpn-hint        the system log and the badge agree about a VPN adapter in the way (both name it or neither does; on a runner whose
                     route runs through a Hyper-V adapter both do)
  CS-restart-nodup   the app is closed and started again on the same folder and the server sends the same lines again: a line that was
                     archived (restored from the chat log) is not shown a second time

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
BURST_LINES = 500
BYTE_LINES = 20  # one byte per segment is thousands of tiny packets: enough to prove the reassembly, few enough that a busy runner keeps up
BURST_WAIT_S = 60  # how long a burst may take to arrive
IDLE_WAIT_S = 100  # silence: the app's watchdog trips ~20 s after the last frame and again every ~20 s; its log line is written at ~20 s and ~80 s
READY_WAIT_S = 90  # a command is held until the app has finished starting (up to 60 s), which takes ~20 s with the sniffer on
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


def burst_frames(count: int, tag: str, seq_base: int, folder: Path) -> list[dict]:
    """`count` GUILD chat frames, "<tag> 1" .. "<tag> <count>", each with a line and a sequence number of its own (so the capture's
    duplicate check keeps all of them), built the way `sample_frames` builds the sample's."""
    folder = Path(folder)
    folder.mkdir(parents=True, exist_ok=True)
    ts = int(time.time())
    lines = [json.dumps({"delay_ms": 0, "channel": "GUILD", "nickname": "Burst", "uid": 7001, "level": 60, "timestamp": ts,
                         "sequence_id": seq_base + n, "text": f"{tag} {n}"}) for n in range(1, count + 1)]
    path = folder / f"{tag}.jsonl"
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return [json.loads(line) for line in cargo_example("frames", str(path)).splitlines() if line.strip()]


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
            conn.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)  # one-byte writes stay one-byte segments
            with self._lock:
                self.conns.append(conn)

    def _drain(self) -> None:
        try:
            while self.client.recv(4096):
                pass
        except OSError:
            pass

    def send(self, data: bytes, chunk: int | None = None) -> int:
        """`data` to everyone connected, in one write, or `chunk` bytes at a time (1 = one byte per segment)."""
        with self._lock:
            conns = list(self.conns)
        sent = 0
        for conn in conns:
            try:
                if chunk is None:
                    conn.sendall(data)
                else:
                    for at in range(0, len(data), chunk):
                        conn.sendall(data[at:at + chunk])
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
        self.bursts: dict[str, list[dict]] = {}

    def run(self, key_path: str | None = None, password: str | None = None) -> None:
        """(`key_path` and `password` are for the pipelines that sign; this one ignores them.)"""
        frames = sample_frames(self.sample)
        self.bursts = {"burst": burst_frames(BURST_LINES, "burst", 1000, self.runs), "bytes": burst_frames(BYTE_LINES, "bytes", 5000, self.runs)}
        first = True
        for variant in self.variants:
            ip = "127.0.0.1" if variant == "loopback" else lan_address()
            check = f"CS-{variant}"
            if ip is None:
                self.rec.record(check, f"the sniffer sees the chat sent from this PC ({variant})", "skip", "no network route: this PC is offline")
                continue
            with updater.step(self.rec, check, self.runs, strays=self.runs, stop=self._stop):
                self.round(variant, ip, frames, deep=first)
            first = False

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

    def start(self, label: str, exe: Path, events: bridge.Serve, ip: str, variant: str, *, new_config: bool) -> bool:
        data = exe.parent / "data"
        if new_config:
            (data / "config").mkdir(parents=True, exist_ok=True)
            (data / "config" / "config.json").write_text(json.dumps({"init_done": True, "network_interface": ip, "use_translation": False}), encoding="utf-8")
        args = mockfeed.flag_args(data, exe.parent / f"{label}-status.json", log_file=exe.parent / f"{label}.log", fresh=False, capture=True,
                                  extra=("--no-update-check", "--bridge-url", events.url))
        mockfeed.start_app(exe, args)
        return bool(bridge.wait_started(events, exe.parent / f"{label}.log", label=f"the app ({variant}, {label})"))

    def wait_bound(self, events: bridge.Serve, check: str, ip: str, label: str) -> bool:
        try:
            events.expect("sniffer-state", {"payload.state": {"regex": "^(Pending|Active)$"}}, timeout=BIND_WAIT_S)
            self.rec.auto(label, f"the sniffer listens on {ip}", True)
            return True
        except RuntimeError:
            self.rec.auto(label, f"the sniffer listens on {ip}", False, self.said(events))
            return False

    @staticmethod
    def packets(events: bridge.Serve) -> list[dict]:
        return [m["message"]["payload"] for m in events.events() if m["topic"] == "rs/app/event/packet-event"]

    def send_frames(self, server: FrameServer, frames: list[dict]) -> None:
        for frame in frames:
            time.sleep(min(frame["delay_ms"] / 1000, MAX_GAP_S))
            server.send(bytes.fromhex(frame["hex"]))

    def round(self, variant: str, ip: str, frames: list[dict], deep: bool = False) -> None:
        check = f"CS-{variant}"
        exe = updater.fresh_copy(self.exe, self.runs, variant)
        if not self.firewall(exe, check):
            return
        events = bridge.Serve(exe.parent / "events.jsonl")
        self._serves.append(events)
        if not self.start("first", exe, events, ip, variant, new_config=True):
            self.rec.auto(f"{check}-start", "the copy started and connected to the bridge", False, "no app-started event in 120 s")
            return
        if not self.wait_bound(events, check, ip, f"{check}-bind"):
            return
        server = FrameServer(ip)
        self._servers.append(server)
        time.sleep(1)
        wanted = [f["text"] for f in frames]
        print(f"  sending {len(frames)} chat frames from port {PORT} on {ip} ...", flush=True)
        self.send_frames(server, frames)
        time.sleep(GRACE_S)
        seen = [p["message"] for p in self.packets(events)]
        got = [text for text in wanted if text in seen]
        self.rec.auto(check, f"the sniffer saw the chat this PC sent on {ip}", got == wanted,
                      f"{len(got)} of {len(wanted)} lines arrived ({len(seen)} packet-events); {self.said(events)}")
        archived = [p["message"] for p in self.packets(events) if p["message"] in wanted and p.get("channel") != "WORLD"]

        self.burst(events, server, ip, variant, "burst", f"{BURST_LINES} frames written at once", None)
        self.burst(events, server, ip, variant, "bytes", f"{BYTE_LINES} frames written one byte per TCP segment", 1)
        if deep:
            self.silence(events, variant)
            self.restart(exe, events, server, frames, archived, ip, variant)
        else:
            self.quit(events)

    def burst(self, events: bridge.Serve, server: FrameServer, ip: str, variant: str, kind: str, what: str, chunk: int | None) -> None:
        frames = self.bursts.get(kind) or []
        wanted = [f["text"] for f in frames]
        if not wanted:
            return
        server.send(b"".join(bytes.fromhex(f["hex"]) for f in frames), chunk=chunk)
        deadline = time.monotonic() + BURST_WAIT_S
        while time.monotonic() < deadline:
            mine = [p["message"] for p in self.packets(events) if p["message"].startswith(f"{kind} ")]
            if len(mine) >= len(wanted):
                break
            time.sleep(0.5)
        time.sleep(1)  # a duplicate would come right behind
        mine = [p["message"] for p in self.packets(events) if p["message"].startswith(f"{kind} ")]
        ok = sorted(mine) == sorted(wanted)
        self.rec.auto(f"CS-{variant}-{kind}", f"the sniffer saw all of {what} on {ip}, each once", ok,
                      f"{len(set(mine) & set(wanted))} of {len(wanted)} lines arrived, {len(mine)} packet-events; {self.said(events)}")

    def silence(self, events: bridge.Serve, variant: str) -> None:
        """No traffic for `IDLE_WAIT_S`: what the watchdog and the system log do about it."""
        began = time.time() * 1000
        print(f"  no traffic for {IDLE_WAIT_S:.0f} s: the watchdog should say so, once in the log ...", flush=True)
        waited = 0.0
        while waited < IDLE_WAIT_S:
            step = min(15.0, IDLE_WAIT_S - waited)
            time.sleep(step)
            waited += step
            if waited < IDLE_WAIT_S:
                print(f"  ... {waited:.0f} s of silence", flush=True)
        after = [m for m in events.events() if m["received_at"] >= began]
        states = [m["message"]["payload"] for m in after if m["topic"] == "rs/app/event/sniffer-state"]
        errors = [p for p in states if p.get("state") == "Error"]
        self.rec.auto("CS-watchdog", "once the chat stops, the sniffer says so (state Error, the red badge's text)", bool(errors),
                      f"{len(errors)} Error state(s); the badge says {errors[0].get('message')!r}" if errors else f"states since: {[p.get('state') for p in states]}")
        lines = [str(m["message"]["payload"].get("message")) for m in after
                 if m["topic"] == "rs/app/event/system-event" and m["message"]["payload"].get("source") == "Sniffer"
                 and "No game traffic" in str(m["message"]["payload"].get("message"))]
        ok = len(lines) == 2 and "repeated" in lines[1] and "repeated" not in lines[0]
        self.rec.auto("CS-log-dedup", f"in {IDLE_WAIT_S:.0f} s of silence the watchdog's log line is written twice, the second with '(repeated N more times)'", ok,
                      f"{len(lines)} line(s): {lines}")
        log_vpn = any("VPN adapter" in line for line in lines)
        badge_vpn = any("VPN" in str(p.get("message")) for p in errors)
        self.rec.auto("CS-vpn-hint", "the system log and the badge agree about a VPN adapter in the way", log_vpn == badge_vpn,
                      f"log names a VPN adapter: {log_vpn}; badge names it: {badge_vpn}" + (f" ({lines[0]})" if log_vpn and lines else ""))

    def restart(self, exe: Path, events: bridge.Serve, server: FrameServer, frames: list[dict], archived: list[str], ip: str, variant: str) -> None:
        """Close the app, start it again on the same folder, and have the server say the same lines again."""
        if not self.quit(events):
            self.rec.auto("CS-restart-nodup", "a line the chat log restored is not shown again", False, "the first run did not end when told to")
            return
        again = bridge.Serve(exe.parent / "again-events.jsonl")
        self._serves.append(again)
        if not self.start("again", exe, again, ip, variant, new_config=False) or not self.wait_bound(again, "CS-restart-nodup", ip, f"CS-{variant}-rebind"):
            self.rec.auto("CS-restart-nodup", "a line the chat log restored is not shown again", False, "the second run did not start or bind")
            return
        time.sleep(1)
        try:
            restored = [m["message"] for m in again.send("get-chat-history", timeout=READY_WAIT_S)["data"]]
        except RuntimeError as e:
            # Seen on the hosted runner: the second start bound its sniffer and then never answered a command. Say what the app
            # itself said, so one run is enough to tell a lost command from a blocked handler.
            self.rec.auto("CS-restart-nodup", "a line the chat log restored is not shown again", False, self.silent_app(exe, again, e))
            self.quit(again)
            return
        time.sleep(1)
        self.send_frames(server, frames)
        time.sleep(GRACE_S)
        shown = [p["message"] for p in self.packets(again)]
        reshown = [t for t in archived if t in shown]
        missing = [t for t in archived if t not in restored]
        self.rec.auto("CS-restart-nodup", "the same lines sent again after a restart: a line the chat log restored is not shown a second time",
                      not reshown and not missing,
                      f"{len(archived)} archived lines; restored {len(restored)} rows, missing {missing}; shown again: {reshown}; "
                      f"shown again that were not archived (WORLD): {[t for t in shown if t not in archived]}")
        self.quit(again)

    @staticmethod
    def silent_app(exe: Path, events: bridge.Serve, error: Exception) -> str:
        """The evidence for an app that stopped answering: the error, the last topics it published, the tail of its own log."""
        topics = [m["topic"].removeprefix("rs/app/") for m in events.events()]
        log = mockfeed.log_tail(exe.parent / "again.log", 40)
        print(f"  the second run stopped answering ({error}); its log:\n" + "\n".join("    " + t for t in log.splitlines()), flush=True)
        return f"{error}; last topics: {topics[-12:]}; app log (last 40 lines): {log!r}"

    @staticmethod
    def quit(events: bridge.Serve) -> bool:
        try:
            events.send("quit", timeout=15)
            events.expect("rs/app/status", "offline", timeout=60)
        except RuntimeError:
            return False
        time.sleep(1)
        return True

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
