"""Pipeline `chat-rules`: what the backend does with chat lines, tried with no game.

Lines go in through `replay-chat` (they enter at the same place a captured line does, after the parser), commands come in
over the bridge (`block-user`, `unblock-user`, `get-chat-history`, `clear-history`: the ones the chat row's menu and the clear button
call), and what comes out is read from the events the app publishes. Rows:

  CR-dedupe       a line sent twice (same sender, time and sequence) and the same line from a second client (same sender, text
                  and time, another sequence) are each shown once
  CR-history      the backend's chat log is what was published, in order (what the window reads when it starts)
  CR-block        blocking a sender flags the rows already there (`chat-message-update`) and no other sender's
  CR-block-later  a line from a blocked sender that arrives afterwards is flagged
  CR-unblock      unblocking takes the flag off again
  CR-clear        clearing the history empties the log

`min_sender_level` is the window's filter, not the backend's (the backend publishes every line), so it is not tested here.
"""
from __future__ import annotations

import json
import time
from pathlib import Path

from runbook import bridge, common, mockfeed, updater
from runbook.common import Recorder

ALICE, BOB = 1001, 2002


def line(nickname: str, uid: int, seq: int, text: str, ts: int, delay_ms: int = 50) -> str:
    return json.dumps({"delay_ms": delay_ms, "channel": "WORLD", "nickname": nickname, "uid": uid, "level": 60,
                       "timestamp": ts, "sequence_id": seq, "text": text}, ensure_ascii=False)


def first_replay(ts: int) -> str:
    return "\n".join([
        line("Alice", ALICE, 1, "A first", ts, 0),
        line("Alice", ALICE, 1, "A first", ts),   # the same line again: the pipeline's duplicate check
        line("Alice", ALICE, 9, "A first", ts),   # a second client's copy: another sequence, same sender / text / time
        line("Bob", BOB, 2, "B first", ts),
    ]) + "\n"


def second_replay(ts: int) -> str:
    return "\n".join([line("Alice", ALICE, 3, "A after block", ts + 1, 0), line("Bob", BOB, 4, "B after", ts + 1)]) + "\n"


class ChatRules:
    def __init__(self, rec: Recorder, exe: str | Path, runs: Path | None = None) -> None:
        self.rec = rec
        self.exe = Path(exe)
        self.runs = Path(runs or common.RUNS) / "chat-rules"
        self._serves: list[bridge.Serve] = []

    def run(self, key_path: str | None = None, password: str | None = None) -> None:
        """(`key_path` and `password` are for the pipelines that sign; this one ignores them.)"""
        with updater.step(self.rec, "CR", self.runs, strays=self.runs, stop=self._stop):
            self.round()

    def _stop(self, folder: str | Path) -> list[int]:
        for serve in self._serves:
            serve.stop()
        self._serves.clear()
        return mockfeed.stop_copies(folder)

    @staticmethod
    def packets(events: bridge.Serve) -> list[dict]:
        return [m["message"]["payload"] for m in events.events() if m["topic"] == "rs/app/event/packet-event"]

    @staticmethod
    def flags_of(rows: list[dict]) -> dict[str, bool]:
        return {row["message"]: row["isBlocked"] for row in rows}

    def round(self) -> None:
        exe = updater.fresh_copy(self.exe, self.runs, "run")
        events = bridge.Serve(exe.parent / "events.jsonl")
        self._serves.append(events)
        args = mockfeed.flag_args(exe.parent / "data", exe.parent / "status.json", log_file=exe.parent / "app.log",
                                  extra=("--no-update-check", "--bridge-url", events.url))
        mockfeed.start_app(exe, args)
        if not bridge.wait_started(events, exe.parent / "app.log"):
            self.rec.auto("CR-start", "the app started and connected to the bridge", False, "no app-started event in 120 s")
            return
        ts = int(time.time())
        first = exe.parent / "first.jsonl"
        first.write_text(first_replay(ts), encoding="utf-8")
        events.send("replay-chat", {"path": str(first)})
        try:
            events.expect("system-event", {"payload.message": "Replay finished"}, timeout=30)
        except RuntimeError:
            self.rec.auto("CR-dedupe", "a repeated line is shown once", False, "the replay never finished")
            return

        shown = [p["message"] for p in self.packets(events)]
        self.rec.auto("CR-dedupe", "a line sent twice, and the same line from a second client, are each shown once",
                      shown == ["A first", "B first"], f"published: {shown} (wanted ['A first', 'B first'])")
        history = events.send("get-chat-history")["data"]
        published = [(p["pid"], p["message"]) for p in self.packets(events)]
        self.rec.auto("CR-history", "the backend's chat log is what was published, in order",
                      [(m["pid"], m["message"]) for m in history] == published,
                      f"log {[(m['pid'], m['message']) for m in history]} / published {published}")

        events.send("block-user", {"uid": ALICE, "nickname": "Alice"})
        flagged = self.wait_update(events, ALICE, True)
        history = events.send("get-chat-history")["data"]
        self.rec.auto("CR-block", "blocking a sender flags the rows already there, and only theirs",
                      flagged and self.flags_of(history) == {"A first": True, "B first": False},
                      f"update seen: {flagged}; flags {self.flags_of(history)}")

        second = exe.parent / "second.jsonl"
        second.write_text(second_replay(ts), encoding="utf-8")
        events.send("replay-chat", {"path": str(second)})
        try:
            events.expect("packet-event", {"payload.message": "B after"}, timeout=30)
        except RuntimeError:
            self.rec.auto("CR-block-later", "a later line from a blocked sender is flagged", False, "the second replay never arrived")
            return
        later = {p["message"]: p["isBlocked"] for p in self.packets(events)}
        self.rec.auto("CR-block-later", "a later line from a blocked sender is flagged, another sender's is not",
                      later.get("A after block") is True and later.get("B after") is False, f"flags {later}")

        events.send("unblock-user", {"uid": ALICE})
        cleared = self.wait_update(events, ALICE, False)
        history = events.send("get-chat-history")["data"]
        self.rec.auto("CR-unblock", "unblocking takes the flag off the sender's rows",
                      cleared and not any(m["isBlocked"] for m in history),
                      f"update seen: {cleared}; flags {self.flags_of(history)}")

        events.send("clear-history")
        left = events.send("get-chat-history")["data"]
        self.rec.auto("CR-clear", "clearing the history empties the backend's log", left == [], f"{len(left)} rows left")
        try:
            events.send("quit", timeout=15)
        except RuntimeError:
            pass

    @staticmethod
    def wait_update(events: bridge.Serve, uid: int, blocked: bool) -> bool:
        try:
            events.expect("chat-message-update", {"payload.uid": uid, "payload.isBlocked": blocked}, timeout=10)
            return True
        except RuntimeError:
            return False
