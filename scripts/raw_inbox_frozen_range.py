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


def exact(stream, bytes_expected):
    blob = stream.read(bytes_expected)
    if len(blob) != bytes_expected:
        raise RuntimeError("truncated PostgreSQL binary COPY stream")
    return blob


def row_fields(stream):
    length, = struct.unpack("!h", exact(stream, 2))
    if length == -1:
        return None
    if length != 8:
        raise RuntimeError("expected six source fields, a position and a portability decision")
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
    statement = f"""BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;
SET LOCAL search_path TO {args.schema}, pg_catalog, pg_temp;
SET LOCAL lock_timeout='500ms';
SET LOCAL statement_timeout='180s';
SET LOCAL work_mem='32MB';
{CONTROL}
COPY (
  SELECT b.id,b.at,b.record,b.initiator,b.family_source,b.observation,p.position,raw_archive_move_portable(b)
  FROM ONLY record_build_submissions b LEFT JOIN state_build_inbox_positions p USING(id)
  WHERE b.ctid >= '({args.start},0)'::tid AND b.ctid < '({end},0)'::tid
    AND b.id COLLATE "C">coalesce((SELECT after_id FROM state_raw_archive_move WHERE id=1),'')
  ORDER BY b.ctid
) TO STDOUT WITH (FORMAT binary);
COMMIT;
"""
    counts = {"hot": 0, "cold": 0}
    source = hashlib.sha256()
    original_bytes = 0
    env = pg_environment(args.source_service)
    with subprocess.Popen([*PSQL, "--no-password"], env=env,
                          stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL) as cmd:
        try:
            cmd.stdin.write(statement.encode())
            cmd.stdin.close()
            # The control row precedes the only COPY stream in this session.
            control = json.loads(cmd.stdout.readline(4096))
            if int(control["oid"]) != args.oid or control["generation"] != str(args.generation):
                raise RuntimeError("source OID or archive generation changed")
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
                    if lane == "cold" and fields[6] != b"\xff\xff\xff\xff":
                        raise RuntimeError("a portable input unexpectedly owns a queue position")
                    outputs[lane].write(b"\x00\x07" + b"".join(fields[:7]))
                    counts[lane] += 1
                for stream in outputs.values():
                    stream.write(END)
            if cmd.stdout.read(1) or cmd.wait(timeout=10):
                raise RuntimeError("the source snapshot did not complete")
        finally:
            if cmd.poll() is None:
                cmd.kill()
                cmd.wait()
    return {"v": 1, "control": control, "start": args.start, "end": end, "source_rows": sum(counts.values()),
            "source_sha256": source.hexdigest(), "source_bytes": original_bytes, "counts": counts}


def prove_restore(service, source_cluster, path):
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
    restore = subprocess.run([*PSQL, "--no-password", "-c", "BEGIN", "-c", verify, "-c", ddl,
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
        receipt = capture(args, root)
        source = receipt["control"]["cluster"]
        receipt["groups"] = {
            lane: prove_restore(args.restore_service, source, root / (lane + ".copy.gz"))
            for lane in receipt["counts"]
        }
        if final.exists():
            # Re-capture the source on a retry. A matching OID alone cannot identify pending input.
            previous = json.loads((final / "receipt.json").read_text())
            if previous != receipt or any(
                prove_restore(args.restore_service, source, final / (lane + ".copy.gz")) != facts
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
    return {**receipt, "replay":False}


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
    args = parser.parse_args()
    if (args.source_service == args.restore_service or not args.schema.isascii()
        or not args.schema.replace("_", "a").isalnum() or not args.schema[0].islower()
        or not (0 < args.oid <= 4294967295 and 0 <= args.start < 4294967295 and 1 <= args.blocks <= 4096)
        or args.start + args.blocks > 4294967295 or not args.work.is_dir()):
        raise ValueError("provide a frozen source, separate restore service and a bounded physical range")
    with clean_archive_lock(args.work):
        emit(verify_range(args))


if __name__ == "__main__":
    main()
