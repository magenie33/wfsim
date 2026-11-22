#!/usr/bin/env python3
"""Return the files of the drained, attested frozen inbox child."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import tempfile
import uuid

from raw_inbox_frozen_cut import HOT, IDENT, literal
from raw_inbox_frozen_range import sync_directory
from raw_inbox_rollover import clean_archive_lock, emit, pg


def cut_receipt(path):
    value = json.loads(path.read_bytes())
    hot = value["hot"]
    if (value["stage"]!="verified_cut_script" or str(uuid.UUID(value["generation"]))!=value["generation"]
        or not str(value["cluster"]).isdecimal()
        or any(type(oid) is not int or not 0<oid<2**32 for oid in
               (value["parent_oid"],hot["oid"],hot["file"],hot["tablespace"]))
        or not re.fullmatch(rf'pg_tblspc/{hot["tablespace"]}/PG_[0-9_]+/[0-9]+/{hot["file"]}',hot["path"])):
        raise RuntimeError("invalid identity in the frozen inbox cut receipt")
    return value


def local_cluster(args):
    result = subprocess.check_output([args.pg_controldata,str(args.pgdata)],timeout=3,
                                     env={**os.environ,"LC_ALL":"C"})
    match = re.search(rb'^Database system identifier:\s+([0-9]+)$',result,re.MULTILINE)
    if not match:
        raise RuntimeError("could not identify the local PostgreSQL files")
    return match[1].decode('ascii')


def frame_query(proof):
    hot, parent = proof["hot"], proof["parent_oid"]
    return f"""WITH target AS (
  SELECT c.* FROM pg_class c WHERE c.oid={hot['oid']}::oid AND c.oid=to_regclass('{HOT}')
    AND c.relowner=current_user::regrole AND c.relnamespace=current_schema()::regnamespace
    AND c.relkind='r' AND c.relpersistence='p' AND c.reltablespace={hot['tablespace']}::oid
    AND (SELECT array_agg(inhparent ORDER BY inhseqno) FROM pg_inherits WHERE inhrelid=c.oid)=ARRAY[{parent}::oid]
    AND NOT EXISTS(SELECT FROM pg_inherits WHERE inhparent=c.oid)
    AND NOT EXISTS(SELECT FROM pg_constraint WHERE contype='f' AND confrelid=c.oid)
    AND EXISTS(SELECT FROM pg_index WHERE indrelid=c.oid AND indisprimary AND indisvalid)
    AND (SELECT array_agg(tgname::text ORDER BY tgname) FROM pg_trigger WHERE tgrelid=c.oid)
        =ARRAY['archive_frozen_hot_identity','archive_frozen_hot_position']
    AND EXISTS(SELECT FROM pg_trigger WHERE tgrelid=c.oid AND tgname='archive_frozen_hot_position'
               AND tgenabled='O' AND tgtype=9 AND tgfoid=to_regprocedure('build_inbox_position()'))
    AND EXISTS(SELECT FROM pg_trigger WHERE tgrelid=c.oid AND tgname='archive_frozen_hot_identity'
               AND tgenabled='O' AND tgtype=19 AND tgfoid=to_regprocedure('raw_archive_move_identity()'))
), relations AS (
  SELECT oid FROM target UNION ALL SELECT reltoastrelid FROM target WHERE reltoastrelid<>0
), files AS (
  SELECT oid FROM relations UNION ALL SELECT indexrelid FROM pg_index WHERE indrelid IN(SELECT oid FROM relations)
)
SELECT jsonb_build_object('cluster',(SELECT system_identifier::text FROM pg_control_system()),
  'file',pg_relation_filenode(t.oid)::bigint,'path',pg_relation_filepath(t.oid),'marker',obj_description(t.oid,'pg_class'),
  'has_rows',EXISTS(SELECT FROM ONLY {HOT}),
  'paths',(SELECT jsonb_agg(pg_relation_filepath(oid) ORDER BY oid) FROM files))
FROM target t
WHERE to_regclass('record_build_submissions')={parent}::oid
  AND EXISTS(SELECT FROM state_raw_archive_move m JOIN state_raw_archive_generation g USING(generation)
      WHERE m.id=1 AND m.phase='complete' AND m.source_oid={parent}::oid
        AND g.generation='{proof['generation']}'::uuid AND g.phase IN('consuming','complete'))
  AND EXISTS(SELECT FROM state_build_inbox_storage WHERE id=1 AND adopted AND readiness IS NOT NULL)
  AND EXISTS(SELECT FROM pg_trigger WHERE tgrelid={parent}::oid AND tgenabled='O'
      AND tgfoid=to_regprocedure('build_inbox_legacy_write()'))
  AND to_regprocedure('build_inbox_submit(text,text,text,text,text,text,bigint)') IS NOT NULL
  AND to_regprocedure('board_request_submit(text,text,text,text,text)') IS NOT NULL"""


def session(args, body):
    return pg(f"BEGIN; SET LOCAL search_path={args.schema},pg_catalog,pg_temp; "
              f"SET LOCAL transaction_timeout='{args.statement_seconds}s';\n{body}\nCOMMIT;",
              service=args.source_service,seconds=args.statement_seconds)


def frame(args, query):
    data = session(args,query+";")
    if not data:
        raise RuntimeError("frozen hot identity, dependency, ACK, or writer guard did not match")
    return json.loads(data)


def allocated(args, tablespace, paths):
    census = {}
    root = args.data_mount.resolve(strict=True)
    device = root.stat().st_dev
    for relative in paths:
        path = Path(relative)
        if not re.fullmatch(rf'pg_tblspc/{tablespace}/PG_[0-9_]+/[0-9]+/[0-9]+',relative):
            raise RuntimeError("frozen hot object is outside its pinned tablespace")
        directory = (args.pgdata/path.parent).resolve(strict=True)
        if not directory.is_relative_to(root) or directory.stat().st_dev!=device:
            raise RuntimeError("frozen hot tablespace is outside its pinned data mount")
        census.setdefault(directory,{})[path.name] = relative
    found = {}
    for directory, names in census.items():
        for path in directory.iterdir():
            match = re.fullmatch(r'([0-9]+)(?:_(?:fsm|vm|init))?(?:\.[0-9]+)?',path.name)
            if match and match[1] in names:
                info = path.lstat()
                if not stat.S_ISREG(info.st_mode) or info.st_dev!=device:
                    raise RuntimeError("frozen hot file identity changed")
                relative = str(Path(names[match[1]]).with_name(path.name))
                found[relative] = info.st_blocks*512
    return found


def store(path, value):
    with tempfile.NamedTemporaryFile(mode='w',dir=path.parent,delete=False) as file:
        json.dump(value,file,sort_keys=True,separators=(',',':'))
        file.write('\n')
        file.flush()
        os.fsync(file.fileno())
        temporary = Path(file.name)
    temporary.replace(path)
    sync_directory(path.parent)


def available(path):
    pages = os.statvfs(path)
    return pages.f_bavail*pages.f_frsize


def retire(args, proof):
    cluster = local_cluster(args)
    if cluster!=str(proof["cluster"]):
        raise RuntimeError("cut receipt belongs to another PostgreSQL cluster")
    query = frame_query(proof)
    current = frame(args,query)
    if current["cluster"]!=cluster:
        raise RuntimeError("SQL service is connected to another PostgreSQL cluster")
    hot = proof["hot"]
    path = args.work/f'{proof["generation"]}.hot-retirement.json'
    evidence = json.loads(path.read_bytes()) if path.exists() else None
    if evidence is not None and evidence["cut"]!=proof:
        raise RuntimeError("retirement evidence belongs to a different frozen cut")
    marker = None if evidence is None else evidence["marker"]
    already = marker is not None and current["marker"]==marker
    if not already and (current["file"]!=hot["file"] or current["path"]!=hot["path"] or current["marker"] is not None):
        raise RuntimeError("the frozen hot child no longer has the exact cut identity")
    if current["has_rows"]:
        if already:
            raise RuntimeError("the retired frozen child received a new write")
        return dict(stage="pending",generation=proof["generation"])
    if not already:
        free = available(args.data_mount)
        root = available('/')
        if root<args.root_reserve_bytes:
            return dict(stage="deferred",reason="root_reserve",available_bytes=root)
        if free<args.data_reserve_bytes:
            return dict(stage="deferred",reason="data_reserve",available_bytes=free)
        paths = current["paths"]
        before = allocated(args,hot["tablespace"],paths)
        digest = hashlib.sha256(json.dumps([proof,paths,before],sort_keys=True).encode()).hexdigest()
        marker = 'wfsim-frozen-hot-retired '+digest
        evidence = dict(cut=proof,paths=paths,before=before,marker=marker,data_before=free)
        store(path,evidence)
        expected = literal(json.dumps(current,separators=(',',':')))
        session(args,f"""LOCK TABLE ONLY {HOT} IN ACCESS EXCLUSIVE MODE NOWAIT;
DO $retire$ BEGIN
  IF ({query}) IS DISTINCT FROM {expected}::jsonb THEN
    RAISE EXCEPTION 'the empty frozen hot child changed before retirement' USING ERRCODE='55000';
  END IF;
  TRUNCATE ONLY {HOT};
  COMMENT ON TABLE {HOT} IS {literal(marker)};
END $retire$;""")
        current = frame(args,query)
    if (current["cluster"]!=cluster or current["has_rows"] or current["marker"]!=marker
        or current["file"]==hot["file"] or set(current["paths"])&set(evidence["paths"])):
        raise RuntimeError("independent readback did not confirm frozen hot retirement")
    remaining = allocated(args,hot["tablespace"],evidence["paths"])
    if "result" in evidence and not remaining:
        return {**evidence["result"],"replay":True}
    free = available(args.data_mount)
    prior = sum(evidence["before"].values())
    result = dict(stage="awaiting_unlink" if remaining else "retired",generation=proof["generation"],
                  old_allocated_bytes=prior,old_remaining_bytes=sum(remaining.values()),
                  old_files_remaining=len(remaining),data_before=evidence["data_before"],data_after=free,
                  old_files=len(evidence["before"]),old_filenode=hot["file"],new_filenode=current["file"])
    if not remaining:
        store(path,{**evidence,"result":result})
    return result


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('--cut-receipt',type=Path,required=True)
    cli.add_argument('--work',type=Path,required=True)
    cli.add_argument('--pgdata',type=Path,required=True)
    cli.add_argument('--data-mount',type=Path,required=True)
    cli.add_argument('--source-service',required=True)
    cli.add_argument('--data-reserve-bytes',type=int,required=True)
    cli.add_argument('--root-reserve-bytes',type=int,required=True)
    cli.add_argument('--schema',default='public')
    cli.add_argument('--statement-seconds',type=int,default=4)
    cli.add_argument('--pg-controldata',default='pg_controldata')
    args = cli.parse_args()
    if (not IDENT.fullmatch(args.schema) or not 1<=args.statement_seconds<=10 or min(args.data_reserve_bytes,args.root_reserve_bytes)<1
        or not args.work.is_dir()):
        raise ValueError("invalid schema, work directory, or frozen hot retirement budgets")
    with clean_archive_lock(args.work):
        emit(retire(args,cut_receipt(args.cut_receipt)))


if __name__=='__main__':
    main()
