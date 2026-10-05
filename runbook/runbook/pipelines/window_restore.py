"""Pipeline `window-restore` (K18): closing with Settings open must not save the enlarged window.

Settings makes the window bigger (`grow_window`); the window-state plugin saves whatever size the window has at exit, so the next
start would open enlarged -- unless the app puts the old size back first. The pipeline does what a person did by hand, over the
bridge: `grow-window` (what opening Settings does), leave by `quit` (the tray's Quit: `ExitRequested`, no close request) and again by
`close-window` (the X: `CloseRequested`), start the app again on the same data folder with the window state ON, and `snapshot` the
window: it must have the size it had before the grow.
"""
from __future__ import annotations

import time
from pathlib import Path

from runbook import bridge, common, mockfeed, updater
from runbook.common import Recorder

TOLERANCE_PX = 2  # rounding of a window's size between two starts


class WindowRestore:
    def __init__(self, rec: Recorder, exe: str | Path, runs: Path | None = None) -> None:
        self.rec = rec
        self.exe = Path(exe)
        self.runs = Path(runs or common.RUNS) / "window-restore"
        self._serves: list[bridge.Serve] = []

    def run(self, key_path: str | None = None, password: str | None = None) -> None:
        """(`key_path` and `password` are for the pipelines that sign; this one ignores them.)"""
        for way, command in (("quit", "quit"), ("close", "close-window")):
            with updater.step(self.rec, f"K18-{way}", self.runs, strays=self.runs, stop=self._stop):
                self.round(way, command)

    def _stop(self, folder: str | Path) -> list[int]:
        for serve in self._serves:
            serve.stop()
        self._serves.clear()
        return mockfeed.stop_copies(folder)

    def start(self, label: str, exe: Path, *, fresh: bool):
        """The app with window state ON, on the data folder of its round; returns its bridge (None: it did not start)."""
        events = bridge.Serve(exe.parent / f"{label}-events.jsonl")
        self._serves.append(events)
        args = mockfeed.flag_args(exe.parent / "data", exe.parent / f"{label}-status.json", log_file=exe.parent / f"{label}.log",
                                  fresh=fresh, window_state=True, extra=("--no-update-check", "--bridge-url", events.url))
        mockfeed.start_app(exe, args)
        return events if bridge.wait_started(events, exe.parent / f"{label}.log", label=f"the app ({label})") else None

    @staticmethod
    def rect(events: bridge.Serve) -> dict:
        return events.send("snapshot", timeout=15)["data"]

    @staticmethod
    def size(rect: dict) -> str:
        return f"{rect['width']}x{rect['height']} at {rect['x']},{rect['y']}"

    def round(self, way: str, command: str) -> None:
        check = f"K18-{way}"
        exe = updater.fresh_copy(self.exe, self.runs, way)
        first = self.start(f"{way}-1", exe, fresh=True)
        self.rec.auto(f"{check}-start", "the copy started with the window state on and connected to the bridge", first is not None,
                      "" if first else "no app-started event in 120 s")
        if first is None:
            return
        before = self.rect(first)
        # What opening Settings does. 1200x900 logical is more than a small overlay; if the window is already that big,
        # ask for everything the screen has.
        replaced = first.send("grow-window", {"min_width": 1200, "min_height": 900}, timeout=15)["data"]
        if replaced is None:
            replaced = first.send("grow-window", {"min_width": 5000, "min_height": 5000}, timeout=15)["data"]
        grown = self.rect(first)
        if replaced is None:
            self.rec.record(f"{check}-grow", "Settings grew the window", "skip",
                            f"the window is already as big as it can get ({self.size(before)}): nothing to restore")
            return
        self.rec.auto(f"{check}-grow", "Settings grew the window", grown["width"] > before["width"] or grown["height"] > before["height"],
                      f"{self.size(before)} -> {self.size(grown)}")
        # Leave with Settings still open.
        try:
            first.send(command, timeout=15)
        except RuntimeError as e:
            self.rec.auto(f"{check}-leave", f"the app accepted {command}", False, str(e))
            return
        try:
            first.expect("rs/app/status", "offline", timeout=60)
            self.rec.auto(f"{check}-leave", f"the app ended after {command}", True)
        except RuntimeError:
            self.rec.auto(f"{check}-leave", f"the app ended after {command}", False, "no 'offline' within 60 s")
            return
        time.sleep(1)  # the window state is written as the process ends
        second = self.start(f"{way}-2", exe, fresh=False)
        if second is None:
            self.rec.auto(f"{check}-again", "the app started again on the same data folder", False, "no app-started event in 120 s")
            return
        after = self.rect(second)
        ok = abs(after["width"] - before["width"]) <= TOLERANCE_PX and abs(after["height"] - before["height"]) <= TOLERANCE_PX
        self.rec.auto(check, f"leaving by {way} with Settings open: the next start has the old size", ok,
                      f"before {self.size(before)}, grown {self.size(grown)}, after restart {self.size(after)}")
        second.send("quit", timeout=15)
