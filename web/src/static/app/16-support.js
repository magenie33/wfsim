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
  // The home page selects its upload count by id.
  const rows = [
    ["roster", rosterSize(), "items modelled"],
    ["builds", boardCount("submissions"), "builds players have uploaded"],
    ["evaluations", boardCount("listed"), "evaluations computed"],
  ];
  return rows.filter(([, n]) => n > 0).map(([id, n, what]) => ({ id, n, what }));
}

/// The home page's upload count. Missing values stay hidden.
const HERO_FACTS = ["builds"];

const HOME_VALUE_MS = 380;
const homeConfirmedValues = new Map();
const homeValueMotion = matchMedia("(prefers-reduced-motion: reduce)");
function paintConfirmedHomeValues(groups) {
  const start = document.timeline.currentTime;
  for (const [kind, value, nodes] of groups) {
    if (!Number.isSafeInteger(value) || value < 0) continue;
    const changed = homeConfirmedValues.get(kind) !== value;
    const text = value.toLocaleString(accountLocale());
    for (const node of nodes) {
      if (!node) continue;
      if (node.firstChild?.nodeType === 3) {
        if (node.firstChild.nodeValue !== text) node.firstChild.nodeValue = text;
      } else node.textContent = text;
      node.dataset.confirmedValue = String(value);
      if (changed && !homeValueMotion.matches && !document.hidden && node.getClientRects().length) {
        node.getAnimations().forEach(animation => animation.cancel());
        const animation = node.animate([
          { transform: "translateY(.12em)", opacity: .65 },
          { transform: "translateY(-.025em)", opacity: 1, offset: .7 },
          { transform: "translateY(0)", opacity: 1 }
        ], { duration: HOME_VALUE_MS, easing: "cubic-bezier(.2,.75,.3,1)" });
        animation.startTime = start;
      }
    }
    homeConfirmedValues.set(kind, value);
  }
}
homeValueMotion.addEventListener("change", () => {
  if (homeValueMotion.matches) for (const node of document.querySelectorAll('.home-stats b,[data-live-device-number]')) {
    node.getAnimations().forEach(animation => animation.cancel());
  }
});
function renderHomeFacts() {
  const el = $("home-facts");
  if (!el || document.hidden || $("home-page")?.hidden) return;
  const title = $("h-home"), lead = "The real ";
  if (LANG === "en" && title && !title.querySelector(".home-title-line")) {
    const text = tr(title.dataset.i18nSrc || title.textContent.trim());
    if (text.startsWith(lead)) title.innerHTML = `<span class="home-title-line">${escHtml(lead)}</span><span class="home-title-line">${
      escHtml(text.slice(lead.length)).replace("Prime", '<span class="prime">Prime</span>')}</span>`;
  }
  const rows = projectFacts().filter((f) => HERO_FACTS.includes(f.id));
  if (!BOARD_META && el.querySelector(".hf")) {
    for (const label of el.querySelectorAll(".metric-label")) label.textContent = tr(label.dataset.i18nSrc);
    paintConfirmedHomeValues([["builds", homeConfirmedValues.get("builds"), [el.querySelector("b")]]]);
    return;
  }
  el.hidden = !rows.length;
  const shape = LANG + ":" + rows.map(f => f.id).join(":");
  if (el.dataset.homeFactsShape !== shape) {
    el.innerHTML = rows.map(f => `<span class="hf" data-home-stat="${f.id}"><b></b><span class="metric-label" data-i18n-src="${escHtml(f.what)}">${escHtml(tr(f.what))}</span></span>`).join("");
    el.dataset.homeFactsShape = shape;
  }
  paintConfirmedHomeValues(rows.map(f => [f.id, f.n, [el.querySelector(`[data-home-stat="${f.id}"] b`)]]));
  if (!tallyTo) loadHomeTally();
}

/// The players' verified scores and devices computing now, under the home
/// hero's claim (/api/board/tally). Asked each minute the page is in view;
/// confirmed values appear immediately and only their presentation moves.
const TALLY_EVERY_MS = 60_000;
let tallyShown = null, tallyFrom = null, tallyTo = null, tallyAt = 0, tallyLoading = null;
/// THE COUNT IS A FILE first (`stats/tally.json`, written by the settler each
/// minute and served by the CDN), and asked of the worker only when the file is
/// unreachable or older than this — docs/BOARD.md §"The public stats".
const TALLY_STALE_MS = 10 * 60_000;
async function statsFile(name, staleMs) {
  const s = await fetch(`${BOARD_ORIGIN}/stats/${name}.json`, { cache: "no-cache", signal: timeoutSignal(15_000) }).then((x) => (x.ok ? x.json() : null)).catch(() => null);
  return s && s.v === 1 && serverNow() - Date.parse(s.at) <= staleMs ? s : null;
}
async function loadHomeTally() {
  if (document.hidden || ($("home-page")?.hidden && $("support-page")?.hidden && location.pathname !== "/compute")) return;
  if (tallyLoading) return tallyLoading;
  tallyLoading = (async () => {
  const r = await statsFile("tally", TALLY_STALE_MS)
    || await fetch("/api/board/tally", { cache: "no-cache" }).then((x) => (x.ok ? x.json() : null)).catch(() => null);
  if (!r || !r.totals) return;
  if (!Number.isSafeInteger(r.totals.volunteers) || r.totals.volunteers < 0) return;
  const next = { volunteers: r.totals.volunteers };
  tallyFrom = tallyShown || next;
  tallyTo = next;
  tallyAt = Date.now();
  renderHomeTally();
  if (!$("support-page")?.hidden) renderSupportCounts();
  if (location.pathname === "/compute") computeCommunityPaint();
  })().finally(() => { tallyLoading = null; });
  return tallyLoading;
}
function renderHomeTally() {
  if (document.hidden) return;
  const box = $("home-tally"), el = $("home-tally-n"), count = $("compute-count");
  const known = Number.isSafeInteger(computingCount) && computingCount >= 0;
  if (count) count.hidden = !known;
  let navValue = count?.querySelector("[data-live-device-number]");
  if (known && count) {
    if (!navValue) {
      const dot = count.querySelector(".online-dot") || document.createElement("span");
      dot.className = "online-dot";
      navValue = document.createElement("span"); navValue.dataset.liveDeviceNumber = "";
      count.replaceChildren(dot, navValue);
    }
    count.hidden = false;
  }
  const visible = box && el && tallyTo && !$("home-page")?.hidden;
  if (visible) {
    const shape = LANG + ":" + known;
    if (el.dataset.tallyShape !== shape) {
      const line = (kind, label) => `<span class="hf" data-home-stat="${kind}"><b data-tally="${kind}"></b><span class="metric-label">${label}</span></span>`;
      el.innerHTML = line("volunteers", escHtml(tr("scores computed by players' devices")))
        + (known ? line("devices", `<span class="online-dot"></span><a href="/compute">${escHtml(tr("devices computing together now"))}</a>`) : "");
      el.dataset.tallyShape = shape;
    }
    box.hidden = false;
    $("home-community").hidden = false;
    tallyShown = { volunteers: tallyTo.volunteers };
  }
  paintConfirmedHomeValues([
    ["volunteers", tallyTo?.volunteers, visible ? [el.querySelector('[data-tally="volunteers"]')] : []],
    ["devices", computingCount, [...(visible ? [el.querySelector('[data-tally="devices"]')] : []), navValue]]
  ]);
  computeLinkPaint();
}
loadHomeTally();
setInterval(loadHomeTally, TALLY_EVERY_MS);
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
    lines.push(trF("archived board {d} · scored {t} · {n} rows", {
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
  if (!META) return;
  const facts = $("support-facts");
  if (facts) {
    facts.innerHTML = projectFacts().map((f) => `
      <div class="sup-fact"><b>${escHtml(f.n.toLocaleString())}</b><span>${escHtml(tr(f.what))}</span></div>`).join("");
  }
  identityLines();
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
    computing.hidden = computingCount === null;
    if (!computing.hidden) computing.textContent = tr("{n} devices computing together now").replace("{n}", computingCount.toLocaleString(accountLocale()));
  }
}

