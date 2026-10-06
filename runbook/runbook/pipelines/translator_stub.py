"""Pipeline `translator-stub`: the whole translate path with no model and no GPU.

The app is started with `--llama-url` at a stand-in llama-server (`runbook/llama_stub.py`: the same endpoints and replies, nothing behind them),
the translator is started over the bridge (`start-translator`), Japanese chat is replayed, and what the app does is read from its events and
from what the stand-in received. Rows:

  TS-ready      the translator goes Starting -> Loading Model -> Active, and only once the server's /health says it is up
  TS-translate  a Japanese line gets its translation (`translation-event`, and the row in the chat log); a line that is not Japanese is not sent
  TS-prompt     what the server was sent is one user turn holding the line, ends by opening the model's turn, and has the sampling settings
                (stream off, stop tokens, an output limit). Also reported, not judged: how many literal `<bos>` the prompt text holds (K8)
  TS-restart    three lines in a row that the server cannot answer (HTTP 500) restart the translator: Restarting, then Active again
  TS-catchup    the lines that failed are translated after the restart, not lost
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

    def states(self, events: bridge.Serve) -> list[str]:
        return [m["message"]["payload"]["state"] for m in events.events() if m["topic"] == "rs/app/event/translator-state"]

    def round(self) -> None:
        stub = LlamaStub(loading_seconds=LOADING_S).start()
        stub.translations = dict(CANNED)
        self._stubs.append(stub)
        exe = updater.fresh_copy(self.exe, self.runs, "run")
        data = exe.parent / "data"
        (data / "config").mkdir(parents=True, exist_ok=True)
        (data / "config" / "config.json").write_text(json.dumps({"init_done": True, "use_translation": True}), encoding="utf-8")
        events = bridge.Serve(exe.parent / "events.jsonl")
        self._serves.append(events)
        args = mockfeed.flag_args(data, exe.parent / "status.json", log_file=exe.parent / "app.log", fresh=False, translator=True,
                                  extra=("--no-update-check", "--llama-url", stub.url, "--bridge-url", events.url))
        mockfeed.start_app(exe, args)
        if not bridge.wait_started(events, exe.parent / "app.log"):
            self.rec.auto("TS-start", "the app started and connected to the bridge", False, "no app-started event in 120 s")
            return

        began = time.monotonic()
        events.send("start-translator")
        try:
            events.expect_sequence([("translator-state", {"payload.state": "Starting"}),
                                    ("translator-state", {"payload.state": "Loading Model"}),
                                    ("translator-state", {"payload.state": "Active"})], timeout=90)
            took = time.monotonic() - began
            ok = took >= LOADING_S - 0.5 and "GET /health" in stub.hits
            detail = f"Active after {took:.1f} s (the server said 503 for {LOADING_S:.0f} s); states {self.states(events)}"
        except RuntimeError as e:
            ok, detail = False, f"{e}; states {self.states(events)}"
        self.rec.auto("TS-ready", "the translator goes Starting -> Loading Model -> Active, and waits for the server's /health", ok, detail)
        if not ok:
            return

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
        try:
            events.send("quit", timeout=15)
        except RuntimeError:
            pass
