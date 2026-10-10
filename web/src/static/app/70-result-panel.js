// ---- the result on screen ------------------------------------------------
// The simulator shows the ACTIVE preset's newest record (`resultLatest`) —
// switching builds switches the displayed numbers too, or clears them when
// that build was never tested.
// WHAT was measured: the build and the fight, together. A number is only
// this build's number while both are unchanged — the share card states the
// two side by side, so it has to know when the stored one stopped matching.
function simKey() {
  const st = snapshotState();
  return JSON.stringify([st.slots, st.arcane, st.arcaneRank, st.evoSel, snapshotScenario()]);
}

function saveSimResult(r, build) {
  resultViewing = null;
  const rec = resultAdd(r, build);
  // …AND IN MEMORY, which is where the REPLAY stays for good: a record has none.
  if (rec) resultMem.set(resultMemKey(), { ...rec, r });
}

// The result to PUT ON A CARD: the stored one if it still describes this
// build in this fight, else a fresh run. Reusing a stale one would be worse than having none — the card
// would attach a measurement to a build that never produced it.
async function resultForShare() {
  const rec = resultLatest(presetWeapon(), activePreset);
  if (rec && rec.r && rec.key === simKey()) return rec;
  // The PANEL first, awaited. `refreshPanel` is debounced and returns before
  // it has answered, so a share clicked seconds after an edit was composing
  // its buff map from the PREVIOUS build's `buffList` — a buff the new mod
  // grants was simply absent from the payload, and the server fell back to
  // its own default. That is the whole of "0.4 shared vs 0.56 run by hand": same seed, same build, a different buff map.
  try { renderPanel(await api("/api/panel", buildPayload())); } catch (_) { /* keep going */ }
  // THE SAME FIGHT RUN SIM RUNS. It spread a raw `sim` until 2026-08-17, which
  // sent no run count (so the server's own default, not the page's) and no
  // `custom_enemies` — so the claim on a share card for a fight against a
  // target you made was measured against a target the server had never heard
  // of.
  // THROUGH THE FLEET, like Run Sim: it is the same fight at the same run
  // count, and on a crowd the difference is a minute against a quarter of one.
  // A share card that takes a minute to appear is a share card nobody waits
  // for.
  const build = snapshotState();
  const r = await simulateFleet({ ...buildPayload(), ...theFight() });
  if (!r || r.ok === false) return null;
  saveSimResult(r, build);
  renderStoredSimResult();
  return { r, at: Date.now(), key: simKey() };
}
/// THE RESULT ON SCREEN, held in memory as well as in storage.
///
/// A RENDER CACHE and nothing else: the record is where a result persists
/// across a reload, and this is only what the current page is showing.
/// KEYED BY WEAPON AND PRESET, because switching either one asks for a
/// different result and both are legal to do without re-running.
const resultMem = new Map();
const resultMemKey = () => JSON.stringify([presetWeapon(), activePreset]);

/// WHICH RECORD THE RESULT BLOCK SHOWS when it is not the newest — an id from
/// the history, or null. It names the open build's record or it is dropped, so
/// opening another build or running again shows the newest.
let resultViewing = null;

function renderStoredSimResult() {
  const box = $("sim-results");
  if (!box) return;
  const viewing = resultViewing
    && results.find((x) => x.id === resultViewing && x.weapon === presetWeapon() && x.preset === activePreset);
  if (!viewing) resultViewing = null;
  renderResultHistory(viewing);
  if (viewing) {
    show("sim-results-block", true);
    renderResults(viewing.r, viewing.at, viewing.build);
    return;
  }
  let shown = resultLatest(presetWeapon(), activePreset);
  // WHAT IS ON SCREEN WINS over the record, and only for the build it was
  // measured on — a cache that outlived its build would attach a number to
  // something that never produced it. A RECORD HAS NO REPLAY, so memory is
  // where the median engagement, the buff curves and the hit account live for
  // the whole session: preferred when it IS that record, by id — two runs of
  // one build in one fight share a `key` and not a number.
  const mem = resultMem.get(resultMemKey());
  if (mem && shown && mem.id === shown.id && mem.r.replay) shown = mem;
  const has = !!(shown && shown.r);
  show("sim-results-block", has); // an untested build shows no Result block
  if (has) renderResults(shown.r, shown.at, shown.build);
  else box.innerHTML = "";
}

/// "3 Oct 15:26" in the page's language; a record that never said when is "—".
const resultTime = (at) => (at ? new Date(at).toLocaleString(LANG === "zh" ? "zh-CN" : "en-GB",
  { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" }) : "—");

/// THE OPEN BUILD'S HISTORY: every run kept, newest first, each opening in the
/// result block above as the page draws a result today. A row says what no
/// longer holds — the build or the fight changed since, another engine
/// measured it — and nothing a record did not record.
function renderResultHistory(viewing) {
  const box = $("sim-history"), note = $("sim-viewing");
  if (!box || !note) return;
  const recs = resultsFor(presetWeapon(), activePreset);
  note.hidden = !viewing;
  note.innerHTML = viewing
    ? `<span>${escHtml(tr("showing the run of {t}").replace("{t}", resultTime(viewing.at)))}</span>`
      + `<button type="button" class="pchip" data-rlatest>${escHtml(tr("back to the newest"))}</button>`
    : "";
  if (!recs.length) { box.innerHTML = ""; return; }
  const on = viewing ? viewing.id : recs[0].id;
  const key = simKey();
  const rows = recs.map((x) => {
    const m = metricOf((x.fight || {}).metric);
    const tags = [];
    if (x.key !== key) tags.push(tr("build or fight changed since"));
    if (!x.engine) tags.push(tr("engine not recorded"));
    else if (x.engine !== ENGINE_ID) tags.push(tr("older engine"));
    const sc = presetLabel(scenarioNamed(x.scenario));
    return `<tr class="${x.id === on ? "on" : ""}" data-rview="${escHtml(x.id)}" tabindex="0">`
      + `<td>${escHtml(resultTime(x.at))}</td>`
      + `<td>${sc ? escHtml(sc) : `<span class="rh-tag">${escHtml(tr("not recorded"))}</span>`}</td>`
      + `<td class="num"><b>${escHtml(sig2(metricValue(m, x.r)))}</b> ${escHtml(metricLabel(m))}</td>`
      + `<td>${tags.map((t) => `<span class="rh-tag">${escHtml(t)}</span>`).join(" ")}</td>`
      + `<td class="rh-act"><button type="button" class="rh-pin" data-rkeep="${escHtml(x.id)}" aria-pressed="${x.kept ? "true" : "false"}"`
      + ` title="${escHtml(tr(x.kept ? "unpin: trimmed with the rest" : "pin: never trimmed"))}">${x.kept ? "★" : "☆"}</button>`
      + `<button type="button" class="rh-del" data-rdel="${escHtml(x.id)}" title="${escHtml(tr("delete this run"))}">×</button></td></tr>`;
  }).join("");
  box.innerHTML = foldBlock("sim-history", tr("History"),
    tr("every run of this build, newest first; the newest {n} per scenario are kept, and every pinned one")
      .replace("{n}", RESULT_KEEP),
    `<div class="rh-wrap"><table class="rh"><tbody>${rows}</tbody></table></div>`);
  wireFolds(box);
}

// One listener each, on containers that outlive every render.
document.addEventListener("click", (e) => {
  const t = e.target;
  if (!t || !t.closest) return;
  if (t.closest("#sim-viewing [data-rlatest]")) {
    resultViewing = null;
    renderStoredSimResult();
    return;
  }
  if (!t.closest("#sim-history")) return;
  const keep = t.closest("[data-rkeep]"), del = t.closest("[data-rdel]"), row = t.closest("[data-rview]");
  if (keep) {
    const rec = results.find((x) => x.id === keep.dataset.rkeep);
    if (rec) resultKeep(rec.id, !rec.kept);
    renderResultHistory(resultViewing && results.find((x) => x.id === resultViewing));
  } else if (del) {
    resultDelete(del.dataset.rdel);
    if (resultViewing === del.dataset.rdel) resultViewing = null;
    renderStoredSimResult();
  } else if (row) {
    const newest = resultLatest(presetWeapon(), activePreset);
    resultViewing = newest && newest.id === row.dataset.rview ? null : row.dataset.rview;
    renderStoredSimResult();
  }
});
document.addEventListener("keydown", (e) => {
  const row = e.target && e.target.closest && e.target.closest("#sim-history [data-rview]");
  if (row && !imeComposing(e) && (e.key === "Enter" || e.key === " ")) { e.preventDefault(); row.click(); }
});

// ---- REPLAY -------------------------------------------------------------
//
// The MEDIAN engagement, played back. Not a new simulation and not an average:
// the engine re-ran that one run from the RNG state it recorded, so this is
// the fight the headline number came from, frame by frame.
//
// One row per buff, each a short curve of LIVE STACKS over time, all open by
// default — the question they answer is "was this thing actually up", and a
// row that has to be clicked to answer it will not be. `mean` and `uptime`
// sit in the header so the group reads without expanding anything at all.
const REPLAY_SPEEDS = [1, 2, 5, 20];
// An UNCAPPED buff has no maximum to draw against, so the curve scales to the
// highest it actually reached and the readout says so.
// NO CEILING IS EITHER SPELLING: the buff roster has always said it with a
// max of 0 (`FightParams::buff_roster`: "0 means no ceiling, which the api
// and the UI both read as such") and the debuff roster now says it the same
// way. Reading only the flag printed a bare "0" for every uncapped BUFF and
// scaled its chart to 1 — an uncapped row drawn as though it were full from
// the first stack.
// ONE SEAT'S KPI SERIES. `kpi` is per seat, in `combatants`' order, because a
// rate is a seat's or it is nobody's — a crit rate over two different weapons
// divides one build's crits by another build's pellets.
//
// `dps` IS THE ONE EXCEPTION and stays the fight's: damage is the fight's
// total, and the per-seat cut of it is `dealt`, which the meter already draws.
const kpiOf = (rp, key) => {
  const s = ((rp && rp.kpi) || {})[key];
  if (!s) return [];
  return key === "dps" ? s : (s[replaySeatIdx(rp)] || []);
};
const rpUncapped = (b) => !!b.uncapped || Number(b.max) === 0;
// A ROW IS A COUNT OR IT IS A NUMBER, and the roster says which. Almost every buff is capped by a stack count and DE publishes
// that count, so `3/4` is the honest reading. A few publish the NUMBER the pile
// stops at and let the counter run — Hata-Satya's "capped at 500% at all mod
// ranks" — and for those a stack count is a chart of the wrong quantity: it
// climbs past a ceiling nobody printed. Such a row carries `value` from the
// engine ({per, max, unit}) and everything below reads the VALUE: the curve,
// the average, the live readout and the ramp.
//
// `unit: "%"` means the engine's numbers are fractions and the reader
// multiplies by 100 — the same convention `build::loadout::pct` uses on the Rust side.
// The engine states the quantity; this states the presentation.
const rpValueOf = (b, stacks) => {
  const v = b && b.value;
  if (!v) return Number(stacks);
  return Math.min(Number(stacks) * v.per, v.max);
};
// A point on the row, already in the row's own quantity.
const rpFmtVal = (b, n) => {
  const v = b && b.value;
  if (!v) return String(n);
  return v.unit === "%" ? `${(n * 100).toFixed(1)}%` : `${n.toFixed(2)}${v.unit}`;
};
// …and the same from a raw stack count, which is what the frames carry.
const rpFmt = (b, stacks) => (b && b.value ? rpFmtVal(b, rpValueOf(b, stacks)) : String(stacks));
// The number the row stops at: the published one where there is one, the stack
// ceiling otherwise, and NaN-free where there is neither.
const rpCeil = (b) => (b && b.value ? b.value.max : Number(b.max));
// The ceiling as the row draws it: a stack count, a published number, or ∞.
const rpCap = (b) => {
  if (b && b.value) return b.value.unit === "%"
    ? `${(b.value.max * 100).toFixed(0)}%`
    : `${b.value.max}${b.value.unit}`;
  return rpUncapped(b) ? "∞" : b.max;
};
/// A BUFF SERIES' NAME, from the CARDS — same ids, so the chart, the record's
/// "buffs up" column and the card a reader clicks can never disagree about what
/// a series is called.
const buffRosterName = (id) => {
  const b = (buffList || []).find((x) => x.id === id);
  return b ? buffCardName(b.name) : id;
};

/// A DEBUFF ROW IS NAMED BY ITS DAMAGE TYPE, which is already translated
/// everywhere else on the page (`DT`). The alternative was a new i18n family
/// for the proc names — Virus, Corrosion, Disrupt — and DE's Chinese for those
/// is not something to invent: a string is transcribed, never translated
/// (AGENTS.md). The damage type says the same thing in words the reader has
/// already seen on the damage meter, and it is 1:1 with the proc everywhere
/// except Cold, whose two states are told apart by a suffix of our own.
const DEBUFF_TYPE = {
  virus: "viral", corrosion: "corrosive", disrupt: "magnetic",
  confusion: "radiation", blast: "blast", freeze: "cold", frozen: "cold",
  stagger: "impact", weakened: "puncture", attractor: "void",
  bleed: "slash", poison: "toxin", ignite: "heat",
};
// MICROWAVE is not a damage type, so it has no `DT` name to borrow — it is
// its own thing and DE named it, so it keeps that name rather than being
// translated into a description of what it does, which is exactly what the
// transcribe-never-translate rule forbids.
const DB_OWN_NAME = { microwave: "Microwave", lifted: "Lifted" };
const debuffRosterName = (id) =>
  (DB_OWN_NAME[id] || DT(DEBUFF_TYPE[id] || id)) +
  (id === "frozen" ? ` (${tr("frozen")})` : "");

let replayState = null; // { data, i, playing, speed, raf }

// ---- EVERY BLOCK FOLDS -------------------------------------------------
//
// The result panel grew from one number to nine blocks, and not every reader
// wants all nine every time. So a block is a heading you
// can click, and it REMEMBERS — per block, across runs and reloads, because a
// panel that re-opens everything on every Run Sim is a panel you have to
// re-close on every Run Sim.
//
// The state lives outside the markup, which is what lets `renderResults`
// rebuild the whole panel without losing what you folded. It is keyed by fold
// id ALONE — not by weapon — so a box you shut on one weapon is shut on the
// next: what a reader wants to look at is a habit, not a property of a gun.
let foldState = {};
try { foldState = JSON.parse(localStorage.getItem("wfsim-folds")) || {}; } catch (_) {}
const saveFolds = () => localStorage.setItem("wfsim-folds", JSON.stringify(foldState));

/// SHUT OR OPEN, and what it is UNTIL the reader says. A box that ships shut
/// carries `shut` in `index.html`; the stored answer outranks it forever after,
/// including the answer "open", which no default may undo.
const folded = (id, byDefault) => (foldState[id] == null ? !!byDefault : foldState[id] === true);

/// ONE ZONE OF THE RESULT, and the result is nothing but zones.
///
/// A zone answers ONE question and carries the question in its own heading, so
/// a reader can tell what a block is for before reading a number in it, and so
/// anything added later has one test to pass: which question does it answer?
/// A block that answers none of them does not belong on this panel.
///
/// Zones do not fold. They are the skeleton — the folds are inside them — and
/// a skeleton that can be collapsed away is a page with no shape.
function zone(n, title, question, body) {
  return `<section class="zone" data-zone="${n}">
    <h2 class="zone-h"><span class="zone-n">${n}</span><span class="zone-t">${escHtml(title)}</span>${
      question ? `<span class="zone-q">${escHtml(question)}</span>` : ""}</h2>
    <div class="zone-b">${body}</div>
  </section>`;
}

/// One collapsible block: a heading, an optional hint, and a body.
function foldBlock(id, title, hint, body) {
  const shut = folded(id);
  return `<div class="fold${shut ? " shut" : ""}" data-fold="${escHtml(id)}">
    <h3 class="fold-h"><span class="fold-c">▾</span>${escHtml(title)}${
      hint ? ` <span class="sim-hint">${escHtml(hint)}</span>` : ""}</h3>
    <div class="fold-b">${body}</div>
  </div>`;
}

/// SHUT OR OPEN ONE FOLD, whatever shape it has. A `.block` keeps its state on
/// the section and its caret in the `.bh` it already had; a `.fold` keeps both
/// on itself. One function, so the two wiring passes, the jump menu and
/// "collapse all" cannot disagree about what shut means.
function setFold(box, shut) {
  box.classList.toggle("shut", shut);
  foldState[box.dataset.fold] = shut;
  saveFolds();
}

const foldCaret = () => {
  const c = document.createElement("span");
  c.className = "fold-c";
  c.textContent = "▾";
  return c;
};

/// A CLICK ON A CONTROL IN THE HEADING IS THAT CONTROL'S. The Buffs section's
/// "all", the mods axis's filter box, the Forma buttons in `.bh`: a heading is
/// allowed to carry them, and folding the thing they belong to instead is the
/// one way this feature can make the page worse. A search box is one control,
/// its tokens and suggestions included (the build finder's).
const foldsOnClick = (e) => !e.target.closest("button,input,select,a,label,[role=search]");

/// Wire every fold on the page. Called once per render, and delegated from the
/// heading rather than the whole block, so a click inside the body — a scrub
/// bar, a mod row — never folds the thing it is inside.
///
/// The caret and the saved state are applied HERE rather than written into the
/// markup: `.fold.sect` is authored in `index.html`, where neither is knowable.
function wireFolds(root) {
  (root || document).querySelectorAll(".fold > .fold-h").forEach((h) => {
    const box = h.parentElement;
    if (!h.querySelector(":scope > .fold-c")) h.insertBefore(foldCaret(), h.firstChild);
    box.classList.toggle("shut", folded(box.dataset.fold, box.classList.contains("shut")));
    h.onclick = (e) => {
      if (!foldsOnClick(e)) return;
      setFold(box, !box.classList.contains("shut"));
      renderJump();
    };
  });
}

/// A `.block` FOLDS FROM THE HEADER IT ALREADY HAS, its own id its fold id — a
/// block that is renamed or added needs nothing said here or in `index.html`.
/// Wiring one twice is a no-op, so a page that redraws (`/compute`) calls it
/// after every draw.
function wireBlockFolds(blocks) {
  blocks.forEach((b) => {
    const h = b.querySelector(":scope > .bh");
    if (!h || !b.id || b.dataset.fold === b.id) return;
    b.dataset.fold = b.id;
    h.insertBefore(foldCaret(), h.firstChild);
    b.classList.toggle("shut", folded(b.id, b.classList.contains("shut")));
    h.addEventListener("click", (e) => {
      if (!foldsOnClick(e)) return;
      setFold(b, !b.classList.contains("shut"));
      renderJump();
    });
  });
}

/// THE PAGE'S OWN BLOCKS FOLD TOO. They are in the markup and outlive every
/// render, so this runs once at boot; the result panel's `foldBlock`s are
/// rebuilt on each render and rewired there.
function wireStaticFolds() {
  wireBlockFolds(document.querySelectorAll(".config-page section.block"));
  wireFolds();
}

// ---- THE JUMP MENU -----------------------------------------------------
//
// Every section on a weapon page folds, which makes "where is the one I want"
// the question the page suddenly answers worst: a shut section is one line, and
// twenty of them are twenty lines that all look alike. This is the page's own
// index — the module's blocks, the sections inside each, and the other tabs —
// and a row OPENS what it jumps to.
//
// IT DRAGS, because no corner is free on every window, and it remembers where
// it was put. The position is clamped back into the viewport on every resize:
// a spot chosen on a wide monitor is off-screen on a laptop, and a menu nobody
// can reach is worse than no menu.
const JUMP_KEY = "wfsim-jump";
/// AN ANCHOR, NOT A CORNER: `ax` is the LEFT edge when `edge` is "l" and the
/// RIGHT edge when it is "r", chosen by the half of the window it was dropped in,
/// so opening grows the panel away from the side it is parked on. Null is the
/// default spot — the right edge, 30% down. A stored position without `edge` is
/// from the old menu, whose default was the corner that covered the topbar.
let jump = { ax: null, y: null, edge: "r", open: false };
try {
  const s = JSON.parse(localStorage.getItem(JUMP_KEY)) || {};
  jump = { ...jump, open: !!s.open, ...(s.edge ? { ax: s.ax ?? null, y: s.y ?? null, edge: s.edge } : {}) };
} catch (_) {}
const saveJump = () => localStorage.setItem(JUMP_KEY, JSON.stringify(jump));

/// EVERY FOLD ON THE PAGE, in the order the reader meets them: this module's
/// blocks, and the sections inside each. Read off the DOM, so a section added
/// tomorrow is in the menu — and in "collapse all" — with no edit here.
///
/// A SHUT BLOCK STILL LISTS ITS SECTIONS. `offsetParent` answers "is this the
/// module on screen" for a block; asked of a section it would answer "is the
/// block above it open", which would make the menu change shape as you fold and
/// leave "expand all" with sections it never reached.
///
/// A FOLD BESIDE THE BLOCKS (the build finder) is a row of its own, in page order.
function pageFolds() {
  const out = [];
  document.querySelectorAll(".config-page section.block, .config-page > section.fold").forEach((b) => {
    // THE MENU LISTS WHAT FOLDS, and a box with no fold id does not: a block
    // with no header, or one drawn after `wireStaticFolds` ran at boot. Listing
    // it wrote `undefined` into the menu and took the whole page down with it.
    if (b.offsetParent === null || !b.dataset.fold) return;
    if (!b.classList.contains("block")) return out.push({ box: b, depth: 0, name: foldTitle(b) });
    const n = b.querySelector(".bh .n"), h2 = b.querySelector(".bh h2");
    out.push({ box: b, depth: 0,
      name: [n && n.textContent.trim(), h2 && h2.textContent.trim()].filter(Boolean).join(" · ") });
    b.querySelectorAll(".fold.sect").forEach((s) => {
      // `hidden` is how an axis a weapon does not have is taken off the page.
      if (s.closest("[hidden]")) return;
      let depth = 0;
      for (let p = s.parentElement; p && p !== b; p = p.parentElement) {
        if (p.classList.contains("sect")) depth += 1;
      }
      out.push({ box: s, depth: depth + 1, name: foldTitle(s) });
    });
  });
  return out;
}

/// WHAT A SECTION IS CALLED, with everything that is not its name removed: the
/// hint, the caret, the controls, and the sub-label the Warframe buffs carry.
/// The optimizer's axes keep their name in an `.axh` stamped from the builder's
/// own block, which is text like any other once it is there. A heading strip
/// that is mostly controls (the build finder's) is named by the heading in it.
function foldTitle(box) {
  const strip = box.querySelector(":scope > .fold-h");
  if (!strip) return box.dataset.fold || "";
  const h = strip.querySelector("h2,h3") || strip;
  const c = h.cloneNode(true);
  c.querySelectorAll(".sim-hint,.sim-h-sub,.fold-c,button").forEach((x) => x.remove());
  return c.textContent.replace(/\s+/g, " ").trim();
}

/// The module whose blocks are on screen — the same fact `route()` states in a
/// body class, read back rather than parsed out of the path a second time.
const jumpMod = () => ["simulator", "optimizer", "rivens", "enemies"]
  .find((m) => document.body.classList.contains("on-" + m)) || "";

const JUMP_TABS = [["", "Builder"], ["simulator", "Simulator"], ["optimizer", "Optimizer"],
                   ["rivens", "Rivens"], ["enemies", "Enemies"]];

function jumpRows() {
  const here = jumpMod();
  const base = weaponPath($("weapon").value);
  // A WARFRAME OR OPERATOR PAGE HAS NO TABS, so it has no row of them.
  let h = ["on-warframe", "on-operator"].some((c) => document.body.classList.contains(c)) ? "" : `<div class="jump-mods">` + JUMP_TABS.map(([m, label]) =>
    `<a class="jump-mod${m === here ? " sel" : ""}" href="${base}${m ? "/" + m : ""}">${
      escHtml(tr(label))}</a>`).join("") + `</div>`;
  h += pageFolds().map((f) => {
    const shut = f.box.classList.contains("shut");
    return `<button class="jump-row ${f.depth ? "sub" : "blk"}${f.depth > 1 ? " deep" : ""}${
      shut ? " is-shut" : ""}" data-jump="${escHtml(f.box.dataset.fold)}">`
      + `<span class="jr-n">${escHtml(f.name)}</span>`
      + `<span class="jr-c" data-jump-fold="${escHtml(f.box.dataset.fold)}" title="${
        escHtml(tr(shut ? "expand" : "collapse"))}">${shut ? "▸" : "▾"}</span></button>`;
  }).join("");
  return h + `<div class="jump-foot">`
    + `<button class="ghost-btn small" data-jump-all="shut">${escHtml(tr("Collapse all"))}</button>`
    + `<button class="ghost-btn small" data-jump-all="open">${escHtml(tr("Expand all"))}</button>`
    + `</div>`;
}

/// Where the menu sits, clamped into the window it is being drawn in.
///
/// THE CLAMP IS APPLIED TO THE DRAWING, NOT TO THE WISH. Writing it back means
/// a window narrowed once and widened again leaves the menu in the corner it
/// was squeezed into — the reader's chosen spot destroyed by a resize they
/// have already undone.
/// NEVER OVER THE TOPBAR: its bottom edge is the ceiling, because the bar is
/// sticky and a menu parked on it hides the controls the reader came up for.
function jumpCeiling() {
  const bar = document.querySelector(".topbar");
  return (bar ? Math.max(0, bar.getBoundingClientRect().bottom) : 0) + 8;
}
function placeJump() {
  const el = $("jump");
  if (!el || el.hidden) return;
  const w = el.offsetWidth, h = el.offsetHeight, top = jumpCeiling();
  const ax = jump.ax == null ? window.innerWidth - 12 : jump.ax;
  const left = jump.ax == null || jump.edge === "r" ? ax - w : ax;
  const y = jump.y == null ? Math.max(top, Math.round(window.innerHeight * 0.3)) : jump.y;
  el.style.left = `${Math.min(Math.max(8, left), Math.max(8, window.innerWidth - w - 8))}px`;
  el.style.top = `${Math.min(Math.max(top, y), Math.max(top, window.innerHeight - h - 8))}px`;
}

/// WHERE THE READER IS: the deepest fold spanning the line just under the
/// topbar, else the last one scrolled past. A section inside a shut block has
/// no height and is skipped, so the answer is always something on screen.
function jumpHere() {
  const line = jumpCeiling() + 32;
  const folds = pageFolds();
  const past = folds.map((f) => ({ f, r: f.box.getBoundingClientRect() }))
    .filter((x) => x.r.height && x.r.top <= line);
  const span = past.filter((x) => x.r.bottom > line);
  const pick = span.length ? span.reduce((a, x) => (x.f.depth >= a.f.depth ? x : a)) : past[past.length - 1];
  return pick ? pick.f : folds[0] || null;
}

/// The grip names it and the open menu marks its row, so the menu says
/// something while it is shut.
function markJumpHere() {
  const f = jumpHere();
  const id = f ? f.box.dataset.fold : "";
  document.querySelectorAll("#jump-body .jump-row").forEach((b) => b.classList.toggle("here", b.dataset.jump === id));
  const now = $("jump-now");
  if (now && now.textContent !== (f ? f.name : "")) {
    now.textContent = f ? f.name : "";
    placeJump();                    // a right-anchored pill grows leftwards
  }
}

/// Drawn on every route and after every fold, because the carets it shows are
/// the page's state and a menu that reports the wrong one is worse than none.
/// The rows are built only while it is OPEN — shut, it is one button.
function renderJump() {
  const el = $("jump");
  if (!el) return;
  const on = [...document.querySelectorAll(".config-page")].some((p) => !p.hidden);
  el.hidden = !on;
  if (!on) return;
  el.classList.toggle("open", !!jump.open);
  if (jump.open) $("jump-body").innerHTML = jumpRows();
  placeJump();
  markJumpHere();
}

/// A JUMP MOVES THE PAGE AND FOLDS NOTHING OF ITS OWN: the row's caret is the
/// fold control. Only what the target is INSIDE is opened, because a section in
/// a shut block has no position and the jump would not move at all.
function jumpTo(id) {
  const box = document.querySelector(`[data-fold="${CSS.escape(id)}"]`);
  if (!box) return;
  for (let el = box.parentElement && box.parentElement.closest("[data-fold]"); el;
       el = el.parentElement && el.parentElement.closest("[data-fold]")) {
    if (el.classList.contains("shut")) setFold(el, false);
  }
  renderJump();
  // UNDER THE STICKY TOPBAR, not behind it: the heading is the thing being
  // jumped to, and a bar covering it is a jump that missed.
  const bar = document.querySelector(".topbar");
  const top = box.getBoundingClientRect().top + window.scrollY - ((bar ? bar.offsetHeight : 0) + 10);
  window.scrollTo({ top: Math.max(0, top), behavior: "smooth" });
}

/// The whole page at once — the reason the menu earns its space. Every section
/// shut is one screen, and the one you want is then a click, not a scroll.
function foldAll(shut) {
  pageFolds().forEach((f) => setFold(f.box, shut));
  renderJump();
}

/// Wired once. The grip is BOTH the handle and the switch: a pointer that never
/// moved is a click, which is how a one-button panel can also be dragged.
///
/// THE DRAG IS TRACKED ON THE WINDOW, not on the grip. A pointer that leaves a
/// 30px button — which is every drag — stops sending the button events, and
/// `setPointerCapture` is not the fix: it THROWS on a pointer the browser does
/// not consider active, and an uncaught throw here puts up the boot-failure
/// notice on a page that booted perfectly well.
function wireJump() {
  const el = $("jump"), grip = $("jump-grip"), head = $("jump-head");
  if (!el || !grip || !head) return;
  let from = null;
  const onMove = (e) => {
    if (!from) return;
    const dx = e.clientX - from.x, dy = e.clientY - from.y;
    if (!from.moved && Math.abs(dx) + Math.abs(dy) < 4) return;
    from.moved = true;
    jump.edge = "l";
    jump.ax = from.ox + dx;
    jump.y = from.oy + dy;
    placeJump();
  };
  const onUp = () => {
    window.removeEventListener("pointermove", onMove);
    el.classList.remove("dragging");
    if (from && from.moved) {
      // RE-ANCHORED ON THE SIDE IT WAS DROPPED ON — see `jump`.
      if (jump.ax + el.offsetWidth / 2 > window.innerWidth / 2) {
        jump.edge = "r";
        jump.ax += el.offsetWidth;
      }
    } else if (from && from.toggles) {
      jump.open = !jump.open;
    }
    from = null;
    saveJump();
    renderJump();
  };
  // THE GRIP AND THE HEADER ARE BOTH HANDLE AND SWITCH; the close button in the
  // header keeps its own click.
  const start = (toggles) => (e) => {
    if (e.button !== 0 || e.target.closest(".jump-x")) return;
    e.preventDefault();             // no text selection while dragging
    placeJump();                    // so a drag starts from a real position
    // FROM WHERE IT IS DRAWN, not from where it was asked to be: the two differ
    // whenever the clamp above is doing anything, and a drag that starts from
    // the wish jumps under the pointer by exactly that much.
    from = { x: e.clientX, y: e.clientY, moved: false, toggles,
      ox: parseFloat(el.style.left) || 0, oy: parseFloat(el.style.top) || 0 };
    el.classList.add("dragging");
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp, { once: true });
  };
  grip.addEventListener("pointerdown", start(true));
  head.addEventListener("pointerdown", start(true));
  // A KEYBOARD CLICK has no pointer behind it (`detail` 0); a pointer's click
  // was already answered on pointerup.
  grip.addEventListener("click", (e) => {
    if (e.detail !== 0) return;
    jump.open = !jump.open;
    saveJump();
    renderJump();
  });
  $("jump-x").addEventListener("click", () => {
    jump.open = false;
    saveJump();
    renderJump();
  });
  let spy = 0;
  window.addEventListener("scroll", () => {
    if (spy) return;
    spy = requestAnimationFrame(() => { spy = 0; if (!el.hidden) markJumpHere(); });
  }, { passive: true });
  $("jump-body").addEventListener("click", (e) => {
    const all = e.target.closest("[data-jump-all]");
    if (all) return foldAll(all.dataset.jumpAll === "shut");
    const caret = e.target.closest("[data-jump-fold]");
    if (caret) {
      const box = document.querySelector(`[data-fold="${CSS.escape(caret.dataset.jumpFold)}"]`);
      if (box) setFold(box, !box.classList.contains("shut"));
      return renderJump();
    }
    const row = e.target.closest("[data-jump]");
    if (row) jumpTo(row.dataset.jump);
  });
  window.addEventListener("resize", placeJump);
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

// ---- WHAT THE COMBAT RECORD REPLACES ----------------------------------
//
// "Every hit, sorted" and "The account of one hit" are both gone: the COMBAT
// RECORD below answers what they did. The account explained
// two instances of an engagement; the record explains every one. The histogram
// was the aggregate that existed because the individual hits were not
// available — it sorted them into six buckets so an impossible number could not
// hide in a mean — and once every hit is a row with its own ledger, six means
// are a summary of something the reader can now simply read.
//
// The engine lost `HitAccount` and the two `[[_; 3]; 2]` counters with them, so
// nothing is being computed for a panel that no longer draws it.

