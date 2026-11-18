// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import './check_ci_fast.mjs';
import { FAST_JOBS, fastJobs } from './ci_fast.mjs';
import { execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, relative, resolve } from 'node:path';
import { ALL_CI_JOBS, assertGate, BROWSER_JOBS, browserCheckPaths, changedPaths, checkJobPaths,
  classify, CONSUMER_JOBS, FRONTEND_JOBS, jobPlan, OPTIONAL_NATIVE_JOBS, PRODUCT_JOBS } from './ci_policy.mjs';
import { releaseArtifactsAvailable, trustedRun, validProof, verifiedBase } from './ci_runs.mjs';
import { COMPANION_JOBS, companionJobs } from './full_work.mjs';
import { CLIENT_INPUTS, clientSource } from './ci_client_inputs.mjs';
import { PARTS } from './gen_worker_parts.mjs';
import { NATIVE_NON_SEARCH_INPUTS } from './ci_native_inputs.mjs';

for (const path of ['engine/src/fight.rs', 'optimizer/src/search.rs', 'webapi/src/optimize.rs',
  'data/weapons/braton.yaml', 'Cargo.lock', '.github/workflows/ci.yml', 'scripts/ci_policy.mjs',
  'new-package/anything', 'desktop/src/main.rs', 'AGENTS.md', 'docs/CLIENTS.md', 'docs/measurements/M001.md']) {
  assert.equal(classify([path]).scope, 'full', path);
}
assert.deepEqual(classify([]).jobs, PRODUCT_JOBS);
assert.equal(NATIVE_NON_SEARCH_INPUTS.length, 75, 'the reviewed Native declaration is complete');
assert.equal(new Set(NATIVE_NON_SEARCH_INPUTS).size, 75);
assert.equal(NATIVE_NON_SEARCH_INPUTS.filter(p => p.startsWith('scripts/')).length, 21);
assert.equal(NATIVE_NON_SEARCH_INPUTS.filter(p => p.startsWith('tests/fixtures/')).length, 18);
assert.equal(NATIVE_NON_SEARCH_INPUTS.filter(p => p.startsWith('docs/receipts/')).length, 36);
for (const path of NATIVE_NON_SEARCH_INPUTS) {
  assert.deepEqual(classify([path]).jobs, PRODUCT_JOBS, path + ' retains every ordinary correctness job');
  assert.equal(classify([path]).deep, false, path + ' cannot alter optimizer enumeration, scheduling or scoring policy');
}
for (const extra of ['optimizer/src/search.rs', 'webapi/src/optimize.rs',
  'scripts/import_native_unknown.py', 'tests/fixtures/client/native_unknown.json', 'docs/receipts/native_unknown.json'])
  assert.equal(classify([...NATIVE_NON_SEARCH_INPUTS, extra]).deep, true, extra + ' requires deep coverage');
assert.equal(classify(NATIVE_NON_SEARCH_INPUTS, true).deep, true);
assert.equal(classify(NATIVE_NON_SEARCH_INPUTS, false, false).deep, true);
const inventoryControlInputs = ['scripts/inventory_control.mjs', 'scripts/inventory_rebuild.mjs',
  'scripts/inventory_pipeline.mjs', 'scripts/inventory_relational_pipeline.mjs'];
for (const path of inventoryControlInputs) {
  const plan = classify([path]);
  assert.deepEqual(plan.jobs, PRODUCT_JOBS);
  assert.equal(plan.deep, false, path + ' retains ordinary correctness coverage without full-catalog search fixtures');
}
assert.equal(classify([...inventoryControlInputs, 'optimizer/src/search.rs']).deep, true);
assert.equal(classify(['scripts/unregistered_inventory.mjs']).deep, true);
const websiteTools = ['deploy/hk-website.nginx', 'deploy/website-public.conf',
  'scripts/check_hk_website.mjs', 'scripts/check_hk_website.py', 'scripts/check_website_proxy.mjs',
  'scripts/ship_website.py', 'scripts/website_config.py', 'scripts/website_hk.py'];
for (const path of websiteTools) {
  assert.deepEqual(classify([path]).jobs, PRODUCT_JOBS, path + ' retains all ordinary correctness checks');
  assert.equal(classify([path]).deep, false, path + ' cannot change search enumeration');
  for (const extra of ['optimizer/src/search.rs', 'webapi/src/optimize.rs', 'scripts/website_unknown.py'])
    assert.equal(classify([path, extra]).deep, true, path + ' mixed with ' + extra);
  assert.equal(classify([path], true).deep, true);
  assert.equal(classify([path], false, false).deep, true);
}
assert.deepEqual([...checkJobPaths(readFileSync('.github/workflows/ci.yml', 'utf8')).get('scripts/check_factory_availability.mjs') || []],
  ['pipeline'], 'the complete availability regression has one pipeline CI owner');
assert.deepEqual([...checkJobPaths(readFileSync('.github/workflows/ci.yml', 'utf8')).get('scripts/check_statistics_clock.mjs') || []],
  ['pipeline'], 'sampled statistics expiration must run in ordinary pipeline CI');
for (const path of ['scripts/check_statistics_clock.mjs', 'worker/statistics_clock.js']) {
  assert.deepEqual(classify([path]).jobs, BROWSER_JOBS, path + ' retains every service and browser job');
  assert.equal(classify([path]).deep, false, path + ' cannot change search enumeration');
  for (const extra of ['optimizer/src/search.rs', 'webapi/src/optimize.rs', 'scripts/check_statistics_unknown.mjs'])
    assert.equal(classify([path, extra]).deep, true);
  assert.equal(classify([path], true).deep, true);
  assert.equal(classify([path], false, false).deep, true);
}
assert.deepEqual([...checkJobPaths(readFileSync('.github/workflows/ci.yml', 'utf8')).get('scripts/check_nona_help.mjs') || []],
  ['services'], 'illustrated help parsing and ruler selection must run in ordinary service CI');
const statisticsToolInputs = ['deploy/statistics_grants.sql', 'scripts/check_statistics_projection.mjs',
  'scripts/check_statistics_provider.mjs', 'scripts/fixtures/statistics_work_421.mjs',
  'scripts/pg/fixtures.mjs', 'scripts/statistics_schema.mjs'];
for (const path of statisticsToolInputs) {
  assert.deepEqual(classify([path]).jobs, PRODUCT_JOBS, path + ' retains every ordinary correctness job');
  assert.equal(classify([path]).deep, false, path + ' cannot change search enumeration or scoring');
  for (const extra of ['optimizer/src/search.rs', 'webapi/src/optimize.rs',
    'deploy/statistics_unknown.sql', 'scripts/check_statistics_unknown.mjs',
    'scripts/fixtures/statistics_unknown.mjs', 'scripts/pg/statistics_unknown.mjs'])
    assert.equal(classify([path, extra]).deep, true, path + ' mixed with ' + extra);
  assert.equal(classify([path], true).deep, true, path + ' forced deep');
  assert.equal(classify([path], false, false).deep, true, path + ' without a verified baseline');
}
assert.deepEqual(classify(statisticsToolInputs).jobs, PRODUCT_JOBS);
assert.equal(classify(statisticsToolInputs).deep, false);
const inputMaintenance = 'scripts/board_input_maintenance.mjs';
assert.deepEqual(classify([inputMaintenance]).jobs, PRODUCT_JOBS);
assert.equal(classify([inputMaintenance]).deep, false);
for (const extra of ['optimizer/src/search.rs', 'webapi/src/optimize.rs',
  'scripts/board_input_maintenance_unknown.mjs'])
  assert.equal(classify([inputMaintenance, extra]).deep, true);
assert.equal(classify([inputMaintenance], true).deep, true);
assert.equal(classify([inputMaintenance], false, false).deep, true);
assert.equal(classify(['scripts/board_input_maintenance_unknown.mjs']).deep, true);
assert.deepEqual([...checkJobPaths(readFileSync('.github/workflows/ci.yml', 'utf8')).get('scripts/check_board_input_maintenance.mjs') || []],
  ['pipeline'], 'existing-input maintenance has one ordinary pipeline CI owner');
assert.match(readFileSync('.github/workflows/ci.yml', 'utf8'),
  /node scripts\/check_source_capture_gc\.mjs\s+node scripts\/check_board_input_maintenance\.mjs/);
assert.deepEqual([...checkJobPaths(readFileSync('.github/workflows/ci.yml', 'utf8')).get('scripts/check_settler.mjs') || []],
  ['pipeline'], 'statistics regressions retain their ordinary pipeline parent');
const settlerCheck = readFileSync('scripts/check_settler.mjs', 'utf8');
assert.match(settlerCheck, /^import \{statisticsProviderCases\} from ['"]\.\/check_statistics_provider\.mjs['"];$/m);
assert.match(settlerCheck, /^await statisticsProviderCases\(\);$/m);
const providerCheck = readFileSync('scripts/check_statistics_provider.mjs', 'utf8');
assert.match(providerCheck, /^import \{statisticsStorageCases\} from ['"]\.\/check_statistics_projection\.mjs['"];$/m);
const providerCases = providerCheck.match(/export async function statisticsProviderCases\(\)\s*\{[\s\S]*?\n\}/)?.[0];
assert.ok(providerCases, 'the pipeline provider regression entry exists');
assert.match(providerCases, /^ await statisticsStorageCases\(\);$/m);
const admissionToolInputs = ['scripts/check_build_workflows.mjs', 'scripts/check_factory_deploy.py',
  'scripts/check_workflow_recovery.mjs', 'scripts/inbox_reader.mjs', 'scripts/migrations/db_library.mjs',
  'tests/fixtures/build-admission-base5a/build_workflows.sql',
  'tests/fixtures/build-admission-base5a/compute_admit_tasks.sql', 'tests/fixtures/build-admission-base5a/source.json'];
for (const path of admissionToolInputs) {
  assert.deepEqual(classify([path]).jobs, PRODUCT_JOBS, path + ' retains ordinary correctness jobs');
  assert.equal(classify([path]).deep, false, path + ' does not change search enumeration or scoring');
  for (const extra of ['optimizer/src/search.rs', 'webapi/src/optimize.rs', 'scripts/unregistered_admission.mjs'])
    assert.equal(classify([path, extra]).deep, true, path + ' mixed with ' + extra);
  assert.equal(classify([path], true).deep, true, path + ' forced deep');
  assert.equal(classify([path], false, false).deep, true, path + ' without a verified baseline');
}
assert.equal(classify(admissionToolInputs).deep, false);
for (const path of ['tests/fixtures/build-admission-base5a/unknown.sql', 'scripts/migrations/unknown.mjs'])
  assert.equal(classify([path]).deep, true, path + ' is outside the exact declarations');
const boardPipelineInputs = ['scripts/check_order_cut.mjs', 'scripts/check_order_cut_pg.mjs', 'scripts/check_order_cut_reads.mjs', 'scripts/check_order_cut_keyed.mjs',
  'scripts/check_inventory_inputs_keyed.mjs', 'tests/fixtures/inventory-inputs-keyed/broad-inputs.sql',
  'scripts/board_input_mailbox.mjs', 'scripts/board_result_flow.mjs', 'scripts/board_result_browser.mjs', 'scripts/board_read_schema.mjs',
  'scripts/board_rank_index.mjs', 'scripts/board_rank_baseline.mjs', 'scripts/board_rank_files.mjs', 'scripts/board_rank_flow.mjs',
  'scripts/board_rank_gc.mjs', 'scripts/board_rank_keys.mjs', 'scripts/board_rank_publication.mjs', 'scripts/board_rank_schema.mjs', 'scripts/board_rank_work.mjs',
  'scripts/board_rank_storage.mjs',
  'deploy/wfsim-board-read.service', 'deploy/board_read_grants.sql',
  'scripts/board_requested_intake.mjs', 'scripts/board_requested_schema.mjs',
  'scripts/board_requested_partitions.mjs',
  'deploy/wfsim-board-requested.service',
  'tests/fixtures/order-cut-activation/f3ffb55e.sql', 'tests/fixtures/order-cut-effective-reads/501b4458.sql',
  'tests/fixtures/order-cut-effective-reads/ef61dd588.sql',
  'scripts/factory_capacity.mjs', 'scripts/pg_health.py', 'scripts/check_pg_health.py',
  'deploy/wfsim-live.service', 'scripts/board_operator.mjs', 'scripts/prepared_board_inputs.mjs',
  'scripts/prepared_work_inventory.mjs', 'scripts/audit_prepared_members.mjs', 'scripts/source_capture_gc.mjs',
  'scripts/sqlite_transaction_recovery.mjs', 'scripts/check_task_pipeline.mjs',
  'scripts/check_pipeline_clock.mjs', 'scripts/check_official_claims.mjs',
  'scripts/capacity_directory_bytes.mjs', 'scripts/check_capacity_walk.mjs',
  'tests/fixtures/board-consumers-61de/scripts/board_heads.mjs',
  'tests/fixtures/board-consumers-61de/scripts/board_meta.py',
  'tests/fixtures/board-consumers-61de/mcp/headless.js', 'tests/fixtures/board-consumers-61de/source.json'];
for (const path of boardPipelineInputs) {
  assert.deepEqual(classify([path]).jobs, PRODUCT_JOBS, path);
  assert.equal(classify([path]).deep, false, path + ' retains full ordinary CI without unrelated catalog search');
}
assert.equal(classify([...boardPipelineInputs, 'optimizer/src/search.rs']).deep, true);
assert.deepEqual(classify(['scripts/check_board_requested_intake.mjs']).jobs,BROWSER_JOBS);
assert.equal(classify(['scripts/check_board_requested_intake.mjs']).deep,false);
assert.equal(classify(['scripts/unregistered_board_pipeline.mjs']).deep, true);
assert.equal(classify(['tests/fixtures/order-cut-effective-reads/unknown.sql']).deep, true);
assert.equal(classify(boardPipelineInputs, false, false).deep, true);
const boardWorkflow = readFileSync('.github/workflows/ci.yml', 'utf8');
assert(boardWorkflow.includes('run: python3 scripts/check_pg_health.py'), 'daemon and snapshot freshness checks have a CI owner');
for (const [parent, helper, calls] of [
  ['scripts/check_task_modules.mjs', 'check_task_pipeline.mjs', ['await taskPipelineCases(']],
  ['scripts/check_task_modules.mjs', 'check_pipeline_clock.mjs', ['await pipelineClockCases(']],
  ['scripts/check_task_modules.mjs', 'check_order_cut.mjs', ['await orderCutCases(', 'await orderCutWorkflowCases(']],
  ['scripts/check_task_modules.mjs', 'check_order_cut_reads.mjs', ['await orderCutReadCases(']],
  ['scripts/check_task_modules.mjs', 'check_order_cut_keyed.mjs', ['await orderCutKeyCases(']],
  ['scripts/check_incremental_inventory_pg.mjs', 'check_inventory_inputs_keyed.mjs', ['await inventoryInputKeyCases(']],
  ['scripts/check_tasks.mjs', 'check_official_claims.mjs',
    ['[officialSourceIndexCases,officialClaimGuardCases]', 'await run(db)']],
  ['scripts/check_inventory_disk_budget.mjs', 'check_capacity_walk.mjs', ["await import('./check_capacity_walk.mjs')"]],
]) {
  assert(checkJobPaths(boardWorkflow).has(parent), parent + ' has a permanent CI owner');
  const source = readFileSync(parent, 'utf8');
  assert(source.includes(helper), parent + ' imports ' + helper);
  for (const call of calls) assert(source.includes(call), parent + ' runs ' + call);
}
const largePlan = classify(Array.from({ length: 4096 }, (_, i) =>
  `data/weapons/melee/${'weapon_'.repeat(10)}${i}.yaml`));
const { reasons: largeReasons, ...executionPlan } = largePlan;
assert.equal(largeReasons.length, 4096);
assert(Buffer.byteLength(JSON.stringify(largePlan)) > 128 * 1024);
assert.deepEqual(jobPlan(largePlan), executionPlan, 'only path explanations leave the job environment');
const encodedPlan = JSON.stringify(jobPlan(largePlan));
assert(Buffer.byteLength(encodedPlan) < 8192);
if (process.platform === 'linux') {
  assert.throws(() => execFileSync(process.execPath, ['-e', ''], {
    env: { CI_PLAN: JSON.stringify(largePlan) }, stdio: 'pipe', timeout: 10000,
  }), error => error.code === 'E2BIG', 'the old environment transport must reproduce the kernel failure');
}
const transportedPlan = execFileSync(process.execPath, ['-e', 'process.stdout.write(process.env.CI_PLAN)'], {
  env: { ...process.env, CI_PLAN: encodedPlan }, encoding: 'utf8', timeout: 10000,
});
assert.deepEqual(JSON.parse(transportedPlan), executionPlan);
assert.deepEqual(jobPlan({ ...largePlan, scope: 'hotfix', paths: ['worker/pg/sync.sql'], reason: 'incident' }),
  { ...executionPlan, scope: 'hotfix', paths: ['worker/pg/sync.sql'], reason: 'incident' });
assert.deepEqual(classify(['docs/UI.md'], true).jobs, PRODUCT_JOBS);
assert.equal(classify(['docs/UI.md', 'README.md']).jobs.length, 0);
for (const paths of [['docs/UI.md'], ['worker/index.js'], ['web/src/static/style.css'], ['site/release.json']]) {
  assert.deepEqual(classify(paths, false, false).jobs, PRODUCT_JOBS, 'an unverified base cannot justify skipping tests');
}
for (const path of ['web/src/static/style.css', 'data/i18n/en/ui.yaml', 'web/src/static/app/41-fissures.js']) {
  const p = classify([path]);
  assert.equal(p.scope, 'frontend');
  assert.deepEqual(p.jobs, FRONTEND_JOBS);
  assert(!p.jobs.includes('services') && !p.jobs.includes('pipeline'));
  assert.deepEqual(companionJobs(p.jobs), ['runtime', 'prepare-desktop', 'prepare-live']);
  for (const server of ['worker/pg/schema.sql', 'worker/board_tasks.js']) {
    assert.deepEqual(classify([path, server]).jobs, BROWSER_JOBS);
    assert.deepEqual(classify([server, path]).jobs, BROWSER_JOBS);
  }
  assert.deepEqual(classify([path, 'unregistered/shared.js']).jobs, PRODUCT_JOBS);
}
for (const path of ['scripts/check_storage.mjs', 'scripts/check_simulation_context.mjs',
  'scripts/check_replay.mjs', 'scripts/check_combat_record.mjs', 'scripts/check_support.mjs',
  'scripts/check_board_review_publication.mjs']) {
  const plan = classify([path]);
  assert.deepEqual(plan.jobs, FRONTEND_JOBS, path + ' has a permanent browser CI owner');
  assert.equal(plan.deep, false, path + ' does not change search enumeration or scoring');
}
const boardReaderCheck = 'scripts/check_board_page_reader.mjs';
assert.deepEqual([...checkJobPaths(readFileSync('.github/workflows/ci.yml', 'utf8')).get(boardReaderCheck) || []],
  ['page'], 'the complete page reader has a direct browser CI owner');
assert.deepEqual(classify([boardReaderCheck]).jobs, FRONTEND_JOBS);
assert.equal(classify([boardReaderCheck]).deep, false);
for (const extra of ['optimizer/src/search.rs', 'scripts/check_board_page_reader_unknown.mjs']) {
  assert.equal(classify([boardReaderCheck, extra]).deep, true, 'browser ownership cannot hide ' + extra);
}
assert.equal(classify([boardReaderCheck], true).deep, true);
assert.equal(classify([boardReaderCheck], false, false).deep, true);
for (const path of PARTS.flatMap(part => [part.part].flat())) {
  assert.deepEqual(classify([path]).jobs, BROWSER_JOBS, path + ' feeds generated worker or headless code');
}
for (const [check, inputs] of Object.entries(CLIENT_INPUTS)) {
  assert(clientSource(check).length > 0);
  for (const path of inputs) {
    const plan = classify([path]);
    assert.deepEqual(plan.jobs, BROWSER_JOBS, path + ' changes protocol fixture input');
    assert(companionJobs(plan.jobs).includes('protocol'));
  }
}
function checkClientReads(roots, read = path => readFileSync(path, 'utf8'), exists = existsSync) {
  const visited = new Set(), pending = [...roots], boundary = new Set(['scripts/ci_client_inputs.mjs', ...PARTS.map(part => part.out)]);
  while (pending.length) {
    const path = pending.pop();
    if (visited.has(path) || boundary.has(path) || /\/(?:node_modules|vendor)\//.test(path)) continue;
    visited.add(path);
    const source = read(path).replace(/^\s*\/\/.*$/gm, '').replace(/\/\*[\s\S]*?\*\//g, '');
    assert(!/(?:web\/(?:src\/)?|["']web["']\s*,\s*["']src["']|data\/i18n\/)/.test(source),
      path + ' must declare frontend fixture inputs through ci_client_inputs.mjs');
    for (const match of source.matchAll(/["'](\.[^"'\n]+\.(?:js|mjs))["']/g)) {
      const input = relative(process.cwd(), resolve(dirname(path), match[1])).replaceAll('\\', '/');
      if (/\.(?:js|mjs)$/.test(input) && exists(input)) pending.push(input);
    }
  }
}
const serviceRoots = [...checkJobPaths(readFileSync('.github/workflows/ci.yml', 'utf8'))]
  .filter(([, jobs]) => [...jobs].some(job => ['services', 'pipeline', 'protocol'].includes(job))).map(([check]) => check);
checkClientReads(serviceRoots);
for (const source of ["import './nested.mjs';", "import(process.env.FIXTURE || './nested.mjs')"]) {
  assert.throws(() => checkClientReads(['scripts/fixture.mjs'], path => path.endsWith('fixture.mjs')
    ? source : "readFileSync(new URL('../web/src/static/app/new-client.js', import.meta.url))", () => true),
    /nested\.mjs must declare/, 'a helper cannot hide an undeclared frontend read');
}
for (const path of ['worker/pg/schema.sql', 'scripts/check_board_verify.mjs', 'scripts/check_tasks.mjs', 'scripts/check_task_reissue.mjs']) {
  const p = classify([path]);
  assert(p.jobs.includes('site') && p.jobs.includes('pipeline') && p.jobs.includes('parity'), path);
  assert(!p.jobs.includes('test') && !p.jobs.includes('gas'), path);
}
for (const path of ['scripts/check_compute_storage.mjs','worker/pg/library.sql']) {
  assert(classify([path]).jobs.includes('services'),path+' runs the storage boundary checks');
}
for (const path of ['scripts/check_browser_db_migration.mjs','web/src/static/app/53-task-store.js',
  'web/src/static/app/35-results.js','web/src/static/app/69-board-work.js','web/src/static/nona/runtime/store.js']) {
  assert(existsSync(path),path+' is an actual migration check input');
  assert(classify([path]).jobs.includes('page-rest'),path+' runs browser upgrade checks');
}
for (const path of ['scripts/check_sqlite_migration.mjs','scripts/sqlite_schema.mjs']) {
  assert.equal(classify([path]).scope,'full',path+' retains conservative selection');
}
for (const [kind, paths] of Object.entries({ mcp: ['mcp/index.js', 'mcp/account.js', 'scripts/check_mcp_auth.mjs',
  'scripts/check_mcp_entry.mjs', 'scripts/mcp_test_loader.mjs'], bot: ['bot/engine.mjs', 'scripts/check_bot_auth.mjs'] })) {
  for (const path of paths) {
    assert.deepEqual(classify([path]).jobs, CONSUMER_JOBS[kind], path);
    assert.deepEqual(classify([path, 'web/src/static/style.css']).jobs, FRONTEND_JOBS);
    assert.deepEqual(classify([path, 'engine/src/lib.rs']).jobs, PRODUCT_JOBS);
    for (const pair of [[path, 'site/release.json'], ['site/release.json', path]]) {
      assert.deepEqual(classify(pair).jobs, PRODUCT_JOBS, 'snapshot mixing cannot narrow validation');
    }
  }
}
assert.deepEqual(new Set(classify(['mcp/index.js', 'bot/engine.mjs']).jobs), new Set(['site', 'checks', 'mcp', 'bot']));
assert.deepEqual(new Set(classify(['mcp/headless.js']).jobs), new Set(['site', 'checks', 'mcp', 'bot']));
assert.deepEqual(classify(['mcp/new_shared_input.js']).jobs, PRODUCT_JOBS, 'unknown MCP dependencies fail closed');
assert.equal(classify(['engine/src/lib.rs']).deep, false);
const ciWiring = ['.github/workflows/ci.yml', '.github/workflows/ship.yml',
  '.github/actions/site-tools/action.yml', '.github/actions/site-tools/install.sh',
  '.github/actions/built-site/action.yml', 'scripts/ci_policy.mjs', 'scripts/ci_runs.mjs',
  'scripts/check_ci_policy.mjs', 'scripts/ci_hotfix.mjs', 'scripts/check_ci_hotfix.mjs',
  'scripts/check_ci_partitions.mjs', 'scripts/check_site_setup.mjs'];
const nonSearchTools = [...inventoryControlInputs, 'scripts/check_weapon_search.mjs', 'scripts/fetch_inbox.sh', 'scripts/cancel_queue.sh',
  'scripts/check_time_storage.mjs',
  'scripts/live_board.sh', 'scripts/live_orders.mjs', 'scripts/ship_builds.sh', 'scripts/retire_scores.mjs', 'scripts/ship_queue.sh',
  'scripts/fetch_facts.sh', 'scripts/ship_facts.sh', 'scripts/live_publish.mjs', '.github/workflows/queue.yml',
  'deploy/factoryd_grants.sql', 'scripts/check_rescore_paths.mjs', 'scripts/capture_compute_inventory.mjs',
  'scripts/build_batches.mjs', 'scripts/ship_family.mjs', 'scripts/ship_bot.py', '.github/workflows/scores.yml',
  'scripts/factoryd.mjs', 'scripts/board_admission_schema.mjs',
  'scripts/fetch_score_inputs.mjs', 'scripts/order_credit.mjs', 'scripts/fetch_queue.sh', '.github/workflows/settle.yml',
  'scripts/task_reissue.mjs',
  'scripts/pg_task_archive.py', 'scripts/ship_task_archive.py', 'scripts/check_task_archive.py',
  'scripts/ship.py', 'scripts/release_stage.py', 'scripts/check_release_stage.py', 'scripts/check_ship_bot_assets.py',
  'scripts/check_ship_workers.py', 'scripts/check_ship_bot_live.py',
  'scripts/payload_manifest.py', 'scripts/release_desktop.py', 'scripts/pg/test_db.mjs', 'scripts/intake_batches.mjs',
  'scripts/import_inverse_speed.py', 'tests/fixtures/client/inverse-speed-2026-10-28.json',
  'deploy/wfsim-task-archive.service', 'deploy/wfsim-task-archive.timer'];
for (const path of [...ciWiring, ...nonSearchTools]) {
  assert.equal(classify([path]).scope, 'full', path);
  assert.equal(classify([path]).deep, false, `${path} does not change search behavior`);
  assert(classify([path, 'optimizer/src/search.rs']).deep, 'CI wiring cannot hide search changes');
  assert(classify([path], true).deep, 'explicit full CI still runs every fixture');
  assert(classify([path], false, false).deep, 'unverified baselines retain deep coverage');
}
assert.equal(classify(['engine/src/board/equipment.rs', 'webapi/src/lib.rs',
  'scripts/check_equipment_library.mjs', '.github/workflows/ci.yml',
  'scripts/check_weapon_search.mjs', 'scripts/fetch_inbox.sh', 'scripts/live_board.sh', 'scripts/ship_builds.sh']).deep, false,
  'equipment-library checks and workflow wiring do not force full-catalog search');
for (const path of ['Cargo.toml', 'Cargo.lock', 'scripts/ci_rust.mjs', 'tests/fixtures/client/future.json',
  'scripts/check_future.mjs', '.github/workflows/unknown.yml']) assert(classify([path]).deep, path);
for (const paths of [[], ['optimizer/src/quick.rs'], ['webapi/src/optimize.rs'], ['unknown/file']]) assert(classify(paths).deep);
assert.equal(classify(['desktop/examples/updatekit.rs']).deep, false, 'independent shell packaging cannot change search enumeration');
const readyFeatureTools = ['scripts/import_kitgun_status.py', 'tests/fixtures/client/kitgun-status-2026-10-28.json',
  'tests/golden/M132-vermisplicer-secondary-status.json', 'docs/measurements/M132-vermisplicer-secondary-status.md',
  'scripts/engine_compare.py', 'scripts/check_engine_compare.py', 'tests/fixtures/engine-compare/calls.json'];
for (const path of readyFeatureTools) {
  assert.deepEqual(classify([path]).jobs, PRODUCT_JOBS, path + ' retains every correctness job');
  assert.equal(classify([path]).deep, false, path + ' cannot change search enumeration');
  for (const search of ['optimizer/src/quick.rs', 'webapi/src/optimize.rs', 'scripts/unknown_tool.py'])
    assert.equal(classify([path, search]).deep, true, path + ' mixed with ' + search);
  assert.equal(classify([path], true).deep, true, path + ' forced deep');
  assert.equal(classify([path], false, false).deep, true, path + ' without verified baseline');
}
assert.deepEqual(classify(readyFeatureTools).jobs, PRODUCT_JOBS);
assert.equal(classify(readyFeatureTools).deep, false);
for (const path of ['tests/fixtures/engine-compare/new-calls.json', 'scripts/new_engine_compare.py',
  'tests/fixtures/client/kitgun-status-other.json', 'tests/golden/M133-other.json', 'docs/measurements/M133-other.md'])
  assert.equal(classify([path]).deep, true, path + ' is outside the exact registered paths');
assert.match(readFileSync('.github/workflows/ci.yml', 'utf8'), /- run: python3 scripts\/check_engine_compare\.py/);
assert(classify(['desktop/examples/updatekit.rs', 'optimizer/src/quick.rs']).deep);
assert(classify(['mcp/index.js'], true).deep);
for (const path of ['scripts/check_kitgun_slot.mjs', 'scripts/check_riven_request.mjs']) {
  assert.deepEqual(classify([path]).jobs, FRONTEND_JOBS, path);
  assert.deepEqual(classify([path], false, false).jobs, PRODUCT_JOBS);
  assert.deepEqual(classify([path], true).jobs, PRODUCT_JOBS);
  assert.equal(classify([path, 'engine/src/lib.rs']).scope, 'full');
}
for (const path of ['scripts/check_future.mjs', 'scripts/check_gas.mjs', 'scripts/check_lane_memory.mjs',
  'scripts/check_ci_partitions.mjs', 'scripts/check_assembly.mjs', 'scripts/cdp.mjs', 'scripts/build_site_app.py']) {
  assert.equal(classify([path]).scope, 'full', path);
}
assert.equal(classify(['web/src/static/style.css', 'engine/src/fight.rs']).scope, 'full');
assert.equal(classify(['data/board_state.yaml']).scope, 'snapshot');
assert(!classify(['site/asset/app.123.js']).jobs.includes('site'));
assert(classify(['site/asset/app.123.js', 'worker/index.js']).jobs.includes('site'));
assert.equal(classify(['data/board_state.yaml', 'worker/index.js']).scope, 'full');
assert.equal(classify(['docs/UI.md', 'data/board_state.yaml']).scope, 'snapshot');

const needs = plan => Object.fromEntries(['plan', 'quick', 'commits', ...ALL_CI_JOBS].map(j =>
  [j, { result: ['plan', 'quick', 'commits', ...plan.jobs].includes(j) ? 'success' : 'skipped' }])
  .concat(OPTIONAL_NATIVE_JOBS.map(j => [j, { result: 'skipped' }]))
  .concat(COMPANION_JOBS.map(j => [j, { result: companionJobs(plan.jobs, !!plan.fast).includes(j) ? 'success' : 'skipped' }])));
for (const plan of [classify(['docs/UI.md']), classify(['web/src/static/style.css']), classify(['worker/index.js']), classify(['engine/src/lib.rs'])]) {
  assertGate(plan, needs(plan));
  for (const job of ['plan', 'quick', 'commits', ...plan.jobs]) {
    for (const result of ['failure', 'cancelled', 'skipped', undefined]) {
      const n = needs(plan);
      n[job].result = result;
      assert.throws(() => assertGate(plan, n), undefined, `${job} ${result} must block`);
    }
  }
}
const full = classify([], true);
const reusedNeeds = needs(classify(['docs/UI.md']));
assertGate(full, reusedNeeds, true);
assert.throws(() => assertGate(full, reusedNeeds));
assert.throws(() => assertGate(full, { ...needs(full), coordination: { result: 'failure' } }));
for (const name of OPTIONAL_NATIVE_JOBS) {
  for (const result of ['success', 'failure', 'skipped', 'cancelled']) assertGate(full, { ...needs(full), [name]: { result } });
  const missing = needs(full);
  delete missing[name];
  assert.throws(() => assertGate(full, missing));
}
const proof = { version: 1, sha: 'abc', policy: 'p', reused: false, deep: true, jobs: PRODUCT_JOBS,
  results: Object.fromEntries(['quick', 'commits', 'plan', ...PRODUCT_JOBS, ...COMPANION_JOBS].map(j => [j, 'success'])) };
assert(validProof(proof, 'abc', 'p', true));
for (const name of COMPANION_JOBS) {
  const results = { ...proof.results }; delete results[name];
  assert(!validProof({ ...proof, results }, 'abc', 'p', true), `${name}: missing companion proof blocks release`);
  assert(!validProof({ ...proof, results: { ...proof.results, [name]: 'failure' } }, 'abc', 'p', true));
}
const artifacts = ['site-built', 'ci-proof', 'release-desktop', 'release-live'].map(name => ({ name, expired: false }));
assert(releaseArtifactsAvailable(artifacts));
for (const artifact of artifacts) {
  assert(!releaseArtifactsAvailable(artifacts.filter(a => a !== artifact)));
  assert(!releaseArtifactsAvailable(artifacts.map(a => a === artifact ? { ...a, expired: true } : a)));
}
const fastProof = { ...proof, scope: 'fast', jobs: FAST_JOBS, fast: { base: 'a'.repeat(40), paths: [], reason: 'maintenance' } };
assert(validProof(fastProof, 'abc', 'p'));
assert(!validProof(fastProof, 'abc', 'p', true));
assert(!validProof({ ...fastProof, fast: null }, 'abc', 'p'));
assert(!validProof({ ...proof, fast: fastProof.fast }, 'abc', 'p'));
assert(validProof(fastProof, 'abc', 'p', false, PRODUCT_JOBS, true));
assert(!validProof({ ...fastProof, jobs: FAST_JOBS.slice(1) }, 'abc', 'p'));
const parentFast = { ...fastProof, fast: { ...fastProof.fast, paths: ['engine/src/board/builds/identity.rs'] } };
assert(!validProof(parentFast, 'abc', 'p'), 'parent repair proof cannot omit gas/native prerequisites');
assert(validProof({ ...parentFast, jobs: fastJobs(parentFast.fast.paths) }, 'abc', 'p'));
assert(!validProof({ ...parentFast, jobs: fastJobs(parentFast.fast.paths),
  results: { ...proof.results, gas: 'failure' } }, 'abc', 'p'));
for (const bad of [{ ...proof, sha: 'other' }, { ...proof, policy: 'old' }, { ...proof, reused: true },
  { ...proof, jobs: ['site'] }, { ...proof, results: {} }, { ...proof, version: 0 }]) {
  assert(!validProof(bad, 'abc', 'p', true));
}
assert(!validProof({ ...proof, jobs: [] }, 'abc', 'p'));
assert(!validProof({ ...proof, jobs: ['site'] }, 'abc', 'p'));
assert(!validProof({ ...proof, jobs: PRODUCT_JOBS.filter(j => j !== 'test') }, 'abc', 'p'));
assert(validProof({ ...proof, jobs: BROWSER_JOBS }, 'abc', 'p'));
assert(!validProof({ ...proof, jobs: BROWSER_JOBS }, 'abc', 'p', true));
assert(validProof({ ...proof, jobs: BROWSER_JOBS }, 'abc', 'p', false, BROWSER_JOBS));
assert(validProof({ ...proof, jobs: FRONTEND_JOBS }, 'abc', 'p', false, FRONTEND_JOBS));
assert(!validProof({ ...proof, jobs: FRONTEND_JOBS }, 'abc', 'p', false, BROWSER_JOBS));
assert(!validProof({ ...proof, jobs: FRONTEND_JOBS }, 'abc', 'p', true));
assert(!validProof({ ...proof, jobs: BROWSER_JOBS }, 'abc', 'p', false, ['test']));
assert(!validProof({ ...proof, jobs: BROWSER_JOBS }, 'abc', 'p', false, ['published-board']));
assert(validProof(proof, 'abc', 'p', false, PRODUCT_JOBS));
assert(!validProof({ ...proof, deep: false }, 'abc', 'p', true));
assert(!validProof({ ...proof, deep: false }, 'abc', 'p', false, PRODUCT_JOBS, true));
for (const jobs of Object.values(CONSUMER_JOBS)) {
  assert(validProof({ ...proof, jobs }, 'abc', 'p'));
  assert(!validProof({ ...proof, jobs }, 'abc', 'p', false, ['page']));
}
assert(!validProof({ ...proof, jobs: CONSUMER_JOBS.mcp }, 'abc', 'p', false, CONSUMER_JOBS.bot));
assert(!validProof({ ...proof, jobs: ['site', 'checks'] }, 'abc', 'p'));
const run = { id: 1, head_sha: 'abc', conclusion: 'success', status: 'completed', event: 'workflow_dispatch',
  head_repository: { full_name: 'owner/repo' }, workflow_id: 9 };
const context = { sha: 'abc', repository: 'owner/repo', workflow: 9, current: 2 };
assert(trustedRun(run, context));
for (const patch of [{ id: 2 }, { head_sha: 'other' }, { conclusion: 'failure' }, { status: 'in_progress' },
  { event: 'pull_request' }, { display_title: 'hotfix: task' }, { workflow_id: 8 }, { head_repository: { full_name: 'fork/repo' } }]) {
  assert(!trustedRun({ ...run, ...patch }, context), JSON.stringify(patch));
}
const baseline = candidate => ({
  git: args => {
    assert(args.includes(':!site') && args.includes(':!data/board_state.yaml'));
    return 'abc';
  },
  api: path => path.endsWith('/ci.yml') ? { id: 9 } : { workflow_runs: candidate ? [candidate] : [] },
});
assert(verifiedBase('snapshot', 'owner/repo', baseline(run)));
for (const bad of [null, { ...run, conclusion: 'failure' }, { ...run, event: 'pull_request' },
  { ...run, display_title: 'hotfix: task' }, { ...run, head_sha: 'old' }]) {
  assert(!verifiedBase('snapshot', 'owner/repo', baseline(bad)));
}
assert.throws(() => verifiedBase('snapshot', 'owner/repo', {
  ...baseline(run), api: () => { throw new Error('API unavailable'); },
}));

const dir = mkdtempSync(join(tmpdir(), 'wfsim-ci-diff-'));
try {
  const git = args => execFileSync('git', args, { cwd: dir, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  git(['init', '-b', 'main']);
  git(['config', 'user.name', 'fixture']);
  git(['config', 'user.email', 'fixture@example.invalid']);
  writeFileSync(join(dir, 'mechanics.rs'), 'fn main() {}');
  git(['add', '.']); git(['commit', '-m', 'fixture']);
  const base = git(['rev-parse', 'HEAD']);
  git(['mv', 'mechanics.rs', 'README.md']); git(['commit', '-am', 'rename']);
  const paths = changedPaths({ before: base }, { GITHUB_EVENT_NAME: 'push' }, git);
  assert.deepEqual(new Set(paths), new Set(['mechanics.rs', 'README.md']));
  assert.equal(classify(paths).scope, 'full', 'a rename out of code still tests code');
  assert.deepEqual(changedPaths({ pull_request: { base: { sha: base } } }, { GITHUB_EVENT_NAME: 'pull_request' }, git), paths);
} finally { rmSync(dir, { recursive: true, force: true }); }

const workflow = readFileSync('.github/workflows/ci.yml', 'utf8');
const browserChecks = browserCheckPaths(workflow);
assert(browserChecks.size > 0);
for (const path of browserChecks) {
  const jobs = [...checkJobPaths(workflow).get(path)];
  const expected = jobs.every(j => j === 'mcp') ? CONSUMER_JOBS.mcp
    : jobs.every(j => j === 'bot') ? CONSUMER_JOBS.bot
    : jobs.every(j => FRONTEND_JOBS.includes(j)) ? FRONTEND_JOBS : BROWSER_JOBS;
  assert.deepEqual(classify([path]).jobs, expected, path);
}
assert.deepEqual([...browserCheckPaths('jobs:\n  gas:\n    run: node scripts/check_gas.mjs\n  page:\n    run: node scripts/check_example.mjs\n    run: node scripts/check_gas.mjs\n  quick:\n    run: node scripts/check_quick.mjs\n')],
  ['scripts/check_example.mjs']);
assert.deepEqual([...browserCheckPaths('jobs:\n  page:\n    # node scripts/check_comment.mjs\n    - name: node scripts/check_title.mjs\n      run: |\n        # node scripts/check_disabled.mjs\n        node scripts/check_example.mjs\n  gas:\n    - run: |\n        MODE=build node scripts/check_example.mjs\n')], []);
assert.deepEqual([...checkJobPaths('jobs:\n  factory-cases:\n    run: node scripts/factory_shards.mjs run mode="$MODE"\n')]
  .map(([path, jobs]) => [path, [...jobs]]), [['scripts/check_tasks.mjs', ['factory-cases']]], 'wrapped checks retain their actual owner');
const gate = workflow.slice(workflow.indexOf('\n  ci-required:'));
assert.match(gate, /if:.*always\(\)/);
const jobIds = [...workflow.slice(workflow.indexOf('\njobs:\n')).matchAll(/^  ([\w-]+):$/gm)]
  .map(m => m[1]).filter(j => j !== 'ci-required');
const gateJobs = gate.match(/needs: \[([^\]]+)\]/)[1].split(',').map(j => j.trim());
assert.deepEqual(new Set(gateJobs), new Set(jobIds), 'every workflow job must reach the required gate');
assert.match(workflow, /cancel-in-progress:.*refs\/heads\/main/);
assert(!workflow.split('\n  site:')[1].split('\n  native-mac:')[0].includes("github.event_name != 'pull_request'"), 'PRs build the current site');
assert.match(workflow, /node scripts\/check_ci_policy\.mjs/);
const pipeline = workflow.split('\n  pipeline:')[1].split('\n  page:')[0];
assert.match(pipeline, /needs: \[plan, quick, runtime\]/);
assert(!pipeline.includes('built-site'), 'SQL and protocol checks must not wait for artwork');
const services = workflow.split('\n  services:')[1].split('\n  mcp:')[0];
assert.match(services, /needs: \[plan, quick, runtime\]/);
assert(!services.includes('built-site'));
assert.match(workflow, /- id: scope\s+env:\s+GH_TOKEN:.*\n\s+run: node scripts\/ci_policy\.mjs/);
const ship = readFileSync('.github/workflows/ship.yml', 'utf8');
assert.match(ship.split('\njobs:')[0], /actions: read/, 'release evidence needs permission to download CI proofs');
console.log('CI selection, failed/cancelled gates, exact-SHA trust and rename coverage hold.');
