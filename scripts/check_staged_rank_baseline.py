#!/usr/bin/env python3
"""A restarted capture accounts for source changes on both sides of the cursor."""

import contextlib
import hashlib
import json
import os
import secrets
import sqlite3
import subprocess
import tempfile
from pathlib import Path
from unittest.mock import patch
from urllib.parse import unquote, urlparse

import stage_rank_baseline as stage

with patch.dict(os.environ):
    key = "WFSIM_BOARD_DATA_RESERVE_BYTES"
    os.environ.pop(key, None)
    assert stage.board_data_reserve() == 32 * 1024**3
    os.environ[key] = str(16 * 1024**3)
    assert stage.board_data_reserve() == 16 * 1024**3
    for invalid in ("", "16GiB", str(15 * 1024**3), str(2**53)):
        os.environ[key] = invalid
        try:
            stage.board_data_reserve()
        except ValueError:
            continue
        raise AssertionError("invalid Data reserve was accepted")

url = os.environ["WFSIM_BOARD_RANK_TEST_URL"]
if urlparse(url).hostname not in {"127.0.0.1", "localhost"}:
    raise ValueError("an isolated local PostgreSQL fixture is required")
connection = urlparse(url)
os.environ.update(
    PGHOST=connection.hostname, PGPORT=str(connection.port or 5432), PGDATABASE=unquote(connection.path[1:])
)
if connection.username:
    os.environ["PGUSER"] = unquote(connection.username)
if connection.password:
    os.environ["PGPASSWORD"] = unquote(connection.password)
schema = "rank_capture_" + secrets.token_hex(6)
weapon, ruler, native = "torid", "standard_single_target", "fixture-native"
subprocess.run(["psql", "-Xq", "-v", "ON_ERROR_STOP=1", "-c", "CREATE SCHEMA " + schema], check=True)
os.environ["PGOPTIONS"] = "-c search_path=" + schema + ",pg_catalog"
try:
    with contextlib.closing(stage.Database()) as setup:
        setup.run("""CREATE TABLE state_board_input_source(id int PRIMARY KEY,source text);
          INSERT INTO state_board_input_source VALUES(1,'source');
          CREATE SEQUENCE board_input_revision CACHE 1;
          CREATE TABLE state_board_input_mailbox(scope text,key text,version bigint,body jsonb,PRIMARY KEY(scope,key));
          CREATE TABLE entity_weapon_builds(id text COLLATE "C" PRIMARY KEY,at text,record text,weapon text);
          CREATE TABLE state_board_measurement_selection(identity text,ruler text,mode text,score float8,review_id text,PRIMARY KEY(identity,ruler,mode));
          CREATE TABLE entity_measurement_reviews(id text PRIMARY KEY,identity text,ruler text,mode text);
          CREATE VIEW board_facts AS SELECT f.*,NULL::text retired_at FROM state_board_measurement_selection f;
          CREATE FUNCTION board_input_timestamp(timestamptz) RETURNS text LANGUAGE sql AS $$SELECT $1::text$$;
          CREATE FUNCTION board_input_put(text,text,text) RETURNS bigint LANGUAGE sql AS $$
            INSERT INTO state_board_input_mailbox VALUES($1,$2,nextval('board_input_revision'),$3::jsonb)
            ON CONFLICT(scope,key) DO UPDATE SET version=excluded.version,body=excluded.body RETURNING version$$""")
        subprocess.run(
            [
                "psql",
                "-Xq",
                "-v",
                "ON_ERROR_STOP=1",
                "-f",
                str(Path(__file__).resolve().parents[1] / "worker/pg/board_capture.sql"),
            ],
            check=True,
            stdout=subprocess.DEVNULL,
        )
        setup.run("""INSERT INTO entity_weapon_builds SELECT id,'day','{}','torid' FROM unnest(ARRAY['a','c','e','g','i']) id;
          INSERT INTO state_board_measurement_selection SELECT id,'standard_single_target','base',3,NULL FROM entity_weapon_builds""")
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            with (
                contextlib.closing(sqlite3.connect(root / "capture.sqlite")) as first,
                contextlib.closing(stage.Database()) as writer,
            ):
                writer.run("BEGIN; INSERT INTO entity_weapon_builds VALUES('m','day','{}','torid')")
                try:
                    stage.initialise(setup, first, weapon, ruler, native)
                except RuntimeError as exc:
                    assert "existing writers" in str(exc)
                else:
                    raise AssertionError("capture started before the existing writer committed")
                setup.run("INSERT INTO entity_weapon_builds VALUES('n','day','{}','torid')")
                writer.run("COMMIT")
                start = stage.initialise(setup, first, weapon, ruler, native)
                stage.capture_page(setup, first, weapon, ruler, start["upper"], 2)
                assert stage.receipt(first, "cursor") == "c"
            setup.run("""UPDATE state_board_measurement_selection SET score=99 WHERE identity='a';
              DELETE FROM entity_weapon_builds WHERE id='c';
              INSERT INTO entity_weapon_builds VALUES('b','day','{}','torid'),('z','day','{}','torid');
              INSERT INTO state_board_measurement_selection VALUES('b','standard_single_target','base',4,NULL),
              ('z','standard_single_target','base',7,NULL)""")
            with contextlib.closing(sqlite3.connect(root / "capture.sqlite")) as resumed:
                while not stage.receipt(resumed, "scanned_all"):
                    stage.capture_page(setup, resumed, weapon, ruler, start["upper"], 2)
                before = dict(resumed.execute("SELECT id,body FROM inputs"))
                assert "c" in before and "b" not in before and "z" not in before
                stage.replay(setup, resumed, start, weapon, ruler)
                out = stage.manifest(resumed, root)
                data = Path(out["path"]).read_bytes()
                assert hashlib.sha256(data).hexdigest() == out["sha256"]
                rows = [json.loads(line) for line in data.splitlines()][1:-1]
                assert {r["source"]["id"]: r["facts"][0]["score"] for r in rows} == {
                    "a": 99,
                    "b": 4,
                    "e": 3,
                    "g": 3,
                    "i": 3,
                    "z": 7,
                }
                assert stage.replay(setup, resumed, start, weapon, ruler)["changed_inputs"] == 0
    print("resumable ranking capture preserves late commits, insertions, corrections and removals")
finally:
    subprocess.run(
        ["psql", "-Xq", "-v", "ON_ERROR_STOP=1", "-c", "DROP SCHEMA " + schema + " CASCADE"], check=True
    )
