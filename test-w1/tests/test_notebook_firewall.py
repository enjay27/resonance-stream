import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from tools import dryrun  # noqa: E402

NB = Path(__file__).resolve().parent.parent / "notebooks" / "w1-firewall.ipynb"


def test_the_notebook_runs_headless_and_reports_every_check(monkeypatch):
    monkeypatch.setenv("W1_TEXT_DEV_EXE", r"C:\dev\resonance-stream\target\debug\resonance-stream.exe")
    monkeypatch.setenv("W1_TEXT_RELEASE_EXE", r"C:\Program Files\Resonance Stream\resonance-stream.exe")
    report = dryrun.execute(NB, "pass,pass,fail: wizard came back")
    assert "pass 6 / fail 1 / skip 0" in report
    assert "wizard came back" in report
    assert "rule for C:\\Program Files" in report


def test_the_notebook_is_committed_without_outputs():
    import json
    nb = json.loads(NB.read_text(encoding="utf-8"))
    assert all(not c.get("outputs") for c in nb["cells"] if c["cell_type"] == "code")
    assert all(c.get("execution_count") in (None, 0) for c in nb["cells"] if c["cell_type"] == "code")
