// SPDX-License-Identifier: AGPL-3.0-or-later
import { execFileSync } from 'node:child_process';
import { appendFileSync, writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

export function timingReport(run, jobs, now = new Date().toISOString()) {
  const seconds = (a, b) => a && b ? Math.max(0, (Date.parse(b) - Date.parse(a)) / 1000) : null;
  const rows = jobs.map(job => ({ name: job.name, conclusion: job.conclusion,
    wait_before_start_seconds: seconds(run.created_at, job.started_at),
    run_seconds: seconds(job.started_at, job.completed_at),
    steps: (job.steps || []).map(step => ({ name: step.name, conclusion: step.conclusion,
      seconds: seconds(step.started_at, step.completed_at) })) }));
  const slowest = rows.flatMap(job => job.steps.map(step => ({ job: job.name, ...step })))
    .filter(step => step.seconds !== null).sort((a, b) => b.seconds - a.seconds).slice(0, 15);
  return { version: 1, run: run.id, source: run.head_sha, observed_at: now,
    elapsed_seconds: seconds(run.created_at, run.status === 'completed' ? run.updated_at : now),
    wait_definition: 'elapsed from workflow creation; includes dependencies and runner queue',
    cache_hits: null, cache_note: 'unreported rather than inferred from short durations', jobs: rows, slowest_steps: slowest };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const repository = process.env.GITHUB_REPOSITORY, id = process.env.GITHUB_RUN_ID;
  if (!/^[\w.-]+\/[\w.-]+$/.test(repository || '') || !/^\d+$/.test(id || '')) throw Error('CI repository and run ID required');
  const get = path => JSON.parse(execFileSync('gh', ['api', path], { encoding: 'utf8', timeout: 30000 }));
  const run = get(`repos/${repository}/actions/runs/${id}`), jobs = [];
  for (let page = 1; ; page++) {
    const result = get(`repos/${repository}/actions/runs/${id}/jobs?filter=latest&per_page=100&page=${page}`);
    jobs.push(...result.jobs);
    if (jobs.length >= result.total_count) break;
  }
  const report = timingReport(run, jobs);
  writeFileSync('ci-timing.json', JSON.stringify(report, null, 2) + '\n');
  const summary = '\n## Candidate timing\n\nDependency and queue wait are reported together; cache hits require producer evidence.\n\n'
    + '| Job / step | Seconds |\n| --- | ---: |\n'
    + report.slowest_steps.map(s => `| ${s.job.replaceAll('|', '/')} / ${s.name.replaceAll('|', '/')} | ${s.seconds} |`).join('\n') + '\n';
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, summary);
  console.log(summary);
}
