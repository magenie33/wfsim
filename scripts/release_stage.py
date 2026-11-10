#!/usr/bin/env python3
"""Prepare checked release components without publishing or signing them."""
import argparse
import ast
import inspect
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
OUTPUTS = {"desktop": ("payload-manifest.json", "updatekit"), "live": ("wfsim-board", "wfsim-intake", "inventory-compat.json")}
CATALOG_RECIPE = "inventory-catalog-v1"
ENGINE_BUILD_TOKENS = "711a2e4a8bc177ed4d500a188b8dc6756c7c9b6b1aff8481a7cab66b43482a10"


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


def local_crates():
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
    return sorted(prefix.rstrip("/") for prefix in prefixes)


def source_inputs(component, source):
    tracked = execute("git", "ls-files", "-z").split("\0")
    if component == "desktop":
        prefixes = {"desktop/"}
    else:
        prefixes = {name + "/" for name in local_crates()}
        # Engine build.rs embeds data; native contracts and compile-time fixtures
        # can be read outside a crate. Include whole trees rather than infer semantics.
        prefixes.update(("data/", "docs/", "tests/", ".cargo/", "native/", "wasm/"))
    selected = [p for p in tracked if p and (any(p.startswith(prefix) for prefix in prefixes)
                or p in ("Cargo.toml", "Cargo.lock", "mise.toml", "rust-toolchain", "rust-toolchain.toml")
                or p.endswith(("/Cargo.toml", "/Cargo.lock"))) and not p.startswith("desktop/payload/")]
    selected += ["scripts/release_stage.py", "scripts/payload_manifest.py"]
    if component == "desktop":
        selected += ["worker/ext_documents.js"]
    else:
        selected += ["scripts/build_site_app.py", "scripts/rust_tokens.py", "scripts/data_packs.py", "scripts/native_pgo.py"]
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


def catalog_rust(paths, rules):
    import rust_tokens
    streams, tests, unknown = {}, [], []
    for path in paths:
        rel = path.relative_to(ROOT).as_posix()
        streams[rel], mods = rust_tokens.digest_text(regular(path).decode())
        tests += rust_tokens.test_module_paths(rel, mods)
    out = {}
    generated = 'include!(concat!(env!("OUT_DIR"),"/embedded_data.rs"))'
    for rel, stream in streams.items():
        if rules.is_test_source(rel) or rel in tests or rel.startswith(tuple(tests)) or stream == "[]":
            continue
        tokens = json.loads(stream)
        text = "".join(tokens)
        macros = [tokens[i] for i in range(len(tokens) - 1) if tokens[i + 1] == "!"]
        if any(name in macros for name in ("include", "include_str", "include_bytes")):
            if not (rel == "engine/src/data/mod.rs" and macros.count("include") == 1 and generated in text):
                unknown.append(rel + ":unknown-include")
        if any(tokens[i:i + 4] == ["#", "[", "path", "="] for i in range(len(tokens))):
            unknown.append(rel + ":unknown-module-path")
        for i in range(len(tokens) - 3):
            if tokens[i] in ("option_env", "env") and tokens[i + 1:i + 3] == ["!", "("]:
                if tokens[i + 3] not in ('"CARGO_MANIFEST_DIR"', '"OUT_DIR"', '"CARGO_PKG_VERSION"', '"WFSIM_GAS_OFF"'):
                    unknown.append(rel + ":unknown-compile-environment")
        out[rel] = digest(stream.encode())
    return out, unknown


def catalog_inputs(binary_sha256):
    """Catalog semantics use the engine's classifiers and normalizers, with full hashes."""
    import build_site_app as rules
    import rust_tokens
    import data_packs
    previous = rules.ROOT
    rules.ROOT = ROOT
    rules.prose_keys.cache_clear()
    unknown, rust, engine_data, engine_code = [], {}, {}, {}
    crates = local_crates()
    if not {"cli", "engine", "optimizer", "webapi"} <= set(crates):
        unknown.append("unrecognized-native-dependency-closure")
    tracked = execute("git", "ls-files", "-z").split("\0")
    manifests = {name: digest(regular(ROOT / name)) for name in sorted(tracked) if name and
                 (name.endswith(("Cargo.toml", "Cargo.lock")) or name.startswith(".cargo/")
                  or name in ("mise.toml", "rust-toolchain", "rust-toolchain.toml"))}
    build = {"rust": rust_version(), "container": LIVE_IMAGE,
             "command": "cargo build --release --locked --bin wfsim-intake --bin wfsim-board",
             "rustflags": "", "features": [], "environment": {"WFSIM_DATA_APART": None, "WFSIM_GAS_OFF": None},
             "yaml": rules.yaml.__version__}
    try:
        paths = sorted({p for crate in crates for p in (ROOT / crate).rglob("*.rs") if "target" not in p.relative_to(ROOT).parts})
        rust, found = catalog_rust(paths, rules)
        unknown += found
        for crate in crates:
            manifest = tomllib.loads(regular(ROOT / crate / "Cargo.toml").decode())
            build_file = manifest.get("package", {}).get("build", "build.rs")
            if build_file is not False and (ROOT / crate / build_file).exists():
                name = (Path(crate) / build_file).as_posix()
                if name != "engine/build.rs" or rust.get(name) != ENGINE_BUILD_TOKENS:
                    unknown.append(name + ":unrecognized-build-script")
            if manifest.get("lib", {}).get("proc-macro"):
                for name in (n for n in rust if n.startswith(crate + "/")):
                    tokens = json.loads(rust_tokens.digest_text(regular(ROOT / name).decode())[0])
                    if any(word in tokens for word in ("include", "include_str", "include_bytes", "read", "read_to_string", "read_dir", "File", "Command")):
                        unknown.append(name + ":unrecognized-proc-macro-input")
        if any(name.startswith(".cargo/") for name in manifests):
            unknown.append(".cargo:unrecognized-build-configuration")
        engine_paths = sorted(p for name in rules.ENGINE_SOURCES for p in (ROOT / name).rglob("*") if p.is_file())
        engine_rust, found = catalog_rust([p for p in engine_paths if p.suffix == ".rs"], rules)
        unknown += found
        for path in engine_paths + [ROOT / name for name in rules.ENGINE_BUILD_INPUTS]:
            name = path.relative_to(ROOT).as_posix()
            if rules.is_test_source(name) or name.startswith(rules.ENGINE_ID_NOT):
                continue
            if name.startswith("data/"):
                if not rules.is_engine_data(name):
                    continue
                body = rules.without_prose(rules.lean_yaml(regular(path).decode())).encode()
                engine_data[name] = digest(body)
            elif path.suffix == ".rs":
                if name in engine_rust:
                    engine_code[name] = engine_rust[name]
            else:
                engine_code[name] = digest(regular(path).replace(b"\r\n", b"\n"))
        algorithm = [canonical, digest, local_crates, catalog_rust, catalog_inputs, rules.is_test_source, rules.is_engine_data,
                     rules.lean_yaml, rules.without_prose, rules.prose_keys, build_live]
        build["rules"] = digest(canonical({"python": [ast.dump(ast.parse(inspect.getsource(fn)), include_attributes=False) for fn in algorithm],
            "rust_tokens": digest(regular(Path(rust_tokens.__file__).resolve())),
            "data_packs": digest(regular(Path(data_packs.__file__).resolve())),
            "engine_rules": [rules.ENGINE_SOURCES, rules.ENGINE_BUILD_INPUTS, rules.ENGINE_SOURCES_NOT, rules.ENGINE_ID_NOT],
            "engine_build_tokens": ENGINE_BUILD_TOKENS}))
    except (OSError, ValueError, KeyError, TypeError) as error:
        unknown.append("unrecognized-semantic-input:" + type(error).__name__)
        build.setdefault("rules", digest(regular(Path(__file__).resolve())))
    finally:
        rules.ROOT = previous
        rules.prose_keys.cache_clear()
    value = {"mode": "full-fallback" if unknown else "semantic", "recipe": CATALOG_RECIPE, "target": TARGET,
             "build": build, "crates": crates, "manifests": manifests, "rust": rust,
             "engine_data": engine_data, "engine_code": digest(canonical(engine_code)), "unknown": sorted(set(unknown))}
    if unknown:
        value["fallback"] = {"binary_sha256": binary_sha256,
                             "raw_inputs": source_inputs("live", os.environ.get("WFSIM_RELEASE_SOURCE") or os.environ.get("GITHUB_SHA")
                                                         or execute("git", "rev-parse", "HEAD"))}
    return value


def write_catalog(directory):
    board = digest(regular(directory / "wfsim-board"))
    semantic = catalog_inputs(board)
    sidecar = {"format": 1, "recipe": CATALOG_RECIPE, "target": TARGET, "key": digest(canonical(semantic)),
               "inputs": semantic, "binary_sha256": board}
    (directory / "inventory-compat.json").write_bytes(canonical(sidecar))


def verify_catalog(directory):
    path = directory / "inventory-compat.json"
    if not path.exists() and not path.is_symlink():
        return False
    record = json.loads(regular(path))
    board = digest(regular(directory / "wfsim-board"))
    if (set(record) != {"format", "recipe", "target", "key", "inputs", "binary_sha256"}
            or type(record["format"]) is not int or record["format"] != 1
            or record["recipe"] != CATALOG_RECIPE or record["target"] != TARGET
            or record["binary_sha256"] != board or record["inputs"] != catalog_inputs(board)
            or record["key"] != digest(canonical(record["inputs"]))):
        raise ValueError("inventory compatibility sidecar or binary binding differs")
    return True


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
        if not name.endswith(".json") and (body[:6] != b"\x7fELF\x02\x01" or body[18:20] != b"\x3e\x00"):
            raise ValueError("prepared executable is not Linux ELF64 little-endian: " + name)
    if component == "desktop":
        import payload_manifest
        if regular(directory / "payload-manifest.json") != payload_manifest.build(version_override=source[:8]):
            raise ValueError("prepared desktop manifest differs from current payload")
    elif not verify_catalog(directory):
        raise ValueError("prepared live artifact lacks inventory compatibility sidecar")
    if previous:
        unknown = json.loads(regular(directory / "inventory-compat.json"))["inputs"]["unknown"]
        if any(any(word in item for word in (":unknown-", "unrecognized-build-script", "unrecognized-proc-macro-input", "build-configuration")) for item in unknown):
            raise ValueError("unknown compilation inputs require a fresh build")
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
            if not name.endswith(".json"):
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
    (out / "inventory-compat.json").unlink(missing_ok=True)
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
    if component == "live":
        write_catalog(out)
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
