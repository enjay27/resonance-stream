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

from runbook import bridge, common
from runbook.common import Recorder
from runbook.pipelines.capture_spike import CaptureSpike
from runbook.pipelines.interface import InterfacePick
from runbook.pipelines.updater_mock import UpdaterMock
from runbook.pipelines.window_restore import WindowRestore

PIPELINES = {"updater-mock": UpdaterMock, "interface": InterfacePick, "window-restore": WindowRestore, "capture-spike": CaptureSpike}


def build(name: str, rec: Recorder, exe, args):
    """The pipeline `name`, with only the options it takes (`--version` and `--ui` are the update pipeline's, `--add-firewall-rule` the capture spike's)."""
    options = {}
    if name == "updater-mock":
        options = {"new_version": args.version, "ui_checks": args.ui}
    elif name == "capture-spike":
        options = {"add_firewall_rule": args.add_firewall_rule}
    return PIPELINES[name](rec, exe, **options)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="python -m runbook.run")
    parser.add_argument("pipeline", choices=sorted(PIPELINES))
    parser.add_argument("--exe", default=None, help="a test-env build of the app (default: RUNBOOK_TEXT_LOCAL_EXE, "
                                                    "else target/release/resonance-stream.exe in this checkout)")
    parser.add_argument("--key", default=os.environ.get("RUNBOOK_TEXT_BACKUP_KEY"), help="the BACKUP signing key file")
    parser.add_argument("--version", default="9.9.9", help="the version the mock release announces")
    parser.add_argument("--ui", action="store_true", help="also ask about what the dialog looked like")
    parser.add_argument("--add-firewall-rule", action="store_true",
                        help="capture-spike: make the firewall rule the app wants for each copy it starts (this copy only, any remote "
                             "address) and remove it at the end -- edits the Windows firewall, so it is never on by default")
    args = parser.parse_args(argv)
    exe = args.exe or os.environ.get("RUNBOOK_TEXT_LOCAL_EXE") or (
        str(common.default_exe()) if common.default_exe().is_file() else None)
    if not exe:
        parser.error(f"--exe is needed: there is no build at {common.default_exe()}")
    if not os.path.isfile(exe):
        parser.error(f"the exe {exe} is not a file (build it: cargo tauri build --no-bundle --features test-env)")
    rec = Recorder(args.pipeline)
    bridge.ensure_installed()  # npm ci in runbook/bridge, the first time
    if not common.require_windows_admin(rec):
        print(rec.report())
        return 1
    pipeline = build(args.pipeline, rec, exe, args)
    pipeline.run(key_path=args.key, password=os.environ.get("RUNBOOK_TEXT_BACKUP_PW"))
    print()
    print(rec.report())
    rec.save()
    return 1 if rec.summary()["fail"] else 0


if __name__ == "__main__":
    sys.exit(main())
