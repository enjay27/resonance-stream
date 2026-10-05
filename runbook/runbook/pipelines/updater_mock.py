"""Pipeline `updater-mock` (K2 against a mock feed): the app's own update path, with no one at the keyboard.

Python defines the steps here; the bridge (`runbook/bridge/`, MQTT) makes the exe do things and tell what it did:
`start-update` / `restart-update` are the dialog's buttons, `update-state` and `app-started` events are what the status
file used to be polled for. A mock GitHub on 127.0.0.1 serves the feed; the "new version" is the exe itself with a few bytes
appended, signed with the backup key (once per exe: the signature is kept in `runs/mock/signed/`).

Only what the bridge cannot see stays visual (what the dialog looked like): recorded as `skip` unless `ui_checks=True`.
"""
from __future__ import annotations

import contextlib
import getpass
import json
import os
import shutil
import textwrap
import time
import types
from pathlib import Path

from runbook import bridge, common, mockfeed, updater
from runbook.common import Recorder, capture

UI_SKIP = "visual: what the dialog looked like is not driven by the bridge (run with ui_checks to be asked)"


class UpdaterMock:
    def __init__(self, rec: Recorder, exe: str | Path, runs: Path | None = None, *, new_version: str = "9.9.9",
                 ui_checks: bool = False) -> None:
        self.rec = rec
        self.exe = Path(exe)
        self.runs = Path(runs or common.RUNS) / "mock"
        self.new_version = new_version
        self.ui_checks = ui_checks
        self._serves: list[bridge.Serve] = []
        self.local_version = ""
        self.old_sha = ""
        self.new_exe: Path | None = None
        self.new_sha = ""
        self.signature: str | None = None
        self.real_config = updater.app_config_path()
        self.real_before = mockfeed.fingerprint(self.real_config)

    # -- the whole pipeline ------------------------------------------------------------------------------
    def run(self, key_path: str | None = None, password: str | None = None) -> None:
        if not self.smoke():
            return
        if not self.prepare_release(key_path, password):
            return
        self.feed_cases()
        self.good_update()
        self.tampered_download()
        self.failing_downloads()
        self.isolation()

    # -- helpers -----------------------------------------------------------------------------------------
    def guard(self, check: str):
        """One step as try / catch / finally (`updater.step`): an exception is a failed row, and whatever happens the
        apps and their bridges are closed afterwards."""
        return updater.step(self.rec, check, self.runs, strays=self.runs, stop=self._stop)

    def _stop(self, folder: str | Path) -> list[int]:
        for serve in self._serves:
            serve.stop()
        self._serves.clear()
        return mockfeed.stop_copies(folder)

    def new_server(self, **kw) -> mockfeed.MockServer:
        return mockfeed.MockServer(exe_bytes=kw.pop("exe_bytes", self.new_exe.read_bytes()), signature=self.signature,
                                   version=kw.pop("version", self.new_version), **kw)

    def start_app(self, label: str, server: mockfeed.MockServer) -> types.SimpleNamespace:
        exe = updater.fresh_copy(self.exe, self.runs, label)
        events = bridge.Serve(exe.parent / "bridge-events.jsonl")
        self._serves.append(events)
        app = types.SimpleNamespace(label=label, exe=exe, dir=exe.parent, log=exe.parent / "app.log", bridge=events,
                                    status=exe.parent / "status.json", pid=None)
        args = mockfeed.flag_args(app.dir / "data", app.status, log_file=app.log, feed_url=server.feed_url,
                                  metadata_url=server.metadata_url,
                                  extra=("--bridge-url", events.url))
        app.pid = mockfeed.start_app(exe, args)
        return app

    def begin(self, label: str, server: mockfeed.MockServer):
        """A fresh copy, started with the test flags, connected to its own bridge. Returns `(app, started)`."""
        app = self.start_app(label, server)
        started = bridge.wait_started(app.bridge, app.log, label=f"the app ({label})")
        self.rec.auto(label, "the copy started with the test flags and connected to the bridge", started is not None,
                      f"pid {started['pid']}, version {started['version']}" if started else "no app-started event in 120 s")
        if started is None:
            self.diagnose(app)
        return app, started

    def diagnose(self, app) -> None:
        events = [f"{e['topic'].rsplit('/', 1)[-1]} {json.dumps(e['message'].get('payload'), ensure_ascii=False)[:120]}"
                  for e in app.bridge.events()[-8:] if isinstance(e["message"], dict)]
        print(f"  [{app.label}] last bridge events:\n" + textwrap.indent("\n".join(events) or "(none)", "    "))
        tail = mockfeed.log_tail(app.log)
        print("  app log, last lines:\n" + textwrap.indent(tail, "    ") if tail else "  (the app wrote no log)")

    def update_state(self, app, state: str | dict, timeout: float) -> str | None:
        """The text of the first `update-state` event matching `state` (an exact text, or a matcher), else None."""
        match = {"payload.state": state}
        try:
            return app.bridge.expect("update-state", match, timeout=timeout)["payload"]["state"]
        except RuntimeError:
            return None

    def announced(self, label: str, app) -> bool:
        got = self.update_state(app, {"regex": r"^(available:|error)"}, 90)
        ok = got == f"available:{self.new_version}"
        self.rec.auto(f"{label}-1", "the feed's newer version is announced (update: available)", ok,
                      f"update: {got or 'no answer in 90 s'}")
        if not ok:
            self.diagnose(app)
        return ok

    def ui(self, check: str, title: str, steps: str, expect: str) -> None:
        if self.ui_checks:
            self.rec.manual(check, title, steps, expect)
        else:
            self.rec.record(check, title, "skip", UI_SKIP)

    # -- 0 · the exe under test -------------------------------------------------------------------------
    def smoke(self) -> bool:
        smoke = self.runs / "smoke"
        shutil.rmtree(smoke, ignore_errors=True)
        smoke.mkdir(parents=True)
        code_, out = mockfeed.run_print_env(self.exe, smoke / "data", smoke / "status.json", timeout=30)
        st = mockfeed.read_status(smoke / "status.json")
        ok = code_ == 0 and st is not None and "print-env" in st.get("flags", [])
        self.rec.auto("M0", "the exe understands the test flags (--print-env wrote its status file)", ok,
                      f"version {st['version']}, data_dir {st['data_dir']}" if ok
                      else " ".join(out.split())[-200:] or f"exit {code_}, no status file")
        if ok:
            self.local_version = st["version"]
            self.old_sha = updater.sha256_file(self.exe)
            print("exe under test:", self.exe, "| version", self.local_version, "| sha256", self.old_sha[:16])
        else:
            print("This exe does not take the test flags -- build it with --features test-env.")
        return ok

    # -- 1 · a "new version", signed with the backup key ----------------------------------------------
    def prepare_release(self, key_path: str | None, password: str | None) -> bool:
        self.new_exe = mockfeed.make_new_exe(self.exe, self.runs / "new" / "update.exe")
        self.new_sha = updater.sha256_file(self.new_exe)
        sig = Path(str(self.new_exe) + ".sig")
        cache = self.runs / "signed" / f"{self.new_sha}-{self.new_version}.sig"
        sig.unlink(missing_ok=True)
        if cache.exists():  # signed before: the password is asked once per exe
            shutil.copy(cache, sig)
            self.rec.auto("M1-1", "the new exe was signed", True, f"signature kept from an earlier run ({cache.name})")
        else:
            if not key_path:
                self.rec.auto("M1-1", "the backup key file can be read", False, "no key file given (BACKUP_KEY)")
                return False
            try:
                key_text = "dry-run-key" if common.dry_run() else updater.read_key_file(key_path)
            except ValueError as e:
                self.rec.auto("M1-1", "the backup key file can be read", False, str(e))
                return False
            if password is None:
                password = getpass.getpass("Backup key password: ")
            os.environ["TAURI_SIGNING_PRIVATE_KEY"] = key_text
            os.environ["TAURI_SIGNING_PRIVATE_KEY_PASSWORD"] = password
            try:
                code_, out = capture(["npx", "--yes", "@tauri-apps/cli@2", "signer", "sign", "--app-version",
                                      self.new_version, str(self.new_exe)], fixture="tauri_sign_ok.txt", timeout=900)
            finally:
                os.environ.pop("TAURI_SIGNING_PRIVATE_KEY", None)
                os.environ.pop("TAURI_SIGNING_PRIVATE_KEY_PASSWORD", None)
            if common.dry_run():
                shutil.copy(common.FIXTURES / "fake_backup.sig", sig)
            signed = code_ == 0 and sig.exists()
            self.rec.auto("M1-1", "the new exe was signed", signed,
                          "signature written" if signed else " ".join(out.split())[-200:])
            if not signed:
                return False
            cache.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(sig, cache)
        sig_text = sig.read_text()
        key_id = updater.signature_key_id(sig_text)
        self.rec.auto("M1-2", "...by the BACKUP key", updater.is_backup_key(key_id), f"key id {key_id}")
        self.rec.auto("M1-3", f"...for version {self.new_version}", updater.signature_version(sig_text) == self.new_version,
                      f"the signature says {updater.signature_version(sig_text)}")
        self.signature = mockfeed.feed_signature(sig_text)
        print("new exe:", self.new_exe, "| sha256", self.new_sha[:16])
        return True

    # -- 2 · what the app does with a feed ---------------------------------------------------------------
    def feed_case(self, label: str, title: str, expect: str, *, feed_mode: str = "ok", version: str | None = None,
                  down: bool = False) -> None:
        with self.guard(label), self.new_server(version=version or self.new_version) as server:
            server.feed_mode = feed_mode
            if down:
                server.stop()  # the URLs stay, nothing answers
            app, started = self.begin(label + "-app", server)
            if started is None:
                return
            if down:
                time.sleep(3 if common.dry_run() else 20)
                states = [e["message"]["payload"]["state"] for e in app.bridge.events()
                          if e["topic"].endswith("/update-state")]
                alive = True
                try:
                    app.bridge.send("ping", timeout=10)
                except RuntimeError:
                    alive = False
                ok = alive and not any(s.startswith(("available", "downloading", "downloaded")) for s in states)
                self.rec.auto(label, title, ok, f"update-state events: {states or 'none'}; the app {'answers' if alive else 'does NOT answer'} a ping")
                return
            want = {"regex": r"^error"} if expect == "error" else "none"
            got = self.update_state(app, want, 90)
            self.rec.auto(label, title, got is not None, f"update: {got or 'the expected state never came in 90 s'}")
            if got is None:
                self.diagnose(app)

    def feed_cases(self) -> None:
        self.feed_case("M8", "a feed that is not JSON ends in update: error", "error", feed_mode="garbage")
        self.feed_case("M9", "a feed announcing the version already running is no update", "none", version=self.local_version)
        self.feed_case("M10", "a feed announcing an OLDER version is no update", "none", version="0.0.1")
        self.feed_case("M11", "a feed that is a 404 is no update (no stable release yet)", "none", feed_mode="404")
        self.feed_case("M12", "nothing answering at all is no update (and no crash)", "none", down=True)

    # -- 3 · a good update, end to end ----------------------------------------------------------------
    def start_update(self, label: str, app, *, wait_for: str = "downloaded", timeout: float = 300) -> str | None:
        """`start-update` (the dialog's button), then the first of `wait_for` / error. Returns what happened."""
        try:
            app.bridge.send("start-update", timeout=30)
        except RuntimeError as e:
            self.rec.auto(f"{label}-start", "the app accepted start-update", False, str(e))
            return None
        return self.update_state(app, {"regex": rf"^({wait_for}|error)"}, timeout)

    def good_update(self) -> None:
        with self.guard("M3"), self.new_server() as server:
            app, started = self.begin("M3", server)
            if started is None or not self.announced("M3", app):
                return
            old_pid = started["pid"]
            got = self.start_update("M3", app)
            self.rec.auto("M3-2", "the download finishes and passes the signature check (update: downloaded)",
                          got == "downloaded", f"update: {got or 'nothing in 5 minutes'}")
            if got != "downloaded":
                self.diagnose(app)
                return
            app.bridge.publish("restart-update")  # success ends the app: there is no ack to wait for
            try:
                again = app.bridge.expect("app-started", {"payload.pid": {"not": old_pid}}, timeout=180)["payload"]
            except RuntimeError:
                again = None
            self.rec.auto("M3-3", "the app restarted into a new process, with the same flags", again is not None,
                          f"pid {old_pid} -> {again['pid']}" if again else "no new process said app-started")
            if again:
                self.rec.auto("M3-4", "the new process runs the swapped exe (same path)",
                              Path(again["exe"]).resolve() == app.exe.resolve(), again["exe"])
            else:
                self.diagnose(app)
            for title, ok, evidence in updater.check_a1(app.dir, app.exe.name, self.old_sha, self.new_sha):
                self.rec.auto("M3-5", title, ok, evidence)
            self.ui("M3-ui", "the dialog behaved",
                    "Think back over the update. Korean release notes? The bar moved? '다운로드 완료' only after the bar was full? "
                    "After 재시작 a new window with the app?", "all of that")

    # -- 4 · a tampered download is refused ------------------------------------------------------------
    def tampered_download(self) -> None:
        with self.guard("M4"), self.new_server() as server:
            app, started = self.begin("M4", server)
            if started is None or not self.announced("M4", app):
                return
            got = self.start_update("M4", app)
            self.rec.auto("M4-2", "the download finishes (update: downloaded)", got == "downloaded",
                          f"update: {got or 'nothing in 5 minutes'}")
            temp = app.dir / "update_temp.exe"
            if got != "downloaded":
                self.diagnose(app)
                return
            if not temp.exists():
                self.rec.auto("M4-3", "update_temp.exe is next to the exe", False, f"not found in {app.dir}")
                self.diagnose(app)
                return
            offset, before, after = updater.flip_one_byte(temp)
            self.rec.auto("M4-3", "update_temp.exe: one byte flipped", True, f"offset {offset}: {before:#04x} -> {after:#04x}")
            try:
                app.bridge.send("restart-update", timeout=30)
                refused, why = False, "the app accepted the tampered update and restarted"
            except RuntimeError as e:
                # "no ack" means the app did not answer at all -- that is not a refusal
                refused, why = not str(e).startswith("no ack"), str(e)
            self.rec.auto("M4-refused", "restart-update on the tampered file is refused (the ack is an error)", refused, why)
            time.sleep(2)
            starts = [e for e in app.bridge.events() if e["topic"].endswith("/app-started")]
            alive = True
            try:
                app.bridge.send("ping", timeout=10)
            except RuntimeError:
                alive = False
            self.rec.auto("M4-4", "no new process started (and the old one still answers)", len(starts) == 1 and alive,
                          f"{len(starts)} app-started event(s); ping {'answered' if alive else 'NOT answered'}")
            for title, ok, evidence in updater.check_a2(app.dir, app.exe.name, self.old_sha):
                self.rec.auto("M4-5", title, ok, evidence)

    # -- 5 · a download that fails ---------------------------------------------------------------------
    def failing_download(self, label: str, title: str, exe_mode: str | None = None, exe_bytes: bytes | None = None,
                         timeout: float = 120) -> None:
        kw = {} if exe_bytes is None else {"exe_bytes": exe_bytes}
        with self.guard(label), self.new_server(**kw) as server:
            if exe_mode:
                server.exe_mode = exe_mode
            app, started = self.begin(label, server)
            if started is None or not self.announced(label, app):
                return
            began = time.monotonic()
            got = self.start_update(label, app, wait_for="downloaded", timeout=timeout + 60)
            took = time.monotonic() - began
            self.rec.auto(f"{label}-2", title, bool(got) and got.startswith("error"),
                          f"update: {got or 'no answer'} (after {took:.0f} s)")
            left = updater.leftovers(app.dir, app.exe.name)
            self.rec.auto(f"{label}-3", "no half file named update_temp.exe is left", not left["temp"] and not left["part"],
                          "update_temp.exe exists" if left["temp"] else (".part file left" if left["part"] else ""))
            if not (got or "").startswith("error"):
                self.diagnose(app)

    def failing_downloads(self) -> None:
        flipped = bytearray(self.new_exe.read_bytes())
        flipped[len(flipped) // 2] ^= 0xFF
        self.failing_download("M5", "a download that does not match its signature is refused (update: error)",
                              exe_bytes=bytes(flipped))
        self.failing_download("M6", "a connection cut mid-download ends in update: error", exe_mode="cut")
        self.failing_download("M7", "a download that goes silent ends in update: error within the stall limit",
                              exe_mode="stall", timeout=150)

    # -- your real data must be untouched ----------------------------------------------------------------
    def isolation(self) -> None:
        with contextlib.suppress(Exception):
            self._stop(self.runs)  # nothing may be left running
        after = mockfeed.fingerprint(self.real_config)
        self.rec.auto("M-iso", "your real config.json was not touched by any of this", after == self.real_before,
                      "same size, time and hash as before" if after == self.real_before
                      else f"before {self.real_before}, after {after}")
