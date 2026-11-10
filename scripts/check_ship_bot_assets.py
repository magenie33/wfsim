#!/usr/bin/env python3
"""Run the actual bot loaded-state probe against offline runtime fixtures."""
import contextlib
import io
import json
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
from types import SimpleNamespace
from unittest.mock import patch

import ship_bot as ship


def exercise(change=None):
    with tempfile.TemporaryDirectory(prefix="wfsim-bot-proof-") as temporary:
        root = Path(temporary).resolve()
        local, remote = root / "local", root / "remote"
        files = {"bot/qq.mjs": 'import "../worker/outer.js";', "bot/helper.mjs": "bot",
                 "worker/outer.js": 'export { x } from "./inner.js";', "worker/inner.js": "export const x = 1;",
                 "mcp/headless.js": "headless", "mcp/engine.js": 'import module from "../site/pkg/a.wasm";\nimport pack0 from "../site/pkg/a.json";',
                 "site/pkg/a.wasm": "wasm", "site/pkg/a.json": "pack", "site/img/a.png": "image",
                 "site/index.html": '<link href="/asset/style.abc.css">', "site/asset/style.abc.css": "style"}
        for name, body in files.items():
            path = local / name; path.parent.mkdir(parents=True, exist_ok=True); path.write_text(body)
        remote.mkdir()
        proc = root / "proc/123"; proc.mkdir(parents=True)
        (proc / "cmdline").write_bytes(b"/usr/bin/node\0bot/qq.mjs\0")
        (proc / "cwd").symlink_to(remote)
        (proc / "exe").symlink_to(shutil.which("node"))
        unit, environment = root / "unit.service", root / "service.env"
        unit.write_text("unit"); environment.write_text("fixture only; no secrets")
        (proc / "environ").write_bytes(b"BOT_ENV=" + str(environment).encode() + b"\0")
        mode, events = {"calls": 0, "active": "active", "reload": "no"}, []
        target = {"root": str(remote), "service": "wfsim-bot", "host": "fixture", "key": "fixture", "known_hosts": "fixture"}
        def command(args, **kwargs):
            if args[0] == "date":
                return str(mode["start"])
            assert args[0] == "systemctl" and "wfsim-bot" in args
            mode["calls"] += 1
            return "\n".join(f"{key}={value}" for key, value in {
                "ActiveState": mode["active"], "MainPID": 124 if mode.get("restart") and mode["calls"] > 1 else 123,
                "ExecMainStartTimestamp": "fixture", "WorkingDirectory": mode.get("cwd", str(remote)),
                "FragmentPath": str(unit), "DropInPaths": "", "EnvironmentFiles": "", "NeedDaemonReload": mode["reload"]}.items())
        def probe(args, **kwargs):
            events.append("probe")
            assert args[0] == "ssh" and kwargs["timeout"] == 45
            if mode.get("timeout"):
                raise subprocess.TimeoutExpired(args, 45)
            if mode.get("malformed"):
                return subprocess.CompletedProcess(args, 0, "{}", "")
            def path(value):
                if mode.get("late") and str(value) == str(environment) and not mode.get("rewritten"):
                    mode["rewritten"] = True
                    (remote / "worker/inner.js").write_text("export const x = 1;")
                return root / "proc" if str(value) == "/proc" else Path(value)
            output = io.StringIO()
            with patch.dict(sys.modules, {"pathlib": SimpleNamespace(Path=path, PurePosixPath=Path)}), \
                    patch.object(subprocess, "check_output", command), patch.object(sys, "stdin", io.StringIO(kwargs["input"])), \
                    contextlib.redirect_stdout(output):
                try:
                    exec(shlex.split(args[-1])[-1], {})
                except (AssertionError, ValueError, OSError):
                    return subprocess.CompletedProcess(args, 1, "", "unproven")
            return subprocess.CompletedProcess(args, 0, output.getvalue(), "")
        with patch.object(ship, "ROOT", local), patch.object(ship, "ENGINE", local / "mcp/engine.js"), \
                patch.object(ship, "target", lambda: target), patch.object(ship, "deploy_live", lambda t: events.append("live")), \
                patch.object(ship, "send", lambda *args: events.append("send")), \
                patch.object(ship, "run", lambda *args: events.append("write")), patch.object(ship.subprocess, "run", probe):
            inventory = ship.bot_files()
            assert "worker/inner.js" in inventory
            for name in inventory:
                path = remote / name; path.parent.mkdir(parents=True, exist_ok=True); path.write_bytes((local / name).read_bytes())
            mode["start"] = time.time_ns()
            if change:
                change(local, remote, mode, unit, environment)
            with contextlib.redirect_stdout(io.StringIO()):
                ship.deploy()
            return events


def main():
    cases = [(None, True, "full runtime and loaded state skip every bot write")]
    for name in ("bot/helper.mjs", "worker/inner.js", "mcp/headless.js", "mcp/engine.js", "site/pkg/a.wasm",
                 "site/pkg/a.json", "site/img/a.png", "site/asset/style.abc.css"):
        def drift(l, r, m, u, e, name=name):
            (r / name).write_text("different"); m["start"] = time.time_ns()
        cases.append((drift, False, name + " drift deploys"))
    cases += [
        (lambda l, r, m, u, e: (r / "worker/inner.js").unlink(), False, "missing transitive import deploys"),
        (lambda l, r, m, u, e: m.update(active="inactive"), False, "inactive bot deploys"),
        (lambda l, r, m, u, e: m.update(timeout=True), False, "unknown timeout deploys"),
        (lambda l, r, m, u, e: m.update(malformed=True), False, "incomplete response deploys"),
        (lambda l, r, m, u, e: m.update(restart=True), False, "PID change deploys"),
        (lambda l, r, m, u, e: m.update(cwd="/other"), False, "wrong service root deploys"),
        (lambda l, r, m, u, e: m.update(reload="yes"), False, "unloaded unit edit deploys"),
        (lambda l, r, m, u, e: u.write_text("changed"), False, "unit newer than process deploys"),
        (lambda l, r, m, u, e: e.write_text("changed"), False, "environment newer than process deploys"),
        (lambda l, r, m, u, e: m.update(late=True), False, "replacement during hashing deploys"),
        (lambda l, r, m, u, e: (r / "mcp/headless.js").write_text("headless"), False, "same bytes uploaded after start deploy"),
    ]
    def stale_style(l, r, m, u, e):
        (r / "site/asset/style.000.css").write_text("old style")
        m["start"] = time.time_ns()
    cases.append((stale_style, False, "extra style that can be selected deploys"))
    def removed_style(l, r, m, u, e):
        path = r / "site/asset/style.000.css"
        path.write_text("old style"); path.unlink()
    cases.append((removed_style, False, "style directory changed after process start deploys"))
    for change, skip, title in cases:
        events = exercise(change)
        assert (events == ["live", "probe"]) == skip, (title, events)
        print("  ok  " + title)
    print(f"bot runtime preservation: {len(cases)} offline controls passed")


if __name__ == "__main__":
    main()
