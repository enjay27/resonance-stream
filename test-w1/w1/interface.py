"""K4: which adapter the sniffer binds. Pure parsing and checks; the notebook runs the commands.

`route print` rows are numbers only, so they parse on any Windows language. Adapters come from
PowerShell as JSON for the same reason (`ipconfig` labels are translated).
"""
from __future__ import annotations

import json
import re
from dataclasses import dataclass

POWERSHELL_ADAPTERS = (
    "$a = @(Get-NetIPAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue | ForEach-Object { "
    "$n = Get-NetAdapter -InterfaceIndex $_.InterfaceIndex -ErrorAction SilentlyContinue; "
    "[pscustomobject]@{Alias=$_.InterfaceAlias; IP=$_.IPAddress; "
    "Description=[string]$n.InterfaceDescription; Status=[string]$n.Status; Virtual=[bool]$n.Virtual} }); "
    "if ($a.Count -gt 0) { ConvertTo-Json -InputObject $a }"
)

_IPV4 = r"(\d{1,3}(?:\.\d{1,3}){3})"
_ROUTE = re.compile(rf"^\s*0\.0\.0\.0\s+0\.0\.0\.0\s+(\S+)\s+{_IPV4}\s+(\d+)\s*$", re.MULTILINE)
_LOG = re.compile(r"Auto-Targeting Network Interface: (\S+) \((default route|first physical adapter)\)")
_KEYWORDS = re.compile(r"VIRTUAL_ADAPTER_KEYWORDS:\s*\[&str;\s*\d+\]\s*=\s*\[(.*?)\];", re.DOTALL)


@dataclass
class Route:
    gateway: str
    interface_ip: str
    metric: int


@dataclass
class Adapter:
    alias: str
    ip: str
    description: str
    status: str
    virtual: bool


def parse_default_routes(text: str) -> list[Route]:
    return [Route(g, ip, int(m)) for g, ip, m in _ROUTE.findall(text)]


def default_route_ip(routes: list[Route]) -> str | None:
    """The interface of the lowest-metric default route (the first one on a tie)."""
    return min(routes, key=lambda r: r.metric).interface_ip if routes else None


def parse_adapters(text: str) -> list[Adapter]:
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
        Adapter(d.get("Alias") or "", d.get("IP") or "", d.get("Description") or "",
                d.get("Status") or "", bool(d.get("Virtual")))
        for d in data
    ]


def virtual_keywords(rust_source: str) -> list[str]:
    """The app's list, read from crates/core/src/sniffer_net.rs so the notebook cannot drift from it."""
    m = _KEYWORDS.search(rust_source)
    if not m:
        raise ValueError("VIRTUAL_ADAPTER_KEYWORDS not found in the source")
    return re.findall(r'"([^"]+)"', m.group(1))


def looks_virtual(name: str, keywords: list[str]) -> bool:
    low = name.lower()
    return any(k.lower() in low for k in keywords)


def parse_log_line(line: str) -> tuple[str, str]:
    m = _LOG.search(line)
    if not m:
        raise ValueError(f"not the sniffer's 'Auto-Targeting Network Interface' line: {line!r}")
    return m.group(1), m.group(2)


def _adapter_of(ip: str, adapters: list[Adapter]) -> Adapter | None:
    return next((a for a in adapters if a.ip == ip), None)


def _is_virtual(adapter: Adapter, keywords: list[str]) -> bool:
    return adapter.virtual or looks_virtual(adapter.alias, keywords) or looks_virtual(adapter.description, keywords)


Check = tuple[str, str, str]  # (title, "pass" | "fail" | "skip", evidence)


def _verdict(title: str, ok: bool, evidence: str = "") -> Check:
    return (title, "pass" if ok else "fail", evidence)


def check_default_route(logged: tuple[str, str], routes: list[Route], adapters: list[Adapter],
                        keywords: list[str]) -> list[Check]:
    """K4 (1): two live adapters -- the app says '(default route)' and uses the OS's route."""
    ip, rule = logged
    route_ip = default_route_ip(routes)
    mine = _adapter_of(ip, adapters)
    return [
        _verdict("the logged address is one of this machine's", mine is not None,
                 f"{ip} on {mine.alias}" if mine else f"{ip} is on none of {[a.ip for a in adapters]}"),
        _verdict("the app used the default-route rule", rule == "default route", f"rule: {rule}"),
        _verdict("the logged address is the one the default route uses", ip == route_ip,
                 f"logged {ip}, route print says {route_ip}"),
        _verdict("the logged adapter is not a virtual one",
                 mine is not None and not _is_virtual(mine, keywords),
                 f"{mine.alias} / {mine.description}" if mine else ""),
    ]


def check_vpn(logged: tuple[str, str], routes: list[Route], adapters: list[Adapter],
              keywords: list[str]) -> list[Check]:
    """K4 (2): a full-tunnel VPN carries the default route; the app must still take the physical adapter."""
    ip, rule = logged
    route_ip = default_route_ip(routes)
    route_adapter = _adapter_of(route_ip, adapters) if route_ip else None
    pre = "precondition: the default route runs through a virtual adapter"
    if route_adapter is None or not _is_virtual(route_adapter, keywords):
        where = route_adapter.alias if route_adapter else "no default route"
        return [(pre, "skip", f"the default route is on {where}: not a full-tunnel VPN, nothing to conclude")]
    known = looks_virtual(route_adapter.alias, keywords) or looks_virtual(route_adapter.description, keywords)
    return [
        (pre, "pass", f"{route_adapter.alias} / {route_adapter.description} ({route_ip})"),
        _verdict("keyword: the app's list recognises this VPN adapter", known,
                 "" if known else f"neither {route_adapter.alias!r} nor {route_adapter.description!r} matches a "
                                  "VIRTUAL_ADAPTER_KEYWORDS entry: the app would trust the route through it"),
        _verdict("the app used the list rule (first physical adapter)", rule == "first physical adapter",
                 f"rule: {rule}"),
        _verdict("the logged address is not the VPN's", ip != route_ip, f"logged {ip}, VPN {route_ip}"),
    ]
