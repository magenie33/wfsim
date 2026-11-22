#!/usr/bin/env python3
"""Stage a verified hot relation or prepare a cut for the original lock-holder session."""

import argparse
from contextlib import ExitStack, contextmanager
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import uuid

from raw_inbox_frozen_assemble import ranges, read_group
from raw_inbox_frozen_owners import current_frame, freeze_sql
from raw_inbox_frozen_range import CONTROL, END, HEADER, exact, row_fields, sync_directory
from raw_inbox_rollover import PSQL, clean_fence, clean_archive_lock, emit, pg_environment, read_local_object, run


HOT = "record_raw_archive_frozen_hot"
LEDGER = "state_raw_archive_frozen_hot_copy"
FIELDS = "id,at,record,initiator,family_source,observation"
IDENT = re.compile(r"[a-z][a-z0-9_]*\Z")
LEGACY = re.compile(r"[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\Z")
HEX = re.compile(r"[0-9a-f]{64}\Z")
HOT_BUCKETS = 128
HOT_HASH_BYTES = 32
HOT_BUCKET_LIMIT = 16*1024*1024


def literal(value):
    return "'" + value.replace("'", "''") + "'"


def verified_manifest(args):
    root = args.work / f"{args.generation}.assembled"
    plan = json.loads((root / "manifest.json").read_bytes())
    control,freeze = current_frame(args)
    if plan["v"]!=1 or plan["control"]!=control or plan["freeze"]!=freeze:
        raise RuntimeError("assembled storage belongs to a different frozen source")
    block = 0
    counts = {"hot":0,"cold":0}
    for capture in plan["ranges"]:
        if (capture["start"]!=block or capture["end"]<=block or capture["control"]!=control
            or capture["freeze"]!=freeze or sum(capture["counts"].values())!=capture["source_rows"]):
            raise RuntimeError("assembled physical coverage is incomplete or inconsistent")
        for lane in counts:
            counts[lane] += capture["counts"][lane]
        block = capture["end"]
    if block*control["block_size"]!=control["bytes"] or counts!=plan["counts"]:
        raise RuntimeError("assembled rows do not account for the frozen source")
    marker = json.loads((root / "spool.json").read_bytes())
    if marker != {"v":1,"generation":str(args.generation),"cloud_through":control["sealed_parts"]}:
        raise RuntimeError("existing cloud archive prefix changed")
    return root,plan,control,freeze


def stage_hot(args, root):
    env = pg_environment(args.source_service)
    settings = f"SET LOCAL search_path TO {args.schema},pg_catalog,pg_temp; SET LOCAL default_tablespace TO {args.tablespace};"
    ddl = f"CREATE TABLE {HOT} (LIKE record_build_submissions INCLUDING ALL)"
    with gzip.open(root / "hot.copy.gz","rb") as hot, subprocess.Popen(
        [*PSQL,"--no-password","-c","BEGIN","-c",settings,"-c",ddl,
         "-c",f"COPY {HOT} ({FIELDS}) FROM STDIN WITH (FORMAT binary)","-c","COMMIT"],
        env=env,stdin=subprocess.PIPE,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,
    ) as proc:
        for block in iter(lambda:hot.read(1024*1024),b""):
            proc.stdin.write(block)
        proc.stdin.close()
        if proc.wait(timeout=60):
            raise RuntimeError("could not stage the verified hot COPY on the data tablespace")


def recorded_hot(args,start):
    query = f"""BEGIN;
SET LOCAL search_path TO {args.schema},pg_catalog,pg_temp;
SELECT coalesce((SELECT to_jsonb(r) FROM {LEDGER} r WHERE r.start_block={start}),'null'::jsonb);
COMMIT;"""
    return json.loads(run(PSQL,data=query.encode(),env=pg_environment(args.source_service)))


def stage_hot_ranges(args):
    control,freeze,entries = ranges(args.work,args.generation,args.range_list,args.legacy_freeze_proof,complete=False)
    if current_frame(args)!=(control,freeze):
        raise RuntimeError("hot ranges do not belong to the live frozen source")
    settings = f"SET LOCAL search_path TO {args.schema},pg_catalog,pg_temp; SET LOCAL default_tablespace TO {args.tablespace};"
    create = f"""BEGIN;
{settings}
CREATE TABLE IF NOT EXISTS {HOT} (LIKE record_build_submissions INCLUDING ALL);
CREATE TABLE IF NOT EXISTS {LEDGER} (
  start_block bigint PRIMARY KEY, end_block bigint NOT NULL, generation uuid NOT NULL,
  source_sha text NOT NULL, group_sha text NOT NULL, rows bigint NOT NULL, hot_sha text NOT NULL
);
COMMIT;"""
    env = pg_environment(args.source_service)
    run(PSQL,data=create.encode(),env=env)
    for capture,folder in entries:
        if current_frame(args)!=(control,freeze):
            raise RuntimeError("the frozen owner changed during hot staging")
        start,end = capture["start"],capture["end"]
        original = capture["groups"]["hot"]["sha256"]
        known = dict(start_block=start,end_block=end,generation=str(args.generation),
                     source_sha=capture["source_sha256"],group_sha=original,rows=capture["counts"]["hot"])
        hold = recorded_hot(args,start)
        file = folder / "hot.copy.gz"
        if hold is None:
            logical = hashlib.sha256()
            body = bytearray(HEADER)
            count = 0
            for values,_ in read_group(file,7,capture["groups"]["hot"]):
                row = b"\x00\x06"+b"".join(values[:6])
                body.extend(row)
                logical.update(row)
                count += 1
            if count!=known["rows"]:
                raise RuntimeError("hot range row count changed")
            body.extend(END)
            known["hot_sha"] = logical.hexdigest()
            guard = f"""LOCK TABLE {LEDGER} IN SHARE ROW EXCLUSIVE MODE NOWAIT;
DO $$ BEGIN
  IF {start}<>coalesce((SELECT max(end_block) FROM {LEDGER}),0)
    OR EXISTS(SELECT FROM {LEDGER} WHERE generation<>'{args.generation}'::uuid)
    OR ({start}=0 AND EXISTS(SELECT FROM {HOT}))
  THEN RAISE EXCEPTION 'hot staging is not at the expected contiguous range'; END IF;
END $$;"""
            values = ",".join(str(value) if isinstance(value,int) else literal(value) for value in known.values())
            write = f"INSERT INTO {LEDGER} ({','.join(known)}) VALUES({values});"
            run([*PSQL,"--no-password","-c","BEGIN","-c",settings,"-c",guard,
                 "-c",f"COPY {HOT} ({FIELDS}) FROM STDIN WITH (FORMAT binary)","-c",write,"-c","COMMIT"],
                env=env,data=body,timeout=120)
            if recorded_hot(args,start)!=known:
                raise RuntimeError("could not independently read the committed hot range receipt")
        elif any(hold[key]!=value for key,value in known.items()):
            raise RuntimeError("committed hot staging receipt belongs to another range")
        if args.retire_hot_copy and file.exists():
            file.unlink()
            sync_directory(file.parent)
        emit(dict(stage="staged_hot_range",start=start,end=end,rows=known["rows"],replay=hold is not None))


def hot_fields(stream):
    while (fields := row_fields(stream,6)) is not None:
        yield fields


def write_hot_hashes(directory, label, rows, limit):
    count = 0
    with ExitStack() as resources:
        files = {}
        for fields in rows:
            count += 1
            if count>limit:
                raise RuntimeError("hot content exceeds the captured row count")
            digest = hashlib.sha256(b"\x00\x06"+b"".join(fields[:6])).digest()
            bucket = digest[0]//2
            if bucket not in files:
                files[bucket] = resources.enter_context((directory/f"{label}-{bucket}").open("wb"))
            files[bucket].write(digest)
    if count!=limit:
        raise RuntimeError("hot content does not cover the captured rows")
    return count


@contextmanager
def hot_content_witness(args, plan):
    if args.hot_verify_spool_bytes is None:
        yield None
        return
    count = plan["counts"]["hot"]
    stat = os.statvfs(args.work)
    projected = 2*(count*HOT_HASH_BYTES+HOT_BUCKETS*stat.f_frsize)
    if not plan.get("hot_staged") or projected>min(args.hot_verify_spool_bytes,stat.f_bavail*stat.f_frsize):
        raise RuntimeError("staged hot content verification exceeds its spool budget")
    control,freeze,entries = ranges(args.work,args.generation,args.range_list,args.legacy_freeze_proof)
    if control!=plan["control"] or freeze!=plan["freeze"] or [capture for capture,_ in entries]!=plan["ranges"]:
        raise RuntimeError("hot content witness changed frozen source ranges")

    def source_rows():
        for capture, folder in entries:
            rows = capture["counts"]["hot"]
            if rows:
                group = read_group(folder/"hot.copy.gz",7,capture["groups"]["hot"])
                seen = 0
                for fields,_ in group:
                    seen += 1
                    yield fields
                if seen!=rows:
                    raise RuntimeError("hot content witness changed a range row count")

    with tempfile.TemporaryDirectory(prefix="raw-hot-verify-",dir=args.work) as tmp:
        directory = Path(tmp)
        write_hot_hashes(directory,"source",source_rows(),count)
        yield directory


def match_hot_hashes(directory, stream, expected):
    count = write_hot_hashes(directory,"staged",hot_fields(stream),expected)

    def bucket_hashes(path):
        if not path.exists():
            return []
        if path.stat().st_size>HOT_BUCKET_LIMIT:
            raise RuntimeError("one hot verification bucket exceeds its memory bound")
        content = path.read_bytes()
        return sorted(content[index:index+HOT_HASH_BYTES] for index in range(0,len(content),HOT_HASH_BYTES))

    for bucket in range(HOT_BUCKETS):
        if bucket_hashes(directory/f"source-{bucket}")!=bucket_hashes(directory/f"staged-{bucket}"):
            raise RuntimeError("the staged hot rows differ from the verified source content")
    return count


def inspect_hot(args, plan, freeze):
    ledger = ""
    if plan.get("hot_staged"):
        ledger = f"""
SELECT coalesce(jsonb_agg(to_jsonb(r) ORDER BY start_block),'[]'::jsonb)
FROM {LEDGER} r
WHERE EXISTS(SELECT FROM pg_locks l WHERE l.relation='{LEDGER}'::regclass AND l.pid={freeze["pid"]}
  AND l.virtualtransaction={literal(freeze["transaction"])} AND l.mode='ShareLock' AND l.granted);
"""
    query = f"""BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;
SET LOCAL search_path TO {args.schema},pg_catalog,pg_temp;
SET LOCAL synchronize_seqscans=off;
SET LOCAL max_parallel_workers_per_gather=0;
SELECT jsonb_build_object('oid',c.oid::bigint,'file',pg_relation_filenode(c.oid)::bigint,'path',pg_relation_filepath(c.oid),
  'tablespace',c.reltablespace::bigint)
FROM pg_class c JOIN pg_locks l ON l.relation=c.oid
WHERE c.oid='{HOT}'::regclass AND l.pid={freeze["pid"]} AND l.granted
  AND l.mode='ShareLock' AND l.virtualtransaction={literal(freeze["transaction"])}
  AND c.relpersistence='p' AND pg_relation_filepath(c.oid) LIKE 'pg_tblspc/%'
  AND NOT EXISTS(SELECT FROM pg_inherits WHERE inhrelid=c.oid OR inhparent=c.oid)
  AND EXISTS(SELECT FROM pg_index WHERE indrelid=c.oid AND indisprimary AND indisvalid);
{ledger}
COPY {HOT} ({FIELDS}) TO STDOUT WITH (FORMAT binary);
COMMIT;"""
    with hot_content_witness(args,plan) as witness, subprocess.Popen(
                          [*PSQL,"--no-password"],env=pg_environment(args.source_service),
                          stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL) as proc:
        try:
            proc.stdin.write(query.encode())
            proc.stdin.close()
            hot = json.loads(proc.stdout.readline(4096))
            checks = json.loads(proc.stdout.readline(32*1024*1024)) if ledger else []
            if exact(proc.stdout,len(HEADER))!=HEADER:
                raise RuntimeError("the staged hot relation did not emit binary COPY")
            digest = hashlib.sha256(HEADER)
            count = 0
            if ledger:
                if len(checks)!=len(plan["ranges"]):
                    raise RuntimeError("staged hot ledger does not cover each proven physical range")
                for check,capture in zip(checks,plan["ranges"]):
                    if ((check["start_block"],check["end_block"],check["generation"],check["source_sha"],
                         check["group_sha"],check["rows"]) !=
                        (capture["start"],capture["end"],str(args.generation),capture["source_sha256"],
                         capture["groups"]["hot"]["sha256"],capture["counts"]["hot"])):
                        raise RuntimeError("hot stage receipt does not match its verified physical rows")
                    if witness is None:
                        segment = hashlib.sha256()
                        for _ in range(check["rows"]):
                            fields = row_fields(proc.stdout,6)
                            if fields is None:
                                raise RuntimeError("staged hot stream ended early")
                            segment.update(b"\x00\x06"+b"".join(fields))
                            count += 1
                        if segment.hexdigest()!=check["hot_sha"]:
                            raise RuntimeError("staged hot range changed after verified COPY")
                if witness is not None:
                    count = match_hot_hashes(witness,proc.stdout,plan["counts"]["hot"])
                elif row_fields(proc.stdout,6) is not None:
                    raise RuntimeError("staged hot stream exceeded its expected coverage")
            else:
                while (fields := row_fields(proc.stdout,6)) is not None:
                    digest.update(b"\x00\x06"+b"".join(fields))
                    count += 1
            digest.update(END)
            if proc.stdout.read(1) or proc.wait(timeout=10):
                raise RuntimeError("the staged hot relation could not be verified")
        finally:
            if proc.poll() is None:
                proc.kill()
                proc.wait()
    if (not ledger and digest.hexdigest()!=plan["hot_sha256"]) or count!=plan["counts"]["hot"]:
        raise RuntimeError("the staged hot relation is not the exact independently verified input")
    return hot


def verify_parts(args, root, plan, control):
    first = control["sealed_parts"]+1
    previous = control["after"] or ""
    rows = 0
    for ordinal, part in enumerate(plan["parts"],start=first):
        if (part["part"]!=ordinal or not HEX.fullmatch(part["digest"])
            or not LEGACY.fullmatch(part["first"]) or not LEGACY.fullmatch(part["last"])
            or not previous<part["first"]<=part["last"] or not 1<=part["rows"]<=2048):
            raise RuntimeError("local parts overlap or cross the existing sealed prefix")
        path = root / f'{ordinal}-{part["digest"]}.json.gz'
        packet = json.loads(read_local_object(path,part["digest"]))
        members = [item["input"]["id"] for item in packet["rows"]]
        if (packet["generation"]!=str(args.generation) or packet["part"]!=ordinal
            or len(members)!=part["rows"] or members!=sorted(set(members))
            or members[0]!=part["first"] or members[-1]!=part["last"]
            or any(item["position"] is not None or item["portable"] is not True for item in packet["rows"])):
            raise RuntimeError("local part identity, members, or queue ownership changed")
        previous = part["last"]
        rows += len(members)
    if rows!=plan["counts"]["cold"]:
        raise RuntimeError("local parts do not preserve the full cold payload")


def write_cut(args, root, plan, control, freeze, next_freeze, hot):
    fence,source = clean_fence(args.fence_file,args.generation)
    if source!=control["source"] or current_frame(args)!=(control,freeze):
        raise RuntimeError("cut evidence belongs to another frozen source or writer")
    expected = literal(json.dumps(control,separators=(",",":")))
    expected_freeze = literal(json.dumps(next_freeze,separators=(",",":")))
    query = CONTROL.strip().rstrip(";")
    keeper = freeze_sql(next_freeze["pid"]).strip().rstrip(";")
    previous_holder = "false"
    if freeze!=next_freeze:
        previous_holder = f"""EXISTS(SELECT FROM pg_locks WHERE pid={freeze["pid"]}
          AND virtualtransaction={literal(freeze["transaction"])} AND granted
          AND relation='record_build_submissions'::regclass)"""
    cold = plan["counts"]["cold"]
    part_count = len(plan["parts"])
    tail = literal(plan["parts"][-1]["last"]) if part_count else "last_id"
    ledger_guard = ""
    retire_ledger = ""
    if plan.get("hot_staged"):
        ledger_guard = f"""OR NOT EXISTS(SELECT FROM pg_locks WHERE pid=pg_backend_pid() AND granted
      AND mode='ShareLock' AND relation='{LEDGER}'::regclass)"""
        retire_ledger = f"DROP TABLE {LEDGER};"
    text = rf"""\set ON_ERROR_STOP on
SAVEPOINT frozen_inbox_cut;
SET LOCAL search_path TO {args.schema},pg_catalog,pg_temp;
CREATE TEMP TABLE frozen_parts(part bigint, first_id text, last_id text, rows int, digest text) ON COMMIT DROP;
CREATE TEMP TABLE frozen_result(bytes bigint,code text) ON COMMIT DROP;
COPY frozen_parts FROM STDIN WITH (FORMAT csv);
"""
    destination = root / "commit.sql"
    with destination.open("x") as script:
        script.write(text)
        for part in plan["parts"]:
            script.write(",".join(str(part[key]) for key in ("part","first","last","rows","digest"))+"\n")
        script.write(rf"""\.
DO $cut$
DECLARE released bigint;
BEGIN
  IF pg_backend_pid()<>{next_freeze["pid"]} OR NOT EXISTS(
    SELECT FROM pg_locks WHERE pid=pg_backend_pid() AND granted AND mode='ShareLock'
      AND virtualtransaction={literal(next_freeze["transaction"])} AND relation='{HOT}'::regclass)
    OR ({previous_holder})
    OR (SELECT pg_relation_filenode('{HOT}'))<>{int(hot["file"])}::oid
    OR '{HOT}'::regclass::oid<>{int(hot["oid"])}::oid
    OR ({query}) IS DISTINCT FROM {expected}::jsonb
    OR ({keeper}) IS DISTINCT FROM {expected_freeze}::jsonb
    {ledger_guard}
  THEN RAISE EXCEPTION 'the verified cut must use the attested source and stage locks' USING ERRCODE='55000';
  END IF;
  LOCK TABLE ONLY record_build_submissions IN ACCESS EXCLUSIVE MODE NOWAIT;
  LOCK TABLE ONLY {HOT} IN ACCESS EXCLUSIVE MODE NOWAIT;
  INSERT INTO record_raw_archive_parts(generation,part,first_id,last_id,rows,digest)
    SELECT '{args.generation}'::uuid,part,first_id,last_id,rows,digest FROM pg_temp.frozen_parts ORDER BY part;
  UPDATE state_raw_archive_generation SET sealed_parts=sealed_parts+{part_count},
    sealed_rows=sealed_rows+{cold},last_id={tail} WHERE generation='{args.generation}'::uuid;
  UPDATE state_raw_archive_move SET after_id=coalesce((
    SELECT id FROM ONLY record_build_submissions ORDER BY id COLLATE "C" DESC LIMIT 1
  ),after_id) WHERE id=1;
  SELECT raw_archive_move_cut('{args.generation}'::uuid,'{fence}','{source}') INTO released;
  CREATE TRIGGER archive_frozen_hot_position AFTER DELETE ON {HOT}
  FOR EACH ROW EXECUTE FUNCTION build_inbox_position();
  CREATE TRIGGER archive_frozen_hot_identity BEFORE UPDATE OF id ON {HOT}
  FOR EACH ROW EXECUTE FUNCTION raw_archive_move_identity();
  REVOKE ALL ON {HOT} FROM public;
  ALTER TABLE {HOT} INHERIT record_build_submissions;
  {retire_ledger}
  INSERT INTO pg_temp.frozen_result VALUES(released,NULL);
EXCEPTION WHEN others THEN
  INSERT INTO pg_temp.frozen_result VALUES(NULL,SQLSTATE);
END
$cut$;
SELECT code IS NULL AS frozen_cut_ok FROM pg_temp.frozen_result
\gset
\if :frozen_cut_ok
SELECT bytes FROM pg_temp.frozen_result;
COMMIT;
\else
SELECT 'held:'||code FROM pg_temp.frozen_result;
ROLLBACK TO SAVEPOINT frozen_inbox_cut;
\endif
""")
        script.flush()
        os.fsync(script.fileno())
    return destination


def main():
    os.umask(0o077)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action",choices=("stage-hot","stage-hot-ranges","prepare-cut"))
    parser.add_argument("--work",type=Path,required=True)
    parser.add_argument("--generation",type=uuid.UUID,required=True)
    parser.add_argument("--source-service",required=True)
    parser.add_argument("--schema",default="public")
    parser.add_argument("--oid",type=int,required=True)
    parser.add_argument("--freeze-pid",type=int,required=True)
    parser.add_argument("--cut-pid",type=int)
    parser.add_argument("--tablespace")
    parser.add_argument("--fence-file",type=Path)
    parser.add_argument("--range-list",type=Path)
    parser.add_argument("--legacy-freeze-proof",type=Path)
    parser.add_argument("--retire-hot-copy",action="store_true")
    parser.add_argument("--hot-verify-spool-bytes",type=int)
    args = parser.parse_args()
    if not IDENT.fullmatch(args.schema) or not 0<args.freeze_pid<=2147483647 or (
        args.cut_pid is not None and not 0<args.cut_pid<=2147483647):
        raise ValueError("provide the active source schema and original lock holder")
    with clean_archive_lock(args.work):
        if args.action=="stage-hot-ranges":
            if not args.tablespace or not IDENT.fullmatch(args.tablespace) or args.tablespace=="pg_default":
                raise ValueError("provide the existing data tablespace")
            stage_hot_ranges(args)
            return
        root,plan,control,freeze = verified_manifest(args)
        if args.action=="stage-hot":
            if not args.tablespace or not IDENT.fullmatch(args.tablespace) or args.tablespace=="pg_default":
                raise ValueError("provide the existing data tablespace")
            stage_hot(args,root)
            emit(dict(stage="staged_hot",rows=plan["counts"]["hot"]))
        else:
            if not args.fence_file:
                raise ValueError("provide both intake consumer maintenance receipts")
            if args.cut_pid and args.cut_pid!=args.freeze_pid:
                successor = argparse.Namespace(**vars(args))
                successor.freeze_pid = args.cut_pid
                other_control,next_freeze = current_frame(successor)
                if other_control!=control:
                    raise RuntimeError("cut successor does not hold the same frozen source")
            else:
                next_freeze = freeze
            hot = inspect_hot(args,plan,next_freeze)
            verify_parts(args,root,plan,control)
            if next_freeze!=freeze and current_frame(successor)!=(control,next_freeze):
                raise RuntimeError("cut successor changed during verification")
            path = write_cut(args,root,plan,control,freeze,next_freeze,hot)
            emit(dict(stage="verified_cut_script",path=str(path),counts=plan["counts"],
                      original=freeze,keeper=next_freeze,generation=str(args.generation),
                      parent_oid=args.oid,cluster=control["cluster"],hot=hot))


if __name__=="__main__":
    main()
