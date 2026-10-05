"""A stand-in for a test-env build of Resonance Stream -- only for the dry run of w1-updater-mock.ipynb.

It reads the same flags, writes the same status file and follows the same update steps (check the
feed, download, verify, swap + restart), so the notebook's cells, waits and checks can be exercised
on any OS. It is NOT the app: what it does is this author's reading of src-tauri, and proves
nothing about Windows or the real exe. The "user" presses Start update by itself after a second,
and 재시작 after FAKE_APP_RESTART_AFTER seconds.

The notebook's "exe" is a shell script that runs this file (so bytes appended to it, as the
notebook does for the new version, are never parsed). Environment:
  FAKE_APP_EXE            path of the "exe" (set by the script)
  FAKE_APP_SIGNED_FILE    the file the "signature" is valid for: a download must equal it
  FAKE_APP_VERSION        the version it reports (default 0.5.0)
  FAKE_APP_RESTART_AFTER  seconds before it presses 재시작 (default 3)
  FAKE_APP_LIFETIME       seconds until it exits by itself (default 40)
  FAKE_APP_STALL_AFTER    seconds of silence it takes for a stalled download (default 3)
  FAKE_APP_SKIP_VERIFY=1  a bug to catch: accepts any download
  FAKE_APP_TOUCH_REAL=1   a bug to catch: also writes to the real %APPDATA% config
  FAKE_APP_BAD_STATUS=1   a bug to catch: a status file without pid and data_dir
"""
from __future__ import annotations

import http.client
import json
import os
import shutil
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

SWITCHES = {"fresh", "assume-setup-done", "no-capture", "no-translator", "no-update-check", "no-popups",
            "no-window-state", "print-env"}
VALUES = {"data-dir", "feed-url", "metadata-url", "status-file", "log-file"}


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

    def restart(self) -> None:
        temp = self.exe.with_name("update_temp.exe")
        if not self.verified(temp.read_bytes()):
            self.log("restart refused: update_temp.exe fails the check")
            return
        old = self.exe.with_name(self.exe.name + ".old")
        old.unlink(missing_ok=True)
        self.exe.rename(old)
        temp.rename(self.exe)
        self.exe.chmod(0o755)  # a downloaded file is not executable on POSIX; the real exe is
        args = [a for a in sys.argv[1:] if a not in ("--fresh", "--print-env")]
        subprocess.Popen([str(self.exe), *args], cwd=str(self.exe.parent), close_fds=True, env=os.environ.copy())
        sys.exit(0)


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
    first_run = not app.exe.with_name(app.exe.name + ".old").exists()  # a copy that was just updated does not click
    url = None if "no-update-check" in flags else app.check()
    if url and first_run:
        time.sleep(1)  # the user presses Start update
        if app.download(url):
            time.sleep(float(os.environ.get("FAKE_APP_RESTART_AFTER", "3")))  # ...and later 재시작
            app.restart()
    while time.monotonic() < deadline:
        time.sleep(0.5)
    return 0


if __name__ == "__main__":
    sys.exit(main())
