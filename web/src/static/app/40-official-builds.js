// ---- THE WIELDER -------------------------------------------------------
//
// Whoever holds the weapon: a link to one of the Warframe module's saved builds,
// or a frame with no build. NEVER EMPTY: the floor is the Prototype frame, and
// its unbuilt form (`{frame: "prototype"}`) is what every ruler scores in. A
// weapon locked to its frames (`wielders`, Valkyr Talons) offers only those and
// never the Prototype. docs/UI.md §The wielder.
const PROTOTYPE_ID = "prototype";
/// …and a robotic weapon's floor host (`engine::data::companions::PROTOTYPE_COMPANION`).
const PROTOTYPE_COMPANION_ID = "prototype_companion";
let buildWielder = { frame: PROTOTYPE_ID, preset: null };

/// WHAT A LINK NAMES, resolved: a preset that exists by its id; the DEFAULT when
/// it names the default or one that is gone; and, UNSET — never chosen — the
/// frame's first preset, or the default while it owns none. An unset link is not
/// a choice, so it follows: a preset written later is what it means from then on.
const wielderIdOf = (v) => {
  const list = wielderList(v.frame);
  if (v.preset) return list.some((x) => x.id === v.preset) ? v.preset : DEFAULT_PRESET_ID;
  return list[0] ? list[0].id : DEFAULT_PRESET_ID;
};
/// The stored build a link means; `null` for the default, the blank.
const wielderBuild = (v) => wielderList(v.frame).find((x) => x.id === wielderIdOf(v)) || null;

/// A HOLDER IS A WARFRAME OR, for a robotic weapon, A COMPANION HOST
/// (`data/companions/`) — a Sentinel or a MOA, which is what carries one. A link
/// is `{frame, preset}` either way, `frame` naming whichever it is, and a host's
/// presets are its own.
const isHostId = (id) => !!compHost(id);
const wielderList = (frame) => (isHostId(frame) ? compList(frame) : presetListWithIds(WF_BUILDS, frame));
/// The page a holder's presets are edited on.
const holderPath = (id) => (isHostId(id) ? companionPath(compHost(id)) : warframePath((META.warframes || []).find((x) => x.id === id)));
const holderName = (id) => (compHost(id) || (META.warframes || []).find((x) => x.id === id) || {}).name || id;

/// A stored wielder, repaired against this weapon: a holder that cannot hold it
/// falls back to the first one allowed, or to the default holder — the
/// Prototype, or the Prototype Companion for a robotic weapon. The preset it names
/// stays as written — resolved when read (`wielderIdOf`) — and one never written
/// is unset.
function defaultWielder(weaponId, v) {
  const info = weaponInfo(weaponId) || {};
  const allowed = info.wielders || [];
  const holds = (id) => (info.sentinel ? isHostId(id)
    : (META.warframes || []).some((f) => f.id === id) && (!allowed.length || allowed.includes(id)));
  // THE FLOOR BY NAME, never by position: the roster ends with it, and a
  // second host would otherwise become every robotic weapon's default.
  const first = info.sentinel
    ? ((compHosts().find((c) => c.id === PROTOTYPE_COMPANION_ID) || compHosts()[0] || {}).id)
    : (allowed.length ? allowed[0] : PROTOTYPE_ID);
  const frame = v && holds(v.frame) ? v.frame : first;
  return { frame, preset: v && v.frame === frame && v.preset ? v.preset : null };
}

/// The linked build as the Warframe module reads it. ABSENT for the Prototype
/// while nothing is written for it: that is the floor every ruler scores in, and
/// the wire keeps saying it as no wielder, so no stored link or board row moves.
///
/// A COMPANION HOST SENDS ITS CARDS, `{companion, mods}`, and nothing while it
/// seats none: the server reads what of them the fight runs (Fired Up) and
/// answers the rest with the companion floor (`sentinel_wielder`). No ruler
/// keeps it (`board::builds::carries_wielder`).
/// WHO IS HOLDING A WEAPON, as request fields — from a LINK rather than from
/// the editor, because a fight holds a seat per build and only one of them is
/// open (`seatPayload`).
function wielderPayloadOf(v) {
  if (isHostId(v.frame)) {
    const slots = ((wielderBuild(v) || {}).state || {}).slots || [];
    const mods = slots.filter((s) => s && s.mod).map((s) => (s.rank == null ? { id: s.mod } : { id: s.mod, rank: s.rank }));
    return mods.length ? { companion: v.frame, mods } : undefined;
  }
  const b = wielderBuild(v);
  if (b) return wfPayloadOf({ ...b.state, frame: v.frame });
  // THE BLANK BUILD STILL HAS AN OPERATOR: its link is unset, which is the first
  // Operator build (docs/WARFRAMES.md §Operator) — the same one the page shows.
  if (v.frame === PROTOTYPE_ID) return undefined;
  const operator = typeof operatorPickFor === "function" ? operatorPickFor(undefined) : null;
  return operator ? { frame: v.frame, operator } : { frame: v.frame };
}

const wielderPayload = () => wielderPayloadOf(buildWielder);

/// A FRAMED EDITOR PANE, drawn or kept: the module's own page framed as itself
/// (`?embed`). AN UNCHANGED PANE IS LEFT ALONE — replacing its `src` reloads the
/// page and drops whatever is being typed in it.
function syncFramedPane(box, name, href, src) {
  if (!box) return;
  const held = box.querySelector(".wld-pane");
  if (!held) {
    box.innerHTML = `<div class="wld-pane" data-src="${src}">
      <div class="wld-head"><b>${escHtml(name)}</b>
      <a class="ghost-btn small" href="${href}">${escHtml(tr("open the full page"))}</a></div>
      <iframe class="wld-frame" loading="lazy" src="${src}" title="${escHtml(name)}"></iframe></div>`;
  } else if (held.dataset.src !== src) {
    held.dataset.src = src;
    held.querySelector("iframe").src = src;
    held.querySelector(".wld-head b").textContent = name;
    held.querySelector(".wld-head a").href = href;
  }
}

/// WHAT THE WEAPON IS CALLED IN THESE HANDS — "Valkyr Prime Talons".
const weaponDisplayName = (w) => ((w && w.wielder_names) || {})[(buildWielder || {}).frame] || weaponEntryName(w);

function renderWeaponName() {
  const w = weaponInfo($("weapon") && $("weapon").value);
  if (w && $("w-name")) $("w-name").innerHTML = wl(weaponDisplayName(w), wikiUrl(wikiWeaponName(w))) + weaponMarketLink(w);
}

function renderWielder() {
  const host = $("wielder-row");
  if (!host || !META) return;
  const w = weaponInfo($("weapon").value) || {};
  const allowed = w.wielders || [];
  // WHAT KIND OF HOLDER, and nothing finer: the Warframes with their faces — or,
  // for a robotic weapon, the companion hosts — searchable. WHICH PRESET of it is
  // the second control's, declared here by the weapon that links.
  const items = w.sentinel
    ? compHosts().map((c) => ({ value: c.id, label: c.name }))
    : (META.warframes || []).filter((f) => !allowed.length || allowed.includes(f.id)).map((f) => ({
      value: f.id, label: f.name, image: f.image,
      hint: f.id === PROTOTYPE_ID ? tr("the floor every board is scored on") : null,
    }));
  // The default is always the last entry: owning none, it is the only one.
  const own = wielderList(buildWielder.frame);
  const linkedId = wielderIdOf(buildWielder);
  const floorHint = buildWielder.frame === PROTOTYPE_ID || isHostId(buildWielder.frame);
  const presetItems = [...own.map((p) => ({ value: p.id, label: p.name })),
    { value: DEFAULT_PRESET_ID, label: `${tr("Default")} · ${tr("read-only")}`,
      hint: floorHint ? tr("the floor every board is scored on") : tr("no build — unmodded") }];
  host.innerHTML = `<label>${escHtml(tr("Wielder"))} ${ddButton("dd-wielder", {
    value: buildWielder.frame, search: true, items,
    onPick: (frame) => setWielder(wielderLinkFor(frame)),
  })}</label> <label>${escHtml(tr("Preset"))} ${ddButton("dd-wielder-preset", {
    value: linkedId, items: presetItems,
    onPick: (id) => setWielder({ frame: buildWielder.frame, preset: id }),
  })}</label>`;
  const sub = $("wielder-sub");
  if (sub) sub.textContent = allowed.length ? tr("only these can hold it") : "";
  renderWielderDetail();
}

/// THE WIELDER'S EDITOR: the Warframe page, framed as itself (`?embed`), and the
/// Operator page framed inside IT — an Operator is a part of the frame that
/// holds it, so the nesting is the model. There is ONE copy of each build, the
/// module's own, and nothing here edits it: the pane is the module.
///
/// WHICH BUILD IT OPENS is the wielder's link. A build born or picked in the
/// pane is announced (`wfAnnounceBuild`) and becomes the wielder's without
/// reloading the pane. Left unloaded until the block opens: a shut
/// `<iframe loading=lazy>` fetches nothing, so a weapon page pays for the
/// editor only when it is asked for.
function renderWielderDetail() {
  const box = $("wielder-detail");
  const id = buildWielder.frame;
  const name = holderName(id);
  // A FRAMED PAGE NEVER FRAMES: it has this markup too, and would nest itself.
  if (!box || EMBED || (!isHostId(id) && !(META.warframes || []).some((x) => x.id === id))) return;
  const linked = wielderBuild(buildWielder);
  const opBuild = linked && !isHostId(id) ? opBuildOf((linked.state || {}).operator) : null;
  syncFramedPane(box, name, holderPath(id),
    `${holderPath(id)}?embed=1&build=${encodeURIComponent(wielderIdOf(buildWielder))}`);
  const sub = $("wielder-sub");
  if (sub && !allowedWielders().length) {
    sub.textContent = [name, linked && linked.name,
      opBuild && `${tr("Operator")}: ${opBuild.name}`].filter(Boolean).join(" · ");
  }
}
const allowedWielders = () => (weaponInfo($("weapon").value) || {}).wielders || [];

// A BUILD OPENED OR BORN IN A PANE IS THE WIELDER'S, and what a pane writes is
// read back here: the storage event reaches this page from the framed one.
window.addEventListener("message", (e) => {
  const d = e.data;
  // ONLY A WEAPON PAGE HAS A WIELDER: a framed page, or a Warframe or companion
  // page, receives the same message from ITS pane and answers it itself.
  if (EMBED || document.querySelector(".config-page").hidden) return;
  if (e.origin !== location.origin || !d || d.wfsim !== "wielder-build") return;
  if (!d.id || d.frame !== buildWielder.frame) return;
  // THE PANE IS ALREADY ON IT: say so before anything redraws, or the redraw
  // reloads the pane it came from.
  const held = document.querySelector("#wielder-detail .wld-pane");
  if (held) held.dataset.src = `${holderPath(d.frame)}?embed=1&build=${encodeURIComponent(d.id)}`;
  // An unset link already means it (the first), so nothing is chosen for it.
  if (d.id !== wielderIdOf(buildWielder)) setWielder({ frame: d.frame, preset: d.id });
});
let wielderSync = null;
window.addEventListener("storage", (e) => {
  if (EMBED || !e.key || !e.key.startsWith("wfsim-presets-")) return;
  clearTimeout(wielderSync);
  wielderSync = setTimeout(() => {
    // A PRESET WRITTEN OR DELETED ELSEWHERE: the link resolves again when read,
    // so it lands on the default, or follows a new first, by itself.
    if (META) { renderWielder(); renderSimBuild(); refreshPanel(); }
  }, 200);
});

/// THE LINK FOR A FRAME, chosen by whoever links: the type, with no preset
/// chosen — unset, so it follows the frame's first. What the frame's own page
/// has open is that page's and moves no link; the weapon says which preset it
/// means (the Wielder's second control).
const wielderLinkFor = (frame) => ({ frame, preset: null });

/// WHO LINKS A PRESET, read off the links themselves — nothing stores it, so it
/// cannot disagree with them. A weapon build links a Warframe preset (`wielder`,
/// the Prototype's unset one meaning the frame's first), and a Warframe preset
/// links an Operator preset (`operator`). Each entry is a name a reader knows.
function linkersOfWarframePreset(frame, id) {
  const first = (presetListWithIds(WF_BUILDS, frame)[0] || {}).id;
  const hit = (v) => v && v.frame === frame && (v.preset ? v.preset === id : id === first);
  return weaponLinks((state, w) => !(w && w.sentinel)
    && hit(state.wielder || (frame === PROTOTYPE_ID ? { frame, preset: null } : null)));
}
/// WHO LINKS A COMPANION PRESET: the weapon builds holding it.
function linkersOfCompanionPreset(companion, id) {
  const first = (compList(companion)[0] || {}).id;
  return weaponLinks((state, w) => {
    if (!(w && w.sentinel)) return false;
    const v = state.wielder || { frame: companion, preset: null };
    return v.frame === companion && (v.preset ? v.preset === id : id === first);
  });
}
/// Every stored weapon build whose state passes `test`, as "<weapon> · <build>".
function weaponLinks(test) {
  const out = [];
  for (const p of loadPresetWhole(BUILDS)) {
    if (!p || typeof p !== "object") continue;
    const w = (META.weapons || []).find((x) => x.id === p.scope);
    if (test(p.state || {}, w)) out.push(`${w ? w.name : p.scope} · ${p.name}`);
  }
  return out;
}
function linkersOfOperatorPreset(id) {
  const out = [];
  for (const f of META.warframes || []) {
    for (const p of presetListWithIds(WF_BUILDS, f.id)) {
      if (opIdOf((p.state || {}).operator) === id) out.push(`${f.name} · ${p.name}`);
    }
  }
  return out;
}

/// WHO HOLDS THE WEAPON — a frame, or a frame's saved build. Set by the
/// dropdown and the door alike.
function setWielder(v) {
  buildWielder = v;
  renderWielder();
  renderSimBuild();
  renderWeaponName();
  markPresetDirty();
  refreshPanel();
}
/// The frames that may hold this weapon, each with its saved builds.
const wielderChoices = () => {
  const w = weaponInfo($("weapon").value) || {};
  const allowed = w.wielders || [];
  const holders = w.sentinel ? compHosts()
    : (META.warframes || []).filter((f) => !allowed.length || allowed.includes(f.id));
  return {
    prototype_allowed: !allowed.length && !w.sentinel,
    frames: holders.map((f) => ({
      id: f.id, name: f.name, builds: wielderList(f.id).map((p) => ({ id: p.id, name: p.name })),
    })),
  };
};

// ---- THE OFFICIAL BUILDS ----------------------------------------------
//
// The board (`data/benchmarks/boards/`), as read-only chips in the BUILD bar.
// Not a tab: what a board row produces is a BUILD, and the
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
/// WHICH BOARD THIS IS — `site/board.meta.json`, written by both writers of the
/// board itself (`scripts/board_meta.py`). It is small on purpose: the payload
/// beside it is 4.3 MB, and "is the copy I hold current" must not cost that.
///
/// ABSENT IS A STATE, not a failure — the dev server has no stamp, and a page
/// that cannot say when the board was scored says nothing rather than guessing.
let BOARD_META = null;
/// THE LIVE BOARD'S OWN ADDRESS: its bucket, served by the CDN with no worker
/// in the way (worker/live_board.js), at the same paths as on the site.
///
/// The board is not part of a release, so a client that has not fetched one has
/// nothing to serve — and a shell answering same-origin cannot invent it. On a
/// shell it is asked second and never first, the local copy being the offline
/// one this exists to keep; on wfsim.app it is asked first (`boardOffOrigin`).
const BOARD_ORIGIN = "https://cdn.wfsim.app";

/// `res.ok` is not the whole test: a shell that falls back to its SPA answers
/// `index.html` with a 200, so what decides is whether the body PARSES.
async function fetchJson(url) {
  try {
    const r = await fetch(url, { cache: "no-cache" });
    if (r.ok) return await r.json();
  } catch (_) { /* nothing */ }
  return null;
}

/// WHICH WEAPON THIS URL IS ABOUT, without routing to it.
///
/// The router resolves the same slug and cannot be called yet — it draws, and
/// boot has not filled the page. This answers the ONE question boot needs
/// before it can fetch the right board: which weapon is about to be opened.
/// Uses the router's canonical id and wiki-name resolver.
function routeWeaponId() {
  const m = location.pathname.match(/^\/weapons\/([^/]+?)(\/[a-z-]+)?\/?$/);
  if (!m) return "";
  const w = headlessWeaponAt(META.weapons || [], m[1]);
  return w ? w.id : "";
}

/// WHICH WEAPONS' ROWS ARE IN HAND — and it is NOT `Object.keys(BOARD)`. A
/// weapon nobody has submitted has an empty list, and "loaded and empty" is a
/// different answer from "never fetched" to everything that states a rank.
const BOARD_HAVE = new Set();

/// EVERY WEAPON'S GROUP LEADERS, which is the only thing that ranks ACROSS
/// weapons — `site/board/index.json`, derived by the publisher from the weapon
/// files it writes.
///
/// IT IS NOT A BOARD AND MUST NOT BE MISTAKEN FOR ONE. One row per (ruler, mode,
/// riven) per weapon is exactly what `benchEntries` reduces to, and nothing
/// else: a weapon page reads its own file, so this never reaches `BOARD` and
/// never enters `BOARD_HAVE`. It is 37 KB on the wire where the whole board,
/// which this view is the only reason to fetch, is 249.
let BOARD_INDEX = null;

/// ONE WEAPON'S ROWS, BECAUSE THE PAGE SHOWS ONE WEAPON.
///
/// 387 weapons are 387 files and a weapon page reads one, so boot fetches the
/// file it is about to draw and nothing else. A reader who never opens the
/// benchmark page never downloads the rest.
///
/// `base` IS THE SITE WHEN THIS ORIGIN HOLDS NO BOARD — see `boardOffOrigin`.
/// The index alone is not enough for a weapon page: the ranking across weapons
/// and the rows one weapon draws are different files, so a client served only
/// the first has a populated benchmark page and an empty board everywhere else.
async function loadWeaponBoard(id, base = boardOffOrigin ? BOARD_ORIGIN : "") {
  if (!id || BOARD_HAVE.has(id)) return;
  let rows = null;
  // A TOP-N CARD READS THE HEADS, every scene in one file (docs/BOARD.md §"The
  // heads"): its places are every row's, from a fraction of the rows. A weapon
  // with no heads yet, and every other page, reads every row.
  if (/\/card\/?$/.test(location.pathname) && cardParams().kind === "pz") {
    try { rows = await fetchJsonPatient(`${base}/board/heads/${id}.json`); } catch (_) { /* every row, below */ }
    if (rows) {
      BOARD[id] = rows;
      BOARD_HAVE.add(id);
      return;
    }
  }
  try { rows = await fetchJsonPatient(`${base}/board/${id}.json`); } catch (_) { /* unreachable */ }
  // NULL IS NOT AN EMPTY BOARD. A dev server has no `board/` at all, and a
  // weapon left unloaded is what `boardProjection` refuses to speak about.
  if (!rows) return;
  BOARD[id] = rows;
  BOARD_HAVE.add(id);
  loadSceneRows(id, base);
}

/// …AND ITS ROWS IN EVERY OTHER SCENE, `board/<cell_dir>/<id>.json`, joined
/// as they land: a cell is an ordinary ruler, so every reader of `BOARD` groups
/// them by `benchmark`. NEVER AWAITED — a route waits on the base rows alone,
/// and a desktop shell asks the site for these (its offline copy holds the base
/// alone, `scripts/board_meta.py`), which a slow network must not turn into a
/// page that never opens. A scene that has not measured this weapon has no file,
/// and one the stamp lists without it (`scene_weapons`) is not asked.
const SCENE_ROWS_WITHIN_MS = 8000;
function loadSceneRows(id, base) {
  const sceneBase = window.__WFSIM_DESKTOP__ ? BOARD_ORIGIN : base;
  const listed = (BOARD_META && BOARD_META.scene_weapons) || {};
  for (const dir of benchScenes().filter(Boolean)) {
    if (Array.isArray(listed[dir]) && !listed[dir].includes(id)) continue;
    fetch(`${sceneBase}/board/${dir}/${id}.json`, { cache: "no-cache", signal: timeoutSignal(SCENE_ROWS_WITHIN_MS) })
      .then((r) => (r.ok ? r.json() : null))
      .then((more) => {
        if (!Array.isArray(more) || !more.length || !BOARD[id]) return;
        BOARD[id] = BOARD[id].concat(more);
        boardRowsLanded(id);
      }, () => { /* the base rows stand alone */ });
  }
}

/// A BOARD FILE THROUGH A BAD NETWORK: a request that never arrived is asked
/// again, and a file that is not there is not. Resolves null for an absent or
/// unparseable file and THROWS once every try failed to arrive — the page tells
/// "this board is empty" from "the board could not be reached" by that.
async function fetchJsonPatient(url, tries = 3) {
  for (let i = 0; ; i++) {
    try {
      const r = await fetch(url, { cache: "no-cache" });
      if (r.ok) {
        try { return await r.json(); } catch (e) { if (e instanceof SyntaxError) return null; throw e; }
      }
      if (r.status < 500) return null;
      if (i + 1 >= tries) throw new Error(`HTTP ${r.status}`);
    } catch (e) {
      if (i + 1 >= tries) throw e;
    }
    await new Promise((res) => setTimeout(res, 600 * 2 ** i));
  }
}

/// EVERY WEAPON'S LEADERS. Awaited only where a rank ACROSS weapons is drawn.
///
/// ONE REQUEST AT A TIME, and its state is what the benchmark page draws while
/// it has no index: in flight is "loading", unreachable is "retry" — never the
/// empty table, which reads as a board nobody has submitted to.
let boardIndexAsk = null;
let boardIndexUnreachable = false;
function loadFullBoard() {
  if (BOARD_INDEX) return Promise.resolve();
  if (!boardIndexAsk) {
    boardIndexUnreachable = false;
    boardIndexAsk = fetchJsonPatient(`${boardOffOrigin ? BOARD_ORIGIN : ""}/board/index.json`)
      .then((idx) => { BOARD_INDEX = idx; }, () => { boardIndexUnreachable = true; })
      .finally(() => { boardIndexAsk = null; });
  }
  return boardIndexAsk;
}

/// EVERY OTHER SCENE'S LEADERS, by its directory (`cell_dir` in `/api/meta`):
/// `site/board/<scene>/index.json`, `BOARD_INDEX`'s shape. A weapon page reads
/// the base board alone, so none of this reaches `BOARD`. A scene nobody has
/// measured has no file, and is an empty board rather than a missing one.
///
/// THE DESKTOP SHELL ASKS THE SITE: its offline copy holds the base board
/// alone (`scripts/board_meta.py`), and its own 404 would read as that empty board.
const SCENE_INDEX = {};
const sceneIndexAsk = {};
const sceneIndexUnreachable = {};
function loadSceneIndex(dir) {
  if (SCENE_INDEX[dir]) return Promise.resolve();
  if (!sceneIndexAsk[dir]) {
    sceneIndexUnreachable[dir] = false;
    sceneIndexAsk[dir] = fetchJsonPatient(`${boardOffOrigin || window.__WFSIM_DESKTOP__ ? BOARD_ORIGIN : ""}/board/${dir}/index.json`)
      .then((idx) => { SCENE_INDEX[dir] = idx || {}; }, () => { sceneIndexUnreachable[dir] = true; })
      .finally(() => { delete sceneIndexAsk[dir]; });
  }
  return sceneIndexAsk[dir];
}
/// THE LEADERS A RULER'S RANKING READS, and the state of asking for them.
const benchIndexOf = (cur) => (cur && cur.cell_dir
  ? { index: SCENE_INDEX[cur.cell_dir] || null, asking: !!sceneIndexAsk[cur.cell_dir],
    lost: !!sceneIndexUnreachable[cur.cell_dir], load: () => loadSceneIndex(cur.cell_dir) }
  : { index: BOARD_INDEX, asking: !!boardIndexAsk, lost: boardIndexUnreachable, load: loadFullBoard });

/// The benchmark page: ask first, so the first draw already knows a request is
/// in flight, then draw again when it settles.
function showBenchBoard() {
  benchReadUrl();
  const ask = benchIndexOf(benchCurrent()).load();
  renderBenchBoard();
  ask.then(() => {
    if ($("bench-board") && !$("bench-page").hidden) renderBenchBoard();
  });
}

/// SAME-ORIGIN ONLY, BECAUSE BOOT WAITS ON THIS. A client whose own origin has
/// not got a board yet must not spend a first launch downloading one before the
/// page appears — "opens instantly" is the whole reason the desktop build
/// exists. So the site is asked afterwards, off the boot path, and the view
/// redraws if an answer arrives.
/// THE STAMP IS ASKED BESIDE THE ROWS, so its scene lists (`scene_weapons`)
/// are usually in hand when the weapon's scenes are asked.
async function loadBoard(weapon) {
  const stamp = fetchJson(`${boardOffOrigin ? BOARD_ORIGIN : ""}/board.meta.json`).then((m) => (BOARD_META = m));
  await loadWeaponBoard(weapon);
  if (!(await stamp) && !boardOffOrigin) {
    boardOffOrigin = true;
    boardFromSite(weapon);
  }
}

/// THIS ORIGIN HOLDS NO BOARD OF ITS OWN, and every later weapon asks
/// `BOARD_ORIGIN` first because of it. `board.meta.json` not answering is the
/// whole test: a shell keeps one beside its release, and a shell too old to
/// know about that directory never will. WFSIM.APP STARTS OFF ITS ORIGIN: its
/// own `/board/*` is a worker call per file, and the same files are free on
/// the board's address.
let boardOffOrigin = location.hostname === new URL(LIVE_ORIGIN).hostname;

/// The rows for `id` landed — and they are PRESETS, so this is `initPresets`
/// and not a repaint: a bar redrawn without them is a bar with no benchmark
/// builds in it, which is what the reader came for.
///
/// ONLY IF THE READER IS STILL THERE. A slow answer for a weapon they have
/// already left must not redraw the one they are looking at.
function boardRowsLanded(id) {
  if (!$("weapon") || $("weapon").value !== id) return;
  const target = routeWeaponId();
  // The next route owns the page while its board is still loading.
  if (target && target !== id) return;
  const reportWeapon = optimizationReport?.request?.weapon || simulationReport?.weapon;
  if (reportWeapon === id || ((optimizationReport?.unavailable && new URLSearchParams(location.search).has("task-result"))
      || (simulationReport?.id && new URLSearchParams(location.search).has("measurement")))) {
    renderPresetBar(); return;
  }
  try {
    // A scene can land inside an edit's debounce; save before restoring it.
    flushPresetSaves();
    const address = location.pathname + location.search + location.hash;
    if (!startEdit) initPresets();
    if (location.pathname + location.search + location.hash !== address) history.replaceState(history.state, "", address);
    renderPresetBar(); refreshPanel();
  }
  catch (_) { /* nothing is showing it yet */ }
}

/// A WEAPON THE READER SWITCHED TO, fetched without blocking the switch.
///
/// NOT AWAITED, the same terms as `boardFromSite`: the rows redraw when they
/// land, and a weapon page renders perfectly well before they do. Awaiting it
/// would put a network round trip inside a control that is otherwise instant.
function ensureWeaponBoard(id) {
  if (!id || BOARD_HAVE.has(id)) return;
  loadWeaponBoard(id, boardOffOrigin ? BOARD_ORIGIN : "").then(() => boardRowsLanded(id));
}

/// The site, for a shell whose own origin has no board: one that has never
/// reached the network, or one older than the release that stopped shipping a
/// copy. The index carries `Access-Control-Allow-Origin` for exactly this.
///
/// IT FETCHES THE OPEN WEAPON'S ROWS TOO, because the ranking across weapons
/// and the rows a weapon page draws are different files. A client handed only
/// the index has a populated benchmark page and an empty board on every weapon,
/// with nothing on screen saying which of the two it is looking at.
///
/// NOT AWAITED ANYWHERE. It redraws when it lands, and an empty board until
/// then is a state the page already renders.
async function boardFromSite(weapon) {
  const idx = await fetchJson(BOARD_ORIGIN + "/board/index.json");
  if (!idx) return;
  BOARD_INDEX = idx;
  BOARD_META = await fetchJson(BOARD_ORIGIN + "/board.meta.json");
  try { renderBenchBoard(); } catch (_) { /* nothing is showing it yet */ }
  await loadWeaponBoard(weapon, BOARD_ORIGIN);
  boardRowsLanded(weapon);
}

/// THE BOARD'S ROWS, as read-only builds you can open.
///
/// RANKED WITHIN A RULER AND A MODE, because that is the only thing a rank is.
/// `#1` across everything the board holds is unambiguous exactly while there
/// is one benchmark and one way to play; two of each turn it into a number
/// that names nothing. So the grouping is (benchmark, mode) and the chip says
/// which.
///
/// The mode travels IN THE STATE, so opening a board build plays it the way it
/// was measured. Without that, picking "#1" would show its mods in whatever
/// mode you happened to be in and quietly report a different number than the
/// board does — the same shape as the scenario leak, and worse, because this
/// one has a published figure sitting next to it.

/// A BOARD ROW'S BUILD, WITHOUT THE CELL IT WAS RANKED IN.
///
/// A CELL IS ONE BENCHMARK, ONE MODE AND ONE RIVEN-NESS, and a build is scored
/// in every one its weapon has — so counting ROWS counts it up to
/// `benchmarks x modes` times, by a multiple that is the weapon's mode count.
/// That makes a row count not merely large but INCOMPARABLE between weapons.
///
/// DERIVED BY EXCLUSION, never by listing the axes, because a hand-written
/// list is the copy that drops the next axis in silence. Out comes the cell,
/// the measurement and the presentation.
///
/// A KEY THAT HAS LEFT THE ROW STAYS ON THIS LIST: `source` is no longer
/// published, and the rows a reader already has still carry it — excluded, the
/// two hash alike, so a remembered board build survives the rescore that drops
/// the key rather than falling out of the bar.
///
/// …AND A RIVEN IS ITS SHAPE: each cell searches its own best corner, so the
/// rolls differ per cell and are a measurement like the score.
const BOARD_CELL_KEYS = ["benchmark", "mode", "score", "rank", "shown", "source"];
function boardRowIdentity(row) {
  const build = {};
  for (const k of Object.keys(row || {}).sort()) {
    if (BOARD_CELL_KEYS.includes(k)) continue;
    build[k] = k === "riven" && row[k] && typeof row[k] === "object"
      ? { bonuses: row[k].bonuses || [], malus: row[k].malus || "" }
      : row[k];
  }
  return JSON.stringify(build);
}

/// HOW DEEP THE LIST GOES, as a share of each group's leader.
///
/// THE BOARD PUBLISHES EVERY ROW IT SCORED; this is the only thing that
/// decides how many of them a reader sees, and it is theirs to change. The
/// default is everything the board kept: the entry line already decided what
/// is worth a row, so a reader narrows from there and never past it.
///
/// A SLIDER IN STEPS OF `BOARD_DEPTH_STEP`, from the leader alone down to the
/// board's own ENTRY LINE (`META.board_entry_share`, `data::boards`'s
/// `KEEP_LEADER_SHARE`). Its last stop is ZERO, "everything the board kept",
/// never a copy of the line: no filter stays true wherever the line is drawn.
const BOARD_DEPTH_STEP = 0.05;
const BOARD_DEPTH_DEFAULT = 0;
const isBoardDepth = (v) => v === 0 || (v > 0 && v <= 1 && Math.abs(v / BOARD_DEPTH_STEP - Math.round(v / BOARD_DEPTH_STEP)) < 1e-9);
let boardDepth = BOARD_DEPTH_DEFAULT;
try {
  const s = Number(localStorage.getItem("wfsim-board-depth"));
  if (localStorage.getItem("wfsim-board-depth") !== null && isBoardDepth(s)) boardDepth = s;
} catch (_) { /* private mode */ }
const setBoardDepth = (v) => {
  boardDepth = v;
  try { localStorage.setItem("wfsim-board-depth", String(v)); } catch (_) { /* private mode */ }
};

/// A ROW'S OWN GROUP — one ruler, one mode, one riven-ness — and the best score
/// in it. That is the denominator `boardDepth` is drawn against, and stating it
/// once is what keeps the control's counts and the list it filters in step.
///
/// A riven build and a plain one compete with each other for nothing, and one
/// ruler's leader says nothing about another's; a shared reference would let
/// whichever group is stronger decide what the others may show, which on most
/// weapons means the builds most players can actually make are the ones to
/// disappear.
function boardGroupLeaders(rows) {
  const key = (r) => `${r.benchmark}#${r.mode || "base"}#${rowHasRiven(r) ? "r" : "p"}`;
  const best = {};
  for (const r of rows || []) best[key(r)] = Math.max(best[key(r)] ?? -1, r.score || 0);
  // INCLUSIVE, and a group whose leader scored ZERO is never emptied: every row
  // ties it, and a ratio has nothing to say with no scale to say it on.
  return (r, depth) => (r.score || 0) >= depth * (best[key(r)] || 0);
}

/// How many rows a weapon's board would show at `depth`. Counted from the raw
/// rows, because the control has to say what each depth WOULD give, not what
/// the depth in force already gave.
const depthCount = (w, depth) => {
  const rows = BOARD[(w || {}).id] || [];
  const deep = boardGroupLeaders(rows);
  return rows.filter((r) => deep(r, depth)).length;
};

/// ONE CONVERSION PER BOARD, NOT PER CALLER. A render asks for these five times
/// (finder, bar, the bar's opened chips, the line under it) and a 1,300-row
/// board is ~60 ms each. Keyed on every input the conversion reads — the rows
/// ARRAY (a fetch or a rescore replaces it, nothing edits one in place), the
/// weapon, its pool and the language — so a stale answer needs one of them to
/// change without being replaced.
let builtinMemo = null;
const builtinBuilds = () => {
  const w = weaponInfo($("weapon").value) || {};
  const m = builtinMemo;
  if (m && m.w === w.id && m.rows === BOARD[w.id] && m.pool === currentPool && m.lang === LANG
      && m.i18n === I18N && m.depth === boardDepth) {
    return m.out;
  }
  const out = builtinBuildsUncached(w);
  builtinMemo = { w: w.id, rows: BOARD[w.id], pool: currentPool, lang: LANG, i18n: I18N,
    depth: boardDepth, out };
  return out;
};
const builtinBuildsUncached = (w) => {
  // THE READING ORDER IS `rankBoard`'s (`87-headless.js`), so the bar and a
  // headless caller rank one board one way.
  //
  // DEEP ENOUGH TO READ, and no deeper — see `boardDepth`. It is drawn
  // against the row's own group, and the boundary is INCLUSIVE, so a row
  // exactly on the line is shown. FILTERING BEFORE THE RANK IS WHAT KEEPS `#1`
  // MEANING `#1`: what survives is always a prefix of a group, and every
  // group's leader survives it.
  const deep = boardGroupLeaders(BOARD[w.id]);
  const rows = (BOARD[w.id] || []).filter((r) => deep(r, boardDepth));
  return rankBoard(rows, benchList().map((b) => b.id), w.modes).map(({ row, mode, riven: rv, rank: n, key }) => {
    const bench = (META.benchmarks || []).find((b) => b.id === row.benchmark);
    // WHAT THIS ROW IS BEST OF, said in its own name. A board that holds both
    // kinds has two leaders per weapon and mode, and "#1" alone would be the
    // one number here that does not say what it ranks among.
    const kind = rv ? tr("riven") : "";
    // A QUALIFIER IN BRACKETS, not another dot-separated part. "Incarnon cycle · riven" reads as two facts of equal weight
    // and the group above it is already dot-separated; "Incarnon cycle (riven)"
    // reads as the same mode, narrowed — which is what it is, and what makes
    // the plain group next to it obviously the rest.
    // THE MODE IS ALWAYS PART OF THE NAME. This label travels away from the
    // control that picked it — the simulator tab, a shared card — where there
    // is no list to infer the missing term from.
    const m = modeLabel(w, mode);
    const label = m && kind ? `${m}（${kind}）` : (m || kind);
    return {
      name: label ? `#${n} · ${label}` : `#${n}`,
      // THE MODE IN WORDS, for the control that picks one — stated even where
      // the name above omits it, since the control is drawn from these entries.
      modeName: modeLabel(w, mode),
      // The rank alone, for the control that picks a row.
      rank: n,
      // Unique per ruler, mode AND kind: the id is what the active pointer
      // stores, and a plain #1 and a riven #1 are two different builds.
      builtin: key,
      // STATED, not parsed back out of the id above. The id is a durable key
      // and reading a fact out of one is how a rename becomes a wrong answer.
      riven: rv,
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
      state: boardRowState(w, row),
    };
  });
};
/// A BOARD ROW AS A BUILD STATE — what the build bar opens, and what the
/// Forma optimizer reads and saves.
function boardRowState(w, row) {
  const axes = headlessBoardRowAxes(pageSay(), w, row, {});
  if (row.riven) axes.slots.forEach((s) => {
    if (isRivenId(s.mod)) s.mod = boardRivenRef(w, row.riven);
  });
  return buildState(w.id, axes);
}
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
function applyBenchLink(w, benchId, wantMode, wantRiven) {
  const sc = builtinScenarios().find((s) => s.builtin === benchId);
  if (sc) pickPreset(scenarioBarCfg(), presetId(sc));
  let rows = builtinBuilds().filter((p) => p.benchmark === benchId);
  if (!rows.length) return;
  // WHICH OF THE TWO LEADERS, narrowed FIRST because it is the coarser split:
  // a weapon's riven rows and its plain ones are two rankings, and taking the
  // mode's leader out of the union hands back whichever of the two happens to
  // score higher. Skipped when the link predates the parameter, and skipped
  // when it would leave nothing — a board that holds only plain rows still has
  // to answer a `riven=1` link with the row it does have rather than with
  // silence.
  if (wantRiven !== null && wantRiven !== undefined) {
    const narrowed = rows.filter((p) => !!p.riven === wantRiven);
    if (narrowed.length) rows = narrowed;
  }
  // The mode the link asked for, else however this weapon is played — the
  // board's own row order inside a ruler is best-first, so `find` is the
  // leader either way.
  const want = wantMode && (w.modes || []).includes(wantMode) ? wantMode : null;
  const row = (want && rows.find((p) => p.mode === want)) || rows[0];
  pickPreset(buildBarCfg(), presetId(row));
}

const buildList = () => builtinBuilds().concat(loadPresetList(BUILDS));
const buildNamed = (n) => presetFind(buildList(), n);
/// Is the build on screen one of the official ones? Then nothing may write it.
const officialBuildActive = () => !!(buildNamed(activePreset) || {}).builtin;

// A benchmark's display name from its id, localized. Falls back to the raw id
// rather than to nothing: a board row naming a benchmark this build of the site
// does not carry is still a row, and hiding which ruler it used would make it
// look like it had none.
const benchmarkName = (id) => benchShort(id);

/// The build collection's config, as a FACTORY like `scenarioBarCfg` — it used
/// to be a local inside the renderer, which meant the only way to select a
/// build was to be a bar. The router selects one too, when a board row names
/// the ruler it came from.
function buildBarCfg() {
  const cfg = {
    domain: BUILDS,
    label: tr("Builds"),
    noun: BUILD_NOUN,
    // Sharing belongs to the BUILD bar: what travels is the open build, plus
    // everything needed to reproduce its number.
    extra: SHARE_ENABLED ? `<button class="pchip share">${escHtml(tr("share"))}</button>` : "",
    onExtra: (bar) => {
      const b = bar.querySelector(".share");
      if (b) b.onclick = (e) => { e.stopPropagation(); openSharePanel(bar); };
    },
    load: () => loadPresetList(BUILDS),
    store: (ps) => storePresetList(BUILDS, ps),
    // THE BOARD'S BUILDS, read-only; one opened from the finder or a board link
    // is kept by what it is, never by its rank (`boardRef`).
    published: builtinBuilds,
    pins: { key: () => openedBoardKey(presetWeapon()), ref: boardRef, same: sameBoardRef },
    roGroup: tr("From the board · read-only"),
    readonly: (p) => !!p.builtin,
    roTitle: (p) =>
      tr("a benchmark build — measured under") + " " + benchmarkName(p.benchmark),
    active: () => activePreset,
    setActive: (n) => { activePreset = n; localStorage.setItem(presetActiveKey(BUILDS), n); noteBoardActive(n); },
    snapshot: snapshotState,
    copyState: materialiseReferencedRivens,
    // Never the payload's weapon — the scope's. See restoreState.
    // The benchmark build's Forma plan is NOT applied here — `restoreState`
    // owns it, so the boot path gets it too. See the comment there.
    apply: (st) => restoreState(st, presetWeapon()),
    blank: blankBuildState,
    // KNOWN ONLY ONCE THE BLANK HAS BEEN SEEN for this weapon (`pristineBuild`),
    // which is why a build opened straight onto a preset is not deleted for
    // being emptied until a blank has been applied here.
    isBlank: (st) => pristineBuild !== null && JSON.stringify(canon(st)) === pristineBuild,
    pristine: notePristineBuild,
    rerender: () => { renderPresetBar(); lockOfficialBuild(); },
    unpin: (id) => unpinPublished(cfg, id),
  };
  return cfg;
}

/// A BUILD'S CONTENTS, IN ONE LINE — every axis it varies on and no other.
///
/// `engine::board::builds::BUILD_AXES` declares what a build consists of, so a
/// describer that omits one is a ranking that cannot say what it scored — on an
/// adversary weapon, which ELEMENT. docs/CHECKS.md `check_opt_row_axes`.
///
/// IT TAKES A DESCRIPTOR RATHER THAN READING STATE: a result off the wire is
/// not the page's live slots, and the sentence is the same either way.
function buildContentsHtml(b) {
  const modName = (id) => {
    const [card, r] = splitRank(id);
    return ((modById(card) || { name: null }).name || prettify(card)) + (r != null ? ` R${r}` : "");
  };
  const bits = [];
  // THE MODE FIRST, because it decides what every number after it means.
  if (b.modeLabel) bits.push(`<span class="bl-mode">${escHtml(b.modeLabel)}</span>`);
  // …AND THE ELEMENT, on the weapons that have one. It is part of what the
  // build IS — two Kuva Nukors differing only in progenitor are two builds
  // with two scores — so a line that omits it describes neither.
  if (b.valence) {
    bits.push(`<span class="bl-valence">${escHtml(tr(prettify(b.valence)))}</span>`);
  }
  // THE PARTS, where the assembly IS the stat line (a Kitgun).
  const parts = b.assembly && Object.values(b.assembly).filter(Boolean);
  if (parts && parts.length) {
    bits.push(escHtml(parts.map((x) => prettify(x)).join(" + ")));
  }
  const arcs = (b.arcanes || [])
    .map((id, i) => (id && id !== "none"
      ? `${arcName(id)}${b.arcaneRanks && b.arcaneRanks[i] != null ? ` r${b.arcaneRanks[i]}` : ""}`
      : ""))
    .filter(Boolean);
  bits.push(`<b>${escHtml(arcs.join(" + ") || tr("no arcane"))}</b>`);
  const evos = (b.evolutions || []).map(evoName).filter(Boolean);
  if (evos.length) bits.push(escHtml(evos.join(" · ")));
  // THE RIVEN IS A MOD and rides in the mod list, but a line that leaves it
  // unnamed cannot tell a riven build from the plain one beside it.
  if (b.riven) bits.push(`<span class="bl-riven">${escHtml(tr("With riven"))}</span>`);
  const mods = (b.mods || []).map(modName).join(", ")
    + (b.exilus && b.exilus !== "none" ? `, ${modName(b.exilus)} (exilus)` : "");
  return `<div class="opt-detail">${bits.join(" · ")}</div>`
    + `<div class="opt-mods">${escHtml(mods)}</div>`;
}

