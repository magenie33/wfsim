#!/usr/bin/env python3
"""Exercise prepared release promotion and its fail-closed boundaries offline."""
import contextlib
import copy
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
from unittest.mock import patch
from types import SimpleNamespace

import payload_manifest as payload
import release_desktop
import release_stage as stage
import ship
import ship_bot


def check_order(root):
    count = 0
    # Invoke the release orchestrator; readiness failure stops every channel pointer.
    release = {"engine": "engine", "release": "release", "wasm": "wasm"}
    (root / "site/release.json").write_text(json.dumps(release))
    for fail in (False, True):
        order = []
        def ready(expected):
            assert expected == release
            order.append("ready")
            if fail:
                raise ValueError("not ready")
        with patch.object(ship, "ROOT", root), patch.object(ship, "SITE", root / "site"), \
                patch.object(ship, "run", lambda *cmd: order.append(cmd)), \
                patch.object(ship, "prepare_desktop", lambda: None), \
                patch.object(ship, "verify", return_value=0), patch.object(stage, "prerequisites", lambda: []), \
                patch.object(ship.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "", "")), \
                patch.dict(sys.modules, {"release_readiness": SimpleNamespace(wait_backend=ready)}), \
                patch.object(sys, "argv", ["ship.py", "--built", "--no-push", "--no-bot"]):
            try:
                ship.main()
            except SystemExit as exit:
                assert exit.code == 0 and not fail
            except ValueError:
                assert fail
            else:
                raise AssertionError("release did not finish or refuse")
        backend = next(i for i, cmd in enumerate(order) if isinstance(cmd, tuple) and str(root / "wrangler.jsonc") in cmd)
        assert backend < order.index("ready")
        after = order[order.index("ready") + 1:]
        assert bool(after) is not fail
        if not fail:
            assert "release_desktop.py" in str(after[0])
        count += 1
    return count


def main():
    passed = 0
    with tempfile.TemporaryDirectory(prefix="wfsim-release-stage-") as temporary:
        root = Path(temporary).resolve()
        bodies = {"Cargo.toml": '[workspace]\nmembers=["cli","engine"]\n', "Cargo.lock": "lock",
                  "cli/Cargo.toml": '[package]\nname="cli"\n[dependencies]\nengine={path="../engine"}\n',
                  "engine/Cargo.toml": '[package]\nname="engine"\n', "engine/src/lib.rs": "engine",
                  "cli/src/main.rs": "cli", "data/data.yaml": "data", "data/board_state.yaml": "original board", "docs/CLIENTS.md": "contract",
                  "tests/fixture.json": "fixture", "worker/ext_documents.js": "pages",
                  "desktop/Cargo.toml": "desktop", "desktop/build.rs": "producer",
                  "desktop/Cargo.lock": "locked", "desktop/payload.lst": "board/\n",
                  "desktop/examples/updatekit.rs": "signer", "scripts/release_stage.py": "stage",
                  "scripts/payload_manifest.py": "manifest", "mise.toml": '[tools]\nrust="1.97.1"\n',
                  "site/index.html": "page", "site/pkg/current.wasm": "wasm",
                  "site/board/current.json": "live", "desktop/payload/native/engine.dll": "native"}
        for name, body in bodies.items():
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(body)
        subprocess.run(["git", "init", "-q", str(root)], check=True)
        subprocess.run(["git", "-C", str(root), "add", "."], check=True)
        subprocess.run(["git", "-C", str(root), "-c", "user.name=fixture", "-c", "user.email=fixture@example.invalid",
                        "commit", "-qm", "fixture"], check=True)
        source = subprocess.check_output(["git", "-C", str(root), "rev-parse", "HEAD"], text=True).strip()
        elf = b"\x7fELF\x02\x01" + b"\0" * 12 + b"\x3e\x00" + b"fixture executable"
        rustc = "rustc 1.97.1 (fixture)\nhost: " + stage.TARGET
        env = {"GITHUB_REPOSITORY": "magenie33/wfsim", "GITHUB_SHA": source, "GITHUB_WORKFLOW": "CI",
               "GITHUB_EVENT_NAME": "workflow_dispatch", "GITHUB_RUN_ID": "123", "GITHUB_RUN_ATTEMPT": "1",
               "WFSIM_RELEASE_SOURCE": source, "WFSIM_RELEASE_RUN": "123", "WFSIM_RELEASE_PREPARED": str(root / "prepared")}
        events = []
        original_execute = stage.execute
        def execute(*command):
            if command[0] == "cargo":
                events.append("desktop-build")
                target = root / "desktop/target"
                (target / "debug/examples").mkdir(parents=True, exist_ok=True)
                (target / "payload-manifest.json").write_bytes(payload.build(version_override=source[:8]))
                (target / "debug/examples/updatekit").write_bytes(elf)
                return ""
            if command[0] == "rustc":
                return rustc
            return original_execute(*command)
        def build_live(out):
            events.append("live-build")
            for name in stage.OUTPUTS["live"]:
                (out / name).write_bytes(elf + name.encode())
            (out / "rustc.txt").write_text(rustc)
            return True
        with patch.object(stage, "ROOT", root), patch.object(payload, "ROOT", root), \
                patch.object(payload, "SITE", root / "site"), patch.object(payload, "ADDED", root / "desktop/payload"), \
                patch.object(payload, "LIST", root / "desktop/payload.lst"), patch.object(stage, "execute", execute), \
                patch.object(stage, "build_live", build_live), patch.dict(os.environ, env, clear=True), \
                contextlib.redirect_stdout(io.StringIO()):
            for component in stage.OUTPUTS:
                stage.prepare(component, root / "prepared" / component, source)
            assert events == ["desktop-build", "live-build"]
            baseline = stage.inputs("live", source)
            assert "engine/src/lib.rs" in baseline["files"] and "data/data.yaml" in baseline["files"]
            assert "docs/CLIENTS.md" in baseline["files"] and "site/index.html" not in baseline["files"]
            passed += 1
            for name in ("engine/src/lib.rs", "data/data.yaml", "Cargo.lock", "docs/CLIENTS.md", "cli/Cargo.toml"):
                path = root / name; before = path.read_bytes(); path.write_bytes(before + b"\n")
                assert stage.inputs("live", source) != baseline, name
                path.write_bytes(before); passed += 1
            page = root / "site/index.html"; page.write_text("UI-only change")
            assert stage.inputs("live", source) == baseline
            page.write_text("page"); passed += 1
            board = root / "data/board_state.yaml"
            board.write_text("later generated board publication")
            stage.verify(root / "prepared/live", "live", source, "123")
            board.write_text("original board"); passed += 1
            data = root / "data/data.yaml"; data.write_text("other runtime data changed")
            try:
                stage.verify(root / "prepared/live", "live", source, "123")
            except ValueError:
                passed += 1
            else:
                raise AssertionError("another data change was treated as board-only")
            data.write_text("data")
            other_source = subprocess.check_output(["git", "-C", str(root), "-c", "user.name=fixture",
                "-c", "user.email=fixture@example.invalid", "commit-tree", source + "^{tree}", "-m", "different source"], text=True).strip()
            try:
                stage.verify(root / "prepared/live", "live", other_source, "123")
            except ValueError:
                passed += 1
            else:
                raise AssertionError("different source with identical input objects was accepted")
            # Prepared bytes are promoted without invoking either compiler.
            before = list(events)
            assert ship_bot.live_binaries(root / "promoted-live")
            assert events == before
            passed += 1
            with patch.object(ship, "ROOT", root), patch.object(ship, "BUILT_MANIFEST", root / "desktop/promoted/payload-manifest.json"), \
                    patch.object(ship, "run", side_effect=AssertionError("unexpected Cargo build")):
                ship.prepare_desktop()
            assert Path(os.environ["WFSIM_UPDATEKIT"]).read_bytes() == elf
            passed += 1
            # All present-but-invalid artifacts block instead of falling back.
            directory = root / "prepared/live"
            original = json.loads((directory / "prepared.json").read_text())
            mutations = [
                (lambda r: r.update(input_sha256="0" * 64), "input digest"),
                (lambda r: r["inputs"].update(rust="1.1"), "input descriptor"),
                (lambda r: r.update(target="aarch64-linux"), "target"),
                (lambda r: r.update(container="rust:latest"), "container pin"),
                (lambda r: r.update(rustc="rustc 1.1"), "toolchain"),
                (lambda r: r["producer"].update(source="0" * 40), "source"),
                (lambda r: r["producer"].update(run="124"), "CI run"),
                (lambda r: r["producer"].update(repository="outsider/repo"), "repository"),
                (lambda r: r["outputs"].pop("wfsim-intake"), "missing output"),
            ]
            for mutate, title in mutations:
                record = copy.deepcopy(original); mutate(record)
                (directory / "prepared.json").write_bytes(stage.canonical(record))
                try:
                    ship_bot.live_binaries(root / "refused")
                except ValueError:
                    assert events == before
                    passed += 1
                else:
                    raise AssertionError("accepted bad " + title)
            (directory / "prepared.json").write_bytes(stage.canonical(original))
            binary = directory / "wfsim-board"; binary.write_bytes(b"bad output")
            try:
                stage.promote("live", root / "refused")
            except ValueError:
                passed += 1
            else:
                raise AssertionError("accepted modified binary")
            binary.write_bytes(elf + b"wfsim-board")
            for malformed in (b"\x7fELF\x01\x01", elf[:18] + b"\xb7\x00" + elf[20:]):
                binary.write_bytes(malformed)
                record = copy.deepcopy(original)
                record["outputs"]["wfsim-board"] = {"sha256": stage.digest(malformed), "size": len(malformed)}
                (directory / "prepared.json").write_bytes(stage.canonical(record))
                try:
                    stage.promote("live", root / "refused")
                except ValueError:
                    passed += 1
                else:
                    raise AssertionError("accepted wrong ELF target")
            binary.write_bytes(elf + b"wfsim-board")
            (directory / "prepared.json").write_bytes(stage.canonical(original))
            binary.unlink(); binary.symlink_to(root / "prepared/desktop/updatekit")
            try:
                stage.promote("live", root / "refused")
            except ValueError:
                passed += 1
            else:
                raise AssertionError("accepted linked output")
            binary.unlink(); binary.write_bytes(elf + b"wfsim-board")
            reuse = root / "reuse"
            shutil.copytree(directory, reuse)
            run_record = {"head_repository": {"full_name": "magenie33/wfsim"}, "workflow_id": 77,
                          "head_sha": source, "conclusion": "success", "status": "completed",
                          "run_attempt": 1, "event": "workflow_dispatch"}
            official = copy.deepcopy(original)
            def api(request, **kwargs):
                return io.BytesIO(stage.canonical({"id": 77} if request.full_url.endswith("ci.yml") else run_record))
            def download(command, **kwargs):
                destination = Path(command[command.index("--dir") + 1])
                (destination / "prepared.json").write_bytes(stage.canonical(official))
                return subprocess.CompletedProcess(command, 0)
            with patch.object(stage.urllib.request, "urlopen", api), patch.object(stage.subprocess, "run", download):
                assert stage.trusted_previous(original)
                passed += 1
                official["outputs"]["wfsim-board"]["sha256"] = "0" * 64
                assert not stage.trusted_previous(original), "a claimed run cannot authenticate different cache bytes"
                passed += 1
                run_record["event"] = "pull_request"
                assert not stage.trusted_previous(original)
                passed += 1
            # Only a genuinely absent legacy component takes the original build.
            shutil.rmtree(directory)
            assert ship_bot.live_binaries(root / "promoted-live")
            assert events == [*before, "live-build"]
            passed += 1
            before = list(events)
            with patch.object(stage, "trusted_previous", return_value=True):
                stage.prepare("live", directory, source, reuse)
            assert events == before, "identical proven live inputs must not rebuild"
            assert json.loads((directory / "prepared.json").read_text())["reused_build"] == original["producer"]
            passed += 1
            # Fresh input differences invalidate even an authenticated previous artifact.
            data = root / "data/data.yaml"; data.write_text("new data")
            with patch.object(stage, "trusted_previous", return_value=True):
                try:
                    stage.verify(reuse, "live", source, previous=True)
                except ValueError:
                    passed += 1
                else:
                    raise AssertionError("reused different source inputs")
            try:
                stage.producer(source)
            except subprocess.CalledProcessError:
                passed += 1
            else:
                raise AssertionError("uncommitted source can produce a claimed exact-SHA artifact")
            data.write_text("data")
            # A moved signer receives only the current absolute key path.
            with patch.object(release_desktop, "ROOT", root), patch.object(release_desktop.subprocess, "run") as run:
                run.return_value = subprocess.CompletedProcess([], 0, "signed", "")
                release_desktop.updatekit("sign", "document")
                args, options = run.call_args
                assert args[0][0] == os.environ["WFSIM_UPDATEKIT"]
                assert options["env"]["WFSIM_UPDATE_KEY_PATH"] == str(root / "private/wfsim_update_key")
                assert not (root / "private").exists()
            passed += 1
            with patch.dict(os.environ, {"GITHUB_EVENT_NAME": "pull_request"}):
                record = stage.producer(source)
                assert record["source"] == source and record["workflow"] == "CI"
                passed += 1
            # SQL prerequisites are explicit; an HTTP authentication refusal is no proof.
            base = "1" * 40
            sql = root / "worker/pg/library.sql"; sql.parent.mkdir(parents=True); sql.write_text("DDL fixture")
            changed = []
            def history(*command):
                return base if command[:2] == ("git", "rev-parse") else "\n".join(changed)
            with patch.object(stage, "execute", history), patch.dict(os.environ, {"WFSIM_RELEASE_BASE": base}):
                assert stage.prerequisites() == []
                passed += 1
                changed.append("worker/pg/library.sql")
                try:
                    stage.prerequisites()
                except ValueError:
                    passed += 1
                else:
                    raise AssertionError("SQL change without operator proof passed")
                receipt = root / "operator.json"
                evidence = {"source": source, "base": base, "sql": {changed[0]: stage.digest(sql.read_bytes())},
                            "checks": [{"ok": True, "evidence": "owner+column+function hashes verified"}]}
                receipt.write_bytes(stage.canonical(evidence))
                with patch.dict(os.environ, {"WFSIM_SCHEMA_READINESS_RECEIPT": str(receipt)}):
                    assert stage.prerequisites() == changed
                    passed += 1
                    evidence["checks"][0]["ok"] = False; receipt.write_bytes(stage.canonical(evidence))
                    try:
                        stage.prerequisites()
                    except ValueError:
                        passed += 1
                    else:
                        raise AssertionError("failed schema proof passed")
        passed += check_order(root)
    print(f"release preparation: {passed} offline controls passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
