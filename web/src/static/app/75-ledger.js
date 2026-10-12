// ---- THE POINTS LEDGER (/compute) ------------------------------------------
//
// WHERE EVERY POINT WENT, day by day in the reader's time zone: this browser's
// own, asked by its id (`/api/board/ledger`), each listed device's for its
// owner, and what came in to the account device by device. The server says
// each credited hour's holder — this device, the reader's account, or another
// account it never names — and the page only groups and draws
// (worker/contribution.js §"The ledger", docs/BOARD.md §"Contribution").

let ledgerHere = null;
let ledgerAccount = null;
const ledgerOf = {};
let ledgerAll = false;
let ledgerOpenDay = null;
let ledgerOpenDevice = null;
/// ASKED AGAIN EVERY `LEDGER_EVERY_MS`, not each half minute (73-compute.js
/// `computeRefresh`), and at once for another account: a day's hours move once
/// an hour, and each ask reads every hour the device was credited.
const LEDGER_EVERY_MS = 5 * 60_000;
let ledgerAskedAt = 0;
let ledgerAskedFor = null;

/// THE FIRST HOUR ASKED: the reader's midnight 29 days back as its UTC hour,
/// "" for all of it.
function ledgerFrom() {
  if (ledgerAll) return "";
  const d = new Date(); d.setHours(0, 0, 0, 0); d.setDate(d.getDate() - 29);
  return d.toISOString().slice(0, 13);
}
/// THIS BROWSER'S ID, never minted here: a browser that never computed has no ledger.
const ledgerOwnId = () => {
  try { const v = localStorage.getItem(VERIFIER_KEY) || ""; return /^[a-z0-9]{16,40}$/.test(v) ? v : ""; } catch (_) { return ""; }
};
const ledgerPost = (path, body) => fetch(path, { method: "POST", credentials: "same-origin",
  headers: { "content-type": "application/json" }, body: JSON.stringify(body) })
  .then((r) => (r.ok ? r.json() : null)).catch(() => null);

async function loadLedgers() {
  const id = ledgerOwnId(), from = ledgerFrom();
  ledgerAskedAt = Date.now();
  ledgerAskedFor = (accountState.account || {}).id || null;
  const [here, acct] = await Promise.all([
    id ? ledgerPost("/api/board/ledger", { verifier: id, from }) : null,
    accountState.account ? accountCall("GET", `/api/account/ledger?from=${encodeURIComponent(from)}`) : null,
  ]);
  ledgerHere = here && here.ok ? here : null;
  ledgerAccount = acct && acct.ok ? acct : null;
  if (ledgerOpenDevice) await loadDeviceLedger(ledgerOpenDevice);
}
async function loadDeviceLedger(id) {
  const r = await accountCall("POST", "/api/account/devices/ledger", { id, from: ledgerFrom() });
  ledgerOf[id] = r && r.ok ? r : null;
}

/// A UTC HOUR AS THE READER'S DAY AND HOUR.
const ledgerLocal = (h) => {
  const t = new Date(`${h}:00:00Z`);
  const p = (n) => String(n).padStart(2, "0");
  return { day: `${t.getFullYear()}-${p(t.getMonth() + 1)}-${p(t.getDate())}`, hour: `${p(t.getHours())}:00` };
};
const LEDGER_HOLDER = { device: "this device", you: "your account", other: "another account" };
/// WHERE A SET OF HOURS WENT, holder by holder, an ended span said as ended.
function ledgerWent(hours) {
  const by = new Map();
  for (const h of hours) {
    const k = `${h.holder}${h.closed ? "+" : ""}`;
    by.set(k, (by.get(k) || 0) + h.points);
  }
  return [...by].map(([k, p]) => `${escHtml(tr(LEDGER_HOLDER[k.replace("+", "")]))}${
    k.endsWith("+") ? ` ${aT("(no longer holds it)")}` : ""} ${computePts(p)}`).join(" · ");
}

/// ONE DEVICE'S LEDGER: the all-time split, the window's unpaid results by why,
/// and its days newest first, a day opening into its hours.
function ledgerHtml(l, key) {
  if (!l) return "";
  const s = l.summary, u = l.unpaid, n = (x) => Number(x || 0).toLocaleString(accountLocale());
  const top = `<p class="set-note" style="margin:0 0 6px">${escHtml(
    tr("{p} points in all — {d} kept on this device, {y} in your account, {o} in another account.")
      .replace("{p}", n(s.points)).replace("{d}", n(s.device)).replace("{y}", n(s.you)).replace("{o}", n(s.other)))}</p>`
    + (l.refused ? `<p class="set-note" style="margin:0 0 6px">${aT("This device is refused for now: its points count for nothing until the refusal ends.")}</p>` : "")
    + (u.disagreed || u.waiting || u.refused ? `<p class="set-note" style="margin:0 0 6px">${escHtml(
      tr("In this window, results that earned nothing: {d} disagreed with the agreed result, {w} wait for another computer's result, {r} were set aside by a refusal.")
        .replace("{d}", n(u.disagreed)).replace("{w}", n(u.waiting)).replace("{r}", n(u.refused)))}</p>` : "");
  const days = new Map();
  for (const h of l.hours) {
    const { day } = ledgerLocal(h.hour);
    if (!days.has(day)) days.set(day, []);
    days.get(day).push(h);
  }
  const rows = [...days].reverse().map(([day, hours]) => {
    const open = ledgerOpenDay === `${key}|${day}`;
    const sum = (k) => hours.reduce((t, h) => t + h[k], 0);
    const given = (hs) => {
      const g = hs.reduce((t, h) => t + (h.given || []).reduce((u, x) => u + x.points, 0), 0);
      return g ? ` <span class="set-note">${escHtml(tr("incl. {n} compensation")).replace("{n}", computePts(g))}</span>` : "";
    };
    const head = `<tr class="lg-day" data-auth="compute-ledger-day" data-day="${escHtml(`${key}|${day}`)}" role="button" tabindex="0">`
      + `<td>${open ? "▾" : "▸"} ${escHtml(day)}</td><td class="w-num">${computePts(sum("tasks"))}</td><td class="w-num">${computePts(sum("points"))}${given(hours)}</td><td>${ledgerWent(hours)}</td></tr>`;
    return head + (open ? hours.slice().reverse().map((h) => `<tr class="lg-hour"><td>${escHtml(ledgerLocal(h.hour).hour)}</td>`
      + `<td class="w-num">${computePts(h.tasks)}</td><td class="w-num">${computePts(h.points)}${given([h])}</td><td>${ledgerWent([h])}</td></tr>`).join("") : "");
  }).join("");
  const more = l.earlier || ledgerAll ? `<button class="ghost-btn btn-sm" data-auth="compute-ledger-span">${
    aT(ledgerAll ? "Show the last 30 days" : "Show all history")}</button>` : "";
  return top + (rows ? `<table class="w-tab lg-tab"><thead><tr><th>${aT("Day")}</th><th>${aT("Results")}</th><th>${aT("Points")}</th><th>${aT("Went to")}</th></tr></thead><tbody>${rows}</tbody></table>`
    : `<p class="set-note" style="margin:0">${aT("Nothing computed yet")}</p>`) + more;
}

/// THIS BROWSER'S LEDGER, under its own block.
function ledgerHereHtml() {
  if (!ledgerHere) return "";
  return `<div class="block" id="compute-ledger"><div class="bh"><h2>${aT("Points ledger")}</h2></div><div class="bb"><p class="set-note">${
    aT("Where this device's points went, day by day, in your time zone.")}</p>${ledgerHtml(ledgerHere, "here")}</div></div>`;
}

/// ONE LISTED DEVICE'S LEDGER, opened under its row in "Your devices".
const ledgerDeviceHtml = (id) => (ledgerOpenDevice === id && ledgerOf[id]
  ? `<div class="lg-device">${ledgerHtml(ledgerOf[id], id)}</div>` : "");

/// WHAT CAME IN TO THE ACCOUNT, day by day, device by device.
function ledgerAccountHtml() {
  const a = ledgerAccount;
  if (!a || !a.hours.length) return "";
  const own = computeOwnId();
  const name = (id) => {
    const d = a.devices.find((x) => x.id === id) || {};
    return `${escHtml(d.label || tr("Device {id}").replace("{id}", id))}${d.removed ? ` ${aT("(removed)")}` : ""}${id === own ? ` ${aT("(this browser)")}` : ""}`;
  };
  const days = new Map();
  for (const h of a.hours) {
    const { day } = ledgerLocal(h.hour);
    const m = days.get(day) || new Map();
    m.set(h.id, (m.get(h.id) || 0) + h.points);
    days.set(day, m);
  }
  const rows = [...days].sort((x, y) => (x[0] < y[0] ? 1 : -1)).map(([day, m]) =>
    `<tr><td>${escHtml(day)}</td><td class="w-num">${computePts([...m.values()].reduce((t, p) => t + p, 0))}</td><td>${
      [...m].map(([id, p]) => `${name(id)} ${computePts(p)}`).join(" · ")}</td></tr>`).join("");
  return `<div class="block" id="compute-account-ledger"><div class="bh"><h2>${aT("Points by day")}</h2></div><div class="bb"><p class="set-note">${
    aT("What came in to your account each day, device by device.")}</p><table class="w-tab lg-tab"><thead><tr><th>${aT("Day")}</th><th>${aT("Points")}</th><th>${aT("Devices")}</th></tr></thead><tbody>${rows}</tbody></table></div></div>`;
}

/// THE LEDGER'S BUTTONS: a day opened or shut, all history or the last 30 days,
/// and a listed device's ledger opened or shut.
async function ledgerAct(el, what) {
  if (what === "compute-ledger-day") ledgerOpenDay = ledgerOpenDay === el.dataset.day ? null : el.dataset.day;
  else if (what === "compute-ledger-span") { ledgerAll = !ledgerAll; await loadLedgers(); }
  else if (what === "compute-ledger-device") {
    const id = el.dataset.id;
    ledgerOpenDevice = ledgerOpenDevice === id ? null : id;
    if (ledgerOpenDevice) await loadDeviceLedger(id);
  }
  renderAuthPage("compute");
}
