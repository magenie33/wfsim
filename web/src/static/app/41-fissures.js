// ---- UTILITY › VOID FISSURES ------------------------------------------------
//
// Every Void Fissure open in the game now — docs/UI.md §"Utility". The bell on
// a row makes a reminder for fissures like it (`reminderDraft`, 27-utility.js).

/// THE THREE LISTS THE GAME KEEPS APART: the star chart, its Steel Path, and
/// Railjack's Void Storms — each a chip after All, in this order everywhere.
const FISSURE_LISTS = [["normal", "Normal"], ["steel_path", "Steel Path"], ["railjack", "Railjack"]];
/// The world state's own order, Lith first (`scripts/world_names.py`).
const FISSURE_TIERS = ["VoidT1", "VoidT2", "VoidT3", "VoidT4", "VoidT5", "VoidT6"];
/// A NEW REMINDER HOLDS these of the row it was made from; the reader adds the
/// node or drops any of them before saving.
const FISSURE_REMINDER_DEFAULT = ["list", "tier", "mission"];

let fissureList = "all";
let fissureTier = "all";

const fissureListName = (v) => tr((FISSURE_LISTS.find(([k]) => k === v) || [v, v])[1]);
UTILITY_KINDS.fissure = {
  tab: "fissures",
  render: renderFissures,
  attributes: [["list"], ["tier"], ["planet"], ["mission"], ["node"]],
  // A PLANET IS NAMED AS `system`, the name every item already carries.
  nameOf: (k, names, v) => k === "list" ? fissureListName(v)
    : worldName((names || {})[k === "planet" ? "system" : k] || (k === "planet" && worldNames && (worldNames.planets || {})[v]), v),
  build: fissureBuild,
  possible: fissurePossible,
  stats: fissureStats,
  describe: (f) => [
    f.attributes && f.attributes.list !== "normal" ? fissureListName(f.attributes.list) : "",
    worldName(f.names.tier), worldName(f.names.mission),
  ].filter(Boolean).join(" ") + ` · ${worldName(f.names.node)}${f.names.system ? ` (${worldName(f.names.system)})` : ""}`,
};

function renderFissures() {
  const filters = $("fissure-filters"), list = $("fissure-list");
  if (!filters || !list) return;
  const wait = worldWaitHtml();
  if (wait) { filters.innerHTML = ""; list.innerHTML = wait; return; }
  const now = Date.now();
  const open = worldOf("fissure");
  const ofList = open.filter((f) => fissureList === "all" || f.attributes.list === fissureList);
  const tiers = FISSURE_TIERS.filter((t) => ofList.some((f) => f.attributes.tier === t));
  if (fissureTier !== "all" && !tiers.includes(fissureTier)) fissureTier = "all";
  const tierName = (t) => worldName((ofList.find((f) => f.attributes.tier === t) || { names: {} }).names.tier, t);
  filters.innerHTML = `<div class="slotf-row">${filterChip(tr("All"), fissureList === "all", `data-flist="all"`, open.length)}${FISSURE_LISTS.map(([k, label]) =>
      filterChip(tr(label), fissureList === k, `data-flist="${k}"`, open.filter((f) => f.attributes.list === k).length)).join("")}</div>
    <div class="slotf-row">${filterChip(tr("All"), fissureTier === "all", `data-ftier="all"`)}${
      tiers.map((t) => filterChip(tierName(t), fissureTier === t, `data-ftier="${t}"`,
        ofList.filter((f) => f.attributes.tier === t).length)).join("")}</div>`;
  filters.querySelectorAll("[data-flist]").forEach((el) => {
    el.onclick = () => { fissureList = el.dataset.flist; reminderDraft = null; renderFissures(); };
  });
  filters.querySelectorAll("[data-ftier]").forEach((el) => {
    el.onclick = () => { fissureTier = el.dataset.ftier; renderFissures(); };
  });
  const order = (t) => { const i = FISSURE_TIERS.indexOf(t); return i < 0 ? FISSURE_TIERS.length : i; };
  const listAt = (l) => FISSURE_LISTS.findIndex(([k]) => k === l);
  const rows = ofList.filter((f) => fissureTier === "all" || f.attributes.tier === fissureTier)
    .sort((a, b) => order(a.attributes.tier) - order(b.attributes.tier)
      || listAt(a.attributes.list) - listAt(b.attributes.list) || a.ends_at_ms - b.ends_at_ms);
  list.innerHTML = rows.length ? `<div class="bench-rows">${rows.map((f) => `
      <div class="brow frow">
        <span class="ftier">${escHtml(worldName(f.names.tier, f.attributes.tier))}</span>
        <span class="bname">${escHtml(worldName(f.names.mission, f.attributes.mission))}
          <span class="fnode">${fissureList === "all" ? `${escHtml(fissureListName(f.attributes.list))} · ` : ""}${escHtml(worldName(f.names.node, f.attributes.node))}${
            f.names.system ? ` · ${escHtml(worldName(f.names.system))}` : ""}</span></span>
        <span class="bscore" data-ends="${f.ends_at_ms}">${utilityLeft(f.ends_at_ms - now)}</span>
        ${reminderBellHtml(f)}
      </div>${reminderDraftHtml(f, tr("Remind me when a fissure opens with"))}`).join("")}</div>`
    : `<div class="sim-empty">${escHtml(tr("No fissure matches these filters."))}</div>`;
  list.insertAdjacentHTML("beforeend", `<p class="bench-note"><a href="/utility/reminders">${
    escHtml(tr("Waiting for one that is not open? Make a reminder for it"))}</a></p>`);
  reminderDraftWire(list, rows, FISSURE_REMINDER_DEFAULT);
}

/// A REMINDER MADE FROM NOTHING, for a fissure that is not open now: its list,
/// era, planet, mission and node, each Any until picked. EACH OFFERS ONLY what
/// some fissure the game has opened holds beside the other picks
/// (`/api/world/names` `fissures`), so no reminder waits for a fissure that
/// never comes. A Void Storm has no mission to match on.
const FISSURE_NEW_KEYS = ["list", "tier", "mission", "planet", "node"];
const fissureNewEmpty = () => Object.fromEntries(FISSURE_NEW_KEYS.map((k) => [k, null]));
let fissureNew = fissureNewEmpty();
/// EVERY COMBINATION SEEN, as `{list, tier, mission, planet, node, recent,
/// last_ms}`: the worker sends `[list, tier, mission, node, recent, last_ms]`,
/// and a node's planet is the node's own.
let fissureSeenOf = null, fissureSeenRows = [];
function fissureSeen() {
  const rows = (worldNames && worldNames.fissures) || [];
  if (fissureSeenOf === rows) return fissureSeenRows;
  const nodes = worldNames.nodes || {};
  fissureSeenOf = rows;
  fissureSeenRows = rows.map(([list, tier, mission, node, recent, last_ms]) => ({ list, tier, mission,
    planet: (nodes[node] || {}).planet || null, node: node || null, recent: recent || 0, last_ms: last_ms || 0 }));
  return fissureSeenRows;
}
/// Whether a seen combination holds every attribute in `picks` (null: any).
const fissureHolds = (c, picks) => Object.entries(picks).every(([k, v]) => v == null || c[k] === v);
/// The values of `k` some seen fissure holds with every other pick.
function fissureOffered(k) {
  const others = { ...fissureNew, [k]: null };
  return new Set(fissureSeen().filter((c) => c[k] !== null && fissureHolds(c, others)).map((c) => c[k]));
}
/// Whether a seen fissure holds every attribute the reminder states.
function fissurePossible(r) {
  const seen = fissureSeen();
  if (!seen.length) return null;
  return seen.some((c) => fissureHolds(c, r.attributes || {}));
}
/// HOW OFTEN ONE LIKE IT COMES: the fissures it would have matched in the
/// worker's window, as one every so long, and — none open now — how long ago
/// the last opened. Null before the log is read.
function fissureStats(r, open) {
  const seen = fissureSeen().filter((c) => fissureHolds(c, r.attributes || {}));
  if (!seen.length) return null;
  const recent = seen.reduce((s, c) => s + c.recent, 0);
  const last = Math.max(...seen.map((c) => c.last_ms));
  const often = recent ? trF("About every {every} in the last 7 days", { every: utilitySpan(worldNames.window_ms / recent) })
    : tr("None in the last 7 days");
  return open || !last ? often : `${often} · ${trF("last {ago}", { ago: utilityAgo(last) })}`;
}
function fissureBuild(box) {
  if (!worldNames) {
    box.innerHTML = worldNamesFailed
      ? `<div class="sim-empty">${escHtml(tr("The world state could not be reached."))} <button type="button" class="ghost-btn small" data-names-retry>${escHtml(tr("Retry"))}</button></div>`
      : `<div class="sim-empty">${escHtml(tr("Loading…"))}</div>`;
    const again = box.querySelector("[data-names-retry]");
    if (again) again.onclick = () => { worldNamesFailed = false; renderUtility(); };
    else worldNamesLoad().then(renderUtility);
    return;
  }
  if (fissureNew.list === "railjack") fissureNew.mission = null;
  const lists = fissureOffered("list"), tiers = fissureOffered("tier");
  const planets = fissureOffered("planet"), offered = fissureOffered("mission"), nodes = fissureOffered("node");
  const chip = (k, v, label) => filterChip(label, fissureNew[k] === v, `data-fnew="${k}" data-v="${v === null ? "" : v}"`);
  const missions = Object.entries(worldNames.missions).filter(([id]) => offered.has(id))
    .map(([id, n]) => ({ value: id, label: worldName(n, id) }))
    .sort((a, b) => a.label.localeCompare(b.label, LANG));
  const planetItems = Object.entries(worldNames.planets || {}).filter(([id]) => planets.has(id))
    .map(([id, n]) => ({ value: id, label: worldName(n, id) }));
  const planetName = (id) => worldName((worldNames.planets || {})[id], id);
  const nodeOf = (id) => (worldNames.nodes || {})[id] || {};
  const nodeItems = [...nodes].map((id) => ({ value: id, label: fissureNew.planet ? worldName(nodeOf(id).name, id)
      : `${worldName(nodeOf(id).name, id)} · ${planetName(nodeOf(id).planet)}` }))
    .sort((a, b) => a.label.localeCompare(b.label, LANG));
  const pick = (k) => (v) => { fissureNew[k] = v || null; renderUtility(); };
  const any = Object.values(fissureNew).some((v) => v !== null);
  box.innerHTML = `<div class="frem frem-new">
      <div class="slotf-row"><span class="slotf-lab">${escHtml(tr("List"))}</span>${chip("list", null, tr("Any"))}${
        FISSURE_LISTS.filter(([k]) => lists.has(k)).map(([k, label]) => chip("list", k, tr(label))).join("")}</div>
      <div class="slotf-row"><span class="slotf-lab">${escHtml(tr("Relic era"))}</span>${chip("tier", null, tr("Any"))}${
        FISSURE_TIERS.filter((t) => tiers.has(t)).map((t) => chip("tier", t, worldName(worldNames.tiers[t], t))).join("")}</div>
      <div class="slotf-row"><span class="slotf-lab">${escHtml(tr("Planet"))}</span>${
        ddButton("fissure-new-planet", { value: fissureNew.planet ?? "", search: true,
          items: [{ value: "", label: tr("Any") }, ...planetItems], onPick: pick("planet") })}</div>
      ${fissureNew.list === "railjack" ? "" : `<div class="slotf-row"><span class="slotf-lab">${escHtml(tr("Mission"))}</span>${
        ddButton("fissure-new-mission", { value: fissureNew.mission ?? "", search: true,
          items: [{ value: "", label: tr("Any") }, ...missions], onPick: pick("mission") })}</div>`}
      <div class="slotf-row"><span class="slotf-lab">${escHtml(tr("Node"))}</span>${
        ddButton("fissure-new-node", { value: fissureNew.node ?? "", search: true,
          items: [{ value: "", label: tr("Any") }, ...nodeItems], onPick: pick("node") })}</div>
      <div class="frem-act">
        <button type="button" class="ghost-btn small" data-fnew-save${any ? "" : " disabled"}>${escHtml(tr("Save reminder"))}</button>
      </div>
    </div>`;
  box.querySelectorAll("[data-fnew]").forEach((el) => {
    el.onclick = () => { fissureNew[el.dataset.fnew] = el.dataset.v || null; renderUtility(); };
  });
  box.querySelector("[data-fnew-save]").onclick = () => {
    const attributes = Object.fromEntries(Object.entries(fissureNew).filter(([, v]) => v !== null));
    if (!Object.keys(attributes).length) return;
    // A NODE NAMES ITS PLANET too, so the reminder reads like one made from a row.
    const planet = attributes.planet || (attributes.node && nodeOf(attributes.node).planet);
    reminderAdd("fissure", attributes, { tier: worldNames.tiers[attributes.tier] || null,
      mission: worldNames.missions[attributes.mission] || null,
      system: (planet && (worldNames.planets || {})[planet]) || null,
      node: (attributes.node && nodeOf(attributes.node).name) || null });
    fissureNew = fissureNewEmpty();
    renderUtility();
    presetToast(tr("Reminder saved"));
  };
}
