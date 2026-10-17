// ---- NOTIFICATIONS --------------------------------------------------------------
//
// EVERYTHING THE SITE TELLS THE READER, in one place — docs/UI.md
// §"Notifications". Three sources, one bell:
//
//   reminders  what fired in this browser (`reminderHits`, 27-utility.js)
//   messages   what the site said (`/api/notices`, worker/notices.js) — to
//              everyone, or to this account
//   account    the account's state while it lasts (a hidden name, a deletion
//              pending, a device cooling down); never stored, so never stale
//
// The bell counts what is unread and opening it reads everything; a card in
// the corner says a reminder or a message as it arrives, and a system
// notification too when the reader turned that on.

/// Asked this often while the tab is on screen, and never while it is not: a
/// desktop client in the tray is a device online, and asking from every one
/// would be calls that grow with devices rather than readers.
const NOTICES_EVERY_MS = 15 * 60_000;
/// Where this browser keeps how far it has read what the site said, for a
/// reader with no account, and the newest it has already said in a card.
const NOTICES_READ_KEY = "wfsim-notices-read";
const NOTICES_SAID_KEY = "wfsim-notices-said";
/// The account states this browser has already counted, by kind and value.
const NOTICES_STATE_SEEN_KEY = "wfsim-notices-state-seen";
/// The reader's choices: per category, a card and a system notification.
const NOTIFY_KEY = "wfsim-notify";
const NOTIFY_CATEGORIES = [["reminders", "Reminders"], ["messages", "Messages"]];

/// `{ notices, read_at, state }` as the site last answered, or null.
let noticesState = null;
let noticesAt = 0;
let noticesAsk = null;
/// What was unread when the bell or the page was last opened, so a row still
/// shows it was new after opening read it.
let noticesFresh = new Set();
let noticesFilter = "all";

/// A CHOICE in `NOTIFY_KEY`. A card is on unless turned off. A system
/// notification is off in a browser until asked for, since the browser asks
/// permission; in the desktop client it is on, since the client is what
/// carries a reminder while its window is away.
function notifyOn(category, channel) {
  const v = (storedJson(NOTIFY_KEY, {})[category] || {})[channel];
  if (typeof v === "boolean") return v;
  if (channel === "card") return true;
  // THE OLD SWITCH, for a reader who turned system notifications on before
  // there were categories.
  if (category === "reminders" && storedJson(REMINDERS_SYSTEM_KEY, 0) === 1) return true;
  return isDesktopShell();
}
function notifySet(category, channel, on) {
  const all = storedJson(NOTIFY_KEY, {});
  storeJson(NOTIFY_KEY, { ...all, [category]: { ...(all[category] || {}), [channel]: on } });
}
const isDesktopShell = () => !!(window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke);

// ---- WHAT THE SITE SAID ---------------------------------------------------------

/// THE WORDS FOR A KIND THE PAGE WORDS ITSELF: `{ title, link }` from its params.
/// A kind missing here is said by its own text, or not at all.
const NOTICE_WORDS = {
  membership_ending: (p) => ({ title: trF("Your membership ends on {date}.", { date: noticeDay(p.ends_at) }), link: "/account/billing" }),
  membership_ended: () => ({ title: tr("Your membership has ended."), link: "/account/billing" }),
  membership_renewal_failed: () => ({ title: tr("Your membership could not renew. Check the payment method."), link: "/account/billing" }),
  device_cooling: (p) => ({ title: trF(p.devices > 1 ? "{n} of your devices take no compute work until {when}." : "One of your devices takes no compute work until {when}.",
    { n: p.devices, when: noticeWhen(p.until) }), link: "/compute" }),
  display_name_hidden: () => ({ title: tr("Your display name is hidden, so others see your username."), link: "/account" }),
  account_deleting: (p) => ({ title: trF("Your account will be deleted on {date}. You can still cancel.", { date: noticeDay(p.at) }), link: "/account" }),
};
/// A DAY OR A MOMENT in the reader's own time zone; the site keeps UTC.
const noticeDay = (iso) => new Date(iso).toLocaleDateString(LANG === "zh" ? "zh-CN" : "en-GB", { year: "numeric", month: "long", day: "numeric" });
const noticeWhen = (iso) => new Date(iso).toLocaleString(LANG === "zh" ? "zh-CN" : "en-GB", { month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit" });

/// ONE NOTICE IN WORDS, or null for one this page cannot say.
function noticeWorded(n) {
  if (n.text) return { title: n.text[LANG] || n.text.en, link: n.link };
  const w = NOTICE_WORDS[n.kind];
  return w ? { ...w(n.params || {}), ...(n.link ? { link: n.link } : {}) } : null;
}

/// THE ACCOUNT'S STATE as told now: what the site answered, and what the page
/// already holds about the account.
function noticeStates() {
  const a = accountState.account;
  const out = [...((noticesState && noticesState.state) || [])];
  if (a && a.display_name_hidden) out.push({ kind: "display_name_hidden", params: { why: a.display_name_why } });
  if (a && a.delete_after) out.push({ kind: "account_deleting", params: { at: a.delete_after } });
  return out.map((s) => ({ ...s, key: `${s.kind}:${JSON.stringify(s.params)}` }));
}

/// The instant everything before which is read: the account's, else this browser's.
function noticesReadAt() {
  const local = storedJson(NOTICES_READ_KEY, null);
  const server = noticesState && noticesState.read_at;
  return [local, server].filter(Boolean).sort().pop() || null;
}

/// ASKED: the account's notices when signed in, else what everyone is told.
/// What arrived since the last answer, and is unread, is said in a card.
function noticesLoad() {
  if (noticesAsk) return noticesAsk;
  noticesAsk = (async () => {
    try {
      const j = accountState.account ? await accountCall("GET", "/api/notices")
        : await fetch("/api/notices/everyone").then((r) => r.json());
      if (!j || !j.ok) return;
      noticesState = { notices: j.notices || [], read_at: j.read_at || null, state: j.state || [] };
      noticesAnnounce();
    } catch (_) { /* no site to ask — the dev server, or offline */ } finally {
      noticesAt = Date.now();
      noticesAsk = null;
      renderBell();
      if (authKindOf(location.pathname) === "notifications") renderAuthPage("notifications");
    }
  })();
  return noticesAsk;
}
setInterval(() => { if (!document.hidden && Date.now() - noticesAt >= NOTICES_EVERY_MS) noticesLoad(); }, 60_000);
document.addEventListener("visibilitychange", () => {
  if (!document.hidden && noticesAt && Date.now() - noticesAt >= NOTICES_EVERY_MS) noticesLoad();
});

/// WHAT IS NEW SINCE THE LAST CARD: said once, on the first browser to see it.
/// The first answer a browser ever gets only sets the line — a backlog is the
/// bell's to count, never a stack of cards.
function noticesAnnounce() {
  const said = storedJson(NOTICES_SAID_KEY, null);
  const newest = noticesState.notices.map((n) => n.created_at).sort().pop() || null;
  if (newest && (!said || newest > said)) storeJson(NOTICES_SAID_KEY, newest);
  if (!said) return;
  const read = noticesReadAt();
  const fresh = noticesState.notices.filter((n) => n.created_at > said && !(read && n.created_at <= read))
    .map((n) => ({ n, w: noticeWorded(n) })).filter((x) => x.w);
  if (!fresh.length) return;
  noticeCards("messages", fresh.map(({ w }) => ({ title: w.title, link: w.link || "/notifications" })));
  noticeSystem("messages", fresh.map(({ n, w }) => ({ tag: `notice-${n.id}`, body: w.title, link: w.link || "/notifications" })));
}

// ---- ONE LIST OF EVERYTHING -----------------------------------------------------

/// EVERY ROW, newest first, the account's state pinned on top: `{ key,
/// category, at_ms, title, sub, link, ends_at_ms, unread, tone }`.
function noticeRows() {
  const read = noticesReadAt();
  const seen = [].concat(storedJson(NOTICES_STATE_SEEN_KEY, []));
  const states = noticeStates().map((s) => {
    const w = (NOTICE_WORDS[s.kind] || (() => null))(s.params || {});
    return w && { key: `state:${s.key}`, category: "messages", at_ms: Infinity, title: w.title, link: w.link,
      unread: !seen.includes(s.key), tone: "warn" };
  }).filter(Boolean);
  const said = ((noticesState && noticesState.notices) || []).map((n) => {
    const w = noticeWorded(n);
    return w && { key: `notice:${n.id}`, category: "messages", at_ms: Date.parse(n.created_at), title: w.title,
      link: w.link, unread: !(read && n.created_at <= read) };
  }).filter(Boolean);
  const fired = reminderHits().map((h) => ({ key: `reminder:${h.id}`, category: "reminders", at_ms: h.at_ms,
    title: (UTILITY_KINDS[h.kind] || { describe: () => h.kind }).describe(h), link: "/utility/reminders",
    ends_at_ms: h.ends_at_ms, unread: !h.read }));
  return [...states, ...[...said, ...fired].sort((a, b) => b.at_ms - a.at_ms)];
}

/// READ, ALL OF IT: the bell or the page was opened. What was unread is kept
/// for the rows on screen, so they still show which were new.
function noticesRead() {
  const rows = noticeRows();
  noticesFresh = new Set(rows.filter((r) => r.unread).map((r) => r.key));
  reminderRead();
  const at = new Date().toISOString();
  storeJson(NOTICES_READ_KEY, at);
  storeJson(NOTICES_STATE_SEEN_KEY, noticeStates().map((s) => s.key));
  if (accountState.account && noticesState && noticesState.notices.some((n) => !(noticesState.read_at && n.created_at <= noticesState.read_at))) {
    accountCall("POST", "/api/notices/read", { at }).then((j) => { if (j && j.ok && noticesState) noticesState.read_at = j.read_at; });
  }
  if ($("rem-toasts")) $("rem-toasts").replaceChildren();
}

const noticeRowHtml = (r, now) => {
  const fresh = r.unread || noticesFresh.has(r.key);
  const tag = r.link ? "a" : "div";
  const when = r.at_ms === Infinity ? "" : new Date(r.at_ms).toLocaleString(LANG === "zh" ? "zh-CN" : "en-GB", { dateStyle: "short", timeStyle: "short" });
  const right = r.ends_at_ms == null ? ""
    : r.ends_at_ms > now ? `<span class="bscore" data-ends="${r.ends_at_ms}">${utilityLeft(r.ends_at_ms - now)}</span>`
      : `<span class="bnone">${escHtml(tr("ended"))}</span>`;
  return `<${tag} class="brow frow nrow${fresh ? " fresh" : ""}${r.tone ? ` ${r.tone}` : ""}${r.ends_at_ms != null && r.ends_at_ms <= now ? " none" : ""}"${
    r.link ? ` href="${escHtml(r.link)}"` : ""}>
      <span class="bname">${escHtml(r.title)}${when ? `<span class="fnode">${escHtml(when)}</span>` : ""}</span>${right}</${tag}>`;
};
/// The filter chips, shared by the bell and the page.
const noticeFilterHtml = (rows) => `<div class="slotf-row nfilter">${[["all", "All"], ...NOTIFY_CATEGORIES].map(([k, label]) =>
  filterChip(tr(label), noticesFilter === k, `data-nfilter="${k}"`, k === "all" ? null : rows.filter((r) => r.category === k).length || null)).join("")}</div>`;
const noticeFiltered = (rows) => rows.filter((r) => noticesFilter === "all" || r.category === noticesFilter);

// ---- THE BELL -------------------------------------------------------------------

const BELL_PANEL_ROWS = 8;

/// THE BELL, on the bar for every reader: its count what is unread, its panel
/// the latest of everything.
function renderBell() {
  const bell = $("rem-bell");
  if (!bell) return;
  const rows = noticeRows();
  const unread = rows.filter((r) => r.unread).length;
  bell.hidden = false;
  const count = $("rem-bell-count");
  count.hidden = !unread;
  count.textContent = unread ? String(Math.min(unread, 99)) : "";
  if (!bell.classList.contains("open")) return;
  const now = Date.now();
  const shown = noticeFiltered(rows).slice(0, BELL_PANEL_ROWS);
  $("rem-bell-panel").innerHTML = `<div class="rbell-h">${escHtml(tr("Notifications"))}</div>
    ${noticeFilterHtml(rows)}
    ${shown.length ? `<div class="bench-rows">${shown.map((r) => noticeRowHtml(r, now)).join("")}</div>`
      : `<div class="sim-empty">${escHtml(tr("Nothing here yet."))}</div>`}
    <a class="tbmenu-link rbell-all" href="/notifications">${escHtml(tr("All notifications"))}</a>`;
}
(function () {
  const bell = $("rem-bell");
  if (!bell) return;
  const set = (open) => {
    bell.classList.toggle("open", open);
    $("rem-bell-btn").setAttribute("aria-expanded", open ? "true" : "false");
    // OPENED, it says what the cards said: they go, read.
    if (open) { noticesRead(); utilityClock(); }
    renderUtility();
  };
  $("rem-bell-btn").addEventListener("click", (e) => { e.stopPropagation(); set(!bell.classList.contains("open")); });
  $("rem-bell-panel").addEventListener("click", (e) => {
    const f = e.target.closest("[data-nfilter]");
    if (f) { e.stopPropagation(); noticesFilter = f.dataset.nfilter; renderBell(); return; }
    if (e.target.closest("a[href]")) set(false);
  });
  document.addEventListener("click", (e) => { if (bell.classList.contains("open") && !bell.contains(e.target)) set(false); });
  document.addEventListener("keydown", (e) => { if (e.key === "Escape" && bell.classList.contains("open") && !imeComposing(e)) set(false); });
})();

// ---- A CARD IN THE CORNER, AND THE SYSTEM'S OWN ---------------------------------

/// A card stays this long, held while the pointer is on it.
const TOAST_MS = 8000;
/// AT MOST THIS MANY CARDS: more arriving at once is one card that counts them.
const TOASTS_MAX = 3;

/// CARDS for what arrived in `category`, unless the reader turned them off:
/// `[{ title, link, ends }]`.
function noticeCards(category, cards) {
  if (!cards.length || !notifyOn(category, "card")) return;
  let box = $("rem-toasts");
  if (!box) {
    box = document.createElement("div");
    box.id = "rem-toasts";
    box.className = "rtoasts";
    box.setAttribute("aria-live", "polite");
    document.body.appendChild(box);
  }
  const now = Date.now();
  const shown = cards.length > TOASTS_MAX
    ? [{ title: trF(category === "reminders" ? "{n} reminders fired" : "{n} new notifications", { n: cards.length }), link: "/notifications" }]
    : cards;
  for (const c of shown) {
    const el = document.createElement("div");
    el.className = "rtoast";
    el.innerHTML = `<span class="rtoast-ic">${BELL_SVG}</span>
      <span class="rtoast-b"><span class="rtoast-t">${escHtml(c.title)}</span>
        <span class="rtoast-s">${c.ends ? `${escHtml(tr("Time left"))} <span data-ends="${c.ends}">${utilityLeft(c.ends - now)}</span> · ` : ""}<a href="${escHtml(c.link || "/notifications")}">${escHtml(tr("View"))}</a></span></span>
      <button type="button" class="rtoast-x" aria-label="${escHtml(tr("Close"))}">×</button>`;
    let timer = null;
    const go = () => { clearTimeout(timer); el.remove(); };
    const wait = () => { clearTimeout(timer); timer = setTimeout(go, TOAST_MS); };
    el.addEventListener("mouseenter", () => clearTimeout(timer));
    el.addEventListener("mouseleave", wait);
    el.querySelector(".rtoast-x").onclick = go;
    el.querySelector("a").addEventListener("click", () => setTimeout(go, 0));
    box.appendChild(el);
    wait();
  }
  while (box.children.length > TOASTS_MAX) box.firstElementChild.remove();
}

/// THE SYSTEM'S OWN NOTIFICATION, only while the page is out of sight and only
/// if the reader turned it on: the desktop client's, else the browser's.
/// `[{ tag, body, link }]`.
function noticeSystem(category, items) {
  if (!items.length || !document.hidden || !notifyOn(category, "system")) return;
  if (isDesktopShell()) {
    for (const it of items) {
      window.__TAURI_INTERNALS__.invoke("notify", { title: "WFSim", body: it.body, link: it.link || "/notifications" })
        .catch(() => noticeBrowserSystem([it]));
    }
    return;
  }
  noticeBrowserSystem(items);
}
function noticeBrowserSystem(items) {
  if (typeof Notification !== "function" || Notification.permission !== "granted") return;
  for (const it of items) {
    try {
      const n = new Notification("WFSim", { body: it.body, tag: it.tag });
      n.onclick = () => { window.focus(); nav(it.link || "/notifications"); n.close(); };
    } catch (_) { /* the browser refused; the bell already holds it */ }
  }
}
/// THE DESKTOP CLIENT, asked to show the page a notification named once its
/// window is back.
window.addEventListener("wfsim-notify-open", (e) => nav((e.detail && e.detail.link) || "/notifications"));

// ---- /notifications -------------------------------------------------------------

function notificationsPage(main) {
  noticesRead();
  const rows = noticeRows();
  const now = Date.now();
  const shown = noticeFiltered(rows);
  const canBrowser = typeof Notification === "function";
  const systemChip = (k) => {
    if (!isDesktopShell() && canBrowser && Notification.permission === "denied") return `<span class="fnode">${escHtml(tr("Blocked by this browser"))}</span>`;
    if (!isDesktopShell() && !canBrowser) return "";
    return filterChip(tr(isDesktopShell() ? "Windows notification" : "System notification"), notifyOn(k, "system"), `data-nset="${k}:system"`);
  };
  main.innerHTML = `<div class="settings solo notices-page"><div class="set-main"><h1 class="page">${escHtml(tr("Notifications"))}</h1>
    <div class="block"><div class="bb">${noticeFilterHtml(rows)}
      ${shown.length ? `<div class="bench-rows">${shown.map((r) => noticeRowHtml(r, now)).join("")}</div>`
        : `<div class="sim-empty">${escHtml(tr("Nothing here yet."))}</div>`}</div></div>
    <div class="block" id="notify-settings"><div class="bh"><h2>${escHtml(tr("How you are told"))}</h2></div><div class="bb">
      <dl class="kvs">${NOTIFY_CATEGORIES.map(([k, label]) => `<div class="kv"><dt>${escHtml(tr(label))}</dt>
        <dd class="nset">${filterChip(tr("Card on the page"), notifyOn(k, "card"), `data-nset="${k}:card"`)}${systemChip(k)}</dd><span></span></div>`).join("")}</dl>
      <p class="set-note">${escHtml(tr(isDesktopShell()
        ? "A Windows notification comes while this window is minimised or in the tray."
        : "A system notification comes while this tab is in the background, if the browser allows it."))}
        <a href="/utility/reminders">${escHtml(tr("Manage reminders"))}</a></p></div></div>
  </div></div>`;
  main.querySelectorAll("[data-nfilter]").forEach((el) => {
    el.onclick = () => { noticesFilter = el.dataset.nfilter; notificationsPage(main); };
  });
  main.querySelectorAll("[data-nset]").forEach((el) => {
    el.onclick = async () => {
      const [k, channel] = el.dataset.nset.split(":");
      const on = !notifyOn(k, channel);
      // THE BROWSER ASKS, on the reader's own click — its permission prompt,
      // never one of ours.
      if (on && channel === "system" && !isDesktopShell() && Notification.permission !== "granted"
        && (await Notification.requestPermission()) !== "granted") { notificationsPage(main); return; }
      notifySet(k, channel, on);
      notificationsPage(main);
    };
  });
  renderBell();
}
