"""A stand-in for a test-env build of Resonance Stream -- only for the dry run of updater-mock.ipynb.

It reads the same flags, writes the same status file and follows the same update steps (check the
feed, download, verify, swap + restart), so the pipeline's steps, waits and checks can be exercised
on any OS. With --bridge-url it also talks to the bridge's MQTT broker the way src-tauri/src/bridge.rs
does (a minimal MQTT 3.1.1 client): it publishes `app-started` and `update-state` events and obeys the
`ping`, `start-update`, `restart-update` and `quit` commands -- nobody "clicks" any more. It is NOT the
app: what it does is this author's reading of src-tauri, and proves nothing about Windows or the real exe.

The notebook's "exe" is a shell script that runs this file (so bytes appended to it, as the
notebook does for the new version, are never parsed). Environment:
  FAKE_APP_EXE            path of the "exe" (set by the script)
  FAKE_APP_SIGNED_FILE    the file the "signature" is valid for: a download must equal it
  FAKE_APP_VERSION        the version it reports (default 0.5.0)
  FAKE_APP_LIFETIME       seconds until it exits by itself (default 40)
  FAKE_APP_STALL_AFTER    seconds of silence it takes for a stalled download (default 3)
  FAKE_APP_SKIP_VERIFY=1  a bug to catch: accepts any download
  FAKE_APP_TOUCH_REAL=1   a bug to catch: also writes to the real %APPDATA% config
  FAKE_APP_IFACE_LINE     what the sniffer says when it is on (no --no-capture): the text of its system-event,
                          e.g. "Auto-Targeting Network Interface: 192.168.0.23 (default route)"
  FAKE_APP_RECT           the main window's first rect, "WxH" (default 800x600; also x,y = 100,100)
  FAKE_APP_SCREEN         the work area, "WxH" (default 1920x1040): a grow never goes beyond it
  FAKE_APP_K18_BUG        a bug to catch: "quit", "close" or "both" -- that way out saves the GROWN window
                          (what window-state would do if settings were still open at exit)
  FAKE_APP_BAD_STATUS=1   a bug to catch: a status file without pid and data_dir
  FAKE_APP_CHAT_BUG       a bug to catch in the chat rules: "no-dedupe" publishes a repeated line again, "no-retro" blocks
                          only later lines (earlier rows keep their flag), "block-later" never flags later lines of a blocked
                          sender, "clear-keeps" leaves the history when told to clear it
  FAKE_APP_PERSIST_BUG    a bug to catch in what survives a restart: "no-archive" writes no chat log, "world-archived" also archives WORLD
                          (ignored by default), "no-config" does not save the block list, "no-reload" starts with an empty log,
                          "pid-restart" numbers new lines from 1 again, "unflagged-reload" restores a blocked sender's rows unflagged
  FAKE_APP_SNIFF_BUG      a bug to catch in the capture: "drop-after-2" publishes only the first two chats it reads,
                          "never" does not start the sniffer at all
                          (the stand-in "sniffer" is a TCP client of the port-5003 server the capture-spike pipeline starts)
"""
from __future__ import annotations

import http.client
import json
import os
import shutil
import queue
import socket
import struct
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

SWITCHES = {"fresh", "assume-setup-done", "no-capture", "no-translator", "no-update-check", "no-popups",
            "no-window-state", "print-env"}
VALUES = {"data-dir", "feed-url", "metadata-url", "status-file", "log-file", "bridge-url"}


def parse(argv: list[str]) -> dict:
    flags: dict = {}
    it = iter(argv)
    for arg in it:
        name = arg[2:] if arg.startswith("--") else None
        if name in SWITCHES:
            flags[name] = True
        elif name in VALUES:
            flags[name] = next(it, "")
        else:
            print(f"resonance-stream: unknown test flag {arg}", file=sys.stderr)
            sys.exit(2)
    return flags


def version_tuple(text: str) -> tuple[int, ...]:
    return tuple(int(part) for part in text.split("."))


class Mqtt:
    """The least MQTT 3.1.1 a stand-in needs: connect (with a will), publish at QoS 0, subscribe, and hand every
    PUBLISH it receives to `inbox`. Not a library -- it only talks to the bridge's local broker."""

    def __init__(self, url: str, client_id: str, will_topic: str, will_payload: bytes) -> None:
        parts = urllib.parse.urlparse(url)
        self.sock = socket.create_connection((parts.hostname, parts.port), timeout=10)
        self.inbox: queue.Queue = queue.Queue()
        body = self._string("MQTT") + bytes([4, 0x2E]) + struct.pack(">H", 0) + self._string(client_id)
        body += self._string(will_topic) + struct.pack(">H", len(will_payload)) + will_payload
        self._send(0x10, body)
        if self._read_packet()[0] >> 4 != 2:
            raise OSError("no CONNACK from the broker")
        self.sock.settimeout(None)
        threading.Thread(target=self._reader, daemon=True).start()

    @staticmethod
    def _string(text: str) -> bytes:
        raw = text.encode("utf-8")
        return struct.pack(">H", len(raw)) + raw

    def _send(self, first: int, body: bytes) -> None:
        length, out = len(body), bytearray()
        while True:
            digit, length = length % 128, length // 128
            out.append(digit | (0x80 if length else 0))
            if not length:
                break
        self.sock.sendall(bytes([first]) + bytes(out) + body)

    def _recv(self, n: int) -> bytes:
        data = b""
        while len(data) < n:
            chunk = self.sock.recv(n - len(data))
            if not chunk:
                raise OSError("the broker closed the connection")
            data += chunk
        return data

    def _read_packet(self) -> tuple[int, bytes]:
        first = self._recv(1)[0]
        length, shift = 0, 0
        while True:
            digit = self._recv(1)[0]
            length |= (digit & 0x7F) << shift
            shift += 7
            if not digit & 0x80:
                break
        return first, self._recv(length)

    def _reader(self) -> None:
        try:
            while True:
                first, body = self._read_packet()
                if first >> 4 == 3:  # PUBLISH
                    (n,) = struct.unpack(">H", body[:2])
                    topic = body[2:2 + n].decode("utf-8")
                    rest = body[2 + n:]
                    if (first >> 1) & 3:
                        rest = rest[2:]  # a packet id: we subscribe at QoS 0, so the broker sends none
                    self.inbox.put((topic, rest))
        except OSError:
            pass

    def publish(self, topic: str, payload: bytes | str, retain: bool = False) -> None:
        data = payload.encode("utf-8") if isinstance(payload, str) else payload
        self._send(0x30 | (1 if retain else 0), self._string(topic) + data)

    def subscribe(self, topic_filter: str) -> None:
        self._send(0x82, struct.pack(">H", 1) + self._string(topic_filter) + bytes([0]))


def _varint(data: bytes, at: int) -> tuple[int, int]:
    value = shift = 0
    while at < len(data):
        byte = data[at]
        at += 1
        value |= (byte & 0x7F) << shift
        if not byte & 0x80:
            break
        shift += 7
    return value, at


def _fields(data: bytes):
    """(field number, value) of a protobuf message: an int for a varint, bytes for a length-delimited field."""
    at = 0
    while at < len(data):
        tag, at = _varint(data, at)
        kind, number = tag & 7, tag >> 3
        if kind == 0:
            value, at = _varint(data, at)
        elif kind == 2:
            size, at = _varint(data, at)
            value, at = data[at:at + size], at + size
        else:
            return
        yield number, value


def decode_chat_frame(frame: bytes) -> dict | None:
    """A live chat frame (`[len][0x0002][16-byte header][root]`) as the ChatMessage JSON the app publishes; None for the rest.
    The stand-in's own reading of the layout resonance_core::capture::synth writes."""
    if len(frame) < 23 or frame[4:6] != b"\x00\x02" or frame[22] != 0x0A:
        return None
    root_len, at = _varint(frame, 23)
    chat = {"channel": "WORLD", "nickname": "", "uid": 0, "level": 0, "message": ""}
    names = {1: "WORLD", 2: "LOCAL", 3: "PARTY", 4: "GUILD", 9: "BEGINNER"}
    for number, value in _fields(frame[at:at + root_len]):
        if number == 1 and isinstance(value, int):
            chat["channel"] = names.get(value, "WORLD")
        elif number == 2 and isinstance(value, bytes):
            for n2, v2 in _fields(value):
                if n2 == 1:
                    chat["sequenceId"] = v2
                elif n2 == 2:
                    for n3, v3 in _fields(v2):
                        if n3 == 1:
                            chat["uid"] = v3
                        elif n3 == 2:
                            chat["nickname"] = v3.decode("utf-8", "replace")
                        elif n3 == 5:
                            chat["level"] = v3
                elif n2 == 4:
                    for n3, v3 in _fields(v2):
                        if n3 == 3:
                            chat["message"] += v3.decode("utf-8", "replace")
    return chat if chat["message"] else None


class App:
    def __init__(self, flags: dict) -> None:
        self.flags = flags
        self.exe = Path(os.environ.get("FAKE_APP_EXE", sys.argv[0])).resolve()
        self.version = os.environ.get("FAKE_APP_VERSION", "0.5.0")
        data = flags.get("data-dir")
        self.status = {
            "version": self.version,
            "exe": str(self.exe),
            "pid": os.getpid(),
            "ready": False,
            "config_dir": str(Path(data) / "config") if data else None,
            "data_dir": str(Path(data) / "data") if data else None,
            "flags": [n for n in [*sorted(SWITCHES), *sorted(VALUES)] if n in flags],
            "update": "none",
        }
        w, _, h = os.environ.get("FAKE_APP_RECT", "800x600").partition("x")
        self.rect = {"x": 100, "y": 100, "width": int(w), "height": int(h)}
        self.grown_from: dict | None = None
        self.bridge: Mqtt | None = None
        self.seq = 0
        self.announced_url: str | None = None
        self.stop = False
        # the chat rules (sniffer/mod.rs, capture/message_processor.rs, events.rs, as understood)
        self.history: list[dict] = []
        self.signatures: dict[tuple, int] = {}
        self.fingerprints: list[tuple[tuple, float]] = []
        self.blocked: dict[int, str] = {}
        self.next_pid = 1

    def write_status(self) -> None:
        path = self.flags.get("status-file")
        if path:
            tmp = Path(path + ".tmp")
            tmp.parent.mkdir(parents=True, exist_ok=True)
            status = dict(self.status)
            if os.environ.get("FAKE_APP_BAD_STATUS") == "1" and "print-env" not in self.flags:  # a bug to catch: a status file missing keys
                status.pop("pid", None)
                status.pop("data_dir", None)
            tmp.write_text(json.dumps(status), encoding="utf-8")
            tmp.replace(path)

    def log(self, text: str) -> None:
        path = self.flags.get("log-file")
        if path:
            Path(path).parent.mkdir(parents=True, exist_ok=True)
            with open(path, "a", encoding="utf-8") as f:
                f.write(text + "\n")

    def set_update(self, state: str) -> None:
        self.status["update"] = state
        self.write_status()
        self.log(f"update -> {state}")
        self.event("update-state", {"state": state})

    # -- the bridge (src-tauri/src/bridge.rs, as understood)
    def event(self, name: str, payload: dict) -> None:
        if self.bridge is None:
            return
        body = {"seq": self.seq, "t_ms": int(time.time() * 1000), "name": name, "payload": payload}
        self.seq += 1
        self.bridge.publish(f"rs/app/event/{name}", json.dumps(body))

    def connect_bridge(self) -> None:
        url = self.flags.get("bridge-url")
        if not url:
            return
        self.bridge = Mqtt(url, "resonance-stream-app", "rs/app/status", b"offline")
        self.bridge.subscribe("rs/test/command/+")
        self.event("app-started", {"pid": os.getpid(), "version": self.version, "exe": str(self.exe)})
        self.bridge.publish("rs/app/status", "online", retain=True)

    # -- the sniffer (services/sniffer, as understood): a TCP client of the capture-spike pipeline's port-5003 server stands in for
    #    the raw socket on the adapter named by `network_interface` in config.json
    def start_sniffer(self) -> None:
        data = self.flags.get("data-dir")
        config = Path(data) / "config" / "config.json" if data else None
        ip = json.loads(config.read_text(encoding="utf-8")).get("network_interface") if config and config.exists() else None
        if not ip or os.environ.get("FAKE_APP_SNIFF_BUG") == "never":  # "never": a bug to catch, the sniffer does not start
            return
        self.event("sniffer-state", {"state": "Pending", "message": "Listening for game traffic...", "seq": 1})
        threading.Thread(target=self._sniff, args=(ip,), daemon=True).start()

    def _sniff(self, ip: str) -> None:
        deadline = time.monotonic() + 60
        sock = None
        while sock is None and time.monotonic() < deadline:
            try:
                sock = socket.create_connection((ip, 5003), timeout=2)
            except OSError:
                time.sleep(0.3)
        if sock is None:
            return
        buf, count = b"", 0
        while True:
            data = sock.recv(4096)
            if not data:
                return
            buf += data
            while len(buf) >= 6 and len(buf) >= int.from_bytes(buf[:4], "big"):
                size = int.from_bytes(buf[:4], "big")
                chat = decode_chat_frame(buf[:size])
                buf = buf[size:]
                if chat is None:
                    continue
                count += 1
                if os.environ.get("FAKE_APP_SNIFF_BUG") == "drop-after-2" and count > 2:
                    continue
                self.event("packet-event", {"pid": count, **chat})

    # -- the chat rules, fed by `replay-chat`
    def replay(self, path: str) -> None:
        lines = []
        for raw in Path(path).read_text(encoding="utf-8").splitlines():
            if raw.strip() and not raw.lstrip().startswith("#"):
                lines.append(json.loads(raw))
        threading.Thread(target=self._replay, args=(lines,), daemon=True).start()

    def _replay(self, lines: list[dict]) -> None:
        time.sleep(float(os.environ.get("FAKE_APP_LEAD_IN_MS", "300")) / 1000)
        for index, line in enumerate(lines):
            time.sleep(line.get("delay_ms", 0) / 1000)
            chat = {"channel": line.get("channel", "WORLD"), "nickname": line.get("nickname", "Tester"),
                    "uid": line.get("uid", 1), "level": line.get("level", 0), "message": line["text"],
                    "timestamp": line.get("timestamp", int(time.time())), "sequenceId": line.get("sequence_id", index + 1),
                    "isBlocked": False}
            self.feed(chat)
        self.event("system-event", {"pid": 0, "level": "info", "source": "Replay", "message": "Replay finished"})

    def feed(self, chat: dict) -> None:
        bug = os.environ.get("FAKE_APP_CHAT_BUG", "")
        if chat["uid"] in self.blocked and bug != "block-later":
            chat["isBlocked"] = True
        signature = (chat["uid"], chat["timestamp"], chat["sequenceId"])
        if signature in self.signatures and bug != "no-dedupe":
            if chat["isBlocked"]:  # the pipeline's UpdateBlockedMessage
                row = next((m for m in self.history if m["pid"] == self.signatures[signature]), None)
                if row and not row["isBlocked"]:
                    row["isBlocked"] = True
                    self.event("chat-message-update", row)
            return
        now = time.monotonic()
        fingerprint = (chat["uid"], chat["message"], chat["timestamp"])  # events.rs: the same line from a second client within 2 s
        self.fingerprints = [(f, t) for f, t in self.fingerprints if now - t <= 2]
        if any(f == fingerprint for f, _ in self.fingerprints) and bug != "no-dedupe":
            return
        self.fingerprints.append((fingerprint, now))
        chat["pid"] = self.next_pid
        self.next_pid += 1
        self.signatures[signature] = chat["pid"]
        self.history.append(chat)
        self.archive(chat)
        self.event("packet-event", chat)

    # -- what survives a restart (data_factory.rs, config/app_config.rs, lib.rs start-up reload, as understood)
    def data_path(self, *parts: str) -> Path | None:
        data = self.flags.get("data-dir")
        return Path(data).joinpath(*parts) if data else None

    def load_persisted(self) -> None:
        bug = os.environ.get("FAKE_APP_PERSIST_BUG", "")
        config = self.data_path("config", "config.json")
        if config and config.exists():
            saved = json.loads(config.read_text(encoding="utf-8"))
            self.blocked = {int(uid): name for uid, name in saved.get("blocked_users", {}).items()}
        logs = self.data_path("data", "chat_logs")
        if logs and logs.is_dir() and bug != "no-reload":
            for file in sorted(logs.glob("*.jsonl")):
                for raw in file.read_text(encoding="utf-8").splitlines():
                    row = json.loads(raw)
                    row["isBlocked"] = row["uid"] in self.blocked and bug != "unflagged-reload"
                    self.history.append(row)
            for pid, row in enumerate(self.history, start=1):  # load_recent: saved pids come from earlier runs, so 1..=n again
                row["pid"] = pid
            if bug != "pid-restart":
                self.next_pid = len(self.history) + 1
            for row in self.history:
                self.signatures[(row["uid"], row["timestamp"], row["sequenceId"])] = row["pid"]

    def archive(self, chat: dict) -> None:
        bug = os.environ.get("FAKE_APP_PERSIST_BUG", "")
        logs = self.data_path("data", "chat_logs")
        if logs is None or bug == "no-archive" or (chat["channel"] == "WORLD" and bug != "world-archived"):
            return
        logs.mkdir(parents=True, exist_ok=True)
        with open(logs / f"{time.strftime('%Y-%m-%d')}.jsonl", "a", encoding="utf-8") as f:
            f.write(json.dumps(chat, ensure_ascii=False) + "\n")

    def save_blocked(self) -> None:
        config = self.data_path("config", "config.json")
        if config is None or os.environ.get("FAKE_APP_PERSIST_BUG") == "no-config":
            return
        saved = json.loads(config.read_text(encoding="utf-8")) if config.exists() else {}
        saved["blocked_users"] = {str(uid): name for uid, name in self.blocked.items()}
        config.parent.mkdir(parents=True, exist_ok=True)
        config.write_text(json.dumps(saved), encoding="utf-8")

    def set_blocked(self, uid: int, blocked: bool, nickname: str = "") -> None:
        if blocked:
            self.blocked[uid] = nickname
        else:
            self.blocked.pop(uid, None)
        self.save_blocked()
        if os.environ.get("FAKE_APP_CHAT_BUG") == "no-retro":
            return
        for row in self.history:
            if row["uid"] == uid and row["isBlocked"] != blocked:
                row["isBlocked"] = blocked
                self.event("chat-message-update", row)

    # -- the main window (window.rs and the window-state plugin, as understood)
    def state_file(self) -> Path | None:
        data = self.flags.get("data-dir")
        return Path(data) / "config" / "window-state.json" if data and "no-window-state" not in self.flags else None

    def load_window_state(self) -> None:
        path = self.state_file()
        if path and path.exists():
            self.rect = json.loads(path.read_text(encoding="utf-8"))

    def leave(self, way: str) -> None:
        """The app is closing by `way` ("quit": the tray's Quit, "close": the X): put a grown window back, save its state."""
        buggy = os.environ.get("FAKE_APP_K18_BUG") in (way, "both")
        if self.grown_from is not None and not buggy:
            self.rect, self.grown_from = self.grown_from, None
        path = self.state_file()
        if path:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(json.dumps(self.rect), encoding="utf-8")
        self.stop = True

    def handle(self, topic: str, payload: bytes) -> None:
        request = json.loads(payload)
        command, id_ = topic.rsplit("/", 1)[-1], request.get("id", "")

        def ack(error: str | None = None, data=None) -> None:
            body = {"id": id_, "ok": True} if error is None else {"id": id_, "ok": False, "error": error}
            if error is None and data is not None:
                body["data"] = data
            self.bridge.publish(f"rs/app/ack/{id_}", json.dumps(body))

        if command == "ping":
            ack()
        elif command == "start-update":
            if self.announced_url is None:
                ack("No update has been announced; check for updates first")
            else:
                ack()
                threading.Thread(target=self.download, args=(self.announced_url,), daemon=True).start()
        elif command == "restart-update":
            error = self.restart()  # a success ends this process
            ack(error or "restart returned without error")
        elif command == "snapshot":
            body = {"id": id_, "ok": True, "data": dict(self.rect)}
            self.bridge.publish(f"rs/app/ack/{id_}", json.dumps(body))
        elif command == "grow-window":
            screen_w, _, screen_h = os.environ.get("FAKE_APP_SCREEN", "1920x1040").partition("x")
            # like grow_to_fit: at least what was asked, but never more than the work area has
            want_w = min(int(request["min_width"]), int(screen_w))
            want_h = min(int(request["min_height"]), int(screen_h))
            old = dict(self.rect)
            if want_w > old["width"] or want_h > old["height"]:
                self.grown_from = self.grown_from or old
                self.rect["width"], self.rect["height"] = max(want_w, old["width"]), max(want_h, old["height"])
                self.bridge.publish(f"rs/app/ack/{id_}", json.dumps({"id": id_, "ok": True, "data": old}))
            else:  # already big enough: nothing changed (data null)
                self.bridge.publish(f"rs/app/ack/{id_}", json.dumps({"id": id_, "ok": True, "data": None}))
        elif command == "replay-chat":
            try:
                self.replay(request["path"])
                ack()
            except (OSError, ValueError, KeyError) as e:
                ack(f"replay failed: {e}")
        elif command == "block-user":
            self.set_blocked(int(request["uid"]), True, request.get("nickname", ""))
            ack()
        elif command == "unblock-user":
            self.set_blocked(int(request["uid"]), False)
            ack()
        elif command == "get-chat-history":
            ack(data=[dict(m) for m in self.history])
        elif command == "clear-history":
            if os.environ.get("FAKE_APP_CHAT_BUG") != "clear-keeps":
                self.history.clear()
            ack()
        elif command == "close-window":
            ack()
            time.sleep(0.2)
            self.leave("close")
        elif command == "quit":
            ack()
            time.sleep(0.2)
            self.leave("quit")
        else:
            ack(f"unknown command {command}")

    # -- the update path (app_updater.rs / gist.rs, as understood)
    def check(self) -> str | None:
        """Returns the announced download url when a newer version is on offer."""
        try:
            urllib.request.urlopen(self.flags["metadata-url"], timeout=5).read()  # noqa: S310 -- local
        except (OSError, KeyError, ValueError):
            self.log("check failed: metadata")
            return None
        try:
            body = urllib.request.urlopen(self.flags["feed-url"], timeout=5).read()  # noqa: S310
        except urllib.error.HTTPError:
            self.set_update("none")  # the real app reports the finished check, whatever it found
            return None
        except (OSError, KeyError, ValueError) as e:
            self.set_update(f"error:Network error: {e}")
            return None
        try:
            feed = json.loads(body)
            version, url, signature = feed["version"], feed["url"], feed["signature"]
        except (ValueError, KeyError, TypeError):
            self.set_update("error:the update feed is not JSON")
            return None
        if version_tuple(version) > version_tuple(self.version):
            self.set_update(f"available:{version}")
            return url
        self.set_update("none")
        return None

    def download(self, url: str) -> bool:
        temp = self.exe.with_name("update_temp.exe")
        part = self.exe.with_name("update_temp.exe.part")
        self.set_update("downloading")
        try:
            with urllib.request.urlopen(url, timeout=float(os.environ.get("FAKE_APP_STALL_AFTER", "3"))) as r:  # noqa: S310
                data = r.read()
        except (http.client.IncompleteRead, socket.timeout, TimeoutError, OSError) as e:
            part.unlink(missing_ok=True)
            reason = "no data for the stall limit" if isinstance(e, (socket.timeout, TimeoutError)) else str(e)
            self.set_update(f"error:{reason}")
            return False
        if not self.verified(data):
            self.set_update("error:the update is not signed by a trusted key")
            return False
        temp.write_bytes(data)
        self.set_update("downloaded")
        return True

    @staticmethod
    def verified(data: bytes) -> bool:
        if os.environ.get("FAKE_APP_SKIP_VERIFY") == "1":
            return True
        signed = os.environ.get("FAKE_APP_SIGNED_FILE")
        return bool(signed) and Path(signed).read_bytes() == data

    def restart(self) -> str:
        """Installs the downloaded update and restarts (this process ends); returns why it refused."""
        temp = self.exe.with_name("update_temp.exe")
        if not temp.exists():
            return "No verified update has been downloaded"
        if not self.verified(temp.read_bytes()):
            self.log("restart refused: update_temp.exe fails the check")
            return "the update is not signed by a trusted key"
        old = self.exe.with_name(self.exe.name + ".old")
        old.unlink(missing_ok=True)
        self.exe.rename(old)
        temp.rename(self.exe)
        self.exe.chmod(0o755)  # a downloaded file is not executable on POSIX; the real exe is
        args = [a for a in sys.argv[1:] if a not in ("--fresh", "--print-env")]
        subprocess.Popen([str(self.exe), *args], cwd=str(self.exe.parent), close_fds=True, env=os.environ.copy())
        os._exit(0)


def main() -> int:
    flags = parse(sys.argv[1:])
    app = App(flags)
    deadline = time.monotonic() + float(os.environ.get("FAKE_APP_LIFETIME", "40"))
    app.write_status()
    if "print-env" in flags:
        print(json.dumps(app.status))
        return 0
    if os.environ.get("FAKE_APP_TOUCH_REAL") == "1" and os.environ.get("APPDATA"):
        real = Path(os.environ["APPDATA"]) / "com.enjay.bpsr.resonance-stream" / "config.json"
        real.parent.mkdir(parents=True, exist_ok=True)
        real.write_text(json.dumps({"touched": time.time()}), encoding="utf-8")
    data = flags.get("data-dir")
    if data:
        if "fresh" in flags:
            for sub in ("config", "data", "webview"):
                shutil.rmtree(Path(data) / sub, ignore_errors=True)
        for sub in ("config", "data", "webview"):
            (Path(data) / sub).mkdir(parents=True, exist_ok=True)
    app.status["ready"] = True
    app.write_status()
    app.load_window_state()
    app.load_persisted()
    app.connect_bridge()
    line = os.environ.get("FAKE_APP_IFACE_LINE")
    if line and "no-capture" not in flags:
        level = "error" if line.startswith("NETWORK_ERROR") else "info"
        app.event("system-event", {"pid": 1, "level": level, "source": "Sniffer", "message": line})
    if "no-capture" not in flags:
        app.start_sniffer()
    app.announced_url = None if "no-update-check" in flags else app.check()
    while time.monotonic() < deadline and not app.stop:
        if app.bridge is None:
            time.sleep(0.5)
            continue
        try:
            topic, payload = app.bridge.inbox.get(timeout=0.5)
        except queue.Empty:
            continue
        app.handle(topic, payload)
    return 0


if __name__ == "__main__":
    sys.exit(main())
