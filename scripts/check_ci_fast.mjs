// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { checkFastChanges, checkFastProof, FAST_JOBS, fastScope } from './ci_fast.mjs';

const base = 'a'.repeat(40), options = { base, verifiedBase: true, reason: 'bounded maintenance release' };
const paths = ['worker/pg/sync.sql', 'web/src/static/app/73-compute.js', 'cli/src/board/run.rs'];
const scope = fastScope(paths, options);
assert.deepEqual(scope.paths, [...paths].sort());
for (const path of ['engine/src/fight.rs', 'optimizer/src/lib.rs', 'data/weapons/torid.yaml',
  'worker/tasks.js', 'worker/pg/schema.sql', 'docs/CLIENTS.md', 'docs/measurements/M09.md', 'site/app.js'])
  assert.throws(() => fastScope([path], options));
for (const changed of [{ verifiedBase: false }, { force: true }, { hotfix: true }, { base: '' }, { reason: '' }])
  assert.throws(() => fastScope(paths, { ...options, ...changed }));
assert.throws(() => fastScope([], options));
const proof = { scope: 'fast', fast: scope, jobs: FAST_JOBS };
assert.deepEqual(checkFastProof(proof, paths), scope);
assert.throws(() => checkFastProof(proof, [...paths, 'worker/tasks.js']));
assert.throws(() => checkFastProof({ ...proof, fast: { ...scope, paths: [] } }, paths));
assert.throws(() => checkFastProof({ ...proof, jobs: FAST_JOBS.slice(1) }, paths));
const sql = 'outside;\nCREATE OR REPLACE FUNCTION factory_forget_codes() RETURNS bigint LANGUAGE sql AS $$ SELECT 1 $$;\n';
const fixture = (before, after) => args => args[1].startsWith(base + ':') ? before : after;
checkFastChanges(scope, 'head', fixture(sql, sql.replace('SELECT 1', 'SELECT 2')));
assert.throws(() => checkFastChanges(scope, 'head', fixture(sql, sql.replace('outside;', 'changed;'))));
assert.throws(() => checkFastChanges({ ...scope, paths: ['worker/pg/library.sql'] }, 'head', () => '+CREATE TABLE unsafe();'));
assert.throws(() => checkFastChanges({ ...scope, paths: ['scripts/factoryd.mjs'] }, 'head', fixture('settle();', 'other();')));
checkFastChanges({ ...scope, paths: ['scripts/factoryd.mjs'] }, 'head', fixture('export const PAST_MS = 60000, PAST_BATCH = 2_000;', 'export const PAST_MS = 60000, PAST_BATCH = 100;'));
assert.throws(() => checkFastChanges({ ...scope, paths: ['cli/src/board/assemble/inventory.rs'] }, 'head', fixture('runtime;#[cfg(test)]tests', 'changed;#[cfg(test)]tests')));
const workflow = readFileSync('.github/workflows/ci.yml', 'utf8');
assert(workflow.includes("if: ${{ !fromJSON(needs.plan.outputs.plan).fast }}"));
assert(workflow.includes('if [ "$FAST_CI" != "true" ]; then node scripts/check_tasks.mjs; fi'));
assert(workflow.includes("if: matrix.group == 'fast'"));
assert(workflow.includes('run: node scripts/ci_fast.mjs page'));
assert(workflow.includes('cargo test -p wfsim-cli --lib board::'));
assert(workflow.split('\n  pipeline:\n')[1].split('\n  page:\n')[0].includes('components: clippy'));

console.log('Fast CI rejects unrelated code, unverified bases, changed schemas/protocol and incomplete proof scope.');
