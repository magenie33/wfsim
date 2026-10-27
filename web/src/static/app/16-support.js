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
    const count = (n) => Number.isSafeInteger(n) && n > 0 ? n : 0;
    return { sims: count(v?.sims), engagements: count(v?.engagements) };
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
/// answers a count walks to the new one over the minute.
const TALLY_EVERY_MS = 60_000;
let tallyShown = null, tallyFrom = null, tallyTo = null, tallyAt = 0, tallyLoading = null;
/// THE COUNT IS A FILE first (`stats/tally.json`, written by the settler each
/// minute and served by the CDN), and asked of the worker only when the file is
/// unreachable or older than this — docs/BOARD.md §"The public stats".
const TALLY_STALE_MS = 10 * 60_000;
const STATS_ORIGIN = "https://cdn.wfsim.app";
async function statsFile(name, staleMs) {
  const s = await fetch(`${STATS_ORIGIN}/stats/${name}.json`, { cache: "no-cache", signal: timeoutSignal(15_000) }).then((x) => (x.ok ? x.json() : null)).catch(() => null);
  return s && s.v === 1 && serverNow() - Date.parse(s.at) <= staleMs ? s : null;
}
async function loadHomeTally() {
  if (document.hidden || ($("home-page")?.hidden && $("support-page")?.hidden)) return;
  if (tallyLoading) return tallyLoading;
  tallyLoading = (async () => {
  const r = await statsFile("tally", TALLY_STALE_MS)
    || await fetch("/api/board/tally", { cache: "no-cache" }).then((x) => (x.ok ? x.json() : null)).catch(() => null);
  if (!r || !r.totals) return;
  const next = { volunteers: r.totals.volunteers, official: r.totals.official, computing: r.computing | 0 };
  // THE FIRST ANSWER WALKS FROM A MINUTE BEFORE IT, at the day's pace per hour,
  // so a count moves the moment the page opens and shows only what it held.
  const back = (k) => Math.max(0, next[k] - Math.round((r.per_hour?.[k] | 0) / 60));
  tallyFrom = tallyShown || { volunteers: back("volunteers"), official: back("official"), computing: next.computing };
  tallyTo = next;
  tallyAt = Date.now();
  renderHomeTally();
  if (!$("support-page")?.hidden) renderSupportCounts();
  })().finally(() => { tallyLoading = null; });
  return tallyLoading;
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
  el.innerHTML = line("{n} scores computed by players' devices, each agreed by two", tallyShown.volunteers)
    + line("{n} by the official servers", tallyShown.official)
    + (tallyShown.computing ? line("{n} devices computing together now", tallyShown.computing, true) : "");
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
  if (!META) return;
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
  renderSupportCounts();
  loadHomeTally();
  if (["#method", "#usage"].includes(location.hash)) $(location.hash.slice(1)).open = true;
}

/// Only measured targets change. Animation stops at the fetched value.
const supportCountStates = new Map();
const supportMotion = matchMedia("(prefers-reduced-motion: reduce)");
function drawSupportCounts(el, rows) {
  if (!el) return;
  const key = JSON.stringify(rows.map((r) => [r.id, r.n, tr(r.what)]));
  const prior = supportCountStates.get(el);
  if (prior?.key === key) return;
  if (prior) { cancelAnimationFrame(prior.frame); prior.observer?.disconnect(); }
  const state = { key, values: new Map(prior?.values), frame: 0, observer: null };
  supportCountStates.set(el, state);
  el.innerHTML = rows.map((r) => `<div class="support-count" data-support-count="${escHtml(r.id)}" data-value="${r.n}">
    <b><span class="count-accessible">${escHtml(r.n.toLocaleString())}</span><span class="count-value" aria-hidden="true">${escHtml(r.n.toLocaleString())}</span></b>
    <span>${aT(r.what)}</span></div>`).join("");
  const nodes = rows.map((r) => [r, el.querySelector(`[data-support-count="${r.id}"] .count-value`)]);
  const settle = () => {
    cancelAnimationFrame(state.frame);
    for (const [r, node] of nodes) { node.textContent = r.n.toLocaleString(); state.values.set(r.id, r.n); }
  };
  state.settle = settle;
  const animate = () => {
    state.observer?.disconnect();
    if (supportMotion.matches || document.hidden) { settle(); return; }
    const from = new Map(state.values), start = performance.now();
    const step = (now) => {
      if (!el.isConnected || $("support-page").hidden || supportMotion.matches || document.hidden) { settle(); return; }
      const p = Math.min(1, (now - start) / 1050), ease = 1 - (1 - p) ** 3;
      for (const [r, node] of nodes) {
        const value = Math.round((from.get(r.id) || 0) + (r.n - (from.get(r.id) || 0)) * ease);
        node.textContent = value.toLocaleString(); state.values.set(r.id, value);
      }
      if (p < 1) state.frame = requestAnimationFrame(step);
    };
    state.frame = requestAnimationFrame(step);
  };
  if (supportMotion.matches || !("IntersectionObserver" in window)) settle();
  else {
    state.observer = new IntersectionObserver((entries) => { if (entries.some((e) => e.isIntersecting)) animate(); }, { threshold: 0.2 });
    state.observer.observe(el);
  }
}
supportMotion.addEventListener("change", () => {
  if (supportMotion.matches) for (const s of supportCountStates.values()) { s.observer?.disconnect(); s.settle(); }
});

function renderSupportCounts() {
  const use = supportUse(), own = $("support-usage"), shared = $("support-community");
  if (own) own.hidden = !use.sims;
  drawSupportCounts($("support-use-facts"), use.sims ? [
    { id: "sims", n: use.sims, what: "simulations run" },
    { id: "engagements", n: use.engagements, what: "fights simulated" },
  ] : []);
  const rows = [
    { id: "builds", n: boardCount("submissions"), what: "builds players have uploaded" },
    { id: "volunteers", n: tallyTo?.volunteers || 0, what: "scores computed by players' devices" },
  ].filter((r) => Number.isSafeInteger(r.n) && r.n > 0);
  if (shared) shared.hidden = !rows.length;
  drawSupportCounts($("support-community-facts"), rows);
  const computing = $("support-computing");
  if (computing) {
    computing.hidden = !tallyTo?.computing;
    if (!computing.hidden) computing.textContent = tr("{n} devices computing together now").replace("{n}", tallyTo.computing.toLocaleString());
  }
}

