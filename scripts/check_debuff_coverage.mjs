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
  sim.duration = 60; sim.runs = 30; sim.level = 30; sim.steel_path = false; sim.eximus = false;
  // The result is captured off the wire rather than out of a global: the page
  // stores it inside the active scenario preset, which is a longer reach than
  // this check needs.
  let shot = null;
  const real = window.api;
  window.api = async (p, b) => { const res = await real(p, b); if (p === '/api/simulate') shot = res; return res; };
  await runSim();
  window.api = real;
  for (let i = 0; i < 60 && !shot; i++) await sleep(500);
  const rp = shot && shot.replay;
  out.hasReplay = !!(rp && rp.t && rp.t.length > 1);
  out.rosterLen = rp ? (rp.debuffs || []).length : 0;
  out.seriesLen = rp ? (rp.dstacks || []).length : 0;
  out.sameFrames = rp ? (rp.dstacks || []).every((s) => s.length === rp.t.length) : false;
  // The SHAPE is the buff table's: (id, max) pairs indexed by the series.
  out.rosterShape = rp && rp.debuffs[0] ? Object.keys(rp.debuffs[0]).sort().join(",") : "";
  // The buff roster of an unmodded build is EMPTY, so the shape is compared
  // against the literal the two share rather than against a row that may not
  // exist.
  out.buffShape = rp && rp.buffs[0] ? Object.keys(rp.buffs[0]).sort().join(",") : "id,max";

  // POISON is the one this weapon guarantees, and it must actually move.
  const iPoison = rp ? rp.debuffs.findIndex((d) => d.id === 'poison') : -1;
  const poison = iPoison >= 0 ? rp.dstacks[iPoison] : [];
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
    .filter((d, i) => (rp.dstacks[i] || []).some((v) => v > 0)).length === rows.length;

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
  return out;
})()`);

check("the run produced a replay", r.hasReplay === true);
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

await app.finish("the debuff table is the buff table, read from the other side");
