// WFSim build configurator — PURE CONFIG. Modules: Mods / Arcane / Evolution /
// Element; each weapon enables only the ones it has. Data from /api/meta;
// official polarity icons from the wiki, art from WFCD.

const $ = (id) => document.getElementById(id);
// THE dropdown's registry. Declared here rather than beside the rest of the
// component because the topbar's language control is drawn by an IIFE that
// runs at load — a `const` further down the file is in its temporal dead zone
// at that moment, which is a ReferenceError rather than an ordering nit.
const DD_SEARCH_MIN = 6;
const ddReg = new Map();
// Transport mode (docs/WASM.md phase 4): the static wasm deployment's
// index.html sets window.WFSIM_WASM = true — then a Web Worker owns the
// wasm engine and api() below is worker RPC. Unset (the native server),
// api() is plain fetch.
const WASM = !!window.WFSIM_WASM;
// Art: on the native server through our OWN origin (img/ proxy: local disk
// cache, WFCD fallback) — fast, offline-capable, one source. The static
// deployment has no proxy: straight to the WFCD CDN (wiki Special:FilePath
// for the paren-named evolution icons).
// Art is SAME-ORIGIN in both deployments: the native server reads
// web/cache/img/, and the static build ships the same files in site/img/
// (build_site_app.py `ship_art`). It used to hotlink the CDN on the static
// build, and that CDN 301s to raw.githubusercontent.com — unreliable to
// blocked from mainland China, which is where the players are.
//
// `wiki:` (data/assets.yaml) marks a file the CDN does not carry; it changes
// where the BUILD fetches from, not where the page asks — the cache holds it
// under its bare name like everything else.
const IMG = (name) => {
  if (!name) return null;
  const s = String(name);
  return "/img/" + encodeURIComponent(s.startsWith("wiki:") ? s.slice(5) : s);
};
// Polarity icons are vendored locally (pol/, shipped with both deployments)
// — no more slow wiki 302 redirects. Omni (universal) uses the "Any" symbol
// (a PNG); the rest are SVGs. Relative path: the app page sits at "/"
// natively and at "/app/" on the static deployment.
const POL = (p) => `/pol/${p === "Omni" ? "Any" : p}_Pol.${p === "Omni" ? "png" : "svg"}`;

// ---- i18n --------------------------------------------------------------
// English is the SOURCE (each entity's own name / the literal UI strings);
// other languages are overlays. UI strings: tr() over the catalog below.
// Game-entity names: LN() over /api/i18n (data/i18n/<locale>.yaml — ids are
// never translated; missing entries fall back to English).
// The language on a FIRST visit is the browser's, not English (user,
// 2026-07-31). Every zh-* maps to our one Chinese: a reader of Traditional
// is closer to Simplified than to English, and the choice is one click away
// either way.
//
// The detected value is deliberately NOT written to storage. The key holds a
// CHOICE and nothing else, so "never picked" stays distinguishable from
// "picked English" — a visitor who lands in the wrong language once and
// fixes it is remembered, and one who never touches it follows their
// browser if they later switch systems. Writing the guess would make those
// two states the same and freeze the guess forever.
const LOCALES = ["en", "zh"];
function detectLang() {
  const want = navigator.languages && navigator.languages.length
    ? navigator.languages : [navigator.language || "en"];
  for (const raw of want) {
    const tag = String(raw).toLowerCase();
    const hit = LOCALES.find((l) => l !== "en" && (tag === l || tag.startsWith(l + "-")));
    if (hit) return hit;
    if (tag === "en" || tag.startsWith("en-")) return "en";
  }
  return "en";
}
let LANG = localStorage.getItem("wfsim-lang") || detectLang();
let I18N = null; // active locale's name overlay, fetched in init()
// EVERY OTHER LOCALE'S NAMES, for SEARCH ONLY — never for display.
//
// The English page had no Chinese in it at all, so a player typing 私法 got "no
// matches" while the mod sat in the list one row down (group report,
// 2026-08-12). The reverse already worked, because a localized overlay keeps
// `name_en` beside the translated name; only English lacked the other side.
//
// `/api/i18n` already returns every locale in one response, so this costs one
// request on the English page and nothing anywhere else.
let ALT_NAMES = null;
// UI strings and effect phrases live in data/i18n/<locale>.yaml (served at
// /api/i18n) — nothing hardcoded here. English needs no catalog: the source
// string is the fallback.
const tr = (s) => (I18N && I18N.ui && I18N.ui[s]) || s;
const LN = (table, id, en) => (I18N && I18N[table] && I18N[table][id]) || en;
// A damage type's NAME. The English fallback is CAPITALISED rather than echoed:
// callers arrive with either spelling — the server sends "Void" in a damage
// meter row and a yaml token is "void" — and echoing put a lowercase "void" on
// the English buff card while the Chinese one read 虚空 (2026-08-09). One helper,
// one answer, whichever spelling reaches it.
const DT = (ty) => {
  const k = String(ty).toLowerCase();
  return LN("damage_types", k, k.charAt(0).toUpperCase() + k.slice(1));
};
// A damage type's OFFICIAL colour and icon — DE's own, transcribed from the
// wiki's `Module:DamageTypes/data` (see style.css for the palette and
// data/assets.yaml for the files).
//
// Keyed on the TYPE and never on a row's position: the meter used to colour by
// index, so Heat was one colour under a direct hit and another under a field
// (owner, 2026-08-06). `null` for anything that is not a damage type — a
// source row like "Direct hits" is not one, and asking for its colour should
// return nothing rather than a wrong one.
const DT_TYPES = new Set(["impact", "puncture", "slash", "cold", "electricity",
  "heat", "toxin", "blast", "corrosive", "gas", "magnetic", "radiation",
  "viral", "true", "void", "tau"]);
const dtKey = (ty) => {
  const k = String(ty || "").toLowerCase();
  return DT_TYPES.has(k) ? k : null;
};
const dtColor = (ty) => (dtKey(ty) ? `var(--dt-${dtKey(ty)})` : null);
const dtIcon = (ty) => {
  const k = dtKey(ty);
  if (!k) return "";
  const file = (META.damage_type_icons || {})[k];
  return file ? `<img class="dt-ico" src="${IMG(file)}" alt="" loading="lazy">` : "";
};
// Effect-line phrase substitution ("+X% Critical Chance" → "+X% 暴击几率"):
// the ORDERED [regex, replacement(, flags)] table comes from the locale's
// effect_phrases (data/i18n). Compiled once on first use.
let EFFECT_RES = null;
const tf = (x) => {
  if (!I18N || typeof x !== "string") return x;
  if (!EFFECT_RES) {
    EFFECT_RES = (I18N.effect_phrases || []).flatMap(([pat, cn, flags]) => {
      try { return [[new RegExp(pat, flags || "gi"), cn]]; } catch (_) { return []; }
    });
  }
  let s = x;
  for (const [re, cn] of EFFECT_RES) s = s.replace(re, cn);
  return s;
};
// One lowercase search haystack per entity: the LOCALIZED name, the
// ENGLISH name, the English effect lines AND their translated phrases —
// every search box matches in English and in the active language alike,
// names and effects both (user, 2026-07-29). Cached on the entity (names
// and overlay are fixed for the page's lifetime).
const searchBlob = (x) => {
  if (x._search) return x._search;
  const eff = (x.effects || []).concat(x.desc_ranks || [], (x.ranks || []).flat());
  // …plus DE's own card text when the locale has it, so a search for 弓类 or
  // 加倍 hits the mod whose card says it (the phrase table never produced
  // those words — see officialDesc).
  const official = (I18N && ((I18N.mod_descriptions || {})[x.id] || (I18N.arcane_descriptions || {})[x.id])) || [];
  // …plus the name every OTHER locale gives it, so a search works whichever
  // language the page happens to be in. Display never reads these.
  // …every OTHER locale's name, plus the ACTIVE one looked up by id. The second
  // half matters for objects the display overlay never touched — `META.mod_pools`
  // holds segment pools the picker does not build from, and searching those for
  // a localized name found nothing because the name was never written onto them.
  const byId = (tbl) => Object.values(tbl || {})
    .map((m) => m && m[x.id]).filter((s) => typeof s === "string");
  const alt = (ALT_NAMES || []).flatMap(byId).concat(I18N ? byId(I18N) : []);
  x._search = [x.name, x.name_en, x.subtype, eff.join(" "), tf(eff.join(" ")),
    official.join(" "), alt.join(" ")]
    .filter(Boolean).join(" ").toLowerCase();
  x._searchTight = squash(x._search);
  return x._search;
};
// SPACES ARE NOT PART OF THE WORD. DE's Chinese names carry one — "私法 军备",
// "野猪 Prime", "布尔斯顿 (虚坏形态)" — and 181 of the 516 names in
// `data/i18n/zh/names.yaml` do, so a player typing the name the way it reads
// (私法军备) found NOTHING while the mod sat in the list (group report,
// 2026-08-12). The name is transcribed correctly and stays as DE writes it;
// what was wrong is asking the query to reproduce a space nobody says.
//
// Squashing BOTH sides is a superset of the old match, never a subset: two
// strings that matched with their spaces still match without them.
const squash = (s) => String(s || "").replace(/\s+/g, "");
// One predicate for every list that filters by a searchable blob.
const searchHit = (x, q) => {
  if (!q) return true;
  const blob = searchBlob(x);
  return blob.includes(q) || (x._searchTight || squash(blob)).includes(squash(q));
};
// Static labels: translate the first text node of every [data-i18n] element
// (children like the .sim-hint spans stay untouched), and the placeholder of
// every [data-i18n-ph] input — a search box's prompt is a UI string like any
// other, and it was the one kind the sweep did not reach.
function applyI18n() {
  // A translated line with ONE word picked out — the hero's "Prime", gold
  // because that is the game's own colour for a Prime item. The whole
  // sentence stays a single key: the marked word is Latin in every language,
  // so it can be found after translation instead of being carved out of the
  // source into a key of its own.
  document.querySelectorAll("[data-i18n-gold]").forEach((el) => {
    if (!el.dataset.i18nSrc) el.dataset.i18nSrc = el.textContent.trim();
    const word = el.dataset.i18nGold;
    el.innerHTML = escHtml(tr(el.dataset.i18nSrc))
      .split(word)
      .join(`<span>${escHtml(word)}</span>`);
  });
  document.querySelectorAll("[data-i18n-ph]").forEach((el) => {
    if (!el.dataset.i18nPhSrc) el.dataset.i18nPhSrc = el.placeholder;
    el.placeholder = tr(el.dataset.i18nPhSrc);
  });
  // …and the tooltip of every [data-i18n-title]. A hover hint is a UI string
  // like any other; it was simply the kind nothing reached, so a fully
  // translated page still explained itself in English on hover.
  document.querySelectorAll("[data-i18n-title]").forEach((el) => {
    if (!el.dataset.i18nTitleSrc) el.dataset.i18nTitleSrc = el.title;
    el.title = tr(el.dataset.i18nTitleSrc);
  });
  document.querySelectorAll("[data-i18n]").forEach((el) => {
    const node = [...el.childNodes].find((n) => n.nodeType === 3 && n.textContent.trim());
    if (!node) return;
    if (!el.dataset.i18nSrc) el.dataset.i18nSrc = node.textContent.trim();
    node.textContent = node.textContent.replace(el.dataset.i18nSrc, tr(el.dataset.i18nSrc)) || tr(el.dataset.i18nSrc);
  });
}
// Mutate META once: every downstream renderer (home grid, selects, pickers,
// optimizer lists) shows overlay names with zero per-site changes. EVERY
// entity keeps its English name as name_en — URLs/wiki links always build
// from it (the wiki is English; a localized name in the URL 404s or lands
// on oddities — user, 2026-07-29).
function applyNameOverlay() {
  if (!I18N) return;
  const over = (x, table) => { x.name_en = x.name; x.name = LN(table, x.id, x.name); };
  for (const w of META.weapons || []) {
    over(w, "weapons");
    for (const t of w.evolutions || []) for (const o of t.options || []) over(o, "evolutions");
  }
  for (const e of META.enemies || []) over(e, "enemies");
  for (const pool of Object.values(META.mod_pools || {})) for (const m of pool) over(m, "mods");
  for (const a of META.arcanes || []) over(a, "arcanes");
}
// Polarities available on GUN slots. Zenurik/Unairu/Penjaga are Warframe-augment
// / melee-stance / companion-ability polarities — not gun slots. "Omni" is the
// Omni Forma universal polarity (matches any mod EXCEPT Umbra mods).
const GUN_POLS = ["Madurai", "Naramon", "Vazarin", "Umbra", "Omni"];
// WHAT THIS WEAPON HAS TO SPEND. Not a constant: an ADVERSARY weapon
// (Kuva/Tenet/Coda) ranks to 40 rather than 30 and finishes at 80, and the
// server sends the finished number per weapon (`/api/meta`) rather than the
// ladder, so this file holds no capacity arithmetic of its own.
//
// 60 is the fallback and nothing more — every weapon in the roster answers.
const capOf = (id) => (weaponInfo(id) || {}).capacity || 60;
// The polarizations the weapon owes its own rank ceiling: FIVE on an adversary
// weapon, none on anyone else. It is a mastery figure, not a capacity one — a
// build that fits in three still pays it, because those three do not put the
// weapon at rank 40 and rank 40 is what the 80 above assumes.
const formaMin = (id) => (weaponInfo(id) || {}).forma_min || 0;
const imgTag = (src, cls) => src ? `<img class="${cls||''}" src="${src}" onerror="this.style.visibility='hidden'"/>` : `<span class="${cls||''}"></span>`;

// ---- transport: api(path, body) --------------------------------------
// Every endpoint call goes through here. Native: fetch to the local server.
// Wasm: a Web Worker owns the engine (worker.js + pkg/); quick endpoints are
// worker RPC, and the optimize start/status/cancel triad is emulated against
// a DEDICATED optimize worker — its progress messages fill the same status
// shape the poller renders, so the progress UI is unchanged. Cancelling a
// busy single-threaded worker is impossible from the outside: cancel =
// terminate that worker (all state lives inside it).
let rpcWorker = null, rpcId = 0;
const rpcPending = new Map();
function ensureRpcWorker() {
  if (rpcWorker) return rpcWorker;
  rpcWorker = new Worker("/worker.js");
  rpcWorker.onmessage = (e) => {
    const p = rpcPending.get(e.data.id);
    if (p) { rpcPending.delete(e.data.id); p(e.data.payload); }
  };
  return rpcWorker;
}
const rpc = (path, body) => new Promise((resolve) => {
  const id = ++rpcId;
  rpcPending.set(id, resolve);
  ensureRpcWorker().postMessage({ id, kind: "api", path, body: body ?? {} });
});

let wopt = null; // the emulated optimize job: a FLEET of workers over disjoint
                 // strides — { id, workers[], statuses[], parts[], result, … }
let woptNextId = 1;
// ---- checkpoint / resume -------------------------------------------------
// A reload KILLS the worker, and there is no browser mechanism that avoids it
// (measured 2026-07-30: a SharedWorker is terminated too, the moment its last
// client disconnects, busy or idle). So instead of pretending a run can
// survive, make losing it cheap: the worker emits the surviving field after
// every completed round, and a fresh page rebuilds from the last one.
//
// Stored as IDENTITIES only — (mod pool indices, evo set, exilus, arcane) — so
// it fits localStorage and cannot drift from what the engine would rebuild.
// The REQUEST is stored with it and is what a resume replays: a checkpoint
// only ever means anything under the scope that produced it, so it is never
// re-derived from whatever the form happens to say later.
const OPT_CKPT = "wfsim-optimize-checkpoint";
function saveCheckpoint(body, cp, board) {
  const write = (payload) => localStorage.setItem(OPT_CKPT, JSON.stringify(payload));
  try {
    write({ body, cp, board, at: Date.now() });
  } catch (_) {
    // A screen cut is the whole surviving field — tens of thousands of pairs.
    // If it does not fit alongside the leaderboard, the RESUME POINT is worth
    // more than the display copy, so drop the board and keep the cut.
    try { write({ body, cp, at: Date.now() }); } catch (_) { /* nothing fits: no checkpoint */ }
  }
}
const clearCheckpoint = () => { try { localStorage.removeItem(OPT_CKPT); } catch (_) {} };
function loadCheckpoint() {
  try {
    const s = JSON.parse(localStorage.getItem(OPT_CKPT));
    if (!s || !s.body || !s.cp) return null;
    // A day-old checkpoint is almost certainly not what the visitor meant to
    // resume, and the data behind it may have been rebuilt since.
    if (Date.now() - (s.at || 0) > 24 * 3600 * 1000) { clearCheckpoint(); return null; }
    return s;
  } catch (_) { return null; }
}

// HOW MANY WORKERS the browser search runs on. This is the only lever the
// browser has: it is single-threaded at ~150 simulated engagements per second
// against ~5,100 on a 26-thread desktop, so coverage is scarcest exactly where
// there is least compute. N workers walk DISJOINT STRIDES of the shuffled
// index range (`shard`, `shard + shards`, …), which is a partition — nothing
// is evaluated twice and nothing is missed (`shards_partition_the_shuffled_
// order_exactly`). Each also climbs on its own, so N workers are also N
// independent hill-climbs, which is the diversity one best-first climb lacks.
//
// The count is the search preset's own `CPU threads` — the setting already
// existed and meant this on the native server; it now means it here too.
// Blank = every core but one, capped at 8: past that the strides get short,
// the wasm instances get expensive (one 2.3 MB module each) and a phone
// starts swapping.
function woptWorkerCount() {
  if (optRun.threads > 0) return Math.min(optRun.threads, 16);
  const cores = Number(navigator.hardwareConcurrency) || 4;
  return Math.max(1, Math.min(cores - 1, 8));
}

// Merge the fleet's leaderboards into one. Each worker ran its own funnel over
// its own elites, so every row here is measured at the SAME run count under the
// SAME scenario and the scores are directly comparable — the merge is a sort.
//
// Deduplicate first: strides are disjoint but the CLIMB is not, so two workers
// can reach the same build from different samples. Counting it twice would
// push a real alternative off the board.
function woptMerge(parts) {
  const bad = parts.find((p) => p && p.ok === false);
  if (bad) return bad;
  // A shard that owned no ground returns an empty but complete envelope, so
  // any part is a valid head — prefer one that actually ranked something.
  const rows = [];
  const seen = new Set();
  for (const p of parts) {
    for (const r of p.results || []) {
      const key = JSON.stringify([r.mods, r.arcane, r.evolutions, r.exilus]);
      if (seen.has(key)) continue;
      seen.add(key);
      rows.push(r);
    }
  }
  rows.sort((a, b) => (b.kill_progress ?? b.kills ?? 0) - (a.kill_progress ?? a.kills ?? 0));
  const head = parts.find((p) => (p.results || []).length) || parts[0] || {};
  const finalists = head.finalists || rows.length;
  const space = head.space || 0;
  const sampled = parts.reduce((n, p) => n + (p.sampled || 0), 0);
  return {
    ...head,
    // EVERY shard must have finished its stride for the union to be the space.
    exhaustive: parts.length > 0 && parts.every((p) => p.exhaustive),
    // Coverage of the FLEET, not of one worker: the strides are disjoint, so
    // the positions add up.
    coverage: space > 0 ? Math.min(1, sampled / space) : 0,
    sampled,
    searched: parts.reduce((n, p) => n + (p.searched || 0), 0),
    candidates: parts.reduce((n, p) => n + (p.candidates || 0), 0),
    jobs: parts.reduce((n, p) => n + (p.jobs || 0), 0),
    cancelled: parts.some((p) => p.cancelled),
    results: rows.slice(0, finalists).map((r, i) => ({ ...r, rank: i + 1 })),
  };
}

function woptStart(body, checkpoint) {
  if (wopt && wopt.workers && wopt.workers.length) {
    return { ok: false, error: "an optimization is already running — cancel it or wait", job_id: wopt.id };
  }
  const { __resume, ...req } = body ?? {}; // the resume marker is transport, not scope
  body = req;
  // A CHECKPOINT is one worker's field, so it can only resume a run that had
  // one worker. Rather than resume a fraction of a fleet, a resume runs
  // unsharded — slower, but it is continuing a search that already exists.
  const shards = checkpoint ? 1 : woptWorkerCount();
  const job = {
    id: woptNextId++, workers: [], status: null, statuses: new Array(shards).fill(null),
    parts: new Array(shards).fill(null), result: null, board: null, boards: new Array(shards).fill(null),
    cancelled: false, shards, t0: Date.now(),
  };
  // One STATUS out of many: the fleet's progress is the sum of its workers,
  // and the phase is the least advanced of them — a run is still searching
  // while any worker still is.
  const rollup = () => {
    const live = job.statuses.filter(Boolean);
    if (!live.length) return null;
    const sum = (k) => live.reduce((n, s) => n + (Number(s[k]) || 0), 0);
    const searching = live.some((s) => s.phase === "searching") || live.length < shards;
    return {
      ...live[0],
      phase: searching ? "searching" : "running",
      sims_done: sum("sims_done"), sims_planned: sum("sims_planned"),
      enumerated: sum("enumerated"),
      round_jobs: sum("round_jobs"),
      workers: shards, workers_done: job.parts.filter(Boolean).length,
    };
  };
  for (let i = 0; i < shards; i++) {
    const w = new Worker("/worker.js");
    job.workers.push(w);
    w.onmessage = (e) => {
      if (e.data.kind === "progress") { job.statuses[i] = e.data.payload; job.status = rollup(); }
      if (e.data.kind === "board") { job.boards[i] = e.data.payload; job.board = woptMerge(job.boards.filter(Boolean)); }
      if (e.data.kind === "checkpoint") {
        const { board, ...cp } = e.data.payload;
        if (board) { job.boards[i] = board; job.board = woptMerge(job.boards.filter(Boolean)); }
        // Only an UNSHARDED run can be resumed from a checkpoint — see above.
        if (shards === 1) saveCheckpoint(body, cp, board || job.board);
      }
      if (e.data.kind === "result") {
        job.parts[i] = e.data.payload;
        w.terminate();
        job.workers[i] = null;
        if (job.parts.every(Boolean)) {
          job.result = woptMerge(job.parts);
          clearCheckpoint();
          job.workers = [];
        }
      }
    };
    w.onerror = (e) => {
      job.parts[i] = { ok: false, error: String((e && e.message) || "worker error") };
      if (job.workers[i]) { job.workers[i].terminate(); job.workers[i] = null; }
      if (job.parts.every(Boolean)) { job.result = woptMerge(job.parts); job.workers = []; }
    };
    w.postMessage({
      kind: "optimize",
      body: { ...body, shard: i, shards },
      checkpoint: checkpoint || null,
    });
  }
  wopt = job;
  return { ok: true, job_id: job.id };
}
function woptStatus() {
  if (!wopt) return { ok: false, error: "no such optimize job" };
  const st = wopt.status || { round: 0, rounds: 0, round_jobs: 0, round_runs: 0, sims_done: 0, sims_planned: 0, notes: [] };
  const out = { ...st, ok: true, job_id: wopt.id, elapsed_s: (Date.now() - wopt.t0) / 1000,
    // The worker's heartbeat carries its own phase (enumerating/running) —
    // keep it; the fallbacks cover the moments before the first message.
    phase: (wopt.status && wopt.status.phase) || (wopt.status ? "running" : "enumerating") };
  if (wopt.cancelled) {
    out.phase = "cancelled";
    // Cancel KILLED the worker, so there is no returned result and never will
    // be — hand back the last best-so-far it pushed out instead. It is already
    // result-shaped and flagged `cancelled`, so the normal renderer labels it
    // lower-precision without knowing where it came from.
    if (!wopt.result && wopt.board) out.result = wopt.board;
  }
  if (wopt.result) {
    out.result = wopt.result;
    out.phase = wopt.result.ok === false ? "error" : (wopt.result.cancelled ? "cancelled" : "done");
  }
  return out;
}
function woptCancel() {
  if (!wopt) return { ok: false, error: "no such optimize job" };
  // Cancel kills the WHOLE fleet. Each worker's last pushed board is already
  // here and merged, which is what a cancel has to show.
  if (wopt.workers && wopt.workers.length) {
    wopt.workers.forEach((w) => w && w.terminate());
    wopt.workers = [];
    wopt.cancelled = true;
  }
  return { ok: true, job_id: wopt.id };
}

async function api(path, body) {
  if (!WASM) {
    // GET endpoints (the rest are POST-with-body):
    if (path === "/api/meta" || path === "/api/i18n") return (await fetch(path)).json();
    return (await fetch(path, {
      method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body ?? {}),
    })).json();
  }
  if (path === "/api/optimize") return woptStart(body, body && body.__resume);
  if (path === "/api/optimize/status") return woptStatus();
  if (path === "/api/optimize/cancel") return woptCancel();
  return rpc(path, body);
}

let META = null;
// 9 × { mod:id|null, pol:string|null, rank:int|null } — POSITIONAL.
// Indices 0–7 are the regular slots; index 8 is the EXILUS slot (utility mods
// only; drain counts toward capacity like any slot; absent on sentinels).
const EXILUS = 8;
let slots = [];
// 9 × innate polarity name|null — index 8 is the EXILUS slot, which HAS one on
// most weapons (wiki "Exilus Polarity"). It used to be documented here as
// "exilus never innate", and the loader sliced it off to match.
let innate = [];
// ONE ENTRY PER ARCANE POOL the weapon seats, in the weapon's own pool
// order. Almost always a single entry; an Arch-Gun seats two — one Primary
// and one Secondary (wiki Arch-Gun) — and a sentinel seats none.
//
// A weapon never seats two of the SAME pool: slot i draws from pool i, so
// two Primary arcanes is not a build the page can express.
let arcanes = ["none"];
let arcaneRanks = [null];   // null → max rank (mirrors mod slot ranks)
// Pad or trim to the weapon's pool count. Storage is migrated to the list
// shape on load (`migrateArcaneShape`), so a bare value only ever arrives
// from something hand-written — and it is read the obvious way rather than
// silently dropped.
const asArcaneList = (v, n) => {
  const a = Array.isArray(v) ? v.slice() : v == null ? [] : [v];
  while (a.length < n) a.push(undefined);
  return a.slice(0, n);
};
// Per-tier evolution selection {tier: id|null}; null = EMPTY (nothing
// installed at that tier). Tier 1 is the Incarnon Form unlock: empty there
// means no transformation, so the panel falls back to the base form.
// Overwritten by META.defaults on init.
let evoSel = { 1: null, 2: null, 3: null, 4: null };
// HOW THIS BUILD IS PLAYED — part of the BUILD, not of the fight.
//
// "Torid, played through its cycle" is the thing a board ranks, and the entry
// it ranks is `weapon + mode + mods + evolutions + arcanes` — mode is inside
// that list, not beside it. A build preset is exactly what gets submitted and
// shared, so a mode kept anywhere else would have to be fetched from the fight
// at submission time, which is the coupling the board just shed.
//
// It is NOT "installed" like a mod — you own one Torid and play it both ways,
// switching mid-engagement for free. What it is part of is the SUBJECT of a
// measurement, which is what a build is (owner, 2026-08-07).
let mode = "base";

/// AN ADVERSARY WEAPON'S VALENCE BONUS — the element it came out of a Lich with
/// and how big the roll was. Part of the BUILD, because it is a property of the
/// COPY a player owns rather than of the model: two Kuva Nukors are two
/// different weapons and neither is "the" Kuva Nukor (owner, 2026-08-13).
///
/// `element: ""` means none chosen, which is what an ordinary weapon means too
/// — the server ignores both fields on a weapon with no valence spec, so this
/// can never hand a bonus to something that does not have one.
let valence = { element: "", bonus: 0 };
// A FRESH scenario, built from the server's defaults and from nothing else.
//
// This is what a weapon that has never been opened gets. It used to be
// `snapshotScenario()` — the live fight, which at that moment still belongs to
// the weapon you just left — so opening a new weapon inherited the previous
// one's level, duration, Tenno and buffs, and saved them as that weapon's
// "scenario 1". Two weapons' fights are ALLOWED to look alike; they are never
// allowed to be the same object or to be born from each other (user,
// 2026-08-02).
//
// Rebuilt FIELD BY FIELD, because a field missing here is a field that
// silently becomes `undefined` — which is how `infinite_ammo` once vanished
// from state while the declaration below set it. The server owns every
// default; this copies them.
function defaultScenario() {
  const d = META.defaults || {};
  return {
    enemy: d.enemy, level: d.level, steel_path: d.steel_path,
    // NULL means "whatever this unit is by default", which the server resolves
    // to its `can_be_eximus`. Only an explicit choice is stored, so switching
    // targets keeps giving you the elite unit wherever one exists rather than
    // carrying a decision made about a different enemy.
    eximus: d.eximus ?? null,
    headshot_pct: d.headshot_pct, aiming: d.aiming !== false,
    invisible: !!d.invisible, airborne: !!d.airborne, overshields: !!d.overshields,
    channeling: !!d.channeling, solo_weapon: !!d.solo_weapon,
    frame: d.frame || "", wf_armor: d.wf_armor || 0, wf_energy: d.wf_energy || 0,
    wf_sprint: d.wf_sprint || 0.9,
    infinite_ammo: d.infinite_ammo !== false, metric: d.metric || "kpm",
    // NO `form`: how the weapon is played belongs to the build.
    duration: d.duration, buffs: {},
    // WARFRAME ABILITY BUFFS. A fraction, not a percent — 1 is 100% Ability
    // Strength — because that is what the server multiplies by, and a scenario
    // that stored a percent would need a converter nobody would remember.
    // `abilities` is what is ticked: `{id, secs}`, and `secs: null` is the
    // whole fight. Empty by default, which is what makes the untouched
    // scenario the same fight it has always been.
    ability_strength: 1, abilities: [],
    // THE FIGHT'S OWN STAT BONUSES — see `EXTRA_STAT_KEYS`. Empty is a fight
    // that hands this weapon nothing it did not earn, which is every ruler.
    extra_stats: { ...(d.extra_stats || {}) },
  };
}

// Sim scenario + per-buff config. Seeded from META.defaults in init().
// `buffs` maps buff id -> { stacks, locked } (section 2); the buff SET comes
// from /api/panel and syncs as the build changes.
// THE TENNO's fields (`aiming`, `invisible`, `airborne`, `wf_armor`,
// `wf_energy`) describe the fight's other actor — who is holding the weapon
// and what they are doing. Every `condition:` on a mod card is a question
// about them, and the arcanes that scale off a Warframe read the two stats.
// They live flat on `sim` like every other scenario field; the engine is
// where they become a Tenno (`data/tenno/default.yaml` + these overrides).
// `aiming` defaults TRUE because that is what the sim silently assumed before
// the knob existed, so no stored preset changes meaning.
let sim = { enemy: "thrax_centurion", level: 9999, steel_path: true, eximus: null, headshot_pct: 100, aiming: true,
  invisible: false, airborne: false, overshields: false, channeling: false,
  // THE LOADOUT, not what the wielder is doing: false = carrying a full one,
  // which is the fight the board is scored under and what every clause about
  // the other slots has always been answered with.
  solo_weapon: false,
  frame: "", wf_armor: 0, wf_energy: 0, wf_sprint: 0.9,
  // NO `form`, AND NO `mode`. How the weapon is played is part of the BUILD;
  // a fight that carried it could decide how the weapon was fired, which is
  // what let a ruler pin an Incarnon weapon at its cycle (owner, 2026-08-07):
  // the official scenarios no longer carry a mode, and a custom one must not
  // either.
  // 180 s: the same length the official rulers run, so a player's first
  // comparison against the board is not a puzzle (owner, 2026-08-10). Only the
  // DEFAULT — a saved scenario carries its own duration and keeps it.
  infinite_ammo: true, metric: "kpm", duration: 180, buffs: {},
  ability_strength: 1, abilities: [], extra_stats: {} };
// The current build's configurable buffs (from the last /api/panel response).
let buffList = [];
// Damage-meter rows the player has expanded into their per-type split, kept
// across runs so a simulate does not re-collapse them.
const simMeterOpen = new Set();
// Optimizer scope, 8 + 1 slots in TWO blocks: `mods` (id -> "search"|"fixed")
// scopes the MAIN 8 slots — exilus-flagged mods may sit here too, all 9
// slots accept them (game rule); `exilus` (same states, exilus-eligible mods
// only) scopes the +1 exilus slot — "search" = a slot option next to "leave
// empty", "fixed" = pin it (max one). Plus the arcane set and per-tier
// evolution option sets. Enemy + buffs are shared with the Sim panel
// (`sim`). Seeded from the current build on weapon change.
let opt = { mods: {}, exilus: {}, arcanes: {}, evos: {}, modes: {}, valence: {}, size: 8, min: 1 };
let optSeeded = false;
// (The optimizer used to keep its own scope-wide buff list and config here.
// It reads the SCENARIO's now — see `renderOptBuffs`.)
// Sort/polarity prefs for the optimizer mod list (independent of the picker's).
let optPrefs = { sort: "name", dir: "asc", pol: null };
// HOW THE SEARCH RUNS — `finalists` and `threads`, and both are the search's
// (user, 2026-08-02).
//
// The optimizer tab is TWO HALVES and the split is now total: everything from
// its own preset bar down through the Search block is the SEARCH and is saved
// in the search preset; everything under the fight's bar is the SIMULATOR's
// and is read-only. There is nothing left that belongs to neither, which is
// what makes the two preset domains legible rather than a rule to remember.
//
// `threads` rides the preset with the rest of it. It does describe this
// MACHINE rather than the search — the earlier reading, and why it used to sit
// in its own localStorage key — but an optimizer preset never leaves this
// machine (a share link carries builds, scenarios and rivens, not searches),
// so the only thing that reading bought was a second place to look. A heavy
// scope wanting more cores than a light one is a real setting to save.
//
// The FINAL-ROUND CONTRACT is `finalists` × a run count, and the count has TWO
// legitimate answers (owner, 2026-08-11), so it is a setting with a default
// rather than a rule:
//
//   0 / blank = THE FIGHT'S OWN, which is the old rule and still the default.
//               A winner crowned at the precision the replay will use is a
//               winner you can reproduce by pressing Run Sim.
//   a number  = this search's own. The fight now measures at 1000 runs, and a
//               wide scope's last round is `finalists × runs` on top of
//               everything before it — so "search cheaply, then measure the
//               winner properly in the simulator" became a real way to work,
//               and it was the one thing the scope could not say.
//
// It is a SEARCH setting, not the fight's: it says how hard to search, like
// finalists and threads, and it travels in the search preset with them.
const finalRuns = () => optRun.runs || simRuns();
const OPT_RUN_DEFAULTS = { finalists: 10, threads: 0, runs: 0 }; // 0 = the fight's own
let optRun = { ...OPT_RUN_DEFAULTS };
// One-time migration off the old machine-local key; the preset auto-save
// takes it from here.
try { const s = JSON.parse(localStorage.getItem("wfsim-opt-run")); if (s && s.threads) optRun.threads = s.threads; } catch (_) {}
let pickerSlot = 0;
// Mod-picker sort/filter prefs — persisted across slots, presets and weapons.
let pickerPrefs = { sort: "gain", dir: "desc", pol: null };
try { const s = JSON.parse(localStorage.getItem("wfsim-picker")); if (s) pickerPrefs = { ...pickerPrefs, ...s }; } catch (_) {}
const savePickerPrefs = () => localStorage.setItem("wfsim-picker", JSON.stringify(pickerPrefs));

// ---- topbar weapon search: filter chips + sort, rows navigate ----------
function initWeaponSearch() {
  const input = $("wsearch-input"), panel = $("wsearch-panel"),
        tools = $("wsearch-tools"), listEl = $("wsearch-list");
  if (!input) return;
  input.placeholder = tr("Search…");
  let flt = "all", srt = "az";
  const cats = [...new Set((META.weapons || []).map((w) => w.subtype || w.mod_class))];
  tools.innerHTML =
    `<span class="pchip sel" data-f="all">${tr("All")}</span>` +
    cats.map((c) => `<span class="pchip" data-f="${c}">${c}</span>`).join("") +
    ddButton("wsearch-sort", {
      value: srt,
      items: [{ value: "az", label: tr("Name A→Z") }, { value: "za", label: tr("Name Z→A") }],
      onPick: (v) => { srt = v; renderList(); },
    });
  const renderList = () => {
    const q = input.value.trim().toLowerCase();
    const list = (META.weapons || [])
      .filter((w) => flt === "all" || (w.subtype || w.mod_class) === flt)
      .filter((w) => searchHit(w, q))
      .sort((a, b) => (srt === "za" ? -1 : 1) * a.name.localeCompare(b.name));
    listEl.innerHTML = list.map((w) => `
      <div class="opt" data-id="${w.id}">
        ${imgTag(IMG(w.image), "mod")}
        <div class="info"><div class="mn">${w.name}</div><div class="me"><div>${w.subtype || ""}</div></div></div>
      </div>`).join("") || `<div class="sim-empty">${tr("No matches")}</div>`;
  };
  const open = () => { panel.hidden = false; renderList(); };
  input.addEventListener("focus", open);
  input.addEventListener("input", open);
  tools.addEventListener("click", (e) => {
    const chip = e.target.closest(".pchip");
    if (!chip) return;
    flt = chip.dataset.f;
    tools.querySelectorAll(".pchip").forEach((c) => c.classList.toggle("sel", c === chip));
    renderList();
  });
  tools.addEventListener("change", (e) => {

  });
  listEl.addEventListener("click", (e) => {
    const row = e.target.closest(".opt");
    if (!row) return;
    panel.hidden = true;
    input.value = "";
    switchWeapon(row.dataset.id);
    nav(weaponModPath(row.dataset.id));
  });
  document.addEventListener("click", (e) => {
    if (!e.target.closest(".wsearch")) panel.hidden = true;
  });
  document.addEventListener("keydown", (e) => { if (e.key === "Escape") panel.hidden = true; });
}

// language dropdown (top right, beside the theme toggle): switching
// reloads with the current build stashed and restored.
(function () {
  // What the DOCUMENT says it is, so a screen reader picks the right voice
  // and a crawler indexes the page under the language it is actually in.
  // The tag ships as `en` and the page may be zh from the first paint.
  document.documentElement.lang = LANG;
  const host = $("lang-select");
  if (!host) return;
  // The topbar's language control is a dropdown like every other, so it is
  // drawn by the same component — the LAST native select on the page, and the
  // most visible one (owner, 2026-08-06).
  //
  // DEFERRED by a microtask, because this block runs DURING script evaluation
  // and the component it calls is declared further down: `const` and `function
  // expression` bindings are in their temporal dead zone until the line that
  // creates them runs, so drawing here directly threw. A microtask runs after
  // the whole script has evaluated, which is the first moment any part of the
  // file may call any other part.
  queueMicrotask(() => {
    const el = $("lang-select");
    if (!el) return;
    el.outerHTML = ddButton("lang-select", {
      value: LANG,
      title: "Language / 语言",
      items: [{ value: "en", label: "English" }, { value: "zh", label: "中文" }],
      onPick: (v) => {
        localStorage.setItem("wfsim-lang", v);
        try { sessionStorage.setItem("wfsim-lang-stash", JSON.stringify(snapshotState())); } catch (_) {}
        location.reload();
      },
    });
  });
})();

// Official QQ community group: the topbar mark and the footer entry LINK
// to the join page (qm.qq.com deep-links into the QQ app, Discord-invite
// style); the footer's ⧉ copies the raw number for manual in-QQ search.
// Feedback is inline: no native dialogs.
const QQ_GROUP = "995078378";
(function () {
  const btn = $("qq-copy-foot");
  if (!btn) return;
  btn.addEventListener("click", async () => {
    try { await navigator.clipboard.writeText(QQ_GROUP); } catch (_) {
      const ta = document.createElement("textarea");
      ta.value = QQ_GROUP; document.body.appendChild(ta);
      ta.select(); document.execCommand("copy"); ta.remove();
    }
    btn.textContent = "✓";
    setTimeout(() => { btn.textContent = "⧉"; }, 1200);
  });
})();

// The phone's topbar menu. It opens ONE container that holds the real
// controls — see index.html — so there is nothing here to keep in sync with a
// second copy; this only decides when the container is a box.
(function () {
  const bar = document.querySelector(".topbar"), btn = $("menu-toggle");
  if (!bar || !btn) return;
  const set = (open) => {
    bar.classList.toggle("menu-open", open);
    btn.setAttribute("aria-expanded", open ? "true" : "false");
  };
  btn.addEventListener("click", (e) => {
    e.stopPropagation();
    set(!bar.classList.contains("menu-open"));
  });
  // `#dd-popover` counts as INSIDE: the language dropdown draws into the
  // shared popover, which is a sibling of the menu in the DOM, so a click on
  // "中文" would otherwise close the panel out from under the control.
  document.addEventListener("click", (e) => {
    if (!e.target.closest("#topmenu, #menu-toggle, #dd-popover")) set(false);
  });
  document.addEventListener("keydown", (e) => { if (e.key === "Escape") set(false); });
  // A DESTINATION closes it — the page moves and the menu would be left open
  // over the new one. A CONTROL does not: after switching the theme you can
  // still want the language, and neither moves the page.
  document.querySelector("#topmenu .topnav")
    .addEventListener("click", () => set(false));
  // Above the breakpoint the panel is `display:contents` again and the class
  // means nothing — but it would still be there on the way back down.
  addEventListener("resize", () => { if (innerWidth > 700) set(false); });
})();

// theme
(function () {
  const saved = localStorage.getItem("wfsim-theme");
  if (saved) document.documentElement.setAttribute("data-theme", saved);
  $("theme-toggle").addEventListener("click", () => {
    const cur = document.documentElement.getAttribute("data-theme");
    const dark = cur === "dark" || (!cur && matchMedia("(prefers-color-scheme: dark)").matches);
    const next = dark ? "light" : "dark";
    document.documentElement.setAttribute("data-theme", next);
    localStorage.setItem("wfsim-theme", next);
  });
})();

async function init() {
  META = await api("/api/meta");
  {
    let all = null;
    try { all = await api("/api/i18n"); } catch (_) { all = null; }
    I18N = (LANG !== "en" && all && all[LANG]) || null;
    // Keep the OTHER locales' name tables for the search blob. English is not
    // among them — it is already on every entity as `name` or `name_en`.
    ALT_NAMES = all
      ? Object.entries(all).filter(([l]) => l !== LANG).map(([, v]) => v)
      : null;
    applyNameOverlay();
  }
  applyI18n();
  fillSelect("weapon", META.weapons);
  initWeaponSearch();
  const d = META.defaults;
  $("weapon").value = d.weapon;
  arcanes = arcanesFor(d.weapon, d.arcane);
  evoSel = { 1: null, 2: null, 3: null, 4: null, ...(d.evolutions || {}) };
  sim = defaultScenario();
  await loadBoard();          // before presets: the board's rows ARE build presets
  applyWeapon(d.weapon, d.mods);

  $("weapon").addEventListener("change", () => {
    switchWeapon($("weapon").value);
    if (!document.querySelector(".config-page").hidden) nav(weaponModPath($("weapon").value));
  });
  $("run-sim").addEventListener("click", runSim);
  $("run-opt").addEventListener("click", runOptimize);
  $("opt-mod-filter").addEventListener("input", renderOptModList);
  $("opt-arc-filter").addEventListener("input", renderOptArcanes);
  // How full a build must be, as a RANGE. The two ends are one setting: a
  // ceiling below the floor is not a scope, so each end pushes the other.
  $("opt-size").addEventListener("input", () => {
    opt.size = Math.max(1, Math.min(8, Number($("opt-size").value) || 8));
    if (opt.min > opt.size) { opt.min = opt.size; $("opt-min").value = opt.min; }
    updateOptEstimate();
  });
  $("opt-min").addEventListener("input", () => {
    opt.min = Math.max(1, Math.min(8, Number($("opt-min").value) || 1));
    if (opt.min > opt.size) { opt.size = opt.min; $("opt-size").value = opt.size; }
    updateOptEstimate();
  });
  // updateOptEstimate is also the scope's auto-save, so finalists lands in the
  // active preset the same way every other search setting does.
  $("opt-finalists").value = optRun.finalists;
  $("opt-finalists").title = tr("how many builds survive to the last round — each is then run at the final-round run count beside this");
  $("opt-finalists").addEventListener("input", () => {
    optRun.finalists = Math.max(1, Math.min(100, Number($("opt-finalists").value) || 10));
    updateOptEstimate();
  });
  // BLANK MEANS THE FIGHT'S, which is why the box is empty rather than
  // pre-filled with the scenario's number: a copy of a number that lives
  // elsewhere is a second opinion waiting to go stale (the fight's count can
  // change under it), and an empty box says "not my question" out loud.
  if (optRun.runs) $("opt-runs").value = optRun.runs;
  $("opt-runs").placeholder = tr("the fight's");
  $("opt-runs").title = tr("how many simulations each finalist gets in the last round — blank uses the fight's own count, which is what the replay will use. A smaller number searches faster and is worth re-measuring in the simulator");
  $("opt-runs").addEventListener("input", () => {
    optRun.runs = Math.max(0, Math.min(20000, Number($("opt-runs").value) || 0));
    updateOptEstimate(); // the scope's auto-save; runs lands in the preset
  });
  if (optRun.threads) $("opt-threads").value = optRun.threads;
  $("opt-threads").title = tr("blank = every core minus two, at low priority — the machine stays responsive either way. Saved with the search, so a heavy scope can ask for more than a light one");
  $("opt-threads").addEventListener("input", () => {
    optRun.threads = Math.max(0, Math.min(128, Number($("opt-threads").value) || 0));
    updateOptEstimate(); // the scope's auto-save; threads lands in the preset
  });
  initPresets();
  reattachOptimize(); // resume progress display if a server-side job survives a reload
  $("auto-forma").addEventListener("click", () => { autoForma(); renderMods(); });
  $("clear-mods").addEventListener("click", () => { slots.forEach((s, i) => { s.mod = null; s.pol = innate[i]; }); renderMods(); });
  document.addEventListener("click", (e) => {
    // `.rv-pick` opens the same popover from the riven tab, so its own click
    // must not be the click that closes it again.
    if (!e.target.closest(".popover") && !e.target.closest(".slot") && !e.target.closest(".rv-pick")) closePopovers();
  });
  document.addEventListener("keydown", (e) => { if (e.key === "Escape") closePopovers(); });
  window.addEventListener("popstate", route);
  // Reloading or closing the tab KILLS a run in progress: the worker dies with
  // the page. That is not a limitation we can engineer around — measured
  // 2026-07-30, a SharedWorker is terminated too, the moment its last client
  // disconnects, whether or not it is busy.
  //
  // The browser's own unload prompt is the only guard that runs before the page
  // goes, and it cannot be replaced by an inline one — so this is the single
  // place the project's no-native-dialogs rule does not reach. It only fires
  // while something is actually running.
  window.addEventListener("beforeunload", (e) => {
    if (optJobId == null) return;
    e.preventDefault();
    e.returnValue = ""; // required by older engines to trigger the prompt
  });
  // In-app navigation: any same-origin root-relative link routes client-side
  // (modified clicks — new tab etc. — keep native behavior; a full page load
  // also works thanks to the server's SPA fallback).
  document.addEventListener("click", (e) => {
    const a = e.target.closest('a[href^="/"]');
    if (!a || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey || e.button !== 0) return;
    e.preventDefault();
    nav(a.getAttribute("href"));
  });
  route();
  // A language switch reloads the page; the pre-switch build is stashed in
  // sessionStorage and restored here so nothing is lost.
  const stash = sessionStorage.getItem("wfsim-lang-stash");
  if (stash) {
    sessionStorage.removeItem("wfsim-lang-stash");
    try { restoreState(JSON.parse(stash)); } catch (_) {}
  }
}

// ---- views: '/' = the weapon list (home); '/weapons/<Wiki_Name>' = the
// BUILDER; '/weapons/<Wiki_Name>/simulator' = the SIMULATOR (tests the
// current build); '/weapons/<Wiki_Name>/optimizer' = the OPTIMIZER — one
// tab per module (the page's three modules — user, 2026-07-29, "Simulator
// sits in the middle"). URLs mirror wiki page names
// (display name, spaces → '_'); internal weapon ids never appear in URLs.
// The weapon <select> stays the internal source of truth; the home grid
// and the path just drive it.
// The WIKI PAGE name behind a weapon's display name. A parenthesised
// qualifier is OURS — "Larkspur Prime (Atmosphere)" is one wiki page with two
// stat columns, and we ship the ground one — so it never reaches a URL.
// `build_site_app.py`'s `wiki_name` splits on the same " (".
const wikiWeaponName = (w) => (w.name_en || w.name).split(" (")[0];
const wikiSlug = (w) => wikiWeaponName(w).replace(/ /g, "_");
const weaponPath = (id) => {
  const w = (META.weapons || []).find((x) => x.id === id);
  return "/weapons/" + (w ? wikiSlug(w) : id);
};
function nav(path) {
  if (location.pathname !== path) history.pushState(null, "", path);
  route();
}
function route() {
  // A SHARED LINK is answered before anything else on the page is drawn for
  // it, and the query is stripped afterwards so a refresh does not import the
  // same build a second time. `?b=` only ever ADDS — see importShare.
  const shared = SHARE_ENABLED && new URLSearchParams(location.search).get(SHARE_PARAM);
  // A LINK POSTED WHILE SHARING WAS ON still has to open something. The query
  // is stripped either way, so a refresh cannot retry it; with sharing off the
  // visitor gets the weapon's own page and a line saying why, which is a page
  // rather than a blank.
  if (!SHARE_ENABLED && new URLSearchParams(location.search).get(SHARE_PARAM)) {
    history.replaceState(null, "", location.pathname);
    setTimeout(() => presetToast(tr("sharing is off for now — this link opened the weapon instead")), 900);
  }
  if (shared) {
    history.replaceState(null, "", location.pathname);
    // DRAW THE PAGE FIRST, then land the payload into it. Returning here
    // instead left the visitor on the home grid staring at nothing until they
    // refreshed: `importShare` fills the editor in, but which module is
    // VISIBLE is this function's job and it had been skipped. The query is
    // already stripped, so this re-entry takes the ordinary path.
    route();
    importShare(shared);
    return;
  }
  // `/support` is a page of the SHELL, not a fourth module and not a weapon's
  // tab: it belongs to no weapon, so it sits beside the home grid rather than
  // under /weapons/<name>.
  const support = /^\/support\/?$/.test(location.pathname);
  const bench = /^\/benchmark\/?$/.test(location.pathname);
  const m = (support || bench) ? null : location.pathname.match(/^\/weapons\/([^/]+?)(\/simulator|\/optimizer|\/rivens|\/enemies)?\/?$/);
  // A hand-typed URL is not the canonical slug. Fold case and treat spaces
  // (and their %20) as underscores, so "/weapons/Dual Toxocyst" reaches the
  // same weapon as "/weapons/Dual_Toxocyst" instead of silently falling back
  // to the home grid — which reads as "the site sent me somewhere else".
  const slug = m && decodeURIComponent(m[1]).trim().toLowerCase().replace(/[\s-]+/g, "_");
  const w = slug && (META.weapons || []).find(
    (x) => wikiSlug(x).toLowerCase() === slug || x.id === slug
  );
  // The active module: "" = builder, "simulator", "optimizer".
  const mod = (w && m[2]) ? m[2].slice(1) : "";
  document.body.classList.toggle("on-home", !w && !support && !bench);
  document.body.classList.toggle("on-support", support);
  document.body.classList.toggle("on-benchmark", bench);
  document.body.classList.toggle("on-simulator", mod === "simulator");
  document.body.classList.toggle("on-optimizer", mod === "optimizer");
  document.body.classList.toggle("on-rivens", mod === "rivens");
  document.body.classList.toggle("on-enemies", mod === "enemies");
  $("home-page").hidden = !!w || support || bench;
  $("support-page").hidden = !support;
  $("bench-page").hidden = !bench;
  // The nav says where you are. `data-nav` rather than a path compare: the
  // roster lives at "/" and a path compare there matches every page.
  const here = bench ? "benchmark" : (!w && !support) ? "home" : "";
  document.querySelectorAll(".tnav").forEach((a) => {
    a.classList.toggle("sel", a.dataset.nav === here);
  });
  document.querySelector(".config-page").hidden = !w;
  const modTitle = { simulator: " · Simulator", optimizer: " · Optimizer", rivens: " · Rivens", enemies: " · Enemies" }[mod] || "";
  // The home title carries the SEARCH TERMS, not the headline: nobody looks
  // for "Simulacrum Prime", and the tab/result/share-card is the one place
  // that has to be found rather than enjoyed (user, 2026-07-31). The joke
  // stays on the page, which is where a player meets it.
  document.title = support ? `${tr("Support")} — WFSim`
    : bench ? `${tr("Benchmark")} — WFSim`
    : w ? `${w.name}${modTitle} — WFSim` : "WFSim — Ultimate Warframe Calculator";
  if (support) {
    renderSupport();
  } else if (bench) {
    renderBenchBoard();
  } else if (w) {
    // `?mode=` — HOW the linked build is played, carried by the link that made
    // it. A board row is a weapon AND a mode ("Burston Prime, base form"), so a
    // link that dropped the second half landed on a page measuring something
    // else (owner, 2026-08-07).
    //
    // The QUERY, not a path segment: the path mirrors the wiki's page name and
    // its next segment is already the module (`/simulator`), so a mode there
    // would be a third meaning for one slot.
    //
    // READ BEFORE THE SWITCH. Loading a weapon restores its preset, and that
    // rewrites the address to the weapon's plain path — so by the time the
    // weapon is on screen the query is already gone, and the mode with it.
    const wantMode = new URLSearchParams(location.search).get("mode");
    // WHICH RULER, from a board row. A row is a build AND the ruler it was
    // measured under; arriving with only the build gives you a number you
    // cannot reproduce, and arriving with neither gave you the FIRST ruler's
    // leader whichever board you clicked (owner, 2026-08-08).
    const wantBench = new URLSearchParams(location.search).get("bench");
    if ($("weapon").value !== w.id) {
      switchWeapon(w.id);
    }
    if (wantBench) applyBenchLink(w, wantBench, wantMode);
    if (wantMode && (w.modes || []).includes(wantMode)) {
      if (wantMode !== mode) {
        mode = wantMode;
        renderMode();
        renderMods();
        refreshPanel();
      }
      // ...and put it back on the address bar, which the restore just cleared.
      // Kept rather than stripped: the link has to survive a refresh and a
      // bookmark, and `renderMode` rewrites it when the visitor changes mode so
      // it never says something the page is not doing.
      if (new URLSearchParams(location.search).get("mode") !== wantMode) {
        const q = new URLSearchParams();
        if (wantBench) q.set("bench", wantBench);
        q.set("mode", wantMode);
        history.replaceState(null, "", `${location.pathname}?${q}`);
      }
    }
    $("module-tabs").innerHTML =
      `<a class="mtab ${mod === "" ? "sel" : ""}" href="${weaponPath(w.id)}">${tr("Builder")}</a>` +
      `<a class="mtab ${mod === "simulator" ? "sel" : ""}" href="${weaponPath(w.id)}/simulator">${tr("Simulator")}</a>` +
      `<a class="mtab ${mod === "optimizer" ? "sel" : ""}" href="${weaponPath(w.id)}/optimizer">${tr("Optimizer")}</a>` +
      `<a class="mtab ${mod === "rivens" ? "sel" : ""}" href="${weaponPath(w.id)}/rivens">${tr("Rivens")}</a>` +
      `<a class="mtab ${mod === "enemies" ? "sel" : ""}" href="${weaponPath(w.id)}/enemies">${tr("Enemies")}</a>`;
    // Arriving on the simulator: refresh its build summary (builder edits
    // don't re-render sim views while they are hidden). The SCENARIO is one
    // state shared with the optimizer, so each tab redraws its own copy of
    // those fields on arrival — the other tab may have moved them, and the
    // tabs are CSS-hidden rather than re-rendered.
    if (mod === "simulator") renderSim();
    if (mod === "optimizer") { renderOptEnemy(); updateOptEstimate(); }
    if (mod === "rivens") renderRivens();
    if (mod === "enemies") renderEnemies();
  } else {
    renderHome();
  }
}

// The current module's path suffix — weapon switches (search, select,
// preset load) keep the visitor on the tab they are on.
const modSuffix = () => (location.pathname.match(/\/(simulator|optimizer|rivens)\/?$/) || [null, ""])[1];
const weaponModPath = (id) => weaponPath(id) + (modSuffix() ? "/" + modSuffix() : "");

// The home grid groups by EQUIPMENT SLOT in loadout order (user, 2026-07-30):
// one flat list stops being readable as soon as the roster holds more than one
// slot's worth. A slot with no weapons renders nothing at all rather than an
// empty heading, and an unknown slot still gets its weapons shown.
// Equipment slots, in the order the arsenal shows them. "sentinel" is a real
// slot, not a kind of primary: a sentinel weapon rides the companion and draws
// from the rifle mod pool without ever occupying a weapon slot.
const SLOT_ORDER = ["primary", "secondary", "melee", "sentinel", "archgun"];
const SLOT_LABEL = { primary: "Primary", secondary: "Secondary", melee: "Melee",
  sentinel: "Sentinel Weapons", archgun: "Arch-Guns", other: "Other" };

function renderHome() {
  const grid = $("weapon-grid");
  if (!grid) return;
  const card = (w) => {
    const tags = [
      `<span class="tag">${w.subtype || w.mod_class}</span>`,
      w.uses_evo2 ? `<span class="tag">Incarnon</span>` : "",
      w.sentinel ? `<span class="tag">Sentinel</span>` : "",
    ].join("");
    return `<a class="wcard" href="/weapons/${wikiSlug(w)}">
      ${imgTag(IMG(w.image), "wc-img")}
      <div class="wc-info">
        <div class="wc-name">${w.name}</div>
        <div class="wc-tags">${tags}</div>
      </div>
    </a>`;
  };
  const all = META.weapons || [];
  const groups = SLOT_ORDER
    .map((s) => [s, all.filter((w) => (w.slot || "") === s)])
    .filter(([, ws]) => ws.length);
  const rest = all.filter((w) => !SLOT_ORDER.includes(w.slot || ""));
  if (rest.length) groups.push(["other", rest]);
  grid.innerHTML = groups.map(([slot, ws]) => `
    <section class="wgroup">
      <h3 class="wgroup-h">${tr(SLOT_LABEL[slot] || slot)}</h3>
      <div class="wgrid">${ws.map(card).join("")}</div>
    </section>`).join("");
}

// ---- THE BOARD: one ruler, every weapon ---------------------------------
//
// WHAT IT IS FOR (owner, 2026-08-07): the fastest way to see which weapons are
// strong, and which nobody has measured yet — so the empty rows are as much the
// point as the full ones. A visitor who fills one is finding an optimum for
// everybody, and a row that looks impossible is how a bug in this engine gets
// found from outside it.
//
// ONE NUMBER PER WEAPON, its best. Everything else about that weapon — the
// other builds, the deeper ranks — lives on the weapon's own page, which is
// where a click goes. That is what keeps this page O(weapons) whatever the
// board grows to, and what will let a benchmark hold a hundred builds per
// weapon without any of them being fetched to draw this.
let benchPick = null;

const benchList = () => META.benchmarks || [];
const benchCurrent = () =>
  benchList().find((b) => b.id === benchPick) || benchList()[0] || null;

/// HOW A WEAPON WAS PLAYED, in words. The ids are the vocabulary a submission
/// and a board row use; this is what a reader sees.
///
/// NAMED FOR THE FORM IT FIRES, not for the mode's own id: a Cernos Prime's
/// `base` mode is its CHARGED shot, because that is what the arsenal hands
/// you, and calling it "base form" would be true of the id and false of the
/// weapon. The mode ids are roles; the labels are the weapon's own words.
const modeLabel = (w, id) => {
  const forms = (w || {}).forms || [];
  if (id === "cycle") return tr("Incarnon cycle");
  // A WEAPON CAN HAVE MORE THAN ONE ALTERNATE, so "the non-default form" is
  // not an answer: a bow with an adapter has a tapped shot AND an Incarnon
  // form, and picking whichever came first labelled one of them with the
  // other's name. The two modes are told apart by the gauge, the same
  // question the engine splits them on.
  const f = id === "alternate" ? forms.find((x) => !x.is_default && !x.gauge_switched)
    : id === "transformed" ? forms.find((x) => !x.is_default && x.gauge_switched)
    : forms.find((x) => x.is_default);
  return f ? tr(f.name) : tr(id);
};

/// One entry per WEAPON AND MODE: its best row under `id`, or null where nobody
/// has submitted. A weapon with no row is not an error, it is the invitation —
/// and a weapon with a row for one mode and none for the other is the sharpest
/// version of it, because the missing one is a question somebody can answer
/// this afternoon.
const benchEntries = (id) => {
  const out = [];
  for (const w of META.weapons || []) {
    for (const m of w.modes || ["base"]) {
      const rows = (BOARD[w.id] || [])
        .filter((r) => r.benchmark === id && (r.mode || "base") === m);
      out.push({ w, mode: m,
        row: rows.length ? rows.reduce((a, r) => (r.score > a.score ? r : a), rows[0]) : null });
    }
  }
  return out;
};

function renderBenchBoard() {
  const box = $("bench-board");
  if (!box || !META) return;
  const picker = $("bench-picker");
  const bs = benchList();
  const cur = benchCurrent();
  if (picker) {
    // A RULER IS PICKED, not scrolled past. Two today and dozens later, so it
    // is a list of chips rather than a stack of tables — the page shows one
    // ranking at a time because two rankings side by side is a comparison
    // nobody asked for yet.
    picker.innerHTML = bs.map((b) => `<button type="button" class="bchip${
      cur && b.id === cur.id ? " sel" : ""}" data-bench="${escHtml(b.id)}">${escHtml(tr(b.name))}</button>`).join("");
    picker.querySelectorAll("[data-bench]").forEach((el) => {
      el.onclick = () => { benchPick = el.dataset.bench; renderBenchBoard(); };
    });
  }
  if (!cur) { box.innerHTML = ""; return; }
  // THE RULES, under the ruler that makes them. Collapsed by default: a reader
  // who wants the ranking should not have to scroll a standard to reach it, and
  // one who doubts a row should not have to leave the page to check the terms.
  const rules = $("bench-rules");
  if (rules) {
    const rs = cur.rules || [];
    rules.innerHTML = !rs.length ? "" : `<details class="brules">
      <summary>${escHtml(tr("What this benchmark measures"))}</summary>
      <ul>${rs.map((x) => `<li>${escHtml(tr(x))}</li>`).join("")}</ul>
    </details>`;
  }
  const entries = benchEntries(cur.id);
  // SORTED BY THE BENCHMARK'S OWN METRIC — it says which one it is measured in
  // (`scenario.metric`), and a second ruler may answer differently. Unmeasured
  // weapons sort last whatever the metric: a zero is not a low score, it is no
  // score, and putting it among the low ones would read as one.
  const metric = ((cur.scenario || {}).metric || "kpm").toUpperCase();
  const rows = entries
    .slice()
    .sort((a, b) => (b.row ? b.row.score : -1) - (a.row ? a.row.score : -1));
  const measured = rows.filter((r) => r.row).length;
  box.innerHTML = `
    <div class="bench-meta">${escHtml(
      tr("{n} of {t} entries measured · ranked by {m}")
        .replace("{n}", measured).replace("{t}", rows.length).replace("{m}", metric))}</div>
    <div class="bench-rows">${rows.map(({ w, mode, row }, i) => `
      <a class="brow${row ? "" : " none"}" href="/weapons/${wikiSlug(w)}${
        // WHICH RULER YOU CAME FROM, not just how the weapon is played. Without
        // it the link landed on whatever official build happened to be first —
        // the AIMED board's leader, even when you clicked a row on the no-aim
        // board (owner, 2026-08-08). The two rows are both called "#1" and both
        // say "Incarnon cycle"; the ruler is the only thing that tells them
        // apart, and it was the one thing the link did not carry.
        //
        // It selects the FIGHT as well as the build. A board row is a build AND
        // the ruler it was measured under, so arriving with only the build is
        // arriving with a number you cannot reproduce.
        `?bench=${encodeURIComponent(cur.id)}${
          (w.modes || []).length > 1 ? `&mode=${encodeURIComponent(mode)}` : ""}`}">
        <span class="brank">${row ? `#${i + 1}` : "—"}</span>
        ${imgTag(IMG(w.image), "bimg")}
        <span class="bname">${escHtml(w.name)}${
          (w.modes || []).length > 1
            ? ` <span class="bmode">${escHtml(modeLabel(w, mode))}</span>`
            : ""}${
          // THE BOARD IS WHERE WEAPONS ARE COMPARED, so it is the one place a
          // weapon with unmodelled parts must not look like one without them.
          // A Stug row is four admissions deep and a Torid row is exact; side
          // by side and unmarked they read as the same kind of number.
          // The mark is the banner's own ◈ and carries the same sentences.
          (w.unmodeled || []).length
            ? ` <span class="bgap" title="${escHtml(
                tr("not modelled on this weapon — the numbers below are a floor, not its full output")
                + ": " + (w.unmodeled || []).map((g) => tr(g)).join(" · "))}">◈</span>`
            : ""}</span>
        <span class="bscore">${row
          ? escHtml(row.shown != null ? String(row.shown) : row.score.toFixed(4))
          : `<span class="bnone">${escHtml(tr("not measured"))}</span>`}</span>
      </a>`).join("")}</div>`;
}

// ---- support: the donation channels -------------------------------------
// A channel is drawn only when it HAS a working link. An option that does not
// work yet is worse than one that is not offered, so an entry with an empty
// `url` renders nothing — and filling that url in is the whole of adding one.
//
// ONE channel serves every locale (owner, 2026-08-06). There is deliberately
// no per-locale ordering and no QR path here: both would be machinery for a
// second channel that does not exist, and the shape a domestic one wants is
// not knowable until there is one to look at.
const SUPPORT_CHANNELS = [
  {
    id: "kofi",
    name: "Ko-fi",
    url: "https://ko-fi.com/magenie33",
    // ONE-OFF ONLY. Ko-fi's memberships are a subscription with perks, which
    // is the one shape DE's non-commercial rule does not allow — they stay
    // switched off in the account, and so do its shop and commissions.
    // $5 is Ko-fi's price per coffee, which is where the floor comes from;
    // its own x1/x3/x5 and free field are the rest, so nothing here repeats
    // them (owner, 2026-08-06).
    what: "One-off, in USD, from $5. Card or PayPal, no account needed.",
  },
];

function renderSupport() {
  const box = $("support-channels");
  if (!box) return;
  box.innerHTML = SUPPORT_CHANNELS.filter((c) => c.url).map((c) => `
    <a class="sup-card" href="${escHtml(c.url)}" target="_blank" rel="noopener">
      <div class="sup-name">${escHtml(c.name)}</div>
      <div class="sup-what">${escHtml(tr(c.what))}</div>
      <span class="run-btn">${escHtml(tr("Open"))} ↗</span>
    </a>`).join("");
}

// ---- Rivens as MODS ----------------------------------------------------
// A saved riven is equipment, so it belongs in the mod list — findable by its
// preset name ("riven 1"), by its generated name ("Visican"), or by any value
// printed on it (user, 2026-07-31).
//
// It is NOT put in `META.mod_pools`: a riven is the visitor's own item, built
// against this weapon's disposition, so it travels WITH the request. The
// engine adds it to the pool for that build only, which is why one shared
// pool can still serve everyone.
//
// An INCOMPLETE riven is deliberately allowed through. A card with no stats
// is a mod that does nothing, which is an ordinary thing for a build to
// contain and not worth refusing.
const RIVEN_PREFIX = "riven:";
const isRivenId = (id) => typeof id === "string" && id.startsWith(RIVEN_PREFIX);
let rivenModCache = { key: null, list: [] };

// The saved rivens of the CURRENT weapon, shaped like mods so every list,
// picker and slot that already understands a mod understands these.
function rivenMods() {
  const w = $("weapon").value;
  const raw = loadPresetList(RIVENS);
  const key = w + "|" + JSON.stringify(raw) + "|" + JSON.stringify(rivenNames);
  if (rivenModCache.key === key) return rivenModCache.list;
  const list = raw.map((p) => {
    const st = p.state || {};
    const stats = (st.bonuses || st.positives || []).concat(st.malus || st.curse || []);
    const lines = (rivenNames[p.name] || {}).lines || [];
    const official = (rivenNames[p.name] || {}).name || "";
    return {
      id: RIVEN_PREFIX + p.name,
      // DE's own riven card — the game draws every riven the same, so one
      // image serves them all (`data/assets.yaml` mods.riven).
      image: META.riven_image || null,
      name: official ? `${p.name} · ${official}` : p.name,
      name_en: official,
      subtype: "Riven",
      riven: true,
      // A weapon takes ONE riven. Same family = mutually exclusive, the rule
      // the pool already has — so every list, slot and scope enforces it
      // without knowing what a riven is (user, 2026-07-31).
      family: "riven",
      rarity: "legendary",
      polarity: (st.polarity || "madurai").replace(/^./, (c) => c.toUpperCase()),
      drain: 2 + 2 * (st.rank ?? 8),
      max_rank: 8,
      exilus: false,
      // The VEILED riven card — DE's own image for every riven type
      // (`imageName` on the riven mod item), and the one on the CDN like
      // every other mod picture (user, 2026-07-31).
      image: "OmegaMod.png",
      // Every printed value is searchable, which is how you find the riven
      // with the crit damage on it without remembering what you called it.
      effects: lines.length ? lines : stats.filter((s) => s && s.id).map((s) => s.id.replace(/_/g, " ")),
      __spec: st,
    };
  });
  rivenModCache = { key, list };
  return list;
}

// The generated name and printed lines per saved riven, filled in by asking
// the engine — the page never computes a riven value itself.
let rivenNames = {};
async function refreshRivenNames() {
  const ps = loadPresetList(RIVENS);
  const out = {};
  for (const p of ps) {
    try {
      const r = await api("/api/riven", { weapon: $("weapon").value, ...(p.state || {}) });
      out[p.name] = { name: r.name || "", lines: (r.stats || []).map((s) => s.text) };
    } catch (_) { /* a name is a nicety; the riven still equips */ }
  }
  rivenNames = out;
  rivenModCache = { key: null, list: [] };
  // The lists that show a riven's printed values may already have rendered
  // with only its stat ids — this arrives afterwards, so they get redrawn.
  if (typeof renderOptModList === "function" && $("opt-mods") && !$("opt-block").hidden) renderOptModList();
  if ($("riven-all") && !$("riven-block").hidden) renderRivenAll();
  if ($("mod-popover") && !$("mod-popover").hidden && rivenPickerSlot != null) renderMenu(rivenPickerSlot, $("mod-search").value || "");
}
// Which slot the builder's mod picker is open for, so an async refresh can
// redraw it without reopening it.
let rivenPickerSlot = null;

/// Everything equippable on this weapon: the pool, plus this weapon's rivens.
const poolWithRivens = () => currentPool.concat(rivenMods());

/// The mod ids a set of EVOLUTIONS takes off the weapon.
///
/// An equip rule is asked of every firing mode a weapon has, and installing the
/// Incarnon form adds one — so Dual Toxocyst wears Semi-Pistol Cannonade until
/// tier 1 goes in and cannot after ("Weapons with an Incarnon mode must have
/// Semi-Auto trigger type for both firing modes in order to equip this mod",
/// wiki Semi-Pistol_Cannonade; user, 2026-08-04). The RULE is the engine's
/// (`pool_for_build`); `evo_forbids` is its answer per evolution, so this is a
/// lookup rather than a second implementation of it — the last time the client
/// re-derived a pool rule it went stale the same week (see `applyWeaponInner`).
const forbiddenByEvos = (sel) => {
  const map = (weaponInfo($("weapon").value) || {}).evo_forbids || {};
  const out = new Set();
  for (const id of Object.values(sel || evoSel)) for (const m of map[id] || []) out.add(m);
  return out;
};
/// What THIS BUILD can equip: the weapon's pool minus what its evolutions cost
/// it. The optimizer keeps asking `poolWithRivens()` — evolutions are a search
/// DIMENSION there, so a mod one variant refuses is still in scope for the ones
/// that do not, and which is which is decided per candidate by the engine.
const buildPool = () => {
  const no = forbiddenByEvos();
  return poolWithRivens().filter((m) => !no.has(m.id));
};
/// What may go in the EXILUS slot. Both modules ask this one function: the
/// builder used `poolWithRivens()` and the optimizer `currentPool`, which
/// agreed only because no riven is exilus-eligible — a coincidence, not a
/// rule, and the sort that stops being true without anyone noticing.
const exilusPool = () => poolWithRivens().filter((m) => m.exilus);

/// THE WEAPON'S BUILD AXES — one description, read by BOTH modules.
///
/// The builder fills these slots and the optimizer searches them, so they are
/// the same question asked twice. Stating them once is what makes a new
/// weapon a ONE-PLACE change (user, 2026-08-01): a sentinel that seats no
/// arcane, an Arch-Gun that seats two, a pool with no exilus mod in it — each
/// is a fact about the weapon, and neither module gets its own opinion.
///
/// AN AXIS IS SHOWN IFF IT HAS OPTIONS. That one rule replaced three separate
/// conditions (`!sentinel`, `arcane_slots >= 1`, `uses_evo2`), each of which
/// was a category guess standing in for "is there anything to choose here" —
/// and each of which had to be remembered twice.
// INFINITE AMMO — on by DEFAULT for every weapon (user, 2026-08-01), because
// the sim models no ammo PICKUPS: a finite reserve is the pessimistic half of
// a mechanic we only half have, and the number people compare across weapons
// is the one where ammo is not the limit.
//
// A weapon whose reserve is infinite IN GAME cannot be switched off it — every
// sentinel weapon prints "Ammo Max: infinity / Ammo Type: None". That shows as
// a TICKED, DISABLED box: the state is real and the control is honestly
// unavailable, which a hidden control would not say.
// WHERE the fight happens, when that changes the weapon. An Arch-Gun on the
// ground and in Archwing is the SAME weapon — same damage, same mod pool, same
// riven — and only its sustain differs (reload 2.50 vs 4.50, a finite 400-round
// reserve vs a regenerating magazine), so it is a scenario axis and not a
// second entry (user, 2026-08-01). Shown only where there is a choice, the
// rule every axis here follows.
const deployField = (w, state) => {
  const opts = w.deployments || [];
  if (opts.length < 2) return "";
  const cur = opts.includes(state.deployment) ? state.deployment : opts[0];
  return `<label title="${escHtml(tr("where the weapon is fired — it changes reload and ammo, nothing else"))}">${escHtml(tr("Environment"))} ` +
    ddButton("dd-deployment", {
      value: cur,
      dataK: "deployment",
      // The data keys are lowercase; the LABEL is the wiki's own column head.
      items: opts.map((o) => ({ value: o, label: tr(o[0].toUpperCase() + o.slice(1)) })),
    }) + `</label>`;
};

// IS THE TARGET ITS ELITE VARIANT? Offered only where one EXISTS, because
// there is no such unit otherwise — the engine rejects the combination rather
// than quietly simulating an ordinary enemy under an elite label (a Thrax's
// overguard is innate, not Eximus-granted), so a control here would be a
// promise the fight cannot keep.
//
// DEFAULT ON wherever it exists (owner, 2026-08-05). The Eximus is what a
// Steel Path player actually meets, and it is not a cosmetic difference: it
// adds health and puts a pool of Overguard in front of it, so a build measured
// on the ordinary unit is measured on a fight nobody has.
//
// `sim.eximus` is NULL until you say otherwise, and null means "this unit's
// answer" — which is what makes the default follow the TARGET rather than
// stick to whatever the last target happened to be.
const eximusOn = (en) => sim.eximus ?? !!(en && en.can_be_eximus);
const eximusField = (en) => {
  if (!en || !en.can_be_eximus) return "";
  return `<label class="check" title="${escHtml(tr("the elite variant: more health, and a pool of Overguard in front of it"))}"><input type="checkbox" data-k="eximus" ${eximusOn(en) ? "checked" : ""}> ${escHtml(tr("Eximus"))}</label>`;
};

// FORCED EITHER WAY, and for opposite reasons — so the control has THREE
// states, not two (owner, 2026-08-04). It had one flag and read it as the
// wrong one of the two facts, which ticked-and-disabled the box on every
// weapon but the single Arch-Gun: the only weapon whose ammo you could adjust
// was the one weapon the game gives no way to adjust.
//
//   no reserve at all (sentinel)      -> ticked, disabled: nothing to run out
//   a reserve it cannot refill (AG)   -> UNticked, disabled: pickups it cannot get
//   a reserve it can refill (the rest) -> yours, defaulting to on
const ammoForcedOn = (w) => !w.has_reserve;
const ammoForcedOff = (w) => !!w.no_resupply;
const ammoForced = (w) => ammoForcedOn(w) || ammoForcedOff(w);
// A SENTINEL WEAPON IS ALWAYS AIMING (user, 2026-08-01) — it just never aims
// at the HEAD, which is why its headshot default is 0 and every on-headshot
// trigger stays dead anyway. Ticked and disabled, the same shape as infinite
// ammo: the state is real, the control is honestly unavailable.
const aimField = (w, state) => {
  const forced = !!w.sentinel;
  const on = forced || state.aiming;
  const why = forced
    ? tr("a sentinel weapon is always aiming — it just never aims at the head, so on-headshot effects never fire")
    : tr("mods that only work while aiming (Galvanized Crosshairs, Argon Scope, Sharpened Bullets…) grant nothing when this is off");
  return `<label class="check" title="${escHtml(why)}"><input type="checkbox" data-k="aiming"${on ? " checked" : ""}${forced ? " disabled" : ""}> ${escHtml(tr("Aiming"))}</label>`;
};
const ammoField = (w, state) => {
  const forced = ammoForced(w);
  const on = ammoForcedOn(w) || (!ammoForcedOff(w) && state.infinite_ammo !== false);
  const why = ammoForcedOn(w)
    ? tr("this weapon has no ammo reserve to run out of")
    : ammoForcedOff(w)
      ? tr("this weapon cannot be resupplied — once its reserve is gone it is removed for five minutes, so the setting has nothing to stand in for")
      : tr("on = ammo pickups keep the reserve topped up, which the sim has no entities for; off = it runs dry. The magazine and its reloads apply either way");
  return `<label class="check" title="${escHtml(why)}"><input type="checkbox" data-k="infinite_ammo"${on ? " checked" : ""}${forced ? " disabled" : ""}> ${escHtml(tr("Infinite ammo"))}</label>`;
};

function weaponAxes(weaponId) {
  const w = weaponInfo(weaponId || $("weapon").value) || {};
  const exilus = w.sentinel ? [] : exilusPool();
  return {
    mods: poolWithRivens(),
    exilus,
    hasExilus: exilus.length > 0,
    // One entry per arcane pool, in the weapon's own pool order.
    arcanes: (w.arcane_pools || []).map((pool, i) => ({ pool, options: arcanePool(i) })),
    // One entry per evolution tier.
    evolutions: weaponEvos(),
    // HOW THE WEAPON IS PLAYED — an axis like the rest, because it is one: the
    // builder picks a value and the optimizer searches the set, and the board
    // ranks weapon x mode. Only the modes a fight can HOLD are offered; the
    // engine derives that from whether entering the form costs a gauge you
    // have to earn ("always Incarnon" is not a playstyle, it is a few seconds
    // at a time), so nothing here is a list anyone maintains.
    //
    // A ONE-MODE WEAPON GETS AN EMPTY AXIS, by the same rule every other axis
    // follows: an axis is shown iff it has options, and "base" alone is not a
    // choice anybody has.
    modes: (w.modes || []).length > 1 ? w.modes : [],
  };
}

/// The mode a build plays in: the asked-for one where this weapon offers it,
/// else however the arsenal plays it — the cycle where there is one to run.
///
/// One resolver, so "no mode named" means the same thing in the builder, in a
/// share link and in a board submission.
function defaultMode(weaponId, want) {
  const ms = (weaponInfo(weaponId) || {}).modes || ["base"];
  if (want && ms.includes(want)) return want;
  return ms.includes("cycle") ? "cycle" : (ms[0] || "base");
}

/// The rivens a request must carry for its `riven:` ids to mean anything.
const rivenPayload = () =>
  loadPresetList(RIVENS).map((p) => ({ name: p.name, spec: p.state || {} }));

// ---- Rivens ------------------------------------------------------------
// A CONSTRUCTOR, not a roller. Every control is bounded by the formula, so
// the only rivens it can express are legal ones — including the corner where
// every bonus rolls maximal and the malus minimal, which is not an edge
// case here but the CEILING, the riven an optimizer wants to know about.
//
// The arithmetic is NOT duplicated: every change asks `/api/riven`, so a
// slider can never drift from what the sim would build, and a typed value
// comes back as the roll it implies. Under wasm that call is local.
let riven = null;      // the riven being edited
let rivenResolved = null;
const RIVENS = "rivens";   // its preset domain, per weapon like the builds

// The stat pool this weapon's rivens draw from. NOT its mod class: a bow's
// mods are `bow` and its rivens are `rifle`, so the server derives which pool
// applies and says so.
// `riven_excludes` takes out what THIS weapon cannot roll, and the server
// answers from three sources in order (MEASUREMENTS M35): a real card someone
// has, a COUNT over ~12 000 live riven listings per family, and only then the
// derivation — a sentinel weapon has no Zoom and no Recoil, a hit-scan one no
// flight speed, an infinite-ammo one no Ammo Maximum, and a weapon with no
// physical damage rolls no physical attribute (the wiki's 25% rule, which is
// wrong on six of 26 families in both directions). The class table stays
// shared; only the weapon's view of it narrows.
const rivenPoolAll = () => {
  const w = weaponInfo($("weapon").value);
  return (META.riven_stats || {})[w.riven_class || w.mod_class] || [];
};
const rivenPool = () => {
  const out = (weaponInfo($("weapon").value).riven_excludes) || [];
  const all = rivenPoolAll();
  return out.length ? all.filter((s) => !out.includes(s.id)) : all;
};
const rivenRules = () => META.riven_rules || { roll_min: 0.9, roll_max: 1.1, max_rank: 8 };
// Lookup goes through the UNFILTERED pool: a riven saved before its weapon
// learned it could not roll that stat still has to render and still resolves
// server-side (`resolved_slots` finds by id in the whole class pool). Only
// what the picker OFFERS narrows.
const rivenStat = (id) => rivenPoolAll().find((s) => s.id === id);
// The stat's NAME, without the placeholder or the unit: the row already
// shows the value, so repeating "X%" in the picker is noise.
//
// A riven line is OUR sentence built from DE's template, so it localizes the
// way every other engine-generated effect line does — through the locale's
// effect_phrases, which are the official client's words (data/i18n). There is
// nothing riven-specific to translate: "+150% Critical Chance" reads the same
// on a riven as on Point Strike.
const rivenStatNameEn = (s) =>
  s.text.replace("|val|", "").replace(/^\s*%\s*/, "").replace(/\s+/g, " ").trim();
const rivenStatName = (s) => tf(rivenStatNameEn(s));

// The shape, in the notation everyone already uses: 2, 3, 2+1, 3+1 — the
// count of bonuses, and a +1 for the malus. It leads because it is the
// ONLY thing that decides the multipliers; a 2 and a 2+1 pay their bonuses
// differently before a single stat is chosen.
// Best first, left to right (user, 2026-08-02): 3+1, 3, 2+1, 2. A riven is
// shopped for from the top — the third bonus is what makes one worth having,
// and the malus is the price, so the pairs read as "with / without price"
// rather than as a count that happens to climb.
const RIVEN_SHAPES = [
  { id: "3+1", bonuses: 3, malus: true },
  { id: "3", bonuses: 3, malus: false },
  { id: "2+1", bonuses: 2, malus: true },
  { id: "2", bonuses: 2, malus: false },
];
// A new card starts at 3+1 (user, 2026-08-02) — the shape a riven worth
// making is in. Stated by ID, not by position, so the display order above
// stays a display decision and never doubles as the default.
const RIVEN_SHAPE_DEFAULT = "3+1";

// An EMPTY card. Nothing is pre-picked: a default stat is a claim the visitor
// did not make. Two bonuses is the game's floor for a riven rather than a
// suggestion, and the rolls sit at 1.0 because a slider has to be somewhere.
// FOUR drafts, one per shape, and only the active one is the riven.
//
// Switching 3+1 -> 2+1 -> 3+1 used to lose the third stat: the slot was
// popped and there was nowhere for it to have gone (user, 2026-07-31).
// Keeping each shape's own stats means a shape switch is a switch, not an
// edit — you can compare a 2+1 against a 3+1 by clicking between them.
//
// `bonuses` / `malus` stay the ACTIVE shape's, so everything downstream —
// the rows, the payload, the engine — keeps reading a riven the same way.
const RIVEN_BLANK_DRAFT = (n, malus) => ({
  bonuses: Array.from({ length: n }, () => ({ id: null, roll: 1.0 })),
  malus: malus ? { id: null, roll: 1.0 } : null,
});
function blankRiven() {
  const drafts = {};
  RIVEN_SHAPES.forEach((s) => { drafts[s.id] = RIVEN_BLANK_DRAFT(s.bonuses, s.malus); });
  return {
    shape: RIVEN_SHAPE_DEFAULT,
    drafts,
    bonuses: drafts[RIVEN_SHAPE_DEFAULT].bonuses,
    malus: drafts[RIVEN_SHAPE_DEFAULT].malus,
    rank: rivenRules().max_rank,
    polarity: "madurai",
  };
}

/// Bring a riven up to the four-draft shape — older saved ones carry only the
/// active stats, and a missing draft is simply an empty one.
function withDrafts(r) {
  const out = JSON.parse(JSON.stringify(r || {}));
  out.rank = out.rank ?? rivenRules().max_rank;
  out.polarity = out.polarity || "madurai";
  // A riven saved before the wiki's Bonus/Malus wording still says
  // positives/curse. It is the visitor's own item and outlives our
  // vocabulary, so it is read either way and re-saved in the new words.
  out.bonuses = out.bonuses || out.positives || [];
  out.malus = out.malus || out.curse || null;
  delete out.positives;
  delete out.curse;
  Object.values(out.drafts || {}).forEach((d) => {
    d.bonuses = d.bonuses || d.positives || [];
    d.malus = d.malus || d.curse || null;
    delete d.positives;
    delete d.curse;
  });
  out.shape = out.shape || `${out.bonuses.length || 2}${out.malus ? "+1" : ""}`;
  out.drafts = out.drafts || {};
  RIVEN_SHAPES.forEach((s) => {
    if (!out.drafts[s.id]) out.drafts[s.id] = RIVEN_BLANK_DRAFT(s.bonuses, s.malus);
  });
  // The stats it was saved with belong to the shape it was saved in.
  if (out.bonuses.length) {
    out.drafts[out.shape] = { bonuses: out.bonuses, malus: out.malus };
  }
  const d = out.drafts[out.shape];
  out.bonuses = d.bonuses;
  out.malus = d.malus;
  return out;
}

/// Every slot filled? An unfinished card is not an ILLEGAL riven — it is one
/// that has not been described yet, and it must not be reported as an error.
const rivenComplete = () =>
  riven && riven.bonuses.every((s) => s.id) && (!riven.malus || riven.malus.id);

async function resolveRiven(pending) {
  if (!riven) return;
  try {
    // `pending` carries a typed VALUE for one slot; the server turns it into
    // the roll it implies, clamped, so the formula stays in one place.
    const body = { weapon: $("weapon").value, ...riven };
    if (pending) {
      const clone = JSON.parse(JSON.stringify(body));
      if (pending.slot === "malus") clone.malus.value = pending.value;
      else clone.bonuses[Number(pending.slot)].value = pending.value;
      rivenResolved = await api("/api/riven", clone);
      // Adopt the roll the value implied, so slider and box agree.
      (rivenResolved.stats || []).forEach((s) => {
        const at = s.slot === "malus" ? riven.malus : riven.bonuses[Number(s.slot)];
        if (at) at.roll = s.roll;
      });
    } else {
      rivenResolved = await api("/api/riven", body);
    }
  } catch (e) {
    rivenResolved = { ok: false, illegal: [String(e)], stats: [] };
  }
  renderRivenCard();
}

// The collection, guaranteed non-empty and with a live active name. There is
// ALWAYS one riven, exactly as the builder always has "build 1": a bar whose
// only option is "+ new" makes the visitor do a step the page could have done
// (user, 2026-07-31), and the first one is the empty card they were going to
// fill in anyway.
// Rivens are an OPTIONAL collection: zero is a legal, and the ordinary,
// number to own (user, 2026-08-02). Nothing is auto-created — a blank card
// standing in for "no riven" is a claim the visitor never made, and it put a
// phantom legendary in every weapon's mod pool. Custom enemies will be the
// same shape when they arrive.
// The stored list, with a stale "open" pointer cleared. It does NOT open
// anything: closing the last file is a state, and re-opening one behind the
// user's back would make "← all rivens" a button that does nothing.
function ensureRivenList() {
  const ps = loadPresetList(RIVENS);
  if (activeRivenName() && !ps.some((p) => p.name === activeRivenName())) {
    activeRiven = "";
    localStorage.removeItem(presetActiveKey(RIVENS));
  }
  return ps;
}

function renderRivens() {
  if (!META || !$("riven-block")) return;
  const w = weaponInfo($("weapon").value);
  const ps = ensureRivenList();
  const open = ps.find((p) => p.name === activeRivenName());
  // The weapon and its disposition, and nothing else: that the values below
  // are scaled by it is what a disposition IS, so saying it was noise (user,
  // 2026-08-02).
  $("riven-sub").textContent =
    `${w.name} · ${tr("disposition")} ${(w.disposition || 1).toFixed(2)}`;
  renderRivenTools();
  if (!open) {
    // LIST MODE — nothing is being edited, so nothing pretends to be. The
    // editor's boxes are emptied rather than hidden: an empty container in
    // the flow keeps the page from jumping when a file is opened.
    riven = null;
    ["riven-shape", "riven-stats", "riven-foot", "riven-card"].forEach((id) => {
      if ($(id)) $(id).innerHTML = "";
    });
    renderRivenAll();
    return;
  }
  // EDIT MODE — the open file, re-read when the weapon changed under it.
  if (!riven || riven.__weapon !== w.id) {
    riven = { ...withDrafts(open.state || blankRiven()), __weapon: w.id };
  }
  renderRivenShape();
  renderRivenStats();
  renderRivenFoot();
  renderRivenAll();
  resolveRiven();
}

// "3 Bonus, 1 Malus" — the shape said in words, in whichever language.
const shapeWords = (s) =>
  `${s.bonuses} ${tr("Bonus")}${s.malus ? `, 1 ${tr("Malus")}` : ""}`;

function renderRivenShape() {
  const now = riven.shape || `${riven.bonuses.length}${riven.malus ? "+1" : ""}`;
  const shapeNow = RIVEN_SHAPES.find((x) => x.id === now)
    || RIVEN_SHAPES.find((x) => x.id === RIVEN_SHAPE_DEFAULT);
  $("riven-shape").innerHTML =
    `<span class="rv-lbl">${escHtml(tr("Shape"))}</span><span class="oseg">` +
    RIVEN_SHAPES.map((s) =>
      `<span class="seg ${s.id === now ? "on" : ""}" data-rv="${s.id}"
             title="${escHtml(shapeWords(s))}">${s.id}</span>`).join("") +
    `</span><span class="rv-lbl dim">${escHtml(shapeWords(shapeNow))}</span>`;
  $("riven-shape").querySelectorAll("[data-rv]").forEach((el) => el.onclick = () => {
    const want = el.dataset.rv;
    if (want === riven.shape) return;
    // Park the current shape's stats in its own draft, then adopt the target
    // shape's. Nothing is discarded, so clicking back and forth is free.
    riven.drafts[riven.shape] = { bonuses: riven.bonuses, malus: riven.malus };
    riven.shape = want;
    const s = RIVEN_SHAPES.find((x) => x.id === want);
    const d = riven.drafts[want] || RIVEN_BLANK_DRAFT(s.bonuses, s.malus);
    riven.drafts[want] = d;
    riven.bonuses = d.bonuses;
    riven.malus = d.malus;
    markRivenDirty();
    renderRivens();
  });
}

// One row per rolled stat: the stat, where in its 0.9-1.1 band it landed, and
// what that comes to. The slider's ENDS are the band and the number box is
// clamped to the same, so an illegal roll is not something a control can
// express — "any legal riven and only legal ones" holds by construction.
function renderRivenStats() {
  const rules = rivenRules();
  const row = (slot, s, isMalus) => {
    const def = rivenStat(s.id);
    return `<div class="rv-row ${isMalus ? "malus" : ""}">
      <span class="rv-tag">${escHtml(tr(isMalus ? "Malus" : "Bonus"))}</span>
      <button class="rv-pick" data-slot="${slot}">${def ? escHtml(rivenStatName(def)) : escHtml(tr("choose a stat"))}</button>
      <input class="rv-roll" type="range" data-slot="${slot}"
             min="${rules.roll_min}" max="${rules.roll_max}" step="0.001" value="${s.roll}">
      <input class="rv-num" type="number" data-slot="${slot}" step="0.1" placeholder="—">
      <span class="rv-pct" data-slot="${slot}" title="${escHtml(tr("where this roll landed in its 0.9-1.1 band"))}"></span>
      <span class="rv-mult" data-slot="${slot}" title="${escHtml(tr("the roll itself — the random multiplier this stat drew, 0.900 to 1.100"))}"></span>
      <span class="rv-unit" data-slot="${slot}"></span>
    </div>`;
  };
  $("riven-stats").innerHTML =
    riven.bonuses.map((s, i) => row(String(i), s, false)).join("") +
    (riven.malus ? row("malus", riven.malus, true) : "");

  const at = (slot) => (slot === "malus" ? riven.malus : riven.bonuses[Number(slot)]);
  $("riven-stats").querySelectorAll(".rv-pick").forEach((el) =>
    el.onclick = () => openRivenPicker(el, el.dataset.slot));
  $("riven-stats").querySelectorAll(".rv-roll").forEach((el) => el.oninput = () => {
    at(el.dataset.slot).roll = Number(el.value);
    markRivenDirty();
    resolveRiven();
  });
  // The number box takes the VALUE, not the roll — the number printed on a
  // riven you own. Out of range snaps to the nearest legal end rather than
  // being refused, so typing is never a dead end.
  $("riven-stats").querySelectorAll(".rv-num").forEach((el) => el.onchange = () => {
    if (!at(el.dataset.slot).id || el.value === "") { renderRivenCard(); return; }
    markRivenDirty();
    resolveRiven({ slot: el.dataset.slot, value: Number(el.value) });
  });
}

// The same searchable popover the mod and arcane pickers use — a riven stat
// is picked the way everything else on this page is picked.
function openRivenPicker(anchor, slot) {
  closePopovers();
  // ITS OWN popover. Borrowing the mod picker's nodes dragged the mod
  // picker's sort header in with them, and using it rendered the mod list
  // into this menu (user, 2026-07-31). Separate elements is the only
  // isolation that cannot leak.
  const pop = $("riven-popover");
  const search = $("riven-search");
  const menu = $("riven-menu");
  const at = slot === "malus" ? riven.malus : riven.bonuses[Number(slot)];
  const used = new Set(riven.bonuses.map((x) => x.id).concat(riven.malus ? [riven.malus.id] : []));
  const draw = (q) => {
    const f = (q || "").trim().toLowerCase();
    menu.innerHTML = rivenPool()
      // A stat cannot appear twice on one riven, and five are bonus-only
      // and can never be the malus.
      .filter((x) => (!used.has(x.id) || x.id === at.id) && (slot !== "malus" || x.malus))
      // Both languages match, exactly as the mod picker does.
      .filter((x) => !f || `${rivenStatNameEn(x)} ${rivenStatName(x)}`.toLowerCase().includes(f))
      .map((x) => `<div class="opt ${x.id === at.id ? "search" : ""}" data-rvid="${x.id}">
        <div class="info"><div class="mn">${escHtml(rivenStatName(x))}</div>
        <div class="me">${x.modeled ? "" : `<div>${escHtml(tr("not modeled — it rolls and it names the riven, but it adds no damage"))}</div>`}</div></div>
      </div>`).join("") || `<div class="opt dis">${escHtml(tr("no matching stat"))}</div>`;
    menu.querySelectorAll("[data-rvid]").forEach((el) => el.onclick = () => {
      at.id = el.dataset.rvid;
      closePopovers();
      markRivenDirty();
      renderRivens();
    });
  };
  const r = anchor.getBoundingClientRect();
  pop.style.left = `${Math.max(8, Math.min(window.innerWidth - 340, r.left))}px`;
  pop.style.top = `${r.bottom + window.scrollY + 4}px`;
  pop.hidden = false;
  search.value = "";
  search.oninput = () => draw(search.value);
  draw("");
  search.focus();
}

function renderRivenFoot() {
  const rules = rivenRules();
  const pols = rules.polarities || ["madurai", "vazarin", "naramon"];
  // Polarity is a SYMBOL everywhere on this page; a name here would be the
  // one place it is spelled out.
  const cap = (s) => s.charAt(0).toUpperCase() + s.slice(1);
  $("riven-foot").innerHTML =
    `<label class="rv-lbl">${escHtml(tr("Rank"))} <input id="rv-rank" type="range" min="0" max="${rules.max_rank}" step="1" value="${riven.rank}">` +
    `<b id="rv-rank-n">${riven.rank}</b></label>` +
    `<span class="rv-lbl">${escHtml(tr("Polarity"))}</span><span class="oseg">` +
    pols.map((x) => `<span class="seg pol ${riven.polarity === x ? "on" : ""}" data-pol="${x}" title="${cap(x)}">${imgTag(POL(cap(x)), "pol")}</span>`).join("") +
    `</span>` +
    `<button class="ghost-btn small" id="rv-max">${escHtml(tr("roll everything maximal"))}</button>`;
  $("rv-rank").oninput = () => {
    riven.rank = Number($("rv-rank").value);
    $("rv-rank-n").textContent = riven.rank;
    markRivenDirty();
    resolveRiven();
  };
  $("riven-foot").querySelectorAll("[data-pol]").forEach((el) => el.onclick = () => {
    riven.polarity = el.dataset.pol; markRivenDirty(); renderRivenFoot(); resolveRiven();
  });
  // The ceiling, in one click: every bonus at the top of its band and the
  // malus at the bottom, which is the least harmful it can be.
  $("rv-max").onclick = () => {
    riven.bonuses.forEach((s) => { s.roll = rules.roll_max; });
    if (riven.malus) riven.malus.roll = rules.roll_min;
    markRivenDirty();
    renderRivens();
  };
}

// Every riven saved for this weapon, with what it actually rolls — the
// collection bar shows names, this shows the numbers you choose between
// (user, 2026-07-31). The values are the engine's, from the same refresh the
// mod lists use, so nothing here is a second opinion.
function renderRivenAll() {
  const box = $("riven-all");
  if (!box) return;
  const ps = loadPresetList(RIVENS);
  const active = activeRivenName();
  const cap = (s) => String(s || "").replace(/^./, (c) => c.toUpperCase());
  box.innerHTML = ps.length
    ? ps.map((p) => {
        const st = p.state || {};
        const meta = rivenNames[p.name] || {};
        const lines = meta.lines || [];
        const shape = st.shape || `${(st.bonuses || st.positives || []).length}${st.malus || st.curse ? "+1" : ""}`;
        const nBonus = (st.bonuses || st.positives || []).length;
        return `<div class="rv-all ${p.name === active ? "sel" : ""}" data-open="${escHtml(p.name)}">
          <div class="rv-all-h">
            ${imgTag(POL(cap(st.polarity || "madurai")), "pol")}
            <b>${escHtml(p.name)}</b>
            ${meta.name ? `<span class="rv-official">${escHtml(meta.name)}</span>` : ""}
            <span class="rv-meta">${shape} · ${escHtml(tr("Rank"))} ${st.rank ?? 8} · ${2 + 2 * (st.rank ?? 8)} ${escHtml(tr("capacity"))}</span>
          </div>
          <div class="rv-all-s">${
            lines.length
              ? lines.map((x, i) => `<span class="rv-chip ${i >= nBonus ? "neg" : ""}">${escHtml(tf(x))}</span>`).join("")
              : `<span class="sim-empty">${escHtml(tr("nothing rolled yet"))}</span>`
          }</div>
        </div>`;
      }).join("")
    : `<div class="sim-empty">${escHtml(tr("no rivens for this weapon yet"))}</div>`;
  // Clicking one opens it, the same as clicking its chip in the bar.
  // Clicking a card OPENS it — the list is the folder, this is the file.
  box.querySelectorAll("[data-open]").forEach((el) => el.onclick = () => {
    const p = loadPresetList(RIVENS).find((x) => x.name === el.dataset.open);
    if (!p) return;
    activeRiven = p.name;
    localStorage.setItem(presetActiveKey(RIVENS), activeRiven);
    riven = null;   // renderRivens re-reads the file it is being asked to open
    renderRivens();
  });
}

function renderRivenCard() {
  const r = rivenResolved;
  const box = $("riven-stats");
  // Blank every row first, so a slot that lost its stat does not keep an old
  // number sitting next to it.
  box.querySelectorAll(".rv-num").forEach((el) => { el.value = ""; el.disabled = true; });
  box.querySelectorAll(".rv-unit").forEach((el) => { el.textContent = ""; el.className = "rv-unit"; });
  box.querySelectorAll(".rv-pct").forEach((el) => { el.textContent = ""; el.className = "rv-pct"; });
  box.querySelectorAll(".rv-mult").forEach((el) => { el.textContent = ""; el.className = "rv-mult"; });
  (r && r.stats || []).forEach((s) => {
    const num = box.querySelector(`.rv-num[data-slot="${s.slot}"]`);
    const unit = box.querySelector(`.rv-unit[data-slot="${s.slot}"]`);
    const pct = box.querySelector(`.rv-pct[data-slot="${s.slot}"]`);
    const mult = box.querySelector(`.rv-mult[data-slot="${s.slot}"]`);
    // The CARD's precision, which is all anyone can read off a riven they
    // own. The roll behind it stays exact — this is the reading, not the
    // number the sim uses.
    const d = s.decimals ?? 2;
    if (num) {
      num.disabled = false;
      num.value = s.shown.toFixed(d);
      num.step = (0.1 ** d).toFixed(d);
      // The ends of the legal band, so the browser guards the box too.
      num.min = Math.min(s.min, s.max).toFixed(d);
      num.max = Math.max(s.min, s.max).toFixed(d);
      // In the box's OWN units: a faction stat holds a multiplier, so its
      // band reads x0.68 … x0.74 and never looks like a percentage.
      const u = (n) => (s.unit === "x" ? `x${n}` : `${n}${s.unit || ""}`);
      num.title = `legal range ${u(num.min)} … ${u(num.max)}`;
    }
    if (pct) {
      // How good the ROLL is, with disposition, shape and base divided out —
      // so two stats on one card compare, and so do two cards.
      const q = Math.round(s.percentile ?? 50);
      // A bare number in ANGLE brackets. An ordinal suffix read as clutter
      // (user, 2026-07-31), and parentheses were taken — DE's own stat text
      // already uses them ("(x2 for Bows)").
      pct.textContent = `<${q}>`;
      pct.className = `rv-pct${q >= 90 ? " top" : ""}${q <= 10 ? " low" : ""}`;
      pct.title = tr("where this roll landed in its 0.9-1.1 band");
    }
    // ...and the ROLL ITSELF, next to it. The percentile says how good the
    // draw was, which is the reading you want when comparing two cards; the
    // multiplier is the number the game actually drew, which is the one you
    // can check against a riven you own (suggested by a player, 2026-08-03).
    // Both, because neither substitutes for the other: <100> and 1.100 are
    // the same fact, <50> and 1.000 are not obviously so, and a MALUS reads
    // backwards — its best draw is the SMALLEST multiplier.
    if (mult) {
      mult.textContent = Number(s.roll).toFixed(3);
      mult.className = "rv-mult";
      mult.title = tr("the roll itself — the random multiplier this stat drew, 0.900 to 1.100");
    }
    if (unit) {
      unit.textContent = tf(s.text);
      unit.className = `rv-unit${s.value < 0 ? " neg" : ""}${s.modeled ? "" : " unmodeled"}`;
    }
    const roll = box.querySelector(`.rv-roll[data-slot="${s.slot}"]`);
    if (roll) roll.value = s.roll;
  });
  const bad = (r && r.illegal) || [];
  if (!rivenComplete()) {
    const n = riven.bonuses.filter((s) => !s.id).length + (riven.malus && !riven.malus.id ? 1 : 0);
    const msg = n === 1
      ? tr("one more stat to finish this riven")
      : tr("pick {n} more stats to finish this riven").replace("{n}", n);
    $("riven-card").innerHTML = `<div class="rv-meta">${escHtml(msg)}</div>`;
    return;
  }
  $("riven-card").innerHTML = bad.length
    ? `<div class="error"><b>${escHtml(tr("not a legal riven"))}</b><ul>${bad.map((x) => `<li>${escHtml(x)}</li>`).join("")}</ul></div>`
    : `<div class="rv-name">${escHtml(r.name)}</div>
       <div class="rv-meta">${r.drain} ${escHtml(tr("capacity"))} · ${escHtml(tr(r.class))} ${escHtml(tr("riven"))} · ${escHtml(tr("disposition"))} ${Number(r.disposition).toFixed(2)}</div>`;
}

// Rivens are a PRESET COLLECTION, on the same bar as builds and the
// optimizer's scopes: "+ new" is a new riven, ⧉ duplicates one to branch a
// roll, ✎ renames, ✕ deletes. Edits auto-save into the active riven, so
// there is no save button — the same contract as everywhere else.
const markRivenDirty = () => { if (typeof markPresetDirty === "function") markPresetDirty(); saveRivenSoon(); };
let rivenSaveTimer = null;
// A riven is CONSUMED by the builder (it is a mod in the pool), so deleting
// one has a consequence no preset delete has: a slot can be left pointing at
// an id that no longer exists. Nothing downstream would say so — the slot
// renders blank and the panel quietly prices a build with a hole in it.
function pruneDanglingRivens() {
  let hit = false;
  slots.forEach((s) => {
    if (isRivenId(s.mod) && !modById(s.mod)) {
      s.mod = null; s.rank = null; hit = true;
    }
  });
  if (hit) { renderMods(); refreshPanel(); markPresetDirty(); }
}

function saveRivenSoon() {
  clearTimeout(rivenSaveTimer);
  rivenSaveTimer = setTimeout(() => {
    const ps = loadPresetList(RIVENS);
    const name = activeRivenName();
    const i = ps.findIndex((p) => p.name === name);
    if (i >= 0) {
      ps[i].state = snapshotRiven();
      storePresetList(RIVENS, ps);
      renderRivenTools();
      // The mod lists show each riven's generated name and printed values, so
      // they have to be re-asked for after an edit.
      refreshRivenNames();
    }
  }, 250);
}
// The saved shape keeps `bonuses`/`malus` at the top level — that is what
// the engine reads — and carries the other three drafts alongside so a
// reload does not flatten them back into one.
const snapshotRiven = () => ({
  bonuses: riven.bonuses, malus: riven.malus, rank: riven.rank, polarity: riven.polarity,
  shape: riven.shape, drafts: riven.drafts,
});
let activeRiven = null;
const activeRivenName = () => activeRiven || localStorage.getItem(presetActiveKey(RIVENS)) || "";

// A custom is a FILE (user, 2026-08-02). Two modes, and the tools strip says
// which one you are in:
//
//   LIST — nothing open. Make one, or bring one over from another weapon.
//          This is a real state, not an empty editor: most weapons own no
//          riven and the page should say so rather than show a blank card.
//   EDIT — one open. Its identity is named, the editor below is live, and
//          "← back" closes it without deleting anything.
//
// It is deliberately NOT the preset bar: a preset is a label you invented for
// a state your module is always in, and switching one is switching state. A
// riven is a thing with its own name, capacity and polarity, which other
// modules then consume. Same storage, same undo, same import underneath —
// different noun on top.
// ---- CUSTOM ENEMIES: a target you MADE ---------------------------------
//
// A CUSTOM in the sense AGENTS.md gives the word: a thing you made that the
// OTHER modules consume — here, an entry in the scenario's target list, which
// is what makes it reach the simulator and the optimizer with no code of its
// own in either. Same shape as the riven editor: a list you pick from, one open
// at a time, none open being a real state.
//
// NOT WEAPON-SCOPED, unlike a riven. A riven is a statement about one weapon; a
// target is not, any more than a fight is — so this collection is SHARED across
// the roster (`SHARED_DOMAINS`) and there is no "⇤ import", because there is no
// other weapon to import from.
//
// THE ID IS THE NAME, and renaming repoints whatever names it. A custom exists
// only on the machine that made it, so its id has to come from something the
// player typed rather than from a table nobody can see.
const ENEMIES = "enemies";
const enemyId = (name) => "custom:" + name;
let activeEnemy = null;
let enemyDoc = null;

const DAMAGE_TYPES = ["impact", "puncture", "slash", "heat", "cold", "electricity",
  "toxin", "blast", "corrosive", "gas", "magnetic", "radiation", "viral", "void", "true"];
const SCALING_FACTIONS = ["grineer", "corpus", "infested", "corrupted", "unaffiliated"];

// The blank target: a plain humanoid with nothing unusual about it. Every
// number here is one the player is expected to replace — it is a starting
// point, not a claim about anything in game, which is what `synthetic` says.
const blankEnemy = () => ({
  synthetic: true,
  faction: "grineer",
  scaling_faction: "grineer",
  can_be_eximus: false,
  // A DIFFERENT MECHANIC from taking no damage of a type — see the engine's
  // `status_immunities`. This one moves the PROC DISTRIBUTION: an immune type
  // leaves the denominator and the rest renormalize onto the roll.
  status_immunities: [],
  stats: { base_level: 1, health: 1000, shield: 0, armor: 0, overguard: 0, affinity: 0 },
  // null = take the faction's own column. The moment a player writes one
  // value, the whole column becomes theirs — see `renderEnemyForm`.
  damage_modifiers: null,
  body_parts: [
    { name: "body", multiplier: 1, is_head: false, crit_bonus: false },
    { name: "head", multiplier: 3, is_head: true, crit_bonus: true },
  ],
});

const activeEnemyName = () => {
  if (activeEnemy === null) activeEnemy = localStorage.getItem(presetActiveKey(ENEMIES));
  return activeEnemy;
};
const snapshotEnemy = () => JSON.parse(JSON.stringify(enemyDoc));

/// Every custom target as an ENGINE enemy spec — the same type a published unit
/// has, which is why nothing downstream learns that an enemy can be homemade.
function customEnemySpecs() {
  return loadPresetList(ENEMIES).map((p) => ({
    ...blankEnemy(), ...(p.state || {}),
    id: enemyId(p.name),
    name: p.name,
  }));
}

/// …and as TARGET CARDS, in the shape `/api/meta` publishes, so the picker, the
/// card and the optimizer's read-only view need no branch for them.
function customEnemyCards() {
  const cols = Object.fromEntries((META.factions || []).map((f) => [f.id, f.modifiers]));
  return customEnemySpecs().map((e) => ({
    id: e.id,
    name: e.name,
    custom: true,
    synthetic: true,
    image: null,
    base_level: e.stats.base_level,
    can_be_eximus: !!e.can_be_eximus,
    faction: e.faction || "unknown",
    scaling: e.scaling_faction,
    health: e.stats.health,
    shield: e.stats.shield,
    armor: e.stats.armor,
    overguard: e.stats.overguard,
    unmodeled: [],
    status_immunities: e.status_immunities || [],
    type_modifiers: e.damage_modifiers
      ? Object.entries(e.damage_modifiers).filter(([, v]) => v !== 1)
          .map(([type, mult]) => ({ type, mult }))
      : (cols[e.faction] || []),
    parts: e.body_parts.map((b) => ({ name: b.name, multiplier: b.multiplier, is_head: b.is_head })),
  }));
}

/// THE TARGET LIST, published plus made. One function, because every reader of
/// `META.enemies` is asking "what can this fight be against", and the answer
/// stopped being the roster the moment a player could add to it.
const allEnemies = () => (META.enemies || []).concat(customEnemyCards());
const enemyCard = (id) => allEnemies().find((e) => e.id === id);

/// The custom targets a request has to carry, given the fight it describes.
/// Empty for a published unit — the server has heard of those.
const customEnemiesFor = (id) => customEnemySpecs().filter((e) => e.id === id);

/// THE FIGHT AS A REQUEST BODY. Every path that sends a scenario goes through
/// here, so "a custom target travels with the fight that names it" is one rule
/// rather than one per endpoint — the same reason a riven rides in `rivens`.
const fightPayload = (st) => {
  const s = st || sim;
  // THE RUN COUNT RIDES HERE and not in the scenario, so every path that sends
  // a fight sends the page's own count — and a caller that wants another one
  // still overrides it after the spread, which is what the quick calc and the
  // panel probes do.
  return { ...s, runs: simRuns(), custom_enemies: customEnemiesFor(s.enemy) };
};

function renderEnemies() {
  if (!META || !$("enemy-block")) return;
  const ps = loadPresetList(ENEMIES);
  const open = ps.find((p) => p.name === activeEnemyName());
  $("enemy-sub").textContent = ps.length
    ? `${ps.length} ${tr("saved")}`
    : tr("none yet — a target you build here appears in every scenario's target list");
  renderEnemyTools();
  if (!open) {
    // LIST MODE — nothing is being edited, so nothing pretends to be.
    enemyDoc = null;
    if ($("enemy-form")) $("enemy-form").innerHTML = "";
    renderEnemyAll();
    return;
  }
  if (!enemyDoc) enemyDoc = JSON.parse(JSON.stringify({ ...blankEnemy(), ...(open.state || {}) }));
  renderEnemyForm();
  renderEnemyAll();
}

function saveEnemyDoc() {
  const ps = loadPresetList(ENEMIES);
  const i = ps.findIndex((p) => p.name === activeEnemyName());
  if (i < 0) return;
  ps[i] = { ...ps[i], savedAt: Date.now(), state: snapshotEnemy() };
  storePresetList(ENEMIES, ps);
  // The SCENARIO may be pointing at this target right now, and its card shows
  // the numbers being edited. Redraw it rather than leaving a stale target on
  // a tab the editor cannot see.
  renderSimTargetIfAny();
}

function renderSimTargetIfAny() {
  if (typeof renderEnemy === "function") renderEnemy();
  if (typeof renderOptEnemy === "function") renderOptEnemy();
}

function renderEnemyAll() {
  const box = $("enemy-all");
  if (!box) return;
  const ps = loadPresetList(ENEMIES);
  box.innerHTML = ps.length
    ? ps.map((p) => {
        const s = { ...blankEnemy(), ...(p.state || {}) };
        const parts = s.body_parts.map((b) => `${escHtml(b.name)} ×${b.multiplier}`).join(", ");
        return `<div class="en-row" data-en="${escHtml(p.name)}">
          <span class="nm">${escHtml(p.name)}</span>
          <span class="sm">${escHtml(s.faction || "unknown")} · ${s.stats.health} HP · ${s.stats.armor} ${escHtml(tr("Armor"))} · ${s.stats.shield} ${escHtml(tr("Shield"))}</span>
          <span class="sm">${parts}</span>
          ${enemyId(p.name) === sim.enemy ? `<span class="sm">✓ ${escHtml(tr("in the current fight"))}</span>` : ""}
        </div>`;
      }).join("")
    : `<div class="placeholder">${escHtml(tr("no targets yet"))}</div>`;
  box.querySelectorAll(".en-row").forEach((el) =>
    el.addEventListener("click", () => {
      activeEnemy = el.dataset.en;
      localStorage.setItem(presetActiveKey(ENEMIES), activeEnemy);
      enemyDoc = null;
      renderEnemies();
    })
  );
}

function renderEnemyTools() {
  const box = $("enemy-tools");
  if (!box) return;
  const ps = loadPresetList(ENEMIES);
  const cur = ps.find((x) => x.name === activeEnemyName());
  if (!cur) {
    box.innerHTML =
      `<button class="cu-btn cu-new">+ ${escHtml(tr("new target"))}</button>` +
      `<span class="cu-ops">${undoButtons(ENEMIES)}</span>`;
  } else {
    box.innerHTML =
      `<button class="cu-btn cu-back">← ${escHtml(tr("all targets"))}</button>` +
      `<span class="cu-open"><b>${escHtml(cur.name)}</b></span>` +
      `<span class="cu-ops">` +
      `<button class="cu-btn cu-dup" title="${escHtml(tr("duplicate"))}">⧉</button>` +
      `<button class="cu-btn cu-ren" title="${escHtml(tr("rename"))}">✎</button>` +
      `<button class="cu-btn cu-del" title="${escHtml(tr("delete"))}">✕</button>` +
      undoButtons(ENEMIES) +
      `</span>`;
  }
  wireUndoButtons(box, ENEMIES);
  const q = (s) => box.querySelector(s);
  const openIt = (name) => {
    activeEnemy = name;
    if (name) localStorage.setItem(presetActiveKey(ENEMIES), name);
    else localStorage.removeItem(presetActiveKey(ENEMIES));
    enemyDoc = null;
    renderEnemies();
  };
  const click = (sel, fn) => { const b = q(sel); if (b) b.onclick = (e) => { e.stopPropagation(); fn(); }; };

  click(".cu-new", () => {
    const ps2 = loadPresetList(ENEMIES);
    const name = freeName(ps2, (n) => "target " + n);
    ps2.push({ name, savedAt: Date.now(), state: blankEnemy() });
    storePresetList(ENEMIES, ps2);
    openIt(name);
  });
  click(".cu-back", () => openIt(null));
  click(".cu-dup", () => {
    const ps2 = loadPresetList(ENEMIES);
    const name = freeName(ps2, (n) => `${cur.name} (${n})`);
    ps2.push({ name, savedAt: Date.now(), state: JSON.parse(JSON.stringify(cur.state)) });
    storePresetList(ENEMIES, ps2);
    openIt(name);
  });
  click(".cu-del", () => {
    const ps2 = loadPresetList(ENEMIES).filter((x) => x.name !== cur.name);
    storePresetList(ENEMIES, ps2);
    // DELETING A CUSTOM BREAKS REFERENCES, and this is the one it can break:
    // the fight may be pointing at it. It falls back to the roster's first unit
    // rather than to an id nothing answers to.
    if (sim.enemy === enemyId(cur.name)) {
      sim.enemy = ((META.enemies || [])[0] || {}).id || "thrax_centurion";
      markPresetDirty();
      renderSimTargetIfAny();
    }
    openIt(null);
  });
  click(".cu-ren", () => {
    // NO NATIVE DIALOGS (AGENTS.md): an inline input, as everywhere else.
    const span = q(".cu-open");
    span.innerHTML = `<input class="cu-rename" type="text" value="${escHtml(cur.name)}">`;
    const inp = span.querySelector("input");
    inp.focus(); inp.select();
    let done = false;
    const commit = () => {
      if (done) return;
      done = true;
      const name = inp.value.trim();
      const ps2 = loadPresetList(ENEMIES);
      if (!name || name === cur.name || ps2.some((x) => x.name === name)) return renderEnemyTools();
      const i = ps2.findIndex((x) => x.name === cur.name);
      ps2[i] = { ...ps2[i], name };
      storePresetList(ENEMIES, ps2);
      // The id IS the name, so a rename moves whatever names it — otherwise the
      // fight would point at a target that no longer exists.
      if (sim.enemy === enemyId(cur.name)) { sim.enemy = enemyId(name); markPresetDirty(); }
      openIt(name);
      renderSimTargetIfAny();
    };
    inp.onkeydown = (e) => { if (e.key === "Enter") commit(); if (e.key === "Escape") { done = true; renderEnemyTools(); } };
    inp.onblur = commit;
  });
}

function renderEnemyForm() {
  const box = $("enemy-form");
  if (!box) return;
  const d = enemyDoc;
  const cols = Object.fromEntries((META.factions || []).map((f) => [f.id, f.modifiers]));
  const factionCol = Object.fromEntries((cols[d.faction] || []).map((m) => [m.type, m.mult]));
  const own = d.damage_modifiers;
  const num = (k, label, val, step) =>
    `<label>${escHtml(label)}<input type="number" data-en-k="${k}" value="${val}" step="${step || 1}" min="0"></label>`;
  const opts = (list, cur) => list.map((x) =>
    `<option value="${escHtml(x)}"${x === cur ? " selected" : ""}>${escHtml(x)}</option>`).join("");
  box.innerHTML =
    `<div class="en-grid">
      <label>${escHtml(tr("Faction"))}
        <select data-en-k="faction">${opts(["unknown"].concat((META.factions || []).map((f) => f.id)), d.faction || "unknown")}</select></label>
      <label>${escHtml(tr("Level scaling"))}
        <select data-en-k="scaling_faction">${opts(SCALING_FACTIONS, d.scaling_faction)}</select></label>
      ${num("stats.base_level", tr("Base level"), d.stats.base_level)}
      ${num("stats.health", tr("Health"), d.stats.health)}
      ${num("stats.shield", tr("Shield"), d.stats.shield)}
      ${num("stats.armor", tr("Armor"), d.stats.armor)}
      ${num("stats.overguard", tr("Overguard"), d.stats.overguard)}
      <label>${escHtml(tr("Eximus possible"))}
        <input type="checkbox" data-en-k="can_be_eximus"${d.can_be_eximus ? " checked" : ""}></label>
    </div>` +
    // THE VULNERABILITY COLUMN, and the two states it has. A FACTION ALREADY
    // ANSWERS THIS — that is what a faction is to incoming damage — so writing
    // your own is a deliberate second state rather than a set of blanks to
    // fill. IMMUNITY IS 0 HERE, not a checkbox of its own: the game has no
    // third state between a multiplier and nothing getting through.
    `<div class="en-sect"><b>${escHtml(tr("Damage taken"))}</b>
      <label><input type="checkbox" id="en-own-col"${own ? " checked" : ""}> ${
        escHtml(tr("write my own column (0 = takes none of that type)"))}</label>
      ${own ? "" : `<span>${escHtml(tr("from the faction"))}: ${
        (cols[d.faction] || []).map((m) => `${escHtml(DT(m.type))} ×${m.mult}`).join(", ")
        || escHtml(tr("takes every type as written"))}</span>`}
    </div>` +
    (own
      ? `<div class="en-mods">${DAMAGE_TYPES.map((k) =>
          `<label>${escHtml(DT(k))}<input type="number" data-en-dm="${k}" step="0.1" min="0"
             value="${own[k] === undefined ? (factionCol[k] === undefined ? 1 : factionCol[k]) : own[k]}"></label>`).join("")}</div>`
      : "") +
    // STATUS IMMUNITY, and it is a SECTION OF ITS OWN rather than a 0 in the
    // column above, because it is a different mechanic and the difference is
    // not a detail (owner, 2026-08-11). Taking no DAMAGE of a type does not
    // stop that type from being drawn for procs; a status immunity removes it
    // from the draw and the remaining types RENORMALIZE — the wiki's own worked
    // example moves the other four from 18/5/9/23% to 33/8/17/42% when
    // Corrosive leaves. So an enemy can be immune to one and not the other, and
    // the page has to let a player say which.
    `<div class="en-sect"><b>${escHtml(tr("Status immunity"))}</b>
      <span>${escHtml(tr("these procs cannot land — the other types take over their share of the roll"))}</span></div>
     <div class="en-mods">${DAMAGE_TYPES.map((k) =>
       `<label><input type="checkbox" data-en-si="${k}"${
         (d.status_immunities || []).includes(k) ? " checked" : ""}> ${escHtml(DT(k))}</label>`).join("")}</div>` +
    // BODY PARTS — the weak points, and what each one is worth. A head is not a
    // NAME: `is_head` is what a headshot-conditional mod asks about, and
    // `crit_bonus` is the separate question of whether the part also multiplies
    // critical damage.
    `<div class="en-sect"><b>${escHtml(tr("Body parts"))}</b>
      <button class="cu-btn" id="en-add-part">+ ${escHtml(tr("part"))}</button></div>
     <div class="en-parts">${d.body_parts.map((b, i) => `
      <div class="en-part" data-i="${i}">
        <input type="text" data-en-p="name" value="${escHtml(b.name)}">
        <input type="number" data-en-p="multiplier" value="${b.multiplier}" step="0.1" min="0">
        <label><input type="checkbox" data-en-p="is_head"${b.is_head ? " checked" : ""}> ${escHtml(tr("head"))}</label>
        <label><input type="checkbox" data-en-p="crit_bonus"${b.crit_bonus ? " checked" : ""}> ${escHtml(tr("crit bonus"))}</label>
        ${d.body_parts.length > 1 ? `<button class="cu-btn en-del-part">✕</button>` : ""}
      </div>`).join("")}</div>`;

  const commit = () => { saveEnemyDoc(); renderEnemyForm(); renderEnemyAll(); };
  const setPath = (path, val) => {
    const [a, b] = path.split(".");
    if (b) d[a][b] = val; else d[a] = val;
  };
  box.querySelectorAll("[data-en-k]").forEach((el) => {
    el.onchange = () => {
      setPath(el.dataset.enK, el.type === "checkbox" ? el.checked
        : el.type === "number" ? Math.max(0, Number(el.value) || 0)
        : el.value);
      commit();
    };
  });
  const oc = $("en-own-col");
  if (oc) oc.onchange = () => {
    // Switching ON copies the faction's column in, so the starting point is
    // what this target already was rather than fifteen ones.
    d.damage_modifiers = oc.checked
      ? Object.fromEntries(DAMAGE_TYPES.map((k) => [k, factionCol[k] === undefined ? 1 : factionCol[k]]))
      : null;
    commit();
  };
  box.querySelectorAll("[data-en-si]").forEach((el) => {
    el.onchange = () => {
      const k = el.dataset.enSi;
      const cur = new Set(d.status_immunities || []);
      if (el.checked) cur.add(k); else cur.delete(k);
      d.status_immunities = DAMAGE_TYPES.filter((x) => cur.has(x));
      commit();
    };
  });
  box.querySelectorAll("[data-en-dm]").forEach((el) => {
    el.onchange = () => {
      d.damage_modifiers = { ...(d.damage_modifiers || {}), [el.dataset.enDm]: Math.max(0, Number(el.value) || 0) };
      commit();
    };
  });
  box.querySelectorAll(".en-part").forEach((row) => {
    const i = Number(row.dataset.i);
    row.querySelectorAll("[data-en-p]").forEach((el) => {
      el.onchange = () => {
        const k = el.dataset.enP;
        d.body_parts[i][k] = el.type === "checkbox" ? el.checked
          : k === "name" ? (el.value.trim() || "part")
          : Math.max(0, Number(el.value) || 0);
        commit();
      };
    });
    const del = row.querySelector(".en-del-part");
    if (del) del.onclick = () => { d.body_parts.splice(i, 1); commit(); };
  });
  const add = $("en-add-part");
  if (add) add.onclick = () => {
    d.body_parts.push({ name: "part", multiplier: 1, is_head: false, crit_bonus: false });
    commit();
  };
}

function renderRivenTools() {
  const box = $("riven-tools");
  if (!box) return;
  const ps = loadPresetList(RIVENS);
  const open = activeRivenName();
  const cur = ps.find((x) => x.name === open);
  const impAvailable = presetSources(RIVENS, presetWeapon()).length > 0;
  const impBtn = impAvailable
    ? `<button class="cu-btn cu-imp" title="${escHtml(tr("copy a riven from another weapon"))}">⇤ ${escHtml(tr("import"))}</button>`
    : "";
  if (!cur) {
    box.innerHTML =
      `<button class="cu-btn cu-new">+ ${escHtml(tr("new riven"))}</button>${impBtn}` +
      `<span class="cu-ops">${undoButtons(RIVENS)}</span>` +
      `<div class="cu-import" hidden></div>`;
  } else {
    const official = (rivenNames[cur.name] || {}).name || "";
    box.innerHTML =
      `<button class="cu-btn cu-back">← ${escHtml(tr("all rivens"))}</button>` +
      `<span class="cu-open"><b>${escHtml(cur.name)}</b>${
        official ? `<span class="rv-official">${escHtml(official)}</span>` : ""}</span>` +
      `<span class="cu-ops">` +
      `<button class="cu-btn cu-dup" title="${escHtml(tr("duplicate"))}">⧉</button>` +
      `<button class="cu-btn cu-ren" title="${escHtml(tr("rename"))}">✎</button>` +
      `<button class="cu-btn cu-del" title="${escHtml(tr("delete"))}">✕</button>` +
      undoButtons(RIVENS) +
      `</span><div class="cu-import" hidden></div>`;
  }
  wireUndoButtons(box, RIVENS);
  const q = (s) => box.querySelector(s);
  const openIt = (name) => {
    activeRiven = name;
    if (name) localStorage.setItem(presetActiveKey(RIVENS), name);
    else localStorage.removeItem(presetActiveKey(RIVENS));
    riven = null;
    renderRivens();
  };
  const click = (sel, fn) => { const b = q(sel); if (b) b.onclick = (e) => { e.stopPropagation(); fn(); }; };

  click(".cu-new", () => {
    const ps2 = loadPresetList(RIVENS);
    const name = freeName(ps2, (n) => "riven " + n);
    riven = { ...withDrafts(blankRiven()), __weapon: $("weapon").value };
    ps2.push({ name, savedAt: Date.now(), state: snapshotRiven() });
    storePresetList(RIVENS, ps2);
    openIt(name);
  });
  click(".cu-back", () => openIt(""));
  click(".cu-dup", () => {
    const ps2 = loadPresetList(RIVENS);
    const name = freeName(ps2, (n) => open + " copy" + (n > 1 ? " " + n : ""));
    ps2.push({ name, savedAt: Date.now(), state: snapshotRiven() });
    storePresetList(RIVENS, ps2);
    openIt(name);
  });
  click(".cu-del", () => {
    storePresetList(RIVENS, loadPresetList(RIVENS).filter((x) => x.name !== open));
    // Back to the LIST, not to another riven: deleting the thing you had open
    // is not a request to open a different one.
    openIt("");
    pruneDanglingRivens();
  });
  // Renaming happens in an INLINE input — no prompt(), which the owner's
  // browser blocks. Enter commits, Esc cancels.
  click(".cu-ren", () => {
    const host = q(".cu-open");
    host.innerHTML = `<input class="cu-name" type="text" maxlength="24" value="${escHtml(open)}">`;
    const inp = q(".cu-name");
    inp.focus(); inp.select();
    let done = false;
    const commit = (ok) => {
      if (done) return;
      done = true;
      const want = (inp.value || "").trim();
      if (!ok || !want || want === open) return renderRivenTools();
      const ps2 = loadPresetList(RIVENS);
      if (ps2.some((x) => x.name === want)) return renderRivenTools();
      const at = ps2.findIndex((x) => x.name === open);
      if (at < 0) return renderRivenTools();
      ps2[at] = { ...ps2[at], name: want };
      storePresetList(RIVENS, ps2);
      // The id a slot holds is `riven:<name>`, so a rename moves the item the
      // builder is pointing at — follow it rather than orphan the slot.
      slots.forEach((s) => { if (s.mod === RIVEN_PREFIX + open) s.mod = RIVEN_PREFIX + want; });
      openIt(want);
      renderMods(); refreshPanel();
    };
    inp.onkeydown = (ev) => {
      if (ev.key === "Enter") commit(true);
      if (ev.key === "Escape") commit(false);
    };
    inp.onblur = () => commit(true);
  });
  click(".cu-imp", () => {
    const panel = q(".cu-import");
    if (!panel.hidden) { panel.hidden = true; return; }
    panel.innerHTML = presetSources(RIVENS, presetWeapon()).map((w) =>
      `<div class="pimp-w"><span class="pimp-wn">${escHtml(w.name)}</span>` +
      w.presets.map((x) =>
        `<span class="cu-btn pimp-p" data-weapon="${escHtml(w.id)}" data-name="${escHtml(x.name)}">${escHtml(x.name)}</span>`
      ).join("") + `</div>`).join("");
    panel.hidden = false;
    panel.querySelectorAll(".pimp-p").forEach((el) => el.onclick = (ev) => {
      ev.stopPropagation();
      const from = loadPresetList(RIVENS, el.dataset.weapon).find((x) => x.name === el.dataset.name);
      if (!from) return;
      const ps2 = loadPresetList(RIVENS);
      const name = freeName(ps2, (n) => el.dataset.name + (n > 1 ? " " + n : ""));
      // A riven's VALUES are its weapon's disposition applied to a roll, so
      // the copy is the roll — the numbers re-derive here, which is the whole
      // reason importing one is useful.
      riven = { ...withDrafts(from.state), __weapon: $("weapon").value };
      ps2.push({ name, savedAt: Date.now(), state: snapshotRiven() });
      storePresetList(RIVENS, ps2);
      openIt(name);
    });
  });
}

// ---- SHARING — a link that reproduces the whole thing ------------------
//
// The principle (user, 2026-08-02): everything travels, and nothing has to be
// set up by hand on arrival. Most build links elsewhere carry the mods and
// leave you to re-enter the enemy, the buffs, the riven — so the number the
// sharer quoted is one you cannot reproduce. Here the payload is the build,
// the rivens it equips, the fight it was measured in, and the measurement
// itself, and opening it creates a fresh copy of each. Nothing is overwritten.
//
// It rides the QUERY, not the fragment (user): a fragment never reaches a
// crawler, and the link is meant to be posted, previewed and looked at.
//
// Encoding: JSON -> deflate-raw -> base64url, behind a one-character version
// so the reader knows what it is holding. A full share — build + riven +
// scenario + result — is ~600 characters, which no chat app will break. IDs
// travel as their own stable English slugs rather than as indices into a
// table: the table would have to be append-only forever, and one reordering
// would silently reinterpret every link ever posted.
const SHARE_PARAM = "b";
/// SHARING IS OFF (owner, 2026-08-07).
///
/// Two people reported a shared link opening blank on the same day and neither
/// case could be reproduced here — the live site produced a link and read it
/// back correctly under every condition tried. An unreproducible failure in the
/// one feature whose whole job is to be pasted somewhere public is not a
/// feature to leave running while it is investigated.
///
/// BOTH HALVES ARE OFF, and the import half matters more: a link already posted
/// must not open blank. With this false an incoming `?b=` is dropped, the query
/// is stripped, and the visitor lands on the weapon's own page with a line
/// saying why — which is the worst case a posted link should ever reach.
///
/// Nothing about the codec is deleted. `sharePayload`/`decodeShare` and their
/// tests stand, so turning this back to true is the whole of turning it back
/// on. What it still needs before that: the payload does not carry the build's
/// MODE, so a shared build is reproduced played the wrong way — the share rule
/// is that a link reproduces the whole thing, and it stopped being true the day
/// mode became part of a build.
const SHARE_ENABLED = false;
const SHARE_V_DEFLATE = "1";
const SHARE_V_PLAIN = "0";

const b64urlEnc = (u8) => {
  let s = "";
  u8.forEach((b) => { s += String.fromCharCode(b); });
  return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
};
const b64urlDec = (s) => {
  const b = atob(s.replace(/-/g, "+").replace(/_/g, "/"));
  return Uint8Array.from(b, (c) => c.charCodeAt(0));
};

async function deflate(u8) {
  if (typeof CompressionStream === "undefined") return null;
  const cs = new CompressionStream("deflate-raw");
  const w = cs.writable.getWriter();
  w.write(u8); w.close();
  return new Uint8Array(await new Response(cs.readable).arrayBuffer());
}
async function inflate(u8) {
  const ds = new DecompressionStream("deflate-raw");
  const w = ds.writable.getWriter();
  w.write(u8); w.close();
  return new Uint8Array(await new Response(ds.readable).arrayBuffer());
}

// v2 — POSITIONAL, and nothing that can be derived travels.
//
// v1 was 2.5 kB of JSON before compression and most of it was waste: a
// 914-byte freshness key (a serialised copy of the build, inside the payload
// describing that build), 401 bytes of riven shape DRAFTS the recipient
// regenerates blank anyway, and a JSON key beside every value. Links are
// posted into chat windows and printed into QR codes, so length is a feature
// (user, 2026-08-02).
//
// The layout, by index — read it beside `decodeShare`:
//   0  version (2)
//   1  weapon id
//   2  build name
//   3  slots: 9 entries, each null | [modId] | [modId, pol] | [modId, pol, rank]
//      pol is one letter, rank omitted when it is the mod's max
//   4  arcanes: [] | [id] | [[id, rank], …]      rank omitted when max
//   5  evolutions by tier: ["evo1_incarnon_form", "", …] — the weapon prefix
//      is stripped, since a tier's options belong to the weapon in field 1
//   6  rivens: [[name, shape, rank, pol, [[statId, roll], …], [malusId, roll]|0], …]
//   7  scenario: only what DIFFERS from the server's own defaults
//   8  measurement: [score, duration, dps] | 0
const POL_LETTER = { Madurai: "M", Naramon: "N", Vazarin: "V", Umbra: "U", Omni: "O" };
const LETTER_POL = Object.fromEntries(Object.entries(POL_LETTER).map(([k, x]) => [x, k]));

// The weapon's evolution group, so ids can travel without their prefix.
const evoPrefix = () => {
  const tiers = weaponEvos();
  const any = (tiers[0] || { options: [] }).options[0];
  if (!any) return "";
  const w = $("weapon").value;
  // Ids are "<group>_<name>" and the group is the transform group, which is
  // the weapon id for every weapon that has one today.
  return any.id.startsWith(w + "_") ? w + "_" : "";
};

function sharePayload() {
  const st = snapshotState();
  const p = loadPresetList(BUILDS).find((x) => x.name === activePreset);
  const pre = evoPrefix();

  // The RIVENS the build equips travel whole (field 6), so a slot names one
  // by its index there — "~0" instead of "riven:some long name" repeated.
  const used = st.slots.map((s) => s.mod).filter(isRivenId);
  const rivenOrder = [...new Set(used)].map((id) => String(id).slice(RIVEN_PREFIX.length));
  const slots9 = st.slots.map((s) => {
    if (!s.mod) return 0;
    const id = isRivenId(s.mod)
      ? "~" + rivenOrder.indexOf(String(s.mod).slice(RIVEN_PREFIX.length))
      : s.mod;
    const m = modById(s.mod);
    const pol = POL_LETTER[s.pol] || "";
    const rankOff = m && s.rank != null && s.rank !== m.max_rank;
    // A plain slot is the id alone. The array form only appears when there is
    // something else to say about it.
    if (!pol && !rankOff) return id;
    return rankOff ? [id, pol, s.rank] : [id, pol];
  });

  const arcs = (st.arcane || []).filter((a) => a && a !== "none").map((a, i) => {
    const def = arcaneById(a);
    const rank = (st.arcaneRank || [])[i];
    return (def && rank != null && rank !== def.max_rank) ? [a, rank] : a;
  });

  const tiers = weaponEvos();
  const evos = tiers.map((x) => {
    const id = evoSel[x.tier];
    return id ? (pre && id.startsWith(pre) ? id.slice(pre.length) : id) : "";
  });
  while (evos.length && !evos[evos.length - 1]) evos.pop();

  // The RIVENS the build actually equips, by definition — the recipient has
  // no such item and never will unless it travels. This is the custom/preset
  // line showing up in the wire format: a preset is a state both sides can
  // hold, a custom is a thing only one of them made. Only the ACTIVE shape
  // goes: the other three drafts are scratch paper.
  const byName = new Map(loadPresetList(RIVENS).map((x) => [x.name, x]));
  const rivens = rivenOrder.map((n) => byName.get(n)).filter(Boolean)
    .map((x) => {
      const s = x.state || {};
      return [x.name, s.shape || "", s.rank ?? 8, POL_LETTER[cap1(s.polarity)] || "M",
        (s.bonuses || []).map((b) => [b.id, r3(b.roll)]),
        s.malus ? [s.malus.id, r3(s.malus.roll)] : 0];
    });

  // Only what DIFFERS from the defaults both sides already have.
  const d = META.defaults || {};
  const sc = {};
  const live = snapshotScenario();
  Object.keys(live).forEach((k) => {
    const val = live[k];
    if (k === "buffs") {
      // A buff left at its own default is not a setting — both sides derive
      // the same default from the same mod, so sending it is pure length.
      const def = Object.fromEntries((buffList || []).map((b) => [b.id, b]));
      const out = {};
      Object.entries(val || {}).forEach(([id, c]) => {
        const b = def[id];
        if (b && c.stacks === b.default_stacks && !!c.locked === !!b.default_locked) return;
        out[id] = c.locked ? [c.stacks, 1] : [c.stacks];
      });
      if (Object.keys(out).length) sc.buffs = out;
      return;
    }
    // AN OBJECT NEVER COMPARES EQUAL to another object, so a plain `!==` would
    // send `extra_stats: {}` on every link. Compared by VALUE, which is what
    // "only what differs from the defaults both sides already have" meant all
    // along — it just had no object field to be wrong about until now.
    const same = (a, b) =>
      a && typeof a === "object" ? JSON.stringify(a) === JSON.stringify(b || {}) : a === b;
    if (!same(val, d[k])) sc[k] = val;
  });

  // The sharer's MEASUREMENT, kept as a claim rather than as a fact: the
  // recipient's own run is what decides. Three numbers, not the whole result
  // object — the card needs the headline and the fight, and the fight is
  // field 7.
  const r = p && p.lastResult && p.lastResult.r;
  const m = r ? [r3(r.score), r.duration, Math.round(r.dps || 0)] : 0;

  // THE TWO AXES THAT ARE THE BUILD AND ARE NOT A SLOT: how the weapon is
  // PLAYED, and what a Lich handed it. `snapshotState` has carried both for
  // some time and this tuple read neither, so a shared Kuva Nukor reopened on
  // the default progenitor element and a shared charged Phantasma reopened in
  // base form — a link that changes the build it is claiming a number for.
  //
  // Omitted when the recipient would derive the same value anyway, like every
  // other field here: `defaultMode`/`defaultValence` are what both ends ask,
  // so sending the answer they already have is pure length.
  const dv = defaultValence(st.weapon, null);
  const md = st.mode === defaultMode(st.weapon, null) ? 0 : st.mode;
  const val = (st.valence && st.valence.element !== dv.element) || (st.valence && st.valence.bonus !== dv.bonus)
    ? [st.valence.element, r3(st.valence.bonus)] : 0;

  const out = [2, st.weapon, activePreset, slots9, arcs, evos, rivens, sc, m, md, val];
  while (out.length > 9 && !out[out.length - 1]) out.pop();
  return out;
}

const r3 = (x) => Math.round((Number(x) || 0) * 1000) / 1000;
const cap1 = (s) => String(s || "").replace(/^./, (c) => c.toUpperCase());

async function shareUrl() {
  const json = new TextEncoder().encode(JSON.stringify(sharePayload()));
  const z = await deflate(json);
  const code = z && z.length < json.length
    ? SHARE_V_DEFLATE + b64urlEnc(z)
    : SHARE_V_PLAIN + b64urlEnc(json);
  const w = weaponInfo($("weapon").value);
  return `${location.origin}${weaponPath(w.id)}?${SHARE_PARAM}=${code}`;
}

async function decodeShare(code) {
  if (!code) return null;
  const bytes = b64urlDec(code.slice(1));
  const json = code[0] === SHARE_V_DEFLATE ? await inflate(bytes) : bytes;
  const data = JSON.parse(new TextDecoder().decode(json));
  if (!Array.isArray(data)) return v1Share(data);      // links posted before v2
  const [, weapon, name, slots9, arcs, evos, rivens, sc, m, md, val] = data;
  return {
    w: weapon,
    n: name,
    // Absent means "whatever this weapon's default is", which is exactly what
    // `defaultMode`/`defaultValence` answer when handed nothing — so a link
    // posted before these two travelled still lands where it always did.
    mode: md || undefined,
    valence: val ? { element: val[0], bonus: val[1] } : undefined,
    slots: (slots9 || []).map((s) => {
      if (!s) return { mod: null, pol: null, rank: null };
      const [id, pol, rank] = typeof s === "string" ? [s] : s;
      return { mod: id, pol: LETTER_POL[pol] || null, rank: rank ?? null };
    }),
    arcane: (arcs || []).map((a) => (Array.isArray(a) ? a[0] : a)),
    arcaneRank: (arcs || []).map((a) => (Array.isArray(a) ? a[1] : null)),
    evos: evos || [],
    rivens: (rivens || []).map(([rn, shape, rank, pol, bonuses, malus]) => ({
      n: rn,
      s: {
        shape, rank, polarity: (LETTER_POL[pol] || "Madurai").toLowerCase(),
        bonuses: (bonuses || []).map(([id, roll]) => ({ id, roll })),
        malus: malus ? { id: malus[0], roll: malus[1] } : null,
      },
    })),
    sc: expandScenario(sc || {}),
    m: m ? { score: m[0], duration: m[1], dps: m[2] } : null,
  };
}

// `buffs` travels as {id: [stacks] | [stacks, 1]} — the pair the panel wants
// is rebuilt here rather than spelled out in the link.
function expandScenario(sc) {
  if (!sc.buffs) return sc;
  const buffs = {};
  Object.entries(sc.buffs).forEach(([id, c]) => {
    buffs[id] = Array.isArray(c) ? { stacks: c[0], locked: !!c[1] } : c;
  });
  return { ...sc, buffs };
}

// A v1 link (the first shape this shipped in) read into the v2 structure.
function v1Share(d) {
  if (!d || !d.b) return null;
  return {
    w: d.w, n: d.n,
    slots: d.b.slots || [],
    arcane: d.b.arcane || [], arcaneRank: d.b.arcaneRank || [],
    evos: Object.values(d.b.evoSel || {}).map((x) => x || ""),
    rivens: d.r || [],
    sc: (d.sc && d.sc.state) || {},
    m: d.m && d.m.r ? { score: d.m.r.score, duration: d.m.r.duration, dps: d.m.r.dps } : null,
  };
}

// Land a shared link: a NEW copy of every part, never a merge into what is
// already there. A link is someone else's work — it may not overwrite yours,
// and it may not need you to rebuild half of it by hand.
async function importShare(code) {
  let data;
  try { data = await decodeShare(code); } catch (_) { data = null; }
  if (!data) { presetToast(tr("that share link could not be read")); return false; }
  const w = (META.weapons || []).find((x) => x.id === data.w);
  if (!w) { presetToast(tr("that share link is for a weapon this build of the site does not have")); return false; }

  switchWeapon(w.id);

  // 1. The RIVENS first: the build's slots point at them by name, and the
  //    name may already be taken here, so the map from old name to new is
  //    what the slots are rewritten with.
  const renamed = {};
  if ((data.rivens || []).length) {
    const ps = loadPresetList(RIVENS);
    data.rivens.forEach((x) => {
      const name = freeName(ps, (n) => x.n + (n > 1 ? " " + n : ""));
      renamed[x.n] = name;
      ps.push({ name, savedAt: Date.now(), state: withDrafts(x.s) });
    });
    storePresetList(RIVENS, ps);
    refreshRivenNames();
  }

  // 2. The SCENARIO, as its own new entry — the fight the number was measured
  //    in is half of what makes the number checkable. Only the differences
  //    travelled, so the defaults fill the rest back in.
  // Only the DIFFERENCES travelled; the server's own defaults fill the rest
  // back in, which is the same table the sender diffed against.
  const scState = { ...defaultScenario(), ...(data.sc || {}) };
  const sc = loadPresetList(SCENARIOS);
  const scName = freeName(sc, (n) => "scenario" + (n > 1 ? " " + n : ""));
  sc.push({ name: scName, savedAt: Date.now(), state: scState });
  storePresetList(SCENARIOS, sc);
  activeScenario = scName;
  localStorage.setItem(presetActiveKey(SCENARIOS), scName);

  // 3. The BUILD, with riven ids repointed at the copies just made and any id
  //    this build of the site does not know dropped — said out loud rather
  //    than silently, the same rule the cross-weapon import follows.
  const dropped = [];
  const slots2 = (data.slots || []).map((s) => {
    if (!s || !s.mod) return { mod: null, pol: s ? s.pol : null, rank: null };
    // "~<n>" names the nth riven in field 6 — resolve it to the copy just
    // imported, whatever that copy had to be renamed to.
    if (/^~\d+$/.test(s.mod)) {
      const src = (data.rivens || [])[Number(s.mod.slice(1))];
      const nm = src && (renamed[src.n] || src.n);
      return nm ? { ...s, mod: RIVEN_PREFIX + nm } : { mod: null, pol: s.pol, rank: null };
    }
    if (isRivenId(s.mod)) {                       // v1 links
      const was = String(s.mod).slice(RIVEN_PREFIX.length);
      return { ...s, mod: RIVEN_PREFIX + (renamed[was] || was) };
    }
    if (!modById(s.mod)) { dropped.push(s.mod); return { mod: null, pol: s.pol, rank: null }; }
    return s;
  });
  const pre = evoPrefix();
  const evoSel2 = {};
  (data.evos || []).forEach((id, i) => { if (id) evoSel2[i + 1] = pre && !id.startsWith(pre) ? pre + id : id; });

  const state = {
    weapon: w.id,
    slots: slots2,
    arcane: data.arcane,
    arcaneRank: data.arcaneRank,
    evoSel: evoSel2,
    // Handed to `restoreState` raw, because it already cleans both against the
    // weapon being opened: an element this spec does not offer is dropped, a
    // mode it cannot be played in falls back to the arsenal's.
    mode: data.mode,
    valence: data.valence,
  };
  const builds = loadPresetList(BUILDS);
  // Named for where it came from. Without it a link lands as "build 1 2",
  // which says nothing about being someone else's work.
  const base = `${data.n || "build"} (shared)`;
  const name = freeName(builds, (n) => base + (n > 1 ? " " + n : ""));
  activePreset = name;
  localStorage.setItem(presetActiveKey(BUILDS), name);
  // The build, then the FIGHT — through the scenario's own door. The link
  // carries both and both must land, but a build does not set a scenario:
  // the copy above is already the active `simulator-scenarios` entry, and
  // this is what puts it on screen.
  whileApplying(() => { restoreState(state, w.id); applyScenario(scState); });
  builds.push({
    name, savedAt: Date.now(), state: snapshotState(),
    // The sharer's number, as THEIR claim: it is stamped with a key that
    // cannot match this machine's, so the first thing measured here replaces
    // it rather than silently inheriting someone else's measurement.
    ...(data.m ? { lastResult: { r: data.m, at: Date.now(), key: "shared" } } : {}),
  });
  storePresetList(BUILDS, builds);

  renderPresetBar(); renderScenarioBar(); renderMods(); renderSim(); refreshPanel();
  renderStoredSimResult();
  const bits = [tr("build")];
  if ((data.rivens || []).length) bits.push(`${data.rivens.length} ${tr("riven")}`);
  bits.push(tr("scenario"));
  presetToast(`${tr("imported")}: ${name} · ${bits.join(" + ")}` +
    (dropped.length ? ` · ${tr("dropped")} ${dropped.length}` : ""));
  return true;
}

// The share panel: the link, and a CARD to paste into a chat. Both carry the
// site's own address — an image that travels without one is a screenshot of
// nowhere (user, 2026-08-02).
async function openSharePanel(bar) {
  const panel = bar.querySelector(".pshare");
  if (!panel) return;
  if (!panel.hidden) { panel.hidden = true; return; }
  // Measure BEFORE building the link, so both the card and the payload carry
  // a number produced by exactly this build in exactly this fight.
  panel.hidden = false;
  panel.innerHTML = `<div class="sh-note">${escHtml(tr("simulating this build in the current scenario…"))}</div>`;
  await resultForShare();
  const url = await shareUrl();
  panel.innerHTML =
    `<div class="sh-row"><input class="sh-url" type="text" readonly value="${escHtml(url)}">` +
    `<button class="cu-btn sh-copy">${escHtml(tr("copy link"))}</button>` +
    `<button class="cu-btn sh-img">${escHtml(tr("copy image"))}</button>` +
    `<button class="cu-btn sh-dl">${escHtml(tr("download image"))}</button></div>` +
    `<div class="sh-note">${escHtml(tr("the link carries the build, its rivens, the fight it was measured in and the measurement — opening it saves a new copy of each"))}</div>` +
    `<canvas class="sh-canvas" width="900" height="640"></canvas>`;
  const urlBox = panel.querySelector(".sh-url");
  urlBox.onclick = () => urlBox.select();
  const canvas = panel.querySelector(".sh-canvas");
  await drawShareCard(canvas, url);

  const say = (msg) => presetToast(tr(msg));
  panel.querySelector(".sh-copy").onclick = async () => {
    try { await navigator.clipboard.writeText(url); say("link copied"); }
    // Clipboard permission can be refused; selecting the text is the fallback
    // that always works, and no dialog is involved either way.
    catch (_) { urlBox.select(); say("press Ctrl+C to copy the selected link"); }
  };
  panel.querySelector(".sh-img").onclick = async () => {
    try {
      const blob = await new Promise((res) => canvas.toBlob(res, "image/png"));
      await navigator.clipboard.write([new ClipboardItem({ "image/png": blob })]);
      say("image copied");
    } catch (_) { say("this browser will not copy images — use download"); }
  };
  panel.querySelector(".sh-dl").onclick = () => {
    const a = document.createElement("a");
    a.href = canvas.toDataURL("image/png");
    a.download = `wfsim-${$("weapon").value}-${presetLabel(buildNamed(activePreset))}.png`.replace(/[\s#]+/g, "-");
    a.click();
  };
}

// ---- QR ---------------------------------------------------------------
// A card is pasted into a chat and read on a phone, and a phone cannot click
// a picture (user, 2026-08-02). Written here rather than pulled in: the repo
// takes no dependencies, and a QR encoder is a bounded piece of arithmetic —
// byte mode, error correction L (the most data per module, and a card is not
// a sticker on a wet crate), smallest version that fits.
//
// Spec: ISO/IEC 18004. The three tables below are the parts of it that cannot
// be derived — everything else is computed.
const QR_ECC_L = [
  [7, 1], [10, 1], [15, 1], [20, 1], [26, 1], [18, 2], [20, 2], [24, 2], [30, 2], [18, 4],
  [20, 4], [24, 4], [26, 4], [30, 4], [22, 6], [24, 6], [28, 6], [30, 6], [28, 7], [28, 8],
  [28, 8], [28, 9], [30, 9], [30, 10], [26, 12], [28, 12], [30, 12], [30, 13], [30, 14], [30, 15],
  [30, 16], [30, 17], [30, 18], [30, 19], [30, 19], [30, 20], [30, 21], [30, 22], [30, 24], [30, 25],
];
// Total codewords per version (data + ecc).
const QR_TOTAL = [26, 44, 70, 100, 134, 172, 196, 242, 292, 346, 404, 466, 532, 581, 655, 733,
  815, 901, 991, 1085, 1156, 1258, 1364, 1474, 1588, 1706, 1828, 1921, 2051, 2185, 2323, 2465,
  2611, 2761, 2876, 3034, 3196, 3362, 3532, 3706];
// Alignment-pattern centres per version (empty for version 1).
const QR_ALIGN = [[], [6, 18], [6, 22], [6, 26], [6, 30], [6, 34], [6, 22, 38], [6, 24, 42],
  [6, 26, 46], [6, 28, 50], [6, 30, 54], [6, 32, 58], [6, 34, 62], [6, 26, 46, 66], [6, 26, 48, 70],
  [6, 26, 50, 74], [6, 30, 54, 78], [6, 30, 56, 82], [6, 30, 58, 86], [6, 34, 62, 90],
  [6, 28, 50, 72, 94], [6, 26, 50, 74, 98], [6, 30, 54, 78, 102], [6, 28, 54, 80, 106],
  [6, 32, 58, 84, 110], [6, 30, 58, 86, 114], [6, 34, 62, 90, 118], [6, 26, 50, 74, 98, 122],
  [6, 30, 54, 78, 102, 126], [6, 26, 52, 78, 104, 130], [6, 30, 56, 82, 108, 134],
  [6, 34, 60, 86, 112, 138], [6, 30, 58, 86, 114, 142], [6, 34, 62, 90, 118, 146],
  [6, 30, 54, 78, 102, 126, 150], [6, 24, 50, 76, 102, 128, 154], [6, 28, 54, 80, 106, 132, 158],
  [6, 32, 58, 84, 110, 136, 162], [6, 26, 54, 82, 110, 138, 166], [6, 30, 58, 86, 114, 142, 170]];

// GF(256) with the QR primitive polynomial 0x11d.
const GF_EXP = new Uint8Array(512), GF_LOG = new Uint8Array(256);
(() => {
  let x = 1;
  for (let i = 0; i < 255; i++) {
    GF_EXP[i] = x; GF_LOG[x] = i;
    x <<= 1; if (x & 0x100) x ^= 0x11d;
  }
  for (let i = 255; i < 512; i++) GF_EXP[i] = GF_EXP[i - 255];
})();
const gfMul = (a, b) => (a && b ? GF_EXP[GF_LOG[a] + GF_LOG[b]] : 0);

// The generator polynomial for `n` ecc codewords.
function qrGenPoly(n) {
  let poly = [1];
  for (let i = 0; i < n; i++) {
    const next = new Array(poly.length + 1).fill(0);
    for (let j = 0; j < poly.length; j++) {
      next[j] ^= gfMul(poly[j], GF_EXP[i]);
      next[j + 1] ^= poly[j];
    }
    poly = next;
  }
  // Built LOW-degree-first; the division below indexes it high-degree-first
  // (gen[0] is the monic leading 1 it skips). Verified against a reference
  // encoder: without this the ECC block is wrong and nothing scans.
  return poly.reverse();
}

function qrEcc(data, n) {
  const gen = qrGenPoly(n);
  const rem = new Array(n).fill(0);
  for (const d of data) {
    const factor = d ^ rem[0];
    rem.shift(); rem.push(0);
    for (let j = 0; j < n; j++) rem[j] ^= gfMul(gen[j + 1], factor);
  }
  return rem;
}

// Bytes -> the module matrix, or null when the text does not fit any version.
function qrMatrix(text) {
  const bytes = new TextEncoder().encode(text);
  let version = 0, dataCw = 0, eccPer = 0, blocks = 0;
  for (let v = 1; v <= 40; v++) {
    const [e, b] = QR_ECC_L[v - 1];
    const total = QR_TOTAL[v - 1];
    const cap = total - e * b;
    const lenBits = v < 10 ? 8 : 16;
    if (4 + lenBits + bytes.length * 8 <= cap * 8) {
      version = v; dataCw = cap; eccPer = e; blocks = b; break;
    }
  }
  if (!version) return null;

  // ---- bit stream: mode 0100, length, data, terminator, pad --------------
  const bits = [];
  const push = (val, n) => { for (let i = n - 1; i >= 0; i--) bits.push((val >> i) & 1); };
  push(0b0100, 4);
  push(bytes.length, version < 10 ? 8 : 16);
  bytes.forEach((b) => push(b, 8));
  for (let i = 0; i < 4 && bits.length < dataCw * 8; i++) bits.push(0);
  while (bits.length % 8) bits.push(0);
  const cw = [];
  for (let i = 0; i < bits.length; i += 8) {
    cw.push(bits.slice(i, i + 8).reduce((a, b) => (a << 1) | b, 0));
  }
  for (let i = 0; cw.length < dataCw; i++) cw.push(i % 2 ? 0x11 : 0xec);

  // ---- split into blocks, interleave data then ecc ----------------------
  const short = Math.floor(dataCw / blocks), extra = dataCw % blocks;
  const dblocks = [], eblocks = [];
  let at = 0;
  for (let i = 0; i < blocks; i++) {
    const n = short + (i >= blocks - extra ? 1 : 0);
    const d = cw.slice(at, at + n); at += n;
    dblocks.push(d);
    eblocks.push(qrEcc(d, eccPer));
  }
  const out = [];
  for (let i = 0; i < Math.max(...dblocks.map((d) => d.length)); i++) {
    dblocks.forEach((d) => { if (i < d.length) out.push(d[i]); });
  }
  for (let i = 0; i < eccPer; i++) eblocks.forEach((e) => out.push(e[i]));

  // ---- the matrix -------------------------------------------------------
  const size = version * 4 + 17;
  const m = Array.from({ length: size }, () => new Array(size).fill(null));
  const set = (r, c, v) => { if (r >= 0 && c >= 0 && r < size && c < size) m[r][c] = v; };
  const finder = (r, c) => {
    for (let i = -1; i <= 7; i++) for (let j = -1; j <= 7; j++) {
      const on = i >= 0 && i <= 6 && j >= 0 && j <= 6
        && (i === 0 || i === 6 || j === 0 || j === 6 || (i >= 2 && i <= 4 && j >= 2 && j <= 4));
      set(r + i, c + j, on ? 1 : 0);
    }
  };
  finder(0, 0); finder(0, size - 7); finder(size - 7, 0);
  for (let i = 8; i < size - 8; i++) {
    m[6][i] = i % 2 === 0 ? 1 : 0;
    m[i][6] = i % 2 === 0 ? 1 : 0;
  }
  // Alignment patterns sit at every pairing of the centres EXCEPT the three
  // that would land on a finder. Testing "is this module already set" instead
  // also skipped the ones that sit ON the timing line — which the spec puts
  // there deliberately — and every version 7 and up came out unreadable.
  const centres = QR_ALIGN[version - 1];
  const last = centres[centres.length - 1];
  centres.forEach((r) => centres.forEach((c) => {
    if ((r === 6 && c === 6) || (r === 6 && c === last) || (r === last && c === 6)) return;
    for (let i = -2; i <= 2; i++) for (let j = -2; j <= 2; j++) {
      set(r + i, c + j, (Math.abs(i) === 2 || Math.abs(j) === 2 || (i === 0 && j === 0)) ? 1 : 0);
    }
  }));
  m[size - 8][8] = 1;                              // the always-dark module

  // Version information (7 versions and up), BCH(18,6).
  if (version >= 7) {
    // BCH(18,6) over the 6-bit version, generator 0x1F25 — 12 check bits.
    let rem = version;
    for (let i = 0; i < 12; i++) rem = (rem << 1) ^ ((rem >> 11) * 0x1f25);
    const info = ((version << 12) | (rem & 0xfff)) >>> 0;
    for (let i = 0; i < 18; i++) {
      const bit = (info >> i) & 1;
      m[Math.floor(i / 3)][size - 11 + (i % 3)] = bit;
      m[size - 11 + (i % 3)][Math.floor(i / 3)] = bit;
    }
  }

  // Reserve the format area so the data walk skips it. Cells are [row, col];
  // the spec states them as (x, y) = (column, row), which is the transposition
  // that had the walk skipping the wrong modules.
  const fmtCells = [[7, 8], [8, 8], [8, 7]];
  for (let i = 0; i <= 5; i++) fmtCells.push([i, 8], [8, i]);
  for (let i = 0; i < 8; i++) fmtCells.push([8, size - 1 - i], [size - 1 - i, 8]);
  fmtCells.forEach(([r, c]) => { if (m[r][c] === null) m[r][c] = 0; });

  // ---- the data walk, mask 0 -------------------------------------------
  // One mask, not eight: the spec's penalty scoring picks the prettiest of
  // eight, but every one of them scans. Mask 0 (`(r+c) % 2`) on a payload
  // this dense is well inside what any reader handles, and eight passes of
  // penalty arithmetic is a lot of code to own for a cosmetic gain.
  const taken = m.map((row) => row.map((x) => x !== null));
  let bi = 0;
  const dataBit = () => {
    const byte = out[bi >> 3];
    const bit = byte === undefined ? 0 : (byte >> (7 - (bi & 7))) & 1;
    bi++;
    return bit;
  };
  let upward = true;
  for (let col = size - 1; col > 0; col -= 2) {
    if (col === 6) col--;                          // the timing column
    for (let n = 0; n < size; n++) {
      const row = upward ? size - 1 - n : n;
      for (const c of [col, col - 1]) {
        if (taken[row][c]) continue;
        const bit = dataBit() ^ ((row + c) % 2 === 0 ? 1 : 0);
        m[row][c] = bit;
      }
    }
    upward = !upward;
  }

  // ---- format information: ecc L (01) + mask 0, BCH(15,5) + 0x5412 ------
  const fmtData = (0b01 << 3) | 0;
  let rem = fmtData << 10;
  for (let i = 4; i >= 0; i--) if ((rem >> (i + 10)) & 1) rem ^= 0x537 << i;
  const fmt = ((fmtData << 10) | rem) ^ 0x5412;
  const fbit = (i) => (fmt >> i) & 1;
  for (let i = 0; i <= 5; i++) m[i][8] = fbit(i);
  m[7][8] = fbit(6);
  m[8][8] = fbit(7);
  m[8][7] = fbit(8);
  for (let i = 9; i <= 14; i++) m[8][14 - i] = fbit(i);
  for (let i = 0; i <= 7; i++) m[8][size - 1 - i] = fbit(i);
  for (let i = 8; i <= 14; i++) m[size - 15 + i][8] = fbit(i);
  m[size - 8][8] = 1;                              // the always-dark module

  return m;
}

// A QR as its OWN canvas, at a whole number of pixels per module.
//
// Drawn straight onto the card it came out unreadable: a dense symbol scaled
// to fit a box lands on fractional module sizes, and rounding each one up
// smears neighbours together. Rendering at an integer scale and then placing
// it at its natural size is the difference between a picture of a QR and one
// that scans. Quiet zone six, not the spec's minimum four — a reference
// encoder's own output fails to scan at four on a dense symbol.
// EIGHT device pixels per module, always — the size is a consequence, not a
// setting. Measured against a real decoder after the resizing a chat app
// does: at 4 px a card only reads at full size, at 6 it survives a 0.66x
// shrink, at 8 it still reads after 0.5x AND JPEG 80. A QR that only scans
// from the original file is a QR nobody scans.
const QR_MODULE_PX = 8;
function qrCanvas(text) {
  const m = qrMatrix(text);
  if (!m) return null;
  const n = m.length, quiet = 6;
  const modulePx = QR_MODULE_PX;
  const side = (n + quiet * 2) * modulePx;
  const c = document.createElement("canvas");
  c.width = side; c.height = side;
  const g = c.getContext("2d");
  g.fillStyle = "#fff"; g.fillRect(0, 0, side, side);
  g.fillStyle = "#000";
  for (let r = 0; r < n; r++) {
    for (let col = 0; col < n; col++) {
      if (m[r][col]) {
        g.fillRect((col + quiet) * modulePx, (r + quiet) * modulePx, modulePx, modulePx);
      }
    }
  }
  return c;
}

// The card. Drawn here rather than server-side: it has to work on a static
// site with no backend, and pasting a picture into a group chat is how a build
// actually gets shown around.
//
// It shows EVERYTHING the link carries that a reader can act on — the weapon,
// the mod cards with their polarities, capacity and Forma, the arcane, the
// evolutions, the riven's own rolls, and the number with the fight and the
// technique it came from. A card that showed the mods and hid the Incarnon
// would be the thing this whole feature exists to stop.
// The site's own name, not wherever this page happens to be served from: the
// card is a thing that travels, and a preview host or a localhost in the
// corner of it would be wrong everywhere it landed (user, 2026-08-02).
const SITE_HOST = "wfsim.app";
const CARD_W = 1000, CARD_DPR = 2;

const loadImg = (src) => new Promise((res) => {
  if (!src) return res(null);
  const im = new Image();
  im.onload = () => res(im);
  im.onerror = () => res(null);      // same-origin art, but never block on it
  im.src = src;
});

// Draw `im` to fit a box, centred, aspect kept.
function drawFit(g, im, x, y, w, h) {
  if (!im) return;
  const s = Math.min(w / im.width, h / im.height);
  const dw = im.width * s, dh = im.height * s;
  g.drawImage(im, x + (w - dw) / 2, y + (h - dh) / 2, dw, dh);
}

async function drawShareCard(canvas, url) {
  const W = CARD_W;
  // The QR is built first: its size is fixed by its module count and cannot be
  // squeezed, so the layout is built around it rather than the other way.
  const qc = qrCanvas(url);
  const qs = qc ? qc.width / CARD_DPR : 0;

  const named = slots.map((s, i) => ({ m: s.mod && modById(s.mod), i })).filter((z) => z.m);
  const arc = (arcanes || []).filter((a) => a && a !== "none")
    .map((a) => (arcaneById(a) || {}).name || a);
  const rid = slots.map((s) => s.mod).find(isRivenId);
  const rlines = (rid && ((rivenNames[String(rid).slice(RIVEN_PREFIX.length)] || {}).lines || [])) || [];

  // Below the mod strip the card is TWO COLUMNS: everything a reader reads on
  // the left, the code on the right. One column under a 340px square left a
  // band of empty background as tall as the code itself.
  const NAME_COLS = 2;
  const perCol = Math.ceil(named.length / NAME_COLS) || 1;
  const colTop = 104 + 88 + 26;
  const leftH = perCol * 24 + (arc.length ? 24 : 0) + (rlines.length ? 24 : 0) + 24 + 78;
  const H = colTop + Math.max(leftH, qs + 40) + 22;

  canvas.width = W * CARD_DPR; canvas.height = H * CARD_DPR;
  const g = canvas.getContext("2d");
  g.scale(CARD_DPR, CARD_DPR);

  const css = getComputedStyle(document.documentElement);
  const v = (n, fallback) => (css.getPropertyValue(n) || "").trim() || fallback;
  const bg = v("--surface", "#171a21"), fg = v("--text", "#f2f4f8");
  const dim = v("--muted", "#6b7280"), fg2 = v("--text-2", "#a6adbb");
  const gold = v("--gold", "#e8c37a"), line = v("--line", "rgba(255,255,255,.09)");
  const F = (s, w) => `${w || ""} ${s}px system-ui, -apple-system, "Segoe UI", "Microsoft YaHei", sans-serif`;
  g.fillStyle = bg; g.fillRect(0, 0, W, H);

  const w = weaponInfo($("weapon").value);
  const art = await loadImg(IMG(w.image));
  // The weapon's own art, large and faint behind the lower left — recognisable
  // at a glance in a chat scroll, and never in the way of the text.
  if (art) {
    const h = 300, wid = h * (art.width / art.height || 2);
    g.save();
    g.globalAlpha = 0.09;
    g.drawImage(art, 20, H - h - 10, wid, h);
    g.restore();
  }

  // ---- header: what it is, and what it costs to build ------------------
  g.fillStyle = fg; g.font = F(34, "600");
  g.fillText(w.name, 36, 56);
  if (art) drawFit(g, art, 40 + g.measureText(w.name).width, 26, 60, 34);
  g.fillStyle = dim; g.font = F(15);
  const evos = Object.values(evoSel).filter(Boolean).length;
  g.fillText([presetLabel(buildNamed(activePreset)), tr(w.subtype || w.mod_class || "")]
    .concat(evos ? [`${evos} ${tr("Evolutions")}`] : []).join(" · "), 36, 82);

  // Capacity and Forma, right-aligned — the price of the build, which is half
  // of what a reader is judging.
  const fc = formaCount();
  const cap = capOf(w.id);
  const cost = [`${capacityUsed()} / ${cap}`,
    [`${fc.regular} Forma`, fc.umbra ? `${fc.umbra} Umbra` : null, fc.omni ? `${fc.omni} Omni` : null]
      .filter(Boolean).join(" · ")].join("   ");
  g.textAlign = "right";
  g.fillStyle = capacityUsed() > cap ? v("--critical", "#e05656") : fg2;
  g.font = F(15);
  g.fillText(cost, W - 36, 56);
  g.textAlign = "left";

  // ---- the mod cards, with their polarities ----------------------------
  // An EMPTY slot draws NOTHING (user, 2026-08-02) — no placeholder, no dash.
  // A gap says "nothing here" without a mark that reads as one.
  const CW = 88, CH = 88, GAP = 8;
  const imgs = await Promise.all(slots.map((s) => {
    const m = s.mod && modById(s.mod);
    return m ? loadImg(IMG(m.image)) : null;
  }));
  const pols = await Promise.all(slots.map((s) => (s.pol ? loadImg(POL(s.pol)) : null)));
  slots.forEach((s, i) => {
    const m = s.mod && modById(s.mod);
    const cx = 36 + i * (CW + GAP), cy = 104;
    if (!m) return;
    if (imgs[i]) drawFit(g, imgs[i], cx, cy, CW, CH);
    else {
      g.strokeStyle = line;
      g.strokeRect(cx + .5, cy + .5, CW - 1, CH - 1);
      g.fillStyle = dim; g.font = F(12);
      g.textAlign = "center";
      g.fillText(m.name.slice(0, 8), cx + CW / 2, cy + CH / 2 + 4);
      g.textAlign = "left";
    }
    if (pols[i]) {
      g.fillStyle = bg;
      g.globalAlpha = .75; g.fillRect(cx, cy, 20, 20); g.globalAlpha = 1;
      drawFit(g, pols[i], cx + 3, cy + 3, 14, 14);
    }
  });

  // ---- left column: names, then the two things names cannot say --------
  let y = colTop;
  g.font = F(15);
  named.forEach((r, n) => {
    const col = Math.floor(n / perCol), row = n % perCol;
    g.fillStyle = fg;
    const tag = r.i === EXILUS ? "E" : String(r.i + 1);
    let label = `${tag}. ${r.m.name}`;
    if (label.length > 22) label = label.slice(0, 21) + "…";
    g.fillText(label, 36 + col * 280, y + row * 24);
  });
  y += perCol * 24 + 8;
  if (arc.length) {
    g.fillStyle = fg2; g.font = F(15);
    g.fillText(`${tr("Arcanes")}: ${arc.join(" · ")}`, 36, y);
    y += 24;
  }
  if (rlines.length) {
    g.fillStyle = fg2; g.font = F(14);
    g.fillText(rlines.map((z) => tf(z)).join("   "), 36, y);
    y += 24;
  }

  // ---- the number, in the app's own units and its own formatting -------
  // `kpm(score, duration)` and `sig2`, exactly as the results panel: the score
  // counts the fraction of a kill too, so a build that drains 0.28% of one
  // enemy reads 0.0028 rather than the 0.00 that `kills` alone produced.
  const p = loadPresetList(BUILDS).find((z) => z.name === activePreset);
  const r = p && p.lastResult && p.lastResult.r;
  const lineEnd = qs ? W - qs - 50 : W - 36;
  y += 8;
  g.strokeStyle = line;
  g.beginPath(); g.moveTo(36, y); g.lineTo(lineEnd, y); g.stroke();
  y += 46;
  if (r) {
    const byDps = sim.metric === "dps";
    g.fillStyle = gold; g.font = F(40, "600");
    g.fillText(byDps
      ? Math.round(r.dps || 0).toLocaleString() + " DPS"
      : sig2(kpm(r.score, r.duration)) + " KPM", 36, y);
    g.fillStyle = dim; g.font = F(15);
    const en = allEnemies().find((e) => e.id === sim.enemy) || {};
    // The fight AND the technique (user, 2026-08-02). Which form, how often
    // the head is hit and whether aim is held change the number as much as the
    // enemy does. Buffs are deliberately absent: they follow from the build,
    // and a card listing eleven of them would say nothing.
    // The mode is the BUILD's now, so the card reads it there. Still on the
    // card: it changes the number as much as the enemy does, and a share that
    // omitted it would be a claim nobody could reproduce.
    const formLabel = (w.modes || []).length > 1 ? modeLabel(w, mode) : "";
    g.fillText([
      en.name || sim.enemy,
      `Lv ${sim.level}${sim.steel_path ? " SP" : ""}`,
      `${sim.duration}s`,
      formLabel,
      `${sim.headshot_pct}% ${tr("headshots")}`,
      (w.sentinel || sim.aiming) ? tr("Aiming") : tr("hip-fire"),
      ...(sim.invisible ? [tr("Invisible")] : []),
      ...(sim.airborne ? [tr("Airborne")] : []),
      ...(sim.overshields ? [tr("Overshields")] : []),
      ...(sim.channeling ? [tr("Channeled ability")] : []),
      ...(sim.solo_weapon ? [tr("Only this weapon")] : []),
      sim.infinite_ammo === false ? tr("finite ammo") : null,
    ].filter(Boolean).join(" · "), 36, y + 25);
  }

  // ---- right column: the code, and the address -------------------------
  // A card is pasted into a chat and read on a PHONE, which cannot click a
  // picture — without this the link and the image are two things to send.
  if (qc) {
    const qx = W - 26 - qs, qy = colTop;
    g.imageSmoothingEnabled = false;
    g.drawImage(qc, qx, qy, qs, qs);
    g.textAlign = "center";
    g.font = F(21, "600");
    const cx = qx + qs / 2;
    const sw = g.measureText("Sim").width, ww = g.measureText("WF").width;
    g.fillStyle = gold; g.fillText("WF", cx - sw / 2, qy + qs + 26);
    g.fillStyle = fg; g.fillText("Sim", cx + ww / 2, qy + qs + 26);
    g.fillStyle = dim; g.font = F(14);
    g.fillText(SITE_HOST, cx, qy + qs + 46);
    g.textAlign = "left";
  }
}

// ---- Presets ----------------------------------------------------------
// The page is THREE MODULES — builder, simulator, optimizer (user,
// 2026-07-29) — plus EDITORS that feed them, and every preset collection is
// owned by one of those:
//   builder-builds        the whole build
//   optimizer-mods / optimizer-arcanes / optimizer-evolutions
//   rivens                the riven editor, which is ALL collection: there
//                         is no second one to tell it apart from, so the
//                         owner name alone is the domain
//   (simulator-enemies, simulator-scenarios … planned)
// A collection's DOMAIN id is "<owner>-<collection>", and every durable
// name derives from it mechanically:
//   localStorage  wfsim-presets-<domain>        the list
//                 wfsim-preset-active-<domain>  the active pointer
//   DOM id        preset-bar-<domain>
// Full words only in durable names; abbreviations stay inside function
// locals. No count cap — presets live in the user's localStorage, not
// with us.
//
// A preset BELONGS TO ONE WEAPON (user, 2026-07-30). The keys used to be
// weapon-less, so one global list served the whole roster and edits on the
// Laetum showed up on the Dual Toxocyst — the "cross-weapon apply prunes
// unknown ids" path existed precisely because of that bleed. The weapon id
// now joins the STORAGE key; the domain still names the collection, so DOM
// ids and labels are untouched:
//   localStorage  wfsim-presets-<weapon>-<domain>
//                 wfsim-preset-active-<weapon>-<domain>
// Copying a preset ACROSS weapons is a deliberate action instead — the
// "⇤ import" control on each bar.
const presetWeapon = () => ($("weapon") && $("weapon").value) || "";
// PRESETS vs CUSTOMS — two kinds of collection, and the difference is who
// consumes them (user, 2026-08-02).
//
// A PRESET is a saved state of something that always exists: the builder
// always has a build, the simulator a fight, the optimizer a search. Only its
// own module reads it, there is always at least one, and "active" means the
// state you are currently in.
//
// A CUSTOM is a thing you MADE, and the other modules consume it: a riven
// becomes a mod in the pool, a custom enemy becomes an entry in the scenario's
// enemy list. Owning none is ordinary, each one carries its own identity
// rather than a label you invented, and deleting one breaks references
// elsewhere — which a preset delete can never do. The mental model is a FILE:
// it sits in a list, you open one to edit it, and you can have none open.
//
// Different noun, different key. Everything BELOW the key is shared —
// storage, undo, per-weapon scoping, ⇤ import — because none of that depends
// on which kind it is.
const CUSTOM_DOMAINS = new Set(["rivens", "enemies"]);
const isCustomDomain = (d) => CUSTOM_DOMAINS.has(d);

// …AND ONE COLLECTION THAT IS NOT A WEAPON'S: the FIGHT (owner,
// 2026-08-09).
//
// It became true rather than being decided: the last weapon-shaped thing a
// scenario carried was `mode`, and mode left the fight and joined the build on
// 2026-08-07. What is left — the enemy, its level, Steel Path, the wielder's
// state, the duration, how many runs — is a description of a FIGHT, and a fight
// is not about any particular gun.
//
// The OFFICIAL rulers were already like this and always had been: one
// `single_target` applies to every weapon on the board, which is the whole
// point of a ruler. A player who wants to measure their own roster under their
// own fight was the only one made to re-create it per weapon, and comparing
// weapons is exactly what a scenario is FOR.
//
// This is a deliberate amendment to "NOTHING CROSSES BETWEEN WEAPONS" (user,
// 2026-08-02: 绝对不能串) and it narrows that rule rather than weakening it. The
// rule exists because a BUILD, a SEARCH and a RIVEN are statements about one
// weapon, and inheriting the last weapon's is how you end up measuring a gun
// you are not looking at. A fight is not a statement about a weapon, so there
// is nothing to inherit wrongly — and the one weapon-scoped knob it still holds
// (headshot %) is handled the way the rulers handle it: the SERVER forces 0 on
// a weapon that cannot headshot.
// …and a TARGET is not a weapon's either, for exactly the reason a fight is
// not: an enemy you built has no opinion about what is shooting it. Same
// consequence — one list for the whole roster, and no "⇤ import", because
// there is no other weapon to import from.
const SHARED_DOMAINS = new Set(["simulator-scenarios", "enemies"]);
const isSharedDomain = (d) => SHARED_DOMAINS.has(d);
const domainScope = (d, w) => (isSharedDomain(d) ? "" : (w ?? presetWeapon()) + "-");
const presetListKey = (d, w) =>
  (isCustomDomain(d) ? "wfsim-customs-" : "wfsim-presets-") + domainScope(d, w) + d;
const presetActiveKey = (d, w) =>
  (isCustomDomain(d) ? "wfsim-custom-open-" : "wfsim-preset-active-") + domainScope(d, w) + d;

// ONE-TIME MERGE of every weapon's scenario list into the shared one. A player
// who made a fight on the Torid must not have to make it again — and the lists
// are additive, so this reads them all and keeps every entry, renaming a
// collision rather than dropping either side.
(function mergeScenarioLists() {
  const D = "simulator-scenarios";
  const shared = presetListKey(D);
  const per = [];
  for (let i = 0; i < localStorage.length; i++) {
    const k = localStorage.key(i);
    const m = k && /^wfsim-presets-(.+)-simulator-scenarios$/.exec(k);
    if (m) per.push([k, m[1]]);
  }
  if (!per.length) return;
  let out = [];
  try { out = JSON.parse(localStorage.getItem(shared) || "[]") || []; } catch (_) { out = []; }
  const seen = new Set(out.map((p) => JSON.stringify(p.state)));
  for (const [k, weapon] of per) {
    let list = [];
    try { list = JSON.parse(localStorage.getItem(k) || "[]") || []; } catch (_) { list = []; }
    for (const p of list) {
      // IDENTICAL FIGHTS COLLAPSE. Most players' per-weapon copies are the same
      // fight made twice, and carrying six of them across would turn a merge
      // into a mess the player has to clean up.
      const sig = JSON.stringify(p.state);
      if (seen.has(sig)) continue;
      seen.add(sig);
      let name = p.name;
      if (out.some((q) => q.name === name)) name = `${name} (${weapon})`;
      out.push({ ...p, name });
    }
    localStorage.removeItem(k);
    localStorage.removeItem(`wfsim-preset-active-${weapon}-${D}`);
  }
  if (out.length) localStorage.setItem(shared, JSON.stringify(out));
})();

// One-time rename of every custom collection out of the preset namespace.
// The data is unchanged; only the noun was wrong.
(function migrateCustomKeys() {
  const moves = [];
  for (let i = 0; i < localStorage.length; i++) {
    const k = localStorage.key(i);
    let m = /^wfsim-presets-(.+)-([^-]+)$/.exec(k);
    if (m && isCustomDomain(m[2])) moves.push([k, `wfsim-customs-${m[1]}-${m[2]}`]);
    m = /^wfsim-preset-active-(.+)-([^-]+)$/.exec(k);
    if (m && isCustomDomain(m[2])) moves.push([k, `wfsim-custom-open-${m[1]}-${m[2]}`]);
  }
  moves.forEach(([from, to]) => {
    const v = localStorage.getItem(from);
    if (v !== null && localStorage.getItem(to) === null) localStorage.setItem(to, v);
    localStorage.removeItem(from);
  });
})();
// Parsed lists, memoised on the RAW STRING. The stored text IS the
// invalidation — nothing to keep in sync, and a stale read is impossible.
// Worth having because `gainKey()` resolves a whole scenario and is called
// from a sort comparator, i.e. O(n log n) times per picker render.
const presetParseCache = new Map();
const loadPresetList = (d, w) => {
  const k = presetListKey(d, w);
  let raw;
  try { raw = localStorage.getItem(k); } catch (_) { return []; }
  const hit = presetParseCache.get(k);
  if (hit && hit.raw === raw) return hit.list;
  let list = [];
  try { const p = JSON.parse(raw); if (Array.isArray(p)) list = p; } catch (_) { /* empty */ }
  presetParseCache.set(k, { raw, list });
  return list;
};
// The first "<thing> N" this collection does not already hold. Shared by both
// kinds — naming a new item is the same problem whatever it is called.
const freeName = (ps, mk) => {
  for (let n = 1; ; n++) { const nm = mk(n); if (!ps.some((p) => p.name === nm)) return nm; }
};

const storePresetList = (d, ps, w) => {
  const weapon = w ?? presetWeapon();
  recordUndo(d, weapon, ps);
  localStorage.setItem(presetListKey(d, weapon), JSON.stringify(ps));
};

// ---- UNDO — Ctrl+Z across every preset collection ----------------------
//
// Presets AUTO-SAVE, which is exactly what makes a slip expensive: a cleared
// tier, a deleted preset, a mis-aimed import is written the instant it
// happens and there is no save button to not press (user, 2026-08-02). Every
// collection — builds, scenarios, searches, rivens — writes through
// `storePresetList`, so that one call is where a "before" can be taken, and
// one stack covers the whole page rather than four.
//
// What is remembered is the whole COLLECTION plus which preset was active,
// not a field-level diff: a delete and an edit are then the same kind of
// event, and undoing either is a plain write-back.
const UNDO_LIMIT = 60;
// Two consecutive EDITS to the same collection within this window are ONE
// step — the auto-save fires per settled edit, and a Ctrl+Z that walked back
// through every keystroke would not be an undo, it would be a rewind. Adding,
// deleting, renaming or importing a preset never coalesces, however fast it
// follows: those are the slips worth one Ctrl+Z each, and folding a delete
// into the edit before it would undo both or neither.
const UNDO_COALESCE_MS = 900;
let undoStack = [], redoStack = [], undoSuspended = false;

const presetSnapshotOf = (d, w) => ({
  domain: d, weapon: w,
  list: localStorage.getItem(presetListKey(d, w)),
  active: localStorage.getItem(presetActiveKey(d, w)),
  at: Date.now(),
});

function recordUndo(d, w, next) {
  if (undoSuspended) return;
  const before = presetSnapshotOf(d, w);
  if (before.list === null) return;          // nothing existed to go back to
  // A no-op write is not a step. Switching weapons re-applies and re-saves
  // every collection, and a stack full of those would make the first Ctrl+Z
  // do nothing visible.
  if (before.list === JSON.stringify(next)) return;
  const names = (l) => (l || []).map((p) => p.name).join("\u0000");
  let structural = true;
  try { structural = names(JSON.parse(before.list)) !== names(next); } catch (_) {}
  const top = undoStack[undoStack.length - 1];
  // Same collection, still mid-gesture, same presets: keep the OLDER "before".
  if (!structural && top && top.domain === d && top.weapon === w
      && before.at - top.at < UNDO_COALESCE_MS) {
    top.at = before.at;
    return;
  }
  undoStack.push(before);
  if (undoStack.length > UNDO_LIMIT) undoStack.shift();
  redoStack = [];   // a new edit forks the timeline
}

// Each domain's "make the page show this again". The bars already own these
// three operations; this is the same trio the preset bar is built from.
function presetDoc(d) {
  if (d === BUILDS) return {
    setActive: (n) => { activePreset = n; },
    apply: (st) => restoreState(st, presetWeapon()),
    rerender: renderPresetBar,
  };
  if (d === SCENARIOS) return {
    setActive: (n) => { activeScenario = n; },
    apply: applyScenario,
    rerender: renderScenarioBar,
  };
  if (d === OPT_DOMAIN) return {
    setActive: (n) => { activeOptPreset = n; },
    apply: applyOptPreset,
    rerender: renderOptPresetBars,
  };
  if (d === RIVENS) return {
    setActive: (n) => { activeRiven = n; },
    // An undo can land on a collection that is now empty (the last riven
    // deleted) — renderRivens is the one that decides between the list and
    // the editor, so it does the applying too.
    apply: () => { riven = null; },
    rerender: () => { renderRivens(); pruneDanglingRivens(); },
  };
  return null;
}

// Write a remembered collection back and make the page reflect it.
function restorePresetSnapshot(s) {
  undoSuspended = true;
  try {
    if (s.list === null) localStorage.removeItem(presetListKey(s.domain, s.weapon));
    else localStorage.setItem(presetListKey(s.domain, s.weapon), s.list);
    if (s.active === null) localStorage.removeItem(presetActiveKey(s.domain, s.weapon));
    else localStorage.setItem(presetActiveKey(s.domain, s.weapon), s.active);
    // A step taken on ANOTHER weapon is restored in storage but not applied:
    // yanking the editor to a weapon the user has since left would be a
    // second surprise on top of the one being undone.
    if (s.weapon !== presetWeapon()) return;
    const doc = presetDoc(s.domain);
    const list = JSON.parse(s.list || "[]");
    const active = list.find((p) => p.name === s.active) || list[0];
    if (!doc || !active) return;
    doc.setActive(active.name);
    whileApplying(() => doc.apply(active.state));   // a restore is not an edit
    doc.rerender();
  } finally {
    undoSuspended = false;
  }
}

// The stack is ONE timeline, but a button lives on ONE collection, so it acts
// on that collection's most recent step (user, 2026-08-02: undo has to be
// visible, not only a shortcut). Clicking ↶ on the riven toolbar undoing a
// scenario edit would be the shortcut's behaviour wearing a button's clothes.
// Ctrl+Z stays global — that is what a global key should mean.
//
// Filtering is safe because every entry is a whole snapshot of ONE collection:
// restoring an older one for a domain does not depend on what other domains
// did in between.
const lastIn = (stack, d, w) => {
  for (let i = stack.length - 1; i >= 0; i--) {
    if (stack[i].domain === d && stack[i].weapon === w) return i;
  }
  return -1;
};
const canUndoIn = (d) => lastIn(undoStack, d, presetWeapon()) >= 0;
const canRedoIn = (d) => lastIn(redoStack, d, presetWeapon()) >= 0;

function stepIn(from, to, d, label) {
  const w = presetWeapon();
  const i = lastIn(from, d, w);
  if (i < 0) return;
  const step = from.splice(i, 1)[0];
  to.push(presetSnapshotOf(d, w));
  restorePresetSnapshot(step);
  presetToast(`${tr(label)} · ${tr(PRESET_LABELS[d] || d)}`);
}
const undoIn = (d) => stepIn(undoStack, redoStack, d, "undone");
const redoIn = (d) => stepIn(redoStack, undoStack, d, "redone");

// The pair of buttons, for any bar that wants to show them.
const undoButtons = (d) =>
  `<span class="pundo">` +
  `<button class="pop pundo-u" ${canUndoIn(d) ? "" : "disabled"} title="${escHtml(tr("undo (Ctrl+Z)"))}">↶</button>` +
  `<button class="pop pundo-r" ${canRedoIn(d) ? "" : "disabled"} title="${escHtml(tr("redo (Ctrl+Shift+Z)"))}">↷</button>` +
  `</span>`;

// Wire them wherever they were drawn.
function wireUndoButtons(host, d) {
  const u = host.querySelector(".pundo-u"), r = host.querySelector(".pundo-r");
  if (u) u.onclick = (e) => { e.stopPropagation(); undoIn(d); };
  if (r) r.onclick = (e) => { e.stopPropagation(); redoIn(d); };
}

function undoPreset() {
  const step = undoStack.pop();
  if (!step) return presetToast(tr("nothing to undo"));
  redoStack.push(presetSnapshotOf(step.domain, step.weapon));
  restorePresetSnapshot(step);
  presetToast(`${tr("undone")} · ${tr(PRESET_LABELS[step.domain] || step.domain)}`);
}

function redoPreset() {
  const step = redoStack.pop();
  if (!step) return presetToast(tr("nothing to redo"));
  undoStack.push(presetSnapshotOf(step.domain, step.weapon));
  restorePresetSnapshot(step);
  presetToast(`${tr("redone")} · ${tr(PRESET_LABELS[step.domain] || step.domain)}`);
}

const PRESET_LABELS = {
  "builder-builds": "Builds",
  "simulator-scenarios": "Scenarios",
  optimizer: "Searches",
  rivens: "Rivens",
};

// Inline feedback, never a native dialog (those are blocked in the owner's
// browser). It has to say WHICH collection moved: the shortcut is global and
// the slip may have been two tabs ago.
let toastTimer = null;
function presetToast(msg) {
  let el = $("toast");
  if (!el) {
    el = document.createElement("div");
    el.id = "toast";
    document.body.appendChild(el);
  }
  el.textContent = msg;
  el.classList.add("on");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => el.classList.remove("on"), 2200);
}

// Ctrl/Cmd+Z, and Ctrl+Shift+Z / Ctrl+Y to come back. Never while a text
// field has focus: there the browser's own undo is the one being asked for,
// and stealing it would make renaming a preset a trap.
document.addEventListener("keydown", (e) => {
  if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
  const k = e.key.toLowerCase();
  if (k !== "z" && k !== "y") return;
  // The event's own target first: that is where a real keystroke lands, and
  // it is true even when the page itself does not hold focus.
  const el = (e.target && e.target.nodeType === 1 ? e.target : null) || document.activeElement;
  if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable)) return;
  e.preventDefault();
  if (k === "y" || e.shiftKey) redoPreset(); else undoPreset();
});


// One-time move of the pre-module storage keys (2026-07-29 rename).
(function migratePresetKeys() {
  try {
    // Targets are the WEAPON-LESS keys of the previous scheme; migrateWeaponScope()
    // below moves those onto a weapon once META has told us which one.
    const moves = {
      "wfsim-presets": "wfsim-presets-builder-builds",
      "wfsim-active-preset": "wfsim-preset-active-builder-builds",
      "wfsim-opt-mods-presets": "wfsim-presets-optimizer-mods",
      "wfsim-opt-arc-presets": "wfsim-presets-optimizer-arcanes",
      "wfsim-opt-evo-presets": "wfsim-presets-optimizer-evolutions",
    };
    Object.entries(moves).forEach(([from, to]) => {
      const v = localStorage.getItem(from);
      if (v !== null && localStorage.getItem(to) === null) localStorage.setItem(to, v);
      localStorage.removeItem(from);
    });
    const oldActives = JSON.parse(localStorage.getItem("wfsim-opt-active") || "null");
    if (oldActives) {
      Object.entries({ mods: "optimizer-mods", arcs: "optimizer-arcanes", evos: "optimizer-evolutions" }).forEach(([k, d]) => {
        const to = "wfsim-preset-active-" + d;
        if (oldActives[k] && localStorage.getItem(to) === null) localStorage.setItem(to, oldActives[k]);
      });
      localStorage.removeItem("wfsim-opt-active");
    }
  } catch (_) {}
})();

// One-time rewrite of the EVOLUTION ids that dropped their ad-hoc
// abbreviations for full weapon names (2026-07-29: dt_ → dual_toxocyst_,
// lae_ → laetum_). Saved builds store the per-tier selection and the
// optimizer's scope stores its option sets, so both collections carry ids
// that would otherwise silently stop resolving.
(function migrateEvolutionIds() {
  try {
    const fix = (id) => (typeof id === "string"
      ? id.replace(/^dt_/, "dual_toxocyst_").replace(/^lae_/, "laetum_")
      : id);
    const rewriteKeys = (obj) => {
      if (!obj || typeof obj !== "object") return obj;
      const out = {};
      for (const [k, v] of Object.entries(obj)) out[fix(k)] = v;
      return out;
    };
    // Build presets: state.evoSel = { tier: id|null }. The domain names
    // are literals here on purpose: this block runs BEFORE the BUILDS
    // constant is initialised, and a temporal-dead-zone throw would be
    // swallowed by the catch, silently skipping the migration.
    const buildsDomain = "builder-builds";
    const builds = loadPresetList(buildsDomain);
    let touched = false;
    builds.forEach((p) => {
      const sel = p.state && p.state.evoSel;
      if (!sel) return;
      Object.keys(sel).forEach((t) => {
        const nv = fix(sel[t]);
        if (nv !== sel[t]) { sel[t] = nv; touched = true; }
      });
    });
    if (touched) storePresetList(buildsDomain, builds);
    // Optimizer evolution presets: state.evos = { tier: { id: mark } }
    const evoDomain = "optimizer-evolutions";
    const evos = loadPresetList(evoDomain);
    let touched2 = false;
    evos.forEach((p) => {
      const tiers = p.state && p.state.evos;
      if (!tiers) return;
      Object.keys(tiers).forEach((t) => {
        const before = JSON.stringify(tiers[t]);
        tiers[t] = rewriteKeys(tiers[t]);
        if (JSON.stringify(tiers[t]) !== before) touched2 = true;
      });
    });
    if (touched2) storePresetList(evoDomain, evos);
  } catch (_) {}
})();

// The builder module's build presets — domain "builder-builds". A build
// preset captures the WHOLE configuration: weapon, mod slots (mod id +
// polarity + rank), arcane + rank, and the per-tier evolution selection.
const BUILDS = "builder-builds";

function snapshotState() {
  return {
    weapon: $("weapon").value,
    evoSel: { ...evoSel },
    arcane: arcanes,
    arcaneRank: arcaneRanks,
    slots: slots.map((s) => ({ mod: s.mod, pol: s.pol, rank: s.rank })),
    mode,
    // The VALENCE, for the same reason `mode` is here: it is part of what this
    // build IS, and two builds of one weapon may differ only in it.
    valence: { ...valence },
    // NO `sim` FIELD. A build used to carry a snapshot of the fight, which
    // `restoreState` then applied — so picking a build silently rewrote the
    // scenario you were working in. The scenario is INDEPENDENT (user,
    // 2026-08-02): nothing outside `simulator-scenarios` writes it.
    //
    // Nothing is lost. "What this build was last measured under" was never
    // this field's job — `lastResult.key` is that record, it lives outside
    // `state`, and it is what makes a stale result show as stale.
  };
}

// Apply a saved state. `weapon` is the weapon it belongs to — pass it and the
// payload's own `st.weapon` is IGNORED.
//
// That parameter is the whole anti-crossing design. A preset's owner is already
// decided by its storage key (`wfsim-presets-<weapon>-<domain>`), so carrying a
// weapon inside the payload too gave the same fact two homes — and every
// crossing bug was those two disagreeing: a payload saying `laetum` under Dual
// Toxocyst's key dragged the editor (and the URL, below) to Laetum. Repairing
// the stored data only chased the symptom; not READING it makes crossing
// structurally impossible, whatever a payload happens to contain.
//
// Only the language-switch stash omits the argument: it restores the whole page
// including which weapon was open, so there the payload IS the authority.
function restoreState(st, weapon) {
  const w = weapon || (st && st.weapon);
  if (!st || !weaponInfo(w)) return;
  $("weapon").value = w;
  // Keep the route honest when the restore changes weapon (the stash case);
  // for a preset `w` is already the current weapon, so this is a no-op.
  if (!document.querySelector(".config-page").hidden) {
    history.replaceState(null, "", weaponModPath(w));
  }
  applyWeapon(w, null); // resets pool/innate/visibility
  (st.slots || []).forEach((s, i) => {
    if (i >= slots.length) return;
    slots[i].mod = s.mod && modById(s.mod) ? s.mod : null; // drop ids gone from the pool
    slots[i].pol = s.pol ?? null;
    slots[i].rank = s.rank ?? null;
  });
  evoSel = { 1: null, 2: null, 3: null, 4: null, ...(st.evoSel || {}) };
  // ONTO A DEFAULT, never onto whatever the last build was playing. A preset
  // written before this field existed is played the way the arsenal plays it,
  // which is exactly what the board's own migration does with a mode-less
  // submission — and what keeps a builtin board build showing the mode it was
  // MEASURED in rather than the one you happened to be in.
  mode = defaultMode(w, st.mode);
  // ONTO A DEFAULT, never onto the last build's — a valence is a statement
  // about one weapon and nothing crosses between them. `defaultValence` also
  // drops an element this weapon's spec does not offer, which is what a preset
  // copied across weapons carries.
  valence = defaultValence(w, st.valence);
  arcanes = arcanesFor(w, st.arcane);
  arcaneRanks = asArcaneList(st.arcaneRank, arcanes.length).map((x) => x ?? null);
  // The scenario is NOT restored: it belongs to `simulator-scenarios` and a
  // build has no opinion about it. An old preset may still carry `st.sim`;
  // it is ignored rather than migrated, because reading it back is the exact
  // behaviour this removed (user, 2026-08-02).
  // A BOARD ROW IS A BUILD, NOT A LAYOUT: it carries mods and no polarities,
  // so restoring it verbatim shows a legal build as an impossible one — full
  // drain, 91/60 in red. The cheapest legal layout is planned on the spot.
  //
  // IT LIVES HERE because TWO callers restore a build — the build bar, and
  // `initPresets` on boot — and the plan used to live in the bar's `apply()`
  // only. So landing on a page whose active build was a benchmark build showed
  // the wrong Forma until you clicked something (owner, 2026-08-04). Both
  // callers set the active preset BEFORE restoring, which is what makes
  // the question answerable here.
  //
  // Nothing of yours is at risk: a benchmark build is read-only and has no
  // hand-set polarity to overwrite.
  if (officialBuildActive()) autoForma();
  renderMods(); renderArcanes(); renderEvo(); renderMode(); renderValence(); renderSim(); refreshPanel();
  renderStoredSimResult(); // the simulator shows THIS preset's last test
}

// Kill score PER MINUTE. The score is whole kills plus the fraction of the
// current target's pool already drained, so it grows with the engagement —
// dividing by the clock is what makes two runs of different length
// comparable, exactly as DPS does for damage (user, 2026-07-31).
const kpm = (score, duration) => (duration > 0 ? ((score || 0) * 60) / duration : 0);

const escHtml = (s) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));

// Decimal formatting with a SIGNIFICANCE floor (user, 2026-07-29): two
// decimals normally, but a very small number grows decimals until it
// carries 2 significant digits — 0.0085 stays 0.0085 instead of
// collapsing to 0.01. Used for every score/percentage in the results.
// EXACT ZERO keeps the plain "0.00" — only a nonzero-but-tiny value grows
// decimals (log10(0) is -Infinity, so zero must short-circuit first).
const sig2 = (x, min = 2) => {
  const v = Number(x);
  if (!Number.isFinite(v) || v === 0) return (0).toFixed(min);
  const a = Math.abs(v);
  const need = a >= 1 ? min : Math.max(min, 1 - Math.floor(Math.log10(a)));
  return v.toFixed(need);
};
const pct2 = (x) => sig2((Number(x) || 0) * 100) + "%";

// One-time move of the weapon-LESS preset lists onto a weapon. They were
// written before presets were scoped, so they belong to whatever the user
// was last looking at — which we cannot know. They go to the weapon that
// is active when the migration runs (the one being restored on this load),
// and everything else starts empty. Nothing is lost: "⇤ import" reaches
// any weapon's presets from any other.
// The optimizer's three scope domains merged into one on 2026-08-02; the old
// names stay listed so a browser arriving from before the weapon-scope move
// still lands its data where migrateOptPresets can find it.
const PRESET_DOMAINS = ["builder-builds", "optimizer", "optimizer-mods",
  "optimizer-arcanes", "optimizer-evolutions", "simulator-scenarios"];
// The SIMULATOR's collection: one saved fight — enemy, technique, measurement.
// Its own domain because a build is tested against SEVERAL of them, and
// because the marginal-gain scan needs to name which one it ran under. The
// build preset keeps carrying its own `sim` (a build remembers what it was
// last tested with); this is the reusable library beside it.
const SCENARIOS = "simulator-scenarios";
let activeScenario = "";
function migratePresetsToWeaponScope() {
  const w = presetWeapon();
  if (!w) return;
  PRESET_DOMAINS.forEach((d) => {
    // A SHARED DOMAIN IS ALREADY AT ITS FINAL KEY. This migration's whole job is
    // to move a weapon-LESS key under a weapon, which is precisely what the
    // scenario list must not have done to it — it would file the shared list
    // under whichever weapon happened to be open and take it away from every
    // other one, on every load.
    if (isSharedDomain(d)) return;
    [["wfsim-presets-" + d, presetListKey(d, w)],
     ["wfsim-preset-active-" + d, presetActiveKey(d, w)]].forEach(([from, to]) => {
      const v = localStorage.getItem(from);
      if (v === null) return;
      // RESCOPE while moving. The legacy list was global, so its entries carry
      // whatever weapon was current when each was saved — filing them verbatim
      // under `w` leaves a preset that yanks the editor to another weapon the
      // moment it loads. Only the preset LIST needs this; the active-name key
      // is a bare string.
      let out = v;
      if (from.startsWith("wfsim-presets-")) {
        try {
          out = JSON.stringify(JSON.parse(v).map((p) =>
            p && p.state ? { ...p, state: { ...p.state, weapon: w } } : p));
        } catch (_) { /* unparseable: move it as-is, initPresets repairs it */ }
      }
      if (localStorage.getItem(to) === null) localStorage.setItem(to, out);
      localStorage.removeItem(from);
    });
  });
}

// Every weapon that has at least one stored preset in `domain`, for the
// import picker: [{ id, name, presets }]. Iterates META rather than parsing
// key strings — weapon ids and domains both contain separators.
function presetSources(domain, exceptWeapon) {
  return (META ? META.weapons : [])
    .filter((w) => w.id !== exceptWeapon)
    .map((w) => ({ id: w.id, name: w.name, presets: loadPresetList(domain, w.id) }))
    .filter((w) => w.presets.length);
}

// The ACTIVE preset — the one the editor is editing. Never null after
// init: the page always has ≥1 preset and is always editing one (user's
// mental model, 2026-07-28: "if no presets exist, the current state IS
// preset 1"). Restored across reloads.
let activePreset = null;

// A stored buff config outlives the RULE it was written under. Every stacking
// buff used to open at full stacks; they open EARNED at zero now, and
// `syncBuffConfig` only seeds an id it has never seen — so every scenario
// saved before that change kept the old numbers and the new rule looked
// broken on exactly the builds that had been tested most (user, 2026-08-03).
//
// One-time, and it drops the whole map rather than guessing which entries were
// deliberate: `{stacks: 3}` on a 3-stack buff is what "never touched" and
// "chose the maximum" both look like, and there is no third field to tell them
// apart. Re-seeding from the server's defaults is the answer the rule change
// promised; a wrong guess about intent is not.
(function migrateBuffDefaults() {
  const FLAG = "wfsim-buff-defaults-earned";
  try {
    if (localStorage.getItem(FLAG)) return;
    Object.keys(localStorage)
      .filter((k) => /^wfsim-presets-.*-(simulator-scenarios|builder-builds)$/.test(k))
      .forEach((k) => {
        const list = JSON.parse(localStorage.getItem(k));
        if (!Array.isArray(list)) return;
        let hit = false;
        list.forEach((p) => {
          if (p.state && p.state.buffs && Object.keys(p.state.buffs).length) {
            p.state.buffs = {}; hit = true;
          }
          // Builds no longer carry a fight at all — drop the dead copy while
          // we are here rather than leaving it to be misread later.
          if (p.state && p.state.sim) { delete p.state.sim; hit = true; }
        });
        if (hit) localStorage.setItem(k, JSON.stringify(list));
      });
    localStorage.setItem(FLAG, "1");
  } catch (_) { /* a browser with no storage has nothing to migrate */ }
})();

function initPresets() {
  migratePresetsToWeaponScope();
  let ps = loadPresetList(BUILDS);
  // (The old 300-run migration lived here. It rewrote `state.sim.runs` inside
  // BUILD presets — a field nothing reads any more, because a build no longer
  // carries a copy of the fight.)
  if (!ps.length) {
    // BLANK, NEVER THE LIVE STATE. A weapon opened for the first time is a bare
    // weapon — the same rule the scenario has carried since 2026-08-02
    // ("绝对不能串"), applied to the build, where it was missing.
    //
    // `snapshotState()` here is the state of the weapon you just LEFT, and this
    // runs during the switch. Mods survived it only because `restoreState`
    // prunes them against the new weapon's pool; an ARCANE has no such prune
    // when it fits, so a Primary Crux picked up from a board build followed you
    // onto every primary you opened afterwards, and was WRITTEN into that
    // weapon's own "build 1" (owner, 2026-08-08). Reproduced: open the
    // Boar through a board row, switch to the Sybaris, and the Sybaris's
    // first build has the arcane.
    //
    // Every axis a future build gains is covered by this, because the answer is
    // "the blank one" rather than "the live one minus what does not fit".
    ps = [{ name: "build 1", savedAt: Date.now(), state: blankBuildState() }];
    storePresetList(BUILDS, ps);
  }
  let sc = loadPresetList(SCENARIOS);
  if (!sc.length) {
    // FROM THE DEFAULTS, never from the live fight — see `defaultScenario`.
    // This runs on a weapon switch, when `sim` still holds the weapon you
    // just left.
    sc = [{ name: "scenario 1", savedAt: Date.now(), state: defaultScenario() }];
    storePresetList(SCENARIOS, sc);
  }
  // Resolved against the JOINT list, so the official scenario can be the one
  // you left open — it is a scenario like any other to everything downstream.
  //
  // THE OFFICIAL ONE IS THE DEFAULT (owner, 2026-08-05). This costs nothing and
  // corrects something that was quietly misleading: `defaultScenario()` is
  // ALREADY the benchmark's fight, field for field — same enemy, level 9999,
  // Steel Path, 180 s, 1000 runs, kpm, aiming, infinite ammo. "scenario 1" was
  // the ruler wearing a private name, so its number could not be compared with
  // anyone's and could not reach the board, for no reason a player could see.
  //
  // Landing on the official one instead means a first number is a COMPARABLE
  // number. Nobody's results move — it is the same fight.
  const lastSc = localStorage.getItem(presetActiveKey(SCENARIOS));
  // THE ID, like every other write to this pointer. Storing a builtin's NAME
  // here left the boot state spelled differently from every later selection —
  // and `presetId(p) === activeScenario` is false against a name, so readers
  // that resolve by id fell through to the first ruler until you touched the
  // bar once.
  activeScenario =
    presetId(scenarioNamed(lastSc))
    || presetId(builtinScenarios()[0])
    || presetId(sc[0]);
  localStorage.setItem(presetActiveKey(SCENARIOS), activeScenario);

  const here = presetWeapon();
  const last = localStorage.getItem(presetActiveKey(BUILDS));
  activePreset = presetId(buildNamed(last)) || presetId(ps[0]);
  localStorage.setItem(presetActiveKey(BUILDS), activePreset);
  // Applied under THIS weapon, never the payload's — a preset filed here
  // belongs here by definition.
  whileApplying(() => {
    restoreState(buildNamed(activePreset).state, here);
    // THE FIGHT, from its own collection. It used to arrive inside the build
    // preset, which is exactly why picking a build changed the scenario; it
    // now comes from the active `simulator-scenarios` entry, and this is the
    // only place the live scenario is seeded on load or on a weapon switch.
    applyScenario(scenarioNamed(activeScenario).state);
  });
  renderPresetBar();
  lockOfficialBuild();
}

// ---- Auto-save (user, 2026-07-29: no manual save click) ---------------
// A preset MIRRORS the editor: every edit debounces straight into the
// active preset's stored state, so there is no save button and no
// unsaved-changes dot. Branch before experimenting with "+ new" (empty)
// or the ⧉ duplicate — the preset you leave behind keeps its content.
//
// `presetApplying` guards the other direction: loading a preset (or a
// weapon switch) re-renders everything, and those renders must NOT be
// mistaken for user edits and written back — a cross-weapon apply drops
// unknown ids, which auto-save would otherwise make permanent.
let presetApplying = 0;
function whileApplying(fn) {
  presetApplying++;
  try {
    fn();
  } finally {
    presetApplying--;
    // Renders during the apply queue their own debounced saves — drop
    // them, or the applied (possibly pruned) state writes itself back.
    clearTimeout(presetSaveTimer);
    clearTimeout(optSaveTimer);
  }
}

let presetSaveTimer = null;
function markPresetDirty() {
  if (presetApplying) return;
  clearTimeout(presetSaveTimer);
  presetSaveTimer = setTimeout(() => {
    if (!activePreset || presetApplying) return;
    // An official build is not written — same rule as the official scenario,
    // and enforced in the same place. Auto-save is what would otherwise make
    // read-only a suggestion.
    if (officialBuildActive()) return;
    const ps = loadPresetList(BUILDS);
    const at = ps.findIndex((p) => p.name === activePreset);
    if (at < 0) return;
    ps[at] = { ...ps[at], savedAt: Date.now(), state: snapshotState() };
    storePresetList(BUILDS, ps);
  }, 400);
}

let scenarioSaveTimer = null;
let gainRefreshTimer = null;
// The scenario's own auto-save. Same contract as the build's — the editor IS
// the preset — but a different collection, because a build is tested against
// several fights and each of them is worth keeping.
function markScenarioDirty() {
  if (presetApplying) return;
  clearTimeout(scenarioSaveTimer);
  scenarioSaveTimer = setTimeout(() => {
    if (!activeScenario || presetApplying) return;
    // THE OFFICIAL SCENARIO IS NOT WRITTEN. Auto-save is what would otherwise
    // make "read-only" a lie: every edit debounces into the active preset, so
    // a disabled control is a suggestion and this line is the rule. It is also
    // why the official one is not in localStorage at all — there is nothing
    // here to write into even if this check were removed.
    if (officialScenarioActive()) return;
    const ps = loadPresetList(SCENARIOS);
    const at = ps.findIndex((p) => presetId(p) === activeScenario);
    if (at < 0) return;
    ps[at] = { ...ps[at], savedAt: Date.now(), state: snapshotScenario() };
    storePresetList(SCENARIOS, ps);
  }, 400);
  // ...and the QUICK CALC, which measures under a scenario and therefore goes
  // stale when one is edited. It waits for the same debounce because the scan
  // resolves the SAVED preset over `sim` — refreshing before the write would
  // re-measure the old fight. `ensureGains` re-checks the key, so an edit the
  // chosen scenario does not use costs nothing.
  clearTimeout(gainRefreshTimer);
  gainRefreshTimer = setTimeout(refreshGains, 450);
}

// ---- The ONE preset-bar component -------------------------------------
// Every preset bar on the page (the build bar and the optimizer's three
// scope bars) is the same template on the same document model: label +
// count, the chips (the active one carries duplicate / rename / delete;
// the last remaining preset cannot be deleted — there is always one),
// "+ new". Edits AUTO-SAVE into the active preset, so there is no save
// button and no dirty marker. "+ new" creates an EMPTY preset instantly
// under an auto-name ("preset N") and switches to it — no naming step
// (user, 2026-07-29); rename after via ✎. Branching an existing preset
// is the ⧉ duplicate on the active chip.
// Counts are UNLIMITED, so past PRESET_FILTER_AT chips the bar grows a
// name filter; the active chip always shows (it is the document being
// edited).
const PRESET_FILTER_AT = 10;
const presetFilters = {}; // per-bar filter text — survives re-renders, not persisted

// SELECT and COPY, lifted out of the bar so the BENCHMARK bar performs the
// same two actions rather than its own versions of them (owner, 2026-08-04).
// They are the only two a read-only entry has, and "the copy is an ordinary
// editable preset" has to stay one behaviour — a second implementation is how
// one bar's copy comes to capture something the other's does not.
/// WHAT THE ACTIVE POINTER STORES, and it is not the label. An official entry's
/// NAME is a rank inside one ruler — "#1 · Incarnon cycle" — so the aimed board
/// and the no-aim board each have one, and `find(x => x.name === n)` returned
/// whichever came first. That is the whole of the bug where the no-aim board's
/// leader opened the AIMED board's leader instead (owner, 2026-08-08).
///
/// `builtin` is already unique per ruler, mode and rank; a preset of your own
/// has none and is its own name. So this is the id, and `presetLabel` is what a
/// reader sees — the two were the same string until the board grew a second
/// ruler.
const presetId = (p) => (p || {}).builtin || (p || {}).name || "";
const presetLabel = (p) => (p || {}).name || "";

const pickPreset = (cfg, key) => {
  const ps = cfg.load();
  // By ID first: a name may now be shared by two rulers' rows.
  const p = ps.find((x) => presetId(x) === key) || ps.find((x) => x.name === key);
  if (!p || presetId(p) === cfg.active()) return;
  cfg.setActive(presetId(p));
  whileApplying(() => cfg.apply(p.state)); // a load is not an edit
  cfg.rerender();
};

// The copy captures the LIVE editor state and becomes the active document; the
// original keeps what auto-save last wrote into it. For a read-only entry the
// live state IS that entry, because selecting it is what put it there.
const copyActivePreset = (cfg) => {
  const ps = cfg.load();
  const base = cfg.active();
  const name = freeName(ps, (n) => base + " copy" + (n > 1 ? " " + n : ""));
  ps.push({ name, savedAt: Date.now(), state: cfg.snapshot() });
  cfg.store(ps);
  cfg.setActive(name);
  cfg.rerender();
};

// THE BENCHMARK BAR — official entries, in a bar of their own above the
// player's (owner, 2026-08-04). Same chip styling, deliberately: it is the same
// kind of thing to pick. A different component, also deliberately: what you can
// DO differs on every count — no new, no rename, no delete, no import, no
// filter, no undo, because none of it is yours.  What is left is select and ⧉.
//
// Splitting them also removes the special-casing that had accumulated inside
// one bar (a `readonly(p)` branch in three places) and the ambiguity it caused:
// a count reading "Builds 4" when three were yours and the fourth was not.
//
// Absent when empty, and empty is ORDINARY — a weapon nobody has submitted a
// build for has no benchmark builds, and a bar with a label and no chips
// invites the question of what is missing.
function renderBenchmarkBarIn(bar, cfg) {
  if (!bar) return;
  const ps = cfg.load().filter((p) => p.builtin);
  const active = cfg.active();
  bar.hidden = !ps.length;
  if (!ps.length) { bar.innerHTML = ""; return; }
  const noun = cfg.noun || "preset";
  // TWO PICKS, BECAUSE THERE ARE TWO QUESTIONS (owner, 2026-08-08). A chip row
  // was right while a weapon had ten official builds under one ruler; one
  // dropdown holding rulers x modes x ranks was right while that was forty
  // rows. The board is designed for a hundred rulers with a hundred rows each,
  // and at that size a
  // single list is not a list — you cannot scan it, and searching it means
  // knowing what a rank is a rank ON before you can ask.
  //
  // So: WHICH RULER, then WHICH ROW INSIDE IT. Each list stays the size of one
  // question. The second one appears only where the first leaves a choice —
  // the official SCENARIOS are one per ruler, so picking the ruler IS picking
  // the scenario and a second control would be a control with one item.
  const ruler = (p) => p.benchmark || p.builtin;
  const sel = ps.find((p) => presetId(p) === active) || null;
  const rulers = [];
  for (const p of ps) {
    if (!rulers.some((r) => r.id === ruler(p))) rulers.push({ id: ruler(p), label: p.group || p.name });
  }
  const curRuler = sel ? ruler(sel) : rulers[0].id;
  const inRuler = ps.filter((p) => ruler(p) === curRuler);
  const grouped = inRuler.some((p) => p.subgroup);
  bar.innerHTML =
    `<span class="plabel bench" title="${escHtml(cfg.benchHint || "")}">${escHtml(cfg.benchLabel)} <b>${ps.length}</b></span>` +
    // THE RULER. Searched, because this list is the one designed to reach a
    // hundred, and every row of the board hangs off which one you are reading.
    ddButton(`dd-bench-${cfg.domain}`, {
      value: curRuler,
      search: true,
      title: cfg.benchHint || "",
      items: rulers.map((r) => ({ value: r.id, label: r.label })),
      // Picking a ruler lands on its FIRST row, which is its leader — the same
      // thing clicking that weapon on the board page gives you.
      onPick: (v) => {
        const first = ps.find((p) => ruler(p) === v);
        if (first) pickPreset(cfg, presetId(first));
      },
    }) +
    (inRuler.length > 1
      ? ddButton(`dd-bench-row-${cfg.domain}`, {
        value: presetId(sel || inRuler[0]),
        search: true,
        title: cfg.rowHintTitle || "",
        // GROUPED BY MODE inside the ruler where the weapon has more than one:
        // a hundred rows split into "base" and "cycle" is two readable lists,
        // and a rank only means something within one way of playing.
        items: inRuler.map((p) => ({
          value: presetId(p),
          label: grouped && p.rank != null ? `#${p.rank}` : p.name,
          hint: p.rowHint || p.hint || (cfg.roTitle ? cfg.roTitle(p) : ""),
          group: p.subgroup || "",
        })),
        onPick: (v) => pickPreset(cfg, v),
      })
      : "") +
    (sel
      ? `<button class="pop dup" title="${escHtml(
          tr("copy it into a {thing} of your own — the official one cannot be edited")
            .replace("{thing}", tr(noun)))}">⧉</button>`
      : "");
  const dup = bar.querySelector(".pop.dup");
  if (dup) dup.addEventListener("click", (e) => { e.stopPropagation(); copyActivePreset(cfg); });
}

function renderPresetBarIn(bar, cfg) {
  // WHAT ONE OF THESE IS CALLED. "Preset" is the CATEGORY — a saved state of
  // a module, as opposed to a custom — and no collection is named after its
  // category (user, 2026-08-02). A build is a build, a scenario a scenario, a
  // search a search; the noun names new ones and every tooltip that has to
  // refer to one.
  const noun = cfg.noun || "preset";
  // YOURS ONLY. The official entries are drawn by `renderBenchmarkBarIn` in the
  // bar above; keeping them out of `ps` here is what makes the count, the
  // filter threshold and the "the last one cannot be deleted" rule all count
  // the same thing — the presets you own.
  const ps = cfg.load().filter((p) => !p.builtin);
  const active = cfg.active();
  const ftext = presetFilters[bar.id] || "";
  const f = ftext.trim().toLowerCase();
  const shown = f ? ps.filter((p) => p.name === active || p.name.toLowerCase().includes(f)) : ps;
  const hint = cfg.hint ? ` (${cfg.hint})` : "";
  const chip = (p) => {
    const sel = p.name === active;
    const ops = !sel
      ? ""
      : `<button class="pop dup" title="${escHtml(tr("duplicate"))}">⧉</button>` +
        `<button class="pop ren" title="rename">✎</button>` +
        (ps.length > 1 || cfg.optional ? `<button class="pop del" title="delete">✕</button>` : "");
    return `<span class="pchip ${sel ? "sel" : ""}" data-name="${escHtml(p.name)}" title="switch to ${escHtml(p.name)}${escHtml(hint)}">${escHtml(p.name)}${ops}</span>`;
  };
  bar.innerHTML =
    // Every bar says the shortcut: auto-save means a slip is written before
    // you can regret it, so the way back has to be visible on the thing that
    // slipped.
    `<span class="plabel" title="${escHtml(tr("Ctrl+Z undoes the last change"))}">${cfg.label} <b>${ps.length}</b></span>` +
    (ps.length > PRESET_FILTER_AT ? `<input class="pfilter" type="text" placeholder="${escHtml(tr("filter…"))}" value="${escHtml(ftext)}">` : "") +
    shown.map(chip).join("") +
    // One template, not two words joined by a space: Chinese does not put one
    // between them, so concatenating produced "新建空白 配装".
    `<span class="pchip add" title="${escHtml(
      tr("new empty {thing}").replace("{thing}", tr(noun)) + (cfg.hint ? " · " + cfg.hint : "")
    )}">+ new</span>` +
    // Presets are per weapon, so bringing one over from another weapon is
    // an explicit action rather than a side effect of switching weapons.
    // …EXCEPT a SHARED one, which has no "another weapon" to import from: the
    // scenario list is the same list everywhere, so the control would offer to
    // copy a fight into the collection it is already in.
    (!isSharedDomain(cfg.domain) && presetSources(cfg.domain, presetWeapon()).length
      ? `<span class="pchip imp" title="${escHtml(tr("copy one from another weapon"))}">⇤ ${escHtml(tr("import"))}</span>`
      : "") +
    (cfg.extra || "") +
    undoButtons(cfg.domain) +
    `<div class="pimport" hidden></div><div class="pshare" hidden></div>`;
  wireUndoButtons(bar, cfg.domain);
  if (cfg.onExtra) cfg.onExtra(bar);

  // Typing re-renders the bar (chips re-filter), so hand focus back.
  const filt = bar.querySelector(".pfilter");
  if (filt) filt.addEventListener("input", () => {
    presetFilters[bar.id] = filt.value;
    cfg.rerender();
    const nf = bar.querySelector(".pfilter");
    if (nf) { nf.focus(); nf.setSelectionRange(nf.value.length, nf.value.length); }
  });
  bar.querySelectorAll(".pchip:not(.add)").forEach((c) =>
    c.addEventListener("click", () => pickPreset(cfg, c.dataset.name)));
  // No prompt()/alert()/confirm() anywhere — the browser can block those
  // dialogs, which made saving silently fail (user, 2026-07-28). Naming
  // happens in an INLINE input: Enter commits, Esc cancels.
  const nameInput = (placeholderEl, initial, onCommit) => {
    placeholderEl.outerHTML = `<input class="pname" type="text" value="${escHtml(initial)}" placeholder="name, then Enter…" maxlength="24">`;
    const inp = bar.querySelector(".pname");
    inp.focus();
    if (initial) inp.select();
    let done = false;
    const commit = () => {
      if (done) return;
      done = true;
      onCommit((inp.value || "").trim());
    };
    inp.addEventListener("keydown", (ev) => {
      if (ev.key === "Enter") commit();
      if (ev.key === "Escape") { done = true; cfg.rerender(); }
    });
    inp.addEventListener("blur", commit);
  };
  // Unique auto-names: "+ new" takes the smallest free "preset N";
  // duplicate takes "<name> copy", then "<name> copy 2", …
  const addBtn = bar.querySelector(".pchip.add");
  addBtn.addEventListener("click", (e) => {
    e.stopPropagation();
    const ps2 = cfg.load();
    // "preset N" everywhere except where the thing has its own noun — the
    // riven bar names them "riven N", because that is what they are.
    const name = freeName(ps2, (n) => noun + " " + n);
    // Activate FIRST so everything that renders during apply() (e.g. the
    // sim's per-preset stored result) already sees the NEW preset; then
    // apply the blank and store the resulting live snapshot, so the stored
    // state matches exactly what the editor now shows.
    cfg.setActive(name);
    whileApplying(() => cfg.apply(cfg.blank()));
    ps2.push({ name, savedAt: Date.now(), state: cfg.snapshot() });
    cfg.store(ps2);
    cfg.rerender();
  });
  const on = (sel, fn) => { const b = bar.querySelector(sel); if (b) b.addEventListener("click", (e) => { e.stopPropagation(); fn(); }); };
  on(".pop.dup", () => copyActivePreset(cfg));
  on(".pop.ren", () => {
    const chipEl = bar.querySelector(".pchip.sel");
    if (!chipEl) return;
    nameInput(chipEl, cfg.active(), (name) => {
      const ps2 = cfg.load();
      // Empty, unchanged, or colliding names just cancel the rename.
      if (name && name !== cfg.active() && !ps2.some((p) => p.name === name)) {
        const at = ps2.findIndex((p) => p.name === cfg.active());
        if (at >= 0) {
          ps2[at].name = name;
          cfg.store(ps2);
          cfg.setActive(name);
        }
      }
      cfg.rerender();
    });
  });
  // ⇤ import: an INLINE panel (no native dialogs) listing every OTHER
  // weapon's presets in this same collection. Picking one copies it here
  // under a free name and makes it active; ids the current weapon cannot
  // use are pruned by apply(), exactly as a normal load would.
  const impBtn = bar.querySelector(".pchip.imp");
  const impPanel = bar.querySelector(".pimport");
  if (impBtn) impBtn.addEventListener("click", (e) => {
    e.stopPropagation();
    if (!impPanel.hidden) { impPanel.hidden = true; return; }
    const src = presetSources(cfg.domain, presetWeapon());
    impPanel.innerHTML = src.map((w) =>
      `<div class="pimp-w"><span class="pimp-wn">${escHtml(w.name)}</span>` +
      w.presets.map((p) =>
        `<span class="pchip pimp-p" data-weapon="${escHtml(w.id)}" data-name="${escHtml(p.name)}">${escHtml(p.name)}</span>`
      ).join("") + `</div>`).join("");
    impPanel.hidden = false;
    impPanel.querySelectorAll(".pimp-p").forEach((el) => el.addEventListener("click", (ev) => {
      ev.stopPropagation();
      const from = loadPresetList(cfg.domain, el.dataset.weapon).find((x) => x.name === el.dataset.name);
      if (!from) return;
      const ps2 = cfg.load();
      const name = freeName(ps2, (n) => el.dataset.name + (n > 1 ? " " + n : ""));
      // RESCOPE before applying. A build preset carries its own weapon, and
      // restoreState honours it — so importing without this would navigate
      // the editor back to the SOURCE weapon and then store the copy in the
      // source's list, which is the bleed we are removing, not a fix for it.
      const state = cfg.rescope ? cfg.rescope(from.state, presetWeapon()) : from.state;
      cfg.setActive(name);
      // Apply FIRST, then snapshot: what gets stored is what this weapon
      // can actually hold, not the source weapon's raw state.
      whileApplying(() => cfg.apply(state));
      ps2.push({ name, savedAt: Date.now(), state: cfg.snapshot() });
      cfg.store(ps2);
      cfg.rerender();
    }));
  });

  on(".pop.del", () => {
    const ps2 = cfg.load().filter((p) => p.name !== cfg.active());
    // OPTIONAL collections may go to zero. For everything else there is
    // always at least one, because the module behind it always has a state —
    // a build, a fight, a search — and "no build" is not a thing the builder
    // can show. A riven is different: not owning one is the common case, and
    // a blank card standing in for it is a claim nobody made.
    if (!ps2.length && !cfg.optional) return;
    cfg.store(ps2);
    cfg.setActive(ps2.length ? ps2[0].name : "");
    whileApplying(() => cfg.apply(ps2.length ? ps2[0].state : null));
    cfg.rerender();
  });
}

// An EMPTY build for "+ new": the CURRENT weapon (the page is a weapon
// page — a new preset should not navigate away), bare slots, no arcane, no
// evolutions. NO scenario: a build does not carry a fight, so making one
// cannot reset the fight you are in.
function blankBuildState() {
  return {
    weapon: $("weapon").value,
    evoSel: {},
    arcane: ["none"],
    arcaneRank: [null],
    slots: [],
  };
}

// ---- THE OFFICIAL BUILDS ----------------------------------------------
//
// The board (`data/benchmarks/boards/`), as read-only chips in the BUILD bar.
// Not a tab (user, 2026-08-04): what a board row produces is a BUILD, and the
// builder is what consumes a build — so it belongs in the collection that
// already holds builds, marked as something you did not make.
//
// Same three properties as the official scenario: nothing stores them, nothing
// edits them, ⧉ copies one into an ordinary build of your own.
// The board, fetched at RUNTIME rather than compiled in. `data/` is embedded
// into the wasm at build time, so a board served through META would make every
// hourly update a full site rebuild. This is one small file on the same origin,
// written by the scoring job beside the canonical yaml.
//
// An absent or unreachable board is an EMPTY board, not an error: before the
// first submissions there is nothing to show, and that is a state the page has
// to render anyway.
let BOARD = {};
async function loadBoard() {
  try {
    const r = await fetch("/board.json", { cache: "no-cache" });
    BOARD = r.ok ? await r.json() : {};
  } catch (_) {
    BOARD = {};
  }
}

/// THE BOARD'S ROWS, as read-only builds you can open.
///
/// RANKED WITHIN A RULER AND A MODE, because that is the only thing a rank is.
/// `#1` used to be the first row for the weapon across everything the board
/// held, which was unambiguous exactly while there was one benchmark and one
/// way to play — two of each turn it into a number that names nothing. So the
/// grouping is (benchmark, mode) and the chip says which.
///
/// The mode travels IN THE STATE, so opening a board build plays it the way it
/// was measured. Without that, picking "#1" would show its mods in whatever
/// mode you happened to be in and quietly report a different number than the
/// board does — the same shape as the scenario leak, and worse, because this
/// one has a published figure sitting next to it.
const builtinBuilds = () => {
  const w = weaponInfo($("weapon").value) || {};
  const rows = BOARD[w.id] || [];
  const many = ((w.modes || []).length > 1);
  const rank = {};
  return rows.map((row) => {
    const mode = row.mode || "base";
    const key = `${row.benchmark}#${mode}`;
    rank[key] = (rank[key] || 0) + 1;
    const n = rank[key];
    const bench = (META.benchmarks || []).find((b) => b.id === row.benchmark);
    return {
      name: many ? `#${n} · ${modeLabel(w, mode)}` : `#${n}`,
      // The two halves of that name, for a picker that puts the MODE in a
      // group header and leaves the row to say the rank.
      rank: n,
      subgroup: many ? modeLabel(w, mode) : "",
      // Unique per ruler AND mode: the id is what the active pointer stores,
      // and two rulers' first rows are two different builds.
      builtin: `${row.benchmark}#${mode}#${n}`,
      benchmark: row.benchmark,
      mode,
      board: row,
      // The RULER is the group header; the row says what varies inside it.
      group: bench ? tr(bench.name) : row.benchmark,
      hint: String(row.shown != null ? row.shown : (row.score || 0).toFixed(4)),
      // ...and the flat form, for anywhere that shows no headers.
      hint_flat: `${bench ? tr(bench.name) : row.benchmark} · ${
        row.shown != null ? row.shown : (row.score || 0).toFixed(4)}`,
      savedAt: 0,
      state: {
        weapon: w.id,
        mode,
        slots: Array.from({ length: 9 }, (_, k) => ({
          mod: (row.mods || [])[k] || null, pol: null, rank: null,
        })),
        evoSel: (row.evolutions || []).reduce((m, id, k) => ({ ...m, [k + 1]: id }), {}),
        arcane: (row.arcanes || []).length ? row.arcanes : ["none"],
        arcaneRank: [null],
        // THE PROGENITOR ELEMENT the row was scored with, at the roll's
        // MAXIMUM — which is the ruler's own term, not the row's, so it is
        // taken from the weapon's spec rather than stored per row. Without
        // this a Kuva row opens at whatever the last build was carrying and
        // re-running it matches no line on the board.
        valence: row.valence
          ? { element: row.valence, bonus: (valenceSpec(w.id) || {}).max || 0 }
          : undefined,
      },
    };
  });
};
/// A BOARD ROW, OPENED. `?bench=<id>` names the ruler the row was read under,
/// `?mode=` how the weapon was played; together they identify exactly one
/// official build, and the row is not reproducible without both halves — so
/// this selects the ruler's own SCENARIO as well as the build.
///
/// Selecting the fight here is not a build writing the fight: it is the LINK
/// carrying both, which is what a board row is. Everything after this point
/// still obeys the rule — picking a build in the bar moves nothing else.
///
/// Silent where it cannot help: a link naming a ruler this build of the site
/// has never heard of, or a weapon with no row under it, leaves the page as it
/// found it rather than clearing what is on screen.
function applyBenchLink(w, benchId, wantMode) {
  const sc = builtinScenarios().find((s) => s.builtin === benchId);
  if (sc) pickPreset(scenarioBarCfg(), presetId(sc));
  const rows = builtinBuilds().filter((p) => p.benchmark === benchId);
  if (!rows.length) return;
  // The mode the link asked for, else however this weapon is played — the
  // board's own row order inside a ruler is best-first, so `find` is the
  // leader either way.
  const want = wantMode && (w.modes || []).includes(wantMode) ? wantMode : null;
  const row = (want && rows.find((p) => p.mode === want)) || rows[0];
  pickPreset(buildBarCfg(), presetId(row));
}

const buildList = () => builtinBuilds().concat(loadPresetList(BUILDS));
const buildNamed = (n) => buildList().find((p) => p.name === n || p.builtin === n);
/// Is the build on screen one of the official ones? Then nothing may write it.
const officialBuildActive = () => !!(buildNamed(activePreset) || {}).builtin;

// A benchmark's display name from its id, localized. Falls back to the raw id
// rather than to nothing: a board row naming a benchmark this build of the site
// does not carry is still a row, and hiding which ruler it used would make it
// look like it had none.
const benchmarkName = (id) => {
  const b = (META.benchmarks || []).find((x) => x.id === id);
  return b ? tr(b.name) : id || "—";
};

/// The build collection's config, as a FACTORY like `scenarioBarCfg` — it used
/// to be a local inside the renderer, which meant the only way to select a
/// build was to be a bar. The router selects one too, when a board row names
/// the ruler it came from.
function buildBarCfg() {
  return {
    domain: BUILDS,
    // An imported build keeps its mods/arcane/sim scenario but belongs to
    // the weapon it lands on; restoreState prunes whatever that weapon
    // cannot equip (a different mod class, other evolution ids).
    rescope: (st, weapon) => ({ ...st, weapon }),
    label: tr("Builds"),
    noun: "build",
    // Sharing belongs to the BUILD bar: what travels is the open build, plus
    // everything needed to reproduce its number.
    extra: SHARE_ENABLED ? `<button class="pchip share">${escHtml(tr("share"))}</button>` : "",
    onExtra: (bar) => {
      const b = bar.querySelector(".share");
      if (b) b.onclick = (e) => { e.stopPropagation(); openSharePanel(bar); };
    },
    load: buildList,
    store: (ps) => storePresetList(BUILDS, ps.filter((p) => !p.builtin)),
    readonly: (p) => !!p.builtin,
    roTitle: (p) =>
      tr("a benchmark build — measured under") + " " + benchmarkName(p.benchmark),
    active: () => activePreset,
    setActive: (n) => { activePreset = n; localStorage.setItem(presetActiveKey(BUILDS), n); },
    snapshot: snapshotState,
    // Never the payload's weapon — the scope's. See restoreState.
    // The benchmark build's Forma plan is NOT applied here — `restoreState`
    // owns it, so the boot path gets it too. See the comment there.
    apply: (st) => restoreState(st, presetWeapon()),
    blank: blankBuildState,
    rerender: () => { renderPresetBar(); lockOfficialBuild(); },
  };
}

function renderPresetBar() {
  // TWO BARS, ONE CONFIG. The benchmark bar and the player's bar are handed
  // the same `cfg` — same load, same active, same apply — so a chip in either
  // one selects the same way and a ⧉ copies the same way. What differs is only
  // which entries each draws and which operations it offers.
  const buildsCfg = buildBarCfg();
  renderBenchmarkBarIn($("bench-bar-builder-builds"), { ...buildsCfg, benchLabel: tr("Benchmark builds"), benchHint: tr("submitted by players, scored here — read-only") });
  renderPresetBarIn($("preset-bar-builder-builds"), buildsCfg);
}

// A scenario is the `sim` object, BUFF CONFIG INCLUDED (user,
// 2026-08-01).
//
// Buff ids are global — `arcane:primary_deadhead`, a mod's own buff — so a
// setting travels, and `sim.buffs` deliberately keeps entries for buffs the
// build does not currently carry. That is what lets a scenario say "in THIS
// fight, Deadhead starts at zero stacks" and have it hold when the mod is
// added later, or when the marginal-gain scan tries it as a candidate.
// Anything the map does not mention takes the buff's own default, which is
// full stacks and unlocked.
/// Fields a scenario may no longer hold. A stored preset written before the
/// mode moved into the build still carries `form`, and applying it would put
/// it back on `sim` — where the next auto-save would write it out again, and
/// keep writing it forever. Dropped on the way in, so a custom scenario is
/// clean the first time it is opened and stays clean.
/// FIELDS A SCENARIO NO LONGER CARRIES, stripped in both directions so a stored
/// one — or a benchmark yaml — cannot reintroduce them.
///
/// `runs` joined them on 2026-08-13. HOW HARD YOU MEASURE IS NOT PART OF THE
/// FIGHT (owner). The official rulers still run at 1,000 — that is the number
/// their yaml states and the number the SCORER uses, and no local setting can
/// move it — while the page runs at whatever you set, defaulting to 100. Two
/// different questions that happened to share a field.
const DEAD_SCENARIO_FIELDS = ["form", "mode", "runs"];

/// HOW MANY TIMES THE PAGE REPLAYS A FIGHT. A preference, not a scenario field:
/// it survives switching fights and switching weapons, because "how hard do I
/// want to measure right now" is a fact about the person and not about the
/// engagement.
const SIM_RUNS_KEY = "wfsim-sim-runs";
const SIM_RUNS_DEFAULT = 100;
const simRuns = () => {
  const v = Math.round(Number(localStorage.getItem(SIM_RUNS_KEY)));
  return Number.isFinite(v) && v >= 1 && v <= 20000 ? v : SIM_RUNS_DEFAULT;
};
const setSimRuns = (n) => {
  const v = Math.max(1, Math.min(20000, Math.round(Number(n)) || SIM_RUNS_DEFAULT));
  localStorage.setItem(SIM_RUNS_KEY, String(v));
};

function snapshotScenario() {
  const { __weapon, ...rest } = sim;
  // Belt and braces with the strip on the way IN: a fight has no opinion about
  // how the weapon is fired, so one can never leave here carrying one either.
  DEAD_SCENARIO_FIELDS.forEach((k) => { delete rest[k]; });
  return JSON.parse(JSON.stringify(rest));
}
function applyScenario(st) {
  st = { ...(st || {}) };
  DEAD_SCENARIO_FIELDS.forEach((k) => { delete st[k]; });
  // ONTO THE DEFAULTS, NEVER ONTO THE FIGHT YOU ARE LEAVING.
  //
  // This used to spread over the live `sim`, so any field the incoming
  // scenario does not mention kept the outgoing one's value — and a benchmark
  // yaml mentions only what it has an opinion about. Tick Eximus on a copy of
  // the official ruler, switch back to the official, and the official fight was
  // now against an Eximus, because `single_target.yaml` never says `eximus:`
  // (owner, 2026-08-07). `invisible` did not leak in the same
  // test only because that yaml happens to state it.
  //
  // A scenario is therefore applied onto a COMPLETE fight — the server's
  // defaults — which makes every preset self-contained whatever it omits. It is
  // the same rule AGENTS.md already states for weapons ("the live `sim` at that
  // moment still belongs to the weapon you just left"), and the same reason: a
  // collection's state may not be written from outside it, and reading the
  // outgoing state is how it gets written from outside it.
  sim = { ...defaultScenario(), ...st, buffs: JSON.parse(JSON.stringify(st.buffs || {})) };
  // A scenario preset is stored per weapon, so its weapon-scoped field
  // (headshot %) is already right — stamp the marker so the re-seed does not
  // overwrite a saved choice with a default.
  sim.__weapon = $("weapon").value;
  renderSim();      // redraws every knob, and the bar with them
  refreshPanel();   // the Tenno half of a scenario changes what the build is worth
}
// A scenario is CONSUMED outside the simulator — the quick calc scans under
// one by name, and the optimizer states the one it will search with — so
// creating, renaming or deleting one has to reach those lists at once, the
// way a new riven reaches the mod pool (user, 2026-08-02). One hook: the bar
// calls `rerender` after every mutation, switching included.
function scenariosChanged() {
  renderScenarioBar();
  // Switching or copying a scenario changes whether the fight is EDITABLE, and
  // this hook is the only thing every mutation goes through — `renderSim` is
  // not called here, so without this line a copy of an official scenario kept
  // the original's inert controls.
  lockOfficialScenario();
  // ...and whether this fight can reach the board at all.
  renderBoardConsent();
  if ($("opt-buffs")) renderOptBuffs();
  // …and the Warframe buffs, which are the SCENARIO's: switching fights
  // switches which abilities are running, so the cards have to be repainted
  // from the incoming state rather than left showing the outgoing one's.
  if ($("sim-wfbuffs")) renderWfBuffs("sim-wfbuffs", false);
  if ($("quick-calc")) renderQuickCalc();
  if ($("opt-target")) renderOptEnemy();
}

// ---- THE OFFICIAL SCENARIOS -------------------------------------------
//
// `data/benchmarks/*.yaml`, served in META. They sit in the scenario bar
// beside the player's own and behave like presets in every way but three:
// nothing stores them (so they cannot drift from one machine to another),
// nothing edits them, and they appear on EVERY weapon — which is the whole
// point, since a number only means something against a ruler someone else can
// pick up. Wanting a variant is what ⧉ is for: it copies the official one into
// an ordinary, editable scenario of your own.
const builtinScenarios = () => (META.benchmarks || []).map((b) => ({
  // TRANSLATED for display, like everything else on the page. Its identity is
  // the `builtin` id below, never this string — see `scenarioNamed`.
  name: tr(b.name),
  // The id, not just a flag: a board row records WHICH ruler it was measured
  // under, and a retired version has to be recognisable as retired.
  builtin: b.id,
  savedAt: 0,
  state: b.scenario,
}));
// The list every reader sees: official first, then the player's own.
const scenarioList = () => builtinScenarios().concat(loadPresetList(SCENARIOS));
// By NAME or by official ID. A benchmark's display name is translated, so the
// name alone cannot be its identity: switching language would orphan the
// pointer that says which scenario is open. The id is what gets stored.
const scenarioNamed = (n) => scenarioList().find((p) => p.name === n || p.builtin === n);
const scenarioKey = (n) => (scenarioNamed(n) || {}).builtin || n;
/// Is the fight on screen the official one? Then nothing may write to it.
const officialScenarioActive = () => !!(scenarioNamed(activeScenario) || {}).builtin;

// The scenario bar's config, as a FACTORY rather than a local — two callers
// need it now: the bar itself, and the "edit a copy of this fight" button in
// the official note, which performs the identical copy. A second hand-written
// config there is how the two come to copy different things.
function scenarioBarCfg() {
  return {
    domain: SCENARIOS,
    label: tr("Scenarios"),
    noun: "scenario",
    load: scenarioList,
    // An official scenario is not stored, so it can never be written back —
    // this is the line that makes "read-only" a property of the DATA rather
    // than of the buttons drawn over it.
    store: (ps) => storePresetList(SCENARIOS, ps.filter((p) => !p.builtin)),
    readonly: (p) => !!p.builtin,
    active: () => activeScenario,
    setActive: (n) => { activeScenario = n; localStorage.setItem(presetActiveKey(SCENARIOS), scenarioKey(n)); },
    snapshot: snapshotScenario,
    apply: applyScenario,
    blank: snapshotScenario,
    rerender: scenariosChanged,
  };
}

/// "Give me an editable copy of the fight I am looking at" — the one
/// implementation, shared by the chip's ⧉ and the note's button.
const copyActiveScenario = () => copyActivePreset(scenarioBarCfg());

function renderScenarioBar() {
  const scenariosCfg = scenarioBarCfg();
  renderBenchmarkBarIn($("bench-bar-simulator-scenarios"), { ...scenariosCfg, benchLabel: tr("Benchmark scenarios"), benchHint: tr("the official rulers — the same fight on every weapon") });
  renderPresetBarIn($("preset-bar-simulator-scenarios"), scenariosCfg);
}

function fillSelect(id, items) {
  const el = $(id); el.innerHTML = "";
  for (const it of items) { const o = document.createElement("option"); o.value = it.id; o.textContent = it.name; el.appendChild(o); }
}
let currentPool = [];
const weaponInfo = (id) => META.weapons.find((w) => w.id === id) || META.weapons[0];
// Evolutions moved into each weapon's meta entry (they are per transform
// group); this reads the CURRENT weapon's tiers.
const weaponEvos = () => weaponInfo($("weapon").value).evolutions || [];
// THE TIER LADDER, in one place. Tier N is choosable only once N-1 is filled:
// a tier-2 perk with no tier 1 is not a weaker build, it is not a build. The
// builder greys the later rows out; the gain scan has to obey the same rule or
// it measures — and recommends — evolutions nobody can select (user,
// 2026-08-03).
const evoOpenTo = () => {
  let n = 0;
  for (const t of weaponEvos()) { if (!evoSel[t.tier]) break; n = t.tier; }
  return n + 1; // the deepest tier that may be chosen
};
const modById = (id) => poolWithRivens().find((m) => m.id === id);
const show = (id, on) => { const el = $(id); if (on) el.removeAttribute("hidden"); else el.setAttribute("hidden", ""); };
// Where (other than exceptIdx) this mod is currently slotted, or -1.
const placedAt = (id, exceptIdx) => slots.findIndex((s, i) => i !== exceptIdx && s.mod === id);

// Switching weapons rebuilds every weapon-scoped view. It runs as an
// APPLY (auto-save stays out of the reset/reseed churn — the optimizer
// scope resets here, and saving that would wipe the scope presets); the
// build preset then follows the new weapon via the explicit
// markPresetDirty() below, which no-ops when this runs inside a preset
// load (restoreState already holds the applying guard).
function applyWeapon(id, presetMods) {
  whileApplying(() => applyWeaponInner(id, presetMods));
  markPresetDirty();
}

// A user-driven weapon CHANGE, as opposed to applyWeapon's "rebuild the
// editor for this weapon". Presets are per weapon, so the bar must reload
// from the new weapon's own storage — initPresets() restores its active
// preset (creating "build 1" the first time). The optimizer's groups
// re-bootstrap on their own: applyWeaponInner clears optSeeded, and
// renderOpt re-runs bootstrapOptPresets against the new scope.
// NOT called from restoreState: loading a preset must not re-enter this.
function switchWeapon(id) {
  $("weapon").value = id;
  applyWeapon(id, null);
  initPresets();
}

function applyWeaponInner(id, presetMods) {
  const w = weaponInfo(id);
  buffList = []; // rebuilt from the next /api/panel response for this build
  // HOW THIS WEAPON IS PLAYED, reset to its own default. "base" is a legal id
  // on almost every weapon, so without this an Incarnon weapon opened after a
  // plain one would keep the plain one's mode and quietly skip the cycle that
  // is supposed to be its default — the same rule that makes a new weapon get
  // `defaultScenario()` rather than the last one's fight. `restoreState`
  // overwrites this immediately when a preset is being applied.
  mode = defaultMode(id, null);
  // NOTHING CROSSES BETWEEN WEAPONS: a valence is a statement about one of
  // them, so opening another starts with none.
  valence = defaultValence(id, null);
  // THE FIGHT DOES NOT MOVE. A scenario is shared across the roster now
  // (`SHARED_DOMAINS`), so switching weapons keeps the fight you are measuring
  // under — which is the entire point of being able to compare two guns.
  //
  // Its one weapon-shaped knob is headshot %, and it is left alone for the same
  // reason the official rulers leave it alone: the ruler pins 100 and applies
  // to the whole roster, and the SERVER forces 0 on a weapon that cannot
  // headshot (`parse_fight`, sentinel). Resetting it here would have rewritten
  // the shared scenario every time you changed weapon, and auto-save would have
  // stored that.
  sim.__weapon = id;
  opt = { mods: {}, exilus: {}, arcanes: {}, evos: {}, size: 8 }; optSeeded = false; // reset scope
  optLast = null;                 // and the winner the quick calc could measure against
  // ...and how it RUNS, for the same reason the scenario resets: a weapon that
  // has never been searched must not inherit the last weapon's finalists or
  // thread count into the "search 1" it is about to be given.
  optRun = { ...OPT_RUN_DEFAULTS };
  // Last weapon's ranking is not this weapon's. Nothing else clears it — the
  // tabs are CSS-hidden rather than re-rendered, so it simply stayed on screen
  // under the new weapon's name (user, 2026-08-02).
  if ($("opt-results")) $("opt-results").innerHTML = "";
  // A weapon's pool is the UNION of the pools it draws from: `primary` mods
  // fit any primary weapon, `rifle` is the class pool. One flat list per
  // weapon was right only while every rifle-class weapon was a launcher.
  rivenModCache = { key: null, list: [] };
  refreshRivenNames(); // this weapon's rivens, named by the engine
  // The SERVER decides which mods this weapon can equip (`pool_for_weapon`)
  // and sends the id list; the class tables are only where the mod objects
  // live. This used to be a JS re-implementation of the same rules, and it
  // went stale the moment the engine learned a new one — Amalgam mods are not
  // equippable on a sentinel weapon and ammo mods do nothing on an infinite
  // reserve, and neither reached the builder or the optimizer while the pool
  // was computed twice.
  const byId = new Map(
    Object.values(META.mod_pools || {}).flat().map((m) => [m.id, m]),
  );
  currentPool = (w.mods || []).map((id) => byId.get(id)).filter(Boolean);
  // NINE, not eight. The server sends 8 main slots plus the EXILUS slot's own
  // innate polarity (`innate_slots_for`, which has appended it since the
  // 2026-07-28 wiki cross-check), and this line sliced that ninth entry off
  // and padded a null over it. So every weapon with an exilus polarity — Boar
  // Prime's Madurai, the Naramon on Torid / Cernos Prime / Dual Toxocyst /
  // Laetum — showed an unpolarized exilus slot: the mod in it paid full drain
  // instead of half, and the Forma plan charged for a polarity the weapon
  // comes with (user, 2026-08-03).
  innate = (w.innate_polarities || []).slice(0, 9);
  while (innate.length < 9) innate.push(null);

  // A weapon with no asset yet must show NOTHING, not a broken-image box:
  // src="" still resolves (to the page) and renders as a failed load.
  const wimg = IMG(w.image);
  $("w-img").hidden = !wimg;
  if (wimg) $("w-img").src = wimg;
  // The weapon name links to its wiki page too (display suffixes like
  // " (sentinel)" are ours, not part of the page name).
  $("w-name").innerHTML = wl(w.name, wikiUrl(wikiWeaponName(w)));
  // Subtype (e.g. "Dual Pistols") + form tags; the mod-eligibility group
  // (mod_class) drives the picker's pool but isn't shown as a tag.
  // Name the POOL, the way the wiki does ("Rifle Mods" / "Pistol Mods"): the
  // eligibility group is what actually decides which mods equip, and a bare
  // "Mods" heading leaves the visitor guessing which pool a launcher draws
  // from. Falls back to the plain word if a class ever has no label.
  const POOL_NAME = { rifle: "Rifle Mods", pistol: "Pistol Mods", primary: "Primary Mods" };
  // Names the pools it actually draws, widest first — "Primary + Rifle Mods".
  const poolName = (w.mod_pools || [w.mod_class])
    .map((p) => (POOL_NAME[p] || p).replace(/ Mods$/, "")).join(" + ") + " Mods";
  $("mod-block-h").textContent = tr(poolName || "Mods");

  // WHAT THIS WEAPON DOES BEYOND ITS STATS. Generated engine-side from the data
  // that implements each passive, so the line and the simulation cannot say
  // different things. Empty for most weapons; the strip hides itself then.
  //
  // It lives here, in the weapon strip, because a passive is part of the
  // WEAPON — not of the build, not of the fight. Gotva Prime's crit set and
  // Dual Toxocyst's Frenzy are most of what those weapons are, and until
  // 2026-08-05 the page never mentioned either, so both read as ordinary guns.
  const wp = $("w-passives");
  if (wp) {
    const lines = w.passives || [];
    wp.hidden = !lines.length;
    wp.innerHTML = lines.map((x) => `<div>${escHtml(tr(x))}</div>`).join("");
  }
  $("w-tags").innerHTML = [w.subtype, w.uses_evo2 ? "Incarnon" : null, w.sentinel ? "Sentinel" : null]
    .filter(Boolean).map((t) => `<span class="tag">${t}</span>`).join("");

  const AX = weaponAxes(w.id);
  show("arcane-block", AX.arcanes.length > 0);
  show("evo-block", AX.evolutions.length > 0);
  // THE VALENCE AXIS exists only where the weapon has one — an adversary
  // weapon. Same "no choice, no control" rule every other axis follows.
  show("element-block", !!valenceSpec(w.id));
  // …AND THE NUMBERS FOLLOW THE BLOCKS THAT ARE ACTUALLY THERE. They were
  // written into the markup, so a weapon with no evolutions numbered its
  // Valence block 5 with no 4 above it (owner, 2026-08-13). Derived from the
  // DOM rather than from a table of weapons, so a block added later — or a
  // weapon built in the app one day — is numbered without anyone maintaining a
  // list.
  //
  // Non-numeric badges (Σ, ≡, ▶) are the ones that are not steps, and they keep
  // whatever they say.
  renumberBlocks();
  // An Arch-Gun's two slots are NOT interchangeable, so the line names the
  // pools rather than counting them: "primary + secondary", not "2 slots".
  $("arcane-sub").textContent = w.sentinel
    ? tr("sentinels cannot equip arcanes")
    : (w.arcane_pools || []).map((p) => tr(SLOT_LABEL[p] || p)).join(" + ");
  // The previous weapon's arcanes may not fit this one, slot by slot.
  arcanes = arcanesFor(w.id, arcanes);
  arcaneRanks = asArcaneList(arcaneRanks, arcanes.length).map((x) => x ?? null);

  slots = Array.from({ length: 9 }, (_, i) => ({ mod: null, pol: innate[i], rank: null }));
  (presetMods || []).filter((m) => modById(m)).slice(0, 8).forEach((m, i) => { slots[i].mod = m; slots[i].rank = modById(m).max_rank; });
  autoForma(); // sensible default: minimum-Forma polarities for the preset

  renderMods(); renderArcanes(); renderEvo(); renderMode(); renderValence(); renderSim(); renderOpt();
}

// ---- forma / capacity plan (mirrors engine::mods::plan_forma) ----
function slotDrain(base, modPol, slotPol) {
  if (!slotPol) return base;                                        // no polarity
  if (slotPol === "Omni") return modPol === "Umbra" ? base : Math.ceil(base / 2); // universal, not Umbra
  if (slotPol === modPol) return Math.ceil(base / 2);              // matched: −50% round up
  return Math.round(base * 1.25);                                  // mismatched: +25%
}

// Drain at a given rank: rises 1 per rank from rank 0 (= max-rank drain − max_rank).
function modDrain(m, rank) {
  const r = rank == null ? m.max_rank : Math.max(0, Math.min(m.max_rank, rank));
  return m.drain - m.max_rank + r;
}

// Capacity = Σ effective drain over slots holding a mod (at its rank).
function capacityUsed() {
  return slots.reduce((sum, s) => { const m = modById(s.mod); return m ? sum + slotDrain(modDrain(m, s.rank), m.polarity, s.pol) : sum; }, 0);
}

// Forma cost, broken down by TYPE (regular / Omni / Umbra cost different items).
// Innate polarities form a free-repositionable pool of REGULAR polarities, so
// regular Forma = max(added-beyond-pool, removed-from-pool): same-polarity
// repositioning nets 0, but BLANKING an innate polarity (removal) costs a Forma,
// and a colour swap costs one (add+remove of one slot). Omni/Umbra are never
// innate here — each such slot is one Omni/Umbra Forma.
function formaCount() {
  const need = {}, pool = {};
  let umbra = 0, omni = 0;
  slots.forEach((s) => {
    if (!s.pol) return;
    if (s.pol === "Omni") omni++;
    else if (s.pol === "Umbra") umbra++;
    else need[s.pol] = (need[s.pol] || 0) + 1;
  });
  innate.forEach((p) => { if (p && p !== "Omni" && p !== "Umbra") pool[p] = (pool[p] || 0) + 1; });
  let added = 0, removed = 0;
  for (const p of new Set([...Object.keys(need), ...Object.keys(pool)])) {
    const d = (need[p] || 0) - (pool[p] || 0);
    if (d > 0) added += d; else removed += -d;
  }
  // ...and an ADVERSARY weapon is billed its five whatever the slots say. The
  // rank-40 ceiling costs five polarizations and the 80 capacity this build
  // was planned against assumes them, so a build using three has still spent
  // five (engine: `cost.regular += spend - cost.total()`).
  const regular = Math.max(added, removed);
  const floor = Math.max(0, formaMin($("weapon").value) - regular - umbra - omni);
  return { regular: regular + floor, umbra, omni };
}

// Auto-assign polarities for MINIMUM Forma-to-fit (mirrors engine plan_forma):
// spend the innate pool on the biggest matching mods, then Forma the biggest
// unmatched until it fits; unmatched slots left blank. Overwrites polarities.
//
// ...with a FLOOR under it on an adversary weapon (`plan_forma_spending`'s
// `at_least`). Those five polarizations are what put the weapon at rank 40,
// and rank 40 is where the 80 capacity this plan is measured against comes
// from — so a plan that fits in two and stops has spent the capacity of a
// weapon it did not build. They are bought either way; this puts them on the
// biggest mods still unmatched instead of leaving them unspent.
function autoForma() {
  const w = $("weapon").value;
  const cap = capOf(w);
  const filled = [];
  slots.forEach((s, i) => { const m = modById(s.mod); if (m) filled.push({ i, m }); });
  slots.forEach((s) => { s.pol = null; });
  const pool = innate.filter(Boolean).slice();
  const bd = ({ i, m }) => modDrain(m, slots[i].rank);
  const order = filled.slice().sort((a, b) => bd(b) - bd(a));
  const matched = new Set(), free = new Set();
  for (const { i, m } of order) { const k = pool.indexOf(m.polarity); if (k >= 0) { pool.splice(k, 1); matched.add(i); free.add(i); } }
  const drainOf = () => filled.reduce((s, x) => s + (matched.has(x.i) ? Math.ceil(bd(x) / 2) : bd(x)), 0);
  const polarize = () => { const next = order.find(({ i }) => !matched.has(i)); if (!next) return false; matched.add(next.i); return true; };
  while (drainOf() > cap) { if (!polarize()) break; }
  // The innate pool is FREE, so only the slots this plan had to BUY count
  // against the floor — the same order the engine works in, where the pool is
  // spent before `at_least` is looked at.
  while (matched.size - free.size < formaMin(w)) { if (!polarize()) break; }
  for (const { i, m } of filled) slots[i].pol = matched.has(i) ? m.polarity : null;
  // Innate polarities are never destroyed by the auto plan (blanking one
  // costs a Forma): leftovers go back onto mod-less slots — preferring
  // their original position — else sit on an unmatched modded slot
  // (+25% mismatch drain beats paying a Forma to remove them). An empty
  // build therefore reports 0 Forma.
  for (const p of pool) {
    let k = slots.findIndex((s, i) => !s.mod && !s.pol && innate[i] === p);
    if (k < 0) k = slots.findIndex((s) => !s.mod && !s.pol);
    if (k < 0) k = slots.findIndex((s) => !s.pol);
    if (k < 0) break;
    slots[k].pol = p;
  }
}

// ---- render mods ----
function polBtn(pol, i) {
  return `<button class="pol-btn" data-i="${i}" title="change polarity">${pol ? imgTag(POL(pol), "pol") : '<span class="nopol">◇</span>'}</button>`;
}
function renderMods() {
  // The quick-calc bar sits above this block and is measured against the same
  // build, so it redraws with it.
  if (typeof renderQuickCalc === "function") renderQuickCalc();
  // ...and so does the mode control: equipping a Cannonade is what takes the
  // cycle away, so the reason it is greyed changes with the slots.
  if (typeof renderMode === "function") renderMode();
  const used = capacityUsed();
  const cap = capOf($("weapon").value);
  const capEl = $("capacity");
  capEl.textContent = `${used} / ${cap}`;
  capEl.classList.toggle("over", used > cap);
  const f = formaCount();
  $("forma").textContent = [`${f.regular} Forma`, f.umbra ? `${f.umbra} Umbra` : null, f.omni ? `${f.omni} Omni` : null]
    .filter(Boolean).join(" · ");

  const box = $("mod-slots");
  box.innerHTML = "";
  for (let i = 0; i < 8; i++) box.appendChild(buildSlot(i));

  // Exilus: a REAL slot (utility mods only, drain counts) — absent on sentinels.
  // A sentinel weapon has no exilus slot, so it shows no exilus block —
  // label included. Standing a placeholder where the slot would be says
  // "something is missing here"; the truth is that nothing belongs there
  // (user, 2026-07-31).
  const hasExilus = weaponAxes().hasExilus;
  show("exilus-block", hasExilus);
  const ex = $("exilus");
  ex.innerHTML = "";
  if (hasExilus) ex.appendChild(buildSlot(EXILUS));
  refreshPanel();
}

// The current build as a request payload — shared by /api/panel and
// /api/simulate so the stats panel and the sim always agree on the loadout.
// Mods keep slot order (elements are position-sensitive).
//
// It carries the TENNO as well as the weapon, because half of what a build is
// worth is a question about the player: a mod gated `while_invisible` pays or
// does not, and Primary Bulwark is worth +500% or nothing depending on the
// frame's armor. The panel used to resolve against the NEUTRAL player while
// the sim resolved against the fight's — so the panel offered a buff card the
// sim never ran, and hid a contribution the sim was paying. One player, both
// answers (user, 2026-08-02). The scenario fields that describe the
// PLAYER rather than the fight.
const TENNO_KEYS = ["aiming", "invisible", "airborne", "overshields", "channeling", "solo_weapon", "frame", "wf_armor", "wf_energy", "wf_sprint", "extra_stats"];

// THE FIGHT'S OWN STAT BONUSES: what this weapon is handed by something that is
// not its build — a squad buff, a Warframe ability, an arcane on another weapon.
//
// (owner, 2026-08-13). So they are not buffs: no trigger, no clock, no stack
// count. They join the same ADDITIVE buckets the mods feed, which is what
// makes them cheap to be right about — a scenario's +60% multishot and Split
// Chamber's +90% sum, exactly as
// two multishot mods would, and every lock still wins over them.
//
// NO ELEMENTS. An elemental mod is position-sensitive and enters a hierarchy,
// so "+90% Heat" is not a number, it is a place in an ordering.
const EXTRA_STAT_KEYS = [
  ["base_damage", "Base Damage"],
  ["multishot", "Multishot"],
  ["crit_chance", "Critical Chance"],
  ["crit_damage", "Critical Damage"],
  ["status_chance", "Status Chance"],
  ["status_damage", "Status Damage"],
  ["fire_rate", "Fire Rate"],
  ["reload_speed", "Reload Speed"],
  ["magazine", "Magazine Capacity"],
];

/// THE WARFRAME ROSTER, and what picking one means: it fills armor, max energy
/// and sprint speed at once. Sprint is the one that could not be set at all
/// before — there was no field for it — so every "With Sprint Speed 1.2 or
/// Higher" perk in the roster was unreachable from this page.
///
/// The numbers stay EDITABLE after a pick, because the roster is unmodded: a
/// built frame carries Steel Fiber and Primed Flow and this one does not, and
/// "With Energy Max Over 700" is a gate no frame can open at all (the highest
/// maxed pool in the game is 300). Replacing the fields with a dropdown would
/// have made that unaskable.
const frames = () => (META && META.frames) || [];
const frameOf = (id) => frames().find((f) => f.id === id);

// The fight's player, as request fields. Its own function because three
// callers need exactly this subset and a fourth will: it is the actor, not the
// scenario, and the enemy half of the scenario has no business travelling with
// a panel request.
function tennoPayload() {
  return Object.fromEntries(TENNO_KEYS.map((k) => [k, sim[k]]));
}

function buildPayload() {
  return {
    ...tennoPayload(),
    weapon: $("weapon").value,
    evolutions: Object.values(evoSel).filter(Boolean),
    // One per pool, in the weapon's pool order — the server reads either
    // this or a bare value, so an old saved build still means what it meant.
    arcane: arcanes,
    arcane_rank: arcaneRanks,
    mods: slots.filter((s) => s.mod).map((s) => s.mod),
    // HOW IT IS PLAYED, from the BUILD. It used to ride in the scenario as
    // `form`, which let the fight decide how a weapon was fired — so the
    // official ruler silently played every Incarnon weapon through its cycle
    // and "never transmuting" could not be asked for.
    mode,
    // THE VALENCE, as two flat fields rather than an object: `base_for` reads
    // them off the request the same way it reads the deployment, and every
    // path that builds a weapon for a request goes through it.
    valence_element: valence.element,
    valence_bonus: valence.bonus,
    // A `riven:` id means nothing without the riven itself — it is the
    // visitor's item, not a pool entry, so it rides along with the request.
    rivens: rivenPayload(),
  };
}

// ---- Stats panel: merged buckets, each explained by source ----

/// WHAT THE PANEL ON SCREEN WAS BUILT FROM. Null until the first one lands.
let panelKey = null;

/// The build, as a value. Same idea as `simKey`: a key DERIVED from the state,
/// never a hand-listed set of things that ought to trigger a refresh.
const buildKey = () =>
  JSON.stringify([$("weapon").value, slots, arcanes, arcaneRanks, evoSel, rivenPayload()]);

/// THE PANEL REFRESHES BECAUSE THE BUILD CHANGED, not because a control
/// remembered to say so.
///
/// Every mutation used to be responsible for calling `refreshPanel`, which is N
/// places to get right and one of them was wrong for as long as arcanes have
/// existed: the picker redrew its own slots and the panel — and the sim's buff
/// bar — kept showing the previous arcane until an unrelated edit happened to
/// refresh them (reported 2026-08-05). Fixing that one site would have left the
/// same trap for the next control someone adds.
///
/// So the trigger is derived instead. After any interaction anywhere, if the
/// build no longer matches what the panel was built from, the panel is rebuilt.
/// A control cannot forget, because it is never asked: this is the same rule
/// `check_gain_freshness` already asserts for the gain scan — the cache key is
/// DERIVED from the thing it describes, never a copy of it maintained by hand.
///
/// The explicit calls that remain are not redundant belt-and-braces; they make
/// the refresh IMMEDIATE rather than waiting for the next event. This is what
/// makes it CORRECT.
function panelWatchdog() {
  // After the handler that caused it, not during — a click that mutates state
  // runs its own listener first.
  setTimeout(() => {
    if (panelKey !== null && buildKey() !== panelKey) refreshPanel();
  }, 0);
}
for (const ev of ["click", "change", "input", "keyup"]) {
  document.addEventListener(ev, panelWatchdog, true);
}

let panelTimer = null;
function refreshPanel() {
  markPresetDirty(); // every build change funnels through here
  clearTimeout(panelTimer);
  panelTimer = setTimeout(async () => {
    const body = buildPayload();
    // Recorded where the payload is actually BUILT, so the key always
    // describes what was sent — recording it at call time would go stale
    // inside the debounce window.
    panelKey = buildKey();
    try {
      const r = await api("/api/panel", body);
      renderPanel(r);
    } catch (e) {
      $("stats-rows").innerHTML = `<div class="error">panel failed: ${e}</div>`;
    }
  }, 120);
}

function renderPanel(r) {
  if (!r || r.ok === false) {
    $("stats-rows").innerHTML = `<div class="error">${r ? r.error : "no data"}</div>`;
    return;
  }
  $("stats-sub").textContent = `max-rank values · ${r.policy}`;
  // A PASSIVE WE DO NOT MODEL makes every number below a FLOOR, and the reader
  // has no way to know that from the numbers themselves. Gotva Prime is the
  // first: its "15% chance to set the next hit's crit chance to 300%" is absent
  // (2026-08-05), so it scores like a weapon without a passive and looks simply
  // weaker rather than partly unmodelled.
  const wInfo = weaponInfo($("weapon").value) || {};
  const passiveNote = $("stats-passive");
  if (passiveNote) {
    // WHAT THIS WEAPON'S ENTRY DOES NOT MODEL, said in words, above the numbers
    // that omit it (owner, 2026-08-08). Two sources, one banner: · the
    // PASSIVE flag, for a weapon whose prose passive has no rule yet; ·
    // `unmodeled`, the weapon file's own list — the bulk Incarnon intake
    // writes one line here per base attack part it could not carry, and a
    //     bow's uncharged shot or the Angstrum's explosion is exactly the kind
    //     of gap that makes a complete-looking number wrong.
    // A yaml comment is honest to whoever opens the file and invisible to
    // everyone else, which is not the same as honest.
    const gaps = (wInfo.unmodeled || []).slice();
    if (wInfo.passive_unmodeled) {
      gaps.unshift(tr("this weapon's passive is not modelled yet"));
    }
    passiveNote.hidden = !gaps.length;
    passiveNote.innerHTML = gaps.length
      ? `<div class="unmod-h">◈ ${escHtml(tr("not modelled on this weapon — the numbers below are a floor, not its full output"))}</div>`
        // THROUGH `tr`, like the enemy card's gaps. These are OUR sentences,
        // not DE's, so they translate rather than being transcribed — and a
        // line with no entry falls through to the English it was written in,
        // which is the overlay's whole contract. Rendering them raw left a
        // Chinese page with the one paragraph that matters in English
        // (2026-08-08).
        + gaps.map((g) => `<div class="unmod-l">${escHtml(tr(g))}</div>`).join("")
      : "";
  }
  // A source the row's LOCK is ignoring still lists, struck through and said
  // out loud: "Fire Rate cannot be modified" means this mod's bonus is not in
  // the number above, and a line that looks like every other line claims the
  // opposite (owner, 2026-08-08).
  const srcLine = (s) =>
    `<div class="ssrc${s.ignored ? " sdead" : ""}"${s.ignored ? ` title="${escHtml(tr("ignored — this stat is locked at the weapon's default"))}"` : ""}>${s.value} — ${s.mod}${s.note ? ` <span class="snote">(${s.note})</span>` : ""}${s.ignored ? ` <span class="snote">(${escHtml(tr("ignored"))})</span>` : ""}</div>`;

  // THE MULTIPLICATIVE BUCKET, DRAWN (community request, 2026-08-05: the app
  // does the hard arithmetic and then shows only its answer, so the mechanics
  // stay as murky as they were).
  //
  // Warframe damage is a product of buckets: bonuses inside one bucket ADD,
  // buckets MULTIPLY. Which bucket a mod lands in is the single most useful
  // thing to know about it — adding to a bucket already at +200% is worth far
  // less than opening a new one — and the panel already had every number
  // needed to show it, arranged so you could only infer it.
  //
  // So draw the expression: `40.0 × (1 + 1.65 + 0.60) = 130`. Everything inside
  // one bracket is one bucket, and that shape needs no sentence to explain it.
  // It is the reader's OWN build's arithmetic, which is the thing a wiki cannot
  // give them.
  //
  // ONLY WHEN EVERY TERM IS A FRACTION. Evolution sources send no `frac`
  // because several are flat additions rather than percentages, and a line that
  // rendered them as `+ 0.5` would be asserting arithmetic the engine did not
  // do. Fewer than two terms is left alone as well — `40 × (1 + 1.65)` teaches
  // nothing that `40 → 106` did not.
  const bucketLine = (row) => {
    const src = row.sources || [];
    // A LOCKED row has no arithmetic to draw: its bucket was emptied, so
    // `3.3 × (1 - 0.20) = 3.3` would be a false equation printed in the one
    // place that exists to show the real one.
    if (row.locked_by) return "";
    if (src.length < 2 || !src.every((x) => typeof x.frac === "number")) return "";
    const base = String(row.base || "").replace(/[^0-9.\-]/g, "");
    if (!base || row.base === "—") return "";
    const terms = src
      .map((x) => `<span class="bterm" title="${escHtml(x.mod)}">${x.frac.toFixed(2)}</span>`)
      .join(" + ");
    return `<div class="sbucket">${escHtml(base)} × ( 1 + ${terms} ) = <b>${escHtml(row.final)}</b>` +
      ` <span class="bhint" title="${escHtml(
        tr("everything inside the bracket is ONE multiplicative bucket: these add together, and the bucket multiplies against the others"),
      )}">?</span></div>`;
  };

  const rowHtml = (row) => `
    <div class="srow">
      <div class="shead"><span class="sk">${tr(row.label)}</span>
        <span class="sv">${row.base !== "—" && row.base !== row.final ? `<span class="sbase">${row.base}</span> → ` : ""}<b>${row.final}</b></span></div>
      ${row.note ? `<div class="srownote">⚙ ${row.note}</div>` : ""}
      ${row.locked_by ? `<div class="srownote">🔒 ${escHtml(tr("locked at the weapon's default by"))} ${escHtml(row.locked_by)}</div>` : ""}
      ${bucketLine(row)}
      ${(row.sources || []).map(srcLine).join("")}
    </div>`;
  const dmgHtml = (p) => (p.damage && p.damage.length)
    ? `<div class="sdmg-title">${tr("Damage (combined)")} — ${p.damage_total} ${tr("total")}</div>` +
      p.damage.map((d) => `<div class="sdmg"><span class="sk">${DT(d.type)}</span><span class="sv"><b>${d.amount}</b> <span class="snote">${d.share}</span></span></div>`).join("")
    : "";
  // A weapon is the GUN plus the PROJECTILE(s) it launches: the gun block
  // carries cadence and capacity, each projectile block its own damage,
  // crit and status — and a radial its blast geometry too. An Incarnon
  // Laetum shot is two instances, so it renders two projectile blocks.
  const partHtml = (p) => `
    <div class="fpart" data-part="${p.id}">
      <div class="fparth">${tr(p.label)}<span class="fmeta">${tr(p.meta)}</span></div>
      ${(p.stats || []).map(rowHtml).join("")}
      ${dmgHtml(p)}
    </div>`;
  // EVERY available form renders as its own section (base + Incarnon side
  // by side — no switching), headed by the form name + trigger mechanics.
  // Indirect stats (recoil, accuracy, ammo…) render like any bucket — they
  // are outside theoretical DPS but real in practice, so the panel states them.
  const section = (f) => `
    <div class="fsec">
      <div class="fhead">${tr(f.label)}<span class="fmeta">${f.meta}</span></div>
      ${[...(f.stats || []), ...(f.elements || []), ...(f.indirect || [])].map(rowHtml).join("")}
      ${(f.parts || []).map(partHtml).join("")}
    </div>`;
  $("stats-rows").innerHTML = (r.forms || []).map(section).join("");
  $("stats-damage").innerHTML = "";

  $("stats-conditionals").innerHTML = (r.conditionals && r.conditionals.length)
    ? `<div class="sdmg-title">Conditional / not merged</div>` +
      r.conditionals.map((c) => `<div class="scond ${c.active ? "" : "off"}"><b>${c.mod}</b>: ${c.desc} <span class="snote">${c.why}</span></div>`).join("")
    : "";
  // The build's configurable buffs (weapon-scoped) drive the Sim section 2.
  buffList = r.buffs || [];
  renderSimBuffs();
}

// Wiki page for an item, from its display name (the data files' source
// urls follow the same Name_With_Underscores convention).
const wikiUrl = (name) => "https://wiki.warframe.com/w/" + encodeURIComponent(name.replace(/ /g, "_"));
// EVERY item name (mod / arcane / evolution) opens its wiki page in a new
// tab; the click never triggers the card's own handlers.
// ALWAYS pass an explicit url built from the ENGLISH name (x.name_en ||
// x.name) — `text` is the displayed (possibly localized) name, and the
// wiki only has English page names. The no-url fallback exists for plain
// English literals only.
const wl = (text, url) => `<a class="wl" href="${url || wikiUrl(text)}" target="_blank" rel="noopener" onclick="event.stopPropagation()">${text}</a>`;

// Description lines at a rank: the verbatim in-game text with the
// rank-varying numbers filled server-side (mods and arcanes alike). Null
// when the pool has no yaml description (hardcoded rifle pool) — callers
// fall back to the model's effect lines.
const descAt = (o, r) => o.desc_ranks
  ? o.desc_ranks[Math.max(0, Math.min(o.desc_ranks.length - 1, r))].split("\n")
  : null;

// The SAME card, in DE's own words, when the active locale has them
// (data/i18n/<locale>/descriptions.yaml — mods and arcanes, one entry per
// rank). Preferred over translating our English line, because a card is not
// a bag of terms: "(x2 for Bows)" is "（弓类武器效果加倍）", which no phrase
// table reaches. Ids never collide across the two tables (engine test).
const officialDesc = (o, r) => {
  if (!I18N || !o || !o.id) return null;
  const t = (I18N.mod_descriptions || {})[o.id] || (I18N.arcane_descriptions || {})[o.id];
  return t && t.length ? t[Math.max(0, Math.min(t.length - 1, r))].split("\n") : null;
};

// Card lines at a rank, ALREADY in the display language: DE's sentence when
// there is one, otherwise our English line with the phrase table applied.
// Every caller renders these verbatim — nothing runs `tf` over a line that
// was already written in the target language.
// An EVOLUTION's card lines. Same rule as a mod's, with one difference that
// matters: when the locale has no transcription, the English falls through
// UNTOUCHED. Running the phrase table over prose is what produced
// "Increase Base 伤害 by +60." — half-swapped is worse than either language,
// and evolutions are the only card written as prose rather than as terms.
// `o.effects` (our own model statement) IS term-shaped, so it still gets tf.
const evoLines = (o) => {
  const zh = I18N && (I18N.evolution_descriptions || {})[o.id];
  if (zh) return zh.split("\n");
  if (o.desc && o.desc.length) return o.desc;      // English, as written
  return (o.effects || []).map(tf);
};

// WHAT WE DO NOT MODEL, appended to whatever the card already says.
//
// Not a replacement: DE's own text is exactly what makes a player expect the
// thing to work, so hiding our gap behind it is the worst of both. `cardLines`
// prefers `officialDesc`, which every one of these has — so the "not modeled"
// line the model already produced was suppressed on precisely the items that
// needed it (reported 2026-08-05: Primary Debilitate "doesn't work". It does
// not: 5 arcanes and 12 mods carry an effect the sim knowingly skips, and none
// of them said so anywhere a player looks).
// TWO DIFFERENT ADMISSIONS, and saying "not modelled" for both is what made the
// whole app look unfinished (2026-08-05). One is a todo; the other is the edge
// of what a single-target damage simulator IS.
/// AN EVOLUTION'S TWO ADMISSIONS, as chips. The mods have said these
/// separately since 2026-08-05 and the evolutions said "not modelled yet" for
/// both — over perks that are not waiting on anyone. A player deciding what to
/// equip needs to know which: a todo may be gone next week, an EDGE is what a
/// single-target damage simulator is.
const evoGapChips = (o, tag) => {
  const todo = o.unmodeled || [];
  const edge = o.out_of_scope || [];
  const out = [];
  if (todo.length) {
    out.push(`<${tag} class="exchip unmod" title="${escHtml(
      (o.fully_unmodeled && !edge.length
        ? tr("this perk does nothing in the simulation — the model has no rule for it yet")
        : tr("part of this perk is not modelled — what it does here is less than the card says")
      ) + ": " + todo.join(", "))}">${
      escHtml(o.fully_unmodeled && !edge.length ? tr("not modelled yet") : tr("partly modelled"))}</${tag}>`);
  }
  if (edge.length) {
    out.push(`<${tag} class="exchip scope" title="${escHtml(
      tr("this cannot pay out in a one-target fight — it is an edge of the model, not a gap in it")
      + ": " + edge.map((x) => tr(x)).join(" · "))}">${escHtml(tr("nothing to earn here"))}</${tag}>`);
  }
  return out.length ? " " + out.join(" ") : "";
};

const notModeledLines = (o) => {
  const out = [];
  if (o.not_modeled) {
    out.push(`<span class="unmodeled" title="${escHtml(
      tr("real damage the simulator does not compute yet"),
    )}">⊘ ${escHtml(tr("not modelled yet"))}</span>`);
  }
  // PARTLY modelled: the mod works, and one named effect on it does not. A
  // third line rather than a third flag, because "not modelled yet" over a card
  // that lands 1,000 damage a blast is as wrong as saying nothing.
  const partial = o.unmodeled_effects || [];
  if (partial.length && !o.not_modeled) {
    out.push(`<span class="unmodeled part" title="${escHtml(
      // Each line TRANSLATED, not the label only. An arcane's are short derived
      // tokens with no zh entry and `tr` hands those straight back, so this
      // costs them nothing — but an ABILITY writes whole sentences here, and
      // one of them went out raw the first time (the same way the disclosure
      // banner's paragraph did).
      tr("everything else on this card is modelled; this is not:") + " " +
        partial.map((x) => tr(x)).join(", "),
    )}">⊘ ${escHtml(tr("partly modelled"))}</span>`);
  }
  if (o.out_of_scope) {
    out.push(`<span class="unmodeled oos" title="${escHtml(
      tr("this acts on something a weapon-damage simulator has none of — Warframe energy, enemy behaviour, movement — so it would change no number here"),
    )}">◇ ${escHtml(tr("outside the sim"))}</span>`);
  }
  // A FOURTH ADMISSION, and the only one that is not a shortfall: this IS
  // modelled, it matches the live game, and DE did not mean it to work this way
  // (owner, 2026-08-08). The other three say the number is lower than the
  // card; this one says the number is right today and a hotfix takes it away,
  // which is a different thing for a player to know before building around
  // it.
  for (const why of o.live_bugs || []) {
    out.push(`<span class="livebug" title="${escHtml(
      tr("this matches the live game and is a bug — DE may patch it, and the number here changes when they do") + ": " + tr(why),
    )}">⚑ ${escHtml(tr("unintended, modelled as it plays"))}</span>`);
  }
  return out;
};

const cardLines = (o, r, fallback) =>
  (officialDesc(o, r) || (descAt(o, r) || fallback || o.effects || []).map(tf))
    .concat(notModeledLines(o));

// One slot card (regular or exilus) with its polarity / rank / menu wiring.
function buildSlot(i) {
  const s = slots[i];
  const el = document.createElement("div");
  const m = s.mod ? modById(s.mod) : null;
  // THE SLOT'S NUMBER, because the picker already speaks it and the grid did
  // not answer. A placed mod's chip in the picker reads "slot 5", and the eight
  // slots are drawn two to a row — so nothing on screen said whether 5 was the
  // third row's left cell or the first column's fifth (player report via the
  // owner, 2026-08-10).
  //
  // It is not decoration: the same mod is worth something different in another
  // slot, because ELEMENTS COMBINE IN SLOT ORDER. A player rearranging for
  // Corrosive instead of Radiation is reading exactly this number.
  //
  // The exilus slot is left unnumbered: there is one of it, its block is
  // labelled, and the picker calls it "exilus" rather than a number.
  if (m) {
    el.className = "slot filled" + (m.rarity ? " rar-" + m.rarity : "");
    const r = s.rank == null ? m.max_rank : s.rank;
    const base = modDrain(m, r);
    const eff = slotDrain(base, m.polarity, s.pol);
    const lowered = r < m.max_rank;
    const rank = m.max_rank > 0
      ? `<span class="rank ${lowered ? "lowered" : ""}"><button class="rk" data-d="-1">−</button><b>R${r}${lowered ? "/" + m.max_rank : ""}</b><button class="rk" data-d="1">+</button></span>`
      : "";
    // The configured mod shows its CURRENT description (values at the
    // slot's rank), exactly like the in-game card.
    const desc = m.desc_ranks || officialDesc(m, r) ? cardLines(m, r) : null;
    el.innerHTML = polBtn(s.pol, i) + imgTag(IMG(m.image), "mod") +
      `<div class="info"><div class="mn">${wl(m.name, wikiUrl(m.name_en || m.name))}</div>${desc ? `<div class="me">${desc.map((x) => `<div>${x}</div>`).join("")}</div>` : ""}<div class="dr">${eff} drain${eff !== base ? ` (base ${base})` : ""}</div>${rank}</div>` +
      `<button class="dots" title="options">⋯</button>`;
    el.querySelector(".dots").addEventListener("click", (e) => { e.stopPropagation(); openSlotMenu(i, e.currentTarget); });
    el.querySelectorAll(".rk").forEach((b) => b.addEventListener("click", (e) => {
      e.stopPropagation();
      const nr = Math.max(0, Math.min(m.max_rank, r + Number(b.dataset.d)));
      slots[i].rank = nr; renderMods();
    }));
  } else {
    el.className = "slot empty";
    el.innerHTML = polBtn(s.pol, i) + `<span class="plus">${i === EXILUS ? "+ add exilus mod" : "+ add mod"}</span>`;
    // the WHOLE empty slot opens the picker (the pol-btn stops propagation)
    el.addEventListener("click", (e) => { e.stopPropagation(); openPicker(i, el); });
  }
  if (i !== EXILUS) {
    const no = document.createElement("span");
    no.className = "slotno";
    no.textContent = String(i + 1);
    // The same words the picker's chip uses, so the two are findable as one
    // thing rather than as a number and a coincidence.
    no.title = tr("slot") + " " + (i + 1);
    el.appendChild(no);
  }
  // polarity is decoupled: clickable on every slot (mod or empty, incl. innate)
  el.querySelector(".pol-btn").addEventListener("click", (e) => { e.stopPropagation(); openPolMenu(i); });
  return el;
}

// The DOM node for slot i (popover anchoring).
const slotEl = (i) => i === EXILUS ? $("exilus").firstElementChild : $("mod-slots").children[i];

// ---- popovers ----
// EVERY popover, found by class rather than by a list that a new one has to
// remember to join — the enemy picker opened and would not close, because it
// was not on the list.
//
// `keep` is the node the new panel is anchored to: a popover that CONTAINS it
// stays open, which is what lets a dropdown live inside a picker (the mod
// picker's Sort control is inside `#mod-popover`) without the act of opening
// it closing the thing it belongs to.
function closePopovers(keep) {
  document.querySelectorAll(".popover").forEach((p) => {
    if (keep && p.contains(keep)) return;
    p.hidden = true;
  });
}

// ---- THE dropdown -------------------------------------------------------
//
// ONE choose-one control for the whole site (owner, 2026-08-06). It was seven
// native `<select>`s and one rich picker, so the quick calc's scenario — a
// list that GROWS, since a scenario is a preset you make — was the plainest
// control on the page while a mod list two blocks away searched and sorted.
//
// It is not a new component: the panel is `.popover`, the search bar is
// `.addbar`, the rows are `.combo-menu .opt` — the same three the pickers have
// always used, which is the whole point. A dropdown cannot drift from the
// pickers because it IS them.
//
// The SEARCH BAR appears on its own rule rather than always: a list of two is
// read at a glance, and a search box over it is furniture. Six is where
// scanning stops being instant, so that is the line; `search: true` forces it
// for a list that will grow past it later.
/// The trigger. Emits a button that LOOKS like the select it replaces, and
/// registers what opening it should show. Callers re-render by innerHTML, so
/// registration happens on every draw rather than once.
// EVERYTHING THIS COMPONENT IMPLEMENTS, named once.
//
// A native `<select>` gave `option.disabled` away for free, and when every
// dropdown moved onto this component `ddRender` simply ignored the field —
// `ddButton` kept passing it, the Form control kept greying its ⊘ options, and
// they stayed clickable for as long as nobody looked. An extra key cost the
// author nothing, which is what made the loss silent.
//
// So an unknown key is an ERROR, not a no-op: it means the author expected a
// behaviour this component does not have. Loud here is cheap — the roster sweep
// loads every weapon on every tab in both languages, and the check scripts do
// the rest — while silent here costs a shipped feature nobody can see missing.
const DD_CFG_KEYS = ["value", "items", "dataK", "title", "placeholder", "onPick", "search"];
const DD_ITEM_KEYS = ["value", "label", "hint", "disabled", "group"];

function ddCheck(id, cfg) {
  const stray = (obj, known) => Object.keys(obj).filter((k) => !known.includes(k));
  const bad = stray(cfg, DD_CFG_KEYS);
  if (bad.length) throw new Error(`dd "${id}": unknown config ${bad.join(", ")}`);
  (cfg.items || []).forEach((i, n) => {
    const b = stray(i, DD_ITEM_KEYS);
    if (b.length) throw new Error(`dd "${id}" item ${n}: unknown field ${b.join(", ")} — the component does not implement it`);
  });
}

function ddButton(id, cfg) {
  ddCheck(id, cfg);
  ddReg.set(id, cfg);
  const cur = cfg.items.find((i) => String(i.value) === String(cfg.value));
  // `value=` is not decoration: `HTMLButtonElement.value` REFLECTS it, so the
  // scenario panel's generic `[data-k]` binding — which reads `el.value` and
  // listens for `change` — keeps working unchanged across the swap. That is
  // also what keeps `el.disabled = true` meaningful on the optimizer tab,
  // where the whole fight is drawn read-only.
  return `<button type="button" class="dd" id="${id}" data-dd="${id}" value="${
    escHtml(String(cfg.value ?? ""))}"${cfg.dataK ? ` data-k="${escHtml(cfg.dataK)}"` : ""}${
    cfg.title ? ` title="${escHtml(cfg.title)}"` : ""}><span class="dd-v">${
    escHtml(cur ? cur.label : (cfg.placeholder || "—"))}</span><span class="dd-c">▾</span></button>`;
}

function ddRender(id, query) {
  const cfg = ddReg.get(id);
  if (!cfg) return;
  const q = (query || "").trim().toLowerCase();
  // SEARCH SPANS EVERYTHING THE ROW SHOWS — its label, its hint and its group.
  // A list of official builds is `#3 · Incarnon cycle` under a ruler whose name
  // carries the enemy, the level and the metric, so "no aim", "thrax" and
  // "cycle" all have to find rows or the search only works for people who
  // already know where things are.
  const hits = cfg.items.filter((i) => {
    if (!q) return true;
    const blob = [i.label, i.hint, i.group].filter(Boolean).join(" ").toLowerCase();
    // …and space-insensitively, for the same reason the mod list is: a
    // localized label carries spaces the player does not type.
    return blob.includes(q) || squash(blob).includes(squash(q));
  });
  // A DISABLED item stays LISTED and greyed: "the weapon has no Incarnon form
  // while that mod is on it" is information, and a vanished option is not. It
  // is `.dis`, and `.dis` is what the click binding below skips — the one
  // native `<select>` behaviour this component has to reproduce by hand, and
  // the one it silently dropped when the selects were replaced.
  //
  // IT KEEPS ITS `data-v`. Dropping the value was the first way this was
  // written, and it left a greyed row identifiable only by the words on it —
  // which is the thing that broke `check_opt_gain` the day an evolution got a
  // Chinese name. An option carries its identity whether or not it can be
  // clicked; being clickable is a separate fact and lives in the class.
  // GROUPED, where the data has a grouping. A flat list is right up to about a
  // screenful; the official builds are rulers x modes x ten and the rulers are
  // meant to reach dozens, and past that a reader cannot SCAN even a list they
  // can search. The header is emitted when the group CHANGES, so grouping costs
  // nothing when no item carries one, and it survives filtering — a search that
  // leaves two rulers standing still says which is which.
  let group = null;
  $("dd-menu").innerHTML = hits.length
    ? hits.map((i) => {
      const head = (i.group && i.group !== group)
        ? `<div class="ddgroup">${escHtml(i.group)}</div>` : "";
      group = i.group || group;
      return head + `<div class="opt${String(i.value) === String(cfg.value) ? " cur" : ""}${
        i.disabled ? " dis" : ""}" data-v="${escHtml(String(i.value))}">
        <div class="info"><div class="mn">${escHtml(i.label)}</div>${
          i.hint ? `<div class="me"><div>${escHtml(i.hint)}</div></div>` : ""}</div>
      </div>`;
    }).join("")
    : `<div class="opt dis">${escHtml(tr("no matches"))}</div>`;
  $("dd-menu").querySelectorAll(".opt[data-v]:not(.dis)").forEach((el) => {
    el.onclick = (e) => {
      e.stopPropagation();
      $("dd-popover").hidden = true;   // only THIS panel — a parent picker stays
      // The TRIGGER THAT OPENED THIS, not `$(id)`: the scenario's fields are
      // drawn twice — once by the simulator, once read-only by the optimizer —
      // so an id resolves to whichever copy is earlier in the document, which
      // is not necessarily the one being used. The anchor is never ambiguous.
      const btn = $("dd-popover")._anchor;
      if (btn) btn.value = el.dataset.v;
      // A `data-k` dropdown belongs to the scenario's generic binding, so it
      // announces itself the way that binding expects rather than carrying a
      // second, private path to the same state.
      if (cfg.dataK && btn) btn.dispatchEvent(new Event("change", { bubbles: true }));
      if (cfg.onPick) cfg.onPick(el.dataset.v);
    };
  });
}

function ddOpen(id, anchor) {
  const cfg = ddReg.get(id);
  if (!cfg) return;
  closePopovers(anchor);
  const pop = $("dd-popover");
  pop._anchor = anchor;
  place(pop, anchor);
  // Match the trigger's width where it is wider than the panel's default, so
  // the panel reads as belonging to the control rather than floating near it.
  pop.style.minWidth = `${Math.max(anchor.getBoundingClientRect().width, 180)}px`;
  const wantSearch = cfg.search === true || cfg.items.length >= DD_SEARCH_MIN;
  $("dd-addbar").hidden = !wantSearch;
  const s = $("dd-search");
  s.value = "";
  s.oninput = () => ddRender(id, s.value);
  ddRender(id, "");
  if (wantSearch) s.focus();
}

// Delegated, because every caller re-renders its trigger by innerHTML — a
// listener bound to the node would be thrown away with it, and rebinding after
// each draw is the kind of thing that gets forgotten on the eighth dropdown.
// CAPTURE, not bubble: several containers call `stopPropagation` on click to
// survive their own innerHTML redraws (`#quick-calc` and `.picker-tools` both
// do), and a bubbling listener would never see a trigger inside one of them.
document.addEventListener("click", (e) => {
  const t = e.target.closest("[data-dd]");
  if (!t || t.disabled) return;
  e.stopPropagation();
  if (!$("dd-popover").hidden && $("dd-popover")._anchor === t) {
    $("dd-popover").hidden = true;    // clicking the open trigger closes it
    return;
  }
  ddOpen(t.id, t);
}, true);
function place(pop, anchor) {
  const r = anchor.getBoundingClientRect();
  pop.hidden = false;
  pop.style.top = (window.scrollY + r.bottom + 4) + "px";
  pop.style.left = (window.scrollX + r.left) + "px";
}

function openPicker(slotIdx, anchor) {
  closePopovers();
  pickerSlot = slotIdx;
  const pop = $("mod-popover");
  place(pop, anchor);
  const search = $("mod-search");
  search.value = "";
  search.oninput = () => renderMenu(slotIdx, search.value);
  renderTools();
  renderMenu(slotIdx, "");
  // Sorted by EFFECT by default (user, 2026-08-01) — which means computing it.
  ensureGains({ kind: "mods", idx: slotIdx },
    () => { if (!$("mod-popover").hidden) renderMenu(pickerSlot, $("mod-search").value); });
  search.focus();
}

function renderTools() {
  const t = $("picker-tools");
  const pols = ["Madurai", "Naramon", "Vazarin", "Umbra"].filter((p) => currentPool.some((m) => m.polarity === p));
  t.innerHTML =
    `<label>${escHtml(tr("Sort"))} ` + ddButton("pk-sort", {
      value: pickerPrefs.sort,
      items: [{ value: "name", label: tr("Name") }, { value: "drain", label: tr("Drain") }]
        .concat(gainPrefs.on === false ? [] : [{ value: "gain", label: tr("Gain") }]),
      onPick: (v) => { pickerPrefs.sort = v; savePickerPrefs(); renderTools(); renderMenu(pickerSlot, $("mod-search").value); },
    }) + `</label>` +
    `<button id="pk-dir" class="ghost-btn small" title="direction">${pickerPrefs.dir === "asc" ? "▲" : "▼"}</button>` +
    `<span class="pk-pols"><span class="pk-pol ${!pickerPrefs.pol ? "sel" : ""}" data-p="">all</span>` +
    pols.map((p) => `<span class="pk-pol ${pickerPrefs.pol === p ? "sel" : ""}" data-p="${p}" title="${p}">${imgTag(POL(p), "pol")}</span>`).join("") +
    `</span>`;
  // redraw() re-renders these tools via innerHTML, which DETACHES the clicked
  // node; without stopPropagation the click would bubble to the document
  // outside-click handler, whose closest(".popover") now fails on the detached
  // target → the picker would wrongly close. Keep every tool click inside.
  const redraw = () => { savePickerPrefs(); renderTools(); renderMenu(pickerSlot, $("mod-search").value); };
  $("pk-dir").onclick = (e) => { e.stopPropagation(); pickerPrefs.dir = pickerPrefs.dir === "asc" ? "desc" : "asc"; redraw(); };
  t.querySelectorAll(".pk-pol").forEach((o) => o.onclick = (e) => { e.stopPropagation(); pickerPrefs.pol = o.dataset.p || null; redraw(); });
}

// ---- MARGINAL GAIN: what is THIS SLOT worth as something else? ------------
//
// The question is asked OF A SLOT, which is the one the picker is already
// open on: "if slot N became this mod, what happens to the kill rate?"
// (user, 2026-08-01). That framing is not a convenience — it is the correct
// one, for two reasons:
//
//   · ELEMENT ORDER. Elements combine by MOD ORDER (MECHANICS §3, Load
//     Order), and the payload's mod list is slot-ordered — so the same mod in
//     slot 2 and in slot 6 can produce different elements. A scan that picked
//     a position for you would be answering a question you did not ask.
//   · A FULL BUILD is the case worth analysing, and "replace slot N" is
//     defined there. An empty slot is just the degenerate case of it.
//
// Answered by SIMULATING each candidate — the builder has no second damage
// model and must not grow one. The metric is KILL PROGRESS (the optimizer's
// own), so a percentage here is a percentage of KPM; it falls back to DPS when
// the baseline cannot kill at all and the ratio would be undefined.
//
// AGAINST A SAVED SCENARIO, chosen by name, at a chosen precision: its own run
// count or a tenth of it. PAIRED randomness is what makes the tenth usable —
// baseline and every candidate take the same seed, so they walk the same
// random stream and the difference between them is the mod, not the dice.
/// ONE MEASUREMENT, and how far it can be from the truth: `{ v, se }`.
///
/// THE MEAN, NOT THE MEDIAN RUN. `score`/`dps` are the median engagement — the
/// right headline for "what a fight looks like", and one run however many were
/// paid for. Ranking wants the average of all of them: measured on this fight,
/// the median moved 9.8% between seeds at 10 runs where the mean moved 5.9%,
/// so reading the mean is a 1.6x narrower answer for no extra simulation. It is
/// also the statistic the OPTIMIZER ranks (`mean_kill_progress`), so the quick
/// calc now previews the number the search will act on.
///
/// `se` is the server's own spread over those runs (sigma / sqrt(runs)). The
/// scans used to estimate it by running the reference a SECOND time at another
/// seed and taking the gap — one sample of a distribution, which on identical
/// inputs answered anywhere from 0.7% to 11.2%. That single draw decided
/// whether EVERY chip was suppressed to "about nothing" or none of them was,
/// which is both of the things the quick calc was reported for (2026-08-12).
const readGain = (r, useKills) => {
  if (!r || !r.ok) return null;
  return useKills
    ? { v: r.score_mean ?? r.score ?? r.kills ?? 0, se: r.score_se ?? 0 }
    : { v: r.dps_mean ?? r.dps ?? 0, se: r.dps_se ?? 0 };
};

/// What `cand` is worth against `ref`, with the uncertainty of the COMPARISON.
///
/// Two independent means, so their relative errors add in quadrature — and the
/// result is scaled by the ratio itself, because a +200% gain carries its error
/// on a number three times the size of the reference.
const gainOver = (cand, ref) => {
  const ratio = cand.v / ref.v;
  return {
    pct: ratio - 1,
    se: ratio * Math.hypot(cand.se / cand.v || 0, ref.se / ref.v || 0),
  };
};

// ONE seed for the whole scan: the reference and every candidate are measured
// under the same luck, so a candidate that does not perturb the fight compares
// against the reference exactly (see `gainBand`).
//
// There is no second seed. A scan's resolution is a property of THIS scope in
// THIS fight — a status mod perturbs it and a damage mod barely does — so it
// has to be measured rather than assumed; but the runs already paid for measure
// it, and one extra run at another seed only draws a single sample of it.
const GAIN_SEED = 0x5EED;
// TEN, and it is both the floor and the default. Below it a status mod's chip
// is a coin flip — M24: one run swings a status mod +-39 points — so a number
// under ten is not a cheaper answer, it is a wrong one.
const GAIN_RUNS_MIN = 10;
const GAIN_RUNS_MAX = 2000;
const gainRuns = () =>
  Math.max(GAIN_RUNS_MIN, Math.min(GAIN_RUNS_MAX, Math.round(Number(gainPrefs.runs)) || GAIN_RUNS_MIN));
// A gain READS with its sign — "12.3%" and "+12.3%" are different claims.
const gainPct = (x) => (x >= 0 ? "+" : "−") + sig2(Math.abs(x) * 100) + "%";

// Quick calc's TWO settings: which saved scenario, and how many runs.
//
// Runs came back (owner, 2026-08-11). The old reasoning — "the algorithm
// answers it better than a person can" — was written when the alternative was
// a MODE selector choosing between two passes at two precisions. A plain count
// is not that: it is the one knob whose right value nobody but the reader
// knows, because it depends on how close the answers turn out to be and on how
// long you are willing to wait for them. Ten is the floor and the default,
// which is where a status mod's answer stops being a coin flip (M24); above it
// you are buying resolution the scan cannot invent.
//
// What a run is MEASURED BY still belongs to the scenario. That has not moved.
//
// There is no "current" scenario either: a scan is only worth reading against
// something that has a name and can be returned to.
// ON by default (user, 2026-08-01): the ranking is the reason the picker is
// worth opening. Off, nothing simulates, no chip is drawn, and is not
// offered as an order — a sort key with no values behind it is a trap.
let gainPrefs = { on: true, scenario: null, runs: GAIN_RUNS_MIN };
try { const s = JSON.parse(localStorage.getItem("wfsim-gain")); if (s) gainPrefs = { ...gainPrefs, ...s }; } catch (_) {}
const saveGainPrefs = () => localStorage.setItem("wfsim-gain", JSON.stringify(gainPrefs));

let gainScan = { key: null, running: false, base: 0, floor: 0, by: {}, done: 0, total: 0, note: "", metric: "" };

/// The scenario a scan runs under: the chosen preset, else the live one.
function gainScenario() {
  const ps = scenarioList();
  // BY ID, not by label. The active pointer stores an official ruler's ID
  // (`single_target_no_aim`) while its NAME is a translated sentence, so a
  // name comparison silently fell through to `ps[0]` — the quick calc then
  // ranked every slot under the FIRST ruler no matter which fight was on
  // screen.
  const p = ps.find((x) => presetId(x) === gainPrefs.scenario)
    || ps.find((x) => presetId(x) === activeScenario) || ps[0];
  // THE ACTIVE FIGHT IS THE ONE ON SCREEN; ANY OTHER IS A STORED DOCUMENT.
  //
  // The picker can aim this scan at a scenario you are not in, and a benchmark
  // yaml names only what it has an opinion about — so spreading one over the
  // live `sim` handed it every field it does not mention. Pick "single_target"
  // here while your own fight has Roar running and the quick calc ranked every
  // slot under the ruler's enemy AND your Roar. A ruler is the same fight for
  // everyone or it is not a ruler. Same rule `applyScenario` already states,
  // and the same failure the Eximus box had (owner, 2026-08-07).
  //
  // The ACTIVE one still reads BOTH — `sim` for the knob you just turned, which
  // has to reach this scan before the auto-save round-trips, and its stored
  // state over the top, which is what a write straight into the preset means.
  // That pair is what `check_gain_freshness` is about, and it is untouched: the
  // change here is only that a scenario you are NOT in stops inheriting the one
  // you are.
  const st = p && presetId(p) === activeScenario
    ? { ...sim, ...p.state }
    : { ...defaultScenario(), ...(p ? p.state : {}) };
  // ONE PASS, at the reader's own count (user, 2026-08-02 for the single pass;
  // owner, 2026-08-11 for the count). It was one run over the field and then
  // the leaders again — two numbers with two precisions, and the cheap one
  // printed a minus sign in front of mods worth +40% (M24: a status mod swings
  // ±39 points on a single run). Ten is where that stops being a coin flip; it
  // is not where it stops moving, which is why every tooltip still says how
  // many runs its number came from.
  const runs = gainRuns();
  const refine = 0;
  // The WHOLE buff map travels, not just the current build's cards: a
  // candidate's buff is by definition not in `buffList`, and the scenario may
  // well have an opinion about it. Unmentioned buffs take their own default
  // (full stacks, unlocked), which is the honest reading of "no opinion".
  return { name: p ? p.name : "—", refine,
    scenario: { ...st, runs, seed: GAIN_SEED, buffs: st.buffs || {} } };
}

// A scan belongs to ONE AXIS POSITION of one build under one scenario.
let gainAxis = { kind: "mods", idx: 0 };
// The key is the AXIS, the BUILD and the FIGHT THIS SCAN WILL ACTUALLY RUN —
// `gainScenario()`'s own output, not a hand-listed copy of some of `sim`.
//
// It used to name the scenario fields one by one, and the list had drifted:
// `buffs` was missing, so raising a buff's starting stacks changed what the
// scan would measure without changing the key, and the old ranking stayed on
// screen looking current (user, 2026-08-03). `metric` was missing too. Any
// hand-maintained list of "the fields that matter" grows a hole the moment a
// field is added; deriving the key from the payload cannot.
const gainKey = () => JSON.stringify([gainAxis, buildPayload(), gainPrefs.on,
  gainScenario().scenario]);

const famOf = (id) => (modById(id) || {}).family || null;
const modsCompatible = (ids) => {
  const fams = ids.map(famOf).filter(Boolean);
  return new Set(fams).size === fams.length;
};

/// Every candidate for an axis position, as `{ id, payload }` — the payload
/// being what to OVERRIDE on `buildPayload()` to try it.
///
/// The three axes differ only here. A mod replaces one slot, an arcane one
/// pool, an evolution one tier — and evolutions are scanned across EVERY tier
/// at once, because they are all on screen at once and there are a dozen of
/// them, not seventy (user, 2026-08-01: arcanes and evolutions use this too).
function gainCandidates(axis) {
  if (axis.kind === "arcane") {
    const cur = arcanes.slice();
    return arcanePool(axis.idx)
      .filter((a) => a.id !== cur[axis.idx])
      .map((a) => { const next = cur.slice(); next[axis.idx] = a.id; return { id: a.id, payload: { arcane: next } }; });
  }
  if (axis.kind === "evo") {
    // Every tier at once, because they are all on screen at once — but only
    // the tiers the LADDER opens. A locked tier's options used to be scanned
    // too, so the picker offered (and ranked) an evolution the builder will
    // not let you click, measured on a build that cannot exist.
    //
    // Within a tier this is exactly "current vs replacement": the base run is
    // the build as it stands, and each candidate swaps ONE tier's choice and
    // leaves the rest alone.
    const openTo = evoOpenTo();
    const equipped = slots.map((s) => s.mod).filter(Boolean);
    const out = [];
    weaponEvos().filter((tier) => tier.tier <= openTo).forEach((tier) => {
      tier.options.forEach((o) => {
        if (evoSel[tier.tier] === o.id) return;
        const next = { ...evoSel, [tier.tier]: o.id };
        // An evolution that would take an EQUIPPED mod off the weapon is not a
        // one-step swap — it is that swap plus an eviction, and scoring it
        // against this build would price a build the game refuses. It is still
        // choosable; it just has no gain to report until the mod comes out.
        const no = forbiddenByEvos(next);
        if (equipped.some((m) => no.has(m))) return;
        out.push({ id: o.id, payload: { evolutions: Object.values(next).filter(Boolean) } });
      });
    });
    return out;
  }
  if (axis.kind === "valence") {
    // THE SEVEN PROGENITOR ELEMENTS, scanned the way a tier of evolutions is —
    // all on screen at once, one swap each, everything else left alone (owner,
    // 2026-08-13).
    //
    // It is the axis a scan is worth the most on: the choice is a whole element
    // entering the hierarchy, so which one wins depends on the mods around it
    // and on the target — a question nobody can answer by reading cards.
    const s = valenceSpec($("weapon").value);
    if (!s) return [];
    return s.elements
      .filter((e) => e !== valence.element)
      .map((e) => ({ id: e, payload: { valence_element: e, valence_bonus: valence.bonus } }));
  }
  const cur = slots.map((s) => s.mod);
  // `buildPool()`, not the weapon's: a scan that ranks a mod this build's
  // evolutions forbid recommends something the picker will not offer.
  return buildPool()
    .filter((m) => !cur.includes(m.id))
    .filter((m) => axis.idx !== EXILUS || m.exilus)
    .map((m) => { const next = cur.slice(); next[axis.idx] = m.id; return { id: m.id, payload: { mods: next.filter(Boolean) } }; })
    .filter((c) => modsCompatible(c.payload.mods));
}

// How many of the leaders AUTO looks at twice. Small on purpose: the second
// pass exists to settle an ORDER, and an order is decided at the top.
const GAIN_REFINE_TOP = 12;

// ---- the quick calc runs WIDE ------------------------------------------
//
// WHY it needed to. A scan is one simulate per candidate, and a mod slot has
// ~80 candidates once family conflicts are dropped, each measured at `runs`
// = 10 — so ~810 full engagements. Every one of them went down the SINGLE
// `rpcWorker`, one after another, while the optimizer next door has run a
// FLEET since 2026-08-03. That is why it felt like a search rather than a
// calculation (user, 2026-08-03).
//
// It also explains why a nearly-full build is so much worse than a bare one:
// the candidate COUNT barely moves, but each engagement does. Seven mods means
// multishot, status and elements all live, so one fight generates several times
// the procs, DoT stacks and ticks a one-mod fight does — the per-sim cost is a
// function of how much is HAPPENING, and a good build makes a lot happen.
//
// The lever is PARALLELISM, and only that: the run count is 10 by decision
// (2026-08-02 — below it a status mod's answer flips sign, M24) and the
// engagement DURATION is never the lever (user, 2026-08-03). So the work is
// spread over lanes instead of being made smaller or shorter.
//
// A lane is "somewhere a simulate can run": a dedicated worker in the wasm
// build, and plain `api` on the native server, where the fetch already
// parallelises and there is no worker to own.
let gainPool = null;
function gainLanes() {
  if (gainPool) return gainPool;
  // Same rule as the optimizer's auto: every core but one, capped at 8. No
  // setting of its own — the quick calc is not a search and has no preset to
  // put one in; it takes what the machine has.
  const n = Math.max(1, Math.min((Number(navigator.hardwareConcurrency) || 4) - 1, 8));
  if (!WASM) {
    gainPool = Array.from({ length: n }, () => ({ call: (p, b) => api(p, b) }));
    return gainPool;
  }
  gainPool = Array.from({ length: n }, () => {
    const w = new Worker("/worker.js");
    const pending = new Map();
    let seq = 0;
    w.onmessage = (e) => {
      const r = pending.get(e.data.id);
      if (r) { pending.delete(e.data.id); r(e.data.payload); }
    };
    return { call: (path, body) => new Promise((res) => {
      const id = ++seq;
      pending.set(id, res);
      w.postMessage({ id, kind: "api", path, body: body ?? {} });
    }) };
  });
  return gainPool;
}

// The generation of the scan that is allowed to write `gainScan`. Bumped on
// every start, so an older scan discovers at its next await that it has been
// superseded and stands down.
//
// WHY it has to be interruptible: a scan is ~90 SERIAL simulate calls, and the
// old guard was "a scan is running, so ignore this". Editing the fight midway
// therefore did not queue and did not cancel — it was DROPPED, the whole stale
// scan ran to the end under the config you had just left, and only then did
// the refresh notice the key had moved and start again. Every rapid edit paid
// for a full measurement of a question nobody was asking any more (user,
// 2026-08-03). Cancelling at the next await bounds that to one sim.
let gainGen = 0;
// An axis whose scan was dropped because another axis was mid-flight. One slot:
// the newest asker wins, and it is consumed when the running scan finishes.
let gainPending = null;

async function scanGains(axis, onTick) {
  const gen = ++gainGen;
  const live = () => gen === gainGen;
  gainAxis = axis;
  const { name, scenario, refine } = gainScenario();
  // The note is WHICH FIGHT this was measured in. The run counts used to ride
  // along here too and were dropped: each chip's tooltip states its own count,
  // which is the only place the number changes how a reading should be taken.
  gainScan = { key: gainKey(), axis, running: true, base: 0, floor: 0, by: {}, done: 0, total: 0,
    note: name, metric: "" };
  // Kill progress is the optimizer's metric and the one a player is actually
  // buying; DPS is the fallback for a target this build cannot kill at all,
  // where the ratio has no denominator. The SCENARIO decides which.
  let useKills = (scenario.metric || "kpm") !== "dps";
  // `procs` rides along so a candidate can be asked whether it moved the fight
  // or only the numbers — see `belowFloor`.
  let baseProcs = null;
  const run = async (override) => {
    const r = await api("/api/simulate", { ...buildPayload(), ...fightPayload(scenario), ...override });
    if (!r || !r.ok) return null;
    if (!override.seed && baseProcs === null) baseProcs = r.procs ?? null;
    return readGain(r, useKills);
  };
  let base = await run({});
  if (!live()) return;
  if (!base?.v && useKills) { useKills = false; base = await run({}); if (!live()) return; }
  if (!base?.v) { gainScan.running = false; if (onTick) onTick(gainScan); return; }
  gainScan.base = base.v;
  gainScan.metric = useKills ? tr("kill rate") : tr("DPS");
  // ...and how far this same build moves on luck alone — the RESOLUTION the
  // server measured across the runs it was already paid for, not a second run
  // at another seed. See `readGain`.
  gainScan.floor = base.se / base.v;
  const cands = gainCandidates(axis);
  gainScan.total = cands.length + (refine ? Math.min(GAIN_REFINE_TOP, cands.length) + 1 : 0);
  // One shared cursor, every lane pulling the next candidate as it frees up —
  // so a slow candidate delays itself and nothing else. `live()` is checked
  // after each await, which is what bounds an interrupted scan to one
  // outstanding sim per lane rather than the whole queue.
  let cursor = 0;
  await Promise.all(gainLanes().map(async (lane) => {
    for (;;) {
      if (!live()) return;
      const c = cands[cursor++];
      if (!c) return;
      const r = await lane.call("/api/simulate", { ...buildPayload(), ...fightPayload(scenario), ...c.payload });
      if (!live()) return;               // the fight moved — this answer is stale
      const g = readGain(r, useKills);
      gainScan.done++;
      if (g) {
        gainScan.by[c.id] = { ...gainOver(g, base), runs: scenario.runs,
          diverged: baseProcs === null || (r.procs ?? null) !== baseProcs };
      }
      if (onTick) onTick(gainScan);
    }
  }));
  if (!live()) return;
  // SECOND PASS. One run ranks the field cheaply but cannot separate its top
  // few — so the leaders are asked again with more, against a baseline
  // measured the same way. Everything below them keeps its first answer,
  // which is all a position near the bottom needs to be right about.
  if (refine) {
    const deep = { ...scenario, runs: refine };
    const runDeep = async (override) => {
      const r = await api("/api/simulate", { ...buildPayload(), ...fightPayload(deep), ...override });
      if (!r || !r.ok) return null;
      return readGain(r, useKills);
    };
    const deepBase = await runDeep({});
    if (!live()) return;
    gainScan.done++;
    if (onTick) onTick(gainScan);
    if (deepBase?.v) {
      const top = cands
        .filter((c) => gainScan.by[c.id])
        .sort((a, b) => gainScan.by[b.id].pct - gainScan.by[a.id].pct)
        .slice(0, GAIN_REFINE_TOP);
      for (const c of top) {
        const v = await runDeep(c.payload);
        if (!live()) return;
        gainScan.done++;
        if (v) {
          // The DEEPER pass keeps the shallow one's `diverged`: it is a fact
          // about the two builds, not about how hard they were measured.
          gainScan.by[c.id] = { ...gainOver(v, deepBase), runs: refine,
            diverged: gainScan.by[c.id]?.diverged !== false };
        }
        if (onTick) onTick(gainScan);
      }
    }
  }
  gainScan.running = false;
  if (onTick) onTick(gainScan);
}

/// The gain chip: what this option is worth, once scanned. Shared by all
/// three lists — a mod, an arcane and an evolution are the same question
/// asked of different axes, so they say it the same way.
/// HOW WIDE this gain's answer is, as a fraction — 0 when the comparison is
/// exact.
///
/// The streams are split (`rng::Draws`), so a candidate that changes only
/// damage numbers does not re-roll the fight at all: its comparison against the
/// reference is EXACT, and a +3% from it is a fact however small. A candidate
/// that changes how many STATUSES land does re-roll — the status and buff
/// streams diverge — and its comparison carries the spread of a different
/// fight. Proc count tells the two apart for free, because it is already in the
/// response.
///
/// So a band is printed only where the fight actually moved, and a small exact
/// gain is still printed as the fact it is.
const gainBand = (g) => (g.diverged ? g.se || 0 : 0);

/// The chip. A gain the scan cannot resolve STATES ITS WIDTH rather than
/// collapsing to "about nothing" (owner, 2026-08-12). "≈0%" was one string for
/// two different findings — a mod that does nothing, and a mod nobody measured
/// hard enough — and the difference between them is the only thing a reader
/// can act on: the first says pick something else, the second says raise the
/// runs. A band says which: `+0.1%` is
/// worthless, `≈+3.1% ±7.2%` is unmeasured.
const gainChip = (g, why) => {
  const band = gainBand(g);
  if (!band) {
    // MEASURED AND MOVED NOTHING, which is a FINDING and not a number. A third
    // of this weapon's pool lands here against one standing target — ammo and
    // magazine mods (nothing runs dry), Firestorm (no distance), punch-through
    // (one target), recoil and zoom (nobody is aiming by hand), Cautious Shot
    // (nobody shoots back), a Bane of the wrong faction. Printing "+0.00%" 38
    // times says the scan is broken; saying it has no effect HERE points at the
    // row's own disclosure line, which states which of those reasons it is.
    if (g.pct === 0) {
      return `<span class="gainchip flat" title="${escHtml(
        `${tr("measured, and it moved nothing in this fight — see what this option's own line says it does not cover")} · ${why}`
      )}">${tr("no effect here")}</span>`;
    }
    // Paired exactly — the fight did not re-roll, so this is not an estimate.
    return `<span class="gainchip ${g.pct >= 0 ? "up" : "down"}" title="${escHtml(
      `${tr("the fight did not re-roll for this option — same statuses landed, so this comparison is exact")} · ${why}`
    )}">${gainPct(g.pct)}</span>`;
  }
  const cls = Math.abs(g.pct) < band ? "flat" : (g.pct >= 0 ? "up" : "down");
  return `<span class="gainchip ${cls}" title="${escHtml(
    `${tr("this option re-rolls the fight, so its answer is only good to ±{x} — raise the run count to narrow it")
      .replace("{x}", sig2(band * 100) + "%")} · ${why}`
  )}">≈${gainPct(g.pct)} ±${sig2(band * 100)}%</span>`;
};

const gainChipFor = (id, where) => {
  const g = gainOf(id);
  // A HALF-FILLED RANKING HAS TO LOOK LIKE ONE.
  //
  // An option with no answer YET rendered exactly like one that finished with
  // nothing to say — no chip at all — while the list re-sorts on every result
  // that lands (an unranked option sorts last, so whatever arrives first sits
  // at the top). Read mid-scan that is a ranking which keeps changing its mind,
  // and the only way to find out it was not final was to click away and back
  // (report, 2026-08-13).
  //
  // The scan already counts itself; it just said so nowhere near the list being
  // read. `gainOf` has checked the key, so this only marks rows on the axis
  // actually being measured.
  if (!g) {
    return gainScan.running && gainScan.key === gainKey()
      ? `<span class="gainchip pend" title="${escHtml(
          tr("still measuring — {a} of {b} options ranked so far, and the order moves until it finishes")
            .replace("{a}", gainScan.done).replace("{b}", gainScan.total))}">…</span>`
      : "";
  }
  // A ONE-RUN number is a SCREEN, not a measurement, and it has to read like
  // one. Measured across three seeds on a status mod, a single run lands
  // anywhere in a ±39-point band — wide enough to print a minus sign in front
  // of a mod worth +40% (user, 2026-08-02: "why does adding status chance
  // LOWER the damage?"). A damage mod barely moves, because paired seeds
  // cancel almost everything about it; a status mod's payoff is decided by
  // which procs land, which is the one thing the dice still choose.
  //
  // So the screen says "about", and only the second pass — the leaders, run
  // again with a tenth of the scenario's count — prints a bare number.
  // EVERY number here is an average of a few runs, so every number reads the
  // same way: approximate, and said to be. A status mod's payoff is decided by
  // which procs land, and ten runs narrows that without settling it.
  const why = tr("averaged over {n} runs — this number moves between scans, most of all for status mods")
    .replace("{n}", g.runs);
  return gainChip(g, `${where} · ${gainScan.metric} · ${gainScan.note} · ${why}`);
};

// ---- the OPTIMIZER's quick calc ----------------------------------------
//
// SAME QUESTION, NO SLOTS. The builder asks "what if this mod went in THIS
// slot"; the optimizer has no slots, so the reference is the REQUIRED set and
// every mod is measured with-against-without it:
//
//     gain(X) = best(reference ∪ {X}) / best(reference ∖ {X}) − 1
//
// One formula, both directions. A pooled mod's numerator carries it (what it
// would add); a REQUIRED mod's denominator drops it (what it is contributing).
//
// `best` is a MAXIMUM OVER PAIRINGS, not a value — with three distinct
// elements a mod set is three builds, and on the Burston Prime the best is
// 3.3x the worst (2.074 against 0.627 kills/min, measured). Canonicalising
// instead would have frozen whichever pairing the insertion order produced:
// `builds::canonical_mods` normalises only the freedoms that are provably free
// and never moves the PARTITION. Taking the max is also the optimizer's own
// rule — it searches this dimension — so a chip cannot rank a mod under a
// build the search would never return.
let optGain = { key: null, running: false, base: 0, by: {}, orders: [], mode: "require",
  done: 0, total: 0, note: "", metric: "", ref: [] };
let optGainGen = 0;

/// The reference build's EVOLUTIONS: what the scope pins, tier by tier, and
/// the ladder stops at the first tier it does not. A tier carrying exactly one
/// option counts as pinned — a scope with one choice has made it.
function optRefEvos() {
  const out = [];
  for (const t of weaponEvos()) {
    const m = opt.evos[t.tier] || {};
    const ids = Object.keys(m);
    const pick = evoPinned(t.tier) || (ids.length === 1 ? ids[0] : null);
    if (!pick) break;
    out.push(pick);
  }
  return out;
}

/// The reference build's MODS. Either the required set, or — once a search has
/// produced one — the winner, which is the same question asked on a build that
/// is actually full. The required set is usually two or three cards, and a mod
/// measured there meets no diminishing returns at all, so flat base damage
/// reads high and everything conditional reads low. Which one is in use is on
/// screen, never inferred.
function optRefMods() {
  if (optGain.mode === "winner") {
    const w = optWinnerMods();
    if (w && w.length) return w.slice();
  }
  return Object.keys(opt.mods).filter((id) => opt.mods[id] === "fixed");
}

/// The winner of the ranking on screen, if there is one.
function optWinnerMods() {
  const r = (typeof optLast !== "undefined" && optLast && optLast.results) || [];
  return r.length && r[0].mods ? r[0].mods.slice() : null;
}

const optGainKey = () => JSON.stringify([$("weapon").value, opt.mods, optRefEvos(),
  optGain.mode, optWinnerMods(), gainScenario().scenario]);

/// The reference build's ARCANES: what the scope pins in each pool, and
/// nothing where it pins nothing — "no arcane" is a real state, not a gap.
function optRefArcanes() {
  return weaponAxes().arcanes.map((ax) => arcanePinnedIn(ax.pool) || "none");
}

/// Every option the optimizer can show a chip for — mods, ARCANES and
/// EVOLUTIONS alike, because all three are marked the same way and the
/// question asked of them is the same one (user, 2026-08-06: "mod/arcane/evo").
///
/// Each candidate is the ONE set that differs from the reference: a required
/// option drops itself, everything else adds itself. Only mods carry an
/// element, so only they can move the pairing — but an EVOLUTION can too,
/// indirectly, since tier 1 installs the form whose innate element the whole
/// partition then includes. Arcanes cannot, so they reuse the reference's
/// orders and cost one engagement each.
function optGainCandidates(ref, refEvos, refArc) {
  const inRef = new Set(ref);
  const out = poolWithRivens()
    .filter((m) => !famReqBy(m))
    .map((m) => ({
      id: m.id,
      kind: "mod",
      // Family exclusivity applies to the SET BEING MEASURED, not only to the
      // scope: adding a mod whose family is already in the reference would
      // price a build the arsenal refuses.
      mods: inRef.has(m.id)
        ? ref.filter((x) => x !== m.id)
        : ref.filter((x) => !m.family || (modById(x) || {}).family !== m.family).concat([m.id]),
      evolutions: refEvos,
      drops: inRef.has(m.id),
    }));

  // ARCANES. One seat per pool, so this is a REPLACEMENT rather than an
  // addition — the same shape the builder's arcane axis has. An arcane already
  // pinned is measured by emptying its seat, which is what "what is it
  // contributing" means when the alternative is nothing.
  weaponAxes().arcanes.forEach((ax, i) => {
    (ax.options || []).forEach((a) => {
      const drops = refArc[i] === a.id;
      const next = refArc.slice();
      next[i] = drops ? "none" : a.id;
      out.push({ id: a.id, kind: "arcane", mods: ref, evolutions: refEvos, drops,
        override: { arcane: next } });
    });
  });

  // EVOLUTIONS. The LADDER decides which tiers are askable: a tier is open
  // only once the one below it is filled, so the candidate set is the
  // reference's prefix with this tier's option substituted and everything
  // above it dropped. Asking about a locked tier would price a build the
  // builder will not let you assemble (the rule `check_gain_axes` asserts of
  // the builder's own scan).
  weaponEvos().forEach((t) => {
    if (t.tier > refEvos.length + 1) return;
    t.options.forEach((o) => {
      const drops = refEvos[t.tier - 1] === o.id;
      const next = drops
        ? refEvos.slice(0, t.tier - 1)
        : refEvos.slice(0, t.tier - 1).concat([o.id]);
      out.push({ id: o.id, kind: "evo", mods: ref, evolutions: next, drops,
        override: { evolutions: next } });
    });
  });
  return out;
}

async function scanOptGains(onTick) {
  const gen = ++optGainGen;
  const live = () => gen === optGainGen;
  const { name, scenario } = gainScenario();
  const ref = optRefMods();
  const evolutions = optRefEvos();
  const refArc = optRefArcanes();
  optGain = { ...optGain, key: optGainKey(), running: true, base: 0, floor: 0, by: {}, orders: [],
    done: 0, total: 0, note: name, metric: "", ref };
  const cands = optGainCandidates(ref, evolutions, refArc);

  // ONE call for every set the scan will measure, the reference first. The
  // browser is never taught to pair elements: that would be a second copy of
  // `elements::combine`'s innate rules, and it would be wrong the first time a
  // weapon carried an innate element — the Burston's Incarnon form carries
  // Heat, so Cold + Toxin is already Viral + Heat with no Heat mod equipped.
  const base = { ...tennoPayload(), weapon: $("weapon").value, evolutions,
    arcane: refArc, rivens: rivenPayload() };
  const pr = await api("/api/pairings", { ...base, ...scenario,
    sets: [{ mods: ref, evolutions }]
      .concat(cands.map((c) => ({ mods: c.mods, evolutions: c.evolutions }))) });
  if (!live()) return;
  if (!pr || !pr.ok) { optGain.running = false; if (onTick) onTick(optGain); return; }
  const orders = pr.sets.map((x) => x.orders);

  const useKills = (scenario.metric || "kpm") !== "dps";
  const read = (r) => readGain(r, useKills)?.v ?? null;
  const seOf = (r) => readGain(r, useKills)?.se ?? 0;
  const procsOf = (r) => (!r || !r.ok ? null : (r.procs ?? null));
  // Every (set, pairing) is ONE job, flattened into one queue so a set with
  // three pairings does not hold a lane while another waits — the same shared
  // cursor the builder's scan uses, for the same reason.
  const jobs = [];
  orders.forEach((os, si) => os.forEach((o, oi) => jobs.push({
    si, oi, mods: o.mods,
    // The reference is set 0; every other set is a candidate and carries
    // whatever it changes ABOUT the build that is not a mod.
    override: si === 0 ? {} : (cands[si - 1].override || {}),
  })));
  optGain.total = jobs.length;
  const got = orders.map((os) => os.map(() => null));
  const ses = orders.map((os) => os.map(() => 0));
  const procs = orders.map((os) => os.map(() => null));
  let cursor = 0;
  await Promise.all(gainLanes().map(async (lane) => {
    for (;;) {
      if (!live()) return;
      const j = jobs[cursor++];
      if (!j) return;
      const r = await lane.call("/api/simulate", { ...base, ...scenario, mods: j.mods, ...j.override });
      if (!live()) return;
      got[j.si][j.oi] = read(r);
      ses[j.si][j.oi] = seOf(r);
      procs[j.si][j.oi] = procsOf(r);
      optGain.done++;
      if (onTick) onTick(optGain);
    }
  }));
  if (!live()) return;

  const best = (si) => {
    const vs = got[si].filter((x) => x != null);
    return vs.length ? Math.max(...vs) : null;
  };
  const b = best(0);
  optGain.base = b || 0;
  optGain.metric = useKills ? tr("kill rate") : tr("DPS");
  // The scan's own resolution, from the runs already paid for rather than one
  // more at another seed — see `readGain`.
  const bSe = b ? ses[0][got[0].indexOf(b)] : 0;
  if (b) optGain.floor = bSe / b;
  // THE PAIRING LADDER — the reference's own orders, ranked. It goes first on
  // screen because the swing between pairings is larger than any single mod's.
  optGain.orders = orders[0].map((o, i) => ({
    combined: o.combined, leftover: o.leftover, mods: o.mods, value: got[0][i],
    pct: b && got[0][i] != null ? got[0][i] / b - 1 : null,
  })).sort((x, y) => (y.value || 0) - (x.value || 0));
  if (b) {
    cands.forEach((c, i) => {
      const v = best(i + 1);
      if (v == null) return;
      // `drops` decides which side of the ratio the reference sits on, which
      // is what lets one formula answer both questions.
      const [withX, without] = c.drops ? [b, v] : [v, b];
      if (!without) return;
      const vi = got[i + 1].indexOf(v);
      const o = orders[i + 1][vi] || {};
      const refProcs = procs[0][got[0].indexOf(b)];
      const candProcs = procs[i + 1][vi];
      // `drops` already decided which side is which; the uncertainty is the
      // same either way, so it is built from the two measurements as they sit.
      const [seWith, seWithout] = c.drops ? [bSe, ses[i + 1][vi]] : [ses[i + 1][vi], bSe];
      optGain.by[c.id] = {
        ...gainOver({ v: withX, se: seWith }, { v: without, se: seWithout }),
        runs: scenario.runs,
        diverged: refProcs === null || candProcs !== refProcs,
        combined: o.combined || [], leftover: o.leftover || [], drops: c.drops };
    });
  }
  optGain.running = false;
  if (onTick) onTick(optGain);
}

/// The gain for `id` in the optimizer, or null when the scan does not cover
/// the scope on screen.
const optGainOf = (id) => (optGain.key === optGainKey() ? optGain.by[id] || null : null);

/// A pairing, as the elements a player reads: what it MAKES, then whatever is
/// left over uncombined. The leftover is dimmed because it is not a choice —
/// it is what the partition could not pair, innate elements included.
const pairingLabel = (combined, leftover) =>
  (combined || []).map((t) => `<span class="pw">${DT(t)}</span>`).join(" + ")
  + (leftover || []).map((t) => `<span class="pw dim">${DT(t)}</span>`).join(" + ")
    .replace(/^(?=.)/, (combined || []).length ? " + " : "");

/// The optimizer's gain chip. One number per row, NEVER a range — the pairing
/// is one decision shared by the whole scope and is stated once, above.
///
/// ...except where a candidate LANDS somewhere else. Adding a fourth element
/// re-pairs everything: Stormbringer on a Viral + Heat build measures −65%
/// despite reading "+90% Electricity", because the best it can reach is Blast
/// + Corrosive (0.728 against 2.074, measured). Without that label the number
/// looks like a bug — which is exactly what happened the last time a chip went
/// negative for a reason the row did not state (user, 2026-08-02: "why does
/// adding status chance LOWER the damage?").
const optGainChipFor = (id) => {
  const g = optGainOf(id);
  if (!g) return "";
  const why = tr("averaged over {n} runs — this number moves between scans, most of all for status mods")
    .replace("{n}", g.runs);
  const how = g.drops ? tr("what it contributes: this scope with it, against without")
    : tr("what it would add: this scope plus it, against the scope as it stands");
  return gainChip(g, `${how} · ${optGain.metric} · ${optGain.note} · ${why}`);
};

/// The pairing a candidate lands on, shown only when it DIFFERS from the
/// reference's — which also silences it for a second mod of an element the
/// build already has, since those pool and change no partition at all.
const optPairingNoteFor = (id) => {
  const g = optGainOf(id);
  if (!g || !optGain.orders.length) return "";
  const best = optGain.orders[0];
  const same = JSON.stringify([g.combined, g.leftover])
    === JSON.stringify([best.combined, best.leftover]);
  if (same) return "";
  return `<div class="pairnote">${g.drops ? "⇠" : "⇢"} ${pairingLabel(g.combined, g.leftover)}</div>`;
};

/// The picker's ONE ordering rule, over whatever keys an axis has.
/// Descending on every key, the chosen one first, the rest in a fixed order —
/// and an unscanned option sorts last whichever way the arrow points, because
/// an absent answer is not a small one.
function gainSort(a, b, keys) {
  const ga = gainOf(a.id), gb = gainOf(b.id);
  if (!ga !== !gb) return ga ? -1 : 1;
  // A GAIN IS RANKED BY ITS MEAN, and the band is shown beside it.
  //
  // The mean is the unbiased estimate of what an option is worth; the spread is
  // a property of the MEASUREMENT, not of the option — run it long enough and
  // the spread goes to zero while the mean stays put. Ranking on a lower bound
  // (`pct - se`) was tried and reverted for exactly that reason: it
  // systematically demotes whatever is merely hard to measure, and a status mod
  // is hard to measure by nature, so the list would have been telling players
  // something about the simulator rather than about their build (owner,
  // 2026-08-13).
  //
  // THIS DOES NOT MAKE THE ORDER STABLE, and it is not meant to. Two options
  // whose bands overlap are genuinely unranked, so which sits higher can move
  // between scans — that is the measurement talking, and the chip says so with
  // its ±. The answer to an order that moves is more runs, not a different
  // ranking rule; hiding it behind a pessimistic sort would have made a coin
  // flip look like a verdict.
  const cmp = { gain: () => (ga && gb ? gb.pct - ga.pct : 0),
                drain: () => (b.drain || 0) - (a.drain || 0),
                name: () => String(b.name).localeCompare(String(a.name)) };
  for (const k of keys) {
    const c = cmp[k]();
    if (Math.abs(c) > 1e-9) return pickerPrefs.dir === "desc" ? c : -c;
  }
  return 0;
}

/// The gain for `id`, or null when this axis position has not been scanned.
const gainOf = (id) => (gainScan.key === gainKey() ? gainScan.by[id] || null : null);

/// QUICK CALC — page level, above the mods.
///
/// It is ONE configuration for every slot's question, so it does not live
/// inside any slot's picker (user, 2026-08-01). Two settings: the SCENARIO (a
/// saved one, which also decides KPM-or-DPS) and how many runs. There is no
/// run button because there is no slot here to run against — opening a
/// picker computes with these, which is what "sorted by effect by default"
/// means in practice.
function renderQuickCalc() {
  const box = $("quick-calc");
  if (!box) return;
  const on = gainPrefs.on !== false;
  const ps = scenarioList();
  // The same identity `gainScenario` resolves with — this control and that
  // reader must agree on which fight is selected, or the label says one thing
  // and the numbers are another's.
  const cur = ps.some((p) => presetId(p) === gainPrefs.scenario) ? gainPrefs.scenario
    : (ps.some((p) => presetId(p) === activeScenario) ? activeScenario : presetId(ps[0]));
  const opt = (v, label, sel) => `<option value="${escHtml(v)}"${v === sel ? " selected" : ""}>${escHtml(label)}</option>`;
  box.innerHTML =
    `<label class="pc-h" title="${escHtml(tr("rank a slot's options by what they would change — off, nothing is simulated"))}">` +
    `<input type="checkbox" id="gp-on"${on ? " checked" : ""}> ⚡ ${escHtml(tr("Quick calc"))}</label>` +
    (!on ? "" :
    ddButton("gp-scen", {
      value: cur,
      // A SCENARIO IS A PRESET, so this list grows with use — which is why it
      // was the worst one to leave as a bare select, and why its search is
      // forced rather than left to the item count.
      search: true,
      title: tr("the saved scenario to measure under — it decides the enemy, the technique and whether the ranking is KPM or DPS"),
      items: ps.map((p) => ({ value: presetId(p), label: p.name })),
      onPick: (v) => { gainPrefs = { ...gainPrefs, scenario: v }; saveGainPrefs(); renderQuickCalc(); refreshGains(); },
    }) +

    // HOW MANY RUNS a chip's number is averaged over. Ten is the floor, not a
    // suggestion: under it a status mod's chip is a coin flip. It is clamped on
    // the way out (`gainRuns`) as well as here, because a number input accepts
    // an empty string and a paste.
    `<label class="pc-runs" title="${escHtml(tr("how many simulations each option is averaged over — 10 is the floor, and more costs proportionally more time"))}">` +
    `<input type="number" id="gp-runs" min="${GAIN_RUNS_MIN}" max="${GAIN_RUNS_MAX}" step="10" value="${gainRuns()}">` +
    `<span>${escHtml(tr("runs"))}</span></label>` +

    // PROGRESS while it runs, and an invitation before it has. The run counts
    // ("1x -> 10x") are gone from here (user, 2026-08-02): they were a property
    // of the algorithm back when there were two passes at two precisions. The
    // count above is the reader's own, and every chip still carries the one its
    // number came from.
    `<span class="pc-note">${gainScan.running
      ? `${gainScan.done}/${gainScan.total}`
      : (gainScan.note ? "" : escHtml(tr("open a slot to rank its mods by effect")))}</span>`);
  // Every click stays inside: a redraw detaches these nodes, and the document
  // outside-click handler closes on a target whose `.popover` ancestor is gone.
  box.onclick = (e) => e.stopPropagation();
  $("gp-on").onchange = (e) => {
    e.stopPropagation();
    gainPrefs = { ...gainPrefs, on: $("gp-on").checked };
    // A stale ranking must not outlive the switch, and "提升" must not stay
    // selected with nothing behind it.
    if (!gainPrefs.on) {
      gainScan = { key: null, running: false, base: 0, floor: 0, by: {}, done: 0, total: 0, note: "", metric: "" };
      if (pickerPrefs.sort === "gain") { pickerPrefs.sort = "drain"; savePickerPrefs(); }
    }
    saveGainPrefs();
    renderQuickCalc();
    if (!$("mod-popover").hidden) { renderTools(); renderMenu(pickerSlot, $("mod-search").value); }
    renderEvo(); renderMode();
  };
  // A COUNT CHANGE IS A NEW QUESTION, so it re-runs rather than waiting for
  // the next picker to open — the same contract the scenario dropdown has.
  // `change` and not `input`, because a half-typed "1" on the way to "100" is
  // not a request to re-scan at one run.
  const gr = $("gp-runs");
  if (gr) gr.onchange = (e) => {
    e.stopPropagation();
    gainPrefs = { ...gainPrefs, runs: Number(gr.value) };
    saveGainPrefs();
    gr.value = gainRuns(); // show what was actually taken
    refreshGains();
  };
  // (Picking a scenario is handled by the dropdown's own `onPick`, which does
  // the same thing it always did: save, then answer the new question NOW
  // rather than at the next time a picker happens to open.)
}

/// Re-run whatever quick-calc surface is on screen. Called after ANY scenario
/// edit: the scan is measured under the scenario, so a change to it makes the
/// numbers on screen answers to a question nobody is asking any more.
///
/// It is not a repaint — `ensureGains` compares the key first, so a change the
/// chosen scenario does not care about costs nothing here. Evolution rows scan
/// without being opened, so they always refresh; the pickers only when open.
function refreshGains() {
  if (gainPrefs.on === false) return;
  renderQuickCalc();
  if ($("mod-popover") && !$("mod-popover").hidden) {
    renderTools();
    renderMenu(pickerSlot, $("mod-search").value);
  }
  if ($("arcane-popover") && !$("arcane-popover").hidden) renderArcaneMenu($("arcane-search").value);
  renderEvo(); renderMode();
}

/// Compute this axis position's ranking, unless it is already on screen.
/// `gainKey` covers the axis, the build, the scenario and the settings, so
/// re-opening the same picker costs nothing and any edit invalidates it.
function ensureGains(axis, repaint) {
  // Nothing to measure against yet. The evolution rows scan without being
  // opened, so on a cold load they can fire before `initPresets` has seeded
  // the scenario library — and a scan with no named scenario is one nobody
  // can reproduce or compare against (it labelled itself "—").
  if (gainPrefs.on === false) return;
  if (!scenarioList().length) return;
  gainAxis = axis;                       // so the key describes what we want
  // Is what we are measuring (or have measured) the fight that is on screen
  // NOW? `gainScan.key` is stamped at scan start, so a matching key covers both
  // "already ranked" and "already ranking the right thing".
  if (gainScan.key === gainKey()) return;
  // A running scan is STALE and gives way — but only to its own axis. The mod
  // picker and the evolution rows both ask on every refresh (`refreshGains`
  // ends in `renderEvo`), so "the newest request wins" makes the two cancel
  // each other on every repaint and neither ever finishes. Which axis is asking
  // is not a staleness signal; the fight moving is.
  if (gainScan.running && JSON.stringify(gainScan.axis) !== JSON.stringify(axis)) {
    // …BUT IT IS NOT FORGOTTEN. Dropping it silently is what made a player have
    // to click between two evolutions until the numbers appeared (report,
    // 2026-08-13): the evolution rows ask on EVERY refresh while a picker asks
    // only while it is open, so with a picker open the evolution request was
    // dropped and nothing ever re-asked — the running scan's completion
    // repaints the caller that started it, which is the picker, not the rows.
    gainPending = { axis, repaint };
    return;
  }
  let last = 0;
  scanGains(axis, (st) => {
    const now = Date.now();
    if (!st.running || now - last > 250) { last = now; renderQuickCalc(); repaint(); }
    // …and now the one that gave way gets its turn. One slot, consumed once:
    // it goes back through `ensureGains`, so a request whose answer is already
    // on screen returns immediately and the two axes cannot ping-pong.
    if (!st.running && gainPending) {
      const p = gainPending;
      gainPending = null;
      ensureGains(p.axis, p.repaint);
    }
  });
}

function familyConflict(mod, exceptIdx) {
  if (!mod.family) return false;
  return slots.some((s, i) => { if (i === exceptIdx || !s.mod) return false; const o = modById(s.mod); return o && o.family === mod.family; });
}

// Rows grouped under STICKY headings, each section in its own box.
//
// A sticky element is confined to its containing block, so headings that are
// all siblings of the rows share one: the first one sticks at the top and
// never leaves, and the second slides underneath it — two headings on one
// line (user, 2026-07-31). A box per section is the whole fix: each heading
// sticks while its own rows are on screen and is pushed out by the next.
function sectionedRows(items, sectionOf, rowHtml) {
  const parts = [];
  let cur = null;
  items.forEach((m, i) => {
    const s = sectionOf(m);
    if (s !== cur) {
      if (cur !== null) parts.push("</div>");
      cur = s;
      parts.push(`<div class="menu-sect"><div class="menu-head">${escHtml(tr(s))}</div>`);
    }
    parts.push(rowHtml(m, i));
  });
  if (cur !== null) parts.push("</div>");
  return parts.join("");
}

function renderMenu(slotIdx, query) {
  rivenPickerSlot = slotIdx;
  const menu = $("mod-menu");
  const q = query.trim().toLowerCase();
  // Equipped mods stay LISTED: the current slot's mod is marked, mods in other
  // slots show their slot number — picking one of those EXCHANGES the two slots.
  // ONE group ahead of the rule: this slot's own mod, because it is the
  // baseline every number below is measured against. Everything else obeys
  // the sort — including mods sitting in OTHER slots, which used to be pinned
  // into a band of their own. That band contradicted whichever order was
  // chosen (eight rows of unsorted drain at the top of a drain sort), and it
  // was never load-bearing: every placed mod already carries a "slot N" chip
  // that says where it is (user, 2026-08-01: the sort follows one rule).
  const group = (m) => (slots[slotIdx].mod === m.id ? 0 : 1);
  const hits = buildPool()
    // The exilus slot takes what `exilusPool()` says, which is the same
    // question the optimizer's exilus scope asks.
    .filter((m) => slotIdx !== EXILUS || m.exilus)
    .filter((m) => !pickerPrefs.pol || m.polarity === pickerPrefs.pol)
    .filter((m) => searchHit(m, q))
    .sort((a, b) => {
      const g = group(a) - group(b); // current first, then equipped, then the rest
      if (g) return g;
      // RIVENS FIRST, as their own block: they are the build's own items and
      // sorting them in by name would scatter them through the pool.
      const r = (b.riven ? 1 : 0) - (a.riven ? 1 : 0);
      if (r) return r;
      // ONE RULE, not three special cases (user, 2026-08-01).
      //
      //   · every key sorts DESCENDING — effect, then drain, then name;
      //   · the key you PICK is hoisted to the front, and the rest keep that
      //     fixed order behind it. Pick drain and you get drain, effect, name.
      //
      // Drain descending is deliberate: two mods the target ignores score the
      // same, and the expensive one is doing more (it just is not doing it
      // HERE), so it is the one worth looking at first.
      //
      // The arrow flips the whole comparison, keys and all — one rule means
      // one direction. What it does not flip is UNSCANNED-LAST: an absent
      // answer is not a small one, so it stays at the bottom either way.
      return gainSort(a, b, [pickerPrefs.sort,
        ...["gain", "drain", "name"].filter((k) => k !== pickerPrefs.sort)]);
    });
  // No cap: every pool mod must be reachable. The popover menu scrolls
  // (`.combo-menu` overflow-y), so the whole sorted/filtered list is browsable.
  // Two sections, each labelled. With rivens leading, an unlabelled pool
  // below them would read as a continuation of the riven list.
  const row = (m) => {
    const isCur = slots[slotIdx].mod === m.id;
    const at = placedAt(m.id, slotIdx);
    // Exchanging with the exilus slot would move OUR mod there — only legal
    // if it is exilus-eligible (or the slot is empty).
    const ownMod = slots[slotIdx].mod ? modById(slots[slotIdx].mod) : null;
    const exIllegal = at === EXILUS && ownMod && !ownMod.exilus;
    const conflict = at < 0 && !isCur && familyConflict(m, slotIdx);
    // Every placed mod shows a "slot N" chip (the current one shows ITS OWN
    // slot). No "current" word — same color family; background does the
    // distinguishing, the current slot rendered a touch stronger.
    // LOCALIZED, and it was not until 2026-08-10: this chip is the ONLY place
    // the app names a slot, so a Chinese page read "slot 5" while everything
    // around it was translated — and the number badge on the slot itself now
    // has to agree with it word for word.
    const slotName = (idx) => idx === EXILUS ? tr("exilus") : tr("slot") + " " + (idx + 1);
    const badge = isCur ? `<span class="slotchip cur">${slotName(slotIdx)}</span>`
      : at >= 0 ? `<span class="slotchip">${slotName(at)}</span>` : "";
    // The gain is THIS SLOT's — the same mod is worth something different in
    // another slot, because elements combine by mod order.
    const gainChip = gainChipFor(m.id, slotName(slotIdx));
    const title = conflict ? `incompatible (${m.family})`
      : exIllegal ? `cannot swap: ${ownMod.name} is not an exilus mod`
      : at >= 0 ? `swap with ${at === EXILUS ? "the exilus slot" : "slot " + (at + 1)}`
      : m.effects.join(" · ");
    return `<div class="opt ${conflict || exIllegal ? "dis" : ""} ${isCur ? "cur" : at >= 0 ? "placed" : ""} ${m.rarity ? "rar-" + m.rarity : ""}" data-id="${m.id}" title="${title}">
      ${imgTag(POL(m.polarity), "pol")}${imgTag(IMG(m.image), "mod")}
      <div class="info"><div class="mn">${m.riven ? escHtml(m.name) : wl(m.name, wikiUrl(m.name_en || m.name))}${m.exilus ? ' <span class="exchip">EXILUS</span>' : ""} ${badge}${gainChip}</div><div class="me">${cardLines(m, m.max_rank).map((x) => `<div>${x}</div>`).join("")}</div></div><span class="dr">${m.drain}</span></div>`;
  };
  menu.innerHTML = hits.length
    ? sectionedRows(hits, (m) => (m.riven ? "Riven" : "Mods"), row)
    : `<div class="opt dis">${escHtml(tr("no matches"))}</div>`;
  menu.querySelectorAll(".opt:not(.dis)").forEach((o) => o.addEventListener("click", () => {
    const id = o.dataset.id;
    if (slots[slotIdx].mod === id) { closePopovers(); return; } // already here
    const at = placedAt(id, slotIdx);
    if (at >= 0) {
      // EXCHANGE the two slots' mods (+ranks); polarities stay with their slots
      const a = slots[slotIdx], b = slots[at];
      [a.mod, b.mod] = [b.mod, a.mod];
      [a.rank, b.rank] = [b.rank, a.rank];
    } else {
      slots[slotIdx].mod = id; // polarity is decoupled — keep the slot's polarity
      slots[slotIdx].rank = modById(id).max_rank; // added mods default to max rank
    }
    closePopovers(); renderMods();
  }));
}

function openSlotMenu(slotIdx, anchor) {
  closePopovers();
  // MOD ops only (polarity lives on the left icon — no duplication).
  const menu = $("slot-menu");
  menu.innerHTML = `
    <div class="mi" data-a="swap">Swap mod</div>
    <div class="mi danger" data-a="remove">Remove mod</div>`;
  place(menu, anchor);
  menu.querySelector('[data-a="swap"]').addEventListener("click", () => { openPicker(slotIdx, slotEl(slotIdx)); });
  menu.querySelector('[data-a="remove"]').addEventListener("click", () => { slots[slotIdx].mod = null; closePopovers(); renderMods(); }); // in-place; keep polarity
}

function openPolMenu(slotIdx) {
  closePopovers();
  const menu = $("slot-menu");
  const cur = slots[slotIdx].pol;
  menu.innerHTML = GUN_POLS.map((p) => `<div class="mi ${p === cur ? "sel" : ""}" data-p="${p}">${imgTag(POL(p), "pol")} ${p === "Omni" ? "Omni (any)" : p}</div>`).join("") +
    `<div class="mi ${!cur ? "sel" : ""}" data-p="">◇ none</div>`;
  place(menu, slotEl(slotIdx));
  menu.querySelectorAll(".mi").forEach((o) => o.addEventListener("click", () => {
    slots[slotIdx].pol = o.dataset.p || null;
    closePopovers(); renderMods();
  }));
}

// ---- Arcane ----
// Full parity with mods: ONE slot card (rank stepper, ⋯ menu) → click opens a
// searchable picker that matches name OR effect, with effect lines, rarity
// frames, and the equipped arcane highlighted in the accent background family.
// The arcanes the CURRENT weapon can equip: its own slot's pool. Arcane ids
// are globally unique, so lookups stay unfiltered — only the PICKERS narrow.
//
// The EQUIPPABLE arcanes of this weapon's slot. "none" is the empty-slot
// sentinel, not an arcane, and it is offered in NO list: the builder slot has
// its own "Remove arcane", and in the optimizer an empty arcane scope already
// means "run no arcane" — a "None" row there was a second way to say the same
// thing, sitting in a list of real choices (user, 2026-07-30 / 2026-07-31).
const arcanePools = (weaponId) =>
  (weaponInfo(weaponId || $("weapon").value) || {}).arcane_pools || [];
// The arcanes SLOT i may hold — pool i's, and only pool i's. A picker that
// offers the whole set and then refuses the pick is a worse way to say the
// same thing.
const arcanePool = (i = 0) => {
  const pool = arcanePools()[i];
  // ...and the weapon's CLASS, for the two arcanes typed by class rather than
  // by slot (Shotgun Vendetta, Longbow Sharpshot). An EQUIP rule: the arsenal
  // does not offer them elsewhere, so neither does the picker (owner,
  // 2026-08-05). `equip_classes` is the engine's answer, not a rule restated
  // here — empty means any weapon the slot seats.
  const cls = (weaponInfo($("weapon").value) || {}).class;
  return (META.arcanes || []).filter(
    (a) =>
      a.id !== "none" &&
      a.slot === pool &&
      (!(a.equip_classes || []).length || a.equip_classes.includes(cls)),
  );
};
// An arcane belongs to ONE slot, so another slot's arcane is not a
// questionable choice on this weapon — it cannot be equipped at all. Ids reach
// the page from saved states, presets, shared URLs and optimizer results, so
// every one of them goes through here instead of being trusted (a SECONDARY
// arcane rode a saved state onto the first primary weapon — user, 2026-07-30).
// The engine refuses the same thing independently
// (`arcanes_data::for_slot`); this keeps the UI from ever showing a build the
// sim would not run.
// Pre-data short names, from builds saved before arcane ids were data. They
// are rewritten HERE, at the one point an id enters state, so the wire format
// stays a single shape and nothing downstream reads two spellings (user,
// 2026-08-01). A row in someone's localStorage is history, not a format.
const ARCANE_RENAMED = {
  enervate: "secondary_enervate",
  deadhead: "secondary_deadhead",
  flare: "cascadia_flare",
};
function arcaneFor(weaponId, id, i = 0) {
  if (!id || id === "none") return "none";
  const canon = ARCANE_RENAMED[id] || id;
  const a = arcaneById(canon);
  return a && a.slot === arcanePools(weaponId)[i] ? canon : "none";
}
/// Can this weapon seat this arcane in ANY of its slots?
///
/// `arcaneFor` answers a different question — "does it fit slot i" — and its
/// `i` defaults to 0. The optimizer's SCOPE is not per-slot (an arcane belongs
/// to exactly one pool, so the flat mark map already says which), so asking
/// `arcaneFor(w, id)` there silently meant "does it fit the FIRST pool": every
/// secondary mark was dropped on restoring an optimizer-arcanes preset, and
/// the search then had nothing to put in the second slot (user, 2026-08-01).
const arcaneFitsWeapon = (weaponId, id) => {
  const a = arcaneById(ARCANE_RENAMED[id] || id);
  return !!a && arcanePools(weaponId).includes(a.slot);
};
/// Every slot's id, validated against the pool that slot draws from.
const arcanesFor = (weaponId, list) =>
  arcanePools(weaponId).map((_, i) => arcaneFor(weaponId, asArcaneList(list, i + 1)[i], i));
/// The builder picker's list. Same set — kept as its own name because the
/// picker is where a reader looks for it.
const arcanePickPool = arcanePool;
/// Which slot the picker is filling — the popover is shared, the slot is not.
let arcaneSlotIdx = 0;
const arcaneById = (id) => META.arcanes.find((x) => x.id === id);
// new arcane → max rank, in the slot the picker was opened from
// EVERY ARCANE MUTATION REFRESHES, because the mutation owns the consequence.
//
// Changing an arcane used to redraw the arcane slots and nothing else — the
// panel, its stat rows and the SIM'S BUFF BAR all kept showing the previous
// arcane until some unrelated edit happened to call `refreshPanel`. Toggling a
// mod was the usual accident, which is exactly how it was reported
// (2026-08-05).
//
// `refreshPanel` is the funnel — its own comment says "every build change
// funnels through here" — so the fix is not to add the call at each picker but
// to make it impossible to mutate an arcane without it. A future control that
// sets an arcane gets the refresh for free; one that assigns `arcanes[i]`
// directly is the thing to look for in review.
function setArcane(id, i = arcaneSlotIdx) {
  arcanes[i] = id;
  arcaneRanks[i] = null;
  refreshPanel();
}
/// An arcane's RANK is a build change too: its numbers scale per rank, so the
/// panel and the buff bar are wrong until they are re-asked.
function setArcaneRank(i, rank) {
  arcaneRanks[i] = rank;
  refreshPanel();
}
// Effect lines for a specific rank (clamped). Arcane strengths scale per rank
// (wiki), so the slot shows the SELECTED rank; the picker shows max rank.
const effectsAt = (a, r) => {
  const rk = a && a.ranks || [];
  if (!rk.length) return [];
  return rk[Math.max(0, Math.min(rk.length - 1, r))] || [];
};
// Renders card lines that are ALREADY in the display language (cardLines
// did the choosing) — this is layout, not translation.
const effLines = (arr) => arr.length ? `<div class="me">${arr.map((x) => `<div>${x}</div>`).join("")}</div>` : "";

function renderArcanes() {
  const box = $("arcane-slots");
  box.innerHTML = "";
  weaponAxes().arcanes.forEach((ax, i) => box.appendChild(arcaneSlotEl(ax.pool, i)));
}

// One arcane slot: the card if filled, the "+ add" plate if not. The POOL is
// named on the plate only when a weapon seats more than one, because that is
// the only time it tells you anything.
function arcaneSlotEl(pool, i) {
  const many = arcanePools().length > 1;
  const a = arcaneById(arcanes[i]);
  const none = !a || a.id === "none";
  const el = document.createElement("div");
  if (none) {
    el.className = "slot empty arc";
    el.innerHTML = `<span class="plus">+ ${escHtml(
      many ? tr("add {pool} arcane").replace("{pool}", tr(SLOT_LABEL[pool] || pool)) : tr("add arcane"),
    )}</span>`;
  } else {
    const maxr = a.max_rank || 0;
    const r = arcaneRanks[i] == null ? maxr : Math.max(0, Math.min(maxr, arcaneRanks[i]));
    const lowered = r < maxr;
    const rank = maxr > 0
      ? `<span class="rank ${lowered ? "lowered" : ""}"><button class="rk" data-d="-1">−</button><b>R${r}${lowered ? "/" + maxr : ""}</b><button class="rk" data-d="1">+</button></span>`
      : "";
    el.className = "slot filled arc" + (a.rarity ? " rar-" + a.rarity : "");
    // The slot shows the verbatim DESCRIPTION at the selected rank (like
    // the mod cards); model effect lines remain the search text.
    el.innerHTML = imgTag(IMG(a.image), "mod") +
      `<div class="info"><div class="mn">${wl(a.name, wikiUrl(a.name_en || a.name))}</div>${effLines(cardLines(a, r, effectsAt(a, r)))}${rank}</div>` +
      `<button class="dots" title="options">⋯</button>`;
    el.querySelector(".dots").addEventListener("click", (e) => { e.stopPropagation(); openArcaneMenu(el, i); });
    el.querySelectorAll(".rk").forEach((b) => b.addEventListener("click", (e) => {
      e.stopPropagation();
      setArcaneRank(i, Math.max(0, Math.min(maxr, r + Number(b.dataset.d))));
      renderArcanes();
    }));
  }
  // Mod-slot parity: only the EMPTY slot opens the picker on click; a
  // filled card swaps via its ⋯ menu — so its text stays selectable.
  if (none) {
    el.addEventListener("click", (e) => { e.stopPropagation(); openArcanePicker(el, i); });
  }
  return el;
}

function openArcanePicker(anchor, i = 0) {
  arcaneSlotIdx = i;
  closePopovers();
  const pop = $("arcane-popover");
  place(pop, anchor);
  const search = $("arcane-search");
  search.value = "";
  search.oninput = () => renderArcaneMenu(search.value);
  renderArcaneMenu("");
  search.focus();
  ensureGains({ kind: "arcane", idx: i },
    () => { if (!$("arcane-popover").hidden) renderArcaneMenu($("arcane-search").value); });
}

// Search matches NAME or any EFFECT line (like the mod picker). "None" always
// stays listed as the clear-out option.
function renderArcaneMenu(query) {
  const menu = $("arcane-menu");
  const q = query.trim().toLowerCase();
  // Search matches NAME (localized or English), ANY rank's effect text,
  // or the description — in either language (searchBlob).
  // Same rule as the mod picker, minus a key an arcane does not have: there
  // is no drain on an arcane, so it is effect then name.
  const hits = arcanePickPool(arcaneSlotIdx)
    .filter((a) => !q || searchBlob(a).includes(q))
    .sort((a, b) => (a.id === arcanes[arcaneSlotIdx] ? -1 : b.id === arcanes[arcaneSlotIdx] ? 1 : 0)
      || gainSort(a, b, ["gain", "name"]));
  menu.innerHTML = hits.length ? hits.map((a) => {
    const isCur = a.id === arcanes[arcaneSlotIdx];
    return `<div class="opt ${isCur ? "cur" : ""} ${a.rarity ? "rar-" + a.rarity : ""}" data-id="${a.id}">
      ${imgTag(IMG(a.image), "mod")}
      <div class="info"><div class="mn">${wl(a.name, wikiUrl(a.name_en || a.name))}${isCur ? ' <span class="slotchip cur">equipped</span>' : ""}${gainChipFor(a.id, tr("Arcane"))}</div>${effLines(cardLines(a, a.max_rank, effectsAt(a, a.max_rank)))}</div></div>`;
  }).join("") : `<div class="opt dis">no matches</div>`;
  menu.querySelectorAll(".opt:not(.dis)").forEach((o) => o.addEventListener("click", () => { setArcane(o.dataset.id); closePopovers(); renderArcanes(); }));
}

// ⋯ on a filled arcane slot: mirror the mod slot menu (remove).
function openArcaneMenu(anchor, i = 0) {
  arcaneSlotIdx = i;
  closePopovers();
  const menu = $("slot-menu");
  menu.innerHTML = `<div class="mi" data-a="swap">Swap arcane</div><div class="mi danger" data-a="remove">Remove arcane</div>`;
  place(menu, anchor);
  menu.querySelector('[data-a="swap"]').addEventListener("click", () => openArcanePicker(anchor, i));
  menu.querySelector('[data-a="remove"]').addEventListener("click", () => { setArcane("none", i); closePopovers(); renderArcanes(); });
}

// ---- Evolution ----
// Every tier (EVO I–IV) renders its options as CARDS — icon, name, and the
// verbatim effect text, like the mod/arcane cards — PLUS an explicit None
// card (nothing installed). Wiki-flagged broken evolutions carry a red
// BROKEN badge, and selecting one shows a red note: the engine really
// computes them as NO EFFECT. Deselecting tier 1 (the Incarnon Form
// unlock) drops the weapon to its base form.
// Evolution tiers are per weapon, not a fixed four: Zariman weapons run
// I-V (Laetum), Incarnon Genesis adapters I-IV. Build the numeral instead
// of indexing a table that stops at IV.
const ROMAN = (n) => {
  const T = [[10, "X"], [9, "IX"], [5, "V"], [4, "IV"], [1, "I"]];
  let out = "";
  for (const [v, sym] of T) while (n >= v) { out += sym; n -= v; }
  return out;
};
/// The modes this build may be played in, each with the reason it cannot be —
/// `[id, label, offReason]`, the same shape the Form control always took.
///
/// A mode a BUILD rules out is offered DISABLED, not dropped: "the weapon has
/// no Incarnon form while that mod is on it" is information, and a vanished
/// option is not. Asking for a cycle implies its unlock, and installing that
/// unlock takes a Cannonade off the weapon — so a build wearing one has no
/// cycle to run and the sim refuses it.
function modeOpts(w) {
  const ids = (w.modes || []);
  if (ids.length < 2) return [];
  const cost = (w.unlock_evo && (w.evo_forbids || {})[w.unlock_evo]) || [];
  const blocker = slots
    .map((s) => s.mod)
    .filter((m) => m && cost.includes(m))
    .map((m) => (modById(m) || {}).name || m)[0];
  const off = blocker
    ? `${blocker} ${tr("needs the same trigger on every firing mode, so this build has no Incarnon form")}`
    : null;
  return ids.map((id) => [id, modeLabel(w, id), id === "cycle" ? off : null]);
}

/// The mode control, in the BUILDER, in a block of its own above the mods.
///
/// ALWAYS DRAWN. A weapon with one way to be fired is still being fired in it,
/// and a panel that says nothing leaves the reader to assume — which is the
/// same rule the Form control carried, and the reason a single form was stated
/// rather than hidden (user, 2026-07-31, restated 2026-08-07). Several modes
/// is a dropdown; one is a value.
/// THE VALENCE THIS WEAPON MAY HAVE, or null. One place asks the question, so a
/// control, a payload and a reset cannot disagree about whether the axis exists.
const valenceSpec = (id) => (weaponInfo(id) || {}).valence || null;

/// A build's valence, cleaned against the weapon it is being opened on.
///
/// An element the spec does not offer is DROPPED rather than kept — a preset
/// imported from another weapon carries one, and so does a stale one written
/// before an element was removed. The percentage is clamped to the roll's own
/// range for the same reason.
function defaultValence(id, st) {
  const s = valenceSpec(id);
  if (!s) return { element: "", bonus: 0 };
  // 60% IMPACT, not "none" (owner, 2026-08-13). Every copy of an adversary
  // weapon in the game comes out of its Lich carrying a bonus, so "no element"
  // is not a weaker build of it — it is a weapon nobody has, and the panel
  // would print numbers no player can reproduce.
  //
  // The FIRST element and the roll's CEILING: the ceiling because every player
  // can Valence-fuse to it and it is what the board scores, the first because
  // the wiki lists them in the game's own order and Impact is where it starts.
  const el = st && s.elements.includes(st.element) ? st.element : s.elements[0];
  const b = st && Number.isFinite(Number(st.bonus)) ? Number(st.bonus) : s.max;
  return { element: el, bonus: Math.min(Math.max(b, s.min), s.max) };
}

/// THE VALENCE BLOCK. Two controls, because a Lich hands you two facts: which
/// element, and how big the roll was.
///
/// THE ELEMENT IS MANDATORY (owner, 2026-08-14). Every copy of an adversary
/// weapon comes out of a Lich carrying one, so there is no "none" to pick and
/// no empty state to open on: `defaultValence` starts on the first element at
/// the roll's ceiling. The weapon's own printed panel — the wiki infobox's
/// figure — is not a build anyone can play, and offering it as one put a
/// number nobody can reproduce on the same row as six that they can.
function renderValence() {
  const box = $("element-cfg");
  if (!box || !META) return;
  const w = weaponInfo($("weapon").value) || {};
  const s = valenceSpec(w.id);
  const sub = $("valence-sub");
  if (!s) { box.innerHTML = ""; if (sub) sub.textContent = ""; return; }
  if (sub) sub.textContent = tr("the bonus this copy came out of its Lich with");
  const pct = (x) => Math.round(x * 1000) / 10;
  // A PICK ROW, not a dropdown — the same shape an evolution tier has, and for
  // the same reason: every option carries its own quick-calc gain, and a chip
  // is the one place a reader can compare seven of them at a glance. Any factor
  // that moves the headline number is ranked with this same UI.
  //
  // Which element wins is the question a scan is worth the most on here: a
  // progenitor element is a whole element entering the hierarchy, so the answer
  // depends on the mods around it and on the target, and no card states it.
  const pick = (id, label, note) => {
    const on = valence.element === id;
    return `<span class="evopick${on ? " sel" : ""}" data-vel="${escHtml(id)}">
      <span class="einfo"><b class="en">${escHtml(label)}${
        gainChipFor(id, tr("Valence"))}</b><span class="ed"><div>${escHtml(note)}</div></span></span></span>`;
  };
  // NO "NONE" OPTION (owner, 2026-08-14). Every copy of an adversary weapon
  // comes out of a Lich carrying an element, so an empty valence is not a
  // weaker build of this weapon — it is a weapon nobody has, and a number
  // nobody can reproduce. It was offered here as "the weapon's printed panel",
  // which is the wiki infobox's figure and not a build.
  const picks = s.elements
    .map((e) => pick(e, DT(e), tr("added as the weapon's own BASE damage — elemental mods and status scale with it")))
    .join("");
  box.innerHTML =
    `<div class="evo"><span class="rank">${escHtml(tr("Element"))}</span><div class="picks">${picks}</div></div>` +
    `<div class="runs-row"><label title="${escHtml(tr("how big the roll was, as a share of base damage — a Lich rolls it randomly and Valence Fusion raises it, capping at the number on the right"))}">${escHtml(tr("Valence bonus"))} <span class="unit">%</span> <input type="number" id="valence-bonus" min="${pct(s.min)}" max="${pct(s.max)}" step="0.5" value="${pct(valence.bonus)}"></label>` +
    `<span class="sim-hint">${escHtml(tr("rolls") + ` ${pct(s.min)}–${pct(s.max)}%`)}</span></div>`;
  // The scan that fills the chips. Keyed like every other axis, so it runs once
  // per (build, fight) and repaints when it lands.
  ensureGains({ kind: "valence", idx: 0 }, () => renderValence());
  box.querySelectorAll(".evopick").forEach((c) =>
    c.addEventListener("click", () => {
      valence.element = c.dataset.vel;
      markPresetDirty();
      renderValence();
      refreshPanel();
    }));
  const inp = $("valence-bonus");
  if (inp) {
    inp.addEventListener("change", () => {
      const v = Number(inp.value) / 100;
      valence.bonus = Math.min(Math.max(Number.isFinite(v) ? v : s.min, s.min), s.max);
      markPresetDirty(); renderValence(); refreshPanel();
    });
  }
}

function renderMode() {
  const box = $("mode-row");
  if (!box || !META) return;
  const w = weaponInfo($("weapon").value) || {};
  const opts = modeOpts(w);
  const sub = $("mode-sub");
  if (!opts.length) {
    // ONE WAY TO FIRE IT. Named from the weapon's own form, so it reads as a
    // fact about the weapon rather than as a control somebody disabled.
    const only = (w.modes || ["base"])[0];
    box.innerHTML = `<label>${escHtml(tr("Mode"))} <span class="fixed-val">${
      escHtml(modeLabel(w, only))}</span></label>`;
    if (sub) sub.textContent = tr("one firing mode");
    return;
  }
  // A build naming a mode this weapon does not offer falls back to how the
  // weapon is played — a stale preset, or one copied from another weapon.
  if (!opts.some(([id]) => id === mode)) mode = defaultMode(w.id, null);
  const why = (opts.find(([id]) => id === mode) || [])[2];
  if (sub) sub.textContent = tr("how this build is played");
  box.innerHTML = `<label>${escHtml(tr("Mode"))} ${ddButton("dd-mode", {
    value: mode,
    items: opts.map(([id, label, offReason]) => ({
      value: id, label: label + (offReason ? " ⊘" : ""), hint: offReason || "",
      disabled: !!offReason,
    })),
    onPick: (v) => {
      mode = v;
      // Keep the address bar honest: it may have said a mode, and it is not
      // saying this one any more.
      const q = new URLSearchParams(location.search);
      if (q.get("mode") && q.get("mode") !== v) {
        q.set("mode", v);
        history.replaceState(null, "", `${location.pathname}?${q}`);
      }
      markPresetDirty(); renderMode(); refreshPanel();
    },
  })}</label>${why ? `<span class="warn">⊘ ${escHtml(why)}</span>` : ""}`;
}

function renderEvo() {
  const tiers = weaponEvos();
  const rows = [];
  // TIERS UNLOCK IN ORDER, as they do in game: tier N is reachable only once
  // tier N-1 is installed (user, 2026-08-01). Without it the whole branch is
  // void — a tier-2 perk with no tier 1 is not a weaker build, it is not a
  // build — so the later rows are shown DISABLED rather than silently
  // contributing to a number nobody could reach.
  const openTo = evoOpenTo();
  for (const t of tiers) {
    const sel = evoSel[t.tier] || null;
    const locked = t.tier > openTo;
    const card = (o) => {
      const icon = o.icon ? `<img class="eicon" src="${IMG(o.icon)}" alt="">` : "";
      const cls = ["evopick", o.id === sel ? "sel" : "", o.broken ? "broken" : "",
        locked ? "tlocked" : ""].join(" ");
      const lines = evoLines(o).map((x) => `<div>${escHtml(x)}</div>`).join("");
      const title = (o.effects || []).join("\n"); // model statement as tooltip
      // The broken warning lives INSIDE the selected card, so it never
      // straddles the row divider into the next tier.
      const warn = o.broken && o.id === sel
        ? `<span class="ed warn">⚠ does not work in-game (wiki) — the simulation computes it as NO EFFECT</span>`
        : "";
      // THE CONDITION OVERLOAD CAVEAT, on the tile you choose from.
      //
      // A perk flagged here raises base damage WITHOUT feeding the CO term, so
      // the card reads strictly better than the tier's other option and is not
      // — every status is worth less, and past a couple of statuses the other
      // one overtakes it. That was reported as a bug (2026-08-05) precisely
      // because the only place saying so was the CO row in the stats panel,
      // which is not where the comparison happens.
      //
      // Shown on EVERY option, not just the selected one: the whole point is to
      // be readable while deciding.
      const coNote = o.co_excluded
        ? `<span class="ed caveat" title="${escHtml(
            tr("Condition Overload computes on this weapon's ORIGINAL base damage — this perk's added base is excluded, so every status type is worth less than the card implies"),
          )}">◈ ${escHtml(tr("its added base does not feed Condition Overload"))}</span>`
        : "";
      // Evolutions have no standalone wiki pages, so they link to the
      // WEAPON's — which carries the same evolution tables and is where you
      // wanted to end up anyway (user, 2026-08-01). It used to point at the
      // "<Weapon> Incarnon Genesis" page: correct, and one hop further from
      // everything else you would look up while reading the card.
      const genesis = wikiUrl(wikiWeaponName(weaponInfo($("weapon").value)));
      // WHAT THE SIM DOES NOT MODEL, on the tile you choose from — the same
      // chip the optimizer's list has carried all along.
      //
      // It was missing here, and the asymmetry only became expensive when the
      // roster grew: eleven Incarnon weapons landed on 2026-08-08 carrying 31
      // perks with an unmodelled effect, and in the BUILDER every one of them
      // read exactly like its working tier-mates. The data knew
      // (`fully_unmodeled`), the optimizer said so, and the surface where the
      // choice is actually made did not (owner, 2026-08-08).
      //
      // TWO STATES, because they are different facts: a perk whose EVERY effect
      // is inert is not a weaker choice, it is not a choice; one with a live
      // half is a real pick that is being under-counted.
      const unmod = evoGapChips(o, "i");
      return `<span class="${cls}" data-tier="${t.tier}" data-id="${o.id}" title="${title}">
        ${icon}<span class="einfo"><b class="en">${wl(o.name, genesis)}${o.broken ? ' <i class="bx">BROKEN</i>' : ""}${unmod}${
          gainChipFor(o.id, `EVO ${ROMAN(t.tier)}`)}</b><span class="ed">${lines}</span>${coNote}${warn}</span></span>`;
    };
    const empty = `<span class="evopick empty ${sel === null ? "sel" : ""} ${locked ? "tlocked" : ""}" data-tier="${t.tier}" data-id="">
      <span class="einfo"><b class="en">None</b><span class="ed"><div>nothing installed at this tier</div></span></span></span>`;
    // None comes FIRST (the default state is a bare weapon).
    rows.push(`<div class="evo${locked ? " locked" : ""}" ${locked
      ? `title="${escHtml(tr("install the previous tier first"))}"` : ""
    }><span class="rank">EVO ${ROMAN(t.tier)}</span><div class="picks">${empty}${t.options.map(card).join("")}</div></div>`);
  }
  $("evo-rows").innerHTML = rows.join("");
  // Evolutions are all on screen at once, so they are scanned ACROSS EVERY
  // TIER in one pass — a dozen candidates, not seventy, which is why they can
  // afford to answer without being opened (user, 2026-08-01: arcanes and
  // evolutions use this too). The key guards the repeat.
  if (tiers.length) ensureGains({ kind: "evo", idx: 0 }, () => renderEvo());
  $("evo-rows").querySelectorAll(".evopick:not(.tlocked)").forEach((c) => c.addEventListener("click", () => {
    const tier = Number(c.dataset.tier);
    evoSel[tier] = c.dataset.id || null;
    // Removing a tier removes everything that stood on it. Leaving them
    // selected-but-void would show a build the game cannot make, and the
    // engine would price perks the weapon never reached.
    if (!evoSel[tier]) tiers.forEach((x) => { if (x.tier > tier) evoSel[x.tier] = null; });
    // ...and an installed form takes with it every mod that needed the weapon
    // not to have it (a Cannonade under the Incarnon form). Said out loud, never
    // silently: the slot emptying under you is exactly the kind of change a
    // build must not make without telling you (user, 2026-08-04).
    const no = forbiddenByEvos();
    const evicted = slots
      .filter((s) => s.mod && no.has(s.mod))
      .map((s) => (modById(s.mod) || {}).name || s.mod);
    if (evicted.length) {
      slots = slots.map((s) => (s.mod && no.has(s.mod) ? { ...s, mod: null, rank: null } : s));
      presetToast(`${tr("unequipped")}: ${evicted.join(", ")} — ${
        tr("it needs the same trigger on every firing mode")}`);
      renderMods();
    }
    // Redraw the whole ladder, not just this row: a pick opens (or a removal
    // shuts) every tier below it.
    renderEvo(); renderMode(); refreshPanel();
  }));
}

// ---- Sim: scenario/buff settings + run against an enemy -----------------
// The build (mods/arcane/evolutions) comes from buildPayload(); this block
// only owns the scenario + engine-modeled buff levers (`sim`). Run POSTs to
// /api/simulate and renders a summary card + an illustrative arena replay.
// The SIMULATOR tab's read-only build summary — the sim always tests the
// ACTIVE preset's build, and this card shows exactly what that is (mods
// at rank, arcane, evolutions). Editing happens in the Builder; the
// button jumps there.
function renderSimBuild() {
  const box = $("sim-build-info");
  if (!box || !META) return;
  const sub = $("sim-build-sub");
  // The LABEL, never the id: an official build's id carries its ruler and rank
  // (`single_target#cycle#1`) and is not a thing to show anyone.
  const activeLabel = presetLabel(buildNamed(activePreset));
  if (sub) sub.textContent = activeLabel ? `${tr("testing build")}: ${activeLabel}` : "";
  const chip = (img, label, rk) =>
    `<span class="sb-chip">${imgTag(img, "sb-img")}<span>${escHtml(label)}</span>${rk != null ? `<span class="rk">R${rk}</span>` : ""}</span>`;
  const w = weaponInfo($("weapon").value);
  const parts = [];
  const modChips = slots.map((s) => {
    const m = s.mod && modById(s.mod);
    if (!m) return "";
    return chip(IMG(m.image), m.name, s.rank == null ? m.max_rank : s.rank);
  }).filter(Boolean);
  // HOW IT IS PLAYED, first, and READ-ONLY. It is part of the build, so the
  // simulator shows it and does not offer to change it — the fight owns the
  // fight and the build owns this. This card is now the ONLY place the mode
  // appears on this tab: the builder's own control is hidden here, because two
  // places to read one field with one of them writable is how a build gets
  // edited somewhere that is not the builder.
  //
  // ALWAYS, including a weapon with one way to be fired — the same rule the
  // builder's block carries ("one mode is stated, not offered"). It used to be
  // drawn only where there was a choice, which made a summary of the build
  // silently drop a field the build has.
  parts.push(`<div class="sb-h">${tr("Mode")}</div>`);
  parts.push(`<div class="sb-chips"><span class="sb-chip">${
    escHtml(modeLabel(weaponInfo($("weapon").value), mode))}</span></div>`);
  // THE VALENCE, beside the mode and for exactly the same reason: it is part
  // of what this build IS. Two Kuva Nukors differing only in progenitor
  // element are two different builds and two different numbers, so a card that
  // says "this is what is being tested" and omits it is telling half of it.
  //
  // Only where the weapon HAS one — the same "no choice, no axis" rule the
  // block in the builder follows. The percentage is shown with the element
  // because the roll is the other half of the fact: 60% Heat and 25% Heat are
  // not the same weapon.
  if (valenceSpec(w.id)) {
    parts.push(`<div class="sb-h">${tr("Valence")}</div>`);
    parts.push(`<div class="sb-chips"><span class="sb-chip">${
      escHtml(DT(valence.element))} +${Math.round(valence.bonus * 1000) / 10}%</span></div>`);
  }
  parts.push(`<div class="sb-h">${tr("Mods")} · ${modChips.length}</div>`);
  parts.push(`<div class="sb-chips">${modChips.join("") || `<span class="sb-empty">${tr("no mods equipped")}</span>`}</div>`);
  if ((w.arcane_slots || 0) >= 1) {
    const arcChips = arcanes
      .map((id, i) => {
        const a = id !== "none" && arcaneById(id);
        return a ? chip(IMG(a.image), a.name, arcaneRanks[i] ?? ((a.ranks || []).length - 1)) : "";
      })
      .filter(Boolean);
    parts.push(`<div class="sb-h">${tr("Arcane")}</div>`);
    parts.push(`<div class="sb-chips">${arcChips.join("") || `<span class="sb-empty">${tr("no arcane")}</span>`}</div>`);
  }
  if (w.uses_evo2) {
    const evoChips = weaponEvos().map((t) => {
      const o = evoSel[t.tier] && t.options.find((x) => x.id === evoSel[t.tier]);
      return o ? chip(o.icon ? IMG(o.icon) : null, `${t.tier} · ${o.name}`) : "";
    }).filter(Boolean);
    parts.push(`<div class="sb-h">${tr("Evolutions")}</div>`);
    parts.push(`<div class="sb-chips">${evoChips.join("") || `<span class="sb-empty">${tr("none selected")}</span>`}</div>`);
  }
  parts.push(`<a class="ghost-btn small sb-edit" href="${weaponPath($("weapon").value)}">${tr("edit in Builder")}</a>`);
  box.innerHTML = parts.join("");
}

// The headshot rate a weapon is played at. A SENTINEL is fired by the
// companion, which picks its own targets and does not aim for the head, so it
// starts at 0 rather than the player's 100 (user, 2026-07-31). Still a knob
// here — the engine is what PINS a sentinel at 0 whatever a request says, so
// this only decides where the control opens.
const defaultHeadshotPct = (w) => ((w || {}).sentinel ? 0 : META.defaults.headshot_pct);

// The SCENARIO — enemy, technique, measurement — is the SIMULATOR's, and the
// optimizer borrows it instead of keeping a lookalike of its own (user,
// 2026-08-02). One state, one renderer: the search is then, by construction,
// scored under the fight you are simulating with, and a scenario preset
// switches both at once. It also ends the whole class of bug where the two
// copies drifted a field apart and the winner was crowned under a fight the
// replay never ran.
//
// `ids` names the host element per section; a section with no host is not
// drawn. Nothing is drawn differently per host — a flag that moved one field
// for one tab would be the same lookalike-that-drifts in miniature (user,
// 2026-08-02, on finding the optimizer's enemy box one field longer).
//
// ENGAGEMENT LENGTH sits with the enemy, not with the measurement: it is a
// property of the fight, which is exactly why the optimizer needs it while
// needing neither Runs (the funnel sets those round by round) nor Measure.
function renderScenarioFields(ids, opts = {}) {
  const w = weaponInfo($("weapon").value);
  const enemies = allEnemies();
  const en = enemies.find((e) => e.id === sim.enemy) || enemies[0];
  if (en) sim.enemy = en.id;

  // ---- 1. THE FIGHT: who, where, how strong, how long ------------------
  // Target and arena in ONE block, not two (user, 2026-08-02): an enemy is
  // not a name, it is a name plus everything about the encounter, and the
  // enemies still to come bring their own assortment of those — a level, an
  // arena, an Eximus flag, a faction override. They all answer "what am I
  // shooting at, under what conditions", so they belong in one place that can
  // grow rather than in a grid that has to be re-cut every time.
  if (ids.target) {
    // The wiki link sits OUTSIDE the picker button — an <a> inside a <button>
    // is not valid HTML, and the two are different actions anyway: one
    // changes the fight, the other reads about the unit. Built from the
    // ENGLISH name like every other wiki link, and absent for a synthetic
    // target, which has no page to land on.
    const wiki = en && !en.synthetic
      ? `<a class="en-wiki" href="${wikiUrl(en.name_en || en.name)}" target="_blank" rel="noopener"
            title="${escHtml(tr("open the wiki page"))}">${escHtml(tr("wiki"))} ↗</a>`
      : "";
    $(ids.target).innerHTML =
      `<div class="en-row">
         <button class="en-card" id="${ids.target}-pick" title="${escHtml(tr("choose the target"))}">
           ${enemyImg(en, "en-img")}
           <span class="en-txt">
             <span class="en-name">${escHtml(en ? en.name : tr("Enemy"))}</span>
             <span class="en-meta">${escHtml(enemyMeta(en))}</span>
             ${enemyVuln(en)}${enemyStatusImmune(en)}${enemyEffectsNulled(en)}
             ${enemyCaveat(en)}
           </span>
         </button>
         ${wiki}
       </div>` +
      `<div class="field-grid">
        <label>${escHtml(tr("Level"))} <input type="number" data-k="level" min="1" max="9999" value="${sim.level}"></label>
        <label class="check"><input type="checkbox" data-k="steel_path" ${sim.steel_path ? "checked" : ""}> Steel Path</label>
        ${eximusField(en)}
        ${deployField(w, sim)}
        <label>${escHtml(tr("Duration (s)"))} <input type="number" data-k="duration" min="1" max="3600" value="${sim.duration}"></label>
      </div>`;
    const pick = $(`${ids.target}-pick`);
    if (pick && !opts.readonly) pick.onclick = (e) => { e.stopPropagation(); openEnemyPicker(pick); };
    if (pick && opts.readonly) pick.disabled = true;
  }

  // ---- 2. THE WIELDER: whoever is holding the weapon, and what they do ---
  // Block 1 is the other actor; this is your side of the fight. It grew from
  // "technique" when the engine got a second actor (user, 2026-08-02): the
  // form you fire, how you fire it, the states a mod card gates on, and the
  // Warframe behind it are all one answer to "who is shooting". A neutral
  // frame wears nothing and is doing nothing, which is why the states are off
  // and the stats are 0 — and why the mods and arcanes that read them
  // contribute nothing until you say otherwise, on the panel, in the sim and
  // in the search alike.
  //
  // NOT "the Tenno" (user, 2026-08-04). A ROBOTIC weapon is not held by one:
  // the wiki is explicit that MOAs "share Robotic weapons with Sentinels and
  // equip their weapons", so the wielder of Verglas Prime is a Sentinel or a
  // MOA. Naming the section after the commonest case made the model say
  // something false about every companion weapon.
  // NO FORM CONTROL HERE. How the weapon is played is part of the BUILD and
  // lives in the builder — a fight that decided it could only ever measure
  // whichever way the ruler happened to pin, which is what kept "the Torid
  // that never transmutes" unaskable (owner, 2026-08-07).
  if (ids.technique) {
    $(ids.technique).innerHTML = `
      ${aimField(w, sim)}
      <label title="${escHtml(tr("a per-PELLET aim weight, not a whole-spread promise — the landing spot is rolled for each pellet"))}">${escHtml(tr("Headshot %"))} <input type="number" data-k="headshot_pct" min="0" max="100" value="${sim.headshot_pct}"></label>
      <label class="check" title="${escHtml(tr("the wielder's state: mods that only pay while Invisible (Spectral Serration) grant nothing when this is off"))}"><input type="checkbox" data-k="invisible"${sim.invisible ? " checked" : ""}> ${escHtml(tr("Invisible"))}</label>
      <label class="check" title="${escHtml(tr("the wielder's state: what a card means by \"while Airborne\""))}"><input type="checkbox" data-k="airborne"${sim.airborne ? " checked" : ""}> ${escHtml(tr("Airborne"))}</label>
      <label class="check" title="${escHtml(tr("the wielder's state: what a card means by \"With Overshields\". Nothing here takes them away, so it is a declaration"))}"><input type="checkbox" data-k="overshields"${sim.overshields ? " checked" : ""}> ${escHtml(tr("Overshields"))}</label>
      <label class="check" title="${escHtml(tr("the wielder's state: what a card means by \"With Channeled Ability active\". The ability must DRAIN ENERGY over time — Desecrate, Haven and an empty Gloom do not count"))}"><input type="checkbox" data-k="channeling"${sim.channeling ? " checked" : ""}> ${escHtml(tr("Channeled ability"))}</label>
      <label class="check" title="${escHtml(tr("the LOADOUT, not what the wielder is doing: off means a full one, which is what the board is scored under. On, this weapon is the only one carried — the Vasto's Lone Gun pays its \"With No Primary Equipped\" half, and every \"On Equip from Primary\" or \"while Holstered\" clause becomes impossible rather than merely unmodelled"))}"><input type="checkbox" data-k="solo_weapon"${sim.solo_weapon ? " checked" : ""}> ${escHtml(tr("Only this weapon"))}</label>
      <label title="${escHtml(tr("picking a frame fills armor, max energy and sprint speed below — the roster is UNMODDED, so a built frame carries more and the numbers stay editable"))}">${escHtml(tr("Warframe"))} <select data-k="frame">
        <option value=""${sim.frame ? "" : " selected"}>${escHtml(tr("none — no frame"))}</option>
        ${frames().map((f) => `<option value="${escHtml(f.id)}"${f.id === sim.frame ? " selected" : ""}>${escHtml(f.name)}</option>`).join("")}
      </select></label>
      <label title="${escHtml(tr("your Warframe's armor, buffs included — Primary Bulwark pays +1% damage per point past 1,000. 0 = no frame"))}">${escHtml(tr("WF Armor"))} <input type="number" data-k="wf_armor" min="0" max="100000" step="1" value="${sim.wf_armor || 0}"></label>
      <label title="${escHtml(tr("your Warframe's MAX energy — Primary Overcharge turns 35% of it into multishot. 0 = no frame"))}">${escHtml(tr("WF Energy"))} <input type="number" data-k="wf_energy" min="0" max="100000" step="1" value="${sim.wf_energy || 0}"></label>
      <label title="${escHtml(tr("your Warframe's sprint speed — several Incarnon perks pay only at 1.2 or higher, and the slowest frame is 0.9"))}">${escHtml(tr("WF Sprint"))} <input type="number" data-k="wf_sprint" min="0" max="3" step="0.05" value="${sim.wf_sprint ?? 0.9}"></label>`;
  }

  // ---- 3. LIMITS: what the simulation is allowed to assume -------------
  // Infinite ammo is NOT a technique (user, 2026-08-02) — nobody plays it. It
  // is a statement about what this run does not model, and the things that
  // will join it are the same kind of statement.
  if (ids.limits) {
    $(ids.limits).innerHTML = ammoField(w, sim);
  }

  // ---- 3b. THE FIGHT'S OWN STAT BONUSES --------------------------------
  //
  // Everything this weapon is handed by something that is not its build. They
  // land in the same ADDITIVE buckets the mods feed — "效果等于又塞mod" — so a
  // player who knows what their squad, their frame or an ability is worth types
  // the number and the whole app treats it as one more card: the panel's own
  // arithmetic, the sim, the optimizer's scoring, and every lock.
  //
  // BLANK IS ZERO, not empty: a fight hands this weapon nothing unless someone
  // says otherwise, which is what every ruler and every stored scenario means.
  if (ids.extra) {
    const ex = sim.extra_stats || {};
    $(ids.extra).innerHTML = EXTRA_STAT_KEYS.map(([k, label]) =>
      `<label title="${escHtml(tr("a percentage, into the same bucket a mod of this stat feeds — permanent, no trigger and no clock"))}">${escHtml(tr(label))} <span class="unit">%</span> <input type="number" data-xk="${k}" step="1" value="${ex[k] ? r3(ex[k] * 100) : ""}" placeholder="0"></label>`
    ).join("");
  }

  // ---- 4. MEASUREMENT: nothing the player does in-game -----------------
  if (ids.run) {
    $(ids.run).innerHTML = `
      <label title="${escHtml(tr("what the run is judged by — the headline number and the picker's gain scan both follow it"))}">${escHtml(tr("Measure"))} ${
        ddButton("dd-metric", {
          value: sim.metric,
          dataK: "metric",
          items: [{ value: "kpm", label: tr("KPM"), hint: tr("kills per minute") },
                  { value: "dps", label: tr("DPS"), hint: tr("damage per second") }],
        })}</label>`;
  }

  const boxes = [ids.target, ids.technique, ids.limits, ids.extra, ids.run]
    .filter(Boolean)
    .map($);
  // READ-ONLY hosts get the same fields, in the same order, showing the same
  // values — and no way to change them. A preset is edited in exactly ONE
  // place (user, 2026-08-02): two editors over one document is how a document
  // gets edited twice and saved once. The optimizer therefore SHOWS the fight
  // and links to the module that owns it.
  if (opts.readonly) {
    boxes.forEach((box) => box.querySelectorAll("[data-k],[data-xk]").forEach((el) => {
      el.disabled = true;
      el.title = tr("edit this in the Simulator");
    }));
    return;
  }
  // The pools at THIS fight's level, fetched once per (level, Steel Path) and
  // painted in when they arrive. Fired from here because this is the one
  // function that draws the target card, on either tab.
  loadTargetStats().then((changed) => { if (changed) paintTargetMeta(); });
  boxes.forEach((box) =>
    box.querySelectorAll("[data-k]").forEach((el) => {
      el.addEventListener("change", () => {
        const k = el.dataset.k;
        if (el.type === "checkbox") sim[k] = el.checked;
        else if (el.type === "number") sim[k] = Number(el.value);
        else sim[k] = el.value;
        // PICKING A FRAME FILLS ITS THREE NUMBERS. They stay editable after —
        // the roster is unmodded, so a built frame carries more armor and more
        // energy than any entry here, and one gate ("With Energy Max Over 700")
        // no frame can open at all. Writing the fields rather than replacing
        // them with a read-only display is what keeps that askable.
        if (k === "frame") {
          const f = frameOf(sim.frame);
          if (f) {
            sim.wf_armor = f.armor;
            sim.wf_energy = f.energy;
            sim.wf_sprint = f.sprint;
          }
        }
        // No `enemy` case here: the target is the picker's, not a field's, and
        // it repaints the arena through renderSim() like everything else.
        // A TENNO field changes what the BUILD is worth, not just what the
        // fight looks like — the panel resolves against the player now, so it
        // has to be asked again. The enemy half changes no panel number.
        if (TENNO_KEYS.includes(k)) refreshPanel();
        // ONLY the scenario. A sim knob used to dirty the build preset too,
        // back when a build carried a copy of the fight; it does not, so this
        // is the scenario's edit and nobody else's (user, 2026-08-02).
        markScenarioDirty();
        // Whichever tab drew the field, both are looking at this one state.
        if (opts.after) opts.after();
      });
    }));
  // …AND THE FIGHT'S OWN STAT BONUSES, whose own map they write into. A
  // separate attribute rather than a `data-k` per stat because they are ONE
  // scenario field: nine numbers in one object, so a blank one is absent rather
  // than a zero nobody typed, and the share link carries only what was set.
  boxes.forEach((box) =>
    box.querySelectorAll("[data-xk]").forEach((el) => {
      el.addEventListener("change", () => {
        const next = { ...(sim.extra_stats || {}) };
        // TYPED IN PERCENT, stored as the fraction every bucket in the engine
        // holds — the same units a mod's `rankMax` is in.
        const v = Number(el.value) / 100;
        if (!Number.isFinite(v) || v === 0) delete next[el.dataset.xk];
        else next[el.dataset.xk] = v;
        sim.extra_stats = next;
        // It changes what the BUILD is worth, so the panel has to be asked
        // again — the same reason a Tenno field does.
        refreshPanel();
        markScenarioDirty();
        if (opts.after) opts.after();
      });
    }));
}

// The unit's portrait, or NOTHING — never an empty box holding its place.
// `imgTag` renders a placeholder span when the src is null, which is right
// for a mod grid (the slots must stay aligned) and wrong here: an enemy with
// no art yet should read as a name, not as a name with a hole beside it.
const enemyImg = (en, cls) => {
  const src = IMG(en && en.image);
  return src ? `<img class="${cls}" src="${src}" alt="" onerror="this.style.display='none'"/>` : "";
};

// What a run against this unit does not account for (`unmodeled` in its data
// file). An Acolyte carries damage attenuation whose constants DE has never
// published, so the number this app reports against one is too HIGH — that is
// a thing to say on the card, not a thing to leave the reader to discover.
const enemyCaveat = (en) => {
  const gaps = (en && en.unmodeled) || [];
  return gaps.length
    ? `<span class="en-gap" title="${escHtml(tr("the sim does not model this yet, so its number against this target is optimistic"))}">⚠ ${
        escHtml(tr("not modeled") + ": " + gaps.map((g) => tr(g)).join(", "))}</span>`
    : "";
};

// What this unit takes MORE and LESS of — the post-U36 faction column
// (`FactionDamageOverride ?? Faction`), which is half of what picks a build's
// elements. Its own line, not another entry in the meta run-on: a reader
// scanning for "what do I bring" should find it in one place, and up and down
// have to look different at a glance. A unit with a neutral column shows
// nothing at all — no line is the honest rendering of "takes damage as
// written", and an empty "Vulnerabilities:" label would not be.
const enemyVuln = (en) => {
  // What to BRING before what to avoid: the server sends them in damage-type
  // order, which interleaves the two answers.
  const mods = [...((en && en.type_modifiers) || [])].sort((a, b) => b.mult - a.mult);
  return mods.length
    ? `<span class="en-vuln" title="${escHtml(tr("this unit's faction takes more or less of these damage types, whatever its armor and shields do"))}">${
        mods.map((m) => `<span class="${m.mult > 1 ? "up" : "dn"}">${escHtml(DT(m.type))} ×${escHtml(String(m.mult))}</span>`).join("")}</span>`
    : "";
};

// WHAT CANNOT BE PROC'D ON IT, which is a different line because it is a
// different mechanic. A vulnerability says what a hit DEALS; a status immunity
// says what it PROCS, and it moves the whole distribution rather than the one
// entry — the immune type leaves the denominator and the others take over its
// share of the roll (wiki `Status_Effect` §Status Immunity Interactions). A
// reader who saw only "Heat ×0" would conclude the Heat procs stopped too, and
// they did not.
const enemyStatusImmune = (en) => {
  const im = (en && en.status_immunities) || [];
  return im.length
    ? `<span class="en-vuln" title="${escHtml(tr("these procs cannot land on this unit — the other damage types take over their share of the status roll"))}">${
        im.map((k) => `<span class="dn">${escHtml(DT(k))} ⃠</span>`).join("")}</span>`
    : "";
};

/// …AND THE THIRD LINE, which is neither of the two above. The proc LANDS, and
/// what it does is nothing.
///
/// It has to be its own line because the arithmetic differs: an immune type
/// leaves the status roll and makes every other type MORE likely, while this
/// one keeps its share of the roll and still counts as a type for Condition
/// Overload. Showing them together would tell a reader the opposite of both.
///
/// `cannot_be_frozen` rides here for the same reason and reads the other way —
/// Cold is worth MORE on such a unit, because the ladder never spends itself.
const enemyEffectsNulled = (en) => {
  const out = [];
  const nn = (en && en.nullified_status_effects) || [];
  if (nn.length) {
    out.push(`<span class="en-vuln" title="${escHtml(
      tr("these procs still land and still count for Condition Overload — this unit simply ignores what they do"))}">${
      nn.map((k) => `<span class="dim">${escHtml(DT(k))} ${escHtml(tr("no effect"))}</span>`).join("")}</span>`);
  }
  if (en && en.cannot_be_frozen) {
    out.push(`<span class="en-vuln" title="${escHtml(
      tr("Cold never converts on this unit, so the stacks climb to their cap and STAY — the bonus is up all fight instead of being spent on a 3-second Frozen window"))}">${
      `<span class="up">${escHtml(tr("never Frozen — Cold stacks hold"))}</span>`}</span>`);
  }
  return out.join("");
};

// THE POOLS AT THE FIGHT'S LEVEL, from the engine.
//
// The card and the picker used to print each unit's stats at its OWN base
// level — a Corrupted Heavy Gunner as "700 Health · 500 Armor" — which is a
// number nobody fights: the scenario runs at 9999 Steel Path, where the same
// unit is millions of health and the armour figure has stopped meaning what
// the raw number suggests. Choosing a target on those is choosing on the wrong
// axis (owner, 2026-08-05).
//
// Fetched, not computed here: the level curves belong to the engine and a
// second implementation in JavaScript is a second answer waiting to drift.
// Keyed on the two inputs, so it costs one call per level change and nothing
// per repaint.
let targetStats = { key: null, by: {} };
async function loadTargetStats() {
  const key = `${sim.level}|${sim.steel_path ? 1 : 0}`;
  if (targetStats.key === key) return false;
  const r = await api("/api/targets", { level: sim.level, steel_path: sim.steel_path });
  if (!r || !r.targets) return false;
  const by = {};
  r.targets.forEach((t) => { by[t.id] = t; });
  // Written together with the key: a half-applied cache would serve one
  // level's numbers under another's label.
  targetStats = { key, by };
  return true;
}

// Repaint the target cards' meta line IN PLACE when the numbers land.
//
// Not a re-render: the renderer is what asked for them, so calling it back
// would recurse. One line of text is the whole difference the answer makes,
// and both tabs draw the same card from the same state, so both are patched
// by one selector.
function paintTargetMeta() {
  const en = allEnemies().find((e) => e.id === sim.enemy);
  if (!en) return;
  document
    .querySelectorAll("#sim-target .en-meta, #opt-target .en-meta")
    .forEach((el) => { el.textContent = enemyMeta(en); });
}

// What the target card says under the name. The enemy's own facts, plus what
// it is MADE OF at the level this fight runs at.
function enemyMeta(en) {
  if (!en) return "";
  const bits = [];
  if (en.faction && en.faction !== "unknown") bits.push(tr(cap1(en.faction)));
  // The pools a build has to get through. AT THE FIGHT'S LEVEL when the engine
  // has told us (`loadTargetStats`), falling back to the unit's base numbers
  // before that answer arrives — never a mix of the two, because a row reading
  // "4.9M Health · 500 Armor" would be a lie about both.
  const at = targetStats.by[en.id];
  const src = at && !at.error ? at : en;
  const pools = [
    src.health ? `${Math.round(src.health).toLocaleString()} ${tr("Health")}` : null,
    src.shield ? `${Math.round(src.shield).toLocaleString()} ${tr("Shield")}` : null,
    src.armor ? `${Math.round(src.armor).toLocaleString()} ${tr("Armor")}` : null,
    src.overguard ? `${Math.round(src.overguard).toLocaleString()} ${tr("Overguard")}` : null,
  ].filter(Boolean);
  if (pools.length) {
    // SAY WHICH LEVEL, always. The same list of numbers means something
    // completely different at 9999 than at the unit's base level, and the
    // fallback above can serve either — so the label is not decoration, it is
    // what makes the row readable.
    // "Lv" stays untranslated, the way every other level label in the app is.
    const lv = src === en
      ? `Lv ${en.base_level ?? 1}`
      : `Lv ${sim.level}${sim.steel_path ? " SP" : ""}${at.eximus ? ` ${tr("Eximus")}` : ""}`;
    bits.push(`${lv}: ${pools.join(" · ")}`);
    // What the armour is WORTH — at these levels the raw figure says little
    // and the reduction it buys says everything.
    if (src !== en && at.armor_dr > 0) {
      bits.push(`${Math.round(at.armor_dr * 1000) / 10}% ${tr("damage reduction")}`);
    }
  }
  const head = (en.parts || []).find((p) => p.is_head);
  if (head) bits.push(`${tr("Headshot")} ×${head.multiplier}`);
  // The bare word "Eximus" is now a claim about WHAT YOU ARE SHOOTING and is
  // made above, beside that variant's own pools — there is a switch for it and
  // it defaults on. This line stays only for the case the numbers have not
  // arrived yet, where nothing else would mention the variant at all.
  if (en.can_be_eximus && !targetStats.by[en.id]) bits.push(tr("has an Eximus variant"));
  return bits.join(" · ");
}

// THE TARGET PICKER — the same component as the mod and arcane pickers, on
// the same rule: search matches anything the reader can see. One enemy in the
// roster today, so this is the shape the roster grows into rather than a
// convenience over a two-item list.
function openEnemyPicker(anchor) {
  closePopovers();
  const pop = $("enemy-popover");
  place(pop, anchor);
  const search = $("enemy-search");
  search.value = "";
  search.oninput = () => renderEnemyMenu(search.value);
  renderEnemyMenu("");
  search.focus();
  // Then again with the real numbers. Drawn twice rather than awaited, so the
  // menu opens at once on a cold cache instead of after a round trip — and the
  // first pass is honest either way, because it labels the base level it is
  // actually showing.
  loadTargetStats().then(() => {
    if (pop.classList.contains("open") || pop.style.display !== "none") {
      renderEnemyMenu(search.value);
    }
  });
}

function renderEnemyMenu(query) {
  const menu = $("enemy-menu");
  const q = (query || "").trim().toLowerCase();
  const blob = (e) => [e.name, e.name_en, e.id, e.faction, e.scaling]
    .filter(Boolean).join(" ").toLowerCase();
  const hits = allEnemies().filter((e) => !q || blob(e).includes(q));
  menu.innerHTML = hits.length
    ? hits.map((e) => `<div class="opt ${e.id === sim.enemy ? "sel" : ""}" data-e="${escHtml(e.id)}">
         ${enemyImg(e, "en-thumb")}
         <div class="info"><div class="mn">${escHtml(e.name)}</div>
         <div class="me">${escHtml(enemyMeta(e))}</div>${enemyVuln(e)}${enemyStatusImmune(e)}${enemyEffectsNulled(e)}${enemyCaveat(e)}</div>
       </div>`).join("")
    : `<div class="sim-empty">${escHtml(tr("no enemy matches"))}</div>`;
  menu.querySelectorAll("[data-e]").forEach((el) => el.onclick = () => {
    sim.enemy = el.dataset.e;
    // An explicit "yes, Eximus" cannot follow you onto a unit that has no
    // Eximus variant — the fight would be refused. Dropping it back to null
    // hands the new target its OWN default, which is the elite one wherever
    // there is one. An explicit "no" is legal everywhere and is kept.
    const picked = allEnemies().find((e) => e.id === sim.enemy);
    if (sim.eximus === true && !(picked && picked.can_be_eximus)) sim.eximus = null;
    closePopovers();
    markPresetDirty(); markScenarioDirty();
    renderSim();
    if ($("opt-target")) renderOptEnemy();
  });
}

// Which forms this weapon offers, and the reseed when it does not offer the
// one currently chosen.
//
// The FORMS are the weapon's own (registered in data/weapons, served by
// /api/meta) — not a hardcoded Incarnon triple. The two-form CYCLE is not a
// form but a MODE over them, so it is listed first and only when the weapon
// has something to transform into (`has_cycle`). A weapon with one form and
// no cycle has nothing to choose, so no selector is drawn.
//
// The cycle is offered — and defaulted to — whenever the WEAPON has one,
// installed perk or not (user, 2026-08-01). It stays honest because the sim
// falls back to the base form when the unlock is missing, and it stays
// STABLE, which is the point: re-seeding the choice every time tier 1 is
// touched would move the selection under someone who had already made it.

// The arena's target actor: its name, and its portrait when it has one. The
// dot stands in for a unit with no art rather than sitting beside it — two
// markers for one actor is one too many.
function setArenaEnemy(en) {
  $("arena-ename").textContent = en ? en.name : "Enemy";
  const src = IMG(en && en.image);
  const img = $("arena-eimg");
  img.hidden = !src;
  if (src) img.src = src;
  $("arena-edot").hidden = !!src;
}


// ---- WARFRAME ABILITY BUFFS (scenario section 3) ------------------------
//
// EARLY ACCESS, and the block says so on screen (owner, 2026-08-08). Today
// you type an Ability Strength; when frames land it comes from the frame and
// the duration from Ability Duration. Nothing else about these buffs changes
// then, which is why the definitions live in `data/abilities/` and only their
// two INPUTS are here.
//
// It is the SCENARIO's, not the build's: a thing done TO this weapon for a
// while. That is what puts it in section 3 beside the wielder, what carries it
// into the optimizer read-only, and what keeps it off the board — a board row
// is a statement about the weapon, and no ruler casts Roar.
const wfAbilities = () => (META && META.abilities) || [];
// DE'S OWN NAME for the ability, never a translation of ours (the house rule).
// The FRAME keeps its English name because DE's Chinese client does too.
const wfName = (a) => ((I18N && I18N.abilities) || {})[a.id] || a.name;
const wfPick = (id) => (sim.abilities || []).find((a) => a.id === id);
// The strength-scaled value, which is the number the card shows. Linear, and
// the engine agrees by construction: `abilities_data::at_strength` is the same
// multiply, and `check_wf_buffs.mjs` asserts the screen and the sim match.
// …AND THE ONES THE KNOB DOES NOT MOVE. Energized Munitions' ammo efficiency is
// a flat 75% — its wiki row carries no Ability Strength icon — so multiplying it
// here would print a number the game never gives, at every strength but 100%.
// The server states which per ability (`scales_with_strength`), so the page
// cannot disagree with the sim about it.
const wfValue = (a) =>
  a.value * (a.scales_with_strength === false ? 1 : (Number(sim.ability_strength) || 0));

// WHICH PICKS ARE ACTUALLY RUNNING. Same family, only the strongest — the
// wiki's own rule ("Multiple Freeze Forces do not stack; the buff with the
// highest Ability Strength will take effect") and the owner's ask for Roar vs
// Roar (Helminth). Computed here TOO, rather than only in the engine, because
// a page that showed both as active would be lying about a number it printed.
function wfRunning() {
  const best = new Map();
  for (const p of sim.abilities || []) {
    const def = wfAbilities().find((a) => a.id === p.id);
    if (!def) continue;
    const cur = best.get(def.family);
    if (!cur || wfValue(def) > wfValue(cur)) best.set(def.family, def);
  }
  return new Set([...best.values()].map((a) => a.id));
}

// WHAT IT IS WORTH, at the strength you set — the number, on its own, big
// enough to read (owner, 2026-08-08). A catalogue that showed only the
// ability's name would make you do the multiply the sim is doing. THE ELEMENT
// THIS BUFF IS ACTUALLY SET TO — the picked one where the ability
// offers a choice (Resupply's gear wheel of ten), its own otherwise.
const wfElement = (a) => {
  const p = wfPick(a.id);
  return (p && p.element) || a.element;
};
// `DT` is the one place a damage type is named, and it reads the locale — this
// used to reach into META and print `corrosive` on a Chinese page.
const wfElementName = (id) => DT(id);

function wfValueLabel(a) {
  const pct = Math.round(wfValue(a) * 1000) / 10;
  if (a.kind === "add_element" || a.kind === "extra_hit") {
    return `+${pct}% ${wfElementName(wfElement(a))}`;
  }
  return `+${pct}%`;
}

// …and WHERE it lands, which is the half a number cannot say. Two buffs both
// reading "+50%" are worth different amounts on a DoT weapon, and this line is
// the difference.
function wfEffectLine(a) {
  if (a.kind === "faction_damage") {
    return tr("faction damage — the bracket a Bane mod is in, so a status tick takes it twice");
  }
  if (a.kind === "final_damage") {
    return tr("damage on its own multiplier — applied once, to the hit and to the status alike");
  }
  // AN EXTRA HIT IS NOT A MULTIPLIER, and the line has to say so or the card
  // reads as "+26% damage" — which is not what it is worth. It is a second
  // damage instance, so it takes the faction bonus and the headshot multiplier
  // a SECOND time, and 26% on the card is more like 40% on the number.
  if (a.kind === "extra_hit") {
    return tr("a second damage instance, not a multiplier — it takes faction damage and the headshot multiplier one more time than the hit it copies");
  }
  // NOT A DAMAGE BRACKET AT ALL. What it buys is reloads not taken, so on a
  // weapon that never reloads it is worth nothing and the line has to say
  // where to look instead of implying a damage number.
  if (a.kind === "ammo_efficiency") {
    return tr("ammo efficiency — it does not touch damage: it divides what a shot costs the magazine, so what it buys is reloads not taken. Multiplicative with other ammo-efficiency sources");
  }
  return tr("added on top and NOT combined with the weapon's own elements");
}

function renderWfBuffs(host, readonly) {
  const box = $(host);
  if (!box) return;
  const list = wfAbilities();
  if (!list.length) { box.innerHTML = ""; return; }
  const running = wfRunning();
  const strength = Math.round((Number(sim.ability_strength) || 0) * 100);
  // THE TARGET GETS A SAY. A Demolisher pulses every 5 s and dispels every
  // Warframe ability in range — so against one, nothing ticked here is up, and
  // the sim scores it that way. A section that let you tick Roar and quietly
  // ignored it would be the worst kind of wrong: a number you cannot explain.
  const nulled = (allEnemies().find((e) => e.id === sim.enemy) || {}).nullifies_abilities;
  const rows = list.map((a) => {
    const pick = wfPick(a.id);
    const on = !!pick;
    // SUPERSEDED, not off: you ticked it and something stronger is running.
    // Saying nothing here is how a player ends up believing they have +80%.
    const dead = on && !running.has(a.id);
    return `<div class="wfb${on ? " on" : ""}${(dead || (on && nulled)) ? " dead" : ""}">
      <label class="check wfb-pick"><input type="checkbox" data-wf="${escHtml(a.id)}"${on ? " checked" : ""}${readonly ? " disabled" : ""}>
        <span class="wfb-n">${escHtml(wfName(a))}</span>
        <span class="wfb-f">${escHtml(tr(a.frame))}</span></label>
      <div class="wfb-v">${escHtml(wfValueLabel(a))}</div>
      ${/* A CHOSEN ELEMENT, where the ability offers one. Drawn from the data's
            own list in the game's own order, so the day a member gains or loses
            a choice this follows without being told. */ ""}
      ${(a.elements || []).length
        ? `<label class="wfb-el" title="${escHtml(tr("this ability lets you pick the element — the gear wheel in game"))}">${
            escHtml(tr("element"))} <select data-wfel="${escHtml(a.id)}"${(!on || readonly) ? " disabled" : ""}>${
            a.elements.map((e) => `<option value="${escHtml(e)}"${
              e === wfElement(a) ? " selected" : ""}>${escHtml(wfElementName(e))}</option>`).join("")
          }</select></label>`
        : ""}
      <div class="wfb-e">${escHtml(wfEffectLine(a))}</div>
      ${/* WHAT IT DOES NOT DO, in the same chips a mod and an arcane card use —
            `notModeledLines` reads `unmodeled_effects` and `live_bugs` off any
            object, and an ability publishes both under those names. The owner
            debugs by reading the card, so a Bullet Attractor this sim has
            nothing to point at has to say so HERE (2026-08-08). */ ""}
      <div class="wfb-u">${notModeledLines(a).join("")}</div>
      ${dead ? `<div class="wfb-dead">${escHtml(tr("a stronger buff of the same kind is running — this one adds nothing"))}</div>` : ""}
      ${on && nulled ? `<div class="wfb-dead">${escHtml(tr("this target dispels it — nothing ticked here is running against it"))}</div>` : ""}
      ${a.url ? `<a class="wfb-w" href="${escHtml(a.url)}" target="_blank" rel="noopener">${escHtml(tr("wiki"))} ↗</a>` : ""}
    </div>`;
  }).join("");
  const sub = $(host === "sim-wfbuffs" ? "wfbuff-sub" : "");
  if (sub) {
    sub.textContent = nulled
      ? tr("none — this target dispels them")
      : running.size
        ? `${running.size} ${tr("running")}`
        : tr("none — the weapon on its own");
  }
  box.innerHTML =
    `<div class="wfb-head">
       <label title="${escHtml(tr("your Warframe's Ability Strength, as the arsenal shows it — every value below is this times the wiki's max-rank number"))}">${escHtml(tr("Ability Strength %"))}
         <input type="number" id="${host}-str" min="0" max="1000" step="1" value="${strength}"${readonly ? " disabled" : ""}></label>
       <span class="wfb-early">${escHtml(tr("early access — every buff runs the whole engagement for now, and this block moves onto the Warframe itself once frames land"))}</span>
     </div>
     ${nulled ? `<div class="wfb-null">${escHtml(
        tr("this target nullifies Warframe abilities — it pulses every 5 seconds and dispels everything in range, so none of these are running and the sim scores it that way"))}</div>` : ""}
     <div class="wfb-grid">${rows}</div>`;
  if (readonly) {
    box.querySelectorAll("input").forEach((el) => {
      el.disabled = true;
      el.title = tr("edit this in the Simulator");
    });
    return;
  }
  const touched = () => { markScenarioDirty(); renderSim(); };
  const str = $(`${host}-str`);
  if (str) str.addEventListener("change", () => {
    sim.ability_strength = Math.max(0, Number(str.value) || 0) / 100;
    touched();
  });
  box.querySelectorAll("[data-wfel]").forEach((el) => el.addEventListener("change", () => {
    const p = wfPick(el.dataset.wfel);
    if (p) p.element = el.value;
    touched();
  }));
  box.querySelectorAll("[data-wf]").forEach((el) => el.addEventListener("change", () => {
    const id = el.dataset.wf;
    sim.abilities = (sim.abilities || []).filter((a) => a.id !== id);
    // TICKING IT OPENS IT AT THE WIKI'S OWN DURATION, not at "whole fight":
    // the honest default for "I cast Roar" is one Roar, and the whole-fight
    // box is the deliberate other question.
    // `secs: null` = the whole engagement. The only thing the page offers
    // today, and the honest question to ask of a build: what is this weapon
    // worth UNDER the buff, rather than around it.
    if (el.checked) {
      const def = wfAbilities().find((a) => a.id === id);
      // AN EXPLICIT ELEMENT from the first tick, where there is a choice: a
      // pick that omits it is answered by the definition's first entry, and a
      // player reading the card should see the same thing the sim runs.
      const first = def && (def.elements || [])[0];
      sim.abilities.push(first ? { id, secs: null, element: first } : { id, secs: null });
    }
    touched();
  }));
}

/// THE RUN COUNT, on its own — the page's, not the fight's.
///
/// It is GLOBAL and it stays put: switching weapons, switching fights, and
/// opening an OFFICIAL ruler all leave it alone (owner, 2026-08-13). The
/// lock that pins a board fight's terms does not reach it, because how
/// long you are willing to wait is not one of them.
///
/// The METRIC is the opposite and sits in the block above: KPM is a term of the
/// scenario, so an official ruler pins it and this page may not argue.
/// THE BUILDER'S STEP NUMBERS, recomputed from what is on screen.
///
/// A block whose axis this weapon does not have is hidden, and the visible ones
/// are then 1..n in document order. Only badges that are already a NUMBER are
/// touched: `Σ`, `≡` and `▶` mark panels that are not steps in the build and
/// say so by not being counted.
/// The BUILDER's blocks, in document order. Named rather than selected by
/// class, because `.config-page` also holds the Sim, Rivens, Enemies and
/// Optimizer pages — they are TABS with their own numbering, and a sweep over
/// the class renumbered the Rivens editor as step 5 of building a gun.
const BUILDER_BLOCKS = [
  "mode-block", "mod-block", "arcane-block", "evo-block", "element-block",
];

function renumberBlocks() {
  let n = 0;
  BUILDER_BLOCKS.forEach((id) => {
    const b = $(id);
    if (!b || b.hidden) return;
    const badge = b.querySelector(".bh .n");
    if (!badge) return;
    badge.textContent = String(++n);
  });
}

function renderSimRuns() {
  const box = $("sim-runs-block");
  if (!box) return;
  box.innerHTML =
    `<label title="${escHtml(tr("how many times to replay this fight HERE. Not part of the scenario: it follows you across fights and weapons, and an official ruler does not pin it. The boards themselves are scored at 1,000 by the server whatever this says"))}">${
      escHtml(tr("Runs"))} <input type="number" id="sim-runs-input" min="1" max="20000" step="10" value="${simRuns()}"></label>` +
    `<span class="sim-hint">${escHtml(tr("yours, not the fight's — the boards score at 1,000"))}</span>`;
  const el = $("sim-runs-input");
  el.addEventListener("change", () => {
    setSimRuns(el.value);
    el.value = String(simRuns());
  });
}

function renderSim() {
  if (!META) return;
  renderSimBuild();
  const enemies = allEnemies();
  const en = enemies.find((e) => e.id === sim.enemy) || enemies[0];
  renderScenarioFields({ target: "sim-target", technique: "sim-technique",
    limits: "sim-limits", extra: "sim-extra", run: "sim-run" });
  renderSimRuns();
  renderWfBuffs("sim-wfbuffs", false);
  // …AND THE OPTIMIZER'S COPY, from the same call. It shows the SIMULATOR's
  // fight, so it is repainted whenever that fight is redrawn rather than when
  // its own tab happens to be entered — a tab that repaints only on arrival
  // shows the buffs you had when you last arrived.
  renderWfBuffs("opt-wfbuffs", true);
  renderScenarioBar();
  setArenaEnemy(en);
  $("sim-sub").textContent = "current build vs the enemy";
  renderSimBuffs();
  lockOfficialScenario();
  renderBoardConsent();
}

// ---- THE BOARD: consent, then submission -------------------------------
//
// WHEN THE ASK HAPPENS, and why it is here rather than on load: the first time
// you finish a run under the OFFICIAL scenario. Only then is there any context
// for the question — a score is on screen and "should this build go on the
// board" is a sentence that means something. Asking at startup would be a
// modal before you have done anything, which is a dialog people click away
// rather than a disclosure. (It is also inline: `prompt`/`alert`/`confirm` are
// blocked in this project.)
//
// WHAT TRAVELS, stated because it is short enough to check: the weapon, its
// mods, evolutions and arcanes, and which benchmark. No account, no
// identifier, no riven (they are out of the benchmark entirely), and none of
// the names you gave anything. NO SCORE either — the board scores builds
// itself, which is what makes a row reproducible and a forged number
// pointless.
const BOARD_CONSENT = "wfsim-board-consent";   // "yes" | "no" | absent = never chosen

// DEFAULT ON (owner, 2026-08-05) — and the important word is not "on", it is
// NOT SILENT. Opt-out versus opt-in is a policy choice the owner gets to make;
// silent versus stated is a trust choice, and the board's whole value is that
// its numbers are believable. One screenshot captioned "wfsim 偷偷上传你的配装"
// costs more than every submission it would ever gain.
//
// So the default is yes, and the line saying so is on screen from the moment
// the official scenario is active — before any run, with a one-click opt-out
// beside it. `check_official.mjs` asserts that pairing rather than asserting
// nothing leaves: what has to be true is that nothing leaves UNSAID.
const boardConsent = () => {
  try { return localStorage.getItem(BOARD_CONSENT) || "yes"; } catch (_) { return "yes"; }
};
/// Has the player actually decided, as against inheriting the default? Only
/// this tells the notice apart from the settled state.
const boardConsentChosen = () => {
  try { return localStorage.getItem(BOARD_CONSENT) !== null; } catch (_) { return false; }
};
const setBoardConsent = (v) => {
  try { localStorage.setItem(BOARD_CONSENT, v); } catch (_) { /* private mode */ }
  renderBoardConsent();
};

/// The submission itself: the BUILD, and nothing else about you.
function boardPayload() {
  const bench = (scenarioNamed(activeScenario) || {}).builtin;
  if (!bench) return null;
  return {
    benchmark: bench,
    weapon: $("weapon").value,
    // HOW IT IS PLAYED, and it has to travel or the dimension is fed by
    // nothing. The scorer's fallback for a mode-less submission is "the cycle
    // where there is one", which is a MIGRATION rule for rows submitted before
    // the dimension existed — and while this field was missing it was the only
    // rule in play, so every Incarnon weapon's row said `cycle` and no board
    // could ever hold a base-form Torid. Measured on the published boards:
    // 62 cycle rows and 41 base ones, and not one weapon with both — every
    // Incarnon weapon cycle, every other one base, which is the fallback's
    // signature rather than anybody's choice.
    mode,
    // THE EIGHT MAIN SLOTS, and the exilus one is DROPPED here (owner,
    // 2026-08-05). It has to happen on this side: the payload is a flat list
    // with no slot positions, and an exilus-eligible mod is legal in a MAIN
    // slot, so nothing downstream can tell which entry came out of the exilus
    // slot. Only the
    // page knows, because only the page has the slots.
    //
    // Sending all nine is what this did until now, which refused exactly the
    // wrong people: a player who fills their exilus slot sent 9 mods and was
    // turned away for not being a complete build.
    //
    // AS PLACED, not sorted. Mods combine elements in the order they sit in,
    // so sorting here submitted a build the player never made — and on the
    // Torid that is 12,424 DPS against 46,583 (measured 2026-08-04). The
    // scorer canonicalises with the pool in front of it, which is the only
    // place that can tell an elemental mod from any other.
    mods: mainSlots().filter((s) => s.mod).map((s) => s.mod),
    evolutions: Object.values(evoSel).filter(Boolean),
    arcanes: arcanes.slice(),
    // THE PROGENITOR ELEMENT, for an adversary weapon. The ELEMENT only: the
    // ruler scores every row at the roll's maximum, so the percentage is not a
    // row's to state — every player can Valence-fuse to it, which makes it
    // investment rather than a choice (the same rule that scores every row at
    // full Forma).
    valence: valence.element,
  };
}

let boardState = "";   // "" | "sent" | "failed"

/// How many mods a board build is, from the ENGINE via META — never a literal
/// here. The rule is `builds::validate_for_board`; this is the page repeating
/// what it was told so it can explain itself before sending nothing.
const boardBuildMods = () => (META || {}).board_build_mods || 8;
/// The slots a benchmark build is made of: the main ones. `slots` holds nine —
/// the exilus slot is the last — and the benchmark does not count it.
const mainSlots = () => slots.slice(0, boardBuildMods());
/// Filled main slots. The exilus slot is not part of the answer either way.
const buildMods = () => mainSlots().filter((s) => s.mod).length;

/// WHAT THE ACTIVE BENCHMARK ADMITS — its own `build` block, from META. Not a
/// constant here: admission is the benchmark's, so a second ruler answers
/// differently and this page must not assume otherwise (2026-08-05).
const boardRequirement = () => {
  const id = (scenarioNamed(activeScenario) || {}).builtin;
  const b = (META.benchmarks || []).find((x) => x.id === id);
  return (b && b.build) || {};
};

/// EVERYTHING THIS BENCHMARK ASKS FOR THAT THIS BUILD LACKS, as sentences.
///
/// "Full" is PER WEAPON: eight main slots for everything, but the evolution
/// tiers and arcane seats THIS weapon actually has. A weapon with nothing to
/// fill is complete by having filled it — a sentinel weapon seats no arcane, an
/// ordinary rifle has no evolutions — which is what lets one rule cover a
/// roster of different shapes.
function buildShortfalls() {
  const req = boardRequirement();
  const w = weaponInfo($("weapon").value) || {};
  const out = [];
  if (req.mods === "full" && buildMods() !== boardBuildMods()) {
    out.push(tr("{n} of {m} mods").replace("{n}", buildMods()).replace("{m}", boardBuildMods()));
  }
  if (req.evolutions === "full") {
    const want = w.evo_tiers || 0;
    const have = Object.values(evoSel).filter(Boolean).length;
    if (have !== want) {
      out.push(tr("{n} of {m} evolutions").replace("{n}", have).replace("{m}", want));
    }
  }
  if (req.arcanes === "full") {
    const want = (w.arcane_pools || []).length;
    const have = arcanes.filter((a) => a && a !== "none").length;
    if (have !== want) {
      out.push(tr("{n} of {m} arcanes").replace("{n}", have).replace("{m}", want));
    }
  }
  return out;
}

/// THE WEAPON AS FAR AS THIS BENCHMARK COUNTS IT. Whether the exilus slot is
/// filled is irrelevant — a build with one is not more complete, and a build
/// without one is not less.
const buildIsComplete = () => buildShortfalls().length === 0;
/// ...and is there an exilus mod that will be left behind? Worth saying, since
/// the number the board reports will not be the number on screen.
const hasExilusMod = () => slots.length > boardBuildMods() && !!slots[boardBuildMods()].mod;

async function offerBoardSubmit() {
  if (!officialScenarioActive()) return;      // only the official ruler feeds the board
  if (officialBuildActive()) return;          // a board row does not resubmit itself
  if (boardConsent() !== "yes") { renderBoardConsent(); return; }
  // INCOMPLETE BUILDS ARE NOT SENT, and the panel says so rather than letting
  // the server refuse in silence. Without this the default-on setting would
  // fire a request on every first visit — the default build is empty — and the
  // player would never learn why nothing appeared on the board.
  if (!buildIsComplete()) { renderBoardConsent(); return; }
  const body = boardPayload();
  if (!body) return;
  try {
    // A REAL fetch, not `api()`. Every other endpoint is answered by the engine
    // — locally by the dev server, in the browser by the wasm worker — and the
    // board is the one thing neither of them can answer: it is a service, not a
    // calculation. Routing it through `api()` would hand the path to a worker
    // that has never heard of it.
    //
    // SAME ORIGIN, deliberately. A separate api domain is a second DNS name and
    // a second thing that can be blocked, which is the failure the art rule was
    // written about ("unreliable to blocked from mainland China, i.e. precisely
    // where the players are"). The board lives under this site's own origin.
    const res = await fetch("/api/board/submit", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    boardState = res.ok ? "sent" : "failed";
  } catch (_) {
    // Never an error dialog: a board that is unreachable is not a failed run.
    boardState = "failed";
  }
  renderBoardConsent();
}

function renderBoardConsent() {
  const box = $("board-consent");
  if (!box) return;
  // TWO REASONS NOTHING IS SENT, and they used to collapse into one silence.
  // A board ROW explains itself elsewhere ("it is already a row on the board"),
  // but a player on their OWN fight got no box at all — so someone who built a
  // scenario, ran it, and watched the board never learn why nothing appeared
  // (player report via the owner, 2026-08-10).
  if (officialBuildActive()) { box.hidden = true; return; }
  if (!officialScenarioActive()) {
    box.hidden = false;
    box.innerHTML = `<span class="board-state">${escHtml(
      tr("the board only measures its own fight — runs under a scenario of your own are yours alone, and nothing is sent"))}</span>`;
    return;
  }
  box.hidden = false;
  const c = boardConsent();
  // THE FLOOR IS A SUFFIX, NOT A REPLACEMENT. While the build is half-built the
  // standing policy is still the thing a reader most needs to know — replacing
  // it with "this one is not sent" would state the exception and hide the rule,
  // which is the same silence the default-on setting exists not to have.
  const short = c === "yes" ? buildShortfalls() : [];
  const floorNote =
    short.length
      ? ` <span class="board-state">` +
        escHtml(tr("{what} — the board takes a weapon built as far as it goes, so this one is not sent")
          .replace("{what}", short.join(tr(", ")))) +
        `</span>`
      : c === "yes" && hasExilusMod()
        ? ` <span class="board-state">` +
          escHtml(tr("your exilus mod is not part of a benchmark build — it is left out of what is sent, so the board's number will differ from this one")) +
          `</span>`
        : "";
  if (!boardConsentChosen()) {
    // THE DEFAULT, STATED. Not a question — the answer is already yes — so it
    // reads as what will happen and what it contains, with the way out next to
    // it. Asking would be dishonest when the default has already decided.
    box.innerHTML =
      `<b>${escHtml(tr("Runs here are added to the official board."))}</b> ` +
      escHtml(tr("What is sent: the weapon and its mods, evolutions and arcanes. Nothing else — no account, no identifier, no names you chose, and no score (the board measures builds itself). The board takes a weapon built as far as it goes: every main slot filled, exilus not counted.")) +
      // WHEN, on the FIRST visit too — this is the branch a new player reads,
      // and it is the one that was silent about the twenty minutes. Saying it
      // only after the consent had been chosen told the fact to everyone except
      // the person meeting the board for the first time.
      ` ${escHtml(tr("A run appears on the board within about 20 minutes, at most 40 — it is re-scored on a schedule, not the instant you send it."))}` +
      floorNote +
      ` <button class="ghost-btn small" id="board-no">${escHtml(tr("don't submit"))}</button>`;
    $("board-no").onclick = () => setBoardConsent("no");
    return;
  }
  // WHEN, not just whether. A submission is stored the moment it is sent and
  // the board is re-scored on a schedule, so "sent" and "on the board" are
  // twenty minutes apart — long enough that a player concludes it failed. The
  // number is the workflow's own: three runs an hour, and GitHub's scheduler
  // slips about fifteen minutes whatever minute is named, so the honest bound
  // is "usually 20, sometimes 40".
  const state = c === "yes"
    ? (boardState === "failed"
        ? tr("could not reach the board — nothing was sent")
        : boardState === "sent"
          ? tr("sent — the board re-scores every 20 minutes, so it appears there within about 20, at most 40")
          : tr("builds you run here are submitted — the board re-scores every 20 minutes, so a run appears there within about 20, at most 40"))
    : tr("nothing is sent from here");
  box.innerHTML =
    `<span class="board-state">${escHtml(state)}</span>` + floorNote + ` ` +
    `<button class="ghost-btn small" id="board-flip">${escHtml(c === "yes" ? tr("stop submitting") : tr("start submitting"))}</button>`;
  $("board-flip").onclick = () => setBoardConsent(c === "yes" ? "no" : "yes");
}

// The official BUILD, on screen. Same contract as the scenario's lock, but the
// build editor is mostly CLICK HANDLERS on divs (a slot, a polarity, an
// evolution tile) rather than form controls, and `disabled` means nothing to a
// div. `pointer-events: none` on the whole region is the honest equivalent:
// it covers everything the region can ever grow, including a control nobody
// has written yet.
function lockOfficialBuild() {
  const on = officialBuildActive();
  // MODE IS PART OF THE BUILD, so it locks with the build. It was the one
  // control on a read-only board row that stayed live, and the consequence was
  // silent in both directions: switching a #1 Felarx to its base form ran the
  // base form, wrote nothing (`markPresetDirty` refuses an official build), and
  // sent nothing (`offerBoardSubmit` refuses one too) — so a player testing the
  // base form several times saw no row appear and nothing on screen said why
  // (owner, 2026-08-09).
  ["mod-block", "arcane-block", "evo-block", "mode-block"].forEach((id) => {
    const b = $(id);
    if (!b) return;
    b.classList.toggle("locked-hard", on);
    // …AND IT SAYS WHY, on the block a player is trying to click. A slot that
    // simply does not react teaches nothing; this names the reason and the way
    // out, and clicking anywhere in the block takes it.
    if (on) {
      b.title = tr("this is a benchmark row — copy it to edit");
      b.onclick = () => copyActivePreset(buildBarCfg());
    } else {
      b.removeAttribute("title");
      b.onclick = null;
    }
  });
  const note = $("build-official");
  if (!note) return;
  note.hidden = !on;
  if (!on) return;
  const row = (buildNamed(activePreset) || {}).board || {};
  const bench = (META.benchmarks || []).find((x) => x.id === row.benchmark);
  // ACTION FIRST. This note used to open with what the build IS and mention
  // copying as a clause, pointing at a ⧉ chip somewhere else on the page — a
  // reader who wants to change something needs the VERB and a thing to click
  // (owner, 2026-08-09). So: what cannot be done here, the one action that
  // fixes all of it, and a real button — which the scenario's own read-only
  // note has had all along.
  const parts = [
    `<b>${escHtml(tr("Benchmark build — read-only"))}</b>`,
    escHtml(tr("It is already a row on the board, so nothing here can be edited and its runs are not submitted. Copy it and everything opens up — mods, arcanes, evolutions and the mode — and what you run goes to the board as your own entry.")),
  ];
  // WHICH BENCHMARK, stated rather than implied (owner, 2026-08-04). A board
  // figure means nothing without the ruler that produced it, and "#1" says
  // even less — so the name of the scenario is part of the build, not a
  // caption on the bar it happens to sit in.
  parts.push(
    escHtml(tr("measured under")) +
      ` <span class="official-def">${escHtml(benchmarkName(row.benchmark))}</span>`,
  );
  if (row.score != null) {
    // LABELLED WITH THE BENCHMARK'S OWN METRIC, not a hardcoded one. The
    // number is published in whatever the benchmark declares — a `dps`
    // benchmark would have read "kill rate" here, and a kill-progress figure
    // read as a kill RATE overstated every row by the length of the fight
    // until 2026-08-04.
    const unit = ((bench || {}).scenario || {}).metric === "dps" ? tr("DPS") : tr("kill rate");
    // `shown` is FORMATTED BY THE SCORER (`boards_data::format_score`): at
    // least four significant figures and at least four decimals. The rule is
    // not reimplemented here — the page prints what the record says, so a
    // change to it cannot show one thing in the yaml and another on screen.
    // `score` is the fallback for a board written before the field existed.
    const shown = row.shown || Number(row.score).toFixed(4);
    parts.push(`<span class="official-def">${escHtml(shown)} ${escHtml(unit)}</span>`);
  }
  // NOT the Forma cost: the builder's own header already states capacity and
  // Forma for whatever build is loaded, and this build IS loaded. Two places
  // showing one number is how they come to disagree.
  if (row.source === "seed") {
    parts.push(`<span class="official-seed">${escHtml(tr("seeded by the optimizer — not yet a player submission"))}</span>`);
  }
  note.innerHTML = parts.join(" · ") +
    ` <button class="ghost-btn small" id="build-copy">⧉ ${
      escHtml(tr("copy it to edit"))}</button>`;
  const cp = $("build-copy");
  if (cp) cp.onclick = () => copyActivePreset(buildBarCfg());
}

// The official scenario, ON SCREEN: every control in the fight goes inert and
// the note says why and what to do instead.
//
// This is the VISIBLE half of read-only, and deliberately only the visible
// half — the rule is enforced in `markScenarioDirty`, which is where a write
// would actually happen. A disabled input that auto-save still read would be a
// lie told twice, so the two are separate on purpose.
function lockOfficialScenario() {
  const note = $("sim-official");
  const on = officialScenarioActive();
  const boxes = ["sim-target", "sim-technique", "sim-wfbuffs", "sim-limits", "sim-run", "sim-buffs"]
    .map((id) => $(id)).filter(Boolean);
  boxes.forEach((b) => {
    b.classList.toggle("locked", on);
    b.querySelectorAll("input,select,button,textarea").forEach((el) => {
      // ONLY UNLOCK WHAT THIS LOCKED. The first shape remembered each
      // element's prior state and restored it, which is a bookkeeping problem
      // that has to survive every re-render and did not: a second lock pass
      // recorded "was disabled" for an element this had just disabled, so the
      // unlock left it inert and a COPIED scenario could not be edited — the
      // one thing copying is for.
      //
      // Marking instead is idempotent by construction. An element disabled for
      // its OWN reason (a sentinel's infinite-ammo box, ticked and disabled
      // whatever scenario is open) is never marked, so it is never re-enabled
      // here and the page cannot contradict the mechanic.
      if (on) {
        if (!el.disabled) { el.disabled = true; el.dataset.officialLock = "1"; }
      } else if (el.dataset.officialLock) {
        el.disabled = false;
        delete el.dataset.officialLock;
      }
    });
  });
  if (!note) return;
  note.hidden = !on;
  if (on) {
    // A DOOR, NOT A WALL. This is now the scenario a first-time visitor lands
    // on, so the first thing they see is a panel of greyed-out controls — which
    // reads as broken until you know why. The note has always explained it and
    // pointed at the ⧉ on the chip; a chip they have not learned to look at yet
    // is not a route. The button does the same copy, where the locked controls
    // are (owner, 2026-08-05).
    note.innerHTML =
      `<b>${escHtml(tr("Official test scenario"))}</b> — ` +
      escHtml(tr("the same fight on every weapon, so results can be compared. It cannot be edited.")) +
      ` <span class="official-def">${escHtml((scenarioNamed(activeScenario) || {}).name || "")}</span>` +
      ` <button class="ghost-btn small" id="sim-official-copy">⧉ ${escHtml(tr("edit a copy of this fight"))}</button>`;
    const cp = $("sim-official-copy");
    // The SAME copy the chip's ⧉ performs — `copyActivePreset` with the
    // scenario bar's own config — so there is one implementation of "make me an
    // editable copy" and it cannot drift from the bar.
    if (cp) cp.onclick = () => copyActiveScenario();
  }
}

// Section 2 — one card per configurable buff of the current build (from the
// last /api/panel `buffs`). Each: initial stacks (stepper / on-off) + lock.
// Missing configs default to the buff's `default_*`; ids no longer present are
// dropped from the payload (kept in `sim.buffs` for preset round-trips).
function syncBuffConfig(list, cfg) {
  list.forEach((b) => {
    if (!cfg[b.id]) cfg[b.id] = { stacks: b.default_stacks, locked: b.default_locked };
    else if (!b.uncapped) cfg[b.id].stacks = Math.min(cfg[b.id].stacks, b.max_stacks);
  });
}

// Shared buff-card renderer (Sim panel + Optimizer scope). `list` = the buff
// metadata; `cfg` = the mutated config map.
/// A buff card's name, in the display language.
///
/// The server names a buff after the mod or arcane that grants it — in
/// ENGLISH, because English is the source everywhere. The overlay that
/// translates that name is already on the client (every mod and arcane in
/// META carries `name_en`), so the lookup is by English name and the grant
/// suffix goes through the ordinary UI table. Invisible while the panel only
/// ever showed a build's own two or three cards; the all-potential view shows
/// eleven at once.
function buffCardName(name) {
  const [head, tail] = String(name).split(" (");
  // EVOLUTIONS grant buffs too (Overwhelming Attrition, Lethal Rearmament),
  // and they were missing from this lookup — so those two cards were the only
  // ones on the page still in English (user, 2026-08-03).
  const evos = (weaponInfo($("weapon").value).evolutions || [])
    .flatMap((tier) => tier.options || []);
  const owner = [...(META.mods || []), ...(META.arcanes || []), ...evos,
    ...Object.values(META.mod_pools || {}).flat()]
    .find((x) => (x.name_en || x.name) === head);
  const label = owner ? owner.name : head;
  return tail ? `${label} (${tr(tail.replace(/\)$/, ""))})` : label;
}

function renderBuffCards(box, list, cfg, have, opts = {}) {
  if (!box) return;
  syncBuffConfig(list, cfg);
  if (!list.length) {
    box.innerHTML = `<div class="sim-empty">no configurable buffs here.</div>`;
    return;
  }
  const card = (b) => {
    const c = cfg[b.id];
    // ONE control for every buff, a toggle included (user, 2026-08-02): a
    // one-stack buff reads "1 / 1" like the rest instead of a checkbox that
    // said "active" and meant the same thing in different words.
    // WHERE THE RUN STARTS, not what the buff is worth. A timed buff opens at
    // 0 because the modelled fight is "in it a while, but not in contact for
    // the last few seconds" — it is earned back on its own trigger. Only a
    // permanent buff opens full, because a lull cannot take it away.
    const startWhy = b.permanent
      ? tr("permanent — nothing grants or decays it, so it holds all run")
      : tr("stacks the run STARTS with. 0 = earned in-fight on this buff's own trigger, which is what a fight that has not been in contact for a few seconds looks like");
    // UNCAPPED buffs exist: Secondary Enervate ramps a stack per hit with no
    // ceiling until a big crit wipes it. `/ ∞` is the honest maximum, and the
    // input takes no `max` — clamping it to a number we invented would be the
    // one place the UI disagreed with the mechanic (user, 2026-08-03).
    const cap = b.uncapped ? "∞" : b.max_stacks;
    const ctl = `<span class="bstep" title="${escHtml(startWhy)}"><input type="number" data-b="${b.id}" data-f="stacks" min="0"${b.uncapped ? "" : ` max="${b.max_stacks}"`} value="${c.stacks}"><span class="bmax">/ ${cap}</span></span>`;
    // NOT "lock" (user, 2026-08-02): that read as "freeze this buff", so
    // locking one at zero looked like a way to switch it off forever. It only
    // removes the TIMEOUT — the count still starts where it is set and still
    // climbs on every trigger.
    const lock = b.permanent
      ? `<label class="block-lock dis" title="${escHtml(tr("permanent stacks — they never decay and cannot build in-sim, so the count holds for the whole run"))}"><input type="checkbox" checked disabled> ${escHtml(tr("no timeout"))}</label>`
      : `<label class="block-lock" title="${escHtml(tr("the stacks never expire — they still start where you set them and still build on every trigger"))}"><input type="checkbox" data-b="${b.id}" data-f="locked" ${c.locked ? "checked" : ""}> ${escHtml(tr("no timeout"))}</label>`;
    // In the WIDER view, a buff the build does not carry is still settable —
    // it just says so, so the panel never reads as "this is active now".
    const off = have && !have.has(b.id);
    // What one stack count buys, when the source grants more than one thing
    // off the same trigger — they are the same count by construction.
    const grants = b.grants ? `<small class="bgr">${escHtml(tf(b.grants))}</small>` : "";
    return `<div class="buff-card${off ? " off" : ""}">
      <span class="bn">${escHtml(buffCardName(b.name))}${grants}${off ? ` <small class="bnot">${escHtml(tr("not equipped"))}</small>` : ""}</span>
      <span class="bctl">${ctl}</span>
      ${lock}
    </div>`;
  };
  box.innerHTML = list.map(card).join("");
  // READ-ONLY: the same cards, the same values, and no way to change them. A
  // preset is edited in exactly one place (the rule the fight above already
  // follows), so the optimizer SHOWS the buffs and links to the module that
  // owns them.
  if (opts.readonly) {
    box.querySelectorAll("[data-b]").forEach((el) => {
      el.disabled = true;
      el.title = tr("edit this in the Simulator");
    });
    return;
  }
  box.querySelectorAll("[data-b]").forEach((el) => {
    el.addEventListener("change", () => {
      const id = el.dataset.b, f = el.dataset.f, c = cfg[id];
      if (f === "locked") c.locked = el.checked;
      else if (el.type === "checkbox") c.stacks = el.checked ? 1 : 0;
      else c.stacks = Math.max(0, Number(el.value));
      // A buff belongs to the FIGHT and to nothing else — including settings
      // for mods this build does not carry. It used to dirty the build too,
      // back when a build kept a copy of the scenario (user, 2026-08-02).
      markScenarioDirty();
      // The optimizer shows these read-only, so redraw its copy if it is up.
      if ($("opt-buffs") && !opts.readonly) renderOptBuffs();
    });
  });
}

// The buff panel has TWO views. By default it lists what the current build
// actually carries. "All potential" widens it to every buff this WEAPON could
// ever have — every mod in its pool, every arcane it can seat, every
// evolution option — because a scenario is meant to describe a fight, not a
// build, and a setting for a mod you have not equipped yet is exactly what the
// marginal-gain scan reads (user, 2026-08-01: the settings were only reachable
// once the mod was on).
//
// The union comes from `/api/opt-buffs`, which already answers "every buff
// this SCOPE could produce" for the optimizer — handing it a scope of
// everything is the same question, and a second implementation of it would be
// a second thing to keep right.
let simBuffsAll = false;
let allBuffList = null;   // cached per weapon; null = not fetched
let allBuffWeapon = null;

async function fetchAllBuffs() {
  const w = $("weapon").value;
  if (allBuffWeapon === w && allBuffList) return allBuffList;
  const mark = (ids) => Object.fromEntries(ids.map((id) => [id, "search"]));
  const AX = weaponAxes(w);
  const r = await api("/api/opt-buffs", {
    weapon: w,
    mods: mark([...AX.mods.map((m) => m.id), ...AX.exilus.map((m) => m.id)]),
    arcanes: mark(AX.arcanes.flatMap((a) => a.options.map((x) => x.id))),
    evolutions: Object.fromEntries(AX.evolutions.map((t, i) => [i, t.options.map((o) => o.id)])),
    rivens: rivenPayload(),
  });
  allBuffList = r && r.ok ? r.buffs || [] : [];
  allBuffWeapon = w;
  return allBuffList;
}

function renderSimBuffs() {
  const btn = $("sim-buffs-all");
  const list = simBuffsAll && allBuffList ? allBuffList : buffList;
  if (btn) {
    btn.textContent = simBuffsAll ? tr("in this build") : tr("all potential buffs");
    btn.title = tr("a scenario can set a buff for a mod this build does not carry — the gain scan reads it");
    btn.onclick = async () => {
      simBuffsAll = !simBuffsAll;
      if (simBuffsAll) { btn.disabled = true; await fetchAllBuffs(); btn.disabled = false; }
      renderSimBuffs();
    };
  }
  // Which of them the build actually has, so the wider list still says where
  // you are: everything else is a setting held for later.
  const have = new Set(buffList.map((b) => b.id));
  renderBuffCards($("sim-buffs"), list, sim.buffs, simBuffsAll ? have : null);
}

async function runSim() {
  const btn = $("run-sim");
  btn.disabled = true; btn.textContent = "Simulating…";
  show("sim-results-block", true);
  $("sim-results").innerHTML = `<div class="placeholder">running ${simRuns()} simulations…</div>`;
  try {
    // Send only the buffs the current build actually has (ids in buffList).
    const buffs = {};
    buffList.forEach((b) => { const c = sim.buffs[b.id]; if (c) buffs[b.id] = { stacks: c.stacks, locked: c.locked }; });
    // `replay: true` only HERE. The gain scan hits the same endpoint once per
    // candidate and shows no replay, so it must not pay for one.
    const body = { ...buildPayload(), ...fightPayload(), buffs, replay: true };
    const r = await api("/api/simulate", body);
    if (!r || r.ok === false) {
      $("sim-results").innerHTML = `<div class="error">sim failed: ${r ? r.error : "no data"}</div>`;
      return;
    }
    renderResults(r);
    animateArena(r);
    saveSimResult(r);
    // A run under the OFFICIAL scenario is the only thing that can reach the
    // board, and only after you have said so. Never blocks the result.
    offerBoardSubmit();
  } catch (e) {
    $("sim-results").innerHTML = `<div class="error">sim failed: ${e}</div>`;
  } finally {
    btn.disabled = false; btn.textContent = "Run Simulation";
  }
}

// ---- per-preset result memory ------------------------------------------
// The simulator shows the ACTIVE preset's LAST test (user, 2026-07-29:
// switching builds must switch the displayed numbers too). A finished run
// saves into the preset's entry — as `lastResult`, OUTSIDE `state`, so
// the unsaved-changes dot ignores it — and every preset switch restores
// it (or clears, when that build was never tested).
// WHAT was measured: the build and the fight, together. A number is only
// this build's number while both are unchanged — the share card states the
// two side by side, so it has to know when the stored one stopped matching.
function simKey() {
  const st = snapshotState();
  return JSON.stringify([st.slots, st.arcane, st.arcaneRank, st.evoSel, snapshotScenario()]);
}

function saveSimResult(r) {
  const ps = loadPresetList(BUILDS);
  const at = ps.findIndex((p) => p.name === activePreset);
  if (at < 0) return;
  ps[at].lastResult = { r, at: Date.now(), key: simKey() };
  storePresetList(BUILDS, ps);
}

// The result to PUT ON A CARD: the stored one if it still describes this
// build in this fight, else a fresh run (user, 2026-08-02: sharing always has
// a number). Reusing a stale one would be worse than having none — the card
// would attach a measurement to a build that never produced it.
async function resultForShare() {
  const p = loadPresetList(BUILDS).find((x) => x.name === activePreset);
  if (p && p.lastResult && p.lastResult.r && p.lastResult.key === simKey()) return p.lastResult;
  // The PANEL first, awaited. `refreshPanel` is debounced and returns before
  // it has answered, so a share clicked seconds after an edit was composing
  // its buff map from the PREVIOUS build's `buffList` — a buff the new mod
  // grants was simply absent from the payload, and the server fell back to
  // its own default. That is the whole of "0.4 shared vs 0.56 run by hand"
  // (user, 2026-08-02): same seed, same build, a different buff map.
  try { renderPanel(await api("/api/panel", buildPayload())); } catch (_) { /* keep going */ }
  const buffs = {};
  buffList.forEach((b) => { const c = sim.buffs[b.id]; if (c) buffs[b.id] = { stacks: c.stacks, locked: c.locked }; });
  const r = await api("/api/simulate", { ...buildPayload(), ...sim, buffs });
  if (!r || r.ok === false) return null;
  saveSimResult(r);
  renderStoredSimResult();
  return { r, at: Date.now(), key: simKey() };
}
function renderStoredSimResult() {
  const box = $("sim-results");
  if (!box) return;
  const p = loadPresetList(BUILDS).find((x) => x.name === activePreset);
  const has = !!(p && p.lastResult && p.lastResult.r);
  show("sim-results-block", has); // an untested build shows no Result block
  if (has) renderResults(p.lastResult.r, p.lastResult.at);
  else box.innerHTML = "";
}

// ---- REPLAY -------------------------------------------------------------
//
// The MEDIAN engagement, played back. Not a new simulation and not an average:
// the engine re-ran that one run from the RNG state it recorded, so this is
// the fight the headline number came from, frame by frame (user, 2026-08-02).
//
// One row per buff, each a short curve of LIVE STACKS over time, all open by
// default — the question they answer is "was this thing actually up", and a
// row that has to be clicked to answer it will not be. `mean` and `uptime`
// sit in the header so the group reads without expanding anything at all.
const REPLAY_SPEEDS = [1, 2, 5, 20];
// An UNCAPPED buff has no maximum to draw against, so the curve scales to the
// highest it actually reached and the readout says so.
const rpCap = (b) => (b.uncapped ? "∞" : b.max);
let replayState = null; // { data, i, playing, speed, raf }

// ---- EVERY BLOCK FOLDS -------------------------------------------------
//
// The result panel grew from one number to nine blocks, and not every reader
// wants all nine every time (owner, 2026-08-11). So a block is a heading you
// can click, and it REMEMBERS — per block, across runs and reloads, because a
// panel that re-opens everything on every Run Sim is a panel you have to
// re-close on every Run Sim.
//
// The state lives outside the markup, which is what lets `renderResults`
// rebuild the whole panel without losing what you folded.
let foldState = {};
try { foldState = JSON.parse(localStorage.getItem("wfsim-folds")) || {}; } catch (_) {}
const saveFolds = () => localStorage.setItem("wfsim-folds", JSON.stringify(foldState));
const folded = (id) => foldState[id] === true;

/// One collapsible block: a heading, an optional hint, and a body.
function foldBlock(id, title, hint, body) {
  const shut = folded(id);
  return `<div class="fold${shut ? " shut" : ""}" data-fold="${escHtml(id)}">
    <h3 class="fold-h"><span class="fold-c">▾</span>${escHtml(title)}${
      hint ? ` <span class="sim-hint">${escHtml(hint)}</span>` : ""}</h3>
    <div class="fold-b">${body}</div>
  </div>`;
}

/// Wire every block on the page. Called once per render, and delegated from the
/// heading rather than the whole block, so a click inside the body — a scrub
/// bar, a mod row — never folds the thing it is inside.
function wireFolds(root) {
  (root || document).querySelectorAll(".fold > .fold-h").forEach((h) => {
    h.onclick = () => {
      const box = h.parentElement;
      const id = box.dataset.fold;
      const shut = !box.classList.contains("shut");
      box.classList.toggle("shut", shut);
      foldState[id] = shut;
      saveFolds();
    };
  });
}

// ---- WHAT A SPEEDRUNNER READS ------------------------------------------
//
// `dps` is the whole engagement, reloads included, which is the honest number
// for a long fight and the wrong one for a room. These are the others: the rate
// while the trigger is actually down, how long the first body takes to fall,
// what the magazine you walked in with was worth, and the biggest single number
// the build can produce.
function speedMarkup(r) {
  if (!r || r.burst_dps == null) return "";
  const n = (x) => Math.round(x || 0).toLocaleString();
  const secs = (x) => `${(x || 0).toFixed(2)}s`;
  const ttk = r.ttk || {};
  const cell = (k, v, sub) =>
    `<div class="kpi"><div class="kv">${v}</div><div class="kl">${escHtml(tr(k))}</div>${
      sub ? `<div class="ksub">${escHtml(sub)}</div>` : ""}</div>`;
  const body = `<div class="kpi-row">
    ${cell("Burst DPS", n(r.burst_dps), tr("while firing"))}
    ${cell("Sustained DPS", n(r.dps), tr("reloads included"))}
    ${ttk.runs ? cell("Time to first kill", secs(ttk.median),
        `${tr("median")} · P90 ${secs(ttk.p90)} · ${ttk.runs}/${r.runs} ${tr("runs killed")}`)
      : cell("Time to first kill", "—", tr("nothing died"))}
    ${cell("First magazine", n(r.first_magazine), tr("before the first reload"))}
    ${cell("Biggest hit", n(r.max_hit), `${tr("best run")} · ${n(r.mean_max_hit)} ${tr("typical")}`)}
    ${cell("Per shot", n(r.damage_per_shot), `${n(r.damage_per_pellet)} ${tr("per pellet")}`)}
    ${cell("Not firing", secs(r.downtime), tr("reloads and transforms"))}
  </div>`;
  return foldBlock("speed", tr("Pace"), tr("what a room-clear is paced by, as opposed to a long fight"), body);
}

// ---- EVERY HIT, SORTED BY WHAT IT WAS ----------------------------------
//
// A mean is where an impossible number goes to hide. The same damage spread
// over "one in twelve hits did 40x" and "every hit did 3.3x" reads identically
// as an average and is two completely different weapons — and only one of them
// is a bug.
function hitTableMarkup(r) {
  const hits = r && r.hits;
  if (!hits || !hits.length) return "";
  const n = (x) => Math.round(x || 0).toLocaleString();
  const TIERS = ["no crit", "crit", "red crit"];
  const total = hits.flat().reduce((a, b) => a + b.count, 0);
  if (!total) return "";
  const row = (label, cells) =>
    `<tr><td>${escHtml(label)}</td>${cells.map((c) => {
      if (!c.count) return `<td class="z">—</td>`;
      return `<td><b>${n(c.damage / c.count)}</b><span class="ct">${
        n(c.count)} · ${((c.count / total) * 100).toFixed(1)}%</span></td>`;
    }).join("")}</tr>`;
  const body = `<table class="hits">
    <tr><th></th>${TIERS.map((t) => `<th>${escHtml(tr(t))}</th>`).join("")}</tr>
    ${row(tr("body"), hits[0])}
    ${row(tr("head"), hits[1])}
  </table>`;
  return foldBlock("hits", tr("Every hit, sorted"),
    tr("mean damage per hit and how often it happened — an impossible number hides in an average"), body);
}

// ---- THE ACCOUNT OF ONE HIT --------------------------------------------
//
// Every other number on this page is an aggregate, and an aggregate hides an
// error inside an average: a factor applied twice, or in the wrong bracket,
// moves a mean by a few per cent and reads as "this build is good". This is the
// one output that can be FALSIFIED — each line is a factor with its value, the
// product is the number that went into the damage meter, and anyone with the
// wiki and a calculator can check it (owner, 2026-08-11).
//
// It is the MEDIAN engagement's, the same run the replay plays back, so the
// account and the curves are the same fight. A factor of exactly 1.00 is drawn
// rather than dropped: "faction ×1.00" is the answer to "why is my Bane doing
// nothing", and a missing line is not an answer.
const TIER_NAME = ["no crit", "crit", "red crit", "orange crit", "purple crit"];
function hitAccountsMarkup(r) {
  const acc = (r && r.replay && r.replay.accounts) || [];
  if (!acc.length) return "";
  const n = (x) => Math.round(x || 0).toLocaleString();
  const f = (x) => (Math.abs(x - 1) < 1e-9 ? "×1.00" : "×" + sig2(x));
  const rows = acc.map((a) => {
    const steps = a.steps
      // A step that is exactly 1 and was NEVER going to be anything else is
      // noise; one that could have moved is evidence. The line is kept when the
      // build could produce it at all, which is what `mult !== 1` cannot say —
      // so the rule is simpler: keep them all, and let the eye skip the ones.
      .map((s) => `<tr><td>${escHtml(tr(s.label))}</td><td class="mul">${f(s.mult)}</td></tr>`)
      .join("");
    const mitigation = a.raw > 0 ? a.effective / a.raw : 1;
    return `<div class="acct">
      <div class="acct-h">${escHtml(tr(a.source === "direct" ? "direct hit" : "explosion"))}
        · ${escHtml(a.part)}${a.head ? " ⌖" : ""}
        · ${escHtml(tr(TIER_NAME[a.tier] || `crit ×${a.tier}`))}
        · ${a.t.toFixed(2)}s</div>
      <table class="acct-t">
        <tr class="base"><td>${escHtml(tr("this instance's modded damage"))}</td><td class="mul">${n(a.base)}</td></tr>
        ${steps}
        <tr class="sum"><td>${escHtml(tr("dealt"))}</td><td class="mul">${n(a.raw)}</td></tr>
        <tr><td>${escHtml(tr("the target's armour, column and attenuation"))}</td><td class="mul">${f(mitigation)}</td></tr>
        <tr class="sum"><td>${escHtml(tr("taken"))}</td><td class="mul">${n(a.effective)}</td></tr>
      </table>
    </div>`;
  }).join("");
  return foldBlock("accounts", tr("The account of one hit"),
    tr("every factor, in the order the engine applies them — the product is what the meter counted"),
    `<div class="accts">${rows}</div>`);
}

function replayMarkup(r) {
  const rp = r && r.replay;
  if (!rp || !rp.t || rp.t.length < 2) return { bar: "", curves: "" };
  // Buff names come from the CARDS — same ids, so the two can never disagree
  // about what a series is called.
  const named = (id) => {
    const b = (buffList || []).find((x) => x.id === id);
    return b ? buffCardName(b.name) : id;
  };
  // A DEBUFF ROW IS NAMED BY ITS DAMAGE TYPE, which is already translated
  // everywhere else on the page (`DT`). The alternative was a new i18n family
  // for the proc names — Virus, Corrosion, Disrupt — and DE's Chinese for those
  // is not something to invent: a string is transcribed, never translated
  // (AGENTS.md). The damage type says the same thing in words the reader has
  // already seen on the damage meter, and it is 1:1 with the proc everywhere
  // except Cold, whose two states are told apart by a suffix of our own.
  const DEBUFF_TYPE = {
    virus: "viral", corrosion: "corrosive", disrupt: "magnetic",
    confusion: "radiation", blast: "blast", freeze: "cold", frozen: "cold",
    stagger: "impact", weakened: "puncture", attractor: "void",
    bleed: "slash", poison: "toxin", ignite: "heat",
  };
  // MICROWAVE is not a damage type, so it has no `DT` name to borrow — it is
  // the Nukor family's own status and DE's own word for it. Left in English on
  // a Chinese page for the same reason "Overshields" is: a string is
  // TRANSCRIBED, never translated, and DE's Chinese for this one could not be
  // reached from here (its status has no page of its own in the CN wiki). The
  // weapon's own name contains 微波, which is evidence and not a source.
  const dbName = (id) =>
    (id === "microwave"
      ? "Microwave"
      : DT(DEBUFF_TYPE[id] || id)) + (id === "frozen" ? ` (${tr("frozen")})` : "");
  const dRoster = rp.debuffs || [];
  const dSeries = rp.dstacks || [];
  const W = 600, H = 28;
  const curveRows = (roster, series, name, kind) => roster.map((b, i) => {
    const s = series[i] || [];
    const max = Math.max(1, b.uncapped ? Math.max(...s) : b.max);
    const px = (j) => (j / (s.length - 1)) * W;
    const py = (v) => H - 1 - (v / max) * (H - 2);
    const pts = s.map((v, j) => `${px(j).toFixed(1)},${py(v).toFixed(1)}`).join(" ");
    const mean = s.reduce((a, v) => a + v, 0) / (s.length || 1);
    const up = s.filter((v) => v > 0).length / (s.length || 1);
    // TWO DECIMALS, which is what made the "impossible 100%" go away for
    // real: 99.83% is the truth, and rounding it to a whole number was the
    // only thing that ever made it look like a perfect run. `100.00%` now
    // means every single frame was up, and nothing else prints it (user,
    // 2026-08-03).
    const upPct = (up * 100).toFixed(2);
    const offPct = (100 - up * 100).toFixed(2);
    // ...and the thing the 100% was hiding: how long the ramp took. This is
    // the answer to "初始肯定要花时间" as a number rather than a rounding.
    const iFull = b.uncapped ? -1 : s.findIndex((v) => v >= b.max);
    const ramp = iFull < 0
      ? tr("never full")
      : `${tr("full at")} ${(iFull * rp.dt).toFixed(2)}s`;
    // WHERE IT WAS OFF. A buff at zero is not a low buff, it is an absent one,
    // and a flat line along the axis says that far too quietly — the run that
    // never earned a stack and the run that lost them all draw the same
    // picture (user, 2026-08-03). Every zero stretch gets a band.
    const dead = [];
    for (let j = 0; j < s.length; j++) {
      if (s[j] > 0) continue;
      const from = j;
      while (j + 1 < s.length && s[j + 1] === 0) j++;
      dead.push(`<rect class="rp-dead" x="${px(from).toFixed(1)}" y="0" width="${Math.max(1, px(j) - px(from)).toFixed(1)}" height="${H}"/>`);
    }
    return `<div class="rp-row" data-${kind}="${i}">
      <div class="rp-head">
        <span class="rp-caret">▾</span>
        <span class="rp-name">${escHtml(name(b.id))}</span>
        <span class="rp-stat">${escHtml(tr("avg"))} ${mean.toFixed(2)}/${rpCap(b)} · ${escHtml(tr("uptime"))} ${upPct}%${up < 1 ? ` · <span class="rp-off">${escHtml(tr("inactive"))} ${offPct}%</span>` : ""} · ${escHtml(ramp)}</span>
        <span class="rp-now" data-now="${i}" data-series="${kind}">${s[s.length - 1]}/${rpCap(b)}</span>
      </div>
      <div class="rp-chart">
        <svg viewBox="0 0 ${W} ${H}" preserveAspectRatio="none">
          ${dead.join("")}
          <polygon class="rp-area" points="0,${H} ${pts} ${W},${H}"/>
          <line class="rp-mean" x1="0" x2="${W}" y1="${py(mean).toFixed(1)}" y2="${py(mean).toFixed(1)}"><title>${escHtml(tr("avg"))} ${mean.toFixed(2)}</title></line>
          <polyline class="rp-line" points="${pts}"/>
          <rect class="rp-ahead" data-ahead="${i}" data-series="${kind}" x="${W}" y="0" width="0" height="${H}"/>
          <line class="rp-cur" data-cur="${i}" data-series="${kind}" y1="0" y2="${H}" x1="${W}" x2="${W}"/>
        </svg>
      </div>
    </div>`;
  }).join("");
  const rows = curveRows(rp.buffs, rp.stacks, named, "buff");
  // THE TARGET'S SIDE OF THE SAME FIGHT. Symmetric with the buff table on
  // purpose (owner, 2026-08-11) — same rows, same uptime, same dead bands. A
  // DEATH IS NOT A NEW SERIES: the arena replaces the body it kills and every
  // stack goes with it, so a respawn reads as the curve dropping to zero and
  // climbing again, and the gap counts
  // against uptime. That is what makes the table worth reading — the ramp you
  // pay for on every body is the thing a single averaged number hides.
  //
  // Rows the run never touched are dropped rather than drawn flat: the roster
  // is every status the engine models, and thirteen empty charts would bury the
  // three that moved. A buff row is kept even at zero because the BUILD claimed
  // it; nothing claims a debuff except the fight.
  const dRows = curveRows(
    // A STORED RESULT PREDATES THIS TABLE and carries no debuff series at all —
    // `lastResult` is saved in the scenario preset and replayed on boot, so a
    // payload written by yesterday's build is the FIRST thing this code sees on
    // a returning visitor's machine. It cost the whole app: an unguarded
    // `.filter` on `undefined` threw inside `restoreState`, which is upstream
    // of everything, so the page did not fail to draw a table — it failed to
    // start (reported 2026-08-11).
    dRoster.filter((_, i) => (dSeries[i] || []).some((v) => v > 0)),
    dSeries.filter((s) => (s || []).some((v) => v > 0)),
    dbName, "debuff");
  // TWO pieces, deliberately far apart (user, 2026-08-03). The transport
  // belongs at the top, next to the numbers it drives; the CURVES are charts
  // and belong with the other chart, under the DPS curve. Moving both up put
  // a wall of graphs above the result they explain.
  const bar = `
      <h3>${escHtml(tr("Replay"))}</h3>
      <div class="rp-bar">
        <button id="rp-play" class="ghost-btn small rp-play">▶ ${escHtml(tr("play"))}</button>
        ${ddButton("rp-speed", {
          value: 5,
          items: REPLAY_SPEEDS.map((sp) => ({ value: sp, label: `${sp}x` })),
        })}
        <input id="rp-scrub" class="rp-scrub" type="range" min="0" max="${rp.t.length - 1}" value="${rp.t.length - 1}">
        <span id="rp-clock" class="rp-clock">${rp.t[rp.t.length - 1].toFixed(0)}s / ${rp.t[rp.t.length - 1].toFixed(0)}s</span>
      </div>
      <div class="rp-pools" id="rp-pools"></div>`;
  const curves =
    foldBlock("buffs", tr("Buff coverage"), tr("live stacks through the engagement"), rows)
    + (dRows
      ? foldBlock("debuffs", tr("Debuff coverage"),
          tr("what was on the target — a respawn is the same target, so its stacks drop to zero and climb again"),
          dRows)
      : "");
  return { bar, curves };
}

// Re-read the WHOLE result panel at frame `i` — KPIs, the damage meter, the
// DPS curve, the buff curves, the target's pools.
//
// This is what makes it a replay rather than a cursor (user, 2026-08-03). The
// panel is rendered ONCE at its final state and then re-read in place:
// rebuilding the markup sixty times a second would drop every open sub-row,
// every scroll position and the caret you just clicked.
//
// It starts at the LAST frame, which is the finished fight — the same numbers
// the panel would show with no replay at all. Playing rewinds to 0 and walks
// forward; stopping anywhere leaves the panel reading that instant.
function replayApply(rp, i) {
  const n = (x) => Math.round(x || 0).toLocaleString();
  const pc = (x) => `${((x || 0) * 100).toFixed(1)}%`;
  const last = rp.t.length - 1;
  const frac = last > 0 ? i / last : 1;

  $("rp-clock").textContent = `${rp.t[i].toFixed(1)}s / ${rp.t[last].toFixed(0)}s`;
  $("rp-scrub").value = i;
  // A FIXED GRID, not a flowing row (user, 2026-08-03). Every value here
  // changes on every frame, and a flex row re-measures itself each time — the
  // labels slide left and right for the whole playback, which reads as the
  // page shaking. Fixed columns and tabular figures hold still, and the grid
  // is what lets a second and third enemy join without a re-layout.
  const cell = (label, v) =>
    `<span class="rp-cell"><i>${escHtml(label)}</i><b>${v}</b></span>`;
  $("rp-pools").innerHTML =
    cell(tr("Overguard"), n(rp.og[i])) +
    cell(tr("Shield"), n(rp.sh[i])) +
    cell(tr("Health"), n(rp.hp[i])) +
    cell(tr("Damage"), n(rp.dmg[i])) +
    cell(tr("Kills"), rp.kills[i]);

  // The headline. KPM is `kill_progress / minutes`, and `kill_progress` is
  // kills plus the fraction of the CURRENT target's pool already gone — which
  // the frames carry directly, so it is derived here rather than shipped as a
  // fourth series that could disagree with them.
  const hero = document.querySelector("[data-hero]");
  if (hero) {
    const pool0 = (rp.og[0] || 0) + (rp.hp[0] || 0) + (rp.sh[0] || 0);
    const left = (rp.og[i] || 0) + (rp.hp[i] || 0) + (rp.sh[i] || 0);
    const progress = (rp.kills[i] || 0) + (pool0 > 0 ? 1 - left / pool0 : 0);
    const mins = rp.t[i] / 60;
    const v = hero.dataset.hero === "dps"
      ? n((rp.kpi && rp.kpi.dps ? rp.kpi.dps[i] : 0))
      : (mins > 0 ? progress / mins : 0).toFixed(2);
    const unit = hero.querySelector(".hero-unit");
    hero.textContent = v;
    if (unit) hero.appendChild(unit);
  }

  // KPIs. Rates are fractions, counters are counts, DPS is a number — the
  // key says which, so a new KPI needs no new branch here.
  const k = rp.kpi || {};
  document.querySelectorAll("[data-kpi]").forEach((el) => {
    const key = el.dataset.kpi, s = k[key];
    if (!s) return;
    el.textContent = key === "crit_tier" ? (s[i] || 0).toFixed(2)
      : /_rate$/.test(key) ? pc(s[i])
      : n(s[i]);
  });

  // The damage meter, rescaled to the damage dealt SO FAR: the bars are a
  // composition, and a composition of a fight in progress is read against
  // that fight, not against its end.
  const byKey = {};
  (rp.sources || []).forEach((s) => {
    byKey[s.source] = s.dmg;
    (s.by_type || []).forEach((ty) => { byKey[`${s.source}::${ty.type}`] = ty.dmg; });
  });
  let total = 0, max = 0;
  (rp.sources || []).forEach((s) => { total += s.dmg[i]; max = Math.max(max, s.dmg[i]); });
  document.querySelectorAll("#sim-results [data-mk]").forEach((el) => {
    const s = byKey[el.dataset.mk];
    if (!s) return;
    const v = s[i];
    const sub = el.classList.contains("sub");
    // A sub-row is drawn against its own source's bar, exactly as at the end.
    const own = sub ? (byKey[el.dataset.mk.split("::")[0]] || [])[i] || 1 : max || 1;
    const bar = el.querySelector(".mbar i");
    if (bar) bar.style.width = `${Math.max(0, (v / own) * 100).toFixed(1)}%`;
    const val = el.querySelector(".mval");
    if (val) val.textContent = `${n(v)} · ${total > 0 ? ((v / total) * 100).toFixed(1) : "0.0"}%`;
  });

  // The DPS curve: everything past `t` is greyed rather than removed, so the
  // shape you are walking through stays legible.
  const svg = $("tl-svg");
  if (svg) {
    const w = Number(svg.viewBox.baseVal.width) || 600;
    const now = $("tl-now"), ahead = $("tl-ahead");
    if (now) { now.hidden = false; now.setAttribute("x1", w * frac); now.setAttribute("x2", w * frac); }
    if (ahead) {
      ahead.hidden = false;
      ahead.setAttribute("x", w * frac);
      ahead.setAttribute("width", w * (1 - frac));
    }
  }

  // The buff curves, same treatment, plus the live count in each header.
  document.querySelectorAll("[data-cur]").forEach((el) => {
    el.setAttribute("x1", 600 * frac); el.setAttribute("x2", 600 * frac);
  });
  document.querySelectorAll("[data-ahead]").forEach((el) => {
    el.setAttribute("x", 600 * frac); el.setAttribute("width", 600 * (1 - frac));
  });
  // THE LIVE COUNT in each header, from whichever side of the fight the row
  // belongs to. The DEBUFF rows index a FILTERED roster — the statuses this run
  // never applied are not drawn — so the row rebuilds the same filter rather
  // than indexing the full one and reading somebody else's series.
  const dLive = (rp.debuffs || [])
    .map((b, k) => [b, (rp.dstacks || [])[k] || []])
    .filter(([, s]) => s.some((v) => v > 0));
  document.querySelectorAll("[data-now]").forEach((el) => {
    const j = Number(el.dataset.now);
    if (el.dataset.series === "debuff") {
      const [b, s] = dLive[j] || [];
      if (b) el.textContent = `${s[i]}/${rpCap(b)}`;
      return;
    }
    el.textContent = `${rp.stacks[j][i]}/${rpCap(rp.buffs[j])}`;
  });
}

function wireReplay(r) {
  const rp = r && r.replay;
  if (replayState && replayState.raf) cancelAnimationFrame(replayState.raf);
  replayState = null;
  if (!rp || !$("rp-scrub")) return;
  // `pos` is a FLOAT cursor in frames — playback advances it by fractions of
  // a frame per animation tick. Every array read goes through the rounded
  // index: `rp.t[3.7]` is `undefined`, which threw inside the animation
  // callback and killed the loop with no console entry to show for it.
  const st = { data: rp, pos: rp.t.length - 1, i: rp.t.length - 1, playing: false, speed: 5, raf: 0, last: 0 };
  replayState = st;

  const draw = () => {
    st.i = Math.max(0, Math.min(rp.t.length - 1, Math.round(st.pos)));
    replayApply(rp, st.i);
  };
  const stop = () => {
    st.playing = false; st.last = 0;
    $("rp-play").textContent = `▶ ${tr("play")}`;
  };
  const tick = (now) => {
    if (!st.playing) return;
    // Wall-clock paced, so `speed` means what it says however fast the
    // browser paints: 5x is five seconds of fight per second of watching.
    const dtms = st.last ? now - st.last : 16;
    st.last = now;
    st.pos += (dtms / 1000) * st.speed / rp.dt;
    if (st.pos >= rp.t.length - 1) { st.pos = rp.t.length - 1; stop(); }
    draw();
    if (st.playing) st.raf = requestAnimationFrame(tick);
  };
  $("rp-play").onclick = () => {
    if (st.playing) { stop(); return; }
    // Pressing play on a FINISHED fight rewinds it — that is what the button
    // means, and leaving it stuck at the end made it look broken.
    if (st.pos >= rp.t.length - 1) st.pos = 0;
    st.playing = true; st.last = 0;
    $("rp-play").textContent = `❚❚ ${tr("pause")}`;
    st.raf = requestAnimationFrame(tick);
  };
  ddReg.get("rp-speed").onPick = (v) => { st.speed = Number(v) || 1; };
  $("rp-scrub").oninput = () => { stop(); st.pos = Number($("rp-scrub").value); draw(); };
  // Collapse one row without losing its place in the group.
  document.querySelectorAll(".rp-row .rp-head").forEach((h) => {
    h.onclick = () => {
      const chart = h.parentElement.querySelector(".rp-chart");
      const open = chart.hidden;
      chart.hidden = !open;
      h.querySelector(".rp-caret").textContent = open ? "▾" : "▸";
    };
  });
  draw();
}

function renderResults(r, testedAt) {
  const t = r.target || {};
  const pc = pct2; // 2 decimals, more when the value would otherwise vanish
  const n0 = (x) => Math.round(x || 0).toLocaleString();
  const n2 = sig2;
  const killed = (r.kills || 0) >= 1;
  // ONE scoring unit, killed or not (user, 2026-07-29): the KILL SCORE
  // (engine `kill_progress`) = whole kills + the fraction of the current
  // target's pool already drained. 0.85% of an EHP is 0.01; two kills and
  // 30% of the next is 2.30. The sub-line adds the context that differs.
  const ttk = killed ? r.duration / r.kills : Infinity;
  // The headline is a RATE, like DPS: kill score PER MINUTE, so a 20-second
  // run and a 120-second one produce comparable numbers (user, 2026-07-31).
  // The score itself is the total over the engagement and stays beside it,
  // the same way total damage sits beside DPS.
  const byDps = sim.metric === "dps";
  const heroNum = byDps ? n0(r.dps) : n2(kpm(r.score, r.duration));
  // The UNIT belongs beside the number, not under it: "5.29" on one line and
  // "KPM · …" starting the next read as two facts (user, 2026-08-03). Set
  // small and spaced away, so it labels the figure without competing with it.
  const heroUnit = byDps ? "DPS" : "KPM";
  const heroSub = (byDps ? `${n2(kpm(r.score, r.duration))} KPM · ` : ``) +
    `${n2(r.score)} kill score in ${n0(r.duration)}s · ` + (killed
    ? `${n0(r.kills)} killed · ~${isFinite(ttk) ? ttk.toFixed(2) : "∞"}s avg per kill`
    : `${pc(r.score)} of one ${LN("enemies", sim.enemy, t.name || "enemy")}'s EHP drained`);
  // No Forma/capacity here — the simulator reports EFFECTS only; build
  // legality is the Builder's business (user, 2026-07-29).
  // `k` names the replay series that re-reads this cell. Without it a replay
  // could only move a cursor; with it the whole row is a function of time.
  const kpi = (l, v, k) => `<div class="kpi"><div class="kv"${k ? ` data-kpi="${k}"` : ""}>${v}</div><div class="kl">${tr(l)}</div></div>`;
  // KPI row: damage pace + crit feel + HANDLING feel (shots, reloads,
  // transforms — user, 2026-07-29). In THIS product "DPS" always means
  // EFFECTIVE dps — what the target actually lost, armor and on-target
  // amps included; the weapon-side raw number is out (user: in our
  // context every stat accounts for the enemy).
  const kpis = [
    kpi("DPS", n0(r.dps), "dps"),
    // The TIER leads, and the rate is renamed to what it actually measures.
    // "Crit rate" reads as "my crit chance", and it stops being that the
    // moment a build passes 100%: every pellet crits, so it pins at 100%
    // whether the build is at 110% or 410% (group, 2026-07-31). The tier is
    // the same number without that truncation — and the one that multiplies
    // the damage. 1 = yellow, 2 = orange, 3 = red, and it keeps going.
    kpi("Crit tier", (r.crit_tier ?? 0).toFixed(2), "crit_tier"),
    kpi("Pellets crit", pc(r.crit_rate), "crit_rate"), kpi("Orange+", pc(r.big_crit_rate), "big_crit_rate"),
    kpi("Procs", n0(r.procs), "procs"), kpi("Shots", n0(r.shots), "shots"),
    kpi("Reloads", n0(r.reloads), "reloads"), kpi("Transforms", n0(r.transforms), "transforms"),
  ].join("");
  // WoW-style damage meter (user, 2026-07-29): effective damage BY SOURCE
  // over the whole engagement — what actually hurt the target. The panel's
  // per-shot theory lives in the Builder's Stats, not here.
  const srcs = r.damage_sources || [];
  const srcTotal = srcs.reduce((a, x) => a + x.dmg, 0) || 1;
  const srcMax = (srcs[0] && srcs[0].dmg) || 1;
  // Bucket rows are named; a status row is named by its damage type. `field`
  // and `radial` fell through to `DT()`, which knows damage types only, so
  // they printed their raw wire key in every language.
  const SRC_LABEL = { direct: "Direct hits", radial: "Radial (AoE)",
    field: "Lingering field", arcane: "Arcane (on status)",
    syndicate: "Syndicate radial", "extra hit": "Extra hit (ability)" };
  const srcLabel = (k) => (SRC_LABEL[k] ? tr(SRC_LABEL[k]) : DT(k));
  // A WEAPON-damage row EXPANDS into the damage types it was dealt as — a
  // status row already IS one type, which is what a proc is. Both levels use
  // the same denominator, so every percentage in the meter reads against the
  // engagement total and the numbers still sum to 100%.
  //
  // The shares are the QUANTIZED ones (the vector that actually landed), so
  // they will not match the Builder's panel exactly — the Torid's 164.73
  // Corrosive / 52.02 Magnetic reads 76/24 there and 75/25 here, because
  // quantization snaps each component to a multiple of total/32.
  // A BAR IS COLOURED BY WHAT IT IS, not by where it sits.
  //
  // A damage TYPE gets DE's own colour (style.css, from the wiki's
  // `Module:DamageTypes/data`); a SOURCE — "Direct hits", "Radial (AoE)" — is
  // not a damage type and has no official colour, so it keeps a positional one
  // from the `--s1..8` ramp. That is the split the meter was missing: it
  // coloured everything positionally, so Heat was one colour under a direct hit
  // and another under a field, and neither was Heat's.
  const mbar = (w, ty, c, dim) => {
    const col = dtColor(ty) || `var(--s${c})`;
    return `<div class="mbar"><i style="width:${w.toFixed(1)}%;background:${col}${dim ? ";opacity:.65" : ""}"></i></div>`;
  };
  // The meter is re-read per frame too, so every row carries the key of the
  // series that feeds it — top-level by source, sub-rows by source+type.
  const mkey = (s, ty) => ` data-mk="${escHtml(ty ? `${s}::${ty}` : s)}"`;
  const meter = srcs.map((x, i) => {
    const c = (i % 8) + 1;
    const parts = x.by_type && x.by_type.length > 1 ? x.by_type : null;
    const open = !!parts && simMeterOpen.has(x.source);
    const head = `<div class="mrow${parts ? " exp" : ""}" data-src="${escHtml(x.source)}"${mkey(x.source)} data-c="${c}">
      <span class="mname">${parts ? `<span class="mcaret">${open ? "▾" : "▸"}</span>` : ""}${dtIcon(x.source)}${srcLabel(x.source)}</span>
      ${mbar(x.dmg / srcMax * 100, x.source, c, false)}
      <span class="mval">${n0(x.dmg)} · ${pct2(x.dmg / srcTotal)}</span>
    </div>`;
    if (!parts) return head;
    return head + parts.map((p) => `<div class="mrow sub" data-of="${escHtml(x.source)}"${mkey(x.source, p.type)} data-c="${c}"${open ? "" : " hidden"}>
      <span class="mname">${dtIcon(p.type)}${DT(p.type)}</span>
      ${mbar(p.dmg / srcMax * 100, p.type, c, true)}
      <span class="mval">${n0(p.dmg)} · ${pct2(p.dmg / srcTotal)}</span>
    </div>`).join("");
  }).join("");
  // WHAT THE DAMAGE WAS MADE OF — one stacked bar over the whole engagement,
  // in DE's own colours (owner, 2026-08-06).
  //
  // The meter above answers "where did it come from" — direct hits, a cloud, a
  // proc. This answers a different question a build actually turns on: what
  // ELEMENTS is that, added up. They are the same damage counted two ways, so
  // both read against the same total and both come to 100%.
  //
  // Aggregated from the meter's own rows rather than from a second field, so
  // the bar cannot disagree with the list under it: a source that splits by
  // type contributes its split, and a source that IS a type (a status row)
  // contributes itself.
  const typeTotals = {};
  for (const x of srcs) {
    const parts = x.by_type && x.by_type.length ? x.by_type : [{ type: x.source, dmg: x.dmg }];
    for (const p of parts) {
      if (!dtKey(p.type)) continue;   // a source name is not a damage type
      typeTotals[dtKey(p.type)] = (typeTotals[dtKey(p.type)] || 0) + p.dmg;
    }
  }
  const typeRows = Object.entries(typeTotals)
    .filter(([, v]) => v > 0)
    .sort((a, b) => b[1] - a[1]);
  const typeSum = typeRows.reduce((a, [, v]) => a + v, 0);
  // Biggest first, so the bar reads left to right in the order the legend
  // does and the eye can match a segment to a line without hunting.
  const composition = typeRows.length ? `
      <h3>${tr("Damage by type")} <span class="sim-hint">${tr("share of the engagement")}</span></h3>
      <div class="dmg-bar">${typeRows.map(([ty, v]) =>
        `<i class="dmg-seg" style="flex:${(v / typeSum).toFixed(5)};background:${dtColor(ty)}" title="${escHtml(DT(ty))} ${pct2(v / typeSum)}"></i>`).join("")}</div>
      <div class="legend">${typeRows.map(([ty, v]) =>
        `<span class="li">${dtIcon(ty)}${escHtml(DT(ty))} <span class="lv">${pct2(v / typeSum)}</span></span>`).join("")}</div>` : "";

  // DPS-over-time curve (user, 2026-07-29): the MEDIAN run's per-bucket
  // EFFECTIVE dps. One series — the accent line, recessive grid, hover
  // crosshair + tooltip; no legend needed.
  const tl = r.timeline || [];
  const bucketSecs = (r.duration || 1) / (tl.length || 1);
  const dpsPts = tl.map((v) => v / bucketSecs);
  const tlMax = Math.max(1, ...dpsPts);
  const W = 600, H = 170, PADL = 8, PADR = 8, PADT = 8, PADB = 6;
  const px = (i) => PADL + ((i + 1) / tl.length) * (W - PADL - PADR);
  const py = (v) => PADT + (1 - v / tlMax) * (H - PADT - PADB);
  const pts = dpsPts.map((v, i) => `${px(i)},${py(v)}`).join(" ");
  const tlGrid = [0.25, 0.5, 0.75].map((f) =>
    `<line class="tl-grid" x1="${PADL}" x2="${W - PADR}" y1="${py(tlMax * f)}" y2="${py(tlMax * f)}"/>`).join("");
  const chart = tl.length ? `
      <h3>${tr("DPS over time")} <span class="sim-hint">${tr("median run")}</span></h3>
      <div class="tl-wrap">
        <svg id="tl-svg" viewBox="0 0 ${W} ${H}" preserveAspectRatio="none">
          ${tlGrid}
          <polyline class="tl-line" points="${pts}"/>
          <rect id="tl-ahead" class="tl-ahead" x="${W}" y="0" width="0" height="${H}" hidden/>
          <line id="tl-now" class="tl-now" y1="0" y2="${H}" x1="0" x2="0" hidden/>
          <line id="tl-cross" class="tl-cross" y1="${PADT}" y2="${H - PADB}" hidden/>
        </svg>
        <div class="tl-x"><span>0s</span><span>${n0(r.duration)}s</span></div>
        <div class="tl-ymax">${n0(tlMax)}</div>
        <div id="tl-tip" class="tl-tip" hidden></div>
      </div>` : "";
  const { bar: replayBar, curves: replayCurves } = replayMarkup(r);
  const row = (k, v) => `<div class="row"><span class="k">${k}</span><span class="v">${v}</span></div>`;
  const detail = [
    row("Target", `${t.name || "?"} · Lv ${t.level}${t.steel_path ? " (SP)" : ""}`),
    row("OG / Shield / Health", `${n0(t.overguard)} / ${n0(t.shield)} / ${n0(t.health)}`),
    row("Armor", n0(t.armor)),
    row("Shots / Pellets", `${n0(r.shots)} / ${n0(r.pellets)}`),
    row("Kills min–max (±σ)", `${r.kills_min}–${r.kills_max} (±${sig2(r.kills_std)})`),
    row("Runs", n0(r.runs)),
  ].join("");
  $("sim-results").innerHTML = `
    <div class="results">
      <div class="hero"><div><div class="hero-num" data-hero="${byDps ? "dps" : "kpm"}">${heroNum}<span class="hero-unit">${heroUnit}</span></div><div class="hero-sub">${heroSub}</div>${testedAt ? `<div class="hero-tested">${tr("last tested")} ${new Date(testedAt).toLocaleString()}</div>` : ""}</div></div>
      ${replayBar}
      <div class="kpi-row">${kpis}</div>
      ${foldBlock("meter", tr("Damage by source"), "",
        `<div class="meter">${meter.length ? meter : `<div class="sb-empty">${tr("no damage dealt")}</div>`}</div>${composition}`)}
      ${speedMarkup(r)}${hitTableMarkup(r)}${hitAccountsMarkup(r)}${chart}${replayCurves}
      ${foldBlock("detail", tr("Detail"), "", `<div class="stat-table">${detail}</div>`)}
    </div>`;
  // Meter rows that carry a per-type split toggle theirs. The choice is kept
  // across runs — a player who opened Direct hits wants it open on the next
  // simulate, not to reopen it every time.
  $("sim-results").querySelectorAll(".mrow.exp").forEach((el) => {
    el.addEventListener("click", () => {
      const k = el.dataset.src;
      const open = !simMeterOpen.has(k);
      if (open) simMeterOpen.add(k);
      else simMeterOpen.delete(k);
      el.querySelector(".mcaret").textContent = open ? "▾" : "▸";
      $("sim-results")
        .querySelectorAll(`.mrow.sub[data-of="${CSS.escape(k)}"]`)
        .forEach((c) => { c.hidden = !open; });
    });
  });
  wireFolds();
  wireReplay(r);
  // Chart hover: crosshair + tooltip on the nearest time bucket.
  const wrap = $("sim-results").querySelector(".tl-wrap");
  if (wrap) {
    const svg = wrap.querySelector("#tl-svg");
    const tip = wrap.querySelector("#tl-tip");
    const cross = wrap.querySelector("#tl-cross");
    wrap.addEventListener("mousemove", (ev) => {
      const b = svg.getBoundingClientRect();
      const fx = (ev.clientX - b.left) / b.width;
      const i = Math.max(0, Math.min(tl.length - 1, Math.round(fx * tl.length) - 1));
      cross.hidden = false;
      cross.setAttribute("x1", px(i)); cross.setAttribute("x2", px(i));
      tip.hidden = false;
      tip.textContent = `${(((i + 1) / tl.length) * r.duration).toFixed(1)}s · ${n0(dpsPts[i])} DPS`;
      tip.style.left = Math.min(b.width - 120, Math.max(0, ev.clientX - b.left + 10)) + "px";
    });
    wrap.addEventListener("mouseleave", () => { cross.hidden = true; tip.hidden = true; });
  }
}

// Illustrative arena replay: drain the enemy's layered bars over a fixed
// visual window (NOT the simulated kill time — the sim is Monte-Carlo over
// many runs, so the headline number is authoritative; this only shows the
// enemy getting beaten).
function animateArena(r) {
  const t = r.target || {};
  const layers = [
    { k: "og", v: t.overguard || 0 }, { k: "sh", v: t.shield || 0 }, { k: "hp", v: t.health || 0 },
  ].filter((l) => l.v > 0);
  $("arena-bars").innerHTML = layers.map((l) =>
    `<div class="bar ${l.k}"><div class="fill" style="width:100%"></div></div>`).join("");
  const killed = (r.kills || 0) >= 1;
  const frac = killed ? 1 : Math.max(0, Math.min(1, r.score || 0));
  const remain = ((1 - frac) * 100).toFixed(1) + "%";
  const dmg = (r.panel && r.panel.damage) || [];
  const chips = dmg.map((d, i) => `<span class="chip" style="--c:var(--s${(i % 8) + 1})">${DT(d.type)}</span>`);
  if ((r.procs || 0) > 0) chips.push(`<span class="chip proc">⚡ ${Math.round(r.procs)} procs</span>`);
  $("arena-status").innerHTML = chips.join("");
  const enemy = $("arena-enemy"), tracer = $("arena-tracer");
  enemy.classList.remove("defeated");
  tracer.classList.remove("firing");
  void tracer.offsetWidth; // force reflow so the animation restarts each run
  tracer.classList.add("firing");
  requestAnimationFrame(() => {
    $("arena-bars").querySelectorAll(".fill").forEach((f) => { f.style.width = remain; });
    if (killed) setTimeout(() => enemy.classList.add("defeated"), 1500);
  });
}

// ---- Optimizer: scope (mods/arcanes/evolutions) → top-10 builds ---------
// Mod scope is a 3-state cycle (off / pool / required); the client estimates
// the candidate count (no cap — the server funnel culls large spaces).
function nChooseK(n, k) {
  if (k < 0 || k > n) return 0;
  k = Math.min(k, n - k);
  let r = 1;
  for (let i = 0; i < k; i++) r = (r * (n - i)) / (i + 1);
  return r;
}

// ---- scope mutex helpers -----------------------------------------------
// A SINGLE-SLOT group (the exilus slot, the arcane slot, one evolution tier)
// holds either "these are the options" or "it is this one" — never both. The
// two used to BLOCK each other, and asymmetrically: any pool mark greyed out
// every req, while a pin still let you click pool (user, 2026-07-31: 点了候选
// 就没法点必带，但是点了必带可以切到候选，能不能直接打通).
//
// Blocking was the wrong answer to a question that has an obvious one. The
// marks are not in conflict, they are two ways of saying what the slot does,
// so the LAST click wins and the group is rewritten to mean it: req clears
// the pools, pool clears the pin. Nothing is refused and the scope never
// lies about itself — the same rule `clearFamMarks` already applies to
// families.
function setSingleSlotMark(map, id, want) {
  if ((map[id] || "off") === want) {   // clicking the ON seg turns it off
    delete map[id];
    return;
  }
  // req pins the slot: every other mark in the group goes, pools included.
  // pool opens it for search: a pin cannot survive that, but other pools can.
  Object.keys(map).forEach((o) => {
    if (o === id) return;
    if (want === "fixed" || map[o] === "fixed") delete map[o];
  });
  map[id] = want;
}

// Family exclusivity is a GAME rule across all 9 slots: req'ing a mod kills
// its family siblings everywhere (e.g. req Primed Pistol Gambit → plain
// Pistol Gambit can be neither pool nor req). The UI greys them out; setting
// a req actively clears conflicting marks so the scope never lies.
function famReqBy(m) {
  if (!m || !m.family) return null;
  const hit = (map) => Object.keys(map).find((id) => map[id] === "fixed" && id !== m.id && (modById(id) || {}).family === m.family);
  return hit(opt.mods) || hit(opt.exilus) || null;
}
function clearFamMarks(id) {
  const m = modById(id);
  if (!m || !m.family) return;
  [opt.mods, opt.exilus].forEach((map) => Object.keys(map).forEach((o) => {
    if (o !== id && (modById(o) || {}).family === m.family) delete map[o];
  }));
}
const reqCountMain = () => Object.values(opt.mods).filter((s) => s === "fixed").length;
const exilusPinned = () => Object.keys(opt.exilus).find((id) => opt.exilus[id] === "fixed") || null;
// The arcane pinned in a given POOL. A weapon with two slots has two
// independent pins — one Primary and one Secondary — so the question only
// means something with a pool attached.
const arcanePinnedIn = (pool) =>
  Object.keys(opt.arcanes).find(
    (id) => opt.arcanes[id] === "fixed" && (arcaneById(id) || {}).slot === pool,
  ) || null;
/// Options this pool contributes to the search: a pin is one, otherwise the
/// marked ones PLUS the empty choice — "no arcane in this slot" is always
/// reachable, so a scope never forces a slot to be filled.
const arcaneOptionsIn = (pool) => {
  if (arcanePinnedIn(pool)) return 1;
  const marked = Object.keys(opt.arcanes).filter(
    (id) => opt.arcanes[id] === "search" && (arcaneById(id) || {}).slot === pool,
  ).length;
  return marked + 1;
};
const evoPinned = (tier) => { const m = opt.evos[tier] || {}; return Object.keys(m).find((id) => m[id] === "fixed") || null; };

function renderOpt() {
  // Every weapon is optimizable: the scope is built from the weapon's OWN
  // pools (mod class, arcane slot, evolution tiers), so nothing here is
  // weapon-specific. META is the only prerequisite.
  show("opt-block", !!META);
  if (!META) return;
  // A scope for an axis the weapon does not have is a heading over nothing —
  // and worse, an invitation to configure a slot it cannot equip. The same
  // three facts the builder hides its blocks on (user, 2026-08-01).
  const w = weaponInfo($("weapon").value) || {};
  const AX = weaponAxes(w.id);
  // The fight's Warframe buffs, read-only. Painted here as well as from
  // `renderOptEnemy`, because arriving on this tab is its own moment: the
  // scenario may have gained a buff while you were in the simulator.
  renderWfBuffs("opt-wfbuffs", true);
  // A WEAPON WITH ONE WAY TO BE FIRED HAS NO AXIS HERE. The builder still
  // STATES its one mode (a fact about the weapon); a search over one option is
  // not a scope, so this section is simply absent — the same rule the exilus,
  // arcane and evolution sections follow.
  show("opt-modes-sect", modeOpts(w).length > 0);
  show("opt-valence-sect", !!valenceSpec(w.id));
  show("opt-exilus-sect", AX.hasExilus);
  show("opt-arcanes-sect", AX.arcanes.length > 0);
  show("opt-evos-sect", AX.evolutions.length > 0);
  // Seed scope from the current build once: equipped mods = fixed.
  if (!optSeeded) {
    opt.mods = {}; opt.exilus = {};
    // Everything equipped seeds as REQ (pinned) — first-ever content for
    // the auto-created "search 1"; afterwards the ACTIVE preset is the
    // scope (document model) and immediately overwrite this seed.
    slots.slice(0, 8).forEach((s) => { if (s.mod) opt.mods[s.mod] = "fixed"; });
    if (slots[EXILUS].mod) opt.exilus[slots[EXILUS].mod] = "fixed";
    opt.arcanes = {};
    arcanes.filter((a) => a && a !== "none").forEach((a) => { opt.arcanes[a] = "fixed"; });
    opt.evos = {};
    Object.entries(evoSel).forEach(([t, id]) => { if (id) opt.evos[t] = { [id]: "fixed" }; });
    // The build's own mode seeds as REQ, like everything else equipped — so a
    // scope opened for the first time searches the weapon the way you are
    // holding it, not every way it can be held.
    opt.modes = modeOpts(w).length ? { [mode]: "fixed" } : {};
    // NOTHING CROSSES BETWEEN WEAPONS: the valence axis is seeded from the
    // build's own element, like the mode is.
    opt.valence = valenceSpec(w.id) ? { [valence.element]: "fixed" } : {};
    optSeeded = true;
    bootstrapOptPresets();
  }
  renderOptMods();
  renderOptPresetBars();
  renderOptModes();
  renderOptValence();
  renderOptExilus();
  renderOptArcanes();
  renderOptEvos();
  renderOptEnemy();
  updateOptEstimate();
  renderOptBuffs();
}

// The buffs across the WHOLE scope (union of every fixed/search mod + every
// searched arcane + every marked evolution option) — enumerated server-side;
// The SCENARIO's buffs, READ-ONLY — the optimizer reads the simulator the way
// the simulator reads the builder (user, 2026-08-02).
//
// It used to keep its own: a scope-wide union with its own stack settings,
// because a candidate carries mods the current build does not. That bought one
// real thing and cost a worse one — the two modules scored the same fight
// under different buffs, and "add this winner, then Run Sim" only matched
// because adding a winner secretly copied the search's config into your
// scenario. One fight, one buff config, and the disagreement cannot exist.
//
// The list is the WIDE one (`fetchAllBuffs`: every buff this weapon could
// produce, cached per weapon), because a search covers builds you are not
// holding — which is exactly what the scenario's "all potential buffs" view is
// for. A buff nobody set falls to its own default, which is now 0 for anything
// timed: a candidate is credited with a stack only if the fight says so.
async function renderOptBuffs() {
  const box = $("opt-buffs");
  if (!box) return;
  renderBuffCards(box, await fetchAllBuffs(), sim.buffs, null, { readonly: true });
}

// The mod scope: the SAME rich list as the mod picker (image, polarity icon,
// sort / polarity filter, effect lines) — the only difference is the rightmost
// control is pool/req instead of the drain. Plus a summary of selections and
// a "mods per build" size. `optPrefs` mirrors the picker's sort/filter.
function renderOptMods() {
  $("opt-size").value = opt.size;
  $("opt-min").value = opt.min;
  renderOptTools();
  renderOptModSel();
  renderOptModList();
}

// The optimizer does NOT own a scenario (user, 2026-08-02): it runs the
// SIMULATOR's, drawn here by the same renderer so the two cannot drift and a
// scenario preset switched on either tab is switched on both. What the search
// owns is its funnel — how many candidates survive each round — and that is
// the block below the buffs, not this one.
//
// No Runs/Measure section here: the funnel decides run counts round by round,
// so the engagement LENGTH is the only measurement input the search takes,
// and it sits beside the enemy.
function renderOptEnemy() {
  if (!$("opt-target")) return;
  renderWfBuffs("opt-wfbuffs", true);
  renderScenarioFields(
    { target: "opt-target", technique: "opt-technique", limits: "opt-limits",
      extra: "opt-extra" },
    { readonly: true },
  );
  // Which fight, and where it is edited. Not a preset bar: that bar can
  // rename, duplicate, delete and import, all of which are edits, and the
  // scenario collection is the SIMULATOR's to edit.
  const ref = $("opt-scenario-ref");
  if (ref) {
    const w = weaponInfo($("weapon").value) || {};
    ref.innerHTML =
      `<span class="plabel">${escHtml(tr("Scenario"))}</span>` +
      `<span class="pchip sel" title="${escHtml(tr("the scenario the simulator is set to"))}">${escHtml(presetLabel(scenarioNamed(activeScenario)) || tr("current"))}</span>` +
      `<a class="pchip" href="${weaponPath(w.id)}/simulator">${escHtml(tr("edit in the Simulator"))} →</a>`;
  }
}

// Exilus-slot scope (the +1 slot) — exilus-eligible mods with the same
// pool/req segs as the main list: pool = a slot option (empty always
// allowed), req = pin the slot (max one). The same mods may ALSO be marked
// in the main scope above — all 9 slots accept exilus mods; the search
// never equips one twice.
function renderOptExilus() {
  const pinned = exilusPinned();
  const hasPool = Object.values(opt.exilus).some((s) => s === "search");
  const row = (m) => {
    const st = opt.exilus[m.id] || "off";
    const fam = famReqBy(m);
    // Only a FAMILY conflict can kill a row: pool and req no longer block
    // each other — clicking one rewrites the group (setSingleSlotMark).
    const poolDead = !!fam;
    const reqDead = !!fam;
    const why = fam ? `excluded: ${(modById(fam) || { name: fam }).name} is required (same family)` : "";
    const eff = cardLines(m, m.max_rank).map((x) => `<div>${x}</div>`).join("");
    return `<div class="opt ${st === "off" ? "" : st} ${fam ? "dis-soft" : ""} ${m.rarity ? "rar-" + m.rarity : ""}" title="${why}">
      ${imgTag(POL(m.polarity), "pol")}${imgTag(IMG(m.image), "mod")}
      <div class="info"><div class="mn">${wl(m.name, wikiUrl(m.name_en || m.name))}</div><div class="me">${eff}</div></div>
      <div class="oseg">
        <span class="seg ${st === "search" ? "on" : ""} ${poolDead ? "dis" : ""}" data-m="${m.id}" data-s="search">${tr("pool")}</span>
        <span class="seg ${st === "fixed" ? "on" : ""} ${reqDead ? "dis" : ""}" data-m="${m.id}" data-s="fixed" ${!reqDead && hasPool ? `title="${escHtml(tr("req pins the slot — the pool marks give way"))}"` : ""}>${tr("req")}</span>
      </div>
    </div>`;
  };
  $("opt-exilus").innerHTML = weaponAxes().exilus.map(row).join("")
    || `<div class="opt dis">no exilus mods in this pool</div>`;
  $("opt-exilus").querySelectorAll(".seg:not(.dis)").forEach((el) =>
    el.addEventListener("click", (e) => {
      e.stopPropagation();
      const id = el.dataset.m, want = el.dataset.s;
      setSingleSlotMark(opt.exilus, id, want);
      if (opt.exilus[id] === "fixed") clearFamMarks(id);
      renderOptMods(); renderOptExilus(); updateOptEstimate();
    }));
}

// Arcane scope — the SAME rich rows as the arcane picker (image, name, effect
// lines), searchable, with an include toggle on the right. Marking nothing is
// what searches the empty slot, so there is no "None" row to mark.
/// THE MODE AXIS — how the weapon is played, as a search dimension.
///
/// Mode belongs to the BUILD (2026-08-07), which is why the builder has a
/// control for it and the simulator does not. The optimizer is the third case
/// and it is neither: it binds a SET where the builder binds a value, exactly
/// as it does for mods, arcanes and evolutions. Before this the builder's own
/// Mode block was simply drawn on this tab, where it looked like a setting and
/// was not one — the request carried no mode at all, so picking the Phantasma's
/// charged mode here searched its base form and said nothing (owner,
/// 2026-08-11).
///
/// An UNSUSTAINABLE mode is still offered. `play_modes` marks the ones a board
/// may rank, which is a rule about the leaderboard rather than about what a
/// player may search — and the builder offers them all.
function renderOptModes() {
  const box = $("opt-modes");
  if (!box) return;
  const w = weaponInfo($("weapon").value) || {};
  const opts = modeOpts(w);
  // NEVER EMPTY, and asserted HERE rather than only where the scope is seeded:
  // a preset written before this axis existed carries no modes, a scope
  // imported from another weapon carries modes this one does not have, and
  // both arrive as "no mode at all" — which is not "search them all", it is a
  // question with no answer. The build's own mode is the answer, the same one
  // an empty scope seeds with.
  if (opts.length && !Object.keys(opt.modes).length) opt.modes = { [mode]: "fixed" };
  const marks = Object.keys(opt.modes).filter((id) => opts.some(([o]) => o === id));
  const pinned = marks.find((id) => opt.modes[id] === "fixed") || null;
  const hasPool = marks.some((id) => opt.modes[id] === "search");
  box.innerHTML = opts
    .map(([id, label, offReason]) => {
      const st = opt.modes[id] || "off";
      return `<div class="opt ${st === "off" ? "" : st} ${offReason ? "dis" : ""}">
        <div class="info"><div class="mn">${escHtml(label)}</div>${
          offReason ? `<div class="ef warn">⊘ ${escHtml(offReason)}</div>` : ""}</div>
        <div class="oseg">
          <span class="seg ${st === "search" ? "on" : ""}${offReason ? " dis" : ""}" data-m="${escHtml(id)}" data-s="search"${
            pinned && pinned !== id ? ` title="${escHtml(tr("pooling opens the slot — the pin gives way"))}"` : ""}>${tr("pool")}</span>
          <span class="seg ${st === "fixed" ? "on" : ""}${offReason ? " dis" : ""}" data-m="${escHtml(id)}" data-s="fixed"${
            hasPool ? ` title="${escHtml(tr("req pins the slot — the pool marks give way"))}"` : ""}>${tr("req")}</span>
        </div>
      </div>`;
    })
    .join("");
  box.querySelectorAll(".seg:not(.dis)").forEach((el) =>
    el.addEventListener("click", (e) => {
      e.stopPropagation();
      const id = el.dataset.m, want = el.dataset.s;
      // ONE GROUP, because a build is played ONE way: the marks here behave
      // like a single slot's — `req` pins it and clears the pool, `pool` opens
      // it and gives way on the pin.
      const was = opt.modes[id];
      if (want === "fixed") {
        opt.modes = { [id]: "fixed" };
      } else if (was === "search") {
        delete opt.modes[id];
      } else {
        Object.keys(opt.modes).forEach((k) => { if (opt.modes[k] === "fixed") delete opt.modes[k]; });
        opt.modes[id] = "search";
      }
      if (was === "fixed" && want === "fixed") delete opt.modes[id];
      // NEVER EMPTY. A scope with no mode is not "search them all", it is a
      // question with no answer — the server would fall back to the request's
      // single mode and the screen would not say so.
      if (!Object.keys(opt.modes).length) opt.modes = { [id]: "fixed" };
      renderOptModes();
      updateOptEstimate(); // the scope's auto-save
    })
  );
}

/// THE VALENCE AXIS, searched exactly like the mode: `pool` opens it, `req`
/// pins one, and the two marks behave as one group because a weapon has ONE
/// progenitor element (owner, 2026-08-13).
///
/// Never empty for a weapon that has the axis — a scope with no element is not
/// "search them all", it is a question with no answer, and the server would
/// fall back to the request's single element without the screen saying so.
function renderOptValence() {
  const box = $("opt-valence");
  if (!box) return;
  const w = weaponInfo($("weapon").value) || {};
  const s = valenceSpec(w.id);
  if (!s) { box.innerHTML = ""; return; }
  if (!Object.keys(opt.valence || {}).length) {
    opt.valence = { [valence.element]: "fixed" };
  }
  const marks = Object.keys(opt.valence).filter((id) => s.elements.includes(id));
  const pinned = marks.find((id) => opt.valence[id] === "fixed") || null;
  const hasPool = marks.some((id) => opt.valence[id] === "search");
  box.innerHTML = s.elements
    .map((id) => {
      const st = opt.valence[id] || "off";
      return `<div class="opt ${st === "off" ? "" : st}">
        <div class="info"><div class="mn">${escHtml(DT(id))}</div></div>
        <div class="oseg">
          <span class="seg ${st === "search" ? "on" : ""}" data-m="${escHtml(id)}" data-s="search"${
            pinned && pinned !== id ? ` title="${escHtml(tr("pooling opens the slot — the pin gives way"))}"` : ""}>${tr("pool")}</span>
          <span class="seg ${st === "fixed" ? "on" : ""}" data-m="${escHtml(id)}" data-s="fixed"${
            hasPool ? ` title="${escHtml(tr("req pins the slot — the pool marks give way"))}"` : ""}>${tr("req")}</span>
        </div>
      </div>`;
    })
    .join("");
  box.querySelectorAll(".seg").forEach((el) =>
    el.addEventListener("click", (e) => {
      e.stopPropagation();
      const id = el.dataset.m, want = el.dataset.s;
      const was = opt.valence[id];
      if (want === "fixed") {
        opt.valence = { [id]: "fixed" };
      } else if (was === "search") {
        delete opt.valence[id];
      } else {
        Object.keys(opt.valence).forEach((k) => { if (opt.valence[k] === "fixed") delete opt.valence[k]; });
        opt.valence[id] = "search";
      }
      if (was === "fixed" && want === "fixed") delete opt.valence[id];
      if (!Object.keys(opt.valence).length) opt.valence = { [id]: "fixed" };
      renderOptValence();
      updateOptEstimate();
    })
  );
}

function renderOptArcanes() {
  const q = ($("opt-arc-filter") && $("opt-arc-filter").value || "").trim().toLowerCase();
  const axes = weaponAxes().arcanes;
  const row = (a, pinned, hasPool) => {
    const st = opt.arcanes[a.id] || "off";
    // Neither mark blocks the other: clicking one rewrites the group
    // (setSingleSlotMark), so every seg here is live.
    const eff = effLines(cardLines(a, a.max_rank, effectsAt(a, a.max_rank)));
    return `<div class="opt ${a.rarity ? "rar-" + a.rarity : ""} ${st === "off" ? "" : st}">
      ${imgTag(IMG(a.image), "mod")}
      <div class="info"><div class="mn">${wl(a.name, wikiUrl(a.name_en || a.name))}${optGainChipFor(a.id)}</div>${eff}</div>
      <div class="oseg">
        <span class="seg ${st === "search" ? "on" : ""}" data-a="${a.id}" data-s="search" ${pinned && pinned !== a.id ? `title="${escHtml(tr("pooling opens the slot — the pin gives way"))}"` : ""}>${tr("pool")}</span>
        <span class="seg ${st === "fixed" ? "on" : ""}" data-a="${a.id}" data-s="fixed" ${hasPool ? `title="${escHtml(tr("req pins the slot — the pool marks give way"))}"` : ""}>${tr("req")}</span>
      </div>
    </div>`;
  };
  // ONE SECTION PER SLOT. An arcane belongs to exactly one pool, so the flat
  // `opt.arcanes` map already says which slot each mark is for — what has to
  // be per-pool is the RULE: "req pins the slot" pins THAT slot, and a pin in
  // the Primary section has nothing to do with the Secondary one. The section
  // header is drawn only when there is more than one, as everywhere else.
  $("opt-arcanes").innerHTML = axes
    .map(({ pool, options }, i) => {
      const inPool = options.filter((a) => !q || searchBlob(a).includes(q));
      const ids = new Set(options.map((a) => a.id));
      const marks = Object.keys(opt.arcanes).filter((id) => ids.has(id));
      const pinned = marks.find((id) => opt.arcanes[id] === "fixed") || null;
      const hasPool = marks.some((id) => opt.arcanes[id] === "search");
      const head = axes.length > 1
        ? `<div class="menu-head">${escHtml(tr(SLOT_LABEL[pool] || pool))}</div>`
        : "";
      const rows = inPool.map((a) => row(a, pinned, hasPool)).join("")
        || `<div class="opt dis">${escHtml(tr("no matches"))}</div>`;
      return `<div class="menu-sect">${head}${rows}</div>`;
    })
    .join("");
  $("opt-arcanes").querySelectorAll(".seg:not(.dis)").forEach((el) =>
    el.addEventListener("click", (e) => {
      e.stopPropagation();
      // The group is this arcane's OWN pool: pinning a Primary must not
      // clear a Secondary mark, because they fill different slots.
      const own = arcaneById(el.dataset.a);
      const group = {};
      Object.keys(opt.arcanes).forEach((id) => {
        if ((arcaneById(id) || {}).slot === (own || {}).slot) group[id] = opt.arcanes[id];
      });
      setSingleSlotMark(group, el.dataset.a, el.dataset.s);
      Object.keys(opt.arcanes).forEach((id) => {
        if ((arcaneById(id) || {}).slot === (own || {}).slot) delete opt.arcanes[id];
      });
      Object.assign(opt.arcanes, group);
      renderOptArcanes(); updateOptEstimate();
    }));
}

// Evolution scope — per tier, the option rows with their verbatim description
// and a search toggle (broken evolutions flagged).
function renderOptEvos() {
  const tiers = weaponEvos();
  // The same LADDER the builder draws: a tier is markable only once the one
  // before it has a mark, because every set the search enumerates installs
  // one option per marked tier — mark tier 2 with tier 1 blank and every set
  // it produces skips a rung. The scope cannot express what the sim would
  // then refuse to price.
  const optOpenTo = (() => {
    let n = 0;
    for (const t of tiers) { if (!Object.keys(opt.evos[t.tier] || {}).length) break; n = t.tier; }
    return n + 1;
  })();
  // A scope preset saved before the rule existed can still carry marks above
  // the gap. Drop them here so what is drawn, what is counted in the estimate
  // and what is sent all say the same thing — the server truncates the sets
  // either way, and a mark that changes nothing is worse than no mark.
  tiers.forEach((t) => { if (t.tier > optOpenTo) delete opt.evos[t.tier]; });
  $("opt-evos").innerHTML = tiers.map((t) => {
    const sel = opt.evos[t.tier] || {};
    const pinned = evoPinned(t.tier);
    const locked = t.tier > optOpenTo;
    const hasPool = Object.values(sel).some((s) => s === "search");
    const rows = t.options.map((o) => {
      const st = sel[o.id] || "off";
      // Neither mark blocks the other — clicking one rewrites the tier.
      const desc = evoLines(o).map((x) => `<div>${escHtml(x)}</div>`).join("");
      return `<div class="opt ${st === "off" ? "" : st} ${o.broken ? "dis-soft" : ""}">
        <div class="info"><div class="mn">${o.name}${o.broken ? ' <span class="exchip brk">BROKEN</span>' : ""}${
          evoGapChips(o, "span")
        }${optGainChipFor(o.id)}</div><div class="me">${desc}</div>${optPairingNoteFor(o.id)}</div>
        <div class="oseg">
          <span class="seg ${st === "search" ? "on" : ""} ${locked ? "tlocked" : ""}" data-t="${t.tier}" data-e="${o.id}" data-s="search" ${pinned && pinned !== o.id ? `title="${escHtml(tr("pooling opens the tier — the pin gives way"))}"` : ""}>${tr("pool")}</span>
          <span class="seg ${st === "fixed" ? "on" : ""} ${locked ? "tlocked" : ""}" data-t="${t.tier}" data-e="${o.id}" data-s="fixed" ${hasPool ? `title="${escHtml(tr("req pins the tier — the pool marks give way"))}"` : ""}>${tr("req")}</span>
        </div>
      </div>`;
    }).join("");
    return `<div class="opt-tier-block${locked ? " locked" : ""}" ${locked
      ? `title="${escHtml(tr("install the previous tier first"))}"` : ""
    }><div class="opt-tier-h">EVO ${ROMAN(t.tier)}</div><div class="combo-menu opt-evolist">${rows}</div></div>`;
  }).join("");
  $("opt-evos").querySelectorAll(".seg:not(.dis):not(.tlocked)").forEach((el) =>
    el.addEventListener("click", (e) => {
      e.stopPropagation();
      const t = el.dataset.t, id = el.dataset.e, want = el.dataset.s;
      opt.evos[t] = opt.evos[t] || {};
      setSingleSlotMark(opt.evos[t], id, want);
      // Clearing a tier shuts every tier above it, marks and all — the same
      // cascade the builder does, for the same reason.
      if (!Object.keys(opt.evos[t]).length) {
        tiers.forEach((x) => { if (x.tier > Number(t)) delete opt.evos[x.tier]; });
      }
      renderOptEvos(); updateOptEstimate();
    }));
}

// ---- The optimizer preset — ONE document per weapon (user, 2026-08-02).
//
// It was three (mods / arcanes / evolutions), split so the parts could be
// reused across weapons. They no longer need to be: preset storage is
// weapon-scoped (`wfsim-presets-<weapon>-optimizer`), and carrying a search to
// another weapon is the explicit IMPORT, which filters per axis. Three lists
// bought nothing and cost three bootstraps, three actives and three bars over
// one search.
//
// What it holds is the SEARCH: the scope (mods + exilus + max size, arcanes,
// per-tier evolutions) and the funnel's final-round contract. NOT the
// scenario — the optimizer does not own one; it runs the simulator's, which
// has its own preset domain.
//
// `threads` stays out too: it is a property of the MACHINE, not of a search,
// and a preset carrying it would re-tune the CPU on every load.
const OPT_DOMAIN = "optimizer";
// Same DOCUMENT MODEL as every other bar (user, 2026-07-28: "all presets use
// this model"): there is always >=1 preset, one is always active and being
// edited, edits auto-save in place, and the active survives a reload.
let activeOptPreset = null;

const loadOptPresets = () => loadPresetList(OPT_DOMAIN);
const storeOptPresets = (ps) => storePresetList(OPT_DOMAIN, ps);

// Called from renderOpt's seed block (page load AND weapon switch). The
// first-ever run creates "search 1" from the build-seeded scope; afterwards
// the active preset IS the scope.
function bootstrapOptPresets() {
  let ps = loadOptPresets();
  if (!ps.length) {
    // Named after what it is, like the scenario and riven collections. An
    // existing "preset 1" is the user's name now and is left alone.
    ps = [{ name: "search 1", savedAt: Date.now(), state: snapshotOpt() }];
    storeOptPresets(ps);
  }
  const want = activeOptPreset || localStorage.getItem(presetActiveKey(OPT_DOMAIN));
  activeOptPreset = ps.some((p) => p.name === want) ? want : ps[0].name;
  localStorage.setItem(presetActiveKey(OPT_DOMAIN), activeOptPreset);
  applyOptState(ps.find((p) => p.name === activeOptPreset).state);
}

// One-time merges, oldest first: the single legacy bar was split into three
// groups, and the three are now one again. Both run over whatever is on the
// machine, so a browser that skipped a release still lands on the current
// shape. Names are the join key — a preset named "crit" in each group was one
// search described three times, which is exactly what it becomes.
(function migrateOptPresets() {
  const parse = (k) => { try { return JSON.parse(localStorage.getItem(k)); } catch (_) { return null; } };
  // Step 1 (2026-07-28): one bar -> three groups, under the current weapon.
  const legacy = parse("wfsim-opt-presets");
  if (Array.isArray(legacy)) {
    legacy.forEach((p) => {
      const st = p.state || {};
      [["mods", { mods: st.mods || {}, exilus: (st.exilus && typeof st.exilus === "object") ? st.exilus : {}, size: st.size || 8 }],
       ["arcanes", { arcanes: st.arcanes || {} }],
       ["evolutions", { evos: st.evos || {} }]].forEach(([g, state]) => {
        const key = "wfsim-presets-" + presetWeapon() + "-optimizer-" + g;
        const ps = parse(key) || [];
        if (!ps.some((x) => x.name === p.name)) {
          ps.push({ name: p.name, savedAt: p.savedAt || Date.now(), state });
          localStorage.setItem(key, JSON.stringify(ps));
        }
      });
    });
    localStorage.removeItem("wfsim-opt-presets");
  }
  // Step 2 (2026-08-02): three groups -> one, for EVERY weapon that has them.
  const groups = ["mods", "arcanes", "evolutions"];
  const weapons = new Set();
  for (let i = 0; i < localStorage.length; i++) {
    const m = /^wfsim-presets-(.+)-optimizer-(mods|arcanes|evolutions)$/.exec(localStorage.key(i));
    if (m) weapons.add(m[1]);
  }
  weapons.forEach((w) => {
    const merged = parse(`wfsim-presets-${w}-optimizer`) || [];
    const byName = new Map(merged.map((p) => [p.name, p]));
    groups.forEach((g) => {
      (parse(`wfsim-presets-${w}-optimizer-${g}`) || []).forEach((p) => {
        const into = byName.get(p.name)
          || { name: p.name, savedAt: p.savedAt || Date.now(), state: {} };
        into.state = { ...into.state, ...(p.state || {}) };
        byName.set(p.name, into);
      });
    });
    if (byName.size) {
      localStorage.setItem(`wfsim-presets-${w}-optimizer`, JSON.stringify([...byName.values()]));
      // The three old actives disagree by construction (three bars, three
      // choices); the mod scope is the one that decided what the search was.
      const act = localStorage.getItem(`wfsim-preset-active-${w}-optimizer-mods`);
      if (act && byName.has(act)) localStorage.setItem(`wfsim-preset-active-${w}-optimizer`, act);
    }
    groups.forEach((g) => {
      localStorage.removeItem(`wfsim-presets-${w}-optimizer-${g}`);
      localStorage.removeItem(`wfsim-preset-active-${w}-optimizer-${g}`);
    });
  });
})();

function snapshotOpt() {
  return {
    mods: { ...opt.mods }, exilus: { ...opt.exilus }, size: opt.size, min: opt.min,
    arcanes: { ...opt.arcanes }, modes: { ...opt.modes },
    evos: JSON.parse(JSON.stringify(opt.evos)),
    finalists: optRun.finalists,
    threads: optRun.threads,
    runs: optRun.runs,
  };
}

// An empty search: nothing marked, a fresh size, the contract left alone (it
// is how hard to search, not what to search).
const blankOpt = () => ({ mods: {}, exilus: {}, size: 8, arcanes: {}, evos: {},
  // The build's own mode, the way an empty scope seeds every other axis from
  // what you are holding.
  modes: { [mode]: "fixed" },
  finalists: optRun.finalists, threads: optRun.threads, runs: optRun.runs });

// State-only apply (validation + cross-weapon id dropping); no re-render.
//
// Every axis drops what THIS weapon cannot hold, which is what makes a preset
// carried over from another weapon land as "the part that still applies"
// rather than as a search the run cannot execute.
function applyOptState(st) {
  const norm = (s) => (s === true ? "search" : s); // boolean-era marks
  // Mods: ids missing from this weapon's pool drop out.
  opt.mods = {}; opt.exilus = {};
  Object.entries(st.mods || {}).forEach(([id, s]) => { if (modById(id)) opt.mods[id] = norm(s); });
  Object.entries(st.exilus || {}).forEach(([id, s]) => { const m = modById(id); if (m && m.exilus) opt.exilus[id] = norm(s); });
  delete opt.exilus["none"]; // brief None-row era
  if (st.size) opt.size = st.size;
  // Presets written before the range existed carry no min; 1 is what they meant.
  opt.min = Math.min(st.min || 1, opt.size);
  // Arcanes: another SLOT's arcanes are not equippable here, so they drop
  // rather than becoming search dimensions the run cannot use.
  opt.arcanes = {};
  const w = $("weapon").value;
  Object.entries(st.arcanes || {}).forEach(([id, s]) => {
    // ANY of the weapon's pools, not just the first — see arcaneFitsWeapon.
    // (A stored "none" fails this too, which is what we want.)
    if (arcaneFitsWeapon(w, id)) opt.arcanes[ARCANE_RENAMED[id] || id] = norm(s);
  });
  // Modes: a scope carried over from another weapon names modes this one does
  // not have, and a search over none of them is not a scope — so what does not
  // apply drops and the build's own mode stands in.
  opt.modes = {};
  const mopts = modeOpts(weaponInfo(w) || {});
  Object.entries(st.modes || {}).forEach(([id, s]) => {
    if (mopts.some(([o]) => o === id)) opt.modes[id] = norm(s);
  });
  if (mopts.length && !Object.keys(opt.modes).length) opt.modes = { [mode]: "fixed" };
  // Evolutions: keep only ids the CURRENT weapon's tiers actually offer (ids
  // are globally unique, so a family sharing evolutions imports cleanly and a
  // different weapon's ids just drop).
  const tiers = weaponEvos();
  opt.evos = {};
  Object.entries(st.evos || {}).forEach(([t, m]) => {
    const tier = tiers.find((x) => String(x.tier) === String(t));
    if (!tier) return;
    const valid = {};
    Object.entries(m || {}).forEach(([id, s]) => {
      if (tier.options.some((o) => o.id === id)) valid[id] = norm(s);
    });
    if (Object.keys(valid).length) opt.evos[t] = valid;
  });
  // How the search RUNS travels with the search it was tuned for. `runs` is
  // read as its own field and NOT from the `final_runs` an old preset may
  // still carry: that one was written when the setting meant something else,
  // and 0/absent is the reading we want from it anyway — the fight's.
  if (st.finalists) optRun.finalists = st.finalists;
  if (st.threads != null) optRun.threads = st.threads;
  optRun.runs = Math.max(0, Math.min(20000, Number(st.runs) || 0));
  const f = $("opt-finalists"), th = $("opt-threads"), rn = $("opt-runs");
  if (f) f.value = optRun.finalists;
  if (th) th.value = optRun.threads || "";
  if (rn) rn.value = optRun.runs || "";
}

function applyOptPreset(st) {
  applyOptState(st);
  optSeeded = true;
  renderOpt(); updateOptEstimate();
}

function renderOptPresetBars() {
  const bar = $("preset-bar-" + OPT_DOMAIN);
  if (!bar) return;
  renderPresetBarIn(bar, {
    domain: OPT_DOMAIN,
    label: tr("Searches"),
    noun: "search",
    hint: "scope + final round; import filters per axis",
    load: loadOptPresets,
    store: storeOptPresets,
    active: () => activeOptPreset,
    setActive: (n) => { activeOptPreset = n; localStorage.setItem(presetActiveKey(OPT_DOMAIN), n); },
    snapshot: snapshotOpt,
    apply: (st) => applyOptPreset(st || {}),
    blank: blankOpt,
    rerender: renderOptPresetBars,
  });
}

function renderOptTools() {
  const t = $("opt-picker-tools");
  const pols = ["Madurai", "Naramon", "Vazarin", "Umbra"].filter((p) => currentPool.some((m) => m.polarity === p));
  t.innerHTML =
    `<label>${escHtml(tr("Sort"))} ` + ddButton("opk-sort", {
      value: optPrefs.sort,
      items: [{ value: "name", label: tr("Name") }, { value: "drain", label: tr("Drain") }],
      onPick: (v) => { optPrefs.sort = v; renderOptTools(); renderOptModList(); },
    }) + `</label>` +
    `<button id="opk-dir" class="ghost-btn small" title="direction">${optPrefs.dir === "asc" ? "▲" : "▼"}</button>` +
    `<span class="pk-pols"><span class="pk-pol ${!optPrefs.pol ? "sel" : ""}" data-p="">all</span>` +
    pols.map((p) => `<span class="pk-pol ${optPrefs.pol === p ? "sel" : ""}" data-p="${p}" title="${p}">${imgTag(POL(p), "pol")}</span>`).join("") +
    `</span>` +
    // QUICK CALC, on a button rather than on every edit. The builder's scan
    // follows an opened slot because opening one IS the question; here every
    // click on a pool/req control would restart ~250 engagements, and the
    // scope is edited many clicks in a row.
    `<span class="pk-gain"><button id="opk-gain" class="ghost-btn small"${optGain.running ? " disabled" : ""}>${
      optGain.running ? `${optGain.done}/${optGain.total}` : escHtml(tr("quick calc"))}</button>` +
    (optWinnerMods()
      ? ddButton("opk-gain-ref", {
        value: optGain.mode,
        title: tr("the build every number is measured on"),
        items: [
          { value: "require", label: tr("vs required"), hint: tr("the mods you have pinned") },
          { value: "winner", label: tr("vs winner"), hint: tr("the build the search returned") },
        ],
        onPick: (v) => { optGain.mode = v; renderOptTools(); renderOptPairings(); renderOptModList(); renderOptArcanes(); renderOptEvos(); },
      })
      : "") +
    `</span>`;
  $("opk-dir").onclick = () => { optPrefs.dir = optPrefs.dir === "asc" ? "desc" : "asc"; renderOptTools(); renderOptModList(); };
  t.querySelectorAll(".pk-pol").forEach((o) => o.onclick = () => { optPrefs.pol = o.dataset.p || null; renderOptTools(); renderOptModList(); });
  // All three axes are on screen at once and all three now carry numbers, so
  // a tick repaints all three — a chip that appeared on one list and not the
  // others would read as "this axis was not scanned".
  const paint = () => {
    renderOptTools(); renderOptPairings(); renderOptModList();
    renderOptArcanes(); renderOptEvos();
  };
  $("opk-gain").onclick = () => scanOptGains(() => paint());
}

// A chip's ✕ removes; the chip itself REVEALS the mod in the list below.
// Making the whole chip a delete button meant reaching for a selected mod to
// look at it threw it away instead (user, 2026-07-30) — and the ✕ was sitting
// right there looking like the control that did it.
function revealOptMod(id) {
  const m = modById(id);
  if (!m) return;
  // The list is filtered; a chip must be able to reach a row the current
  // filter hides, so clear whatever would keep it off screen.
  if (optPrefs.pol && m.polarity !== optPrefs.pol) { optPrefs.pol = null; renderOptTools(); }
  const q = ($("opt-mod-filter").value || "").trim().toLowerCase();
  if (q && !searchBlob(m).includes(q)) $("opt-mod-filter").value = "";
  renderOptModList();
  const row = $("opt-mods").querySelector(`.opt .seg[data-m="${CSS.escape(id)}"]`);
  if (!row) return;
  const box = row.closest(".opt");
  box.scrollIntoView({ block: "center", behavior: "smooth" });
  box.classList.add("revealed");
  setTimeout(() => box.classList.remove("revealed"), 1600);
}

function renderOptModSel() {
  const chip = (id, cls) => {
    const m = modById(id);
    return `<span class="oselchip ${cls}" data-m="${id}" title="click to find it in the list below">`
      + `${m ? m.name : id}<button class="oselx" data-x="${id}" title="remove">✕</button></span>`;
  };
  const req = Object.keys(opt.mods).filter((id) => opt.mods[id] === "fixed").map((id) => chip(id, "fixed"));
  const pool = Object.keys(opt.mods).filter((id) => opt.mods[id] === "search").map((id) => chip(id, "search"));
  const box = $("opt-mods-sel");
  box.innerHTML =
    (req.length ? `<div class="oselrow"><span class="osellbl">required (${req.length}/${opt.size})</span>${req.join("")}</div>` : "") +
    (pool.length ? `<div class="oselrow"><span class="osellbl">pool (${pool.length})</span>${pool.join("")}</div>` : "") +
    (!req.length && !pool.length ? `<div class="sim-empty">nothing selected yet — mark mods below as pool or required.</div>` : "");
  box.querySelectorAll("[data-x]").forEach((el) =>
    el.addEventListener("click", (e) => {
      e.stopPropagation();
      delete opt.mods[el.dataset.x];
      renderOptMods(); renderOptExilus(); updateOptEstimate();
    }));
  box.querySelectorAll(".oselchip[data-m]").forEach((el) =>
    el.addEventListener("click", () => revealOptMod(el.dataset.m)));
}

/// THE PAIRING LADDER — the quick calc's first statement, above the mods.
///
/// Absent until there is a choice to make: one pairing is not a ladder, and a
/// scope with no elemental mod has nothing to say. What it reports is the
/// reference build measured every way its elements can pair, best first.
function renderOptPairings() {
  const box = $("opt-pairings");
  if (!box) return;
  const fresh = optGain.key === optGainKey();
  const rows = fresh ? optGain.orders : [];
  if (rows.length < 2) { box.innerHTML = ""; return; }
  const head = `${tr("element pairings")} · ${rows.length} · ${escHtml(optGain.metric)} · ${escHtml(optGain.note)}`;
  box.innerHTML = `<div class="pairbox"><div class="pairhead">${head}</div>${rows.map((o, i) => `
    <div class="pairrow${i === 0 ? " best" : ""}">
      <span class="pl">${pairingLabel(o.combined, o.leftover)}</span>
      <span class="pv">${o.value == null ? "—" : sig2(o.value)}</span>
      <span class="pd">${i === 0 ? tr("best") : gainPct(o.pct)}</span>
    </div>`).join("")}</div>`;
}

function renderOptModList() {
  const q = ($("opt-mod-filter").value || "").trim().toLowerCase();
  // Exilus mods are IN this list too — all 9 slots accept them (game rule),
  // so marking one here makes it compete for a MAIN slot; the exilus SLOT
  // has its own block below.
  const hits = poolWithRivens()
    .filter((m) => !optPrefs.pol || m.polarity === optPrefs.pol)
    .filter((m) => !q || searchBlob(m).includes(q))
    .sort((a, b) => {
      // Rivens first, as their own block, as in the builder's picker.
      const r = (b.riven ? 1 : 0) - (a.riven ? 1 : 0);
      if (r) return r;
      const c = optPrefs.sort === "drain" ? a.drain - b.drain : a.name.localeCompare(b.name);
      return optPrefs.dir === "desc" ? -c : c;
    });
  // The picker's `.opt` row markup verbatim; only the trailing `.dr` is
  // replaced by the pool/req control (`.oseg`). Mutex-aware: a family
  // sibling of a req'd mod is dead (game exclusivity); once required fills
  // every slot, unmarked mods can no longer join; and pooled mods RESERVE
  // one open slot — req may only grow to size−1 while any pool mark exists
  // (pinning the last slot would silently kill the search).
  const fixedN = reqCountMain();
  const poolN = Object.values(opt.mods).filter((s) => s === "search").length;
  const full = fixedN >= opt.size;
  const row = (m) => {
    const st = opt.mods[m.id] || "off";
    const fam = famReqBy(m);
    const dead = !!fam || (full && st === "off");
    // Would req'ing this row leave pooled mods with zero open slots?
    const poolAfter = poolN - (st === "search" ? 1 : 0);
    const reqBlocked = st !== "fixed" && (fixedN + 1 > opt.size - (poolAfter > 0 ? 1 : 0));
    const why = fam ? `excluded: ${(modById(fam) || { name: fam }).name} is required (same family)`
      : dead ? `all ${opt.size} slots are required already` : "";
    const eff = cardLines(m, m.max_rank).map((x) => `<div>${x}</div>`).join("");
    return `<div class="opt ${st === "off" ? "" : st} ${dead ? "dis-soft" : ""} ${m.rarity ? "rar-" + m.rarity : ""}" title="${why || (m.effects || []).join(" · ")}">
      ${imgTag(POL(m.polarity), "pol")}${imgTag(IMG(m.image), "mod")}
      <div class="info"><div class="mn">${m.riven ? escHtml(m.name) : wl(m.name, wikiUrl(m.name_en || m.name))}${m.exilus ? ' <span class="exchip">EXILUS</span>' : ""}${optGainChipFor(m.id)}</div><div class="me">${eff}</div>${optPairingNoteFor(m.id)}</div>
      <div class="oseg">
        <span class="seg ${st === "search" ? "on" : ""} ${dead ? "dis" : ""}" data-m="${m.id}" data-s="search">${tr("pool")}</span>
        <span class="seg ${st === "fixed" ? "on" : ""} ${dead || reqBlocked ? "dis" : ""}" data-m="${m.id}" data-s="fixed" ${!dead && reqBlocked ? `title="${escHtml(tr("pooled mods reserve ≥1 open slot — raise max mods or clear pools"))}"` : ""}>${tr("req")}</span>
      </div>
    </div>`;
  };
  $("opt-mods").innerHTML = hits.length
    ? sectionedRows(hits, (m) => (m.riven ? "Riven" : "Mods"), row)
    : `<div class="opt dis">${escHtml(tr("no matches"))}</div>`;
  $("opt-mods").querySelectorAll(".seg:not(.dis)").forEach((el) =>
    el.addEventListener("click", (e) => {
      e.stopPropagation();
      const id = el.dataset.m, want = el.dataset.s, cur = opt.mods[id] || "off";
      if (cur === want) delete opt.mods[id]; else opt.mods[id] = want; // toggle off if same
      if (opt.mods[id] === "fixed") clearFamMarks(id);
      renderOptMods(); renderOptExilus(); updateOptEstimate();
    }));
}

function updateOptEstimate() {
  // 8 + 1 slots, slots may stay EMPTY: builds are every subset of the main
  // scope from `required` up to `size` mods (an empty scope = the bare
  // weapon, still a legal search). `opt.exilus` scopes the +1 slot (req
  // pins it, pool adds options next to "empty"); arcanes and evolution
  // tiers are pool/req the same way.
  const fixed = Object.values(opt.mods).filter((s) => s === "fixed").length;
  const search = Object.values(opt.mods).filter((s) => s === "search").length;
  const exFixed = exilusPinned();
  const exSearch = Object.values(opt.exilus).filter((s) => s === "search").length;
  // Pooled exilus marks ARE the option set; nothing marked = the slot
  // stays empty (one option).
  const exOptions = exFixed ? 1 : Math.max(1, exSearch);
  // Required in BOTH blocks = impossible (a mod equips once).
  const dupReq = exFixed && opt.mods[exFixed] === "fixed" ? exFixed : null;
  // Slots MULTIPLY: a weapon that seats two arcanes is searched over pairs,
  // because the best Primary and the best Secondary are not independent
  // questions.
  const arcCount = arcanePools().reduce((n, p) => n * arcaneOptionsIn(p), 1);
  let evoProduct = 1;
  (weaponEvos()).forEach((t) => {
    const m = opt.evos[t.tier] || {};
    evoProduct *= evoPinned(t.tier) ? 1
      : Math.max(1, Object.values(m).filter((s) => s === "search").length);
  });
  const size = opt.size;
  // The pool group occupies ≥1 slot: with pools marked, every build carries
  // at least one pooled mod (k starts above the required count).
  // ...and `opt.min` raises that floor: "exactly 8 mods" is min 8, max 8.
  const minK = Math.max(fixed + (search > 0 ? 1 : 0), opt.min);
  let subsets = 0;
  for (let k = minK; k <= size; k++) subsets += nChooseK(search, k - fixed);
  subsets *= evoProduct * exOptions;
  const jobs = subsets * arcCount;
  // Pooled mods reserve ≥1 open slot (reachable only via shrinking max
  // mods after marking — req clicks are blocked before this point).
  const poolStarved = search > 0 && fixed >= size;
  const valid = fixed <= size && subsets > 0 && !dupReq && !poolStarved;
  // No cap (user: use local resources). Show the estimate + a heads-up when big;
  // only block genuinely invalid scopes.
  const big = jobs > 500000;
  // Scenario + funnel preview: what every candidate is actually tested
  // against — the Sim panel's enemy settings, and the successive-halving
  // schedule (survivors × runs per round; a JS mirror of schedule()).
  let scenario = "";
  if (valid) {
    const en = allEnemies().find((e) => e.id === sim.enemy) || {};
    // Mirror of schedule_to()'s auto-planned cadence: k = ceil(log8(N/F))
    // rounds, even log-space culls landing exactly on the finalists, runs
    // from a halving cost budget ((ρ/2)^i, capped at final/4), then the
    // guaranteed final. Racing/amnesty adapt this plan at runtime.
    const F = optRun.finalists, FR = finalRuns();
    const N = Math.round(jobs);
    const rounds = [];
    if (N > F) {
      const k = Math.max(1, Math.ceil(Math.log(N / F) / Math.log(8)));
      const rho = Math.pow(N / F, 1 / k);
      const growth = Math.max(1, rho / 2);
      const cap = Math.max(1, Math.floor(FR / 4));
      let field = N, runsF = 1;
      for (let i = 0; i < k; i++) {
        const keep = i + 1 === k ? F : Math.max(F, Math.round(field / rho));
        rounds.push([Math.min(cap, Math.max(1, Math.round(runsF))), keep]);
        field = keep; runsF *= growth;
      }
    }
    rounds.push([FR, F]);
    const parts = [];
    let field = Math.round(jobs);
    rounds.forEach(([r, k]) => { parts.push(`${field.toLocaleString()}×${r}`); field = Math.min(field, k); });
    // The scenario's Measure is the SIMULATOR's; the funnel still ranks by
    // kills whatever it says, so say so rather than letting the shared state
    // imply a DPS search that does not exist yet.
    const measured = sim.metric === "dps"
      ? ` · <span class="warn">${escHtml(tr("the search ranks by kills — the scenario's DPS measure applies to the simulator"))}</span>`
      : "";
    scenario = `<div class="opt-scn">each build vs <b>${en.name || sim.enemy}</b> Lv ${sim.level}${sim.steel_path ? " (SP)" : ""} · ${sim.headshot_pct}% headshots${sim.aiming ? "" : " · hip-fire"} · ${sim.duration} s engagements · planned funnel (builds×runs): ${parts.join(" → ")} → ${F} finalists at ${FR.toLocaleString()} runs (racing cuts deeper, tie-amnesty keeps up to 2×)${measured}</div>`;
  }
  // ONE total, no decomposition — "×N arcanes" leaked a search-internal
  // dimension into the summary line (user, 2026-07-29).
  $("opt-estimate").innerHTML = (valid
    ? `~<b>${Math.round(jobs).toLocaleString()}</b> candidate builds${big ? ` <span class="warn">— large; this may take a while</span>` : ""}`
    : `<span class="warn">${dupReq ? `${(modById(dupReq) || { name: dupReq }).name} is required in both blocks — a mod equips once` : poolStarved ? `pooled mods reserve ≥1 open slot — raise max mods or clear pools` : `more required (${fixed}) than slots (${size})`}</span>`) + scenario;
  // Never re-enable while a background job is still running.
  $("run-opt").disabled = !valid || optJobId != null;
  // Every scope mutation funnels through here — AUTO-SAVE into the active
  // preset (debounced), same contract as the build bar.
  clearTimeout(optSaveTimer);
  optSaveTimer = setTimeout(() => {
    if (presetApplying) return;
    const ps = loadOptPresets();
    const at = ps.findIndex((p) => p.name === activeOptPreset);
    if (at < 0) return;
    ps[at] = { ...ps[at], savedAt: Date.now(), state: snapshotOpt() };
    storeOptPresets(ps);
    renderOptPresetBars();
  }, 400);
}
let optSaveTimer = null;

// The optimize run is a BACKGROUND JOB on the server: POST /api/optimize
// returns a job_id immediately; we poll /api/optimize/status for live funnel
// progress (overall % is exact — the schedule fixes every round's sim count
// up front) and can /api/optimize/cancel. On page reload, init() reattaches
// to a still-running job via a no-id status call.
let optJobId = null;
let optPollTimer = null;
let optCancelling = false; // survives the poll's 500 ms re-renders

const postJson = (url, body) => api(url, body);

async function runOptimize() {
  clearCheckpoint(); // a fresh run supersedes any interrupted one
  $("run-opt").disabled = true; $("run-opt").textContent = "Optimizing…";
  $("opt-results").innerHTML = `<div class="placeholder">starting…</div>`;
  try {
    // pool/req collapse to effective option lists: a req pins its slot/tier
    // (single option), pools are the searched set.
    const evolutions = {};
    Object.entries(opt.evos).forEach(([t, m]) => {
      const f = Object.keys(m).find((id) => m[id] === "fixed");
      const ids = f ? [f] : Object.keys(m).filter((id) => m[id] === "search");
      if (ids.length) evolutions[t] = ids;
    });
    // The MARKS, like `mods` and `exilus` — a pin means "this slot is
    // settled", which a flat list of ids cannot say. The server splits them
    // by pool and takes the product.
    const arcs = {};
    Object.keys(opt.arcanes).forEach((id) => {
      if (opt.arcanes[id] && opt.arcanes[id] !== "off") arcs[id] = opt.arcanes[id];
    });
    // The SCENARIO's buffs, WHOLE — not pruned to the current build's, the way
    // the simulator prunes. A candidate carries mods you are not holding, and
    // a setting for one of them is exactly what the scenario's wide buff view
    // exists to record. An id no candidate has is simply never matched.
    const buffs = {};
    Object.entries(sim.buffs || {}).forEach(([id, c]) => {
      if (c) buffs[id] = { stacks: c.stacks, locked: c.locked };
    });
    const body = {
      weapon: $("weapon").value,
      mods: opt.mods,
      rivens: rivenPayload(),
      build_size: opt.size,
      build_min: opt.min,
      arcanes: arcs,
      evolutions,
      // HOW IT IS PLAYED, as a search dimension — the marks, like the arcanes'.
      // `mode` travels too and is what a scope with no axis falls back to, so
      // every caller written before this keeps meaning what it meant.
      modes: opt.modes,
      mode,
      // THE VALENCE, pinned to the builder's. It is not a search axis yet —
      // every candidate is built with the same bonus, which is the weapon the
      // replay will fire. The day it becomes an axis it joins `modes` above.
      // THE VALENCE AXIS, as marks — the same shape `modes` takes. `_element`
      // travels too and is what a scope with no axis falls back to, so every
      // caller written before this keeps meaning what it meant.
      valence: opt.valence,
      valence_element: valence.element,
      valence_bonus: valence.bonus,
      exilus: opt.exilus,
      // THE FIGHT, WHOLE AND DERIVED — never a hand-written list of its
      // fields. This was twelve of them copied out one by one, under a comment
      // claiming "the TENNO travels whole", which was true only by inspection:
      // every scenario field added since had to be remembered here, and the
      // one nobody remembered would score builds under a fight the replay
      // never runs. That is the divergence AGENTS.md's hard rule is about, and
      // a spread cannot forget (`eximus` was the field that found it).
      //
      // Safe to send everything: `parse_fight` reads the fight's fields and
      // `parse_optimize` reads only its own five, so a scenario field the
      // optimizer has no opinion about simply arrives and is used.
      ...fightPayload(snapshotScenario()),
      final_runs: finalRuns(), finalists: optRun.finalists,
      threads: optRun.threads || 0, // 0 = auto (cores − 2)
      buffs,
    };
    const r = await postJson("/api/optimize", body);
    if (!r || r.ok === false) {
      optFinish(`<div class="error">optimize failed: ${r ? r.error : "no data"}</div>`);
      return;
    }
    optJobId = r.job_id;
    pollOptimize();
  } catch (e) {
    optFinish(`<div class="error">optimize failed: ${e}</div>`);
  }
}

function optFinish(html) {
  if (optPollTimer) { clearTimeout(optPollTimer); optPollTimer = null; }
  optJobId = null;
  optCancelling = false;
  if (html !== undefined) $("opt-results").innerHTML = html;
  $("run-opt").textContent = "Run Optimizer";
  updateOptEstimate(); // re-enables the button when the scope is valid
}

async function pollOptimize() {
  let st;
  try {
    st = await postJson("/api/optimize/status", optJobId != null ? { id: optJobId } : {});
  } catch (e) {
    optFinish(`<div class="error">optimize status failed: ${e}</div>`);
    return;
  }
  if (!st || st.ok === false) {
    optFinish(`<div class="error">optimize failed: ${st ? st.error : "no data"}</div>`);
    return;
  }
  optJobId = st.job_id;
  if (st.phase === "error") {
    optFinish(`<div class="error">optimize failed: ${(st.result && st.result.error) || "unknown error"}</div>`);
    return;
  }
  if (st.phase === "done" || st.phase === "cancelled") {
    optFinish();
    if (st.result && st.result.results && st.result.results.length) {
      renderOptResults(st.result);
      // A cancel is not necessarily the end of the search — the run stopped,
      // but its resume point is still on disk. Offer it under the results.
      if (st.phase === "cancelled") appendResumeOffer();
    } else {
      $("opt-results").innerHTML = `<div class="placeholder">cancelled before anything had been ranked — no results</div>`;
    }
    return;
  }
  renderOptProgress(st);
  optPollTimer = setTimeout(pollOptimize, 500);
}

function renderOptProgress(st) {
  const pct = st.sims_planned ? Math.min(100, (100 * st.sims_done) / st.sims_planned) : 0;
  const head = st.phase === "enumerating"
    ? `enumerating candidates…${st.enumerated ? ` ${st.enumerated.toLocaleString()} so far` : ""}${st.sims_done ? ` · ${st.sims_done.toLocaleString()} screened` : ""}`
    : `round ${st.round}/${st.rounds} — ${(st.round_jobs || 0).toLocaleString()} jobs × ${st.round_runs} runs`;
  const notes = (st.notes || []).map((n) =>
    `<div class="opt-note">round ${n.round}: ${n.jobs.toLocaleString()} × ${n.runs} (${n.by_kills ? "kills" : "dmg"}) → keep ${n.kept.toLocaleString()} · best ${n.by_kills ? sig2(kpm(n.best, sim.duration)) + " KPM" : n.best.toExponential(2) + " dmg"} · ${(n.ms / 1000).toFixed(1)}s</div>`
  ).join("");
  const sub = st.phase === "enumerating"
    ? ""
    : `<div class="opt-prog-sub">${pct.toFixed(1)}% · ${st.sims_done.toLocaleString()} / ${st.sims_planned.toLocaleString()} sims${st.jobs ? ` · ${st.jobs.toLocaleString()} candidate builds` : ""}</div>`;
  $("opt-results").innerHTML = `<div class="opt-progress">
    <div class="opt-prog-head"><span>${head}</span><span class="opt-elapsed">${st.elapsed_s.toFixed(0)}s</span></div>
    <div class="opt-bar"><i style="width:${pct}%"></i></div>
    ${sub}${notes}
    <button class="ghost-btn small" id="opt-cancel" ${optCancelling ? "disabled" : ""}>${optCancelling ? "Cancelling…" : "Cancel"}</button>
  </div>`;
  // The 500 ms poll re-renders this whole block — `optCancelling` keeps the
  // button's cancelling state alive across re-renders (it used to snap back
  // to a live-looking "Cancel", making cancellation look ignored).
  $("opt-cancel").addEventListener("click", async () => {
    optCancelling = true;
    $("opt-cancel").disabled = true; $("opt-cancel").textContent = "Cancelling…";
    try { await postJson("/api/optimize/cancel", { id: optJobId }); } catch (e) { /* poll reports */ }
  });
}

// Reattach to a job that is still running server-side (e.g. after a page
// reload): a no-id status call returns the latest job.
async function reattachOptimize() {
  try {
    const st = await postJson("/api/optimize/status", {});
    if (st && st.ok !== false && (st.phase === "enumerating" || st.phase === "running")) {
      optJobId = st.job_id;
      $("run-opt").disabled = true; $("run-opt").textContent = "Optimizing…";
      renderOptProgress(st);
      optPollTimer = setTimeout(pollOptimize, 500);
      return;
    }
  } catch (e) { /* no server-side job — nothing to reattach */ }
  offerResume(); // nothing is running: a reload may have killed a wasm run
}

// The run itself is gone, but the field it had narrowed to is not. Offer to
// continue from the last completed round instead of paying for it again.
// Never auto-start: resuming costs minutes of the visitor's CPU, so it takes a
// click — and the offer only appears for the weapon the checkpoint belongs to.
function offerResume() {
  const el = resumeControl();
  if (!el) return;
  const box = $("opt-results");
  box.innerHTML = "";
  box.append(el);
}

// The same control, under a cancelled run's leaderboard.
function appendResumeOffer() {
  const el = resumeControl();
  if (el) $("opt-results").append(el);
}

function resumeControl() {
  const saved = loadCheckpoint();
  const box = $("opt-results");
  if (!saved || !box || optJobId != null) return null;
  if (saved.body.weapon !== $("weapon").value) return null;
  const cp = saved.cp;
  const el = document.createElement("div");
  el.className = "opt-resume";
  const sel = $("weapon");
  const shown = (sel.selectedOptions[0] || {}).textContent || sel.value;
  const where = cp.kind === "screen"
    ? `while screening — ${cp.start_seq.toLocaleString()} candidates walked, `
      + `${(cp.keepers.length / 2).toLocaleString()} jobs still standing`
    : `after round ${cp.round} — ${cp.alive.length.toLocaleString()} builds still standing`;
  el.innerHTML = `<div>An optimization for <b>${escHtml(shown)}</b> stopped ${where}.</div>`;
  const go = document.createElement("button");
  go.className = "ghost-btn"; go.textContent = "resume it";
  go.onclick = () => resumeOptimize(saved);
  const no = document.createElement("button");
  no.className = "ghost-btn small"; no.textContent = "discard";
  no.onclick = () => { clearCheckpoint(); el.remove(); };
  el.append(go, no);
  return el;
}

async function resumeOptimize(saved) {
  $("run-opt").disabled = true; $("run-opt").textContent = "Optimizing…";
  $("opt-results").innerHTML = `<div class="placeholder">${saved.cp.kind === "screen"
    ? `re-walking to the saved point (${saved.cp.start_seq.toLocaleString()} candidates)…`
    : `resuming from round ${saved.cp.round}…`}</div>`;
  try {
    // The STORED body, not the current form: the checkpoint describes a field
    // narrowed under that exact scope, and re-deriving the body from the UI
    // would let an edited setting resume into a run it never belonged to.
    const r = await postJson("/api/optimize", { ...saved.body, __resume: saved.cp });
    if (!r || r.ok === false) {
      clearCheckpoint();
      optFinish(`<div class="error">resume failed: ${r ? r.error : "no data"}</div>`);
      return;
    }
    optJobId = r.job_id;
    pollOptimize();
  } catch (e) {
    optFinish(`<div class="error">resume failed: ${e}</div>`);
  }
}

const prettify = (id) => id.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
const arcName = (id) => (id === "none" ? "no arcane" : ((META.arcanes || []).find((a) => a.id === id) || {}).name || prettify(id));
const evoName = (id) => {
  for (const t of weaponEvos()) { const o = t.options.find((o) => o.id === id); if (o) return o.name; }
  return prettify(id);
};

/// The ranking on screen. Kept so the quick calc can offer the WINNER as its
/// reference build — a mod measured on two required cards meets no diminishing
/// returns, and the winner is the same question asked on a build that is full.
/// Cleared with the results themselves when the weapon changes.
let optLast = null;

function renderOptResults(r) {
  optLast = r;
  const modName = (id) => (modById(id) || { name: null }).name || prettify(id);
  const rows = (r.results || []).map((res) => {
    const ex = res.exilus && res.exilus !== "none" ? `, ${modName(res.exilus)} (exilus)` : "";
    const mods = res.mods.map(modName).join(", ") + ex;
    // One id per slot: name the ones that are filled, or say there are none.
    const arcNames = asArcaneList(res.arcane, (res.arcane || []).length)
      .map((id, i) => (id && id !== "none"
        ? `${arcName(id)} r${asArcaneList(res.arcane_rank, i + 1)[i] ?? ""}`
        : ""))
      .filter(Boolean);
    const arc = arcNames.join(" + ") || tr("no arcane");
    const evos = (res.evolutions || []).map(evoName).join(" · ") || "—";
    // HOW THIS ROW WAS PLAYED, drawn only when the search RANGED over modes —
    // otherwise every row would repeat the one answer the scope already
    // states. A ranking that mixes them has to say which is which: two rows
    // with the same mods and different modes are two different builds.
    const modes = new Set((r.results || []).map((x) => x.mode).filter(Boolean));
    const md = modes.size > 1 && res.mode
      ? `<span class="opt-mode">${escHtml(modeLabel(weaponInfo($("weapon").value) || {}, res.mode))}</span>`
      : "";
    return `<div class="opt-row">
      <div class="opt-head">
        <span class="opt-rank">#${res.rank}</span>
        <span class="opt-kills">${sig2(kpm(res.kill_progress ?? res.kills, r.duration))}<small> KPM</small></span>
        <span class="opt-dps">${Math.round(res.dps || res.effective_dps || 0).toLocaleString()} DPS</span>
        <span class="opt-total">${sig2(res.kill_progress ?? res.kills)} kill score / ${Math.round(r.duration || 0)}s</span>
        <span class="forma-badge legal">${res.forma.used} Forma</span>
        <button class="ghost-btn small opt-add" title="${escHtml(tr("save as a new build"))}" data-r='${JSON.stringify(res).replace(/'/g, "&#39;")}'>+ add</button>
      </div>
      <div class="opt-detail">${md}<b>${arc}</b> · ${evos}</div>
      <div class="opt-mods">${mods}</div>
    </div>`;
  }).join("");
  // WHAT THE SEARCH COVERED, whenever it did not cover everything. This is not
  // CANCELLED — that means you stopped it and this is the best it had. This
  // means the scope is bigger than one search's budget, so the ranking below
  // was chosen from a SAMPLE. The sample is uniform over the whole space (the
  // search walks a shuffled index range), so the number is a real confidence
  // statement rather than an apology: at 3% of the space the winner is a good
  // build, not necessarily THE build.
  //
  // `exhaustive` is the other half and it is the one worth saying out loud:
  // when the search reaches the end of its space, the answer is not a
  // best-so-far, it is the optimum of everything you pooled.
  const cov = r.exhaustive
    ? `<span class="ok">${escHtml(tr("every build in this scope was searched"))}</span> · `
    : (r.coverage != null && r.coverage < 1
      ? `<span class="warn">${escHtml(tr("searched {pct}% of this scope ({n} of {total} builds) — a uniform sample, so this is a strong build rather than a proven best; pool fewer mods to search all of it"))
          .replace("{pct}", (r.coverage * 100).toFixed(r.coverage < 0.01 ? 3 : 1))
          .replace("{n}", (r.searched || 0).toLocaleString())
          .replace("{total}", Math.round(r.space || 0).toLocaleString())}</span> · `
      : "");
  $("opt-results").innerHTML = `<div class="opt-meta">${cov}${r.cancelled ? `<span class="warn">cancelled — best-so-far ranking (lower precision than a full run)</span> · ` : ""}${(r.jobs || 0).toLocaleString()} candidate builds · vs ${r.target.name} Lv ${r.target.level}${r.target.steel_path ? " (SP)" : ""} · ${r.headshot_pct ?? "?"}% headshots · ${r.duration ?? "?"} s engagements · ${r.finalists || 20} finalists × ${(r.final_runs || 1024).toLocaleString()} runs</div>${rows}`;
  $("opt-results").querySelectorAll(".opt-add").forEach((el) =>
    el.addEventListener("click", () => addResult(JSON.parse(el.dataset.r), el)));
}

// An optimizer result as a builder-builds preset STATE (snapshotState
// shape) — built without touching the build being edited. autoForma()
// works on the global `slots`, so swap in a scratch array for the plan.
function resultToState(res) {
  const live = slots;
  slots = Array.from({ length: 9 }, (_, i) => ({ mod: null, pol: innate[i], rank: null }));
  res.mods.slice(0, 8).forEach((mid, i) => {
    if (modById(mid)) { slots[i].mod = mid; slots[i].rank = modById(mid).max_rank; }
  });
  // The scope's exilus choice rides along on every result row.
  if (res.exilus && res.exilus !== "none" && modById(res.exilus)) {
    slots[EXILUS].mod = res.exilus; slots[EXILUS].rank = modById(res.exilus).max_rank;
  }
  autoForma(); // minimum-Forma polarities, same as a hand-loaded build
  const sl = slots.map((s) => ({ mod: s.mod, pol: s.pol, rank: s.rank }));
  slots = live;
  const evo = { 1: null, 2: null, 3: null, 4: null };
  (res.evolutions || []).forEach((id) => {
    const t = (weaponEvos()).find((tt) => tt.options.some((o) => o.id === id));
    if (t) evo[t.tier] = id;
  });
  return {
    weapon: $("weapon").value,
    evoSel: evo,
    // The optimizer reports one id PER SLOT, in the same pool order the
    // builder uses — so applying a result is a copy, not a translation.
    arcane: asArcaneList(res.arcane, arcanePools($("weapon").value).length)
      .map((x) => x || "none"),
    arcaneRank: asArcaneList(res.arcane_rank, arcanePools($("weapon").value).length)
      .map((x) => x ?? null),
    slots: sl,
    // HOW THE WINNER IS PLAYED, and it comes from the ROW rather than from the
    // page: mode is a search dimension, so two rows of one ranking can differ
    // in it, and "add" has to carry the one that was actually scored. A row
    // from a run predating the dimension names none, and then the build takes
    // the mode the page is in — which is the mode that run used.
    mode: res.mode || mode,
    // NO `sim`. Adding a winner used to copy the optimizer's own buff config
    // into the scenario so that "add then Run Sim" matched its score. It does
    // not any more: a result is a BUILD, and a build does not get to rewrite
    // the fight you are working in (user, 2026-08-02). The two configs can
    // still disagree — the search's is scope-wide, the scenario's is this
    // build's — and that disagreement is now visible instead of resolved by
    // silently editing a preset the user owns.
  };
}

// "+ add" (not load — user, 2026-07-29): the result becomes a NEW preset
// appended after the existing builds; the build being edited is never
// clobbered. Auto-named "opt N" (rename in the preset bar if it earns a
// real name).
function addResult(res, btn) {
  const ps = loadPresetList(BUILDS);
  let n = 1;
  while (ps.some((p) => p.name === "opt " + n)) n++;
  const name = "opt " + n;
  ps.push({ name, savedAt: Date.now(), state: resultToState(res) });
  storePresetList(BUILDS, ps);
  renderPresetBar(); // the builder's bar shows the new chip when you switch back
  if (btn) { btn.textContent = "✓ " + name; btn.disabled = true; }
}

// THE BOOT IS OVER, one way or the other, and the page must say which.
//
// This used to write a failure into `.config-page`, which is hidden on the home
// page — so a boot that failed there left a blank screen and no reason, which
// is what two people reported on 2026-08-07. The banner installed in the
// document head is visible on every page and outlives an app.js that never
// parsed; this hands it the reason and clears the placeholder on success.
init()
  .then(() => {
    window.__wfsimReady = true;
    const b = document.getElementById("booting");
    if (b) b.remove();
  })
  .catch((e) => {
    if (window.__wfsimBootFailed) {
      window.__wfsimBootFailed(
        "WFSim could not start. / WFSim 启动失败。",
        String((e && (e.stack || e.message)) || e),
      );
    }
  });
