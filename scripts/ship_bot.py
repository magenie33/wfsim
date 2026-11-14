#!/usr/bin/env python3
"""The QQ bot server, shipped and verified by `ship.py` beside the site.

THE BOT RUNS THE ENGINE THE SITE SHIPS — `mcp/engine.js`, its wasm and
`mcp/headless.js` — on a server nothing else deploys to. Copied by hand it
drifts the way the desktop channel once did: new weapons, names and words
reach the site and not the bot, and nothing fails. So the release that ships
the site copies these files over, restarts the bot, and then asks the server
which engine it now holds (docs/AGENT.md §"The QQ bot").

WHERE THE SERVER IS lives in `private/qq/bot.json` with its key, because this
repository is public. A tree without it cannot ship the bot and says so. Its
`live` names the live board's server when that is another one: the bot sits
where QQ, Discord and the site are all near, the live board where it always
was; each field `live` leaves out is the bot's.

    python scripts/ship_bot.py            # deploy, then verify
    python scripts/ship_bot.py --verify   # only: does the bot run the site's engine?
    python scripts/ship_bot.py --self-test

THE LIVE BOARD SHIPS WITH IT (`deploy/wfsim-live.service`): `wfsim-intake` and
`wfsim-board` built for Linux in Docker from this tree, and the scripts that
drive them. Without Docker the bot still ships and the live board keeps the
binaries it had, which is said out loud — a stale intake refuses new weapons.
"""
import hashlib
import json
import pathlib
import re
import shlex
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
TARGET = ROOT / "private" / "qq" / "bot.json"
ENGINE = ROOT / "mcp" / "engine.js"


def target() -> dict:
    if not TARGET.exists():
        sys.exit(f"ship: no bot target ({TARGET.relative_to(ROOT)}) — the QQ bot cannot be "
                 "shipped from this tree; pass --no-bot to ship without it")
    t = json.loads(TARGET.read_text(encoding="utf-8"))
    for k in ("key", "known_hosts"):
        t[k] = str(ROOT / t[k])
    return t


def live_target(t: dict) -> dict:
    """THE LIVE BOARD'S SERVER: the bot's, but for what `live` says."""
    live = dict(t.get("live") or {})
    for k in ("key", "known_hosts"):
        if k in live:
            live[k] = str(ROOT / live[k])
    return {**t, **live}


def ssh_opts(t: dict) -> list:
    return ["-i", t["key"], "-o", f"UserKnownHostsFile={t['known_hosts']}", "-o", "BatchMode=yes"]


def run(cmd: list) -> str:
    print(f"\n$ {' '.join(cmd)}", flush=True)
    r = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"ship: the bot step failed ({r.returncode}): {r.stderr.strip()[-400:]}")
    return r.stdout


def engine_digest(text: str) -> str:
    m = re.search(r'ENGINE_DIGEST = "(\w+)"', text)
    return m.group(1) if m else ""


def send(t: dict, files: list, dest: str) -> None:
    """FILES INTO A DIRECTORY ON THE SERVER: scp, or through the bucket.

    A RUNNER OUTSIDE MAINLAND CHINA REACHES THE SERVER AT A CRAWL (measured: 20
    MB by scp unfinished after ten minutes), while the server reads the bucket
    in seconds. So the ship workflow (`BOT_VIA_COS=1`) packs the files, puts
    them under an unguessable key through acceleration (`cos.py`), has the
    server fetch and unpack them, and deletes the key."""
    import os
    if os.environ.get("BOT_VIA_COS") != "1":
        run(["scp", "-q", *ssh_opts(t), *[str(f) for f in files], f"{t['host']}:{dest}/"])
        return
    import io, secrets, tarfile
    import cos
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz") as tar:
        for f in files:
            tar.add(ROOT / f, arcname=pathlib.Path(f).name)
    c, key = cos.creds(), f"bot-courier/{secrets.token_hex(16)}.tar.gz"
    cos.put(c, key, buf.getvalue(), "application/gzip")
    try:
        run(["ssh", *ssh_opts(t), t["host"], f"curl -sSf --retry 3 https://{cos.host(c)}/{key} | tar xzf - -C {dest}"])
    finally:
        cos.delete(c, key)


def deploy() -> None:
    t = target()
    files = bot_files()
    # Probe live before any possibly co-hosted bot imports are replaced.
    deploy_live(live_target(t))
    wanted = {name: file_sha256(ROOT / name) for name in files}
    current = runtime_probe(t, wanted, "bot")
    if current and current["files"] == wanted:
        print("bot runtime and assets unchanged — keeping the loaded service")
        return
    root, host, o = t["root"], t["host"], ssh_opts(t)
    for directory in sorted({str(pathlib.PurePosixPath(name).parent) for name in files}):
        run(["ssh", *o, host, f"mkdir -p {root}/{directory}"])
        send(t, [name for name in files if str(pathlib.PurePosixPath(name).parent) == directory], f"{root}/{directory}")
    for directory in ("site/pkg", "site/asset"):
        keep = " ".join("-e " + shlex.quote(pathlib.PurePosixPath(name).name) for name in files
                        if str(pathlib.PurePosixPath(name).parent) == directory)
        run(["ssh", *o, host, f"cd {root}/{directory} && ls -A | grep -vxF {keep} | xargs -r rm -f"])
    run(["ssh", *o, host, f"sudo systemctl restart {t['service']}"])


def bot_files() -> list:
    roots = sorted(p.relative_to(ROOT).as_posix() for p in (ROOT / "bot").glob("*.mjs"))
    reached = imported(roots + ["mcp/headless.js"])
    src = ENGINE.read_text(encoding="utf-8")
    names = re.findall(r'^import (?:module|pack[0-9]+) from "\.\./site/pkg/([^"]+)";$', src, re.M)
    if not names:
        raise ValueError("bot engine names no wasm or packs")
    index = (ROOT / "site/index.html").read_text(encoding="utf-8")
    css = re.search(r'href="/asset/(style\.[0-9a-f]+\.css)"', index).group(1)
    images = [p.relative_to(ROOT).as_posix() for p in (ROOT / "site/img").rglob("*") if p.is_file()]
    files = sorted(set([*roots, *reached, "mcp/engine.js", f"site/asset/{css}",
                        *(f"site/pkg/{name}" for name in names), *images]))
    for name in files:
        if not name.startswith(("bot/", "mcp/", "worker/", "site/")) or (ROOT / name).resolve(strict=True) != ROOT / name:
            raise ValueError("bot import is outside its deploy inventory: " + name)
    return files


LIVE_SCRIPTS = ["scripts/live_board.sh", "scripts/live_orders.mjs", "scripts/wasm_witness.mjs", "scripts/order_credit.mjs",
                "scripts/live_publish.mjs", "scripts/board_heads.mjs", "scripts/inbox_reader.mjs",
                "scripts/inventory_pipeline.mjs", "scripts/incremental_inventory.mjs", "scripts/inventory_inputs.mjs",
                "scripts/inventory_relational_pipeline.mjs", "scripts/inventory_relations.mjs", "scripts/inventory_operator.mjs",
                "scripts/sqlite_schema.mjs",
                "scripts/inventory_control.mjs", "scripts/inventory_rebuild.mjs",
                "scripts/inventory_board_pipeline.mjs", "scripts/inventory_projection.mjs", "scripts/inventory_publication.mjs",
                "scripts/inventory_publication_io.mjs", "scripts/inventory_disk_budget.mjs", "scripts/json_array_stream.mjs",
                "scripts/board_pipeline.mjs", "scripts/live_mailbox.mjs", "scripts/live_intake.mjs", "scripts/intake_batches.mjs",
                "scripts/pipeline_monitor.mjs",
                "scripts/classify_compute_inventory.mjs", "scripts/capture_compute_inventory.mjs",
                "scripts/board_meta.py", "scripts/fetch_library.sh", "scripts/library_array.mjs", "scripts/fetch_facts.sh",
                "scripts/ship_facts.sh", "scripts/ship_builds.sh", "scripts/build_batches.mjs", "scripts/ship_family.mjs",
                "scripts/ship_queue.sh", "scripts/retire_scores.mjs", "scripts/db.sh", "scripts/db.mjs", "scripts/pgq.mjs"]
# Shell code reaches these outside the JavaScript import statements.
LIVE_IMPORTS = ["worker/deadlines.js"]
LIVE_READS = ["worker/board_tasks.js"]


# A RELATIVE IMPORT, static or dynamic, in a script the live board runs.
IMPORT = re.compile(r"""^\s*(?:(?:import|export)\s[^;]*?from\s+|import\s*)["'](\.[^"']+)["']|import\(\s*["'](\.[^"']+)["']\s*\)""",
                    re.M | re.S)


def imported(roots: list) -> list:
    """EVERY REPO FILE THE LIVE SCRIPTS REACH, walked from their imports: the
    server holds only what is sent, so a module a script starts importing is
    shipped with it rather than failing there on every cycle."""
    seen, todo = set(), [ROOT / r for r in roots if r.endswith((".mjs", ".js"))]
    while todo:
        f = todo.pop().resolve()
        if f in seen or "node_modules" in f.parts:
            continue
        seen.add(f)
        for m in IMPORT.finditer(f.read_text(encoding="utf-8")):
            todo.append(f.parent / (m.group(1) or m.group(2)))
    return sorted(p.relative_to(ROOT).as_posix() for p in seen)


def unshipped(roots: list) -> list:
    """A script a live script imports that `LIVE_SCRIPTS` leaves out."""
    return [p for p in imported(roots) if p.startswith("scripts/") and p not in roots]


def toolchain() -> str:
    m = re.search(r'^rust = "([0-9.]+)"', (ROOT / "mise.toml").read_text(encoding="utf-8"), re.M)
    return m.group(1) if m else "1"


def live_binaries(out: pathlib.Path) -> bool:
    """`wfsim-intake` and `wfsim-board` for the server, or False with the reason."""
    import release_stage
    return release_stage.promote("live", out) or release_stage.build_live(out)


def live_files(out: pathlib.Path, rel: dict) -> dict:
    """The exact final directories and source files sent to the live board."""
    reached = imported(LIVE_SCRIPTS + LIVE_IMPORTS)
    missing = [p for p in reached if p.startswith("scripts/") and p not in LIVE_SCRIPTS]
    if missing:
        sys.exit(f"ship_bot: the live scripts import {missing}, which LIVE_SCRIPTS does not send")
    files = {"bin": [out / n for n in ("wfsim-intake", "wfsim-board", "VERSION", "ENGINE")],
             "scripts": LIVE_SCRIPTS,
             "worker": sorted(set(p for p in reached if p.startswith("worker/")) | set(LIVE_READS)),
             "mcp": [p for p in reached if p.startswith("mcp/")],
             "scripts/pg": ["scripts/pg/package.json", "scripts/pg/package-lock.json"],
             "bin/witness": ["site/release.json"],
             "bin/witness/pkg": [f"site/pkg/wfsim_wasm.{rel['wasm']}.js", f"site/pkg/wfsim_wasm_bg.{rel['wasm']}.wasm",
                                 *(f"site/pkg/{n}" for n in rel.get("packs", []))]}
    delivered = {f"{directory}/{pathlib.Path(p).name}" for directory, sources in files.items() for p in sources}
    unmapped = [p for p in reached + LIVE_READS if p not in delivered]
    if unmapped:
        sys.exit(f"ship_bot: live imports have no matching deployed path: {unmapped}")
    return files


def file_sha256(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


LIVE_PROBE = r'''
import hashlib, json, pathlib, re, shlex, subprocess, sys
request = json.load(sys.stdin)
bot = request.get("kind") == "bot"
root = pathlib.Path(request["root"])
assert root.is_absolute() and root.resolve(strict=True) == root
def state():
    fields = "ActiveState,MainPID,ExecMainStartTimestamp,WorkingDirectory,FragmentPath,DropInPaths,EnvironmentFiles,NeedDaemonReload"
    text = subprocess.check_output(["systemctl", "show", request.get("service", "wfsim-live"), "--timestamp=us+utc", "--property=" + fields], text=True)
    values = dict(line.split("=", 1) for line in text.splitlines())
    assert values["ActiveState"] == "active" and values["NeedDaemonReload"] == "no" and int(values["MainPID"]) > 0
    assert values["WorkingDirectory"] == str(root)
    start = int(subprocess.check_output(["date", "-d", values["ExecMainStartTimestamp"], "+%s%N"], text=True))
    assert start > 0
    return values, int(values["MainPID"]), start
first = state()
process = pathlib.Path("/proc") / str(first[1])
args = [item.decode() for item in (process / "cmdline").read_bytes().split(b"\0") if item]
if bot:
    assert len(args) == 2 and pathlib.Path(args[0]).name == "node"
    assert (process / "exe").resolve(strict=True).name == "node"
    assert (root / args[1]) == root / "bot/qq.mjs"
    assert (process / "cwd").resolve(strict=True) == root
else:
    assert len(args) == 4 and pathlib.Path(args[0]).name == "bash"
    assert (process / "exe").resolve(strict=True).name == "bash"
    assert (root / args[1]) == root / "scripts/live_board.sh" and args[2:] == [str(root / "live"), str(root / "bin")]
    assert (process / "cwd").resolve(strict=True) == root / "live"
hashes, latest, observed = {}, 0, {}
def signature(stat):
    return stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns
for name in [*request["files"], *([] if bot else ["bin/VERSION", "bin/witness/release.json"])]:
    path = root / name
    assert path.resolve(strict=True) == path and path.is_file()
    before = path.stat()
    h = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            h.update(chunk)
    after = path.stat()
    assert signature(before) == signature(after)
    if name in ("bin/wfsim-intake", "bin/wfsim-board"):
        assert after.st_mode & 0o111 == 0o111
    observed[str(path)] = signature(after)
    latest = max(latest, after.st_ctime_ns)
    if name in request["files"]:
        hashes[name] = h.hexdigest()
release = None if bot else json.loads((root / "bin/witness/release.json").read_text())
assert bot or isinstance(release, dict)
version = None if bot else (root / "bin/VERSION").read_text().strip()
unit = first[0]
config = [unit["FragmentPath"], *shlex.split(unit["DropInPaths"]), str((process / "exe").resolve(strict=True))]
environments = re.findall(r"(\S+) \(ignore_errors=(?:yes|no)\)", unit["EnvironmentFiles"])
assert (bot or environments) and unit["EnvironmentFiles"] == " ".join(
    item.group(0) for item in re.finditer(r"\S+ \(ignore_errors=(?:yes|no)\)", unit["EnvironmentFiles"]))
if bot:
    environment = (process / "environ").read_bytes().split(b"\0")
    selected = [item.split(b"=", 1)[1].decode() for item in environment if item.startswith(b"BOT_ENV=")]
    environments.append(selected[0] if selected else "/etc/wfsim-bot.env")
for name in config + environments:
    path = pathlib.Path(name)
    assert path.is_absolute() and path.resolve(strict=True) == path and path.is_file()
    stat = path.stat()
    observed[str(path)] = signature(stat)
    latest = max(latest, stat.st_ctime_ns)
directories = {}
if bot:
    for name in ("site/asset", "site/pkg"):
        path = root / name
        assert path.resolve(strict=True) == path
        expected = {pathlib.PurePosixPath(item).name for item in request["files"] if str(pathlib.PurePosixPath(item).parent) == name}
        assert {item.name for item in path.iterdir()} == expected
        stat = path.stat()
        directories[str(path)] = signature(stat)
        latest = max(latest, stat.st_ctime_ns)
for name, prior in observed.items():
    path = pathlib.Path(name)
    assert path.resolve(strict=True) == path and signature(path.stat()) == prior
for name, prior in directories.items():
    assert signature(pathlib.Path(name).stat()) == prior
assert state() == first and latest <= first[2]
print(json.dumps({"active": True, "pid": first[1], "started_ns": first[2], "latest_change_ns": latest,
                  "files": hashes, "release": release, "version": version}))
'''


def runtime_probe(t: dict, wanted: dict, kind: str = "live") -> dict | None:
    try:
        result = subprocess.run(["ssh", *ssh_opts(t), "-o", "ConnectTimeout=10", t["host"],
                                 "python3 -c " + shlex.quote(LIVE_PROBE)],
                                input=json.dumps({"root": t["root"], "files": list(wanted), "kind": kind,
                                                  "service": t["service"] if kind == "bot" else "wfsim-live"}),
                                cwd=ROOT, capture_output=True, text=True, timeout=45)
        if result.returncode:
            return None
        current = json.loads(result.stdout)
        if (current["active"] is True and type(current["pid"]) is int and current["pid"] > 0
                and type(current["latest_change_ns"]) is int and type(current["started_ns"]) is int
                and 0 < current["latest_change_ns"] <= current["started_ns"]):
            return current
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError):
        pass
    return None


def live_unchanged(t: dict, files: dict, rel: dict, engine: str) -> str | None:
    """Only a complete actual-file and loaded-service proof can preserve VERSION."""
    wanted = {}
    for directory, sources in files.items():
        for source in sources:
            dest = f"{directory}/{pathlib.Path(source).name}"
            if dest in ("bin/VERSION", "bin/witness/release.json"):
                continue
            wanted[dest] = hashlib.sha256(f"{engine}\n".encode()).hexdigest() if dest == "bin/ENGINE" else file_sha256(ROOT / source)
    wanted["scripts/pg/.installed-lock"] = wanted["scripts/pg/package-lock.json"]
    try:
        current = runtime_probe(t, wanted)
        runtime = lambda release: json.dumps({k: v for k, v in release.items() if k not in ("commit", "release")},
                                             sort_keys=True, separators=(",", ":"), allow_nan=False)
        if (isinstance(current, dict) and isinstance(current.get("release"), dict)
                and current["active"] is True and type(current["pid"]) is int and current["pid"] > 0
                and type(current["latest_change_ns"]) is int and type(current["started_ns"]) is int
                and 0 < current["latest_change_ns"] <= current["started_ns"]
                and current["files"] == wanted and runtime(current["release"]) == runtime(rel)
                and re.fullmatch(r"[0-9a-f]{40}", current["version"])):
            return current["version"]
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError):
        pass
    return None


def deploy_live(t: dict) -> None:
    out = ROOT / "target" / "live-linux"
    if not live_binaries(out):
        return
    root, host, o = t["root"], t["host"], ssh_opts(t)
    # WHICH COMMIT MEASURED A FACT THE SERVER SHIPS, and which engine it runs —
    # the page's `ENGINE_ID`: a claim of another engine settles nothing.
    app = re.search(r'/asset/(app\.[0-9a-f]+\.js)', (ROOT / "site" / "index.html").read_text(encoding="utf-8")).group(1)
    engine = re.search(r'const ENGINE_ID = "([^"]+)"', (ROOT / "site" / "asset" / app).read_text(encoding="utf-8")).group(1)
    rel = json.loads((ROOT / "site" / "release.json").read_text(encoding="utf-8"))
    files = live_files(out, rel)
    version = live_unchanged(t, files, rel, engine)
    if version:
        print(f"live board runtime unchanged — keeping the running service and actual VERSION {version}")
        return
    print("live board runtime not proven identical — deploying normally")
    (out / "VERSION").write_bytes((run(["git", "rev-parse", "HEAD"]).strip() + "\n").encode())
    (out / "ENGINE").write_bytes(f"{engine}\n".encode())
    run(["ssh", *o, host, f"mkdir -p {root}/bin {root}/scripts {root}/live {root}/bin/incoming"])
    # INTO A SPARE DIRECTORY, THEN RENAMED: the live board may be running the
    # binary this replaces, and a running file cannot be written over — but it
    # can be renamed over, which also never leaves half a binary in place.
    send(t, files["bin"], f"{root}/bin/incoming")
    run(["ssh", *o, host, f"chmod +x {root}/bin/incoming/wfsim-*; mv -f {root}/bin/incoming/* {root}/bin/"])
    send(t, files["scripts"], f"{root}/scripts")
    # THE LIBRARY IN POSTGRES (scripts/db.mjs): its one shared piece of the
    # worker, and the client, installed there. /etc/wfsim-live.env's PG_URL
    # moves the live board onto it (deploy/wfsim-live.service).
    run(["ssh", *o, host, f"mkdir -p {root}/worker {root}/scripts/pg"])
    send(t, files["worker"], f"{root}/worker")
    # …AND THE PAGE'S OWN CODE a live script runs (`board_heads.mjs`).
    run(["ssh", *o, host, f"mkdir -p {root}/mcp"])
    send(t, files["mcp"], f"{root}/mcp")
    send(t, files["scripts/pg"], f"{root}/scripts/pg")
    # INSTALLED ONLY WHEN THE LOCK MOVED: the server has node and no npm, so an
    # install it does not need must not be what fails the deploy.
    run(["ssh", *o, host, f"cd {root}/scripts/pg && (cmp -s package-lock.json .installed-lock"
                          " || (npm ci --silent --omit=dev && cp package-lock.json .installed-lock))"])
    # THE WITNESS'S ENGINE IS THE SITE'S OWN WASM (scripts/wasm_witness.mjs): the
    # one that counts the volunteers' work, which no native build can.
    run(["ssh", *o, host, f"rm -rf {root}/bin/witness.next && mkdir -p {root}/bin/witness.next/pkg"])
    send(t, files["bin/witness"], f"{root}/bin/witness.next")
    send(t, files["bin/witness/pkg"], f"{root}/bin/witness.next/pkg")
    run(["ssh", *o, host, f"rm -rf {root}/bin/witness && mv {root}/bin/witness.next {root}/bin/witness"])
    # RESTARTED ONLY ONCE INSTALLED: the unit and its read token are put on the
    # server by hand (`deploy/wfsim-live.service`), and until then there is
    # nothing to restart.
    run(["ssh", *o, host, f"chmod +x {root}/bin/*; "
                          "if systemctl is-enabled --quiet wfsim-live 2>/dev/null; "
                          "then sudo systemctl restart wfsim-live; fi"])


def verify() -> int:
    """The engine the server holds and whether the bot is up, against `site/`'s."""
    t = target()
    want = engine_digest(ENGINE.read_text(encoding="utf-8"))
    out = run(["ssh", *ssh_opts(t), t["host"],
               f"grep -o 'ENGINE_DIGEST = \"[0-9a-f]*\"' {t['root']}/mcp/engine.js; "
               f"sleep 3; systemctl is-active {t['service']} || true"])
    got = engine_digest(out)
    active = out.strip().splitlines()[-1] if out.strip() else ""
    if got != want:
        print(f"BOT STALE — it holds engine {got or '(none)'}, site/ serves {want}")
        return 1
    if active != "active":
        print(f"BOT DOWN — {t['service']} is {active or 'unknown'} after the restart")
        return 1
    print(f"bot ok — the QQ bot runs engine {got}, the one site/ serves")
    return 0


LIVE_RELEASE = "https://wfsim.app/release.json"


def refusal(dirty: list, local_wasm: str, live_wasm: str, other: dict | None) -> str | None:
    """WHY A HAND-RUN DEPLOY MUST NOT GO, or None. `ship.py` deploys right after
    the push that publishes this tree; run alone, this copies whatever `site/`
    holds now, and a half-written build is an engine the site does not serve."""
    if other:
        return f"{other['what']} (pid {other['pid']}) is using this checkout"
    mid = [p for p in dirty if p.startswith("site/") or p == "mcp/engine.js"]
    if mid:
        return f"{len(mid)} uncommitted file(s) under site/ or mcp/engine.js, e.g. {mid[0]}: a build is mid-way or unshipped"
    if not live_wasm:
        return "the live release could not be read, so the engine cannot be compared"
    if local_wasm != live_wasm:
        return f"this tree's engine {local_wasm} is not the one wfsim.app serves ({live_wasm})"
    return None


def standalone_refusal() -> str | None:
    import urllib.request
    import checkout_lock
    porcelain = subprocess.run(["git", "status", "--porcelain"], cwd=ROOT, capture_output=True, text=True).stdout
    dirty = [ln[3:].strip() for ln in porcelain.splitlines() if ln.strip()]
    try:
        # A NAMED AGENT: the zone challenges a library's default one with a 403.
        req = urllib.request.Request(LIVE_RELEASE, headers={"user-agent": "wfsim-ship"})
        with urllib.request.urlopen(req, timeout=20) as r:
            live = json.loads(r.read()).get("wasm", "")
    except (OSError, ValueError):
        live = ""
    return refusal(dirty, engine_digest(ENGINE.read_text(encoding="utf-8")), live, checkout_lock.held_by_other())


def self_test() -> int:
    cases = [
        (([], "a1", "a1", None), None, "a clean tree on the served engine ships"),
        (([], "a1", "a1", {"what": "build_site_app.py", "pid": 7}), "using this checkout", "a peer holding the checkout stops it"),
        ((["site/pkg/wfsim_wasm_bg.x.wasm"], "a1", "a1", None), "uncommitted", "a mid-way site/ stops it"),
        ((["mcp/engine.js"], "a1", "a1", None), "uncommitted", "an uncommitted engine stops it"),
        ((["worker/x.js"], "a1", "a1", None), None, "an edit outside site/ does not"),
        (([], "b2", "a1", None), "not the one wfsim.app serves", "an engine the site does not serve stops it"),
        (([], "a1", "", None), "could not be read", "a live release it cannot read stops it"),
    ]
    bad = 0
    for args, want, what in cases:
        got = refusal(*args)
        ok = got is None if want is None else (got is not None and want in got)
        bad += 0 if ok else 1
        print(f"  {'ok  ' if ok else 'FAIL'}  {what}" + ("" if ok else f"   {got}"))
    # THE LIVE BOARD GOES WHERE `live` SAYS, and with no `live` beside the bot.
    one = {"host": "u@bot", "root": "/opt/b", "key": "k", "known_hosts": "h", "service": "s"}
    two = {**one, "live": {"host": "u@board"}}
    for ok, what in [
        (live_target(one) == one, "no `live`: the live board ships to the bot's server"),
        (live_target(two)["host"] == "u@board", "`live.host` moves the live board"),
        (live_target(two)["root"] == "/opt/b" and live_target(two)["key"] == "k", "...and keeps every field it leaves out"),
    ]:
        bad += 0 if ok else 1
        print(f"  {'ok  ' if ok else 'FAIL'}  {what}")
    # THE LIVE SCRIPTS' IMPORTS ARE ALL SENT: a worker module is shipped by the
    # walk, a script only by being listed.
    for ok, what in [
        (not unshipped(LIVE_SCRIPTS), f"every script a live script imports is sent {unshipped(LIVE_SCRIPTS)}"),
        ("worker/placeholders.js" in imported(LIVE_SCRIPTS), "the walk reaches the worker module db.mjs imports"),
        ("scripts/db.mjs" in unshipped(["scripts/live_publish.mjs"]), "...and names a script left out of the list"),
    ]:
        bad += 0 if ok else 1
        print(f"  {'ok  ' if ok else 'FAIL'}  {what}")
    from check_ship_bot_live import self_test as live_self_test
    bad += live_self_test(sys.modules[__name__])
    print()
    print("a hand-run deploy is guarded" if not bad else f"{bad} failed")
    return 1 if bad else 0


if __name__ == "__main__":
    if "--self-test" in sys.argv:
        sys.exit(self_test())
    if "--verify" not in sys.argv:
        why = standalone_refusal()
        if why:
            sys.exit(f"ship_bot: not deploying, {why}. Ship with scripts/ship.py, which deploys the tree it pushed.")
        deploy()
    sys.exit(verify())
