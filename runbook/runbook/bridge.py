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
    npm = shutil.which("npm")
    if npm is None:
        raise RuntimeError("npm was not found: install Node 20+ from nodejs.org")
    done = subprocess.run([npm, "ci"], cwd=BRIDGE_DIR, capture_output=True, text=True, timeout=timeout)
    if done.returncode != 0:
        raise RuntimeError("npm ci failed:\n" + (done.stdout + done.stderr)[-500:])


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
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, cwd=BRIDGE_DIR)
        first = self._proc.stdout.readline()
        try:
            self.port = int(json.loads(first)["port"])
        except (ValueError, KeyError):
            self.stop()
            raise RuntimeError("the bridge did not start: " + (first + self._proc.stderr.read())[:300])

    @property
    def url(self) -> str:
        return f"mqtt://127.0.0.1:{self.port}"

    def send(self, name: str, args: dict | None = None, timeout: float = 10) -> dict:
        """A command for the app; returns its ack. Raises when the app does not answer or refuses."""
        self._proc.stdin.write(json.dumps({"send": name, "args": args or {}, "timeout": int(timeout * 1000)}) + "\n")
        self._proc.stdin.flush()
        answer = json.loads(self._proc.stdout.readline())
        if "error" in answer:
            raise RuntimeError(answer["error"])
        return answer["ack"]

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
                          capture_output=True, text=True, cwd=BRIDGE_DIR, timeout=60)
    if done.returncode not in (0, 1):
        raise RuntimeError(done.stderr.strip() or "verify failed")
    return json.loads(done.stdout)
