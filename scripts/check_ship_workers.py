"""The real release path deploys one build to both workers and stops on failure."""
import argparse
import contextlib
import hashlib
import importlib.util
import io
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
from unittest.mock import patch


def exercise(script, failure=False):
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

        modules = {"payload_manifest": SimpleNamespace(build=lambda: b"{}"),
                   "ship_bot": SimpleNamespace(deploy=lambda: calls.append({"kind": "bot"}))}
        with patch.object(ship, "ROOT", root), patch.object(ship, "BUILT_MANIFEST", manifest), \
                patch.object(ship.subprocess, "run", fake_run), \
                patch.object(ship.shutil, "which", return_value="npx"), \
                patch.object(ship, "verify", verify), patch.dict(sys.modules, modules), \
                patch.object(sys, "argv", [str(script), "--built"]), \
                contextlib.redirect_stdout(io.StringIO()):
            try:
                ship.main()
            except SystemExit as error:
                exit_code = error.code
            else:
                raise AssertionError("the publication path must return its verification status")

        deploys = [c for c in calls if "config" in c]
        if failure:
            assert isinstance(exit_code, str) and "failed (23)" in exit_code, exit_code
            assert [c["config"] for c in deploys] == ["wrangler.jsonc"], deploys
            assert calls[-1] is deploys[0], "backend failure must stop every later release action"
        else:
            assert exit_code == 0, exit_code
            assert [c["config"] for c in deploys] == [
                "wrangler.jsonc", "wrangler.edge.jsonc", "mcp/wrangler.jsonc"], deploys
            for call in deploys:
                assert call["args"][:4] == ["npx", "--yes", ship.WRANGLER, "deploy"], call
                assert call["cwd"] == root, "every worker must publish from the same build checkout"
            assert len({c["artifact"] for c in deploys}) == 1, "the artifact changed between workers"
            push = next(c for c in calls if c.get("args") == ["git", "push"])
            assert calls.index(push) < calls.index(deploys[0]), "the source push must land first"
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
    print("publication checks passed: one build, backend before edge, backend failure stops release")


if __name__ == "__main__":
    main()
