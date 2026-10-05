"""K6: one firewall rule per exe. Pure parsing and checks; the notebook runs PowerShell.

`Get-NetFirewallRule` + JSON is used instead of `netsh ... show rule`: netsh prints its field
labels in the system language (Korean Windows has none of "Program:"), the JSON does not.
"""
from __future__ import annotations

import json
import re
from dataclasses import dataclass

LEGACY_RULE_NAME = "Resonance Stream (Packet Sniffing)"

POWERSHELL_COMMAND = (
    "$r = @(Get-NetFirewallRule -DisplayName 'Resonance Stream (Packet Sniffing)*' "
    "-ErrorAction SilentlyContinue | ForEach-Object { "
    "$f = $_ | Get-NetFirewallApplicationFilter; "
    "[pscustomobject]@{Name=$_.DisplayName; Enabled=[string]$_.Enabled; "
    "Direction=[string]$_.Direction; Action=[string]$_.Action; Program=$f.Program} }); "
    "if ($r.Count -gt 0) { ConvertTo-Json -InputObject $r }"
)

_HASHED = re.compile(re.escape(LEGACY_RULE_NAME) + r" \[([0-9a-f]{8})\]$")


def _normal(path: str) -> str:
    return path.lower().replace("\\", "/")


def rule_name_for(exe_path: str) -> str:
    """Twin of `resonance_core::sniffer_net::rule_name_for` (FNV-1a over the normalised path)."""
    h = 0x811C9DC5
    for b in _normal(exe_path).encode("utf-8"):
        h = ((h ^ b) * 0x01000193) & 0xFFFFFFFF
    return f"{LEGACY_RULE_NAME} [{h:08x}]"


@dataclass
class Rule:
    name: str
    program: str
    enabled: bool
    direction: str
    action: str


def parse_rules(text: str) -> list[Rule]:
    """The JSON PowerShell prints: nothing, one object, or a list of them."""
    text = text.strip().lstrip("﻿")
    if not text:
        return []
    try:
        data = json.loads(text)
    except json.JSONDecodeError as e:
        raise ValueError(f"not the JSON the command prints: {text[:120]!r}") from e
    if isinstance(data, dict):
        data = [data]
    return [
        Rule(
            name=d.get("Name") or "",
            program=d.get("Program") or "",
            enabled=str(d.get("Enabled")).lower() == "true",
            direction=str(d.get("Direction") or ""),
            action=str(d.get("Action") or ""),
        )
        for d in data
    ]


def check_rules(rules: list[Rule], exes: list[str]) -> list[tuple[str, bool, str]]:
    """(title, ok, evidence) for every thing K6 says must hold."""
    out: list[tuple[str, bool, str]] = []
    names = [r.name for r in rules]
    out.append(("the old shared rule is gone", LEGACY_RULE_NAME not in names,
                "still there: " + LEGACY_RULE_NAME if LEGACY_RULE_NAME in names else ""))
    dupes = sorted({n for n in names if names.count(n) > 1})
    out.append(("rule names are unique", not dupes, ", ".join(dupes)))
    for exe in exes:
        want = rule_name_for(exe)
        mine = [r for r in rules if r.name == want]
        title = f"rule for {exe}"
        if not mine:
            out.append((title, False, f"no rule named {want}"))
            continue
        r = mine[0]
        problems = []
        if _normal(r.program) != _normal(exe):
            problems.append(f"program is {r.program!r}")
        if not r.enabled:
            problems.append("disabled")
        if r.direction.lower() != "inbound":
            problems.append(f"direction {r.direction}")
        if r.action.lower() != "allow":
            problems.append(f"action {r.action}")
        out.append((title, not problems, "; ".join(problems) or want))
    for r in rules:
        m = _HASHED.search(r.name)
        if m and r.program and rule_name_for(r.program) != r.name:
            out.append((f"{r.name} hashes its own program", False,
                        f"its program {r.program!r} would be {rule_name_for(r.program)}"))
    return out
