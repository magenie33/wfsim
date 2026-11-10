// The actual Bash consumer drains native publication rounds between inventory updates.
import assert from 'node:assert/strict';
import { execFile, execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

const binary = resolve(process.env.WFSIM_INVENTORY_BOARD_BIN || 'target/debug/wfsim-board');
assert(existsSync(binary), 'build wfsim-board before checking its Bash publication consumer');
const source = readFileSync(new URL('./live_board.sh', import.meta.url), 'utf8');
const functions = ['ask', 'publish_incremental', 'consume_inventory_status', 'start_push', 'stop_push', 'start_ranker', 'stop_ranker'].map(name => {
  const match = source.match(new RegExp('^'+name+'\\(\\) \\{[\\s\\S]*?^\\}', 'm'));
  assert(match, `${name} must be the production shell function`);
  return match[0];
}).join('\n');
const dir = mkdtempSync(join(tmpdir(), 'wfsim-live-ranker-'));
let server;
try {
  const records = JSON.parse(readFileSync(new URL('../tests/gas/rows.json', import.meta.url))).slice(0, 3).map(row => row.record);
  const canonical = execFileSync(binary, ['--inventory-builds'], { input: JSON.stringify(records), encoding: 'utf8', timeout: 30000 })
    .trim().split('\n').map(line => JSON.parse(line));
  assert.equal(canonical.length, 3);
  const rulers = execFileSync(binary, ['--rulers'], { encoding: 'utf8', timeout: 30000 }).trim().split(/\s+/);
  const facts = (rows, salt) => rows.flatMap((record, i) => rulers.map((ruler, r) => JSON.stringify({
    identity: record.id, ruler, mode: 'base', score: 100+i+r+salt,
    measured_by: 'verified:fixture', finished_at: '2026-11-07T00:00:00.000Z',
  }))).join('\n')+'\n';
  writeFileSync(join(dir, 'library.json'), '[]');
  writeFileSync(join(dir, 'builds.ndjson'), canonical.map(row => JSON.stringify(row)).join('\n')+'\n');
  writeFileSync(join(dir, 'facts.ndjson'), facts(canonical, 0));
  writeFileSync(join(dir, 'hot.ndjson'), facts(canonical.slice(0, 1), 1000));
  writeFileSync(join(dir, 'state.yaml'), 'boards: {}\n');
  writeFileSync(join(dir, 'blocked.json'), JSON.stringify({ blocked: true, stage: 'needs_restore', pending: true, engine_epoch: 'unsafe' }));
  const initializer = source.match(/^INCREMENTAL_INVENTORY=.*\ncase "\$INCREMENTAL_INVENTORY".*$/m)?.[0];
  assert(initializer, 'the check must execute the production inventory mode initializer');
  const scripts = fileURLToPath(new URL('.', import.meta.url)), bash = process.env.WFSIM_BASH || 'bash';
  for (const [mode, expected] of [['unset', '1'], ['0', '0']]) {
    const work = join(dir, 'mode-' + mode); mkdirSync(work);
    writeFileSync(join(work, 'library.json'), JSON.stringify(records));
    writeFileSync(join(work, 'new-builds.ndjson'), readFileSync(join(dir, 'builds.ndjson')));
    writeFileSync(join(work, 'facts-known.ndjson'), readFileSync(join(dir, 'facts.ndjson')));
    const modeScript = `set -euo pipefail
${mode === 'unset' ? 'unset INCREMENTAL_INVENTORY' : 'INCREMENTAL_INVENTORY=0'}
${initializer}
${functions}
WORK="$WFSIM_FIXTURE"
BIN="$WFSIM_FIXTURE_BIN"
cd "$WORK"
trap 'stop_ranker' EXIT
start_ranker
[ "$INCREMENTAL_INVENTORY" = "$WFSIM_EXPECTED_MODE" ]
`;
    execFileSync(bash, ['-c', modeScript], { encoding: 'utf8', timeout: 30000,
      env: { ...process.env, WFSIM_FIXTURE: work, WFSIM_FIXTURE_BIN: dirname(binary), WFSIM_EXPECTED_MODE: expected } });
    assert.equal(existsSync(join(work, 'inventory/empty-library.json')), expected === '1');
    assert.match(readFileSync(join(work, 'project.log'), 'utf8'), new RegExp('library: ' + (expected === '1' ? '0' : '[1-9][0-9]*') + ' record'));
  }
  console.log('ok: actual Bash/native startup defaults to guarded empty inputs; explicit zero selects the legacy library');
  mkdirSync(join(dir, 'verified'));
  for (const name of ['a.json', 'b.json', 'meta.json']) writeFileSync(join(dir, 'verified', name), JSON.stringify({ name }));
  const requests = [];
  server = createServer((request, response) => {
    requests.push(request.url); request.resume();
    request.on('end', () => writeFileSync(join(dir, 'push-started'), 'in-flight'));
    response.on('error', () => {});
  });
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  const script = `set -euo pipefail
coproc RANKER { exec "$WFSIM_RANKER" --serve "$WFSIM_FIXTURE/board" --library "$WFSIM_FIXTURE/library.json" --state "$WFSIM_FIXTURE/state.yaml"; }
trap 'stop_push; if [ -n "\u0024{RANKER_PID:-}" ]; then kill "$RANKER_PID" 2>/dev/null || true; wait "$RANKER_PID" 2>/dev/null || true; fi' EXIT
trap 'exit 143' TERM
trap 'exit 130' INT
${functions}
PUSH_PID=""
WORK="$WFSIM_FIXTURE"
HERE="$WFSIM_SCRIPTS"
cd "$WORK"
RANKER_PUBLISHED_EPOCH=""
RANKER_PUBLISH_PENDING=0
ask ""
ask "builds $WFSIM_FIXTURE/builds.ndjson"
ask "facts $WFSIM_FIXTURE/facts.ndjson"
unfinished=0
stamped_with_dirty=0
turns=0
while [ "$turns" -lt 128 ]; do
  changed=0
  publish_incremental first
  turns=$((turns + 1))
  if [ "$changed" = 1 ]; then
    [ "$unfinished" = 1 ] && [ "$RANKER_PUBLISH_PENDING" = 1 ]
    stamped_with_dirty=1
    break
  fi
  [ "$RANKER_PUBLISH_PENDING" = 1 ]
  [ -z "$RANKER_PUBLISHED_EPOCH" ]
  unfinished=1
  ask "facts $WFSIM_FIXTURE/hot.ndjson"
done
[ "$stamped_with_dirty" = 1 ]
while [ "$RANKER_PUBLISH_PENDING" = 1 ] && [ "$turns" -lt 256 ]; do
  changed=0
  publish_incremental first
  turns=$((turns + 1))
done
[ "$RANKER_PUBLISH_PENDING" = 0 ] && [ "$changed" = 1 ]
changed=0
publish_incremental first
[ "$changed" = 0 ] && [ "$RANKER_PUBLISHED_EPOCH" = first ]
ask publish
[[ "$RANKER_REPLY" =~ ^ok\\ publish\\ 0\\ 0$ ]]
inventory_blocked=0
inventory_pending=0
RANKER_PUBLISH_PENDING=1
changed=0
start_push
started_push="$PUSH_PID"
for attempt in {1..250}; do
  [ -f push-started ] && break
  sleep 0.02
done
[ -f push-started ] && kill -0 "$started_push"
consume_inventory_status "$WFSIM_FIXTURE/blocked.json"
[ -z "$PUSH_PID" ]
if kill -0 "$started_push" 2>/dev/null; then echo "blocked inventory left its uploader running" >&2; exit 1; fi
[ "$inventory_blocked" = 1 ] && [ "$inventory_pending" = 0 ] && [ "$changed" = 0 ]
[ "$RANKER_PUBLISHED_EPOCH" = first ] && [ "$RANKER_PUBLISH_PENDING" = 1 ]
kill -0 "$RANKER_PID"
for malformed in 'ok publish_step 1 0 0' 'ok publish_step 1 0 2 0' 'ok publish 1 0'; do
  ask() { RANKER_REPLY="$malformed"; }
  changed=0
  if publish_incremental second; then echo "accepted malformed publication reply" >&2; exit 1; fi
  [ "$changed" = 0 ] && [ "$RANKER_PUBLISHED_EPOCH" = first ]
done
printf '{"native_round":true,"stamp_with_deferred_updates":true,"quiescent":true,"malformed_replies":3,"turns":%s}\\n' "$turns"
`;
  const { stdout: output } = await promisify(execFile)(bash, ['-c', script], {
    encoding: 'utf8', timeout: 30000, maxBuffer: 1024*1024,
    env: { ...process.env, WFSIM_RANKER: binary, WFSIM_FIXTURE: dir, WFSIM_SCRIPTS: scripts,
      WFSIM_SITE: `http://127.0.0.1:${server.address().port}`, BOARD_PUSH_TOKEN: 'fixture' },
  });
  const result = JSON.parse(output);
  assert(result.native_round && result.stamp_with_deferred_updates && result.quiescent);
  assert.equal(result.malformed_replies, 3);
  assert(result.turns > 1 && result.turns < 256);
  assert.equal(requests.length, 1, 'the stopped real uploader sent another file or publication stamp');
  assert(!requests[0].includes('meta.json'));
  console.log('ok: blocked inventory stops its real in-flight HTTP uploader before another file or publication stamp');
  console.log('ok: actual Bash/native rounds yield, drain, reject malformed replies and retain the live process without publishing blocked inventory');
} finally {
  server?.closeAllConnections();
  if (server) await new Promise(resolve => server.close(resolve));
  rmSync(dir, { recursive: true, force: true });
}
