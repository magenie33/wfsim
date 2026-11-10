#!/usr/bin/env python3
"""Prepare checked release components without publishing or signing them."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import tomllib
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
TARGET = "x86_64-unknown-linux-gnu"
# Official rust:1.97.1-bookworm, linux/amd64 manifest (not a moving tag).
LIVE_IMAGE = "rust@sha256:408fe88047cef61a2087653b0c5255fa51c0f2d6d94ddedd7a2562a9b91a46f6"
OUTPUTS = {"desktop": ("payload-manifest.json", "updatekit"), "live": ("wfsim-board", "wfsim-intake")}


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()


def digest(body):
    return hashlib.sha256(body).hexdigest()


def execute(*command):
    return subprocess.check_output(command, cwd=ROOT, text=True, timeout=900).strip()


def rust_version():
    return tomllib.loads((ROOT / "mise.toml").read_text())["tools"]["rust"]


def regular(path):
    if path.resolve(strict=True) != path.absolute() or not path.is_file():
        raise ValueError(f"release input is missing or linked: {path}")
    return path.read_bytes()


def source_inputs(component, source):
    tracked = execute("git", "ls-files", "-z").split("\0")
    if component == "desktop":
        prefixes = {"desktop/"}
    else:
        # Cargo's local path dependency closure, including build/proc-macro crates.
        workspace = tomllib.loads((ROOT / "Cargo.toml").read_text())
        prefixes, todo = set(), ["cli"]
        while todo:
            directory = todo.pop()
            prefix = directory.rstrip("/") + "/"
            if prefix in prefixes:
                continue
            prefixes.add(prefix)
            manifest = tomllib.loads((ROOT / directory / "Cargo.toml").read_text())
            tables = [manifest, *manifest.get("target", {}).values()]
            for table in tables:
                for section in ("dependencies", "build-dependencies"):
                    for name, dep in table.get(section, {}).items():
                        if isinstance(dep, dict) and dep.get("workspace"):
                            dep = workspace.get("workspace", {}).get("dependencies", {}).get(name)
                        if isinstance(dep, dict) and "path" in dep:
                            base = ROOT if table.get(section, {}).get(name, {}).get("workspace") else ROOT / directory
                            todo.append((base / dep["path"]).resolve().relative_to(ROOT).as_posix())
        # Engine build.rs embeds data; native contracts and compile-time fixtures
        # can be read outside a crate. Include whole trees rather than infer semantics.
        prefixes.update(("data/", "docs/", "tests/", ".cargo/"))
    selected = [p for p in tracked if p and (any(p.startswith(prefix) for prefix in prefixes)
                or p in ("Cargo.toml", "Cargo.lock", "mise.toml", "rust-toolchain", "rust-toolchain.toml")
                or p.endswith("/Cargo.toml")) and not p.startswith("desktop/payload/")]
    selected += ["scripts/release_stage.py", "scripts/payload_manifest.py"]
    if component == "desktop":
        selected += ["worker/ext_documents.js"]
    result = {}
    for name in sorted(set(selected)):
        # Board publication may advance main without advancing the selected source.
        # The checked binary embeds CI's source snapshot, never that later snapshot.
        if component == "live" and name == "data/board_state.yaml":
            body = subprocess.check_output(["git", "show", source + ":" + name], cwd=ROOT, timeout=30)
        else:
            body = regular(ROOT / name)
        result[name] = digest(body)
    return result


def inputs(component, source):
    value = {"files": source_inputs(component, source), "rust": rust_version(), "target": TARGET,
             "container": LIVE_IMAGE if component == "live" else None,
             "command": "cargo build --release --locked --bin wfsim-intake --bin wfsim-board"
                        if component == "live" else "cargo build --locked --manifest-path desktop/Cargo.toml --example updatekit"}
    if component == "live":
        value["rustflags"] = ""
    else:
        import payload_manifest
        value["payload_manifest_sha256"] = digest(payload_manifest.build(version_override=source[:8]))
        value["version"] = source[:8]
    return value


def producer(source):
    if not re.fullmatch(r"[0-9a-f]{40}", source) or execute("git", "rev-parse", "HEAD") != source:
        raise ValueError("prepare requires the exact checked-out source SHA")
    execute("git", "diff", "--exit-code", source, "--", ".", ":!site", ":!desktop/payload", ":!mcp/engine.js")
    env = os.environ
    if (env.get("GITHUB_REPOSITORY") != "magenie33/wfsim" or env.get("GITHUB_SHA") != source
            or env.get("GITHUB_WORKFLOW") != "CI" or env.get("GITHUB_EVENT_NAME") not in ("push", "workflow_dispatch", "schedule", "pull_request")
            or not env.get("GITHUB_RUN_ID", "").isdigit() or not env.get("GITHUB_RUN_ATTEMPT", "").isdigit()):
        raise ValueError("prepare requires a CI producer in the source repository")
    return {"repository": env["GITHUB_REPOSITORY"], "workflow": "CI", "run": env["GITHUB_RUN_ID"],
            "attempt": env["GITHUB_RUN_ATTEMPT"], "source": source}


def trusted_previous(record):
    """An optional reuse directory must come from successful repository CI."""
    repo = "magenie33/wfsim"
    provenance = record["producer"]
    if (not re.fullmatch(r"[0-9a-f]{40}", provenance["source"])
            or not str(provenance["run"]).isdigit() or not str(provenance["attempt"]).isdigit()):
        return False
    headers = {"accept": "application/vnd.github+json", "user-agent": "wfsim-release-stage"}
    if os.environ.get("GH_TOKEN"):
        headers["authorization"] = "Bearer " + os.environ["GH_TOKEN"]
    def api(path):
        request = urllib.request.Request("https://api.github.com/repos/" + repo + path, headers=headers)
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.load(response)
    workflow = api("/actions/workflows/ci.yml")["id"]
    run = api("/actions/runs/" + str(provenance["run"]))
    trusted = (provenance["repository"] == repo and provenance["workflow"] == "CI"
            and run["head_repository"]["full_name"] == repo and run["workflow_id"] == workflow
            and run["head_sha"] == provenance["source"] and run["conclusion"] == "success"
            and run["status"] == "completed" and str(run["run_attempt"]) == provenance["attempt"]
            and run["event"] in ("push", "workflow_dispatch", "schedule"))
    if not trusted:
        return False
    # A claimed successful run does not authenticate a local cache's bytes.
    with tempfile.TemporaryDirectory(prefix="wfsim-release-reuse-") as temporary:
        directory = Path(temporary).resolve()
        subprocess.run(["gh", "run", "download", str(provenance["run"]), "--repo", repo,
                        "--name", "release-live", "--dir", str(directory)], check=True, timeout=60,
                       capture_output=True)
        official = json.loads(regular(directory / "prepared.json"))
        return canonical(official) == canonical(record)


def verify(directory, component, source, run=None, previous=False):
    record = json.loads(regular(directory / "prepared.json"))
    wanted = inputs(component, source)
    provenance = record["producer"]
    if (type(record["format"]) is not int or record["format"] != 1 or record["component"] != component or record["inputs"] != wanted
            or record["input_sha256"] != digest(canonical(wanted))
            or record["target"] != TARGET or record["container"] != wanted["container"]
            or not record["rustc"].startswith("rustc " + rust_version() + " ")
            or "host: " + TARGET not in record["rustc"].splitlines()
            or set(record["outputs"]) != set(OUTPUTS[component])):
        raise ValueError("prepared component inputs or build environment differ")
    if previous:
        if component != "live" or not trusted_previous(record):
            raise ValueError("previous producer is not trusted successful CI")
    elif (provenance != {"repository": "magenie33/wfsim", "workflow": "CI", "run": str(run),
                        "attempt": provenance["attempt"], "source": source} or not run):
        raise ValueError("prepared component is not from the selected exact-source CI run")
    for name, entry in record["outputs"].items():
        body = regular(directory / name)
        if entry != {"sha256": digest(body), "size": len(body)}:
            raise ValueError("prepared output bytes differ: " + name)
        if name != "payload-manifest.json" and (body[:6] != b"\x7fELF\x02\x01" or body[18:20] != b"\x3e\x00"):
            raise ValueError("prepared executable is not Linux ELF64 little-endian: " + name)
    if component == "desktop":
        import payload_manifest
        if regular(directory / "payload-manifest.json") != payload_manifest.build(version_override=source[:8]):
            raise ValueError("prepared desktop manifest differs from current payload")
    return record


def promote(component, destination):
    env = os.environ
    source, run = env.get("WFSIM_RELEASE_SOURCE", ""), env.get("WFSIM_RELEASE_RUN", "")
    directory = Path(env.get("WFSIM_RELEASE_PREPARED", str(ROOT / "target/release-stage"))) / component
    if not directory.exists():
        return False
    try:
        if not re.fullmatch(r"[0-9a-f]{40}", source):
            raise ValueError("prepared artifact has no exact release source")
        verify(directory, component, source, run)
        destination.mkdir(parents=True, exist_ok=True)
        for name in OUTPUTS[component]:
            shutil.copyfile(directory / name, destination / name)
            if name != "payload-manifest.json":
                (destination / name).chmod(0o755)
        print(f"prepared {component}: verified exact source {source}, CI {run}; no build")
        return True
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        raise ValueError(f"prepared {component} artifact refused: {error}") from error


def prerequisites():
    source = os.environ.get("WFSIM_RELEASE_SOURCE") or execute("git", "rev-parse", "HEAD")
    base = os.environ.get("WFSIM_RELEASE_BASE")
    if not base:
        base = json.loads(execute("git", "show", "HEAD:site/release.json"))["commit"].removesuffix("+")
    base = execute("git", "rev-parse", base + "^{commit}")
    paths = execute("git", "diff", "--name-only", "--no-renames", base, source).splitlines()
    sql = [name for name in paths if name.startswith("worker/pg/") and name.endswith(".sql")]
    if sql:
        path = os.environ.get("WFSIM_SCHEMA_READINESS_RECEIPT")
        if not path:
            raise ValueError("schema/API preparation requires an exact operator receipt before release: " + ", ".join(sql))
        record = json.loads(regular(Path(path)))
        expected = {name: digest(regular(ROOT / name)) if (ROOT / name).exists() else None for name in sql}
        if (record.get("source") != source or record.get("base") != base or record.get("sql") != expected
                or not record.get("checks") or not all(item.get("ok") is True and isinstance(item.get("evidence"), str) and item["evidence"].strip()
                                                     for item in record["checks"])):
            raise ValueError("schema/API operator receipt does not prove this exact release")
    print("schema prerequisites: " + ("exact operator receipt verified" if sql else "empty (no SQL change)"))
    return paths


def build_live(out):
    try:
        available = subprocess.run(["docker", "version"], capture_output=True, timeout=15).returncode == 0
    except (OSError, subprocess.SubprocessError):
        available = False
    if not available:
        print("LIVE BOARD NOT SHIPPED — Docker is not running; retaining existing binaries")
        return False
    out.mkdir(parents=True, exist_ok=True)
    command = ["docker", "run", "--rm", "--platform", "linux/amd64", "-v", f"{ROOT}:/src:ro", "-v", f"{out.resolve()}:/out",
               "-v", "wfsim-lin-target:/target", "-v", "wfsim-cargo-reg:/usr/local/cargo/registry", "-w", "/src", LIVE_IMAGE,
               "bash", "-euc", "rustc -vV > /out/rustc.txt; "
               f"test $(rustc --version | cut -d' ' -f2) = {rust_version()}; "
               "CARGO_TARGET_DIR=/target cargo build --release --locked --bin wfsim-intake --bin wfsim-board; "
               "cp /target/release/wfsim-intake /target/release/wfsim-board /out/"]
    try:
        result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=900)
    except (OSError, subprocess.SubprocessError):
        print("LIVE BOARD NOT SHIPPED — Linux build unavailable; retaining existing binaries")
        return False
    if result.returncode:
        print("LIVE BOARD NOT SHIPPED — Linux build failed: " + result.stderr[-400:])
        return False
    return True


def prepare(component, out, source, reuse=None):
    provenance = producer(source)
    before = inputs(component, source)
    out.mkdir(parents=True, exist_ok=True)
    reused = None
    if reuse and component == "live":
        try:
            reused = verify(reuse, component, source, previous=True)
        except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError):
            print("previous live artifact is unproven; building normally")
    if reused:
        for name in OUTPUTS[component]:
            shutil.copyfile(reuse / name, out / name)
        rustc = reused["rustc"]
    elif component == "live":
        if not build_live(out):
            raise ValueError("Linux runtime preparation failed")
        rustc = (out / "rustc.txt").read_text().strip()
    else:
        execute("cargo", "build", "--locked", "--manifest-path", str(ROOT / "desktop/Cargo.toml"), "--example", "updatekit")
        import payload_manifest
        manifest = regular(ROOT / "desktop/target/payload-manifest.json")
        if manifest != payload_manifest.build(version_override=source[:8]):
            raise ValueError("desktop/build.rs and Python payload producers differ")
        (out / "payload-manifest.json").write_bytes(manifest)
        shutil.copyfile(ROOT / "desktop/target/debug/examples/updatekit", out / "updatekit")
        rustc = execute("rustc", "-vV")
    if inputs(component, source) != before:
        raise ValueError("release inputs changed while preparing")
    producer(source)
    record = {"format": 1, "component": component, "producer": provenance, "inputs": before,
              "input_sha256": digest(canonical(before)), "target": TARGET, "container": before["container"],
              "rustc": rustc, "runner": {key: os.environ.get(key) for key in ("ImageOS", "ImageVersion")},
              "reused_build": reused["producer"] if reused else None,
              "outputs": {name: {"sha256": digest(regular(out / name)), "size": (out / name).stat().st_size}
                          for name in OUTPUTS[component]}}
    (out / "prepared.json").write_bytes(canonical(record))
    verify(out, component, source, provenance["run"])
    print(f"prepared {component}: input {record['input_sha256']}, source {source}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("prepare", "key"))
    parser.add_argument("component", choices=OUTPUTS)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--source", required=True)
    parser.add_argument("--reuse", type=Path)
    args = parser.parse_args()
    if args.command == "key":
        print(digest(canonical(inputs(args.component, args.source))))
    else:
        if not args.out:
            parser.error("prepare requires --out")
        prepare(args.component, args.out, args.source, args.reuse)


if __name__ == "__main__":
    main()
