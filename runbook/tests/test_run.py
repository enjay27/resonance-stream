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
    args = types.SimpleNamespace(version="9.9.9", ui=False, add_firewall_rule=False)
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


def test_output_with_japanese_survives_a_windows_console_codepage():
    """On the hosted Windows runner stdout is a pipe in cp1252: printing a report row with Japanese text raised
    `UnicodeEncodeError: 'charmap' codec can't encode characters` and `translator-stub` stopped after `TS-ready`."""
    import os
    import subprocess
    import sys

    code = ("from runbook import common\n"
            "common.utf8_output()\n"
            "print('[PASS] TS-translate -- 日本語のテスト')\n")
    done = subprocess.run([sys.executable, "-c", code], capture_output=True, cwd=os.path.dirname(os.path.dirname(__file__)),
                          env={**os.environ, "PYTHONIOENCODING": "cp1252", "PYTHONUTF8": "0"})
    assert done.returncode == 0, done.stderr.decode("utf-8", "replace")
    assert "日本語のテスト" in done.stdout.decode("utf-8")


def test_main_makes_its_output_utf8_before_anything_prints(monkeypatch):
    from runbook import run

    seen = []
    monkeypatch.setattr(run.common, "utf8_output", lambda: seen.append(True))
    with pytest.raises(SystemExit):
        run.main(["popups", "--exe", "no-such-file.exe"])
    assert seen == [True]
