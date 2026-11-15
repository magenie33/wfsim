// SPDX-License-Identifier: AGPL-3.0-or-later
import { execFileSync, spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';

const ordinaryDocs = path => /^docs\/.*\.md$/.test(path) && !/^docs\/(CLIENTS\.md$|measurements\/)/.test(path);

export const HOTFIX_PROFILES = {
  'board-entry': {
    paths: ['scripts/live_board.sh', 'scripts/split_board_pipeline.mjs', 'scripts/check_board_cli_entry.mjs'],
    groups: ['board-entry'],
  },
  'factory-sql': {
    paths: ['worker/pg/sync.sql', 'scripts/check_tasks.mjs', 'scripts/check_board_sync.mjs',
      'scripts/check_board_verify.mjs', 'scripts/check_sync_proc.mjs', 'scripts/check_fair.mjs',
      'scripts/check_settler.mjs', 'scripts/check_settler_async.mjs', 'scripts/check_initiator.mjs',
      'scripts/check_moot_checks.mjs', 'scripts/check_forget_past.mjs', 'scripts/check_idle_devices.mjs'],
    groups: ['factory', 'async'],
  },
  ci: {
    paths: ['.github/workflows/ci.yml', 'scripts/ci_policy.mjs', 'scripts/ci_runs.mjs',
      'scripts/ci_hotfix.mjs', 'scripts/check_ci_policy.mjs', 'scripts/check_ci_partitions.mjs',
      'scripts/check_ci_hotfix.mjs'],
    groups: ['policy'],
  },
};

export const HOTFIX_CHECKS = {
  'board-entry': ['check_board_cli_entry'],
  policy: ['check_ci_policy', 'check_ci_partitions', 'check_ci_hotfix'],
  factory: ['check_tasks', 'check_board_verify', 'check_board_sync', 'check_sync_proc',
    'check_fair', 'check_settler', 'check_settler_async', 'check_initiator',
    'check_moot_checks', 'check_forget_past', 'check_idle_devices', 'check_contribution',
    'check_ledger', 'check_dials', 'check_clients_doc', 'check_appraise'],
  async: ['check_tasks', 'check_board_verify', 'check_board_sync', 'check_fair'],
};

export const BOARD_ENTRY_CHANGE = {
  liveBefore: '  node --max-old-space-size=192 "$HERE/inventory_pipeline.mjs" "$WORK" "$BIN" --serve',
  liveAfter: '  node --max-old-space-size=192 "$HERE/split_board_pipeline.mjs" serve "$WORK" "$BIN"',
  marker: 'if(process.argv[1]&&pathToFileURL(resolve(process.argv[1])).href===import.meta.url) {\n',
  footerBefore: " const [stage,work,bin]=process.argv.slice(2);if(!['normalization','inventory','projection'].includes(stage)||!work||!bin)throw Error('usage: split_board_pipeline.mjs normalization|inventory|projection <work> <bin>');\n await serveBoardStage(stage,resolve(work),resolve(bin));\n}\n",
  footerAfter: " const [stage,work,bin]=process.argv.slice(2);if(!['serve','normalization','inventory','projection'].includes(stage)||!work||!bin)throw Error('usage: split_board_pipeline.mjs serve|normalization|inventory|projection <work> <bin>');\n if(stage==='serve')await serveSplitBoardPipeline(resolve(work),resolve(bin));\n else await serveBoardStage(stage,resolve(work),resolve(bin));\n}\n",
};

export function assertBoardEntryChanges(paths, base, head) {
  if (![base, head].every(sha => /^[a-f0-9]{40}$/.test(sha || ''))) throw new Error('Board-entry requires exact Git base and head');
  const git = args => execFileSync('git', args, { stdio: ['ignore', 'pipe', 'pipe'] });
  git(['merge-base', '--is-ancestor', base, head]);
  const actual = git(['diff', '--name-only', '--no-renames', '-z', base, head]).toString().split('\0').filter(Boolean).sort();
  if (JSON.stringify(actual) !== JSON.stringify([...paths].sort())) throw new Error('Board-entry paths differ from the actual Git diff');
  const allowed = [...HOTFIX_PROFILES['board-entry'].paths, ...HOTFIX_PROFILES.ci.paths];
  if (actual.some(path => !allowed.includes(path) && path !== 'docs/CHECKS.md')) throw new Error('Board-entry cannot include other runtime or contract changes');
  const [live, split, check] = HOTFIX_PROFILES['board-entry'].paths;
  if (![live, split, check].every(path => actual.includes(path))) throw new Error('Board-entry requires both entry repairs and their new CLI regression');
  const mode = (sha, path) => git(['ls-tree', sha, '--', path]).toString().match(/^(100644|100755) blob [a-f0-9]{40}\t/)?.[1];
  for (const path of [live, split]) {
    if (!mode(base, path) || mode(base, path) !== mode(head, path)) throw new Error('Board-entry cannot change runtime file types or modes');
  }
  if (git(['ls-tree', base, '--', check]).length || mode(head, check) !== '100644' || !git(['show', `${head}:${check}`]).length)
    throw new Error('Board-entry requires a new regular CLI check');
  const before = path => git(['show', `${base}:${path}`]), after = path => git(['show', `${head}:${path}`]);
  const original = before(live), oldLine = Buffer.from('\n' + BOARD_ENTRY_CHANGE.liveBefore + '\n');
  const line = original.indexOf(oldLine);
  if (line < 0 || original.indexOf(oldLine, line + 1) !== -1 || !after(live).equals(Buffer.concat([
    original.subarray(0, line), Buffer.from('\n' + BOARD_ENTRY_CHANGE.liveAfter + '\n'), original.subarray(line + oldLine.length),
  ]))) throw new Error('Board-entry permits only the live command replacement');
  const oldSplit = before(split), newSplit = after(split), marker = Buffer.from(BOARD_ENTRY_CHANGE.marker);
  const oldMain = oldSplit.indexOf(marker), newMain = newSplit.indexOf(marker);
  if (oldMain < 0 || newMain !== oldMain || oldSplit.indexOf(marker, oldMain + 1) !== -1 || newSplit.indexOf(marker, newMain + 1) !== -1
      || !oldSplit.subarray(0, oldMain).equals(newSplit.subarray(0, newMain))
      || !oldSplit.subarray(oldMain).equals(Buffer.from(BOARD_ENTRY_CHANGE.marker + BOARD_ENTRY_CHANGE.footerBefore))
      || !newSplit.subarray(newMain).equals(Buffer.from(BOARD_ENTRY_CHANGE.marker + BOARD_ENTRY_CHANGE.footerAfter)))
    throw new Error('Board-entry permits only the split CLI footer; runtime bodies must be byte-identical');
}

export function hotfixPlan(paths, { verifiedBase = true, reason = '', event = 'workflow_dispatch', force = false, base, head } = {}) {
  if (event !== 'workflow_dispatch' || force) throw new Error('Hotfix CI requires a manual dispatch without full=true');
  if (!verifiedBase || !paths.length) throw new Error('Hotfix CI requires a verified baseline and a nonempty diff');
  if (!reason.trim() || reason.length > 500) throw new Error('Hotfix CI requires a reason of 1–500 characters');
  const profiles = new Set(), reasons = [];
  for (const path of paths) {
    if (ordinaryDocs(path)) {
      reasons.push({ path, kind: 'docs' });
      continue;
    }
    const profile = Object.entries(HOTFIX_PROFILES).find(([, p]) => p.paths.includes(path))?.[0];
    if (!profile) throw new Error(`Outside hotfix coverage: ${path}`);
    profiles.add(profile);
    reasons.push({ path, kind: profile });
  }
  if (!profiles.size) throw new Error('Hotfix CI requires a supported code or check change');
  if (profiles.has('board-entry')) assertBoardEntryChanges(paths, base, head);
  const groups = new Set(['policy']);
  for (const p of profiles) for (const group of HOTFIX_PROFILES[p].groups) groups.add(group);
  return { scope: 'hotfix', jobs: ['hotfix'], profiles: [...profiles].sort(),
    matrix: { group: [...groups].sort() }, paths: [...paths].sort(), reasons, reason: reason.trim(), verifiedBase: true };
}

export function assertHotfixProof(proof, sha, policy, paths) {
  if (proof?.version !== 1 || proof.sha !== sha || proof.policy !== policy || proof.reused !== false
      || proof.scope !== 'hotfix' || !/^[a-f0-9]{40}$/.test(proof.hotfix?.base || '')) throw new Error('Invalid hotfix proof identity');
  const plan = hotfixPlan(paths, { reason: proof.hotfix.reason, base: proof.hotfix.base, head: sha });
  for (const key of ['paths', 'profiles', 'matrix']) {
    if (JSON.stringify(proof.hotfix[key]) !== JSON.stringify(plan[key])) throw new Error(`Hotfix proof ${key} does not cover this diff`);
  }
  if (JSON.stringify(proof.jobs) !== JSON.stringify(plan.jobs)) throw new Error('Hotfix proof jobs do not match');
  for (const job of ['plan', 'quick', 'commits', 'coordination', 'hotfix']) {
    if (proof.results?.[job] !== 'success') throw new Error(`Hotfix proof ${job} did not succeed`);
  }
  return plan;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const group = process.argv[2], checks = HOTFIX_CHECKS[group];
  if (!checks) throw new Error(`Unknown hotfix check group: ${group}`);
  const env = { ...process.env, ...(group === 'async' ? { WFSIM_SETTLE_ASYNC: '1', WFSIM_SYNC_PROC: '1' } : {}) };
  for (const name of checks) {
    console.log(`\n${group}: ${name}`);
    const result = spawnSync(process.execPath, [`scripts/${name}.mjs`], { env, stdio: 'inherit' });
    if (result.error) throw result.error;
    if (result.status !== 0) process.exit(result.status || 1);
  }
}
