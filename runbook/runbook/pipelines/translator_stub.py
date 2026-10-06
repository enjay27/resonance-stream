"""Pipeline `translator-stub`: the whole translate path with no model and no GPU.

The app is started with `--llama-url` at a stand-in llama-server (`runbook/llama_stub.py`: the same endpoints and replies, nothing behind them),
the translator is started over the bridge (`start-translator`), Japanese chat is replayed, and what the app does is read from its events and
from what the stand-in received. Rows:

  TS-ready      the translator goes Starting -> Loading Model -> Active, asking /health until it says it is up (the app starts the
                translator itself at start-up when `use_translation` is on; `start-translator` is then a no-op)
  TS-translate  a Japanese line gets its translation (`translation-event`, and the row in the chat log); a line that is not Japanese is not sent
  TS-prompt     what the server was sent is one user turn holding the line, ends by opening the model's turn, and has the sampling settings
                (stream off, stop tokens, an output limit). Also reported, not judged: how many literal `<bos>` the prompt text holds (K8)
  TS-restart    three lines in a row that the server cannot answer (HTTP 500) restart the translator: Restarting, then Active again
  TS-catchup    the lines that failed are translated after the restart, not lost
  TS-log-quiet  the system log has no "Polling .../health" line (the health wait is quiet there)
  TS-reload     after the app is closed and started again on the same folder, each Japanese line is served once, with its translation

Three more runs of their own:

  TS-later-wait / TS-later-catchup  translation off (`use_translation` false), five Japanese lines are said, then `start-translator` with
                `translation_catch_up_limit` = 3: nothing is asked of the server while the translator is off, and then Catching Up
                translates the newest three, oldest first (the two older ones are passed over)
  TS-live-first  the server is slow and still loading when four Japanese lines arrive; during the catch-up a fifth, live line is said:
                it is translated before the last catch-up item, not after all of them
  TS-hang       the server takes a request and never answers: the line is given up on after the app's 30 s limit, no restart, and the next
                line is translated
"""
from __future__ import annotations

import json
import time
from pathlib import Path

from runbook import bridge, common, mockfeed, updater
from runbook.common import Recorder
from runbook.llama_stub import LlamaStub, source_of

GREETING, THANKS, ENGLISH = "こんにちは", "ありがとうございました", "Hello from an English speaker"
CANNED = {GREETING: "안녕하세요", THANKS: "감사합니다"}
LOADING_S = 3.0
LATER = ["おはよう", "こんばんは", "お疲れさま", "よろしくお願いします", "がんばって"]
LATER_LIMIT = 3
SLOW_LOADING_S = 12.0  # the lines of the live-first run arrive while the server loads
SLOW_DELAY_S = 2.0  # then each reply takes this long, so the catch-up is still running when the live line arrives
REQUEST_TIMEOUT_S = 30.0  # the app's own limit for a reply (resonance_llama::REQUEST_TIMEOUT)
HANG_MARGIN_S = 30.0
WAIT_S = 60.0  # how long a run waits for a state or a translation it expects before it says the row failed


def line(text: str, seq: int, ts: int, delay_ms: int = 100) -> str:
    return json.dumps({"delay_ms": delay_ms, "channel": "GUILD", "nickname": "Tester", "uid": 4242, "level": 60,
                       "timestamp": ts, "sequence_id": seq, "text": text}, ensure_ascii=False)


class TranslatorStub:
    def __init__(self, rec: Recorder, exe: str | Path, runs: Path | None = None) -> None:
        self.rec = rec
        self.exe = Path(exe)
        self.runs = Path(runs or common.RUNS) / "translator-stub"
        self._serves: list[bridge.Serve] = []
        self._stubs: list[LlamaStub] = []

    def run(self, key_path: str | None = None, password: str | None = None) -> None:
        """(`key_path` and `password` are for the pipelines that sign; this one ignores them.)"""
        with updater.step(self.rec, "TS", self.runs, strays=self.runs, stop=self._stop):
            self.round()

    def _stop(self, folder: str | Path) -> list[int]:
        for stub in self._stubs:
            stub.stop()
        self._stubs.clear()
        for serve in self._serves:
            serve.stop()
        self._serves.clear()
        return mockfeed.stop_copies(folder)

    def replay(self, events: bridge.Serve, path: Path, lines: list[str]) -> None:
        path.write_text("\n".join(lines) + "\n", encoding="utf-8")
        events.send("replay-chat", {"path": str(path)})

    def translated(self, events: bridge.Serve) -> dict[int, str]:
        return {m["message"]["payload"]["pid"]: m["message"]["payload"]["translated"]
                for m in events.events() if m["topic"] == "rs/app/event/translation-event"}

    @staticmethod
    def done(events: bridge.Serve) -> dict[str, str]:
        """The lines of the chat log that have a translation now, {text: translation}."""
        return {m["message"]: m["translated"] for m in events.send("get-chat-history")["data"] if m.get("translated")}

    def states(self, events: bridge.Serve) -> list[str]:
        return [m["message"]["payload"]["state"] for m in events.events() if m["topic"] == "rs/app/event/translator-state"]

    def round(self) -> None:
        stub = LlamaStub(loading_seconds=LOADING_S).start()
        stub.translations = dict(CANNED)
        self._stubs.append(stub)
        exe = updater.fresh_copy(self.exe, self.runs, "run")
        events = self.launch("first", exe, stub, {"init_done": True, "use_translation": True})
        if events is None:
            self.rec.auto("TS-start", "the app started and connected to the bridge", False, "no app-started event in 120 s")
            return

        began = time.monotonic()
        events.send("start-translator")
        try:
            events.expect_sequence([("translator-state", {"payload.state": "Starting"}),
                                    ("translator-state", {"payload.state": "Loading Model"}),
                                    ("translator-state", {"payload.state": "Active"})], timeout=90)
            took = time.monotonic() - began
            ok = stub.health_503 >= 1 and "GET /health" in stub.hits
            detail = (f"Active; the app asked /health {stub.hits.count('GET /health')} times and was told 503 {stub.health_503} times "
                      f"before the stand-in was up ({LOADING_S:.0f} s after the first ask); {took:.1f} s since the command; states {self.states(events)}")
        except RuntimeError as e:
            ok, detail = False, f"{e}; states {self.states(events)}"
        self.rec.auto("TS-ready", "the translator goes Starting -> Loading Model -> Active, and waits for the server's /health", ok, detail)
        if not ok:
            return
        polling = [m["message"]["payload"]["message"] for m in events.events()
                   if m["topic"] == "rs/app/event/system-event" and "polling" in str(m["message"]["payload"].get("message", "")).lower()]
        self.rec.auto("TS-log-quiet", "the health wait leaves no 'Polling .../health' lines in the system log", not polling,
                      f"{len(polling)} such line(s)" + (f", e.g. {polling[0]!r}" if polling else ""))

        ts = int(time.time())
        self.replay(events, exe.parent / "first.jsonl", [line(GREETING, 1, ts, 0), line(THANKS, 2, ts), line(ENGLISH, 3, ts)])
        try:
            events.expect("system-event", {"payload.message": "Replay finished"}, timeout=30)
            time.sleep(3)
        except RuntimeError:
            self.rec.auto("TS-translate", "a Japanese line gets its translation", False, "the replay never finished")
            return
        packets = {p["message"]["payload"]["message"]: p["message"]["payload"]["pid"] for p in events.events()
                   if p["topic"] == "rs/app/event/packet-event"}
        got = self.translated(events)
        history = {m["message"]: m.get("translated") for m in events.send("get-chat-history")["data"]}
        want = {packets.get(GREETING): CANNED[GREETING], packets.get(THANKS): CANNED[THANKS]}
        asked = [source_of(r["prompt"]) for r in stub.requests]
        ok = (got == want and history.get(GREETING) == CANNED[GREETING] and history.get(THANKS) == CANNED[THANKS]
              and not history.get(ENGLISH) and sorted(asked) == sorted([GREETING, THANKS]))
        self.rec.auto("TS-translate", "a Japanese line gets its translation, a line that is not Japanese is not sent", ok,
                      f"translation events {got}; chat log {history}; the server was asked {asked}")

        prompt = str(stub.requests[0].get("prompt", "")) if stub.requests else ""
        request = stub.requests[0] if stub.requests else {}
        ok = (prompt.startswith("<bos><start_of_turn>user\n") and prompt.endswith("<end_of_turn>\n<start_of_turn>model\n")
              and prompt.count("<start_of_turn>user") == 1 and source_of(prompt) in (GREETING, THANKS)
              and request.get("stream") is False and "<end_of_turn>" in (request.get("stop") or [])
              and 1 <= int(request.get("n_predict") or 0) <= 512)
        self.rec.auto("TS-prompt", "the prompt is one user turn with the line and ends by opening the model's turn; sampling settings are set", ok,
                      f"literal <bos> in the prompt text: {prompt.count('<bos>')} (K8: llama.cpp may add its own); "
                      f"stream={request.get('stream')} stop={request.get('stop')} n_predict={request.get('n_predict')}; prompt starts {prompt[:40]!r}")

        stub.completion_mode = "error"
        before = len(stub.requests)
        lost = ["さようなら", "おやすみなさい", "またあした"]
        self.replay(events, exe.parent / "second.jsonl", [line(t, 10 + i, ts + 5, 100) for i, t in enumerate(lost)])
        try:
            events.expect("translator-state", {"payload.state": "Restarting"}, timeout=60)
            stub.completion_mode = "ok"
            events.expect_sequence([("translator-state", {"payload.state": "Restarting"}),
                                    ("translator-state", {"payload.state": "Active"})], timeout=90)
            ok, detail = True, f"{len(stub.requests) - before} requests while the server failed; states {self.states(events)[-6:]}"
        except RuntimeError as e:
            stub.completion_mode = "ok"
            ok, detail = False, f"{e}; states {self.states(events)}"
        self.rec.auto("TS-restart", "three failed lines in a row restart the translator (Restarting, then Active)", ok, detail)

        deadline = time.monotonic() + 45
        while time.monotonic() < deadline:
            korean = {m["message"]: m.get("translated") for m in events.send("get-chat-history")["data"]}
            if all(korean.get(t) for t in lost):
                break
            time.sleep(1)
        self.rec.auto("TS-catchup", "the lines that failed are translated after the restart", all(korean.get(t) for t in lost),
                      f"{ {t: korean.get(t) for t in lost} }")
        time.sleep(2)  # the archive writer is a thread of its own
        if not self.leave(events):
            self.rec.auto("TS-reload", "a translated line is served once, translated, after a restart", False, "the first run did not end when told to")
        else:
            self.reload(exe, stub, [GREETING, THANKS, *lost])
        self.later_round()
        self.slow_round()

    def launch(self, label: str, exe: Path, stub: LlamaStub, config: dict | None = None) -> bridge.Serve | None:
        """Starts the app on its own folder, at `stub`; writes `config` first when given. None when it never says app-started."""
        data = exe.parent / "data"
        if config is not None:
            (data / "config").mkdir(parents=True, exist_ok=True)
            (data / "config" / "config.json").write_text(json.dumps(config), encoding="utf-8")
        events = bridge.Serve(exe.parent / f"{label}-events.jsonl")
        self._serves.append(events)
        args = mockfeed.flag_args(data, exe.parent / f"{label}-status.json", log_file=exe.parent / f"{label}.log", fresh=False, translator=True,
                                  extra=("--no-update-check", "--llama-url", stub.url, "--bridge-url", events.url))
        mockfeed.start_app(exe, args)
        return events if bridge.wait_started(events, exe.parent / f"{label}.log", label=f"the app ({label})") else None

    @staticmethod
    def leave(events: bridge.Serve) -> bool:
        try:
            events.send("quit", timeout=15)
            events.expect("rs/app/status", "offline", timeout=60)
        except RuntimeError:
            return False
        time.sleep(1)
        return True

    def reload(self, exe: Path, stub: LlamaStub, japanese: list[str]) -> None:
        """The same folder again: every Japanese line comes back once, with its translation (a line saved untranslated and again
        translated is one line, the newer)."""
        events = self.launch("reload", exe, stub)
        if events is None:
            self.rec.auto("TS-reload", "a translated line is served once, translated, after a restart", False, "no app-started event in 120 s")
            return
        history = events.send("get-chat-history")["data"]
        rows = {t: [m for m in history if m["message"] == t] for t in japanese}
        ok = all(len(r) == 1 and r[0].get("translated") for r in rows.values())
        self.rec.auto("TS-reload", "after a restart each Japanese line is served once, with its translation", ok,
                      "; ".join(f"{t}: {len(r)} row(s), translated {r[0].get('translated')!r}" if r else f"{t}: missing" for t, r in rows.items()))
        self.leave(events)

    def later_round(self) -> None:
        """Translation off while Japanese chat is said; turned on later it catches up on the newest `LATER_LIMIT` only."""
        stub = LlamaStub().start()
        self._stubs.append(stub)
        exe = updater.fresh_copy(self.exe, self.runs, "later")
        events = self.launch("later", exe, stub, {"init_done": True, "use_translation": False, "translation_catch_up_limit": LATER_LIMIT})
        if events is None:
            self.rec.auto("TS-later-wait", "nothing is translated while translation is off", False, "no app-started event in 120 s")
            return
        ts = int(time.time())
        self.replay(events, exe.parent / "later.jsonl", [line(t, 20 + i, ts, 0 if i == 0 else 100) for i, t in enumerate(LATER)])
        try:
            events.expect("system-event", {"payload.message": "Replay finished"}, timeout=30)
        except RuntimeError:
            self.rec.auto("TS-later-wait", "nothing is translated while translation is off", False, "the replay never finished")
            return
        time.sleep(2)
        states = self.states(events)
        waited = not stub.requests and not self.done(events) and "Active" not in states
        self.rec.auto("TS-later-wait", "with translation off, nothing is asked of the server and nothing is translated", waited,
                      f"{len(stub.requests)} request(s), {len(self.done(events))} translation(s), translator states {states}")
        events.send("start-translator")
        want = LATER[-LATER_LIMIT:]
        deadline = time.monotonic() + WAIT_S
        while time.monotonic() < deadline and len(self.done(events)) < LATER_LIMIT:
            time.sleep(0.5)
        time.sleep(2)  # a fourth would have come right after the third
        asked = [source_of(r["prompt"]) for r in stub.requests]
        history = self.done(events)
        caught = "Catching Up" in self.states(events)
        ok = caught and asked == want and not any(history.get(t) for t in LATER[:-LATER_LIMIT])
        self.rec.auto("TS-later-catchup", f"turned on later, Catching Up translates the newest {LATER_LIMIT} lines, oldest first, and passes over the older",
                      ok, f"Catching Up seen: {caught}; the server was asked {asked} (wanted {want}); older lines translated: "
                          f"{[t for t in LATER[:-LATER_LIMIT] if history.get(t)]}")
        self.leave(events)

    def slow_round(self) -> None:
        """A slow server that is still loading when the lines arrive: the catch-up serves a live line before its last item; and a request
        the server never answers costs one line, not the translator."""
        stub = LlamaStub(loading_seconds=SLOW_LOADING_S).start()
        stub.completion_delay = SLOW_DELAY_S
        self._stubs.append(stub)
        exe = updater.fresh_copy(self.exe, self.runs, "slow")
        events = self.launch("slow", exe, stub, {"init_done": True, "use_translation": True})
        if events is None:
            self.rec.auto("TS-live-first", "a live line is served during the catch-up", False, "no app-started event in 120 s")
            return
        ts = int(time.time())
        owed, live = LATER[:4], LATER[4]
        self.replay(events, exe.parent / "slow-owed.jsonl", [line(t, 30 + i, ts, 0 if i == 0 else 100) for i, t in enumerate(owed)])
        try:
            events.expect("translator-state", {"payload.state": "Catching Up"}, timeout=SLOW_LOADING_S + WAIT_S)
        except RuntimeError as e:
            self.rec.auto("TS-live-first", "a live line is served during the catch-up", False, f"{e}; states {self.states(events)}")
            self.leave(events)
            return
        self.replay(events, exe.parent / "slow-live.jsonl", [line(live, 40, ts + 5, 0)])
        deadline = time.monotonic() + SLOW_DELAY_S * 8 + WAIT_S / 2
        while time.monotonic() < deadline and len(self.done(events)) < len(LATER):
            time.sleep(0.5)
        asked = [source_of(r["prompt"]) for r in stub.requests]
        ok = live in asked and asked.index(live) < len(asked) - 1 and len(self.done(events)) == len(LATER)
        self.rec.auto("TS-live-first", "a live line said during the catch-up is translated before its last item, not after all of them", ok,
                      f"the server was asked, in order: {asked}; {len(self.done(events))} of {len(LATER)} translated")
        self.hang(events, stub, exe, ts)
        self.leave(events)

    def hang(self, events: bridge.Serve, stub: LlamaStub, exe: Path, ts: int) -> None:
        stub.completion_delay = 0
        stub.completion_mode = "hang"
        before = len(stub.requests)
        states = len(self.states(events))
        self.replay(events, exe.parent / "hang-1.jsonl", [line("ちょっと待って", 50, ts + 10, 0)])
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline and len(stub.requests) == before:
            time.sleep(0.2)
        stub.completion_mode = "ok"  # the hung request stays hung; the next one is answered
        began = time.monotonic()
        self.replay(events, exe.parent / "hang-2.jsonl", [line("もう一度", 51, ts + 11, 0)])
        limit = REQUEST_TIMEOUT_S + HANG_MARGIN_S
        while time.monotonic() < began + limit and "もう一度" not in self.done(events):
            time.sleep(0.5)
        took = time.monotonic() - began
        history = self.done(events)
        later = self.states(events)[states:]
        stub.release_hang()
        ok = bool(history.get("もう一度")) and "Restarting" not in later
        self.rec.auto("TS-hang", f"a request the server never answers costs one line, not the translator (the app's limit is {REQUEST_TIMEOUT_S:.0f} s)", ok,
                      f"the next line was translated after {took:.0f} s: {bool(history.get('もう一度'))}; the hung line translated: "
                      f"{bool(history.get('ちょっと待って'))}; translator states since: {later}")
