import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from tools import dryrun  # noqa: E402

NB = Path(__file__).resolve().parent.parent / "notebooks" / "w1-updater.ipynb"
ANSWERS = "pass,pass,pass,skip: hung at 40% (expected on 0.6.0),pass"


@pytest.fixture(autouse=True)
def env(monkeypatch, tmp_path):
    from w1 import common
    monkeypatch.setattr(common, "RUNS", tmp_path)  # the kernel is another process: set below too
    monkeypatch.setenv("W1_RUNS_DIR", str(tmp_path))
    monkeypatch.setenv("W1_TEXT_NEW_TAG", "v0.6.1")
    monkeypatch.setenv("W1_TEXT_BACKUP_KEY", r"C:\keys\backup.key")
    monkeypatch.setenv("W1_TEXT_BACKUP_PW", "not-a-real-password")
    monkeypatch.setenv("APPDATA", str(tmp_path / "appdata"))
    monkeypatch.setenv("W1_TEXT_SETUP_OK", "yes")
    monkeypatch.setenv("W1_TEXT_RESTORE", "yes")


def test_every_path_passes_when_the_app_behaves(monkeypatch):
    report = dryrun.execute(NB, ANSWERS)
    assert "fail 0" in report, report
    assert "skip 1" in report and "hung at 40%" in report
    assert "A2-flip update_temp.exe: one byte flipped" in report
    assert "A5-2 it was signed by the BACKUP key, not the primary | pass | key id 0099CF71" in report


def test_an_update_that_never_happened_is_caught(monkeypatch):
    monkeypatch.setenv("W1_SIM_A1", "nothing")
    report = dryrun.execute(NB, ANSWERS)
    assert "the exe is now the published new version | fail" in report
    assert ".old is left, and is the old version | fail" in report


def test_skipped_steps_are_not_reported_as_failures(monkeypatch):
    # Pressing Enter at a prompt is `skip`: the folder checks behind it did not run, so they are skips too.
    report = dryrun.execute(NB, "skip,skip,skip,skip")
    assert "fail 0" in report, report
    assert "A1 the exe is now the published new version | skip" in report
    assert "A3 no half file named update_temp.exe | skip" in report


def test_an_empty_key_path_is_said_plainly_not_a_tauri_error(monkeypatch):
    monkeypatch.setenv("W1_TEXT_BACKUP_KEY", "")
    report = dryrun.execute(NB, ANSWERS)
    assert "A5-1" in report and "no key path" in report


def test_the_password_never_reaches_the_report(monkeypatch):
    assert "not-a-real-password" not in dryrun.execute(NB, ANSWERS)


def test_committed_without_outputs():
    nb = json.loads(NB.read_text(encoding="utf-8"))
    assert all(not c.get("outputs") for c in nb["cells"] if c["cell_type"] == "code")


def test_the_running_app_being_another_copy_is_caught(monkeypatch):
    # the A1 run on Kade's PC changed another folder than the one the notebook watched
    monkeypatch.setenv("W1_SIM_RUNNING", "other")
    report = dryrun.execute(NB, ANSWERS)
    assert "A2-which the running app is the A2 copy | fail" in report


def test_the_right_copy_running_passes(monkeypatch):
    assert "A2-which the running app is the A2 copy | pass" in dryrun.execute(NB, ANSWERS)


def config_file(tmp_path):
    return tmp_path / "appdata" / "com.enjay.bpsr.resonance-stream" / "config.json"


def test_a_fresh_app_is_set_up_for_the_run_and_put_back_afterwards(tmp_path):
    report = dryrun.execute(NB, ANSWERS)
    assert "K2-prep config.json marked init_done" in report and "fail 0" in report
    assert not config_file(tmp_path).exists()  # it was created by the notebook, so restore removes it


def test_declining_the_restore_keeps_the_marked_config(tmp_path, monkeypatch):
    monkeypatch.setenv("W1_TEXT_RESTORE", "no")
    dryrun.execute(NB, ANSWERS)
    assert json.loads(config_file(tmp_path).read_text()) == {"init_done": True}


def test_declining_the_setup_leaves_the_data_folder_alone_and_says_so(tmp_path, monkeypatch):
    monkeypatch.setenv("W1_TEXT_SETUP_OK", "no")
    report = dryrun.execute(NB, ANSWERS)
    assert "K2-prep the app is set up for the update checks | skip" in report
    assert not config_file(tmp_path).exists()


def test_an_app_that_is_already_set_up_is_not_touched(tmp_path):
    cfg = config_file(tmp_path)
    cfg.parent.mkdir(parents=True)
    cfg.write_text('{"init_done": true, "theme": "dark"}')
    report = dryrun.execute(NB, ANSWERS)
    assert "K2-prep the app is set up (init_done)" in report
    assert cfg.read_text() == '{"init_done": true, "theme": "dark"}'


def test_every_cell_that_starts_the_app_does_it_inside_a_step_that_closes_it():
    import nbformat

    nb = nbformat.read(str(NB), as_version=4)
    launching = [c.source for c in nb.cells if c.cell_type == "code" and "updater.launch(" in c.source]
    assert len(launching) >= 4, "A1-A4 start copies"
    for source in launching:
        assert "with updater.step(" in source, source[:120]
        # the launch is indented under the with: not before it
        assert source.index("with updater.step(") < source.index("updater.launch(")
