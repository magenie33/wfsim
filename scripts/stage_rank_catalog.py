#!/usr/bin/env python3
"""Capture measured ranking inputs once for every partition, preserving a resumable cut."""

import argparse
import contextlib
import fcntl
import hashlib
import itertools
import json
import os
import re
import shutil
import sqlite3
import subprocess
import time
import zlib
from pathlib import Path

from stage_rank_baseline import Database, drained_fence, identity, literal, receipt, start_capture, write_manifest


def packed(value):
    return zlib.compress(json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode(), 1)


def unpacked(value):
    return json.loads(zlib.decompress(value))


def initialise(db, store, native, archive):
    store.executescript("""
      CREATE TABLE IF NOT EXISTS progress(k TEXT PRIMARY KEY,v TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS builds(id TEXT PRIMARY KEY,weapon TEXT,body BLOB,version TEXT) WITHOUT ROWID;
      CREATE TABLE IF NOT EXISTS facts(id TEXT,ruler TEXT,mode TEXT,weapon TEXT,review TEXT,body BLOB NOT NULL,
        version TEXT,review_version TEXT,verified INTEGER NOT NULL,PRIMARY KEY(id,ruler,mode)) WITHOUT ROWID;
      CREATE INDEX IF NOT EXISTS by_scope ON facts(weapon,ruler,id,mode);
      CREATE INDEX IF NOT EXISTS by_review ON facts(review,id) WHERE review IS NOT NULL;
      CREATE TABLE IF NOT EXISTS removals(id TEXT,ruler TEXT,mode TEXT,version TEXT,PRIMARY KEY(id,ruler,mode)) WITHOUT ROWID;
    """)
    return start_capture(db, store, {"format": 1, "native": native, "archive": archive},
                         """(SELECT json_build_array(identity,ruler,mode) FROM state_board_measurement_selection
                                ORDER BY identity DESC,ruler DESC,mode DESC LIMIT 1)""", ["", "", ""])


def boundary(key):
    return "(" + ",".join(map(literal, key)) + ")"


def read_page(db, cursor, upper, limit, owner=None):
    own = "" if owner is None else "AND s.identity=" + literal(owner)
    return db.run(f"""WITH page AS MATERIALIZED (
      SELECT s.identity,s.ruler,s.mode FROM state_board_measurement_selection s WHERE (s.identity,s.ruler,s.mode)>{boundary(cursor)}
      AND (s.identity,s.ruler,s.mode)<={boundary(upper)} {own} ORDER BY s.identity,s.ruler,s.mode LIMIT {int(limit)}
    ), builds AS MATERIALIZED (
      SELECT DISTINCT p.identity FROM page p
    ), records AS (
      SELECT k.identity,json_build_object('build',k.identity,'weapon',b.weapon,'at',b.at,'record',b.record,'v',m.version::text) item
      FROM builds k LEFT JOIN LATERAL (SELECT weapon,at,record FROM entity_weapon_builds WHERE id=k.identity OFFSET 0) b ON true
      LEFT JOIN LATERAL (SELECT version FROM state_board_input_mailbox WHERE scope='build' AND key=k.identity OFFSET 0) m ON true
    ), facts AS (
      SELECT p.identity,p.ruler,p.mode,json_build_object('fact',to_jsonb(p),'v',m.version::text,'rv',rm.version::text,
        'verified',coalesce(r.state='succeeded' AND (r.identity,r.ruler,r.mode)=(p.identity,p.ruler,p.mode)
          AND p.measured_by='verified:'||coalesce(r.result_engine,r.engine) AND r.completed_at=p.finished_at,false)) item
      FROM page k CROSS JOIN LATERAL (SELECT * FROM board_facts s WHERE (s.identity,s.ruler,s.mode)=(k.identity,k.ruler,k.mode) OFFSET 0) p
      LEFT JOIN LATERAL (SELECT state,identity,ruler,mode,result_engine,engine,completed_at FROM entity_measurement_reviews WHERE id=p.review_id OFFSET 0) r ON true
      LEFT JOIN LATERAL (SELECT version FROM state_board_input_mailbox WHERE scope='measurement' AND key=jsonb_build_array(p.identity,p.ruler,p.mode)::text OFFSET 0) m ON true
      LEFT JOIN LATERAL (SELECT version FROM state_board_input_mailbox WHERE scope='review' AND key=p.review_id OFFSET 0) rm ON true
    ) SELECT item FROM (SELECT 0 seq,identity,'' ruler,'' mode,item FROM records
      UNION ALL SELECT 1,identity,ruler,mode,item FROM facts) ordered ORDER BY seq,identity,ruler,mode""")


def save(store, rows):
    builds = {r["build"]: r for r in rows if "build" in r}
    for key, b in builds.items():
        value = packed({"id": key, "at": b["at"], "record": b["record"]}) if b["weapon"] else None
        store.execute("INSERT OR REPLACE INTO builds VALUES(?,?,?,?)", (key, b["weapon"], value, b["v"]))
    tail = None
    count = 0
    for r in rows:
        if "fact" not in r:
            continue
        f = r["fact"].copy()
        key = [f.pop(k) for k in ("identity", "ruler", "mode")]
        if tail is not None and key <= tail:
            raise ValueError("source fact cursor did not advance in primary-key order")
        tail = key
        count += 1
        store.execute("INSERT OR REPLACE INTO facts VALUES(?,?,?,?,?,?,?,?,?)",
                      (*key, builds[key[0]]["weapon"], f.get("review_id"), packed(f), r["v"], r["rv"], r["verified"]))
    return tail, count


def scan_page(db, store, start, limit):
    cursor = receipt(store, "cursor")
    rows = read_page(db, cursor, start["upper"] or cursor, limit)
    with store:
        tail, count = save(store, rows)
        if tail:
            receipt(store, "cursor", tail)
        receipt(store, "scanned", (receipt(store, "scanned") or 0) + count)
        if count < limit or tail == start["upper"]:
            receipt(store, "scanned_all", True)


def changes(db, store, floor, cursor, budget):
    result = db.run(f"""WITH page AS MATERIALIZED (
       SELECT scope,key,version,removed FROM state_board_input_mailbox WHERE (scope,key)>{boundary(cursor)}
       ORDER BY scope,key LIMIT 4096)
       SELECT json_build_object('n',count(*),'tail',(SELECT json_build_array(scope,key) FROM page
         ORDER BY scope DESC,key DESC LIMIT 1),'delta',coalesce(json_agg(json_build_array(scope,key,removed,version::text))
         FILTER (WHERE version>{int(floor)} AND scope IN ('build','review','measurement')),'[]')) FROM page""")[0]
    reviews = []
    for scope, key, removed, version in result["delta"]:
        if scope == "review":
            reviews.append(key)
            store.execute("INSERT OR IGNORE INTO dirty SELECT id FROM facts WHERE review=?", (key,))
            continue
        if scope == "measurement":
            measurement = json.loads(key)
            key = measurement[0]
            if removed:
                store.execute("INSERT OR REPLACE INTO removals VALUES(?,?,?,?)", (*measurement, version))
            else:
                store.execute("DELETE FROM removals WHERE (id,ruler,mode)=(?,?,?)", measurement)
        store.execute("INSERT OR IGNORE INTO dirty VALUES(?)", (key,))
    for i in range(0, len(reviews), 64):
        budget()
        keys = ",".join(literal(v) for v in reviews[i:i + 64])
        ids = db.run(f"SELECT to_json(identity) FROM entity_measurement_reviews WHERE id IN ({keys})")
        store.executemany("INSERT OR IGNORE INTO dirty VALUES(?)", ((v,) for v in ids))
    return result


def reload_id(db, store, key, limit, budget):
    store.execute("DELETE FROM facts WHERE id=?", (key,))
    store.execute("DELETE FROM builds WHERE id=?", (key,))
    upper = db.run(f"""SELECT json_build_array(identity,ruler,mode) FROM state_board_measurement_selection
          WHERE identity={literal(key)} ORDER BY ruler DESC,mode DESC LIMIT 1""")
    cursor = [key, "", ""]
    while upper:
        budget()
        tail, n = save(store, read_page(db, cursor, upper[0], limit, key))
        if n < limit or tail == upper[0]:
            break
        cursor = tail


def catchup_page(db, store, start, limit, budget):
    if not receipt(store, "scanned_all"):
        raise ValueError("source scan incomplete")
    wave = receipt(store, "wave")
    if wave is None:
        fence = drained_fence(db, store, receipt(store, "spec"), "next_fence")
        wave = {"from": receipt(store, "floor") or start["version"], "to": fence["version"], "cursor": ["", ""]}
        with store:
            store.execute("CREATE TABLE IF NOT EXISTS dirty(id TEXT PRIMARY KEY) WITHOUT ROWID")
            store.execute("DELETE FROM dirty")
            store.execute("DELETE FROM progress WHERE k='replay'")
            receipt(store, "wave", wave)
    if wave["cursor"] is not None:
        with store:
            result = changes(db, store, wave["from"], wave["cursor"], budget)
            wave["cursor"] = result["tail"] if result["n"] == 4096 else None
            receipt(store, "wave", wave)
        return False
    row = store.execute("SELECT id FROM dirty ORDER BY id LIMIT 1").fetchone()
    with store:
        if row:
            reload_id(db, store, row[0], limit, budget)
            store.execute("DELETE FROM dirty WHERE id=?", row)
            return False
        receipt(store, "floor", wave["to"])
        store.execute("DELETE FROM progress WHERE k IN ('wave','next_fence')")
    return True


def replay(db, store, start, limit, seconds):
    if not receipt(store, "scanned_all"):
        raise ValueError("source scan incomplete")
    if receipt(store, "wave"):
        raise ValueError("finish the checkpointed catchup first")
    floor = receipt(store, "floor") or start["version"]
    end = time.monotonic() + seconds

    def budget():
        if time.monotonic() >= end:
            raise TimeoutError("closing window expired; retained the previous committed capture")

    db.run("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
    try:
        cut = db.run("""SELECT json_build_object('snapshot',pg_current_snapshot()::text,'captured_at',
                   board_input_timestamp(transaction_timestamp()))""")[0]
        with store:
            store.execute("CREATE TABLE IF NOT EXISTS dirty(id TEXT PRIMARY KEY) WITHOUT ROWID")
            store.execute("DELETE FROM dirty")
            cursor = ("", "")
            examined = changed = loaded = 0
            while True:
                budget()
                result = changes(db, store, floor, cursor, budget)
                examined += result["n"]
                changed += len(result["delta"])
                if result["n"] < 4096:
                    break
                cursor = result["tail"]
            for key, in store.execute("SELECT id FROM dirty ORDER BY id"):
                budget()
                reload_id(db, store, key, limit, budget)
                loaded += 1
            spec = receipt(store, "spec")
            if any(spec[k] != v for k, v in identity(db).items()):
                raise ValueError("capture source changed")
            budget()
            store.execute("DELETE FROM dirty")
            db.run("COMMIT")
            receipt(store, "replay", {**cut, "examined": examined, "changed": changed, "reloaded": loaded})
    except BaseException:
        if db.p.poll() is None:
            db.run("ROLLBACK")
        raise
    return receipt(store, "replay")


def inputs(store, weapon, ruler):
    rows = store.execute("""SELECT f.id,f.mode,f.body,f.version,f.review_version,b.body,b.version FROM facts f
      JOIN builds b ON b.id=f.id WHERE f.weapon=? AND f.ruler=? ORDER BY f.id,f.mode""", (weapon, ruler))
    for key, group in itertools.groupby(rows, lambda row: row[0]):
        facts, headers = [], {}
        for _, mode, raw, ver, rv, build, bv in group:
            fact = dict(unpacked(raw), identity=key, ruler=ruler, mode=mode)
            facts.append(fact)
            if bv is not None:
                headers["build", key] = bv
            if ver is not None:
                headers["measurement", json.dumps([key,ruler,mode], ensure_ascii=False)] = ver
            if rv is not None:
                headers["review", fact["review_id"]] = rv
        yield json.dumps({"kind": "source", "source": unpacked(build), "facts": facts,
                          "headers": [{"scope": s, "key": k, "version": v} for (s,k),v in sorted(headers.items())]},
                         ensure_ascii=False, separators=(",", ":"))


def manifest(store, directory, weapon, ruler):
    cut, spec = receipt(store, "replay"), receipt(store, "spec")
    if not cut:
        raise ValueError("coherent replay required")
    meta = {"kind": "meta", "v": 1, "weapon": weapon, "ruler": ruler, "source": spec["source"],
            "native": spec["native"], "snapshot": cut["snapshot"], "captured_at": cut["captured_at"]}
    return write_manifest(directory, meta, inputs(store, weapon, ruler))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("stage", type=Path)
    p.add_argument("action", choices=("scan","catchup","replay","manifest"))
    p.add_argument("--archive", required=True, help="immutable archive inventory SHA-256")
    p.add_argument("--native", required=True)
    p.add_argument("--page", type=int, default=256)
    p.add_argument("--seconds", type=int, default=30)
    p.add_argument("--scope", nargs=2, metavar=("WEAPON", "RULER"))
    a = p.parse_args()
    if not (0 < a.page <= 512 and 0 < a.seconds <= 300 and
            all(re.fullmatch("[a-f0-9]{64}", x) for x in [a.native, a.archive])):
        p.error("invalid cap or digest")
    if a.stage.is_symlink() or not a.stage.is_dir():
        p.error("use a private stage directory")
    if os.environ.get("WFSIM_BOARD_STORAGE_REQUIRED") == "1":
        checker = Path(__file__).with_name("board_storage.mjs")
        root = Path(subprocess.check_output(["node", str(checker), str(a.stage)], text=True).strip())
        if a.stage.resolve() != a.stage.absolute() or not a.stage.resolve().is_relative_to(root):
            raise ValueError("catalog stage outside verified work directory")
    def capacity():
        if os.environ.get("WFSIM_BOARD_STORAGE_REQUIRED") == "1":
            mount = Path(os.environ["WFSIM_BOARD_DATA_MOUNT"])
            if (a.stage.stat().st_dev != mount.stat().st_dev or mount.stat().st_dev == os.stat("/").st_dev
                or shutil.disk_usage(mount).free < 32 * 1024**3 or shutil.disk_usage("/").free < 1024**3):
                raise ValueError("ranking storage reserve unavailable")
    capacity()
    with (a.stage/"catalog.lock").open("a") as lock, contextlib.closing(sqlite3.connect(a.stage/"catalog.sqlite")) as store:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        if a.action == "manifest":
            if not a.scope or not all(re.fullmatch("[a-z0-9_]{1,160}", x) for x in a.scope):
                p.error("manifest needs a valid scope")
            spec = receipt(store, "spec") or {}
            if (spec.get("native"),spec.get("archive")) != (a.native,a.archive):
                raise ValueError("capture contract mismatch")
            cut = receipt(store, "replay")
            if not cut:
                raise ValueError("coherent replay required")
            token = hashlib.sha256(json.dumps([spec, cut],sort_keys=True).encode()).hexdigest()
            out = a.stage.joinpath(token, *a.scope)
            if out.resolve() != out.absolute():
                raise ValueError("linked output directory")
            out.mkdir(parents=True, exist_ok=True)
            print(json.dumps(manifest(store,out,*a.scope)))
            return
        with contextlib.closing(Database()) as db:
            start = initialise(db,store,a.native,a.archive)
            if a.action == "replay":
                print(json.dumps(replay(db,store,start,a.page,a.seconds)))
                return
            end = time.monotonic() + a.seconds
            if a.action == "catchup":
                def budget():
                    capacity()
                    if time.monotonic() >= end:
                        raise TimeoutError("catchup page exceeded invocation window")
                while time.monotonic() < end and not catchup_page(db,store,start,a.page,budget):
                    pass
                print(json.dumps({"floor": receipt(store,"floor"), "wave": receipt(store,"wave")}))
                return
            while not receipt(store,"scanned_all") and time.monotonic() < end:
                capacity()
                scan_page(db,store,start,a.page)
            print(json.dumps({k: receipt(store,k) for k in ("scanned","cursor","scanned_all")}))


if __name__ == "__main__":
    main()
