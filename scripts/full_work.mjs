// SPDX-License-Identifier: AGPL-3.0-or-later
// Each expensive operation names its authority, invalidation and durable boundary.
export const FULL_WORK = [
  { id: 'validation', inputs: ['source', 'CI policy', 'toolchain'], invalidate: 'unknown or shared dependencies select full coverage',
    checkpoint: 'exact-source successful CI proof and complete case receipts', budget: 'independent test shards; existing case and numerical limits',
    evidence: 'ci-proof.json, factory receipts, ci-timing.json', rollback: 'block promotion; retain previous release' },
  { id: 'site', inputs: ['UI source', 'engine code', 'data', 'pinned assets', 'build tools'], invalidate: 'content digest per module, pack and asset',
    checkpoint: 'site-built from the qualified CI run', budget: 'one producer per candidate; existing WASM memory limit',
    evidence: 'release.json and artifact hashes', rollback: 'retain content-addressed previous assets and release pointer' },
  { id: 'desktop', inputs: ['site-built', 'desktop source', 'Cargo.lock', 'toolchain'], invalidate: 'complete prepared payload and tool inputs',
    checkpoint: 'verified prepared manifest and updatekit; signing remains in release', budget: 'one secretless producer; no release-time duplicate compilation',
    evidence: 'release-desktop receipt and dual-producer payload comparison', rollback: 'reject mismatched receipt; retain signed channel' },
  { id: 'live-runtime', inputs: ['native source', 'runtime closure', 'Cargo.lock', 'pinned Linux image'], invalidate: 'complete bytes and loaded-state proof',
    checkpoint: 'verified release-live artifact, deployed files and process identity', budget: 'bounded probe; unchanged closure keeps the running process',
    evidence: 'input/output receipt and runtime equality proof', rollback: 'previous binaries/configuration; unknown state follows normal deployment' },
  { id: 'intake', inputs: ['durable inbox', 'engine admission', 'family and task rules'], invalidate: 'unacknowledged source items',
    checkpoint: 'builds and families, named demands, then source acknowledgement per bounded pack', budget: 'bounded item/byte packs and disk-backed identity state',
    evidence: 'pack counts, first timestamps and ACK/replay checks', rollback: 'retain source; replay idempotently after failure' },
  { id: 'scoring', inputs: ['claimed task identities', 'authoritative builds', 'engine', 'ruler'], invalidate: 'engine/ruler or exact claimed work changes',
    checkpoint: 'each computed fact is committed independently', budget: 'bounded claim and indexed selected-key reads; original fight duration and fairness',
    evidence: 'leased/processed/committed rows and native/claimed-input parity', rollback: 'leases expire; committed facts remain reusable' },
  { id: 'board', inputs: ['complete frozen inventory cut', 'fact provenance', 'engine', 'publication rules'], invalidate: 'input contract or engine change requires a fresh baseline',
    checkpoint: 'transactional disk import offsets and publication rounds', budget: 'bounded batches and indexed disk projection; largest weapon included',
    evidence: 'full native oracle, restart/ACK tests, file bytes and capacity measurements', rollback: 'retain completed baseline and published board until replacement completes' },
  { id: 'clients', inputs: ['release pointer', 'content-addressed module, packs and assets'], invalidate: 'only changed resource identities',
    checkpoint: 'cached immutable resource bytes and validated current release', budget: 'reuse existing resource cache; preserve complete model data',
    evidence: 'engine-apart, browser cache and native/WASM parity checks', rollback: 'previous coherent release and immutable resources' },
];

export const COMPANION_JOBS = ['runtime', 'protocol', 'factory-cases', 'factory', 'prepare-desktop', 'prepare-live'];
export function companionJobs(jobs, fast = false) {
  const selected = [];
  if (jobs.some(j => ['site', 'services', 'pipeline'].includes(j))) selected.push('runtime');
  if (jobs.includes('pipeline')) selected.push('protocol');
  if (!fast && jobs.some(j => ['services', 'pipeline'].includes(j))) selected.push('factory-cases', 'factory');
  if (jobs.includes('site')) selected.push('prepare-desktop', 'prepare-live');
  return selected;
}
