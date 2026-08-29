// A TARGET YOU MADE IS A TARGET LIKE ANY OTHER.
//
// The second CUSTOM, and AGENTS.md named it before it
// existed: "custom enemies will become entries in the scenario's enemy list".
// That sentence is the whole test — if it is true, the simulator, the optimizer
// and the target card need no code of their own for it, because they all read
// the scenario's target list and the server reads one `EnemySpec` whether a
// wiki published it or a player typed it.
//
// What can go wrong here is specific, so this checks the specific things:
//   - a custom that never reaches the SERVER, so the fight silently runs against
//     the default unit and the number is somebody else's
//   - an editor that saves a shape the server rejects
//   - a vulnerability column that is shown and not applied (immunity is the
//     sharp case: 0 must mean nothing gets through)
//   - a delete that leaves the fight pointing at a target nobody answers to
//
//   node scripts/check_custom_enemies.mjs
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

  // ---- the EDITOR ------------------------------------------------------
  await go('/enemies');
  const vis = (id) => { const e=document.getElementById(id); return !!e && e.offsetParent !== null; };
  out.tabShows = vis('enemy-block');
  out.emptyAtFirst = loadPresetList('enemies').length === 0;
  document.querySelector('#enemy-tools .cu-new').click(); await sleep(300);
  out.created = loadPresetList('enemies').map(p => p.name);
  out.formShown = !!document.querySelector('#enemy-form [data-en-k="stats.health"]');

  // Type a target: 5000 HP, no armour, a x4 head, and IMMUNE TO HEAT.
  const set = async (sel, v) => {
    const el = document.querySelector(sel);
    if (el.type === 'checkbox') el.checked = v; else el.value = String(v);
    el.dispatchEvent(new Event('change', {bubbles:true}));
    await sleep(150);
  };
  await set('[data-en-k="stats.health"]', 5000);
  await set('[data-en-k="stats.armor"]', 0);
  await set('[data-en-k="stats.shield"]', 0);
  await set('[data-en-k="stats.base_level"]', 1);
  await set('#en-own-col', true);
  await set('[data-en-dm="heat"]', 0);
  await set('[data-en-dm="void"]', 2);
  document.querySelectorAll('.en-part')[1].querySelector('[data-en-p="multiplier"]').value = '4';
  document.querySelectorAll('.en-part')[1].querySelector('[data-en-p="multiplier"]')
    .dispatchEvent(new Event('change', {bubbles:true}));
  await sleep(200);
  const doc = loadPresetList('enemies')[0].state;
  out.saved = { hp: doc.stats.health, heat: doc.damage_modifiers.heat, void: doc.damage_modifiers.void,
                head: doc.body_parts[1].multiplier };

  // ---- it is a TARGET, in every list that asks --------------------------
  const id = enemyId('target 1');
  out.inList = allEnemies().some(e => e.id === id);
  out.card = (enemyCard(id) || {}).type_modifiers;

  // ---- and it REACHES THE SERVER ---------------------------------------
  await go('/simulator');
  sim.enemy = id; sim.level = 1; sim.steel_path = false; sim.eximus = false;
  sim.runs = 20; sim.duration = 10;
  // Equipped with nothing: the Torid's own Toxin, so the immunity below is
  // about the target rather than about a mod.
  const body = { ...buildPayload(), ...theFight(), buffs: {} };
  out.sentCustom = (body.custom_enemies || []).map(e => e.id);
  const shot = await api('/api/simulate', body);
  out.ok = shot && shot.ok !== false;
  out.targetName = shot && shot.target && shot.target.name;
  out.err = shot && shot.error;

  // THE IMMUNITY IS THE SHARP ONE, and it is checked by MEASURING rather than
  // by reading the card back: a column that is shown and not applied looks
  // exactly like one that works. The Torid deals Toxin and nothing else, so a
  // Toxin-immune target must take nothing at all, and the same target at x1
  // must take something.
  const dmgVs = async (mult) => {
    const ps = loadPresetList('enemies');
    ps[0].state.damage_modifiers = { toxin: mult };
    storePresetList('enemies', ps);
    const res = await api('/api/simulate', { ...buildPayload(), ...theFight(), buffs: {} });
    return (res && res.dps) || 0;
  };
  out.immune = await dmgVs(0);
  out.normal = await dmgVs(1);

  // ---- STATUS IMMUNITY IS A DIFFERENT MECHANIC --------------------------
  //
  // The wiki puts both halves in one paragraph (Status_Effect, Status
  // Immunity Interactions): proc type chances are NOT altered by resistances or
  // weaknesses, but they ARE altered by status immunities, which drop the type
  // out of the draw so the rest renormalize. So on the Torid — Toxin and
  // nothing else — the two look completely different from each other:
  //
  //   toxin damage x0   procs continue at the same rate, dealing nothing
  //   toxin STATUS immune   there is no eligible type left, so procs stop
  //
  // Conflating them is the mistake this pins, and it is measured on the PROC
  // COUNT because that is the only place the difference shows. (The
  // RENORMALISATION itself is an engine test —
  // status_immunities_renormalize_toward_other_procs — where a mixed vector can
  // be held fixed.)
  const procsWhen = async (mut) => {
    const ps = loadPresetList('enemies');
    // UNKILLABLE IN EVERY CASE, so the three runs are the same fight. A target
    // that takes no damage never dies and a target that does dies often, and
    // "how many procs per pellet" is not comparable across two fights that
    // differ in how many times the target was replaced.
    ps[0].state.stats.health = 1e9;
    mut(ps[0].state);
    storePresetList('enemies', ps);
    // THE RUN COUNT IS THIS CALLER'S, and it is the one axis a caller owns
    // (AGENTS: the reader's precision, not an edit to the fight — so it lands
    // LAST in the spread). The scenario the check happens to be on runs at 100,
    // which is 4,500 pellets, and the assertion below wants 30,000: a check that
    // inherits its precision from whatever fight was active is a check whose
    // strength nobody chose.
    const res = await api('/api/simulate',
      { ...buildPayload(), ...theFight(), buffs: {}, runs: 1000 });
    // PER PELLET, not per engagement: a target that takes no damage never
    // dies, so the raw counts are two different fights. The RATE is the thing
    // the wiki is talking about.
    //
    // AND POOLED OVER EVERY RUN, which is the whole sample this assertion has.
    // It read procs/pellets until 2026-08-25, and those are the MEDIAN
    // ENGAGEMENT — 45 pellets, however many runs were paid for. So the
    // measurement's sample size was fixed at 45 and raising the run count added
    // nothing to it: deterministic, never converging, and about two sigma wide
    // against a tolerance of 15%, which is how it sat red on a correct engine.
    // procs_mean and pellets_mean are the counts over all N.
    // THE SAMPLE COMES BACK WITH THE RATE. A tolerance says nothing without
    // the n behind it — that is the whole fault being repaired here — so the
    // assertion below states the sample it actually got rather than trusting
    // that the fight was measured hard enough.
    if (!res) return { rate: 0, n: 0 };
    const pel = res.pellets_mean || 0;
    return { rate: (res.procs_mean || 0) / Math.max(1e-9, pel), n: pel * (res.runs || 1) };
  };
  const plain = await procsWhen((s) => { s.damage_modifiers = null; s.status_immunities = []; });
  const zeroed = await procsWhen((s) => { s.damage_modifiers = { toxin: 0 }; s.status_immunities = []; });
  const immune = await procsWhen((s) => { s.damage_modifiers = null; s.status_immunities = ['toxin']; });
  out.plainProcs = plain.rate; out.noDamageProcs = zeroed.rate; out.immuneProcs = immune.rate;
  out.procSample = Math.min(plain.n, zeroed.n);

  // ---- and DELETING one does not leave the fight pointing at nothing -----
  await go('/enemies');
  document.querySelector('.en-row').click(); await sleep(300);
  document.querySelector('#enemy-tools .cu-del').click(); await sleep(300);
  out.afterDelete = sim.enemy;
  out.listEmpty = loadPresetList('enemies').length === 0;
  return out;
})()`);

check("the Enemies tab draws", r.tabShows === true);
check("...starting empty", r.emptyAtFirst === true);
check("...+ new target makes one", String(r.created) === "target 1", String(r.created));
check("...and opens its editor", r.formShown === true);
check("every field saves", JSON.stringify(r.saved) === JSON.stringify({ hp: 5000, heat: 0, void: 2, head: 4 }),
  JSON.stringify(r.saved));

check("it is in the target list", r.inList === true);
// Its own column, and it STARTED from the faction's rather than from fifteen
// ones — switching the toggle on copies what the target already was, so the
// Grineer entries survive beside the two that were typed.
check("...carrying its own vulnerability column", JSON.stringify(r.card) === JSON.stringify([
  { type: "impact", mult: 1.5 }, { type: "heat", mult: 0 },
  { type: "corrosive", mult: 1.5 }, { type: "void", mult: 2 },
]), JSON.stringify(r.card));

check("the fight carries it to the server", String(r.sentCustom) === "custom:target 1", String(r.sentCustom));
check("...which accepts it", r.ok === true, String(r.err));
check("...and fights the target that was typed", r.targetName === "target 1", String(r.targetName));

check("an immune column lets NOTHING through", r.immune === 0, String(r.immune));
check("...and the same target at x1 takes damage", r.normal > 0, String(r.normal));

// The two mechanics, told apart by measurement rather than by reading the
// card back.
check("procs happen at all", r.plainProcs > 0, String(r.plainProcs));
// THE SAMPLE IS THE WHOLE RUN SET, and for a day it was not. This assertion
// went red at 0.533/pellet -> 0.689 and stayed red identically on every re-run,
// which reads as a systematic effect and was not one: procs and pellets in the
// response are the MEDIAN ENGAGEMENT's counts, so the sample was 45 pellets no
// matter how many runs the fight was paid for. At a rate near 0.6 the binomial
// sd over 45 trials is 3.3 procs and 15% of 24 is 3.6 — a two-sigma draw fails
// this by construction, and no run count could ever have fixed it, because
// raising it picks a different median run rather than adding a trial. It is
// SELECTION-BIASED on top of that: the median run is chosen by damage and more
// procs means more damage, so it climbs with the run count (measured on the
// Torid: 0.5556 at 1 run, 0.6444 at 200, 0.7778 at 1000, while the pooled rate
// held at 0.6244).
//
// It is procs_mean over pellets_mean now — 45,000 trials at the default 1000
// runs — and the answer is EXACT: 0.6244 both ways, 0.000% apart, because the
// damage column is not read by the proc draw at all and the two fights roll the
// same dice. So the tolerance is 3% rather than 15%: loose enough that a
// legitimate feedback path from damage into the fight would not redden it,
// tight enough to catch a PARTIAL conflation, where the old one could only ever
// have caught a total one.
//
// AND THE SAMPLE IS ASSERTED BESIDE IT, because a tolerance without an n is the
// fault this repaired. A fight measured at a handful of runs would pass this
// vacuously and look exactly like one measured properly.
check("a damage x0 does NOT change the proc RATE",
  Math.abs(r.noDamageProcs - r.plainProcs) / r.plainProcs < 0.03,
  `${r.plainProcs.toFixed(4)}/pellet -> ${r.noDamageProcs.toFixed(4)}`);
// THE FLOOR IS DERIVED FROM THE CLAIM ABOVE, not picked. Two binomial rates at
// p about 0.62 differ with sd sqrt(2p(1-p)/n)/p as a fraction of the rate; for
// four sigma of that to sit inside the 3% tolerance takes about 35,000 trials,
// and 1000 runs of this fight is 45,000. It is CONSERVATIVE besides, because the
// two fights are paired on one seed rather than independent — which is why the
// measured difference is 0.000% rather than merely small.
check("...measured over a sample that can carry the claim",
  r.procSample >= 30000, `${Math.round(r.procSample)} pellets`);
check("...a STATUS immunity does", r.immuneProcs === 0, String(r.immuneProcs));

check("deleting it repoints the fight at a real target",
  r.afterDelete && !String(r.afterDelete).startsWith("custom:"), String(r.afterDelete));
check("...and the list is empty again", r.listEmpty === true);

await app.finish("a target you made is a target like any other");
