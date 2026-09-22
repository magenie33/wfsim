// ---- THE BOARD BUILDS IN THE BUILD BAR ------------------------------------
//
// EVERY BUILD IS IN THE BUILD BAR: your own, and every board build you opened,
// read-only. The bar is the one place that says which build is open.
//
// KEPT BY WHAT THE BUILD IS, NEVER BY ITS RANK. A board build's `builtin` id
// ends in its rank, and a rescore renumbers the board — a remembered "#3" would
// reopen somebody else's build. So an entry is the build's identity
// (`boardRowIdentity`) plus the cell it was ranked in; its rank and score are
// read fresh, and a build that has left the board stops resolving. Nothing is
// pruned on a miss: a board that has not loaded yet misses everything.
const LOCK_SVG = '<svg class="plock" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.4" aria-hidden="true"><rect x="2.2" y="5.2" width="7.6" height="5.3" rx="1.2"/><path d="M4 5.2V3.8a2 2 0 0 1 4 0v1.4"/></svg>';
const openedBoardKey = (w) => `wfsim-opened-board-${w}`;
const openedRefs = (w) => {
  try { return JSON.parse(localStorage.getItem(openedBoardKey(w)) || "[]"); } catch (_) { return []; }
};
const storeOpenedRefs = (w, refs) => {
  try { localStorage.setItem(openedBoardKey(w), JSON.stringify(refs)); } catch (_) { /* a private window keeps none */ }
};
const boardRef = (p) => ({ b: p.benchmark, m: p.mode, k: p.riven ? "r" : "p", id: boardRowIdentity(p.board || {}) });
const sameBoardRef = (a, b) => a.b === b.b && a.m === b.m && a.k === b.k && a.id === b.id;
const rememberBoardBuild = (p) => {
  const w = presetWeapon();
  const refs = openedRefs(w);
  const ref = boardRef(p);
  if (!refs.some((r) => sameBoardRef(r, ref))) storeOpenedRefs(w, refs.concat([ref]));
};
/// …AND THE OPEN ONE BY WHAT IT IS. The active pointer holds a builtin id, which
/// is a rank, so opening a board build also records its ref and boot resolves
/// the ref (`resolveBoardActive`). Both do nothing while the weapon's board is
/// not in hand — a board that has not loaded would otherwise read as one the
/// build has left.
const boardActiveKey = (w) => `wfsim-opened-board-active-${w}`;
function noteBoardActive(id) {
  const w = presetWeapon();
  if (!BOARD_HAVE.has(w)) return;
  const p = builtinBuilds().find((x) => presetId(x) === id);
  try {
    if (p) localStorage.setItem(boardActiveKey(w), JSON.stringify(boardRef(p)));
    else localStorage.removeItem(boardActiveKey(w));
  } catch (_) { /* a private window keeps none */ }
}
function resolveBoardActive(last) {
  const w = presetWeapon();
  if (!BOARD_HAVE.has(w)) return last;
  let ref = null;
  try { ref = JSON.parse(localStorage.getItem(boardActiveKey(w)) || "null"); } catch (_) { ref = null; }
  if (!ref || loadPresetList(BUILDS).some((p) => p.name === last)) return last;
  const p = builtinBuilds().find((x) => sameBoardRef(boardRef(x), ref));
  return p ? presetId(p) : "";
}
/// The board builds in the bar, in the order they were opened. The OPEN one is
/// always among them, however it was opened — the finder, a board link, or a
/// cold load restoring it.
function openedBoardBuilds() {
  const all = builtinBuilds();
  const act = all.find((p) => presetId(p) === activePreset);
  if (act) rememberBoardBuild(act);
  const out = [];
  for (const ref of openedRefs(presetWeapon())) {
    const p = all.find((x) => sameBoardRef(boardRef(x), ref));
    if (p && !out.includes(p)) out.push(p);
  }
  return out;
}
/// × ON A BOARD BUILD: out of the bar, and off the page if it was the one open —
/// onto your first build, or the blank one if you own none, which is exactly
/// what deleting your last build leaves.
function unpinBoardBuild(id) {
  const cfg = buildBarCfg();
  const p = builtinBuilds().find((x) => presetId(x) === id);
  if (p) {
    const ref = boardRef(p);
    storeOpenedRefs(presetWeapon(), openedRefs(presetWeapon()).filter((r) => !sameBoardRef(r, ref)));
  }
  if (id === activePreset) {
    const own = cfg.load().filter((x) => !x.builtin);
    cfg.setActive(own.length ? own[0].name : "");
    whileApplying(() => cfg.apply(own.length ? own[0].state : cfg.blank()));
    if (!own.length && cfg.pristine) cfg.pristine();
  }
  cfg.rerender();
}

// ---- THE BUILD FINDER -------------------------------------------------------
//
// THE BOARD'S BUILDS AS A TABLE: scoped by ruler, mode and riven, filtered by
// what a build contains, sorted, compared with its group's #1. It holds a QUERY
// and never a selection — which build is open is the build bar's to say, and
// "Open" does one thing: puts that build in the bar and makes it current.
// Builder only (style.css). docs/UI.md §"The build finder".
/// FIVE ROWS UNTIL ASKED: the top of a scope is what most readers came for, and
/// the rest is one click away rather than a page of scrolling.
const FINDER_FIRST = 5;
const FINDER_STEP = 20;
const finder = {
  weapon: null, b: null, mo: null, rv: "all",
  req: new Set(), exc: new Set(),
  view: "list", open: null, shown: FINDER_FIRST, sort: "score", dir: -1, cmp: true, hi: 0,
};

/// A RULER'S NAME, SHORT: its first clause and the unit off its last one.
/// "Standard Single Target · Thrax Centurion Lv 9999 SP · 180 s · KPM" ->
/// "Standard Single Target · Thrax Centurion", and "KPM". The whole name rides
/// the tooltip.
const rulerShort = (name) => {
  const parts = String(name || "").split(" · ");
  const second = (parts[1] || "").replace(/\s*(Lv\s*)?\d.*$/, "").trim();
  return second ? `${parts[0]} · ${second}` : parts[0];
};
const rulerUnit = (name) => {
  const last = String(name || "").split(" · ").pop();
  return last && last.length <= 6 ? last : "";
};

/// EVERYTHING A BUILD CONTAINS, as tokens a query can require or refuse. Ids,
/// not names: a name is a translation and two languages must find one build.
function finderTokens(p) {
  const r = p.board || {};
  const out = [];
  for (const id of r.mods || []) if (id && id !== BOARD_RIVEN_SLOT) out.push("mod:" + id);
  if (r.exilus && r.exilus !== "none") out.push("mod:" + r.exilus);
  for (const id of r.arcanes || []) if (id && id !== "none") out.push("arc:" + id);
  for (const id of r.evolutions || []) if (id) out.push("evo:" + id);
  if (r.riven) {
    for (const s of r.riven.bonuses || []) out.push("rv+:" + s);
    if (r.riven.malus) out.push("rv-:" + r.riven.malus);
  }
  if (r.grip) out.push("part:" + r.grip, "part:" + r.loader);
  if (r.valence) out.push("val:" + r.valence);
  return out;
}
function finderTokenName(t) {
  const i = t.indexOf(":");
  const kind = t.slice(0, i), id = t.slice(i + 1);
  if (kind === "mod") {
    const [card, r] = splitRank(id);
    return ((modById(card) || {}).name || prettify(card)) + (r != null ? ` R${r}` : "");
  }
  if (kind === "arc") return arcName(id);
  if (kind === "evo") return evoName(id);
  if (kind === "rv+" || kind === "rv-") {
    const s = rivenStat(id);
    return (kind === "rv+" ? "+" : "−") + (s ? rivenStatName(s) : prettify(id));
  }
  if (kind === "val") return tr(prettify(id));
  return prettify(id);
}
const finderTokenKind = (t) => ({ mod: "mod", arc: tr("Arcane"), evo: tr("Evolution"), "rv+": tr("Riven"), "rv-": tr("Riven"), part: tr("Parts"), val: tr("Element") })[t.slice(0, t.indexOf(":"))] || "";

function renderBuildFinder() {
  const box = $("build-finder");
  if (!box) return;
  const w = weaponInfo(presetWeapon()) || {};
  const all = builtinBuilds();
  if (finder.weapon !== w.id) {
    // A WEAPON IS ITS OWN QUESTION: what you were looking for on the last one
    // means nothing here. The scope starts where the open build is, else on the
    // board's leader, which is where the rows arrive first.
    const act = all.find((p) => presetId(p) === activePreset) || all[0];
    Object.assign(finder, { weapon: w.id, b: act ? act.benchmark : null, mo: act ? act.mode : null,
      rv: "all", req: new Set(), exc: new Set(), open: null, shown: FINDER_FIRST, hi: 0 });
  }
  box.hidden = false;
  // THE HEAD IS THE FOLD'S HEADING (`wireFolds`), redrawn with every render, so
  // it is rewired after each one; shut, the finder is its title line.
  const title = `<h2 class="fd-title">${escHtml(tr("Build finder"))}</h2>`;
  if (!all.length) {
    box.innerHTML = `<div class="fd-head fold-h">${title}</div>` +
      `<div class="fd-empty">${escHtml(tr("Nobody has submitted a build for this weapon yet"))}</div>`;
    wireFolds(box);
    return;
  }
  // A SCOPE THE BOARD NO LONGER HOLDS falls back to what it does hold.
  if (!all.some((p) => p.benchmark === finder.b)) finder.b = all[0].benchmark;
  if (!all.some((p) => p.benchmark === finder.b && p.mode === finder.mo)) {
    finder.mo = all.find((p) => p.benchmark === finder.b).mode;
  }
  const inScope = (p) => p.benchmark === finder.b && p.mode === finder.mo
    && (finder.rv === "all" || (finder.rv === "riven") === !!p.riven);
  const toks = new Map(all.map((p) => [p, finderTokens(p)]));
  const passes = (p) => [...finder.req].every((t) => toks.get(p).includes(t))
    && ![...finder.exc].some((t) => toks.get(p).includes(t));
  const score = (p) => (p.board || {}).score || 0;
  const list = all.filter((p) => inScope(p) && passes(p)).sort((a, b) =>
    finder.sort === "rank"
      ? finder.dir * (a.rank - b.rank) || score(b) - score(a)
      : finder.dir * (score(a) - score(b)));
  const scopeTotal = all.filter((p) => p.benchmark === finder.b && p.mode === finder.mo).length;
  const leaderOf = (p) => all.find((x) => x.benchmark === p.benchmark && x.mode === p.mode
    && !!x.riven === !!p.riven && x.rank === 1) || p;
  const usage = (rows, keep) => {
    const c = new Map();
    for (const p of rows) for (const t of new Set(toks.get(p).filter(keep))) c.set(t, (c.get(t) || 0) + 1);
    return [...c.entries()].sort((a, b) => b[1] - a[1]);
  };
  const isMod = (t) => t.startsWith("mod:");
  // COMMON CARDS FIRST: a build's mods in the weapon's own usage order, so the
  // cards a scope shares lead every row and what differs sits at the end.
  const modRank = new Map(usage(all, isMod).map(([t], i) => [t.slice(4), i]));
  const byUse = (a, b) => (modRank.get(a) ?? 999) - (modRank.get(b) ?? 999);
  const benchOf = (id) => (META.benchmarks || []).find((b) => b.id === id) || { name: id };
  const unit = rulerUnit(tr(benchOf(finder.b).name));
  const rivenKey = (r) => JSON.stringify([((r || {}).bonuses || []).slice().sort(), (r || {}).malus || ""]);
  const mark = (t, lead) => [lead ? (toks.get(lead).includes(t) ? "same" : "diff") : "",
    finder.req.has(t) ? "hit" : ""].filter(Boolean).join(" ");
  // THE ROW IS THE SIMULATOR'S BUILD CARD (`buildCardHtml`), fed from the board
  // row: one picture of a build wherever the page shows one. No ranks — a board
  // row carries none — and no mode, which the scope above already states.
  const card = (p) => {
    const r = p.board || {};
    const lead = finder.cmp && leaderOf(p) !== p ? leaderOf(p) : null;
    const ex = r.exilus && r.exilus !== "none" ? r.exilus : null;
    const modChip = (id) => {
      if (id === BOARD_RIVEN_SLOT) {
        const rv = r.riven || {};
        const stats = [...(rv.bonuses || []).map((s) => finderTokenName("rv+:" + s)),
          rv.malus ? finderTokenName("rv-:" + rv.malus) : ""].filter(Boolean).join(" ");
        const same = lead ? rivenKey((lead.board || {}).riven) === rivenKey(rv) : null;
        return { label: `${tr("Riven")} ${stats}`, title: stats,
          cls: ["rv", same === true ? "same" : same === false ? "diff" : ""].filter(Boolean).join(" ") };
      }
      const [card, rank] = splitRank(id);
      const m = modById(card);
      return { img: m ? IMG(m.image) : null,
        label: (m ? m.name : prettify(card)) + (rank != null ? ` R${rank}` : ""),
        title: id === ex ? "Exilus" : "", cls: mark("mod:" + id, lead) };
    };
    const marked = (chips) => chips.map((c) => ({ ...c, cls: mark(c.key, lead) }));
    return `<div class="fd-card${lead ? " cmp" : ""}">` + buildCardHtml({
      mods: (r.mods || []).filter(Boolean).slice().sort(byUse).map(modChip),
      parts: r.grip ? marked(partChipsOf(w.id, r.grip, r.loader)) : null,
      arcanes: (w.arcane_slots || 0) >= 1
        ? (r.arcanes || []).filter((id) => id && id !== "none").map((id) => {
          const a = arcaneById(id);
          return { img: a ? IMG(a.image) : null, label: arcName(id), cls: mark("arc:" + id, lead) };
        })
        : null,
      evolutions: w.uses_evo2 ? marked(evoChipsOf(r.evolutions || [])) : null,
      valence: r.valence ? `${DT(r.valence)} +${Math.round(((valenceSpec(w.id) || {}).max || 0) * 1000) / 10}%` : null,
    }) + `</div>`;
  };
  const legend = `<span>${escHtml(tr("Configuration"))}</span> <small>${escHtml(tr("the cards this weapon uses most come first"))}</small>`;

  const inBar = new Set(openedBoardBuilds().map(presetId));
  const openBtn = (p) => inBar.has(presetId(p))
    ? `<button type="button" class="fd-open in" data-fopen="${escHtml(presetId(p))}">${escHtml(tr("In the bar"))}</button>`
    : `<button type="button" class="fd-open" data-fopen="${escHtml(presetId(p))}">${escHtml(tr("Open build"))}</button>`;
  const maxScore = Math.max(...list.map(score), 0) || 1;
  const sbar = (p) => `<div class="fd-sbar"><i style="width:${(score(p) / maxScore * 100).toFixed(1)}%"></i></div>`;
  const shown = (p) => String((p.board || {}).shown != null ? p.board.shown : score(p).toFixed(2));

  // THE EXPANDED ROW says what the card cannot: how it differs from its group's
  // #1, and what it was measured under.
  const detail = (p) => {
    const lead = leaderOf(p);
    const mine = toks.get(p), theirs = toks.get(lead);
    const plus = mine.filter((t) => !theirs.includes(t)), minus = theirs.filter((t) => !mine.includes(t));
    return `<tr class="fdet"><td colspan="3"><div class="fd-det"><div>` +
      `<div class="fd-dt">${escHtml(trF("vs #1 ({score})", { score: shown(lead) }))}</div>` +
      (lead === p ? `<p class="small">${escHtml(tr("This is #1."))}</p>`
        : `<div class="fd-diff">${plus.map((t) => `<span class="plus">+ ${escHtml(finderTokenName(t))}</span>`).join("")}${
          minus.map((t) => `<span class="minus">− ${escHtml(finderTokenName(t))}</span>`).join("")}${
          !plus.length && !minus.length ? `<span>${escHtml(tr("the same build — the difference is how the riven rolled"))}</span>` : ""}</div>`) +
      `</div><div>` +
      `<div class="fd-dt">${escHtml(tr("Ruler"))}</div><p class="small">${escHtml(tr(benchOf(p.benchmark).name))} · ${escHtml(p.modeName || "")}</p>` +
      `</div></div></td></tr>`;
  };

  const listHtml = () => {
    if (!list.length) return `<div class="fd-empty">${escHtml(tr("No build matches — remove a condition"))}</div>`;
    const arrow = (k) => (finder.sort === k ? (finder.dir < 0 ? " ↓" : " ↑") : "");
    const rows = list.slice(0, finder.shown).map((p) => {
      const lead = leaderOf(p);
      const d = lead === p ? "#1" : `${((score(p) / (score(lead) || 1) - 1) * 100).toFixed(1)}%`;
      return `<tr class="fr${finder.open === presetId(p) ? " x" : ""}" data-frow="${escHtml(presetId(p))}">` +
        `<td><div class="fd-sv"><span class="fd-rank">#${p.rank}</span><b>${escHtml(shown(p))}</b></div>` +
        `<div class="fd-sd">${escHtml(unit ? `${unit} · ${d}` : d)}</div>${sbar(p)}</td>` +
        `<td>${card(p)}</td><td>${openBtn(p)}</td></tr>` +
        (finder.open === presetId(p) ? detail(p) : "");
    }).join("");
    const more = list.length > finder.shown || finder.shown > FINDER_FIRST
      ? `<div class="fd-more">` +
        (list.length > finder.shown
          ? `<button type="button" data-fmore="1">${escHtml(trF("Show {k} more of {n}", { k: Math.min(FINDER_STEP, list.length - finder.shown), n: list.length }))}</button>`
          : "") +
        (finder.shown > FINDER_FIRST
          ? `<button type="button" data-fless="1">${escHtml(trF("Back to the top {n}", { n: FINDER_FIRST }))}</button>`
          : "") +
        `</div>`
      : "";
    return `<table><thead><tr><th><span class="sort" data-fsort="rank">${escHtml(tr("Board rank"))}${arrow("rank")}</span> · ` +
      `<span class="sort" data-fsort="score">${escHtml(tr("Score"))}${arrow("score")}</span></th><th>${legend}</th><th></th></tr></thead>` +
      `<tbody>${rows}</tbody></table>${more}`;
  };
  // THE MATRIX: one row a build, one column a card, filled where it is carried.
  // A school of builds is a PATTERN here, which no list of names shows.
  const matrixHtml = () => {
    if (!list.length) return `<div class="fd-empty">${escHtml(tr("No build matches — remove a condition"))}</div>`;
    const cols = usage(list, isMod).slice(0, 16).map(([t]) => t);
    const head = `<tr><th style="vertical-align:bottom">${escHtml(tr("Board rank"))} · ${escHtml(tr("Score"))}</th>${
      cols.map((t) => `<th class="c" title="${escHtml(finderTokenName(t))}">${escHtml(finderTokenName(t))}</th>`).join("")}<th class="c">${escHtml(tr("riven"))}</th></tr>`;
    const body = list.slice(0, 40).map((p) => `<tr class="fr" data-frow="${escHtml(presetId(p))}"><td class="lab">#${p.rank} ${escHtml(shown(p))}${sbar(p)}</td>${
      cols.map((t) => `<td class="cell${toks.get(p).includes(t) ? " on" : ""}${finder.req.has(t) ? " hit" : ""}"><i></i></td>`).join("")}<td class="cell${p.riven ? " on" : ""}"><i></i></td></tr>`).join("");
    return `<table class="fd-mx"><thead>${head}</thead><tbody>${body}</tbody></table>` +
      (list.length > 40 ? `<p class="fd-hint">${escHtml(trF("the matrix shows the first {n}", { n: 40 }))}</p>` : "");
  };

  const railBlock = (items, limit) => {
    const n = list.length || 1;
    const shownItems = items.slice(0, limit);
    // AN EXCLUDED CARD reads 0% by construction; it stays in view so it can be undone.
    for (const t of finder.exc) if (!shownItems.some(([x]) => x === t) && items.every(([x]) => x !== t) && t.startsWith(items.kind || "")) shownItems.push([t, 0]);
    return shownItems.map(([t, c]) => `<button type="button" class="fd-u ${finder.req.has(t) ? "req" : finder.exc.has(t) ? "exc" : ""}" data-fcyc="${escHtml(t)}" title="${escHtml(finderTokenName(t))}">` +
      `<i class="ub" style="width:${(c / n * 100).toFixed(1)}%"></i><span>${escHtml(finderTokenName(t))}</span><span class="pc">${Math.round(c / n * 100)}%</span></button>`).join("");
  };
  const withKind = (items, kind) => Object.assign(items, { kind });
  const segBtn = (key, v, text, n, hint) => `<button type="button" data-fseg="${key}" data-v="${escHtml(v)}" class="${finder[key] === v ? "on" : ""}"${n ? "" : " disabled"}${hint ? ` title="${escHtml(hint)}"` : ""}>${escHtml(text)}<em>${n}</em></button>`;
  const rulers = [...new Set(all.map((p) => p.benchmark))];
  const modes = [...new Set(all.filter((p) => p.benchmark === finder.b).map((p) => p.mode))];
  const inMode = all.filter((p) => p.benchmark === finder.b && p.mode === finder.mo);

  box.innerHTML =
    `<div class="fd-head fold-h">${title}<small class="fd-count">${escHtml(trF("{n} of {m} builds", { n: list.length, m: scopeTotal }))}</small>` +
    `<div class="fd-q" role="search">` +
    [...finder.req].map((t) => `<span class="fd-tok req"><i>${escHtml(tr("must have"))}</i> <b>${escHtml(finderTokenName(t))}</b><button type="button" data-funtok="${escHtml(t)}" aria-label="${escHtml(tr("remove"))}">×</button></span>`).join("") +
    [...finder.exc].map((t) => `<span class="fd-tok exc"><i>${escHtml(tr("must not have"))}</i> <b>${escHtml(finderTokenName(t))}</b><button type="button" data-funtok="${escHtml(t)}" aria-label="${escHtml(tr("remove"))}">×</button></span>`).join("") +
    `<input id="fd-input" type="text" autocomplete="off" placeholder="${escHtml(tr("search mods, arcanes, evolutions or riven stats"))}">` +
    `<div class="fd-sugg" id="fd-sugg" hidden></div></div>` +
    `<label class="fd-cmp"><input type="checkbox" id="fd-cmp"${finder.cmp ? " checked" : ""}> ${escHtml(tr("compare with #1"))}</label>` +
    `<div class="fd-views" role="group"><button type="button" data-fview="list" class="${finder.view === "list" ? "on" : ""}">${escHtml(tr("List"))}</button>` +
    `<button type="button" data-fview="mx" class="${finder.view === "mx" ? "on" : ""}">${escHtml(tr("Mod matrix"))}</button></div></div>` +
    `<div class="fd-scope">` +
    `<div class="fd-seg"><span>${escHtml(tr("Ruler"))}</span>${rulers.map((id) => {
      const name = tr(benchOf(id).name);
      return segBtn("b", id, rulerShort(name), all.filter((p) => p.benchmark === id).length, name);
    }).join("")}</div>` +
    `<div class="fd-seg"><span>${escHtml(tr("Mode"))}</span>${modes.map((m) => segBtn("mo", m,
      (all.find((p) => p.benchmark === finder.b && p.mode === m) || {}).modeName || m,
      all.filter((p) => p.benchmark === finder.b && p.mode === m).length)).join("")}</div>` +
    `<div class="fd-seg"><span>${escHtml(tr("Riven"))}</span>` +
    segBtn("rv", "all", tr("All"), inMode.length) +
    segBtn("rv", "riven", tr("With riven"), inMode.filter((p) => p.riven).length) +
    segBtn("rv", "plain", tr("Without riven"), inMode.filter((p) => !p.riven).length) + `</div>` +
    // HOW DEEP THE BOARD IS READ — the one control here that is not a filter on
    // what is loaded but on what was loaded at all. Every other segment narrows
    // the rows in hand; this one decides how many the conversion produces, so
    // it is counted from the raw board rather than from `all`.
    `<div class="fd-seg fd-depth"><span>${escHtml(tr("Depth"))}</span>` +
    BOARD_DEPTHS.map((d) => `<button type="button" data-fdepth="${d}" class="${
      boardDepth === d ? "on" : ""}" title="${escHtml(d
        ? trF("builds scoring at least {p}% of their group's leader", { p: Math.round(d * 100) })
        : tr("every build the board has scored"))}">${escHtml(d ? `≥${Math.round(d * 100)}%` : tr("All"))
      }<em>${depthCount(w, d)}</em></button>`).join("") + `</div></div>` +
    `<div class="fd-rail">` +
    `<div><h4>${escHtml(tr("Mod usage"))}<small>${escHtml(trF("in {n} builds", { n: list.length }))}</small></h4>` +
    `<div class="fd-use mods">${railBlock(withKind(usage(list, isMod), "mod:"), 15)}</div>` +
    `<p class="fd-hint">${escHtml(tr("click: must have · again: exclude · again: clear"))}</p></div>` +
    `<div><h4>${escHtml(tr("Arcane"))}</h4><div class="fd-use">${railBlock(withKind(usage(list, (t) => t.startsWith("arc:")), "arc:"), 6) || "—"}</div></div>` +
    `<div><h4>${escHtml(tr("Evolutions"))}</h4><div class="fd-use">${railBlock(withKind(usage(list, (t) => t.startsWith("evo:")), "evo:"), 8) || "—"}</div></div>` +
    `</div>` +
    `<div class="fd-main">${finder.view === "list" ? listHtml() : matrixHtml()}</div>`;

  // ---- wiring --------------------------------------------------------------
  wireFolds(box);
  const rerender = (focus) => {
    renderBuildFinder();
    if (focus) { const i = $("fd-input"); if (i) i.focus(); }
  };
  const input = $("fd-input");
  const sugg = $("fd-sugg");
  const vocab = [...new Set(all.flatMap((p) => toks.get(p)))];
  const hits = () => {
    const q = input.value.trim().toLowerCase();
    if (!q) return [];
    return vocab.filter((t) => !finder.req.has(t) && !finder.exc.has(t)
      && (finderTokenName(t).toLowerCase().includes(q) || t.toLowerCase().includes(q))).slice(0, 8);
  };
  const drawSugg = () => {
    const hs = hits();
    if (!input.value.trim()) { sugg.hidden = true; return; }
    const n = list.length || 1;
    finder.hi = Math.min(finder.hi, Math.max(0, hs.length - 1));
    sugg.innerHTML = hs.length
      ? hs.map((t, i) => `<div class="fd-sg${i === finder.hi ? " hi" : ""}"><span class="k">${escHtml(finderTokenKind(t))}</span>` +
        `<span class="nm">${escHtml(finderTokenName(t))}</span><span class="pc">${escHtml(trF("{p}% use it", { p: Math.round(list.filter((p) => toks.get(p).includes(t)).length / n * 100) }))}</span>` +
        `<button type="button" class="p" data-freq="${escHtml(t)}">${escHtml(tr("must have"))}</button><button type="button" class="n" data-fexc="${escHtml(t)}">${escHtml(tr("must not have"))}</button></div>`).join("")
      : `<div class="fd-sg"><span class="nm">${escHtml(trF("nothing is called “{q}”", { q: input.value.trim() }))}</span></div>`;
    sugg.hidden = false;
  };
  input.addEventListener("input", () => { finder.hi = 0; drawSugg(); });
  input.addEventListener("keydown", (e) => {
    const hs = hits();
    if (e.key === "ArrowDown") { finder.hi = Math.min(finder.hi + 1, hs.length - 1); drawSugg(); e.preventDefault(); }
    else if (e.key === "ArrowUp") { finder.hi = Math.max(finder.hi - 1, 0); drawSugg(); e.preventDefault(); }
    else if (e.key === "Enter" && hs[finder.hi]) { finder.req.add(hs[finder.hi]); finder.shown = FINDER_FIRST; rerender(true); }
    else if (e.key === "Escape") sugg.hidden = true;
    else if (e.key === "Backspace" && !input.value) {
      const last = [...finder.exc].pop() || [...finder.req].pop();
      if (last) { finder.req.delete(last); finder.exc.delete(last); rerender(true); }
    }
  });
  $("fd-cmp").addEventListener("change", (e) => { finder.cmp = e.target.checked; rerender(); });
  box.onclick = (e) => {
    const g = (sel) => e.target.closest(sel);
    let el;
    if ((el = g("[data-freq]"))) { finder.req.add(el.dataset.freq); finder.shown = FINDER_FIRST; return rerender(true); }
    if ((el = g("[data-fexc]"))) { finder.exc.add(el.dataset.fexc); finder.shown = FINDER_FIRST; return rerender(true); }
    if (!g(".fd-q")) sugg.hidden = true;
    if ((el = g("[data-funtok]"))) { finder.req.delete(el.dataset.funtok); finder.exc.delete(el.dataset.funtok); return rerender(); }
    if ((el = g("[data-fdepth]"))) {
      setBoardDepth(Number(el.dataset.fdepth));
      finder.open = null; finder.shown = FINDER_FIRST;
      // EVERY LIST THAT READS THE BOARD, not just this one: the build bar and
      // its chips are drawn from the same conversion.
      renderMods();
      return rerender();
    }
    if ((el = g("[data-fseg]"))) {
      finder[el.dataset.fseg] = el.dataset.v;
      finder.open = null; finder.shown = FINDER_FIRST;
      if (el.dataset.fseg !== "rv") finder.rv = "all";
      return rerender();
    }
    if ((el = g("[data-fcyc]"))) {
      const t = el.dataset.fcyc;
      if (finder.req.has(t)) { finder.req.delete(t); finder.exc.add(t); }
      else if (finder.exc.has(t)) finder.exc.delete(t);
      else finder.req.add(t);
      finder.shown = FINDER_FIRST;
      return rerender();
    }
    if ((el = g("[data-fsort]"))) {
      const k = el.dataset.fsort;
      finder.dir = finder.sort === k ? -finder.dir : (k === "rank" ? 1 : -1);
      finder.sort = k;
      return rerender();
    }
    if ((el = g("[data-fview]"))) { finder.view = el.dataset.fview; return rerender(); }
    if ((el = g("[data-fmore]"))) { finder.shown += FINDER_STEP; return rerender(); }
    if ((el = g("[data-fless]"))) { finder.shown = FINDER_FIRST; finder.open = null; return rerender(); }
    if ((el = g("[data-fopen]"))) {
      e.stopPropagation();
      const p = all.find((x) => presetId(x) === el.dataset.fopen);
      if (!p) return;
      rememberBoardBuild(p);
      if (presetId(p) === activePreset) return renderPresetBar();
      return pickPreset(buildBarCfg(), presetId(p));
    }
    if ((el = g("tr.fr[data-frow]"))) {
      const id = el.dataset.frow;
      if (finder.view === "mx") {
        finder.view = "list";
        finder.open = id;
        finder.shown = Math.max(finder.shown, list.findIndex((p) => presetId(p) === id) + 1);
      } else finder.open = finder.open === id ? null : id;
      return rerender();
    }
  };
}

function renderPresetBar() {
  renderBuildFinder();
  renderPresetBarIn($("preset-bar-builder-builds"), buildBarCfg());
}

// A scenario is the `sim` object, BUFF CONFIG INCLUDED.
//
// Buff ids are global — `arcane:primary_deadhead`, a mod's own buff — so a
// setting travels, and `sim.buffs` deliberately keeps entries for buffs the
// build does not currently carry: that is what lets a scenario say "in THIS
// fight, Deadhead starts at zero stacks" and have it hold when the mod is added
// later. Anything unmentioned takes the buff's own default, full and unlocked.
/// Fields a scenario may no longer hold. A stored preset written before the
/// mode moved into the build still carries `form`, and applying it would put
/// it back on `sim` — where the next auto-save would write it out again, and
/// keep writing it forever. Dropped on the way in, so a custom scenario is
/// clean the first time it is opened and stays clean.
/// FIELDS A SCENARIO NO LONGER CARRIES, stripped in both directions so a stored
/// one — or a benchmark yaml — cannot reintroduce them.
///
/// `runs` joined them on 2026-08-13. HOW HARD YOU MEASURE IS NOT PART OF THE
/// FIGHT. The official rulers still run at 1,000 — that is the number
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
  // Spreading over the live `sim` leaves any field the incoming scenario does
  // not mention holding the outgoing one's value — and a benchmark yaml
  // mentions only what it has an opinion about. Tick Eximus on a copy of the
  // official ruler, switch back to the official, and the official fight is
  // now against an Eximus, because `standard_single_target.yaml` never says `eximus:`. `invisible` did not leak in the same
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
  // …AND EVERY BODY GETS ITS UNIT, ONCE.
  //
  // A body placed since 2026-08-18 carries the unit it was placed with. Every
  // body placed BEFORE that carries nothing, and a blank means "the aimed
  // body's" — which is exactly what those bodies meant when they were placed,
  // and is also, from the reader's side, indistinguishable from the bug that
  // rule was written to end: switch the enemy on the left and the whole
  // formation follows it — which is still true after the placement fix for
  // every formation saved before it.
  //
  // So the blank is filled in HERE, at the one place a scenario becomes the
  // live fight, from the enemy that scenario itself carries. It changes nothing
  // about what those bodies are — it writes down what they already were — and
  // from that moment they stop following. Growth stopping is not the same as
  // the existing ones being fixed, which is the same lesson as
  // `reclaimStoredReplays` and was missed the same way.
  (sim.formation || []).forEach((f) => { if (!f.enemy) f.enemy = sim.enemy; });
  renderSim();      // redraws every knob, and the bar with them
  refreshPanel();   // the Tenno half of a scenario changes what the build is worth
}
// A scenario is CONSUMED outside the simulator — the quick calc scans under
// one by name, and the optimizer states the one it will search with — so
// creating, renaming or deleting one has to reach those lists at once, the
// way a new riven reaches the mod pool. One hook: the bar
// calls `rerender` after every mutation, switching included.
function scenariosChanged() {
  renderScenarioBar();
  // Switching or copying a scenario changes whether the fight is EDITABLE, and
  // this hook is the only thing every mutation goes through — `renderSim` is
  // not called here, so without this line a copy of an official scenario kept
  // the original's inert controls.
  lockOfficialScenario();
  // ...and whether this fight can reach the board at all — asked of the board.
  renderBoardConsent();
  refreshBoardDoor();
  if ($("opt-buffs")) renderOptBuffs();
  // …and the Warframe buffs, which are the SCENARIO's: switching fights
  // switches which abilities are running, so the cards have to be repainted
  // from the incoming state rather than left showing the outgoing one's.
  if ($("sim-wfbuffs")) renderWfBuffs("sim-wfbuffs", false);
  // …and the QUICK CALC RE-ASKS. Not just repaints: switching fights is the
  // biggest thing that can happen to a ranking, and this hook drew the box
  // under the new fight's name while every chip beside it still answered the
  // old one's question. A `markScenarioDirty` EDIT has
  // re-run the scan since it existed; a SWITCH never did, because a switch is
  // a replacement rather than an edit and goes nowhere near that debounce.
  //
  // Here rather than at the call sites, because this hook is already "the only
  // thing every scenario mutation goes through" — the line above says so for
  // `lockOfficialScenario`, and the same reasoning makes it the one place that
  // cannot be forgotten by a mutation added later.
  refreshGains();
  if ($("opt-target")) renderOptEnemy();
}

