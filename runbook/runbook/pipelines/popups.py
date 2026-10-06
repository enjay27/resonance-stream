"""Pipeline `popups`: the popup windows (cheat sheet, favorites) beside the overlay, with real windows and no clicks.

The commands are the ones the title bar's buttons and a person's mouse reach: `open-popup`, `hide-popup` (the X), `place-popup` (drag and resize),
`pin-main` (the pin), `close-window` (the main window's X), and `snapshot-popups` to read every window back. Rows:

  PP-prewarm  about 2 s after start both popups exist, hidden, and the overlay is visible
  PP-open     opening one shows it (the `popup-shown` event, visible, a size); opening it again brings the same window forward -- never a second window
  PP-hide     the X hides it (the window stays), and opening it again shows it
  PP-pin      pinning the overlay pins both popups, and letting it go lets them go
  PP-exit     closing the main window ends the app: it says offline, and no `resonance-stream.exe` is left (the popups, which are windows too,
              must not keep it alive)
  PP-place    a popup put somewhere and left there opens at that place after a restart (not at the default place, no jump)
"""
from __future__ import annotations

import time
from pathlib import Path

from runbook import bridge, common, mockfeed, updater
from runbook.common import Recorder

CHEAT, FAV = "popup-cheatsheet", "popup-favorites"
PLACE = {"x": 140, "y": 110, "width": 520, "height": 660}
TOLERANCE_PX = 3


def near(rect: dict | None, want: dict) -> bool:
    return bool(rect) and all(abs(rect[k] - want[k]) <= TOLERANCE_PX for k in ("x", "y", "width", "height"))


class Popups:
    def __init__(self, rec: Recorder, exe: str | Path, runs: Path | None = None) -> None:
        self.rec = rec
        self.exe = Path(exe)
        self.runs = Path(runs or common.RUNS) / "popups"
        self._serves: list[bridge.Serve] = []

    def run(self, key_path: str | None = None, password: str | None = None) -> None:
        """(`key_path` and `password` are for the pipelines that sign; this one ignores them.)"""
        with updater.step(self.rec, "PP", self.runs, strays=self.runs, stop=self._stop):
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
                                  fresh=fresh, window_state=True, popups=True, extra=("--no-update-check", "--bridge-url", events.url))
        mockfeed.start_app(exe, args)
        return events if bridge.wait_started(events, exe.parent / f"{label}.log", label=f"the app ({label})") else None

    @staticmethod
    def snap(events: bridge.Serve) -> dict:
        return events.send("snapshot-popups", timeout=15)["data"]

    def round(self) -> None:
        exe = updater.fresh_copy(self.exe, self.runs, "run")
        first = self.start("first", exe, fresh=True)
        self.rec.auto("PP-start", "the app started with its popups on and connected to the bridge", first is not None,
                      "" if first else "no app-started event in 120 s")
        if first is None:
            return

        time.sleep(6)  # the popups are made ~2 s after start, off the main thread
        state = self.snap(first)
        ok = (state["main"].get("visible") and all(state[k].get("exists") and not state[k].get("visible") for k in (CHEAT, FAV)))
        self.rec.auto("PP-prewarm", "both popups exist, hidden, a moment after start", ok,
                      f"main visible={state['main'].get('visible')}; windows {state.get('labels')}; "
                      f"cheat visible={state[CHEAT].get('visible')}, favorites visible={state[FAV].get('visible')}")

        first.send("open-popup", {"kind": "cheatsheet"}, timeout=40)
        shown = False
        try:
            first.expect("popup-shown", timeout=10)
            shown = True
        except RuntimeError:
            pass
        first.send("open-popup", {"kind": "cheatsheet"}, timeout=40)
        state = self.snap(first)
        popups = [label for label in state.get("labels", []) if label.startswith("popup-")]
        rect = state[CHEAT].get("rect") or {}
        ok = shown and state[CHEAT].get("visible") and rect.get("width", 0) > 200 and sorted(popups) == [CHEAT, FAV]
        self.rec.auto("PP-open", "opening a popup shows it; opening it again makes no second window", ok,
                      f"popup-shown seen: {shown}; visible={state[CHEAT].get('visible')}; rect {rect}; popup windows {popups}")

        first.send("hide-popup", {"kind": "cheatsheet"}, timeout=15)
        hidden = self.snap(first)[CHEAT]
        first.send("open-popup", {"kind": "cheatsheet"}, timeout=40)
        again = self.snap(first)[CHEAT]
        self.rec.auto("PP-hide", "the X hides a popup and the window stays; opening it shows it again",
                      hidden.get("exists") and not hidden.get("visible") and again.get("visible"),
                      f"after the X: exists={hidden.get('exists')} visible={hidden.get('visible')}; after open: visible={again.get('visible')}")

        first.send("pin-main", {"on": True}, timeout=15)
        pinned = self.snap(first)
        first.send("pin-main", {"on": False}, timeout=15)
        free = self.snap(first)
        windows = ("main", CHEAT, FAV)
        ok = all(pinned[k].get("always_on_top") for k in windows) and not any(free[k].get("always_on_top") for k in windows)
        self.rec.auto("PP-pin", "pinning the overlay pins both popups, letting it go lets them go", ok,
                      f"pinned {[pinned[k].get('always_on_top') for k in windows]}, released {[free[k].get('always_on_top') for k in windows]} (main, cheat sheet, favorites)")

        first.send("open-popup", {"kind": "favorites"}, timeout=40)
        first.send("place-popup", {"kind": "favorites", **PLACE}, timeout=15)
        placed = self.snap(first)[FAV].get("rect")
        first.send("hide-popup", {"kind": "favorites"}, timeout=15)
        # The main window's X: the app must end, and take its hidden popups with it.
        first.send("close-window", timeout=15)
        try:
            first.expect("rs/app/status", "offline", timeout=60)
            time.sleep(3)
            left = mockfeed.stop_copies(exe.parent)
            self.rec.auto("PP-exit", "closing the main window ends the app, no process is left", not left,
                          f"offline seen; processes still running afterwards: {left}")
        except RuntimeError:
            self.rec.auto("PP-exit", "closing the main window ends the app, no process is left", False, "no 'offline' within 60 s")
            return
        time.sleep(1)

        second = self.start("second", exe, fresh=False)
        if second is None:
            self.rec.auto("PP-place", "a popup opens where it was left after a restart", False, "no app-started event in 120 s")
            return
        second.send("open-popup", {"kind": "favorites"}, timeout=40)
        rect = self.snap(second)[FAV].get("rect")
        self.rec.auto("PP-place", "a popup left at a place opens there after a restart", near(rect, PLACE),
                      f"left at {placed}; asked for {PLACE}; after restart {rect}")
        try:
            second.send("quit", timeout=15)
        except RuntimeError:
            pass
