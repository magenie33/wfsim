#!/usr/bin/env python3
"""Stage one indexed ranking input in restartable pages on the board Data volume."""

import argparse
import contextlib
import hashlib
import json
import os
import re
import shutil
import sqlite3
import subprocess
import time
from pathlib import Path

PAGE = 4096


def literal(text):
    return "'" + text.replace("'", "''") + "'"


class Database:
    def __init__(self):
        self.p = subprocess.Popen(
            ["psql", "-XqAt", "-v", "ON_ERROR_STOP=1"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True,
        )
        self.run(
            "SET statement_timeout='2400ms'; SET lock_timeout='120ms'; SET jit=off; SET application_name='board_canary_capture'"
        )

    def run(self, sql):
        self.p.stdin.write(sql.rstrip(";") + ";\n\\echo __page_end__\n")
        self.p.stdin.flush()
        rows = []
        for line in self.p.stdout:
            line = line.rstrip("\n")
            if line == "__page_end__":
                return rows
            rows.append(json.loads(line))
        raise RuntimeError(f"source query failed ({self.p.wait()})")

    def close(self):
        self.p.stdin.close()
        self.p.wait(timeout=8)


def identity(db):
    return db.run("""SELECT json_build_object('source',(SELECT source FROM state_board_input_source WHERE id=1),
      'cluster',(SELECT system_identifier::text FROM pg_control_system()),
      'cache',(SELECT seqcache FROM pg_sequence WHERE seqrelid='board_input_revision'::regclass),
      'triggers',(SELECT count(*) FROM pg_trigger WHERE tgname='board_input_changed' AND tgenabled IN('O','A')
        AND tgrelid IN('entity_weapon_builds'::regclass,'entity_measurement_reviews'::regclass,'state_board_measurement_selection'::regclass)))""")[
        0
    ]


def receipt(s, key, value=None):
    if value is not None:
        s.execute(
            "INSERT INTO progress VALUES(?,?) ON CONFLICT(k) DO UPDATE SET v=excluded.v",
            (key, json.dumps(value, separators=(",", ":"))),
        )
    row = s.execute("SELECT v FROM progress WHERE k=?", (key,)).fetchone()
    return json.loads(row[0]) if row else None


def initialise(db, store, weapon, ruler, native):
    store.executescript("""CREATE TABLE IF NOT EXISTS progress(k TEXT PRIMARY KEY,v TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS inputs(id TEXT PRIMARY KEY,body TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS reviews(review TEXT NOT NULL,id TEXT NOT NULL,PRIMARY KEY(review,id));
        CREATE INDEX IF NOT EXISTS reviews_by_build ON reviews(id);""")
    return start_capture(db, store, {"weapon": weapon, "ruler": ruler, "native": native},
                         "(SELECT id FROM entity_weapon_builds ORDER BY id DESC LIMIT 1)", "")


def drained_fence(db, store, spec, key):
    # INSERT locks the mailbox before nextval. Drain only the transactions
    # already holding writer locks after reading the uncached sequence.
    # Later writers receive higher versions, even if they commit first.
    fence = receipt(store, key)
    lock_sql = """SELECT coalesce(json_agg(DISTINCT virtualtransaction),'[]'::json)
      FROM pg_locks WHERE locktype='relation' AND database=(SELECT oid FROM pg_database WHERE datname=current_database())
      AND relation='state_board_input_mailbox'::regclass AND granted AND mode IN
      ('RowExclusiveLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')"""
    if fence is None:
        version = db.run("SELECT last_value FROM board_input_revision")[0]
        fence = {
            "version": version,
            "writers": db.run(lock_sql)[0],
            "source": spec["source"],
            "cluster": spec["cluster"],
        }
        with store:
            receipt(store, key, fence)
    if (
        fence.get("source", spec["source"]) != spec["source"]
        or fence.get("cluster", spec["cluster"]) != spec["cluster"]
    ):
        raise ValueError("source changed while waiting for the capture fence")
    pending = set(fence["writers"]).intersection(db.run(lock_sql)[0])
    prepared = db.run("SELECT count(*) FROM pg_prepared_xacts WHERE database=current_database()")[0]
    if pending or prepared:
        raise RuntimeError("start fence waiting for existing writers or prepared transactions")
    return fence


def start_capture(db, store, contract, upper_sql, cursor):
    state = identity(db)
    if state["cache"] != 1 or state["triggers"] != 3:
        raise ValueError(
            "source change capture needs all three producer triggers and an uncached revision sequence"
        )
    spec = {**state, **contract}
    if receipt(store, "spec") is None:
        fence = drained_fence(db, store, spec, "fence")
        header = db.run(f"""SELECT json_build_object('upper',{upper_sql},
                      'at',clock_timestamp())""")[0]
        with store:
            receipt(store, "spec", spec)
            receipt(store, "start", {**header, "version": fence["version"]})
            receipt(store, "cursor", cursor)
    if receipt(store, "spec") != spec:
        raise ValueError("partition, source or Native contract changed")
    return receipt(store, "start")


def source_rows(b, ruler):
    return f"""SELECT json_build_object('kind','source','source',json_build_object('id',b.id,'at',b.at,'record',b.record),
      'facts',f.items,'headers',h.items) FROM ({b}) b
      CROSS JOIN LATERAL(SELECT jsonb_agg(to_jsonb(s) ORDER BY mode) items FROM board_facts s
        WHERE s.identity=b.id AND s.ruler={literal(ruler)}) f
      CROSS JOIN LATERAL(SELECT coalesce(jsonb_agg(jsonb_build_object('scope',k.scope,'key',k.key,'version',m.version::text)),'[]') items
        FROM (SELECT 'build' scope,b.id key UNION SELECT 'measurement',jsonb_build_array(b.id,{literal(ruler)}::text,x->>'mode')::text
          FROM jsonb_array_elements(f.items)x UNION SELECT 'review',x->>'review_id' FROM jsonb_array_elements(f.items)x
          WHERE x->>'review_id' IS NOT NULL) k
        CROSS JOIN LATERAL(SELECT version FROM state_board_input_mailbox WHERE scope=k.scope AND key=k.key OFFSET 0)m)h
      WHERE f.items IS NOT NULL"""


def save(store, rows):
    for r in rows:
        key = r["source"]["id"]
        body = json.dumps(r, separators=(",", ":"), ensure_ascii=False)
        if len(body.encode()) > 8 * 1024**2:
            raise ValueError("single source exceeds the publisher row budget")
        store.execute("INSERT OR REPLACE INTO inputs VALUES(?,?)", (key, body))
        store.execute("DELETE FROM reviews WHERE id=?", (key,))
        store.executemany(
            "INSERT OR IGNORE INTO reviews VALUES(?,?)",
            [(f["review_id"], key) for f in r["facts"] if f.get("review_id")],
        )


def capture_page(db, store, weapon, ruler, upper, count):
    cursor = receipt(store, "cursor")
    db.run("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
    page = db.run(f"""SELECT json_build_object('end',max(id),'n',count(*)) FROM
      (SELECT id FROM entity_weapon_builds WHERE id>{literal(cursor)} AND id<={literal(upper)}
       ORDER BY id LIMIT {int(count)}) keys""")[0]
    if page["n"]:
        key = page["end"]
        rows = db.run(
            source_rows(
                f"""SELECT id,at,record FROM entity_weapon_builds
          WHERE id>{literal(cursor)} AND id<={literal(key)} AND weapon={literal(weapon)} OFFSET 0""",
                ruler,
            )
        )
    else:
        key, rows = cursor, []
    db.run("COMMIT")
    with store:
        save(store, rows)
        receipt(store, "cursor", key)
        receipt(store, "scanned", (receipt(store, "scanned") or 0) + page["n"])
        receipt(store, "pages", (receipt(store, "pages") or 0) + 1)
        if page["n"] < count or key == upper:
            receipt(store, "scanned_all", True)


def chunks(items, size):
    for i in range(0, len(items), size):
        yield items[i : i + size]


def replay(db, store, start, weapon, ruler):
    if not receipt(store, "scanned_all"):
        raise ValueError("complete source capture required")
    db.run("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
    cut = db.run(
        "SELECT json_build_object('snapshot',pg_current_snapshot()::text,'captured_at',board_input_timestamp(transaction_timestamp()))"
    )[0]
    scope, key = "", ""
    ids, review_ids = set(), set()
    examined = changed = 0
    while True:
        page = db.run(f"""WITH page AS MATERIALIZED (SELECT scope,key,version,
          CASE WHEN scope='review' AND version>{int(start["version"])} THEN body->>'identity' END identity
          FROM state_board_input_mailbox WHERE (scope,key)>({literal(scope)},{literal(key)})
          ORDER BY scope,key LIMIT 4096)
          SELECT json_build_object('n',count(*),'tail',(SELECT json_build_array(scope,key) FROM page
            ORDER BY scope DESC,key DESC LIMIT 1),
            'changed',coalesce(json_agg(json_build_array(scope,key,identity)) FILTER(WHERE version>{int(start["version"])}),'[]'))
            FROM page""")[0]
        examined += page["n"]
        for kind, item, identity_id in page["changed"]:
            changed += 1
            if kind == "build":
                ids.add(item)
            elif kind == "measurement":
                identity_id, board, _ = json.loads(item)
                if board == ruler:
                    ids.add(identity_id)
            elif kind == "review":
                review_ids.add(item)
                if identity_id:
                    ids.add(identity_id)
        if page["n"] < 4096:
            break
        scope, key = page["tail"]
    for batch in chunks(sorted(review_ids), 128):
        marks = ",".join("?" for _ in batch)
        ids.update(
            r[0] for r in store.execute(f"SELECT DISTINCT id FROM reviews WHERE review IN({marks})", batch)
        )
        values = ",".join(literal(v) for v in batch)
        ids.update(
            db.run(
                f"SELECT to_json(identity) FROM entity_measurement_reviews WHERE id IN({values}) AND ruler={literal(ruler)}"
            )
        )
    changed_sources = changed_facts = 0
    with store:
        for batch in chunks(sorted(ids), 128):
            marks = ",".join("?" for _ in batch)
            prior = {
                k: json.loads(v)
                for k, v in store.execute(f"SELECT id,body FROM inputs WHERE id IN({marks})", batch)
            }
            store.execute(f"DELETE FROM inputs WHERE id IN({marks})", batch)
            store.execute(f"DELETE FROM reviews WHERE id IN({marks})", batch)
            values = ",".join(literal(v) for v in batch)
            rows = db.run(
                source_rows(
                    f"SELECT id,at,record FROM entity_weapon_builds WHERE id IN({values}) AND weapon={literal(weapon)}",
                    ruler,
                )
            )
            current = {r["source"]["id"]: r for r in rows}
            for key in prior.keys() | current.keys():
                old, new = prior.get(key), current.get(key)
                if not old or not new or old["source"] != new["source"] or old["facts"] != new["facts"]:
                    changed_sources += 1
                if (old or {}).get("facts") != (new or {}).get("facts"):
                    changed_facts += 1
            save(store, rows)
        if identity(db) != {k: receipt(store, "spec")[k] for k in ("source", "cluster", "cache", "triggers")}:
            raise ValueError("source changed during capture")
        db.run("COMMIT")
        result = {
            **cut,
            "examined_mailbox": examined,
            "changed_keys": changed,
            "reloaded_ids": len(ids),
            "changed_inputs": changed_sources,
            "changed_fact_sets": changed_facts,
        }
        receipt(store, "replay", result)
    return result


def manifest(store, stage):
    spec = receipt(store, "spec")
    cut = receipt(store, "replay")
    if not cut:
        raise ValueError("a coherent change replay is required")
    meta = {
        "kind": "meta",
        "v": 1,
        **{k: spec[k] for k in ("weapon", "ruler", "source", "native")},
        "snapshot": cut["snapshot"],
        "captured_at": cut["captured_at"],
    }
    result = write_manifest(stage, meta, (body for (body,) in store.execute("SELECT body FROM inputs ORDER BY id")))
    with store:
        receipt(store, "manifest", result)
    return result


def write_manifest(stage, meta, bodies):
    path = stage / "inputs.ndjson"
    partial = stage / "inputs.partial"
    digest = hashlib.sha256()
    count = facts = 0
    with partial.open("wb") as out:

        def add(line):
            data = (line + "\n").encode()
            if out.tell() + len(data) > 512 * 1024**2:
                raise ValueError("captured input exceeds the 512 MiB ranking limit")
            out.write(data)
            digest.update(data)

        add(json.dumps(meta, separators=(",", ":")))
        for body in bodies:
            add(body)
            count += 1
            facts += len(json.loads(body)["facts"])
        add(
            json.dumps(
                {"kind": "complete", "total": count, "facts": facts, "snapshot": meta["snapshot"]},
                separators=(",", ":"),
            )
        )
        out.flush()
        os.fsync(out.fileno())
    os.replace(partial, path)
    directory = os.open(stage, os.O_RDONLY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)
    result = {
        **meta,
        "path": str(path.resolve()),
        "total": count,
        "facts": facts,
        "sha256": digest.hexdigest(),
        "bytes": path.stat().st_size,
    }
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("stage", type=Path)
    parser.add_argument("--weapon", required=True)
    parser.add_argument("--ruler", required=True)
    parser.add_argument("--native", required=True)
    parser.add_argument("--seconds", type=float, default=12)
    parser.add_argument("--page", type=int, default=PAGE)
    parser.add_argument("--action", choices=["scan", "replay", "manifest"], default="scan")
    args = parser.parse_args()
    if not all(re.fullmatch("[a-z0-9_]{1,160}", v) for v in (args.weapon, args.ruler)) or not re.fullmatch(
        "[a-f0-9]{64}", args.native
    ):
        parser.error("invalid scope or Native SHA-256")
    if not 1 <= args.page <= 8192 or not 0 < args.seconds <= 60:
        parser.error("invalid page or invocation budget")
    if args.stage.is_symlink() or not args.stage.is_dir():
        parser.error("use an existing private stage directory")
    if os.environ.get("WFSIM_BOARD_STORAGE_REQUIRED") == "1":
        data = os.environ["WFSIM_BOARD_DATA_MOUNT"]
        if os.stat(data).st_dev == os.stat("/").st_dev or os.stat(args.stage).st_dev != os.stat(data).st_dev:
            raise ValueError("staging requires the mounted Data disk")
        if shutil.disk_usage(data).free < 32 * 1024**3 or shutil.disk_usage("/").free < 1024**3:
            raise ValueError("insufficient ranking storage reserve")
    import fcntl

    with (args.stage / "capture.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        with (
            contextlib.closing(sqlite3.connect(args.stage / "capture.sqlite")) as store,
            contextlib.closing(Database()) as db,
        ):
            start = initialise(db, store, args.weapon, args.ruler, args.native)
            if args.action == "replay":
                print(json.dumps(replay(db, store, start, args.weapon, args.ruler)))
                return
            if args.action == "manifest":
                print(json.dumps(manifest(store, args.stage)))
                return
            end = time.monotonic() + args.seconds
            while not receipt(store, "scanned_all") and time.monotonic() < end:
                capture_page(db, store, args.weapon, args.ruler, start["upper"] or "", args.page)
            print(
                json.dumps(
                    {k: receipt(store, k) for k in ["start", "cursor", "pages", "scanned", "scanned_all"]}
                ),
                flush=True,
            )


if __name__ == "__main__":
    main()
