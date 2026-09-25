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
// `board::builds::canonical_mods` normalises only the freedoms that are provably free
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
/// question asked of them is the same one.
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
  // `rules::elements::combine`'s innate rules, and it would be wrong the first time a
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

  // BY THE SCENARIO'S OWN METRIC, whatever it is. `per_minute` is the question
  // that decides which baseline is read — a rate over the engagement against a
  // field the run reports directly — and it is the metric's to answer.
  const useKills = metricOf(scenario.metric).per_minute;
  const read = (r) => readGain(r, useKills)?.v ?? null;
  const seOf = (r) => readGain(r, useKills)?.se ?? 0;
  // The runs behind each number, so the winner of a candidate set pairs against
  // the winner of the reference set rather than being compared to it as an
  // independent sample — same seed, same run order, so run `i` is run `i`.
  const runsOf = (r) => readGain(r, useKills)?.runs ?? [];
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
  const runsAt = orders.map((os) => os.map(() => []));
  let cursor = 0;
  await Promise.all((await gainLanes()).map(async (lane) => {
    for (;;) {
      if (!live()) return;
      const j = jobs[cursor++];
      if (!j) return;
      const r = await laneAsk(lane, "/api/simulate",
        { ...base, ...scenario, mods: j.mods, ...j.override }, live);
      if (!live()) return;
      if (r === null) return;            // the pool was taken twice — stand down
      got[j.si][j.oi] = read(r);
      ses[j.si][j.oi] = seOf(r);
      runsAt[j.si][j.oi] = runsOf(r);
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
      const bi = got[0].indexOf(b);
      const o = orders[i + 1][vi] || {};
      // `drops` already decided which side is which; the uncertainty is the
      // same either way, so it is built from the two measurements as they sit.
      const [seWith, seWithout] = c.drops ? [bSe, ses[i + 1][vi]] : [ses[i + 1][vi], bSe];
      const [rWith, rWithout] = c.drops
        ? [runsAt[0][bi], runsAt[i + 1][vi]]
        : [runsAt[i + 1][vi], runsAt[0][bi]];
      optGain.by[c.id] = {
        ...gainOver({ v: withX, se: seWith, runs: rWith },
                    { v: without, se: seWithout, runs: rWithout }),
        runs: scenario.runs,
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
/// negative for a reason the row does not state, which reads as adding status
/// chance LOWERING the damage.
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

/// **ONE CONTROL FOR EVERY QUANTIFIABLE AXIS** — a CARD saying what is
/// installed, with a ⋯ in its corner that opens the RANKED list.
///
/// The arcane slot has been this shape all along and is the model: a card while
/// something is in it, a `+ add` plate while nothing is, and a menu offering
/// Swap and Remove. The parts, the valence element and the evolution tiers are
/// the same question — pick one of N, each worth a measurable amount on THIS
/// build — so they are the same control.
///
/// **WHETHER THE AXIS CAN BE EMPTY IS ONE MENU ITEM, NOT A SECOND SHAPE.**
/// `Remove` appears where nothing-installed is a real state (an arcane, an
/// evolution tier) and does not where it is not: a Kitgun has a grip, and every
/// copy of a Lich weapon carries an element. Offering it there would be a verb
/// that can never be used, which is the "always drawn and disabled" noise this
/// page already refuses — and hiding it costs no consistency, because the
/// consistency a reader actually reads is the CARD and the ranked list behind
/// it, not the length of a menu they have to open to see.
///
/// It registers with `ddReg` and opens through `ddOpen`, so the list, its
/// search and its ordering are the dropdown's own — only the TRIGGER differs.
function rankedSlot(id, cfg) {
  ddReg.set(id, {
    value: cfg.value,
    items: rankedItems(cfg),
    search: cfg.items.length > 8,
    // WHICH AXIS THIS LIST RANKS — what `openRanked` measures, what the strip
    // inside the list reports on, and what tells `ddRender` to rank its rows.
    axis: cfg.axis,
    axisLabel: cfg.label,
    onPick: cfg.onPick,
  });
  const card = cfg.card;
  if (!card) {
    // NOTHING INSTALLED. A locked tier says why instead of inviting a click it
    // would refuse.
    return `<div class="slot empty axis${cfg.locked ? " axlocked" : ""}" data-slot="${id}"${
      cfg.lockedWhy ? ` title="${escHtml(cfg.lockedWhy)}"` : ""}>
      <span class="axl">${escHtml(cfg.label)}</span>
      <span class="plus">${cfg.locked ? escHtml(cfg.lockedWhy || "") : "+ " + escHtml(cfg.addLabel || cfg.label)}</span>
    </div>`;
  }
  const icon = card.icon ? `<img class="eicon" src="${IMG(card.icon)}" alt="">` : "";
  const lines = (card.lines || []).map((x) => `<div>${escHtml(x)}</div>`).join("");
  return `<div class="slot filled axis${card.broken ? " broken" : ""}" data-slot="${id}" data-id="${
    escHtml(String(cfg.value))}"${card.title ? ` title="${escHtml(card.title)}"` : ""}>
    <span class="axl">${escHtml(cfg.label)}</span>
    ${icon}<div class="info"><div class="mn">${card.href ? wl(card.name, card.href) : escHtml(card.name)}${
      card.chips || ""}</div><div class="ed">${lines}</div>${card.notes || ""}</div>
    <button class="dots" title="${escHtml(tr("options"))}">⋯</button>
  </div>`;
}

/// The rows a `rankedSlot` offers, ordered by gain.
function rankedItems(cfg) {
  // NEITHER RANKED NOR CHIPPED HERE. Both are read from the SCAN, which has
  // not started when a list is registered — `ddRender` does them, every time
  // it draws.
  return cfg.items.map((it) => ({
    value: it.value,
    label: it.label,
    hint: it.hint,
    disabled: it.disabled,
    key: it.key,
    badge: it.extra || "",
  }));
}

/// OPEN A RANKED LIST BY NAME — what a slot's ⋯ "Swap" and an empty plate
/// reach for, neither of which has a trigger element to delegate off.
/// Measuring is `ddOpen`'s, so every list that declares an axis is scanned
/// whichever control opened it.
function openRanked(id, anchor) {
  if (ddReg.get(id)) ddOpen(id, anchor);
}

/// The ⋯ menu a `rankedSlot` opens — Swap, and Remove where the axis has one.
function openSlotMenu(anchor, id, cfg) {
  closePopovers();
  const menu = $("slot-menu");
  const swap = `<div class="mi" data-a="swap">${escHtml(tr("Swap") + " " + cfg.label)}</div>`;
  const rm = cfg.removable
    ? `<div class="mi danger" data-a="remove">${escHtml(tr("Remove") + " " + cfg.label)}</div>` : "";
  menu.innerHTML = swap + rm;
  place(menu, anchor);
  menu.querySelector('[data-a="swap"]')
    .addEventListener("click", () => (cfg.onSwap ? cfg.onSwap() : openRanked(id, anchor)));
  const r = menu.querySelector('[data-a="remove"]');
  if (r) r.addEventListener("click", () => { closePopovers(); cfg.onPick(""); });
}

/// WIRE A BLOCK'S SLOTS, once its markup is in the DOM. One call per block
/// rather than per slot: the cards are re-rendered wholesale on every change,
/// so a listener bound to a card dies with the card that carried it — the
/// arena's own lesson.
function bindRankedSlots(box, cfgs) {
  box.querySelectorAll(".slot.axis").forEach((el) => {
    const id = el.dataset.slot;
    const cfg = cfgs[id];
    if (!cfg || cfg.locked) return;
    const dots = el.querySelector(".dots");
    if (dots) {
      // THE MENU HANGS OFF THE ⋯, NOT OFF THE CARD. `place` puts a popover's
      // top-left under its anchor's bottom-left, so anchoring to the whole card
      // dropped the menu at the card's BOTTOM-LEFT while a mod slot's — the one
      // that passes its own button — came out under the ⋯ at the top right.
      // Same control, same gesture, two different places on the page.
      dots.addEventListener("click", (e) => { e.stopPropagation(); openSlotMenu(e.currentTarget, id, cfg); });
    } else {
      // An EMPTY slot opens the list on a click anywhere, which is the mod and
      // arcane slots' own rule: there is nothing in it to select, so the whole
      // plate is the button.
      el.addEventListener("click", () => openRanked(id, el));
    }
  });
}

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
  // something about the simulator rather than about their build.
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
const gainOf = (id) => (gainAbout() === gainKey() ? gainScan.by[id] || null : null);

/// QUICK CALC — page level, above the mods.
///
/// It is ONE configuration for every slot's question, so it does not live
/// inside any slot's picker. Two settings: the SCENARIO (a
/// saved one, which also decides KPM-or-DPS) and how many runs. There is no
/// run button because there is no slot here to run against — opening a
/// picker computes with these, which is what "sorted by effect by default"
/// means in practice.
function renderQuickCalc() {
  const box = $("quick-calc");
  if (!box) return;
  const on = gainPrefs.on !== false;
  box.innerHTML =
    `<label class="pc-h" title="${escHtml(tr("rank a slot's options by what they would change — off, nothing is simulated"))}">` +
    `<input type="checkbox" id="gp-on"${on ? " checked" : ""}> ⚡ ${escHtml(tr("Quick calc"))}</label>` +
    (!on ? "" :

    // WHICH FIGHT, stated and not chosen. It is the simulator's, always — see
    // `gainScenario`. Naming it here is the whole reason the old picker could
    // lie for a session without being noticed: a reader could not see which
    // fight the numbers beside every mod had come from.
    `<span class="pc-scen" id="gp-scen" title="${escHtml(tr("the fight these numbers are measured in — the quick calc runs the Simulator's own scenario, so switch it there and every chip follows"))}">${escHtml(gainScenario().name)}</span>` +

    // HOW MANY RUNS a chip's number is averaged over. Ten is the floor, not a
    // suggestion: under it a status mod's chip is a coin flip. It is clamped on
    // the way out (`gainRuns`) as well as here, because a number input accepts
    // an empty string and a paste.
    `<label class="pc-runs" title="${escHtml(tr("how many simulations each option is averaged over — 10 is the floor, and more costs proportionally more time"))}">` +
    `<input type="number" id="gp-runs" min="${GAIN_RUNS_MIN}" max="${GAIN_RUNS_MAX}" step="10" value="${gainRuns()}">` +
    `<span>${escHtml(tr("runs"))}</span></label>` +

    // WHICH CARDS THE LISTS OFFER AT EVERY RANK, and not only at max.
    `<button class="ghost-btn small" id="gp-ranks" aria-expanded="${everyRankOpen}" title="${escHtml(tr("cards every list offers at each of their ranks, not only at max — a card whose lower rank can beat its max, like Status Duration beside a malus"))}">${
      escHtml(tr("every rank"))} · ${everyRank().mods.length + everyRank().arcanes.length}</button>` +

    // PROGRESS while it runs, and an invitation before it has. The run counts
    // ("1x -> 10x") are gone from here: they were a property
    // of the algorithm back when there were two passes at two precisions. The
    // count above is the reader's own, and every chip still carries the one its
    // number came from.
    `<span class="pc-note">${gainScan.running
      ? `${gainScan.done}/${gainScan.total}`
      : (gainScan.note ? "" : escHtml(tr("open a slot to rank its mods by effect")))}</span>` +
    (everyRankOpen ? everyRankPanel() : ""));
  // Every click stays inside: a redraw detaches these nodes, and the document
  // outside-click handler closes on a target whose `.popover` ancestor is gone.
  box.onclick = (e) => e.stopPropagation();
  $("gp-on").onchange = (e) => {
    e.stopPropagation();
    gainPrefs = { ...gainPrefs, on: $("gp-on").checked };
    // A stale ranking must not outlive the switch, and "提升" must not stay
    // selected with nothing behind it.
    if (!gainPrefs.on) {
      // OFF MEANS STOP, not "stop asking for more". A scan already in flight
      // kept its workers and its right to write, so switching off and back on —
      // the first thing anyone does to something that looks stuck — resumed
      // into the state it was stuck in. Now it is the rebuild button.
      restartCalc();
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
  const rb = $("gp-ranks");
  if (rb) rb.onclick = () => { everyRankOpen = !everyRankOpen; renderQuickCalc(); };
  box.querySelectorAll(".rk-x").forEach((b) => { b.onclick = () => {
    const cur = everyRank();
    setEveryRank({ ...cur, [b.dataset.k]: cur[b.dataset.k].filter((x) => x !== b.dataset.id) });
    if (!$("rank-popover").hidden) renderEveryRankMenu($("rank-search").value);
  }; });
  const add = $("gp-ranks-add");
  if (add) add.onclick = () => openEveryRankPicker(add);
  const all = $("gp-ranks-all");
  if (all) all.onclick = () => { everyRankAll = !everyRankAll; renderQuickCalc(); };
  const reset = $("gp-ranks-reset");
  if (reset) reset.onclick = () => setEveryRank(null);
}

/// THE EVERY-RANK LIST, EDITED IN PLACE — by card, never by effect. `null` is
/// the published default (`every_rank` in /api/meta). A list is a new question,
/// so the open scan re-asks.
function setEveryRank(next) {
  gainPrefs = { ...gainPrefs, everyRank: next || undefined };
  if (!next) delete gainPrefs.everyRank;
  saveGainPrefs();
  renderQuickCalc();
  if (!$("mod-popover").hidden) renderMenu(pickerSlot, $("mod-search").value);
  if (!$("arcane-popover").hidden) renderArcaneMenu($("arcane-search").value);
  refreshGains();
}

/// ONE CARD OF THE LIST, as the picker draws it — the mod or arcane row the
/// slot pickers use, so this list reads as the same cards and not as labels.
const everyRankRow = (k, card, opts = {}) => (k === "mods"
  ? modRow(card, { ...opts, attrs: `data-id="${card.id}" ${opts.attrs || ""}` })
  : arcaneRow(card, opts));
const everyRankCard = (k, id) => (k === "mods"
  ? modById(id) || (META.mods || []).find((m) => m.id === id)
  : arcaneById(id));
/// THE LIST IS ONE LIST FOR EVERY WEAPON; the panel shows this weapon's part
/// of it, and `everyRankAll` shows the rest — kept, and applied on the weapons
/// that take them.
let everyRankAll = false;
const everyRankHere = (k, id) => (k === "mods" ? !!modById(id) : arcaneFitsWeapon($("weapon").value, id));

/// Every card that could go on the list: this weapon's ranked mods and its
/// seats' ranked arcanes — or, with the whole list open, every ranked card.
function everyRankOffers() {
  if (everyRankAll) {
    return [
      ...(META.mods || []).filter((m) => !m.riven && !m.stance && m.max_rank > 0).map((m) => ["mods", m]),
      ...(META.arcanes || []).filter((a) => a.id !== "none" && (a.max_rank || 0) > 0).map((a) => ["arcanes", a]),
    ];
  }
  const arcs = [...new Map(arcanePools().flatMap((_, i) => arcanePool(i)).map((a) => [a.id, a])).values()];
  return [
    ...currentPool.filter((m) => !m.riven && m.max_rank > 0).map((m) => ["mods", m]),
    ...arcs.filter((a) => (a.max_rank || 0) > 0).map((a) => ["arcanes", a]),
  ];
}

/// The editor's body: the list as picker rows, each removable, the button
/// that opens the picker, and the way back to the default.
function everyRankPanel() {
  const list = everyRank();
  const cards = ["mods", "arcanes"].flatMap((k) => list[k].map((id) => [k, everyRankCard(k, id)]))
    .filter(([, c]) => c);
  const row = ([k, c]) => everyRankRow(k, c, {
    trailing: `<button class="rk-x" data-k="${k}" data-id="${escHtml(c.id)}" title="${escHtml(tr("remove"))}">×</button>`,
  });
  const here = cards.filter(([k, c]) => everyRankHere(k, c.id));
  const elsewhere = cards.filter(([k, c]) => !everyRankHere(k, c.id));
  return `<div class="pc-ranks">` +
    (here.length ? `<div class="combo-menu pc-rank-list">${here.map(row).join("")}</div>`
      : `<span class="pc-note">${escHtml(tr("every card at max rank only"))}</span>`) +
    (everyRankAll && elsewhere.length
      ? `<div class="pc-note">${escHtml(tr("on the list for other weapons"))}</div><div class="combo-menu pc-rank-list">${elsewhere.map(row).join("")}</div>`
      : "") +
    `<button class="ghost-btn small" id="gp-ranks-all">${escHtml(everyRankAll
      ? tr("this weapon's cards only")
      : tr("the whole list ({n} more for other weapons)").replace("{n}", elsewhere.length))}</button>` +
    `<button class="ghost-btn small" id="gp-ranks-add">+ ${escHtml(tr("add a card"))}</button>` +
    (gainPrefs.everyRank ? `<button class="ghost-btn small" id="gp-ranks-reset">${escHtml(tr("default list"))}</button>` : "") +
    `</div>`;
}

/// THE LIST'S PICKER — its own popover, like the riven stat picker, because a
/// slot picker's clicks EQUIP. A click here toggles membership and the picker
/// stays open, so several cards are one visit.
function openEveryRankPicker(anchor) {
  closePopovers();
  const pop = $("rank-popover");
  place(pop, anchor);
  const search = $("rank-search");
  search.value = "";
  search.oninput = () => renderEveryRankMenu(search.value);
  renderEveryRankMenu("");
  search.focus();
}

function renderEveryRankMenu(query) {
  const menu = $("rank-menu");
  const q = query.trim().toLowerCase();
  const list = everyRank();
  const on = ([k, c]) => list[k].includes(c.id);
  const hits = everyRankOffers()
    .filter(([k, c]) => !q || (k === "mods" ? searchHit(c, q) : searchBlob(c).includes(q)))
    // Mods before arcanes, the listed cards first in each, then by name.
    .sort((a, b) => (a[0] === b[0] ? 0 : a[0] === "mods" ? -1 : 1)
      || (on(b) - on(a))
      || String(a[1].name).localeCompare(String(b[1].name)));
  menu.innerHTML = hits.length
    ? sectionedRows(hits, ([k]) => (k === "mods" ? "Mods" : "Arcanes"), ([k, c]) => everyRankRow(k, c, {
      cls: on([k, c]) ? "cur" : "",
      attrs: `data-k="${k}"`,
      chips: on([k, c]) ? ` <span class="slotchip cur">${escHtml(tr("every rank"))}</span>` : "",
    }))
    : `<div class="opt dis">${escHtml(tr("no matches"))}</div>`;
  menu.querySelectorAll(".opt:not(.dis)").forEach((o) => o.addEventListener("click", (e) => {
    // The redraw below detaches this row; the document handler would then
    // read the click as outside every popover and close this one.
    e.stopPropagation();
    if (e.target.closest("a")) return;
    const { k, id } = o.dataset;
    const cur = everyRank();
    setEveryRank({ ...cur, [k]: cur[k].includes(id) ? cur[k].filter((x) => x !== id) : [...cur[k], id] });
    renderEveryRankMenu($("rank-search").value);
  }));
}

/// WHAT THE CALCULATOR IS DOING, AND WHAT IS LEFT OF IT.
///
/// THE POOL'S HEALTH WAS INVISIBLE. A lane dies quietly — the module did not
/// download, the renderer reclaimed the worker, a stop abandoned the pool — and
/// nothing on the page said so, so a scan that lost its workers looked exactly
/// like one that was slow. The reader's only signal was that numbers stopped
/// appearing, and their only move was a reload that rebuilt the same pool the
/// same way.
///
/// IT IS COLLAPSED UNTIL IT HAS SOMETHING TO SAY. A panel that is always there
/// is furniture; this one opens while a scan is running and stays open after
/// one that failed, which are the two moments a reader wants it.
/// OPEN UNTIL SOMEBODY CLOSES IT. A tab is what a reader does not notice, and
/// not noticing is the whole failure this panel exists to end — the report was
/// "the quick calc stopped and there is no indication of anything". Collapsing
/// is remembered, so the reader who does not want it says so once.
let calcStatusOpen = localStorage.getItem("wfsim-calc-status") !== "closed";
/// Opened while there is NOTHING to report — the reader going looking for the
/// rebuild button. Not persisted: an idle panel left open for ever is the
/// clutter collapsing exists to avoid.
let calcStatusPeek = false;
function renderCalcStatus() {
  const box = $("calc-status");
  if (!box) return;
  const made = pool.filter(Boolean);
  const alive = made.filter((l) => !l.dead).length;
  const lost = made.length - alive;
  const busy = gainScan.running;
  // NEVER HIDDEN, because the way out lives in here.
  //
  // A panel that disappears whenever there is nothing to report puts the
  // rebuild button behind the very condition it exists for: a calculator wedged
  // in a state that is neither busy nor failed shows no surface at all, and the
  // reader whose list will not produce numbers has a reload and nothing else.
  // IDLE COLLAPSES TO THE TAB instead — the panel is one click away at all
  // times and costs a tab's worth of corner when quiet.
  box.hidden = false;
  const idle = !busy && !lost && !gainScan.failed;
  const open = idle ? calcStatusPeek : calcStatusOpen;
  // THE PHASE, WHERE THERE IS ONE. `0/77` is not a stall — it is the BASELINE
  // being measured, which every candidate is compared against and which runs
  // before any of them. It can be two of them, when nothing died under the
  // ruler's own metric and the question falls back to DPS. A counter that only
  // counts candidates has nothing to say through either, and the reader reads
  // the silence as a hang.
  // HOW LONG THIS WILL TAKE, from what the baseline actually cost — shown only
  // while it is still true, and only when it is long enough to be worth
  // reading. A number under a couple of seconds is noise a reader has to
  // dismiss; a minute is the one they wanted before they started.
  const left = busy && gainScan.etaMs ? Math.round(gainScan.etaMs / 1000) : 0;
  const head = busy
    ? (gainScan.phase
        ? escHtml(gainScan.phase)
        : `${escHtml(tr("calculating"))} ${gainScan.done}/${gainScan.total}`)
    : (gainScan.failed ? escHtml(tr("the last calculation did not finish"))
                       : escHtml(tr("calculator")));
  if (!open) {
    box.className = `calc-status${gainScan.failed ? " bad" : ""}`;
    box.innerHTML = `<button class="cs-tab" id="cs-tab">⚡ ${head}${
      lost ? ` <span class="cs-lost">${lost}</span>` : ""}</button>`;
  } else {
    box.className = `calc-status open${gainScan.failed ? " bad" : ""}`;
    box.innerHTML =
      `<button class="cs-tab" id="cs-tab">⚡ ${head} ▾</button>` +
      `<div class="cs-body">` +
      // WHICH FIGHT AND WHICH LIST, because "calculating" alone does not say
      // whether it is the thing you are looking at.
      `<div class="cs-row"><span>${escHtml(tr("fight"))}</span><b>${
        escHtml(gainScenario().name)}</b></div>` +
      `<div class="cs-row"><span>${escHtml(tr("measuring"))}</span><b>${
        escHtml(gainScan.note && !gainScan.failed ? gainScan.note : "—")}</b></div>` +
      // WHAT STEP, AND HOW FAR THROUGH IT. Two facts, because the counter is
      // only meaningful during the candidate pass and the phase is only
      // meaningful outside it.
      `<div class="cs-row"><span>${escHtml(tr("step"))}</span><b>${
        escHtml(gainScan.phase || tr("ranking the options"))}</b></div>` +
      `<div class="cs-row"><span>${escHtml(tr("ranked"))}</span><b>${
        gainScan.done} / ${gainScan.total}</b></div>` +
      // THE ONE FACT THAT WAS MISSING. Workers alive against workers made: a
      // pool that has lost half of itself is the difference between slow and
      // broken, and nothing said it.
      `<div class="cs-row"><span>${escHtml(tr("workers"))}</span><b>${alive} / ${
        made.length || poolSize()}${lost ? ` — ${lost} ${escHtml(tr("lost"))}` : ""}</b></div>` +
      (left > 2
        ? `<div class="cs-row"><span>${escHtml(tr("about"))}</span><b>${
          escHtml(tr("{s}s left").replace("{s}", left))}</b></div>`
        : "") +
      (gainScan.failed
        ? `<div class="cs-why">${escHtml(gainScan.note || tr("unknown"))}</div>` : "") +
      // A WAY OUT THAT IS NOT A RELOAD. A reload rebuilds the same pool the
      // same way; this drops the dead workers so the next question opens fresh
      // ones, which is the move a reader could not previously make at all.
      `<button class="ghost-btn small" id="cs-reset">${
        escHtml(tr("rebuild the calculator"))}</button>` +
      `</div>`;
  }
  const tab = $("cs-tab");
  if (tab) tab.onclick = () => {
    if (idle) { calcStatusPeek = !calcStatusPeek; renderCalcStatus(); return; }
    calcStatusOpen = !calcStatusOpen;
    try {
      localStorage.setItem("wfsim-calc-status", calcStatusOpen ? "open" : "closed");
    } catch (_) { /* a browser with storage off still gets the panel */ }
    renderCalcStatus();
  };
  const reset = $("cs-reset");
  if (reset) reset.onclick = () => {
    restartCalc();
    calcStatusPeek = false;
    renderCalcStatus();
    refreshGains();
  };
}

/// STOP EVERYTHING THE QUICK CALC IS DOING, and leave nothing that can write.
///
/// THE GENERATION IS BUMPED FIRST, before a worker is touched. A scan in flight
/// is `live()` until something bumps it, so a restart that only replaced the
/// state left the old scan free to finish into the NEW one and stamp its key —
/// filing a ranking of a fight nobody is looking at as complete, which is worse
/// than the stall it was asked to end.
///
/// BOTH WAYS OUT ARE THIS ONE FUNCTION: the rebuild button, and switching the
/// quick calc off. A reader flicks that switch when it looks stuck, so it has
/// to mean what the button means — anything less makes the obvious gesture the
/// one that does not work.
function restartCalc() {
  gainGen += 1;
  gainPending = null;
  resetPool();
  gainScan = { key: null, want: null, running: false, base: 0, floor: 0, by: {},
    done: 0, total: 0, note: "", metric: "", failed: false, lanesLost: 0 };
}

/// DROP EVERY WORKER, so the next question builds fresh ones.
///
/// A dead lane is replaced on demand now, so this is not the recovery path — it
/// is the reader's own, for the case nobody predicted. `abandon` first, because
/// a worker holding a live request has waiters that must be settled rather than
/// left hanging on a lane nobody will read again.
function resetPool() {
  for (const l of pool) {
    if (!l) continue;
    try { l.abandon(); } catch (_) { /* already gone */ }
    try { l.worker.terminate(); } catch (_) { /* already gone */ }
  }
  pool = [];
  gainPool = null;
}

/// Re-run whatever quick-calc surface is on screen. Called after ANY scenario
/// edit: the scan is measured under the scenario, so a change to it makes the
/// numbers on screen answers to a question nobody is asking any more.
///
/// It is not a repaint — `ensureGains` compares the key first, so a change the
/// chosen scenario does not care about costs nothing here. Evolution rows scan
/// without being opened, so they always refresh; the pickers only when open.
function refreshGains() {
  // The BOX first and unconditionally — it names the fight being measured, so
  // it has to follow a switch even when the scan itself is off.
  renderQuickCalc();
  if (gainPrefs.on === false) return;
  // **AN OPEN PICKER RE-ASKS; A SHUT ONE HAS NOTHING TO RE-ASK**. Every ranked axis now measures when its list is OPENED, so an
  // open list is the only place an answer exists that a fight change can make
  // stale — and all three kinds of list are treated the same way here, which
  // is the point.
  //
  // IT WAS A REPAINT FOR TWO OF THEM. The mod and arcane pickers were
  // re-rendered without being re-measured, and the freshness check passed
  // anyway because this function ended in `renderEvo()` — which asked for an
  // EVOLUTION scan, which made `gainScan.key` catch up with the fight while the
  // open mod picker's own chips still answered the old one. A check passing for
  // the wrong reason is what hid it.
  if ($("mod-popover") && !$("mod-popover").hidden) {
    renderTools();
    ensureGains({ kind: "mods", idx: pickerSlot },
      () => { if (!$("mod-popover").hidden) renderMenu(pickerSlot, $("mod-search").value); });
  }
  if ($("arcane-popover") && !$("arcane-popover").hidden) {
    ensureGains({ kind: "arcane", idx: arcaneSlotIdx },
      () => { if (!$("arcane-popover").hidden) renderArcaneMenu($("arcane-search").value); });
  }
  const dd = $("dd-popover");
  if (dd && !dd.hidden && dd._anchor) {
    const id = dd._anchor.dataset.slot || dd._anchor.dataset.dd;
    if (id && (ddReg.get(id) || {}).axis) openRanked(id, dd._anchor);
  }
  renderMode();
}

/// Compute this axis position's ranking, unless it is already on screen.
/// `gainKey` covers the axis, the build, the scenario and the settings, so
/// re-opening the same picker costs nothing and any edit invalidates it.
/// `user` — a person opened this list, rather than a repaint asking again for
/// one already on screen. It is the difference between preempting and queueing;
/// see the comment on the queue below.
function ensureGains(axis, repaint, user) {
  // Nothing to measure against yet. The evolution rows scan without being
  // opened, so on a cold load they can fire before `initPresets` has seeded
  // the scenario library — and a scan with no named scenario is one nobody
  // can reproduce or compare against (it labelled itself "—").
  if (gainPrefs.on === false) return;
  if (!scenarioList().length) return;
  gainAxis = axis;                       // so the key describes what we want
  // Is what we have MEASURED the fight on screen now? The key is stamped on
  // completion, so this means answered and nothing else — a scan that died
  // half way leaves it null and the next request re-asks, which is the whole
  // point of moving it there.
  if (gainScan.key === gainKey()) return;
  // …AND IS ONE ALREADY IN FLIGHT FOR IT? A separate question from the line
  // above, and it has to be: one check covering both cannot tell "answering"
  // from "answered", and a scan that never finishes is then filed for ever as
  // one that did.
  if (scanIsLive() && gainScan.want === gainKey()
      && JSON.stringify(gainScan.axis) === JSON.stringify(axis)) return;
  // A running scan is STALE and gives way — but only to its own axis. The mod
  // picker and the evolution rows both ask on every refresh (`refreshGains`
  // ends in `renderEvo`), so "the newest request wins" makes the two cancel
  // each other on every repaint and neither ever finishes. Which axis is asking
  // is not a staleness signal; the fight moving is.
  // A PERSON ASKING PREEMPTS; A REPAINT WAITS ITS TURN.
  //
  // The two are not the same request and were treated as one. Opening a slot is
  // a decision — the reader has stopped caring about the list they were looking
  // at — and queueing it behind a scan that is 87 melee engagements deep means
  // minutes before the list they just opened says anything, which reads as the
  // calculator being stuck. A REPAINT is not a decision: the mod picker and the
  // evolution rows both re-ask on every refresh, and letting those preempt each
  // other is what made the two cancel each other for ever and neither finish.
  //
  // So the queue stays, for the case it was built for, and a user request goes
  // straight past it. `scanGains` bumps the generation, so whatever was running
  // stands down at its next await — bounded to one outstanding simulation per
  // lane rather than the whole queue.
  if (user && scanIsLive()) gainPending = null;
  if (!user && scanIsLive()
      && JSON.stringify(gainScan.axis) !== JSON.stringify(axis)) {
    // …BUT IT IS NOT FORGOTTEN. Dropping it silently is what made a player have
    // to click between two evolutions until the numbers appeared (report,
    // 2026-08-13): the evolution rows ask on EVERY refresh while a picker asks
    // only while it is open, so with a picker open the evolution request was
    // dropped and nothing ever re-asked — the running scan's completion
    // repaints the caller that started it, which is the picker, not the rows.
    gainPending = { axis, repaint };
    return;
  }
  // A REJECTION HERE STOPPED THE CALCULATOR FOR GOOD. This is launched without
  // `await`, so anything that throws inside would otherwise leave `running`
  // true and the counter frozen. The scan is handed the LIST'S REPAINT and
  // draws nothing itself — `paintCalc` owns every surface, including the queue
  // taking its turn, so a new exit path cannot forget one of them.
  scanGains(axis, repaint).catch((e) => {
    gainStop(`${tr("the calculator stopped")}: ${(e && e.message) || e}`);
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
// line. A box per section is the whole fix: each heading
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

/// ONE MOD ROW, drawn by the builder's picker and by the optimizer's scope.
///
/// Both modules show the same card — polarity, image, the wiki-linked name,
/// the EXILUS chip, the effect lines — and the only thing that differs is the
/// TRAILING control, which is the whole difference between the modules: the
/// builder binds a VALUE (the drain of the card it is about to seat) where the
/// optimizer binds a SET (pool / req). Stating that once here is what makes a
/// change to the card reach both tabs.
///
/// It was two copies, with `// The picker's .opt row markup verbatim` written
/// over the second — a comment that stops being true in silence, which is
/// exactly what it did: the optimizer's copy never grew the builder's stance
/// filter, so a melee weapon offered its stances as MAIN-slot marks.
/// `exilusChip` is off in the EXILUS SLOT's own list, where every row is
/// exilus-eligible and the chip is the heading repeated once per card.
const modRow = (m, { cls = "", title = "", attrs = "", chips = "", note = "",
  trailing = "", exilusChip = true, rank = m.max_rank } = {}) =>
  `<div class="opt ${cls} ${m.rarity ? "rar-" + m.rarity : ""}" ${attrs} title="${title}">
      ${imgTag(POL(m.polarity), "pol")}${imgTag(IMG(m.image), "mod")}
      <div class="info"><div class="mn">${
    wl(m.name, modWikiUrl(m))}${modMarketLink(m)}${
    exilusChip && m.exilus ? ' <span class="exchip">EXILUS</span>' : ""}${chips}</div><div class="me">${
    cardLines(m, rank).map((x) => `<div>${x}</div>`).join("")}</div>${note}</div>${trailing}</div>`;

/// THE SCOPE CONTROL — the optimizer's trailing half of the row above.
///
/// `pool` = an option the search may take, `req` = pinned into every candidate.
/// Six lists carry it (mods, exilus, modes, valence, arcanes, evolution tiers)
/// and it was written out six times; the ATTRIBUTE differs per list because
/// each one's click handler reads its own (`data-m`, `data-a`, `data-t`+`data-e`),
/// which is why that is the parameter and the rest is not.
const oseg = (attrs, st, { poolDead = false, reqDead = false, poolTitle = "",
  reqTitle = "", extra = "" } = {}) =>
  `<div class="oseg">
        <span class="seg ${st === "search" ? "on" : ""} ${poolDead ? "dis" : ""} ${extra}" ${attrs} data-s="search"${
    poolTitle ? ` title="${escHtml(poolTitle)}"` : ""}>${tr("pool")}</span>
        <span class="seg ${st === "fixed" ? "on" : ""} ${reqDead ? "dis" : ""} ${extra}" ${attrs} data-s="fixed"${
    reqTitle ? ` title="${escHtml(reqTitle)}"` : ""}>${tr("req")}</span>
      </div>`;

function renderMenu(slotIdx, query) {
  rivenPickerSlot = slotIdx;
  const menu = $("mod-menu");
  const q = query.trim().toLowerCase();
  // Equipped mods stay LISTED: the current slot's mod is marked, mods in other
  // slots show their slot number — picking one of those EXCHANGES the two slots.
  // ONE group ahead of the rule: this slot's own mod, because it is the
  // baseline every number below is measured against. Everything else obeys
  // the sort — including mods sitting in OTHER slots, which are not pinned
  // into a band of their own. Such a band contradicts whichever order is
  // chosen (eight rows of unsorted drain at the top of a drain sort), and it
  // carries nothing: every placed mod already has a "slot N" chip
  // that says where it is.
  const here = slotModId(slots[slotIdx]);
  const group = (m) => (here === m.id ? 0 : 1);
  const hits = buildPool()
    .filter((m) => modFitsSlot(m, slotIdx))
    .filter((m) => !pickerPrefs.pol || m.polarity === pickerPrefs.pol)
    .filter((m) => searchHit(m, q))
    // A CARD ON THE EVERY-RANK LIST is a row per rank, each with its own gain.
    .flatMap((m) => [m, ...lowerRanks(m)])
    .sort((a, b) => {
      const g = group(a) - group(b); // current first, then equipped, then the rest
      if (g) return g;
      // RIVENS FIRST, as their own block: they are the build's own items and
      // sorting them in by name would scatter them through the pool.
      const r = (b.riven ? 1 : 0) - (a.riven ? 1 : 0);
      if (r) return r;
      // ONE RULE, not three special cases.
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
    const isCur = here === m.id;
    const at = placedAt(m.card || m.id, slotIdx);
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
    const slotName = (idx) =>
      idx === EXILUS ? tr("exilus")
      : idx === STANCE ? tr("stance")
      : tr("slot") + " " + (idx + 1);
    const badge = isCur ? `<span class="slotchip cur">${slotName(slotIdx)}</span>`
      : at >= 0 ? `<span class="slotchip">${slotName(at)}</span>` : "";
    // The gain is THIS SLOT's — the same mod is worth something different in
    // another slot, because elements combine by mod order.
    const gainChip = gainChipFor(m.id, slotName(slotIdx));
    const title = conflict ? `incompatible (${m.family})`
      : exIllegal ? `cannot swap: ${ownMod.name} is not an exilus mod`
      : at >= 0 ? `swap with ${at === EXILUS ? "the exilus slot" : "slot " + (at + 1)}`
      : m.effects.join(" · ");
    return modRow(m, {
      cls: `${conflict || exIllegal ? "dis" : ""} ${isCur ? "cur" : at >= 0 ? "placed" : ""}`,
      attrs: `data-id="${m.id}"`,
      title,
      rank: m.card ? m.rank : m.max_rank,
      chips: ` ${m.card ? `<span class="rkchip">R${m.rank}</span>` : ""}${badge}${gainChip}`,
      // THE BUILDER BINDS A VALUE, and this is it: what seating this card
      // costs the slot you are standing in.
      trailing: `<span class="dr">${m.drain}</span>`,
    });
  };
  menu.innerHTML = scanStrip(gainScan, { kind: "mods", idx: slotIdx }, hits.map((m) => m.id))
    + (hits.length
      ? sectionedRows(hits, (m) => (m.riven ? "Riven" : "Mods"), row)
      : `<div class="opt dis">${escHtml(tr("no matches"))}</div>`);
  menu.querySelectorAll(".opt:not(.dis)").forEach((o) => o.addEventListener("click", () => {
    if (here === o.dataset.id) { closePopovers(); return; } // already here
    const [id, rank] = splitRank(o.dataset.id);
    equipMod(slotIdx, id, rank);
    closePopovers(); renderMods();
  }));
}

// THE MOD SLOT'S MENU, on the shared one. It was a THIRD copy of
// the same two items — and the collision that found it is worth recording: this
// function was called `openSlotMenu` too, and being declared later in the file
// it silently SHADOWED the shared one, so an evolution card's ⋯ opened "Swap
// mod". Its items were hardcoded English as well, which is the same gap the
// arcane menu carried.
//
// MOD ops only: polarity lives on the left icon, and duplicating it here is
// what this menu has always refused.
function openModSlotMenu(slotIdx, anchor) {
  openSlotMenu(anchor, null, {
    label: tr("Mod"),
    removable: true,
    onSwap: () => openPicker(slotIdx, slotEl(slotIdx)),
    // In place: the slot keeps its polarity, which is a property of the SLOT
    // and not of what was in it.
    onPick: () => { equipMod(slotIdx, null); renderMods(); },
  });
}

function openPolMenu(slotIdx) {
  closePopovers();
  const menu = $("slot-menu");
  const cur = slots[slotIdx].pol;
  menu.innerHTML = GUN_POLS.map((p) => `<div class="mi ${p === cur ? "sel" : ""}" data-p="${p}">${imgTag(POL(p), "pol")} ${p === "Omni" ? "Omni (any)" : p}</div>`).join("") +
    `<div class="mi ${!cur ? "sel" : ""}" data-p="">◇ none</div>`;
  place(menu, slotEl(slotIdx));
  menu.querySelectorAll(".mi").forEach((o) => o.addEventListener("click", () => {
    setSlotPolarity(slotIdx, o.dataset.p || null);
    closePopovers(); renderMods();
  }));
}

/// A slot's polarity, or none. The mod stays: polarity is the slot's.
function setSlotPolarity(i, pol) {
  slots[i].pol = pol || null;
}

