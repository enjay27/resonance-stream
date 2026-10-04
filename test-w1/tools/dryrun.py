"""Run a W1 notebook headless, the way CI-less Linux sessions prove it executes.

    python tools/dryrun.py notebooks/w1-updater.ipynb "pass,pass,fail:why"

W1_DRYRUN=1 swaps commands for recorded fixtures; the answers feed the manual prompts.
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
    os.environ["W1_DRYRUN"] = "1"
    os.environ["W1_ANSWERS"] = answers
    nb = nbformat.read(str(path), as_version=4)
    NotebookClient(nb, timeout=120, kernel_name="python3",
                   resources={"metadata": {"path": str(Path(path).resolve().parent)}}).execute()  # as Jupyter: the notebook's folder
    last = ""
    for cell in nb.cells:
        for out in cell.get("outputs", []):
            if out.get("output_type") == "stream":
                last = out["text"]
    return last


if __name__ == "__main__":
    print(execute(sys.argv[1], sys.argv[2] if len(sys.argv) > 2 else ""))
