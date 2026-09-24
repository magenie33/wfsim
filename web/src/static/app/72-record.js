// ---- THE COMBAT RECORD -------------------------------------------------
//
// One row per number the game pops, in order, with everything behind it. Every
// other block on this panel is an aggregate, and an aggregate hides an error
// inside an average — a factor applied twice moves a mean by a few per cent and
// reads as a build being good. This is the one output that can be laid beside a
// recording and checked number for number.
//
// IT IS FETCHED, NOT CARRIED. `/api/log` re-runs the median engagement from its
// own RNG state — about a millisecond single-target — so the record costs
// nothing until somebody opens it, nothing on the wire for the runs nobody
// reads, and nothing on disk ever. An ordinary fight is a few thousand rows and
// the densest measured is 408,817, which is why the request takes a WINDOW.
let recordState = null;

/// The fight this record is about, as the key that says whether it is stale.
/// A record is only meaningful for the run it was taken from, so it is thrown
/// away whenever the result it explains is.
function recordKey(r) {
  return `${simKey()}|${(r.run || []).join(",")}`;
}

function recordMarkup(r) {
  if (!r) return "";
  // A RESULT FROM BEFORE THIS EXISTED SAYS SO, rather than drawing nothing.
  //
  // The record is fetched by naming the engagement it explains — the median
  // run's own RNG state, which `/api/simulate` started returning the day this
  // panel landed. A result SAVED before that has no name to give, and the
  // block must not simply vanish: a reader coming back to a stored result then
  // finds the feature absent with nothing saying why. An absence that is not
  // explained reads as a feature that is not there — the same rule
  // this panel already follows about its own caps.
  if (!r.run) {
    return foldBlock("record", tr("Combat record"),
      tr("one row per number the game pops, and everything behind it"),
      `<div class="rec-idle"><span class="sim-hint">${escHtml(
        tr("this result was saved before the record existed — run the fight again to read it"))}</span></div>`);
  }
  // AN EMPTY HOST, filled by `paintRecord` in the same wire pass. Rendering
  // the table here as well builds it TWICE on every result — and, since the
  // record can live in its own window, builds it into the wrong document
  // before the paint moves it out again.
  return foldBlock("record", tr("Combat record"),
    tr("one row per number the game pops, and everything behind it"),
    `<div class="rec" id="rec-host"></div>`);
}

/// THE IDLE STATE, and it is not the same question in the two documents.
///
/// IN THE WINDOW it is "read it": the reader has already asked for the record
/// and is looking at the place it goes.
///
/// IN THE PAGE it is "open it", because the table is very wide and very long
/// and this column is neither — the panel offers the window and the window
/// holds the table. One click does both: the button opens the window and the
/// read starts in it, so the reader never has to ask twice for one thing.
function recordIdle() {
  if (recWinOpen()) {
    return `<div class="rec-idle">
      <button class="ghost-btn small" id="rec-load">${escHtml(tr("Read the record"))}</button>
      <span class="sim-hint">${escHtml(tr("re-runs this engagement to list every damage instance"))}</span>
    </div>`;
  }
  // THE SAME id, because it is the same act — the label says which half of it
  // the reader is standing in front of.
  return `<div class="rec-idle">
    <button class="ghost-btn small" id="rec-load">${escHtml(tr("Open the record"))}</button>
    <span class="sim-hint">${escHtml(tr("one row per number the game popped — it opens in a window of its own, because the table is wider than this column"))}</span>
  </div>`;
}

/// FETCH A WINDOW. `from`/`to` are seconds; the whole fight is the default and
/// is the right answer for almost every build — only a status weapon over a
/// formation reaches the cap, and when it does the answer says so.
/// HOW MANY EVENTS A READ ASKS FOR, by default.
///
/// Not a limit on what the fight DID — a limit on what one read hands over. At
/// the measured 481 bytes an event this is ~10 MB and about half a second, and
/// it covers an ordinary fight several times over; what it does not cover is a
/// dense build over the full 180 s, which is exactly the fight somebody is
/// arguing about. So it is a DEFAULT rather than a ceiling: when something did
/// not fit, the panel offers to go and get it, and says what that costs.
const RECORD_LIMIT = 20000;

/// HOW MANY EVENTS THE PANEL READS WITHOUT BEING ASKED.
///
/// THE RECORD IS THIS PANEL'S BEST ARGUMENT AND IT WAS BEHIND THREE CLICKS —
/// a fold, a button, and a window. Nobody who has not already decided to trust
/// the page ever pressed them, which is exactly backwards: the rows are what
/// earn the trust. So a small window is read on its own, the panel shows a few
/// rows of it in its own narrow shape, and the full table still opens in the
/// window it needs.
///
/// Small on purpose. The fetch costs ONE re-run of the engagement whatever the
/// limit, and the panel is already paying a hundred of them; what a limit buys
/// is the transfer, and a peek needs a screenful.
const RECORD_PEEK = 300;
/// HOW MANY ROWS THE PEEK DRAWS. Enough to see that they are real and that one
/// leads to the next; a reader who wants the fight opens the window.
const RECORD_PEEK_ROWS = 6;
/// Result keys a peek has already been tried for, so a repaint — and a repaint
/// happens on every roll-call click — does not re-run the engagement again.
const recordPeeked = new Set();

/// HOW MANY PAGES A "read the whole fight" WILL ASK FOR before giving up.
///
/// A backstop, not a budget: at `RECORD_LIMIT` a page it takes twelve to cover
/// the densest build ever measured, and anything past that is a runaway rather
/// than a fight.
const RECORD_MAX_PAGES = 40;

/// READ THE RECORD, in as many PAGES as it takes.
///
/// `pages` is 1 for the ordinary read and unbounded for "read the whole
/// fight". Each page is one more re-run of the same engagement — about 45 ms of
/// engine — which is the price the owner named when he asked for all of it
/// however many runs that takes.
///
/// PAGING IS BY OFFSET, never by time: a page boundary can fall in the middle
/// of an instant, and several numbers share a timestamp — so "continue from t"
/// either loses them or repeats them. The server skips a count instead, and an
/// event's id is its place in the FIGHT, so the pages concatenate into one
/// stream and a floating number can still name its row across a boundary.
async function loadRecord(r, from, to, limit = RECORD_LIMIT, pages = 1) {
  const key = recordKey(r);
  recordState = { key, from, to, limit, loading: true, body: null, filter: "all", at: 0,
                  page: 0, pages };
  paintRecord(r);
  // THE BUILD AND THE FIGHT, exactly as `runSim` sends them. `theFight()` is
  // the SCENARIO — it carries no mods, no mode, no evolutions, no riven — so a
  // record asked for with it alone was computed for a BLANK build and explained
  // a fight nobody ran. Worse than wrong: with no `evolutions` the parser hands
  // the Incarnon form over free, so the record opened already transmuted while
  // the report beside it had earned the form.
  //
  // The pairing is the whole claim this panel makes, so it is written the same
  // way the run is: `buildPayload()` then the fight, one line apart.
  // `to` IS OMITTED WHEN THERE IS NONE, never sent as null: the server reads a
  // missing key as "the whole fight" and a null one as zero, which silently
  // truncated the window to nothing after the opening events.
  const win = to == null ? { from } : { from, to };
  const events = [];
  let out = null, dropped = 0;
  for (let page = 0; page < Math.max(1, pages); page++) {
    out = await api("/api/log",
      { ...buildPayload(), ...theFight({ run: r.run, ...win, limit, skip: events.length }) });
    if (!recordState || recordState.key !== key) return;  // the result moved on
    const got = (out && out.events) || [];
    events.push(...got);
    dropped = (out && out.dropped) || 0;
    // NOTHING LEFT, or nothing coming: a page that returns no events cannot be
    // followed by one that does, and stopping on it is what keeps a runaway
    // from turning into forty requests.
    if (!dropped || !got.length) break;
    // THE PANEL COUNTS ITSELF WHILE IT WORKS. A read that takes eight runs is a
    // read that looks hung, which is this repo's own rule about long work.
    recordState.page = page + 1;
    recordState.events = events;
    paintRecord(r);
  }
  if (!recordState || recordState.key !== key) return;
  recordState.loading = false;
  recordState.events = events;
  recordState.dropped = dropped;
  // THE TWO ROSTERS the rows' stack lists index into — the shooter's buffs and
  // the target's debuffs, both positional and both named once rather than per
  // row.
  recordState.rosters = { buffs: (out && out.buffs) || [], debuffs: (out && out.debuffs) || [] };
  // THE COMBATANT ROSTER, the third of the three a row indexes into.
  recordState.combatants = (out && out.combatants) || [];
  // THE FACTOR TABLE, sent once. A row names its factors by their index here,
  // because the same thirteen "did nothing" names on every row of a fight were
  // the largest single share of a 17.2 MB window.
  recordState.factors = (out && out.factors) || [];
  // …AND WHAT A ROW LEFT OUT, filled forward. The weapon's state is identical
  // on every row of a trigger pull and the two stack lists change only when
  // something is applied or expires, so an absent field means "the same as the
  // row before". Done ONCE here rather than at each reader, so nothing
  // downstream has to know the wire is compressed at all.
  let carriedWeapon = null, carriedBuffs = null, carriedDebuffs = null;
  for (const e of recordState.events) {
    if (e.weapon) carriedWeapon = e.weapon; else e.weapon = carriedWeapon;
    if (e.kind !== "damage") continue;
    if (e.buffs) carriedBuffs = e.buffs; else e.buffs = carriedBuffs;
    if (e.debuffs) carriedDebuffs = e.debuffs; else e.debuffs = carriedDebuffs;
  }
  recordState.error = out && out.ok === false ? out.error : null;
  paintRecord(r);
}

/// HOW MANY ROWS THE TABLE HOLDS AT ONCE.
///
/// The record covers the WHOLE fight — that was the point of paging the fetch
/// — and the densest build measured is 24,652 events. A table of 24,652 rows is
/// ~250,000 cells, and the browser lays every one of them out on every repaint
/// of the result panel: picking an enemy, scrubbing the replay, or simply
/// re-rendering froze the page for seconds.
///
/// SO THE FETCH AND THE VIEW ARE PAGED SEPARATELY, which is the whole fix: the
/// stream in memory is still the entire fight — `Copy as text` writes all of
/// it, and a floating number can still name its row — and only what is on
/// SCREEN is bounded. A reader looks at one screenful either way.
const REC_PAGE = 500;

/// THE RECORD'S OWN WINDOW, while one is open.
///
/// Asked for as "open the combat record in a whole new page, it is very long
/// and very big and should not be in this window". It is
/// the same markup: the parent keeps the state and calls the same
/// `recordBody`/`wireRecord` against the child's host, so there is ONE
/// implementation of the table and the window is only where it is drawn.
let recWin = null;
/// WHETHER THE BROWSER REFUSED THE WINDOW. The record belongs in a window of its
/// own and the panel is not a second home for it — so the table is drawn in this
/// page only when there is nowhere else for it to go.
let recPopupBlocked = false;
/// The result the open record explains, so the window can be repainted — and
/// handed back to the panel — without the caller that opened it.
let recordResult = null;

const recWinOpen = () => !!(recWin && !recWin.closed && recWin.document);

/// THE READER'S OWN THEME AND LANGUAGE, carried into the record's window rather
/// than re-derived there — the page decides both and a second answer would be a
/// second setting. `data-theme` is the one that matters and was the one missing:
/// the window copied only the class, so an explicitly DARK page opened a light
/// window and the toggle in the topbar did not reach it.
///
/// Called on open and again on every flip, because the window outlives both.
function syncRecordChrome() {
  if (!recWinOpen()) return;
  const src = document.documentElement, dst = recWin.document.documentElement;
  const theme = src.getAttribute("data-theme");
  if (theme) dst.setAttribute("data-theme", theme);
  else dst.removeAttribute("data-theme");
  dst.lang = src.lang || "en";
  dst.className = src.className;
  recWin.document.body.className = document.body.className;
}

/// WHERE THE TABLE GOES: the child's host while a window is open, this page's
/// otherwise. Every paint goes through it, so nothing else has to know.
function recordHostEl() {
  if (recWinOpen()) {
    const el = recWin.document.getElementById("rec-host");
    if (el) return el;
  }
  return $("rec-host");
}

/// OPEN IT. The child is written rather than fetched: it is this app's own
/// stylesheet over one host element, and a real navigation would boot a second
/// copy of the whole SPA — a second wasm module, a second worker fleet — to
/// display a table the parent already holds.
function openRecordWindow() {
  if (recWinOpen()) { recWin.focus(); return; }
  const w = window.open("", "wfsim-record", "width=1500,height=900");
  // A BLOCKED POPUP IS NOT AN ERROR AND NOT A SILENCE: the table falls back into
  // the panel and the panel says why, which is the only outcome the reader can
  // act on. It is ALSO the one case the table may be drawn there at all — see
  // `paintRecord`.
  if (!w) { recWin = null; recPopupBlocked = true; paintRecord(recordResult); return; }
  recPopupBlocked = false;
  recWin = w;
  const doc = w.document;
  doc.open();
  // THE WINDOW IS A PAGE OF THIS SITE, and it is dressed like one: the same
  // stylesheet, the same wordmark, and a heading that says which fight the rows
  // below are about. A bare table on a white rectangle reads as a debug dump of
  // the app rather than a part of it.
  doc.write(`<!doctype html><meta charset="utf-8">`
    + `<meta name="viewport" content="width=device-width,initial-scale=1">`
    + `<title>${escHtml(tr("Combat record"))} · WFSim</title>`
    + `<link rel="icon" type="image/svg+xml" href="/logo.svg">`
    // THIS PAGE'S OWN STYLESHEETS, by the hrefs it is wearing — never a path
    // written here. The built site serves `/asset/style.<digest>.css` and only
    // the dev server has ever had a `/style.css`, so the hardcoded one 404'd on
    // every shipped build and the window came up as a bare HTML table.
    + [...document.querySelectorAll('link[rel="stylesheet"]')]
        .map((l) => `<link rel="stylesheet" href="${escHtml(l.href)}">`).join("")
    + `<body><header class="recwin-top"><span class="brand">WF<span>Sim</span></span>`
    + `<span class="recwin-title">${escHtml(tr("Combat record"))}</span>`
    + `<span class="recwin-sub" id="rec-what"></span>`
    // THE SAME BUTTON AS THE TOPBAR'S, calling the same flip: the window is a
    // page of this app and the light/dark choice is one setting, so it is
    // thrown from either document and both follow.
    + `<button class="ghost-btn" id="rec-theme" title="${escHtml(tr("Toggle light / dark"))}"`
    + ` aria-label="${escHtml(tr("Toggle theme"))}">◐</button></header>`
    + `<div class="rec recwin" id="rec-host"></div>`);
  doc.close();
  syncRecordChrome();
  const th = doc.getElementById("rec-theme");
  if (th) th.onclick = () => flipTheme();
  // CLOSING IT HANDS BACK THE OFFER, not the table: the panel goes to the button
  // that opens the window again, which is the only thing this column ever shows.
  w.addEventListener("pagehide", () => {
    recWin = null;
    paintRecord(recordResult);
  });
  paintRecord(recordResult);
}

/// WHICH FIGHT THE ROWS ARE ABOUT, for the window's heading — the weapon, the
/// target it was fired at, and the run the record replays. The window outlives
/// the run that opened it (the panel repaints it on every result), so this is
/// re-read on every paint rather than written once when it opened.
function recordSubtitle(r) {
  const w = weaponInfo($("weapon").value);
  const t = (r && r.target) || {};
  const bits = [];
  if (w && w.name) bits.push(w.name);
  if (t.name) {
    bits.push(`${t.name}${t.level != null ? ` Lv ${t.level}` : ""}${
      t.steel_path ? " SP" : ""}`);
  }
  return bits.join("  ·  ");
}

function paintRecord(r) {
  if (r) recordResult = r;
  const host = recordHostEl();
  if (recWinOpen()) {
    const what = recWin.document.getElementById("rec-what");
    if (what) what.textContent = recordSubtitle(recordResult);
  }
  if (host) {
    // THE STATE HAS TO BE ABOUT THIS RUN. A record explains ONE engagement, and
    // this now paints on every result render — a stored result picked from the
    // list, an enemy chosen in the roll call — so a record left over from the
    // previous run would be drawn under the new one's heading.
    const st = recordState && recordResult && recordState.key === recordKey(recordResult)
      ? recordState : null;
    // THE TABLE IS THE WINDOW'S, AND ONLY THE WINDOW'S. This column is one
    // column wide and the record is neither — so a loaded record does not get
    // drawn here just because it is in hand. The panel keeps the button, and
    // closing the window gives the button back rather than the table.
    //
    // THE ONE EXCEPTION IS A REFUSED POPUP, because then there is nowhere else
    // for it to go and a feature the browser blocked must not simply vanish.
    const mine = host === $("rec-host");
    // THE FULL TABLE IS THE WINDOW'S, AND ONLY THE WINDOW'S — it is ten columns
    // and 1080px, and this is one column. What the panel draws is a PEEK: the
    // same rows from the same renderer, with the wide columns dropped by CSS,
    // so there is one implementation and two widths rather than two tables that
    // have to be kept agreeing.
    // …AND A PEEK IS NOT A READ. A blocked popup sends the table into the
    // panel, and this slot drops `recordIdle()` with it — so a PEEK landing
    // here drew 300 rows where the record belongs and took away the button
    // that would fetch the rest: a truncated record presenting itself as the
    // whole one, with no way left to ask again. A read supersedes a peek and
    // a fresh result peeks again, so the two land in this slot in either
    // order; `limit` is what tells them apart.
    const read = st && (st.limit || 0) > RECORD_PEEK;
    const draw = read && (!mine || recPopupBlocked);
    host.innerHTML = draw ? recordBody(st)
      : (mine && st ? recordBody(st, true) : "") + recordIdle();
    wireRecord(recordResult, host);
    // ...AND IF THERE IS NOTHING TO PEEK AT, GO AND GET A LITTLE. Once per
    // result: `paintRecord` runs on every repaint, and each read is a re-run.
    if (mine && !st && recordResult && recordResult.run) {
      const key = recordKey(recordResult);
      if (!recordPeeked.has(key)) {
        recordPeeked.add(key);
        loadRecord(recordResult, 0, undefined, RECORD_PEEK);
      }
    }
  }
  // …AND THE PANEL SAYS WHERE IT WENT. An empty block where the table belongs
  // reads as the feature breaking.
  const inline = $("rec-host");
  if (inline && inline !== host) {
    inline.innerHTML = `<div class="rec-idle">`
      + `<span class="sim-hint">${escHtml(tr("the record is open in its own window"))}</span>`
      + `<button class="ghost-btn small" id="rec-focus">${escHtml(tr("Show me"))}</button>`
      + `</div>`;
    // …AND THE WAY BACK IS TO THE WINDOW, not away from it: a window behind the
    // browser reads exactly like one that never opened.
    const focus = inline.querySelector("#rec-focus");
    if (focus) focus.onclick = () => { if (recWinOpen()) recWin.focus(); else paintRecord(recordResult); };
  }
}

/// WHO THE ROWS ARE ABOUT. A weapon event belongs to NOBODY — a reload is not
/// an enemy's business — so a per-enemy view is a FILTER over one stream rather
/// than the same event copied into every table.
function recordBodies(events) {
  const seen = new Map();
  for (const e of events) {
    if (e.body == null) continue;
    const cur = seen.get(e.body) || 0;
    seen.set(e.body, cur + (e.kind === "damage" ? e.effective : 0));
  }
  return [...seen.entries()].sort((a, b) => b[1] - a[1]);
}

const REC_KINDS = [
  ["all", "everything"], ["own", "own"], ["multishot", "multishot"],
  ["punch_through", "punch through"], ["status", "status"],
  // A SHOT, AN ARRIVAL AND A MISS are weapon events rather than numbers, so
  // they sit under one chip with the reloads and the transmutes — and "only
  // the misses" is its own question, because a pellet that went nowhere is
  // invisible in every other view this app has.
  ["event", "events"], ["miss", "misses"],
];

function recordBody(st, peek) {
  if (st.loading) {
    return `<div class="rec-idle"><span class="sim-hint">${escHtml(tr("reading…"))}${
      st.page ? ` ${escHtml(tr("pass {n}").replace("{n}", st.page + 1))} · ${
        (st.events || []).length.toLocaleString()}` : ""}</span></div>`;
  }
  if (st.error) return `<div class="rec-idle"><span class="sim-hint">${escHtml(st.error)}</span></div>`;
  const events = st.events || [];
  if (!events.length) {
    return `<div class="rec-idle"><span class="sim-hint">${escHtml(tr("nothing happened in this window"))}</span></div>`;
  }
  // THE SHEET GOES ABOVE EITHER VIEW. The panel shows a PEEK of six rows and
  // the full table lives in its own window, but the fold is over every row
  // LOADED — so the sheet is the same answer in both places, and it is the one
  // thing a reader who never opens the window still gets.
  const sheet = st.actor ? actorSheet(st.actor) : "";
  const bodies = recordBodies(events);
  const pick = st.body == null ? (bodies.length ? bodies[0][0] : null) : st.body;
  const rows = events.filter((e) => e.body == null || e.body === pick);
  const shown = rows.filter((e) => recordPasses(e, st.filter));
  const n = (x) => Math.round(x).toLocaleString();
  // ONE SCREENFUL OF THE STREAM — see `REC_PAGE`. `at` is clamped rather than
  // trusted: the list under it changes length whenever the filter or the body
  // does, and a stale offset past the end would draw an empty table over a
  // fight that has thousands of rows in it.
  const at = Math.max(0, Math.min(st.at || 0, Math.max(0, shown.length - 1)));
  const page = shown.slice(at, at + REC_PAGE);

  const chips = bodies.map(([b, dmg]) =>
    `<button class="pchip${b === pick ? " sel" : ""}" data-recbody="${b}">${
      escHtml(b === 0 ? tr("aimed") : `e${b + 1}`)}<span class="ct">${n(dmg)}</span></button>`).join("");
  const kinds = REC_KINDS.map(([k, label]) =>
    `<button class="pchip${st.filter === k ? " sel" : ""}" data-reckind="${k}">${escHtml(tr(label))}</button>`).join("");

  const dmg = shown.filter((e) => e.kind === "damage").length;
  // THE PEEK: the same rows, the same renderer, no tools and no pager. It
  // exists so the record is ON SCREEN instead of behind a button — the rows are
  // what earn this panel its trust, and nothing that has to be asked for earns
  // anything. The wide columns are dropped by CSS (`.rec-peek`), not by a
  // second markup path, so the two views cannot drift apart.
  if (peek) {
    const few = shown.filter((e) => e.kind === "damage").slice(0, RECORD_PEEK_ROWS);
    if (!few.length) return sheet;
    return `${sheet}<div class="rec-peek"><table class="rec-t">
      <thead><tr>
        <th>${escHtml(tr("time"))}</th>${
        `<th>${escHtml(tr("who dealt it"))}</th>`}<th>${escHtml(tr("damage source"))}</th>
        <th>${escHtml(tr("part"))}</th>
        <th class="num">${escHtml(tr("damage"))}</th>
        <th>${escHtml(tr("where the number comes from"))}</th>
      </tr></thead>
      <tbody>${few.map((e) => recordRow(e, st.rosters || {})).join("")}</tbody>
    </table></div>
    <div class="rec-peek-n">${escHtml(tr("{a} of the {b} numbers this engagement popped")
      .replace("{a}", String(few.length)).replace("{b}", n(dmg)))}</div>`;
  }
  // THE SLICE THIS STREAM ACTUALLY COVERS: where it was asked to start, and
  // where it ran out — which is the LAST EVENT in it when the cap bit, not the
  // window it asked for.
  const cut = st.dropped > 0 || (st.from || 0) > 0;
  const window0 = cut ? (st.from || 0) : null;
  const window1 = cut ? events[events.length - 1].t : null;
  return `${sheet}
    <div class="rec-tools">
      <span class="tlabel">${escHtml(tr("whose"))}</span>${chips}<span class="rec-sep"></span>
      <span class="tlabel">${escHtml(tr("only"))}</span>${kinds}
      <span class="rec-sep"></span>
      <button class="ghost-btn small" id="rec-copy">${escHtml(tr("Copy as text"))}</button>${
      // …AND A WINDOW OF ITS OWN, offered only where it is not already in one.
      recWinOpen() ? "" :
        ` <button class="ghost-btn small" id="rec-pop">${escHtml(tr("Open in its own window"))}</button>`}
    </div>
    <div class="rec-count">${escHtml(tr("{n} numbers popped").replace("{n}", n(dmg)))}${
      // WHICH SLICE OF THE FIGHT THIS IS. Silent for a stream that covers the
      // whole engagement, which is most of them — and never silent when it does
      // not, because a window nobody is told about reads as the whole fight.
      window0 != null
        ? ` · ${escHtml(tr("{a}s to {b}s of the fight")
            .replace("{a}", window0.toFixed(1)).replace("{b}", window1.toFixed(1)))}`
        : ""}${
      st.dropped ? ` · ${escHtml(tr("{n} more did not fit").replace("{n}", n(st.dropped)))}` : ""}${
      // …AND AN OFFER TO GO AND GET THEM. The cap is a default, not a verdict:
      // a reader who came here to check a number against a recording wants the
      // whole fight, and the cost of that is theirs to spend. It says the size
      // rather than hiding it, which is the same rule the cap itself follows.
      st.dropped
        ? ` <button class="ghost-btn small" id="rec-all">${escHtml(tr("read the whole fight"))}</button>`
          + `<span class="sim-hint"> ${escHtml(tr("about {mb} MB").replace("{mb}",
              (((events.length + st.dropped) * 481) / 1e6).toFixed(0)))}</span>`
        : ""}</div>
    ${recordPager(at, shown.length)}
    <div class="rec-scroll"><table class="rec-t">
      <thead><tr>
        <th>${escHtml(tr("time"))}</th>${
        `<th>${escHtml(tr("who dealt it"))}</th>`}<th>${escHtml(tr("damage source"))}</th>
        <th>${escHtml(tr("part"))}</th>
        <th class="num">${escHtml(tr("damage"))}</th>
        <th>${escHtml(tr("procs"))}</th>
        <th>${escHtml(tr("where the number comes from"))}</th>
        <th>${escHtml(tr("weapon"))}</th>
        <th>${escHtml(tr("buffs up"))}</th>
        <th>${escHtml(tr("before · target"))}</th>
        <th>${escHtml(tr("statuses on the target"))}</th>
      </tr></thead>
      <tbody>${page.map((e) => recordRow(e, st.rosters || { buffs: [], debuffs: [] })).join("")}</tbody>
    </table></div>
    ${recordPager(at, shown.length)}`;
}

/// WHICH SCREENFUL OF THE LIST THIS IS, above the table and below it — a reader
/// who has scrolled to the bottom of five hundred rows is exactly the reader
/// who wants the next five hundred, and sending them back to the top for the
/// control would be the whole point of paging thrown away.
///
/// It draws NOTHING for a list that fits, which is most of them: a pager on a
/// forty-row record is furniture that says the same thing twice.
function recordPager(at, total) {
  if (total <= REC_PAGE) return "";
  const to = Math.min(total, at + REC_PAGE);
  const step = (target, label, on) => on
    ? `<button class="pchip" data-recat="${target}">${escHtml(label)}</button>`
    : `<button class="pchip" disabled>${escHtml(label)}</button>`;
  return `<div class="rec-pager">
    ${step(0, "«", at > 0)}
    ${step(Math.max(0, at - REC_PAGE), "‹", at > 0)}
    <span class="tlabel">${escHtml(tr("{a}–{b} of {n}")
      .replace("{a}", (at + 1).toLocaleString())
      .replace("{b}", to.toLocaleString())
      .replace("{n}", total.toLocaleString()))}</span>
    ${step(at + REC_PAGE, "›", to < total)}
    ${step(Math.max(0, (Math.ceil(total / REC_PAGE) - 1) * REC_PAGE), "»", to < total)}
  </div>`;
}

function recordPasses(e, filter) {
  if (filter === "all") return true;
  if (filter === "event") return e.kind !== "damage";
  if (filter === "miss") return e.kind === "miss";
  return e.kind === "damage" && e.origin === filter;
}

const REC_POOL = { shield: "on the shield", health: "through to health", overguard: "on overguard" };

/// A STACK LIST AS CHIPS, with the ones at zero left out. Both sides of a row
/// use it: the shooter's buffs and the target's debuffs are the same shape seen
/// from opposite ends, so they are drawn by one function.
///
/// IT NAMES THEM THE WAY THE REPLAY'S OWN TABLES DO. `tr(id)` for a roster id
/// is the id, so a Chinese reader gets `corrosion`, `on_kill_multishot` and
/// `arcane:secondary_enervate` in a column whose whole job is to say what was
/// up, while the chart two blocks down says 腐蚀 and the
/// buff's own card name. Two spellings of one thing, which is the mistake this
/// panel exists to stop making.
function recStacks(list, roster, cls, now) {
  const nameOf = cls === "on" ? debuffRosterName : buffRosterName;
  const on = (list || [])
    .map(([n, until], i) => [roster[i], n, until])
    .filter(([id, n]) => id && n > 0);
  if (!on.length) return `<span class="z">—</span>`;
  // …AND HOW LONG IT HAS LEFT. The wire carries an ABSOLUTE expiry, so the
  // countdown is this row's own clock subtracted from it — which means two
  // rows a second apart show a buff ticking down rather than both showing the
  // number it had when it was applied. `null` is one whose end the loop does
  // not track and it shows no time at all rather than a guess.
  const left = (until) => {
    if (until == null) return "";
    if (until === "inf") return "";
    const s = until - now;
    return s > 0 ? `<em>${s < 10 ? s.toFixed(1) : Math.round(s)}s</em>` : "";
  };
  return `<div class="rec-stk">${on.map(([id, n, until]) =>
    `<span class="rec-s ${cls}">${escHtml(nameOf(id))}<b>${n}</b>${left(until)}</span>`).join("")}</div>`;
}

/// THE LEDGER, LAYER BY LAYER — and the SHAPE says which mechanic it is.
///
/// A bracket lists its terms and adds them; a snap shows its grid; only a real
/// multiplicative bracket gets a `×`. That is not decoration: printing
/// `×5.706 Condition Overload` puts a multiplication sign in front of a
/// QUOTIENT — the base-damage bracket divided by itself — and
/// `×4.156 element bracket + quantization`, which is the ratio of a per-element
/// snap's two totals. Two numbers the game does not have, both looking exactly
/// like a factor.
///
/// There is no shape here that can draw a quotient, which is what stops it
/// coming back.
function ledgerRows(e) {
  const F = (i) => ((recordState && recordState.factors) || [])[i] || String(i);
  const n = (x) => Math.round(x).toLocaleString();
  const n2 = (x) => x.toLocaleString(undefined, { maximumFractionDigits: 2 });
  const sign = (v) => (v < 0 ? "−" : "+");
  // A TERM OF A BRACKET, with the pair it is a product of where it has one.
  // A TERM NAMES ITSELF ONLY WHEN THE NAME IS EXACT. Where the engine holds a
  // SUM and cannot say which cards are in it, the number goes on alone — a
  // category label there sends a reader looking for a card that matches it and
  // there is none, which is worse than saying nothing.
  const term = (t) => `<span class="lg-term"${t.f == null ? "" : ` data-factor="${
    escHtml(F(t.f))}"`}><i class="lg-op">${sign(t.v)}</i>${n2(Math.abs(t.v))}${
    t.f == null ? "" : `<b>${escHtml(tr(F(t.f)))}</b>`}${
    t.o ? `<em>${n2(t.o[0])} × ${n2(t.o[1])}</em>` : ""}</span>`;

  return (e.layers || []).map((l) => {
    if (l.k === "b") {
      return `<div class="lg lg-b">
        <span class="lg-lbl">${escHtml(tr(F(l.f)))}</span>
        <span class="lg-body">${(l.t || []).map(term).join("")}<span class="lg-sum">= ${
          n2(l.s)}</span><span class="lg-out">${n(l.o)}</span></span></div>`;
    }
    if (l.k === "q") {
      // THE GRID, and the units — because the mechanic IS integer units. A
      // component going from 89.81 units to 90 is a sentence; 2,178.00 going to
      // 2,182.50 is not.
      return `<div class="lg lg-q">
        <span class="lg-lbl">${escHtml(tr("quantization"))}</span>
        <span class="lg-body"><span class="lg-scale">${escHtml(tr("grid"))} ${
          n2(l.scale)}</span><table class="lg-snap"><thead><tr><td></td><td class="num pc">${
          escHtml(tr("of base"))}</td><td class="num">${escHtml(tr("before"))}</td><td></td><td class="num">${
          escHtml(tr("units"))}</td><td></td><td class="num sn">${escHtml(tr("snapped"))}</td><td class="num to">${
          escHtml(tr("after"))}</td></tr></thead>${(l.c || []).map(([ty, from, units, to]) =>
          `<tr><td>${escHtml(DT(ty))}</td><td class="num pc">${
            l.o ? "+" + ((from / (l.scale * 32)) * 100).toFixed(1) + "%" : ""}</td><td class="num">${
            n2(from)}</td><td class="ar">→</td><td class="num">${n2(units)}</td><td class="ar">→</td>${
            ""}<td class="num sn">${Math.round(units)}</td><td class="num to">${n2(to)}</td></tr>`).join("")}</table><span class="lg-out">${n(l.o)}</span></span></div>`;
    }
    if (l.k === "s") {
      // A SUM OF WHOLE NUMBERS, and the only shape here that is not about a
      // multiplier. Each part is damage, so it is drawn at full precision and
      // never with a bracket term's `+0.80` formatting — the accumulator's own
      // 1 is worth a fraction of a point and rounding it away would leave the
      // row not adding up, which is the one thing this panel may not do.
      return `<div class="lg lg-s">
        <span class="lg-lbl">${escHtml(tr("sum"))}</span>
        <span class="lg-body">${(l.p || []).map((x, i) =>
          `<span class="lg-term" data-factor="${escHtml(F(x.f))}">${
            i ? `<i class="lg-op">+</i>` : ""}<span class="lg-amt">${n2(x.a)}</span><b>${
            escHtml(tr(F(x.f)))}</b>${
            // WHAT THAT NUMBER IS A PRODUCT OF. The two parts differ in exactly
            // one place — the seeds carry the payload's faction depth, the
            // accumulator carries one layer — and printing the products side by
            // side is the only way that is checkable rather than asserted.
            x.of ? `<em>${n2(x.head)}${(x.of || []).map((g) =>
              ` × ${n2(g.v)} ${escHtml(tr(F(g.f)))}`).join("")}</em>` : ""}</span>`).join("")}<span class="lg-out">${
          n(l.o)}</span></span></div>`;
    }
    // A MULTIPLICATIVE BRACKET — the only shape that earns a sign.
    return `<div class="lg lg-m">
      <span class="lg-lbl"><i class="lg-x">×</i>${n2(l.v)}</span>
      <span class="lg-body"><b>${escHtml(tr(F(l.f)))}</b>${
        l.t ? `<span class="lg-of">${n2(l.head)} × (1${(l.t || []).map(term).join("")})</span>` : ""}${
        F(l.f) === "critical" && e.crit_damage
          ? `<span class="lg-of">1 + ${e.crit} × (${e.crit_damage} − 1)</span>` : ""}<span class="lg-out">${n(l.o)}</span></span></div>`;
  }).join("");
}

/// ASK ABOUT ONE ACTOR, from wherever the reader is looking — a seat or a
/// body on the fight's own floor in zone 2, a chip in the roll call, a row in
/// the record itself.
///
/// IT LANDS ON THE RECORD, because that is what the answer is made of: the
/// sheet is a fold of these rows, so putting it anywhere else would be a
/// summary standing apart from the thing it summarises. Selecting a BODY also
/// filters the rows to it, which the roll call already did; selecting a SEAT
/// leaves the rows alone, because "what did this gun do" is a cut of the whole
/// engagement rather than a slice of it.
function openActor(side, i, name, sub) {
  if (!recordState) return;
  recordState.actor = { side, i: Number(i), name, sub };
  if (side === "foe") { recordState.body = Number(i); recordState.at = 0; }
  paintRecord(recordResult);
  const host = recordHostEl();
  if (host) host.scrollIntoView({ block: "start", behavior: "smooth" });
}

/// ONE ACTOR'S OWN SHEET, folded out of the combat record.
///
/// WCL GIVES YOU A PAGE PER ACTOR; SimC GIVES YOU A LINE PER ACTION. This is
/// both, and it is built from the RECORD rather than from a second set of
/// counters — so every figure on it is a sum of rows the reader can open, and
/// the sheet cannot disagree with the ledger it is made of. A per-actor
/// accumulator in `RunResult` would be 25 KB an engagement (`MAX_BODIES` is
/// 400) and a second answer to every question this already answers.
///
/// IT IS ONE ENGAGEMENT, not the mean, and it says so: the record is the
/// sampled run, which is what everything below the replay bar already is.
/// The means live in zone 1 and in the per-seat table.
///
/// SIDE DECIDES THE QUESTION. An ALLY is asked what it DEALT and by what; an
/// ENEMY is asked what it TOOK and from whom. They are the same fold read
/// along its two axes, which is why one function draws both.
function actorSheet(who) {
  const st = recordState;
  const rows = (st && st.events) || [];
  const dmg = rows.filter((e) => e.kind === "damage");
  const seats = (st && st.combatants) || [];
  const ally = who.side === "ally";
  // THE ROWS THAT ARE THIS ACTOR'S, and the axis the other side is cut by.
  const mine = dmg.filter((e) => (ally ? (e.combatant || 0) === who.i : (e.body || 0) === who.i));
  const total = mine.reduce((a, e) => a + (e.effective || 0), 0);
  // …AND THE CUT. An ally is cut by WHERE THE DAMAGE CAME FROM (its own
  // sources, SimC's action list); an enemy by WHO DEALT IT (WCL's, and the
  // one thing a single-dummy simulator cannot answer at all).
  const bucket = new Map();
  for (const e of mine) {
    const k = ally ? (e.origin || "?") : String(e.combatant || 0);
    const b = bucket.get(k) || { k, sum: 0, n: 0, top: 0, crit: 0, crits: 0 };
    b.sum += e.effective || 0;
    b.n += 1;
    b.top = Math.max(b.top, e.effective || 0);
    if ((e.crit || 0) > 0) b.crits += 1;
    bucket.set(k, b);
  }
  const cuts = [...bucket.values()].sort((a, b) => b.sum - a.sum);
  const most = cuts.length ? cuts[0].sum : 1;
  const n0 = (x) => Math.round(x || 0).toLocaleString();
  const pc = (x) => `${(x * 100).toFixed(1)}%`;
  // THE SAME NAME THE ROWS BELOW USE. A second table for origin names is a
  // second spelling of one word, and the row already has one.
  const label = (k) => (ally
    ? tr(String(k).replace(/_/g, " "))
    : combatantName((seats[Number(k)] || {}).id || k, seats[Number(k)]));
  const head = ally ? tr("what it came from") : tr("who dealt it");
  return `<div class="ac-sheet">
    <div class="ac-h">
      <span class="ac-side ${ally ? "ac-ally" : "ac-foe"}">${escHtml(ally ? tr("dealt") : tr("taken"))}</span>
      <span class="ac-name">${escHtml(who.name)}</span>
      <span class="ac-sub">${escHtml(who.sub || "")}</span>
      <span class="ac-tot">${n0(total)}</span>
    </div>
    <table class="ac-tab"><thead><tr>
      <th>${escHtml(head)}</th><th class="num">${escHtml(tr("total"))}</th>
      <th class="num">${escHtml(tr("share"))}</th><th class="num">${escHtml(tr("rows"))}</th>
      <th class="num">${escHtml(tr("biggest"))}</th><th class="num">${escHtml(tr("crit"))}</th>
    </tr></thead><tbody>${cuts.map((c) => `<tr>
      <td class="ac-bar"><i style="width:${(c.sum / most * 100).toFixed(1)}%"></i><span>${escHtml(label(c.k))}</span></td>
      <td class="num">${n0(c.sum)}</td>
      <td class="num">${total > 0 ? pc(c.sum / total) : "—"}</td>
      <td class="num">${n0(c.n)}</td>
      <td class="num">${n0(c.top)}</td>
      <td class="num">${c.n ? pc(c.crits / c.n) : "—"}</td>
    </tr>`).join("") || `<tr><td colspan="6" class="ac-none">${escHtml(tr("nothing in this window"))}</td></tr>`}</tbody></table>
    <p class="ac-n">${escHtml(trF(
      "{n} rows of this engagement's own ledger — every figure above is their sum, and each one opens",
      { n: n0(mine.length) }))}${
      // THE SLICE, AND ONLY WHERE THERE IS ONE. `st.to` is the window that was
      // ASKED for and is unset on a full read, so printing it unconditionally
      // said "between 0s and 0s" about a record covering the whole fight.
      (st && (st.dropped > 0 || (st.from || 0) > 0))
        ? ` ${escHtml(trF("{a}s to {b}s of the fight", {
          a: st.from || 0, b: (rows[rows.length - 1] || {}).t || 0 }))}`
        : ""}</p>
  </div>`;
}

function recordRow(e, rosters) {
  const n = (x) => Math.round(x).toLocaleString();
  // A FACTOR IS AN INDEX INTO THE RESPONSE'S OWN TABLE — see `loadRecord`.
  // AT THE TOP, because `const` has no hoisting: declared beside the `return`
  // where the other helpers live, every use above it threw a ReferenceError
  // from inside an async paint, which surfaces as the panel sitting on
  // "reading…" for ever and nothing in the console.
  const F = (i) => ((recordState && recordState.factors) || [])[i] || String(i);
  const w = e.weapon || {};
  // THE WHOLE AMMO PICTURE IN ONE COLUMN — magazine, what is left in reserve,
  // and the Incarnon gauge. They are one question ("can this weapon keep
  // firing, and what is it about to become") and a reader should not have to
  // hold three columns in their head.
  const g = w.gauge;
  const idle = w.idle_magazine;
  const inc = w.form === "transmuted";
  // NOTHING IS HIDDEN HERE. Both magazines are drawn on
  // every row — the one being fired and the one that is not — because a
  // transmute REFILLS the base form's behind the scenes, and a column that
  // showed only the active one made that free reload invisible. Same for the
  // gauge: it is what decides when the form arrives, so it is on every row of
  // the fight rather than only the ones where it moved.
  const wep = `<span class="rec-form ${inc ? "inc" : "base"}">${
    escHtml(tr(inc ? "transmuted" : "base"))}</span>${
    w.magazine_max ? `<span class="rec-mag firing">${escHtml(tr(inc ? "charges" : "magazine"))} ${
      w.magazine} / ${w.magazine_max}</span>` : ""}${
    idle ? `<span class="rec-mag idle">${escHtml(tr(inc ? "magazine" : "charges"))} ${
      idle[0]} / ${idle[1]}</span>` : ""}${
    w.reserve != null ? `<span class="rec-mag">${escHtml(tr("reserve"))} ${n(w.reserve)}</span>` : ""}${
    g ? `<span class="rec-gauge" title="${escHtml(tr("Incarnon gauge"))}"><i style="width:${
      Math.min(100, (g[0] / Math.max(1, g[1])) * 100).toFixed(0)}%"></i></span><span class="rec-mag">${
      escHtml(tr("gauge"))} ${g[0]} / ${g[1]}</span>` : ""}`;

  if (e.kind !== "damage") {
    return `<tr class="rec-evt rec-${escHtml(e.kind)}" data-recevent="${e.id}">
      <td class="rec-t">${e.t.toFixed(3)}${e.cause != null ? `<span class="rec-cause">#${e.cause}</span>` : ""}</td>
      <td colspan="7"><b>${escHtml(tr(recordEventName(e)))}</b>${
        e.into ? ` <span class="sim-hint">${escHtml(tr(e.into === "transmuted" ? "into the transmuted form" : "back to the base form"))}</span>` : ""}${
        e.seconds != null ? ` <span class="sim-hint">${e.seconds.toFixed(2)}s</span>` : ""}${
        e.reason ? ` <span class="sim-hint">${escHtml(tr(e.reason))}</span>` : ""}${
        e.pellets != null ? ` <span class="sim-hint">${e.pellets} ${escHtml(tr("pellets"))}</span>` : ""}</td>
      <td class="rec-wep">${wep}</td><td colspan="2"></td></tr>`;
  }

  // THE CRIT FACTOR CARRIES ITS FORMULA. `×4.40` is a product; `1 + 2 × (2.20
  // − 1)` is something a reader can check against the weapon's card, and the
  // two numbers in it are the only ones they have to trust.
  // A FACTOR CARRIES ITS OWN KEY. The label is translated and the key is not,
  // which is what lets anything asking "did the shield gate apply here" — a
  // check, a bug report, the browser's own find — ask about the FACTOR rather
  // than about the sentence a particular reader happens to see. `data-rpevent`
  // is the same idea for a floating number.
  const mit = (e.mitigation || []).map(([k, v]) =>
    `<span class="rec-f mit" data-factor="${escHtml(F(k))}">×${v}<i>${escHtml(tr(F(k)))}</i></span>`).join("");
  const b = e.before || {};
  const crit = e.crit ? `<span class="rec-crit">${escHtml(tr("crit"))} ${e.crit}</span>` : "";
  // WHICH PELLET, AND WHICH HALF OF ITS ATTACK. A pellet that has an explosion
  // is TWO rows and one pellet, so the two facts are drawn together: three
  // pellets with a radial read as six numbers in three pairs.
  const which = e.pellet != null
    ? `<span class="rec-pel">${escHtml(tr("pellet"))} ${e.pellet}</span><span class="rec-half">${
        escHtml(tr(e.radial ? "explosion" : "direct"))}</span>`
    : "";
  // THE ROW CARRIES ITS OWN NAME. The same id the floating number over the
  // arena points at with `data-rpevent`, so a number on the scene and a line
  // here are the same event under one name rather than two lists that happen
  // to agree.
  // EVERY CELL CARRIES ITS OWN HEADING. On a phone the table becomes a stack
  // of cards — a ten-column, 1080px-wide ledger cannot be read through a 326px
  // window, and the column that matters most is the one furthest off the right
  // edge — and a stacked cell with no heading is a number
  // with nothing to say what it is.
  const L = (k) => ` data-label="${escHtml(tr(k))}"`;
  // WHO DEALT IT, on the row itself. The ATTRIBUTION is never omitted — the
  // row is the ledger, and one that names its dealer only when a reader might
  // care is one nobody can audit — but the visible COLUMN is drawn only where
  // there is more than one thing firing, which is the rule the foe chips
  // already follow: a control with one option is not a control.
  const roster = (recordState && recordState.combatants) || [];
  const seat = roster[(e.combatant || 0)] || {};
  // THE STACK NAMES OF THE SEAT THAT DEALT IT. `rosters.buffs` is one list
  // per seat, because two seats are two builds — labelling this row's counts
  // with the wielder's cards would name a buff the dealer never carried.
  const myBuffs = ((rosters.buffs || [])[(e.combatant || 0)]) || [];
  const who = seat.id || "";
  // THE COLUMN IS ALWAYS THERE — see the rule on `foeChips`. A ledger that
  // names its dealer only when a reader might care is one nobody can audit,
  // and the header beside it is drawn unconditionally for the same reason.
  const whoCell = `<td class="rec-who"${L("who dealt it")}>${escHtml(combatantName(who, seat))}</td>`;
  return `<tr class="rec-dmg rec-${escHtml(e.pool)}" data-recevent="${e.id}" data-combatant="${escHtml(who)}">
    <td class="rec-t"${L("time")}>${e.t.toFixed(3)}${e.cause != null ? `<span class="rec-cause">#${e.cause}</span>` : ""}</td>
    ${whoCell}
    <td${L("damage source")}><span class="rec-org rec-o-${escHtml(e.origin)}" data-origin="${escHtml(e.origin)}">${escHtml(tr(e.origin.replace(/_/g, " ")))}</span>${which}</td>
    <td${L("part")}>${e.part ? `<span class="${e.head ? "rec-head" : ""}">${escHtml(tr(e.part))}${e.head ? " ⌖" : ""}</span>` : "<span class=\"z\">—</span>"}</td>
    <td class="num"${L("damage")}><b>${n(e.effective)}</b><span class="rec-pool">${escHtml(tr(REC_POOL[e.pool] || e.pool))}</span>${crit}</td>
    <td class="rec-proc"${L("procs")}>${
      // TWO SIDES, ONE COLUMN. What this instance put ON THE TARGET and what it
      // set off ON THE SHOOTER — the deltas that turn the two state columns
      // into something a reader can check: the next row's state is this row's
      // state plus these.
      (e.procs || []).map((p) => `<span class="rec-p on">${escHtml(DT(p))}</span>`).join("")
      + (e.triggered || []).map((i) => `<span class="rec-p up">${
          escHtml(buffRosterName(myBuffs[i] || String(i)))}</span>`).join("")
      || "<span class=\"z\">—</span>"}</td>
    <td class="rec-calc"${L("where the number comes from")}><div class="lg lg-base"><span class="lg-lbl">${
      escHtml(tr("base damage"))}</span><span class="lg-body"><span class="lg-out">${n(e.base)}</span></span></div>${
      ledgerRows(e)}${mit ? `<div class="lg-mit">${mit}</div>` : ""}<div class="lg lg-end"><span class="lg-lbl">${
      escHtml(tr("popped"))}</span><span class="lg-body"><span class="lg-exact">${
      escHtml(tr("pool lost"))} ${e.effective.toLocaleString(undefined, { maximumFractionDigits: 4 })}</span><span class="lg-out">${
      n(e.effective)}</span></span></div></td>
    <td class="rec-wep"${L("weapon")}>${wep}</td>
    <td class="rec-buff"${L("buffs up")}>${recStacks(e.buffs, myBuffs, "up", e.t)}</td>
    <td class="rec-state"${L("before · target")}>${
      b.overguard > 0 ? `<span data-pool="overguard"><i>${escHtml(tr("overguard"))}</i>${n(b.overguard)}</span>` : ""}<span data-pool="shield"><i>${escHtml(tr("shield"))}</i>${n(b.shield || 0)}</span><span data-pool="health"><i>${escHtml(tr("health"))}</i>${n(b.health || 0)}</span>${
      b.armor > 0 ? `<span data-pool="armour"><i>${escHtml(tr("armour"))}</i>${n(b.armor)}</span>` : ""}${
      // THE SHIELD-GATE WINDOW, drawn even though nothing this app fires takes
      // it: only a melee ground slam does (MEASUREMENTS M61), and melee is not
      // modelled. It is on screen so the claim can be checked the day it is.
      b.shield_gate_until != null
        ? `<span class="rec-gate"><i>${escHtml(tr("gate"))}</i>${b.shield_gate_until.toFixed(2)}s</span>`
        : ""}</td>
    <td class="rec-dbf"${L("statuses on the target")}>${recStacks(e.debuffs, rosters.debuffs, "on", e.t)}</td>
  </tr>`;
}

function recordEventName(e) {
  return {
    shot: "shot", miss: "missed",
    reload_start: "reload begins", reload_end: "reload ends",
    transform_start: "transform begins", transform_end: "transform ends",
    status_expired: "status expired", killed: "killed",
  }[e.kind] || e.kind;
}

/// EVERY CONTROL IS FOUND INSIDE THE HOST, never in `document`.
///
/// The table draws into this page or into the record's own window, and the two
/// are different documents — a `$("rec-copy")` would reach the wrong one, or
/// nothing at all, the moment the window opened. The host is the one thing both
/// paths have.
function wireRecord(r, host) {
  const el = (sel) => (host || document).querySelector(sel);
  // ONE CLICK IS THE WHOLE GESTURE: the window opens and the read starts in it.
  // Opening FIRST and synchronously is what keeps the reader's click on the
  // stack — `window.open` after an await is a blocked popup. A blocked one
  // leaves `recWinOpen()` false and the read lands in the panel instead: the
  // table is worse there and it is still the table.
  const load = el("#rec-load");
  if (load) {
    load.onclick = () => {
      if (!recWinOpen()) openRecordWindow();
      loadRecord(r, 0, null);
    };
  }
  const pop = el("#rec-pop");
  if (pop) pop.onclick = () => openRecordWindow();
  (host || document).querySelectorAll("[data-recat]").forEach((b) => {
    b.onclick = () => { recordState.at = Number(b.dataset.recat); paintRecord(r); };
  });
  const all = el("#rec-all");
  // ROOM FOR WHAT IT SAID DID NOT FIT, plus a fifth — `dropped` is counted one
  // per INSTANCE and a hit on a shielded body is two rows, so asking for
  // exactly the shortfall would come up short again on the one target shape
  // where it matters. The server clamps at 200,000 either way.
  if (all) {
    all.onclick = () => {
      const st = recordState || {};
      loadRecord(r, st.from || 0, st.to, st.limit || RECORD_LIMIT, RECORD_MAX_PAGES);
    };
  }
  // CHANGING WHAT IS LISTED GOES BACK TO THE TOP OF IT. Keeping the row
  // offset across a filter change lands the reader in the middle of a
  // different list, at a position that means nothing.
  (host || document).querySelectorAll("[data-recbody]").forEach((b) => {
    b.onclick = () => { recordState.body = Number(b.dataset.recbody); recordState.at = 0; paintRecord(r); };
  });
  (host || document).querySelectorAll("[data-reckind]").forEach((b) => {
    b.onclick = () => { recordState.filter = b.dataset.reckind; recordState.at = 0; paintRecord(r); };
  });
  const copy = el("#rec-copy");
  if (copy) {
    copy.onclick = () => {
      const txt = (recordState.events || []).map(recordLine).join("\n");
      if (navigator.clipboard) navigator.clipboard.writeText(txt);
      copy.textContent = tr("Copied");
      setTimeout(() => { copy.textContent = tr("Copy as text"); }, 1500);
    };
  }
}

/// ONE ROW AS TEXT, so two runs can be DIFFED. `one_fight` says an answer moved;
/// a diff of two records says WHICH row, which factor, and from what to what —
/// which is the thing nothing in this app could do before.
function recordLine(e) {
  const t = e.t.toFixed(3).padStart(8);
  if (e.kind === "miss") return `${t}  #${e.cause ?? "-"}  [miss]  ${e.reason || ""}`;
  if (e.kind !== "damage") return `${t}  [${e.kind}]`;
  const F = (i) => ((recordState && recordState.factors) || [])[i] || String(i);
  const chain = [...(e.steps || []), ...(e.mitigation || [])]
    .map(([k, v]) => `x${v} ${F(k)}`).join("  ");
  const b = e.before || {};
  return `${t}  #${e.cause ?? "-"}  ${e.origin}  ${e.part || "-"}  ${e.pool}  ${Math.round(e.effective)}`
    + `  | ${Math.round(e.base)}  ${chain}  = ${Math.round(e.raw)}`
    + `  | og ${Math.round(b.overguard || 0)} sh ${Math.round(b.shield || 0)} hp ${Math.round(b.health || 0)}`
    + `  | ${(e.procs || []).join("+") || "-"}`;
}

