#!/usr/bin/env python3
"""Capture one frozen physical range without altering the source or archive cursor."""

import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
import tempfile
import time
import uuid

from raw_inbox_rollover import PSQL, clean_archive_lock, emit, pg_environment


HEADER = b"PGCOPY\n\xff\r\n\x00" + bytes(8)
END = b"\xff\xff"
MAX_SOURCE_BYTES = 64 * 1024 * 1024
CONTROL = """
SELECT jsonb_build_object('cluster', (SELECT system_identifier::text FROM pg_control_system()),
  'db', current_database(), 'oid', m.source_oid, 'generation', g.generation,
  'source', m.online_source, 'writer_receipt', m.writer_receipt, 'after', m.after_id,
  'sealed_parts', g.sealed_parts, 'sealed_rows', g.sealed_rows,
  'heap', pg_relation_filenode(m.source_oid), 'bytes', pg_relation_size(m.source_oid),
  'block_size', current_setting('block_size')::int)
FROM state_raw_archive_move m JOIN state_raw_archive_generation g USING(generation)
WHERE m.id=1 AND m.phase='copying' AND g.phase='sealing' AND m.online_source IS NOT NULL
  AND m.upload_digest IS NULL AND m.source_oid='record_build_submissions'::regclass::oid
  AND NOT EXISTS (SELECT FROM record_raw_archive_uploads u WHERE u.generation=m.generation);
"""
PORTABLE_BODY = "314a8bf58b12fa57c00df597272da8971b25a0e047a220d5de5b74608e0780df"
INTRINSIC_PORTABLE = """
CASE WHEN NOT raw_archive_legacy_id(b.id) OR b.observation IS NOT NULL
    OR split_part(coalesce(b.family_source,''),'|',1)='workflow' THEN false
  ELSE coalesce(CASE WHEN pg_input_is_valid(b.record,'jsonb') THEN b.record::jsonb->>'kind' END,'')
       NOT IN('warframe','companion')
END
"""
INLINE_OWNERS = f"""
CASE WHEN p.id IS NOT NULL THEN false
 ELSE ({INTRINSIC_PORTABLE})
    AND NOT EXISTS(SELECT FROM state_build_family_inbox WHERE id=b.id)
    AND NOT EXISTS(SELECT FROM entity_board_requests WHERE id=b.id)
    AND NOT EXISTS(SELECT FROM relation_task_consumers WHERE ref=b.id AND state='live')
END
"""


def exact(stream, bytes_expected):
    blob = stream.read(bytes_expected)
    if len(blob) != bytes_expected:
        raise RuntimeError("truncated PostgreSQL binary COPY stream")
    return blob


def row_fields(stream, columns=8):
    length, = struct.unpack("!h", exact(stream, 2))
    if length == -1:
        return None
    if length != columns:
        raise RuntimeError("unexpected field count in PostgreSQL binary COPY")
    fields = []
    for _ in range(length):
        size = exact(stream, 4)
        width, = struct.unpack("!i", size)
        if not -1 <= width <= MAX_SOURCE_BYTES:
            raise RuntimeError("invalid binary COPY field length")
        fields.append(size + (exact(stream, width) if width > 0 else b""))
    return fields


def capture(args, directory):
    end = args.start + args.blocks
    guard = ""
    decision_sql = "raw_archive_move_portable(b)"
    lock_frame = ""
    join_sql = "LEFT JOIN state_build_inbox_positions p USING(id)"
    position_sql = "p.position"
    if args.inline_owners or args.owner_map:
        guard = f"""DO $$ BEGIN
  IF NOT EXISTS(SELECT FROM pg_proc WHERE oid='raw_archive_move_portable(record_build_submissions)'::regprocedure
    AND encode(sha256(convert_to(prosrc,'UTF8')),'hex')='{PORTABLE_BODY}')
  THEN RAISE EXCEPTION 'archive portability rules changed'; END IF;
END $$;"""
        decision_sql = INLINE_OWNERS
    if args.owner_map:
        from raw_inbox_frozen_owners import OwnerMap, freeze_sql
        lock_frame = freeze_sql(args.freeze_pid)
        join_sql = ""
        position_sql = "NULL::bigint"
        decision_sql = INTRINSIC_PORTABLE
    statement = f"""BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;
SET LOCAL search_path TO {args.schema}, pg_catalog, pg_temp;
SET LOCAL lock_timeout='500ms';
SET LOCAL statement_timeout='180s';
SET LOCAL work_mem='32MB';
{guard}
{CONTROL}
{lock_frame}
COPY (
  SELECT b.id,b.at,b.record,b.initiator,b.family_source,b.observation,{position_sql},{decision_sql}
  FROM ONLY record_build_submissions b {join_sql}
  WHERE b.ctid >= '({args.start},0)'::tid AND b.ctid < '({end},0)'::tid
    AND b.id COLLATE "C">coalesce((SELECT after_id FROM state_raw_archive_move WHERE id=1),'')
  ORDER BY b.ctid
) TO STDOUT WITH (FORMAT binary);
{lock_frame}
COMMIT;
"""
    counts = {"hot": 0, "cold": 0}
    source = hashlib.sha256()
    original_bytes = 0
    env = pg_environment(args.source_service)
    with subprocess.Popen([*PSQL, "--no-password"], env=env,
                          stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL) as cmd:
        owners = None
        try:
            cmd.stdin.write(statement.encode())
            cmd.stdin.close()
            # The control row precedes the only COPY stream in this session.
            control = json.loads(cmd.stdout.readline(4096))
            if int(control["oid"]) != args.oid or control["generation"] != str(args.generation):
                raise RuntimeError("source OID or archive generation changed")
            if args.owner_map:
                freeze = json.loads(cmd.stdout.readline(4096))
                if args.owners is None:
                    args.owners = OwnerMap(args.owner_map,control,freeze)
                owners = args.owners
                if owners.control != control or owners.freeze != freeze:
                    raise RuntimeError("the exact owner lock or source changed")
            if end > control["bytes"] // control["block_size"]:
                raise RuntimeError("range extends beyond the captured heap")
            if exact(cmd.stdout, len(HEADER)) != HEADER:
                raise RuntimeError("unrecognized PostgreSQL binary COPY header")
            with gzip.open(directory / "hot.copy.gz", "wb", compresslevel=1) as hot, \
                    gzip.open(directory / "cold.copy.gz", "wb", compresslevel=1) as cold:
                outputs = {"hot": hot, "cold": cold}
                for stream in outputs.values():
                    stream.write(HEADER)
                while (fields := row_fields(cmd.stdout)) is not None:
                    raw = b"\x00\x06" + b"".join(fields[:6])
                    original_bytes += len(raw)
                    if original_bytes > MAX_SOURCE_BYTES:
                        raise RuntimeError("physical range exceeds its decoded payload budget")
                    source.update(raw)
                    decision = fields[7]
                    if decision not in (b"\x00\x00\x00\x01\x00", b"\x00\x00\x00\x01\x01"):
                        raise RuntimeError("portability decision must be a PostgreSQL boolean")
                    lane = "cold" if decision[-1] == 1 else "hot"
                    if owners is not None:
                        active, position = owners.get(fields[0][4:])
                        if active:
                            lane = "hot"
                            if position is not None:
                                fields[6] = b"\x00\x00\x00\x08" + position
                    if lane == "cold" and fields[6] != b"\xff\xff\xff\xff":
                        raise RuntimeError("a portable input unexpectedly owns a queue position")
                    outputs[lane].write(b"\x00\x07" + b"".join(fields[:7]))
                    counts[lane] += 1
                for stream in outputs.values():
                    stream.write(END)
            if owners is not None and json.loads(cmd.stdout.readline(4096)) != freeze:
                raise RuntimeError("the exact owner lock was lost during capture")
            if cmd.stdout.read(1) or cmd.wait(timeout=10):
                raise RuntimeError("the source snapshot did not complete")
        finally:
            if cmd.poll() is None:
                cmd.kill()
                cmd.wait()
    return {"v": 1, "control": control, "start": args.start, "end": end, "source_rows": sum(counts.values()),
            "source_sha256": source.hexdigest(), "source_bytes": original_bytes, "counts": counts}


def prove_restore(service, source_cluster, path, temp_mb=None):
    if not source_cluster.isdecimal():
        raise RuntimeError("invalid source system identifier")
    with gzip.open(path, "rb") as content:
        body = content.read(MAX_SOURCE_BYTES + 2 * 1024 * 1024 + 1)
    if len(body) > MAX_SOURCE_BYTES + 2 * 1024 * 1024:
        raise RuntimeError("decoded part exceeded the restore budget")
    verify = f"SELECT 1/((system_identifier::text<>'{source_cluster}')::int) FROM pg_control_system()"
    ddl = """CREATE TEMP TABLE restored (
  id text COLLATE "C" PRIMARY KEY, at text NOT NULL, record text NOT NULL,
  initiator text, family_source text, observation text, position bigint
)"""
    buffers = ["-c", f"SET LOCAL temp_buffers='{temp_mb}MB'"] if temp_mb else []
    restore = subprocess.run([*PSQL, "--no-password", "-c", "BEGIN", *buffers, "-c", verify, "-c", ddl,
        "-c", "COPY restored FROM STDIN WITH (FORMAT binary)",
        "-c", "COPY restored TO STDOUT WITH (FORMAT binary)", "-c", "ROLLBACK"],
        env=pg_environment(service), input=body, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, timeout=180)
    if restore.returncode or restore.stdout != b"1\n" + body:
        raise RuntimeError("independent PostgreSQL did not replay the exact source fields and FIFO positions")
    return {"sha256":hashlib.sha256(body).hexdigest(), "bytes":len(body), "compressed":path.stat().st_size}


def sync_file(path):
    with path.open("rb") as source:
        os.fsync(source.fileno())


def sync_directory(path):
    fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def verify_range(args):
    final = args.work / f"{args.generation}.{args.start}-{args.start + args.blocks}"
    with tempfile.TemporaryDirectory(prefix=".raw-range-", dir=args.work) as staging:
        root = Path(staging)
        started = time.monotonic()
        receipt = capture(args, root)
        captured = time.monotonic()
        source = receipt["control"]["cluster"]
        receipt["groups"] = {
            lane: prove_restore(args.restore_service, source, root / (lane + ".copy.gz"), args.restore_temp_mb)
            for lane in receipt["counts"]
        }
        restored = time.monotonic()
        if final.exists():
            # Re-capture the source on a retry. A matching OID alone cannot identify pending input.
            previous = json.loads((final / "receipt.json").read_text())
            if previous != receipt or any(
                prove_restore(args.restore_service, source, final / (lane + ".copy.gz"), args.restore_temp_mb) != facts
                for lane, facts in previous["groups"].items()
            ):
                raise RuntimeError("the range or its existing verified payload changed")
            return {**receipt, "replay":True}
        (root / "receipt.json").write_text(json.dumps(receipt, separators=(",", ":")) + "\n")
        for path in root.iterdir():
            sync_file(path)
        sync_directory(root)
        root.rename(final)
        sync_directory(args.work)
    return {**receipt, "replay":False, "timing_ms":{
        "capture":round((captured-started)*1000), "restore":round((restored-captured)*1000),
        "durability":round((time.monotonic()-restored)*1000),
    }}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-service",required=True)
    parser.add_argument("--restore-service",required=True)
    parser.add_argument("--schema",default="public")
    parser.add_argument("--generation",type=uuid.UUID,required=True)
    parser.add_argument("--oid",type=int,required=True)
    parser.add_argument("--work",type=Path,required=True)
    parser.add_argument("--start",type=int,required=True)
    parser.add_argument("--blocks",type=int,required=True)
    parser.add_argument("--parts",type=int,default=1)
    parser.add_argument("--inline-owners",action="store_true")
    parser.add_argument("--owner-map",type=Path)
    parser.add_argument("--freeze-pid",type=int)
    parser.add_argument("--restore-temp-mb",type=int,choices=(16,32,64,96,128))
    args = parser.parse_args()
    if (args.source_service == args.restore_service or not args.schema.isascii()
        or not args.schema.replace("_", "a").isalnum() or not args.schema[0].islower()
        or not (0 < args.oid <= 4294967295 and 0 <= args.start < 4294967295 and 1 <= args.blocks <= 4096)
        or not 1<=args.parts<=64 or args.start + args.blocks*args.parts > 4294967295 or not args.work.is_dir()):
        raise ValueError("provide a frozen source, separate restore service and a bounded physical range")
    if args.owner_map and (args.freeze_pid is None or not 0<args.freeze_pid<=2147483647):
        raise ValueError("the exact owner map requires its active PostgreSQL lock holder")
    args.owners = None
    try:
        with clean_archive_lock(args.work):
            for _ in range(args.parts):
                emit(verify_range(args))
                args.start += args.blocks
    finally:
        if args.owners is not None:
            args.owners.close()


if __name__ == "__main__":
    main()
