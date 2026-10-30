// ---- Riven Analyst: what each riven the board has measured is worth -----
//
// docs/BOARD.md §"The Riven Analyst". GLOBAL: every riven row this weapon's
// board publishes against the #1 riven-free row of its ruler and mode. Every
// number is a published score, grouped by `rivenGroups`, the function the
// headless query `builder.rivens.read` answers with. A build is drawn with the
// simulator's own card and opens in the Builder as the board row it is.

/// The ruler the reader picked, by id. Null = the first one with a riven row.
let raRuler = null;
/// The riven row the reader has open, by its board key.
let raPick = null;

const raStat = (id) => {
  const s = rivenStat(id);
  return s ? rivenStatName(s) : id;
};
const raShown = (r) => String(r.shown != null ? r.shown : (r.score || 0).toFixed(4));
/// A board riven's stats WITH the roll each was stored at — the shape alone is
/// not the card the score was measured on.
const raRolls = (rv) => {
  const rows = rv.bonuses.map((id, i) => ({ id, slot: String(i), roll: rv.rolls?.[i] }))
    .concat(rv.malus ? [{ id: rv.malus, slot: "malus", roll: rv.rolls?.[rv.bonuses.length] }] : []);
  return rivenDisplayOrder(rivenPoolAll(), rows).map((s) => `${s.slot === "malus" ? "−" : "+"}${raStat(s.id)}`
    + (s.roll == null ? "" : ` ×${Number(Number(s.roll).toFixed(3))}`)).join(" · ");
};

/// A BOARD ROW AS THE SIMULATOR DRAWS A BUILD, and the way into the Builder.
function raBuild(w, x, head) {
  const rec = measuredRecordHtml(x.row);
  return `<div class="ra-build"><div class="sb-h">${escHtml(head)} · <b>${escHtml(raShown(x.row))}</b>${
    rec ? ` <span class="fd-when" title="${escHtml(measuredNote())}">${rec}</span>` : ""}</div>`
    + cardOfState(boardRowState(w, x.row), w)
    + `<a class="ghost-btn small sb-edit" href="${weaponPath(w.id)}" data-ra-open="${escHtml(x.key)}">${
      escHtml(tr("open in Builder"))}</a></div>`;
}

/// THE ROW OPENED THE WAY THE BUILD BAR OPENS IT — its ruler, then the row,
/// through the door — and then the Builder, where an official build is read-only.
async function raOpen(w, x) {
  await agentDo("shell.preset.open", { bar: "scenario", preset: x.row.benchmark });
  await agentDo("shell.preset.open", { bar: "build", preset: x.key });
  nav(weaponPath(w.id));
}

function renderRivenAnalyst() {
  const box = $("riven-analyst");
  const w = weaponInfo($("weapon").value);
  if (!box || !w || !META) return;
  // AS DEEP AS THE BUILD BAR READS, so every row drawn here is one it can open.
  const deep = boardGroupLeaders(BOARD[w.id]);
  const groups = rivenGroups(META, w, (BOARD[w.id] || []).filter((r) => deep(r, boardDepth)));
  if (!benchList().some((b) => b.id === raRuler)) raRuler = benchList()[0]?.id;
  const ruler = raRuler;
  const selector = benchSelectorHtml(ruler);
  const wire = () => wireBenchSelector(box, (id) => { raRuler = id; raPick = null; renderRivenAnalyst(); });
  if (!groups.some((g) => g.ruler_id === ruler)) {
    box.innerHTML = selector + `<p class="wb-empty">${escHtml(tr("No riven build of this weapon has been measured yet."))}</p>`;
    wire(); return;
  }
  const mine = groups.filter((g) => g.ruler_id === ruler);
  const all = mine.flatMap((g) => g.rivens);
  const pick = all.find((x) => x.key === raPick) || all[0];
  const group = mine.find((g) => g.rivens.includes(pick));
  const tables = mine.map((g) => {
    const rows = g.rivens.map((x) => `<tr class="wb-row${x === pick ? " sel" : ""}" data-ra="${escHtml(x.key)}">`
      + `<td>${escHtml(raRolls(x.row.riven))}</td><td class="wb-score">${escHtml(raShown(x.row))}</td>`
      + `<td class="wb-score">${x.gain == null ? "—" : escHtml(gainPct(x.gain))}</td>`
      + `<td class="wb-when">${measuredRecordHtml(x.row, false)}</td></tr>`).join("");
    return `<p class="wb-ceiling">${escHtml(modeLabel(w, g.mode))} · ${escHtml(tr("The board's best riven-free build"))}: <b>${
      g.top ? escHtml(raShown(g.top.row)) : "—"}</b></p>`
      + `<table class="wb-tab"><thead><tr><th>${escHtml(tr("Riven"))}</th><th>${escHtml(tr("Score"))}</th>`
      + `<th>${escHtml(tr("vs the best riven-free build"))}</th><th>${escHtml(tr("Record"))}</th></tr></thead><tbody>${rows}</tbody></table>`;
  }).join("");
  const builds = [pick && raBuild(w, pick, tr("With this riven")),
    group && group.top && raBuild(w, group.top, tr("The board's best riven-free build"))].filter(Boolean).join("");
  box.innerHTML = selector
    + `<p class="wb-terms">${escHtml(benchmarkName(ruler))}</p>` + tables
    + `<div class="ra-builds">${builds}</div>`;
  const byKey = new Map(mine.flatMap((g) => (g.top ? [g.top] : []).concat(g.rivens)).map((x) => [x.key, x]));
  wire();
  box.querySelectorAll("[data-ra]").forEach((el) => el.addEventListener("click", () => {
    raPick = el.dataset.ra;
    renderRivenAnalyst();
  }));
  box.querySelectorAll("[data-ra-open]").forEach((el) => el.addEventListener("click", (e) => {
    e.preventDefault();
    const x = byKey.get(el.dataset.raOpen);
    if (x) raOpen(w, x);
  }));
}
