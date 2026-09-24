// ---- Benchmark: this weapon's own board -------------------------------
// WHAT THIS WEAPON HAS BEEN MEASURED AT, one row per ruler and mode, and the
// build behind the row the reader picks. Nothing here computes a score: every
// number is a published row, and the only thing derived is a RANK, which is
// counted off `site/board/index.json` at read time rather than stored — a
// second copy of a ranking is a second answer to one question.
//
// TWO LEVELS, BECAUSE THE GRID IS NOT SMALL. A melee carries seven modes and
// the roster carries three rulers; twenty-one blocks is not a page. So the
// matrix states every cell in one line each, and one cell at a time opens.

/// The cell the reader has open, as `<benchmark>#<mode>`. Null = the first row.
let wbenchPick = null;

/// NO SCORE IS COMPARED ACROSS RULERS. Two rulers are two environments and
/// their numbers are not like terms, so the only figure that crosses a row is
/// the rank — a position inside one ruler, with the count it is out of.
const wbenchRank = (benchmark, mode, score, exalted) => {
  if (!BOARD_INDEX || score == null) return null;
  let total = 0;
  let above = 0;
  for (const [wid, rows] of Object.entries(BOARD_INDEX)) {
    // AMONG ITS OWN KIND. An Exalted weapon's numbers are its Warframe's
    // ability's — it seats no capacity of its own and scales with strength —
    // so a position counted against guns would be a comparison nothing
    // measured. The ranking page splits the same two lists.
    if (!!(weaponInfo(wid) || {}).exalted !== !!exalted) continue;
    let best = null;
    for (const r of rows || []) {
      if (r.benchmark !== benchmark || (r.mode || "base") !== mode) continue;
      if (rowHasRiven(r)) continue;
      if (best == null || r.score > best) best = r.score;
    }
    if (best == null) continue;
    total += 1;
    if (best > score) above += 1;
  }
  return total ? { rank: above + 1, total } : null;
};

/// Every (ruler, mode) this weapon has a riven-free row for, in the order the
/// roster declares its rulers — `board::benchmarks::all` sorts them once and
/// every consumer inherits it, this one included.
function wbenchCells(id) {
  const rows = BOARD[id] || (BOARD_INDEX && BOARD_INDEX[id]) || [];
  const order = (META.benchmarks || []).map((b) => b.id);
  // THE WEAPON'S OWN MODE ORDER, not the alphabet: `w.modes` is the order the
  // weapon declares them in, and it is the order every other surface draws.
  const modes = ((weaponInfo(id) || {}).modes || ["base"]);
  const cells = new Map();
  for (const r of rows) {
    const mode = r.mode || "base";
    const key = `${r.benchmark}#${mode}`;
    const cell = cells.get(key) || { benchmark: r.benchmark, mode, best: null, riven: null };
    const slot = rowHasRiven(r) ? "riven" : "best";
    if (cell[slot] == null || r.score > cell[slot].score) cell[slot] = r;
    cells.set(key, cell);
  }
  return [...cells.values()]
    .filter((c) => c.best)
    .sort((a, b) => order.indexOf(a.benchmark) - order.indexOf(b.benchmark)
      || modes.indexOf(a.mode) - modes.indexOf(b.mode));
}

/// One cell's build, as the cards it wears. The same shape the simulator's
/// read-only summary uses, so a build reads the same wherever it is quoted.
const wbenchGear = (row) => [
  ...(row.mods || []).map((m) => ({ id: m, kind: "mod" })),
  ...(row.exilus ? [{ id: row.exilus, kind: "exilus" }] : []),
  ...(row.arcanes || []).map((a) => ({ id: a, kind: "arcane" })),
];

/// THE PAGE'S OWN ANSWER, kept true. The prerender wrote this block for ONE
/// url; the moment the reader routes to another weapon without a page load it
/// is describing something that is no longer on screen, so it is rewritten
/// from the rows this client already holds. `data-url` is how the two tell
/// each other apart — while it still names the open page, the server's copy
/// stands, which keeps a first load byte-identical to what a crawler indexed.
function renderWeaponDoc() {
  const box = $("doc");
  const w = weaponInfo($("weapon").value);
  if (!box || !w || !META) return;
  const here = location.pathname.replace(/\/[a-z]+\/?$/, (m) =>
    /^\/(simulator|optimizer|rivens|enemies|benchmark)\/?$/.test(m) ? "" : m);
  if (box.dataset.url && box.dataset.url === here) { box.hidden = false; return; }
  const cells = wbenchCells(w.id);
  box.hidden = !cells.length;
  if (!cells.length) { box.innerHTML = ""; return; }
  const split = (id) => {
    const b = (META.benchmarks || []).find((x) => x.id === id);
    const parts = b ? tr(b.name).split(" · ") : [id];
    return [parts[0], parts.slice(1).join(" · ")];
  };
  // ONE LINE PER RULER, not per cell: a weapon played seven ways has seven
  // rows under one ruler and they are the TAB's subject, not the summary's.
  const lead = new Map();
  for (const c of cells) {
    const had = lead.get(c.benchmark);
    if (!had || c.best.score > had.best.score) lead.set(c.benchmark, c);
  }
  // THE SAME SHAPE THE PRERENDER WROTE, because this replaces it in place: a
  // block that turned from a table into a list of sentences the first time the
  // reader changed weapon would be two designs for one thing.
  const rows = [...lead.values()].map((c) => {
    const [ruler, fight] = split(c.benchmark);
    const gear = wbenchGear(c.best).map((g) => {
      const m = g.kind === "arcane" ? arcaneById(g.id) : modById(g.id);
      return m ? m.name : g.id;
    }).join(", ");
    return `<tr><td>${escHtml(ruler)}</td><td>${escHtml(fight)}</td>`
      + `<td>${escHtml(modeLabel(w, c.mode))}</td>`
      + `<td class="w-num">${escHtml(String(
        c.best.shown != null ? c.best.shown : c.best.score.toFixed(4)))}</td>`
      + `<td>${escHtml(gear)}</td></tr>`;
  }).join("");
  const gaps = gapsOf(w).map(trGap).filter(Boolean);
  // THE SAME SHAPE THE PRERENDER WROTE, down to the `shut` and the header: the
  // two write one block, and a reader who changes weapon must not find the
  // default flipped under them. `wireFolds` lets a stored answer outrank it,
  // so somebody who opened it keeps it open on every weapon after.
  const hint = tr("the board's best riven-free build for each ruler")
    + (gaps.length ? tr(", and what this weapon's number does not account for") : "");
  box.innerHTML = `<div class="fold sect w-brief shut" data-fold="w-brief">`
    + `<div class="fold-h"><b>${escHtml(tr("Measured builds"))}</b>`
    + `<span class="sim-hint">${escHtml(hint)}</span></div>`
    + `<div class="fold-b">`
    + `<table class="w-tab w-answers"><caption>${escHtml(tr("The best riven-free build"))}`
    + `</caption><thead><tr><th>${escHtml(tr("Ruler"))}</th><th>${escHtml(tr("Fight"))}</th>`
    + `<th>${escHtml(tr("Mode"))}</th><th>${escHtml(tr("Score"))}</th>`
    + `<th>${escHtml(tr("Build"))}</th></tr></thead><tbody>${rows}</tbody></table>`
    + (gaps.length
      ? `<div class="w-notes"><b>${escHtml(tr("Not modelled here"))}</b><ul>`
        + gaps.map((g) => `<li>${escHtml(g)}</li>`).join("") + `</ul></div>`
      : "")
    + `</div></div>`;
  wireFolds(box);
}

function renderWeaponBench() {
  const box = $("wbench");
  const w = weaponInfo($("weapon").value);
  if (!box || !w || !META) return;
  // THE RANK NEEDS EVERY WEAPON'S BEST ROW, which is the index and not this
  // weapon's file. Asked here rather than at boot — a reader who never opens
  // this tab should not pay for it — and the table redraws when it lands.
  if (!BOARD_INDEX) {
    loadFullBoard().then(() => {
      if ($("weapon") && $("weapon").value === w.id
        && document.body.classList.contains("on-wbench")) renderWeaponBench();
    });
  }
  const cells = wbenchCells(w.id);
  const sub = $("wbench-sub");
  if (sub) sub.textContent = cells.length ? "" : tr("not measured yet");
  if (!cells.length) {
    // NOT MEASURED IS NOT BROKEN, and the page has to say which it is: a
    // weapon nobody has submitted a build for is an invitation, not a fault.
    box.innerHTML = `<p class="wb-empty">${escHtml(tr(
      "No build has been measured for this weapon yet. Submitting one puts it in the queue."))}</p>`;
    return;
  }
  const pick = cells.find((c) => `${c.benchmark}#${c.mode}` === wbenchPick) || cells[0];
  const nameOf = (id) => {
    const b = (META.benchmarks || []).find((x) => x.id === id);
    return b ? tr(b.name).split(" · ")[0] : id;
  };
  const rows = cells.map((c) => {
    const r = wbenchRank(c.benchmark, c.mode, c.best.score, w.exalted);
    const on = c === pick ? " sel" : "";
    return `<tr class="wb-row${on}" data-cell="${escHtml(c.benchmark + "#" + c.mode)}">`
      + `<td>${escHtml(nameOf(c.benchmark))}</td>`
      + `<td class="wb-mode">${escHtml(modeLabel(w, c.mode))}</td>`
      + `<td class="wb-score">${escHtml(String(c.best.shown != null ? c.best.shown : c.best.score.toFixed(4)))}</td>`
      + `<td class="wb-rank">${r ? `${r.rank} / ${r.total}` : "—"}</td></tr>`;
  }).join("");
  // THE SAME LOOKUPS EVERY OTHER SURFACE USES. A card named here and named
  // differently in the build finder would be two names for one id.
  const gear = wbenchGear(pick.best).map((g) => {
    const m = g.kind === "arcane" ? arcaneById(g.id) : modById(g.id);
    return `<span class="wb-card">${escHtml(m ? m.name : g.id)}</span>`;
  }).join("");
  const evos = evoChipsOf(pick.best.evolutions || [])
    .map((c) => `<span class="wb-card wb-evo">${escHtml(c.label)}</span>`).join("");
  // THE RIVEN CEILING IS A SEPARATE LINE, never the headline: a riven is a
  // roll nobody else has, so the build a reader can actually copy leads.
  const ceil = pick.riven
    ? `<p class="wb-ceiling">${escHtml(tr("With a riven"))}: <b>${
      escHtml(String(pick.riven.shown != null ? pick.riven.shown : pick.riven.score.toFixed(4)))}</b></p>`
    : "";
  box.innerHTML = `<table class="wb-tab"><thead><tr>`
    + `<th>${escHtml(tr("Ruler"))}</th><th>${escHtml(tr("Mode"))}</th>`
    + `<th>${escHtml(tr("Score"))}</th><th>${escHtml(tr("Board rank"))}</th>`
    + `</tr></thead><tbody>${rows}</tbody></table>`
    + `<div class="wb-detail"><p class="wb-terms">${escHtml(tr(
      ((META.benchmarks || []).find((x) => x.id === pick.benchmark) || {}).name || pick.benchmark))}</p>`
    + `<div class="wb-cards">${gear}${evos}</div>${ceil}</div>`;
  box.querySelectorAll(".wb-row").forEach((tr_) => {
    tr_.addEventListener("click", () => {
      wbenchPick = tr_.dataset.cell;
      renderWeaponBench();
    });
  });
}
