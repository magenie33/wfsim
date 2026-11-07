// Reuse only completed measurements of the same compiled fight and data inputs.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

export function measurementProof(run, jobs, repository, workflow) {
  assert.equal(run.head_repository?.full_name, repository, 'foreign measurement repository');
  assert.equal(run.workflow_id, workflow, 'different measurement workflow');
  assert.equal(run.event, 'workflow_dispatch', 'measurements must come from the manual workflow');
  assert.equal(run.status, 'completed', 'measurement run is still active');
  assert.match(run.head_sha, /^[a-f0-9]{40}$/);
  for (const name of ['build', 'measure (1)', 'measure (2)', 'measure (3)', 'measure (4)'])
    assert(jobs.some(job => job.name === name && job.conclusion === 'success'), `missing successful ${name}`);
  return run.head_sha;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const id = process.argv[2], repository = process.env.GITHUB_REPOSITORY;
  assert(/^\d+$/.test(id || '') && repository, 'provide a measurement run and repository');
  const api = path => JSON.parse(execFileSync('gh', ['api', path], { encoding: 'utf8' }));
  const run = api(`repos/${repository}/actions/runs/${id}`);
  const jobs = api(`repos/${repository}/actions/runs/${id}/jobs?per_page=100`).jobs;
  const workflow = api(`repos/${repository}/actions/workflows/work.yml`).id;
  const source = measurementProof(run, jobs, repository, workflow);
  execFileSync('git', ['fetch', '--filter=blob:none', '--no-tags', 'origin', source], { stdio: 'pipe' });
  execFileSync('git', ['diff', '--exit-code', '--quiet', source, 'HEAD', '--',
    'engine', 'webapi', 'wasm', 'optimizer', 'native', 'data', 'tools/gas-macro',
    'Cargo.toml', 'Cargo.lock', 'mise.toml', 'scripts/data_packs.py', 'scripts/wasm_engine.mjs',
    'tests/fixtures/simulation-cost.json', 'tests/gas/rows.json']);
  const prior = execFileSync('git', ['show', `${source}:scripts/work_calibrate.mjs`], { encoding: 'utf8' });
  const current = readFileSync('scripts/work_calibrate.mjs', 'utf8');
  const timing = text => text.slice(text.indexOf('function boardRows('), text.indexOf('/// An account inside one run:'))
    .replace(/\/\/\/ ONE OF THE FACTORY[\s\S]*$/, '').trim();
  assert.equal(timing(current), timing(prior), 'measurement sampling or timing changed');
  const priorWorkflow = execFileSync('git', ['show', `${source}:.github/workflows/work.yml`], { encoding: 'utf8' });
  const build = text => {
    const body = text.split('\n  build:\n')[1]?.split('\n  measure:\n')[0];
    assert(body, 'missing measurement build recipe');
    return body.replace("    if: inputs.measurement_run == ''\n", '');
  };
  assert.equal(build(readFileSync('.github/workflows/work.yml', 'utf8')), build(priorWorkflow), 'measurement compiler recipe changed');
  console.log(JSON.stringify({ measurement_run: id, source, machines: 4, fight_inputs_match: true }));
}
