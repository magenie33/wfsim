// SPDX-License-Identifier: AGPL-3.0-or-later
/// THE COMPUTE PAGE (`/compute`) — what this browser is doing for WFSim's free
/// services and what it did, and, signed in, every device of the account
/// (docs/BOARD.md §"Contribution"). This browser's task list lives in this
/// browser alone and is never sent. A task is drawn by its KIND
/// (`COMPUTE_KINDS`), which names it from public facts only — a board order is a
/// weapon, a ruler and a mode, never its mods or who submitted it — so a new
/// kind of work adds an entry there and nothing on the page.
const COMPUTE_LOG_KEY = "wfsim-compute-log";
const COMPUTE_LOG_MAX = 50;
/// The task this browser is on: `{ kind, ...facts, started, done, total }`.
let computeNow = null;
let computeEditing = null;
let computeRemoving = null;
let computeDrawnAt = 0;

const COMPUTE_KINDS = {
  // `verb` is which of the three verbs the task is: one build fought, or a search.
  board: {
    name: "Leaderboard order",
    verb: "Simulate",
    what: (t) => {
      const w = META && (META.weapons || []).find((x) => x.id === t.weapon);
      const b = META && (META.benchmarks || []).find((x) => x.id === t.ruler);
      return [w ? w.name : t.weapon, b ? tr(b.name) : t.ruler, t.mode && t.mode !== "base" && w ? modeLabel(w, t.mode) : ""]
        .filter(Boolean).join(" · ");
    },
    href: (t) => (META && t.weapon ? `${weaponPath(t.weapon)}?bench=${encodeURIComponent(t.ruler)}` : null),
  },
  // A RIVEN GAIN someone asked about in a chat: the weapon and the ruler, never
  // the card's rolls or who asked.
  riven_gain: {
    name: "Riven gain",
    verb: "Optimize",
    what: (t) => {
      const w = META && (META.weapons || []).find((x) => x.id === t.weapon);
      const b = META && (META.benchmarks || []).find((x) => x.id === t.ruler);
      return [w ? w.name : t.weapon, b ? tr(b.name) : t.ruler].filter(Boolean).join(" · ");
    },
    href: () => null,
  },
};
const computeKind = (t) => COMPUTE_KINDS[t && t.kind] || { name: "Task", verb: "", what: () => "", href: () => null };
/// THE NUMBER A TASK FOUND: a simulation's is the score of the build it was
/// given, a search's the best build it found among those it tried.
function computeResultHtml(t) {
  if (t.score == null) return "";
  const v = `${fmtScore(t.score)} ${metricLabel(metricOf(t.metric))}`;
  if (computeKind(t).verb !== "Optimize") return escHtml(tr("computed {s}").replace("{s}", v));
  const of = t.search && t.search.builds ? ` · ${tr("{b} builds, {s} fights").replace("{b}", computeCount(t.search.builds)).replace("{s}", computeCount(t.search.fights))}` : "";
  return escHtml(tr("best {s}").replace("{s}", v) + of);
}

/// THE QUESTION, asked once on a computer that can compute and has not
/// answered the current statement (69-board-work.js `computeConsent`): a card
/// in the corner, never a dialog, never on a card page a bot photographs, and
/// gone once answered either way. Its words are the statement consented to —
/// change them and `COMPUTE_CONSENT_V` with them.
const computeAskable = () => WASM && computeConsent() === null
  && !/\/card$|^\/appraise\//.test(location.pathname);
function computeAskHtml() {
  return `<b>${aT("Compute WFSim together?")}</b>
    <p>${aT("Turned on, this device joins the other volunteers in running WFSim's simulation and optimization tasks for every player. A result counts only when two volunteers compute it separately and get exactly the same answer, and your device is one of them.")}</p>
    <p>${aT("It takes 30% of this device's processing power by default, only while a WFSim page is open. You can set it from 10% to 100% in the compute menu at the top, beside Solo, the share your own calculations take (50% by default); past 100% together, the two share it in proportion while both run. It pauses on battery unless you change that in the menu; while it runs the device uses more power and runs warmer, and one click stops it.")}</p>
    <p>${aT("It is a voluntary contribution: everything it computes is free for every player, WFSim never makes money from it, and it is unpaid. Your contribution is counted to this browser, and, if you like, under your name on the contributors' ranking.")}</p>
    <div class="ca-acts"><button class="run-btn btn-sm" data-compute-ask="yes">${aT("Join")}</button>
      <button class="ghost-btn btn-sm" data-compute-ask="no">${aT("No thanks")}</button>
      <a href="/compute">${aT("Learn more")}</a></div>`;
}
/// …AND WHILE COMPUTING IS ON, THE RING IN THE TOP BAR (`#compute-mark`):
/// work nobody can see is work nobody agreed to keep doing. It is drawn at a
/// fixed size whenever computing is on, so a task starting or ending fills
/// it rather than moving the bar; it links to this page, and the settings
/// menu holds the switch (10-weapon-search.js `renderComputePicker`).
function computeChrome() {
  let ask = document.getElementById("compute-ask");
  if (computeAskable()) {
    if (!ask) {
      ask = document.createElement("div");
      ask.id = "compute-ask";
      ask.setAttribute("role", "region");
      // BOTTOM LEFT: Nona's button holds the bottom right.
      ask.style.cssText = "position:fixed;left:16px;bottom:16px;z-index:50;max-width:360px;padding:14px 16px;"
        + "border:1px solid var(--line);border-radius:10px;background:var(--surface);box-shadow:0 6px 24px rgba(0,0,0,.25);font-size:13px;line-height:1.6";
      ask.addEventListener("click", (e) => {
        const b = e.target.closest("[data-compute-ask]");
        if (b) setBoardVerify(b.dataset.computeAsk === "yes");
      });
      document.body.appendChild(ask);
    }
    ask.innerHTML = computeAskHtml();
  } else if (ask) ask.remove();
  const mark = document.getElementById("compute-mark");
  if (!mark) return;
  mark.hidden = !(WASM && boardVerifyOn());
  if (!mark.hidden) computeMarkPaint(mark);
}
function computeMarkPaint(mark) {
  const st = computeState(), f = computeFraction();
  mark.dataset.state = st.key;
  mark.classList.toggle("cm-spin", st.key === "computing" && f === null);
  const arc = mark.querySelector(".cm-arc");
  if (arc) arc.style.strokeDashoffset = st.key === "computing" ? String(100 * (1 - (f === null ? 0.25 : f))) : "";
  const label = `${st.text}${st.key === "computing" && f !== null ? ` ${Math.round(f * 100)}%` : ""}`;
  mark.title = label;
  mark.setAttribute("aria-label", label);
}
/// THE STATE, one answer for the ring and the page's own line: `{ key, text }`,
/// where `key` is what the ring draws — computing, waiting, paused, held or off.
function computeState() {
  const held = computeHeld();
  const s = (key, text) => ({ key, text: tr(text) });
  if (!WASM) return s("off", "This copy of WFSim does not compute; the site at wfsim.app does.");
  if (!boardVerifyOn()) return s("off", "Computing is off in this browser.");
  if (boardBanned) return { key: "off", text: tr("This browser is given no work until {t}: a result it sent differed from the server's own.")
    .replace("{t}", new Date(boardBannedUntil).toLocaleString(accountLocale())) };
  if (held === "battery") return s("held", "Paused while this device runs on battery.");
  if (held === "data") return s("held", "Paused while the browser saves data.");
  if (boardStale) return s("held", "A new version is out; this page refreshes itself once it is left idle.");
  if (computeNow) return s("computing", "Computing now.");
  return s("waiting", "Waiting for the next task.");
}
/// HOW FAR THE TASK IS, 0..1, or null when its size is not known (a riven gain).
const computeFraction = () => (computeNow && computeNow.total ? Math.min(1, computeNow.done / computeNow.total) : null);

function computeLog() {
  try {
    const l = JSON.parse(localStorage.getItem(COMPUTE_LOG_KEY) || "[]");
    // A riven gain's stored 0 is a misread search row (69-board-work.js
    // `rivenGainOnce`), not a measurement: it is drawn as no number at all.
    return Array.isArray(l) ? l.map((t) => (t && t.kind === "riven_gain" && t.score === 0 ? { ...t, score: null } : t)) : [];
  } catch (_) { return []; }
}
/// A TASK BEGUN, advanced and ended — called by whatever does the work.
function computeStart(task) {
  computeNow = { ...task, started: Date.now(), done: 0, total: 0 };
  computeRedraw();
  computeChrome();
}
/// `search`: a search's own status (08-checkpoint-api.js `quickFleet`), its starts drawn as lanes.
function computeProgress(done, total, search) {
  if (computeNow) { computeNow.done = done; computeNow.total = total; if (search) computeNow.search = search; }
  computeRedraw(true);
}
function computeEnd(result) {
  const t = computeNow;
  computeNow = null;
  if (t && result) {
    const { done: _d, total: _t, search: _s, ...facts } = t;
    const entry = { ...facts, at: Date.now(), ms: result.ms, ...(result.cpu_ms != null ? { cpu_ms: result.cpu_ms } : {}),
      work: result.work, score: result.score, metric: result.metric,
      ...(result.record ? { record: result.record } : {}), ...(result.search ? { search: result.search } : {}) };
    try { localStorage.setItem(COMPUTE_LOG_KEY, JSON.stringify([entry, ...computeLog()].slice(0, COMPUTE_LOG_MAX))); } catch (_) { /* this page only */ }
    // …HELD FULL ON THE CARD a moment, then into the list, today's count asked again.
    computeJustDone = { task: entry, until: Date.now() + COMPUTE_DONE_MS };
    setTimeout(computeLive, COMPUTE_DONE_MS + 20);
    devicePointsAt = 0;
    Promise.resolve(loadDevicePoints()).then(computeLive);
  }
  computeRedraw();
  computeChrome();
}
/// Drawn again while the page is open — a progress tick at most once a second.
function computeRedraw(tick) {
  const mark = document.getElementById("compute-mark");
  if (mark && !mark.hidden) computeMarkPaint(mark);
  if (typeof authKindOf !== "function" || authKindOf(location.pathname) !== "compute") return;
  if (tick) { if (Date.now() - computeDrawnAt >= 1000) { computeDrawnAt = Date.now(); computeLive(); } return; }
  renderAuthPage("compute");
}

/// WHAT A DEVICE IS CALLED until its owner names it — the owner's own name
/// always wins (`device-rename`). Read the way account pages everywhere read it:
/// ua-parser-js (MIT, pinned and served same-origin, web/lib/pins.json) for the
/// system, the browser — WeChat's, QQ's, Quark, UC among them — and a phone's
/// model, the browser's own client hints for the model a reduced user agent
/// hides ("K"), and an iPad told from the Mac it claims to be by its touch.
/// `deviceGuessReady()` loads it once; until then, and where it cannot load,
/// the coarse family from the user agent stands in.
const DEVICE_LIB = "/lib/ua-parser-1.0.41.min.js";
let deviceGuess = null, deviceGuessing = null;
const deviceLabelOf = (thing, browser) => [thing, browser].filter(Boolean).join(" · ").slice(0, 40) || tr("Device");
function deviceGuessReady() {
  if (deviceGuessing) return deviceGuessing;
  deviceGuessing = (async () => {
    try {
      if (typeof UAParser !== "function") {
        await new Promise((ok, no) => {
          const el = document.createElement("script");
          el.src = DEVICE_LIB; el.async = true; el.onload = ok; el.onerror = no;
          document.head.appendChild(el);
        });
      }
      const r = new UAParser(navigator.userAgent).getResult();
      let model = r.device.model && r.device.model !== "K" ? r.device.model : "";
      const hints = navigator.userAgentData;
      if (!model && hints && hints.getHighEntropyValues) {
        try { model = (await hints.getHighEntropyValues(["model"])).model || ""; } catch (_) { /* refused: no model */ }
      }
      const ipad = r.os.name === "Mac OS" && navigator.maxTouchPoints > 1;
      const os = ipad ? "iPadOS" : r.os.name === "Mac OS" ? "macOS" : r.os.name || "";
      const kind = ipad ? "iPad" : r.device.model === "iPhone" ? "iPhone" : "";
      const thing = kind || (model ? [r.device.vendor, model].filter(Boolean).join(" ") : os);
      deviceGuess = deviceLabelOf(thing, (r.browser.name || "").replace(/^Mobile /, ""));
    } catch (_) { deviceGuess = null; }
    return computeDeviceGuess();
  })();
  return deviceGuessing;
}
function computeDeviceGuess() {
  if (deviceGuess) return deviceGuess;
  const ua = navigator.userAgent || "";
  const os = /Android/.test(ua) ? "Android" : /iPhone/.test(ua) ? "iPhone" : /iPad/.test(ua) ? "iPad"
    : /Macintosh|Mac OS X/.test(ua) ? (navigator.maxTouchPoints > 1 ? "iPad" : "macOS") : /Windows/.test(ua) ? "Windows"
    : /CrOS/.test(ua) ? "ChromeOS" : /Linux/.test(ua) ? "Linux" : "";
  const browser = /MicroMessenger/.test(ua) ? "WeChat" : /Edg\//.test(ua) ? "Edge" : /Firefox\//.test(ua) ? "Firefox"
    : /Chrome\//.test(ua) ? "Chrome" : /Safari\//.test(ua) ? "Safari" : "";
  return deviceLabelOf(os, browser);
}
const computeOwnId = () => { try { return (localStorage.getItem(VERIFIER_KEY) || "").slice(0, 6); } catch (_) { return ""; } };

const computeAgo = (iso) => {
  const t = typeof iso === "number" ? iso : Date.parse(iso || "");
  if (!Number.isFinite(t)) return "";
  const m = Math.floor((Date.now() - t) / 60000);
  if (m < 1) return tr("just now");
  if (m < 60) return tr("{n} min ago").replace("{n}", String(m));
  if (m < 48 * 60) return tr("{n} h ago").replace("{n}", String(Math.floor(m / 60)));
  return new Date(t).toLocaleDateString(accountLocale());
};
const computeTook = (ms) => (ms < 60000 ? tr("{n} s").replace("{n}", String(Math.max(1, Math.round(ms / 1000))))
  : tr("{n} min").replace("{n}", String(Math.round(ms / 60000))));
const computePts = (n) => escHtml(Number(n || 0).toLocaleString(accountLocale()));
/// HOW LONG A TASK TOOK is the clock from taken to done. A board order's
/// `cpu_ms` is its pieces' time SUMMED OVER ITS CORES — six cores for five
/// seconds is thirty — said as core time beside it, never as how long it took.
/// An entry kept before `cpu_ms` held that sum in `ms`.
const taskWallMs = (t) => (t.started && t.at > t.started ? t.at - t.started : t.ms || 0);
const taskCoreMs = (t) => (t.kind === "board" ? Number(t.cpu_ms ?? t.ms) || 0 : 0);
function computeTookHtml(t) {
  const wall = taskWallMs(t), core = taskCoreMs(t);
  return `${computeTook(wall)}${core > 1.5 * wall ? ` · ${tr("{t} of core time").replace("{t}", computeTook(core))}` : ""}`;
}
function computeTaskHtml(t) {
  const k = computeKind(t);
  const what = escHtml(k.what(t));
  const href = k.href(t);
  return `${escHtml(tr(k.name))}${what ? ` · ${href ? `<a href="${escHtml(href)}">${what}</a>` : what}` : ""}`;
}

/// THIS BROWSER: whether it computes and why not, what it is on, what it earned.
function computeHereHtml() {
  const on = boardVerifyOn();
  const state = computeState().text;
  const again = on && boardStale ? ` <button class="ghost-btn btn-sm" data-auth="compute-reload">${aT("refresh now")}</button>` : "";
  // ITS POINTS ARE ITS NOTEBOOK: what no account holds yet, empty once one does (worker/contribution.js §"Spans").
  const d = devicePoints;
  const pts = !d ? "" : d.claimed
    ? `<div class="kv"><dt>${aT("Points")}</dt><dd>${aT("Everything this browser computes counts under the account signed in on it.")}</dd></div>`
    : `<div class="kv"><dt>${aT("Points")}</dt><dd>${computePts(d.points)} · ${
      escHtml(tr("{n} in the last 30 days").replace("{n}", Number(d.recent || 0).toLocaleString(accountLocale())))}<br><span class="set-note">${
      aT("Kept for this browser until you sign in here; the first account to sign in takes them all.")}</span>${
      accountState.account ? "" : ` <a href="/login?return=${encodeURIComponent("/compute")}">${aT("Sign in to count it under your name")}</a>`}</dd></div>`;
  return `<div class="block" id="compute-browser"><div class="bh"><h2>${aT("This device")}</h2></div><div class="bb"><dl class="kvs">
    <div class="kv"><dt>${aT("State")}</dt><dd><span id="compute-state">${escHtml(state)}</span>${on ? `<br><span class="set-note">${
      escHtml(tr("Computing together with the volunteers, with {pct}% of this device's processing power.").replace("{pct}", communityShare()))}</span>` : ""}</dd>${again}</div>
    <div class="kv"><dt>${aT("Processing power")}</dt><dd><div class="cs"></div></dd></div>${pts}</dl>${on && computeSlept ? `<p class="set-note" style="margin:8px 0 0">${
      aT("The browser put this page to sleep a moment ago, and computing together stopped while it slept. To keep it computing while you are away, add wfsim.app to the sites your browser keeps active: in Edge, Settings › System and performance › Never put these sites to sleep; in Chrome, Settings › Performance › Always keep these sites active.")}</p>` : ""}</div></div>`;
}

/// THE HONOUR, once earned: a device of the account said yes and has been
/// credited. Before that, the one line saying how it is earned.
function computeHonourHtml(d) {
  if (!d) return "";
  const v = d.volunteer;
  return `<div class="kv"><dt>${aT("Honour")}</dt><dd>${v
    ? `<b>${aT("WFSim Volunteer")}</b> · ${escHtml(tr("since {date}").replace("{date}", new Date(v).toLocaleDateString(accountLocale())))}`
    : aT("WFSim Volunteer, once a device you turned on has computed its first task")}</dd></div>`;
}

/// EVERY DEVICE OF THE ACCOUNT — the server's word on what each last did and
/// holds now, named as its owner calls it; this browser marked.
function computeDevicesHtml() {
  if (!accountState.providers.length) return "";
  if (!accountState.account) {
    return `<div class="block" id="compute-devices"><div class="bh"><h2>${aT("Your devices")}</h2></div><div class="bb"><p class="set-note" style="margin:0">${
      aT("Signed in, every device you leave computing shows here together, and its work counts under your name.")}
      <a href="/login?return=${encodeURIComponent("/compute")}">${aT("Sign in")}</a></p></div></div>`;
  }
  const s = devicesState;
  if (!s) return "";
  const own = computeOwnId();
  const rows = s.devices.map((d) => {
    const name = d.label || tr("Device {id}").replace("{id}", d.id);
    const here = d.id === own ? ` <span class="set-note">${aT("(this browser)")}</span>` : "";
    const status = d.now ? `${aT("Computing")}: ${computeTaskHtml(d.now)}`
      : d.last_at ? escHtml(tr("Last answered {when}").replace("{when}", computeAgo(d.last_at))) : aT("Nothing computed yet");
    const pts = `${computePts(d.points)} · ${escHtml(tr("{n} in the last 30 days").replace("{n}", Number(d.recent || 0).toLocaleString(accountLocale())))}`;
    if (computeEditing === d.id) {
      return `<div class="kv"><dt><input id="device-label" maxlength="40" value="${escHtml(d.label || computeDeviceGuess())}"></dt><dd></dd>
        <button class="ghost-btn btn-sm" data-auth="device-rename-save" data-id="${d.id}">${aT("Save")}</button>
        <button class="ghost-btn btn-sm" data-auth="device-rename-cancel">${aT("Cancel")}</button></div>`;
    }
    if (computeRemoving === d.id) {
      return `<div class="kv"><dt>${escHtml(name)}</dt><dd>${aT("Remove it from this list? Its points stay yours; signed in on it again, it is listed again.")}</dd>
        <button class="btn-danger btn-sm" data-auth="device-remove-confirm" data-id="${d.id}">${aT("Remove")}</button>
        <button class="ghost-btn btn-sm" data-auth="device-remove-cancel">${aT("Cancel")}</button></div>`;
    }
    return `<div class="kv"><dt>${escHtml(name)}${here}</dt><dd>${status}<br><span class="set-note">${pts}</span></dd>
      <button class="ghost-btn btn-sm" data-auth="device-rename" data-id="${d.id}">${aT("Rename")}</button>
      <button class="ghost-btn btn-sm" data-auth="device-remove" data-id="${d.id}">${aT("Remove")}</button></div>`;
  }).join("");
  const total = `${computePts(s.points)} · ${escHtml(tr("{n} in the last 30 days").replace("{n}", Number(s.recent || 0).toLocaleString(accountLocale())))}`;
  // A REMOVED DEVICE'S POINTS ARE STILL IN THE TOTAL, and the list says so (worker/contribution.js `release`).
  const removed = s.removed && s.removed.count ? `<p class="set-note" style="margin:4px 0 0">${escHtml(tr("{n} removed devices, their {p} points still counted")
    .replace("{n}", Number(s.removed.count).toLocaleString(accountLocale())).replace("{p}", Number(s.removed.points).toLocaleString(accountLocale())))}</p>` : "";
  return `<div class="block" id="compute-devices"><div class="bh"><h2>${aT("Your devices")}</h2></div><div class="bb">${rows
    ? `<dl class="kvs">${s.contributor_rank ? `<div class="kv"><dt>${aT("Contributor rank")}</dt><dd>${contributorRankBar(s.contributor_rank)}</dd></div>` : ""}${
      rows}${computeHonourHtml(s)}</dl><p class="set-note" style="margin:8px 0 0">${aT("All together")}: ${total}</p>${removed}`
    : `<p class="set-note" style="margin:0">${aT("No device yet: leave WFSim open on a device while you are signed in.")}</p>`}</div></div>`;
}

/// WHAT THIS BROWSER DID, newest first, with the points each will add once
/// another computer's answer agrees.
/// WHERE EACH RECENT RESULT STANDS, asked of the server by this browser's own id
/// for the ones still open — a board order of `/api/board/mine`, a riven gain of
/// `/api/appraise/mine` — and kept on the entry once it is final, confirmed or
/// gone, so the list says it after the task is long done.
const taskKey = (t) => `${t.identity}|${t.ruler}|${t.mode}`;
const taskOpen = (t) => !["confirmed", "gone"].includes(t.state);
async function loadTaskStates() {
  const id = typeof verifierId === "function" ? verifierId() : null;
  if (!id) return;
  const orders = computeLog().filter((t) => t.kind === "board" && t.identity && taskOpen(t));
  const gains = computeLog().filter((t) => t.kind === "riven_gain" && taskOpen(t));
  const [o, g] = await Promise.all([
    orders.length ? postBoardWork("/api/board/mine", { verifier: id, orders: orders.map(({ identity, ruler, mode }) => ({ identity, ruler, mode })) }) : null,
    gains.length ? postBoardWork("/api/appraise/mine", { verifier: id, tasks: gains.map(({ code, weapon, at }) => ({ code, weapon, at })) }) : null,
  ]);
  const byOrder = new Map(o && o.ok ? o.orders.map((x) => [taskKey(x), { state: x.state, at: x.at }]) : []);
  const byGain = new Map(g && g.ok ? g.tasks.map((x) => [x.at, { state: x.state, at: x.agreed_at, code: x.code }]) : []);
  if (!byOrder.size && !byGain.size) return;
  const log = computeLog().map((t) => {
    const s = t.kind === "riven_gain" ? byGain.get(t.at) : t.identity && byOrder.get(taskKey(t));
    return s ? { ...t, state: s.state, ...(s.code && !t.code ? { code: s.code } : {}), ...(s.at ? { confirmedAt: Date.parse(s.at) } : {}),
      changedAt: s.state !== t.state ? Date.now() : t.changedAt } : t;
  });
  try { localStorage.setItem(COMPUTE_LOG_KEY, JSON.stringify(log)); } catch (_) { /* this page only */ }
}

/// A RECORD AS THE BOARD'S OWN ROW, so the builder's card draws it (`boardRowState`).
const taskRow = (t) => {
  const r = t.record || {};
  return { mods: r.mods, arcanes: r.arcanes, evolutions: r.evolutions, exilus: r.exilus, valence: r.valence, mode: t.mode || r.mode,
    ...(r.riven_pos && r.riven_pos.length ? { riven: { bonuses: r.riven_pos, malus: r.riven_neg || null, rolls: r.riven_rolls } } : {}),
    ...(r.assembly ? { grip: r.assembly.grip, loader: r.assembly.loader } : {}) };
};
const taskPts = (t) => Math.round(Number(t.work || 0) / 1e9);
/// THE LIVE BLOCK (`#rt-live`) — today's tiles, the task being computed and the
/// list — is drawn ONCE and changed in place from then on (`computeLive`); a
/// redraw of the page keeps that node where it stands (`computeDraw`). Drawn
/// again, a card sliding in, a pill turning green or a number counting up was
/// cut off every second and everything under it jumped. docs/UI.md §"The compute
/// page changes in place".
let computeOpenTask = null;
const computeStill = () => matchMedia("(prefers-reduced-motion: reduce)").matches;
const computeCount = (x) => Number(x || 0).toLocaleString(accountLocale());
function computeRecentHtml() {
  // NOT BEFORE THE META: a reader landing on /compute is drawn once before it
  // arrives, and a task names its weapon, ruler and metric through it.
  if (typeof META === "undefined" || !META) return "";
  return `<div class="block" id="rt-live"><div class="bh"><h2>${aT("Recent tasks on this browser")}</h2></div><div class="bb">
    <div class="rt-today"><div class="rt-tile"><b data-tile="tasks"></b><span>${aT("tasks finished today")}</span></div>
      <div class="rt-tile gold" data-gold><b data-tile="points"></b><span>${aT("points credited today")}</span></div>
      <div class="rt-tile"><b data-tile="ms"></b><span>${aT("core time today")}</span></div></div>
    <div data-now></div><div class="rt-list" data-list></div>
    <p class="set-note" data-empty hidden style="margin:8px 0 0">${aT("Nothing yet.")}</p>
    <p class="set-note" style="margin:8px 0 0">${aT("A result counts once another device, of another owner on another network, computes exactly the same number. Open a task to see the build and the fight. This list stays in this browser.")}</p></div></div>`;
}
/// THE PAGE DRAWN AGAIN AROUND THE LIVE BLOCK: every other block is replaced,
/// the live one is never taken out of the page — moving it would restart what
/// it is in the middle of.
function computeDraw(main) {
  const live = document.getElementById("rt-live");
  const t = document.createElement("template");
  t.innerHTML = computePage();
  const col = live && live.parentNode;
  const fresh = t.content.getElementById("rt-live");
  // …UNLESS ITS OWN WORDS CHANGED — another language — which only a new one says.
  const same = live && fresh && live.querySelector("h2").textContent === fresh.querySelector("h2").textContent;
  if (same && col && main.contains(col)) {
    for (const el of [...col.children]) if (el !== live) el.remove();
    let after = false;
    for (const el of [...fresh.parentNode.children]) {
      if (el === fresh) { after = true; continue; }
      if (after) col.append(el); else col.insertBefore(el, live);
    }
  } else main.replaceChildren(t.content);
  // EVERY BLOCK FOLDS, as the rest of the site's do (70-result-panel.js `wireBlockFolds`).
  wireBlockFolds(main.querySelectorAll(".block[id]"));
  renderComputePicker();
  computeOpened();
}
/// EVERYTHING THE LIVE BLOCK SHOWS, brought up to date. Cheap: a tick calls it.
function computeLive() {
  const root = document.getElementById("rt-live");
  const state = document.getElementById("compute-state");
  if (state && state.textContent !== computeState().text) state.textContent = computeState().text;
  if (!root) return;
  computeLiveTiles(root);
  computeLiveNow(root);
  computeLiveList(root);
}

/// A NUMBER COUNTS to its new value instead of jumping; drawn the first time as it is.
const computeRolled = new WeakMap();
function computeRoll(el, to, fmt) {
  const from = computeRolled.get(el);
  if (from === to) return;
  computeRolled.set(el, to);
  if (from === undefined || computeStill()) { el.textContent = fmt(to); return; }
  const t0 = performance.now();
  const step = (now) => {
    if (computeRolled.get(el) !== to) return;
    const k = Math.min(1, (now - t0) / 700);
    el.textContent = fmt(from + (to - from) * (1 - (1 - k) ** 3));
    if (k < 1) requestAnimationFrame(step);
  };
  requestAnimationFrame(step);
}
/// TODAY IS THE SERVER'S COUNT (`/api/board/points` `today`): this list keeps
/// the last fifty tasks of one browser, and a machine finishes hundreds a day.
/// Points still flying to the tile are held back until they land.
let computeInFlight = 0;
function computeLiveTiles(root) {
  const day = (devicePoints && devicePoints.today) || { tasks: 0, ms: 0, points: 0 };
  const tile = (k) => root.querySelector(`[data-tile="${k}"]`);
  computeRoll(tile("tasks"), Number(day.tasks || 0), computeCount);
  const pts = tile("points");
  const held = Number(day.points || 0) - computeInFlight;
  computeRoll(pts, computeInFlight ? Math.max(held, computeRolled.get(pts) || 0) : held, (x) => `+${computeCount(Math.round(x))}`);
  // NONE IS NONE: `computeTook` never says less than a second.
  computeRoll(tile("ms"), Number(day.ms || 0), (x) => (x < 500 ? "0" : computeTook(x)));
}

/// THE TASK BEING COMPUTED: one card that stays while computing is on, task or
/// none — only what is in it changes. A task just finished holds it, full and
/// green, for `COMPUTE_DONE_MS` before it drops into the list, even when the
/// next one has already begun: cut short, the finish was never seen.
const COMPUTE_DONE_MS = 1400;
let computeJustDone = null;
const computeSubOf = (t, w) => (t.kind === "board" ? [computeRulerName(t.ruler), w && t.mode ? modeLabel(w, t.mode) : t.mode].filter(Boolean).join(" · ")
  : t.kind === "riven_gain" ? [tr("Riven gain"), computeRulerName(t.ruler)].filter(Boolean).join(" · ") : "");
const computeRulerName = (id) => { const b = (META.benchmarks || []).find((x) => x.id === id); return b ? tr(b.name).split(" · ")[0] : id || ""; };
const computeWeaponOf = (t) => (t && t.weapon ? (META.weapons || []).find((x) => x.id === t.weapon) : null);
function computeLiveNow(root) {
  const slot = root.querySelector("[data-now]");
  if (!(WASM && boardVerifyOn())) { slot.replaceChildren(); return; }
  let card = slot.firstElementChild;
  if (!card) {
    slot.innerHTML = `<div class="rt-now idle"><img alt="" hidden><div style="min-width:0"><div class="rt-eyebrow"><span class="rt-dot"></span><span data-k="eyebrow"></span></div>
      <div class="rt-now-name" data-k="name"></div><div class="rt-now-sub" data-k="sub"></div><div class="rt-bar"><i></i></div><div class="rt-lanes" data-lanes></div></div>
      <div class="rt-now-side"><div class="rt-pct" data-k="pct"></div><div class="rt-t" data-k="t"></div></div></div>`;
    card = slot.firstElementChild;
  }
  const done = computeJustDone && Date.now() < computeJustDone.until ? computeJustDone.task : null;
  const n = done ? null : computeNow;
  const t = done || n, w = computeWeaponOf(t);
  const f = n ? computeFraction() : done ? 1 : 0;
  card.className = `rt-now ${n ? "computing" : done ? "done" : "idle"}${n && f === null ? " unknown" : ""}`;
  const set = (k, v) => { const e = card.querySelector(`[data-k="${k}"]`); if (e.textContent !== v) e.textContent = v; };
  const img = card.querySelector("img");
  if (w && w.image) { const src = IMG(w.image); if (img.getAttribute("src") !== src) img.src = src; img.hidden = false; }
  else if (t) img.hidden = true;
  // A NEW TASK'S BAR STARTS EMPTY, never drawn back from the last one's end.
  const bar = card.querySelector(".rt-bar i");
  const id = t ? String(t.started) : "";
  if (card.dataset.task !== id) { card.dataset.task = id; bar.style.transition = "none"; bar.style.width = "0%"; void bar.offsetWidth; bar.style.transition = ""; }
  bar.style.width = `${Math.round(100 * (f || 0))}%`;
  computeLanes(card.querySelector("[data-lanes]"), n && n.search && n.search.starts || []);
  set("eyebrow", n ? tr("Computing") : done ? tr("Computed — another device checks it next") : computeState().text);
  set("name", t ? (w ? tr(w.name) : tr(computeKind(t).name)) : "");
  set("sub", t ? computeSubOf(t, w) : "");
  set("pct", n ? (f === null ? computeTook(Date.now() - n.started) : `${Math.round(100 * f)}%`) : done ? "100%" : "");
  set("t", n ? (f === null ? (n.done ? tr("{n} fights").replace("{n}", computeCount(n.done)) : "") : computeTook(Date.now() - n.started))
    : done ? computeTook(taskWallMs(done)) : "");
}

/// A SEARCH'S STARTS, one lane each, in the words the optimizer's own progress
/// uses (80-optimizer-preset.js `renderOptProgress`): where each start is in
/// its round, and which have settled.
function computeLanes(box, starts) {
  if (box.children.length !== starts.length) box.innerHTML = starts.map(() => `<div class="rt-lane"><span></span><i><b></b></i></div>`).join("");
  starts.forEach((s, i) => {
    const el = box.children[i];
    const text = `${tr("start")} ${i + 1} · ${s.settled ? tr("settled")
      : s.round === 0 ? tr("filling {k} of {n}").replace("{k}", s.at + 1).replace("{n}", s.of)
      : tr("round {r}, position {k} of {n}").replace("{r}", s.round).replace("{k}", s.at + 1).replace("{n}", s.of)}`;
    const label = el.querySelector("span");
    if (label.textContent !== text) label.textContent = text;
    el.classList.toggle("settled", !!s.settled);
    el.querySelector("b").style.width = `${s.settled ? 100 : Math.round((100 * (s.at + 1)) / Math.max(1, s.of))}%`;
  });
}

/// THE LIST, keyed by task: a new card slides in at the top and the ones under
/// it glide down to make room; a card whose state changed changes in place.
const computeKeyOf = (t) => (t.identity ? taskKey(t) : `${t.kind}|${t.at}`);
const computeCardState = (t) => ((t.kind === "board" && t.identity) || t.kind === "riven_gain" ? t.state || "waiting" : "local");
function computePillHtml(t, fresh) {
  const n = computeCount;
  const s = computeCardState(t);
  if (s === "local") return `<span class="rt-pill rt-gone">${escHtml(tr("≈ {n} points").replace("{n}", n(taskPts(t))))}</span>`;
  if (s === "confirmed") return `<span class="rt-pill rt-ok${fresh ? " rt-fresh" : ""}"><svg class="rt-tick" viewBox="0 0 12 12" aria-hidden="true"><path d="M2 6.5 5 9.2 10 3"/></svg>${
    escHtml(tr("confirmed +{n}").replace("{n}", n(taskPts(t))))}</span>`;
  if (s === "checking") return `<span class="rt-pill rt-check">${aT("results differ — the server is checking")}</span>`;
  if (s === "gone") return `<span class="rt-pill rt-gone">${aT("not counted — the row was no longer owed")}</span>`;
  return `<span class="rt-pill rt-wait"><span class="rt-dot"></span>${escHtml(tr("waiting for another device · about +{n}").replace("{n}", n(taskPts(t))))}</span>`;
}
function computeCardEl(t) {
  const w = computeWeaponOf(t);
  const tpl = document.createElement("template");
  tpl.innerHTML = `<div class="rt-card" data-auth="compute-task" data-key="${escHtml(computeKeyOf(t))}" data-state="${escHtml(computeCardState(t))}" role="button" tabindex="0">
    ${w && w.image ? `<img src="${IMG(w.image)}" alt="">` : "<span></span>"}
    <div style="min-width:0"><div class="rt-name">${computeKind(t).verb ? `<span class="rt-verb" data-verb="${escHtml(computeKind(t).verb)}">${aT(computeKind(t).verb)}</span>` : ""}${escHtml(w ? tr(w.name) : tr(computeKind(t).name))}</div>
      <div class="rt-sub">${escHtml(computeSubOf(t, w))}</div>
      ${t.score != null ? `<div class="rt-result">${computeResultHtml(t)}</div>` : ""}</div>
    <div class="rt-side"><span data-pill>${computePillHtml(t, false)}</span><div class="rt-when" data-when></div></div></div>`;
  return tpl.content.firstElementChild;
}
let computeDetail = null;
function computeLiveList(root) {
  const list = root.querySelector("[data-list]");
  const held = computeJustDone && Date.now() < computeJustDone.until ? computeJustDone.task.at : null;
  const log = computeLog().filter((t) => t.at !== held);
  root.querySelector("[data-empty]").hidden = log.length > 0 || held !== null;
  const have = new Map([...list.querySelectorAll(":scope > .rt-card")].map((el) => [el.dataset.key, el]));
  const first = !list.dataset.drawn;
  list.dataset.drawn = "1";
  const tops = new Map([...have.values()].map((el) => [el, el.getBoundingClientRect().top]));
  const still = computeStill();
  const want = [];
  let lands = 0;
  for (const t of log) {
    const key = computeKeyOf(t);
    let el = have.get(key);
    if (!el) {
      el = computeCardEl(t);
      if (!first && !still && Date.now() - t.at < 60_000) {
        el.classList.add("rt-arrive");
        el.addEventListener("animationend", () => el.classList.remove("rt-arrive"), { once: true });
      }
    } else if (el.dataset.state !== computeCardState(t)) {
      el.dataset.state = computeCardState(t);
      const fresh = el.dataset.state === "confirmed";
      el.querySelector("[data-pill]").innerHTML = computePillHtml(t, fresh && !still);
      if (fresh) computeCelebrate(el, t, lands++);
    }
    const when = `${computeAgo(t.at)} · ${computeTook(taskWallMs(t))}`;
    const w = el.querySelector("[data-when]");
    if (w.textContent !== when) w.textContent = when;
    const open = computeOpenTask === key;
    el.classList.toggle("rt-open", open);
    want.push(el);
    const weapon = computeWeaponOf(t);
    if (open && weapon && t.record) {
      const sig = `${key}|${t.state}`;
      if (!computeDetail || computeDetail.sig !== sig) {
        const tpl = document.createElement("template");
        tpl.innerHTML = computeTaskDetail(t, weapon).trim();
        computeDetail = { sig, el: tpl.content.firstElementChild };
      }
      want.push(computeDetail.el);
    }
  }
  if (!computeOpenTask) computeDetail = null;
  let cur = list.firstElementChild;
  for (const el of want) {
    if (el === cur) cur = cur.nextElementSibling;
    else list.insertBefore(el, cur);
  }
  while (cur) { const next = cur.nextElementSibling; cur.remove(); cur = next; }
  if (still) return;
  for (const [el, top] of tops) {
    if (!el.isConnected) continue;
    const dy = top - el.getBoundingClientRect().top;
    if (Math.abs(dy) > 1) el.animate([{ transform: `translateY(${dy}px)` }, { transform: "none" }], { duration: 450, easing: "cubic-bezier(.2,.8,.2,1)" });
  }
}
/// CONFIRMED: the card lights, and its points fly to today's gold tile, which
/// counts them in as they land — several at once go one after another.
function computeCelebrate(el, t, order) {
  el.classList.add("rt-lit");
  setTimeout(() => el.classList.remove("rt-lit"), 1600 + order * 160);
  devicePointsAt = 0;
  const asked = loadDevicePoints();
  const glow = () => {
    const g = document.querySelector("#rt-live [data-gold]");
    if (!g) return;
    g.classList.add("rt-glow");
    setTimeout(() => g.classList.remove("rt-glow"), 900);
  };
  const pill = el.querySelector("[data-pill]"), target = document.querySelector('#rt-live [data-tile="points"]');
  const pts = taskPts(t);
  if (computeStill() || !pill || !target || !pts) { Promise.resolve(asked).then(() => { computeLive(); glow(); }); return; }
  computeInFlight += pts;
  const a = pill.getBoundingClientRect(), b = target.getBoundingClientRect();
  const fly = document.createElement("div");
  fly.className = "rt-fly";
  fly.textContent = `+${computeCount(pts)}`;
  fly.style.left = `${a.left + a.width / 2 - 14}px`;
  fly.style.top = `${a.top - 6}px`;
  document.body.append(fly);
  const dx = b.left + 10 - (a.left + a.width / 2 - 14), dy = b.top - (a.top - 6);
  fly.animate([
    { transform: "translate(0,0) scale(1)", opacity: 0 },
    { transform: "translate(0,-14px) scale(1.15)", opacity: 1, offset: 0.18 },
    { transform: `translate(${dx}px,${dy}px) scale(.8)`, opacity: 0.9 },
  ], { duration: 950, delay: order * 160, easing: "cubic-bezier(.5,0,.3,1)", fill: "backwards" }).finished
    .catch(() => null)
    .then(() => Promise.resolve(asked))
    .then(() => { fly.remove(); computeInFlight = Math.max(0, computeInFlight - pts); computeLive(); glow(); });
}
/// ONE TASK OPENED: how far it has come, the fight, the build as the builder
/// draws it, and the way into both.
function computeTaskDetail(t, w) {
  const clock = (ms) => (ms ? new Date(ms).toLocaleTimeString(accountLocale()) : "");
  const st = t.state;
  const step = (on, label, note) => `<div class="tl-step${on ? " on" : ""}"><span class="tl-dot"></span><b>${aT(label)}</b><small>${escHtml(note || "")}</small></div>`;
  const third = st === "checking" ? step(true, "the server is checking", "") : st === "gone" ? step(false, "no longer owed", "")
    : step(st === "confirmed", "another volunteer confirmed", st === "confirmed" ? `${clock(t.confirmedAt)} · ${tr("another owner, another network")}` : tr("waiting"));
  const bench = (META.benchmarks || []).find((b) => b.id === t.ruler);
  const sim = `${weaponPath(w.id)}/simulator?task=${encodeURIComponent(computeKeyOf(t))}`;
  // A SEARCH SHOWS WHAT IT TRIED AND WHAT IT CONCLUDED: the same build card, as its answer.
  const search = t.kind === "riven_gain";
  const mode = t.mode || (t.record && t.record.mode);
  const here = search ? step(true, "searched here", `${clock(t.at)} · ${computeTookHtml(t)}${t.search ? ` · ${
    tr("{b} builds, {s} fights").replace("{b}", computeCount(t.search.builds)).replace("{s}", computeCount(t.search.fights))}` : ""}`)
    : step(true, "computed here", `${clock(t.at)} · ${computeTookHtml(t)}`);
  return `<div class="rt-detail"><div class="rt-h">${aT("Progress")}</div><div class="tl">
      ${step(true, "task taken", clock(t.started))}${here}${third}
      ${step(st === "confirmed", "points credited", st === "confirmed" ? `+${taskPts(t)}` : st === "gone" ? tr("none") : "")}</div>
    <div class="rt-h">${aT("The fight")}</div><dl class="rt-fight">
      <dt>${aT("Ruler")}</dt><dd>${escHtml(bench ? tr(bench.name) : t.ruler)}</dd>
      ${mode ? `<dt>${aT("Mode")}</dt><dd>${escHtml(modeLabel(w, mode))}</dd>` : ""}
      ${t.score != null ? `<dt>${aT(search ? "Best found" : "Result")}</dt><dd><b>${escHtml(fmtScore(t.score))}</b> ${escHtml(metricLabel(metricOf(t.metric)))}</dd>` : ""}</dl>
    <div class="rt-h">${aT(search ? "The answer: the best build the search found for this riven" : "The build")}</div>${cardOfState(boardRowState(w, taskRow(t)), w)}
    <div class="rt-acts"><a class="run-btn btn-sm" href="${escHtml(sim)}">${aT("Open this build in the simulator")}</a>
      <a class="ghost-btn btn-sm" href="${escHtml(weaponPath(w.id))}/benchmark">${aT("See this weapon's board")}</a></div></div>`;
}
/// A TASK OPENED FROM ITS LINK (`?task=`): its ruler's fight and its build, in
/// the simulator, from this browser's own list.
async function openComputeTask(w, key) {
  const t = computeLog().find((x) => computeKeyOf(x) === key);
  if (!t || t.weapon !== w.id || !t.record) return false;
  await agentDo("shell.preset.open", { bar: "scenario", preset: t.ruler }).catch(() => null);
  restoreState(boardRowState(w, taskRow(t)), w.id);
  return true;
}

/// THE WHOLE PICTURE, above this browser's own: what the board asks for and
/// what is answering it (`/api/board/demand`), asked when the page opens and on
/// every refresh, signed in or not. Nothing is drawn until it has answered.
let computeDemand = null;
async function loadComputeDemand() {
  const r = await accountCall("GET", "/api/board/demand");
  computeDemand = r && r.ok ? r : null;
}
function computeDemandHtml() {
  const d = computeDemand;
  if (!d) return "";
  const n = (x) => Number(x || 0).toLocaleString(accountLocale());
  const big = (x) => `<b style="font-size:16px">${escHtml(x)}</b>`;
  const owed = d.owed.new_builds + d.owed.rescores + d.owed.sweeps;
  const done = d.per_hour.volunteers + d.per_hour.official;
  const share = done ? Math.round((100 * d.per_hour.volunteers) / done) : 0;
  const parts = [["new builds {n}", d.owed.new_builds], ["rescores after a model change {n}", d.owed.rescores],
    ["a ruler's first pass over older builds {n}", d.owed.sweeps], ["riven gains {n}", d.riven_gains]]
    .filter(([, v]) => v).map(([s, v]) => escHtml(tr(s).replace("{n}", n(v)))).join(" · ");
  const hours = done ? owed / done : null;
  // EACH SURVEY IS A GOAL the reader can watch close: every riven shape of one
  // weapon, how many are confirmed and how many begun.
  const goals = (d.surveys || []).map((g) => {
    const w = (META && META.weapons || []).find((x) => x.id === g.weapon);
    const pct = (x) => (g.shapes ? Math.min(100, (100 * x) / g.shapes) : 0).toFixed(1);
    return `<div class="goal">${w && w.image ? `<img src="${IMG(w.image)}" alt="">` : "<span></span>"}<div style="min-width:0">
      <div class="goal-h"><b>${escHtml(tr("Every riven of {w}").replace("{w}", w ? tr(w.name) : g.weapon))}</b>
        <span>${escHtml(tr("{a} of {n} confirmed").replace("{a}", n(g.agreed)).replace("{n}", n(g.shapes)))}</span></div>
      <div class="goal-bar"><i class="begun" style="width:${pct(g.started)}%"></i><i style="width:${pct(g.agreed)}%"></i></div>
      <div class="set-note">${escHtml(tr("{n} begun · each shape is searched from four element starts, and counts once two owners on two networks agree").replace("{n}", n(g.started)))}</div></div></div>`;
  }).join("");
  const eta = hours === null || !owed ? "" : hours < 1 ? tr("under an hour") : tr("about {n} hours").replace("{n}", n(Math.round(hours)));
  return `<div class="block" id="compute-demand"><div class="bh"><h2>${aT("Demand and compute")}</h2><span class="set-note" style="margin-left:auto">${
    aT("All devices together · updated every minute")}</span></div><div class="bb"><dl class="kvs">
    <div class="kv"><dt>${aT("Computing now")}</dt><dd><span class="online-dot"></span>${big(computingCount === null ? "—" : n(computingCount))} ${aT("devices")}</dd></div>
    <div class="kv"><dt>${aT("Queued")}</dt><dd>${big(n(owed))} ${aT("rows of scores")}${parts ? `<div class="set-note" style="margin-top:4px">${parts}</div>` : ""}</dd></div>
    ${goals ? `<div class="kv"><dt>${aT("Goals")}</dt><dd style="min-width:0">${goals}</dd></div>` : ""}
    <div class="kv"><dt>${aT("Done per hour")}</dt><dd>${big(n(done))} ${aT("rows of scores")}
      <div class="demand-bar"><div style="width:${share}%"></div></div>
      <div class="set-note"><span class="demand-key on">■</span> ${escHtml(tr("volunteers {n} ({p}%)").replace("{n}", n(d.per_hour.volunteers)).replace("{p}", share))}
        &nbsp; <span class="demand-key">■</span> ${escHtml(tr("official machines {n} ({p}%)").replace("{n}", n(d.per_hour.official)).replace("{p}", 100 - share))}</div></dd></div>
    ${eta ? `<div class="kv"><dt>${aT("Cleared in")}</dt><dd>${big(eta)}<div class="set-note" style="margin-top:4px">${aT("at the last hour's pace")}</div></dd></div>` : ""}
  </dl></div></div>`;
}

/// THE NAV'S COUNT: how many computers are computing for WFSim now, asked when
/// the page starts and each minute it stays in view — the number as it is,
/// zero too, and nothing where the site has no such count (a dev server).
/// ONE SOURCE FOR THE COUNT: the bar and the compute page's "computing now"
/// both draw `computingCount`, and only this asks for it — so the two are the
/// same number at every moment, never two reads a minute apart. Asked past the
/// browser's cache (`no-cache` revalidates with the edge), so the edge's minute
/// is the one delay.
let computingCount = null;
async function navComputing() {
  const el = document.getElementById("compute-count");
  if (!el || document.hidden) return;
  const r = await fetch("/api/board/computing", { cache: "no-cache" }).then((x) => (x.ok ? x.json() : null)).catch(() => null);
  if (!r || typeof r.computing !== "number") return;
  computingCount = r.computing;
  const n = computingCount.toLocaleString(accountLocale());
  el.innerHTML = `<span class="online-dot"></span>${escHtml(n)}`;
  el.title = tr("{n} devices computing WFSim together now").replace("{n}", n);
  el.hidden = false;
  computeRedraw();
}
navComputing();
setInterval(navComputing, 60_000);
addEventListener("visibilitychange", navComputing);

function computePage() {
  return `<div class="settings solo"><div class="set-main"><h1 class="page">${aT("Compute")}</h1>
    <p class="set-note">${aT("The volunteers' devices compute WFSim's free features together. Everything they compute is free for every player, and WFSim never makes money from it.")}
      <a href="/contributors">${aT("Contributors")}</a></p>
    ${computeDemandHtml()}${computeHereHtml()}${computeDevicesHtml()}${computeRecentHtml()}</div></div>`;
}

/// DRAWN: this browser's points asked, the account's devices asked once per
/// account however the reader arrived — signing in on this page included — and
/// again every half minute while the page stays open, and a device claimed
/// before it had a name given its guess. Cheap when called again, which every
/// redraw does.
let computeTimer = null;
let computeAskedFor = null;
let computeClock = null;
function computeOpened() {
  Promise.resolve(loadDevicePoints()).then(computeLive);
  computeLive();
  // THE CLOCKS ON THE PAGE — a task's time, each card's age — every second.
  if (!computeClock) computeClock = setInterval(() => {
    if (authKindOf(location.pathname) !== "compute") { clearInterval(computeClock); computeClock = null; return; }
    computeLive();
  }, 1000);
  const who = accountState.account && accountState.account.id;
  if (who) deviceGuessReady();
  if (who && computeAskedFor !== who) { computeAskedFor = who; computeRefresh(); }
  // ONCE A VISIT, NEVER A RENDER: this runs on every redraw, and a redraw asked
  // for here redrew for ever — the page froze.
  if (!computeTimer) {
    computeTimer = setInterval(computeRefresh, 30000);
    Promise.all([loadComputeDemand(), navComputing(), loadTaskStates()]).then(computeRedraw);
  }
}
async function computeRefresh() {
  if (authKindOf(location.pathname) !== "compute") { clearInterval(computeTimer); computeTimer = null; computeAskedFor = null; return; }
  await Promise.all([loadComputeDemand(), navComputing(), loadTaskStates()]);
  if (!accountState.account) return computeRedraw();
  await loadDevices();
  const own = computeOwnId();
  const mine = devicesState && devicesState.devices.find((d) => d.id === own && !d.label);
  if (mine) {
    await accountCall("POST", "/api/account/devices/label", { id: own, label: await deviceGuessReady() });
    await loadDevices();
  }
  computeRedraw();
}

/// THE PAGE'S BUTTONS, through the account pages' one click handler.
async function computeAct(el, what) {
  const id = el.dataset.id;
  if (what === "compute-reload") return reloadForRelease();
  else if (what === "device-rename") computeEditing = id;
  else if (what === "device-rename-cancel" || what === "device-remove-cancel") { computeEditing = null; computeRemoving = null; }
  else if (what === "device-remove") computeRemoving = id;
  else if (what === "compute-task") computeOpenTask = computeOpenTask === el.dataset.key ? null : el.dataset.key;
  else if (what === "device-rename-save") {
    const v = ($("device-label") || {}).value || "";
    const r = await accountCall("POST", "/api/account/devices/label", { id, label: v });
    if (!(r && r.ok)) { presetToast(tr("That name cannot be used.")); return; }
    computeEditing = null;
    await loadDevices();
  } else if (what === "device-remove-confirm") {
    const r = await accountCall("POST", "/api/account/devices/remove", { id });
    if (r && r.ok) { computeRemoving = null; await loadDevices(); }
  }
  renderAuthPage("compute");
}
