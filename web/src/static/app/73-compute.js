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
  board: {
    name: "Leaderboard order",
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
    what: (t) => {
      const w = META && (META.weapons || []).find((x) => x.id === t.weapon);
      const b = META && (META.benchmarks || []).find((x) => x.id === t.ruler);
      return [w ? w.name : t.weapon, b ? tr(b.name) : t.ruler].filter(Boolean).join(" · ");
    },
    href: () => null,
  },
};
const computeKind = (t) => COMPUTE_KINDS[t && t.kind] || { name: "Task", what: () => "", href: () => null };

/// THE QUESTION, asked once on a computer that can compute and has not
/// answered the current statement (69-board-work.js `computeConsent`): a card
/// in the corner, never a dialog, never on a card page a bot photographs, and
/// gone once answered either way. Its words are the statement consented to —
/// change them and `COMPUTE_CONSENT_V` with them.
const computeAskable = () => WASM && !onPhone() && computeConsent() === null
  && !/\/card$|^\/appraise\//.test(location.pathname);
function computeAskHtml() {
  return `<b>${aT("Help compute WFSim's free features?")}</b>
    <p>${aT("This computer would compute the leaderboard and riven gains for everyone, only while a WFSim page is open. While you use the computer it takes one core and steps aside the moment you use the calculator; left idle, it takes up to 30% of its cores, which you can change in the compute menu at the top. It pauses on battery, never runs on a phone, and stops with one click. Nothing it computes is sold, and you are not paid. The work is counted to this browser, and to your name only if you choose.")}</p>
    <div class="ca-acts"><button class="run-btn btn-sm" data-compute-ask="yes">${aT("Turn on")}</button>
      <button class="ghost-btn btn-sm" data-compute-ask="no">${aT("No thanks")}</button>
      <a href="/compute">${aT("Learn more")}</a></div>`;
}
/// …AND WHILE COMPUTING IS ON, THE RING IN THE TOP BAR (`#compute-mark`):
/// work nobody can see is work nobody agreed to keep doing. It is drawn at a
/// fixed size whenever computing is on, so a task starting or ending fills
/// it rather than moving the bar; it links to this page, and the settings
/// menu holds the pause (10-weapon-search.js `renderComputePicker`).
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
  mark.hidden = !(WASM && !onPhone() && boardVerifyOn());
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
/// THE READER'S PAUSE, for this tab, from the settings menu.
function computeTogglePause() {
  computePaused = !computePaused;
  computeChrome();
  computeRedraw();
  renderComputePicker();
}
/// THE STATE, one answer for the ring and the page's own line: `{ key, text }`,
/// where `key` is what the ring draws — computing, waiting, paused, held or off.
function computeState() {
  const held = computeHeld();
  const s = (key, text) => ({ key, text: tr(text) });
  if (!WASM) return s("off", "This copy of WFSim does not compute; the site at wfsim.app does.");
  if (onPhone()) return s("off", "Phones never compute.");
  if (!boardVerifyOn()) return s("off", "Computing is off in this browser.");
  if (boardBanned) return { key: "off", text: tr("This browser is given no work until {t}: a result it sent differed from the server's own.")
    .replace("{t}", new Date(boardBannedUntil).toLocaleString(accountLocale())) };
  if (held === "paused") return s("paused", "Paused in this tab.");
  if (held === "battery") return s("held", "Paused while this computer runs on battery.");
  if (held === "data") return s("held", "Paused while the browser saves data.");
  if (boardStale) return s("held", "A new version is out; this page refreshes itself once it is left idle.");
  if (computeNow) return s("computing", "Computing now.");
  if (readerBusy()) return s("held", "Paused while you use the calculator.");
  return s("waiting", "Waiting for the next task.");
}
/// HOW FAR THE TASK IS, 0..1, or null when its size is not known (a riven gain).
const computeFraction = () => (computeNow && computeNow.total ? Math.min(1, computeNow.done / computeNow.total) : null);

function computeLog() {
  try {
    const l = JSON.parse(localStorage.getItem(COMPUTE_LOG_KEY) || "[]");
    return Array.isArray(l) ? l : [];
  } catch (_) { return []; }
}
/// A TASK BEGUN, advanced and ended — called by whatever does the work.
function computeStart(task) {
  computeNow = { ...task, started: Date.now(), done: 0, total: 0 };
  computeRedraw();
  computeChrome();
}
function computeProgress(done, total) {
  if (computeNow) { computeNow.done = done; computeNow.total = total; }
  computeRedraw(true);
}
function computeEnd(result) {
  const t = computeNow;
  computeNow = null;
  if (t && result) {
    const { done: _d, total: _t, ...facts } = t;
    const entry = { ...facts, at: Date.now(), ms: result.ms, work: result.work, score: result.score, metric: result.metric };
    try { localStorage.setItem(COMPUTE_LOG_KEY, JSON.stringify([entry, ...computeLog()].slice(0, COMPUTE_LOG_MAX))); } catch (_) { /* this page only */ }
  }
  computeRedraw();
  computeChrome();
}
/// Drawn again while the page is open — a progress tick at most once a second.
function computeRedraw(tick) {
  const mark = document.getElementById("compute-mark");
  if (mark && !mark.hidden) computeMarkPaint(mark);
  if (typeof authKindOf !== "function" || authKindOf(location.pathname) !== "compute") return;
  if (tick && Date.now() - computeDrawnAt < 1000) return;
  computeDrawnAt = Date.now();
  renderAuthPage("compute");
}

/// WHAT A DEVICE IS CALLED until its owner names it: the coarse family of its
/// system and browser, read from the browser itself.
function computeDeviceGuess() {
  const ua = navigator.userAgent || "";
  const os = /Macintosh|Mac OS X/.test(ua) ? "Mac" : /Windows/.test(ua) ? "Windows" : /CrOS/.test(ua) ? "ChromeOS"
    : /Linux/.test(ua) ? "Linux" : tr("Computer");
  const browser = /Edg\//.test(ua) ? "Edge" : /Firefox\//.test(ua) ? "Firefox" : /Chrome\//.test(ua) ? "Chrome"
    : /Safari\//.test(ua) ? "Safari" : "";
  return browser ? `${os} · ${browser}` : os;
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
  const flip = WASM && !onPhone()
    ? ` <button class="ghost-btn btn-sm" data-auth="compute-flip">${aT(on ? "stop computing" : "start computing")}</button>` : "";
  const again = on && boardStale ? ` <button class="ghost-btn btn-sm" data-auth="compute-reload">${aT("refresh now")}</button>` : "";
  const n = computeNow;
  const pct = n && n.total ? Math.round((100 * n.done) / n.total) : 0;
  // THE ROW STAYS WHILE COMPUTING IS ON, task or none: it came and went between
  // tasks, and everything under it jumped every few seconds.
  // …AND ONE LINE HIGH, a long task cut short and whole on hover.
  const task = n ? `${computeTaskHtml(n)} · ${escHtml(computeTook(Date.now() - n.started))}` : "";
  const now = WASM && !onPhone() && on ? `<div class="kv"><dt>${aT("Now")}</dt><dd style="min-width:0"><div style="white-space:nowrap;overflow:hidden;text-overflow:ellipsis"${
    n ? ` title="${task.replace(/<[^>]*>/g, "")}"` : ""}>${n ? task : `<span class="set-note">${aT("Waiting for the next task.")}</span>`}</div>
      <div style="height:4px;border-radius:2px;background:var(--line);margin-top:6px"><div style="height:4px;border-radius:2px;background:var(--accent);width:${pct}%"></div></div></dd></div>` : "";
  const d = devicePoints;
  const pts = d ? `<div class="kv"><dt>${aT("Points")}</dt><dd>${computePts(d.points)} · ${
    escHtml(tr("{n} in the last 30 days").replace("{n}", Number(d.recent || 0).toLocaleString(accountLocale())))}${
    accountState.account ? "" : ` · <a href="/login?return=${encodeURIComponent("/compute")}">${aT("Sign in to count it under your name")}</a>`}</dd></div>` : "";
  return `<div class="block"><div class="bh"><h2>${aT("This browser")}</h2></div><div class="bb"><dl class="kvs">
    <div class="kv"><dt>${aT("State")}</dt><dd>${escHtml(state)}${on ? `<br><span class="set-note">${
      escHtml(tr("{pct}% of this computer's cores while it is idle, one core while you use it.").replace("{pct}", communityShare()))}</span>` : ""}</dd>${flip}${again}</div>${now}${pts}</dl></div></div>`;
}

/// THE HONOUR, once earned: a device of the account said yes and has been
/// credited. Before that, the one line saying how it is earned.
function computeHonourHtml(d) {
  if (!d) return "";
  const v = d.volunteer;
  return `<div class="kv"><dt>${aT("Honour")}</dt><dd>${v
    ? `<b>${aT("WFSim Volunteer")}</b> · ${escHtml(tr("since {date}").replace("{date}", new Date(v).toLocaleDateString(accountLocale())))}`
    : aT("WFSim Volunteer, once a computer you turned on has computed its first task")}</dd></div>`;
}

/// EVERY DEVICE OF THE ACCOUNT — the server's word on what each last did and
/// holds now, named as its owner calls it; this browser marked.
function computeDevicesHtml() {
  if (!accountState.providers.length) return "";
  if (!accountState.account) {
    return `<div class="block"><div class="bh"><h2>${aT("Your devices")}</h2></div><div class="bb"><p class="set-note" style="margin:0">${
      aT("Signed in, every computer you leave computing shows here together, and its work counts under your name.")}
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
      return `<div class="kv"><dt>${escHtml(name)}</dt><dd>${aT("Remove it? Its points leave your account, and it computes for nobody until it is claimed again.")}</dd>
        <button class="btn-danger btn-sm" data-auth="device-remove-confirm" data-id="${d.id}">${aT("Remove")}</button>
        <button class="ghost-btn btn-sm" data-auth="device-remove-cancel">${aT("Cancel")}</button></div>`;
    }
    return `<div class="kv"><dt>${escHtml(name)}${here}</dt><dd>${status}<br><span class="set-note">${pts}</span></dd>
      <button class="ghost-btn btn-sm" data-auth="device-rename" data-id="${d.id}">${aT("Rename")}</button>
      <button class="ghost-btn btn-sm" data-auth="device-remove" data-id="${d.id}">${aT("Remove")}</button></div>`;
  }).join("");
  const total = `${computePts(s.points)} · ${escHtml(tr("{n} in the last 30 days").replace("{n}", Number(s.recent || 0).toLocaleString(accountLocale())))}`;
  return `<div class="block"><div class="bh"><h2>${aT("Your devices")}</h2></div><div class="bb">${rows
    ? `<dl class="kvs">${rows}${computeHonourHtml(s)}</dl><p class="set-note" style="margin:8px 0 0">${aT("All together")}: ${total}</p>`
    : `<p class="set-note" style="margin:0">${aT("No device yet: leave WFSim open on a computer while you are signed in.")}</p>`}</div></div>`;
}

/// WHAT THIS BROWSER DID, newest first, with the points each will add once
/// another computer's answer agrees.
/// WHERE EACH RECENT RESULT STANDS, asked of the server by this browser's own id
/// (`/api/board/mine`) for the ones still open, and kept on the entry once it is
/// final — confirmed or gone — so the list says it after the order is long done.
const taskKey = (t) => `${t.identity}|${t.ruler}|${t.mode}`;
async function loadTaskStates() {
  const id = typeof verifierId === "function" ? verifierId() : null;
  const open = computeLog().filter((t) => t.kind === "board" && t.identity && !["confirmed", "gone"].includes(t.state));
  if (!id || !open.length) return;
  const r = await postBoardWork("/api/board/mine", { verifier: id, orders: open.map(({ identity, ruler, mode }) => ({ identity, ruler, mode })) });
  if (!(r && r.ok)) return;
  const by = new Map(r.orders.map((o) => [taskKey(o), o]));
  const log = computeLog().map((t) => {
    const o = t.identity && by.get(taskKey(t));
    return o ? { ...t, state: o.state, ...(o.at ? { confirmedAt: Date.parse(o.at) } : {}), changedAt: o.state !== t.state ? Date.now() : t.changedAt } : t;
  });
  try { localStorage.setItem(COMPUTE_LOG_KEY, JSON.stringify(log)); } catch (_) { /* this page only */ }
}

/// A RECORD AS THE BOARD'S OWN ROW, so the builder's card draws it (`boardRowState`).
const taskRow = (t) => {
  const r = t.record || {};
  return { mods: r.mods, arcanes: r.arcanes, evolutions: r.evolutions, exilus: r.exilus, valence: r.valence, mode: t.mode,
    ...(r.riven_pos && r.riven_pos.length ? { riven: { bonuses: r.riven_pos, malus: r.riven_neg || null, rolls: r.riven_rolls } } : {}),
    ...(r.assembly ? { grip: r.assembly.grip, loader: r.assembly.loader } : {}) };
};
const taskPts = (t) => Math.round(Number(t.work || 0) / 1e9);
/// …SAID ONCE: an entry new to the page slides in, a result newly confirmed
/// pops, and neither again on the next redraw a second later.
const computeShown = new Set();
let computeOpenTask = null;
function computeRecentHtml() {
  // NOT BEFORE THE META: a reader landing on /compute is drawn once before it
  // arrives, and a task names its weapon, ruler and metric through it.
  if (typeof META === "undefined" || !META) return "";
  const log = computeLog();
  if (!log.length) return `<div class="block"><div class="bh"><h2>${aT("Recent tasks on this browser")}</h2></div><div class="bb"><p class="set-note" style="margin:0">${aT("Nothing yet.")}</p></div></div>`;
  const n = (x) => Number(x || 0).toLocaleString(accountLocale());
  const midnight = new Date(); midnight.setHours(0, 0, 0, 0);
  const today = log.filter((t) => t.at >= midnight.getTime());
  const paid = log.filter((t) => t.state === "confirmed" && (t.confirmedAt || t.at) >= midnight.getTime()).reduce((s, t) => s + taskPts(t), 0);
  const tiles = `<div class="rt-today"><div class="rt-tile"><b>${escHtml(n(today.length))}</b><span>${aT("tasks finished today")}</span></div>
    <div class="rt-tile gold"><b>+${escHtml(n(paid))}</b><span>${aT("points credited today")}</span></div>
    <div class="rt-tile"><b>${escHtml(computeTook(today.reduce((s, t) => s + (t.ms || 0), 0)))}</b><span>${aT("computed today")}</span></div></div>`;
  const pill = (t) => {
    if (t.kind !== "board" || !t.identity) return `<span class="rt-pill rt-gone">${escHtml(tr("≈ {n} points").replace("{n}", n(taskPts(t))))}</span>`;
    const pop = t.changedAt && !computeShown.has(`pop:${taskKey(t)}:${t.state}`) ? " rt-pop" : "";
    computeShown.add(`pop:${taskKey(t)}:${t.state}`);
    if (t.state === "confirmed") return `<span class="rt-pill rt-ok${pop}">✓ ${escHtml(tr("confirmed +{n}").replace("{n}", n(taskPts(t))))}</span>`;
    if (t.state === "checking") return `<span class="rt-pill rt-check${pop}">${aT("results differ — the server is checking")}</span>`;
    if (t.state === "gone") return `<span class="rt-pill rt-gone${pop}">${aT("not counted — the row was no longer owed")}</span>`;
    return `<span class="rt-pill rt-wait">${escHtml(tr("waiting for another computer · about +{n}").replace("{n}", n(taskPts(t))))}</span>`;
  };
  const benches = META.benchmarks || [];
  const rulerName = (id) => { const b = benches.find((x) => x.id === id); return b ? tr(b.name).split(" · ")[0] : id; };
  const card = (t) => {
    const w = (META.weapons || []).find((x) => x.id === t.weapon);
    const key = t.identity ? taskKey(t) : `${t.kind}|${t.at}`;
    const fresh = !computeShown.has(`in:${key}`) && Date.now() - t.at < 15000 ? " rt-in" : "";
    computeShown.add(`in:${key}`);
    const open = computeOpenTask === key;
    const head = `<div class="rt-card${fresh}${open ? " rt-open" : ""}" data-auth="compute-task" data-key="${escHtml(key)}" role="button" tabindex="0">
      ${w && w.image ? `<img src="${IMG(w.image)}" alt="">` : "<span></span>"}
      <div><div class="rt-name">${escHtml(w ? tr(w.name) : t.weapon || "")}</div>
        <div class="rt-sub">${escHtml(t.kind === "board" ? `${rulerName(t.ruler)} · ${w ? modeLabel(w, t.mode) : t.mode}` : tr("Riven gain"))}</div>
        ${t.score != null ? `<div class="rt-result">${escHtml(tr("computed {s}").replace("{s}", fmtScore(t.score)))} ${escHtml(metricLabel(metricOf(t.metric)))}</div>` : ""}</div>
      <div class="rt-side">${pill(t)}<div class="rt-when">${escHtml(computeAgo(t.at))} · ${escHtml(computeTook(t.ms || 0))}</div></div></div>`;
    return head + (open && w && t.record ? computeTaskDetail(t, w) : "");
  };
  return `<div class="block"><div class="bh"><h2>${aT("Recent tasks on this browser")}</h2></div><div class="bb">${tiles}${log.map(card).join("")}
    <p class="set-note" style="margin:8px 0 0">${aT("A result counts once another computer, of another owner on another network, computes exactly the same number. Open a task to see the build and the fight. This list stays in this browser.")}</p></div></div>`;
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
  const sim = `${weaponPath(w.id)}/simulator?task=${encodeURIComponent(taskKey(t))}`;
  return `<div class="rt-detail"><div class="rt-h">${aT("Progress")}</div><div class="tl">
      ${step(true, "task taken", clock(t.started))}${step(true, "computed here", `${clock(t.at)} · ${computeTook(t.ms || 0)}`)}${third}
      ${step(st === "confirmed", "points credited", st === "confirmed" ? `+${taskPts(t)}` : st === "gone" ? tr("none") : "")}</div>
    <div class="rt-h">${aT("The fight")}</div><dl class="rt-fight">
      <dt>${aT("Ruler")}</dt><dd>${escHtml(bench ? tr(bench.name) : t.ruler)}</dd>
      <dt>${aT("Mode")}</dt><dd>${escHtml(modeLabel(w, t.mode))}</dd>
      ${t.score != null ? `<dt>${aT("Result")}</dt><dd><b>${escHtml(fmtScore(t.score))}</b> ${escHtml(metricLabel(metricOf(t.metric)))}</dd>` : ""}</dl>
    <div class="rt-h">${aT("The build")}</div>${cardOfState(boardRowState(w, taskRow(t)), w)}
    <div class="rt-acts"><a class="run-btn btn-sm" href="${escHtml(sim)}">${aT("Open this build in the simulator")}</a>
      <a class="ghost-btn btn-sm" href="${escHtml(weaponPath(w.id))}/benchmark">${aT("See this weapon's board")}</a></div></div>`;
}
/// A TASK OPENED FROM ITS LINK (`?task=`): its ruler's fight and its build, in
/// the simulator, from this browser's own list.
async function openComputeTask(w, key) {
  const t = computeLog().find((x) => x.identity && taskKey(x) === key);
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
  const eta = hours === null || !owed ? "" : hours < 1 ? tr("under an hour") : tr("about {n} hours").replace("{n}", n(Math.round(hours)));
  return `<div class="block"><div class="bh"><h2>${aT("Demand and compute")}</h2><span class="set-note" style="margin-left:auto">${
    aT("All computers together · updated every minute")}</span></div><div class="bb"><dl class="kvs">
    <div class="kv"><dt>${aT("Computing now")}</dt><dd><span class="online-dot"></span>${big(computingCount === null ? "—" : n(computingCount))} ${aT("computers")}</dd></div>
    <div class="kv"><dt>${aT("Queued")}</dt><dd>${big(n(owed))} ${aT("rows of scores")}${parts ? `<div class="set-note" style="margin-top:4px">${parts}</div>` : ""}</dd></div>
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
  el.title = tr("{n} computers computing for WFSim now").replace("{n}", n);
  el.hidden = false;
  computeRedraw();
}
navComputing();
setInterval(navComputing, 60_000);
addEventListener("visibilitychange", navComputing);

function computePage() {
  return `<div class="settings solo"><div class="set-main"><h1 class="page">${aT("Compute")}</h1>
    <p class="set-note">${aT("What this browser computes is free for everyone, never sold, and never runs a paid feature. It runs only while a WFSim page is open on a computer, steps aside the moment you run something yourself, never runs on a phone, and one click turns it off.")}
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
function computeOpened() {
  loadDevicePoints();
  const who = accountState.account && accountState.account.id;
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
    await accountCall("POST", "/api/account/devices/label", { id: own, label: computeDeviceGuess() });
    await loadDevices();
  }
  computeRedraw();
}

/// THE PAGE'S BUTTONS, through the account pages' one click handler.
async function computeAct(el, what) {
  const id = el.dataset.id;
  if (what === "compute-flip") setBoardVerify(!boardVerifyOn());
  else if (what === "compute-reload") return reloadForRelease();
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
