// A BUILD THE BOARD ALREADY HOLDS IS NOT SENT TO IT AGAIN.
//
// Answering "is this already a row?" with a POINTER fails:
// `officialBuildActive()` says whether the ACTIVE PRESET is a builtin, which is
// true of a board row opened from the picker and false of the same build
// reached any other way. So a player who copies a board build into a preset of
// their own — the ⧉ the picker offers on every benchmark row — was told their
// run was being uploaded to a board that already holds it.
//
// THE ANSWER IS THE ENGINE'S, and that is the whole point of the endpoint this
// asserts. A build is not its spelling: `builds::canonical_mods` sorts the
// non-elementals by drain and leaves the elementals in the order that PAIRS
// them, evolutions are a set, a riven is a shape and not its rolls — and the
// mod POOL is what tells an elemental mod from any other, which only the engine
// has. `/api/build/keys` is `builds::board_key` itself, the same key the scorer
// files rows under, so the two sides cannot be keyed by two different answers.
//
// THE NEGATIVE CONTROL IS THE HALF THAT MATTERS. "Nothing is uploaded" passes
// perfectly on a page that has stopped uploading altogether, which would be a
// far worse bug than the one this fixes — so a build the board does NOT hold
// must still be offered.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 20000 });
const { evaluate, check, finish } = app;
const tag = "board dedup";

await app.load("/weapons/Acceltra_Prime", 22000);

// ---------------------------------------------------------------------------
// 1. THE KEY, asked of the engine about a row and about the same row respelled.
const keys = await evaluate(`(async () => {
  const w = $("weapon").value;
  const r0 = (BOARD[w] || [])[0];
  if (!r0) return { none: true, w };
  const asBody = (mods) => ({
    weapon: w, mods, evolutions: r0.evolutions || [], arcanes: r0.arcanes || [],
    valence: r0.valence || "", exilus: r0.exilus || undefined, mode: r0.mode || "base",
    riven_pos: (r0.riven || {}).bonuses || [], riven_neg: (r0.riven || {}).malus || "",
  });
  const exact = await boardRowMatching(asBody(r0.mods.slice()));
  // A RESPELLING OF THE SAME BUILD. The page cannot tell an elemental mod from
  // any other — META.mods carries no element, deliberately, because that is
  // the engine's question — so this tries every adjacent swap and counts how
  // many the ENGINE still calls the same row. The non-elemental ones do
  // (canonical_mods sorts them by drain); moving an elemental one changes
  // what it pairs with and is a different fight, which the engine's own
  // the_order_of_the_mods_is_part_of_the_identity pins from the other side.
  //
  // WHAT IT PROVES is that the page is not comparing mod lists: a raw
  // comparison would call every one of these a new build.
  let respellings = 0;
  for (let i = 0; i + 1 < r0.mods.length; i++) {
    const sw = r0.mods.slice();
    const t = sw[i]; sw[i] = sw[i + 1]; sw[i + 1] = t;
    const hit = await boardRowMatching(asBody(sw));
    if (hit && hit.name === (exact && exact.name)) respellings += 1;
  }
  // …AND A BUILD THAT IS GENUINELY DIFFERENT.
  const fewer = await boardRowMatching(asBody(r0.mods.slice(0, 4)));
  return {
    w, respellings, of: r0.mods.length - 1,
    exact: exact && exact.name, fewer: fewer && fewer.name,
  };
})()`);

check(`${tag} a board row's own build is recognised as one`,
  !!keys.exact, JSON.stringify(keys));
check(`${tag} ...and moving a mod between two slots is the same build`,
  keys.respellings > 0, `${keys.respellings} of ${keys.of} adjacent swaps still match`);
check(`${tag} ...while a build the board does not hold is not`,
  keys.fewer == null, String(keys.fewer));

// ---------------------------------------------------------------------------
// 2. THE WHOLE PATH: a board row COPIED into a preset of your own, run, and not
//    uploaded — which is the report, exactly.
const copied = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const cfg = buildBarCfg();
  const row = buildList().find((p) => p.builtin);
  if (!row) return { none: true };
  cfg.setActive(row.builtin);
  cfg.apply(row.state);
  // …and COPY it, which is what the picker's ⧉ does: the same build under a
  // name of your own, so nothing on the page says any more where it came from.
  const list = loadPresetList(BUILDS);
  list.push({ name: "copy 1", savedAt: Date.now(), state: cfg.snapshot() });
  storePresetList(BUILDS, list);
  cfg.setActive("copy 1");
  cfg.rerender();
  await sleep(600);
  const before = officialBuildActive();
  document.querySelectorAll('.tab').forEach((x) => { if (/Sim/i.test(x.textContent)) x.click(); });
  document.getElementById("run-sim").click();
  for (let i = 0; i < 240 && !boardState; i++) await sleep(500);
  return {
    pointerSaysNothing: before === false,
    state: boardState,
    row: boardOnBoard && boardOnBoard.name,
    text: (document.querySelector("#sim-board-outcome .bo-h") || {}).textContent || "",
  };
})()`);

check(`${tag} a copied board build is not offered to the board again`,
  copied.pointerSaysNothing === true && copied.state === "onboard",
  JSON.stringify(copied));
check(`${tag} ...and the page NAMES the row it already is`,
  !!copied.row && (copied.text || "").includes(copied.row), JSON.stringify(copied));

// ---------------------------------------------------------------------------
// 3. THE NEGATIVE CONTROL. Take one mod out and the same run is offered again —
//    without this, a page that had simply stopped submitting would pass.
const changed = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  document.querySelectorAll('.tab').forEach((x) => { if (/Build/i.test(x.textContent)) x.click(); });
  await sleep(400);
  // A DAMAGE MOD SWAPPED FOR ANOTHER, so the build is still complete — the
  // board refuses a half-built one, which would be a different branch and not
  // the one under test.
  const have = new Set(slots.map((s) => s.mod).filter(Boolean));
  const spare = (META.mods || []).find((m) => !have.has(m.id) && !m.element && m.pool !== "exilus");
  if (!spare) return { none: true };
  slots[0] = { mod: spare.id, pol: slots[0].pol, rank: null };
  renderAll();
  await sleep(600);
  boardState = "";
  document.querySelectorAll('.tab').forEach((x) => { if (/Sim/i.test(x.textContent)) x.click(); });
  document.getElementById("run-sim").click();
  for (let i = 0; i < 240 && !boardState; i++) await sleep(500);
  return { state: boardState, swappedIn: spare.id };
})()`);

check(`${tag} ...but a build the board does NOT hold is still offered`,
  changed.none === true || (changed.state && changed.state !== "onboard"),
  JSON.stringify(changed));

await finish("a build already on the board is not sent to it again");
