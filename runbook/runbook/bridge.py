"""Python side of the test bridge (`runbook/bridge/`, Node): start its broker before the app, point the app at
it with `--bridge-url`, and check afterwards what the app said. The Node code does the work; this file starts
it and reads its JSON. Needs Node 20+ and, once, `npm ci` in `runbook/bridge/` (`ensure_installed`)."""
from __future__ import annotations

import json
import shutil
import subprocess
from pathlib import Path

BRIDGE_DIR = Path(__file__).resolve().parent.parent / "bridge"
CLI = BRIDGE_DIR / "cli.mjs"


def node_path() -> str | None:
    return shutil.which("node")


def ensure_installed(timeout: int = 300) -> None:
    """`npm ci` in the bridge folder when its packages are not there yet."""
    if (BRIDGE_DIR / "node_modules" / "aedes").is_dir():
        return
    print("installing the bridge's packages (npm ci, once; about a minute) ...", flush=True)
    npm = shutil.which("npm")
    if npm is None:
        raise RuntimeError("npm was not found: install Node 20+ from nodejs.org")
    done = subprocess.run([npm, "ci"], cwd=BRIDGE_DIR, capture_output=True, text=True, timeout=timeout)
    if done.returncode != 0:
        raise RuntimeError("npm ci failed:\n" + (done.stdout + done.stderr)[-500:])


def _topic(event: str) -> str:
    return event if "/" in event else f"rs/app/event/{event}"


class Serve:
    """The broker + recorder, as a child process: `with Serve(out) as bridge:` ... `bridge.url` for the app."""

    def __init__(self, out: Path, port: int = 0) -> None:
        node = node_path()
        if node is None:
            raise RuntimeError("node was not found: install Node 20+ from nodejs.org")
        self.out = Path(out)
        self.out.parent.mkdir(parents=True, exist_ok=True)
        self._proc = subprocess.Popen(
            [node, str(CLI), "serve", "--port", str(port), "--out", str(self.out)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding="utf-8", errors="replace",
            cwd=BRIDGE_DIR)
        first = self._proc.stdout.readline()
        try:
            self.port = int(json.loads(first)["port"])
        except (ValueError, KeyError):
            self.stop()
            raise RuntimeError("the bridge did not start: " + (first + self._proc.stderr.read())[:300])

    @property
    def url(self) -> str:
        return f"mqtt://127.0.0.1:{self.port}"

    def _ask(self, command: dict) -> dict:
        self._proc.stdin.write(json.dumps(command) + "\n")
        self._proc.stdin.flush()
        answer = json.loads(self._proc.stdout.readline())
        if "error" in answer:
            raise RuntimeError(answer["error"])
        return answer

    def send(self, name: str, args: dict | None = None, timeout: float = 10) -> dict:
        """A command for the app; returns its ack. Raises when the app does not answer or refuses."""
        return self._ask({"send": name, "args": args or {}, "timeout": int(timeout * 1000)})["ack"]

    def publish(self, name: str, args: dict | None = None) -> str:
        """A command whose ack may never come (`restart-update` ends the app when it works): sent, not awaited.
        Returns the command id."""
        return self._ask({"publish": name, "args": args or {}})["sent"]

    def expect(self, event: str, match: dict | str | None = None, timeout: float = 10) -> dict:
        """The first message of `event` (a name like "update-state", or a full topic) that satisfies `match`, looking at
        what was already recorded first. `match` maps dotted paths to what they must equal -- or to `{"contains": text}`
        / `{"regex": pattern}`: `{"payload.state": {"regex": "^error"}}`. Raises when none comes within `timeout` s."""
        return self._ask({"expect": {"topic": _topic(event), "match": match, "timeout": int(timeout * 1000)}})["found"]

    def expect_sequence(self, steps: list[tuple[str, dict | str | None]], timeout: float = 10) -> list[dict]:
        """`(event, match)` steps, each seen after the one before it (other messages in between are fine)."""
        body = [{"topic": _topic(event), "match": match} for event, match in steps]
        return self._ask({"expect_sequence": {"steps": body, "timeout": int(timeout * 1000)}})["found"]

    def events(self) -> list[dict]:
        """Everything recorded so far: `{"topic", "received_at", "message"}` per message, oldest first."""
        text = self.out.read_text(encoding="utf-8") if self.out.exists() else ""
        return [json.loads(line) for line in text.splitlines() if line.strip()]

    def stop(self) -> None:
        if self._proc.poll() is None:
            try:
                self._proc.stdin.close()
                self._proc.wait(timeout=10)
            except (OSError, subprocess.TimeoutExpired):
                self._proc.kill()

    def __enter__(self) -> "Serve":
        return self

    def __exit__(self, *exc) -> None:
        self.stop()


def verify(scenario: str, log: Path, sample: Path) -> dict:
    """The checks of `scenario` over a recording: `{"ok": bool, "checks": [{id, title, ok, detail}]}`."""
    node = node_path()
    if node is None:
        raise RuntimeError("node was not found: install Node 20+ from nodejs.org")
    done = subprocess.run([node, str(CLI), "verify", scenario, "--log", str(log), "--sample", str(sample)],
                          capture_output=True, text=True, encoding="utf-8", errors="replace", cwd=BRIDGE_DIR,
                          timeout=60)
    if done.returncode not in (0, 1):
        raise RuntimeError(done.stderr.strip() or "verify failed")
    return json.loads(done.stdout)


def wait_started(events: "Serve", app_log: Path, timeout: float = 120, label: str = "the app", beat: float = 15) -> dict | None:
    """The `app-started` payload once the app has connected to `events`, else None after `timeout` seconds. Says what it waits
    for and that it still does (a pipeline that is silent for two minutes looks stuck), and when nothing comes shows the
    app's own log -- the first place to look."""
    import time

    print(f"  waiting for {label} to start and connect to the bridge (up to {timeout:.0f} s) ...", flush=True)
    began = time.monotonic()
    while True:
        left = timeout - (time.monotonic() - began)
        if left <= 0:
            break
        try:
            return events.expect("app-started", timeout=min(beat, left))["payload"]
        except RuntimeError:
            waited = time.monotonic() - began
            if waited < timeout:
                print(f"  ... still waiting for {label} ({waited:.0f} s)", flush=True)
    try:
        tail = Path(app_log).read_text(encoding="utf-8", errors="replace").splitlines()[-15:]
    except OSError:
        tail = []
    print(f"  {label} never said app-started. Its log ({app_log}):\n" + ("\n".join("    " + t for t in tail) or "    (no log: it did not start, or ignores --log-file)"),
          flush=True)
    return None
