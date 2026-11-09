// ---- UTILITY › ARBITRATIONS -------------------------------------------------
//
// The arbitration this hour and every one of the coming days — docs/UI.md
// §"Utility". DE's world state does not carry them; the worker reads a schedule
// computed ahead (worker/world.js `arbitrationsOf`), so a reminder can also say
// when it fires next.

/// A NEW REMINDER HOLDS these of the row it was made from.
const ARBITRATION_REMINDER_DEFAULT = ["mission", "faction"];
/// The schedule is asked again after this; it changes only by the hour.
const ARBITRATION_STALE_MS = 30 * 60_000;

let arbitrationMission = "all";
let arbitrationFaction = "all";

UTILITY_KINDS.arbitration = {
  tab: "arbitrations",
  render: renderArbitrations,
  attributes: [["mission"], ["faction"], ["node"]],
  nameOf: (k, names, v) => worldName((names || {})[k], v),
  describe: (a) => `${tr("Arbitration")} ${worldName(a.names.mission)} · ${worldName(a.names.node)}${
    a.names.system ? ` (${worldName(a.names.system)})` : ""}${a.names.faction ? ` · ${worldName(a.names.faction)}` : ""}`,
  build: arbitrationBuild,
  next: arbitrationNext,
  possible: arbitrationPossible,
};

// ---- THE SCHEDULE -----------------------------------------------------------

/// `{read_at_ms, items, choices}` from `/api/world/arbitrations`, or null.
let arbitrations = null;
let arbitrationsAsk = null;
let arbitrationsFailed = false;
let arbitrationsAt = 0;
function arbitrationsLoad() {
  if (arbitrationsAsk) return arbitrationsAsk;
  arbitrationsAsk = (async () => {
    try {
      const r = await fetch(`${LIVE_ORIGIN}/api/world/arbitrations`);
      const j = await r.json();
      if (!j.ok) throw new Error(j.error);
      arbitrations = j;
      arbitrationsFailed = false;
    } catch (_) {
      arbitrationsFailed = true;
    } finally {
      arbitrationsAt = Date.now();
      arbitrationsAsk = null;
    }
  })();
  return arbitrationsAsk;
}
/// THE SCHEDULE IF IT IS HERE, and asked for (then drawn again) when it is not
/// or has gone stale. Null until the first answer.
function arbitrationsNow() {
  if (!arbitrationsAsk && !arbitrationsFailed && Date.now() - arbitrationsAt >= ARBITRATION_STALE_MS) {
    arbitrationsLoad().then(renderUtility);
  }
  return arbitrations && arbitrations.items.filter((x) => x.ends_at_ms > Date.now());
}
function arbitrationsWaitHtml() {
  return arbitrationsFailed
    ? `<div class="sim-empty">${escHtml(tr("The arbitration schedule could not be reached."))} <button type="button" class="ghost-btn small" data-arb-retry>${escHtml(tr("Retry"))}</button></div>`
    : `<div class="sim-empty">${loadingHtml()}</div>`;
}
document.addEventListener("click", (e) => {
  if (e.target.closest("[data-arb-retry]")) { arbitrationsFailed = false; arbitrationsAt = 0; renderUtility(); }
});

/// WHEN A REMINDER FIRES NEXT, from the schedule: null while it is not here.
function arbitrationNext(r) {
  const items = arbitrationsNow();
  if (!items) return null;
  const now = Date.now();
  const hit = items.find((x) => x.started_at_ms > now && reminderMatches(r, x));
  return hit ? trF("Next: {when}", { when: utilityWhen(hit.started_at_ms) }) : tr("Not in the next 14 days");
}

/// WHETHER A NODE of the schedule from now on holds every attribute the
/// reminder states; null while the schedule is not here.
function arbitrationPossible(r) {
  if (!arbitrationsNow()) return null;
  return Object.entries(arbitrations.choices.node).some(([id, n]) =>
    Object.entries(r.attributes || {}).every(([k, v]) => (k === "node" ? id : n[k]) === v));
}

// ---- THE PAGE ---------------------------------------------------------------

function renderArbitrations() {
  const filters = $("arbitration-filters"), list = $("arbitration-list");
  if (!filters || !list) return;
  const all = arbitrationsNow();
  if (!all) { filters.innerHTML = ""; list.innerHTML = arbitrationsWaitHtml(); return; }
  const now = Date.now();
  // EACH FILTER OFFERS what the coming days hold, counted under the other.
  const offered = (k, other, otherV) => {
    const seen = new Map();
    for (const x of all) {
      if (otherV !== "all" && x.attributes[other] !== otherV) continue;
      const v = x.attributes[k];
      if (!v) continue;
      const e = seen.get(v) || { v, name: worldName(x.names[k], v), n: 0 };
      e.n++;
      seen.set(v, e);
    }
    return [...seen.values()].sort((a, b) => b.n - a.n || a.name.localeCompare(b.name, LANG));
  };
  const missions = offered("mission", "faction", arbitrationFaction);
  const factions = offered("faction", "mission", arbitrationMission);
  if (arbitrationMission !== "all" && !missions.some((m) => m.v === arbitrationMission)) arbitrationMission = "all";
  if (arbitrationFaction !== "all" && !factions.some((f) => f.v === arbitrationFaction)) arbitrationFaction = "all";
  filters.innerHTML = `<div class="slotf-row">${filterChip(tr("All"), arbitrationMission === "all", `data-amission="all"`)}${
      missions.map((m) => filterChip(m.name, arbitrationMission === m.v, `data-amission="${escHtml(m.v)}"`, m.n)).join("")}</div>
    <div class="slotf-row">${filterChip(tr("All"), arbitrationFaction === "all", `data-afaction="all"`)}${
      factions.map((f) => filterChip(f.name, arbitrationFaction === f.v, `data-afaction="${escHtml(f.v)}"`, f.n)).join("")}</div>`;
  filters.querySelectorAll("[data-amission]").forEach((el) => {
    el.onclick = () => { arbitrationMission = el.dataset.amission; renderUtility(); };
  });
  filters.querySelectorAll("[data-afaction]").forEach((el) => {
    el.onclick = () => { arbitrationFaction = el.dataset.afaction; renderUtility(); };
  });
  const rows = all.filter((x) => (arbitrationMission === "all" || x.attributes.mission === arbitrationMission)
    && (arbitrationFaction === "all" || x.attributes.faction === arbitrationFaction));
  // ONE GROUP PER DAY, in the reader's own time zone; the hour open now first.
  const day = (ms) => new Date(ms).toLocaleDateString(LANG === "zh" ? "zh-CN" : "en-GB", { weekday: "long", month: "long", day: "numeric" });
  const hour = (ms) => new Date(ms).toLocaleTimeString(LANG === "zh" ? "zh-CN" : "en-GB", { hour: "2-digit", minute: "2-digit" });
  let html = "", group = null;
  for (const a of rows) {
    const open = a.started_at_ms <= now;
    const g = open ? "now" : day(a.started_at_ms);
    if (g !== group) {
      html += `${group === null ? "" : "</div>"}<h3 class="wgroup-h">${escHtml(open ? tr("Now") : g)}</h3><div class="bench-rows">`;
      group = g;
    }
    html += `
      <div class="brow frow${open ? " fnow" : ""}">
        <span class="ftier">${escHtml(hour(a.started_at_ms))}</span>
        <span class="bname">${escHtml(worldName(a.names.mission, a.attributes.mission))}
          <span class="fnode">${escHtml(worldName(a.names.node, a.attributes.node))}${
            a.names.system ? ` · ${escHtml(worldName(a.names.system))}` : ""}${
            a.names.faction ? ` · ${escHtml(worldName(a.names.faction, a.attributes.faction))}` : ""}</span></span>
        <span class="bscore" title="${escHtml(tr(open ? "Time left" : "Starts in"))}">${open ? "" : `${escHtml(tr("in"))} `}<span data-ends="${
          open ? a.ends_at_ms : a.started_at_ms}">${utilityLeft((open ? a.ends_at_ms : a.started_at_ms) - now)}</span></span>
        ${reminderBellHtml(a)}
      </div>${reminderDraftHtml(a, tr("Remind me when an arbitration is"))}`;
  }
  list.innerHTML = rows.length ? `${html}</div>`
    : `<div class="sim-empty">${escHtml(tr("No arbitration in the next 14 days matches these filters."))}</div>`;
  reminderDraftWire(list, rows, ARBITRATION_REMINDER_DEFAULT);
}

// ---- A REMINDER MADE FROM NOTHING -------------------------------------------
//
// Its mission type, faction and node, each Any until picked. A node is one
// mission type and faction (the worker's `choices.node`), so EACH OFFERS ONLY
// what some node of the schedule from now on holds beside the other picks: no
// reminder waits for an arbitration that never comes.

let arbitrationNew = { mission: null, faction: null, node: null };
function arbitrationBuild(box) {
  if (!arbitrationsNow()) { box.innerHTML = arbitrationsWaitHtml(); return; }
  const c = arbitrations.choices;
  const fits = (id, n, skip) => ["mission", "faction", "node"].every((k) => k === skip || arbitrationNew[k] === null
    || (k === "node" ? id : n[k]) === arbitrationNew[k]);
  const offered = (k) => new Set(Object.entries(c.node).filter(([id, n]) => fits(id, n, k)).map(([id, n]) => k === "node" ? id : n[k]));
  const missions = offered("mission"), factions = offered("faction"), nodeIds = offered("node");
  const chip = (k, v, label) => filterChip(label, arbitrationNew[k] === v, `data-anew="${k}" data-v="${v === null ? "" : escHtml(v)}"`);
  const named = (m) => Object.entries(m).map(([id, n]) => ({ id, label: worldName(n, id) }))
    .sort((a, b) => a.label.localeCompare(b.label, LANG));
  const nodes = Object.entries(c.node).filter(([id]) => nodeIds.has(id)).map(([id, n]) => ({ value: id,
    label: `${worldName(n.name, id)}${n.system ? ` · ${worldName(n.system)}` : ""}` }))
    .sort((a, b) => a.label.localeCompare(b.label, LANG));
  const any = Object.values(arbitrationNew).some((v) => v !== null);
  box.innerHTML = `<div class="frem frem-new">
      <div class="slotf-row"><span class="slotf-lab">${escHtml(tr("Mission"))}</span>${chip("mission", null, tr("Any"))}${
        named(c.mission).filter((m) => missions.has(m.id)).map((m) => chip("mission", m.id, m.label)).join("")}</div>
      <div class="slotf-row"><span class="slotf-lab">${escHtml(tr("Faction"))}</span>${chip("faction", null, tr("Any"))}${
        named(c.faction).filter((f) => factions.has(f.id)).map((f) => chip("faction", f.id, f.label)).join("")}</div>
      <div class="slotf-row"><span class="slotf-lab">${escHtml(tr("Node"))}</span>${
        ddButton("arbitration-new-node", { value: arbitrationNew.node ?? "", search: true,
          items: [{ value: "", label: tr("Any") }, ...nodes],
          onPick: (v) => { arbitrationNew.node = v || null; renderUtility(); } })}</div>
      <div class="frem-act">
        <button type="button" class="ghost-btn small" data-anew-save${any ? "" : " disabled"}>${escHtml(tr("Save reminder"))}</button>
      </div>
    </div>`;
  box.querySelectorAll("[data-anew]").forEach((el) => {
    el.onclick = () => { arbitrationNew[el.dataset.anew] = el.dataset.v || null; renderUtility(); };
  });
  box.querySelector("[data-anew-save]").onclick = () => {
    const attributes = Object.fromEntries(Object.entries(arbitrationNew).filter(([, v]) => v !== null));
    if (!Object.keys(attributes).length) return;
    const node = attributes.node ? c.node[attributes.node] : null;
    reminderAdd("arbitration", attributes, { mission: c.mission[attributes.mission] || null,
      faction: c.faction[attributes.faction] || null, node: node ? node.name : null, system: node ? node.system : null });
    arbitrationNew = { mission: null, faction: null, node: null };
    renderUtility();
    presetToast(tr("Reminder saved"));
  };
}
