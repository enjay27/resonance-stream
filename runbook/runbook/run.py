"""Run a pipeline with no one at the keyboard:

    python -m runbook.run updater-mock --key C:\\keys\\backup.key      # the exe: target\\release\\resonance-stream.exe

The backup key's password is asked once per exe (the signature is kept in runs/mock/signed/); set
RUNBOOK_TEXT_BACKUP_PW to answer it too. Run from an Administrator terminal on Windows. Prints the report block;
exit code 0 when no row failed.
"""
from __future__ import annotations

import argparse
import os
import sys

from runbook import common
from runbook.common import Recorder
from runbook.pipelines.interface import InterfacePick
from runbook.pipelines.updater_mock import UpdaterMock

PIPELINES = {"updater-mock": UpdaterMock, "interface": InterfacePick}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="python -m runbook.run")
    parser.add_argument("pipeline", choices=sorted(PIPELINES))
    parser.add_argument("--exe", default=None, help="a test-env build of the app (default: RUNBOOK_TEXT_LOCAL_EXE, "
                                                    "else target/release/resonance-stream.exe in this checkout)")
    parser.add_argument("--key", default=os.environ.get("RUNBOOK_TEXT_BACKUP_KEY"), help="the BACKUP signing key file")
    parser.add_argument("--version", default="9.9.9", help="the version the mock release announces")
    parser.add_argument("--ui", action="store_true", help="also ask about what the dialog looked like")
    args = parser.parse_args(argv)
    exe = args.exe or os.environ.get("RUNBOOK_TEXT_LOCAL_EXE") or (
        str(common.default_exe()) if common.default_exe().is_file() else None)
    if not exe:
        parser.error(f"--exe is needed: there is no build at {common.default_exe()}")
    rec = Recorder(args.pipeline)
    if not common.require_windows_admin(rec):
        print(rec.report())
        return 1
    pipeline = PIPELINES[args.pipeline](rec, exe, new_version=args.version, ui_checks=args.ui)
    pipeline.run(key_path=args.key, password=os.environ.get("RUNBOOK_TEXT_BACKUP_PW"))
    print()
    print(rec.report())
    rec.save()
    return 1 if rec.summary()["fail"] else 0


if __name__ == "__main__":
    sys.exit(main())
