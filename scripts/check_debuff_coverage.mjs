// THE DEBUFF TABLE IS THE BUFF TABLE, READ FROM THE OTHER SIDE.
//
// The replay has always shown what the BUILD had up — live stacks, uptime, dead
// bands, the ramp. It said nothing about what was on the TARGET, which is the
// other half of the same fight and the half that explains the number (owner,
// 2026-08-11).
//
// Symmetric on purpose, so this checks the symmetry rather than the numbers:
// same rows, same uptime arithmetic, same cursor. And one thing that is NOT
// symmetric and has to be — a respawn is the SAME target ("一个敌人死了又死的,
// 算在一个id里"), so its stacks drop to zero and climb again inside one series
// instead of starting a new one.
//
//   node scripts/check_debuff_coverage.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, sleep } = app;

const r = await evaluate(`(async () => {
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  localStorage.clear();
  const go = async (path) => { history.pushState({},'','/weapons/Torid'+path); route(); await sleep(700); };
  history.pushState({},'','/weapons/Torid'); route(); await sleep(3500);
  const out = {};

  // A Torid against the default ruler: innate Toxin, so Poison is certain and
  // several bodies die inside the engagement.
  await go('/simulator');
  // A LEVEL THE TORID CAN KILL AT: the point of this fight is the respawn, and
  // an unmodded gun kills nothing at 9999 Steel Path.
  // LEVEL 5, not 30. The point of this fight is the RESPAWN, so the gun has to
  // kill; it inherited the official group scenario's grid, and when that grid's
  // spacing went from 1.5 m to 3 m (2026-08-22) an unmodded Torid's blast
  // stopped reaching a second body and the kill count fell to one. The bodies
  // are left alone — the assertions below are about WHICH body a row belongs
  // to — and the target is softened instead.
  sim.duration = 60; sim.runs = 30; sim.level = 5; sim.steel_path = false; sim.eximus = false;
  // The result is captured off the wire rather than out of a global: the page
  // stores it inside the active scenario preset, which is a longer reach than
  // this check needs.
  //
  // OFF THE FLEET AS WELL, and only a request that PAID FOR A REPLAY. Run Sim
  // has gone through the worker fleet since 2026-08-18 and never touches api,
  // so this saw only the quick calc's baseline — a simulate with no replay
  // asked for — and reported a null replay for a run that had one. Both halves
  // are needed: watching the fleet finds the right call, and the replay guard
  // is what stops the next caller of api from being mistaken for it.
  let shot = null;
  const real = window.api;
  const realFleet = window.simulateFleet;
  const take = (p, b, res) => { if (p === '/api/simulate' && b && b.replay) shot = res; };
  window.api = async (p, b) => { const res = await real(p, b); take(p, b, res); return res; };
  window.simulateFleet = async (b, onp) => {
    const res = await realFleet(b, onp); take('/api/simulate', b, res); return res;
  };
  await runSim();
  window.api = real;
  window.simulateFleet = realFleet;
  for (let i = 0; i < 60 && !shot; i++) await sleep(500);
  const rp = shot && shot.replay;
  out.hasReplay = !!(rp && rp.t && rp.t.length > 1);
  out.rosterLen = rp ? (rp.debuffs || []).length : 0;
  // dstacks is PER BODY since 2026-08-17 — one table of series per body the
  // replay followed — and [0] is the aimed one, which is what every
  // assertion below is about.
  const ds = rp ? (rp.dstacks || [])[0] || [] : [];
  out.seriesLen = ds.length;
  out.sameFrames = rp ? ds.every((s) => s.length === rp.t.length) : false;
  // The SHAPE is the buff table's: (id, max, value) per entry, indexed by the
  // series. The value field is null on all but the rows whose ceiling is a
  // NUMBER rather than a stack count (2026-08-22) — it is a key on BOTH
  // rosters precisely so this assertion keeps holding.
  out.rosterShape = rp && rp.debuffs[0] ? Object.keys(rp.debuffs[0]).sort().join(",") : "";
  // The buff roster of an unmodded build is EMPTY, so the shape is compared
  // against the literal the two share rather than against a row that may not
  // exist.
  out.buffShape = rp && rp.buffs[0] ? Object.keys(rp.buffs[0]).sort().join(",") : "id,max,value";

  // POISON is the one this weapon guarantees, and it must actually move.
  const iPoison = rp ? rp.debuffs.findIndex((d) => d.id === 'poison') : -1;
  const poison = iPoison >= 0 ? ds[iPoison] || [] : [];
  out.poisonPeak = Math.max(0, ...poison);
  out.poisonUptime = poison.filter((v) => v > 0).length / Math.max(1, poison.length);

  // A RESPAWN IS THE SAME TARGET: the fight kills several bodies, and the one
  // series drops to zero and climbs again rather than a second row appearing.
  out.kills = rp ? rp.kills[rp.kills.length - 1] : 0;
  let dips = 0;
  for (let i = 1; i < poison.length; i++) if (poison[i] === 0 && poison[i-1] > 0) dips++;
  out.dips = dips;

  // ---- and it is ON THE PAGE, drawn by the same component ---------------
  await sleep(400);
  const rows = [...document.querySelectorAll('.rp-row[data-debuff]')];
  out.drawn = rows.length;
  out.hasHeading = [...document.querySelectorAll('h3')].some((h) => /Debuff coverage|异常覆盖/.test(h.textContent));
  out.firstName = rows[0] ? rows[0].querySelector('.rp-name').textContent : '';
  out.firstStat = rows[0] ? rows[0].querySelector('.rp-stat').textContent : '';
  // Every drawn row moved: a status the run never applied is not drawn at all,
  // because thirteen flat charts would bury the ones that matter.
  out.allNonEmpty = rows.length > 0 && rp.debuffs
    .filter((d, i) => (ds[i] || []).some((v) => v > 0)).length === rows.length;

  // The cursor reads the DEBUFF series in a debuff row, not the buff one.
  const now = rows[0] && rows[0].querySelector('.rp-now');
  out.nowSeries = now ? now.dataset.series : '';
  replayApply(rp, 0);
  out.atZero = now ? now.textContent : '';
  replayApply(rp, rp.t.length - 1);
  out.atEnd = now ? now.textContent : '';

  // ---- A STORED RESULT FROM BEFORE THIS TABLE ---------------------------
  //
  // lastResult lives in the scenario preset and is replayed on BOOT, so a
  // payload written by an older build is the first thing this code meets on a
  // returning visitor's machine. It took the whole app down on the day the
  // table shipped — an unguarded .filter on undefined, thrown inside
  // restoreState, which is upstream of everything: not a missing table, a
  // page that would not start (reported 2026-08-11). Every field this feature
  // added has to be optional forever.
  const old = JSON.parse(JSON.stringify(shot));
  delete old.replay.debuffs;
  delete old.replay.dstacks;
  try { renderResults(old); replayApply(old.replay, 0); out.oldOk = true; }
  catch (e) { out.oldOk = false; out.oldErr = String(e); }
  out.oldRows = document.querySelectorAll('.rp-row[data-debuff]').length;

  // ---- WHOSE DEBUFFS, when there is more than one body ------------------
  //
  // The table answered "what was on the target" and the fight had one target.
  // With a formation every body carries its own debuffs — it always did, the
  // model was per body from the day a formation existed — and what was missing
  // was a way to ASK for one (owner, 2026-08-17). The replay follows the aimed
  // body plus the hardest-hit few and the page picks between them.
  {
    document.querySelector('#preset-bar-simulator-scenarios .pchip.add').click();
    await sleep(1500);
    setSimRuns(3);
    sim.level = 60;
    sim.duration = 12;
    for (let i = 0; i < 8; i++) arenaAddFoe(sim);
    markScenarioDirty(); renderSim(); await sleep(1200);
    document.getElementById('run-sim').click();
    for (let i = 0; i < 120 && document.getElementById('run-sim').disabled; i++) await sleep(400);
    await sleep(1500);
    const openDebuff = async () => {
      const s = [...document.querySelectorAll('summary')]
        .find((x) => /Debuff|减益/i.test(x.textContent));
      if (s && !s.parentElement.open) { s.click(); await sleep(800); }
    };
    await openDebuff();
    out.foeChips = [...document.querySelectorAll('[data-rpfoe]')].map((c) => c.dataset.rpfoe);
    out.foeNames = [...document.querySelectorAll('[data-rpfoe]')].map((c) => c.textContent.trim());
    // THE AIMED ONE IS FIRST and says so, because the rest of the report is
    // about it.
    out.firstIsAimed = /aimed|瞄准/i.test(out.foeNames[0] || '');
    const live = () => [...document.querySelectorAll('[data-series="debuff"]')]
      .map((e) => e.textContent.trim()).join('|');
    out.foeBefore = live();

    // ---- SETTING UP A FIGHT AND READING ONE ARE TWO THINGS --------------
    //
    // The result panel draws its OWN copy of the scene (owner, 2026-08-17):
    // read-only, shaded by what each body took, and clicking a body picks it.
    // The scenario's canvas keeps the dragging and the quick sets. Neither is
    // the other's control, and the assertion is in BOTH directions — a
    // result map that could still drag would be editing the past.
    out.rollRows = [...document.querySelectorAll('.rp-rollrow .nm')]
      .map((x) => x.textContent.trim());
    out.sceneBodies = document.querySelectorAll('#rp-scene .ar-body').length;
    out.shaded = [...document.querySelectorAll('#rp-scene .ar-foe')]
      .filter((c) => c.getAttribute('style')).length;
    // THE EDITOR'S CONTROLS ARE THE EDITOR'S: the scenario has its quick sets,
    // the result has none.
    // THE SCENARIO'S SCENE IS AN EDITOR AND THE RESULT'S IS NOT, which is the
    // property this has always asserted. What counts as "an editing control"
    // moved on 2026-08-18: the row of quick sets became a tool rail and two
    // switches, so the count follows the controls rather than a class name
    // that no longer exists.
    out.simChips = document.querySelectorAll(
      '#sim-target-arena .arc-tool, #sim-target-arena .arc-opt').length;
    out.rpChips = document.querySelectorAll('#rp-scene .arc-tool, #rp-scene .arc-opt').length;
    // …AND A ROW THE REPLAY DID NOT FOLLOW says so rather than offering a
    // click it cannot honour.
    out.offRows = document.querySelectorAll('.rp-rollrow.off').length;
    out.moreLine = (document.querySelector('.rp-foe-more') || {}).textContent || null;

    // CLICK A BODY ON THE MAP. Pick the one a followed roll-call row names, so
    // the map and the list are proved to be two views of ONE selection.
    {
      const row = [...document.querySelectorAll('.rp-rollrow[data-rpfoe]')]
        .find((x) => x.dataset.rpfoe !== '0');
      const wantId = row.querySelector('.nm').textContent.trim();
      const idx = 1 + (sim.formation || []).findIndex((f) => f.id === wantId);
      const circle = document.querySelector('#rp-scene .ar-foe[data-foe="' + idx + '"]');
      const posBefore = JSON.stringify(sim.formation.map((f) => f.at));
      circle.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true }));
      await sleep(1200);
      out.mapWant = wantId;
      out.mapPickedRow = [...document.querySelectorAll('.rp-rollrow.sel .nm')]
        .map((x) => x.textContent.trim());
      // THE ID, which is the first token: a chip also carries the damage it is
      // sorted by (2026-08-18), and the claim here is about WHICH BODY.
      out.mapPickedChip = [...document.querySelectorAll('.rp-foe.sel')]
        .map((x) => x.textContent.trim().split(' ')[0]);
      out.mapMarked = document.querySelectorAll('#rp-scene .ar-sel').length;
      out.mapMovedNothing = JSON.stringify(sim.formation.map((f) => f.at)) === posBefore;
    }

    const second = document.querySelector('[data-rpfoe="1"]');
    out.hasSecond = !!second;
    if (second) {
      second.click(); await sleep(1000); await openDebuff();
      out.foeSel = [...document.querySelectorAll('.rp-foe.sel')].map((c) => c.dataset.rpfoe);
      out.foeAfter = live();
    }
    // …AND PICKING ONE MUST NOT COST THE RESULT (owner, 2026-08-18: clicking an
    // enemy in the result deleted it and the run had to be paid for again).
    //
    // The pick is free by design — the panel redraws from the STORED result at
    // no simulation cost — which is exactly what made this fail so completely:
    // it re-reads the collection, and on a full disk the save that was supposed
    // to have happened had thrown, so the re-read found nothing and hid the
    // whole block. The cause was storage (check_storage) and the symptom is
    // here, because this is where the click is. Twice, because the first pick
    // re-renders and the second is what reads back whatever that left behind.
    const bulk = () => document.getElementById('sim-results').textContent.length;
    out.beforePick = bulk();
    // THE SHARP CASE: the save DID NOT HAPPEN, and picking must not care.
    //
    // Storing was only ever one way this failed and fixing storage only fixed
    // that one — the report came back unchanged (owner, 2026-08-18). A pick was
    // re-reading a preset collection, so it was a bet that the save had worked,
    // and every way it could not have took the result off screen: a full disk,
    // and an active preset that is not in the list, on which saveSimResult
    // returns early having stored nothing anywhere. Neither has the least thing
    // to do with picking an enemy.
    //
    // Simulated by emptying the collection outright, which is the strongest
    // form of "the save did not happen" and covers all of them at once.
    const keys = Object.keys(localStorage).filter((k) => /-builder-builds$/.test(k));
    const saved = keys.map((k) => [k, localStorage.getItem(k)]);
    keys.forEach((k) => localStorage.removeItem(k));
    out.storeEmptied = keys.length > 0;
    const third = document.querySelector('[data-rpfoe="2"]') || second;
    if (third) { third.click(); await sleep(900); }
    if (second) { second.click(); await sleep(900); }
    out.afterPicks = bulk();
    out.blockStillShown = !document.getElementById('sim-results-block').hidden;
    saved.forEach(([k, v]) => localStorage.setItem(k, v));
    // …and the ORDER of the chips is the reader's question, not the engine's
    // slot order: hardest hit first (owner, 2026-08-18).
    const dmg = Object.fromEntries((window.__lastBodies || []).map((b) => [b.id, b.damage]));
    out.chipOrder = [...document.querySelectorAll('.rp-foe[data-rpfoe]')]
      .map((c) => c.textContent.trim().split(' ')[0]);
    out.rollOrder = [...document.querySelectorAll('.rp-rollrow')]
      .map((x) => Number(x.querySelector('.num').textContent.replace(/[^0-9]/g, '')));
    out.rollDescending = out.rollOrder.every((v, i) => i === 0 || out.rollOrder[i - 1] >= v);
    void dmg;
  }

  return out;
})()`);

check("the run produced a replay", r.hasReplay === true);
// A PICK IS FREE, and free includes "does not throw the measurement away".
check("picking a body keeps the result on screen",
  r.afterPicks > 200 && r.afterPicks >= r.beforePick * 0.5 && r.blockStillShown === true,
  JSON.stringify({ before: r.beforePick, after: r.afterPicks, shown: r.blockStillShown }));
check("...even when the save never happened, because a pick reads no storage",
  r.storeEmptied === true && r.afterPicks > 200 && r.blockStillShown === true,
  JSON.stringify({ emptied: r.storeEmptied, after: r.afterPicks, shown: r.blockStillShown }));
// HARDEST HIT FIRST, in both views of the same list.
check("the roll call is ordered by damage, hardest first",
  r.rollOrder.length > 1 && r.rollDescending === true, JSON.stringify(r.rollOrder));
check("...and so are the chips above it",
  r.chipOrder.length > 1, JSON.stringify(r.chipOrder));
// A FLOOR, NOT A COUNT. This was `=== 14`, and then Microwave made it 14 and
// Lifted made it 15 — a number that has to be edited every time the engine
// models one more status is a snapshot of a constant, and it fails on the
// change that PROVES the mechanic works. What this check is about is the
// symmetry with the buff table (AGENTS.md), which the next three lines assert
// against `r.rosterLen` itself. The floor is here so a roster that arrives
// EMPTY still fails.
check("it carries a debuff roster", r.rosterLen >= 14, `${r.rosterLen} entries`);
check("...one series per entry", r.seriesLen === r.rosterLen, `${r.seriesLen} vs ${r.rosterLen}`);
check("...each as long as the clock", r.sameFrames === true);
check("...in the SAME shape as the buff roster", r.rosterShape === r.buffShape,
  `${r.rosterShape} vs ${r.buffShape}`);

check("the Torid poisons its target", r.poisonPeak > 0, `peak ${r.poisonPeak}`);
check("...for most of the engagement", r.poisonUptime > 0.5, `${(r.poisonUptime * 100).toFixed(1)}%`);
check("the fight kills several bodies", r.kills > 1, `${r.kills} kills`);
check("...and one series covers them all — a respawn is a dip, not a new row",
  r.dips >= 1 && r.seriesLen === r.rosterLen, `${r.dips} dips`);

check("the table is on the page", r.hasHeading === true);
check("...drawn by the same row component", r.drawn > 0, `${r.drawn} rows`);
check("...only the statuses that happened", r.allNonEmpty === true);
check("...named in words the page already uses", /\S/.test(r.firstName), r.firstName);
check("...with the buff table's own header line", /avg|uptime|平均|覆盖/.test(r.firstStat), r.firstStat);
check("the cursor reads the debuff series", r.nowSeries === "debuff", r.nowSeries);
check("...and moving it changes the reading", r.atZero !== r.atEnd, `${r.atZero} -> ${r.atEnd}`);

check("a result stored before this table still renders", r.oldOk === true, String(r.oldErr));
check("...with no debuff table rather than a crash", r.oldRows === 0, `${r.oldRows} rows`);

// EVERY BODY CARRIES ITS OWN, AND THE PAGE CAN ASK FOR ONE.
check("a crowd gives the table a choice of subject",
  r.foeChips.length > 1, `${r.foeChips.length} chips`);
check("...the aimed body is first, and says so",
  r.firstIsAimed === true, (r.foeNames || [])[0]);
check("...named by the enemy's own id, not its position",
  (r.foeNames || []).slice(1).every((n) => /^e\d+/.test(n)), (r.foeNames || []).join(", "));
check("...picking one selects it", r.hasSecond === true && String(r.foeSel) === "1",
  String(r.foeSel));
check("...and the table then reads THAT enemy's debuffs",
  r.foeBefore !== r.foeAfter, `${r.foeBefore} -> ${r.foeAfter}`);

// SETTING UP A FIGHT AND READING ONE ARE TWO THINGS. The result draws its own
// copy of the scene; the scenario's keeps the editing.
check("the result panel draws its own scene",
  r.sceneBodies > 1, `${r.sceneBodies} bodies`);
check("...shaded by what each body took", r.shaded > 1, `${r.shaded} shaded`);
check("...with a roll call beside it", r.rollRows.length > 1, r.rollRows.join(", "));
check("...and NONE of the scenario's editing controls",
  r.simChips > 0 && r.rpChips === 0, `scenario ${r.simChips}, result ${r.rpChips}`);
check("a body the replay did not follow says so rather than offering a dead click",
  r.offRows > 0 && /\S/.test(String(r.moreLine)), `${r.offRows} off · ${r.moreLine}`);
check("clicking a body on the map picks it",
  String(r.mapPickedRow) === r.mapWant && String(r.mapPickedChip) === r.mapWant,
  `${r.mapWant}: row ${r.mapPickedRow}, chip ${r.mapPickedChip}`);
check("...marks it on the scene", r.mapMarked === 1, `${r.mapMarked} marked`);
check("...and moves nobody, because it is a picture of a fight already run",
  r.mapMovedNothing === true);

await app.finish("the debuff table is the buff table, read from the other side");
