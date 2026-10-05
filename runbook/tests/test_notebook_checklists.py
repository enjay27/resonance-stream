import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from tools import dryrun  # noqa: E402
from runbook import checklists  # noqa: E402

NB = Path(__file__).resolve().parent.parent / "notebooks" / "checklists.ipynb"
FILE = Path(__file__).resolve().parents[2] / ".memory" / "active-issues" / "unverified-on-windows.md"


def sections():
    items = checklists.plan(checklists.parse_bullets(FILE.read_text(encoding="utf-8")))
    return list(dict.fromkeys(i.section for i in items)), items


def esc_number():
    secs, _ = sections()
    return next(n for n, s in enumerate(secs, 1) if s.startswith("Esc closes"))


def test_k7_and_the_esc_section_are_asked_and_reported(monkeypatch):
    monkeypatch.setenv("RUNBOOK_TEXT_SECTIONS", str(esc_number()))
    report = dryrun.execute(NB, "pass,skip: it pasted,pass," + "pass," * 6 + "fail: IME left the window open")
    assert "pass 8 / fail 1 / skip 1" in report
    assert "K7-2" in report and "esc-closes-a-popup-window-7" in report
    assert "IME left the window open" in report


def test_every_section_runs_with_the_default_selection(monkeypatch):
    _, items = sections()
    monkeypatch.setenv("RUNBOOK_TEXT_SECTIONS", "")
    report = dryrun.execute(NB, ",".join(["pass"] * (3 + len(items))))
    assert f"pass {3 + len(items)} / fail 0 / skip 0" in report


def test_the_covered_jobs_are_not_asked_here(monkeypatch):
    monkeypatch.setenv("RUNBOOK_TEXT_SECTIONS", "")
    _, items = sections()
    report = dryrun.execute(NB, ",".join(["skip"] * (3 + len(items))))
    assert "route-based-interface-pick" not in report and "firewall-rule-per-exe" not in report


def test_a_bad_selection_stops_the_notebook(monkeypatch):
    monkeypatch.setenv("RUNBOOK_TEXT_SECTIONS", "999")
    with pytest.raises(Exception, match="outside"):
        dryrun.execute(NB, "pass,pass,pass")


def test_committed_without_outputs():
    nb = json.loads(NB.read_text(encoding="utf-8"))
    assert all(not c.get("outputs") for c in nb["cells"] if c["cell_type"] == "code")
