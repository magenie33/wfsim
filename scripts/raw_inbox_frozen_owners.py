#!/usr/bin/env python3
"""Snapshot exact archive owners once while PostgreSQL keeps the source frozen."""

import argparse
from contextlib import ExitStack
import gzip
import hashlib
import json
import mmap
import os
from pathlib import Path
import re
import resource
import struct
import subprocess
import tempfile
import uuid

from raw_inbox_rollover import PSQL, clean_archive_lock, emit, pg_environment


ENTRY = struct.Struct("!16s?8s")
UUID = re.compile(rb"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\Z")
LOCKED = (
    "record_build_submissions", "state_build_inbox_positions", "state_build_family_inbox",
    "entity_board_requests", "relation_task_consumers", "state_raw_archive_move",
    "state_raw_archive_generation", "record_raw_archive_parts", "record_raw_archive_uploads",
)
SOURCES = (
    "SELECT id,position,0::smallint AS owner_kind FROM state_build_inbox_positions",
    "SELECT id,NULL::bigint,1::smallint FROM state_build_family_inbox",
    "SELECT id,NULL::bigint,2::smallint FROM entity_board_requests",
    "SELECT ref,NULL::bigint,3::smallint FROM relation_task_consumers WHERE state='live'",
)
EXPORT_NAMES = ("positions","family","requests","consumers")


def freeze_sql(pid):
    locked = ",".join(f"'{table}'::regclass::oid" for table in LOCKED)
    return f"""
SELECT jsonb_build_object('pid',{pid},'transaction',max(l.virtualtransaction),
  'backend_start',max(a.backend_start))
FROM pg_locks l JOIN pg_stat_activity a USING(pid)
WHERE l.pid={pid} AND l.mode='ShareLock' AND l.granted AND l.relation=ANY(ARRAY[{locked}])
HAVING count(DISTINCT l.relation)={len(LOCKED)} AND count(DISTINCT l.virtualtransaction)=1;
"""


def id_bytes(key):
    return bytes.fromhex(key.replace(b"-", b"").decode("ascii")) if UUID.fullmatch(key) else None


class OwnerMap:
    """An exact 16-bit prefix index over compact full keys and optional FIFO positions."""

    def __init__(self, folder, control, freeze):
        self.root = folder
        proof = json.loads((folder / "owners.json").read_bytes())
        if proof["v"] != 1 or proof["control"] != control or proof["freeze"] != freeze:
            raise RuntimeError("frozen owner map belongs to another source or lock holder")
        self.control = control
        self.freeze = freeze
        other = (folder / "other.json").read_bytes()
        if hashlib.sha256(other).hexdigest() != proof["other_sha256"]:
            raise RuntimeError("non-UUID ownership digest changed")
        self.extra = json.loads(other)
        expect = proof["uuid_keys"] * ENTRY.size
        index = (folder / "offsets").read_bytes()
        if hashlib.sha256(index).hexdigest() != proof["index_sha256"]:
            raise RuntimeError("frozen owner offset digest changed")
        self.offsets = struct.unpack("!65537Q",index)
        if self.offsets[0] or self.offsets[-1] * ENTRY.size != expect:
            raise RuntimeError("frozen owner map offsets changed")
        with (folder / "owners").open("rb") as content:
            if os.fstat(content.fileno()).st_size != expect:
                raise RuntimeError("frozen owner map size changed")
            self.records = mmap.mmap(content.fileno(),0,access=mmap.ACCESS_READ) if expect else b""
        if hashlib.sha256(self.records).hexdigest()!=proof["uuid_sha256"]:
            self.close()
            raise RuntimeError("frozen owner digest changed")

    def get(self, key):
        binary = id_bytes(key)
        if binary is None:
            text = key.decode("utf8")
            if text not in self.extra:
                return False,None
            value = self.extra[text]
            return True, (bytes.fromhex(value) if value is not None else None)
        prefix = int.from_bytes(binary[:2],"big")
        row, upper = self.offsets[prefix:prefix+2]
        stop = upper
        while row < upper:
            middle = (row+upper)//2
            offset = middle*ENTRY.size
            if self.records[offset:offset+16] < binary:
                row = middle+1
            else:
                upper = middle
        if row == stop:
            return False,None
        found,positioned,value = ENTRY.unpack_from(self.records,row*ENTRY.size)
        return (True,value if positioned else None) if found==binary else (False,None)

    def close(self):
        if self.records:
            self.records.close()


def remember(extras, key, position):
    existing = extras.get(key)
    if existing is not None and position is not None and existing != position:
        raise RuntimeError("owner snapshot has conflicting FIFO positions")
    if key not in extras or position is not None:
        extras[key] = position


def stage_member(root, stack, bins, extras, key, position, origin, after):
    if key <= after:
        return
    binary = id_bytes(key)
    if origin and (binary is None or binary[6] >> 4 != 4 or binary[8] >> 6 != 2):
        return
    if binary is None:
        remember(extras,key.decode(),position.hex() if position is not None else None)
        return
    prefix = binary[0]
    if prefix not in bins:
        bins[prefix] = stack.enter_context((root / f"bin-{prefix:02x}").open("wb",buffering=262144))
    bins[prefix].write(ENTRY.pack(binary,position is not None,position or bytes(8)))


def export(args, root):
    from raw_inbox_frozen_range import CONTROL, HEADER, exact, row_fields

    select = "\nUNION ALL\n".join(SOURCES)
    lock_frame = freeze_sql(args.freeze_pid)
    command = f"""BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;
SET LOCAL search_path TO {args.schema}, pg_catalog, pg_temp;
SET LOCAL statement_timeout='0';
SET LOCAL lock_timeout='500ms';
{CONTROL}
{lock_frame}
COPY (SELECT id,position,owner_kind FROM ({select}) AS members
  WHERE id COLLATE "C">coalesce((SELECT after_id FROM state_raw_archive_move WHERE id=1),''))
  TO STDOUT WITH (FORMAT binary);
{lock_frame}
COMMIT;"""
    counts = [0] * len(SOURCES)
    digest = hashlib.sha256()
    extras = {}
    with ExitStack() as stack:
        proc = stack.enter_context(subprocess.Popen([*PSQL,"--no-password"],env=pg_environment(args.source_service),
                                  stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL))
        proc.stdin.write(command.encode())
        proc.stdin.close()
        bins = {}
        try:
            control = json.loads(proc.stdout.readline(4096))
            freeze = json.loads(proc.stdout.readline(4096))
            if int(control["oid"]) != args.oid or control["generation"] != str(args.generation):
                raise RuntimeError("the owner export source changed")
            after = (control["after"] or "").encode()
            if exact(proc.stdout,len(HEADER)) != HEADER:
                raise RuntimeError("owner source did not emit PostgreSQL binary COPY")
            while (fields := row_fields(proc.stdout,3)) is not None:
                key = fields[0][4:]
                position = fields[1][4:] or None
                origin, = struct.unpack("!h",fields[2][4:])
                if not 0<=origin<len(counts) or (origin==0) != (position is not None):
                    raise RuntimeError("unexpected queue position or owner kind")
                counts[origin] += 1
                digest.update(b"\x00\x03"+b"".join(fields))
                stage_member(root,stack,bins,extras,key,position,origin,after)
            if json.loads(proc.stdout.readline(4096)) != freeze or proc.stdout.read(1) or proc.wait(timeout=10):
                raise RuntimeError("the owner freeze changed during export")
        finally:
            if proc.poll() is None:
                proc.kill()
                proc.wait()
    return control,freeze,counts,digest.hexdigest(),extras


def current_frame(args):
    from raw_inbox_frozen_range import CONTROL

    statement = f"""BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;
SET LOCAL search_path TO {args.schema}, pg_catalog, pg_temp;
{CONTROL}
{freeze_sql(args.freeze_pid)}
COMMIT;"""
    proof = subprocess.run([*PSQL,"--no-password"],env=pg_environment(args.source_service),input=statement.encode(),
        stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,timeout=15,check=True).stdout.splitlines()
    if len(proof)!=2:
        raise RuntimeError("the original PostgreSQL source and owner locks must remain active")
    control,freeze = map(json.loads,proof)
    if int(control["oid"])!=args.oid or control["generation"]!=str(args.generation):
        raise RuntimeError("the owner export source changed")
    return control,freeze


def import_exports(args, root):
    from raw_inbox_frozen_range import HEADER, exact, row_fields

    witness = json.loads(args.export_proof.read_bytes())
    control,freeze = current_frame(args)
    if (witness["freeze_pid"],witness["freeze_vxid"]) != (freeze["pid"],freeze["transaction"]):
        raise RuntimeError("external owner exports belong to another freeze")
    after = (control["after"] or "").encode()
    counts = [0] * len(EXPORT_NAMES)
    source_digest = hashlib.sha256()
    extras = {}
    with ExitStack() as stack:
        bins = {}
        for origin,name in enumerate(EXPORT_NAMES):
            path = args.exports / (name + ".copy.gz")
            with path.open("rb") as raw:
                expected = witness["files"][name]
                observed = hashlib.sha256()
                for block in iter(lambda:raw.read(1024*1024),b""):
                    observed.update(block)
                if observed.hexdigest()!=expected["sha256"]:
                    raise RuntimeError("owner export file digest changed")
                raw.seek(0)
                with gzip.GzipFile(fileobj=raw,mode="rb") as stream:
                    if exact(stream,len(HEADER))!=HEADER:
                        raise RuntimeError("owner export does not contain PostgreSQL binary COPY")
                    cols = 2 if origin==0 else 1
                    while (fields := row_fields(stream,cols)) is not None:
                        counts[origin] += 1
                        key = fields[0][4:]
                        position = fields[1][4:] if origin==0 else None
                        if origin==0 and len(position)!=8:
                            raise RuntimeError("an exported FIFO position has changed type")
                        member = fields[0]+(fields[1] if origin==0 else b"\xff\xff\xff\xff")
                        source_digest.update(b"\x00\x03"+member+struct.pack("!ih",2,origin))
                        stage_member(root,stack,bins,extras,key,position,origin,after)
                    if stream.read(1) or counts[origin]!=expected["rows"]:
                        raise RuntimeError("an owner export is incomplete")
    if current_frame(args)!=(control,freeze):
        raise RuntimeError("the owner freeze changed during import")
    return control,freeze,counts,source_digest.hexdigest(),extras


def arrange(root):
    sizes = [0] * 65536
    digest = hashlib.sha256()
    count = 0
    with (root / "owners").open("wb") as output:
        for path in sorted(root.glob("bin-*")):
            mapping = {}
            for key, positioned, value in ENTRY.iter_unpack(path.read_bytes()):
                remember(mapping,key,value if positioned else None)
            for key,value in sorted(mapping.items()):
                entry = ENTRY.pack(key,value is not None,value or bytes(8))
                output.write(entry)
                digest.update(entry)
                sizes[int.from_bytes(key[:2],"big")] += 1
                count += 1
            path.unlink()
    offsets = [0]
    for size in sizes:
        offsets.append(offsets[-1]+size)
    index = struct.pack("!65537Q",*offsets)
    (root / "offsets").write_bytes(index)
    return count,digest.hexdigest(),hashlib.sha256(index).hexdigest()


def main():
    from raw_inbox_frozen_range import sync_directory, sync_file

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-service",required=True)
    parser.add_argument("--schema",default="public")
    parser.add_argument("--oid",type=int,required=True)
    parser.add_argument("--generation",type=uuid.UUID,required=True)
    parser.add_argument("--freeze-pid",type=int,required=True)
    parser.add_argument("--work",type=Path,required=True)
    parser.add_argument("--exports",type=Path)
    parser.add_argument("--export-proof",type=Path)
    args = parser.parse_args()
    if not re.fullmatch(r"[a-z][a-z0-9_]*",args.schema) or not (0<args.freeze_pid<=2147483647):
        raise ValueError("provide the current frozen lock holder and source schema")
    if (args.exports is None)!=(args.export_proof is None):
        raise ValueError("provide both the owner exports and their locked snapshot receipt")
    soft, hard = resource.getrlimit(resource.RLIMIT_NOFILE)
    if soft < 300:
        resource.setrlimit(resource.RLIMIT_NOFILE,(300,hard))
    final = args.work / f"{args.generation}.owners"
    with clean_archive_lock(args.work):
        if final.exists():
            raise RuntimeError("an exact owner snapshot already exists")
        with tempfile.TemporaryDirectory(prefix=".owners-",dir=args.work) as temporary:
            root = Path(temporary)
            control,freeze,counts,source_digest,extras = (import_exports if args.exports else export)(args,root)
            rows,digest,index_digest = arrange(root)
            other = json.dumps(extras,separators=(",", ":"),sort_keys=True)
            (root / "other.json").write_text(other)
            receipt = dict(v=1,control=control,freeze=freeze,counts=counts,
                           source_sha256=source_digest,uuid_keys=rows,other_keys=len(extras),
                           uuid_sha256=digest,index_sha256=index_digest,
                           other_sha256=hashlib.sha256(other.encode()).hexdigest())
            (root / "owners.json").write_text(json.dumps(receipt,separators=(",", ":"))+"\n")
            for path in root.iterdir():
                sync_file(path)
            sync_directory(root)
            root.rename(final)
            sync_directory(args.work)
    emit({**receipt,"path":str(final)})


if __name__=="__main__":
    main()
