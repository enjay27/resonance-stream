import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from runbook import checklists  # noqa: E402

REAL = Path(__file__).resolve().parents[2] / ".memory" / "active-issues" / "unverified-on-windows.md"

SAMPLE = """# Unverified on Windows

Some intro text.

- **Alpha thing (2026-10-01, `claude/alpha`).** Tested here. Check on Windows
  (`cargo tauri dev`): (1) open it; it shows; (2) press **Esc** -> it hides;
  (3) done.
- **Beta thing.** No numbered list here, one paragraph
  that continues on a second line.

- **Gamma (K9).** Worth a real run: (1) a; (2) b.
"""


def test_parse_bullets_splits_on_dash_bold_and_joins_continuations():
    bullets = checklists.parse_bullets(SAMPLE)
    assert [b.title for b in bullets] == ["Alpha thing", "Beta thing", "Gamma"]
    assert "press **Esc** -> it hides" in bullets[0].body
    assert bullets[1].body.endswith("continues on a second line.")


def test_the_title_stops_at_the_parenthesis_or_the_full_stop():
    assert checklists.parse_bullets("- **Plain title.** body")[0].title == "Plain title"
    assert checklists.parse_bullets("- **With date (2026-10-01).** body")[0].title == "With date"


def test_numbered_checks_are_split_in_order():
    pre, items = checklists.numbered_checks(checklists.parse_bullets(SAMPLE)[0].body)
    assert items == ["open it; it shows;", "press **Esc** -> it hides;", "done."]
    assert "Check on Windows" in pre


def test_text_without_a_numbered_list_has_no_items():
    pre, items = checklists.numbered_checks("Just words, with (a) letters and a 2020 year.")
    assert items == [] and pre.startswith("Just words")


def test_numbering_must_start_at_one_and_count_up():
    # "(2) ... (3) ..." with no (1) is not a list; neither is a repeated number
    assert checklists.numbered_checks("see (2) a and (3) b")[1] == []
    assert checklists.numbered_checks("(1) a (1) b")[1] == ["a (1) b"]


def test_the_three_jobs_with_their_own_notebook_are_covered_elsewhere():
    assert checklists.covered_elsewhere("Route-based interface pick") == "interface"
    assert checklists.covered_elsewhere("Firewall rule per exe") == "firewall"
    assert checklists.covered_elsewhere("Signed app updates") == "updater"
    assert checklists.covered_elsewhere("Cheat sheet") is None


def test_plan_has_unique_ids_and_skips_what_is_covered_elsewhere():
    items = checklists.plan(checklists.parse_bullets(SAMPLE), covered={"Gamma"})
    ids = [i.check for i in items]
    assert len(ids) == len(set(ids))
    assert all(not i.section.startswith("Gamma") for i in items)
    assert [i.check for i in items if i.section == "Alpha thing"] == ["alpha-thing-1", "alpha-thing-2", "alpha-thing-3"]
    assert [i.check for i in items if i.section == "Beta thing"] == ["beta-thing"]


@pytest.mark.parametrize("text,n,expected", [
    ("", 5, [0, 1, 2, 3, 4]), ("all", 3, [0, 1, 2]), ("2", 5, [1]), ("1,3", 5, [0, 2]),
    ("2-4", 5, [1, 2, 3]), (" 1 , 4-5 ", 5, [0, 3, 4]), ("3,3,1", 5, [0, 2]),
])
def test_parse_selection(text, n, expected):
    assert checklists.parse_selection(text, n) == expected


@pytest.mark.parametrize("text", ["0", "6", "x", "3-2", "1,,2", "-1"])
def test_a_bad_selection_is_an_error(text):
    with pytest.raises(ValueError):
        checklists.parse_selection(text, 5)


# --- against the real file, so the notebook cannot drift from it -----------------------------------
def real():
    return checklists.parse_bullets(REAL.read_text(encoding="utf-8"))


def test_the_real_file_has_bullets_with_titles():
    bullets = real()
    assert len(bullets) > 10 and all(b.title for b in bullets)


def test_the_esc_bullet_is_found_and_has_seven_numbered_checks():
    esc = next(b for b in real() if b.title.startswith("Esc closes a popup window"))
    assert len(checklists.numbered_checks(esc.body)[1]) == 7


def test_the_real_plan_leaves_out_the_three_covered_jobs():
    sections = {i.section for i in checklists.plan(real())}
    assert not any(s.startswith(("Route-based", "Firewall rule per exe", "Signed app updates")) for s in sections)
    assert any(s.startswith("Esc closes") for s in sections)


# --- K7 ------------------------------------------------------------------------------------------------
def test_k7_is_three_questions_with_the_failure_answers_spelled_out():
    assert len(checklists.K7) == 3
    assert all(item.steps and item.expect for item in checklists.K7)
    assert "CLIPBOARD_RESTORE_DELAY" in " ".join(i.steps + i.expect for i in checklists.K7)
