import os
import sys
from pathlib import Path

import nbformat
from nbformat.v4 import new_code_cell, new_notebook

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from tools import dryrun  # noqa: E402


def tiny_notebook(tmp_path):
    nb = new_notebook(cells=[new_code_cell("import os; print('dry', os.environ.get('RUNBOOK_DRYRUN'), os.environ.get('RUNBOOK_ANSWERS'))")])
    nb.metadata["kernelspec"] = {"display_name": "Python 3", "language": "python", "name": "python3"}
    path = tmp_path / "notebooks" / "tiny.ipynb"
    path.parent.mkdir()
    nbformat.write(nb, str(path))
    return path


def test_the_kernel_sees_the_dry_run_and_the_answers(tmp_path):
    assert "dry 1 pass,fail" in dryrun.execute(tiny_notebook(tmp_path), "pass,fail")


def test_running_a_notebook_leaves_this_process_environment_as_it_was(tmp_path, monkeypatch):
    monkeypatch.delenv("RUNBOOK_DRYRUN", raising=False)
    monkeypatch.delenv("RUNBOOK_ANSWERS", raising=False)
    dryrun.execute(tiny_notebook(tmp_path), "pass")
    assert "RUNBOOK_DRYRUN" not in os.environ and "RUNBOOK_ANSWERS" not in os.environ


def test_values_that_were_set_before_are_put_back(tmp_path, monkeypatch):
    monkeypatch.setenv("RUNBOOK_DRYRUN", "0")
    monkeypatch.setenv("RUNBOOK_ANSWERS", "keep")
    dryrun.execute(tiny_notebook(tmp_path), "pass")
    assert os.environ["RUNBOOK_DRYRUN"] == "0" and os.environ["RUNBOOK_ANSWERS"] == "keep"
