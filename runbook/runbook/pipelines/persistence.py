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
  CP-version       the version the app announces is the workspace version of this checkout (a stale exe says so here)

Favorites (`save-favorites` is what the favorites popup sends; the favorites are saved before the block, so the block must not undo them):

  CP-fav-save      `get-favorites` gives back what was saved: messages in order with their notes, shortcuts and tab ids; tabs in order
                   with ids that are not 1..n (a renumbering would show)
  CP-fav-event     `favorites-changed` was published once, with that state
  CP-fav-block     after `block-user` (another config write) the favorites are as they were
  CP-fav-config    `config.json` holds `favorite_messages` and `favorite_tabs` as saved, next to the block list
  CP-fav-reload    after a restart the favorites are the same, tab ids included

Then, on the same folder, with `chat_log_retention_days` set to 2 and old day logs put there by hand:

  CP-retention         the day logs older than that are deleted at start-up; yesterday's and today's stay; a file that is no day log
                       and the legacy `dataset_raw.jsonl` are not touched
  CP-retention-served  the chat log the app serves has yesterday's line and none from the deleted days

And on a folder of its own, WORLD archived and `tab_limits.WORLD` = 10, five GUILD lines then forty WORLD lines:

  CP-busy-world        after a restart all five GUILD lines are back and only the newest ten WORLD lines (a busy WORLD chat does not
                       push GUILD out of the reload, nor does WORLD overflow its own limit)

And on a folder of its own, `tab_limits.GUILD` = 400 and 300 GUILD lines replayed:

  CP-big-ack           `get-chat-history` answers with all 300 lines: the answer is far over 10 KiB, the MQTT client's default packet limit,
                       which once dropped it (smoke run of #200: 108005 bytes, CS-restart-nodup)
"""
from __future__ import annotations

import datetime
import json
import re
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
# Tab ids are not 1..n and not in order, so a renumbering or a re-sort shows; one shortcut that a PC does not use for anything else.
FAVORITES = {
    "messages": [
        {"text": "こんにちは！", "note": "안녕하세요", "shortcut": "", "tab": 0},
        {"text": "ボス戦行きます", "note": "보스전 갑니다", "shortcut": "Ctrl+Alt+F8", "tab": 12},
        {"text": "ありがとう", "note": "", "shortcut": "", "tab": 7},
    ],
    "tabs": [{"id": 12, "name": "Boss"}, {"id": 7, "name": "Thanks"}],
}
KEEP_DAYS = 2  # retention: today and yesterday stay
BUSY_WORLD_LIMIT = 10
BUSY_WORLD_LINES = 40
BUSY_GUILD_LINES = 5
BIG_LINES = 300  # about 60 KB of JSON in one answer; the MQTT client's default limit is 10 KiB
REPO = Path(__file__).resolve().parents[3]


def workspace_version() -> str | None:
    """`[workspace.package] version` of this checkout's Cargo.toml (what a freshly built exe announces), None when there is no checkout."""
    try:
        text = (REPO / "Cargo.toml").read_text(encoding="utf-8")
    except OSError:
        return None
    found = re.search(r'^\[workspace\.package\][^\[]*?^version\s*=\s*"([^"]+)"', text, re.MULTILINE | re.DOTALL)
    return found.group(1) if found else None


def busy_replay(ts: int) -> tuple[str, list[str], list[str]]:
    """Five GUILD lines first, then a flood of WORLD lines: the file, the GUILD texts, the WORLD texts."""
    guild = [f"guild {n}" for n in range(1, BUSY_GUILD_LINES + 1)]
    world = [f"world {n}" for n in range(1, BUSY_WORLD_LINES + 1)]
    lines = [line("GUILD", "Alice", ALICE, n, text, ts, 0 if n == 1 else 10) for n, text in enumerate(guild, start=1)]
    lines += [line("WORLD", "Bob", BOB, 100 + n, text, ts, 5) for n, text in enumerate(world, start=1)]
    return "\n".join(lines) + "\n", guild, world


class Persistence:
    def __init__(self, rec: Recorder, exe: str | Path, runs: Path | None = None) -> None:
        self.rec = rec
        self.exe = Path(exe)
        self.runs = Path(runs or common.RUNS) / "persistence"
        self._serves: list[bridge.Serve] = []
        self.started: dict[str, dict] = {}

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
        payload = bridge.wait_started(events, exe.parent / f"{label}.log", label=f"the app ({label})")
        self.started[label] = payload or {}
        return events if payload else None

    def leave(self, events: bridge.Serve) -> bool:
        """`quit` and wait until the app is gone, so the next start finds the folder free."""
        try:
            events.send("quit", timeout=15)
            events.expect("rs/app/status", "offline", timeout=60)
        except RuntimeError:
            return False
        time.sleep(1)
        return True

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

    def favorites_saved(self, events: bridge.Serve) -> None:
        """Save `FAVORITES` as the popup does and judge what the app gives back and what it announced."""
        began = time.time() * 1000
        events.send("save-favorites", {"favorites": FAVORITES})
        told = []
        for _ in range(20):  # the event follows the saved file; a second or two is plenty
            told = [m["message"]["payload"] for m in events.events()
                    if m["topic"] == "rs/app/event/favorites-changed" and m["received_at"] >= began]
            if told:
                break
            time.sleep(0.25)
        time.sleep(0.5)  # a second, doubled event would come right behind
        told = [m["message"]["payload"] for m in events.events()
                if m["topic"] == "rs/app/event/favorites-changed" and m["received_at"] >= began]
        got = events.send("get-favorites")["data"]
        self.rec.auto("CP-fav-save", "get-favorites gives back what was saved, tab ids and order included", got == FAVORITES, f"got: {got}")
        self.rec.auto("CP-fav-event", "favorites-changed was published once, with the saved state", told == [FAVORITES],
                      f"{len(told)} event(s): {told}")

    def round(self) -> None:
        exe = updater.fresh_copy(self.exe, self.runs, "run")
        first = self.start("first", exe, fresh=True)
        self.rec.auto("CP-start", "the app started and connected to the bridge", first is not None,
                      "" if first else "no app-started event in 120 s")
        if first is None:
            return
        expected, announced = workspace_version(), self.started["first"].get("version")
        if expected is None:
            self.rec.record("CP-version", "the app announces this checkout's version", "skip", "no Cargo.toml next to the runbook")
        else:
            self.rec.auto("CP-version", "the app announces this checkout's version", announced == expected,
                          f"the app says {announced}, Cargo.toml says {expected}" + ("" if announced == expected else " -- a stale exe? pull and rebuild"))
        ts = int(time.time())
        if not self.replay(first, exe.parent / "first.jsonl", first_replay(ts), "guild two"):
            self.rec.auto("CP-archive", "the chat log holds what was said", False, "the replay never arrived")
            return
        self.favorites_saved(first)
        first.send("block-user", {"uid": ALICE, "nickname": "Alice"})
        time.sleep(GRACE_S)
        self.rec.auto("CP-fav-block", "a later config write (the block) leaves the favorites as they were",
                      first.send("get-favorites")["data"] == FAVORITES, f"favorites now: {first.send('get-favorites')['data']}")
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
        on_disk = {"messages": saved.get("favorite_messages"), "tabs": saved.get("favorite_tabs")}
        self.rec.auto("CP-fav-config", "config.json holds the favorites as saved, next to the block list", on_disk == FAVORITES,
                      f"on disk: {on_disk}")

        second = self.start("second", exe, fresh=False)
        if second is None:
            self.rec.auto("CP-reload", "the app started again on the same data folder", False, "no app-started event in 120 s")
            return
        history = second.send("get-chat-history")["data"]
        self.rec.auto("CP-reload", "after a restart the chat log is the archived lines, oldest first",
                      [m["message"] for m in history] == ARCHIVED, f"served: {[m['message'] for m in history]}")
        again = second.send("get-favorites")["data"]
        self.rec.auto("CP-fav-reload", "after a restart the favorites are the same, tab ids included", again == FAVORITES, f"served: {again}")
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
        if not self.leave(second):
            self.rec.auto("CP-retention", "the old day logs are deleted", False, "the second run did not end when told to")
            return
        self.retention(exe, ts)
        self.busy_world(ts)
        self.big_answer(ts)

    @staticmethod
    def day_file(exe: Path, day: datetime.date) -> Path:
        return exe.parent / "data" / "data" / "chat_logs" / f"{day.isoformat()}.jsonl"

    def retention(self, exe: Path, ts: int) -> None:
        """`chat_log_retention_days` = 2, day logs of today - 1 (stays), today - 2 and a long time ago (go), a file that is no
        day log, and the legacy `dataset_raw.jsonl`; then a start."""
        template = self.logged(exe)[0]
        today = datetime.date.today()
        old = {today - datetime.timedelta(days=1): "from yesterday", today - datetime.timedelta(days=2): "two days ago",
               datetime.date(2020, 1, 1): "long ago"}
        for day, text in old.items():
            row = {**template, "message": text, "timestamp": ts - 86400, "sequenceId": 900 + day.toordinal() % 100}
            self.day_file(exe, day).write_text(json.dumps(row, ensure_ascii=False) + "\n", encoding="utf-8")
        logs = self.day_file(exe, today).parent
        (logs / "notes.jsonl").write_text("not a day log\n", encoding="utf-8")
        dataset = exe.parent / "data" / "data" / "dataset_raw.jsonl"
        dataset.write_text('{"legacy": true}\n', encoding="utf-8")
        config_path = exe.parent / "data" / "config" / "config.json"
        saved = json.loads(config_path.read_text(encoding="utf-8"))
        config_path.write_text(json.dumps({**saved, "chat_log_retention_days": KEEP_DAYS}), encoding="utf-8")

        third = self.start("third", exe, fresh=False)
        if third is None:
            self.rec.auto("CP-retention", "the old day logs are deleted", False, "no app-started event in 120 s")
            return
        names = sorted(f.name for f in logs.glob("*"))
        want = sorted([f"{today.isoformat()}.jsonl", f"{today - datetime.timedelta(days=1)}.jsonl", "notes.jsonl"])
        untouched = dataset.exists() and dataset.read_text(encoding="utf-8") == '{"legacy": true}\n'
        self.rec.auto("CP-retention", f"day logs older than {KEEP_DAYS} days are deleted at start-up, nothing else is",
                      names == want and untouched, f"in chat_logs: {names}; wanted {want}; dataset_raw.jsonl untouched: {untouched}")
        served = [m["message"] for m in third.send("get-chat-history")["data"]]
        self.rec.auto("CP-retention-served", "the served chat log has yesterday's line and none from the deleted days",
                      "from yesterday" in served and not {"two days ago", "long ago"} & set(served), f"served: {served}")
        self.leave(third)

    def busy_world(self, ts: int) -> None:
        """A folder of its own with WORLD archived and a WORLD limit of 10: a flood of WORLD lines after five GUILD lines."""
        exe = updater.fresh_copy(self.exe, self.runs, "busy")
        config_path = exe.parent / "data" / "config" / "config.json"
        config_path.parent.mkdir(parents=True)
        config_path.write_text(json.dumps({"archive_ignored_channels": [], "tab_limits": {"WORLD": BUSY_WORLD_LIMIT}}), encoding="utf-8")
        text, guild, world = busy_replay(ts)
        first = self.start("busy-first", exe, fresh=False)
        if first is None or not self.replay(first, exe.parent / "busy.jsonl", text, world[-1]):
            self.rec.auto("CP-busy-world", "a busy WORLD chat does not push GUILD out of the reload", False,
                          "the app did not start" if first is None else "the replay never arrived")
            return
        time.sleep(GRACE_S)
        if not self.leave(first):
            self.rec.auto("CP-busy-world", "a busy WORLD chat does not push GUILD out of the reload", False, "the first run did not end when told to")
            return
        second = self.start("busy-second", exe, fresh=False)
        if second is None:
            self.rec.auto("CP-busy-world", "a busy WORLD chat does not push GUILD out of the reload", False, "no app-started event in 120 s")
            return
        history = second.send("get-chat-history")["data"]
        got_guild = [m["message"] for m in history if m["channel"] == "GUILD"]
        got_world = [m["message"] for m in history if m["channel"] == "WORLD"]
        self.rec.auto("CP-busy-world", f"after a restart all GUILD lines and the newest {BUSY_WORLD_LIMIT} WORLD lines are back",
                      got_guild == guild and got_world == world[-BUSY_WORLD_LIMIT:],
                      f"GUILD {len(got_guild)} of {len(guild)}, WORLD {len(got_world)} (wanted the newest {BUSY_WORLD_LIMIT}: {world[-BUSY_WORLD_LIMIT]} .. {world[-1]}); got {got_world[:1]} .. {got_world[-1:]}")
        self.leave(second)

    def big_answer(self, ts: int) -> None:
        """A folder of its own: 300 GUILD lines, then one `get-chat-history` whose answer is several times the MQTT client's 10 KiB default."""
        what = "get-chat-history answers with a whole busy chat, however big the answer"
        exe = updater.fresh_copy(self.exe, self.runs, "big")
        config_path = exe.parent / "data" / "config" / "config.json"
        config_path.parent.mkdir(parents=True)
        config_path.write_text(json.dumps({"tab_limits": {"GUILD": BIG_LINES + 100}}), encoding="utf-8")
        texts = [f"guild line {n}" for n in range(1, BIG_LINES + 1)]
        text = "\n".join(line("GUILD", "Alice", ALICE, n, t, ts, 0) for n, t in enumerate(texts, start=1)) + "\n"
        app = self.start("big", exe, fresh=False)
        if app is None or not self.replay(app, exe.parent / "big.jsonl", text, texts[-1]):
            self.rec.auto("CP-big-ack", what, False, "the app did not start" if app is None else "the replay never arrived")
            return
        try:
            history = app.send("get-chat-history", timeout=30)["data"]
        except RuntimeError as e:
            self.rec.auto("CP-big-ack", what, False, f"no answer: {e}")
            self.leave(app)
            return
        got = [m["message"] for m in history]
        self.rec.auto("CP-big-ack", f"get-chat-history answers with all {BIG_LINES} lines in one answer",
                      got == texts, f"{len(got)} of {len(texts)} lines (first {got[:1]}, last {got[-1:]})")
        self.leave(app)

    @staticmethod
    def packets(events: bridge.Serve) -> list[dict]:
        return [m["message"]["payload"] for m in events.events() if m["topic"] == "rs/app/event/packet-event"]
