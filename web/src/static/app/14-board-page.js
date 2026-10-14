// ---- THE BOARD: one ruler, every weapon ---------------------------------
//
// WHAT IT IS FOR: the fastest way to see which weapons are
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
// THE LIST IS ALREADY IN THE RIGHT ORDER: `board::benchmarks::all()` puts the
// PRIMARY ruler first, so `[0]` is the one a reader should meet. It was path
// order until a second ruler arrived and put a brand-new empty board in front
// of the populated one.
const benchCurrent = () =>
  benchList().find((b) => b.id === benchPick) || benchList()[0] || null;

/// A RULER'S SCENE (`cell_dir`, "" the base) — the first scene every list
/// reads before the next. The engine lists each fight's scenes together;
/// boot regroups them (`benchScenesFirst`) so Lv 9999 comes whole, then Lv 200.
const benchSceneOf = (b) => (b && b.cell_dir) || "";
const benchScenes = () => [...new Set(benchList().map(benchSceneOf))];
function benchScenesFirst(list) {
  const scenes = [...new Set(list.map(benchSceneOf))];
  return list.map((b, i) => [scenes.indexOf(benchSceneOf(b)), i, b]).sort((x, y) => x[0] - y[0] || x[1] - y[1]).map((x) => x[2]);
}
/// A SCENE IN WORDS, as its rulers' `scene_name` says it.
const benchSceneName = (dir) => {
  const b = benchList().find((x) => benchSceneOf(x) === dir);
  return b && b.scene_name ? tr(b.scene_name) : dir;
};
/// A RULER'S SHORT NAME: its first clause, and its scene once a fight is
/// measured in more than one — "Standard Single Target" alone would name two
/// boards. The whole name is `tr(b.name)`.
function benchShort(id) {
  const b = benchList().find((x) => x.id === id);
  if (!b) return id || "";
  const head = tr(b.name).split(" · ")[0];
  return benchScenes().length > 1 && b.scene_name ? `${head} · ${tr(b.scene_name)}` : head;
}

/// HOW A WEAPON WAS PLAYED, in words. The ids are the vocabulary a submission
/// and a board row use; this is what a reader sees.
///
/// NAMED FOR THE FORM IT FIRES, not for the mode's own id: a Cernos Prime's
/// `base` mode is its CHARGED shot, because that is what the arsenal hands
/// you, and calling it "base form" would be true of the id and false of the
/// weapon. The mode ids are roles; the labels are the weapon's own words.
/// THE FORM A CYCLE SPENDS — the gauge-fed one, which is the only thing that
/// tells a cycle apart from a free alternate.
const earnedForm = (w) =>
  ((w || {}).forms || []).find((x) => !x.is_default && x.gauge_switched) || null;

/// A MODE'S NAME, which is its FORM's — before it is told from its neighbour.
const bareModeLabel = (w, id) => {
  // NAMED AFTER THE FORM IT SPENDS, not after a product: the Mausolon buys its
  // charged shot with five kills and has no Genesis anywhere, so "Incarnon
  // cycle" names something sixty-nine weapons have and the seventieth does not.
  if (isCycleMode(id)) {
    const f = earnedForm(w);
    return f ? trF("{form} cycle", { form: tr(f.name) }) : tr("cycle");
  }
  // A MODE'S NAME IS FIXED, and the stance changes what it is WORTH. Deriving
  // it from the equipped stance breaks the one question a stance slot exists
  // to answer: "which stance is best for the neutral combo" cannot be asked if
  // the two builds call that mode different things.
  const f = modeForm(w, id);
  return f ? tr(f.name) : tr(id);
};

const modeLabel = (w, id) => {
  const name = bareModeLabel(w, id);
  // TWO MODES CAN ARRIVE AT ONE NAME, and then it names neither: a weapon with
  // two shots has a cycle per shot, and both cycles spend the same form. WHICH
  // SHOT is the difference, so that is what is added — and only where a
  // sibling mode would read the same, so one of each is untouched.
  const twin = ((w || {}).modes || []).some((o) => o !== id && bareModeLabel(w, o) === name);
  const f = twin && modeForm(w, id);
  if (!f) return name;
  // A CYCLE IS TOLD FROM A CYCLE BY THE FORM THAT FILLS ITS GAUGE, which is
  // the one thing the two do differently; anything else is told apart by how
  // it is fired, which is what `firedForm` adds.
  return isCycleMode(id)
    ? trF("{form} cycle ({base})",
      { form: tr((earnedForm(w) || {}).name || ""), base: tr(f.name) })
    : firedForm(f);
};

/// One entry per WEAPON AND MODE: its best row under `id`, or null where nobody
/// has submitted. A weapon with no row is not an error, it is the invitation —
/// and a weapon with a row for one mode and none for the other is the sharpest
/// version of it, because the missing one is a question somebody can answer
/// this afternoon.
/// WHICH BUILDS THE BOARD IS SHOWING — `all`, `plain` or `riven`.
///
/// The RANKING is one list: a riven build does not always beat a plain one, so
/// ranking them apart would publish a comparison the fight does not make. What this decides is which subset each weapon's SHOWN
/// row is drawn from — and the reason it exists is `plain`: under `all`, a
/// weapon whose riven build wins hides its plain build entirely, and those are
/// the builds most readers can actually make.
let benchRivenView = "all";
const BENCH_VIEWS = [
  ["all", "All builds"],
  ["plain", "No riven"],
  ["riven", "Riven only"],
];
const rowHasRiven = (r) => !!(r && r.riven);

/// WHICH ENTRANTS ARE BEING RANKED: a slot (or all of them), that slot's own
/// chips (`slotFilterRows`, the home page's), a name, and whether rows nobody
/// has measured are listed. All of it rides the address, so a filtered board
/// is a link a reader can send.
///
/// AN ABILITY-SCALED EXALTED WEAPON IS NEVER IN ANOTHER'S LIST. Its numbers
/// are its Warframe's ability's: it seats no capacity of its own and scales
/// with ability strength, so its KPM and a gun's are not like terms. "All"
/// leaves it out, and the Exalted slot ranks it in a list of its own.
let benchSlot = "all";
let benchSlotFilter = slotFilterNew();
let benchMeasuredOnly = false;
let benchQuery = "";
const benchPool = () => (META.weapons || []).filter((w) => (benchSlot === "all" ? !w.exalted : w.slot === benchSlot));

/// THE ADDRESS IS THE STATE: read once when the page opens, written on every
/// change without a history entry per click.
function benchReadUrl() {
  const p = new URLSearchParams(location.search);
  benchPick = p.get("ruler") || benchPick;
  benchSlot = p.get("slot") || "all";
  benchSlotFilter = { type: p.get("type") || "", cls: p.get("class") || "",
    tags: (p.get("tags") || "").split(",").filter(Boolean) };
  benchRivenView = ["plain", "riven"].includes(p.get("riven")) ? p.get("riven") : "all";
  benchMeasuredOnly = p.get("measured") === "1";
  benchQuery = p.get("q") || "";
}
function benchWriteUrl() {
  const p = new URLSearchParams();
  const cur = benchCurrent();
  if (cur && cur !== benchList()[0]) p.set("ruler", cur.id);
  if (benchSlot !== "all") p.set("slot", benchSlot);
  if (benchSlotFilter.type) p.set("type", benchSlotFilter.type);
  if (benchSlotFilter.cls) p.set("class", benchSlotFilter.cls);
  if (benchSlotFilter.tags.length) p.set("tags", benchSlotFilter.tags.join(","));
  if (benchRivenView !== "all") p.set("riven", benchRivenView);
  if (benchMeasuredOnly) p.set("measured", "1");
  if (benchQuery) p.set("q", benchQuery);
  const q = p.toString();
  if (/^\/benchmark\/?$/.test(location.pathname) && location.search !== (q ? "?" + q : "")) {
    history.replaceState(history.state, "", location.pathname + (q ? "?" + q : ""));
  }
}

/// A BOARD ROW'S RIVEN, AS A LOCAL ITEM — the name it takes on the reader's
/// machine, DERIVED from the shape so opening the same row twice reuses the
/// one riven instead of stacking copies.
///
/// A row cannot name somebody's riven: the item is on one machine and what the
/// board holds is a SHAPE at the corner its fight chose. So the reader's copy
/// is named after every stat AND the roll it was stored at: two rows of one
/// shape at two corners are two cards, and a name that dropped a roll handed
/// one row's riven to the other.
const boardRivenName = (rv) => {
  const stat = (id, i) => {
    const roll = (rv.rolls || [])[i];
    return roll == null ? id : `${id} x${roll}`;
  };
  const bonuses = rv.bonuses || [];
  return `${tr("board")} · ${bonuses.map(stat).join(" / ")}`
    + (rv.malus ? ` − ${stat(rv.malus, bonuses.length)}` : "");
};

/// The definitions `builtinBuilds` met, by the mod id it put in the slot.
///
/// A REGISTRY RATHER THAN A WRITE, so listing the board's builds stays pure:
/// a reader who never opens a riven row never gets one in their riven list.
/// `restoreState` is where it lands, because that is the moment the build is
/// actually being taken: copying a board build also copies its riven
/// locally.
const boardRivenDefs = {};
/// One board riven as a riven PRESET's state — the editor's own shape.
const boardRivenState = (rv) => ({
  bonuses: (rv.bonuses || []).map((id, i) => ({ id, roll: (rv.rolls || [])[i] ?? 1 })),
  malus: rv.malus
    ? { id: rv.malus, roll: (rv.rolls || [])[(rv.bonuses || []).length] ?? 1 }
    : null,
  rank: 8,
  polarity: "madurai",
});
const inBenchView = (r) =>
  benchRivenView === "all"
  || (benchRivenView === "riven") === rowHasRiven(r);

/// THE WEAPON'S OWN FILE WINS OVER THE INDEX, and either answers this the
/// same: the index holds each group's leader and this reduces a group to its
/// leader, so a weapon whose full rows are already in hand needs no second
/// source. A weapon in neither is "never fetched", which `boardProjection`
/// refuses to speak about — and is why the index does not join `BOARD_HAVE`.
const benchRows = (id) => BOARD[id] || (BOARD_INDEX && BOARD_INDEX[id]) || [];

/// …UNDER ANOTHER SCENE, its own index alone: those rows are in no weapon file.
const benchRowsUnder = (cur, id) => (cur.cell_dir ? (SCENE_INDEX[cur.cell_dir] || {})[id] || [] : benchRows(id));

const benchEntries = (cur, weapons) => {
  const out = [];
  for (const w of weapons) {
    for (const m of w.modes || ["base"]) {
      const rows = benchRowsUnder(cur, w.id)
        .filter((r) => r.benchmark === cur.id && (r.mode || "base") === m)
        .filter(inBenchView);
      const row = rows.length ? rows.reduce((a, r) => (r.score > a.score ? r : a), rows[0]) : null;
      if (row || !benchMeasuredOnly) out.push({ w, mode: m, row });
    }
  }
  return out;
};

/// HOW MANY BUILDS ARE IN THE LIBRARY, asked once per page and cached.
///
/// The board is a STATIC FILE committed to the repo and served from the CDN,
/// which is what makes it fast and free — and what makes it only as fresh as
/// the last scoring run. Moving it behind a service to make it live would trade
/// the fast, unblockable path for a slow one; instead the file stays, and this
/// asks the library the one fact the file cannot carry about itself.
///
/// A COUNT is all that comes back. It is enough for the sentence it is in and
/// it is the most the store can honestly say: nothing about a submitter is kept
/// there, so nothing about one can be reported here.
let benchPending = null;
function benchPendingAsk() {
  if (benchPending !== null) return;
  benchPending = "asking";
  fetch("/api/board/pending")
    .then((r) => (r.ok ? r.json() : null))
    .then((j) => {
      benchPending = j && j.ok ? j : "failed";
      // Only a repaint, and only if the reader is still on the board — the
      // number is a footnote and never worth stealing a render for.
      if ($("bench-board")) renderBenchBoard();
    })
    .catch(() => { benchPending = "failed"; });
}

/// The footnote itself. Silent unless the library is genuinely AHEAD of the
/// board: "0 waiting" is furniture, and a board that IS current should simply
/// look current.
///
/// IT PROMISES NOTHING ABOUT WHEN. The schedule fires hourly, but a scheduled
/// run STANDS DOWN while the engine fingerprint has moved — the push that moved
/// it owns that rescore — and a row too expensive for one sitting carries into
/// the next, so any named wait is a promise the pipeline does not keep and the
/// page cannot check. How far behind the board is, it can say exactly.
/// HOW OLD A NUMBER IS, in the coarsest unit that still says something. A board
/// that should move in hours cannot answer "today", and a reader deciding
/// whether to trust a row wants the age before the count.
function benchAgeText(seconds) {
  const mins = Math.floor((Date.now() / 1000 - seconds) / 60);
  if (!(mins >= 0)) return "";
  if (mins < 60) return tr("{n} min").replace("{n}", String(mins));
  if (mins < 60 * 48) return tr("{n} h").replace("{n}", String(Math.floor(mins / 60)));
  return tr("{n} d").replace("{n}", String(Math.floor(mins / 1440)));
}

/// WHAT THE BOARD IN HAND SAYS ABOUT ITSELF, and NOT what the binary was built
/// with. `board_state.yaml` is compiled into the wasm, so the copy behind
/// `META.benchmarks` is as old as the last site build — while the scoring job
/// rewrites `board.meta.json` every hour and never rebuilds the binary. Read
/// the compiled copy and the age below dates the BUILD, and "N more submitted"
/// counts everything sent since it. The fetched stamp carries the same fields
/// for exactly this reason; `META`'s is the fallback for a page that has none.
const benchState = (cur) =>
  (BOARD_META && BOARD_META.boards && cur && BOARD_META.boards[cur.id]) || cur || {};

function benchPendingNote(cur) {
  benchPendingAsk();
  const st = benchState(cur);
  // THE AGE IS NOT CONDITIONAL ON THE COUNT. A board with nothing waiting can
  // still be days old — a data correction adds no submission — and that is
  // exactly the case a reader cannot see any other way. ZERO means a board
  // written before the field existed, which is unknown rather than 1970.
  const at = st.scored_at_epoch_seconds || 0;
  const age = at > 0 ? benchAgeText(at) : "";
  const aged = age
    ? ` <span class="bench-pending">${escHtml(tr("· scored {t} ago").replace("{t}", age))}</span>`
    : "";
  if (!benchPending || typeof benchPending !== "object") return aged;
  const note = (text) => ` <span class="bench-pending">${escHtml(text)}</span>`;
  let out = aged;
  const scored = st.submissions || 0;
  const waiting = benchPending.count - scored;
  // AND NEVER A NEGATIVE ONE. A ruler that has just been added has scored
  // nothing, and a store that has expired records can sit below what the last
  // run read; both are real and neither is "builds are waiting".
  if (waiting > 0 && scored) {
    out += note(tr("· {n} more submitted since this board was scored")
      .replace("{n}", waiting));
  }
  // …AND WHAT IS STILL BEING MEASURED, which is a different question and the
  // one a reader of the BOARD has. They part company exactly when somebody asks
  // for rows to be measured again: nothing has arrived, the line above says
  // nothing, and the board is about to change anyway.
  //
  // ABSENT MEANS "CANNOT SAY", NEVER "NOTHING LEFT" — a door that could not
  // answer sends no `owed` at all, and a page that read that as zero would
  // promise a finished board it knows nothing about.
  const owed = benchPending.owed && cur ? benchPending.owed[cur.id] : undefined;
  if (Number.isFinite(owed) && owed > 0) {
    out += note(tr("· {n} rows still being measured").replace("{n}", owed));
  }
  return out;
}

/// THE RULER'S FIGHT AS A PICTURE, drawn only while its fold is open: shut on
/// a phone by default, where it would fill the screen above the ranking, and
/// drawn the moment a reader opens it.
function benchArena(cur) {
  const fold = $("bench-arena-fold");
  const bar = $("bench-arena");
  if (fold && !fold.dataset.set) {
    fold.dataset.set = "1";
    fold.open = !matchMedia("(max-width: 700px)").matches;
    fold.ontoggle = () => { if (fold.open) benchArena(benchCurrent()); };
  }
  if (bar && cur && (!fold || fold.open)) {
    const sc = { ...defaultScenario(), ...(cur.scenario || {}) };
    sc.player_at = [...(sc.player_at || [0, 0])];
    sc.target_at = [...(sc.target_at || [0, CONTACT_M])];
    // THE RULER'S OWN CROWD, never `= []` and `= null`: a benchmark CAN have
    // a formation, and hardcoding its absence draws a single body for a
    // 361-body fight while the rules beside it say otherwise. A picture that
    // contradicts the standard beside it
    // is worse than no picture.
    sc.formation = (sc.formation || []).map((f) => ({ ...f, at: [...f.at] }));
    sc.aim_at = sc.aim_at ? [...sc.aim_at] : null;
    mountArena(bar, sc, (allEnemies().find((e) => e.id === sc.enemy) || allEnemies()[0]), { readonly: true });
  }
}

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
    //
    // ONE CHIP PER FIGHT, its scenes on the row below: a cell is the same
    // ruler at another level or mode, and a chip each would bury the fights
    // under their scenes. A fight's chip names it in the scene on show, and
    // switching fight keeps that scene where the fight has it.
    const inScene = (base) => (cur && bs.find((b) => b.base === base && b.cell_dir === cur.cell_dir))
      || bs.find((b) => b.id === base);
    picker.innerHTML = bs.filter((b) => b.id === b.base).map((b) => `<button type="button" class="bchip${
      cur && b.id === cur.base ? " sel" : ""}" data-bench="${escHtml(b.id)}">${escHtml(tr(inScene(b.id).name))}</button>`).join("");
    picker.querySelectorAll("[data-bench]").forEach((el) => {
      el.onclick = () => { benchPick = inScene(el.dataset.bench).id; benchShowPicked(); };
    });
  }
  const scenes = $("bench-scenes");
  if (scenes) {
    const mine = cur ? bs.filter((b) => b.base === cur.base) : [];
    scenes.innerHTML = mine.length < 2 ? "" : mine.map((b) => `<button type="button" class="bchip${
      b.id === cur.id ? " sel" : ""}" data-scene="${escHtml(b.id)}">${escHtml(tr(b.scene_name || b.name))}</button>`).join("");
    scenes.querySelectorAll("[data-scene]").forEach((el) => {
      el.onclick = () => { benchPick = el.dataset.scene; benchShowPicked(); };
    });
  }
  if (!cur) { box.innerHTML = ""; return; }
  // …AND WHICH ENTRANTS AND WHICH BUILDS, one panel above the list: the slot,
  // that slot's own chips, the riven view, measured-only and a name. Every row
  // of it is one scrolling line, so on a phone the list is not pushed below
  // the fold by filters.
  const filters = $("bench-filters");
  if (filters) {
    const slots = ((META && META.equipment_slots) || []).filter((sl) => sl.holds === "weapon");
    const inSlot = (id) => (META.weapons || []).filter((w) => w.slot === id).length;
    const sfChips = benchSlot === "all" ? "" : slotFilterRows(benchPool(), benchSlotFilter);
    const active = (benchSlot !== "all") + !!benchSlotFilter.type + !!benchSlotFilter.cls
      + benchSlotFilter.tags.length + (benchRivenView !== "all") + benchMeasuredOnly;
    const q = $("bench-q");
    const typing = q && document.activeElement === q;
    const open = filters.querySelector("details") ? filters.querySelector("details").open
      : !matchMedia("(max-width: 700px)").matches;
    filters.innerHTML = `<div class="slotf-row"><input id="bench-q" class="slotf-search" type="search"
        placeholder="${escHtml(tr("Search by name"))}" value="${escHtml(benchQuery)}"></div>
      <details class="slotf-box"${open ? " open" : ""}><summary>${escHtml(tr("Filters"))}${
        active ? ` <span class="bcnt">${active}</span>` : ""}</summary>
      <div class="slotf-row"><span class="slotf-lab">${escHtml(tr("Equipment slot"))}</span>${
        filterChip(tr("All"), benchSlot === "all", `data-bslot="all"`)}${
        slots.map((sl) => filterChip(tr(sl.name), benchSlot === sl.id, `data-bslot="${escHtml(sl.id)}"`, inSlot(sl.id))).join("")}</div>
      ${sfChips}
      <div class="slotf-row"><span class="slotf-lab">${escHtml(tr("Builds"))}</span>${
        BENCH_VIEWS.map(([v, label]) => filterChip(tr(label), benchRivenView === v, `data-bview="${v}"`)).join("")}${
        filterChip(tr("Hide unmeasured"), benchMeasuredOnly, `data-bmeasured="1"`)}</div>
      </details>`;
    const redraw = () => { benchWriteUrl(); renderBenchBoard(); };
    filters.querySelectorAll("[data-bslot]").forEach((el) => {
      el.onclick = () => { benchSlot = el.dataset.bslot; benchSlotFilter = slotFilterNew(); redraw(); };
    });
    filters.querySelectorAll("[data-sf]").forEach((el) => {
      el.onclick = () => { slotFilterClick(benchSlotFilter, el); redraw(); };
    });
    filters.querySelectorAll("[data-bview]").forEach((el) => {
      el.onclick = () => { benchRivenView = el.dataset.bview; redraw(); };
    });
    filters.querySelector("[data-bmeasured]").onclick = () => { benchMeasuredOnly = !benchMeasuredOnly; redraw(); };
    const input = $("bench-q");
    onTyped(input, () => { benchQuery = input.value; redraw(); });
    if (typing) { input.focus(); input.setSelectionRange(input.value.length, input.value.length); }
  }
  // THE RULES, under the ruler that makes them. Collapsed by default: a reader
  // who wants the ranking should not have to scroll a standard to reach it, and
  // one who doubts a row should not have to leave the page to check the terms.
  const rules = $("bench-rules");
  if (rules) {
    const rs = cur.rules || [];
    // THE CLAIM FIRST, THEN THE TERMS. A ruler whose name says "standard" owes
    // the reader what that rests on, and it belongs above the rules rather than
    // beside them: the rules say what is done, this says why it is worth doing.
    const why = cur.standard
      ? `<p class="bstd">${escHtml(tr(cur.standard))}</p>` : "";
    rules.innerHTML = !rs.length && !why ? "" : `<details class="brules">
      <summary>${escHtml(tr("What this benchmark measures"))}</summary>
      ${why}<ul>${rs.map((x) => `<li>${escHtml(tr(x))}</li>`).join("")}</ul>
    </details>`;
  }
  // THE RULER'S FIGHT, DRAWN. The rules above say it in sentences; this is
  // the same fight as a picture, by the same component the simulator uses, so
  // the two cannot describe different arenas.
  //
  // READ-ONLY, and doubly so: `mountArena` is told, and the scene also asks
  // `officialScenarioActive` at the gesture — a board's positions are pinned.
  benchArena(cur);
  const leaders = benchIndexOf(cur);
  if (!leaders.index && (leaders.asking || leaders.lost)) {
    box.innerHTML = leaders.asking
      ? `<div class="sim-empty">${escHtml(tr("Loading the board…"))}</div>`
      : `<div class="sim-empty">${escHtml(tr("The board could not be reached."))} <button type="button" class="ghost-btn small" id="bench-retry">${escHtml(tr("Retry"))}</button></div>`;
    const again = $("bench-retry");
    if (again) again.onclick = showBenchBoard;
    return;
  }
  // SORTED BY THE BENCHMARK'S OWN METRIC — it says which one it is measured in
  // (`scenario.metric`), and a second ruler may answer differently. Unmeasured
  // weapons sort last whatever the metric: a zero is not a low score, it is no
  // score, and putting it among the low ones would read as one. A RANK IS
  // COUNTED INSIDE THE LIST SHOWN, so a filtered board reads from #1.
  const metric = metricLabel(metricOf((cur.scenario || {}).metric));
  const ranked = (weapons) => benchEntries(cur, weapons.filter((w) =>
    slotFilterMatch(benchSlotFilter, w) && nameMatches(benchQuery, w.name, w.name_en)))
    .sort((a, b) => (b.row ? b.row.score : -1) - (a.row ? a.row.score : -1));
  const head = (rows) => `<div class="bench-meta">${escHtml(
      tr("{n} of {t} entries measured · ranked by {m}")
        .replace("{n}", rows.filter((r) => r.row).length).replace("{t}", rows.length).replace("{m}", metric))}${
      benchPendingNote(cur)}</div><p class="bench-note">${escHtml(measuredNote())}</p>`;
  const lists = [];
  const list = (rows) => {
    lists.push(rows);
    return rows.length ? `<div class="bench-rows" data-blist="${lists.length - 1}"></div><div class="bmore" data-bmore="${lists.length - 1}"></div>`
      : `<div class="sim-empty">${escHtml(tr("No weapon matches these filters."))}</div>`;
  };
  const pool = benchPool();
  // A RANK HOLDS UNDER THIS RULER'S FIGHT AND NO OTHER, said above the lists:
  // inside the collapsed rules, a reader comparing rows never sees it.
  const scope = `<p class="bench-note">${escHtml(tr("This ranking shows results in this benchmark's scenario, a starting point for a build. Adjust it to how you actually play."))}</p>`;
  if (benchSlot === "exalted") {
    // TWO LISTS, NEVER ONE — see `benchSlot`.
    const ability = ranked(pool.filter((w) => w.exalted)), weapon = ranked(pool.filter((w) => !w.exalted));
    box.innerHTML = scope + ([[ability, "Scales with ability strength — ranked on its own"], [weapon, "Does not scale with ability strength"]]
      .filter(([rows]) => rows.length).map(([rows, label]) =>
        `<h3 class="wgroup-h">${escHtml(tr(label))}</h3>${head(rows)}${list(rows)}`).join("")
      || `<div class="sim-empty">${escHtml(tr("No weapon matches these filters."))}</div>`);
  } else {
    const rows = ranked(pool);
    box.innerHTML = scope + head(rows) + (benchSlot === "all" && (META.weapons || []).some((w) => w.exalted)
      ? `<p class="bench-note">${escHtml(tr("Exalted weapons that scale with ability strength are ranked on their own, under Exalted Weapon."))}</p>` : "")
      + list(rows);
  }
  // ROWS ARRIVE AS THE READER SCROLLS: a batch now, the next when the end of
  // a list comes into view, until every row is on the page.
  if (benchScroll) benchScroll.disconnect();
  const more = (mark) => {
    const k = Number(mark.dataset.bmore), rows = lists[k];
    const into = box.querySelector(`[data-blist="${k}"]`);
    const at = into.children.length;
    into.insertAdjacentHTML("beforeend", rows.slice(at, at + BENCH_BATCH).map((e, j) => benchRowHtml(cur, e, at + j)).join(""));
    if (into.children.length >= rows.length) { if (benchScroll) benchScroll.unobserve(mark); mark.remove(); }
    else mark.textContent = trF("{n} of {t} shown · scroll for more", { n: into.children.length, t: rows.length });
  };
  benchScroll = typeof IntersectionObserver === "function"
    ? new IntersectionObserver((seen) => seen.forEach((e) => e.isIntersecting && more(e.target)), { rootMargin: "800px" })
    : null;
  box.querySelectorAll("[data-bmore]").forEach((mark) => {
    more(mark);
    // NO OBSERVER, NO PAGING: draw the rest at once rather than strand it.
    if (mark.isConnected) { if (benchScroll) benchScroll.observe(mark); else while (mark.isConnected) more(mark); }
  });
}
const BENCH_BATCH = 60;

/// A RULER OR SCENE PICKED: drawn at once, and again when its leaders land.
function benchShowPicked() {
  benchWriteUrl();
  const ask = benchIndexOf(benchCurrent()).load();
  renderBenchBoard();
  ask.then(() => { if ($("bench-board") && !$("bench-page").hidden) renderBenchBoard(); });
}
let benchScroll = null;

/// ONE BOARD ROW: its rank inside the list shown, the weapon and how it was
/// played, and its best score under the ruler — or the invitation to measure it.
const benchRowHtml = (cur, { w, mode, row }, i) => `
      <a class="brow${row ? "" : " none"}" href="/weapons/${urlSlug(w)}${
        // WHICH RULER YOU CAME FROM, not just how the weapon is played:
        // without it the link lands on whatever official build is first, and
        // the two boards' leaders are both called "#1 · Incarnon cycle". It
        // selects the FIGHT as well as the build, because a board row is a
        // build AND the ruler it was measured under — arriving with only the
        // build is arriving with a number you cannot reproduce.
        // …AND WHICH OF THE TWO LEADERS. The board holds a riven row and a
        // plain one per weapon and mode, both called "#1", and the ruler and
        // the mode tell them apart no better than they told the two rulers
        // apart in 2026-08-08. Clicking the leader of the "riven only" view
        // opened the PLAIN leader — a different build, with a riven slot the
        // landing page then had nothing to put in.
        //
        // STATED EITHER WAY (`riven=1` / `riven=0`) rather than only when
        // there is one, so an ABSENT parameter keeps meaning what it always
        // meant. The MODE is stated on the same terms: a row is one weapon in
        // one mode whether or not it has a second.
        `?bench=${encodeURIComponent(cur.id)}&mode=${encodeURIComponent(mode)}${
          row ? `&riven=${rowHasRiven(row) ? 1 : 0}` : ""}`}">
        <span class="brank">${row ? `#${i + 1}` : "—"}</span>
        ${imgTag(IMG(w.image), "bimg")}
        <span class="bname">${escHtml(w.name)}${
          // ALWAYS NAMED, including a weapon with one way to be fired — the
          // rule the simulator's build card already carries. A blank beside a
          // neighbour that names its mode reads as "no mode", and this is where
          // weapons are COMPARED.
          ` <span class="bmode">${escHtml(modeLabel(w, mode))}</span>`}${
          // THE BOARD IS WHERE WEAPONS ARE COMPARED, so it is the one place a
          // weapon with unmodelled parts must not look like one without them.
          // A Stug row is four admissions deep and a Torid row is exact; side
          // by side and unmarked they read as the same kind of number.
          // The mark is the banner's own ◈ and carries the same sentences.
          (w.unmodeled || []).length
            ? ` <span class="bgap" title="${escHtml(
                tr("not modelled on this weapon — the numbers below are a floor, not its full output")
                + ": " + gapsOf(w).map(trGap).join(" · "))}">◈</span>`
            : ""}</span>
        <span class="bscore">${row
          ? escHtml(row.shown != null ? String(row.shown) : row.score.toFixed(4))
            + (measuredRecordHtml(row, false, false) ? `<small class="bwhen" title="${escHtml(measuredNote())}">${measuredRecordHtml(row, false, false)}</small>` : "")
          : `<span class="bnone">${escHtml(tr("not measured"))}</span>`}</span>
      </a>`;

