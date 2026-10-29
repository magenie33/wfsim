// ---- THE BOARD BUILDS IN THE BUILD BAR ------------------------------------
//
// EVERY BUILD IS IN THE BUILD BAR: your own, and every board build you opened,
// read-only. The bar is the one place that says which build is open.
//
// KEPT BY WHAT THE BUILD IS, NEVER BY ITS RANK. A board build's `builtin` id
// ends in its position, and a rescore renumbers the board — a remembered "#3" would
// reopen somebody else's build. So an entry is the build's identity
// (`boardRowIdentity`) plus the cell it was ranked in; its rank and score are
// read fresh, and a build that has left the board stops resolving. Nothing is
// pruned on a miss: a board that has not loaded yet misses everything.
const LOCK_SVG = '<svg class="plock" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.4" aria-hidden="true"><rect x="2.2" y="5.2" width="7.6" height="5.3" rx="1.2"/><path d="M4 5.2V3.8a2 2 0 0 1 4 0v1.4"/></svg>';
const openedBoardKey = (w) => `wfsim-opened-board-${w}`;
const boardRef = (p) => ({ b: p.benchmark, m: p.mode, k: p.riven ? "r" : "p", id: boardRowIdentity(p.board || {}) });
const sameBoardRef = (a, b) => a.b === b.b && a.m === b.m && a.k === b.k && a.id === b.id;
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
  if (!ref || presetFind(loadPresetList(BUILDS), last)) return last;
  const p = builtinBuilds().find((x) => sameBoardRef(boardRef(x), ref));
  return p ? presetId(p) : "";
}
// ---- THE BUILD FINDER -------------------------------------------------------
//
// THE BOARD'S BUILDS AS A LIST, best first: scoped by ruler, mode and riven,
// filtered by what a build contains, each row opening into the whole build. It
// holds a QUERY and never a selection — which build is open is the build bar's
// to say, and "Open" does one thing: puts that build in the bar and makes it
// current. Builder only (style.css). docs/UI.md §"The build finder".
/// FIVE ROWS UNTIL ASKED: the top of a scope is what most readers came for, and
/// the rest is one click away rather than a page of scrolling.
const FINDER_FIRST = 5;
const FINDER_STEP = 20;
/// The Mode segment's "All": every mode of the ruler in one list, best first.
/// A row's rank is still its own mode's, so in this scope each row names it.
const FINDER_EVERY_MODE = "*";
const finder = {
  weapon: null, sc: "", b: null, mo: null, rv: "all",
  req: new Set(), exc: new Set(),
  open: null, ref: null, shown: FINDER_FIRST, hi: 0,
  // The ties opened to their exact builds, by `tieGroups` key.
  ties: new Set(),
};

/// THE TRIGGERS A READER REFUSES, kept across weapons and visits: "no weak-point
/// kills" is how somebody plays, not a question about one weapon. The words are
/// the fight's own (`/api/meta.buff_triggers`), so refusing a weak-point kill
/// leaves every card fired by any kill standing.
const FINDER_OFF_KEY = "wfsim-finder-triggers-off";
function finderOff() {
  try { return new Set(JSON.parse(localStorage.getItem(FINDER_OFF_KEY) || "[]")); } catch (_) { return new Set(); }
}
function setFinderOff(set) {
  try { localStorage.setItem(FINDER_OFF_KEY, JSON.stringify([...set])); } catch (_) { /* a private window keeps none */ }
}
/// WHAT FIRES A BUILD'S PARTS: trigger -> the names of the parts asking for it,
/// read from `/api/meta.part_triggers` by each part's build axis.
function finderTriggers(p) {
  const pt = (META && META.part_triggers) || {};
  const out = new Map();
  for (const x of boardRowParts(p.board || {})) {
    const id = x.axis === "mods" ? splitRank(x.id)[0] : x.id;
    for (const t of (pt[x.axis] || {})[id] || []) {
      const name = x.axis === "mods" ? ((modById(id) || {}).name || prettify(id)) : x.axis === "arcanes" ? arcName(id) : evoName(id);
      out.set(t, [...(out.get(t) || []), name]);
    }
  }
  return out;
}

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

/// A LIST, AND EVERY ROW OPENS THE SAME WAY. The rows are the scope's builds,
/// best first; a row opens in place into the whole build and, per piece of it,
/// how many builds here carry that piece and the best build without it. A
/// comparison is the reader's: any build can be made the reference, and every
/// row then reads against it. Nothing here is a threshold somebody chose.
/// THE DEPTH SLIDER, in whole per cent: the leader alone on the left, the
/// board's entry line on the right, which stands for zero — every row it kept.
/// Its value runs mirrored (`100 + floor - share`) so the filled track is the
/// span shown: full from 100% to the line, all of it, by default.
const finderDepthFloor = () => Math.round(((META && META.board_entry_share) || 0) * 100);
const depthOfSlider = (el) => {
  const p = 100 + finderDepthFloor() - Number(el.value);
  return p <= finderDepthFloor() ? 0 : p / 100;
};
function finderDepthLabel(w, d) {
  const text = d ? `≥${Math.round(d * 100)}%` : `${tr("All")} (≥${finderDepthFloor()}%)`;
  return `<span title="${escHtml(d ? trF("builds scoring at least {p}% of their group's leader", { p: Math.round(d * 100) })
    : tr("every build the board has scored"))}">${escHtml(text)}</span><em>${depthCount(w, d)}</em>`;
}
function finderDepthHtml(w) {
  const floor = finderDepthFloor();
  const at = 100 + floor - (boardDepth ? Math.max(floor, Math.min(100, Math.round(boardDepth * 100))) : floor);
  return `<input type="range" data-fdepth-range min="${floor}" max="100" step="${Math.round(BOARD_DEPTH_STEP * 100)}" value="${at}"
    aria-label="${escHtml(tr("Depth"))}"><span id="fd-depth-at" class="fd-depth-at">${finderDepthLabel(w, boardDepth)}</span>`;
}

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
    Object.assign(finder, { weapon: w.id, b: act ? act.benchmark : null, mo: FINDER_EVERY_MODE,
      sc: act ? benchSceneOf((META.benchmarks || []).find((b) => b.id === act.benchmark)) : "",
      rv: "all", req: new Set(), exc: new Set(), open: null, ref: null, shown: FINDER_FIRST, hi: 0, ties: new Set(),
      ships_open: !loadPresetList(BUILDS).length });
  }
  box.hidden = false;
  // IT SHIPS SHUT ABOVE A BUILD YOU CAME TO EDIT. With none of your own on this
  // weapon there is nothing to edit, so it ships open — decided on arriving at
  // the weapon, so a first edit does not shut it under the reader.
  box.classList.toggle("shut", folded("build-finder", !finder.ships_open));
  // THE HEAD IS THE FOLD'S HEADING (`wireFolds`), redrawn with every render, so
  // it is rewired after each one; shut, the finder is its title line.
  const title = `<h2 class="fd-title">${escHtml(tr("Build finder"))}</h2>`;
  if (!benchList().some((b) => b.id === finder.b)) finder.b = benchList()[0]?.id;
  const benchOf = (id) => benchList().find((b) => b.id === id) || { name: id };
  if (!all.length) {
    box.innerHTML = `<div class="fd-head fold-h">${title}</div>` + benchSelectorHtml(finder.b)
      + `<div class="fd-empty">${escHtml(tr("Nobody has submitted a build for this weapon yet"))}</div>`;
    wireFolds(box);
    wireBenchSelector(box, (id) => { finder.b = id; renderBuildFinder(); });
    return;
  }
  const inMo = (p) => p.benchmark === finder.b && (finder.mo === FINDER_EVERY_MODE || p.mode === finder.mo);
  if (finder.mo !== FINDER_EVERY_MODE && !all.some(inMo)) finder.mo = FINDER_EVERY_MODE;
  const everyMode = finder.mo === FINDER_EVERY_MODE;
  const inScope = (p) => inMo(p)
    && (finder.rv === "all" || (finder.rv === "riven") === !!p.riven);
  const toks = new Map(all.map((p) => [p, finderTokens(p)]));
  const off = finderOff();
  const trigs = new Map(all.map((p) => [p, finderTriggers(p)]));
  const passes = (p) => [...finder.req].every((t) => toks.get(p).includes(t))
    && ![...finder.exc].some((t) => toks.get(p).includes(t))
    && ![...trigs.get(p).keys()].some((t) => off.has(t));
  const score = (p) => (p.board || {}).score || 0;
  // A TIE IS ONE PLACE OF ONE RANKING (`placed`): consecutive, so builds of two
  // modes on one score stay two answers.
  const tieKey = (p) => `${p.benchmark}#${p.mode}#${p.riven ? "r" : "p"}#${p.rank}`;
  const list = all.filter((p) => inScope(p) && passes(p))
    .sort((a, b) => score(b) - score(a) || (tieKey(a) < tieKey(b) ? -1 : tieKey(a) > tieKey(b) ? 1 : 0));
  const ties = tieGroups(list, tieKey, (p) => p.board || {}, META);
  const scopeTotal = all.filter(inMo).length;
  const updated = measuredText(boardMeasuredAt(all.filter((p) => p.benchmark === finder.b).map((p) => p.board)));
  const unit = rulerUnit(tr(benchOf(finder.b).name));
  const shown = (p) => String((p.board || {}).shown != null ? p.board.shown : score(p).toFixed(2));
  const byId = (id) => all.find((p) => presetId(p) === id) || null;
  // THE REFERENCE the reader picked, while it is still in the rows.
  const ref = finder.ref && list.find((p) => presetId(p) === finder.ref) || null;
  const gap = (p, to) => score(p) / (score(to) || 1) - 1;
  const pct = (g) => `${g >= 0 ? "+" : "−"}${Math.abs(g * 100).toFixed(1)}%`;
  const sgn = (g) => (g > 0 ? "up" : g < 0 ? "down" : "");
  const rivenKey = (r) => JSON.stringify([((r || {}).bonuses || []).slice().sort(), (r || {}).malus || ""]);
  const rivenStats = (rv) => [...(rv.bonuses || []).map((s) => finderTokenName("rv+:" + s)),
    rv.malus ? finderTokenName("rv-:" + rv.malus) : ""].filter(Boolean).join(" ");

  // THE BUILD CARD (`buildCardHtml`). THE MODS RUN IN THE ORDER THE BUILD WAS
  // SAVED: elements combine in slot order, so a reordered row can read as a
  // different element build. Against `lead`, what it also carries is muted and
  // what it does not is marked.
  const card = (p, lead, big) => {
    const r = p.board || {};
    const ex = r.exilus && r.exilus !== "none" ? r.exilus : null;
    const mark = (t) => [lead ? (toks.get(lead).includes(t) ? "same" : "diff") : "", finder.req.has(t) ? "hit" : ""]
      .filter(Boolean).join(" ");
    const modChip = (id) => {
      if (id === BOARD_RIVEN_SLOT) {
        const rv = r.riven || {};
        const same = lead ? rivenKey((lead.board || {}).riven) === rivenKey(rv) : null;
        return { label: `${tr("Riven")} ${rivenStats(rv)}`, title: rivenStats(rv),
          cls: ["rv", same === true ? "same" : same === false ? "diff" : ""].filter(Boolean).join(" ") };
      }
      const [c, rank] = splitRank(id);
      const m = modById(c);
      return { img: m ? IMG(m.image) : null, label: (m ? m.name : prettify(c)) + (rank != null ? ` R${rank}` : ""),
        title: id === ex ? "Exilus" : "", cls: mark("mod:" + id) };
    };
    const marked = (chips) => chips.map((c) => ({ ...c, cls: mark(c.key) }));
    return `<div class="fd-card${lead ? " cmp" : ""}${big ? " big" : ""}">` + buildCardHtml({
      // THE EXILUS RIDES AT THE END when the list does not already hold it.
      mods: [...(r.mods || []).filter(Boolean), ...(ex && !(r.mods || []).includes(ex) ? [ex] : [])].map(modChip),
      parts: r.grip ? marked(partChipsOf(w.id, r.grip, r.loader)) : null,
      arcanes: (w.arcane_slots || 0) >= 1
        ? (r.arcanes || []).filter((id) => id && id !== "none").map((id) => {
          const a = arcaneById(id);
          return { img: a ? IMG(a.image) : null, label: arcName(id), cls: mark("arc:" + id) };
        })
        : null,
      evolutions: w.uses_evo2 ? marked(evoChipsOf(r.evolutions || [])) : null,
      valence: r.valence ? `${DT(r.valence)} +${Math.round(((valenceSpec(w.id) || {}).max || 0) * 1000) / 10}%` : null,
    }) + (measuredRecordHtml(r) ? `<span class="sb-h">${escHtml(tr("Record"))}</span><span class="fd-when" title="${escHtml(measuredNote())}">${measuredRecordHtml(r)}</span>` : "")
      + `</div>`;
  };
  const inBar = new Set(openedPublished(buildBarCfg()).map(presetId));
  const openBtn = (p) => inBar.has(presetId(p))
    ? `<button type="button" class="fd-open in" data-fopen="${escHtml(presetId(p))}">${escHtml(tr("In the bar"))}</button>`
    : `<button type="button" class="fd-open" data-fopen="${escHtml(presetId(p))}">${escHtml(tr("Open build"))}</button>`;
  const diffHtml = (p, to) => {
    const mine = toks.get(p), theirs = toks.get(to);
    const plus = mine.filter((t) => !theirs.includes(t)), minus = theirs.filter((t) => !mine.includes(t));
    // A riven stat names its own sign ("+Critical Chance"), so it takes no second one.
    const sign = (t, s) => (t.startsWith("rv") ? "" : s + " ");
    return plus.map((t) => `<span class="plus">${sign(t, "+")}${escHtml(finderTokenName(t))}</span>`).join("")
      + minus.map((t) => `<span class="minus">${sign(t, "−")}${escHtml(finderTokenName(t))}</span>`).join("")
      + (!plus.length && !minus.length ? `<span>${escHtml(tr("the same build — the difference is how the riven rolled"))}</span>` : "");
  };

  // ---- A ROW, OPENED -------------------------------------------------------
  // Per piece of THIS build: the share of the builds here that carry it, and
  // the best build here without it, against this one.
  const whyHtml = (x) => {
    const n = list.length;
    const mine = toks.get(x).filter((t) => !t.startsWith("rv"));
    const row = (name, img, carriers, alt, action) => {
      const share = carriers / n;
      const g = alt ? gap(alt, x) : null;
      return `<div class="fd-why-row"><span class="nm">${img ? imgTag(img, "sb-img") : ""}<span>${escHtml(name)}</span></span>` +
        `<span class="share"><i style="width:${(share * 100).toFixed(1)}%"></i><em>${carriers} / ${n}</em></span>` +
        `<span class="cost">${alt == null ? `<span class="fd-sd">${escHtml(tr("every build here carries it"))}</span>`
          : `<b class="${sgn(g)}">${escHtml(pct(g))}</b> <span class="fd-sd">#${alt.rank}</span>`}</span>` +
        (alt == null ? "<span></span>" : `<button type="button" class="fd-nohave" ${action}>${escHtml(tr("I don't have it"))}</button>`) + `</div>`;
    };
    const tokRow = (t) => {
      const carriers = list.filter((p) => toks.get(p).includes(t)).length;
      const alt = list.find((p) => p !== x && !toks.get(p).includes(t)) || null;
      const kind = t.slice(0, t.indexOf(":")), id = t.slice(t.indexOf(":") + 1);
      const m = kind === "mod" ? modById(splitRank(id)[0]) : null, a = kind === "arc" ? arcaneById(id) : null;
      return row(finderTokenName(t), m ? IMG(m.image) : a ? IMG(a.image) : null, carriers, alt, `data-fnohave="${escHtml(t)}"`);
    };
    const groups = [["mod", tr("Mods")], ["part", tr("Parts")], ["arc", tr("Arcane")], ["evo", tr("Evolutions")], ["val", tr("Valence")]];
    let body = groups.map(([k, head]) => {
      const rows = mine.filter((t) => t.startsWith(k + ":")).map(tokRow);
      return rows.length ? `<div class="fd-why-g">${escHtml(head)}</div>${rows.join("")}` : "";
    }).join("");
    if (x.riven) {
      const carriers = list.filter((p) => p.riven).length;
      const alt = all.filter((p) => inMo(p) && !p.riven && passes(p))
        .sort((a, b) => score(b) - score(a))[0] || null;
      body += `<div class="fd-why-g">${escHtml(tr("Riven"))}</div>` +
        row(`${tr("Riven")} ${rivenStats(x.board.riven || {})}`, null, carriers, alt, `data-fnoriven="1"`);
    }
    return `<div class="fd-why"><div class="fd-why-head"><span>${escHtml(tr("In this build"))}</span>` +
      `<span>${escHtml(tr("builds here that carry it"))}</span>` +
      `<span>${escHtml(tr("best build here without it, against this one"))}</span><span></span></div>${body}</div>` +
      `<p class="fd-hint">${escHtml(tr("The gap is to the best board build without that piece — not a measured one-for-one swap."))}</p>`;
  };
  const detail = (p) => {
    const isRef = ref === p;
    return `<tr class="fdet"><td colspan="3"><div class="fd-open-row">` +
      `<div class="fd-ans-head"><div class="fd-ans-t"><div class="fd-sd">${escHtml(trF("#{r} on the board", { r: p.rank }))} · ` +
      `${escHtml(benchmarkName(p.benchmark))} · ${escHtml(p.modeName || "")}</div></div>` +
      `<div class="fd-ans-score"><b>${escHtml(shown(p))}</b>${unit ? `<small>${escHtml(unit)}</small>` : ""}</div>` +
      `<button type="button" class="fd-refbtn${isRef ? " on" : ""}" data-fref="${escHtml(presetId(p))}">${escHtml(tr(isRef ? "Stop comparing" : "Compare others with this"))}</button>` +
      `${openBtn(p)}${SHARE_ENABLED ? `<button type="button" class="fd-share" data-fshare="${escHtml(presetId(p))}">${escHtml(tr("share"))}</button>` : ""}</div>` +
      card(p, null, true) +
      (ref && !isRef ? `<div class="fd-dt">${escHtml(trF("Against the reference, #{r}", { r: ref.rank }))} · <b class="${sgn(gap(p, ref))}">${escHtml(pct(gap(p, ref)))}</b></div>` +
        `<div class="fd-diff">${diffHtml(p, ref)}</div>` : "") +
      `<div class="fd-dt">${escHtml(tr("Piece by piece"))}</div>${whyHtml(p)}` +
      `</div></td></tr>`;
  };

  // ---- A TIE: one row, its alternatives, and its exact builds on request ----
  const posName = (pos) => {
    const [axis, i] = pos.split("#");
    if (axis === "arcanes") return (w.arcane_slots || 0) > 1 ? `${tr("Arcane")} ${Number(i) + 1}` : tr("Arcane");
    if (axis === "evolutions") return `${tr("Evolution")} · ${trF("tier {n}", { n: Number(i) + 1 })}`;
    return ({ mods: tr("Mods"), exilus: "Exilus", riven: tr("Riven"), assembly: tr("Parts"), valence: tr("Valence") })[axis] || pos;
  };
  const altChip = (x) => {
    if (!x) return `<span class="sb-chip fd-none"><span>${escHtml(tr("empty"))}</span></span>`;
    if (x.axis === "rivens") {
      const [bonuses, malus] = JSON.parse(x.id);
      return `<span class="sb-chip rv"><span>${escHtml(rivenStats({ bonuses, malus }))}</span></span>`;
    }
    const t = ({ mods: "mod:", arcanes: "arc:", evolutions: "evo:", assembly: "part:", valence: "val:" })[x.axis] + x.id;
    const m = x.axis === "mods" ? modById(splitRank(x.id)[0]) : null, a = x.axis === "arcanes" ? arcaneById(x.id) : null;
    const e = x.axis === "evolutions" ? evoChipsOf([x.id], w.id)[0] : null;
    const img = m ? IMG(m.image) : a ? IMG(a.image) : e ? e.img : null;
    return `<span class="sb-chip${finder.req.has(t) ? " hit" : ""}">${img ? imgTag(img, "sb-img") : ""}<span>${escHtml(finderTokenName(t))}</span></span>`;
  };
  const freeHtml = (g) => !g.free.length ? "" : `<div class="fd-free"><div class="t">${escHtml(tr("Interchangeable here: every build of this place scores exactly the same"))}${
    g.free.length > 1 ? ` · ${escHtml(g.every ? tr("any combination") : trF("only these {n} combinations are measured", { n: g.members.length }))}` : ""}</div>` +
    g.free.map(([pos, alts]) => `<div class="fd-free-r"><span>${escHtml(posName(pos))}</span><div class="sb-chips">${
      alts.map(altChip).join(`<span class="or">/</span>`)}</div></div>`).join("") + `</div>`;
  const rowHtml = (p, maxScore, g, lead) => {
    const n = g ? g.members.length : 1;
    const opened = g && finder.ties.has(g.key);
    const d = ref && ref !== p ? gap(p, ref) : null;
    return `<tr class="fr${lead ? " mem" : ""}${n > 1 ? " tie" : ""}${finder.open === presetId(p) && !opened ? " x" : ""}${ref === p ? " ref" : ""}" data-frow="${escHtml(presetId(p))}">` +
      `<td><div class="fd-sv"><span class="fd-rank">#${p.rank}</span><b>${escHtml(shown(p))}</b></div>` +
      (n > 1 ? `<div class="fd-tie">${escHtml(trF("tied · {n} builds", { n }))}</div>` : "") +
      `<div class="fd-sd">${ref === p ? escHtml(tr("reference")) : d != null ? `<span class="${sgn(d)}">${escHtml(pct(d))}</span>` : escHtml(unit)}` +
      `${everyMode ? ` · ${escHtml(p.modeName || "")}` : ""}</div>` +
      `<div class="fd-sbar"><i style="width:${(score(p) / maxScore * 100).toFixed(1)}%"></i></div></td>` +
      `<td>${card(p, ref && ref !== p ? ref : lead && lead !== p ? lead : null)}${n > 1 ? freeHtml(g) +
        `<button type="button" class="fd-tiex" data-ftie="${escHtml(g.key)}">${escHtml(opened ? tr("Hide the exact builds") : trF("Show the {n} exact builds", { n }))}</button>` : ""}</td>` +
      `<td>${openBtn(p)}</td></tr>` +
      (finder.open === presetId(p) && !opened ? detail(p) : "") +
      (opened ? g.members.map((m) => rowHtml(m, maxScore, null, g.members[0])).join("") : "");
  };

  const listHtml = () => {
    if (!list.length) return `<div class="fd-empty">${escHtml(tr(all.some((p) => p.benchmark === finder.b)
      ? "No build matches — remove a condition" : "No build has been measured under these board conditions."))}</div>`;
    const maxScore = Math.max(...list.map(score), 0) || 1;
    const rows = ties.slice(0, finder.shown).map((g) => rowHtml(g.members[0], maxScore, g, null)).join("");
    const more = ties.length > finder.shown || finder.shown > FINDER_FIRST
      ? `<div class="fd-more">` +
        (ties.length > finder.shown
          ? `<button type="button" data-fmore="1">${escHtml(trF("Show {k} more of {n}", { k: Math.min(FINDER_STEP, ties.length - finder.shown), n: ties.length }))}</button>`
          : "") +
        (finder.shown > FINDER_FIRST
          ? `<button type="button" data-fless="1">${escHtml(trF("Back to the top {n}", { n: FINDER_FIRST }))}</button>`
          : "") +
        `</div>`
      : "";
    const refBar = ref
      ? `<div class="fd-refbar">${escHtml(tr("Comparing with"))} <b>#${ref.rank} ${escHtml(shown(ref))}</b> — ${escHtml(tr("muted: the same as it; the percentage is against it"))}` +
        ` <button type="button" data-fref="${escHtml(presetId(ref))}">${escHtml(tr("Stop comparing"))}</button></div>`
      : `<div class="fd-refbar off">${escHtml(tr("Click a build to open it; from there, make it the reference to compare the others with."))}</div>`;
    return refBar + `<table><thead><tr><th>${escHtml(tr("Board rank"))} · ${escHtml(tr("Score"))}</th>` +
      `<th>${escHtml(tr("Configuration"))}</th><th></th></tr></thead><tbody>${rows}</tbody></table>${more}`;
  };

  // ---- THE OVERVIEW, FOLDED ------------------------------------------------
  // Every piece any build here carries, by module: how many carry it and the
  // best of them. Counts, never a verdict; a click makes it a condition.
  const overviewHtml = () => {
    const n = list.length;
    const count = new Map();
    for (const p of list) for (const t of new Set(toks.get(p))) count.set(t, (count.get(t) || 0) + 1);
    for (const t of finder.exc) if (!count.has(t)) count.set(t, 0);
    const groups = [["mod", tr("Mods")], ["part", tr("Parts")], ["arc", tr("Arcane")], ["evo", tr("Evolutions")],
      ["val", tr("Valence")], ["rv", tr("Riven stats")]];
    const body = groups.map(([k, head]) => {
      const items = [...count.entries()].filter(([t]) => t.startsWith(k)).sort((a, b) => b[1] - a[1]);
      if (!items.length) return "";
      return `<div class="fd-ov-g"><h4>${escHtml(head)}<small>${escHtml(tr("builds carrying it · the best of them"))}</small></h4><div class="fd-use mods">${items.map(([t, c]) => {
        const best = list.find((p) => toks.get(p).includes(t));
        return `<button type="button" class="fd-u fd-ov-u ${finder.req.has(t) ? "req" : finder.exc.has(t) ? "exc" : ""}" data-fcyc="${escHtml(t)}" title="${escHtml(finderTokenName(t))}">` +
          `<i class="ub" style="width:${(c / (n || 1) * 100).toFixed(1)}%"></i><span>${escHtml(finderTokenName(t))}</span>` +
          `<span class="pc">${c}</span><span class="pc">${best ? `#${best.rank}` : "—"}</span></button>`;
      }).join("")}</div></div>`;
    }).join("");
    return `<div class="fold sect shut fd-ov" data-fold="finder-overview"><h3 class="sim-h fold-h">${escHtml(tr("Overview"))}` +
      ` <span class="sim-hint">${escHtml(trF("what the {n} builds here carry: how many carry each piece, and the best of them", { n }))}</span></h3>` +
      `<div class="fd-ov-body fold-b">${body}<p class="fd-hint">${escHtml(tr("click: must have · again: I don't have · again: clear"))}</p></div></div>`;
  };
  // ---- REFUSE A TRIGGER --------------------------------------------------------
  // Every trigger a build here asks for, grouped as the fight's own switches are;
  // a group refuses its members. Each says how many builds here carry it, and
  // names the parts that do.
  const inScopeAll = all.filter(inScope);
  const trigHtml = () => {
    const groups = buffTriggerGroups().map((g) => ({ ...g, shown: g.ids.filter((id) => off.has(id) || inScopeAll.some((p) => trigs.get(p).has(id))) }))
      .filter((g) => g.shown.length);
    if (!groups.length) return "";
    return `<div class="fd-trig"><span class="h">${escHtml(tr("refuse builds with a part triggered by"))}</span>` + groups.map((g) => {
      const on = g.ids.filter((id) => off.has(id)).length;
      return `<span class="fd-tg"><button type="button" class="g${on === g.ids.length ? " on" : on ? " some" : ""}" data-ftrgg="${escHtml(g.id)}" aria-pressed="${on === g.ids.length}">${
        escHtml(tr(BUFF_GROUP_NAME[g.id] || g.id))}</button>${g.shown.map((id) => {
        const carriers = inScopeAll.filter((p) => trigs.get(p).has(id));
        const names = [...new Set(carriers.flatMap((p) => trigs.get(p).get(id)))];
        return `<button type="button" class="${off.has(id) ? "on" : ""}" data-ftrg="${escHtml(id)}" aria-pressed="${off.has(id)}" title="${escHtml(names.join(" · "))}">${
          escHtml(tr(BUFF_TRIGGER_NAME[id] || id))}<em>${carriers.length}</em></button>`;
      }).join("")}</span>`;
    }).join("") + `</div>`;
  };
  const segBtn = (key, v, text, n, hint) => `<button type="button" data-fseg="${key}" data-v="${escHtml(v)}" class="${finder[key] === v ? "on" : ""}"${n ? "" : " disabled"}${hint ? ` title="${escHtml(hint)}"` : ""}>${escHtml(text)}<em>${n}</em></button>`;
  const modes = [...new Set(all.filter((p) => p.benchmark === finder.b).map((p) => p.mode))];
  const inMode = all.filter(inMo);

  box.innerHTML =
    `<div class="fd-head fold-h">${title}<small class="fd-count">${escHtml(trF("{n} of {m} builds", { n: list.length, m: scopeTotal })
      + (updated ? " · " + trF("updated {t}", { t: updated }) : ""))}</small></div>` +
    `<div class="fd-scope">` +
    benchSelectorHtml(finder.b) +
    `<div class="fd-seg"><span>${escHtml(tr("Mode"))}</span>${segBtn("mo", FINDER_EVERY_MODE, tr("All"), all.filter((p) => p.benchmark === finder.b).length)}${modes.map((m) => segBtn("mo", m,
      (all.find((p) => p.benchmark === finder.b && p.mode === m) || {}).modeName || m,
      all.filter((p) => p.benchmark === finder.b && p.mode === m).length)).join("")}</div>` +
    `<div class="fd-seg"><span>${escHtml(tr("Riven"))}</span>` +
    segBtn("rv", "all", tr("All"), inMode.length) +
    segBtn("rv", "riven", tr("With riven"), inMode.filter((p) => p.riven).length) +
    segBtn("rv", "plain", tr("Without riven"), inMode.filter((p) => !p.riven).length) + `</div>` +
    `<div class="fd-seg fd-depth"><span>${escHtml(tr("Depth"))}</span>${finderDepthHtml(w)}</div></div>` +
    `<div class="fd-conds"><div class="fd-q" role="search">` +
    [...finder.exc].map((t) => `<span class="fd-tok exc"><i>${escHtml(tr("I don't have"))}</i> <b>${escHtml(finderTokenName(t))}</b><button type="button" data-funtok="${escHtml(t)}" aria-label="${escHtml(tr("remove"))}">×</button></span>`).join("") +
    [...finder.req].map((t) => `<span class="fd-tok req"><i>${escHtml(tr("must have"))}</i> <b>${escHtml(finderTokenName(t))}</b><button type="button" data-funtok="${escHtml(t)}" aria-label="${escHtml(tr("remove"))}">×</button></span>`).join("") +
    `<input id="fd-input" type="text" autocomplete="off" placeholder="${escHtml(tr("what you don't have, or must have — mods, arcanes, evolutions, riven stats"))}">` +
    `<div class="fd-sugg" id="fd-sugg" hidden></div></div>${trigHtml()}</div>` +
    overviewHtml() + `<div class="fd-main">${listHtml()}</div>`;

  wireBenchSelector(box, (id) => {
    finder.b = id; finder.shown = FINDER_FIRST; finder.open = null; finder.ref = null;
    renderBuildFinder();
  });
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
        `<button type="button" class="n" data-fexc="${escHtml(t)}">${escHtml(tr("I don't have it"))}</button><button type="button" class="p" data-freq="${escHtml(t)}">${escHtml(tr("must have"))}</button></div>`).join("")
      : `<div class="fd-sg"><span class="nm">${escHtml(trF("nothing is called “{q}”", { q: input.value.trim() }))}</span></div>`;
    sugg.hidden = false;
  };
  input.addEventListener("input", () => { finder.hi = 0; drawSugg(); });
  input.addEventListener("keydown", (e) => {
    if (imeComposing(e)) return;
    const hs = hits();
    if (e.key === "ArrowDown") { finder.hi = Math.min(finder.hi + 1, hs.length - 1); drawSugg(); e.preventDefault(); }
    else if (e.key === "ArrowUp") { finder.hi = Math.max(finder.hi - 1, 0); drawSugg(); e.preventDefault(); }
    else if (e.key === "Enter" && hs[finder.hi]) { finder.exc.add(hs[finder.hi]); finder.shown = FINDER_FIRST; rerender(true); }
    else if (e.key === "Escape") sugg.hidden = true;
    else if (e.key === "Backspace" && !input.value) {
      const last = [...finder.req].pop() || [...finder.exc].pop();
      if (last) { finder.req.delete(last); finder.exc.delete(last); rerender(true); }
    }
  });
  // THE SLIDER says where it is while dragged and filters when let go: a
  // redraw on every step would drop the thumb out of the reader's hand.
  const setFinderDepth = (d) => {
    setBoardDepth(d);
    finder.open = null; finder.shown = FINDER_FIRST;
    // EVERY LIST THAT READS THE BOARD, not just this one: the build bar and
    // its chips are drawn from the same conversion.
    renderMods();
    return rerender();
  };
  const slider = box.querySelector("[data-fdepth-range]");
  if (slider) {
    slider.oninput = () => { const o = $("fd-depth-at"); if (o) o.innerHTML = finderDepthLabel(w, depthOfSlider(slider)); };
    slider.onchange = () => setFinderDepth(depthOfSlider(slider));
  }
  box.onclick = (e) => {
    const g = (sel) => e.target.closest(sel);
    let el;
    if ((el = g("[data-freq]"))) { finder.req.add(el.dataset.freq); finder.shown = FINDER_FIRST; return rerender(true); }
    if ((el = g("[data-fexc]"))) { finder.exc.add(el.dataset.fexc); finder.shown = FINDER_FIRST; return rerender(true); }
    if (!g(".fd-q")) sugg.hidden = true;
    if ((el = g("[data-funtok]"))) { finder.req.delete(el.dataset.funtok); finder.exc.delete(el.dataset.funtok); return rerender(); }
    if ((el = g("[data-fnohave]"))) { finder.exc.add(el.dataset.fnohave); finder.req.delete(el.dataset.fnohave); finder.shown = FINDER_FIRST; return rerender(); }
    if ((el = g("[data-fnoriven]"))) { finder.rv = "plain"; finder.shown = FINDER_FIRST; return rerender(); }
    if ((el = g("[data-fcyc]"))) {
      const t = el.dataset.fcyc;
      if (finder.req.has(t)) { finder.req.delete(t); finder.exc.add(t); }
      else if (finder.exc.has(t)) finder.exc.delete(t);
      else finder.req.add(t);
      finder.shown = FINDER_FIRST;
      return rerender();
    }
    if ((el = g("[data-fref]"))) { finder.ref = finder.ref === el.dataset.fref ? null : el.dataset.fref; return rerender(); }
    if ((el = g("[data-ftie]"))) {
      if (!finder.ties.delete(el.dataset.ftie)) finder.ties.add(el.dataset.ftie);
      return rerender();
    }
    if ((el = g("[data-ftrg]"))) {
      if (!off.delete(el.dataset.ftrg)) off.add(el.dataset.ftrg);
      setFinderOff(off); finder.shown = FINDER_FIRST;
      return rerender();
    }
    if ((el = g("[data-ftrgg]"))) {
      const ids = (buffTriggerGroups().find((x) => x.id === el.dataset.ftrgg) || { ids: [] }).ids;
      const full = ids.every((id) => off.has(id));
      ids.forEach((id) => (full ? off.delete(id) : off.add(id)));
      setFinderOff(off); finder.shown = FINDER_FIRST;
      return rerender();
    }
    if ((el = g("[data-fdepth]"))) return setFinderDepth(Number(el.dataset.fdepth));
    if ((el = g("[data-fseg]"))) {
      finder[el.dataset.fseg] = el.dataset.v;
      finder.open = null; finder.shown = FINDER_FIRST;
      if (el.dataset.fseg !== "rv") finder.rv = "all";
      return rerender();
    }
    if ((el = g("[data-fmore]"))) { finder.shown += FINDER_STEP; return rerender(); }
    if ((el = g("[data-fless]"))) { finder.shown = FINDER_FIRST; finder.open = null; return rerender(); }
    if ((el = g("[data-fopen]"))) {
      e.stopPropagation();
      const p = byId(el.dataset.fopen);
      if (!p) return;
      rememberPublished(buildBarCfg(), p);
      if (presetId(p) === activePreset) return renderPresetBar();
      return pickPreset(buildBarCfg(), presetId(p));
    }
    // SHARE IS OPEN, THEN THE BAR'S PANEL: what leaves is the open build.
    if ((el = g("[data-fshare]"))) {
      e.stopPropagation();
      const p = byId(el.dataset.fshare);
      if (!p) return;
      rememberPublished(buildBarCfg(), p);
      if (presetId(p) === activePreset) renderPresetBar();
      else pickPreset(buildBarCfg(), presetId(p));
      return openBuildShare("finder");
    }
    if (g("tr.fdet")) return;
    if ((el = g("tr.fr[data-frow]"))) { finder.open = finder.open === el.dataset.frow ? null : el.dataset.frow; return rerender(); }
  };
}

function renderPresetBar() {
  renderBuildFinder();
  renderPresetBarIn($("preset-bar-builder-builds"), buildBarCfg());
  renderShareBy();
}

// A scenario is the `sim` object, BUFF CONFIG INCLUDED.
//
// Buff ids are global — `arcane:primary_deadhead`, a mod's own buff — so a
// setting travels, and `sim.buffs` deliberately keeps entries for buffs the
// build does not currently carry: that is what lets a scenario say "in THIS
// fight, Deadhead starts at zero stacks" and have it hold when the mod is added
// later. Anything unmentioned takes the buff's own default, full and unlocked.
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
  HEADLESS_DEAD_SCENARIO_FIELDS.forEach((k) => { delete rest[k]; });
  return JSON.parse(JSON.stringify(rest));
}
function applyScenario(st) {
  // ONTO THE DEFAULTS, NEVER ONTO THE FIGHT YOU ARE LEAVING (`headlessScenarioOnto`).
  sim = headlessScenarioOnto(META, st, CONTACT_M, METRIC_FALLBACK.id);
  // A scenario preset is stored per weapon, so its weapon-scoped field
  // (headshot %) is already right — stamp the marker so the re-seed does not
  // overwrite a saved choice with a default.
  sim.__weapon = $("weapon").value;
  // A CUSTOM TARGET THIS BROWSER DOES NOT HOLD is held, not dropped
  // (`holdAbsent`): the roster's first unit stands in until it syncs.
  releaseAbsent(SCENARIOS);
  const stand = ((META.enemies || [])[0] || {}).id || "thrax_centurion";
  const absent = (id) => typeof id === "string" && id.startsWith(enemyId("")) && !enemyCard(id);
  if (absent(sim.enemy)) { holdAbsent(SCENARIOS, activeScenario, ["enemy"], sim.enemy, stand); sim.enemy = stand; }
  (sim.formation || []).forEach((f, i) => {
    if (absent(f.enemy)) { holdAbsent(SCENARIOS, activeScenario, ["formation", i, "enemy"], f.enemy, stand); f.enemy = stand; }
  });
  if (absentOf(SCENARIOS, activeScenario).length) {
    noteInline(tr("This fight's custom target is not on this browser. It is kept, and the fight here is against {name} until it syncs.")
      .replace("{name}", (enemyCard(stand) || {}).name || stand));
  }
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
  renderOptFight();
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
}

