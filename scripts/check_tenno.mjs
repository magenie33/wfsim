// The TENNO block, on screen: the fields exist, they change the panel, and
// they survive a share link.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, sleep, send, BASE } = app;

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Torid'); route(); await sleep(2500);
  document.querySelectorAll('.tab').forEach(t => { if (/Sim/i.test(t.textContent)) t.click(); });
  await sleep(1200);
  const box = document.getElementById('sim-technique');
  const keys = [...box.querySelectorAll('[data-k]')].map(e => e.dataset.k);

  // Equip Primary Bulwark, then give the Warframe armor.
  arcanes = ['primary_bulwark'];
  markPresetDirty(); renderMods(); refreshPanel(); await sleep(2200);
  // Bulwark is NOT a buff card — its value is a Warframe stat, not a stack
  // anyone earns. It lists as a CONDITIONAL, the panel's channel for "this
  // pays and here is what decides it".
  const conds = () => [...document.querySelectorAll('#stats-conditionals .scond')]
    .map(e => e.textContent).join(' | ');
  const before = conds();

  const armor = box.querySelector('[data-k="wf_armor"]');
  armor.value = '1500'; armor.dispatchEvent(new Event('change'));
  await sleep(2500);
  const after = conds();

  // …AND A FRAME FILLS ALL THREE AT ONCE. Sprint is the one that matters most
  // here: it could not be set at all before this control existed, so every
  // "With Sprint Speed 1.2 or Higher" perk in the roster was unreachable from
  // the page no matter what a player did.
  const pick = box.querySelector('[data-k="frame"]');
  const nFrames = pick ? pick.options.length : 0;
  pick.value = 'valkyr_prime'; pick.dispatchEvent(new Event('change'));
  await sleep(1800);
  const picked = { armor: sim.wf_armor, energy: sim.wf_energy, sprint: sim.wf_sprint };
  // The numbers stay EDITABLE after a pick — the roster is unmodded, and one
  // gate no frame can open is only askable by typing.
  const e2 = box.querySelector('[data-k="wf_energy"]');
  e2.value = '900'; e2.dispatchEvent(new Event('change'));
  await sleep(1200);

  return { keys, nFrames, picked, overridden: sim.wf_energy, simArmor: 1500,
           before: before.replace(/\s+/g,' ').trim().slice(0,120),
           after: after.replace(/\s+/g,' ').trim().slice(0,120),
           url: await shareUrl() };
})()`);

check("the Tenno block carries every player field",
  ["aiming", "headshot_pct", "invisible", "airborne", "overshields", "channeling", "solo_weapon",
   "frame", "wf_armor", "wf_energy", "wf_sprint"]
    .every((k) => r.keys.includes(k)),
  r.keys.join(","));
check("the whole Warframe roster is offered", r.nFrames >= 120, `${r.nFrames} options`);
// Valkyr Prime, from data/frames.yaml: 1000 armor, 1.1 sprint, 225 max energy
// (175 at rank 0, +50). Three DIFFERENT numbers from one pick is the claim —
// filling only the two that already had fields would leave every sprint gate
// shut.
check("picking one fills armor, max energy AND sprint",
  r.picked.armor === 1000 && r.picked.sprint === 1.1 && r.picked.energy === 225,
  JSON.stringify(r.picked));
check("...and they stay editable — no frame reaches the 700-energy gate",
  r.overridden === 900, String(r.overridden));
check("typing armor reaches the scenario state", r.simArmor === 1500, String(r.simArmor));
check("no frame: Bulwark says nothing", !/Bulwark/i.test(r.before), r.before || "(no conditionals)");
check("1,500 armor: the panel states Bulwark's +500%", /Bulwark/i.test(r.after) && /500/.test(r.after), r.after || "(no conditionals)");

// OVERSHIELDS, the one player state that is not a number and not derivable from
// a frame: every Warframe can hold them and none has them by default, so it is
// a declaration the player makes. VERBATIM (Paris_Incarnon_Genesis, Guardian's
// Might): "*Increase Base Damage by +X. *With Overshields: Increase Base Damage
// by +Y." — the Paris Prime's Y is 74 against an X of 20, so the gate is worth
// more than the unconditional half and cannot hide inside rounding.
//
// It is checked on the DAMAGE rather than on the checkbox, because a control
// that stores a flag nobody reads looks exactly like one that works.
const os = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Paris_Prime'); route(); await sleep(3500);
  document.querySelectorAll('.tab').forEach(t => { if (/Sim/i.test(t.textContent)) t.click(); });
  await sleep(1200);
  // THE BASE FORM, so the number under test is the one the card's X and Y are
  // printed against. The default mode is the Incarnon cycle, whose panel is a
  // different weapon entry with its own stats.
  mode = 'base';
  // The PANEL's own number, not a fight's: this grant lands on base damage,
  // which the panel reports exactly where a hundred rolls report it through
  // noise. buildPayload() already carries tennoPayload(), so the state travels.
  const mb = async () => {
    const s = await api('/api/simulate', { ...buildPayload(), ...theFight(), runs: 2 });
    return (s.panel || {}).modified_base;
  };
  const bare = await mb();
  // BOTH TIERS. A tier-2 perk needs its tier-1 rung — the server enforces the
  // ladder and drops a perk whose rung is missing, which is why this reads as
  // "the perk does nothing" if only tier 2 is set.
  evoSel[1] = 'paris_prime_evo1_incarnon_form';
  evoSel[2] = 'paris_prime_guardians_might';
  markPresetDirty(); renderMods(); refreshPanel(); await sleep(2200);
  const withEvo = await mb();

  const box = document.getElementById('sim-technique');
  const cb = box.querySelector('[data-k="overshields"]');
  cb.checked = true; cb.dispatchEvent(new Event('change'));
  await sleep(2200);
  const withOS = await mb();
  return { present: !!cb, sent: !!sim.overshields, bare, withEvo, withOS,
           url: await shareUrl() };
})()`);

check("the overshield state has a control", os.present, String(os.present));
check("...ticking it reaches the scenario", os.sent === true, String(os.sent));
// THREE EXACT NUMBERS, all readable off the sources. 360 is the Paris Prime's
// own base (9 Impact + 63 Slash + 288 Puncture, data/weapons/); +20 is the
// card's X and +74 its Y, from the Paris Prime column of Guardian's Might.
// A direction check would pass on any bracket pointing the right way — these
// say the grant landed on the BASE, undiluted, and that the gate is worth its
// printed number and not some share of it.
check("the perk's unconditional half is +20 base damage",
  os.bare === 360 && os.withEvo === 380, `${os.bare} -> ${os.withEvo}`);
check("...and overshields pay its +74, exactly",
  os.withOS === 454, `${os.withEvo} -> ${os.withOS}`);

// THE LOADOUT — the fourth player state, and the first that is not about the
// wielder's body or what it is doing but about WHAT ELSE IS ON THEIR BACK.
//
// VERBATIM (Vasto_Incarnon_Genesis, EVO2 Perk 1, Lone Gun):
//   *Increase Base Damage by '''+X'''.        X = 24 on the Prime
//   *With No Primary Equipped:
//   **Increase Base Damage by '''+40'''
//   **Increase Base Magazine Capacity by '''+14'''.
//
// Checked on BOTH halves and on exact numbers, because this control exists to
// make a clause reachable that the app spent a week answering "no" to — a
// checkbox that stores a flag nobody reads looks exactly like one that works
// (owner, 2026-08-13).
const solo = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Vasto_Prime'); route(); await sleep(3500);
  document.querySelectorAll('.tab').forEach(t => { if (/Sim/i.test(t.textContent)) t.click(); });
  await sleep(1200);
  // THE BASE FORM: the card's numbers are printed against it, and the magazine
  // half is the one clause that says "does not affect Incarnon Form".
  mode = 'base';
  // TWO ENDPOINTS, because the two halves of this card are reported by two
  // different ones: base damage is the simulate call's resolved panel, and the
  // magazine is a STATS ROW on the panel call — the number the page prints.
  // (No backticks in here: this whole block is a template literal.)
  const panel = async () => {
    const body = { ...buildPayload(), ...theFight() };
    const s = await api('/api/simulate', { ...body, runs: 2 });
    const p = await api('/api/panel', body);
    const rows = (((p.forms || [])[0] || {}).stats) || [];
    const row = rows.find(x => x.key === 'magazine') || {};
    // …and WHO grew it. A magazine that moves with no source listed is the
    // panel telling half a story, and the gated add reaches the number by a
    // different route than the ungated one, so it needs its own row.
    const src = (row.sources || []).map(x => x.mod + ' ' + x.value).join(' | ');
    return { base: (s.panel || {}).modified_base, mag: Number(row.final), src };
  };
  const bare = await panel();
  evoSel[1] = 'vasto_prime_evo1_incarnon_form';
  evoSel[2] = 'vasto_prime_lone_gun';
  markPresetDirty(); renderMods(); refreshPanel(); await sleep(2200);
  const withEvo = await panel();

  const box = document.getElementById('sim-technique');
  const cb = box.querySelector('[data-k="solo_weapon"]');
  cb.checked = true; cb.dispatchEvent(new Event('change'));
  await sleep(2200);
  const withSolo = await panel();
  // Read the flag HERE, while it is on: the negative control below unticks it.
  const sent = !!sim.solo_weapon;

  // …AND THE TIER-MATE IS NOT REACHED BY IT. Deathtrap Trigger's clause is "On
  // Equip From Primary", which with no primary is not merely unmodelled but
  // impossible — so the same tick must move nothing.
  evoSel[2] = 'vasto_prime_deathtrap_trigger';
  markPresetDirty(); renderMods(); refreshPanel(); await sleep(2200);
  const trapSolo = await panel();
  cb.checked = false; cb.dispatchEvent(new Event('change'));
  await sleep(2200);
  const trapFull = await panel();

  return { present: !!cb, sent, bare, withEvo, withSolo, trapSolo, trapFull };
})()`);

check("the loadout has a control", solo.present, String(solo.present));
check("...ticking it reaches the scenario", solo.sent === true, String(solo.sent));
// 110 is the Vasto Prime's own base (16.5 Impact + 77 Slash + 16.5 Puncture),
// +24 is the card's X and +40 its conditional half. Exact numbers rather than a
// direction, so the grant is shown to land on the BASE and undiluted.
check("Lone Gun's unconditional half is +24 base damage",
  solo.bare.base === 110 && solo.withEvo.base === 134,
  `${solo.bare.base} -> ${solo.withEvo.base}`);
check("...and carrying nothing else pays its +40, exactly",
  solo.withSolo.base === 174, `${solo.withEvo.base} -> ${solo.withSolo.base}`);
// THE SECOND HALF, which no other player state has: a gated grant that is not
// damage. 6 rounds + 14.
check("...and the magazine half lands too: 6 -> 20",
  solo.withEvo.mag === 6 && solo.withSolo.mag === 20,
  `${solo.withEvo.mag} -> ${solo.withSolo.mag}`);
check("...and the panel says WHICH perk grew it",
  /Lone Gun/.test(solo.withSolo.src) && /14/.test(solo.withSolo.src)
    && !/Lone Gun/.test(solo.withEvo.src),
  `${solo.withEvo.src || "(none)"} -> ${solo.withSolo.src || "(none)"}`);
// THE NEGATIVE CONTROL, and the reason the option is not a blanket "turn the
// tier-2 perks on": its sibling asks to have SWAPPED FROM a primary, which
// carrying none makes impossible rather than possible.
check("...and Deathtrap Trigger is unmoved by it — no primary to equip from",
  solo.trapSolo.base === solo.trapFull.base && solo.trapSolo.mag === solo.trapFull.mag,
  `${solo.trapFull.base}/${solo.trapFull.mag} -> ${solo.trapSolo.base}/${solo.trapSolo.mag}`);

// THE FIGHT'S OWN STAT BONUSES — a panel, not a buff (owner, 2026-08-13).
// Three claims, and the third is the one a screenshot cannot show: it is the
// SCENARIO's, so it saves with the fight, travels to the optimizer read-only,
// and no ruler carries one.
const extra = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Torid'); route(); await sleep(3000);
  document.querySelectorAll('.tab').forEach(t => { if (/Sim/i.test(t.textContent)) t.click(); });
  await sleep(1200);
  const box = document.getElementById('sim-extra');
  const keys = [...box.querySelectorAll('[data-xk]')].map(e => e.dataset.xk);
  const dps = async () => {
    const s = await api('/api/simulate', { ...buildPayload(), ...theFight(), runs: 6, seed: 4 });
    return { base: (s.panel || {}).modified_base, ms: (s.panel || {}).multishot };
  };
  const before = await dps();

  // TYPED IN PERCENT, stored as the fraction the engine holds.
  const set = async (k, pct) => {
    const el = box.querySelector('[data-xk="' + k + '"]');
    el.value = String(pct); el.dispatchEvent(new Event('change'));
    await sleep(900);
  };
  await set('base_damage', 165);
  await set('multishot', 90);
  const after = await dps();
  const stored = JSON.parse(JSON.stringify(sim.extra_stats || {}));

  // …AND IT IS PART OF THE FIGHT'S DOCUMENT. snapshotScenario() is what a
  // scenario preset stores and snapshotState() is what a build preset does,
  // so asking both is the claim itself: this saves with the fight and is not
  // the build's (owner).
  //
  // Asserted on the DOCUMENTS rather than on a stored preset, because the
  // scenario a fresh page opens on is the OFFICIAL ruler — which is read-only
  // by design and therefore saves nothing, which would make a storage read
  // green for the wrong reason.
  await sleep(600);
  const saved = snapshotScenario();
  const buildDoc = snapshotState();

  // BLANK IS ABSENT, not a zero nobody typed.
  await set('base_damage', '');
  const cleared = JSON.parse(JSON.stringify(sim.extra_stats || {}));
  return { keys, before, after, stored, savedExtra: saved.extra_stats || null, cleared,
           inBuild: JSON.stringify(buildDoc).includes('extra_stats') };
})()`);

check("the extra-stats panel carries every mod-like bucket",
  ["base_damage", "multishot", "crit_chance", "crit_damage", "status_chance",
   "status_damage", "fire_rate", "reload_speed", "magazine"]
    .every((k) => extra.keys.includes(k)),
  extra.keys.join(","));
// STORED AS A FRACTION, typed as a percent — the units every bucket in the
// engine holds, and the units a mod's own `rankMax` is in.
check("...typed in percent, stored as the fraction the engine holds",
  extra.stored.base_damage === 1.65 && extra.stored.multishot === 0.9,
  JSON.stringify(extra.stored));
// THE ONLY CLAIM THAT MATTERS: it lands in the bucket, in the shipping build.
// The Torid's base is 45; +165% is x2.65, and 1 multishot + 90% is 1.9.
check("...and it reaches the number as a mod would",
  Math.abs(extra.after.base / extra.before.base - 2.65) < 1e-6
    && Math.abs(extra.after.ms - (extra.before.ms + 0.9)) < 1e-6,
  `base x${(extra.after.base / extra.before.base).toFixed(3)}, multishot ${extra.before.ms} -> ${extra.after.ms}`);
check("...it saves with the SCENARIO, not the build",
  extra.savedExtra && extra.savedExtra.base_damage === 1.65 && extra.inBuild === false,
  JSON.stringify({ scenario: extra.savedExtra, inBuild: extra.inBuild }));
// A BLANK BOX IS NOT A ZERO. Keeping the key would send a bonus nobody set and
// put an empty object into every share link.
check("...and clearing a box drops the key entirely",
  !("base_damage" in extra.cleared) && extra.cleared.multishot === 0.9,
  JSON.stringify(extra.cleared));

// ...AND IT TRAVELS, when there is a way to send it. Sharing can be switched
// off (SHARE_ENABLED) and is while its reliability is being investigated; the
// claim is about the PAYLOAD, so it is asserted whenever a payload can be made
// and returns of its own accord when the feature does.
const canShare = await evaluate("typeof SHARE_ENABLED === 'undefined' ? true : SHARE_ENABLED");
if (canShare) {
  await evaluate(`(() => { localStorage.clear(); location.href = ${JSON.stringify(r.url)}; })()`);
  await sleep(12000);
  const got = await evaluate(`(async () => { await new Promise(r=>setTimeout(r,2500)); return { armor: sim.wf_armor }; })()`);
  check("the Warframe travels in a share link", got.armor === 1500, String(got.armor));
} else {
  console.log("  --  sharing is off; the share half of this check is waiting for it");
}

await app.finish("the Tenno is on the field, and the page knows it");
