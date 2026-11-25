// ---- Popularity: every weapon, ranked by how many people test it ---------
//
// docs/ANALYTICS.md §"Shown publicly". ONE ranking of every weapon, by the
// people-days that ran a simulation or a search on it in the last 30 finished
// days (`/api/popularity`); the board page's own filters narrow it to a kind,
// and its rows draw it. The PLACE is shown, never the count behind it. Ten
// show; the rest is one click away.

/// `/api/popularity`'s answer; null while unasked, false where it is not offered.
let hotScores = null;
let hotAsk = null;
let hotSlot = "all";
let hotFilter = slotFilterNew();
let hotAll = false;
const HOT_SHOWN = 10;

function loadHot() {
  if (hotScores !== null || hotAsk) return;
  hotAsk = fetch("/api/popularity")
    .then((r) => (r.ok ? r.json() : null))
    .then((j) => { hotScores = j && j.ok ? j : false; }, () => { hotScores = false; })
    .finally(() => { hotAsk = null; renderHomeHot(); });
}

function renderHomeHot() {
  const box = $("home-hot");
  if (!box || !META) return;
  loadHot();
  box.hidden = !hotScores;
  if (!hotScores) { box.innerHTML = ""; return; }
  const score = (w) => hotScores.scores[w.id] || 0;
  const slots = ((META && META.equipment_slots) || []).filter((sl) => sl.holds === "weapon");
  const pool = (META.weapons || []).filter((w) => hotSlot === "all" || w.slot === hotSlot);
  // A WEAPON NOBODY TESTED HAS NO RANK: a zero is not a low score, and it
  // sorts after every weapon that has one, the way the board's unmeasured do.
  const rows = pool.filter((w) => slotFilterMatch(hotFilter, w))
    .sort((a, b) => score(b) - score(a) || a.name.localeCompare(b.name));
  const ranked = rows.filter((w) => score(w) > 0).length;
  const shown = hotAll ? rows : rows.slice(0, HOT_SHOWN);
  const active = (hotSlot !== "all") + !!hotFilter.type + !!hotFilter.cls + hotFilter.tags.length;
  const open = box.querySelector("details") ? box.querySelector("details").open : false;
  box.innerHTML = `<h2 class="home-h">${escHtml(tr("Popular"))} <span class="muted">${ranked} / ${rows.length}</span></h2>
    <details class="slotf-box"${open ? " open" : ""}><summary>${escHtml(tr("Filters"))}${
      active ? ` <span class="bcnt">${active}</span>` : ""}</summary>
      <div class="slotf-row"><span class="slotf-lab">${escHtml(tr("Equipment slot"))}</span>${
        filterChip(tr("All"), hotSlot === "all", `data-hslot="all"`)}${
        slots.map((sl) => filterChip(tr(sl.name), hotSlot === sl.id, `data-hslot="${escHtml(sl.id)}"`,
          (META.weapons || []).filter((w) => w.slot === sl.id).length)).join("")}</div>
      ${hotSlot === "all" ? "" : slotFilterRows(pool, hotFilter)}
    </details>
    ${rows.length ? `<div class="bench-rows">${shown.map((w, i) => `<a class="brow${score(w) ? "" : " none"}" href="${weaponPath(w.id)}">
        <span class="brank">${score(w) ? `#${i + 1}` : "—"}</span>
        ${imgTag(IMG(w.image), "bimg")}
        <span class="bname">${escHtml(w.name)}</span></a>`).join("")}</div>`
      : `<div class="sim-empty">${escHtml(tr("No weapon matches these filters."))}</div>`}
    ${rows.length > HOT_SHOWN ? `<button type="button" class="ghost-btn small hot-more">${escHtml(hotAll
      ? tr("Show the top 10") : trF("Show all {n}", { n: rows.length }))}</button>` : ""}`;
  box.querySelectorAll("[data-hslot]").forEach((el) => {
    el.onclick = () => { hotSlot = el.dataset.hslot; hotFilter = slotFilterNew(); renderHomeHot(); };
  });
  box.querySelectorAll("[data-sf]").forEach((el) => {
    el.onclick = () => { slotFilterClick(hotFilter, el); renderHomeHot(); };
  });
  const more = box.querySelector(".hot-more");
  if (more) more.onclick = () => { hotAll = !hotAll; renderHomeHot(); };
}
