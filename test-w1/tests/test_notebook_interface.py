import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from tools import dryrun  # noqa: E402

NB = Path(__file__).resolve().parent.parent / "notebooks" / "w1-interface.ipynb"
GOOD1 = "Auto-Targeting Network Interface: 192.168.0.23 (default route)"
GOOD2 = "Auto-Targeting Network Interface: 192.168.0.23 (first physical adapter)"


def run(monkeypatch, log1, log2, answers):
    monkeypatch.setenv("W1_TEXT_LOG1", log1)
    monkeypatch.setenv("W1_TEXT_LOG2", log2)
    return dryrun.execute(NB, answers)


def test_both_scenarios_pass_with_the_right_lines(monkeypatch):
    report = run(monkeypatch, GOOD1, GOOD2, "pass,pass,pass")
    assert "fail 0" in report and "pass 11 / fail 0 / skip 0" in report


def test_the_vpn_open_question_lands_in_the_report(monkeypatch):
    report = run(monkeypatch, GOOD1, GOOD2, "pass,fail: packets only on the VPN adapter,pass")
    assert "fail 1" in report and "packets only on the VPN adapter" in report


def test_the_app_picking_the_vpn_adapter_is_reported(monkeypatch):
    report = run(monkeypatch, GOOD1, "Auto-Targeting Network Interface: 10.8.0.2 (default route)", "pass,pass,pass")
    assert "fail 2" in report  # the rule and the address


def test_a_line_that_is_not_the_log_line_stops_the_notebook(monkeypatch):
    import pytest
    with pytest.raises(Exception, match="Auto-Targeting"):
        run(monkeypatch, "Sniffer: Binding", GOOD2, "pass,pass,pass")


def test_committed_without_outputs():
    nb = json.loads(NB.read_text(encoding="utf-8"))
    assert all(not c.get("outputs") for c in nb["cells"] if c["cell_type"] == "code")
