// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { DEEP_SCOPE_GROUPS, DEEP_SCOPE_TESTS, RUST_GROUPS, RUST_SUITES, WHOLE_SCOPE_GROUPS, wholeScopeGroups } from './ci_rust.mjs';
import { rivenRequestCases } from './riven_request_cases.mjs';

const list = text => text.split(',').map(s => s.trim());
function pageGroups(workflow, job = 'page-rest') {
  const section = workflow.split('\n  ' + job + ':\n')[1]?.split(/\n  [\w-]+:\n/)[0];
  assert(section, `${job} exists`);
  assert.match(section, /fail-fast: false/);
  const literal = section.match(/group: \[([^\]]+)\]/);
  const choices = [...section.matchAll(/fromJSON\('(\[[^']+\])'\)/g)].map(m => JSON.parse(m[1]));
  const groups = literal ? list(literal[1]) : choices.flat();
  assert(groups.length > 1 && new Set(groups).size === groups.length, 'distinct page groups');
  const commands = Object.fromEntries(groups.map(g => [g, []]));
  for (const step of section.split(/^      - /m).slice(1)) {
    const run = step.match(/^\s*run: (.+)$/m)?.[1];
    if (!run || run === 'npm ci --silent --prefix scripts/pg') continue;
    const group = step.match(/^\s*if: matrix\.group == '([\w-]+)'$/m)?.[1];
    assert(groups.includes(group), `one declared group for ${run}`);
    commands[group].push(run);
  }
  for (const [g, rows] of Object.entries(commands)) assert(rows.length, `nonempty page group ${g}`);
  const all = Object.values(commands).flat();
  assert.equal(new Set(all).size, all.length, 'a page command executes once');
  return commands;
}

function protocolCases(workflow) {
  const section = workflow.split('\n  protocol:\n')[1]?.split(/\n  [\w-]+:\n/)[0];
  assert(section, 'protocol matrix exists');
  assert.match(section, /fail-fast: false/);
  assert.deepEqual(list(section.match(/group: \[([^\]]+)\]/)[1]), ['async', 'proc', 'proc-async']);
  const cases = [];
  for (const step of section.split(/^      - /m).slice(1)) {
    const group = step.match(/if: matrix\.group == '([\w-]+)'/)?.[1];
    const commands = [...step.matchAll(/\bnode scripts\/(check_[\w-]+\.mjs)\b/g)].map(match => match[1]);
    if (!commands.length) continue;
    assert(['async', 'proc', 'proc-async'].includes(group));
    assert.equal(/WFSIM_SYNC_PROC: "1"/.test(step), group !== 'async', group);
    assert.equal(/WFSIM_SETTLE_ASYNC: "1"/.test(step), group !== 'proc', group);
    cases.push(...commands.map(command => `${group}/${command}`));
  }
  assert.equal(new Set(cases).size, cases.length, 'each command and protocol mode executes once');
  assert.deepEqual([...cases].sort(), [
    'async/check_board_verify.mjs', 'async/check_queue_once.mjs', 'async/check_board_sync.mjs',
    'async/check_board_reviews.mjs', 'async/check_fair.mjs', 'proc/check_board_sync.mjs',
    'proc-async/check_board_sync.mjs', 'proc-async/check_board_reviews.mjs', 'proc-async/check_board_family.mjs',
  ].sort(), 'all original protocol command/mode pairs remain');
  return cases;
}

const prefix = 'optimize::whole_scope_tests::';
const tests = [...Object.values(WHOLE_SCOPE_GROUPS), ...DEEP_SCOPE_TESTS, prefix + 'first', prefix + 'second'];
const groups = wholeScopeGroups([...tests, 'other::test']);
assert.deepEqual(new Set(Object.values(groups).flat()), new Set(tests));
assert.equal(Object.values(groups).flat().length, tests.length, 'each Rust test exactly once');
assert(groups['whole-scope'].includes(prefix + 'first'));
assert.throws(() => wholeScopeGroups(tests.slice(1)), /required whole-scope test missing/);
assert.throws(() => wholeScopeGroups([...Object.values(WHOLE_SCOPE_GROUPS), ...DEEP_SCOPE_TESTS]), /basics is empty/);
assert.throws(() => wholeScopeGroups([...tests, tests[0]]), /duplicate/);

const meta = { weapons: Array.from({ length: 11 }, (_, i) => ({ id: `weapon${i}` })),
  benchmarks: Array.from({ length: 8 }, (_, i) => ({ id: `ruler${i}`, primary: i === 2 })) };
const everyRulerOn = ['weapon1', 'weapon3', 'weapon8'];
for (const shards of [1, 4]) {
  const old = [], balanced = [];
  for (let shard = 0; shard < shards; shard++) {
    const prior = meta.weapons.filter((_, i) => i % shards === shard).map(w => [w.id, 'ruler2']);
    if (shard === 0) for (const weapon of everyRulerOn) for (const b of meta.benchmarks) {
      if (b.id !== 'ruler2') prior.push([weapon, b.id]);
    }
    old.push(...prior.map(([weapon, ruler], index) => ({ weapon, ruler, card: index % 4, index })));
    balanced.push(...rivenRequestCases(meta, everyRulerOn, shard, shards, 4));
  }
  const canonical = rows => rows.map(r => JSON.stringify(r)).sort();
  assert.deepEqual(canonical(balanced), canonical(old), 'same weapon/ruler/card/code cases');
  const counts = Array.from({ length: shards }, (_, i) => rivenRequestCases(meta, everyRulerOn, i, shards, 4).length);
  assert(Math.max(...counts) - Math.min(...counts) <= 2, 'both case sets share the workers');
  assert.notDeepEqual(canonical(balanced.slice(1)), canonical(old), 'a lost case is visible');
}
assert.throws(() => rivenRequestCases(meta, everyRulerOn, 4, 4, 4));
assert.throws(() => rivenRequestCases({ weapons: [], benchmarks: [] }, everyRulerOn, 0, 1, 4));

const workflow = readFileSync('.github/workflows/ci.yml', 'utf8');
protocolCases(workflow);
assert.throws(() => protocolCases(workflow.replace('group: [async, proc, proc-async]', 'group: [async, proc]')));
assert.throws(() => protocolCases(workflow.replace('          node scripts/check_board_family.mjs\n', '')));
assert.throws(() => protocolCases(workflow.replace('WFSIM_SYNC_PROC: "1"', 'WFSIM_SYNC_PROC: "0"')));
assert.deepEqual(new Set([...RUST_SUITES, ...Object.keys(DEEP_SCOPE_GROUPS)]), new Set(['workspace', 'webapi', 'accuracy', ...RUST_GROUPS]));
assert.match(workflow, /matrix:.*fromJSON\(needs\.plan\.outputs\.test_matrix\)/);
assert.match(workflow, /fleet-work\|deep-descent\|deep-screened\|deep-fleet\) node/);
assert.match(workflow, /node scripts\/ci_rust\.mjs "\$SUITE"/);
assert.match(workflow, /webapi\).*--skip optimize::whole_scope_tests::.*--skip optimize::quick_descent_tests::/);
assert.match(workflow, /\*\).*exit 2/);
const pages = pageGroups(workflow);
const mainPages = pageGroups(workflow, 'page');
assert.throws(() => pageGroups(workflow.replace("if: matrix.group == 'builder-1'", "if: matrix.group == 'absent'")));
assert.throws(() => pageGroups(workflow.replace("        if: matrix.group == 'builder-1'\n", '')));
assert.throws(() => pageGroups(workflow.replace('group: [account, views, builder-1, builder-2, builder-3, heads, library]', 'group: [account]')));
assert.throws(() => pageGroups(workflow.replace("if: matrix.group == 'controls'", "if: matrix.group == 'absent'"), 'page'));
assert.throws(() => pageGroups(workflow.replace('fromJSON(\'["fast"]\')', 'fromJSON(\'["controls"]\')'), 'page'));
assert.throws(() => pageGroups(workflow.replace('fromJSON(\'["fast"]\')', 'fromJSON(\'[]\')'), 'page'));
const allPages = [...Object.values(pages).flat(), ...Object.values(mainPages).flat()];
assert.equal(new Set(allPages).size, allPages.length, 'browser commands have one owner');
const parity = workflow.split('\n  parity:')[1].split('\n  page-rest:')[0];
assert.match(parity, /fail-fast: false/);
const ship = readFileSync('.github/workflows/ship.yml', 'utf8');
assert.match(workflow, /shared-key: engine-macos-\$\{\{ matrix\.arch \}\}/);
assert.match(workflow, /cache-workspace-crates: true/);
assert(!ship.includes('\n  engine-mac:'), 'native builds precede the release workflow');
assert.match(ship, /pattern: engine-native-\*/);
assert.match(ship, /path: target\/engine-lib\s+run-id: \$\{\{ needs\.due\.outputs\.run \}\}/);
console.log(`CI partitions cover every Rust case once and ${Object.values(pages).flat().length} page commands; missing, duplicate and empty groups fail.`);
