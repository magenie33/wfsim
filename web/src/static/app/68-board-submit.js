// ---- THE BOARD: consent, then submission -------------------------------
//
// WHEN THE ASK HAPPENS, and why it is here rather than on load: the first time
// you finish a run under the OFFICIAL scenario. Only then is there any context
// for the question — a score is on screen and "should this build go on the
// board" is a sentence that means something. Asking at startup would be a
// modal before you have done anything, which is a dialog people click away
// rather than a disclosure. (It is also inline: `prompt`/`alert`/`confirm` are
// blocked in this project.)
//
// WHAT TRAVELS, stated because it is short enough to check: the weapon, its
// mods, evolutions and arcanes, and which benchmark. No account, no
// identifier, no riven (they are out of the benchmark entirely), and none of
// the names you gave anything. NO SCORE either — the board scores builds
// itself, which is what makes a row reproducible and a forged number
// pointless.
const BOARD_CONSENT = "wfsim-board-consent";   // "yes" | "no" | absent = never chosen

// DEFAULT ON — and the important word is not "on", it is
// NOT SILENT. Opt-out versus opt-in is a policy choice the owner gets to make;
// silent versus stated is a trust choice, and the board's whole value is that
// its numbers are believable. One screenshot captioned "wfsim 偷偷上传你的配装"
// costs more than every submission it would ever gain.
//
// So the default is yes, and the line saying so is on screen from the moment
// the official scenario is active — before any run, with a one-click opt-out
// beside it. `check_official.mjs` asserts that pairing rather than asserting
// nothing leaves: what has to be true is that nothing leaves UNSAID.
const boardConsent = () => {
  try { return localStorage.getItem(BOARD_CONSENT) || "yes"; } catch (_) { return "yes"; }
};
/// Has the player actually decided, as against inheriting the default? Only
/// this tells the notice apart from the settled state.
const boardConsentChosen = () => {
  try { return localStorage.getItem(BOARD_CONSENT) !== null; } catch (_) { return false; }
};
// WHY the board turned this build away, in its own words. Kept beside
// `boardState` rather than inside it: the state is what happened, this is what
// was said, and only one of the states has anything to say.
let boardRefusal = "";
/// HOW MANY BOARDS TOOK IT, out of how many were asked. Null until a run has
/// asked — "sent" alone does not say whether it will be RANKED anywhere, which
/// is the question a submitter actually has.
let boardBoards = null;
const setBoardConsent = (v) => {
  try { localStorage.setItem(BOARD_CONSENT, v); } catch (_) { /* private mode */ }
  renderBoardConsent();
  refreshBoardDoor();
};

/// The submission itself: the BUILD, and nothing else about you.
/// The mod id a RIVEN takes in a board record — the bare word, because the
/// endpoint's ids are `[a-z0-9_]` and a riven's local name is one player's
/// label for their own item.
const BOARD_RIVEN_SLOT = "riven";

/// THE EQUIPPED RIVEN AS A SHAPE, for a board record: `{riven_pos, riven_neg}`,
/// or `{}` when the build wears none.
///
/// Two flat fields rather than an object, because that is the worker's shape —
/// it validates `id` and `ids` and has no game data to check anything richer
/// against. Spellings are per-protocol and always have been; what is shared is
/// the AXIS, which both fields name.
function boardRivenShape() {
  const slot = mainSlots().find((s) => isRivenId(s.mod));
  if (!slot) return { riven_pos: [], riven_neg: "" };
  const st = (loadPresetList(RIVENS).find(
    (p) => RIVEN_PREFIX + p.id === slot.mod) || {}).state || {};
  const ids = (xs) => (xs || []).map((x) => x && x.id).filter(Boolean);
  return {
    // SORTED, because a riven's stats do not combine with each other — two
    // players listing them in different orders described one riven and must
    // produce one row.
    riven_pos: ids(st.bonuses || st.positives).sort(),
    riven_neg: ((st.malus || st.curse) || {}).id || "",
  };
}

function boardPayload() {
  const bench = (scenarioNamed(activeScenario) || {}).builtin;
  return {
    // WHERE THE SUBMITTER HAPPENED TO BE STANDING — provenance, not identity. A submission has never carried a score, so the ruler
    // was never a property of the record; the store is a LIBRARY OF BUILDS and
    // every ruler crosses the whole of it. A build measured under a scenario of
    // the player's own has no ruler to name and simply omits the field.
    // `undefined`, not a conditional spread and not `null`: `JSON.stringify`
    // drops an undefined key, which is exactly "omit it on the wire", while the
    // key stays visible at this indentation to the cross-file check that reads
    // this literal against the worker's own table (`check_board_submit`). A
    // `null` would travel and the worker would refuse it as a non-string id.
    benchmark: bench || undefined,
    weapon: $("weapon").value,
    // HOW IT IS PLAYED, and it has to travel or the dimension is fed by
    // nothing. The scorer's fallback for a mode-less submission is "the cycle
    // where there is one", which is a MIGRATION rule for rows submitted before
    // the dimension existed — and while this field was missing it was the only
    // rule in play, so every Incarnon weapon's row said `cycle` and no board
    // could ever hold a base-form Torid. Measured on the published boards:
    // 62 cycle rows and 41 base ones, and not one weapon with both — every
    // Incarnon weapon cycle, every other one base, which is the fallback's
    // signature rather than anybody's choice.
    mode,
    // THE EIGHT MAIN SLOTS, and the exilus one is DROPPED here — it has to
    // happen on this side, because the payload is a flat list with no slot
    // positions and an exilus-eligible mod is legal in a MAIN slot. Sending all
    // nine turns away exactly the wrong people: a player who fills their exilus
    // slot sends 9 mods and is refused for not being a complete build.
    //
    // AS PLACED, not sorted: mods combine elements in the order they sit in,
    // so sorting here submits a build the player never made — on the Torid,
    // 12,424 DPS against 46,583. The scorer canonicalises with the pool in
    // front of it, the only place that knows an elemental mod.
    // THE RIVEN RIDES AT ITS OWN POSITION, under the bare id `riven`, because
    // position is the build: an elemental riven pairs with the build's other
    // elementals. The endpoint's ids are `[a-z0-9_]`, so its local name cannot
    // travel — and should not, being what one player called their item.
    // …AND THE STANCE RIDES WITH THEM, appended. It needs no field of its own
    // because a stance mod is legal in the stance slot and NOWHERE else, which
    // is what the exilus slot cannot say and why THAT one is its own key.
    // APPENDED RATHER THAN INSERTED, because the order of `mods` pairs the
    // elementals and a stance carries no element.
    mods: mainSlots()
      .filter((s) => s.mod)
      .map((s) => (isRivenId(s.mod) ? BOARD_RIVEN_SLOT : slotModId(s)))
      .concat((slots[STANCE] || {}).mod ? [slotModId(slots[STANCE])] : []),
    // …AND WHAT THAT SLOT HOLDS, as a SHAPE. Which stats, and which is the
    // malus — never the rolls: the board scores a shape at its own ceiling, the
    // same way it scores every row at full Forma and every valence at the
    // roll's maximum. What one copy landed on is luck.
    //
    // STATED EVEN WHEN EMPTY, like `valence` beside it: a payload that names
    // every axis is one a reader can check against the worker's table, and the
    // endpoint drops an empty one rather than storing it.
    riven_pos: boardRivenShape().riven_pos,
    riven_neg: boardRivenShape().riven_neg,
    evolutions: Object.values(evoSel).filter(Boolean),
    arcanes: arcanes.slice(),
    // THE PROGENITOR ELEMENT, for an adversary weapon. The ELEMENT only: the
    // ruler scores every row at the roll's maximum, so the percentage is not a
    // row's to state — every player can Valence-fuse to it, which makes it
    // investment rather than a choice (the same rule that scores every row at
    // full Forma).
    valence: valence.element,
    // THE EXILUS SLOT'S MOD, as its own field — optional on every ruler, and
    // sent rather than dropped here and never counted.
    //
    // IT HAS TO BE ITS OWN FIELD, which is the same reason it was dropped
    // rather than appended before: the payload's `mods` is a flat list with no
    // slot positions, and an exilus-eligible mod is legal in a MAIN slot, so
    // nothing downstream could tell which entry came out of the exilus slot.
    // Only the page knows, because only the page has the slots.
    //
    // WHY IT IS COUNTED NOW: "exilus mods are handling and mobility, with no
    // single-target damage model" was true of most of the pool and false of the
    // part that decides a group fight — BEAM RANGE is exilus, and beam range is
    // how many bodies a beam reaches.
    exilus: (slots[boardBuildMods()] || {}).mod || undefined,
    // THE PARTS of a modular weapon, flat, because that is the worker's shape.
    // Empty strings on everything else — the field is always sent so a record
    // written today and one written by a Kitgun submitter have the same keys,
    // and the scorer's own fallback never has to guess which kind it is looking
    // at.
    grip: (assembly && assembly.grip) || "",
    loader: (assembly && assembly.loader) || "",
  };
}

/// THE SAME PAYLOAD, FROM A SEARCH RESULT rather than from the page.
///
/// A FINALIST IS A BUILD AND THE BOARD TAKES BUILDS. A search that ranks twenty
/// of them uploaded NONE: the only path to the store ran off a simulator run,
/// one build at a time, so the strongest thing this app produces reached the
/// board only if a player copied a row into the builder by hand and ran it
/// again. Every axis below comes off the ROW — mods, arcanes, evolutions, mode,
/// valence, exilus — because the page's own state is a different build.
///
/// THE RIVEN AND THE ASSEMBLY COME FROM THE PAGE, and that is not an
/// inconsistency: the search was handed the riven the player defined and the
/// weapon they assembled, so those are the row's too. It has no separate answer
/// to give.
/// WHICH MOD CAME OUT OF THE EXILUS SLOT, said once for the two places that
/// need it — the list it must leave and the key it must arrive in.
const exilusOf = (res) =>
  (res && res.exilus && res.exilus !== "none" ? res.exilus : undefined);

function boardPayloadFromResult(res) {
  const bench = (scenarioNamed(activeScenario) || {}).builtin;
  const arcs = asArcaneList(res.arcane, (res.arcane || []).length)
    .filter((a) => a && a !== "none");
  return {
    benchmark: bench || undefined,
    weapon: $("weapon").value,
    mode: res.mode || mode,
    // AS RANKED, not sorted — the order pairs the elementals, and a sorted list
    // is a build the search never measured. The same rule `boardPayload` states.
    // THE EIGHT MAIN SLOTS, and the exilus one is DROPPED — the same rule
    // `boardPayload` states above, and it has to be stated twice because this
    // path builds its list from a RESULT rather than from the slots. A flat
    // list has no positions and an exilus-eligible mod is legal in a main slot,
    // so the endpoint reads a ninth entry as a ninth MAIN mod and refuses the
    // build for having nine. Which one it is, is `exilus` below.
    // …AND THE STANCE RIDES WITH THEM, the same rule `boardPayload` states. The
    // search is HANDED the builder's stance and pins it into every candidate,
    // so a result already names it — the append is for a result that predates
    // that (a saved checkpoint), and it is conditional so the same card cannot
    // arrive twice.
    mods: (() => {
      const st = (slots[STANCE] || {}).mod;
      const out = (res.mods || [])
        .filter((m) => !exilusOf(res) || m !== exilusOf(res))
        .map((m) => (isRivenId(m) ? BOARD_RIVEN_SLOT : m));
      return st && !out.includes(st) ? out.concat([st]) : out;
    })(),
    riven_pos: boardRivenShape().riven_pos,
    riven_neg: boardRivenShape().riven_neg,
    evolutions: (res.evolutions || []).slice(),
    arcanes: arcs,
    valence: res.valence || valence.element,
    exilus: exilusOf(res),
    grip: (assembly && assembly.grip) || "",
    loader: (assembly && assembly.loader) || "",
  };
}

/// EVERY FINALIST, THROUGH THE BOARD'S OWN DOOR.
///
/// NO CLIENT-SIDE PRE-FILTER, HERE OR ANYWHERE. A search result can be short —
/// how full a build must be is the searcher's own setting, and a seven-mod
/// scope produces seven-mod winners — and those are refused by
/// `/api/board/check`, which IS `validate_for_board` rather than a copy of it:
/// a second implementation is a second answer, and the player has to be given
/// the board's. The simulator's path asks the same door for the same reason.
///
/// NO CAP ON HOW MANY. The finalist count is the searcher's setting, they are
/// all real builds, and the store is keyed by identity — so twenty submissions
/// of which twelve are already held collapse onto twelve rows.
async function offerOptBoardSubmit(r) {
  const box = $("opt-board");
  if (!box) return;
  const rows = (r.results || []).filter((x) => x && (x.mods || []).length);
  if (!rows.length) return;
  if (boardConsent() !== "yes") {
    box.innerHTML = `<span class="ob-off">${escHtml(
      tr("board upload is off, so these were not sent"))}</span>`;
    return;
  }
  box.innerHTML = `<span class="ob-run">${escHtml(
    tr("sending {n} finalists to the board…").replace("{n}", rows.length))}</span>`;
  let sent = 0;
  const refused = new Map();
  let failed = 0;
  // ONE AT A TIME. The door is a real request and so is the store's; a search
  // with fifty finalists firing fifty of each at once is a burst nobody asked
  // for, and nothing here is waiting on the answer.
  for (const res of rows) {
    const body = boardPayloadFromResult(res);
    try {
      const verdict = await boardVerdict(body);
      if (verdict && verdict.ok && verdict.accepted === false) {
        const why = verdict.reason || tr("refused");
        refused.set(why, (refused.get(why) || 0) + 1);
        continue;
      }
      const ok = await fetch("/api/board/submit", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
      }).then((x) => x.ok).catch(() => false);
      if (ok) sent += 1; else failed += 1;
    } catch (_) {
      failed += 1;
    }
  }
  // AGGREGATE, because the reader asked one question and got twenty answers.
  // The REASONS are counted rather than listed: twenty rows off one search
  // differ in mods and not in why the board would not take them.
  const parts = [tr("{n} of {of} finalists uploaded")
    .replace("{n}", sent).replace("{of}", rows.length)];
  for (const [why, n] of refused) parts.push(`${n} × ${why}`);
  if (failed) parts.push(tr("{n} could not be sent").replace("{n}", failed));
  box.innerHTML = `<span class="${sent ? "ob-sent" : "ob-off"}">${
    escHtml(parts.join(" · "))}</span>`;
}

let boardState = "";   // "" | "sent" | "failed" | "onboard"
/// THE ROW THIS BUILD ALREADY IS, when it is one — a `builtinBuilds()` entry,
/// so it can be NAMED ("#1 · Incarnon cycle") rather than merely asserted.
let boardOnBoard = null;

/// How many mods a board build is, from the ENGINE via META — never a literal
/// here. The rule is `board::builds::validate_for_board`; this is the page repeating
/// what it was told so it can explain itself before sending nothing.
const boardBuildMods = () => (META || {}).board_build_mods || 8;
/// The slots a benchmark build is made of: the main ones. `slots` holds nine —
/// the exilus slot is the last — and the benchmark does not count it.
const mainSlots = () => slots.slice(0, boardBuildMods());
/// Filled main slots. The exilus slot is not part of the answer either way.
const buildMods = () => mainSlots().filter((s) => s.mod).length;

/// WHAT THE ACTIVE BENCHMARK ADMITS — its own `build` block, from META. Not a
/// constant here: admission is the benchmark's, so a second ruler answers
/// differently and this page must not assume otherwise.
const boardRequirement = () => {
  const id = (scenarioNamed(activeScenario) || {}).builtin;
  const b = (META.benchmarks || []).find((x) => x.id === id);
  return (b && b.build) || {};
};

/// WHAT THE BOARD SAYS ABOUT THE BUILD ON SCREEN — its own words, cached.
///
/// ONE RULE, AND IT IS THE BOARD'S. This was a second implementation of
/// admission living on the page — how many mods, how many evolution tiers, how
/// many arcane seats, and a capacity floor of its own — and a second
/// implementation is a second answer. It drifted exactly where a copy does: the
/// floor it computed ignored the capacity a STANCE hands back, so a melee build
/// that fits could be told it does not and never reach the door at all.
///
/// `/api/board/check` IS `validate_for_board`, the same call the scorer makes.
/// Asking costs nothing — it is the engine already in this page — and what it
/// answers is what will happen, which no copy can promise.
///
/// `null` = accepted, or not asked yet. A string = the board's own reason.
let boardDoor = null;

/// Ask the door about the build on screen, then repaint what says so.
///
/// TRAILING, because the build changes a slot at a time and the answer is only
/// wanted about the one the reader stopped on. `boardDoorGen` drops a reply
/// that lost the race, so a slow answer about an old build cannot land on top
/// of a fast answer about the current one.
let boardDoorGen = 0;
let boardDoorTimer = null;
function refreshBoardDoor() {
  clearTimeout(boardDoorTimer);
  const gen = ++boardDoorGen;
  boardDoorTimer = setTimeout(async () => {
    if (boardConsent() !== "yes" || officialBuildActive()) {
      boardDoor = null;
      return;
    }
    const body = boardPayload();
    if (!body) return;
    const v = await boardVerdict(body);
    if (gen !== boardDoorGen) return;
    boardDoor = v && v.ok && v.accepted === false ? (v.reason || "") : null;
    renderBoardConsent();
    renderBoardOutcome();
  }, 250);
}

/// WHICH RULERS WOULD TAKE THIS BUILD, asked of the board's own door.
///
/// `/api/board/check` is `validate_for_board` itself — the same call the scorer
/// makes, not a copy of its rules, because a second implementation is a second
/// answer and the player has to be given the board's.
///
/// FROM A RULER, it is asked about that one. FROM A FIGHT OF YOUR OWN there is
/// no ruler to name, so it is asked about ALL of them: a build only has to
/// qualify somewhere to be worth uploading, and "it qualifies for 2 of 3" is
/// the one thing a reader can act on. The page is not predicting a SCORE here —
/// that is the scorer's and always was — only whether the door opens at all.
async function boardVerdict(body) {
  const ids = body.benchmark ? [body.benchmark] : benchList().map((b) => b.id);
  if (!ids.length) return { ok: true, accepted: true, boards: 0, of: 0 };
  let accepted = 0;
  let reason = "";
  for (const id of ids) {
    const v = await api("/api/board/check", { ...body, benchmark: id });
    if (!v || !v.ok) return { ok: false };
    if (v.accepted === false) reason = reason || v.reason || "";
    else accepted += 1;
  }
  return { ok: true, accepted: accepted > 0, reason, boards: accepted, of: ids.length };
}

/// **IS THIS BUILD ALREADY A ROW?** — asked of the ENGINE, in one call.
///
/// Answering it with a POINTER fails: `officialBuildActive()` says whether the
/// ACTIVE PRESET is a builtin, true of a board row opened from the picker and
/// false of the same build reached any other way — so a player who copies one
/// into a preset of their own is told their run is being uploaded to a board
/// that already holds it.
///
/// A BUILD IS NOT ITS SPELLING, which is why this cannot be a comparison here:
/// `board::builds::canonical_mods` sorts the non-elementals by drain and leaves the
/// elementals in the order that PAIRS them, evolutions are a set, a riven is a
/// shape rather than its rolls, and only the engine has the mod POOL that tells
/// an elemental mod from any other. `/api/build/keys` IS `board::builds::board_key`,
/// the key the scorer files rows under, asked of the build on screen and every
/// row this weapon holds in one pass.
///
/// A MATCH IS PROOF, AN ABSENCE IS NOT. The board LISTS only builds scoring at
/// least half their weapon's leading row, so a build the store already holds
/// can be missing from the board — which is why this only ever suppresses an
/// upload it can prove is redundant, and never claims the reverse.
async function boardRowMatching(body) {
  const rows = builtinBuilds();
  if (!rows.length) return null;
  const asBuild = (r) => ({
    weapon: body.weapon,
    mods: r.board.mods || [],
    evolutions: r.board.evolutions || [],
    arcanes: r.board.arcanes || [],
    valence: r.board.valence || "",
    exilus: r.board.exilus || undefined,
    mode: r.board.mode || "base",
    // A ROW'S RIVEN IS A SHAPE, in the same two fields the submission uses —
    // the ROLLS beside it are what this engine happened to settle on and are
    // not part of the build.
    riven_pos: (r.board.riven || {}).bonuses || [],
    riven_neg: (r.board.riven || {}).malus || "",
  });
  const res = await api("/api/build/keys", { builds: [body].concat(rows.map(asBuild)) });
  const keys = res && res.ok ? res.keys || [] : [];
  if (!keys[0]) return null;
  const i = keys.findIndex((k, n) => n > 0 && k === keys[0]);
  return i > 0 ? rows[i - 1] : null;
}

/// That row, named the way the picker names it: the ruler it is on and its rank
/// within it. A bare "already on the board" would leave the reader hunting for
/// which row they are looking at.
const boardRowLabel = (r) =>
  `${r.group}${r.name ? ` · ${r.name}` : ""}`;

async function offerBoardSubmit() {
  // NO RULER GATE. Any fight can upload, because what is uploaded is the BUILD
  // and the number is produced by the scorer under ITS fight. Of 914 distinct builds players had submitted, only 46 had ever
  // been scored on more than one board — the gate was holding 95% of everything
  // anyone contributed back from the boards it could also have answered.
  if (officialBuildActive()) return;          // a board row does not resubmit itself
  if (boardConsent() !== "yes") { renderBoardConsent(); renderBoardOutcome(); return; }
  const body = boardPayload();
  if (!body) return;
  // ALREADY A ROW? Asked before the door is, because a build the board already
  // holds has nothing to be refused ABOUT — sending it again would be answered
  // by the store collapsing it onto the row it is, silently, after the reader
  // has been told it was uploaded.
  boardOnBoard = await boardRowMatching(body);
  if (boardOnBoard) {
    boardState = "onboard";
    renderBoardConsent();
    renderBoardOutcome();
    return;
  }
  // THE BOARD'S OWN DOOR, ASKED BEFORE KNOCKING. The store accepts anything;
  // the SCORER decides, an hour later, in a workflow log — so a build the board
  // will never take looked exactly like one it took, forever. Three Kuva Nukor
  // submissions sat there carrying no progenitor element, refused on every run
  // since they arrived, while this panel said "sent".
  //
  // `/api/board/check` is `validate_for_board` itself, the same call the scorer
  // makes — not a copy of its rules, because a second implementation is a
  // second answer and the player has to be given the board's.
  const verdict = await boardVerdict(body);
  boardBoards = verdict && verdict.ok ? { n: verdict.boards, of: verdict.of } : null;
  // ONE ANSWER, TWO READERS. The panel above the run and the line under it both
  // say what the board thinks of this build, and they say it from here.
  boardDoor = verdict && verdict.ok && verdict.accepted === false
    ? (verdict.reason || "") : null;
  if (verdict && verdict.ok && verdict.accepted === false) {
    boardState = "refused";
    boardRefusal = verdict.reason || "";
    renderBoardConsent();
    renderBoardOutcome();
    return;
  }
  boardRefusal = "";
  try {
    // A REAL fetch, not `api()`. Every other endpoint is answered by the engine
    // — locally by the dev server, in the browser by the wasm worker — and the
    // board is the one thing neither of them can answer: it is a service, not a
    // calculation. Routing it through `api()` would hand the path to a worker
    // that has never heard of it.
    //
    // SAME ORIGIN, deliberately. A separate api domain is a second DNS name and
    // a second thing that can be blocked, which is the failure the art rule was
    // written about ("unreliable to blocked from mainland China, i.e. precisely
    // where the players are"). The board lives under this site's own origin.
    const res = await fetch("/api/board/submit", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    boardState = res.ok ? "sent" : "failed";
  } catch (_) {
    // Never an error dialog: a board that is unreachable is not a failed run.
    boardState = "failed";
  }
  renderBoardConsent();
  renderBoardOutcome();
}

/// THE LINE THE BOARD DRAWS, said to the person it applies to. A submission
/// below half its weapon's leading row is stored, scored and then not listed,
/// which from the submitter's side is indistinguishable from one that was lost
/// — and that exact silence has cost this board two migrations already. The
/// rule is stated rather than the consequence counted: a number of hidden rows
/// would need the scorer to publish one, while the RULE is what makes an
/// absence readable, and it is checkable against the board on screen.
const boardCutNote = () =>
  tr("Only builds scoring at least half their weapon's leading row are listed — the board holds the answers, not every attempt.");

/// **WHAT HAPPENED TO THIS RUN**, in one sentence — the ONE statement of it.
///
/// It is written once and read twice: by the consent box at the top of the Sim
/// panel, where the standing policy lives, and by the RESULT, where the reader
/// actually is when they want to know. A run that was sent
/// and a run that was never going to be sent looked identical from the result,
/// which is the same silence the consent box itself was written to end — a
/// player who built their own scenario, ran it, and watched the board.
///
/// A second copy of these branches would be a second answer to "was it sent",
/// which is what `/api/board/check` exists not to have.
function boardRunOutcome() {
  if (officialBuildActive()) {
    return { kind: "none", text: tr("this build is already a row on the board — running it sends nothing") };
  }

  if (boardConsent() !== "yes") return { kind: "none", text: tr("nothing is sent from here") };
  // AN INCOMPLETE BUILD IS A REFUSED ONE, said in the board's own words. It had
  // a branch of its own here fed by a page-side copy of admission, which is how
  // the two could disagree; there is one rule now and one sentence for it.
  if (boardDoor) {
    return { kind: "short",
      text: tr("{what} — the board takes a weapon built as far as it goes, so this one is not sent")
        .replace("{what}", boardDoor) };
  }
  if (boardState === "onboard" && boardOnBoard) {
    // NAMED, not merely asserted. "It is already there" that cannot say WHERE
    // is indistinguishable from a page that declined to send for its own
    // reasons, which is the silence this whole panel exists to end.
    return { kind: "none",
      text: tr("this build is already on the board ({row}) — nothing was sent")
        .replace("{row}", boardRowLabel(boardOnBoard)) };
  }
  if (boardState === "refused") {
    return { kind: "refused",
      text: `${tr("the board would not take this build, so nothing was sent")}: ${boardRefusal}` };
  }
  if (boardState === "failed") {
    return { kind: "failed", text: tr("could not reach the board — nothing was sent") };
  }
  if (boardState === "sent") {
    // WHICH BOARDS, when there is more than one to qualify for. "Sent" alone
    // does not say whether it will be RANKED anywhere, which is the question a
    // submitter actually has — and from a fight of your own it is the only
    // thing the page can honestly answer, since the SCORE is the scorer's.
    //
    // IT SAYS "A BUILD" AND NOT "THIS RUN", because that is what was stored: no
    // submission carries a name, a score or the mode it was tuned for, so the
    // scorer plays it in every mode the weapon has and every ruler crosses the
    // whole library. A sentence about "your run on this mode" would describe a
    // record that does not exist and leave the reader looking for one row when
    // there may be four.
    const b = boardBoards;
    const where = b && b.of > 1
      ? " " + tr("({n} of {of} boards will take it)")
        .replace("{n}", b.n).replace("{of}", b.of)
      : "";
    return { kind: "sent",
      text: tr("uploaded — a submission is a BUILD, so it is scored in every mode this weapon can be played and on every board that takes it. It is stored the moment it arrives; the board re-scores hourly and picks it up on its next run, and says how long ago that was") + where };
  }
  // NOT YET ANSWERED. `offerBoardSubmit` runs after the result is drawn, so
  // this is the state the first paint is in and it has to say so rather than
  // claiming either outcome.
  return { kind: "pending", text: tr("submitting to the board…") };
}

/// WHERE THIS RUN WOULD STAND, answered without waiting for the pipeline.
///
/// It returns null wherever the two numbers are not ONE number, which is most
/// of the reasoning — docs/BOARD.md §"The standing a submitter sees at once".
///
/// A PROJECTION IS NOT A ROW: it is against the board as it stands, it is shown
/// to the submitter alone, and it is never sent. The ranking holds numbers this
/// project measured, which is the whole of where it gets its authority.
function boardProjection() {
  // A BOARD THAT DID NOT LOAD IS NOT AN EMPTY BOARD. Both leave `BOARD[w.id]`
  // undefined, and the second is ordinary — a weapon nobody has submitted is
  // the invitation — but the first would answer "#1 of 1" to a reader whose
  // network dropped one file. So the file has to be here before this speaks.
  const body = boardPayload();
  if (!body || !body.benchmark) return null;
  if ((body.mods || []).includes(BOARD_RIVEN_SLOT) || body.valence) return null;
  const w = weaponInfo(body.weapon) || {};
  if (!BOARD_HAVE.has(w.id)) return null;
  const p = loadPresetList(BUILDS).find((z) => z.name === activePreset);
  const r = p && p.lastResult && p.lastResult.r;
  if (!r || !w.id) return null;
  const met = metricOf((scenarioNamed(activeScenario) || {}).metric);
  const mine = metricValue(met, r);
  if (!(mine > 0)) return null;
  // THE CELL, spelled the way `builtinBuilds` spells it — one benchmark, one
  // mode, one riven-ness. A rank across anything wider names nothing.
  const rows = (BOARD[w.id] || []).filter((x) =>
    x.benchmark === body.benchmark
    && (x.mode || "base") === (body.mode || "base")
    && !rowHasRiven(x));
  const ahead = rows.filter((x) => (x.score || 0) > mine).length;
  return {
    text: fmtScore(mine)
      + " " + metricLabel(met),
    place: ahead + 1,
    of: rows.length + 1,
    group: `${benchmarkName(body.benchmark)} · ${modeLabel(w, body.mode || "base")}`,
  };
}

/// The result's own copy of that sentence. Re-rendered when the verdict lands.
function renderBoardOutcome() {
  const box = $("sim-board-outcome");
  if (!box) return;
  const o = boardRunOutcome();
  const stand = o.kind === "sent" ? boardProjection() : null;
  box.className = `board-outcome ${o.kind}`;
  box.innerHTML = `<div class="bo-h">${escHtml(o.text)}</div>`
    // THE CUT LINE FOLLOWS A SENT RUN, because that is the reader who is about
    // to go and look for a row that may never appear — and it is its OWN line.
    // Trailing it onto the sentence above made one run-on paragraph out of two
    // separate facts: what happened to this run, and the rule that decides
    // whether it will be listed.
    // THE STANDING FIRST, then the rule that decides listing. A submitter reads
    // the panel to find out how they did; the cut line answers a question they
    // have not asked yet, and putting it first buries the answer under it.
    + (stand
      ? `<div class="board-place">${escHtml(
        tr("this run measured {v} — #{n} of {of} in {group}, against the board as it stands")
          .replace("{v}", stand.text)
          .replace("{n}", stand.place)
          .replace("{of}", stand.of)
          .replace("{group}", stand.group))}</div>` : "")
    + (o.kind === "sent"
      ? `<div class="board-state">${escHtml(boardCutNote())}</div>` : "")
    // THE SWITCH STANDS WHERE THE OUTCOME IS READ. Submission is automatic and
    // default-on, so the line above is a statement about something that has
    // ALREADY happened — and the one control it wants is the one that stops it
    // happening again. It sat only in the scenario block above the fight, which
    // is where a first-time reader is warned; this is where a reader who just
    // watched it happen looks.
    //
    // ONE STATE, TWO SWITCHES. Both read and write `boardConsent()` and both
    // repaint, so there is no second answer to keep in step — and the sentence
    // beside it says what travels, because the strongest thing this mechanism
    // has going for it is that the SCORE is not sent: the board re-runs it.
    + `<div class="bo-consent">
        <span>${escHtml(boardConsent() === "yes"
          ? tr("the build travels — the weapon, its mods, evolutions and arcanes. No account, no names you gave anything, and no score: the board measures it again itself.")
          : tr("nothing is sent from here."))}</span>
        <button type="button" class="ghost-btn small" id="bo-consent">${escHtml(
          boardConsent() === "yes" ? tr("turn automatic submission off") : tr("turn automatic submission on"))}</button>
      </div>`;
  const sw = $("bo-consent");
  if (sw) sw.onclick = () => { setBoardConsent(boardConsent() === "yes" ? "no" : "yes"); renderBoardOutcome(); };
}

function renderBoardConsent() {
  const box = $("board-consent");
  if (!box) return;
  // TWO REASONS NOTHING IS SENT, and they must not collapse into one silence.
  // A board ROW explains itself elsewhere ("it is already a row on the
  // board"); with no box on a player's OWN fight, someone who builds a
  // scenario, runs it and watches the board never learns why nothing appeared
  // (player report via the owner, 2026-08-10).
  if (officialBuildActive()) { box.hidden = true; return; }
  box.hidden = false;
  const c = boardConsent();
  // THE FLOOR IS A SUFFIX, NOT A REPLACEMENT. While the build is half-built the
  // standing policy is still the thing a reader most needs to know — replacing
  // it with "this one is not sent" would state the exception and hide the rule,
  // which is the same silence the default-on setting exists not to have.
  const floorNote =
    c === "yes" && boardDoor
      ? ` <span class="board-state">` +
        escHtml(tr("{what} — the board takes a weapon built as far as it goes, so this one is not sent")
          .replace("{what}", boardDoor)) +
        `</span>`
      : "";
  if (!boardConsentChosen()) {
    // THE DEFAULT, STATED. Not a question — the answer is already yes — so it
    // reads as what will happen and what it contains, with the way out next to
    // it. Asking would be dishonest when the default has already decided.
    box.innerHTML =
      `<b>${escHtml(tr("Builds you run here are added to the official board."))}</b> ` +
      escHtml(tr("What is sent is the BUILD: the weapon and its mods, evolutions and arcanes. Not the fight you ran it under — the board scores every build under its OWN rulers, so any scenario can contribute. And nothing about you: no account, no identifier, no address, no time finer than the day, and no score. The board takes a weapon built as far as it goes: every main slot filled, and the exilus slot counted if you use one.")) +
      // WHEN, on the FIRST visit too — this is the branch a new player reads,
      // and saying it only after the consent had been chosen told the fact to
      // everyone except the person meeting the board for the first time.
      ` ${escHtml(tr("A run is stored the moment it arrives and appears on the board at its next re-score — the board re-scores hourly, not the instant you send it. It says how long ago it was scored."))}` +
      ` ${escHtml(boardCutNote())}` +
      floorNote +
      ` <button class="ghost-btn small" id="board-no">${escHtml(tr("don't submit"))}</button>`;
    $("board-no").onclick = () => setBoardConsent("no");
    return;
  }
  // WHEN, not just whether. A submission is stored the moment it is sent and
  // the board is re-scored on a schedule, so "sent" and "on the board" are an
  // hour apart — long enough that a player concludes it failed. The number is
  // the workflow's own: one run an hour, and GitHub's scheduler slips about
  // fifteen minutes whatever minute is named.
  // THE SAME SENTENCE THE RESULT SHOWS — see `boardRunOutcome`. Before any run
  // this box states the standing POLICY instead, which is what a reader at the
  // top of an unrun panel needs; the result says what happened to a run.
  const state = c !== "yes"
    ? tr("nothing is sent from here")
    : boardState === ""
      ? tr("builds you run here are submitted — stored the moment they arrive, and picked up at the board's next hourly re-score")
      : boardRunOutcome().text;
  box.innerHTML =
    `<span class="board-state">${escHtml(state)}</span>` +
    // THE RULE FOLLOWS THE STATE, and only where it can bite: a reader who
    // has turned submission off is not waiting for a row to appear.
    (c === "yes" ? ` <span class="board-state">${escHtml(boardCutNote())}</span>` : "") +
    floorNote + ` ` +
    `<button class="ghost-btn small" id="board-flip">${escHtml(c === "yes" ? tr("stop submitting") : tr("start submitting"))}</button>`;
  $("board-flip").onclick = () => setBoardConsent(c === "yes" ? "no" : "yes");
}

// The official BUILD, on screen. Same contract as the scenario's lock, but the
// build editor is mostly CLICK HANDLERS on divs (a slot, a polarity, an
// evolution tile) rather than form controls, and `disabled` means nothing to a
// div. `pointer-events: none` on the whole region is the honest equivalent:
// it covers everything the region can ever grow, including a control nobody
// has written yet.
function lockOfficialBuild() {
  const on = officialBuildActive();
  // MODE IS PART OF THE BUILD, so it locks with the build. It was the one
  // control on a read-only board row that stayed live, and the consequence was
  // silent in both directions: switching a #1 Felarx to its base form ran the
  // base form, wrote nothing (`markPresetDirty` refuses an official build), and
  // sent nothing (`offerBoardSubmit` refuses one too) — so a player testing the
  // base form several times saw no row appear and nothing on screen said why.
  // EVERY BUILDER STEP, derived — see `builderSteps`. It was four ids, which
  // is why the Parts block stayed live on a read-only board row. The Stats
  // panel is deliberately not among them: there is nothing in it to click.
  builderSteps().forEach((b) => {
    b.classList.toggle("locked-hard", on);
    // …AND IT SAYS WHY, on the block a player is trying to click. A slot that
    // simply does not react teaches nothing; this names the reason and the way
    // out, and clicking anywhere in the block takes it.
    if (on) {
      b.title = tr("this is a benchmark row — copy it to edit");
      b.onclick = () => copyActivePreset(buildBarCfg());
    } else {
      b.removeAttribute("title");
      b.onclick = null;
    }
  });
}

// The official scenario, ON SCREEN: every control in the fight goes inert and
// the note says why and what to do instead.
//
// This is the VISIBLE half of read-only, and deliberately only the visible
// half — the rule is enforced in `markScenarioDirty`, which is where a write
// would actually happen. A disabled input that auto-save still read would be a
// lie told twice, so the two are separate on purpose.
function lockOfficialScenario() {
  const note = $("sim-official");
  const on = officialScenarioActive();
  // `sim-whole-fight-body` IS IN THE SWEEP, and it was missing for as long as
  // the panel existed. It draws a live control for every editable
  // axis of the fight, so a ruler's pinned level, duration and distance were
  // all editable there — auto-save refuses to write them, which hides the fault
  // rather than preventing it: `sim` still moves in memory, so Run Sim reports
  // a modified ruler under the ruler's own name, which is the exact thing this
  // lock exists to stop. An escape hatch is a place to LOOK at the whole
  // document, not a second editor that outranks the rule.
  const boxes = ["sim-target", "sim-technique", "sim-wfbuffs", "sim-limits", "sim-run",
                 "sim-buffs", "sim-whole-fight-body"]
    .map((id) => $(id)).filter(Boolean);
  // The ARENA's bodies are SVG circles and the input sweep below cannot reach
  // them, so the scene is marked here and refuses the gesture itself.
  ["sim-target-arena", "opt-target-arena"].forEach((id) => {
    const a = $(id);
    if (a) a.classList.toggle("ar-ro", on || id.startsWith("opt-"));
  });
  boxes.forEach((b) => {
    b.classList.toggle("locked", on);
    b.querySelectorAll("input,select,button,textarea").forEach((el) => {
      // ONLY UNLOCK WHAT THIS LOCKED. The first shape remembered each
      // element's prior state and restored it, which is a bookkeeping problem
      // that has to survive every re-render and did not: a second lock pass
      // recorded "was disabled" for an element this had just disabled, so the
      // unlock left it inert and a COPIED scenario could not be edited — the
      // one thing copying is for.
      //
      // Marking instead is idempotent by construction. An element disabled for
      // its OWN reason (a sentinel's infinite-ammo box, ticked and disabled
      // whatever scenario is open) is never marked, so it is never re-enabled
      // here and the page cannot contradict the mechanic.
      if (on) {
        if (!el.disabled) { el.disabled = true; el.dataset.officialLock = "1"; }
      } else if (el.dataset.officialLock) {
        el.disabled = false;
        delete el.dataset.officialLock;
      }
    });
  });
  if (!note) return;
  note.hidden = !on;
  if (on) {
    // A DOOR, NOT A WALL. This is now the scenario a first-time visitor lands
    // on, so the first thing they see is a panel of greyed-out controls — which
    // reads as broken until you know why. The note has always explained it and
    // pointed at the ⧉ on the chip; a chip they have not learned to look at yet
    // is not a route. The button does the same copy, where the locked controls
    // are.
    note.innerHTML =
      `<b>${escHtml(tr("Official test scenario"))}</b> — ` +
      escHtml(tr("the same fight on every weapon, so results can be compared. It cannot be edited.")) +
      ` <span class="official-def">${escHtml((scenarioNamed(activeScenario) || {}).name || "")}</span>` +
      ` <button class="ghost-btn small" id="sim-official-copy">⧉ ${escHtml(tr("edit a copy of this fight"))}</button>`;
    const cp = $("sim-official-copy");
    // The SAME copy the chip's ⧉ performs — `copyActivePreset` with the
    // scenario bar's own config — so there is one implementation of "make me an
    // editable copy" and it cannot drift from the bar.
    if (cp) cp.onclick = () => copyActiveScenario();
  }
}

// Section 2 — one card per configurable buff of the current build (from the
// last /api/panel `buffs`). Each: initial stacks (stepper / on-off) + lock.
// Missing configs default to the buff's `default_*`; ids no longer present are
// dropped from the payload (kept in `sim.buffs` for preset round-trips).
function syncBuffConfig(list, cfg) {
  list.forEach((b) => {
    if (!cfg[b.id]) cfg[b.id] = { stacks: b.default_stacks, locked: b.default_locked };
    else if (!b.uncapped) cfg[b.id].stacks = Math.min(cfg[b.id].stacks, b.max_stacks);
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
  // EVOLUTIONS grant buffs too (Overwhelming Attrition, Lethal Rearmament),
  // and they were missing from this lookup — so those two cards were the only
  // ones on the page still in English.
  const evos = (weaponInfo($("weapon").value).evolutions || [])
    .flatMap((tier) => tier.options || []);
  const owner = [...(META.mods || []), ...(META.arcanes || []), ...evos,
    ...Object.values(META.mod_pools || {}).flat()]
    .find((x) => (x.name_en || x.name) === head);
  // NO OWNER MEANS THE NAME IS OURS. While every card here is a mod's, an
  // arcane's or an evolution's, a head with no owner can only be a lookup that
  // has gone stale — and falling through to the English is the
  // right way to notice. A WEAPON PASSIVE has no owner by construction (the
  // sniper's Shot Combo Counter is the weapon's, not a mod's), so its name is
  // an ordinary UI string and belongs in the overlay like every other one.
  const label = owner ? owner.name : tr(head);
  return tail ? `${label} (${tr(tail.replace(/\)$/, ""))})` : label;
}

function renderBuffCards(box, list, cfg, have, opts = {}) {
  if (!box) return;
  syncBuffConfig(list, cfg);
  if (!list.length) {
    box.innerHTML = `<div class="sim-empty">no configurable buffs here.</div>`;
    return;
  }
  const card = (b) => {
    const c = cfg[b.id];
    // ONE control for every buff, a toggle included: a
    // one-stack buff reads "1 / 1" like the rest instead of a checkbox that
    // said "active" and meant the same thing in different words.
    // WHERE THE RUN STARTS, not what the buff is worth. A timed buff opens at
    // 0 because the modelled fight is "in it a while, but not in contact for
    // the last few seconds" — it is earned back on its own trigger. Only a
    // permanent buff opens full, because a lull cannot take it away.
    const startWhy = b.permanent
      ? tr("permanent — nothing grants or decays it, so it holds all run")
      : tr("stacks the run STARTS with. 0 = earned in-fight on this buff's own trigger, which is what a fight that has not been in contact for a few seconds looks like");
    // UNCAPPED buffs exist: Secondary Enervate ramps a stack per hit with no
    // ceiling until a big crit wipes it. `/ ∞` is the honest maximum, and the
    // input takes no `max` — clamping it to a number we invented would be the
    // one place the UI disagreed with the mechanic.
    const cap = b.uncapped ? "∞" : b.max_stacks;
    const ctl = `<span class="bstep" title="${escHtml(startWhy)}"><input type="number" data-b="${b.id}" data-f="stacks" min="0"${b.uncapped ? "" : ` max="${b.max_stacks}"`} value="${c.stacks}"><span class="bmax">/ ${cap}</span></span>`;
    // NOT "lock": that read as "freeze this buff", so
    // locking one at zero looked like a way to switch it off forever. It only
    // removes the TIMEOUT — the count still starts where it is set and still
    // climbs on every trigger.
    const lock = b.permanent
      ? `<label class="block-lock dis" title="${escHtml(tr("permanent stacks — they never decay and cannot build in-sim, so the count holds for the whole run"))}"><input type="checkbox" checked disabled> ${escHtml(tr("no timeout"))}</label>`
      : `<label class="block-lock" title="${escHtml(tr("the stacks never expire — they still start where you set them and still build on every trigger"))}"><input type="checkbox" data-b="${b.id}" data-f="locked" ${c.locked ? "checked" : ""}> ${escHtml(tr("no timeout"))}</label>`;
    // In the WIDER view, a buff the build does not carry is still settable —
    // it just says so, so the panel never reads as "this is active now".
    const off = have && !have.has(b.id);
    // …AND A BUFF THE FIGHT DENIES IS SHOWN, NOT HIDDEN: "no such buff" and
    // "nothing here triggers it" are the one pair a reader must tell apart.
    const denied = buffDenied(b);
    // What one stack count buys, when the source grants more than one thing
    // off the same trigger — they are the same count by construction.
    const grants = b.grants ? `<small class="bgr">${escHtml(tf(b.grants))}</small>` : "";
    const why = denied
      ? ` <small class="bnot bdenied" title="${escHtml(
          tr("what would trigger it triggers no buff in this fight, so it stays at zero all run"))}">${
          escHtml(tr("not triggered here"))}</small>`
      : "";
    return `<div class="buff-card${off ? " off" : ""}${denied ? " denied" : ""}">
      <span class="bn">${escHtml(buffCardName(b.name))}${grants}${off ? ` <small class="bnot">${escHtml(tr("not equipped"))}</small>` : ""}${why}</span>
      <span class="bctl">${ctl}</span>
      ${lock}
    </div>`;
  };
  box.innerHTML = list.map(card).join("");
  // READ-ONLY: the same cards, the same values, and no way to change them. A
  // preset is edited in exactly one place (the rule the fight above already
  // follows), so the optimizer SHOWS the buffs and links to the module that
  // owns them.
  if (opts.readonly) {
    box.querySelectorAll("[data-b]").forEach((el) => {
      el.disabled = true;
      el.title = tr("edit this in the Simulator");
    });
    return;
  }
  // A DENIED CARD'S KNOBS ARE DEAD, and say why — a control that cannot move
  // the number is worse than none, because it looks like one that can.
  const byId = new Map(list.map((b) => [b.id, b]));
  box.querySelectorAll("[data-b]").forEach((el) => {
    const b = byId.get(el.dataset.b);
    if (b && buffDenied(b)) {
      el.disabled = true;
      el.title = tr("what would trigger it triggers no buff in this fight, so it stays at zero all run");
    }
  });
  box.querySelectorAll("[data-b]").forEach((el) => {
    el.addEventListener("change", () => {
      const id = el.dataset.b, f = el.dataset.f, c = cfg[id];
      if (f === "locked") c.locked = el.checked;
      else if (el.type === "checkbox") c.stacks = el.checked ? 1 : 0;
      else c.stacks = Math.max(0, Number(el.value));
      // A buff belongs to the FIGHT and to nothing else — including settings
      // for mods this build does not carry. A build keeps no copy of the
      // scenario, so this dirties the scenario alone.
      markScenarioDirty();
      // The optimizer shows these read-only, so redraw its copy if it is up.
      if ($("opt-buffs") && !opts.readonly) renderOptBuffs();
    });
  });
}

// The buff panel has TWO views. By default it lists what the current build
// actually carries. "All potential" widens it to every buff this WEAPON could
// ever have — every mod in its pool, every arcane it can seat, every
// evolution option — because a scenario is meant to describe a fight, not a
// build, and a setting for a mod you have not equipped yet is exactly what the
// marginal-gain scan reads: settings reachable only once the mod is on are
// settings nobody can use to decide whether to equip it.
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

/// WHICH TRIGGERS FIRE NO BUFF IN THIS FIGHT — the subject is docs/BUFFS.md and
/// the vocabulary is `/api/meta.buff_triggers`, which is the DATA'S OWN: a buff
/// declares what fires it, so a switch per trigger conflates nothing. A coarse
/// set came first and collapsed the distinctions the data already draws — a
/// weak-point KILL is not a kill, and there was no way to say so.
///
/// IT SWITCHES OFF A TRIGGER, NOT AN EVENT. The run still kills, still hits weak
/// points, still procs, and the score still counts every one; what stops is the
/// BUFF those would have granted.
///
/// IT SITS IN LIMITS, beside Infinite ammo, because it is the same kind of
/// statement — not a technique anybody plays, but what this run is allowed to
/// assume. Every trigger is drawn on every weapon: a scenario describes a FIGHT
/// and not a build, and a term that vanished with the mods equipped would be a
/// ruler that changed shape between weapons.
///
/// NINETEEN IS A WALL, so they are GROUPED and folded. The group is presentation
/// and the switches are the truth — a header ticks its members and stores
/// nothing of its own, so a ruler can say `kill` alone or name `headshot_kill`.
const BUFF_TRIGGER_NAME = {
  kill: "any kill",
  headshot_kill: "weak-point kills",
  melee_kill: "melee kills",
  hit: "any hit",
  plain_hit: "hits that neither crit nor proc",
  headshot: "weak-point hits",
  weakpoint_hit: "weak-point hits, counted per pellet",
  consecutive_headshot: "weak-point hits in a row",
  punch_through: "punch through into a second body",
  status_applied: "a status landing",
  hit_enemy_with_status: "hitting a target that already carries a status",
  heat_status: "a Heat status landing",
  electricity_status: "an Electricity status landing",
  toxin_status: "a Toxin status landing",
  cold_status: "a Cold status landing",
  reload_complete: "a completed reload",
  reload_from_empty: "reloads from empty",
  firing: "a round leaving the barrel",
  full_burst: "a burst landing whole",
};
const BUFF_GROUP_NAME = {
  kill: "killing", hit: "hitting", status: "status effects",
  reload: "reloading", firing: "firing",
};

/// The groups, in the order `/api/meta.buff_triggers` lists them.
function buffTriggerGroups() {
  const out = [];
  for (const t of META.buff_triggers || []) {
    const last = out[out.length - 1];
    if (last && last.id === t.group) last.ids.push(t.id);
    else out.push({ id: t.group, ids: [t.id] });
  }
  return out;
}

function buffTriggersField() {
  const groups = buffTriggerGroups();
  if (!groups.length) return "";
  const off = new Set(sim.buff_triggers_off || []);
  const box = (id, label, extra) =>
    `<label class="bev${off.has(id) ? " on" : ""}"><input type="checkbox" ${extra}${
      off.has(id) ? " checked" : ""}> ${escHtml(label)}</label>`;
  return `<div class="bev-h">${escHtml(tr("no buff is triggered by"))}</div>${
    groups.map((g) => {
      // A HEADER IS TICKED WHEN ALL OF ITS OWN ARE, and half-ticked otherwise —
      // the state a reader needs is "is this whole group off", and a bare
      // checkbox has no third position to say "some of it".
      const on = g.ids.filter((id) => off.has(id)).length;
      const mark = on === g.ids.length ? " checked" : (on ? " data-some=\"1\"" : "");
      return `<div class="bevg">
        <label class="bevg-h${on ? " on" : ""}"><input type="checkbox" data-bevg="${
          escHtml(g.id)}"${mark}> ${escHtml(tr(BUFF_GROUP_NAME[g.id] || g.id))}${
          on && on < g.ids.length ? ` <small>${on}/${g.ids.length}</small>` : ""}</label>
        <div class="bev-row">${g.ids.map((id) =>
          box(id, tr(BUFF_TRIGGER_NAME[id] || id), `data-bev="${escHtml(id)}"`)).join("")}</div>
      </div>`;
    }).join("")}
    <div class="bev-why">${escHtml(tr(
      "the run still does all of these and the score still counts them — only the buffs they would trigger are off"))}</div>`;
}

/// THE VOCABULARY'S ORDER, not click order: this travels in a share link, and
/// two spellings of one fight compare as two fights. An id the page does not
/// know is KEPT, so an older page cannot strip a newer fight's terms.
function setBuffTriggersOff(set) {
  const known = (META.buff_triggers || []).map((t) => t.id);
  sim.buff_triggers_off = known.filter((x) => set.has(x))
    .concat([...set].filter((x) => !known.includes(x)));
  markScenarioDirty();
  renderSim();
}

function toggleBuffTrigger(id, on) {
  const set = new Set(sim.buff_triggers_off || []);
  if (on) set.add(id); else set.delete(id);
  setBuffTriggersOff(set);
}

/// A GROUP TICKS ITS MEMBERS AND STORES NOTHING. Off = every one of them off,
/// which is what makes the header's own state readable back out of the list.
function toggleBuffTriggerGroup(group, on) {
  const g = buffTriggerGroups().find((x) => x.id === group);
  if (!g) return;
  const set = new Set(sim.buff_triggers_off || []);
  g.ids.forEach((id) => { if (on) set.add(id); else set.delete(id); });
  setBuffTriggersOff(set);
}

/// Is this card's trigger switched off? The RUN answers it from the same table
/// server-side, which keeps the grey and the number together.
const buffDenied = (b) =>
  !!b.trigger && (sim.buff_triggers_off || []).includes(b.trigger);

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
  // A BAR, BECAUSE THE WAIT IS UNBOUNDED. A single-target fight is a
  // millisecond a run; a 361-body one is tens of them, so the rulers' 1000 runs
  // is a minute in the browser — and a line that says "running 1000
  // simulations…" for a minute reads as a hang.
  //
  // IT SAYS THE COUNT, not just a proportion: "412 / 1000" is a number a reader
  // can act on (lower the runs, take the fight apart) where a bar alone is only
  // a feeling.
  $("sim-results").innerHTML = `<div class="placeholder simrun">
    <div class="simrun-t">${escHtml(tr("running {n} simulations…").replace("{n}", simRuns()))}</div>
    <div class="simrun-bar"><span id="sim-prog" style="width:0%"></span></div>
    <div class="simrun-n" id="sim-prog-n"></div>
    <button type="button" id="sim-stop" class="ghost-btn small">${escHtml(tr("stop"))}</button>
  </div>`;
  // STOPPABLE, because the wait is unbounded and a reader who realises the
  // fight is too big should not have to reload the page.
  $("sim-stop").onclick = () => {
    cancelSim();
    $("sim-results").innerHTML =
      `<div class="placeholder">${escHtml(tr("stopped — nothing was measured"))}</div>`;
  };
  // THE READER IS WAITING FOR THIS ONE, so it outranks whatever the quick calc
  // is measuring — released in the `finally` below, whichever way this ends.
  const releasePriority = holdForeground();
  try {
    // `replay: true` only HERE. The gain scan hits the same endpoint once per
    // candidate and shows no replay, so it must not pay for one.
    const body = { ...buildPayload(), ...theFight({ replay: true }) };
    // THROTTLED BY THE FRAME, not by the message: the worker already sends one
    // per percent, and a DOM write per percent is still 100 of them for a fight
    // that takes a second.
    let painted = -1;
    const began = Date.now();
    const r = await simulateFleet(body, (done, total) => {
      const pct = total ? Math.round((done / total) * 100) : 100;
      if (pct === painted) return;
      painted = pct;
      const bar = $("sim-prog");
      const num = $("sim-prog-n");
      if (bar) bar.style.width = `${pct}%`;
      // HOW LONG IS LEFT, from how long it has taken — free, because the run is
      // already happening, and exact enough after a few runs because every run
      // is the same fight. A 361-body ruler is ~28 ms a run, so the default
      // 1000 is half a minute: the number a reader needs is not "how far" but
      // "how much longer".
      //
      // AFTER 5% only. Before that the estimate is one or two runs' noise
      // extrapolated a hundredfold, which reads as a wild guess and is one.
      const secs = (Date.now() - began) / 1000;
      const left = done > 0 && pct >= 5 ? (secs / done) * (total - done) : null;
      if (num) {
        num.textContent = `${done} / ${total}`
          + (left !== null && left > 1
            ? ` · ${tr("about {s}s left").replace("{s}", Math.ceil(left))}`
            : "");
      }
    });
    // A CANCEL IS NOT A FAILURE: the reader asked for it, and the panel already
    // says so. Nothing to report and nothing to complain about.
    if (r && r.cancelled) return;
    if (!r || r.ok === false) {
      $("sim-results").innerHTML = `<div class="error">sim failed: ${r ? r.error : "no data"}</div>`;
      return;
    }
    // A NEW RUN IS A NEW VERDICT. `renderResults` draws the outcome line
    // itself, so leaving the last run's answer standing would state it under a
    // build that has not been asked about yet — and now that one of the answers
    // is "already on the board", a stale one is a claim about the wrong build.
    boardState = "";
    boardOnBoard = null;
    renderResults(r);
    saveSimResult(r);
    // ONE INTEGER PAIR, FOR THE READER'S OWN EYES. `/support` is the only thing
    // that reads it and it never leaves this browser — see `SUPPORT_USE`. It is
    // counted HERE rather than in the worker because what is being counted is a
    // measurement the reader ASKED FOR: the quick calc and the optimizer run
    // thousands of engagements nobody sat and watched.
    noteSimRun(simRuns());
    // A run under the OFFICIAL scenario is the only thing that can reach the
    // board, and only after you have said so. Never blocks the result.
    offerBoardSubmit();
    offerSupportOnce($("sim-results"));
  } catch (e) {
    $("sim-results").innerHTML = `<div class="error">sim failed: ${e}</div>`;
  } finally {
    releasePriority();
    btn.disabled = false; btn.textContent = "Run Simulation";
  }
}

