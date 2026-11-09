"""Exercise the actual live deployment path with local files and a simulated SSH service."""
import contextlib
import io
import json
import pathlib
import shlex
import subprocess
import sys
import tempfile
import time
from types import SimpleNamespace
from unittest.mock import patch


def exercise(ship, change=None):
    with tempfile.TemporaryDirectory(prefix="wfsim-live-ship-") as temporary:
        root = pathlib.Path(temporary).resolve()
        local, remote = root / "local", root / "remote"
        out = local / "target/live-linux"
        scripts = ["scripts/live_board.sh", "scripts/main.mjs"]
        rel = {"commit": "new-ui", "release": "new-release", "wasm": "abc", "engine": "engine",
               "packs": ["pack.data.def.json"], "data": "def", "protocol": 8, "unknown": {"kept": True}}
        bodies = {"scripts/live_board.sh": "board", "scripts/main.mjs": 'import "../worker/a.js";\nimport("../mcp/a.js");',
                  "worker/a.js": 'export { x } from "./nested.js";', "worker/nested.js": "export const x = 1;",
                  "mcp/a.js": "mcp", "worker/deadlines.js": "deadline", "worker/board_tasks.js": "text read",
                  "scripts/pg/package.json": "{}", "scripts/pg/package-lock.json": '{"lock":1}',
                  "site/release.json": json.dumps(rel), "site/index.html": '<script src="/asset/app.abc.js"></script>',
                  "site/asset/app.abc.js": 'const ENGINE_ID = "engine";',
                  "site/pkg/wfsim_wasm.abc.js": "glue", "site/pkg/wfsim_wasm_bg.abc.wasm": "wasm",
                  "site/pkg/pack.data.def.json": "pack", "target/live-linux/wfsim-intake": "intake",
                  "target/live-linux/wfsim-board": "binary"}
        for name, body in bodies.items():
            path = local / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(body)
        target = {"root": str(remote), "host": "fixture", "key": "fixture-key", "known_hosts": "fixture-hosts"}
        events, mode = [], {"pid": 123, "active": "active", "reload": "no", "calls": 0}
        proc = root / "proc/123"
        proc.mkdir(parents=True)
        (remote / "live").mkdir(parents=True)
        (proc / "cmdline").write_bytes(b"\0".join(str(x).encode() for x in
            ["/bin/bash", "scripts/live_board.sh", remote / "live", remote / "bin"]) + b"\0")
        (proc / "cwd").symlink_to(remote / "live")
        (proc / "exe").symlink_to("/bin/bash")
        unit, environment = root / "unit.service", root / "service.env"
        unit.write_text("unit")
        environment.write_text("never read this secret fixture")

        def fake_command(command, **kwargs):
            assert command[0] in ("systemctl", "date")
            if command[0] == "date":
                return str(mode["start"])
            assert "wfsim-live" in command and "--timestamp=us" in command
            mode["calls"] += 1
            pid = mode["pid"] + (1 if mode.get("restart") and mode["calls"] > 1 else 0)
            return "\n".join(f"{key}={value}" for key, value in {
                "ActiveState": mode["active"], "MainPID": pid, "ExecMainStartTimestamp": "fixture-time",
                "WorkingDirectory": mode.get("cwd", str(remote)), "FragmentPath": str(unit),
                "DropInPaths": "", "EnvironmentFiles": f"{environment} (ignore_errors=no)",
                "NeedDaemonReload": mode["reload"]}.items())

        def fake_probe(command, **kwargs):
            events.append("probe")
            assert command[0] == "ssh" and kwargs["timeout"] == 45
            if mode.get("timeout"):
                raise subprocess.TimeoutExpired(command, 45)
            if mode.get("malformed"):
                return subprocess.CompletedProcess(command, 0, "{}", "")
            code = shlex.split(command[-1])[-1]
            output = io.StringIO()
            def fake_path(value):
                if mode.get("late_rewrite") and str(value) == str(environment) and not mode.get("rewritten"):
                    mode["rewritten"] = True
                    (remote / "scripts/main.mjs").write_bytes((local / "scripts/main.mjs").read_bytes())
                return root / "proc" if str(value) == "/proc" else pathlib.Path(value)
            with patch.dict(sys.modules, {"pathlib": SimpleNamespace(Path=fake_path)}), \
                    patch.object(subprocess, "check_output", fake_command), \
                    patch.object(sys, "stdin", io.StringIO(kwargs["input"])), contextlib.redirect_stdout(output):
                try:
                    exec(code, {})
                except (AssertionError, OSError, ValueError):
                    return subprocess.CompletedProcess(command, 1, "", "probe unavailable")
            return subprocess.CompletedProcess(command, 0, output.getvalue(), "")

        def fake_run(command):
            events.append(("write", list(map(str, command))))
            return "2" * 40 if command[0] == "git" else ""

        def build(directory):
            events.append("build")
            assert directory == out
            return not mode.get("build_failure")

        with patch.object(ship, "ROOT", local), patch.object(ship, "LIVE_SCRIPTS", scripts), \
                patch.object(ship, "live_binaries", build), patch.object(ship, "run", fake_run), \
                patch.object(ship, "send", lambda t, files, dest: events.append(("send", files, dest))), \
                patch.object(ship.subprocess, "run", fake_probe):
            files = ship.live_files(out, rel)
            for directory, sources in files.items():
                for source in sources:
                    dest = remote / directory / pathlib.Path(source).name
                    dest.parent.mkdir(parents=True, exist_ok=True)
                    if dest.name == "VERSION":
                        dest.write_text("1" * 40 + "\n")
                    elif dest.name == "ENGINE":
                        dest.write_text("engine\n")
                    elif dest.name == "release.json":
                        dest.write_text(json.dumps({**rel, "commit": "old-ui", "release": "old-release"}, indent=2))
                    else:
                        dest.write_bytes((local / source).read_bytes())
                    if directory == "bin" and dest.name.startswith("wfsim-"):
                        dest.chmod(0o755)
            (remote / "scripts/pg/.installed-lock").write_bytes((local / "scripts/pg/package-lock.json").read_bytes())
            mode["start"] = time.time_ns()
            if change:
                change(local, remote, mode, unit, environment)
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                try:
                    ship.deploy_live(target)
                except (OSError, SystemExit):
                    assert events == ["build"], events
                    raise
            assert events[0] == "build", events
            return {"events": events, "output": output.getvalue(), "version_written": (out / "VERSION").exists(),
                    "inventory": sorted(str(p) for sources in files.values() for p in sources)}


def self_test(ship):
    failures = 0
    cases = [(None, True, "matching runtime and UI-only metadata retain the actual old VERSION")]
    for path in ["bin/wfsim-board", "bin/wfsim-intake", "scripts/main.mjs", "worker/nested.js", "mcp/a.js",
                 "worker/deadlines.js", "worker/board_tasks.js", "scripts/pg/package.json", "scripts/pg/package-lock.json",
                 "scripts/pg/.installed-lock", "bin/witness/pkg/wfsim_wasm.abc.js", "bin/witness/pkg/wfsim_wasm_bg.abc.wasm",
                 "bin/witness/pkg/pack.data.def.json", "bin/ENGINE"]:
        def drift(local, remote, mode, unit, environment, path=path):
            (remote / path).write_bytes(b"drift")
            mode["start"] = time.time_ns()
        cases.append((drift, False, f"actual {path} byte drift deploys"))
    cases += [
        (lambda l, r, m, u, e: (r / "worker/nested.js").unlink(), False, "a missing imported file deploys"),
        (lambda l, r, m, u, e: (r / "bin/VERSION").unlink(), False, "missing provenance deploys"),
        (lambda l, r, m, u, e: m.update(active="inactive"), False, "inactive service deploys"),
        (lambda l, r, m, u, e: m.update(timeout=True), False, "unknown SSH timeout deploys"),
        (lambda l, r, m, u, e: m.update(malformed=True), False, "incomplete probe response deploys"),
        (lambda l, r, m, u, e: m.update(late_rewrite=True), False, "a file replaced after hashing prevents skip"),
        (lambda l, r, m, u, e: m.update(restart=True), False, "PID changes during the probe deploy"),
        (lambda l, r, m, u, e: m.update(cwd="/wrong-root"), False, "a service on another deployment root deploys"),
        (lambda l, r, m, u, e: m.update(reload="yes"), False, "unloaded unit edits deploy"),
        (lambda l, r, m, u, e: u.write_text("changed"), False, "unit changes after process start deploy"),
        (lambda l, r, m, u, e: e.write_text("changed secret fixture"), False, "environment changes after process start deploy"),
        (lambda l, r, m, u, e: (r / "mcp/a.js").write_text("mcp"), False, "co-host bot rewrite after process start deploys"),
    ]
    def release_drift(local, remote, mode, unit, environment):
        path = remote / "bin/witness/release.json"
        value = json.loads(path.read_text())
        value["unknown"]["kept"] = False
        path.write_text(json.dumps(value))
        mode["start"] = time.time_ns()
    cases.append((release_drift, False, "release semantics exclude only commit and release"))
    def release_type_drift(local, remote, mode, unit, environment):
        path = remote / "bin/witness/release.json"
        value = json.loads(path.read_text())
        value["unknown"]["kept"] = 1
        path.write_text(json.dumps(value))
        mode["start"] = time.time_ns()
    cases.append((release_type_drift, False, "JSON true never equals numeric one in runtime metadata"))
    def symlink(local, remote, mode, unit, environment):
        path = remote / "worker/nested.js"
        alias = remote / "same-bytes.js"
        alias.write_bytes(path.read_bytes())
        path.unlink()
        path.symlink_to(alias)
        mode["start"] = time.time_ns()
    cases.append((symlink, False, "same-byte symlink or retarget ambiguity prevents skip"))
    for name in ["wfsim-board", "wfsim-intake"]:
        def no_execute(local, remote, mode, unit, environment, name=name):
            (remote / "bin" / name).chmod(0o644)
            mode["start"] = time.time_ns()
        cases.append((no_execute, False, f"{name} without execute bits before process start deploys"))
    for change, skip, title in cases:
        try:
            result = exercise(ship, change)
            writes = [event for event in result["events"] if event != "build" and event != "probe"]
            assert (not writes) == skip, result["events"]
            assert result["version_written"] is not skip, result
            if skip:
                assert "actual VERSION " + "1" * 40 in result["output"]
            else:
                assert any(event[0] == "write" and "restart wfsim-live" in event[1][-1] for event in writes)
                assert sorted(str(p) for event in writes if event[0] == "send" for p in event[1]) == result["inventory"]
            print(f"  ok    {title}")
        except (AssertionError, OSError, SystemExit) as error:
            print(f"FAIL    {title}: {str(error)[:180]}")
            failures += 1
    for name, target, expected in [
        ("scripts/missing.mjs", "./missing.mjs", "LIVE_SCRIPTS"),
        ("other/helper.js", "../other/helper.js", "matching deployed path"),
        ("worker/sub/helper.js", "../worker/sub/helper.js", "matching deployed path"),
    ]:
        def new_import(local, remote, mode, unit, environment, name=name, target=target):
            path = local / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("export const x = 1;")
            (local / "scripts/main.mjs").write_text(f'import "{target}";')
        try:
            exercise(ship, new_import)
        except SystemExit as error:
            assert expected in str(error), error
            print(f"  ok    unshipped import {name} refuses before remote writes")
        else:
            print(f"FAIL    unshipped import {name} was ignored")
            failures += 1
    result = exercise(ship, lambda l, r, m, u, e: m.update(build_failure=True))
    if result["events"] == ["build"] and not result["version_written"]:
        print("  ok    a failed Linux build still leaves the live deployment untouched")
    else:
        print("FAIL    a failed Linux build attempted live operations")
        failures += 1
    return failures
