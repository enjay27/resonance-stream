"""Pipeline `persistence`: what is still there after the app is closed and started again on the same data folder.

Round 1 (a fresh data folder): replay chat in three channels -- one of them WORLD, which the archive ignores by default --
block a sender, ask for the chat log, and leave with `quit`. Then the files are read from disk. Round 2 starts the app again on the same
folder. Rows:

  CP-archive       the day's chat log (`data/chat_logs/*.jsonl`) holds the GUILD and PARTY lines, not the WORLD one
  CP-config        `config/config.json` holds the block list
  CP-reload        the chat log the second start serves (`get-chat-history`) is the archived lines, oldest first
  CP-block-reload  the blocked sender's restored rows are still flagged
  CP-pid           a line that arrives after the restart gets a pid above every restored one, and a blocked sender's new line is
                   flagged (the block list was read back)
"""
from __future__ import annotations

import json
import time
from pathlib import Path

from runbook import bridge, common, mockfeed, updater
from runbook.common import Recorder

ALICE, BOB = 1001, 2002
GRACE_S = 2  # the archive writer is a thread of its own


def line(channel: str, nickname: str, uid: int, seq: int, text: str, ts: int, delay_ms: int = 50) -> str:
    return json.dumps({"delay_ms": delay_ms, "channel": channel, "nickname": nickname, "uid": uid, "level": 60,
                       "timestamp": ts, "sequence_id": seq, "text": text}, ensure_ascii=False)


def first_replay(ts: int) -> str:
    return "\n".join([
        line("GUILD", "Alice", ALICE, 1, "guild one", ts, 0),
        line("WORLD", "Bob", BOB, 2, "world one", ts),
        line("PARTY", "Bob", BOB, 3, "party one", ts),
        line("GUILD", "Alice", ALICE, 4, "guild two", ts),
    ]) + "\n"


def second_replay(ts: int) -> str:
    return line("GUILD", "Alice", ALICE, 5, "guild after restart", ts + 5, 0) + "\n"


ARCHIVED = ["guild one", "party one", "guild two"]


class Persistence:
    def __init__(self, rec: Recorder, exe: str | Path, runs: Path | None = None) -> None:
        self.rec = rec
        self.exe = Path(exe)
        self.runs = Path(runs or common.RUNS) / "persistence"
        self._serves: list[bridge.Serve] = []

    def run(self, key_path: str | None = None, password: str | None = None) -> None:
        """(`key_path` and `password` are for the pipelines that sign; this one ignores them.)"""
        with updater.step(self.rec, "CP", self.runs, strays=self.runs, stop=self._stop):
            self.round()

    def _stop(self, folder: str | Path) -> list[int]:
        for serve in self._serves:
            serve.stop()
        self._serves.clear()
        return mockfeed.stop_copies(folder)

    def start(self, label: str, exe: Path, *, fresh: bool) -> bridge.Serve | None:
        events = bridge.Serve(exe.parent / f"{label}-events.jsonl")
        self._serves.append(events)
        args = mockfeed.flag_args(exe.parent / "data", exe.parent / f"{label}-status.json", log_file=exe.parent / f"{label}.log",
                                  fresh=fresh, extra=("--no-update-check", "--bridge-url", events.url))
        mockfeed.start_app(exe, args)
        return events if bridge.wait_started(events, exe.parent / f"{label}.log", label=f"the app ({label})") else None

    @staticmethod
    def logged(exe: Path) -> list[dict]:
        rows = []
        for file in sorted((exe.parent / "data" / "data" / "chat_logs").glob("*.jsonl")):
            rows += [json.loads(raw) for raw in file.read_text(encoding="utf-8").splitlines() if raw.strip()]
        return rows

    def replay(self, events: bridge.Serve, path: Path, text: str, last: str) -> bool:
        path.write_text(text, encoding="utf-8")
        events.send("replay-chat", {"path": str(path)})
        try:
            events.expect("packet-event", {"payload.message": last}, timeout=30)
            return True
        except RuntimeError:
            return False

    def round(self) -> None:
        exe = updater.fresh_copy(self.exe, self.runs, "run")
        first = self.start("first", exe, fresh=True)
        self.rec.auto("CP-start", "the app started and connected to the bridge", first is not None,
                      "" if first else "no app-started event in 120 s")
        if first is None:
            return
        ts = int(time.time())
        if not self.replay(first, exe.parent / "first.jsonl", first_replay(ts), "guild two"):
            self.rec.auto("CP-archive", "the chat log holds what was said", False, "the replay never arrived")
            return
        first.send("block-user", {"uid": ALICE, "nickname": "Alice"})
        time.sleep(GRACE_S)
        shown = [m["message"] for m in first.send("get-chat-history")["data"]]
        try:
            first.send("quit", timeout=15)
            first.expect("rs/app/status", "offline", timeout=60)
        except RuntimeError as e:
            self.rec.auto("CP-quit", "the first run ended when told to", False, str(e))
            return
        time.sleep(1)

        logged = [row["message"] for row in self.logged(exe)]
        self.rec.auto("CP-archive", "the chat log holds the GUILD and PARTY lines and not the WORLD one", logged == ARCHIVED,
                      f"on disk: {logged}; the run showed {shown}")
        try:
            saved = json.loads((exe.parent / "data" / "config" / "config.json").read_text(encoding="utf-8"))
        except (OSError, ValueError) as e:
            saved = {"error": str(e)}
        self.rec.auto("CP-config", "config.json holds the block list", saved.get("blocked_users") == {str(ALICE): "Alice"},
                      f"blocked_users: {saved.get('blocked_users', saved)}")

        second = self.start("second", exe, fresh=False)
        if second is None:
            self.rec.auto("CP-reload", "the app started again on the same data folder", False, "no app-started event in 120 s")
            return
        history = second.send("get-chat-history")["data"]
        self.rec.auto("CP-reload", "after a restart the chat log is the archived lines, oldest first",
                      [m["message"] for m in history] == ARCHIVED, f"served: {[m['message'] for m in history]}")
        flags = {m["message"]: m["isBlocked"] for m in history}
        self.rec.auto("CP-block-reload", "the blocked sender's restored rows are still flagged",
                      flags == {"guild one": True, "party one": False, "guild two": True}, f"flags: {flags}")
        top = max((m["pid"] for m in history), default=0)
        if not self.replay(second, exe.parent / "second.jsonl", second_replay(ts), "guild after restart"):
            self.rec.auto("CP-pid", "a line after the restart is numbered after the restored ones", False, "the line never arrived")
            return
        new = [p for p in self.packets(second) if p["message"] == "guild after restart"][0]
        self.rec.auto("CP-pid", "a line after the restart gets a higher pid, and a blocked sender's new line is flagged",
                      new["pid"] > top and new["isBlocked"] is True, f"pid {new['pid']} (restored up to {top}), isBlocked {new['isBlocked']}")
        try:
            second.send("quit", timeout=15)
        except RuntimeError:
            pass

    @staticmethod
    def packets(events: bridge.Serve) -> list[dict]:
        return [m["message"]["payload"] for m in events.events() if m["topic"] == "rs/app/event/packet-event"]
