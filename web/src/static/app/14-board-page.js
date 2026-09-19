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

const benchEntries = (id) => {
  const out = [];
  for (const w of META.weapons || []) {
    for (const m of w.modes || ["base"]) {
      const rows = benchRows(w.id)
        .filter((r) => r.benchmark === id && (r.mode || "base") === m)
        .filter(inBenchView);
      out.push({ w, mode: m,
        row: rows.length ? rows.reduce((a, r) => (r.score > a.score ? r : a), rows[0]) : null });
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
  // …AND WHICH BUILDS. A separate row of chips from the ruler's, because they
  // are separate questions: the ruler is the FIGHT, this is which entrants are
  // being listed. Drawn on every ruler, since every ruler takes both kinds.
  const view = $("bench-view");
  if (view) {
    view.innerHTML = BENCH_VIEWS.map(([v, label]) => `<button type="button" class="bchip${
      benchRivenView === v ? " sel" : ""}" data-bview="${v}">${escHtml(tr(label))}</button>`).join("");
    view.querySelectorAll("[data-bview]").forEach((el) => {
      el.onclick = () => { benchRivenView = el.dataset.bview; renderBenchBoard(); };
    });
  }
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
  // THE RULER'S FIGHT, DRAWN. The rules above say it in sentences; this is
  // the same fight as a picture, by the same component the simulator uses, so
  // the two cannot describe different arenas.
  //
  // READ-ONLY, and doubly so: `mountArena` is told, and the scene also asks
  // `officialScenarioActive` at the gesture — a board's positions are pinned.
  const bar = $("bench-arena");
  if (bar) {
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
  if (!BOARD_INDEX && (boardIndexAsk || boardIndexUnreachable)) {
    box.innerHTML = boardIndexAsk
      ? `<div class="sim-empty">${escHtml(tr("Loading the board…"))}</div>`
      : `<div class="sim-empty">${escHtml(tr("The board could not be reached."))} <button type="button" class="ghost-btn small" id="bench-retry">${escHtml(tr("Retry"))}</button></div>`;
    const again = $("bench-retry");
    if (again) again.onclick = showBenchBoard;
    return;
  }
  const entries = benchEntries(cur.id);
  // SORTED BY THE BENCHMARK'S OWN METRIC — it says which one it is measured in
  // (`scenario.metric`), and a second ruler may answer differently. Unmeasured
  // weapons sort last whatever the metric: a zero is not a low score, it is no
  // score, and putting it among the low ones would read as one.
  const metric = metricLabel(metricOf((cur.scenario || {}).metric));
  const rows = entries
    .slice()
    .sort((a, b) => (b.row ? b.row.score : -1) - (a.row ? a.row.score : -1));
  const measured = rows.filter((r) => r.row).length;
  box.innerHTML = `
    <div class="bench-meta">${escHtml(
      tr("{n} of {t} entries measured · ranked by {m}")
        .replace("{n}", measured).replace("{t}", rows.length).replace("{m}", metric))}${
      benchPendingNote(cur)}</div>
    <div class="bench-rows">${rows.map(({ w, mode, row }, i) => `
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
          : `<span class="bnone">${escHtml(tr("not measured"))}</span>`}</span>
      </a>`).join("")}</div>`;
}

