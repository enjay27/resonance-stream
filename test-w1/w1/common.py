"""What every W1 notebook shares: a result recorder, manual-step prompts, command capture.

A notebook does what a script can and *asks* for the rest (`manual`). Every check ends as
pass / fail / skip with evidence, and `Recorder.report()` prints one block to paste back.
Set W1_DRYRUN=1 (and W1_ANSWERS="pass,fail:why,...") to run a notebook headless on any OS:
commands are replaced by recorded fixtures and prompts are answered from the queue.
"""
from __future__ import annotations

import ctypes
import datetime
import json
import os
import platform
import shutil
import subprocess
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "tests" / "fixtures"
RUNS = Path(os.environ.get("W1_RUNS_DIR", ROOT / "runs"))

STATUSES = ("pass", "fail", "skip")


def dry_run() -> bool:
    return os.environ.get("W1_DRYRUN") == "1"


def is_windows() -> bool:
    return platform.system() == "Windows"


def is_admin() -> bool:
    """True when this process is elevated (Windows). The app's raw socket needs it."""
    if not is_windows():
        return False
    try:
        return bool(ctypes.windll.shell32.IsUserAnAdmin())  # type: ignore[attr-defined]
    except Exception:
        return False


def parse_answer(text: str) -> tuple[str, str]:
    """'p', 'pass', 'y' -> pass; 'f', 'fail', 'n' -> fail; 's', 'skip' -> skip.

    Anything after the first space or colon is the note: 'fail: bar stays at 0%'.
    An answer that is none of these (or empty) is skip with the text as the note --
    a typo never becomes a pass.
    """
    text = text.strip()
    if not text:
        return "skip", ""
    head, _, note = text.replace(":", " ", 1).partition(" ")
    word = head.lower()
    if word in ("p", "pass", "y", "yes", "ok"):
        return "pass", note.strip()
    if word in ("f", "fail", "n", "no"):
        return "fail", note.strip()
    if word in ("s", "skip"):
        return "skip", note.strip()
    return "skip", text


def _next_answer(prompt: str) -> str:
    queue = os.environ.get("W1_ANSWERS")
    if queue is not None:
        items = queue.split(",")
        if not items or items == [""]:
            raise RuntimeError(f"W1_ANSWERS ran out at: {prompt}")
        os.environ["W1_ANSWERS"] = ",".join(items[1:])
        return items[0]
    return input(prompt)


def ask_text(name: str, prompt: str) -> str:
    """Free text from the person (a path, a pasted log line). Quotes and spaces around it
    are dropped -- Explorer's "Copy as path" adds quotes. In a dry run the text comes from
    the environment variable W1_TEXT_<name>."""
    if dry_run():
        value = os.environ.get(f"W1_TEXT_{name}")
        if value is None:
            raise RuntimeError(f"dry run: set W1_TEXT_{name}")
        return value.strip()
    return input(prompt).strip().strip('"').strip()


@dataclass
class Row:
    check: str
    title: str
    status: str
    evidence: str = ""


@dataclass
class Recorder:
    job: str
    rows: list[Row] = field(default_factory=list)

    def record(self, check: str, title: str, status: str, evidence: str = "") -> Row:
        if status not in STATUSES:
            raise ValueError(f"status must be one of {STATUSES}, not {status!r}")
        row = Row(check, title, status, evidence.strip())
        self.rows.append(row)
        print(f"[{status.upper():4}] {check} {title}" + (f" -- {row.evidence}" if row.evidence else ""))
        return row

    def auto(self, check: str, title: str, ok: bool, evidence: str = "") -> Row:
        """An automated check: pass or fail, never skip."""
        return self.record(check, title, "pass" if ok else "fail", evidence)

    def manual(self, check: str, title: str, steps: str, expect: str) -> Row:
        """A step only a person can do: print what to do, ask how it went."""
        print(f"\n=== {check}: {title} ===\n{steps}\nExpect: {expect}")
        status, note = parse_answer(_next_answer("pass / fail / skip [: note] > "))
        return self.record(check, title, status, note)

    def summary(self) -> dict[str, int]:
        return {s: sum(1 for r in self.rows if r.status == s) for s in STATUSES}

    def report(self) -> str:
        """One markdown block to paste back; also saved under runs/."""
        stamp = datetime.datetime.now().astimezone().isoformat(timespec="seconds")
        counts = self.summary()
        lines = [
            f"W1 report: {self.job} @ {stamp} on {platform.node()} "
            f"({platform.platform()}, admin={is_admin()})",
            f"pass {counts['pass']} / fail {counts['fail']} / skip {counts['skip']}",
            "",
            "| check | result | evidence |",
            "|---|---|---|",
        ]
        for r in self.rows:
            evidence = r.evidence.replace("|", "\\|").replace("\n", " ")
            lines.append(f"| {r.check} {r.title} | {r.status} | {evidence} |")
        text = "\n".join(lines)
        self.save()
        return text

    def save(self) -> Path | None:
        if dry_run():
            return None
        RUNS.mkdir(exist_ok=True)
        stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
        path = RUNS / f"{self.job}-{stamp}.json"
        path.write_text(
            json.dumps([r.__dict__ for r in self.rows], ensure_ascii=False, indent=2),
            encoding="utf-8",
        )
        return path


def capture(command: list[str], fixture: str | None = None, timeout: int = 120) -> tuple[int, str]:
    """Run a command, return (exit code, stdout + stderr).

    In a dry run the named fixture's text is returned instead, exit code 0 -- the notebook's
    parsing and checks still run, the system is not touched.
    """
    if dry_run():
        if fixture is None:
            raise RuntimeError(f"dry run: no fixture for {command!r}")
        return 0, (FIXTURES / fixture).read_text(encoding="utf-8")
    # `npx` is `npx.CMD` on Windows: without a shell only the resolved path starts.
    command = [shutil.which(command[0]) or command[0], *command[1:]]
    try:
        done = subprocess.run(
            command, capture_output=True, text=True, timeout=timeout, encoding="utf-8", errors="replace"
        )
    except FileNotFoundError:
        return 127, f"not found: {command[0]}"
    except subprocess.TimeoutExpired:
        return 124, f"timed out after {timeout}s: {' '.join(command)}"
    return done.returncode, (done.stdout or "") + (done.stderr or "")


def fetch_text(url: str, fixture: str | None = None, timeout: float = 30) -> str:
    """GET a URL as the app would (redirects followed). A dry run reads the fixture instead."""
    if dry_run():
        if fixture is None:
            raise RuntimeError(f"dry run: no fixture for {url}")
        return (FIXTURES / fixture).read_text(encoding="utf-8")
    import urllib.request

    with urllib.request.urlopen(url, timeout=timeout) as response:  # noqa: S310 -- fixed https URL
        return response.read().decode("utf-8")


def require_windows_admin(rec: Recorder) -> bool:
    """Record the starting condition of a Windows job; False means stop."""
    if dry_run():
        return True
    ok = is_windows() and is_admin()
    rec.auto("0", "Windows, elevated (Run as administrator)", ok,
             "" if ok else "start Jupyter from an Administrator terminal")
    return ok
