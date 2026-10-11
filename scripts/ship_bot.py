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


def deploy() -> None:
    t = target()
    src = ENGINE.read_text(encoding="utf-8")
    wasm = re.search(r'^import module from "\.\./site/pkg/([^"]+)";$', src, re.M).group(1)
    root, host, o = t["root"], t["host"], ssh_opts(t)
    bot = sorted(str(p.relative_to(ROOT)) for p in (ROOT / "bot").glob("*.mjs"))
    run(["scp", "-q", *o, *bot, f"{host}:{root}/bot/"])
    run(["scp", "-q", *o, "mcp/engine.js", "mcp/headless.js", f"{host}:{root}/mcp/"])
    run(["scp", "-q", *o, f"site/pkg/{wasm}", f"{host}:{root}/site/pkg/"])
    # ONE WASM ON THE SERVER: the engine names exactly one, and an old one left
    # beside it is a file nothing reads that a later glance mistakes for live.
    run(["ssh", *o, host, f"cd {root}/site/pkg && ls | grep -vx '{wasm}' | xargs -r rm -f; "
                          f"sudo systemctl restart {t['service']}"])
    deploy_live(t)


LIVE_SCRIPTS = ["scripts/live_board.sh", "scripts/live_orders.mjs", "scripts/order_credit.mjs", "scripts/live_publish.mjs",
                "scripts/board_meta.py", "scripts/fetch_library.sh", "scripts/fetch_facts.sh",
                "scripts/ship_facts.sh", "scripts/ship_builds.sh",
                "scripts/ship_queue.sh"]


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
    run(["scp", "-q", *o, str(out / "wfsim-intake"), str(out / "wfsim-board"),
         str(out / "VERSION"), str(out / "ENGINE"), f"{host}:{root}/bin/incoming/"])
    run(["ssh", *o, host, f"chmod +x {root}/bin/incoming/wfsim-*; mv -f {root}/bin/incoming/* {root}/bin/"])
    run(["scp", "-q", *o, *LIVE_SCRIPTS, f"{host}:{root}/scripts/"])
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


if __name__ == "__main__":
    if "--verify" not in sys.argv:
        deploy()
    sys.exit(verify())
