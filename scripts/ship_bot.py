#!/usr/bin/env python3
"""The QQ bot server, shipped and verified by `ship.py` beside the site.

THE BOT RUNS THE ENGINE THE SITE SHIPS — `mcp/engine.js`, its wasm and
`mcp/headless.js` — on a server nothing else deploys to. Copied by hand it
drifts the way the desktop channel once did: new weapons, names and words
reach the site and not the bot, and nothing fails. So the release that ships
the site copies these files over, restarts the bot, and then asks the server
which engine it now holds (docs/AGENT.md §"The QQ bot").

WHERE THE SERVER IS lives in `private/qq/bot.json` with its key, because this
repository is public. A tree without it cannot ship the bot and says so.

    python scripts/ship_bot.py            # deploy, then verify
    python scripts/ship_bot.py --verify   # only: does the bot run the site's engine?
    python scripts/ship_bot.py --self-test

THE LIVE BOARD SHIPS WITH IT (`deploy/wfsim-live.service`): `wfsim-intake` and
`wfsim-board` built for Linux in Docker from this tree, and the scripts that
drive them. Without Docker the bot still ships and the live board keeps the
binaries it had, which is said out loud — a stale intake refuses new weapons.
"""
import json
import pathlib
import re
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
    src = ENGINE.read_text(encoding="utf-8")
    wasm = re.search(r'^import module from "\.\./site/pkg/([^"]+)";$', src, re.M).group(1)
    packs = re.findall(r'^import pack[0-9]+ from "[.][.]/site/pkg/([^"]+)";$', src, re.M)
    root, host, o = t["root"], t["host"], ssh_opts(t)
    bot = sorted(str(p.relative_to(ROOT)) for p in (ROOT / "bot").glob("*.mjs"))
    send(t, bot, f"{root}/bot")
    send(t, ["mcp/engine.js", "mcp/headless.js"], f"{root}/mcp")
    send(t, [f"site/pkg/{n}" for n in (wasm, *packs)], f"{root}/site/pkg")
    # ONE WASM ON THE SERVER, and its packs: the engine names exactly those, and
    # an old one left beside them is a file nothing reads that a later glance
    # mistakes for live.
    keep = " ".join(f"-e '{n}'" for n in (wasm, *packs))
    run(["ssh", *o, host, f"cd {root}/site/pkg && ls | grep -vx {keep} | xargs -r rm -f; "
                          f"sudo systemctl restart {t['service']}"])
    deploy_live(t)


LIVE_SCRIPTS = ["scripts/live_board.sh", "scripts/live_orders.mjs", "scripts/wasm_witness.mjs", "scripts/order_credit.mjs",
                "scripts/live_publish.mjs",
                "scripts/board_meta.py", "scripts/fetch_library.sh", "scripts/fetch_facts.sh",
                "scripts/ship_facts.sh", "scripts/ship_builds.sh",
                "scripts/ship_queue.sh", "scripts/db.sh", "scripts/db.mjs", "scripts/pgq.mjs"]


def toolchain() -> str:
    m = re.search(r'^rust = "([0-9.]+)"', (ROOT / "mise.toml").read_text(encoding="utf-8"), re.M)
    return m.group(1) if m else "1"


def live_binaries(out: pathlib.Path) -> bool:
    """`wfsim-intake` and `wfsim-board` for the server, or False with the reason."""
    if subprocess.run(["docker", "version"], capture_output=True).returncode != 0:
        print("LIVE BOARD NOT SHIPPED — Docker is not running, so no Linux binaries; "
              "the server keeps the ones it has")
        return False
    out.mkdir(parents=True, exist_ok=True)
    image = f"rust:{toolchain()}-bookworm"
    # A FAILED BUILD IS SAID, NOT FATAL: by now the site is out, and stopping
    # here would leave the bot unverified for the sake of the live board.
    r = subprocess.run(["docker", "run", "--rm", "-v", f"{ROOT}:/src:ro", "-v", f"{out}:/out",
                        "-v", "wfsim-lin-target:/target", "-v", "wfsim-cargo-reg:/usr/local/cargo/registry",
                        "-e", "CARGO_TARGET_DIR=/target", "-w", "/src", image, "bash", "-c",
                        "cargo build --release --locked --bin wfsim-intake --bin wfsim-board && "
                        "cp /target/release/wfsim-intake /target/release/wfsim-board /out/"],
                       cwd=ROOT, capture_output=True, text=True, encoding="utf-8", errors="replace")
    if r.returncode != 0:
        print(f"LIVE BOARD NOT SHIPPED — the Linux build failed: {r.stderr.strip()[-400:]}")
        return False
    return True


def deploy_live(t: dict) -> None:
    out = ROOT / "target" / "live-linux"
    if not live_binaries(out):
        return
    root, host, o = t["root"], t["host"], ssh_opts(t)
    # WHICH COMMIT MEASURED A FACT THE SERVER SHIPS, and which engine it runs —
    # the page's `ENGINE_ID`: a claim of another engine settles nothing.
    (out / "VERSION").write_bytes((run(["git", "rev-parse", "HEAD"]).strip() + "\n").encode())
    app = re.search(r'/asset/(app\.[0-9a-f]+\.js)', (ROOT / "site" / "index.html").read_text(encoding="utf-8")).group(1)
    engine = re.search(r'const ENGINE_ID = "([^"]+)"', (ROOT / "site" / "asset" / app).read_text(encoding="utf-8")).group(1)
    (out / "ENGINE").write_bytes(f"{engine}\n".encode())
    run(["ssh", *o, host, f"mkdir -p {root}/bin {root}/scripts {root}/live {root}/bin/incoming"])
    # INTO A SPARE DIRECTORY, THEN RENAMED: the live board may be running the
    # binary this replaces, and a running file cannot be written over — but it
    # can be renamed over, which also never leaves half a binary in place.
    send(t, [out / "wfsim-intake", out / "wfsim-board", out / "VERSION", out / "ENGINE"], f"{root}/bin/incoming")
    run(["ssh", *o, host, f"chmod +x {root}/bin/incoming/wfsim-*; mv -f {root}/bin/incoming/* {root}/bin/"])
    send(t, LIVE_SCRIPTS, f"{root}/scripts")
    # THE LIBRARY IN POSTGRES (scripts/db.mjs): its one shared piece of the
    # worker, and the client, installed there. /etc/wfsim-live.env's PG_URL
    # moves the live board onto it (deploy/wfsim-live.service).
    run(["ssh", *o, host, f"mkdir -p {root}/worker {root}/scripts/pg"])
    send(t, ["worker/placeholders.js"], f"{root}/worker")
    send(t, ["scripts/pg/package.json", "scripts/pg/package-lock.json"], f"{root}/scripts/pg")
    # INSTALLED ONLY WHEN THE LOCK MOVED: the server has node and no npm, so an
    # install it does not need must not be what fails the deploy.
    run(["ssh", *o, host, f"cd {root}/scripts/pg && (cmp -s package-lock.json .installed-lock"
                          " || (npm ci --silent --omit=dev && cp package-lock.json .installed-lock))"])
    # THE WITNESS'S ENGINE IS THE SITE'S OWN WASM (scripts/wasm_witness.mjs): the
    # one that counts the volunteers' work, which no native build can.
    rel = json.loads((ROOT / "site" / "release.json").read_text(encoding="utf-8"))
    wasm = rel["wasm"]
    run(["ssh", *o, host, f"rm -rf {root}/bin/witness.next && mkdir -p {root}/bin/witness.next/pkg"])
    send(t, ["site/release.json"], f"{root}/bin/witness.next")
    send(t, [f"site/pkg/wfsim_wasm.{wasm}.js", f"site/pkg/wfsim_wasm_bg.{wasm}.wasm",
             *(f"site/pkg/{n}" for n in rel.get("packs", []))], f"{root}/bin/witness.next/pkg")
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
