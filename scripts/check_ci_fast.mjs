// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { checkFastChanges, checkFastProof, FAST_JOBS, fastJobs, fastScope } from './ci_fast.mjs';

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
assert.throws(() => checkFastChanges({ ...scope, paths: ['worker/pg/library.sql'] }, 'head', fixture('', 'CREATE TABLE unsafe();')));
checkFastChanges({ ...scope, paths: ['worker/pg/library.sql'] }, 'head', fixture('', 'CREATE UNIQUE INDEX IF NOT EXISTS queue_calculation ON queue (build_id, ruler, mode);'));
assert.throws(() => checkFastChanges({ ...scope, paths: ['worker/pg/library.sql'] }, 'head', fixture('', 'CREATE UNIQUE INDEX unsafe ON scores (identity);')));
assert.throws(() => checkFastChanges({ ...scope, paths: ['worker/deadlines.js'] }, 'head', fixture('board_review: 15', 'board_review: 20')));
checkFastChanges({ ...scope, paths: ['worker/deadlines.js'] }, 'head', fixture('export const REFRESH_WITHIN_MS = 15;\n  refresh: 12,\n', 'export const REFRESH_WITHIN_MS = 14;\n  refresh: REFRESH_WITHIN_MS,\n'));
assert.throws(() => checkFastChanges({ ...scope, paths: ['scripts/ship_factoryd.py'] }, 'head', fixture('verify();', 'skip();')));
assert.throws(() => checkFastChanges({ ...scope, paths: ['scripts/factoryd.mjs'] }, 'head', fixture('settle();', 'other();')));
checkFastChanges({ ...scope, paths: ['scripts/factoryd.mjs'] }, 'head', fixture('export const PAST_MS = 60000, PAST_BATCH = 2_000;', 'export const PAST_MS = 60000, PAST_BATCH = 100;'));
assert.throws(() => checkFastChanges({ ...scope, paths: ['cli/src/board/assemble/inventory.rs'] }, 'head', fixture('runtime;#[cfg(test)]tests', 'changed;#[cfg(test)]tests')));
const parent = 'engine/src/board/builds/identity.rs';
const parentTests = 'engine/src/board/builds/tests/catalog.rs';
const parentScope = { ...scope, paths: [parent, parentTests, 'scripts/check_default_parent.mjs'] };
assert(fastJobs(parentScope.paths).includes('gas'), 'parent engine changes retain native/WASM and native library builds');
assert(!fastJobs(paths).includes('gas'), 'ordinary maintenance retains its bounded scope');
assert.throws(() => checkFastProof({ scope: 'fast', fast: parentScope, jobs: FAST_JOBS }, parentScope.paths));
assert.deepEqual(checkFastProof({ scope: 'fast', fast: parentScope, jobs: fastJobs(parentScope.paths) }, parentScope.paths), parentScope);
const unchanged = 'fn identity() {}\n';
const oldParent = unchanged + 'd.mods = mods;\n';
const fixedParent = unchanged + 'd.mods = canonical_mods_with(&d.weapon, &mods, d.riven.as_ref());\n';
const parentFixture = (old, current) => args => args[1].endsWith(parentTests) ? unchanged
  : args[1].startsWith(base + ':') ? old : current;
checkFastChanges(parentScope, 'head', parentFixture(oldParent, fixedParent));
assert.throws(() => checkFastChanges(parentScope, 'head', parentFixture(oldParent, fixedParent.replace('fn identity()', 'fn changed()'))));
assert.throws(() => checkFastChanges(parentScope, 'head', parentFixture(oldParent, fixedParent.replace('d.riven.as_ref()', 'None'))));
assert.throws(() => checkFastChanges({ ...parentScope, paths: [parent] }, 'head', parentFixture(oldParent, fixedParent)));
assert.throws(() => checkFastChanges({ ...scope, paths: [parentTests] }, 'head', fixture('existing tests', 'deleted tests')));
const workflow = readFileSync('.github/workflows/ci.yml', 'utf8');
assert(workflow.includes("if: ${{ !fromJSON(needs.plan.outputs.plan).fast }}"));
assert(workflow.includes('if [ "$FAST_CI" != "true" ]; then node scripts/check_tasks.mjs; fi'));
assert(workflow.includes("if: matrix.group == 'fast'"));
assert(workflow.includes('run: node scripts/ci_fast.mjs page'));
assert(workflow.includes('cargo test -p wfsim-cli --lib board::'));
assert(workflow.includes('cargo test -p wfsim-engine --lib board::'));
assert(workflow.includes('node scripts/check_default_parent.mjs'));
assert(workflow.split('\n  pipeline:\n')[1].split('\n  page:\n')[0].includes('components: clippy'));

console.log('Fast CI rejects unrelated code, unverified bases, changed schemas/protocol and incomplete proof scope.');
