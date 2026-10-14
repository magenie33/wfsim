#!/usr/bin/env python3
"""The settler's daemon on the database server, shipped and verified.

THE DAEMON RUNS THE WORKER'S OWN CODE (worker/settler.js and everything it
imports), so it ships from the same tree as the worker or the two judge by
different rules. This copies the worker, the daemon and its Postgres client to
/opt/wfsim-factoryd beside the old copy, runs one pass of every tick there
(`--once`), and only then swaps the copies and restarts the service — a tree
whose daemon cannot run leaves the running one alone (docs/BOARD.md
§"The settler").

WHERE THE SERVER IS lives in `private/hetzner/ssh.env` with its key, because
this repository is public.

    python scripts/ship_factoryd.py            # deploy, then verify
    python scripts/ship_factoryd.py --verify   # only: does the daemon run HEAD, and beat?
"""
import io
import pathlib
import subprocess
import sys
import tarfile
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
TARGET = ROOT / "private" / "hetzner" / "ssh.env"
REMOTE = "/opt/wfsim-factoryd"
SERVICE = "wfsim-factoryd"
UNIT = ROOT / "deploy" / f"{SERVICE}.service"
# THE HEARTBEAT IS WRITTEN EVERY TEN SECONDS; older than this, the daemon is down.
BEAT_S = 60


def target() -> dict:
    if not TARGET.exists():
        sys.exit(f"ship_factoryd: no target ({TARGET.relative_to(ROOT)}) — the daemon cannot be shipped from this tree")
    env = dict(line.split("=", 1) for line in TARGET.read_text(encoding="utf-8").splitlines() if "=" in line and not line.startswith("#"))
    return {"host": env["SSH_TARGET"].strip(), "key": str(ROOT / env["SSH_KEY"].strip()),
            "known_hosts": str(ROOT / env["SSH_KNOWN_HOSTS"].strip())}


def ssh(t: dict, command: str, stdin: bytes | None = None) -> str:
    cmd = ["ssh", "-i", t["key"], "-o", f"UserKnownHostsFile={t['known_hosts']}", "-o", "StrictHostKeyChecking=yes",
           "-o", "BatchMode=yes", t["host"], command]
    print(f"\n$ ssh {t['host']} {command[:160]}", flush=True)
    r = subprocess.run(cmd, input=stdin, capture_output=True)
    if r.returncode != 0:
        sys.exit(f"ship_factoryd: failed ({r.returncode}): {r.stderr.decode(errors='replace').strip()[-600:]}")
    return r.stdout.decode(errors="replace")


def head() -> str:
    return subprocess.run(["git", "rev-parse", "--short=10", "HEAD"], cwd=ROOT, capture_output=True, text=True, check=True).stdout.strip()


def tracked(path: str) -> list:
    out = subprocess.run(["git", "ls-files", path], cwd=ROOT, capture_output=True, text=True, check=True).stdout
    return [p for p in out.splitlines() if p]


def bundle(commit: str) -> bytes:
    """The worker as committed, the daemon, and the Postgres client it imports."""
    client = ROOT / "scripts" / "pg" / "node_modules" / "postgres"
    if not (client / "src" / "index.js").exists():
        sys.exit("ship_factoryd: scripts/pg/node_modules/postgres is missing — npm ci --prefix scripts/pg")
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz") as tar:
        for p in tracked("worker") + ["scripts/factoryd.mjs"]:
            tar.add(ROOT / p, arcname=p)
        tar.add(client, arcname="scripts/pg/node_modules/postgres")
        data = (commit + "\n").encode()
        info = tarfile.TarInfo("COMMIT")
        info.size = len(data)
        tar.addfile(info, io.BytesIO(data))
    return buf.getvalue()


def deploy() -> None:
    t, commit = target(), head()
    dirty = subprocess.run(["git", "status", "--porcelain", "--", "worker", "scripts/factoryd.mjs"], cwd=ROOT,
                           capture_output=True, text=True, check=True).stdout.strip()
    if dirty:
        sys.exit("ship_factoryd: worker/ or scripts/factoryd.mjs has uncommitted changes — the daemon ships a commit")
    new = f"{REMOTE}.new"
    ssh(t, f"rm -rf {new} && mkdir -p {new} && tar xzf - -C {new} && chown -R root:root {new} && chmod -R a+rX {new}",
        stdin=bundle(commit))
    # ONE PASS OF EVERY TICK, as the service's user, before anything is swapped.
    ssh(t, f"cd {new} && set -a && . /etc/{SERVICE}.env && set +a && "
           f"runuser -u wfsim_factoryd -- /usr/local/bin/node scripts/factoryd.mjs --once")
    ssh(t, f"install -m 644 /dev/stdin /etc/systemd/system/{SERVICE}.service && systemctl daemon-reload",
        stdin=UNIT.read_bytes())
    ssh(t, f"rm -rf {REMOTE}.old; if [ -d {REMOTE} ]; then mv {REMOTE} {REMOTE}.old; fi; mv {new} {REMOTE} && "
           f"systemctl enable --now {SERVICE} && systemctl restart {SERVICE}")
    time.sleep(15)
    sys.exit(verify(t))


def verify(t: dict | None = None) -> int:
    t = t or target()
    row = ssh(t, "sudo -u postgres psql -d wfsim -Atc \"SELECT daemon_commit, daemon_at FROM factory_snapshot WHERE id = 1\"").strip()
    commit, _, at = row.partition("|")
    want = head()
    age = ssh(t, f"sudo -u postgres psql -d wfsim -Atc \"SELECT extract(epoch FROM now() - '{at}'::timestamptz)::int\"").strip() if at else ""
    alive = age.isdigit() and int(age) <= BEAT_S
    print(f"factoryd: runs {commit or 'nothing'}, beat {age or '?'} s ago; HEAD is {want}")
    if commit != want or not alive:
        print("factoryd: NOT CURRENT — " + ("an old commit" if commit != want else "no heartbeat"))
        return 1
    print("factoryd ok")
    return 0


if __name__ == "__main__":
    sys.exit(verify() if "--verify" in sys.argv else deploy())
