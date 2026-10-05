"""`python -m runbook.run <pipeline>`: every pipeline the command line offers can be built from what it parses."""
import sys
import types
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from runbook import run  # noqa: E402
from runbook.common import Recorder  # noqa: E402


@pytest.mark.parametrize("name", sorted(run.PIPELINES))
def test_every_pipeline_can_be_built_from_the_command_line_options(name, tmp_path, monkeypatch):
    monkeypatch.setenv("APPDATA", str(tmp_path / "appdata"))  # the update pipeline looks for the real config
    args = types.SimpleNamespace(version="9.9.9", ui=False)
    pipeline = run.build(name, Recorder(name), tmp_path / "app.exe", args)
    assert callable(pipeline.run)


def test_the_command_line_runs_a_pipeline_it_was_asked_for(monkeypatch, tmp_path, capsys):
    ran = []

    class Fake:
        def __init__(self, rec, exe, **kw):
            ran.append((str(exe), kw))

        def run(self, key_path=None, password=None):
            ran.append(("run", key_path))

    monkeypatch.setitem(run.PIPELINES, "interface", Fake)
    monkeypatch.setenv("RUNBOOK_DRYRUN", "1")  # no Administrator check
    (tmp_path / "x.exe").write_text("x")
    assert run.main(["interface", "--exe", str(tmp_path / "x.exe")]) == 0
    assert ran[0][0].endswith("x.exe") and ran[1] == ("run", None)
    assert "Runbook report" in capsys.readouterr().out


def test_an_exe_that_is_not_there_is_said_plainly(tmp_path, capsys):
    with pytest.raises(SystemExit) as stop:
        run.main(["window-restore", "--exe", str(tmp_path / "missing.exe")])
    assert stop.value.code == 2
    assert "missing.exe" in capsys.readouterr().err
