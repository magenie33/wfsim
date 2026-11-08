// ---- CLOUD SYNC, IN ONE PLACE ------------------------------------------------
//
// `/account/sync` (docs/UI.md §"Build sync"): every item this browser holds,
// across every collection and every weapon, with what the account syncs of it.
// It switches an item's sync and nothing else — an item is made, renamed and
// deleted in its own bar, so a list of hundreds is never a place to lose one.
let cloudView = { q: "", status: "all", kind: "all", by: "time" };
/// THE ACCOUNT'S BROWSERS as the server last listed them, or null before it has.
let cloudDevices = null;
/// WHOSE THE LISTS ABOVE AND BELOW ARE: signing in as someone else on this page
/// forgets them, or the next account is shown the last one's browsers and trash.
let cloudFor = null;
function cloudForget(who) {
  if (cloudFor === who) return;
  cloudFor = who;
  cloudDevices = null;
  cloudTrash = null;
}
async function loadCloudDevices() {
  const who = cloudFor;
  const r = await syncCall({ devices: true });
  if (who !== cloudFor) return;
  cloudDevices = r && r.ok && Array.isArray(r.devices) ? r.devices : [];
  if (authKindOf(location.pathname) === "sync" && $("cloud-items")) renderAuthPage("sync");
}

/// EVERY BROWSER THAT SYNCS THE ACCOUNT: when it last synced, and whether that
/// went through — a failure and what it left behind, said where every other
/// browser can see it.
function cloudDevicesHtml() {
  if (cloudDevices === null) { loadCloudDevices(); return ""; }
  if (!cloudDevices.length) return "";
  const me = syncDevice().id;
  const when = (t) => (t ? new Date(t).toLocaleString(accountLocale(), { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" }) : "—");
  const rows = cloudDevices.map((d) => {
    const state = d.ok === null ? "" : d.ok
      ? `<span class="tag ok">${aT("Synced")}</span>${d.unsynced ? ` <span class="tag warn">${escHtml(tr("{n} items did not sync").replace("{n}", d.unsynced))}</span>` : ""}`
      : `<span class="tag bad">${aT("Could not sync")}</span> <span class="set-note">${escHtml(d.reason || "")}</span>`;
    return `<div class="kv"><dt>${escHtml(d.label)}${d.id === me ? ` <span class="tag">${aT("This device")}</span>` : ""}</dt>
      <dd>${escHtml(tr("last synced {time}").replace("{time}", when(d.round_at || d.seen_at)))} ${state}${
        d.held != null ? ` <span class="set-note">${escHtml(tr("{n} items here").replace("{n}", d.held))}</span>` : ""}</dd><span></span></div>`;
  }).join("");
  return `<div class="block"><div class="bh"><h2>${aT("Devices")}</h2><span class="sub">${cloudDevices.length}</span></div>
    <div class="bb"><dl class="kvs">${rows}</dl></div></div>`;
}
const cloudPicked = new Set();

/// WHERE AN ITEM IS ABOUT: its weapon, frame, companion or riven family, by
/// name; a fight, a target or an Operator build is about nothing.
function cloudOwner(domain, scope) {
  if (!scope) return "";
  const ws = (META && META.weapons) || [];
  if (domain === RIVENS) return (ws.find((w) => rivenScope(w.id) === scope) || {}).name || scope;
  if (domain === WF_BUILDS) return (((META && META.warframes) || []).find((f) => f.id === scope) || {}).name || scope;
  if (domain === COMP_BUILDS) return (compHost(scope) || {}).name || scope;
  return (ws.find((w) => w.id === scope) || {}).name || scope;
}
/// The page an item opens on, or "" where it has none of its own.
function cloudLink(domain, p) {
  const w = p.scope && ((META && META.weapons) || []).find((x) => x.id === p.scope);
  if (domain === BUILDS && w) return `${weaponPath(w.id)}?build=${encodeURIComponent(p.id)}`;
  if (domain === OPT_DOMAIN && w) return `${weaponPath(w.id)}/optimizer`;
  // A FRAME OR COMPANION THE ROSTER NO LONGER NAMES has no page to open.
  if (domain === WF_BUILDS && ((META && META.warframes) || []).some((f) => f.id === p.scope)) return holderPath(p.scope);
  if (domain === COMP_BUILDS && p.scope && compHost(p.scope)) return holderPath(p.scope);
  if (domain === OPS) return "/operator";
  return "";
}
const CLOUD_STATE = { error: "Could not sync just now.", other: "This browser holds items synced with another account.",
  paused: "Sync is off on this browser.", not_included: "Not available for this account" };
const CLOUD_KIND = { "builder-builds": "Builds", "simulator-scenarios": "Scenarios", optimizer: "Searches",
  warframes: "Warframe builds", companions: "Companions", operators: "Operator builds", rivens: "Rivens", enemies: "Custom enemies",
  reminders: "Reminders" };

/// Every item this browser holds, as the page lists it.
function cloudItems() {
  const rejected = new Map(((syncStatus && syncStatus.unsynced) || []).map((u) => [u.id, u.reason]));
  // ANOTHER ACCOUNT'S ITEMS are on this browser only, as far as this account knows.
  const mine = syncStatus.state !== "other" && syncEnabled();
  const out = [];
  for (const [id, { list, p }] of syncLocal()) {
    const domain = syncDomain(list);
    const reason = rejected.get(id);
    // A REMINDER IS NAMED BY WHAT IT HOLDS, and opens on its tab.
    const reminder = domain === "reminders";
    out.push({ id, list, domain, p, name: reminder ? reminderLabel(p) : p.name || "", saved: p.savedAt || p.made_at_ms || 0,
      owner: reminder ? "" : cloudOwner(domain, p.scope), link: reminder ? "/utility/reminders" : cloudLink(domain, p),
      state: reason ? "rejected" : mine && isCloudSynced(p) ? "synced" : "local", reason });
  }
  return out;
}

function cloudPage(a) {
  cloudForget(a.id);
  const n = syncedCounts();
  const meter = (pool, label, what) => {
    const cap = syncAllowance && syncAllowance[pool];
    const pct = cap ? Math.min(100, Math.round(100 * n[pool] / cap)) : 0;
    return `<div class="meter"><div class="lbl"><span>${aT(label)}</span><span>${aT(what)}</span></div>
      <b>${n[pool]}</b><span class="of">${cap ? escHtml(` / ${cap}`) : ""} ${aT("synced")}${cap && n[pool] >= cap ? ` · ${aT("allowance used")}` : ""}</span>
      ${cap ? `<div class="bar${n[pool] >= cap ? " warn" : ""}"><i style="width:${pct}%"></i></div>` : ""}</div>`;
  };
  const s = syncStatus;
  const when = s.at ? new Date(s.at).toLocaleTimeString(accountLocale(), { hour: "2-digit", minute: "2-digit" }) : "";
  const usage = `<div class="block"><div class="bh"><h2>${aT("Usage")}</h2><span class="sub">${aT("Saving on this browser is never limited.")}</span></div>
    <div class="bb"><div class="meters">${meter("presets", "Presets", "builds, fights, searches, Warframe, companion and Operator builds")}${
      meter("customs", "Customs", "rivens, custom enemies")}${meter("reminders", "Reminders", "Void Fissures, Arbitrations")}</div>
      <div class="sync-acts"><span>${s.state === "on"
        ? `<span class="tag ok">${aT("On")}</span> ${escHtml(tr("last synced {time}").replace("{time}", when))}`
        : aT(CLOUD_STATE[s.state] || "Checking…")}</span>
        <label><input type="checkbox" id="sync-auto" ${syncAuto() ? "checked" : ""}> ${aT("Upload new items")}</label>
        <span class="sp"></span>
        <button class="ghost-btn btn-sm" data-saves="export">${aT("Export")}</button>
        <button class="ghost-btn btn-sm" data-saves="import">${aT("Import")}</button>
        ${syncEnabled() ? `<button class="ghost-btn btn-sm" data-auth="sync-pause">${aT("Pause sync")}</button>` : ""}
        <button class="ghost-btn btn-sm" data-auth="sync-now">${aT(syncEnabled() ? "Sync now" : "Enable sync")}</button></div></div></div>`;
  return `<div class="settings">${settingsNav(a, "sync")}
    <div class="set-main"><h1 class="page">${aT("Cloud sync")}</h1>${usage}${cloudDevicesHtml()}
      <div class="block" id="cloud-items">${cloudListHtml()}</div>
      <div class="block" id="cloud-trash">${cloudTrashHtml()}</div></div></div>`;
}

/// WHAT THE ACCOUNT CAN STILL RESTORE, as the server last listed it, or null
/// before it has. A deletion is kept for 30 days (docs/SYNC.md).
let cloudTrash = null;
async function loadCloudTrash() {
  const who = cloudFor;
  const r = await syncCall({ trash: true });
  if (who !== cloudFor) return;
  cloudTrash = r && r.ok && Array.isArray(r.trash) ? r.trash : [];
  const box = $("cloud-trash");
  if (box) box.innerHTML = cloudTrashHtml();
}
function cloudTrashHtml() {
  if (cloudTrash === null) { loadCloudTrash(); return ""; }
  const day = (t) => new Date(t).toLocaleString(accountLocale(), { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
  const rows = cloudTrash.map((x) => {
    const domain = syncDomain(x.list);
    const owner = META ? cloudOwner(domain, x.scope) : "";
    const left = Math.max(1, Math.ceil((x.ends_at - Date.now()) / 86400000));
    return `<tr><td class="nm">${escHtml(x.name || "")}</td><td class="muted">${aT(CLOUD_KIND[domain] || domain)}</td>
      <td>${owner ? escHtml(owner) : `<span class="muted">—</span>`}</td><td class="muted">${escHtml(day(x.deleted_at))}</td>
      <td class="muted">${escHtml(tr("{n} days left").replace("{n}", left))}</td>
      <td><button type="button" class="ghost-btn btn-sm" data-crestore="${escHtml(x.id)}">${aT("Restore")}</button></td></tr>`;
  }).join("");
  return `<div class="bh"><h2>${aT("Recently deleted")}</h2><span class="sub">${aT("Kept for 30 days, then erased.")}</span></div>
    ${rows ? `<div class="tbl-wrap"><table class="items"><thead><tr><th>${aT("Name")}</th><th>${aT("Kind")}</th><th>${aT("About")}</th>
      <th>${aT("Deleted")}</th><th></th><th></th></tr></thead><tbody>${rows}</tbody></table></div>`
      : `<div class="bb"><p class="set-note">${aT("Nothing deleted in the last 30 days.")}</p></div>`}`;
}

// RESTORE: the server puts the item back as a new write, and the round that
// follows brings it into this browser like any change made elsewhere.
document.addEventListener("click", async (e) => {
  const t = e.target && e.target.closest && e.target.closest("#cloud-trash [data-crestore]");
  if (!t) return;
  t.disabled = true;
  const r = await syncCall({ restore: [t.dataset.crestore] });
  if (r && r.ok && (r.restored || []).length) await syncNow({ automatic: true });
  else noteInline(tr(r && r.ok && (r.refused || []).length
    ? "Not restored: the account's sync allowance is used"
    : "Could not restore just now."));
  cloudTrash = null;
  renderAuthPage("sync");
});

function cloudListHtml() {
  // THE PAGE CAN BE OPENED BEFORE THE ROSTER HAS LOADED — a link straight to
  // /account/sync — and the roster names every item's weapon. The route draws
  // the page again once it is here.
  if (!META) return `<div class="bh"><h2>${aT("All items")}</h2></div><div class="bb"><p class="set-note">${aT("Loading…")}</p></div>`;
  const all = cloudItems();
  const v = cloudView;
  const count = (st) => all.filter((x) => st === "all" || x.state === st).length;
  const q = v.q.trim().toLowerCase();
  let rows = all.filter((x) => (v.status === "all" || x.state === v.status) && (v.kind === "all" || x.domain === v.kind)
    && (!q || `${x.name} ${x.owner}`.toLowerCase().includes(q)));
  rows.sort((x, y) => y.saved - x.saved);
  const seg = (st, label) => `<button type="button" data-cstatus="${st}" class="${v.status === st ? "on" : ""}">${aT(label)}<em>${count(st)}</em></button>`;
  const kinds = [...new Set(all.map((x) => x.domain))];
  const day = (t) => (t ? new Date(t).toLocaleString(accountLocale(), { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" }) : "—");
  const STATE = { synced: ["on", "Synced"], local: ["", "This browser only"], rejected: ["bad", ""] };
  const row = (x) => {
    const [cls, label] = STATE[x.state];
    const text = x.state === "rejected" ? tr(SYNC_REJECTED[x.reason] || x.reason) : tr(label);
    return `<tr><td><input type="checkbox" data-cpick="${escHtml(x.id)}" ${cloudPicked.has(x.id) ? "checked" : ""} aria-label="${aT("Select")}"></td>
      <td class="nm">${escHtml(x.name)}</td><td class="muted">${aT(CLOUD_KIND[x.domain] || x.domain)}</td>
      <td>${x.owner ? escHtml(x.owner) : `<span class="muted">—</span>`}</td><td class="muted">${escHtml(day(x.saved))}</td>
      <td><button type="button" class="cloud ${cls}" data-ctoggle="${escHtml(x.id)}" ${x.state === "rejected" || !syncEnabled() ? "disabled" : ""}>${CLOUD_SVG}${escHtml(text)}</button></td>
      <td>${x.link ? `<a class="open" href="${escHtml(x.link)}">${aT("Open")} ↗</a>` : ""}</td></tr>`;
  };
  // BY WEAPON: one group per owner, the owners in the order their newest item was saved.
  let body;
  if (v.by === "owner") {
    const groups = new Map();
    for (const x of rows) { const k = x.owner || tr("Not about a weapon"); if (!groups.has(k)) groups.set(k, []); groups.get(k).push(x); }
    body = [...groups].map(([k, xs]) => `<tr class="grp"><td colspan="7">${escHtml(k)} · ${xs.length}</td></tr>${xs.map(row).join("")}`).join("");
  } else body = rows.map(row).join("");
  const picked = [...cloudPicked].filter((id) => all.some((x) => x.id === id)).length;
  return `<div class="bh"><h2>${aT("All items")}</h2><span class="sub">${escHtml(tr("{n} items · {m} synced")
      .replace("{n}", all.length).replace("{m}", count("synced")))}</span></div>
    <div class="toolbar">
      <input type="search" id="cloud-q" placeholder="${aT("Search names, weapons, Warframes…")}" value="${escHtml(v.q)}" aria-label="${aT("Search")}">
      <div class="fd-seg" role="group">${seg("all", "All")}${seg("synced", "Synced")}${seg("local", "This browser only")}${
        count("rejected") ? seg("rejected", "Did not sync") : ""}</div>
      <select id="cloud-kind" aria-label="${aT("Kind")}"><option value="all">${aT("Every kind")}</option>${kinds.map((k) =>
        `<option value="${escHtml(k)}" ${v.kind === k ? "selected" : ""}>${aT(CLOUD_KIND[k] || k)}</option>`).join("")}</select>
      <select id="cloud-by" aria-label="${aT("Order")}"><option value="time">${aT("Newest first")}</option>
        <option value="owner" ${v.by === "owner" ? "selected" : ""}>${aT("By weapon")}</option></select></div>
    ${picked ? `<div class="bulk"><b>${escHtml(tr("{n} selected").replace("{n}", picked))}</b>
      <button class="ghost-btn btn-sm" data-cbulk="on">${aT("Sync to the account")}</button>
      <button class="ghost-btn btn-sm" data-cbulk="off">${aT("Keep on this browser only")}</button>
      <button class="ghost-btn btn-sm" data-cbulk="clear">${aT("Clear selection")}</button></div>` : ""}
    <div class="tbl-wrap"><table class="items"><thead><tr><th><input type="checkbox" data-cpick="*" aria-label="${aT("Select all")}"></th>
      <th>${aT("Name")}</th><th>${aT("Kind")}</th><th>${aT("About")}</th><th>${aT("Saved")}</th><th>${aT("Sync")}</th><th></th></tr></thead>
      <tbody>${body || `<tr><td colspan="7" class="muted">${aT("Nothing here.")}</td></tr>`}</tbody></table></div>`;
}

function renderCloudList() {
  const box = $("cloud-items");
  if (!box) return;
  const focus = document.activeElement && document.activeElement.id;
  box.innerHTML = cloudListHtml();
  const el = focus && $(focus);
  if (el) { el.focus(); if (el.setSelectionRange) el.setSelectionRange(el.value.length, el.value.length); }
}

/// Switch one item; the list and the meters redraw from what was written.
function cloudSwitch(id, on) {
  const x = cloudItems().find((i) => i.id === id);
  return !!x && setCloudSync(x.list, id, on);
}

// ONE LISTENER EACH, on the page's own container, which outlives every render.
document.addEventListener("click", (e) => {
  const t = e.target && e.target.closest && e.target.closest("#cloud-items [data-ctoggle], #cloud-items [data-cstatus], #cloud-items [data-cbulk]");
  if (!t) return;
  if (t.dataset.cstatus) cloudView.status = t.dataset.cstatus;
  else if (t.dataset.ctoggle) cloudSwitch(t.dataset.ctoggle, !t.classList.contains("on"));
  else if (t.dataset.cbulk === "clear") cloudPicked.clear();
  else {
    // EACH ITEM NOT ALREADY SO, until the allowance refuses one — which says so.
    const on = t.dataset.cbulk === "on";
    const items = cloudItems();
    for (const id of cloudPicked) {
      const x = items.find((i) => i.id === id);
      if (!x || isCloudSynced(x.p) === on) continue;
      if (!setCloudSync(x.list, id, on)) break;
    }
    cloudPicked.clear();
  }
  renderAuthPage("sync");
});
document.addEventListener("change", (e) => {
  const t = e.target;
  if (!t || !t.closest || !t.closest("#cloud-items")) return;
  if (t.id === "cloud-kind") cloudView.kind = t.value;
  else if (t.id === "cloud-by") cloudView.by = t.value;
  else if (t.dataset.cpick === "*") {
    for (const b of document.querySelectorAll("#cloud-items [data-cpick]:not([data-cpick='*'])")) {
      if (t.checked) cloudPicked.add(b.dataset.cpick); else cloudPicked.delete(b.dataset.cpick);
    }
  } else if (t.dataset.cpick) {
    if (t.checked) cloudPicked.add(t.dataset.cpick); else cloudPicked.delete(t.dataset.cpick);
  } else return;
  renderCloudList();
});
onTyped(document, (e) => {
  if (!e.target || e.target.id !== "cloud-q") return;
  cloudView.q = e.target.value;
  renderCloudList();
});
