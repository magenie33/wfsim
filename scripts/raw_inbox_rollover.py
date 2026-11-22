#!/usr/bin/env python3
"""Back and drain bounded raw inbox pages through the private R2 archive."""
import argparse
import gzip
import hashlib
import io
import json
import os
import re
import selectors
import subprocess
import tempfile
import time
import uuid
from contextlib import contextmanager
from pathlib import Path

SOURCE = Path(__file__).resolve().parents[1]
PREFIX = 'r2:wfsim-pg-backup/raw-inbox-rollover-v1'
MAX_BODY = 8392704
RCLONE_FLAGS = ['--s3-no-check-bucket', '--contimeout', '8s', '--timeout', '25s', '--retries', '1']
PSQL = ['psql', '-XqAt', '-v', 'ON_ERROR_STOP=1', '-v', 'VERBOSITY=verbose']


def run(command, *, data=None, env=None, timeout=45, pass_fds=()):
    child = subprocess.run(command, input=data, env=env, capture_output=True, timeout=timeout, pass_fds=pass_fds)
    if child.returncode:
        # SQL statements and object contents can include private submission text.
        code = re.search(rb'(?:ERROR|FATAL):\s+([0-9A-Z]{5}):',child.stderr)
        sqlstate = '' if code is None else ' SQLSTATE '+code[1].decode('ascii')
        raise RuntimeError(f'{Path(command[0]).name} exited {child.returncode}{sqlstate}; reconcile before repeating a write')
    return child.stdout


def pg_environment(service):
    environment = os.environ.copy()
    if service:
        environment['PGSERVICE'] = service
        for key in ('PGHOST', 'PGHOSTADDR', 'PGPORT', 'PGDATABASE', 'PGUSER', 'PGPASSWORD', 'PGOPTIONS'):
            environment.pop(key, None)
    return environment


def pg_options(seconds):
    return f"SET statement_timeout='{seconds}s'; SET lock_timeout='500ms'; SET plan_cache_mode='force_custom_plan';\n"


def pg(statement, *, service=None, seconds=12):
    output = run(PSQL, data=(pg_options(seconds)+statement).encode(),
                 env=pg_environment(service), timeout=seconds+7)
    return output.rstrip(b'\n')


class RestoreConnection:
    """One independent psql connection with one fully rolled-back transaction per package."""

    def __init__(self, service):
        self.service = service
        self.child = None

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()

    def close(self):
        if self.child is None:
            return
        child, self.child = self.child, None
        child.stdin.close()
        child.stdout.close()
        try:
            child.wait(timeout=1)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait(timeout=5)

    def query(self, statement, *, service):
        if service != self.service:
            raise ValueError('restore service changed within the window')
        first = self.child is None
        if first:
            self.child = subprocess.Popen([*PSQL, '--no-password'], env=pg_environment(service), bufsize=0,
                                          stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        marker = uuid.uuid4().hex.encode('ascii') + b'\n'
        sql = (pg_options(12) if first else '').encode() + statement.encode() + b'\\echo ' + marker
        end = time.monotonic()+19
        child = self.child
        result = bytearray()
        try:
            os.set_blocking(child.stdin.fileno(), False)
            os.set_blocking(child.stdout.fileno(), False)
            with selectors.DefaultSelector() as ready:
                ready.register(child.stdin, selectors.EVENT_WRITE)
                ready.register(child.stdout, selectors.EVENT_READ)
                pending = memoryview(sql)
                while time.monotonic()<end:
                    for key, event in ready.select(max(0,end-time.monotonic())):
                        if event==selectors.EVENT_WRITE:
                            pending = pending[os.write(key.fd,pending):]
                            if not pending:
                                ready.unregister(child.stdin)
                        else:
                            received = os.read(key.fd,65536)
                            if not received:
                                raise RuntimeError('independent restore closed before rollback confirmation')
                            result.extend(received)
                            if len(result)>MAX_BODY+65536:
                                raise RuntimeError('independent restore exceeded its readback bound')
                            if not pending and result.endswith(b'\n'+marker):
                                return bytes(result[:-len(marker)-1])
            raise TimeoutError('independent restore did not confirm rollback in time')
        except BaseException:
            self.close()
            raise


def literal_json(body):
    return "convert_from(decode('"+body.hex()+"','hex'),'UTF8')::jsonb"


def sha(body):
    return hashlib.sha256(body).hexdigest()


def emit(body):
    print(json.dumps(body, separators=(',', ':')), flush=True)


def json_pg(statement):
    return json.loads(pg(statement))


def object_key(generation, part, digest):
    generation = str(uuid.UUID(generation))
    if not isinstance(part, int) or part < 1 or not re.fullmatch('[0-9a-f]{64}', digest):
        raise ValueError('invalid raw inbox archive object identifier')
    return f'{PREFIX}/{generation}/{part}-{digest}.json.gz'


def get(key, digest):
    child = subprocess.Popen(['rclone', 'cat', key, *RCLONE_FLAGS], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    try:
        packed = child.stdout.read(MAX_BODY+65537)
        if len(packed)>MAX_BODY+65536 or child.wait(timeout=30):
            raise RuntimeError('could not read a bounded archive object')
    finally:
        if child.poll() is None:
            child.kill()
            child.wait(timeout=5)
        child.stdout.close()
    with gzip.GzipFile(fileobj=io.BytesIO(packed)) as stream:
        plain = stream.read(MAX_BODY+1)
    if len(plain)>MAX_BODY or sha(plain)!=digest:
        raise RuntimeError('remote archive bytes do not match the verified digest')
    return plain


def put(key, plain, work, *, pass_fds=()):
    with tempfile.TemporaryDirectory(prefix='raw-archive-', dir=work) as temporary:
        outgoing = Path(temporary)/'part.json.gz'
        outgoing.write_bytes(gzip.compress(plain, compresslevel=3, mtime=0))
        run(['rclone', 'copyto', str(outgoing), key, '--immutable', '--no-traverse', *RCLONE_FLAGS],pass_fds=pass_fds)


def prove_restore(body, restore_service, live_identity, *, execute=None):
    if not restore_service or restore_service==os.environ.get('PGSERVICE'):
        raise ValueError('use a separate PostgreSQL restore service')
    # Compare the initialized cluster identity in the same restore connection.
    # Running this check requires the restore service's own disposable cluster.
    header = f"""BEGIN;
SELECT 1/((system_identifier::text<>'{live_identity}')::int) FROM pg_control_system();
CREATE TEMP TABLE restored (
  id text COLLATE "C" NOT NULL PRIMARY KEY, at text COLLATE "C" NOT NULL, record text COLLATE "C" NOT NULL,
  initiator text COLLATE "C", family_source text COLLATE "C", observation text COLLATE "C",
  ordinal bigint NOT NULL UNIQUE, position bigint, portable boolean NOT NULL
);
WITH parts AS (
  SELECT item,ordinal FROM jsonb_array_elements(({literal_json(body)})->'rows') WITH ORDINALITY parts(item,ordinal)
)
INSERT INTO restored
SELECT x.id,x.at,x.record,x.initiator,x.family_source,x.observation,p.ordinal,
  (p.item->>'position')::bigint,(p.item->>'portable')::boolean
FROM parts p CROSS JOIN LATERAL jsonb_to_record(p.item->'input')
  AS x(id text,at text,record text,initiator text,family_source text,observation text);
SELECT jsonb_agg(jsonb_build_object('input',to_jsonb(r)-ARRAY['ordinal','position','portable'],
  'position',r.position::text,'portable',r.portable) ORDER BY r.ordinal) FROM restored r;
ROLLBACK;
"""
    lines = (execute or pg)(header, service=restore_service).splitlines()
    if len(lines)!=2 or lines[0]!=b'1' or json.loads(lines[1])!=json.loads(body)['rows']:
        raise RuntimeError('remote packet did not restore every captured field and queue position')


def control():
    return json_pg("""SELECT jsonb_build_object('phase',r.phase,'generation',r.generation,'source_oid',r.source_oid,
      'after',c.after_id,'next_part',c.next_part,'cut_at',c.cut_at,'completed_at',c.completed_at,'aborted_at',c.aborted_at)
      FROM state_raw_inbox_rollover r LEFT JOIN state_raw_inbox_archive_cursor c USING(generation);""")


def disk_gate(work, root_reserve, work_reserve):
    def mount(path):
        info = os.statvfs(path)
        return info.f_bavail*info.f_frsize
    if mount('/')<root_reserve or mount(work)<work_reserve:
        raise RuntimeError('archive paused at filesystem reserve')


def copy_page(args, live_identity):
    disk_gate(args.work, args.root_reserve, args.work_reserve)
    body = pg(f'SELECT raw_inbox_archive_page({args.rows},{args.bytes})::text;')
    packet = json.loads(body)
    if not packet['rows']:
        return False
    if len(body)>MAX_BODY:
        raise RuntimeError('archive page exceeded the wire limit')
    digest = sha(body)
    key = object_key(packet['generation'], packet['part'], digest)
    put(key, body, args.work)
    restored = get(key, digest)
    prove_restore(restored, args.restore_service, live_identity)
    disk_gate(args.work, args.root_reserve, args.work_reserve)
    response = pg(f"SELECT raw_inbox_archive_accept({literal_json(restored)},'{digest}');")
    if int(response)!=packet['part']:
        raise RuntimeError('archive acceptance returned a different part')
    emit({'stage':'copied','generation':packet['generation'],'part':packet['part'],'rows':len(packet['rows']), 'sha256':digest})
    return True


def drain_page(args):
    disk_gate(args.work, args.root_reserve, args.work_reserve)
    index = json_pg("""SELECT coalesce((
      SELECT jsonb_build_object('generation',c.generation,'part',c.part,'digest',c.digest)
      FROM state_raw_inbox_rollover live JOIN record_raw_inbox_archive_chunks c USING(generation)
      WHERE live.phase='draining' AND c.loaded_at IS NULL ORDER BY c.part LIMIT 1),'null'::jsonb);""")
    if index is None:
        return False
    key = object_key(index['generation'], index['part'], index['digest'])
    body = get(key,index['digest'])
    disk_gate(args.work, args.root_reserve+args.part_peak_bytes,args.work_reserve)
    queued = int(pg('SELECT count(*) FROM (SELECT 1 FROM record_build_submissions '
                    f'LIMIT {args.max_hot_rows+1}) live;'))
    new_inputs = sum(entry['portable'] for entry in json.loads(body)['rows'])
    if queued+new_inputs>args.max_hot_rows:
        raise RuntimeError('archive recovery paused until intake drains the hot backlog')
    loaded = json_pg(f'SELECT raw_inbox_archive_load({literal_json(body)});')
    emit({'stage':'drained',**index,**loaded})
    return True


def consume_sealed_page(args):
    disk_gate(args.work, args.root_reserve+args.peak_root_bytes, args.work_reserve)
    index = json_pg("SELECT coalesce(raw_archive_next(),'null'::jsonb);")
    if index is None:
        return
    part = int(index['part'])
    key = object_key(index['generation'],part,index['digest'])
    body = get(key,index['digest'])
    packet = json.loads(body)
    if packet.get('generation')!=index['generation'] or packet.get('part')!=part:
        raise RuntimeError('raw archive object belongs to another part')
    disk_gate(args.work, args.root_reserve+args.peak_root_bytes, args.work_reserve)
    environment = {**os.environ,'TMPDIR':str(args.work.resolve())}

    def reserve_blocks(path, reserve):
        size = os.statvfs(path).f_frsize
        return str((reserve+size-1)//size)

    result = run(['node',str(SOURCE/'scripts/raw_inbox_archive_consume.mjs'),
                  str(args.native.resolve()),index['digest'],str(args.statement_ms),environment['TMPDIR'],
                  reserve_blocks('/',args.root_reserve+args.peak_root_bytes),
                  reserve_blocks(args.work,args.work_reserve)],data=body,env=environment)
    receipt = json.loads(result)
    stage = 'consumed' if receipt['committed'] else 'partial'
    if receipt.get('blocked'):
        stage = 'blocked'
    if receipt.get('paused'):
        stage = 'paused'
    emit({'stage':stage,**index,**receipt})
    if stage=='blocked':
        raise RuntimeError('a single pending archive input exceeds its intake limits')
    if stage=='paused':
        raise RuntimeError('archive paused at filesystem reserve')


def retire_clean_page(generation):
    index = json_pg(f"SELECT coalesce(raw_archive_retire_next('{generation}'::uuid),'null'::jsonb);")
    if index is None:
        pg('SELECT raw_archive_trim_skips();')
        return
    part, digest = int(index['part']), index['digest']
    key = object_key(str(generation),part,digest)

    def object_present():
        stat = json.loads(run(['rclone','lsjson',key,'--stat',*RCLONE_FLAGS]))
        if isinstance(stat,dict):
            if stat.get('IsDir') is True and stat.get('Size')==-1:
                return False
            if stat.get('IsDir') is False and type(stat.get('Size')) is int and stat['Size']>=0:
                return True
        raise RuntimeError('unrecognized archive object state')

    if object_present():
        run(['rclone','deletefile',key,*RCLONE_FLAGS])
    if object_present():
        raise RuntimeError('archive object remains after retirement')
    pg(f"SELECT raw_archive_retired('{generation}'::uuid,{part},'{digest}');")
    emit({'stage':'retired','generation':str(generation),'part':part,'digest':digest})


def install_clean(args):
    names = (args.tablespace,args.http_role,args.consume_role)
    if any(not re.fullmatch('[a-z_][a-z0-9_]{0,62}', name) for name in names):
        raise ValueError('use an existing data tablespace and explicit database roles')
    archive, move = [(SOURCE/'worker/pg'/name).read_text() for name in
                     ('raw_inbox_archive.sql','raw_inbox_archive_move.sql')]
    pg(f"""BEGIN;
SET LOCAL default_tablespace='{args.tablespace}';
SELECT set_config('search_path',quote_ident(current_schema())||',pg_catalog,pg_temp',true);
LOCK TABLE ONLY record_build_submissions IN ACCESS EXCLUSIVE MODE NOWAIT;
DO $$ BEGIN
  IF to_regclass('state_raw_inbox_rollover') IS NOT NULL
    OR (SELECT pg_get_userbyid(relowner) FROM pg_class WHERE oid='record_build_submissions'::regclass)<>current_user
    THEN RAISE EXCEPTION 'install the clean archive under the detached source owner'; END IF;
  IF to_regclass('state_raw_archive_move') IS NOT NULL THEN
    IF EXISTS(SELECT FROM state_raw_archive_move WHERE phase<>'idle') THEN
      RAISE EXCEPTION 'upgrade the clean archive when its move is idle';
    END IF;
  END IF;
END $$;
{archive}
{move}
GRANT EXECUTE ON FUNCTION raw_inbox_library_count() TO "{args.http_role}";
GRANT EXECUTE ON FUNCTION raw_archive_next(),raw_archive_pending(uuid,bigint,text),
  raw_archive_consume(uuid,bigint,text,text[],text),
  raw_archive_narrow_slice(uuid,bigint,text,integer,integer),
  raw_archive_consume_slice(uuid,bigint,text,integer,integer,text[],text) TO "{args.consume_role}";
COMMIT;
""")


def widen_precopy(args):
    move = (SOURCE/'worker/pg/raw_inbox_archive_move.sql').read_text()
    start = move.index('CREATE OR REPLACE FUNCTION raw_archive_move_page(')
    end = move.index('CREATE OR REPLACE FUNCTION raw_archive_move_prepare(',start)
    page = move[start:end]
    pg(f"""BEGIN;
SELECT set_config('search_path',quote_ident(current_schema())||',pg_catalog,pg_temp',true);
LOCK TABLE state_raw_archive_move, state_raw_archive_generation, record_raw_archive_parts
  IN ACCESS EXCLUSIVE MODE NOWAIT;
DO $$ BEGIN
  IF NOT EXISTS (SELECT FROM state_raw_archive_move m JOIN state_raw_archive_generation g USING(generation)
    WHERE m.id=1 AND m.phase='copying' AND g.phase='sealing' AND m.online_source IS NOT NULL
      AND m.upload_digest IS NULL AND m.source_oid='record_build_submissions'::regclass::oid)
    OR EXISTS (SELECT FROM record_raw_archive_uploads u JOIN state_raw_archive_move m USING(generation))
  THEN RAISE EXCEPTION 'wide archive upgrade requires the current online move between resolved pages'
    USING ERRCODE='55000'; END IF;
END $$;
ALTER TABLE record_raw_archive_parts DROP CONSTRAINT record_raw_archive_parts_rows_check,
  ADD CONSTRAINT raw_archive_parts_rows_wide CHECK(rows BETWEEN 1 AND 2048) NOT VALID;
ALTER TABLE state_raw_archive_generation DROP CONSTRAINT state_raw_archive_generation_next_offset_check,
  ADD CONSTRAINT raw_archive_generation_offset_wide CHECK(next_offset BETWEEN 0 AND 2047) NOT VALID;
{page}
COMMIT;
""")


@contextmanager
def clean_archive_lock(work):
    import fcntl
    with (work/'raw-inbox-archive.lock').open('ab') as lock:
        try:
            fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        except BlockingIOError:
            raise RuntimeError('another raw archive transfer still owns the work directory') from None
        yield lock.fileno()


def copy_clean_page(args, live_identity, lease, restore=None):
    disk_gate(args.work,args.root_reserve,args.work_reserve)
    body = pg(f"SELECT coalesce(raw_archive_move_prepare({args.rows},{args.bytes}),'null');")
    packet = json.loads(body)
    if packet is None:
        return False
    if len(body)>MAX_BODY:
        raise RuntimeError('raw archive page exceeds its transfer bound')
    digest = sha(body)
    if packet['rows']:
        key = object_key(packet['generation'],packet['part'],digest)
        try:
            put(key,body,args.work,pass_fds=(lease,))
        except (RuntimeError,subprocess.TimeoutExpired):
            # A fresh readback also resolves a lost upload reply for this exact key.
            pass
        body = get(key,digest)
        prove_restore(body,args.restore_service,live_identity,execute=restore.query if restore else None)
    disk_gate(args.work,args.root_reserve,args.work_reserve)
    literal_text = f"convert_from(decode('{body.hex()}','hex'),'UTF8')"
    accepted = json_pg(f"SELECT raw_archive_move_accept({literal_text},'{digest}',{args.rows},{args.bytes});")
    emit({'stage':'sealed',**accepted})
    return True


def clean_receipt(path, generation):
    with path.open('rb') as source:
        body = source.read(65537)
    if len(body)>65536:
        raise ValueError('raw archive receipt is too large')
    receipt = json.loads(body)
    if (not isinstance(receipt,dict) or receipt.get('v')!=1 or receipt.get('generation')!=str(generation)
            or not re.fullmatch('[0-9a-f]{40}',str(receipt.get('source')))):
        raise ValueError('raw archive receipt does not identify the deployed source and generation')
    return sha(body), receipt


def clean_fence(path, generation):
    digest, receipt = clean_receipt(path,generation)
    for consumer in ('live','hourly'):
        state = receipt.get(consumer)
        if (not isinstance(state,dict) or state.get('paused') is not True or state.get('inflight')!=0
                or state.get('capture') not in ('acknowledged','discarded')
                or not re.fullmatch('[0-9a-f]{64}',str(state.get('receipt')))):
            raise ValueError('both raw intake consumers and their capture receipts must be fenced')
    return digest, receipt['source']


def writer_fence(path, generation):
    digest, receipt = clean_receipt(path,generation)
    writers = receipt.get('writers')
    if (receipt.get('legacy_raw_writers') is not False or not isinstance(writers,dict)
            or any(not re.fullmatch('[0-9a-f]{64}',str(writers.get(role)))
                   for role in ('wfsim','factoryd','pipeline'))):
        raise ValueError('verify the deployed writers and the end of legacy raw writes')
    return digest, receipt['source']


def clean_action(args, lease=None):
    if args.action=='install-clean':
        install_clean(args)
    elif args.action=='widen-precopy-clean':
        widen_precopy(args)
    elif args.action=='begin-precopy-clean':
        digest, source = writer_fence(args.writer_file,args.generation)
        pg(f"SELECT raw_archive_move_begin_online('{args.generation}'::uuid,'{source}','{digest}');")
    elif args.action=='status-clean':
        emit(json_pg("""SELECT jsonb_build_object('phase',m.phase,'generation',m.generation,'after',m.after_id,
          'source_oid',m.source_oid,'online_source',m.online_source,'writer_receipt',m.writer_receipt,
          'sealed_parts',g.sealed_parts,'sealed_rows',g.sealed_rows,
          'next_part',g.next_part,'next_offset',g.next_offset,'next_slice_rows',g.next_slice_rows,
          'consumed_rows',g.consumed_rows,'retired_part',g.retired_part,
          'source_bytes',pg_total_relation_size('record_build_submissions'),
          'hot_bytes',pg_total_relation_size('record_raw_archive_hot'),
          'arrival_bytes',pg_total_relation_size('record_raw_archive_arrivals'),
          'skip_bytes',pg_total_relation_size('record_raw_archive_skips'))
          FROM state_raw_archive_move m LEFT JOIN state_raw_archive_generation g USING(generation);"""))
    elif args.action=='retire-clean':
        retire_clean_page(args.generation)
    elif args.action=='copy-clean':
        if min(args.parts,args.seconds,args.root_reserve,args.work_reserve)<1 or not args.work.is_dir():
            raise ValueError('specify a bounded archive window and both disk reserves')
        identity = pg('SELECT system_identifier::text FROM pg_control_system();').decode()
        if not identity.isdecimal():
            raise RuntimeError('could not identify the source cluster')
        until = time.monotonic()+args.seconds
        with RestoreConnection(args.restore_service) as restore:
            for _ in range(args.parts):
                if time.monotonic()>=until or not copy_clean_page(
                        args,identity,lease,restore if args.reuse_restore_connection else None):
                    break
    else:
        mode = {'begin-clean':'begin','cut-clean':'cut','abort-clean':'abort'}[args.action]
        parameters = f"'{args.generation}'::uuid"
        if mode!='abort':
            digest, source = clean_fence(args.fence_file,args.generation)
            parameters += f",'{digest}'"
        if mode=='cut':
            if min(args.root_reserve,args.work_reserve,args.peak_root_bytes)<1 or not args.work.is_dir():
                raise ValueError('specify the approved cut reserves and root overhead')
            disk_gate(args.work,args.root_reserve+args.peak_root_bytes,args.work_reserve)
            parameters += f",'{source}'"
        value = pg(f'SELECT raw_archive_move_{mode}({parameters});')
        if value:
            emit({'stage':'cut','old_source_bytes':int(value)})


def install(tablespace):
    if not re.fullmatch('[a-z_][a-z0-9_]{0,62}', tablespace):
        raise ValueError('invalid PostgreSQL tablespace')
    module = (SOURCE/'worker/pg/raw_inbox_rollover.sql').read_text()
    fingerprints = {}
    for filename, signatures in {
        'build_workflows.sql': ['build_ack_submissions(text)','build_inbox_position()','build_family_capture_output()'],
        'board_requested_submit.sql': ['board_request_capture_review()'],
    }.items():
        code = (SOURCE/'worker/pg'/filename).read_text()
        for signature in signatures:
            body = re.search(r'CREATE OR REPLACE FUNCTION '+re.escape(signature)+r' RETURNS [^$]+AS \$\$(.*?)\$\$;', code, re.S)
            if body is None:
                raise RuntimeError('required raw inbox source signature changed')
            fingerprints[signature] = sha(body[1].encode())
    pg(f"""BEGIN;
SET LOCAL default_tablespace='{tablespace}';
SELECT set_config('search_path',quote_ident(current_schema())||',pg_catalog,pg_temp',true);
DO $$ BEGIN
  IF (SELECT pg_get_userbyid(relowner) FROM pg_class WHERE oid='record_build_submissions'::regclass)<>current_user
    OR (SELECT pg_get_userbyid(proowner) FROM pg_proc WHERE oid='build_ack_submissions(text)'::regprocedure)<>current_user
    OR NOT (SELECT prosrc LIKE '%PERFORM raw_inbox_rollover_ack(%'
            FROM pg_proc WHERE oid='build_ack_submissions(text)'::regprocedure) THEN
    RAISE EXCEPTION 'install the exact rollover-aware intake under the canonical storage owner'; END IF;
END $$;
{module}
UPDATE state_raw_inbox_rollover SET source_fingerprint={literal_json(json.dumps(fingerprints).encode())} WHERE id=1;
SELECT raw_inbox_rollover_shape(false);
COMMIT;
""")


def read_arguments():
    base = argparse.ArgumentParser(description=__doc__)
    choices = base.add_subparsers(required=True,dest='action')
    choices.add_parser('status')
    setup = choices.add_parser('install')
    setup.add_argument('--tablespace',required=True)
    for name in ('begin','cut','abort','finish'):
        cmd = choices.add_parser(name)
        cmd.add_argument('generation',type=uuid.UUID)
        if name=='cut':
            cmd.add_argument('--hot-rows',type=int,required=True)
            cmd.add_argument('--hot-bytes',type=int,required=True)
            cmd.add_argument('--peak-root-bytes',type=int,required=True)
            cmd.add_argument('--seconds',type=int,required=True)
            disk_arguments(cmd)
    for name in ('copy','drain'):
        cmd = choices.add_parser(name)
        disk_arguments(cmd)
        cmd.add_argument('--parts',type=int,required=True)
        cmd.add_argument('--seconds',type=int,required=True)
        if name=='copy':
            cmd.add_argument('--restore-service',required=True)
            cmd.add_argument('--rows',type=int,default=512)
            cmd.add_argument('--bytes',type=int,default=1048576)
        else:
            cmd.add_argument('--max-hot-rows',type=int,required=True)
            cmd.add_argument('--part-peak-bytes',type=int,required=True)
    consume = choices.add_parser('consume')
    disk_arguments(consume)
    consume.add_argument('--native',type=Path,required=True)
    consume.add_argument('--peak-root-bytes',type=int,required=True)
    consume.add_argument('--statement-ms',type=int,required=True)
    clean = choices.add_parser('install-clean')
    clean.add_argument('--tablespace',required=True)
    clean.add_argument('--http-role',required=True)
    clean.add_argument('--consume-role',required=True)
    precopy = choices.add_parser('begin-precopy-clean')
    precopy.add_argument('generation',type=uuid.UUID)
    precopy.add_argument('--writer-file',type=Path,required=True)
    wide = choices.add_parser('widen-precopy-clean')
    wide.add_argument('--work',type=Path,required=True)
    for name in ('begin-clean','cut-clean','abort-clean'):
        cmd = choices.add_parser(name)
        cmd.add_argument('generation',type=uuid.UUID)
        if name!='abort-clean':
            cmd.add_argument('--fence-file',type=Path,required=True)
        if name=='cut-clean':
            disk_arguments(cmd)
            cmd.add_argument('--peak-root-bytes',type=int,required=True)
        if name=='abort-clean':
            cmd.add_argument('--work',type=Path,required=True)
    clean = choices.add_parser('copy-clean')
    disk_arguments(clean)
    clean.add_argument('--restore-service',required=True)
    clean.add_argument('--reuse-restore-connection',action='store_true')
    clean.add_argument('--parts',type=int,required=True)
    clean.add_argument('--seconds',type=int,required=True)
    clean.add_argument('--rows',type=int,default=512)
    clean.add_argument('--bytes',type=int,default=1048576)
    clean = choices.add_parser('retire-clean')
    clean.add_argument('generation',type=uuid.UUID)
    clean.add_argument('--work',type=Path,required=True)
    choices.add_parser('status-clean')
    return base.parse_args()


def disk_arguments(cmd):
    cmd.add_argument('--work',type=Path,required=True)
    cmd.add_argument('--root-reserve',type=int,required=True)
    cmd.add_argument('--work-reserve',type=int,required=True)


def main():
    os.umask(0o077)
    args = read_arguments()
    if args.action=='install':
        install(args.tablespace)
    elif args.action in ('copy-clean','cut-clean','abort-clean','retire-clean','widen-precopy-clean'):
        with clean_archive_lock(args.work) as lease:
            clean_action(args,lease)
        return
    elif args.action.endswith('-clean'):
        clean_action(args)
        return
    elif args.action=='consume':
        if (min(args.root_reserve,args.work_reserve,args.peak_root_bytes)<1 or not args.work.is_dir()
            or not args.native.is_file() or not 1<=args.statement_ms<=12000 or not os.environ.get('PG_URL')):
            raise ValueError('specify the approved one-part archive budgets, Native binary and database URL')
        consume_sealed_page(args)
        return
    elif args.action in ('copy','drain'):
        if min(args.parts,args.seconds,args.root_reserve,args.work_reserve)<1 or not args.work.is_dir():
            raise ValueError('specify a bounded archive window and an existing work directory')
        expected = 'copying' if args.action=='copy' else 'draining'
        if control()['phase']!=expected:
            raise RuntimeError(f'raw inbox archive is not {expected}')
        if args.action=='drain' and min(args.max_hot_rows,args.part_peak_bytes)<1:
            raise ValueError('specify the approved hot inbox backlog and peak recovery budgets')
        if args.action=='copy':
            identity = pg('SELECT system_identifier::text FROM pg_control_system();').decode()
            if not identity.isdecimal():
                raise RuntimeError('could not identify the source cluster')
            def step():
                return copy_page(args,identity)
        else:
            def step():
                return drain_page(args)
        until = time.monotonic()+args.seconds
        for _ in range(args.parts):
            if time.monotonic()>=until or not step():
                break
    elif args.action!='status':
        generation = str(args.generation)
        state = control()
        if args.action=='cut' and state['generation']==generation and state['cut_at'] is not None:
            emit({'stage':'already_cut',**state})
            return
        parameters = f"'{generation}'::uuid"
        if args.action=='cut':
            if (min(args.root_reserve,args.work_reserve,args.peak_root_bytes,args.seconds)<1
                or args.peak_root_bytes<args.hot_bytes or not args.work.is_dir()):
                raise ValueError('specify an approved cut window and both peak and reserve budgets')
            disk_gate(args.work,args.root_reserve+args.peak_root_bytes,args.work_reserve)
            parameters += f',{args.hot_rows},{args.hot_bytes}'
        result = pg(f'SELECT raw_inbox_rollover_{args.action}({parameters});',
                    seconds=args.seconds if args.action=='cut' else 12)
        if result:
            emit(json.loads(result))
    emit(control())


if __name__=='__main__':
    main()
