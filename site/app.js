// WFSim build configurator — PURE CONFIG. Modules: Mods / Arcane / Evolution /
// Element; each weapon enables only the ones it has. Data from /api/meta;
// official polarity icons from the wiki, art from WFCD.

const $ = (id) => document.getElementById(id);
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
// UI strings and effect phrases live in data/i18n/<locale>.yaml (served at
// /api/i18n) — nothing hardcoded here. English needs no catalog: the source
// string is the fallback.
const tr = (s) => (I18N && I18N.ui && I18N.ui[s]) || s;
const LN = (table, id, en) => (I18N && I18N[table] && I18N[table][id]) || en;
const DT = (ty) => LN("damage_types", String(ty).toLowerCase(), ty);
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
  x._search = [x.name, x.name_en, x.subtype, eff.join(" "), tf(eff.join(" ")), official.join(" ")]
    .filter(Boolean).join(" ").toLowerCase();
  return x._search;
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
const CAP = 60;
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

let wopt = null; // the emulated optimize job: { id, worker, status, result, cancelled, t0 }
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

function woptStart(body, checkpoint) {
  if (wopt && wopt.worker) {
    return { ok: false, error: "an optimization is already running — cancel it or wait", job_id: wopt.id };
  }
  const { __resume, ...req } = body ?? {}; // the resume marker is transport, not scope
  body = req;
  const job = { id: woptNextId++, worker: new Worker("/worker.js"), status: null, result: null, board: null, cancelled: false, t0: Date.now() };
  job.worker.onmessage = (e) => {
    if (e.data.kind === "progress") job.status = e.data.payload;
    // The board is what a CANCEL shows. Cancelling terminates the worker, so
    // nothing can be asked of it afterwards — the newest snapshot it managed
    // to push out is all there is, and it has to already be here.
    if (e.data.kind === "board") job.board = e.data.payload;
    if (e.data.kind === "checkpoint") {
      const { board, ...cp } = e.data.payload;
      if (board) job.board = board; // a completed round outranks a screen snapshot
      saveCheckpoint(body, cp, board || job.board);
    }
    if (e.data.kind === "result") {
      job.result = e.data.payload;
      clearCheckpoint(); // finished: nothing left to resume
      job.worker.terminate(); job.worker = null;
    }
  };
  job.worker.onerror = (e) => {
    job.result = { ok: false, error: String((e && e.message) || "worker error") };
    if (job.worker) { job.worker.terminate(); job.worker = null; }
  };
  job.worker.postMessage({ kind: "optimize", body, checkpoint: checkpoint || null });
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
  if (wopt.worker) { wopt.worker.terminate(); wopt.worker = null; wopt.cancelled = true; }
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
let innate = [];     // 9 × innate polarity name|null (exilus never innate)
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
// Sim scenario + per-buff config. Seeded from META.defaults in init().
// `buffs` maps buff id -> { stacks, locked } (section 2); the buff SET comes
// from /api/panel and syncs as the build changes.
// `aiming`: is the player holding aim? Gates the while_aiming mod effects
// (Galvanized Crosshairs / Scope, Argon Scope, Sharpened Bullets, Bladed
// Rounds, Pressurized Magazine, the Catalyzers). Defaults TRUE because that is
// what the sim silently assumed before the knob existed, so no stored preset
// changes meaning.
let sim = { enemy: "thrax_centurion", level: 9999, steel_path: true, headshot_pct: 100, aiming: true,
  infinite_ammo: true, metric: "kpm", duration: 300, runs: 100, form: "default", buffs: {} };
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
let opt = { mods: {}, exilus: {}, arcanes: {}, evos: {}, size: 8, buffs: {} };
let optSeeded = false;
// Buffs across the WHOLE optimizer scope (from /api/opt-buffs), + a debounce.
let optBuffList = [];
let optBuffTimer = null;
// Sort/polarity prefs for the optimizer mod list (independent of the picker's).
let optPrefs = { sort: "name", dir: "asc", pol: null };
// The optimizer's OWN enemy scenario (user: fully decoupled from the Sim
// panel — two independent configs that merely look alike; identical
// parameters give identical numbers, verified 2026-07-28).
let optSim = { enemy: "thrax_centurion", level: 9999, steel_path: true, headshot_pct: 100, aiming: true,
  infinite_ammo: true,
  duration: 300, form: "default" };
// The FINAL-ROUND CONTRACT (user): the funnel's last round is guaranteed
// `finalists` candidates × `final_runs` runs. Persisted; survives weapon
// switches (it is a run setting, not weapon scope).
let optRun = { final_runs: 100, finalists: 10, threads: 0 }; // threads 0 = auto (cores − 2)
try { const s = JSON.parse(localStorage.getItem("wfsim-opt-run")); if (s) optRun = { ...optRun, ...s }; } catch (_) {}
const saveOptRun = () => localStorage.setItem("wfsim-opt-run", JSON.stringify(optRun));
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
    `<select id="wsearch-sort">
       <option value="az">${tr("Name A→Z")}</option>
       <option value="za">${tr("Name Z→A")}</option>
     </select>`;
  const renderList = () => {
    const q = input.value.trim().toLowerCase();
    const list = (META.weapons || [])
      .filter((w) => flt === "all" || (w.subtype || w.mod_class) === flt)
      .filter((w) => !q || searchBlob(w).includes(q))
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
    if (e.target.id === "wsearch-sort") { srt = e.target.value; renderList(); }
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
  const sel = $("lang-select");
  if (!sel) return;
  sel.value = LANG;
  sel.addEventListener("change", () => {
    localStorage.setItem("wfsim-lang", sel.value);
    try { sessionStorage.setItem("wfsim-lang-stash", JSON.stringify(snapshotState())); } catch (_) {}
    location.reload();
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
  if (LANG !== "en") {
    try { I18N = (await api("/api/i18n"))[LANG] || null; } catch (_) { I18N = null; }
    applyNameOverlay();
  }
  applyI18n();
  fillSelect("weapon", META.weapons);
  initWeaponSearch();
  const d = META.defaults;
  $("weapon").value = d.weapon;
  arcanes = arcanesFor(d.weapon, d.arcane);
  evoSel = { 1: null, 2: null, 3: null, 4: null, ...(d.evolutions || {}) };
  // Both scenarios are rebuilt FIELD BY FIELD from the server's defaults, so
  // a field missing here is a field that silently reverts to `undefined` —
  // which is how `infinite_ammo` came to be absent from state while the
  // declaration above set it. The server owns every default; this copies them.
  sim = { enemy: d.enemy, level: d.level, steel_path: d.steel_path,
    headshot_pct: d.headshot_pct, aiming: d.aiming !== false,
    infinite_ammo: d.infinite_ammo !== false, metric: d.metric || "kpm",
    duration: d.duration, runs: d.runs, form: d.form, buffs: {} };
  optSim = { enemy: d.enemy, level: d.level, steel_path: d.steel_path,
    headshot_pct: d.headshot_pct, aiming: d.aiming !== false,
    infinite_ammo: d.infinite_ammo !== false, duration: d.duration, form: d.form };
  optRun = { ...optRun, final_runs: d.final_runs ?? optRun.final_runs,
    finalists: d.finalists ?? optRun.finalists };
  applyWeapon(d.weapon, d.mods);

  $("weapon").addEventListener("change", () => {
    switchWeapon($("weapon").value);
    if (!document.querySelector(".config-page").hidden) nav(weaponModPath($("weapon").value));
  });
  $("run-sim").addEventListener("click", runSim);
  $("run-opt").addEventListener("click", runOptimize);
  $("opt-mod-filter").addEventListener("input", renderOptModList);
  $("opt-arc-filter").addEventListener("input", renderOptArcanes);
  $("opt-size").addEventListener("input", () => {
    opt.size = Math.max(1, Math.min(8, Number($("opt-size").value) || 8));
    updateOptEstimate();
  });
  $("opt-final-runs").value = optRun.final_runs;
  $("opt-finalists").value = optRun.finalists;
  $("opt-final-runs").addEventListener("input", () => {
    optRun.final_runs = Math.max(1, Math.min(100000, Number($("opt-final-runs").value) || 100));
    saveOptRun(); updateOptEstimate();
  });
  $("opt-finalists").addEventListener("input", () => {
    optRun.finalists = Math.max(1, Math.min(100, Number($("opt-finalists").value) || 10));
    saveOptRun(); updateOptEstimate();
  });
  if (optRun.threads) $("opt-threads").value = optRun.threads;
  $("opt-threads").addEventListener("input", () => {
    optRun.threads = Math.max(0, Math.min(128, Number($("opt-threads").value) || 0));
    saveOptRun();
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
  const m = location.pathname.match(/^\/weapons\/([^/]+?)(\/simulator|\/optimizer|\/rivens)?\/?$/);
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
  document.body.classList.toggle("on-home", !w);
  document.body.classList.toggle("on-simulator", mod === "simulator");
  document.body.classList.toggle("on-optimizer", mod === "optimizer");
  document.body.classList.toggle("on-rivens", mod === "rivens");
  $("home-page").hidden = !!w;
  document.querySelector(".config-page").hidden = !w;
  const modTitle = { simulator: " · Simulator", optimizer: " · Optimizer", rivens: " · Rivens" }[mod] || "";
  // The home title carries the SEARCH TERMS, not the headline: nobody looks
  // for "Simulacrum Prime", and the tab/result/share-card is the one place
  // that has to be found rather than enjoyed (user, 2026-07-31). The joke
  // stays on the page, which is where a player meets it.
  document.title = w ? `${w.name}${modTitle} — WFSim` : "WFSim — Ultimate Warframe Calculator";
  if (w) {
    if ($("weapon").value !== w.id) {
      switchWeapon(w.id);
    }
    $("module-tabs").innerHTML =
      `<a class="mtab ${mod === "" ? "sel" : ""}" href="${weaponPath(w.id)}">${tr("Builder")}</a>` +
      `<a class="mtab ${mod === "simulator" ? "sel" : ""}" href="${weaponPath(w.id)}/simulator">${tr("Simulator")}</a>` +
      `<a class="mtab ${mod === "optimizer" ? "sel" : ""}" href="${weaponPath(w.id)}/optimizer">${tr("Optimizer")}</a>` +
      `<a class="mtab ${mod === "rivens" ? "sel" : ""}" href="${weaponPath(w.id)}/rivens">${tr("Rivens")}</a>`;
    // Arriving on the simulator: refresh its build summary (builder edits
    // don't re-render sim views while they are hidden).
    if (mod === "simulator") renderSimBuild();
    if (mod === "rivens") renderRivens();
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
  return `<label title="${escHtml(tr("where the weapon is fired — it changes reload and ammo, nothing else"))}">${escHtml(tr("Environment"))} <select data-k="deployment">${
    // The data keys are lowercase; the LABEL is the wiki's own column head.
    opts.map((o) => `<option value="${o}"${o === cur ? " selected" : ""}>${escHtml(tr(o[0].toUpperCase() + o.slice(1)))}</option>`).join("")
  }</select></label>`;
};

const ammoForced = (w) => !w.finite_reserve;
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
  const on = forced || state.infinite_ammo !== false;
  const why = forced
    ? tr("this weapon has no ammo reserve to run out of")
    : tr("off = the reserve is finite and the weapon can run dry; the magazine and its reloads apply either way");
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
  };
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
// `riven_excludes` takes out what THIS weapon cannot roll: a sentinel weapon
// has no Zoom and no Recoil, a hit-scan one no flight speed, an infinite-ammo
// one no Ammo Maximum, and a weapon with no physical damage rolls no physical
// attribute (the wiki's 25% rule). The class table stays shared; only the
// weapon's view of it narrows.
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
const RIVEN_SHAPES = [
  { id: "2", bonuses: 2, malus: false },
  { id: "3", bonuses: 3, malus: false },
  { id: "2+1", bonuses: 2, malus: true },
  { id: "3+1", bonuses: 3, malus: true },
];

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
    shape: "2",
    drafts,
    bonuses: drafts["2"].bonuses,
    malus: drafts["2"].malus,
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
// ALWAYS one riven, exactly as the builder always has "preset 1": a bar whose
// only option is "+ new" makes the visitor do a step the page could have done
// (user, 2026-07-31), and the first one is the empty card they were going to
// fill in anyway.
function ensureRivenList() {
  let ps = loadPresetList(RIVENS);
  if (!ps.length) {
    ps = [{ name: "riven 1", savedAt: Date.now(), state: blankRiven() }];
    storePresetList(RIVENS, ps);
  }
  if (!ps.some((p) => p.name === activeRivenName())) {
    activeRiven = ps[0].name;
    localStorage.setItem(presetActiveKey(RIVENS), activeRiven);
  }
  return ps;
}

function renderRivens() {
  if (!META || !$("riven-block")) return;
  const w = weaponInfo($("weapon").value);
  if (!riven || riven.__weapon !== w.id) {
    // Arriving (or reloading) opens the ACTIVE riven, not a blank one — the
    // chip said "riven 1" while the editor showed an empty card, which is the
    // bar and the editor disagreeing about which document is open.
    const ps = ensureRivenList();
    const cur = ps.find((p) => p.name === activeRivenName());
    riven = { ...withDrafts((cur && cur.state) || blankRiven()), __weapon: w.id };
  }
  $("riven-sub").textContent =
    `${w.name} · ${tr("disposition")} ${(w.disposition || 1).toFixed(2)} — ${tr("every value below is already scaled by it")}`;
  renderRivenPresetBar();
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
  const shapeNow = RIVEN_SHAPES.find((x) => x.id === now) || RIVEN_SHAPES[0];
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
  box.querySelectorAll("[data-open]").forEach((el) => el.onclick = () => {
    const p = loadPresetList(RIVENS).find((x) => x.name === el.dataset.open);
    if (!p) return;
    activeRiven = p.name;
    localStorage.setItem(presetActiveKey(RIVENS), activeRiven);
    riven = { ...withDrafts(p.state || blankRiven()), __weapon: $("weapon").value };
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
  (r && r.stats || []).forEach((s) => {
    const num = box.querySelector(`.rv-num[data-slot="${s.slot}"]`);
    const unit = box.querySelector(`.rv-unit[data-slot="${s.slot}"]`);
    const pct = box.querySelector(`.rv-pct[data-slot="${s.slot}"]`);
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
function saveRivenSoon() {
  clearTimeout(rivenSaveTimer);
  rivenSaveTimer = setTimeout(() => {
    const ps = loadPresetList(RIVENS);
    const name = activeRivenName();
    const i = ps.findIndex((p) => p.name === name);
    if (i >= 0) {
      ps[i].state = snapshotRiven();
      storePresetList(RIVENS, ps);
      renderRivenPresetBar();
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

function renderRivenPresetBar() {
  const bar = $("preset-bar-rivens");
  if (!bar) return;
  ensureRivenList();
  renderPresetBarIn(bar, {
    newName: (n) => "riven " + n,
    domain: RIVENS,
    label: tr("Rivens"),
    hint: "per weapon — a riven's values are its weapon's disposition applied to a roll",
    load: () => loadPresetList(RIVENS),
    store: (ps) => storePresetList(RIVENS, ps),
    active: () => activeRivenName(),
    setActive: (n) => { activeRiven = n; localStorage.setItem(presetActiveKey(RIVENS), n); },
    snapshot: snapshotRiven,
    apply: (st) => {
      riven = { ...withDrafts(st), __weapon: $("weapon").value };
      renderRivenShape(); renderRivenStats(); renderRivenFoot(); resolveRiven();
    },
    blank: blankRiven,
    rerender: renderRivenPresetBar,
  });
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
const presetListKey = (d, w) => "wfsim-presets-" + (w ?? presetWeapon()) + "-" + d;
const presetActiveKey = (d, w) => "wfsim-preset-active-" + (w ?? presetWeapon()) + "-" + d;
const loadPresetList = (d, w) => {
  try { const p = JSON.parse(localStorage.getItem(presetListKey(d, w))); return Array.isArray(p) ? p : []; }
  catch (_) { return []; }
};
const storePresetList = (d, ps, w) => localStorage.setItem(presetListKey(d, w), JSON.stringify(ps));

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
    sim: { ...sim },
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
  arcanes = arcanesFor(w, st.arcane);
  arcaneRanks = asArcaneList(st.arcaneRank, arcanes.length).map((x) => x ?? null);
  // The preset's own scenario wins over the per-weapon seeding, so the marker
  // is stamped with it rather than left for renderSim to notice.
  if (st.sim) sim = { ...sim, ...st.sim, __weapon: w };
  renderMods(); renderArcanes(); renderEvo(); renderSim(); refreshPanel();
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
const PRESET_DOMAINS = ["builder-builds", "optimizer-mods", "optimizer-arcanes",
  "optimizer-evolutions", "simulator-scenarios"];
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

function initPresets() {
  migratePresetsToWeaponScope();
  let ps = loadPresetList(BUILDS);
  // One-time default migration: presets saved under the old 300-run
  // default keep pinning the sim back to 300 — rewrite them to the new
  // default (100). A deliberate non-default choice is left alone.
  let migrated = false;
  ps.forEach((p) => {
    if (p.state && p.state.sim && p.state.sim.runs === 300) { p.state.sim.runs = 100; migrated = true; }
  });
  if (migrated) storePresetList(BUILDS, ps);
  if (!ps.length) {
    ps = [{ name: "preset 1", savedAt: Date.now(), state: snapshotState() }];
    storePresetList(BUILDS, ps);
  }
  let sc = loadPresetList(SCENARIOS);
  if (!sc.length) {
    sc = [{ name: "scenario 1", savedAt: Date.now(), state: snapshotScenario() }];
    storePresetList(SCENARIOS, sc);
  }
  const lastSc = localStorage.getItem(presetActiveKey(SCENARIOS));
  activeScenario = sc.some((p) => p.name === lastSc) ? lastSc : sc[0].name;
  localStorage.setItem(presetActiveKey(SCENARIOS), activeScenario);

  const here = presetWeapon();
  const last = localStorage.getItem(presetActiveKey(BUILDS));
  activePreset = ps.some((p) => p.name === last) ? last : ps[0].name;
  localStorage.setItem(presetActiveKey(BUILDS), activePreset);
  // Applied under THIS weapon, never the payload's — a preset filed here
  // belongs here by definition.
  whileApplying(() => restoreState(ps.find((p) => p.name === activePreset).state, here));
  renderPresetBar();
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
    const ps = loadPresetList(BUILDS);
    const at = ps.findIndex((p) => p.name === activePreset);
    if (at < 0) return;
    ps[at] = { ...ps[at], savedAt: Date.now(), state: snapshotState() };
    storePresetList(BUILDS, ps);
  }, 400);
}

let scenarioSaveTimer = null;
// The scenario's own auto-save. Same contract as the build's — the editor IS
// the preset — but a different collection, because a build is tested against
// several fights and each of them is worth keeping.
function markScenarioDirty() {
  if (presetApplying) return;
  clearTimeout(scenarioSaveTimer);
  scenarioSaveTimer = setTimeout(() => {
    if (!activeScenario || presetApplying) return;
    const ps = loadPresetList(SCENARIOS);
    const at = ps.findIndex((p) => p.name === activeScenario);
    if (at < 0) return;
    ps[at] = { ...ps[at], savedAt: Date.now(), state: snapshotScenario() };
    storePresetList(SCENARIOS, ps);
  }, 400);
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

function renderPresetBarIn(bar, cfg) {
  const ps = cfg.load();
  const active = cfg.active();
  const ftext = presetFilters[bar.id] || "";
  const f = ftext.trim().toLowerCase();
  const shown = f ? ps.filter((p) => p.name === active || p.name.toLowerCase().includes(f)) : ps;
  const hint = cfg.hint ? ` (${cfg.hint})` : "";
  const chip = (p) => {
    const sel = p.name === active;
    const ops = sel
      ? `<button class="pop dup" title="duplicate into a new preset">⧉</button>` +
        `<button class="pop ren" title="rename">✎</button>` +
        (ps.length > 1 ? `<button class="pop del" title="delete">✕</button>` : "")
      : "";
    return `<span class="pchip ${sel ? "sel" : ""}" data-name="${escHtml(p.name)}" title="switch to ${escHtml(p.name)}${escHtml(hint)}">${escHtml(p.name)}${ops}</span>`;
  };
  bar.innerHTML =
    `<span class="plabel">${cfg.label} <b>${ps.length}</b></span>` +
    (ps.length > PRESET_FILTER_AT ? `<input class="pfilter" type="text" placeholder="${escHtml(tr("filter…"))}" value="${escHtml(ftext)}">` : "") +
    shown.map(chip).join("") +
    `<span class="pchip add" title="new empty preset${escHtml(hint)}">+ new</span>` +
    // Presets are per weapon, so bringing one over from another weapon is
    // an explicit action rather than a side effect of switching weapons.
    (presetSources(cfg.domain, presetWeapon()).length
      ? `<span class="pchip imp" title="${escHtml(tr("copy a preset from another weapon"))}">⇤ ${escHtml(tr("import"))}</span>`
      : "") +
    `<div class="pimport" hidden></div>`;

  // Typing re-renders the bar (chips re-filter), so hand focus back.
  const filt = bar.querySelector(".pfilter");
  if (filt) filt.addEventListener("input", () => {
    presetFilters[bar.id] = filt.value;
    cfg.rerender();
    const nf = bar.querySelector(".pfilter");
    if (nf) { nf.focus(); nf.setSelectionRange(nf.value.length, nf.value.length); }
  });
  bar.querySelectorAll(".pchip:not(.add)").forEach((c) => c.addEventListener("click", () => {
    const p = cfg.load().find((x) => x.name === c.dataset.name);
    if (p && p.name !== cfg.active()) {
      cfg.setActive(p.name);
      whileApplying(() => cfg.apply(p.state)); // a load is not an edit
      cfg.rerender();
    }
  }));
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
  const freeName = (ps2, mk) => {
    for (let n = 1; ; n++) { const nm = mk(n); if (!ps2.some((p) => p.name === nm)) return nm; }
  };
  const addBtn = bar.querySelector(".pchip.add");
  addBtn.addEventListener("click", (e) => {
    e.stopPropagation();
    const ps2 = cfg.load();
    // "preset N" everywhere except where the thing has its own noun — the
    // riven bar names them "riven N", because that is what they are.
    const name = freeName(ps2, cfg.newName || ((n) => "preset " + n));
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
  on(".pop.dup", () => {
    const ps2 = cfg.load();
    const base = cfg.active();
    const name = freeName(ps2, (n) => base + " copy" + (n > 1 ? " " + n : ""));
    // The copy captures the live editor state and becomes the active
    // document; the original keeps what auto-save last wrote into it.
    ps2.push({ name, savedAt: Date.now(), state: cfg.snapshot() });
    cfg.store(ps2);
    cfg.setActive(name);
    cfg.rerender();
  });
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
    // Only offered while >1 preset exists — there is always at least one.
    const ps2 = cfg.load().filter((p) => p.name !== cfg.active());
    if (!ps2.length) return;
    cfg.store(ps2);
    cfg.setActive(ps2[0].name);
    whileApplying(() => cfg.apply(ps2[0].state));
    cfg.rerender();
  });
}

// An EMPTY build for "+ new": the CURRENT weapon (the page is a weapon
// page — a new preset should not navigate away), bare slots, no arcane,
// no evolutions, sim back to the META defaults.
function blankBuildState() {
  const d = META.defaults;
  return {
    weapon: $("weapon").value,
    evoSel: {},
    arcane: ["none"],
    arcaneRank: [null],
    slots: [],
    sim: { enemy: d.enemy, level: d.level, steel_path: d.steel_path,
      headshot_pct: defaultHeadshotPct(weaponInfo($("weapon").value)),
      duration: d.duration, runs: d.runs, form: d.form, buffs: {} },
  };
}

function renderPresetBar() {
  renderPresetBarIn($("preset-bar-" + BUILDS), {
    domain: BUILDS,
    // An imported build keeps its mods/arcane/sim scenario but belongs to
    // the weapon it lands on; restoreState prunes whatever that weapon
    // cannot equip (a different mod class, other evolution ids).
    rescope: (st, weapon) => ({ ...st, weapon }),
    label: tr("Presets"),
    load: () => loadPresetList(BUILDS),
    store: (ps) => storePresetList(BUILDS, ps),
    active: () => activePreset,
    setActive: (n) => { activePreset = n; localStorage.setItem(presetActiveKey(BUILDS), n); },
    snapshot: snapshotState,
    // Never the payload's weapon — the scope's. See restoreState.
    apply: (st) => restoreState(st, presetWeapon()),
    blank: blankBuildState,
    rerender: renderPresetBar,
  });
}

// A scenario is the `sim` object, BUFF CONFIG INCLUDED (user, 2026-08-01:
// "preset 要记住所有潜在 buff 的设置情况").
//
// Buff ids are global — `arcane:primary_deadhead`, a mod's own buff — so a
// setting travels, and `sim.buffs` deliberately keeps entries for buffs the
// build does not currently carry. That is what lets a scenario say "in THIS
// fight, Deadhead starts at zero stacks" and have it hold when the mod is
// added later, or when the marginal-gain scan tries it as a candidate.
// Anything the map does not mention takes the buff's own default, which is
// full stacks and unlocked.
function snapshotScenario() {
  const { __weapon, ...rest } = sim;
  return JSON.parse(JSON.stringify(rest));
}
function applyScenario(st) {
  sim = { ...sim, ...st, buffs: JSON.parse(JSON.stringify(st.buffs || {})) };
  renderSim();      // redraws every knob, and the bar with them
  markPresetDirty(); // the build remembers what it is being tested with
}
function renderScenarioBar() {
  renderPresetBarIn($("preset-bar-" + SCENARIOS), {
    domain: SCENARIOS,
    label: tr("Scenarios"),
    load: () => loadPresetList(SCENARIOS),
    store: (ps) => storePresetList(SCENARIOS, ps),
    active: () => activeScenario,
    setActive: (n) => { activeScenario = n; localStorage.setItem(presetActiveKey(SCENARIOS), n); },
    snapshot: snapshotScenario,
    apply: applyScenario,
    blank: snapshotScenario,
    rerender: renderScenarioBar,
  });
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
// preset (creating "preset 1" the first time). The optimizer's groups
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
  opt = { mods: {}, exilus: {}, arcanes: {}, evos: {}, size: 8, buffs: {} }; optSeeded = false; optBuffList = []; // reset scope
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
  innate = (w.innate_polarities || []).slice(0, 8);
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

  $("w-tags").innerHTML = [w.subtype, w.uses_evo2 ? "Incarnon" : null, w.sentinel ? "Sentinel" : null]
    .filter(Boolean).map((t) => `<span class="tag">${t}</span>`).join("");

  const AX = weaponAxes(w.id);
  show("arcane-block", AX.arcanes.length > 0);
  show("evo-block", AX.evolutions.length > 0);
  show("element-block", !!w.element_config);
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

  renderMods(); renderArcanes(); renderEvo(); renderSim(); renderOpt();
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
  return { regular: Math.max(added, removed), umbra, omni };
}

// Auto-assign polarities for MINIMUM Forma-to-fit (mirrors engine plan_forma):
// spend the innate pool on the biggest matching mods, then Forma the biggest
// unmatched until it fits; unmatched slots left blank. Overwrites polarities.
function autoForma() {
  const filled = [];
  slots.forEach((s, i) => { const m = modById(s.mod); if (m) filled.push({ i, m }); });
  slots.forEach((s) => { s.pol = null; });
  const pool = innate.filter(Boolean).slice();
  const bd = ({ i, m }) => modDrain(m, slots[i].rank);
  const order = filled.slice().sort((a, b) => bd(b) - bd(a));
  const matched = new Set();
  for (const { i, m } of order) { const k = pool.indexOf(m.polarity); if (k >= 0) { pool.splice(k, 1); matched.add(i); } }
  const drainOf = () => filled.reduce((s, x) => s + (matched.has(x.i) ? Math.ceil(bd(x) / 2) : bd(x)), 0);
  while (drainOf() > CAP) { const next = order.find(({ i }) => !matched.has(i)); if (!next) break; matched.add(next.i); }
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
  const used = capacityUsed();
  const capEl = $("capacity");
  capEl.textContent = `${used} / ${CAP}`;
  capEl.classList.toggle("over", used > CAP);
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
function buildPayload() {
  return {
    weapon: $("weapon").value,
    evolutions: Object.values(evoSel).filter(Boolean),
    // One per pool, in the weapon's pool order — the server reads either
    // this or a bare value, so an old saved build still means what it meant.
    arcane: arcanes,
    arcane_rank: arcaneRanks,
    mods: slots.filter((s) => s.mod).map((s) => s.mod),
    // A `riven:` id means nothing without the riven itself — it is the
    // visitor's item, not a pool entry, so it rides along with the request.
    rivens: rivenPayload(),
  };
}

// ---- Stats panel: merged buckets, each explained by source ----
let panelTimer = null;
function refreshPanel() {
  markPresetDirty(); // every build change funnels through here
  clearTimeout(panelTimer);
  panelTimer = setTimeout(async () => {
    const body = buildPayload();
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
  const srcLine = (s) =>
    `<div class="ssrc">${s.value} — ${s.mod}${s.note ? ` <span class="snote">(${s.note})</span>` : ""}</div>`;
  const rowHtml = (row) => `
    <div class="srow">
      <div class="shead"><span class="sk">${tr(row.label)}</span>
        <span class="sv">${row.base !== "—" && row.base !== row.final ? `<span class="sbase">${row.base}</span> → ` : ""}<b>${row.final}</b></span></div>
      ${row.note ? `<div class="srownote">⚙ ${row.note}</div>` : ""}
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

const cardLines = (o, r, fallback) =>
  officialDesc(o, r) || (descAt(o, r) || fallback || o.effects || []).map(tf);

// One slot card (regular or exilus) with its polarity / rank / menu wiring.
function buildSlot(i) {
  const s = slots[i];
  const el = document.createElement("div");
  const m = s.mod ? modById(s.mod) : null;
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
  // polarity is decoupled: clickable on every slot (mod or empty, incl. innate)
  el.querySelector(".pol-btn").addEventListener("click", (e) => { e.stopPropagation(); openPolMenu(i); });
  return el;
}

// The DOM node for slot i (popover anchoring).
const slotEl = (i) => i === EXILUS ? $("exilus").firstElementChild : $("mod-slots").children[i];

// ---- popovers ----
function closePopovers() {
  $("mod-popover").hidden = true;
  $("slot-menu").hidden = true;
  $("arcane-popover").hidden = true;
  const rv = $("riven-popover");
  if (rv) rv.hidden = true;
}
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
    `<label>${escHtml(tr("Sort"))} <select id="pk-sort"><option value="name">${escHtml(tr("Name"))}</option><option value="drain">${escHtml(tr("Drain"))}</option><option value="gain">${escHtml(tr("Gain"))}</option></select></label>` +
    `<button id="pk-dir" class="ghost-btn small" title="direction">${pickerPrefs.dir === "asc" ? "▲" : "▼"}</button>` +
    `<span class="pk-pols"><span class="pk-pol ${!pickerPrefs.pol ? "sel" : ""}" data-p="">all</span>` +
    pols.map((p) => `<span class="pk-pol ${pickerPrefs.pol === p ? "sel" : ""}" data-p="${p}" title="${p}">${imgTag(POL(p), "pol")}</span>`).join("") +
    `</span>`;
  // redraw() re-renders these tools via innerHTML, which DETACHES the clicked
  // node; without stopPropagation the click would bubble to the document
  // outside-click handler, whose closest(".popover") now fails on the detached
  // target → the picker would wrongly close. Keep every tool click inside.
  const redraw = () => { savePickerPrefs(); renderTools(); renderMenu(pickerSlot, $("mod-search").value); };
  $("pk-sort").value = pickerPrefs.sort;
  $("pk-sort").onclick = (e) => e.stopPropagation();
  $("pk-sort").onchange = (e) => { e.stopPropagation(); pickerPrefs.sort = $("pk-sort").value; redraw(); };
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
const GAIN_SEED = 0x5EED;
// A gain READS with its sign — "12.3%" and "+12.3%" are different claims.
const gainPct = (x) => (x >= 0 ? "+" : "−") + sig2(Math.abs(x) * 100) + "%";

// Quick calc's ONE setting: which saved scenario. Everything else that was
// ever offered here turned out not to be a choice (user, 2026-08-01) — what a
// run is measured by belongs to the scenario, and how many runs it takes is a
// question the algorithm answers better than a person can, since the right
// number depends on how close the leaders turn out to be. A mode selector is
// one more thing to get wrong for no answer it can give you.
//
// There is no "current" scenario either: a scan is only worth reading against
// something that has a name and can be returned to.
let gainPrefs = { scenario: null };
try { const s = JSON.parse(localStorage.getItem("wfsim-gain")); if (s) gainPrefs = { ...gainPrefs, ...s }; } catch (_) {}
const saveGainPrefs = () => localStorage.setItem("wfsim-gain", JSON.stringify(gainPrefs));

let gainScan = { key: null, running: false, base: 0, by: {}, done: 0, total: 0, note: "", metric: "" };

/// The scenario a scan runs under: the chosen preset, else the live one.
function gainScenario() {
  const ps = loadPresetList(SCENARIOS);
  const p = ps.find((x) => x.name === gainPrefs.scenario)
    || ps.find((x) => x.name === activeScenario) || ps[0];
  const st = p ? { ...sim, ...p.state } : { ...sim };
  // TWO PASSES, always: one run over everything, then the leaders again with
  // a tenth of the scenario's runs. A ranking is cheap at the bottom and
  // expensive at the top, and this spends accordingly.
  const runs = 1;
  const refine = Math.max(2, Math.ceil((st.runs || 1) / 10));
  // The WHOLE buff map travels, not just the current build's cards: a
  // candidate's buff is by definition not in `buffList`, and the scenario may
  // well have an opinion about it. Unmentioned buffs take their own default
  // (full stacks, unlocked), which is the honest reading of "no opinion".
  return { name: p ? p.name : "—", refine,
    scenario: { ...st, runs, seed: GAIN_SEED, buffs: st.buffs || {} } };
}

// A scan belongs to ONE AXIS POSITION of one build under one scenario.
let gainAxis = { kind: "mods", idx: 0 };
const gainKey = () => JSON.stringify([gainAxis, buildPayload(), gainPrefs,
  sim.enemy, sim.level, sim.steel_path, sim.headshot_pct, sim.aiming,
  sim.infinite_ammo, sim.duration, sim.runs, sim.form, sim.deployment]);

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
    const out = [];
    weaponEvos().forEach((tier) => {
      tier.options.forEach((o) => {
        if (evoSel[tier.tier] === o.id) return;
        const next = { ...evoSel, [tier.tier]: o.id };
        out.push({ id: o.id, payload: { evolutions: Object.values(next).filter(Boolean) } });
      });
    });
    return out;
  }
  const cur = slots.map((s) => s.mod);
  return poolWithRivens()
    .filter((m) => !cur.includes(m.id))
    .filter((m) => axis.idx !== EXILUS || m.exilus)
    .map((m) => { const next = cur.slice(); next[axis.idx] = m.id; return { id: m.id, payload: { mods: next.filter(Boolean) } }; })
    .filter((c) => modsCompatible(c.payload.mods));
}

// How many of the leaders AUTO looks at twice. Small on purpose: the second
// pass exists to settle an ORDER, and an order is decided at the top.
const GAIN_REFINE_TOP = 12;

async function scanGains(axis, onTick) {
  if (gainScan.running) return;
  gainAxis = axis;
  const { name, scenario, refine } = gainScenario();
  gainScan = { key: gainKey(), running: true, base: 0, by: {}, done: 0, total: 0,
    note: `${name} · ${scenario.runs}×${refine ? ` → ${refine}×` : ""}`, metric: "" };
  // Kill progress is the optimizer's metric and the one a player is actually
  // buying; DPS is the fallback for a target this build cannot kill at all,
  // where the ratio has no denominator. The SCENARIO decides which.
  let useKills = (scenario.metric || "kpm") !== "dps";
  const run = async (override) => {
    const r = await api("/api/simulate", { ...buildPayload(), ...scenario, ...override });
    if (!r || !r.ok) return null;
    return useKills ? (r.score ?? r.kills ?? 0) : (r.dps || 0);
  };
  let base = await run({});
  if (!base && useKills) { useKills = false; base = await run({}); }
  if (!base) { gainScan.running = false; if (onTick) onTick(gainScan); return; }
  gainScan.base = base;
  gainScan.metric = useKills ? tr("kill rate") : tr("DPS");
  const cands = gainCandidates(axis);
  gainScan.total = cands.length + (refine ? Math.min(GAIN_REFINE_TOP, cands.length) + 1 : 0);
  for (const c of cands) {
    const v = await run(c.payload);
    gainScan.done++;
    if (v != null) gainScan.by[c.id] = { pct: (v - base) / base, runs: scenario.runs };
    if (onTick) onTick(gainScan);
  }
  // SECOND PASS. One run ranks the field cheaply but cannot separate its top
  // few — so the leaders are asked again with more, against a baseline
  // measured the same way. Everything below them keeps its first answer,
  // which is all a position near the bottom needs to be right about.
  if (refine) {
    const deep = { ...scenario, runs: refine };
    const runDeep = async (override) => {
      const r = await api("/api/simulate", { ...buildPayload(), ...deep, ...override });
      if (!r || !r.ok) return null;
      return useKills ? (r.score ?? r.kills ?? 0) : (r.dps || 0);
    };
    const deepBase = await runDeep({});
    gainScan.done++;
    if (onTick) onTick(gainScan);
    if (deepBase) {
      const top = cands
        .filter((c) => gainScan.by[c.id])
        .sort((a, b) => gainScan.by[b.id].pct - gainScan.by[a.id].pct)
        .slice(0, GAIN_REFINE_TOP);
      for (const c of top) {
        const v = await runDeep(c.payload);
        gainScan.done++;
        if (v != null) gainScan.by[c.id] = { pct: (v - deepBase) / deepBase, runs: refine };
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
const gainChipFor = (id, where) => {
  const g = gainOf(id);
  return g
    ? `<span class="gainchip ${g.pct >= 0 ? "up" : "down"}${g.runs > 1 ? " deep" : ""}" title="${escHtml(
        `${where} · ${gainScan.metric} · ${gainScan.note} · ${g.runs}×`)}">${gainPct(g.pct)}</span>`
    : "";
};

/// The picker's ONE ordering rule, over whatever keys an axis has.
/// Descending on every key, the chosen one first, the rest in a fixed order —
/// and an unscanned option sorts last whichever way the arrow points, because
/// an absent answer is not a small one.
function gainSort(a, b, keys) {
  const ga = gainOf(a.id), gb = gainOf(b.id);
  if (!ga !== !gb) return ga ? -1 : 1;
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
  const ps = loadPresetList(SCENARIOS);
  const cur = ps.some((p) => p.name === gainPrefs.scenario) ? gainPrefs.scenario
    : (ps.some((p) => p.name === activeScenario) ? activeScenario : (ps[0] || {}).name);
  const opt = (v, label, sel) => `<option value="${escHtml(v)}"${v === sel ? " selected" : ""}>${escHtml(label)}</option>`;
  box.innerHTML =
    `<span class="pc-h">⚡ ${escHtml(tr("Quick calc"))}</span>` +
    `<select id="gp-scen" title="${escHtml(tr("the saved scenario to measure under — it decides the enemy, the technique and whether the ranking is KPM or DPS"))}">${
      ps.map((p) => opt(p.name, p.name, cur)).join("")}</select>` +

    `<span class="pc-note">${gainScan.running
      ? `${gainScan.done}/${gainScan.total}`
      : (gainScan.note ? escHtml(gainScan.note) : escHtml(tr("open a slot to rank its mods by effect")))}</span>`;
  // Every click stays inside: a redraw detaches these nodes, and the document
  // outside-click handler closes on a target whose `.popover` ancestor is gone.
  box.onclick = (e) => e.stopPropagation();
  $("gp-scen").onchange = (e) => {
    e.stopPropagation();
    gainPrefs = { scenario: $("gp-scen").value };
    saveGainPrefs();
    renderQuickCalc();
  };
}

/// Compute this axis position's ranking, unless it is already on screen.
/// `gainKey` covers the axis, the build, the scenario and the settings, so
/// re-opening the same picker costs nothing and any edit invalidates it.
function ensureGains(axis, repaint) {
  // Nothing to measure against yet. The evolution rows scan without being
  // opened, so on a cold load they can fire before `initPresets` has seeded
  // the scenario library — and a scan with no named scenario is one nobody
  // can reproduce or compare against (it labelled itself "—").
  if (!loadPresetList(SCENARIOS).length) return;
  gainAxis = axis;                       // so the key describes what we want
  if (gainScan.running || gainScan.key === gainKey()) return;
  let last = 0;
  scanGains(axis, (st) => {
    const now = Date.now();
    if (!st.running || now - last > 250) { last = now; renderQuickCalc(); repaint(); }
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
  const hits = poolWithRivens()
    // The exilus slot takes what `exilusPool()` says, which is the same
    // question the optimizer's exilus scope asks.
    .filter((m) => slotIdx !== EXILUS || m.exilus)
    .filter((m) => !pickerPrefs.pol || m.polarity === pickerPrefs.pol)
    .filter((m) => !q || searchBlob(m).includes(q))
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
    const slotName = (idx) => idx === EXILUS ? "exilus" : "slot " + (idx + 1);
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
  return (META.arcanes || []).filter((a) => a.id !== "none" && a.slot === pool);
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
function setArcane(id, i = arcaneSlotIdx) { arcanes[i] = id; arcaneRanks[i] = null; }
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
      arcaneRanks[i] = Math.max(0, Math.min(maxr, r + Number(b.dataset.d)));
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
function renderEvo() {
  const tiers = weaponEvos();
  const rows = [];
  for (const t of tiers) {
    const sel = evoSel[t.tier] || null;
    const card = (o) => {
      const icon = o.icon ? `<img class="eicon" src="${IMG(o.icon)}" alt="">` : "";
      const cls = ["evopick", o.id === sel ? "sel" : "", o.broken ? "broken" : ""].join(" ");
      const lines = evoLines(o).map((x) => `<div>${escHtml(x)}</div>`).join("");
      const title = (o.effects || []).join("\n"); // model statement as tooltip
      // The broken warning lives INSIDE the selected card, so it never
      // straddles the row divider into the next tier.
      const warn = o.broken && o.id === sel
        ? `<span class="ed warn">⚠ does not work in-game (wiki) — the simulation computes it as NO EFFECT</span>`
        : "";
      // Evolutions have no standalone wiki pages — link to the weapon's
      // Incarnon Genesis page.
      const wInfo = weaponInfo($("weapon").value);
      const genesis = wikiUrl(wikiWeaponName(wInfo) + " Incarnon Genesis");
      return `<span class="${cls}" data-tier="${t.tier}" data-id="${o.id}" title="${title}">
        ${icon}<span class="einfo"><b class="en">${wl(o.name, genesis)}${o.broken ? ' <i class="bx">BROKEN</i>' : ""}${
          gainChipFor(o.id, `EVO ${ROMAN(t.tier)}`)}</b><span class="ed">${lines}</span>${warn}</span></span>`;
    };
    const empty = `<span class="evopick empty ${sel === null ? "sel" : ""}" data-tier="${t.tier}" data-id="">
      <span class="einfo"><b class="en">None</b><span class="ed"><div>nothing installed at this tier</div></span></span></span>`;
    // None comes FIRST (the default state is a bare weapon).
    rows.push(`<div class="evo"><span class="rank">EVO ${ROMAN(t.tier)}</span><div class="picks">${empty}${t.options.map(card).join("")}</div></div>`);
  }
  $("evo-rows").innerHTML = rows.join("");
  // Evolutions are all on screen at once, so they are scanned ACROSS EVERY
  // TIER in one pass — a dozen candidates, not seventy, which is why they can
  // afford to answer without being opened (user, 2026-08-01: arcanes and
  // evolutions use this too). The key guards the repeat.
  if (tiers.length) ensureGains({ kind: "evo", idx: 0 }, () => renderEvo());
  $("evo-rows").querySelectorAll(".evopick").forEach((c) => c.addEventListener("click", () => {
    evoSel[Number(c.dataset.tier)] = c.dataset.id || null;
    renderEvo(); refreshPanel();
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
  if (sub) sub.textContent = activePreset ? `${tr("testing preset")}: ${activePreset}` : "";
  const chip = (img, label, rk) =>
    `<span class="sb-chip">${imgTag(img, "sb-img")}<span>${escHtml(label)}</span>${rk != null ? `<span class="rk">R${rk}</span>` : ""}</span>`;
  const w = weaponInfo($("weapon").value);
  const parts = [];
  const modChips = slots.map((s) => {
    const m = s.mod && modById(s.mod);
    if (!m) return "";
    return chip(IMG(m.image), m.name, s.rank == null ? m.max_rank : s.rank);
  }).filter(Boolean);
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
// starts at 0 rather than the player's 100 (user, 2026-07-31). Still a knob —
// this seeds it, it does not cap it.
const defaultHeadshotPct = (w) => ((w || {}).sentinel ? 0 : META.defaults.headshot_pct);

// How THIS weapon is played: the Incarnon cycle where there is one to run,
// and the weapon's own default form (`default_form` in data/weapons — the
// arsenal's form) where there is not.
// The Form control, ALWAYS drawn: several forms is a dropdown, one form is
// still stated rather than left silent — a weapon with a single form is being
// fired in it, and the panel should say so (user, 2026-07-31).
const formField = (formOpts, current) => {
  if (!formOpts.length) return "";
  const body = formOpts.length > 1
    ? `<select data-k="form">${formOpts.map(([id, label]) =>
        `<option value="${id}" ${current === id ? "selected" : ""}>${escHtml(label)}</option>`).join("")}
       </select>`
    : `<span class="fixed-val">${escHtml(formOpts[0][1])}</span>`;
  return `<label>${escHtml(tr("Form"))} ${body}</label>`;
};

const defaultFormId = (w, formOpts) =>
  ((w || {}).has_cycle && "incarnon_cycle") ||
  (((w || {}).forms || []).find((f) => f.is_default) || {}).id ||
  (formOpts[0] || [])[0] || "base";

function renderSim() {
  if (!META) return;
  renderSimBuild();
  const w = weaponInfo($("weapon").value);
  const enemies = META.enemies || [];
  const en = enemies.find((e) => e.id === sim.enemy) || enemies[0];
  if (en) sim.enemy = en.id;
  const eopts = enemies.map((e) =>
    `<option value="${e.id}" ${e.id === sim.enemy ? "selected" : ""}>${e.name}</option>`).join("");
  // Section 1 — the enemy / scenario.
  $("sim-enemy").innerHTML = `
    <label>Enemy <select data-k="enemy">${eopts}</select></label>
    <label>Level <input type="number" data-k="level" min="1" max="9999" value="${sim.level}"></label>
    <label class="check"><input type="checkbox" data-k="steel_path" ${sim.steel_path ? "checked" : ""}> Steel Path</label>
    ${deployField(w, sim)}`;
  // Section 2 — TECHNIQUE: the player's execution, which is a different kind
  // of input from the enemy (1) and from the measurement (4). Which form you
  // fight in, whether you hold aim, and how often you land the head are all
  // choices the player makes — and aiming GATES buffs, so it belongs here
  // rather than mixed into the run settings (user, 2026-07-30).
  // The FORMS are the weapon's own (registered in data/weapons, served by
  // /api/meta) — not a hardcoded Incarnon triple. The two-form CYCLE is not a
  // form but a MODE over them, so it is listed first and only when the weapon
  // has something to transform into (`has_cycle`). A weapon with one form and
  // no cycle has nothing to choose, so no selector is drawn.
  const formOpts = [
    ...(w.has_cycle ? [["incarnon_cycle", tr("Incarnon cycle")]] : []),
    ...(w.forms || []).map((f) => [f.id, w.has_cycle ? `${tr(f.name)} ${tr("only")}` : tr(f.name)]),
  ];
  // A stale preset (or another weapon's choice) can name a form this weapon
  // does not have — fall back to the first option rather than sending it.
  // A stale preset (or another weapon's choice, or the "default" seed) names
  // a form this weapon does not list — fall back to how this weapon is
  // played, which is a question about the weapon and not about list order.
  if (formOpts.length && !formOpts.some(([id]) => id === sim.form)) sim.form = defaultFormId(w, formOpts);
  // Scenario knobs that are a property of the WEAPON rather than of the
  // fight: re-seeded when the weapon changes, kept when a preset set them
  // (restoreState stamps the marker itself, so a saved value stands).
  if (sim.__weapon !== w.id) {
    sim.__weapon = w.id;
    sim.headshot_pct = defaultHeadshotPct(w);
    optSim.headshot_pct = sim.headshot_pct;
  }
  $("sim-technique").innerHTML = `
    ${formField(formOpts, sim.form)}
    ${aimField(w, sim)}
    <label title="${escHtml(tr("a per-PELLET aim weight, not a whole-spread promise — the landing spot is rolled for each pellet"))}">${escHtml(tr("Headshot %"))} <input type="number" data-k="headshot_pct" min="0" max="100" value="${sim.headshot_pct}"></label>
    ${ammoField(w, sim)}`;
  // Section 4 — the MEASUREMENT: nothing the player does in-game.
  $("sim-run").innerHTML = `
    <label>Duration (s) <input type="number" data-k="duration" min="1" max="3600" value="${sim.duration}"></label>
    <label>Runs <input type="number" data-k="runs" min="1" max="20000" value="${sim.runs}"></label>
    <label title="${escHtml(tr("what the run is judged by — the headline number and the picker's gain scan both follow it"))}">${escHtml(tr("Measure"))} <select data-k="metric">${
      [["kpm", tr("KPM")], ["dps", tr("DPS")]].map(([v, l]) =>
        `<option value="${v}"${sim.metric === v ? " selected" : ""}>${escHtml(l)}</option>`).join("")
    }</select></label>`;
  [$("sim-enemy"), $("sim-technique"), $("sim-run")].forEach((box) =>
    box.querySelectorAll("[data-k]").forEach((el) => {
      el.addEventListener("change", () => {
        const k = el.dataset.k;
        if (el.type === "checkbox") sim[k] = el.checked;
        else if (el.type === "number") sim[k] = Number(el.value);
        else sim[k] = el.value;
        if (k === "enemy") $("arena-ename").textContent = (enemies.find((e) => e.id === sim.enemy) || {}).name || "Enemy";
        // A sim knob belongs to BOTH: the build remembers what it was last
        // tested with, and the scenario library keeps the fight itself.
        markPresetDirty();
        markScenarioDirty();
      });
    }));
  renderScenarioBar();
  $("arena-ename").textContent = en ? en.name : "Enemy";
  $("sim-sub").textContent = "current build vs the enemy";
  renderSimBuffs();
}

// Section 2 — one card per configurable buff of the current build (from the
// last /api/panel `buffs`). Each: initial stacks (stepper / on-off) + lock.
// Missing configs default to the buff's `default_*`; ids no longer present are
// dropped from the payload (kept in `sim.buffs` for preset round-trips).
function syncBuffConfig(list, cfg) {
  list.forEach((b) => {
    if (!cfg[b.id]) cfg[b.id] = { stacks: b.default_stacks, locked: b.default_locked };
    else cfg[b.id].stacks = Math.min(cfg[b.id].stacks, b.max_stacks);
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
  const owner = [...(META.mods || []), ...(META.arcanes || []),
    ...Object.values(META.mod_pools || {}).flat()]
    .find((x) => (x.name_en || x.name) === head);
  const label = owner ? owner.name : head;
  return tail ? `${label} (${tr(tail.replace(/\)$/, ""))})` : label;
}

function renderBuffCards(box, list, cfg, have) {
  if (!box) return;
  syncBuffConfig(list, cfg);
  if (!list.length) {
    box.innerHTML = `<div class="sim-empty">no configurable buffs here.</div>`;
    return;
  }
  const card = (b) => {
    const c = cfg[b.id];
    const ctl = b.kind === "toggle"
      ? `<label class="bchk"><input type="checkbox" data-b="${b.id}" data-f="stacks" ${c.stacks > 0 ? "checked" : ""}> active</label>`
      : `<span class="bstep"><input type="number" data-b="${b.id}" data-f="stacks" min="0" max="${b.max_stacks}" value="${c.stacks}"><span class="bmax">/ ${b.max_stacks}</span></span>`;
    // Permanent stacks (Fevered Frenzy): no in-sim trigger, no decay — the
    // count above holds for the whole run, so the lock is implied and greyed.
    const lock = b.permanent
      ? `<label class="block-lock dis" title="permanent stacks — they never decay (and cannot build in-sim), so the count holds for the whole run; lock is implied"><input type="checkbox" checked disabled> lock</label>`
      : `<label class="block-lock" title="lock = permanent 100% uptime"><input type="checkbox" data-b="${b.id}" data-f="locked" ${c.locked ? "checked" : ""}> lock</label>`;
    // In the WIDER view, a buff the build does not carry is still settable —
    // it just says so, so the panel never reads as "this is active now".
    const off = have && !have.has(b.id);
    return `<div class="buff-card${off ? " off" : ""}">
      <span class="bn">${escHtml(buffCardName(b.name))}${off ? ` <small class="bnot">${escHtml(tr("not equipped"))}</small>` : ""}</span>
      <span class="bctl">${ctl}</span>
      ${lock}
    </div>`;
  };
  box.innerHTML = list.map(card).join("");
  box.querySelectorAll("[data-b]").forEach((el) => {
    el.addEventListener("change", () => {
      const id = el.dataset.b, f = el.dataset.f, c = cfg[id];
      if (f === "locked") c.locked = el.checked;
      else if (el.type === "checkbox") c.stacks = el.checked ? 1 : 0;
      else c.stacks = Math.max(0, Number(el.value));
      // A buff edit belongs to BOTH: the build remembers what it was tested
      // with, and the scenario library keeps the fight — including settings
      // for mods this build does not carry.
      markPresetDirty();
      if (typeof markScenarioDirty === "function") markScenarioDirty();
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
  $("sim-results").innerHTML = `<div class="placeholder">running ${sim.runs} simulations…</div>`;
  try {
    // Send only the buffs the current build actually has (ids in buffList).
    const buffs = {};
    buffList.forEach((b) => { const c = sim.buffs[b.id]; if (c) buffs[b.id] = { stacks: c.stacks, locked: c.locked }; });
    const body = { ...buildPayload(), ...sim, buffs };
    const r = await api("/api/simulate", body);
    if (!r || r.ok === false) {
      $("sim-results").innerHTML = `<div class="error">sim failed: ${r ? r.error : "no data"}</div>`;
      return;
    }
    renderResults(r);
    animateArena(r);
    saveSimResult(r);
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
function saveSimResult(r) {
  const ps = loadPresetList(BUILDS);
  const at = ps.findIndex((p) => p.name === activePreset);
  if (at < 0) return;
  ps[at].lastResult = { r, at: Date.now() };
  storePresetList(BUILDS, ps);
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
  const heroSub = (byDps ? `DPS · ${n2(kpm(r.score, r.duration))} KPM · ` : `KPM · `) +
    `${n2(r.score)} kill score in ${n0(r.duration)}s · ` + (killed
    ? `${n0(r.kills)} killed · ~${isFinite(ttk) ? ttk.toFixed(2) : "∞"}s avg per kill`
    : `${pc(r.score)} of one ${LN("enemies", sim.enemy, t.name || "enemy")}'s EHP drained`);
  // No Forma/capacity here — the simulator reports EFFECTS only; build
  // legality is the Builder's business (user, 2026-07-29).
  const kpi = (l, v) => `<div class="kpi"><div class="kv">${v}</div><div class="kl">${tr(l)}</div></div>`;
  // KPI row: damage pace + crit feel + HANDLING feel (shots, reloads,
  // transforms — user, 2026-07-29). In THIS product "DPS" always means
  // EFFECTIVE dps — what the target actually lost, armor and on-target
  // amps included; the weapon-side raw number is out (user: in our
  // context every stat accounts for the enemy).
  const kpis = [
    kpi("DPS", n0(r.dps)),
    // The TIER leads, and the rate is renamed to what it actually measures.
    // "Crit rate" reads as "my crit chance", and it stops being that the
    // moment a build passes 100%: every pellet crits, so it pins at 100%
    // whether the build is at 110% or 410% (group, 2026-07-31). The tier is
    // the same number without that truncation — and the one that multiplies
    // the damage. 1 = yellow, 2 = orange, 3 = red, and it keeps going.
    kpi("Crit tier", (r.crit_tier ?? 0).toFixed(2)),
    kpi("Pellets crit", pc(r.crit_rate)), kpi("Orange+", pc(r.big_crit_rate)),
    kpi("Procs", n0(r.procs)), kpi("Shots", n0(r.shots)),
    kpi("Reloads", n0(r.reloads)), kpi("Transforms", n0(r.transforms)),
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
    field: "Lingering field", arcane: "Arcane (on status)" };
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
  const mbar = (w, c, dim) =>
    `<div class="mbar"><i style="width:${w.toFixed(1)}%;background:var(--s${c})${dim ? ";opacity:.5" : ""}"></i></div>`;
  const meter = srcs.map((x, i) => {
    const c = (i % 8) + 1;
    const parts = x.by_type && x.by_type.length > 1 ? x.by_type : null;
    const open = !!parts && simMeterOpen.has(x.source);
    const head = `<div class="mrow${parts ? " exp" : ""}" data-src="${escHtml(x.source)}">
      <span class="mname">${parts ? `<span class="mcaret">${open ? "▾" : "▸"}</span>` : ""}${srcLabel(x.source)}</span>
      ${mbar(x.dmg / srcMax * 100, c, false)}
      <span class="mval">${n0(x.dmg)} · ${pct2(x.dmg / srcTotal)}</span>
    </div>`;
    if (!parts) return head;
    return head + parts.map((p) => `<div class="mrow sub" data-of="${escHtml(x.source)}"${open ? "" : " hidden"}>
      <span class="mname">${DT(p.type)}</span>
      ${mbar(p.dmg / srcMax * 100, c, true)}
      <span class="mval">${n0(p.dmg)} · ${pct2(p.dmg / srcTotal)}</span>
    </div>`).join("");
  }).join("");
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
          <line id="tl-cross" class="tl-cross" y1="${PADT}" y2="${H - PADB}" hidden/>
        </svg>
        <div class="tl-x"><span>0s</span><span>${n0(r.duration)}s</span></div>
        <div class="tl-ymax">${n0(tlMax)}</div>
        <div id="tl-tip" class="tl-tip" hidden></div>
      </div>` : "";
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
      <div class="hero"><div><div class="hero-num">${heroNum}</div><div class="hero-sub">${heroSub}</div>${testedAt ? `<div class="hero-tested">${tr("last tested")} ${new Date(testedAt).toLocaleString()}</div>` : ""}</div></div>
      <div class="kpi-row">${kpis}</div>
      <h3>${tr("Damage by source")}</h3>
      <div class="meter">${meter.length ? meter : `<div class="sb-empty">${tr("no damage dealt")}</div>`}</div>${chart}
      <h3>Detail</h3>
      <div class="stat-table">${detail}</div>
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
  show("opt-exilus-sect", AX.hasExilus);
  show("opt-arcanes-sect", AX.arcanes.length > 0);
  show("opt-evos-sect", AX.evolutions.length > 0);
  // Seed scope from the current build once: equipped mods = fixed.
  if (!optSeeded) {
    opt.mods = {}; opt.exilus = {};
    // Everything equipped seeds as REQ (pinned) — first-ever content for
    // the auto-created "preset 1"s; afterwards the ACTIVE presets are the
    // scope (document model) and immediately overwrite this seed.
    slots.slice(0, 8).forEach((s) => { if (s.mod) opt.mods[s.mod] = "fixed"; });
    if (slots[EXILUS].mod) opt.exilus[slots[EXILUS].mod] = "fixed";
    opt.arcanes = {};
    arcanes.filter((a) => a && a !== "none").forEach((a) => { opt.arcanes[a] = "fixed"; });
    opt.evos = {};
    Object.entries(evoSel).forEach(([t, id]) => { if (id) opt.evos[t] = { [id]: "fixed" }; });
    optSeeded = true;
    bootstrapOptPresets();
  }
  renderOptMods();
  renderOptPresetBars();
  renderOptExilus();
  renderOptArcanes();
  renderOptEvos();
  renderOptEnemy();
  updateOptEstimate();
  fetchOptBuffs();
}

// The buffs across the WHOLE scope (union of every fixed/search mod + every
// searched arcane + every marked evolution option) — enumerated server-side;
// the optimizer configures these, NOT the current build's buffs. Debounced
// as the scope changes.
function fetchOptBuffs() {
  clearTimeout(optBuffTimer);
  optBuffTimer = setTimeout(async () => {
    try {
      const evolutions = {};
      Object.entries(opt.evos).forEach(([t, m]) => { const ids = Object.keys(m); if (ids.length) evolutions[t] = ids; });
      const r = await api("/api/opt-buffs",
        { weapon: $("weapon").value, mods: opt.mods, arcanes: Object.keys(opt.arcanes), evolutions });
      optBuffList = (r && r.buffs) || [];
    } catch (_) { optBuffList = []; }
    renderOptBuffs();
  }, 250);
}

function renderOptBuffs() {
  renderBuffCards($("opt-buffs"), optBuffList, opt.buffs);
}

// The mod scope: the SAME rich list as the mod picker (image, polarity icon,
// sort / polarity filter, effect lines) — the only difference is the rightmost
// control is pool/req instead of the drain. Plus a summary of selections and
// a "mods per build" size. `optPrefs` mirrors the picker's sort/filter.
function renderOptMods() {
  $("opt-size").value = opt.size;
  renderOptTools();
  renderOptModSel();
  renderOptModList();
}

// The optimizer's OWN enemy/scenario block — the same fields as the Sim
// panel's section 1 but writing `optSim` (fully decoupled; user).
function renderOptEnemy() {
  const box = $("opt-enemy");
  if (!box) return;
  const enemies = META.enemies || [];
  const en = enemies.find((e) => e.id === optSim.enemy) || enemies[0];
  if (en) optSim.enemy = en.id;
  const eopts = enemies.map((e) =>
    `<option value="${e.id}" ${e.id === optSim.enemy ? "selected" : ""}>${e.name}</option>`).join("");
  box.innerHTML = `
    <label>Enemy <select data-k="enemy">${eopts}</select></label>
    <label>Level <input type="number" data-k="level" min="1" max="9999" value="${optSim.level}"></label>
    <label class="check"><input type="checkbox" data-k="steel_path" ${optSim.steel_path ? "checked" : ""}> Steel Path</label>
    ${deployField(weaponInfo($("weapon").value) || {}, optSim)}
    <label>Duration (s) <input type="number" data-k="duration" min="1" max="3600" value="${optSim.duration}"></label>`;
  // Same split as the Sim: technique is the player's execution, and it must
  // match how the build will actually be played — the winner is only the
  // winner under these assumptions.
  const tech = $("opt-technique");
  if (tech) {
    // WHICH FORM the search fires. The optimizer scores builds the way the
    // sim will replay them, so it offers the weapon's own registered forms —
    // exactly the sim's list, cycle included where there is one to cycle.
    const w = weaponInfo($("weapon").value) || {};
    const formOpts = [
      ...(w.has_cycle ? [["incarnon_cycle", tr("Incarnon cycle")]] : []),
      ...(w.forms || []).map((f) => [f.id, w.has_cycle ? `${tr(f.name)} ${tr("only")}` : tr(f.name)]),
    ];
    if (formOpts.length && !formOpts.some(([id]) => id === optSim.form)) optSim.form = defaultFormId(w, formOpts);
    tech.innerHTML = `
    ${formField(formOpts, optSim.form)}
    ${aimField(weaponInfo($("weapon").value) || {}, optSim)}
    ${ammoField(weaponInfo($("weapon").value) || {}, optSim)}
    <label title="${escHtml(tr("a per-PELLET aim weight, not a whole-spread promise — the landing spot is rolled for each pellet"))}">${escHtml(tr("Headshot %"))} <input type="number" data-k="headshot_pct" min="0" max="100" value="${optSim.headshot_pct}"></label>`;
  }
  [box, tech].filter(Boolean).forEach((b) =>
    b.querySelectorAll("[data-k]").forEach((el) =>
      el.addEventListener("change", () => {
        const k = el.dataset.k;
        if (el.type === "checkbox") optSim[k] = el.checked;
        else if (el.type === "number") optSim[k] = Number(el.value);
        else optSim[k] = el.value;
        updateOptEstimate();
      })));
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
      <div class="info"><div class="mn">${wl(a.name, wikiUrl(a.name_en || a.name))}</div>${eff}</div>
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
      renderOptArcanes(); updateOptEstimate(); fetchOptBuffs();
    }));
}

// Evolution scope — per tier, the option rows with their verbatim description
// and a search toggle (broken evolutions flagged).
function renderOptEvos() {
  $("opt-evos").innerHTML = (weaponEvos()).map((t) => {
    const sel = opt.evos[t.tier] || {};
    const pinned = evoPinned(t.tier);
    const hasPool = Object.values(sel).some((s) => s === "search");
    const rows = t.options.map((o) => {
      const st = sel[o.id] || "off";
      // Neither mark blocks the other — clicking one rewrites the tier.
      const desc = evoLines(o).map((x) => `<div>${escHtml(x)}</div>`).join("");
      return `<div class="opt ${st === "off" ? "" : st} ${o.broken ? "dis-soft" : ""}">
        <div class="info"><div class="mn">${o.name}${o.broken ? ' <span class="exchip brk">BROKEN</span>' : ""}</div><div class="me">${desc}</div></div>
        <div class="oseg">
          <span class="seg ${st === "search" ? "on" : ""}" data-t="${t.tier}" data-e="${o.id}" data-s="search" ${pinned && pinned !== o.id ? `title="${escHtml(tr("pooling opens the tier — the pin gives way"))}"` : ""}>${tr("pool")}</span>
          <span class="seg ${st === "fixed" ? "on" : ""}" data-t="${t.tier}" data-e="${o.id}" data-s="fixed" ${hasPool ? `title="${escHtml(tr("req pins the tier — the pool marks give way"))}"` : ""}>${tr("req")}</span>
        </div>
      </div>`;
    }).join("");
    return `<div class="opt-tier-block"><div class="opt-tier-h">EVO ${ROMAN(t.tier)}</div><div class="combo-menu opt-evolist">${rows}</div></div>`;
  }).join("");
  $("opt-evos").querySelectorAll(".seg:not(.dis)").forEach((el) =>
    el.addEventListener("click", (e) => {
      e.stopPropagation();
      const t = el.dataset.t, id = el.dataset.e, want = el.dataset.s;
      opt.evos[t] = opt.evos[t] || {};
      setSingleSlotMark(opt.evos[t], id, want);
      renderOptEvos(); updateOptEstimate(); fetchOptBuffs();
    }));
}

// ---- Optimizer scope presets — THREE independent groups (user):
//   mods (mods + exilus + max size) — CROSS-WEAPON: mod pools are shared
//     between similar weapons, so a saved scope imports anywhere (ids the
//     current weapon's pool lacks are dropped on apply);
//   arcanes — cross-weapon likewise;
//   evolutions — cross-weapon TOO (user): weapon families can share the
//     same evolutions, so matching ids import; ids this weapon lacks drop
//     out harmlessly (ids are globally unique).
// Saving uses an INLINE name input — the native prompt() dialog can be
// blocked by the browser, which made saving silently fail.
// Group ids are the optimizer module's collection names; the storage /
// DOM names derive from the domain "optimizer-<group>".
const OPT_PRESET_GROUPS = {
  mods: { label: "Mod presets", hint: "cross-weapon; unknown ids drop" },
  arcanes: { label: "Arcane presets", hint: "cross-weapon; unknown ids drop" },
  evolutions: { label: "Evolution presets", hint: "cross-weapon; unknown ids drop" },
};
const optDomain = (g) => "optimizer-" + g;
// Same DOCUMENT MODEL as the build presets (user, 2026-07-28: "all presets
// use this model"): each group always has ≥1 preset, is always editing one
// (the active), saves in place, and restores the active across reloads.
let activeOptPreset = { mods: null, arcanes: null, evolutions: null };

const loadOptGroup = (g) => loadPresetList(optDomain(g));
const storeOptGroup = (g, ps) => storePresetList(optDomain(g), ps);

// Ensure every group has ≥1 preset and the actives are applied — called
// from renderOpt's seed block (page load AND weapon switch). First-ever run
// creates "preset 1" from the build-seeded scope; afterwards the active
// preset IS the scope (cross-weapon: unknown ids drop on apply).
function bootstrapOptPresets() {
  Object.keys(OPT_PRESET_GROUPS).forEach((g) => {
    let ps = loadOptGroup(g);
    if (!ps.length) {
      ps = [{ name: "preset 1", savedAt: Date.now(), state: snapshotOptGroup(g) }];
      storeOptGroup(g, ps);
    }
    const want = activeOptPreset[g] || localStorage.getItem(presetActiveKey(optDomain(g)));
    activeOptPreset[g] = ps.some((p) => p.name === want) ? want : ps[0].name;
    localStorage.setItem(presetActiveKey(optDomain(g)), activeOptPreset[g]);
    applyOptGroupState(g, ps.find((p) => p.name === activeOptPreset[g]).state);
  });
}

// One-time split of the legacy single-bar presets into the three groups.
(function migrateLegacyOptPresets() {
  try {
    const old = JSON.parse(localStorage.getItem("wfsim-opt-presets"));
    if (Array.isArray(old)) {
      old.forEach((p) => {
        const st = p.state || {};
        const add = (g, state) => {
          const ps = loadOptGroup(g);
          if (!ps.some((x) => x.name === p.name)) {
            ps.push({ name: p.name, savedAt: p.savedAt || Date.now(), state });
            storeOptGroup(g, ps);
          }
        };
        add("mods", { mods: st.mods || {}, exilus: typeof st.exilus === "object" && st.exilus ? st.exilus : {}, size: st.size || 8 });
        add("arcanes", { arcanes: st.arcanes || {} });
        add("evolutions", { evos: st.evos || {} });
      });
    }
    localStorage.removeItem("wfsim-opt-presets");
  } catch (_) {}
})();

function snapshotOptGroup(g) {
  if (g === "mods") return { mods: { ...opt.mods }, exilus: { ...opt.exilus }, size: opt.size };
  if (g === "arcanes") return { arcanes: { ...opt.arcanes } };
  return { evos: JSON.parse(JSON.stringify(opt.evos)) };
}

// State-only apply (validation + cross-weapon id dropping); no re-render.
function applyOptGroupState(g, st) {
  const norm = (s) => (s === true ? "search" : s); // boolean-era marks
  if (g === "mods") {
    // Cross-weapon import: ids missing from THIS weapon's pool drop out.
    opt.mods = {}; opt.exilus = {};
    Object.entries(st.mods || {}).forEach(([id, s]) => { if (modById(id)) opt.mods[id] = norm(s); });
    Object.entries(st.exilus || {}).forEach(([id, s]) => { const m = modById(id); if (m && m.exilus) opt.exilus[id] = norm(s); });
    delete opt.exilus["none"]; // brief None-row era
    if (st.size) opt.size = st.size;
  } else if (g === "arcanes") {
    // Cross-weapon import: another SLOT's arcanes are not equippable here, so
    // they drop rather than becoming search dimensions the run cannot use.
    opt.arcanes = {};
    const w = $("weapon").value;
    Object.entries(st.arcanes || {}).forEach(([id, s]) => {
      // ANY of the weapon's pools, not just the first — see arcaneFitsWeapon.
      // (A stored "none" fails this too, which is what we want.)
      if (arcaneFitsWeapon(w, id)) opt.arcanes[ARCANE_RENAMED[id] || id] = norm(s);
    });
  } else {
    // Cross-weapon: keep only ids the CURRENT weapon's tiers actually offer
    // (ids are globally unique, so a family sharing evolutions imports
    // cleanly and a different weapon's ids just drop).
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
  }
}

function applyOptGroup(g, st) {
  applyOptGroupState(g, st);
  optSeeded = true;
  renderOpt(); fetchOptBuffs(); updateOptEstimate();
}

function renderOptPresetBars() { Object.keys(OPT_PRESET_GROUPS).forEach(renderOptPresetBar); }

function renderOptPresetBar(g) {
  const bar = $("preset-bar-" + optDomain(g));
  if (!bar) return;
  const meta = OPT_PRESET_GROUPS[g];
  renderPresetBarIn(bar, {
    domain: optDomain(g),
    label: tr(meta.label),
    hint: meta.hint,
    load: () => loadOptGroup(g),
    store: (ps) => storeOptGroup(g, ps),
    active: () => activeOptPreset[g],
    setActive: (n) => { activeOptPreset[g] = n; localStorage.setItem(presetActiveKey(optDomain(g)), n); },
    snapshot: () => snapshotOptGroup(g),
    apply: (st) => applyOptGroup(g, st || {}),
    // An empty scope: nothing selected (size stays a fresh 8 for mods).
    blank: () => (g === "mods" ? { mods: {}, exilus: {}, size: 8 } : g === "arcanes" ? { arcanes: {} } : { evos: {} }),
    rerender: () => renderOptPresetBar(g),
  });
}

function renderOptTools() {
  const t = $("opt-picker-tools");
  const pols = ["Madurai", "Naramon", "Vazarin", "Umbra"].filter((p) => currentPool.some((m) => m.polarity === p));
  t.innerHTML =
    `<label>Sort <select id="opk-sort"><option value="name">Name</option><option value="drain">Drain</option></select></label>` +
    `<button id="opk-dir" class="ghost-btn small" title="direction">${optPrefs.dir === "asc" ? "▲" : "▼"}</button>` +
    `<span class="pk-pols"><span class="pk-pol ${!optPrefs.pol ? "sel" : ""}" data-p="">all</span>` +
    pols.map((p) => `<span class="pk-pol ${optPrefs.pol === p ? "sel" : ""}" data-p="${p}" title="${p}">${imgTag(POL(p), "pol")}</span>`).join("") +
    `</span>`;
  $("opk-sort").value = optPrefs.sort;
  $("opk-sort").onchange = () => { optPrefs.sort = $("opk-sort").value; renderOptModList(); };
  $("opk-dir").onclick = () => { optPrefs.dir = optPrefs.dir === "asc" ? "desc" : "asc"; renderOptTools(); renderOptModList(); };
  t.querySelectorAll(".pk-pol").forEach((o) => o.onclick = () => { optPrefs.pol = o.dataset.p || null; renderOptTools(); renderOptModList(); });
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
      renderOptMods(); renderOptExilus(); updateOptEstimate(); fetchOptBuffs();
    }));
  box.querySelectorAll(".oselchip[data-m]").forEach((el) =>
    el.addEventListener("click", () => revealOptMod(el.dataset.m)));
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
      <div class="info"><div class="mn">${m.riven ? escHtml(m.name) : wl(m.name, wikiUrl(m.name_en || m.name))}${m.exilus ? ' <span class="exchip">EXILUS</span>' : ""}</div><div class="me">${eff}</div></div>
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
      renderOptMods(); renderOptExilus(); updateOptEstimate(); fetchOptBuffs();
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
  const minK = fixed + (search > 0 ? 1 : 0);
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
    const en = (META.enemies || []).find((e) => e.id === optSim.enemy) || {};
    // Mirror of schedule_to()'s auto-planned cadence: k = ceil(log8(N/F))
    // rounds, even log-space culls landing exactly on the finalists, runs
    // from a halving cost budget ((ρ/2)^i, capped at final/4), then the
    // guaranteed final. Racing/amnesty adapt this plan at runtime.
    const F = optRun.finalists, FR = optRun.final_runs;
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
    scenario = `<div class="opt-scn">each build vs <b>${en.name || optSim.enemy}</b> Lv ${optSim.level}${optSim.steel_path ? " (SP)" : ""} · ${optSim.headshot_pct}% headshots${optSim.aiming ? "" : " · hip-fire"} · ${optSim.duration} s engagements · planned funnel (builds×runs): ${parts.join(" → ")} → ${F} finalists at ${FR.toLocaleString()} runs (racing cuts deeper, tie-amnesty keeps up to 2×)</div>`;
  }
  // ONE total, no decomposition — "×N arcanes" leaked a search-internal
  // dimension into the summary line (user, 2026-07-29).
  $("opt-estimate").innerHTML = (valid
    ? `~<b>${Math.round(jobs).toLocaleString()}</b> candidate builds${big ? ` <span class="warn">— large; this may take a while</span>` : ""}`
    : `<span class="warn">${dupReq ? `${(modById(dupReq) || { name: dupReq }).name} is required in both blocks — a mod equips once` : poolStarved ? `pooled mods reserve ≥1 open slot — raise max mods or clear pools` : `more required (${fixed}) than slots (${size})`}</span>`) + scenario;
  // Never re-enable while a background job is still running.
  $("run-opt").disabled = !valid || optJobId != null;
  // Every scope mutation funnels through here — AUTO-SAVE each group into
  // its active preset (debounced), same contract as the build bar.
  clearTimeout(optSaveTimer);
  optSaveTimer = setTimeout(() => {
    if (presetApplying) return;
    Object.keys(OPT_PRESET_GROUPS).forEach((g) => {
      const ps = loadOptGroup(g);
      const at = ps.findIndex((p) => p.name === activeOptPreset[g]);
      if (at < 0) return;
      ps[at] = { ...ps[at], savedAt: Date.now(), state: snapshotOptGroup(g) };
      storeOptGroup(g, ps);
    });
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
    // Buffs configured over the whole scope (opt.buffs), pruned to the current scope's ids.
    const buffs = {};
    optBuffList.forEach((b) => { const c = opt.buffs[b.id]; if (c) buffs[b.id] = { stacks: c.stacks, locked: c.locked }; });
    const body = {
      weapon: $("weapon").value,
      mods: opt.mods,
      rivens: rivenPayload(),
      build_size: opt.size,
      arcanes: arcs,
      evolutions,
      exilus: opt.exilus,
      enemy: optSim.enemy, level: optSim.level, steel_path: optSim.steel_path,
      headshot_pct: optSim.headshot_pct, aiming: optSim.aiming,
      infinite_ammo: optSim.infinite_ammo, duration: optSim.duration,
      form: optSim.form,
      final_runs: optRun.final_runs, finalists: optRun.finalists,
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
    `<div class="opt-note">round ${n.round}: ${n.jobs.toLocaleString()} × ${n.runs} (${n.by_kills ? "kills" : "dmg"}) → keep ${n.kept.toLocaleString()} · best ${n.by_kills ? sig2(kpm(n.best, optSim.duration)) + " KPM" : n.best.toExponential(2) + " dmg"} · ${(n.ms / 1000).toFixed(1)}s</div>`
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

function renderOptResults(r) {
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
    return `<div class="opt-row">
      <div class="opt-head">
        <span class="opt-rank">#${res.rank}</span>
        <span class="opt-kills">${sig2(kpm(res.kill_progress ?? res.kills, r.duration))}<small> KPM</small></span>
        <span class="opt-dps">${Math.round(res.dps || res.effective_dps || 0).toLocaleString()} DPS</span>
        <span class="opt-total">${sig2(res.kill_progress ?? res.kills)} kill score / ${Math.round(r.duration || 0)}s</span>
        <span class="forma-badge legal">${res.forma.used} Forma</span>
        <button class="ghost-btn small opt-add" title="add as a new build preset" data-r='${JSON.stringify(res).replace(/'/g, "&#39;")}'>+ add</button>
      </div>
      <div class="opt-detail"><b>${arc}</b> · ${evos}</div>
      <div class="opt-mods">${mods}</div>
    </div>`;
  }).join("");
  $("opt-results").innerHTML = `<div class="opt-meta">${r.cancelled ? `<span class="warn">cancelled — best-so-far ranking (lower precision than a full run)</span> · ` : ""}${(r.jobs || 0).toLocaleString()} candidate builds · vs ${r.target.name} Lv ${r.target.level}${r.target.steel_path ? " (SP)" : ""} · ${r.headshot_pct ?? "?"}% headshots · ${r.duration ?? "?"} s engagements · ${r.finalists || 20} finalists × ${(r.final_runs || 1024).toLocaleString()} runs</div>${rows}`;
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
    // The optimizer's per-buff config rides along — otherwise "add then
    // Run Sim" silently reverts to the Sim panel's own defaults and the
    // two scores stop matching (user, 2026-07-28).
    sim: { ...sim, buffs: JSON.parse(JSON.stringify(opt.buffs)) },
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

init().catch((e) => { document.querySelector(".config-page").insertAdjacentHTML("afterbegin", `<div class="error">failed to load: ${e}</div>`); });
