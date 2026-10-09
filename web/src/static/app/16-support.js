// ---- support: what the project holds and what the reader has run --------

/// HOW MUCH OF THIS THE READER HAS ACTUALLY USED — on their own machine, and
/// nowhere else.
///
/// The strongest thing this page can say is not what the project is, it is what
/// it has already done FOR the person reading, and that is a number the app can
/// count rather than claim. It is two integers in one key: engagements are what
/// the machine actually paid for (a run at the rulers' 1000 is a thousand of
/// them), and simulations are what the reader remembers doing.
///
/// IT NEVER LEAVES THE BROWSER, and the page says so where it prints it —
/// which is the same promise the board makes about a submission, made in the
/// one place a reader might reasonably suspect otherwise.
///
/// TWO INTEGERS, which is the whole of its storage budget: a measurement costs
/// its summary and never its replay, and a counter that grew per run would be
/// the same mistake one size down.
const SUPPORT_USE = "wfsim-use";
function supportUse() {
  try {
    const v = JSON.parse(localStorage.getItem(SUPPORT_USE) || "{}");
    return { sims: v.sims | 0, engagements: v.engagements | 0 };
  } catch (_) { return { sims: 0, engagements: 0 }; }
}
function noteSimRun(engagements) {
  const v = supportUse();
  v.sims += 1;
  v.engagements += Math.max(1, engagements | 0);
  try { localStorage.setItem(SUPPORT_USE, JSON.stringify(v)); } catch (_) { /* private mode */ }
}

/// THE FACTS STRIP — what this repository holds, counted rather than claimed.
///
/// Two sources and the split is deliberate: anything the page can count for
/// itself is counted here and is right on the dev server too, and only what
/// lives outside the shipped data — tests, checks, measurements, commits —
/// comes from `PROJECT_FACTS`, which the site build writes. A figure that is
/// missing is DROPPED rather than drawn as a zero.
/// EVERYTHING THE ROSTER HOLDS, summed rather than listed — so a category
/// added to `META` counts itself and this function is not edited again. That
/// is the whole reason it is a sum: the roster grows sideways (frames today,
/// companions and Necramechs next) and a hand-written list of categories is a
/// figure that silently stops being true.
function rosterSize() {
  const mods = new Set();
  for (const pool of Object.values(META.mod_pools || {})) {
    for (const m of pool || []) mods.add(m.id);
  }
  // A weapon's evolutions arrive as TIERS, and what is modelled is the perks
  // inside them.
  let evolutions = 0;
  for (const w of META.weapons || []) {
    for (const tier of w.evolutions || []) evolutions += (tier.options || []).length;
  }
  const n = (k) => (META[k] || []).length;
  return n("weapons") + n("frames") + mods.size + evolutions
    + n("arcanes") + n("abilities") + n("auras") + n("shards") + n("enemies");
}

/// WHAT THE BOARD HAS BEEN ASKED, and what it answered.
///
/// SUBMISSIONS ARE ONE POOL AND SCORES ARE PER RULER, which is why these two
/// fields are summed differently: every ruler reports the SAME submission
/// count because they all read the same pool, so adding them would state the
/// uploads three times. `listed` is that ruler's own answers and does add up.
function boardCount(field) {
  const all = Object.values((BOARD_META && BOARD_META.boards) || {});
  if (!all.length) return 0;
  // BOTH BRANCHES READ `field`, which is also what keeps the one spelling of
  // `.submissions` in this file on the binding `check_release_identity` guards:
  // that field must be read from the FETCHED stamp, never the copy compiled
  // into the wasm, or the board dates itself by the build.
  return field === "submissions"
    ? Math.max(...all.map((b) => b[field] | 0))
    : all.reduce((t, b) => t + (b[field] | 0), 0);
}

function projectFacts() {
  // IDENTIFIED, because the home hero states these too. Picking by label would
  // break the moment a label is reworded, and a hero silently short of a
  // number is the failure.
  //
  // THREE ORDERS OF MAGNITUDE, ON PURPOSE: thousands, thousands, tens of
  // thousands read as three different facts, where two figures of the same
  // size read as one fact printed twice. They are also the three modules in
  // the order a reader meets them — what is modelled, what was built with it,
  // what came back out.
  const rows = [
    ["roster", rosterSize(), "items modelled"],
    ["builds", boardCount("submissions"), "builds players have uploaded"],
    ["evaluations", boardCount("listed"), "evaluations computed"],
  ];
  return rows.filter(([, n]) => n > 0).map(([id, n, what]) => ({ id, n, what }));
}

/// THE HERO'S NUMBERS — the same three the support page states, and all of
/// them. A figure whose source has not landed is DROPPED, so a dev server with
/// no board draws the roster alone rather than a zero or a placeholder.
const HERO_FACTS = ["roster", "builds", "evaluations"];

function renderHomeFacts() {
  const el = $("home-facts");
  if (!el) return;
  const rows = projectFacts().filter((f) => HERO_FACTS.includes(f.id));
  el.hidden = !rows.length;
  el.innerHTML = rows.map((f) => `<span class="hf"><b>${
    escHtml(f.n.toLocaleString())}</b> ${escHtml(tr(f.what))}</span>`).join("");
  if (!tallyTo) loadHomeTally();
}

/// THE BOARD'S SCORES BY WHO COMPUTED THEM, under the home hero's claim
/// (/api/board/tally): the players' machines apart from the official ones, and
/// the computers at it now. Asked each minute the page is in view; between
/// answers a count walks to the new one over the minute, so it never shows a
/// number the server has not.
const TALLY_EVERY_MS = 60_000;
let tallyShown = null, tallyFrom = null, tallyTo = null, tallyAt = 0;
async function loadHomeTally() {
  if (document.hidden || $("home-page")?.hidden) return;
  const r = await fetch("/api/board/tally", { cache: "no-cache" }).then((x) => (x.ok ? x.json() : null)).catch(() => null);
  if (!r || !r.totals) return;
  const next = { volunteers: r.totals.volunteers, official: r.totals.official, computing: r.computing | 0 };
  tallyFrom = tallyShown || next;
  tallyTo = next;
  tallyAt = Date.now();
  renderHomeTally();
}
function renderHomeTally() {
  const box = $("home-tally"), el = $("home-tally-n");
  if (!box || !el || !tallyTo) return;
  const f = Math.min(1, (Date.now() - tallyAt) / TALLY_EVERY_MS);
  const at = (k) => Math.round(tallyFrom[k] + (tallyTo[k] - tallyFrom[k]) * f);
  tallyShown = { volunteers: at("volunteers"), official: at("official"), computing: tallyTo.computing };
  const b = (n) => `<b>${escHtml(n.toLocaleString())}</b>`;
  const line = (s, n, dot) => `<span class="hf">${dot ? '<span class="online-dot"></span>' : ""}${
    escHtml(tr(s)).replace("{n}", b(n))}</span>`;
  el.innerHTML = line("{n} scores computed by players' machines, each agreed by two", tallyShown.volunteers)
    + line("{n} by the official servers", tallyShown.official)
    + (tallyShown.computing ? line("{n} computers computing now", tallyShown.computing, true) : "");
  box.hidden = false;
}
loadHomeTally();
setInterval(loadHomeTally, TALLY_EVERY_MS);
setInterval(() => { if (tallyTo && Date.now() - tallyAt < TALLY_EVERY_MS + 1000) renderHomeTally(); }, 250);
addEventListener("visibilitychange", loadHomeTally);

/// WHAT THIS CLIENT IS RUNNING, in the three identifiers of
/// docs/DISTRIBUTION.md §Identity: the release, the board, the shell.
///
/// EVERY LINE IS OMITTED WHEN IT WOULD BE A GUESS. The dev server has no
/// release and no board stamp, a browser has no shell version, and a line
/// saying `dev` beside two real digests is worse than three lines that are all
/// true — a reader quoting it would be quoting nothing.
///
/// THE THREE ARE THREE FACTS, not one restated. The release is what every
/// client of this version runs, the board moves on its own hourly schedule, and
/// the shell is the binary — the only one of them an update cannot move.
function identityLines() {
  const el = $("support-identity");
  if (!el) return;
  const lines = [];
  if (RELEASE_ID !== "dev") {
    lines.push(trF("release {r} · commit {c}", { r: RELEASE_ID, c: BUILD_SHA }));
  }
  if (BOARD_META && BOARD_META.digest) {
    lines.push(trF("board {d} · scored {t} · {n} rows", {
      d: BOARD_META.digest.slice(0, 12),
      t: BOARD_META.scored_at
        ? new Date(BOARD_META.scored_at * 1000).toLocaleString()
        : "—",
      n: (BOARD_META.rows || 0).toLocaleString(),
    }));
  }
  const draw = () => {
    el.hidden = !lines.length;
    el.innerHTML = lines.map(escHtml).join("<br>");
  };
  draw();
  // THE BINARY, NEVER THE RELEASE IT UNPACKED. `app_version` answers the
  // second, which the release line above already carries; this line is the one
  // number that dates the executable, on the page a bug report is read off.
  //
  // A SHELL THAT CANNOT SAY IS A LINE THAT IS NOT DRAWN: one older than this
  // global leaves it undefined, and that absence is itself the answer.
  if (!window.__WFSIM_DESKTOP__ || !window.__WFSIM_SHELL__) return;
  lines.push(trF("shell {v}", { v: window.__WFSIM_SHELL__ }));
  draw();
}

function renderSupport() {
  renderUsageNote();
  // WHAT AN EXTENSION ADDS to this page, drawn into its one slot.
  const slot = $("ext-support");
  if (slot) { slot.innerHTML = ""; extHook("support", slot); }
  const facts = $("support-facts");
  if (facts) {
    facts.innerHTML = projectFacts().map((f) => `
      <div class="sup-fact"><b>${escHtml(f.n.toLocaleString())}</b><span>${escHtml(tr(f.what))}</span></div>`).join("");
  }
  // WHEN IT STARTED AND HOW MUCH HAS HAPPENED SINCE. One line rather than two
  // more tiles: it is context for the strip above, not a sixth figure.
  const built = $("support-built");
  if (built) {
    const f = PROJECT_FACTS || {};
    const say = f.commits && f.first_commit_day;
    built.hidden = !say;
    if (say) {
      built.textContent = tr("Built in the open since {day} — {n} commits, every one of them public.")
        .replace("{day}", f.first_commit_day).replace("{n}", f.commits.toLocaleString());
    }
  }
  identityLines();
  const used = $("support-usage");
  if (used) {
    const u = supportUse();
    used.hidden = !u.sims;
    if (u.sims) {
      used.textContent = tr("You have run {n} simulations on this machine — {e} fights. That number is kept in this browser.")
        .replace("{n}", u.sims.toLocaleString()).replace("{e}", u.engagements.toLocaleString());
    }
  }
}

