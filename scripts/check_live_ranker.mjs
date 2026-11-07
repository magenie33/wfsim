// The actual Bash consumer drains native publication rounds between inventory updates.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const binary = resolve(process.env.WFSIM_INVENTORY_BOARD_BIN || 'target/debug/wfsim-board');
assert(existsSync(binary), 'build wfsim-board before checking its Bash publication consumer');
const source = readFileSync(new URL('./live_board.sh', import.meta.url), 'utf8');
const functions = ['ask', 'publish_incremental'].map(name => {
  const match = source.match(new RegExp('^'+name+'\\(\\) \\{[\\s\\S]*?^\\}', 'm'));
  assert(match, `${name} must be the production shell function`);
  return match[0];
}).join('\n');
const dir = mkdtempSync(join(tmpdir(), 'wfsim-live-ranker-'));
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
  const script = `set -euo pipefail
coproc RANKER { exec "$WFSIM_RANKER" --serve "$WFSIM_FIXTURE/board" --library "$WFSIM_FIXTURE/library.json" --state "$WFSIM_FIXTURE/state.yaml"; }
trap 'if [ -n "\u0024{RANKER_PID:-}" ]; then kill "$RANKER_PID" 2>/dev/null || true; wait "$RANKER_PID" 2>/dev/null || true; fi' EXIT
trap 'exit 143' TERM
trap 'exit 130' INT
${functions}
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
for malformed in 'ok publish_step 1 0 0' 'ok publish_step 1 0 2 0' 'ok publish 1 0'; do
  ask() { RANKER_REPLY="$malformed"; }
  changed=0
  if publish_incremental second; then echo "accepted malformed publication reply" >&2; exit 1; fi
  [ "$changed" = 0 ] && [ "$RANKER_PUBLISHED_EPOCH" = first ]
done
printf '{"native_round":true,"stamp_with_deferred_updates":true,"quiescent":true,"malformed_replies":3,"turns":%s}\\n' "$turns"
`;
  const output = execFileSync(process.env.WFSIM_BASH || 'bash', ['-c', script], {
    encoding: 'utf8', timeout: 30000, maxBuffer: 1024*1024,
    env: { ...process.env, WFSIM_RANKER: binary, WFSIM_FIXTURE: dir },
  });
  const result = JSON.parse(output);
  assert(result.native_round && result.stamp_with_deferred_updates && result.quiescent);
  assert.equal(result.malformed_replies, 3);
  assert(result.turns > 1 && result.turns < 256);
  console.log('ok: actual Bash/native rounds yield, stamp with deferred updates, drain and reject malformed replies');
} finally {
  rmSync(dir, { recursive: true, force: true });
}
