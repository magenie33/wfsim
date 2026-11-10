"""The backend precedes the channel and edge, from one build, and failure stops release."""
import argparse
import contextlib
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
from unittest.mock import patch


def exercise(script, failure=False, readiness_failure=False):
    spec = importlib.util.spec_from_file_location("ship_workers", script)
    ship = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(ship)
    calls = []
    with tempfile.TemporaryDirectory(prefix="wfsim-ship-workers-") as directory:
        root = Path(directory)
        site = root / "site"
        site.mkdir()
        artifact = site / "index.html"
        artifact.write_bytes(b"validated CI artifact")
        release = {"release": "fixture", "engine": "engine", "wasm": "wasm"}
        (site / "release.json").write_text(json.dumps(release))
        manifest = root / "payload-manifest.json"
        manifest.write_bytes(b"{}")

        def fake_run(command, **kwargs):
            args = list(map(str, command))
            call = {"kind": "process", "args": args, "cwd": kwargs.get("cwd")}
            if "deploy" in args:
                config = Path(args[args.index("-c") + 1])
                call.update(config=config.relative_to(root).as_posix(),
                            artifact=hashlib.sha256(artifact.read_bytes()).hexdigest())
            calls.append(call)
            code = 23 if failure and call.get("config") == "wrangler.jsonc" else 0
            return subprocess.CompletedProcess(command, code, stdout="", stderr="")

        def verify(bot=True):
            calls.append({"kind": "verify", "bot": bot})
            return 0

        def ready(expected):
            assert expected == release
            calls.append({"kind": "ready"})
            if readiness_failure:
                raise ValueError("backend contracts refused")

        modules = {"payload_manifest": SimpleNamespace(build=lambda **kwargs: b"{}"),
                   "release_stage": SimpleNamespace(promote=lambda *args: False,
                                                    prerequisites=lambda: calls.append({"kind": "prerequisites"})),
                   "release_readiness": SimpleNamespace(wait_backend=ready),
                   "ship_bot": SimpleNamespace(deploy=lambda: calls.append({"kind": "bot"}))}
        with patch.object(ship, "ROOT", root), patch.object(ship, "SITE", site), patch.object(ship, "BUILT_MANIFEST", manifest), \
                patch.object(ship.subprocess, "run", fake_run), \
                patch.object(ship.shutil, "which", return_value="npx"), \
                patch.object(ship, "verify", verify), patch.dict(sys.modules, modules), \
                patch.dict(os.environ, {"WEBSITE_TARGET": "{}"}), \
                patch.object(sys, "argv", [str(script), "--built"]), \
                contextlib.redirect_stdout(io.StringIO()):
            try:
                ship.main()
            except SystemExit as error:
                exit_code = error.code
            except ValueError as error:
                if not readiness_failure:
                    raise
                exit_code = str(error)
            else:
                raise AssertionError("the publication path must return its verification status")

        deploys = [c for c in calls if "config" in c]
        stage = next(c for c in calls if any(a.endswith("release_engine_lib.py") for a in c.get("args", []))
                     and "--stage" in c["args"])
        push = next(c for c in calls if c.get("args") == ["git", "push"])
        channels = [c for c in calls if any(a.endswith("release_desktop.py") for a in c.get("args", []))]
        assert calls.index(stage) < calls.index(push) < calls.index(deploys[0]), \
            "stage the native library, then land the source push before the backend publishes"
        prerequisites = next(c for c in calls if c["kind"] == "prerequisites")
        assert calls.index(prerequisites) < calls.index(stage)
        if failure or readiness_failure:
            expected_error = "backend contracts refused" if readiness_failure else "failed (23)"
            assert isinstance(exit_code, str) and expected_error in exit_code, exit_code
            assert [c["config"] for c in deploys] == ["wrangler.jsonc"], deploys
            assert not channels, "backend failure must leave the desktop channel unchanged"
            if readiness_failure:
                assert calls[-1]["kind"] == "ready", "failed contracts must stop every later release action"
            else:
                assert calls[-1] is deploys[0], "backend failure must stop every later release action"
        else:
            assert exit_code == 0, exit_code
            assert [c["config"] for c in deploys] == [
                "wrangler.jsonc", "wrangler.edge.jsonc", "mcp/wrangler.jsonc"], deploys
            for call in deploys:
                assert call["args"][:4] == ["npx", "--yes", ship.WRANGLER, "deploy"], call
                assert call["cwd"] == root, "every worker must publish from the same build checkout"
            assert len({c["artifact"] for c in deploys}) == 1, "the artifact changed between workers"
            assert len(channels) == 1, channels
            readiness = [c for c in calls if c["kind"] == "ready"]
            assert len(readiness) == 1, "backend readiness must run before any client pointer advances"
            assert calls.index(deploys[0]) < calls.index(readiness[0]) < calls.index(channels[0]) < calls.index(deploys[1]), \
                "the backend must accept the new client before the channel and edge advance"
            library = next(c for c in calls if c.get("args", [""])[-1].endswith("release_engine_lib.py"))
            assert calls.index(channels[0]) < calls.index(library) < calls.index(deploys[1]), \
                "publish the staged native library after the channel and before the edge"
            website = next(c for c in calls if any(a.endswith("ship_website.py") for a in c.get("args", [])))
            assert calls.index(deploys[1]) < calls.index(website) < calls.index(deploys[2]), calls
            assert [c["kind"] for c in calls[-2:]] == ["bot", "verify"], calls[-2:]
            assert calls[-1]["bot"], "the full publication must verify the bot"
        builds = [c for c in calls if "build_site_app.py" in " ".join(c.get("args", []))]
        assert len(builds) == 1 and builds[0]["args"][-1] == "--board-files", builds


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ship-script", type=Path, default=Path(__file__).with_name("ship.py"))
    args = parser.parse_args()
    exercise(args.ship_script)
    exercise(args.ship_script, failure=True)
    exercise(args.ship_script, readiness_failure=True)
    print("publication checks passed: one build, backend before channel and edge, backend failure stops release")


if __name__ == "__main__":
    main()
