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
this repository is public. THE WORK SIGNAL'S R2 TOKEN lives in
`private/hetzner/r2-public.env`; when both its keys are filled they are merged
into the server's /etc/wfsim-factoryd.env, sent on stdin and never printed.

    python scripts/ship_factoryd.py            # deploy, then verify
    python scripts/ship_factoryd.py --verify   # only: does the daemon run HEAD, and beat?
"""
from __future__ import annotations

import io
import json
import os
import pathlib
import re
import shlex
import subprocess
import sys
import tarfile
import time
import urllib.request
from datetime import datetime, timezone
from email.utils import parsedate_to_datetime

ROOT = pathlib.Path(__file__).resolve().parent.parent
TARGET = ROOT / "private" / "hetzner" / "ssh.env"
R2_TOKEN = ROOT / "private" / "hetzner" / "r2-public.env"
R2_KEYS = ("R2_ACCESS_KEY_ID", "R2_SECRET_ACCESS_KEY", "R2_ENDPOINT", "R2_BUCKET")
SIGNAL_URL = "https://cdn.wfsim.app/work.json"
# WRITTEN EVERY TEN SECONDS AND KEPT FIVE AT THE EDGE; older than this, the signal is down.
SIGNAL_S = 60
# WHAT THE DAEMON RUNS: the worker, and the two scripts of its own.
DAEMON_FILES = ["scripts/factoryd.mjs", "scripts/factory_capacity.mjs", "scripts/sigv4.mjs"]
REMOTE = "/opt/wfsim-factoryd"
SERVICE = "wfsim-factoryd"
UNIT = ROOT / "deploy" / f"{SERVICE}.service"
# THE HEARTBEAT IS WRITTEN EVERY TEN SECONDS; older than this, the daemon is down.
BEAT_S = 60


def target() -> dict:
    if os.environ.get("FACTORY_TARGET"):
        required = ("FACTORY_TARGET", "FACTORY_SSH_KEY", "FACTORY_KNOWN_HOSTS")
        if any(not os.environ.get(k) for k in required):
            sys.exit("factory target needs FACTORY_TARGET, FACTORY_SSH_KEY and FACTORY_KNOWN_HOSTS")
        return {"host": os.environ[required[0]], "key": os.environ[required[1]], "known_hosts": os.environ[required[2]]}
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
        for p in tracked("worker") + DAEMON_FILES:
            tar.add(ROOT / p, arcname=p)
        tar.add(client, arcname="scripts/pg/node_modules/postgres")
        data = (commit + "\n").encode()
        info = tarfile.TarInfo("COMMIT")
        info.size = len(data)
        tar.addfile(info, io.BytesIO(data))
    return buf.getvalue()


def r2_token() -> dict | None:
    """The R2 token's four values, or None (said why) when a key is unfilled."""
    if any(os.environ.get(k) for k in R2_KEYS) and not all(os.environ.get(k) for k in R2_KEYS):
        sys.exit("factory signal needs all four R2 environment values")
    if all(os.environ.get(k) for k in R2_KEYS):
        return {k: os.environ[k] for k in R2_KEYS}
    if not R2_TOKEN.exists():
        print(f"signal: {R2_TOKEN.relative_to(ROOT)} is missing — the daemon ships without the work signal")
        return None
    env = dict(line.split("=", 1) for line in R2_TOKEN.read_text(encoding="utf-8").splitlines() if "=" in line and not line.startswith("#"))
    values = {k: env.get(k, "").strip() for k in R2_KEYS}
    empty = [k for k, v in values.items() if not v]
    if empty:
        print(f"signal: {', '.join(empty)} empty in {R2_TOKEN.relative_to(ROOT)} — the daemon ships without the work signal")
        return None
    return values


def merge_r2(t: dict, values: dict) -> None:
    """The token into the server's env file, every other line kept, 640 root:wfsim_factoryd."""
    lines = "".join(f"{k}={v}\n" for k, v in values.items()).encode()
    env = f"/etc/{SERVICE}.env"
    ssh(t, f"{{ grep -v '^R2_' {env} || true; cat; }} > {env}.new && install -m 640 -o root -g wfsim_factoryd {env}.new {env} && rm -f {env}.new",
        stdin=lines)


def index_sql() -> str:
    """Concurrent indexes from the same DDL a new database is laid with."""
    ddl = "\n".join((ROOT / "worker" / "pg" / p).read_text() for p in ("schema.sql", "library.sql"))
    statements = []
    for name in ("questions_odd_at", "questions_dispatched", "scores_pace"):
        found = re.search(rf"CREATE INDEX IF NOT EXISTS {name}\b[^;]+;", ddl)
        if not found:
            raise ValueError(f"factory index missing from schema: {name}")
        statements.append(found.group().replace("CREATE INDEX ", "CREATE INDEX CONCURRENTLY ", 1))
    return "SET ROLE wfsim;\nSET lock_timeout = '5s';\n" + "\n".join(statements) + "\n"


def bootstrap(t: dict) -> None:
    """Install the daemon role on a host whose databases and Node already exist."""
    ssh(t, "test -x /usr/local/bin/node && command -v psql >/dev/null && "
        "(id -u wfsim_factoryd >/dev/null 2>&1 || useradd --system --no-create-home --shell /usr/sbin/nologin wfsim_factoryd)")
    for database, filename in (("wfsim", "factoryd_grants.sql"), ("wfsim_accounts", "factoryd_accounts_grants.sql")):
        ssh(t, f"sudo -u postgres psql -d {database} -X -q -v ON_ERROR_STOP=1",
            stdin=(ROOT / "deploy" / filename).read_bytes())
    source = os.environ.get("FACTORY_ENV_FILE")
    if source:
        ssh(t, "install -m 640 -o root -g wfsim_factoryd /dev/stdin /etc/wfsim-factoryd.env",
            stdin=pathlib.Path(source).read_bytes())
    else:
        ssh(t, "if [ ! -f /etc/wfsim-factoryd.env ]; then install -m 640 -o root -g wfsim_factoryd /dev/stdin /etc/wfsim-factoryd.env; fi",
            stdin=b"PG_URL=postgres://wfsim_factoryd@localhost/wfsim\nPG_ACCOUNTS_URL=postgres://wfsim_factoryd@localhost/wfsim_accounts\nPG_SOCKET=/var/run/postgresql\n")


def deploy() -> None:
    t, commit = target(), head()
    dirty = subprocess.run(["git", "status", "--porcelain", "--", "worker", *DAEMON_FILES,
                            "scripts/ship_factoryd.py", "deploy/wfsim-factoryd.service", "deploy/factoryd_grants.sql",
                            "deploy/factoryd_accounts_grants.sql"], cwd=ROOT,
                           capture_output=True, text=True, check=True).stdout.strip()
    if dirty:
        sys.exit("ship_factoryd: worker/ or the daemon's scripts have uncommitted changes — the daemon ships a commit")
    ssh(t, "id wfsim_factoryd >/dev/null && /usr/local/bin/node --version && "
        "runuser -u wfsim_factoryd -- test -r /etc/wfsim-factoryd.env")
    token = r2_token()
    if token:
        merge_r2(t, token)
    new = f"{REMOTE}.new"
    ssh(t, f"rm -rf {new} && mkdir -p {new} && tar xzf - -C {new} && chown -R root:root {new} && chmod -R a+rX {new}",
        stdin=bundle(commit))
    ssh(t, "sudo -u postgres psql -d wfsim -X -q -v ON_ERROR_STOP=1", stdin=index_sql().encode())
    ready = ssh(t, "sudo -u postgres psql -d wfsim -X -q -At -v ON_ERROR_STOP=1", stdin=b"""
SELECT count(*) = 3 AND bool_and(i.indisvalid AND i.indisready)
FROM pg_index i JOIN pg_class c ON c.oid = i.indexrelid
WHERE c.relnamespace = 'public'::regnamespace
AND c.relname IN ('questions_odd_at', 'questions_dispatched', 'scores_pace');
""").strip()
    if ready != "t":
        sys.exit("factoryd: required indexes are not valid and ready; running daemon kept")
    # ONE PASS OF EVERY TICK, as the service's user, before anything is swapped.
    ssh(t, f"cd {new} && set -a && . /etc/{SERVICE}.env && set +a && "
           f"runuser -u wfsim_factoryd -- /usr/local/bin/node scripts/factoryd.mjs --once")
    ssh(t, f"cat > /etc/systemd/system/{SERVICE}.service && chmod 644 /etc/systemd/system/{SERVICE}.service && systemctl daemon-reload",
        stdin=UNIT.read_bytes())
    ssh(t, f"rm -rf {REMOTE}.old; if [ -d {REMOTE} ]; then mv {REMOTE} {REMOTE}.old; fi; mv {new} {REMOTE} && "
           f"systemctl enable --now {SERVICE} && systemctl restart {SERVICE}")
    time.sleep(15)
    code = verify(t)
    if code:
        ssh(t, f"systemctl stop {SERVICE} && rm -rf {REMOTE}.failed && mv {REMOTE} {REMOTE}.failed && mv {REMOTE}.old {REMOTE} && systemctl start {SERVICE}")
        print("factoryd: restored the previous daemon; new indexes retained")
    sys.exit(code)


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
    if r2_token():
        try:
            # AGED BY THE EDGE'S CLOCK (its Date header), never this machine's: a
            # desktop clock minutes off reads a fresh signal as stale.
            try:
                with urllib.request.urlopen(urllib.request.Request(SIGNAL_URL, headers={"user-agent": "ship_factoryd"}), timeout=20) as r:
                    at = json.load(r).get("at")
                    now = parsedate_to_datetime(r.headers["date"]) if r.headers.get("date") else datetime.now(timezone.utc)
            except Exception:
                probe = ("import json,urllib.request; "
                    f"r=urllib.request.urlopen({SIGNAL_URL!r}, timeout=20); "
                    "print(json.dumps({'body':json.load(r),'date':r.headers.get('date')}))")
                seen = json.loads(ssh(t, "python3 -c " + shlex.quote(probe)))
                at = seen["body"].get("at")
                now = parsedate_to_datetime(seen["date"]) if seen.get("date") else datetime.now(timezone.utc)
            seen = (now - datetime.fromisoformat(at.replace("Z", "+00:00"))).total_seconds() if at else None
        except Exception as e:  # noqa: BLE001 — any failure to read it is a signal that is down
            seen, at = None, f"unreadable ({e})"
        print(f"signal: {SIGNAL_URL} at {at}" + (f", {int(seen)} s old" if seen is not None else ""))
        if seen is None or seen > SIGNAL_S:
            print("factoryd: SIGNAL STALE")
            return 1
    print("factoryd ok")
    return 0


if __name__ == "__main__":
    if "--bootstrap" in sys.argv:
        bootstrap(target())
    sys.exit(verify() if "--verify" in sys.argv else deploy())
