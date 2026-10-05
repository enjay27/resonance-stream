"""Pipeline `interface` (K4, the route-based interface pick): which adapter the sniffer binds, read from the bridge.

The app is started with the sniffer ON and its own bridge; it says which adapter it took as a system event
(`Auto-Targeting Network Interface: <ip> (default route | first physical adapter)`) -- no pasting. The pipeline looks at the
machine itself (`route print`, the adapter list) to see which K4 situation it is in -- two live adapters (K4-1), a full-tunnel VPN
carrying the default route (K4-2), or offline (K4-3) -- and runs the check that fits; the others are `skip`. Turning the VPN on or
the network off is physical: do it, run the pipeline again. "Chat is captured" needs game traffic (or the synthetic-capture
scenario, not built yet): recorded as `skip`.
"""
from __future__ import annotations

from pathlib import Path

from runbook import bridge, common, interface, mockfeed, updater
from runbook.common import Recorder, capture

ENV_TITLES = {
    "two": "this machine has two or more live physical adapters (K4-1)",
    "vpn": "a full-tunnel VPN carries the default route (K4-2)",
    "offline": "this machine is offline: no default route or no live adapter (K4-3)",
    "single": "one live adapter and no VPN: neither K4-1 nor K4-2 applies",
}
FIXTURES = {  # the recorded command output a dry run uses, per situation
    "two": ("route_print_two_adapters.txt", "adapters_two.json"),
    "vpn": ("route_print_vpn.txt", "adapters_vpn.json"),
    "offline": ("route_print_offline.txt", "adapters_offline.json"),
    "single": ("route_print_single.txt", "adapters_single.json"),
}
CAPTURED_SKIP = "needs chat traffic from the game (or the synthetic-capture scenario, not built yet)"


class InterfacePick:
    def __init__(self, rec: Recorder, exe: str | Path, runs: Path | None = None, *, dry_situation: str = "two") -> None:
        self.rec = rec
        self.exe = Path(exe)
        self.runs = Path(runs or common.RUNS) / "interface"
        self.dry_situation = dry_situation
        self.keywords = interface.virtual_keywords(
            (common.ROOT.parent / "crates/core/src/sniffer_net.rs").read_text(encoding="utf-8"))

    def snapshot(self):
        route_fixture, adapters_fixture = FIXTURES[self.dry_situation]
        _, route_text = capture(["route", "print", "-4"], fixture=route_fixture)
        routes = interface.parse_default_routes(route_text)
        _, adapter_text = capture(["powershell", "-NoProfile", "-Command", interface.POWERSHELL_ADAPTERS],
                                  fixture=adapters_fixture)
        adapters = interface.parse_adapters(adapter_text)
        print("default routes:", [(r.interface_ip, r.metric) for r in routes])
        for a in adapters:
            print(f"  {a.ip:15} {a.alias}  [{a.description}]  status={a.status} virtual={a.virtual}")
        return routes, adapters

    def record_all(self, check: str, checks) -> None:
        for title, status, evidence in checks:
            self.rec.record(check, title, status, evidence)

    def run(self, key_path: str | None = None, password: str | None = None) -> None:
        """(`key_path` and `password` are for the pipelines that sign; this one ignores them.)"""
        routes, adapters = self.snapshot()
        situation = interface.classify(routes, adapters, self.keywords)
        self.rec.record("K4-env", ENV_TITLES[situation], "pass", "what this run can check; the other K4 rows are skipped")
        for check, mine in (("K4-1", "two"), ("K4-2", "vpn"), ("K4-3", "offline")):
            if situation != mine:
                self.rec.record(check, ENV_TITLES[mine].rsplit(" (", 1)[0], "skip", "not this machine's situation right now")

        events = bridge.Serve(self.runs / "bridge-events.jsonl")
        try:
            exe = updater.fresh_copy(self.exe, self.runs, "pick")
            args = mockfeed.flag_args(exe.parent / "data", exe.parent / "status.json", log_file=exe.parent / "app.log",
                                      capture=True, extra=("--no-update-check", "--bridge-url", events.url))
            mockfeed.start_app(exe, args)
            try:
                started = events.expect("app-started", timeout=120)["payload"]
                self.rec.auto("K4-start", "the copy started with the sniffer on and connected to the bridge", True,
                              f"pid {started['pid']}")
            except RuntimeError:
                self.rec.auto("K4-start", "the copy started with the sniffer on and connected to the bridge", False,
                              "no app-started event in 120 s")
                return
            line = self.sniffer_line(events)
            alive = self.alive(events)
            if situation == "offline":
                self.rec.auto("K4-3", "offline: the app stays up", alive, "ping answered" if alive else "no answer to a ping")
                self.rec.auto("K4-3", "offline: the system tab says which adapter, or that there is no interface", line is not None,
                              line or "no 'Auto-Targeting' or 'NETWORK_ERROR' line in 60 s")
                self.rec.record("K4-3", "after reconnecting the sniffer binds again", "skip", "connect the network and run again")
                return
            self.rec.auto("K4-alive", "the app stays up while the sniffer starts", alive,
                          "ping answered" if alive else "no answer to a ping")
            if line is None or not line.startswith("Auto-Targeting"):
                self.rec.auto("K4-line", "the sniffer says which adapter it took (Auto-Targeting Network Interface)", False,
                              line or "no such system line in 60 s -- is the firewall rule missing? is Jupyter/this shell elevated?")
                return
            if situation == "two":
                self.record_all("K4-1", interface.check_default_route(interface.parse_log_line(line), routes, adapters, self.keywords))
                self.rec.record("K4-1", "two adapters: chat is captured without the manual picker", "skip", CAPTURED_SKIP)
            elif situation == "vpn":
                self.record_all("K4-2", interface.check_vpn(interface.parse_log_line(line), routes, adapters, self.keywords))
                self.rec.record("K4-2", "VPN on: chat is still captured (THE OPEN QUESTION)", "skip", CAPTURED_SKIP)
            else:
                self.rec.record("K4-line", "the sniffer's pick on a machine with one adapter", "pass", line)
        finally:
            events.stop()
            mockfeed.stop_copies(self.runs)

    @staticmethod
    def sniffer_line(events: "bridge.Serve") -> str | None:
        """The first system line that says which adapter was taken or that there is none."""
        try:
            message = events.expect("system-event", {"payload.message": {"regex": r"^(Auto-Targeting Network Interface|NETWORK_ERROR)"}},
                                    timeout=60)
        except RuntimeError:
            return None
        return message["payload"]["message"]

    @staticmethod
    def alive(events: "bridge.Serve") -> bool:
        try:
            events.send("ping", timeout=10)
            return True
        except RuntimeError:
            return False
