"""Run a runbook notebook headless, the way CI-less Linux sessions prove it executes.

    python tools/dryrun.py notebooks/updater.ipynb "pass,pass,fail:why"

RUNBOOK_DRYRUN=1 swaps commands for recorded fixtures; the answers feed the manual prompts.
This proves the cells run and the checks read their fixtures -- it is not a Windows run.
"""
from __future__ import annotations

import os
import sys
from pathlib import Path

import nbformat
from nbclient import NotebookClient


def execute(path: str | Path, answers: str = "") -> str:
    """Execute every cell; return the text of the last cell's stdout."""
    # The kernel inherits this process's environment; put it back afterwards, or every later test in
    # the same process would silently run in dry-run mode.
    keys = ("RUNBOOK_DRYRUN", "RUNBOOK_ANSWERS")
    before = {k: os.environ.get(k) for k in keys}
    os.environ["RUNBOOK_DRYRUN"] = "1"
    os.environ["RUNBOOK_ANSWERS"] = answers
    try:
        nb = nbformat.read(str(path), as_version=4)
        NotebookClient(nb, timeout=120, kernel_name="python3",
                       resources={"metadata": {"path": str(Path(path).resolve().parent)}}).execute()  # as Jupyter: the notebook's folder
    finally:
        for k, v in before.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
    last = ""
    for cell in nb.cells:
        for out in cell.get("outputs", []):
            if out.get("output_type") == "stream":
                last = out["text"]
    return last


if __name__ == "__main__":
    print(execute(sys.argv[1], sys.argv[2] if len(sys.argv) > 2 else ""))
