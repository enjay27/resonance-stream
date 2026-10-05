"""K7, and K25 (everything left in .memory/active-issues/unverified-on-windows.md).

K25 is read from that file when the notebook runs, so the checklist cannot drift from it: every
`- **Title (...).** ... (1) ...; (2) ...` bullet becomes a section, every numbered check a prompt.
"""
from __future__ import annotations

import re
from dataclasses import dataclass

# Jobs that have a notebook of their own: left out of K25.
COVERED = {
    "Route-based interface pick": "interface",
    "Firewall rule per exe": "firewall",
    "Signed app updates": "updater",
}


@dataclass
class Bullet:
    title: str
    body: str


@dataclass
class Item:
    check: str
    section: str
    title: str
    steps: str
    expect: str
    preface: str = ""


_BULLET = re.compile(r"^- \*\*(.+?)\*\*\s*(.*)$")


def _title(bold: str) -> str:
    return re.split(r"\s\(", bold.strip().rstrip("."), maxsplit=1)[0].strip().rstrip(".")


def parse_bullets(md: str) -> list[Bullet]:
    """`- **Title (...).** text` bullets; indented lines continue the bullet, anything else ends it."""
    bullets: list[Bullet] = []
    current: list[str] | None = None
    title = ""
    for line in md.splitlines():
        m = _BULLET.match(line)
        if m:
            if current is not None:
                bullets.append(Bullet(title, " ".join(current).strip()))
            title, current = _title(m.group(1)), [m.group(2)]
        elif current is not None and line.startswith("  ") and line.strip():
            current.append(line.strip())
        elif current is not None:
            bullets.append(Bullet(title, " ".join(current).strip()))
            current = None
    if current is not None:
        bullets.append(Bullet(title, " ".join(current).strip()))
    return bullets


def numbered_checks(body: str) -> tuple[str, list[str]]:
    """(text before the list, the items of `(1) ... (2) ...`). The numbers must run 1, 2, 3, ...;
    anything else that looks like (n) stays part of the text."""
    marks: list[tuple[int, int]] = []  # (start, end) of each marker that continues the count
    for m in re.finditer(r"\((\d+)\)\s", body):
        if int(m.group(1)) == len(marks) + 1:
            marks.append((m.start(), m.end()))
    if not marks:
        return body.strip(), []
    items = []
    for i, (_, end) in enumerate(marks):
        stop = marks[i + 1][0] if i + 1 < len(marks) else len(body)
        items.append(body[end:stop].strip())
    return body[: marks[0][0]].strip(), items


def covered_elsewhere(title: str) -> str | None:
    return next((job for prefix, job in COVERED.items() if title.startswith(prefix)), None)


def _slug(text: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")[:40] or "item"


def plan(bullets: list[Bullet], covered: set[str] | None = None) -> list[Item]:
    """Every prompt K25 asks, in file order. Sections with a notebook of their own (or named in
    `covered`) are left out."""
    covered = covered or set()
    items: list[Item] = []
    used: set[str] = set()

    def unique(base: str) -> str:
        name, n = base, 2
        while name in used:
            name, n = f"{base}-{n}", n + 1
        used.add(name)
        return name

    for b in bullets:
        if b.title in covered or covered_elsewhere(b.title):
            continue
        preface, checks = numbered_checks(b.body)
        base = unique(_slug(b.title))
        if checks:
            for i, text in enumerate(checks, 1):
                items.append(Item(f"{base}-{i}", b.title, f"{b.title} ({i}/{len(checks)})", text,
                                  "as described above", preface))
        else:
            items.append(Item(base, b.title, b.title, b.body, "as described above"))
    return items


def parse_selection(text: str, n: int) -> list[int]:
    """'all' / '' -> every section; '3', '1,3', '2-4' (1-based) -> their zero-based indexes, sorted, unique."""
    text = text.strip().lower()
    if text in ("", "all"):
        return list(range(n))
    picked: set[int] = set()
    for token in text.split(","):
        token = token.strip()
        m = re.fullmatch(r"(\d+)(?:-(\d+))?", token)
        if not m:
            raise ValueError(f"cannot read {token!r}: use numbers like 3, 1,4 or 2-5")
        lo = int(m.group(1))
        hi = int(m.group(2) or lo)
        if lo < 1 or hi > n or lo > hi:
            raise ValueError(f"{token!r} is outside 1-{n}")
        picked.update(range(lo - 1, hi))
    return sorted(picked)


K7 = [
    Item("K7-1", "K7 favorites paste", "a favorite's shortcut pastes into the game's chat box",
         "Set a shortcut on a favorite (favorites window, shortcut box). Log into the game, open its chat box "
         "(click into it), press the shortcut. (Run the dev build or 0.6.1 as Administrator.)",
         "the favorite's Japanese text appears in the chat box. If NOTHING appears, answer `fail: nothing pasted` -- the "
         "game or its anti-cheat may ignore the synthetic Ctrl+V (SendInput)"),
    Item("K7-2", "K7 favorites paste", "if it did not paste: the text is on the clipboard",
         "Only if K7-1 failed: click into the chat box and press Ctrl+V yourself. (Answer `skip` if K7-1 passed.)",
         "your own Ctrl+V pastes the favorite's text -- so the fallback is 'keep it on the clipboard and say so in the UI'"),
    Item("K7-3", "K7 favorites paste", "what was pasted is the NEW favorite, not an older clipboard text",
         "Copy some other text (e.g. 'old text') with Ctrl+C. Press the favorite's shortcut in the chat box. "
         "Then, 2 seconds later, paste with Ctrl+V once more.",
         "the favorite pasted the first time; afterwards Ctrl+V gives back 'old text'. If the FIRST paste was 'old text', answer "
         "`fail: pasted old text` -- CLIPBOARD_RESTORE_DELAY (500 ms, shortcut.rs) must go up"),
]
