// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { RUST_GROUPS, WHOLE_SCOPE_GROUPS, wholeScopeGroups } from './ci_rust.mjs';
import { rivenRequestCases } from './riven_request_cases.mjs';

const list = text => text.split(',').map(s => s.trim());
function pageGroups(workflow) {
  const section = workflow.split('\n  page-rest:')[1]?.split('\n  ci-required:')[0];
  assert(section, 'page-rest exists');
  assert.match(section, /fail-fast: false/);
  const groups = list(section.match(/group: \[([^\]]+)\]/)?.[1] || '');
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

const prefix = 'optimize::whole_scope_tests::';
const tests = [...Object.values(WHOLE_SCOPE_GROUPS), prefix + 'first', prefix + 'second'];
const groups = wholeScopeGroups([...tests, 'other::test']);
assert.deepEqual(new Set(Object.values(groups).flat()), new Set(tests));
assert.equal(Object.values(groups).flat().length, tests.length, 'each Rust test exactly once');
assert(groups['whole-scope'].includes(prefix + 'first'));
assert.throws(() => wholeScopeGroups(tests.slice(1)), /required whole-scope test missing/);
assert.throws(() => wholeScopeGroups(Object.values(WHOLE_SCOPE_GROUPS)), /basics is empty/);
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
const suites = list(workflow.match(/suite: \[([^\]]+)\]/)[1]);
assert.deepEqual(new Set(suites), new Set(['workspace', 'webapi', 'accuracy', ...RUST_GROUPS]));
assert.match(workflow, /node scripts\/ci_rust\.mjs "\$SUITE"/);
assert.match(workflow, /webapi\).*--skip optimize::whole_scope_tests::.*--skip optimize::quick_descent_tests::/);
assert.match(workflow, /\*\).*exit 2/);
const pages = pageGroups(workflow);
assert.throws(() => pageGroups(workflow.replace("if: matrix.group == 'cards'", "if: matrix.group == 'absent'")));
assert.throws(() => pageGroups(workflow.replace("        if: matrix.group == 'cards'\n", '')));
assert.throws(() => pageGroups(workflow.replace('group: [account, views, cards, heads, library]', 'group: [account]')));
const ship = readFileSync('.github/workflows/ship.yml', 'utf8');
assert.match(ship, /shared-key: engine-macos-\$\{\{ matrix\.arch \}\}/);
assert.match(ship, /cache-workspace-crates: true/);
console.log(`CI partitions cover every Rust case once and ${Object.values(pages).flat().length} page commands; missing, duplicate and empty groups fail.`);
