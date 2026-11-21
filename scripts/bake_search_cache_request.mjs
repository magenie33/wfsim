// SPDX-License-Identifier: AGPL-3.0-or-later
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { runInNewContext } from 'node:vm';

const json = async (file) => JSON.parse(await readFile(file, 'utf8'));

function section(source, start, end) {
  const from = source.indexOf(`\n${start}`);
  const to = source.indexOf(`\n${end}`, from + 1);
  if (from < 0 || to < 0) throw Error(`Release app is missing ${start} or ${end}`);
  return source.slice(from, to);
}

async function releaseInputs(site) {
  const [generation, html] = await Promise.all([
    json(join(site, 'pkg', 'generation.json')),
    readFile(join(site, 'index.html'), 'utf8'),
  ]);
  const meta = generation.names?.filter((name) => /^meta\.[0-9a-f]+\.json$/.test(name));
  const assets = [...html.matchAll(/<script\s+[^>]*?src=["']\/(asset\/app\.[0-9a-f]+\.js)["']/g)];
  if (meta?.length !== 1 || assets.length !== 1) throw Error('Expected one release app and meta asset');
  return Promise.all([json(join(site, 'pkg', meta[0])), readFile(join(site, assets[0][1]), 'utf8')]);
}

export async function officialSearchRequest({ site, weapon, ruler, strategy = 'four_elements',
  finalists = 5, candidateRuns = 10, excludeArcanes }) {
  if (typeof site !== 'string' || !site) throw Error('site must name the local release site directory');
  const [meta, source] = await releaseInputs(site);
  return buildOfficialRequest(meta, source, { weapon, ruler, strategy, finalists, candidateRuns, excludeArcanes });
}

export function buildOfficialRequest(META, source, { weapon, ruler, strategy, finalists, candidateRuns, excludeArcanes = [] }) {
  if (!Number.isInteger(finalists) || finalists < 1 || finalists > 100 || ![1, 10].includes(candidateRuns)) {
    throw Error('Expected finalists in 1..100 and candidateRuns of 1 or 10');
  }
  if (!Array.isArray(excludeArcanes)) throw Error('Expected an array of public arcane IDs');
  const bench = META.benchmarks?.find((item) => item.id === ruler);
  if (!META.weapons?.some((item) => item.id === weapon)) throw Error(`Unknown official weapon: ${weapon}`);
  if (!bench?.scenario) throw Error(`Unknown official ruler: ${ruler}`);
  const strategies = META.search_presets?.strategies || [{ id: 'regular' }];
  if (!strategies.some((item) => item.id === strategy)) throw Error(`Unknown search strategy: ${strategy}`);

  const headless = section(source, 'const HEADLESS_EXILUS =', 'function headlessDefaultRuns(');
  const fight = section(source, 'function theFight(', 'function renderEnemies(');
  const optimize = section(source, 'function optimizationFinalRuns(', 'function runOptimize(');
  const arena = section(source, 'let BODY_R_M =', 'const ARENA_VW =');
  const metric = section(source, 'const METRIC_FALLBACK =', 'function defaultScenario(');
  const result = runInNewContext(`${headless}\n${arena}\n${metric}\n${fight}\n${optimize}
    adoptBodyRadius(META);
    const sim = { ...headlessScenarioOnto(META, bench.scenario, CONTACT_M, METRIC_FALLBACK.id), __weapon: weapon };
    const mode = headlessDefaultMode(META, weapon, null);
    const valence = headlessDefaultValence(META, weapon, null);
    const assembly = headlessDefaultAssembly(META, weapon, null);
    const opt = { search_strategy: strategy, limits: headlessBlankLimits(META, weapon) };
    if (excludeArcanes.length) {
      const eligibleArcanes = new Set(headlessWeaponOf(META, weapon).arcanes || []);
      opt.limits.exclude.arcanes = [...new Set(excludeArcanes)].filter(id => eligibleArcanes.has(id));
    }
    const startsPayload = () => headlessBuiltinStarts(META, weapon).map(start => headlessStartPayload(META, start));
    const optRun = { finalists, candidate_runs: candidateRuns };
    const $ = () => ({ value: weapon });
    const everyRank = () => META.every_rank || { mods: [], arcanes: [] };
    const rivenPayload = () => [];
    const customEnemiesFor = () => [];
    const alsoActingPayload = () => [];
    const benchList = () => META.benchmarks;
    const scenarioNamed = () => ({ builtin: bench.id });
    const activeScenario = bench.id;
    const simRuns = () => Number(META.defaults?.runs) || 1000;
    JSON.stringify(optimizeBody());`,
  { META, bench, weapon, strategy, finalists, candidateRuns, excludeArcanes }, { timeout: 500, filename: 'release-optimize-body.js' });
  return JSON.parse(result);
}
