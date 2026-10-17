// ---- UTILITY ----------------------------------------------------------------
//
// The game's live state and the reader's reminders on it — docs/UI.md
// §"Utility". Nothing here feeds the builder, the simulator or the optimizer.
// A kind the worker lists (worker/world.js) has a tab of its own and registers
// itself in `UTILITY_KINDS`; this file is what every kind shares: the feed, the
// clock, the reminders and their tab.

/// THE TABS, in order: each kind's list, then the reminders. `[path, title]`.
const UTILITY_TABS = [["fissures", "Void Fissures"], ["arbitrations", "Arbitrations"], ["reminders", "Reminders"]];
const utilityTitle = (tab) => (UTILITY_TABS.find(([t]) => t === tab) || UTILITY_TABS[0])[1];
/// EACH KIND, by the worker's `kind`: `{ tab, render(), describe(item),
/// nameOf(key, names, value), attributes: [[key]], build(box), next(r),
/// possible(r), stats(r, open) }` — `attributes` are what a reminder may hold,
/// in the order its editor offers them; `build` draws a reminder made from
/// nothing into the Reminders tab; `next`, for a kind known ahead, says when a
/// reminder fires next; `possible` says whether anything can ever match it
/// (null: not known yet); `stats` says how often one like it comes.
const UTILITY_KINDS = {};
let utilityTab = null;

/// A NAME FROM THE WORKER is `{en, zh}`; a missing one is the id it names.
const worldName = (n, id) => (n && (n[LANG] || n.en)) || id || "";

/// TIME LEFT, as h:mm:ss — the same in every language.
function utilityLeft(ms) {
  const s = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(s / 3600), m = Math.floor(s / 60) % 60;
  return `${h}:${String(m).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
}

function showUtility(tab) {
  if (tab !== utilityTab) reminderDraft = null;
  utilityTab = tab;
  $("h-utility").textContent = tr(utilityTitle(tab));
  for (const [t] of UTILITY_TABS) $(`utility-${t}`).hidden = t !== tab;
  if (tab === "reminders") { reminderRead(); if ($("rem-toasts")) $("rem-toasts").replaceChildren(); }
  renderUtility();
  if (Date.now() - worldAt >= WORLD_ON_PAGE_MS) worldLoad().then(renderUtility);
  utilityClock();
}

function renderUtility() {
  renderBell();
  const unread = reminderHits().filter((h) => !h.read).length;
  if (!utilityTab || $("utility-page").hidden) return;
  $("utility-tabs").innerHTML = UTILITY_TABS.map(([t, title]) => `<a class="mtab${t === utilityTab ? " sel" : ""}" href="/utility/${t}">${
    escHtml(tr(title))}${t === "reminders" && unread ? ` <span class="tbadge">${unread}</span>` : ""}</a>`).join("");
  if (utilityTab === "reminders") renderReminders();
  else Object.values(UTILITY_KINDS).forEach((k) => { if (k.tab === utilityTab) k.render(); });
}

/// THE STATE WHERE THE PAGE CANNOT SHOW IT: still loading, or unreachable.
/// Null when there is a world to draw.
function worldWaitHtml() {
  if (world) return null;
  return worldFailed
    ? `<div class="sim-empty">${escHtml(tr("The world state could not be reached."))} <button type="button" class="ghost-btn small" data-world-retry>${escHtml(tr("Retry"))}</button></div>`
    : `<div class="sim-empty">${escHtml(tr("Loading…"))}</div>`;
}
document.addEventListener("click", (e) => {
  if (e.target.closest("[data-world-retry]")) { worldFailed = false; renderUtility(); worldLoad().then(renderUtility); }
});

// ---- THE FEED ---------------------------------------------------------------

/// THE WORLD AS THE WORKER LAST ANSWERED — `{read_at_ms, items}` — or null.
let world = null;
let worldAsk = null;
let worldFailed = false;
let worldAt = 0;
/// Asked each minute while a Utility page is open — DE refreshes its file each
/// minute — and every two minutes elsewhere while a reminder waits, since what
/// it waits for stays open for most of an hour.
const WORLD_ON_PAGE_MS = 60_000;
const WORLD_ELSEWHERE_MS = 120_000;

/// THE LIVE SITE, from a dev server and the desktop client too: the worker is
/// the only reader of DE's file and answers any origin.
function worldLoad() {
  if (worldAsk) return worldAsk;
  worldAsk = (async () => {
    try {
      const r = await fetch(`${LIVE_ORIGIN}/api/world`, { cache: "no-cache" });
      const j = await r.json();
      if (!j.ok) throw new Error(j.error);
      world = j;
      worldFailed = false;
    } catch (_) {
      worldFailed = true;
    } finally {
      worldAt = Date.now();
      worldAsk = null;
    }
    reminderCheck();
  })();
  return worldAsk;
}
/// What is open now, of one kind; null before the first answer.
const worldOf = (kind) => world && world.items.filter((x) => x.kind === kind && x.ends_at_ms > Date.now());

/// EVERYTHING A REMINDER CAN NAME, open or not — `{tiers, missions, planets,
/// nodes}`, and every fissure combination seen with how often it came — asked
/// once, when a reminder is first built from nothing or listed.
let worldNames = null;
let worldNamesAsk = null;
let worldNamesFailed = false;
let worldNamesAt = 0;
/// The worker keeps its answer this long; a list on screen longer asks again,
/// so how long ago the last fissure opened follows the log.
const WORLD_NAMES_MS = 600_000;
const worldNamesStale = () => Date.now() - worldNamesAt >= WORLD_NAMES_MS;
function worldNamesLoad() {
  if (worldNamesAsk) return worldNamesAsk;
  if (worldNames && !worldNamesStale()) return Promise.resolve();
  worldNamesFailed = false;
  worldNamesAsk = fetch(`${LIVE_ORIGIN}/api/world/names`).then((r) => r.json())
    .then((j) => { if (!j.ok) throw new Error(j.error); worldNames = j; })
    .catch(() => { worldNamesFailed = !worldNames; })
    .finally(() => { worldNamesAsk = null; worldNamesAt = Date.now(); });
  return worldNamesAsk;
}

/// ONE CLOCK: each second the countdowns on screen move and an ended row
/// leaves; the feed is asked again when it is due. It stops when there is
/// neither a Utility page on screen nor a reminder to watch for.
let utilityTimer = null;
function utilityClock() {
  if (utilityTimer) return;
  utilityTimer = setInterval(() => {
    const onPage = !$("utility-page").hidden;
    if (!onPage && !reminders().length) { clearInterval(utilityTimer); utilityTimer = null; return; }
    if (!worldAsk && Date.now() - worldAt >= (onPage ? WORLD_ON_PAGE_MS : WORLD_ELSEWHERE_MS)) worldLoad().then(renderUtility);
    const now = Date.now();
    let ended = false;
    document.querySelectorAll(`${onPage ? "#utility-page [data-ends], " : ""}#rem-bell.open [data-ends], #rem-toasts [data-ends]`).forEach((el) => {
      const left = Number(el.dataset.ends) - now;
      if (left <= 0) ended = true;
      el.textContent = utilityLeft(left);
    });
    if (ended) renderUtility();
  }, 1000);
}

// ---- REMINDERS --------------------------------------------------------------
//
// KEPT IN THIS BROWSER, synced to a signed-in account like a saved build
// (37-sync.js), and fired while the site is open, on any page. A reminder is
// `{id, kind, attributes, names, made_at_ms, quiet?}` and matches an item of its
// kind whose attributes hold every one it states (`reminderMatches`).

const REMINDERS_KEY = "wfsim-reminders";
/// Items a reminder already fired for (id → its end), so one fires once.
const REMINDERS_SEEN_KEY = "wfsim-reminders-seen";
/// What fired, newest first: `{id, reminder, kind, attributes, names, ends_at_ms, at_ms, read}`.
const REMINDERS_HITS_KEY = "wfsim-reminders-hits";
const REMINDERS_HITS_MAX = 30;
/// 1 when the reader asked for a system notification, before the choice moved
/// to `NOTIFY_KEY` (49-notices.js), which still honours it.
const REMINDERS_SYSTEM_KEY = "wfsim-reminders-system";

function storedJson(key, empty) {
  try { const v = JSON.parse(localStorage.getItem(key)); return v == null ? empty : v; } catch (_) { return empty; }
}
function storeJson(key, v) {
  try { localStorage.setItem(key, JSON.stringify(v)); } catch (_) { /* this page only */ }
}
const reminders = () => { const v = storedJson(REMINDERS_KEY, []); return Array.isArray(v) ? v : []; };
const reminderHits = () => { const v = storedJson(REMINDERS_HITS_KEY, []); return Array.isArray(v) ? v : []; };
const reminderMatches = (r, item) => item.kind === r.kind
  && Object.entries(r.attributes || {}).every(([k, v]) => item.attributes[k] === v);

/// A NEW REMINDER, and what it already matches counts as seen: the reader is
/// looking at it, so it is not news.
function reminderAdd(kind, attributes, names) {
  const r = { id: Math.random().toString(36).slice(2, 12), kind, attributes, names, made_at_ms: Date.now() };
  storeJson(REMINDERS_KEY, [...reminders(), r]);
  const seen = storedJson(REMINDERS_SEEN_KEY, {});
  for (const item of (world ? world.items : [])) if (reminderMatches(r, item)) seen[item.id] = item.ends_at_ms;
  storeJson(REMINDERS_SEEN_KEY, seen);
  utilityClock();
  syncSoon();
  return r;
}
function reminderRemove(id) {
  storeJson(REMINDERS_KEY, reminders().filter((r) => r.id !== id));
  syncSoon();
}
/// A QUIET REMINDER fires into the bell only — no card, no system notification.
function reminderQuiet(id) {
  storeJson(REMINDERS_KEY, reminders().map((r) => r.id !== id ? r
    : r.quiet ? Object.fromEntries(Object.entries(r).filter(([k]) => k !== "quiet")) : { ...r, quiet: true }));
  syncSoon();
}

/// ON EVERY ROUTE: a reader with a reminder has the clock running, and a first
/// answer asked for, whichever page they are on.
function reminderWatch() {
  renderUtility();
  if (!reminders().length) return;
  utilityClock();
  if (!world && !worldAsk && !worldAt) worldLoad().then(renderUtility);
}

/// WHAT FIRED SINCE THE LAST ANSWER: each open item a reminder matches and has
/// not fired for. It is kept under the bell and counted on it; a reminder not
/// set quiet also says it in a card in the corner, and — if the reader asked
/// and the tab is in the background — in a system notification.
function reminderCheck() {
  if (!world) return;
  const rs = reminders();
  const now = Date.now();
  const seen = Object.fromEntries(Object.entries(storedJson(REMINDERS_SEEN_KEY, {})).filter(([, end]) => end > now));
  const fresh = [];
  for (const item of world.items) {
    if (item.ends_at_ms <= now || seen[item.id]) continue;
    // WHAT WAS OPEN WHEN A REMINDER WAS MADE IS NOT NEWS to it, on whichever
    // browser it was made: one synced here would otherwise fire at once.
    const r = rs.find((x) => reminderMatches(x, item) && !(item.started_at_ms < (x.made_at_ms || 0)));
    if (!r) continue;
    seen[item.id] = item.ends_at_ms;
    fresh.push({ id: item.id, reminder: r.id, kind: item.kind, attributes: item.attributes, names: item.names, ends_at_ms: item.ends_at_ms, at_ms: now, read: false });
  }
  storeJson(REMINDERS_SEEN_KEY, seen);
  if (!fresh.length) return;
  storeJson(REMINDERS_HITS_KEY, [...fresh, ...reminderHits()].slice(0, REMINDERS_HITS_MAX));
  // WHERE IT IS SAID is the bell's (49-notices.js): a card and a system
  // notification for each one not set quiet, and the bell for all of them.
  const loud = fresh.filter((h) => !(rs.find((x) => x.id === h.reminder) || {}).quiet);
  if (utilityTab === "reminders" && !$("utility-page").hidden) reminderRead();
  else noticeCards("reminders", loud.map((h) => ({ title: (UTILITY_KINDS[h.kind] || { describe: () => h.kind }).describe(h),
    ends: h.ends_at_ms, link: "/utility/reminders" })));
  noticeSystem("reminders", loud.map((h) => ({ tag: h.id, body: reminderSay(h), link: "/utility/reminders" })));
  renderUtility();
}

/// ONE FIRED ITEM AS A ROW, the same under the bell and on the Reminders tab.
const reminderHitHtml = (h, now) => `
      <div class="brow frow${h.ends_at_ms <= now ? " none" : ""}">
        <span class="bname">${escHtml((UTILITY_KINDS[h.kind] || { describe: () => h.kind }).describe(h))}
          <span class="fnode">${escHtml(new Date(h.at_ms).toLocaleString(LANG === "zh" ? "zh-CN" : "en-GB", { dateStyle: "short", timeStyle: "short" }))}</span></span>
        ${h.ends_at_ms > now ? `<span class="bscore" data-ends="${h.ends_at_ms}">${utilityLeft(h.ends_at_ms - now)}</span>`
          : `<span class="bnone">${escHtml(tr("ended"))}</span>`}
      </div>`;
/// ONE FIRED ITEM IN A SENTENCE: what it is, and how long it stays open.
const reminderSay = (h) => trF("{what} is open — {left} left", {
  what: (UTILITY_KINDS[h.kind] || { describe: () => h.kind }).describe(h),
  left: utilityLeft(h.ends_at_ms - Date.now()) });
function reminderRead() {
  const hs = reminderHits();
  if (hs.some((h) => !h.read)) storeJson(REMINDERS_HITS_KEY, hs.map((h) => ({ ...h, read: true })));
}

/// A REMINDER IN WORDS: each attribute it holds, by name. The names were
/// copied from the item it was made from; an item on screen now that carries
/// the same attribute names it instead, so a corrected name reaches it.
function reminderLabel(r) {
  const kind = UTILITY_KINDS[r.kind];
  if (!kind) return r.kind;
  const live = worldOf(r.kind) || [];
  return kind.attributes.filter(([k]) => k in (r.attributes || {})).map(([k]) => {
    const v = r.attributes[k];
    const now = live.find((x) => x.attributes[k] === v);
    return kind.nameOf(k, now ? now.names : r.names, v);
  }).join(" · ");
}

/// UNDER A REMINDER: how many it matches open now, or, for a kind known ahead
/// and nothing open, when it fires next. A reminder nothing can match says so
/// instead — one an older page saved, before its editor offered only what occurs.
function reminderStatus(r) {
  const open = (worldOf(r.kind) || []).filter((x) => reminderMatches(r, x)).length;
  const kind = UTILITY_KINDS[r.kind];
  if (!open && kind && kind.possible && kind.possible(r) === false) return { text: tr("The game has never opened one like this"), never: true };
  const next = !open && kind && kind.next ? kind.next(r) : null;
  const stats = kind && kind.stats ? kind.stats(r, open) : null;
  return { text: [next || trF("{n} open now", { n: open }), stats].filter(Boolean).join(" · "), never: false };
}

/// A LENGTH OF TIME in minutes under two hours, then hours, then days: "every
/// 2 h" for a fissure that comes every 90 minutes would round a third away.
function utilitySpan(ms) {
  const m = Math.max(1, Math.round(ms / 60000));
  if (m < 120) return trF("{n} min", { n: m });
  const h = Math.round(m / 60);
  return h < 48 ? trF("{n} h", { n: h }) : trF("{n} d", { n: Math.round(h / 24) });
}
/// A MOMENT PAST, as how long ago, in the same units.
function utilityAgo(ms) {
  const m = Math.floor((Date.now() - ms) / 60000);
  if (m < 1) return tr("just now");
  if (m < 60) return trF("{n} min ago", { n: m });
  const h = Math.floor(m / 60);
  return h < 48 ? trF("{n} h ago", { n: h }) : trF("{n} d ago", { n: Math.floor(h / 24) });
}

/// A MOMENT AHEAD, in the reader's own time zone: weekday, date and time.
const utilityWhen = (ms) => new Date(ms).toLocaleString(LANG === "zh" ? "zh-CN" : "en-GB",
  { weekday: "short", month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit" });

// ---- A REMINDER FROM A ROW --------------------------------------------------
//
// THE BELL ON A ROW opens, under it, the reminder being made: what it will hold,
// each a chip the reader turns off or on. One at a time, on any list; every
// chip off is no reminder, so Save waits.

/// `{kind, id, pick: Set}`, or null.
let reminderDraft = null;
const BELL_SVG = `<svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 8a6 6 0 0 0-12 0c0 7-3 9-3 9h18s-3-2-3-9"/><path d="M13.7 21a2 2 0 0 1-3.4 0"/></svg>`;

function reminderBellHtml(item) {
  const on = reminders().some((r) => reminderMatches(r, item));
  const open = reminderDraft && reminderDraft.id === item.id;
  return `<button type="button" class="fbell${on ? " on" : ""}${open ? " open" : ""}" data-rbell="${escHtml(item.id)}"
    title="${escHtml(tr(on ? "A reminder matches this" : "Remind me of ones like this"))}" aria-label="${
    escHtml(tr("Remind me of ones like this"))}">${BELL_SVG}</button>`;
}
/// The editor, when this row's is open; `lead` says what it reminds of.
function reminderDraftHtml(item, lead) {
  if (!reminderDraft || reminderDraft.id !== item.id) return "";
  const kind = UTILITY_KINDS[item.kind];
  const chips = kind.attributes.filter(([k]) => item.attributes[k] != null).map(([k]) =>
    filterChip(kind.nameOf(k, item.names, item.attributes[k]), reminderDraft.pick.has(k), `data-rpick="${k}"`)).join("");
  return `<div class="frem">
      <span class="slotf-lab">${escHtml(lead)}</span>
      <div class="slotf-row">${chips}</div>
      <div class="frem-act">
        <button type="button" class="ghost-btn small" data-rsave${reminderDraft.pick.size ? "" : " disabled"}>${escHtml(tr("Save reminder"))}</button>
        <button type="button" class="ghost-btn small" data-rcancel>${escHtml(tr("Cancel"))}</button>
      </div>
    </div>`;
}
/// THE BELLS AND THE EDITOR IN `box` answer clicks; `rows` are the items drawn,
/// and a new draft holds the attributes in `defaults` the item has.
function reminderDraftWire(box, rows, defaults) {
  box.querySelectorAll("[data-rbell]").forEach((el) => {
    el.onclick = () => {
      const item = rows.find((x) => x.id === el.dataset.rbell);
      reminderDraft = reminderDraft && reminderDraft.id === item.id ? null
        : { kind: item.kind, id: item.id, pick: new Set(defaults.filter((k) => item.attributes[k] != null)) };
      renderUtility();
    };
  });
  const ed = box.querySelector(".frem");
  const item = ed && rows.find((x) => reminderDraft && x.id === reminderDraft.id);
  if (!item) return;
  ed.querySelectorAll("[data-rpick]").forEach((el) => {
    el.onclick = () => {
      const k = el.dataset.rpick;
      if (reminderDraft.pick.has(k)) reminderDraft.pick.delete(k); else reminderDraft.pick.add(k);
      renderUtility();
    };
  });
  ed.querySelector("[data-rsave]").onclick = () => {
    if (!reminderDraft.pick.size) return;
    reminderAdd(item.kind, Object.fromEntries([...reminderDraft.pick].map((k) => [k, item.attributes[k]])), item.names);
    reminderDraft = null;
    renderUtility();
    presetToast(tr("Reminder saved"));
  };
  ed.querySelector("[data-rcancel]").onclick = () => { reminderDraft = null; renderUtility(); };
}

/// The kind the Reminders tab is building a reminder for.
let reminderNewKind = null;
function renderReminders() {
  const box = $("utility-reminders");
  if (!box) return;
  const rs = reminders();
  const hits = reminderHits();
  const now = Date.now();
  if (!worldNamesAsk && worldNamesStale() && rs.some((r) => r.kind === "fissure")) worldNamesLoad().then(renderUtility);
  box.innerHTML = `<p class="bench-note rem-lead">${escHtml(tr("Reminders live in this browser and fire while WFSim is open in it, on any page."))}
      <a href="/notifications#notify-settings">${escHtml(tr("How you are told"))}</a></p>
    <h3 class="wgroup-h">${escHtml(tr("New reminder"))}</h3>
    <div id="reminder-new"></div>
    <h3 class="wgroup-h">${escHtml(tr("Your reminders"))}</h3>
    ${rs.length ? `<div class="bench-rows">${rs.map((r) => { const st = reminderStatus(r); return `
      <div class="brow frow">
        <span class="ftier">${escHtml(UTILITY_KINDS[r.kind] ? tr(utilityTitle(UTILITY_KINDS[r.kind].tab)) : r.kind)}</span>
        <span class="bname">${escHtml(reminderLabel(r))}
          <span class="fnode${st.never ? " warn" : ""}">${escHtml(st.text)}${
            r.cloud_sync === false && accountState.account ? ` · ${escHtml(tr("This browser only"))}` : ""}</span></span>
        ${filterChip(tr("Pop up"), !r.quiet, `data-rquiet="${escHtml(r.id)}" title="${escHtml(tr("Off: kept under the bell only"))}"`)}
        <button type="button" class="ghost-btn small" data-rdel="${escHtml(r.id)}">${escHtml(tr("Delete"))}</button>
      </div>`; }).join("")}</div>`
      : `<div class="sim-empty">${escHtml(tr("No reminders yet. Make one above, or with the bell on a row of a list."))}</div>`}
    <h3 class="wgroup-h">${escHtml(tr("Recently fired"))}</h3>
    ${hits.length ? `<div class="bench-rows">${hits.map((h) => reminderHitHtml(h, now)).join("")}</div>`
      : `<div class="sim-empty">${escHtml(tr("Nothing has fired yet."))}</div>`}`;
  box.querySelectorAll("[data-rdel]").forEach((el) => {
    el.onclick = () => { reminderRemove(el.dataset.rdel); renderUtility(); };
  });
  box.querySelectorAll("[data-rquiet]").forEach((el) => {
    el.onclick = () => { reminderQuiet(el.dataset.rquiet); renderUtility(); };
  });
  // ONE BUILDER AT A TIME, the picked kind's own (`UTILITY_KINDS[kind].build`).
  const tabAt = (k) => UTILITY_TABS.findIndex(([t]) => t === k.tab);
  const builders = Object.entries(UTILITY_KINDS).filter(([, k]) => k.build).sort(([, a], [, b]) => tabAt(a) - tabAt(b));
  if (!builders.some(([id]) => id === reminderNewKind)) reminderNewKind = builders.length ? builders[0][0] : null;
  const fresh = $("reminder-new");
  fresh.innerHTML = `<div class="slotf-row">${builders.map(([id, k]) =>
    filterChip(tr(utilityTitle(k.tab)), id === reminderNewKind, `data-rkind="${id}"`)).join("")}</div><div></div>`;
  fresh.querySelectorAll("[data-rkind]").forEach((el) => {
    el.onclick = () => { reminderNewKind = el.dataset.rkind; renderUtility(); };
  });
  if (reminderNewKind) UTILITY_KINDS[reminderNewKind].build(fresh.lastElementChild);
}
