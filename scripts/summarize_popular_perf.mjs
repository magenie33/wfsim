// SPDX-License-Identifier: AGPL-3.0-or-later
// node scripts/summarize_popular_perf.mjs <evidence-dir> <compact.json | ->
import fs from 'node:fs';
import path from 'node:path';
import { createHash, randomUUID } from 'node:crypto';
import { fileURLToPath, pathToFileURL } from 'node:url';

const files = ['sample.json', 'popularity.json', 'final-fights.json', 'final-fleet.json', 'final-opening.json',
  ...['current-api', 'accepted-api', 'accepted-prepared'].map(n => `dll-lab/${n}.json`)];
const pkg = fileURLToPath(new URL('../site/pkg/', import.meta.url));
const geometries = ['single', 'multi'], aim = [0, 50, 100];
const runs = 3, sides = ['A', 'B'];
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const digest = s => typeof s === 'string' && /^[0-9a-f]{64}$/.test(s);
const uint = n => Number.isSafeInteger(n) && n >= 0;
const positive = n => Number.isFinite(n) && n > 0;
const finite = n => Number.isFinite(n) && n >= 0;
const rounded = n => Number(n.toFixed(6));
const close = (a, b) => Number.isFinite(a) && Number.isFinite(b) && Math.abs(a - b) <= 1e-9 * Math.max(1, Math.abs(a), Math.abs(b));
const sum = (xs, fn) => xs.reduce((n, x) => n + fn(x), 0);
const totals = (rows, fields) => Object.fromEntries(fields.map(k => [k, sum(rows, r => r[k])]));
const table = (columns, rows) => ({columns, rows});
function check(ok, message) { if (!ok) throw new Error(message); }
function median(xs) {
  const a = [...xs].sort((l, r) => l - r);
  return (a[(a.length - 1) >> 1] + a[a.length >> 1]) / 2;
}
function sameKeys(label, actual, wanted) {
  const got = new Set(actual), expected = new Set(wanted);
  check(got.size === actual.length, `${label}: duplicate ID`);
  for (const id of expected) check(got.has(id), `${label}: missing ${id}`);
  for (const id of got) check(expected.has(id), `${label}: unexpected ${id}`);
}
function index(label, rows, ids) {
  check(Array.isArray(rows), `${label}: expected an array`);
  sameKeys(label, rows.map(r => r.id), ids);
  return new Map(rows.map(r => [r.id, r]));
}
function keys(label, object, ids) {
  check(object && typeof object === 'object' && !Array.isArray(object), `${label}: expected an object`);
  sameKeys(label, Object.keys(object), ids);
}
function strata(rows, summarize, byAim = false) {
  const groups = {all: summarize(rows)};
  for (const g of geometries) {
    const part = rows.filter(r => r.id.split(':')[1] === g);
    groups[g] = summarize(part);
    if (byAim) for (const h of aim) groups[`${g}:${h}`] = summarize(part.filter(r => r.id.endsWith(`:${h}`)));
  }
  return groups;
}

function population(read) {
  const sample = read(files[0]), pop = read(files[1]);
  check(pop.ok === true && pop.window_days === 30 && /^\d{4}-\d{2}-\d{2}$/.test(pop.from) &&
    /^\d{4}-\d{2}-\d{2}$/.test(pop.to) && sample.from === pop.from && sample.to === pop.to, 'sample/popularity window mismatch');
  const names = read('site/pkg/generation.json').names;
  const catalogue = prefix => {
    const hits = names.filter(n => new RegExp(`^${prefix}\\.[a-f0-9]+\\.json$`).test(n));
    check(hits.length === 1, `generation: expected one ${prefix} file`);
    return read(`site/pkg/${hits[0]}`);
  };
  const eligible = catalogue('meta').weapons.filter(w => pop.scores[w.id] > 0);
  const ranked = eligible.sort((a, b) => pop.scores[b.id] - pop.scores[a.id] || a.name.localeCompare(b.name, 'en')).slice(0, 40);
  const zh = catalogue('i18n').zh.weapons;
  check(ranked.length === 40 && sample.samples.length === 40, 'expected a top-40 population');
  sameKeys('sample weapons', sample.samples.map(w => w.weapon), ranked.map(w => w.id));
  const byId = new Map(sample.samples.map(w => [w.weapon, w]));
  const weapons = ranked.map(({id, name}, i) => {
    const w = byId.get(id);
    check(w.rank === i + 1 && w.name === name && w.popularity === pop.scores[id] && uint(w.popularity), `sample rank/name/popularity: ${id}`);
    check(/^[a-z0-9_]+$/.test(id) && w.record?.weapon === id && /^[0-9a-f]{32}$/.test(w.identity) &&
      finite(w.publishedScore) && typeof zh[id] === 'string' && zh[id].length && /^[a-z0-9_]+$/.test(w.mode), `sample witness/name: ${id}`);
    return {rank: w.rank, id, en: name, zh: zh[id], popularityScore: w.popularity, publishedScore: w.publishedScore,
      mode: w.mode, buildIdentity: w.identity};
  });
  return {from: pop.from, to: pop.to, windowDays: pop.window_days, weapons};
}

function fights(data, ids) {
  const byId = index('final-fights', data, ids);
  const rows = ids.map(id => {
    const r = byId.get(id);
    for (const side of sides) {
      const times = r.times?.[side];
      check(Array.isArray(times) && times.length === 4 && times.every(positive) && positive(r[`ms${side}`]) &&
        close(r[`ms${side}`], median(times) / runs), `final-fights: ${id} ${side} timings/normalization`);
    }
    check(digest(r.sha256) && ['ticks', 'shots', 'procs', 'gas'].every(k => finite(r[k])), `final-fights: ${id} outcome`);
    return r;
  });
  return {timing: 'Sum of per-case median ms per fight (four timed three-fight calls per side). Ratio = sum A / sum B, not mean of ratios.',
    equivalence: 'Producer compared every timed raw response and both warmups byte-for-byte. SHA-256 is of the raw response, not re-encoded JSON.',
    aggregate: strata(rows, xs => {
      const {msA, msB} = totals(xs, ['msA', 'msB']);
      return {cases: xs.length, fightsPerPass: xs.length * runs, timedCallsPerSide: xs.length * 4,
        timedFightsPerSide: xs.length * runs * 4, msA: rounded(msA), msB: rounded(msB), speedup: rounded(msA / msB)};
    }, true),
    cases: table(['id', 'msA/fight', 'msB/fight', 'dot_ticks/fight', 'shots/fight', 'procs/fight', 'gas/fight', 'response_sha256'],
      rows.map(r => [r.id, rounded(r.msA), rounded(r.msB), r.ticks, r.shots, r.procs, r.gas, r.sha256]))};
}

const traffic = ['bytesA', 'bytesB', 'carryA', 'carryB'];
const phases = ['msA', 'msB', 'leadA', 'leadB', 'scoreA', 'scoreB'];
function search(data, ids, opening) {
  const label = opening ? 'final-opening' : 'final-fleet';
  const byId = index(label, data, ids);
  const counts = ['steps', 'builds', 'fights', 'work', ...traffic];
  const rows = ids.map(id => {
    const r = byId.get(id);
    check(phases.every(k => finite(r[k])) && counts.every(k => uint(r[k])) && uint(r.batch0) && r.batch0 > 0 && r.steps > 0 && r.builds > 0,
      `${label}: ${id} missing/invalid counters`);
    for (const side of sides) check(positive(r[`ms${side}`]) && close(r[`ms${side}`], r[`lead${side}`] + r[`score${side}`]) &&
      r[`bytes${side}`] >= r[`carry${side}`], `${label}: ${id} ${side} phase timing/bytes`);
    if (opening) check(r.steps === 1 && r.builds === Math.min(4, r.batch0) && r.result === undefined && r.jobs === undefined, `${label}: ${id} not an opening`);
    else check(digest(r.result) && uint(r.jobs) && r.jobs > 0, `${label}: ${id} missing completed result`);
    return r;
  });
  return {timing: 'Sum of per-case elapsed lead + score ms; only one traversal per side (per-case median n=1).',
    equivalence: opening ? 'Compared first pending batch and measured scores/expanded receipts only; no completed result.' :
      'Compared pending batches, scores/expanded receipts and completed result. result_sha256 hashes JSON.stringify of the completed A result.',
    counts: 'builds = score-batch measurements (not unique builds); fights and work are those batches, per side, excluding any unreported leader/final work. bytes = requests + responses; carry = returned score receipt bytes, included in transport, not additive.',
    aggregate: strata(rows, xs => {
      const t = totals(xs, [...phases, ...counts]);
      return {cases: xs.length, ...t, ...Object.fromEntries(phases.map(k => [k, rounded(t[k])])), speedup: rounded(t.msA / t.msB)};
    }),
    cases: table(['id', ...phases, 'firstPending', ...counts, ...(opening ? [] : ['jobs', 'result_sha256'])],
      rows.map(r => [r.id, ...phases.map(k => rounded(r[k])), r.batch0, ...counts.map(k => r[k]), ...(opening ? [] : [r.jobs, r.result])]))};
}

function libraries(raw, current) {
  const entries = Object.entries(raw);
  check(entries.length === 2 && entries.every(([name]) => /^[a-zA-Z0-9_-]+$/.test(name)), 'expected two named DLLs');
  const kinds = entries.map(([, l]) => l.kind);
  if (current) check(kinds.every(k => k === 'embedded'), 'current-api: expected two embedded DLLs');
  else sameKeys('accepted DLL kinds', kinds, ['official', 'sfe']);
  return Object.fromEntries(entries.map(([name, l]) => {
    const kind = l.kind;
    check(digest(l.sha256), `${name}: invalid DLL hash`);
    const out = {kind, sha256: l.sha256, boardABI: l.board_abi ?? null};
    if (!current) {
      check(/^[0-9a-f]{12,40}$/.test(l.engine) && digest(l.data_sha256) && digest(l.manifest_sha256), `${name}: invalid release identity`);
      Object.assign(out, {engine: l.engine, dataSHA256: l.data_sha256, manifestSHA256: l.manifest_sha256});
    }
    if (kind === 'sfe') {
      check(l.board_abi === 1 && /^[0-9a-f]{40}$/.test(l.source) && /^[0-9a-f]{40}$/.test(l.wrapper), `${name}: invalid prepared ABI/revisions`);
      Object.assign(out, {source: l.source, wrapper: l.wrapper});
      if (l.pgo) {
        check(digest(l.pgo.profile_sha256) && digest(l.pgo.orders_sha256), `${name}: invalid PGO hashes`);
        out.profileSHA256 = l.pgo.profile_sha256;
        out.trainingOrdersSHA256 = l.pgo.orders_sha256;
      }
    }
    return [name, out];
  }));
}

function dll(name, raw, ids, context) {
  const current = name === 'current-api', prepared = name === 'accepted-prepared';
  check(raw.mode === (prepared ? 'prepared' : 'api') && raw.smoke === false && raw.check_only === false &&
    uint(raw.threads) && raw.threads > 0 && (!prepared || (uint(raw.batch) && raw.batch > 0)), `${name}: expected a timed full report`);
  check(digest(raw.fights_sha256) && raw.sample_sha256 === context.sampleSHA &&
    (context.fightsSHA ?? raw.fights_sha256) === raw.fights_sha256, `${name}: frozen corpus digest mismatch`);
  context.fightsSHA = raw.fights_sha256;
  const cases = index(`${name} cases`, raw.cases, ids);
  for (const [id, c] of cases) {
    check(c.start === 0 && c.count === runs && c.original_runs === 1000 && uint(c.seed) && digest(c.request_sha256), `${name}: ${id} run contract`);
    if (context.inputSeeds) check(c.seed === context.inputSeeds.get(id), `${name}: ${id} seed differs from fights.json`);
    const prior = context.requests.get(id);
    if (prior) check(prior.seed === c.seed && prior.request_sha256 === c.request_sha256, `${name}: ${id} seed/request mismatch`);
    else context.requests.set(id, c);
  }
  const libs = libraries(raw.libraries, current);
  if (!current) {
    const byKind = Object.fromEntries(Object.values(libs).map(l => [l.kind, l]));
    check(byKind.official.engine === byKind.sfe.engine && byKind.official.dataSHA256 === byKind.sfe.dataSHA256, `${name}: release engine/data mismatch`);
    if (prepared) for (const [kind, l] of Object.entries(byKind)) check(l.sha256 === context.acceptedLibs[kind].sha256 &&
      l.manifestSHA256 === context.acceptedLibs[kind].manifestSHA256, `${name}: ${kind} differs from accepted-api`);
    else context.acceptedLibs = byKind;
  }
  const variants = Object.keys(libs).sort().flatMap(k => prepared && libs[k].boardABI === 1 ?
    [`${k}:generic-row`, `${k}:prepared`] : [`${k}:${prepared ? 'generic-row' : 'api'}`]);
  for (const k of ['milliseconds', 'counters', 'results']) {
    keys(`${name} ${k}`, raw[k], variants);
    for (const v of variants) keys(`${name} ${k}/${v}`, raw[k][v], ids);
  }
  const timing = new Map(), work = new Map(), scores = new Map();
  let repeats;
  for (const id of ids) {
    const ms = {};
    for (const v of variants) {
      const t = raw.milliseconds[v][id], c = raw.counters[v][id], result = raw.results[v][id];
      check(Array.isArray(t) && t.length > 0 && t.every(positive), `${name}: ${v} ${id} no timed samples`);
      repeats ??= t.length;
      check(t.length === repeats, `${name}: ${v} ${id} repeat mismatch`);
      ms[v] = median(t) / runs;
      check(result.ok === true && c.runs === runs && uint(c.gas) && uint(c.work_from_gas) &&
        c.work_from_gas === Math.floor(c.gas / 20) + runs * 400000, `${name}: ${v} ${id} inconsistent gas/work`);
      if (prepared) {
        check(c.work === c.work_from_gas && result.work === c.work && result.counts.gas === c.gas &&
          result.counts.runs === runs && Number.isFinite(result.score) && /^[a-z0-9_]+$/.test(result.metric), `${name}: ${v} ${id} score/counts`);
        const prev = scores.get(id);
        if (prev) check(prev[0] === result.metric && Object.is(prev[1], result.score), `${name}: ${v} ${id} prepared metric/score differs`);
        scores.set(id, [result.metric, result.score]);
      } else {
        const sh = result.shards;
        check(Array.isArray(sh) && sh.length === runs && sh.every(s => s.runs === 1 && uint(s.gas)) && sum(sh, s => s.gas) === c.gas, `${name}: ${v} ${id} invalid shards`);
        if (current) {
          const f = context.fights.get(id);
          for (const [k, field] of [['gas','gas'], ['shots','shots'], ['ticks','dot_ticks'], ['procs','procs']]) {
            check(f[k] === sum(sh, s => s[field]) / runs, `WASM/current DLL: ${id} ${k} mismatch`);
          }
        }
      }
      const other = work.get(id) ?? (prepared ? context.acceptedWork.get(id) : null);
      if (other) check(other.gas === c.gas && other.work_from_gas === c.work_from_gas, `${name}: ${v} ${id} workload differs`);
      work.set(id, c);
    }
    timing.set(id, ms);
  }
  if (name === 'accepted-api') context.acceptedWork = work;
  check(Array.isArray(raw.passes), `${name}: missing passes`);
  sameKeys(`${name} passes`, raw.passes.map(p => `${p.variant}/${p.pass_index}`),
    variants.flatMap(v => Array.from({length: repeats}, (_, i) => `${v}/${i}`)));
  check(raw.passes.every(p => positive(p.wall_ms)), `${name}: invalid pass wall time`);

  const pair = (a, b) => [a, b].sort().join('|');
  const pairs = variants.flatMap((a, i) => variants.slice(i + 1).map(b => pair(a, b)));
  sameKeys(`${name} comparisons`, raw.comparisons.map(c => pair(c.A, c.B)), pairs);
  const comparisons = raw.comparisons.map(c => {
    const label = `${name} ${c.A} vs ${c.B}`;
    keys(`${label} (${c.verdict}) first_difference_by_case`, c.first_difference_by_case, []);
    check(c.verdict === 'exact-on-selected-cases' && c.required === true && c.known_engine_data_conflict === false, `${label}: ${c.verdict} or unverified comparison`);
    const paired = index(`${label} timings`, c.timings, ids);
    for (const [id, row] of paired) {
      const ms = timing.get(id);
      check(close(row.msA, ms[c.A]) && close(row.msB, ms[c.B]) && close(row.speedup, ms[c.A] / ms[c.B]), `${label}: ${id} reported medians/ratio disagree`);
    }
    const a = sum(ids, id => timing.get(id)[c.A]), b = sum(ids, id => timing.get(id)[c.B]);
    return {A: c.A, B: c.B, verdict: c.verdict, differingCases: 0, exactCases: ids.length,
      timingBasis: c.comparison_group ? 'matching engine/data group' : c.explicitly_required ? 'operator-required pair' : 'same DLL',
      msA: rounded(a), msB: rounded(b), speedup: rounded(a / b)};
  });
  let reference = null;
  if (raw.reference != null) {
    check(digest(raw.reference.sha256), `${name}: reference digest`);
    keys(`${name} references`, raw.reference.differences, variants);
    for (const v of variants) keys(`${name} reference differences/${v}`, raw.reference.differences[v], []);
    reference = {sha256: raw.reference.sha256, differingCasesPerVariant: 0};
  }
  check(typeof raw.host === 'string' && /^Windows-[a-zA-Z0-9.-]+$/.test(raw.host), `${name}: unexpected host format`);
  return {scope: prepared ? 'Short-row score, metric, work and counts; not a published board score. The opaque prepared ABI does not return individual shards.' : 'Full /api/board/runs shards.',
    timing: 'Sum of per-case median latency / three indexed fights. A timed measurement is one case, not one ABI call. Work/gas refer to one logical pass; preflight/warmup excluded.',
    os: raw.host, threads: raw.threads, span: prepared ? raw.batch : null, repeats, libraries: libs, comparisons, reference,
    aggregate: strata(ids.map(id => ({id})), xs => ({cases: xs.length, fightsPerPass: xs.length * runs,
      timedCaseMeasurementsPerVariant: xs.length * repeats, timedFightsPerVariant: xs.length * repeats * runs,
      gasPerPass: sum(xs, r => work.get(r.id).gas), workPerPass: sum(xs, r => work.get(r.id).work_from_gas),
      msByVariant: Object.fromEntries(variants.map(v => [v, rounded(sum(xs, r => timing.get(r.id)[v]))]))}), true),
    cases: table(['id', ...variants.map(v => `ms/fight ${v}`), 'gas/three fights', 'work/three fights', ...(prepared ? ['short_row_metric', 'short_row_score'] : [])],
      ids.map(id => [id, ...variants.map(v => rounded(timing.get(id)[v])), work.get(id).gas, work.get(id).work_from_gas, ...(scores.get(id) ?? [])]))};
}

function inputPath(dir, name) {
  return name.startsWith('site/pkg/') ? path.join(pkg, name.slice(9)) : path.join(dir, name);
}

export function summarize(dir) {
  const missing = files.filter(n => !fs.existsSync(inputPath(dir, n)));
  check(!missing.length, `missing required evidence: ${missing.join(', ')}`);
  const sourceSHA256 = {};
  function read(name) {
    let bytes;
    try { bytes = fs.readFileSync(inputPath(dir, name)); } catch { throw new Error(`cannot read ${name}`); }
    sourceSHA256[name] = sha(bytes);
    try { return JSON.parse(bytes.toString('utf8').replace(/^\uFEFF/, '')); } catch { throw new Error(`invalid or incomplete JSON: ${name}`); }
  }
  const sample = population(read);
  const scopes = sample.weapons.flatMap(w => geometries.map(g => `${w.id}:${g}`));
  const ids = scopes.flatMap(s => aim.map(h => `${s}:${h}`));
  const fightData = read('final-fights.json');
  const fight = fights(fightData, ids);
  const fleet = search(read('final-fleet.json'), scopes.map(s => `${s}:search`), false);
  const opening = search(read('final-opening.json'), scopes.map(s => `${s}:lead`), true);
  const context = {sampleSHA: sourceSHA256['sample.json'], requests: new Map(), fights: new Map(fightData.map(r => [r.id, r]))};
  if (fs.existsSync(inputPath(dir, 'fights.json'))) {
    const inputs = index('fights.json', read('fights.json'), ids);
    context.inputSeeds = new Map();
    for (const [id, c] of inputs) {
      const [w, g, h] = id.split(':'), {request, from, count} = c.body;
      check(c.path === '/api/board/runs' && request.weapon === w && request.headshot_pct === Number(h) &&
        (request.formation ?? []).length === (g === 'multi' ? 24 : 0) && from === 0 && count === runs && request.runs === 1000,
      `fights.json: ${id} does not match its frozen stratum`);
      context.inputSeeds.set(id, request.seed);
    }
    context.fightsSHA = sourceSHA256['fights.json'];
  }
  const dllLab = {};
  for (const name of ['current-api', 'accepted-api', 'accepted-prepared']) {
    dllLab[name] = dll(name, read(`dll-lab/${name}.json`), ids, context);
  }
  const seeds = [...new Set([...context.requests.values()].map(c => c.seed))];
  check(seeds.length === 1, 'unexpected mixed seeds in frozen corpus');
  // Attach request attestations by ID, never by DLL report order.
  fight.cases.columns.push('request_sha256');
  for (const row of fight.cases.rows) row.push(context.requests.get(row[0]).request_sha256);
  for (const [group, t] of Object.entries(fight.aggregate)) {
    const dllGroup = dllLab['current-api'].aggregate[group];
    t.gasPerPass = dllGroup.gasPerPass;
    t.workPerPass = dllGroup.workPerPass;
  }
  const optional = {}, has = n => fs.existsSync(inputPath(dir,n));
  if (has('whole.json')) {
    const whole = read('whole.json');
    for (const r of whole) check(digest(r.hash) && r.times.A.length === 2 && r.times.B.length === 2 &&
      close(median(r.times.A),r.msA) && close(median(r.times.B),r.msB) && r.branches === 5 && r.jobs > 0,
      `invalid completed whole search: ${r.id}`);
    optional.wholeSearch = whole;
  }
  if (has('chrome-A2.json')) {
    const trials = ['A1','B1','B2','A2'].map(tag => read(`chrome-${tag}.json`));
    const expected = trials[0].map(r=>r.id);
    const maps = trials.map((t,i)=>index(`Chrome trial ${i}`,t,expected));
    optional.chromeMs = table(['id','A1','B1','B2','A2','finalists','screen_fights'], expected.map(id=>{
      const rows=maps.map(m=>m.get(id)), {cpu_ms,...result}=rows[0].result;
      for (const row of rows) {const {cpu_ms,...rest}=row.result;check(row.ms>0 && rest.ok && JSON.stringify(rest)===JSON.stringify(result),`Chrome result differs: ${id}`);}
      return [id,...rows.map(r=>rounded(r.ms)),result.jobs,result.fights];
    }));
  }
  if (has('dll-lab/tuned-current.json')) {
    const raw = read('dll-lab/tuned-current.json'), labels = ['baseline:api','generic:api','tuned:api'];
    const requests = index('tuned DLL cases',raw.cases,ids);
    const tunedPairs=raw.comparisons.filter(c=>[c.A,c.B].includes('tuned:api'));
    check(raw.threads === 1 && raw.comparisons.length === 3 && tunedPairs.length === 2 &&
      tunedPairs.every(c=>c.explicitly_required && c.verdict==='exact-on-selected-cases') &&
      raw.comparisons.every(c=>!c.known_engine_data_conflict && !Object.keys(c.first_difference_by_case).length), 'DLL qualification mismatch');
    for (const label of labels) {
      check(raw.reference && Object.keys(raw.reference.differences[label]).length === 0,`WASM/PGO mismatch: ${label}`);
      keys(label,raw.milliseconds[label],ids);
    }
    optional.nativeBuilds = Object.fromEntries(labels.map(l => [l,raw.libraries[l.split(':')[0]].sha256]));
    optional.nativeMsPerFight = table(['id',...labels],ids.map(id=>{
      check(requests.get(id).request_sha256===context.requests.get(id).request_sha256,`DLL workload: ${id}`);
      const ms=labels.map(l=>raw.milliseconds[l][id]);
      check(ms.every(t=>t.length===4 && t.every(positive)),`DLL timing: ${id}`);
      return [id,...ms.map(t=>rounded(median(t)/runs))];
    }));
  }
  const searchWork = totals([fleet.aggregate.all, opening.aggregate.all], ['builds', 'fights', 'work', ...traffic]);
  return {schema: 'wfsim-popular-perf/v1', sourceSHA256,
    corpus: {weapons: sample.weapons.length, geometries: {single: 1, multi: 25}, headshotPercent: aim,
      seed: seeds[0], firstRun: 0, fightsPerCell: runs, originalRequestRuns: 1000, frozenFightInputsSHA256: context.fightsSHA},
    population: sample,
    totals: {wasmFight: {cells: ids.length, fightsPerPass: fight.aggregate.all.fightsPerPass,
      timedCallMeasurementsBothSides: 2 * fight.aggregate.all.timedCallsPerSide, timedFightsBothSides: 2 * fight.aggregate.all.timedFightsPerSide},
    wasmFleetPlusOpening: {traversalsPerSide: scopes.length * 2, candidateMeasurementsPerSide: searchWork.builds,
      fightsPerSide: searchWork.fights, workPerSide: searchWork.work, transportBytesA: searchWork.bytesA,
      transportBytesB: searchWork.bytesB, returnedReceiptBytesA: searchWork.carryA, returnedReceiptBytesB: searchWork.carryB},
    note: 'The opening is a separate partial workload, not additional work inside the completed fleet search. DLL tracks repeat the same corpus; their work is reported per variant/pass and not added to WASM.'},
    wasm: {fight, fleet, opening}, dllLab, ...optional};
}

// Keep the named sections legible and one machine-readable table row per line.
function pretty(value, depth = 0) {
  if (value === null || typeof value !== 'object') return JSON.stringify(value);
  const array = Array.isArray(value), pad = '  '.repeat(depth);
  if (array && value.every(x => x === null || typeof x !== 'object')) return JSON.stringify(value);
  const entries = array ? value.map(x => pretty(x, depth + 1)) : Object.entries(value).map(([k, x]) => `${JSON.stringify(k)}: ${pretty(x, depth + 1)}`);
  if (!entries.length) return array ? '[]' : '{}';
  return `${array ? '[' : '{'}\n${pad}  ${entries.join(`,\n${pad}  `)}\n${pad}${array ? ']' : '}'}`;
}

function main(args) {
  check(args.length === 2 && (args[1] === '-' || path.extname(args[1]).toLowerCase() === '.json'),
    'usage: node scripts/summarize_popular_perf.mjs <evidence-dir> <compact.json | ->');
  const [dir, output] = args, report = summarize(dir), text = pretty(report) + '\n';
  if (output === '-') return process.stdout.write(text);
  if (fs.existsSync(output)) {
    const target = fs.realpathSync.native(output);
    check(!Object.keys(report.sourceSHA256).some(n => path.relative(target, fs.realpathSync.native(inputPath(dir, n))) === ''),
      'refusing to overwrite a source file');
  }
  fs.mkdirSync(path.dirname(output), {recursive: true});
  const tmp = output + `.${randomUUID()}.tmp`;
  try {
    fs.writeFileSync(tmp, text, {flag: 'wx'});
    fs.renameSync(tmp, output);
  } finally {
    if (fs.existsSync(tmp)) fs.unlinkSync(tmp);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  try { main(process.argv.slice(2)); }
  catch (err) { console.error(err.message); process.exitCode = 1; }
}
