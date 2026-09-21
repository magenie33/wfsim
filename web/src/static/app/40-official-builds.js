// ---- THE WIELDER -------------------------------------------------------
//
// Whoever holds the weapon: a link to one of the Warframe module's saved builds,
// or a frame with no build. NEVER EMPTY: the floor is the Prototype frame, and
// its unbuilt form (`{frame: "prototype"}`) is what every ruler scores in. A
// weapon locked to its frames (`wielders`, Valkyr Talons) offers only those and
// never the Prototype. docs/UI.md §The wielder.
const PROTOTYPE_ID = "prototype";
let buildWielder = { frame: PROTOTYPE_ID, preset: PRESET_SEED_ID };

/// THE STORED BUILD A LINK MEANS: the preset it names, else the frame's first —
/// the repair for one that is gone — and none when the frame owns none, which is
/// its virtual "preset 1", the blank.
const wielderBuild = (v) => {
  const list = presetListWithIds(WF_BUILDS, v.frame);
  return list.find((x) => x.id === v.preset) || list[0] || null;
};
/// A link always names a preset: written, or the seed while it is only drawn.
const wielderIdOf = (v) => (wielderBuild(v) || { id: PRESET_SEED_ID }).id;

/// A stored wielder, repaired against this weapon: a frame that cannot hold it
/// falls back to the first one allowed, or to the Prototype on a weapon anyone
/// carries, and the preset it names to a real one (`wielderBuild`). A preset
/// written before links named one is the frame's first.
function defaultWielder(weaponId, v) {
  const allowed = (weaponInfo(weaponId) || {}).wielders || [];
  const holds = (id) => (META.warframes || []).some((f) => f.id === id) && (!allowed.length || allowed.includes(id));
  const frame = v && holds(v.frame) ? v.frame : (allowed.length ? allowed[0] : PROTOTYPE_ID);
  return { frame, preset: wielderIdOf({ frame, preset: v && v.frame === frame ? v.preset : null }) };
}

/// The linked build as the Warframe module reads it. ABSENT for the Prototype
/// while nothing is written for it: that is the floor every ruler scores in, and
/// the wire keeps saying it as no wielder, so no stored link or board row moves.
function wielderPayload() {
  const v = buildWielder;
  const b = wielderBuild(v);
  if (b) return wfPayloadOf({ ...b.state, frame: v.frame });
  return v.frame === PROTOTYPE_ID ? undefined : { frame: v.frame };
}

/// WHAT THE WEAPON IS CALLED IN THESE HANDS — "Valkyr Prime Talons".
const weaponDisplayName = (w) => ((w && w.wielder_names) || {})[(buildWielder || {}).frame] || (w && w.name) || "";

function renderWeaponName() {
  const w = weaponInfo($("weapon") && $("weapon").value);
  if (w && $("w-name")) $("w-name").innerHTML = wl(weaponDisplayName(w), wikiUrl(wikiWeaponName(w))) + weaponMarketLink(w);
}

function renderWielder() {
  const host = $("wielder-row");
  if (!host || !META) return;
  const w = weaponInfo($("weapon").value) || {};
  const allowed = w.wielders || [];
  const frames = (META.warframes || []).filter((f) => !allowed.length || allowed.includes(f.id));
  // WHAT KIND OF HOLDER, and nothing finer: the list is the Warframes, with their
  // faces, searchable. WHICH PRESET of it is the pane's own preset bar's — the
  // wielder only remembers it, so choosing a type reopens the one that frame
  // last had open (`wielderLinkFor`).
  const items = frames.map((f) => ({
    value: f.id, label: f.name, image: f.image,
    hint: f.id === PROTOTYPE_ID ? tr("the floor every board is scored on") : null,
  }));
  host.innerHTML = `<label>${escHtml(tr("Wielder"))} ${ddButton("dd-wielder", {
    value: buildWielder.frame, search: true, items,
    onPick: (frame) => setWielder(wielderLinkFor(frame)),
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
  const f = (META.warframes || []).find((x) => x.id === buildWielder.frame);
  // A FRAMED PAGE NEVER FRAMES: it has this markup too, and would nest itself.
  if (!box || !f || EMBED) return;
  const linked = wielderBuild(buildWielder);
  const opId = linked ? (linked.state || {}).operator : null;
  const opBuild = opId && opList().find((x) => x.id === opId);
  const src = `${warframePath(f)}?embed=1&build=${encodeURIComponent(buildWielder.preset)}`;
  const held = box.querySelector(".wld-pane");
  // AN UNCHANGED PANE IS LEFT ALONE: replacing its `src` reloads the page and
  // drops whatever is being typed in it.
  if (!held) {
    box.innerHTML = `<div class="wld-pane" data-src="${src}">
      <div class="wld-head"><b>${escHtml(f.name)}</b>
      <a class="ghost-btn small" href="${warframePath(f)}">${escHtml(tr("open the full page"))}</a></div>
      <iframe class="wld-frame" loading="lazy" src="${src}" title="${escHtml(f.name)}"></iframe></div>`;
  } else if (held.dataset.src !== src) {
    held.dataset.src = src;
    held.querySelector("iframe").src = src;
    held.querySelector(".wld-head b").textContent = f.name;
    held.querySelector(".wld-head a").href = warframePath(f);
  }
  const sub = $("wielder-sub");
  if (sub && !allowedWielders().length) {
    sub.textContent = [f.name, linked && linked.name, opBuild && `${tr("Operator")}: ${opBuild.name}`].filter(Boolean).join(" · ");
  }
}
const allowedWielders = () => (weaponInfo($("weapon").value) || {}).wielders || [];

// A BUILD OPENED OR BORN IN A PANE IS THE WIELDER'S, and what a pane writes is
// read back here: the storage event reaches this page from the framed one.
window.addEventListener("message", (e) => {
  const d = e.data;
  if (e.origin !== location.origin || !d || d.wfsim !== "wielder-build") return;
  if (!d.id || d.frame !== buildWielder.frame || d.id === buildWielder.preset) return;
  const held = document.querySelector("#wielder-detail .wld-pane");
  const f = (META.warframes || []).find((x) => x.id === d.frame);
  if (held && f) held.dataset.src = `${warframePath(f)}?embed=1&build=${encodeURIComponent(d.id)}`;
  setWielder({ frame: d.frame, preset: d.id });
});
let wielderSync = null;
window.addEventListener("storage", (e) => {
  if (EMBED || !e.key || !e.key.startsWith("wfsim-presets-")) return;
  clearTimeout(wielderSync);
  wielderSync = setTimeout(() => { if (META) { renderWielder(); refreshPanel(); } }, 200);
});

/// THE LINK FOR A FRAME: the preset that frame last had open on its own page, or
/// its first one — the seed, while it is only drawn.
function wielderLinkFor(frame) {
  const last = localStorage.getItem(presetActiveKey(WF_BUILDS, frame));
  const p = presetToOpen(presetListWithIds(WF_BUILDS, frame), null, last);
  return { frame, preset: p ? p.id : PRESET_SEED_ID };
}

/// WHO HOLDS THE WEAPON — a frame, or a frame's saved build. Set by the
/// dropdown and the door alike.
function setWielder(v) {
  buildWielder = v;
  renderWielder();
  renderWeaponName();
  markPresetDirty();
  refreshPanel();
}
/// The frames that may hold this weapon, each with its saved builds.
const wielderChoices = () => {
  const w = weaponInfo($("weapon").value) || {};
  const allowed = w.wielders || [];
  return {
    prototype_allowed: !allowed.length,
    frames: (META.warframes || []).filter((f) => !allowed.length || allowed.includes(f.id)).map((f) => ({
      id: f.id, name: f.name, builds: presetListWithIds(WF_BUILDS, f.id).map((p) => ({ id: p.id, name: p.name })),
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
/// THE SITE, for a shell whose own origin has no board yet.
///
/// The board is not part of a release, so a client that has not fetched one has
/// nothing to serve — and a shell answering same-origin cannot invent it. Asked
/// second and never first: on the web the same-origin copy always answers, and
/// on a shell the local one is the offline copy this exists to keep.
const BOARD_ORIGIN = "https://wfsim.app";

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
/// Spelled the same way `route` spells it, id first and the wiki slug second.
function routeWeaponId() {
  const m = location.pathname.match(/^\/weapons\/([^/]+?)(\/[a-z]+)?\/?$/);
  if (!m) return "";
  const slug = decodeURIComponent(m[1]).trim().toLowerCase().replace(/[\s-]+/g, "_");
  const w = (META.weapons || []).find((x) => x.id === slug)
    || (META.weapons || []).find((x) => wikiSlug(x).toLowerCase() === slug);
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
async function loadWeaponBoard(id, base = "") {
  if (!id || BOARD_HAVE.has(id)) return;
  let rows = null;
  try { rows = await fetchJsonPatient(`${base}/board/${id}.json`); } catch (_) { /* unreachable */ }
  // NULL IS NOT AN EMPTY BOARD. A dev server has no `board/` at all, and a
  // weapon left unloaded is what `boardProjection` refuses to speak about.
  if (!rows) return;
  BOARD[id] = rows;
  BOARD_HAVE.add(id);
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

/// The benchmark page: ask first, so the first draw already knows a request is
/// in flight, then draw again when it settles.
function showBenchBoard() {
  const ask = loadFullBoard();
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
async function loadBoard(weapon) {
  await loadWeaponBoard(weapon);
  BOARD_META = await fetchJson("/board.meta.json");
  if (!BOARD_META) {
    boardOffOrigin = true;
    boardFromSite(weapon);
  }
}

/// THIS ORIGIN HOLDS NO BOARD OF ITS OWN, and every later weapon asks the site
/// first because of it. `board.meta.json` not answering is the whole test: the
/// site always has one, a shell keeps one beside its release, and a shell too
/// old to know about that directory never will.
let boardOffOrigin = false;

/// The rows for `id` landed — and they are PRESETS, so this is `initPresets`
/// and not a repaint: a bar redrawn without them is a bar with no benchmark
/// builds in it, which is what the reader came for.
///
/// ONLY IF THE READER IS STILL THERE. A slow answer for a weapon they have
/// already left must not redraw the one they are looking at.
function boardRowsLanded(id) {
  if (!$("weapon") || $("weapon").value !== id) return;
  try { initPresets(); renderPresetBar(); refreshPanel(); }
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
/// decides how many of them a reader sees, and it is theirs to change. Half is
/// the default for the reasons `docs/BOARD.md` gives: the rows below it carry
/// 8 of 8 mods like the rows above and differ by taking the worse arcane or by
/// spending slots this fight cannot pay, so half marks where a build stops
/// being a DIFFERENT answer rather than where it stops being the best one.
///
/// ZERO IS "EVERYTHING THE BOARD KEPT", and it is offered because the
/// alternative is a reader who cannot tell a build that was never submitted
/// from one the page declined to draw. What it bottoms out at is the board's
/// own ENTRY LINE — a tenth of the group's leader, `data::boards`'s
/// `KEEP_LEADER_SHARE` — and the number is not repeated here: zero means "no
/// filter", which stays true wherever that line is drawn, and a copy of it
/// would be a second answer to one question the day the line moves.
const BOARD_DEPTHS = [0.5, 0.25, 0];
let boardDepth = BOARD_DEPTHS[0];
try {
  const s = Number(localStorage.getItem("wfsim-board-depth"));
  if (BOARD_DEPTHS.includes(s)) boardDepth = s;
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
  // IN THE RULERS' OWN ORDER, which puts the PRIMARY one first — the same
  // declaration the board page and the scenario bar read (`Benchmark::primary`).
  //
  // It was the published file's order, which is the scorer's, and that was
  // indistinguishable from "the primary ruler first" until a second ruler took
  // rows: a cold load then restored a GROUP-CLEAR build under a weapon page,
  // which is not the row a first-time reader is looking at.
  const order = (id) => {
    const i = benchList().findIndex((b) => b.id === id);
    return i < 0 ? 99 : i;
  };
  // …THEN BY MODE AND BY KIND, STRONGEST GROUP FIRST. Each is a control on the
  // bar and a control's order is its own answer — which way of playing this
  // weapon wins here, and whether the winner carries a riven — so a reader who
  // touches nothing lands on the board's leader. A GROUP IS RANKED BY ITS BEST
  // ROW: one huge build and fifty mediocre ones are not comparable by any
  // other statistic the board holds.
  const best = {};
  for (const r of BOARD[w.id] || []) {
    const k = `${r.benchmark}#${r.mode || "base"}`;
    const k2 = `${k}#${rowHasRiven(r) ? "r" : "p"}`;
    best[k] = Math.max(best[k] ?? -1, r.score || 0);
    best[k2] = Math.max(best[k2] ?? -1, r.score || 0);
  }
  const modeKey = (r) => `${r.benchmark}#${r.mode || "base"}`;
  const kindKey = (r) => `${modeKey(r)}#${rowHasRiven(r) ? "r" : "p"}`;
  // TIES KEEP THE GROUPS CONTIGUOUS, or a rank means nothing — the picker
  // numbers each group as it walks the list. `w.modes` is the order the
  // builder's own Mode control offers, so the tiebreak agrees with it.
  const modeOrder = (m) => {
    const i = (w.modes || []).indexOf(m || "base");
    return i < 0 ? 99 : i;
  };
  // DEEP ENOUGH TO READ, and no deeper — see `BOARD_DEPTHS`. It is drawn
  // against `best[kindKey]`, which is this row's own group: a riven build and
  // a plain one compete with each other for nothing, and one ruler's leader
  // says nothing about another's. The boundary is INCLUSIVE, so a row exactly
  // on the line is shown — a cut drawn with `>` deletes the one row a reader
  // is most likely to go looking for. A group whose leader scored ZERO is
  // never emptied: every row ties it, and a ratio has nothing to say with no
  // scale to say it on.
  //
  // FILTERING BEFORE THE RANK IS WHAT KEEPS `#1` MEANING `#1`: the list is
  // descending, so what survives is always a prefix of a group and the numbers
  // below it are the same ones the full list would give.
  const deep = boardGroupLeaders(BOARD[w.id]);
  const rows = (BOARD[w.id] || []).filter((r) => deep(r, boardDepth))
    .sort((a, b) =>
      order(a.benchmark) - order(b.benchmark)
      || (best[modeKey(b)] || 0) - (best[modeKey(a)] || 0)
      || modeOrder(a.mode) - modeOrder(b.mode)
      || (best[kindKey(b)] || 0) - (best[kindKey(a)] || 0)
      || (rowHasRiven(a) ? 1 : 0) - (rowHasRiven(b) ? 1 : 0)
      // Best first inside a group. The published rows already arrive this way;
      // stating it here is what makes `#1` the leader rather than a bet on the
      // scorer's write order.
      || (b.score || 0) - (a.score || 0));
  const rank = {};
  return rows.map((row) => {
    const mode = row.mode || "base";
    const rv = rowHasRiven(row);
    const key = `${row.benchmark}#${mode}#${rv ? "r" : "p"}`;
    rank[key] = (rank[key] || 0) + 1;
    const n = rank[key];
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
      builtin: `${row.benchmark}#${mode}#${rv ? "r" : "p"}#${n}`,
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
  const mode = row.mode || "base";
  return buildState(w.id, {
    mode,
    // A BOARD ROW HAS NO WIELDER: every ruler scores in the Prototype's hands.
    wielder: null,
    // THE RIVEN'S SLOT IS TRANSLATED BACK. A record carries the bare
    // `riven` at the riven's own position — position is the build — and on
    // this side a mod id has to name an ITEM. The definition is registered
    // rather than written: `restoreState` creates it if and when this build
    // is actually taken.
    // TEN SLOTS, because a melee row's mod list carries its STANCE with the
    // rest — appended, and told apart by looking at it. A row from a gun
    // has nine of these empty and is unchanged.
    // …AND THE EXILUS SLOT IS ITS OWN FIELD, which this has to put back in
    // it: `mods` carries the MAINS, so a row read as a flat list restores
    // eight cards where the board scored nine. The number then disagrees
    // with the row it was opened from — and on a melee that card is the
    // Tennokai one, which is most of what the row is.
    slots: (() => {
      const ids = (row.mods || []).slice();
      const si = ids.findIndex((id) => (modById(splitRank(id)[0]) || {}).stance);
      const stance = si >= 0 ? ids.splice(si, 1)[0] : null;
      const ex = row.exilus && row.exilus !== "none" ? row.exilus : null;
      const main = ids.filter((id) => id !== ex);
      const out = Array.from({ length: 10 }, () => ({ mod: null, pol: null, rank: null }));
      main.slice(0, 8).forEach((id, i) => { out[i].mod = id; });
      // A ROW WRITTEN BEFORE THE SLOT WAS RECORDED packed nine into `mods`,
      // and its ninth card is the exilus one by position — the same
      // fallback `stateFromBuild` keeps for a share link.
      out[EXILUS].mod = ex || main[8] || null;
      if (stance) out[STANCE].mod = stance;
      return out;
    })().map((s, k) => {
      const id = s.mod;
      if (id !== BOARD_RIVEN_SLOT || !row.riven) {
        const [card, rank] = splitRank(id);
        return { mod: card, pol: null, rank };
      }
      const local = RIVEN_PREFIX + boardRivenName(row.riven);
      boardRivenDefs[local] = row.riven;
      return { mod: local, pol: null, rank: null };
    }),
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
      : null,
    // THE PARTS the row was scored with, from the row's own two flat
    // fields. A modular weapon's assembly IS its stat line, so a row that
    // opened on the default parts would be a build that scores nothing
    // like the number beside it — which is exactly what a missing `mode`
    // did to every Incarnon row, and `valence` to seven Kuva Nukors.
    assembly: row.grip ? { grip: row.grip, loader: row.loader } : null,
  });
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
    label: tr("Builds"),
    noun: BUILD_NOUN,
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
    setActive: (n) => { activePreset = n; localStorage.setItem(presetActiveKey(BUILDS), n); noteBoardActive(n); },
    snapshot: snapshotState,
    // Never the payload's weapon — the scope's. See restoreState.
    // The benchmark build's Forma plan is NOT applied here — `restoreState`
    // owns it, so the boot path gets it too. See the comment there.
    apply: (st) => restoreState(st, presetWeapon()),
    blank: blankBuildState,
    pristine: notePristineBuild,
    rerender: () => { renderPresetBar(); lockOfficialBuild(); },
    opened: openedBoardBuilds,
    unpin: unpinBoardBuild,
  };
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

