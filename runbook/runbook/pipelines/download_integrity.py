"""Pipeline `download-integrity`: the model download refuses what it must refuse, and never breaks the model that is installed.

A mock server on 127.0.0.1 (`mockfeed.MockServer`, `/model.gguf`) stands in for GitHub; the app is started with `--metadata-url` pointing
at it, which is what lets a test run download over local http. The test plays the setup wizard over the bridge: `download-model` with the
url, version and SHA-256 the UI would take from the gist, and the end is the `download-result` event. After every case the installed model
file is looked at on disk, and so is the folder for a partial file. Rows:

  DI-good     a good download installs the model (its SHA-256 on disk is the published one), shows progress up to 100 %, leaves no partial file
  DI-skip     the same model again is not downloaded again (the server is not asked), progress says it was skipped
  DI-hash     bytes that do not match the published SHA-256 are refused; the installed model and the folder are as they were
  DI-cut      a download the server cuts off half way is refused, likewise
  DI-404      a server that answers 404 is a failure, likewise
  DI-nohash   a model with no published SHA-256 is refused before anything is downloaded
  DI-https    an address that is neither https nor this machine's is refused before anything is downloaded

(The stall case -- a server that goes silent -- takes the app's 30 s stall limit and is covered for the app update by `updater-mock`.)
"""
from __future__ import annotations

import hashlib
from pathlib import Path

from runbook import bridge, common, mockfeed, updater
from runbook.common import Recorder


def blob(seed: str, size: int = 3 * 1024 * 1024) -> bytes:
    """Deterministic pseudo-random bytes: big enough for progress to move in more than one step."""
    out, counter = bytearray(), 0
    while len(out) < size:
        out += hashlib.sha256(f"{seed}:{counter}".encode()).digest()
        counter += 1
    return bytes(out[:size])


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


class DownloadIntegrity:
    def __init__(self, rec: Recorder, exe: str | Path, runs: Path | None = None) -> None:
        self.rec = rec
        self.exe = Path(exe)
        self.runs = Path(runs or common.RUNS) / "download-integrity"
        self._serves: list[bridge.Serve] = []
        self._servers: list[mockfeed.MockServer] = []

    def run(self, key_path: str | None = None, password: str | None = None) -> None:
        """(`key_path` and `password` are for the pipelines that sign; this one ignores them.)"""
        with updater.step(self.rec, "DI", self.runs, strays=self.runs, stop=self._stop):
            self.round()

    def _stop(self, folder: str | Path) -> list[int]:
        for server in self._servers:
            server.stop()
        self._servers.clear()
        for serve in self._serves:
            serve.stop()
        self._serves.clear()
        return mockfeed.stop_copies(folder)

    @staticmethod
    def model_dir(exe: Path) -> Path:
        return exe.parent / "data" / "data" / "models" / "translation-model"

    def state(self, exe: Path) -> str:
        """The installed model's SHA-256 (or 'none') and the files next to it that are not the model: what must not change."""
        folder = self.model_dir(exe)
        model = folder / "model.gguf"
        digest = sha(model.read_bytes()) if model.exists() else "none"
        extra = sorted(p.name for p in folder.glob("*") if p.name != "model.gguf") if folder.exists() else []
        return f"model {digest[:12]}, other files {extra}"

    def download(self, events: bridge.Serve, url: str, digest: str) -> dict:
        """`download-model` and what the app said when it ended: the `download-result` payload."""
        ack = events.send("download-model", {"url": url, "version": "runbook", "sha256": digest}, timeout=15)
        return events.expect("download-result", {"payload.id": ack["id"]}, timeout=90)["payload"]

    def round(self) -> None:
        good, other = blob("good"), blob("other")
        server = mockfeed.MockServer().start()
        self._servers.append(server)
        server.model_bytes = good
        exe = updater.fresh_copy(self.exe, self.runs, "run")
        events = bridge.Serve(exe.parent / "events.jsonl")
        self._serves.append(events)
        args = mockfeed.flag_args(exe.parent / "data", exe.parent / "status.json", log_file=exe.parent / "app.log",
                                  metadata_url=server.metadata_url, extra=("--no-update-check", "--bridge-url", events.url))
        mockfeed.start_app(exe, args)
        if not bridge.wait_started(events, exe.parent / "app.log"):
            self.rec.auto("DI-start", "the app started and connected to the bridge", False, "no app-started event in 120 s")
            return
        url = server.base_url + "/model.gguf"

        # DI-good
        result = self.download(events, url, sha(good))
        installed = (self.model_dir(exe) / "model.gguf")
        progress = [m["message"]["payload"]["percent"] for m in events.events() if m["topic"] == "rs/app/event/download-progress"]
        ok = (result["ok"] and installed.exists() and sha(installed.read_bytes()) == sha(good) and 100 in progress
              and self.state(exe) == f"model {sha(good)[:12]}, other files []")
        self.rec.auto("DI-good", "a good download installs the model, shows progress to 100 %, leaves no partial file", ok,
                      f"result {result}; {self.state(exe)}; progress steps {len(set(progress))}, last {progress[-1] if progress else None}")
        reference = self.state(exe)

        # DI-skip
        before = len(server.hits)
        result = self.download(events, url, sha(good))
        skipped = [m for m in events.events() if m["topic"] == "rs/app/event/download-progress"
                   and "Skipped" in str(m["message"]["payload"].get("current_file"))]
        self.rec.auto("DI-skip", "the model it already has is not downloaded again", result["ok"] and len(server.hits) == before and bool(skipped),
                      f"result {result}; server asked {len(server.hits) - before} more time(s); 'Skipped' progress seen: {bool(skipped)}")

        def refused(check: str, title: str, url_: str, digest: str, *, asks_server: bool, needle: str = "") -> None:
            hits = len(server.hits)
            result = self.download(events, url_, digest)
            unchanged = self.state(exe) == reference
            asked = len(server.hits) - hits
            ok = (not result["ok"] and bool(result.get("error")) and needle.lower() in str(result["error"]).lower()
                  and unchanged and (asked > 0 if asks_server else asked == 0))
            self.rec.auto(check, title, ok, f"ok={result['ok']}, error={result.get('error')!r}; {self.state(exe)} "
                                            f"(was: {reference}); server asked {asked} time(s)")

        server.model_bytes = other
        refused("DI-hash", "bytes that do not match the published SHA-256 are refused and change nothing", url, sha(good)[::-1], asks_server=True)
        server.model_mode = "cut"
        refused("DI-cut", "a download cut off half way is refused and changes nothing", url, sha(other), asks_server=True)
        server.model_mode = "404"
        refused("DI-404", "a 404 is a failure and changes nothing", url, sha(other), asks_server=True)
        server.model_mode = "ok"
        refused("DI-nohash", "a model with no published SHA-256 is refused before anything is downloaded", url, "", asks_server=False, needle="sha-256")
        refused("DI-https", "an address that is neither https nor this machine's is refused before anything is downloaded",
                "http://203.0.113.9/model.gguf", sha(other), asks_server=False, needle="non-HTTPS")
        try:
            events.send("quit", timeout=15)
        except RuntimeError:
            pass
