// SPDX-License-Identifier: AGPL-3.0-or-later
import { execFileSync, spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';

const PREFIX = 'optimize::whole_scope_tests::';
export const WHOLE_SCOPE_GROUPS = {
  descent: PREFIX + 'a_descent_split_across_workers_lands_where_one_worker_does',
  'screened-descent': PREFIX + 'a_descent_split_across_workers_lands_where_one_worker_does_with_step_screen',
  'fleet-work': PREFIX + 'a_search_costs_the_same_work_however_the_fleet_splits_it',
};
export const RUST_GROUPS = ['whole-scope', ...Object.keys(WHOLE_SCOPE_GROUPS)];

export function wholeScopeGroups(names) {
  const tests = names.filter(n => n.startsWith(PREFIX));
  if (new Set(tests).size !== tests.length) throw new Error('duplicate whole-scope test');
  for (const name of Object.values(WHOLE_SCOPE_GROUPS)) {
    if (!tests.includes(name)) throw new Error(`required whole-scope test missing: ${name}`);
  }
  const groups = { 'whole-scope': tests.filter(n => !Object.values(WHOLE_SCOPE_GROUPS).includes(n)),
    ...Object.fromEntries(Object.entries(WHOLE_SCOPE_GROUPS).map(([group, name]) => [group, [name]])) };
  if (!groups['whole-scope'].length) throw new Error('whole-scope basics is empty');
  return groups;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const group = process.argv[2];
  if (!RUST_GROUPS.includes(group)) throw new Error(`unknown Rust group: ${group}`);
  const cargo = ['test', '-p', 'wfsim-webapi', '--lib'];
  const listed = execFileSync('cargo', [...cargo, '--', '--list'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'] });
  const names = listed.split('\n').filter(s => s.endsWith(': test')).map(s => s.slice(0, -6));
  const selected = wholeScopeGroups(names)[group];
  console.log(`${group}: ${selected.length} exact tests`);
  for (const name of selected) {
    console.log(`::group::${name}`);
    const started = Date.now();
    const result = spawnSync('cargo', [...cargo, name, '--', '--exact', '--nocapture'], { stdio: 'inherit' });
    console.log(`::endgroup::\n${name}: ${((Date.now() - started) / 1000).toFixed(1)}s`);
    if (result.error) throw result.error;
    if (result.status !== 0) process.exit(result.status || 1);
  }
}
