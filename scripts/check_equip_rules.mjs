// WHAT A MOD'S CARD SAYS THE WEAPON MAY DO — the equip rule, and the LOCK.
//
// Two claims, one pair of mod families (the Cannonades and the Acuity twins),
// and both are about a weapon the game will not let you build or a number it
// will not let you move.
//
// "Weapons with an Incarnon mode must have Semi-Auto trigger type for both
// firing modes in order to equip this mod" (wiki, Semi-Pistol_Cannonade). Dual
// Toxocyst is semi-auto and transforms into a full-auto form, so the mod fits
// while the Genesis is not installed and does not the moment tier 1 is.
//
// The engine decides (`pool_for_build`) and the page is TOLD the consequence
// (`evo_forbids` in /api/meta) — this asserts the page acts on it, on SCREEN:
//
//   · the picker offers the mod on a bare weapon and stops offering it once the
//     form is installed, and offers it again when the form comes back off
//   · installing the form UNEQUIPS it and says so — a slot emptying under you
//     silently is the one thing a build must never do
//   · the Form control greys the Incarnon options while the mod is worn, with
//     the reason on screen, and does NOT move the scenario's own selection
//   · and the rule holds through the shipping wasm: the sim refuses the pair
//
// Then the LOCK. "Equipping this mod will set weapon's Fire Rate to its default
// ignoring other bonuses, even negative effects" (wiki) — so the panel shows the
// weapon's own fire rate and NAMES what pinned it, and Frenzy, whose only effect
// is fire rate, is not offered as a buff to configure.
//
//   node scripts/check_equip_rules.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

// ENGLISH, so the assertions below read the strings the repo's source is
// written in rather than a locale's rendering of them.
const app = await openApp({ lang: "en", boot: 12000 });
const { evaluate, check } = app;
await app.load("/weapons/Dual_Toxocyst", 12000);

const MOD = "semi_pistol_cannonade";
const EVO1 = "dual_toxocyst_evo1_incarnon_form";

const r = await evaluate(`(async () => {
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  const go = async (p) => { history.pushState({},'',p); route(); await sleep(1500); };
  await sleep(1000);
  const out = {};
  // What the PICKER shows, read off the rendered rows rather than the pool.
  const offered = async () => {
    const slot = document.querySelector('#mod-slots .slot.empty') || document.querySelector('#mod-slots .slot');
    slot.click(); await sleep(600);
    const ids = [...document.querySelectorAll('#mod-menu .opt[data-id]')].map(e => e.dataset.id);
    closePopovers();
    return ids;
  };
  // A TIER IS A DROPDOWN, so installing and removing a perk is
  // the control own path rather than a click on a tile, and pickEvolution is
  // that path — the one the onPick handler calls.
  const evoTierOf = (id) => (weaponEvos().find(t => (t.options || []).some(o => o.id === id)) || {}).tier;

  out.bare = (await offered()).includes('${MOD}');

  // Equip it through the picker, the way a visitor does.
  const slot = document.querySelector('#mod-slots .slot.empty');
  slot.click(); await sleep(600);
  document.querySelector('#mod-menu .opt[data-id="${MOD}"]').click(); await sleep(600);
  out.equipped = slots.some(s => s.mod === '${MOD}');

  // THE MODE CONTROL, and it is in the BUILDER. How a weapon is played is part
  // of the build — "Torid, played through its cycle" is the thing a board ranks
  // — so the fight may not offer it at all, and this asserts both halves.
  //
  // It is the site's ONE dropdown (ddButton), not a native <select>. Its
  // options exist only while the panel is open, and a greyed one is .dis with
  // its data-v kept: an option carries its identity whether or not it can be
  // clicked.
  const trigger = document.querySelector('#mode-row [data-dd]');
  out.modeIsOneDropdown = !!trigger && trigger.tagName === 'BUTTON';
  document.querySelectorAll('.popover').forEach((x) => { if (x.id !== 'dd-popover') x.hidden = true; });
  if (trigger) trigger.click();
  await sleep(900);
  const opts = [...document.querySelectorAll('#dd-menu .opt')];
  out.formOff = opts.filter((o) => o.classList.contains('dis')).map((o) => o.dataset.v);
  out.formOn = opts.filter((o) => !o.classList.contains('dis')).map((o) => o.dataset.v);
  out.offText = opts.filter((o) => o.classList.contains('dis')).map((o) => o.textContent.trim()).join(' | ');
  // READ WHILE THE BUILD IS STILL IN THE BLOCKED MODE. The control names the
  // reason for the mode you are IN — once the build moves to a playable one
  // there is nothing left to explain, which is correct and is why this is read
  // here rather than at the end.
  out.why = (document.querySelector('#mode-row .warn') || {}).textContent || '';
  closePopovers();

  // ...and the FIGHT does not offer it. This is the decoupling: a scenario that
  // could decide how the weapon is fired could only ever measure whichever way
  // it happened to pin.
  await go('/weapons/Dual_Toxocyst/simulator');
  // A CONTROL THAT BINDS THE FORM, in either shape — a native field or one of
  // the page's own dropdowns, both of which carry data-k.
  //
  // NOT "any dropdown in this block at all", which is a PROXY for "a form
  // control" and holds only while this block has no dropdowns of its own: a
  // Warframe picker that becomes a dropdown is then read as a form. A proxy
  // that names the wrong thing passes for months and
  // then fails for a reason unrelated to what it is about.
  const formish = (el) => /form|mode/i.test(
    (el.dataset.k || '') + ' ' + (el.dataset.dd || '') + ' ' + (el.id || ''));
  out.fightOffersForm = [...document.querySelectorAll('#sim-technique [data-k], #sim-technique [data-dd]')]
    .some(formish);
  // The scenario still says what it said: a build may report that an option is
  // unavailable to it, and may not move a selection the visitor owns.
  out.formKept = sim.form === 'incarnon_cycle';

  // ...and the sim REFUSES the pair rather than reporting a number for it, in
  // the shipping wasm build. Run it for real — the form is still the cycle,
  // because nothing moved it — and read what lands ON SCREEN.
  document.getElementById('run-sim').click();
  for (let i = 0; i < 60 && !document.querySelector('#sim-results .error'); i++) await sleep(500);
  out.simSaid = (document.querySelector('#sim-results .error') || {}).textContent || '';

  // ...and NOW the click test, because it moves the build out of the blocked
  // mode and the two claims above only exist while it is in it.
  await go('/weapons/Dual_Toxocyst');
  const trigger2 = document.querySelector('#mode-row [data-dd]');
  if (trigger2) trigger2.click();
  await sleep(800);
  // GREYED MUST MEAN UNCLICKABLE, not merely grey. The build arrives in the
  // mode that is now blocked, so it is moved to a playable one FIRST — clicking
  // the option you are already on moves nothing whatever the rule says, and
  // could not tell a working guard from a missing one.
  const playable = [...document.querySelectorAll('#dd-menu .opt')].find((o) => !o.classList.contains('dis'));
  if (playable) playable.click();
  await sleep(700);
  out.movedToPlayable = mode;
  if (trigger2) trigger2.click();
  await sleep(800);
  const opts2 = [...document.querySelectorAll('#dd-menu .opt')];
  const before = mode;
  const greyed = opts2.find((o) => o.classList.contains('dis'));
  out.greyedTried = greyed ? greyed.dataset.v : null;
  if (greyed) greyed.click();
  await sleep(700);
  out.clickTook = mode !== before ? mode : null;
  closePopovers();

  // Installing the form takes the mod off, out loud.
  await go('/weapons/Dual_Toxocyst');
  pickEvolution(evoTierOf('${EVO1}'), '${EVO1}'); await sleep(900);
  out.evicted = !slots.some(s => s.mod === '${MOD}');
  out.said = (document.getElementById('toast') || {}).textContent || '';
  out.installedOffered = (await offered()).includes('${MOD}');

  // Taking the form back off gives it back — this excludes, it does not delete.
  pickEvolution(evoTierOf('${EVO1}'), null); await sleep(900);
  out.backOffered = (await offered()).includes('${MOD}');

  // ---- THE LOCK. Re-equip it and read the panel: Fire Rate must sit at the
  // weapon's own value and say what pinned it there.
  const slot2 = document.querySelector('#mod-slots .slot.empty');
  slot2.click(); await sleep(600);
  document.querySelector('#mod-menu .opt[data-id="${MOD}"]').click(); await sleep(2500);
  const frRowEl = () => [...document.querySelectorAll('#stats-rows .srow')]
    .find((r) => /Fire Rate/i.test(r.querySelector('.sk')?.textContent || ''));
  const frRow = frRowEl();
  out.frRow = frRow ? frRow.textContent.replace(/\\s+/g, ' ').trim() : '';
  // ...AND WHAT THE LOCK IGNORES. Creeping Bullseye is the pistol's
  // fire-rate-for-crit trade, so this is the build the question came from:
  // under a Cannonade its -20% must not be in the number, and the row must
  // not list it as though it were.
  const slot3 = document.querySelector('#mod-slots .slot.empty');
  slot3.click(); await sleep(600);
  document.querySelector('#mod-menu .opt[data-id="creeping_bullseye"]').click(); await sleep(2500);
  const withSlow = frRowEl();
  out.frLocked = withSlow ? withSlow.textContent.replace(/\\s+/g, ' ').trim() : '';
  out.frDead = withSlow
    ? [...withSlow.querySelectorAll('.ssrc')].map(e => ({
        text: e.textContent.replace(/\\s+/g, ' ').trim(), dead: e.classList.contains('sdead') }))
    : [];
  out.frBucket = !!(withSlow && withSlow.querySelector('.sbucket'));
  // THE OTHER HALF OF THE FAMILY. Pistol Acuity locks MULTISHOT with the same
  // sentence, and two multishot mods under it is the case where the bucket
  // line would print a visibly false equation — 1.0 x (1 + 1.20 + 0.60) =
  // x1.0 — rather than merely an unmarked row. (No backticks in here: this
  // whole block is inside a template literal.)
  for (const id of ['pistol_acuity', 'barrel_diffusion', 'lethal_torrent']) {
    const s = document.querySelector('#mod-slots .slot.empty');
    s.click(); await sleep(600);
    document.querySelector(\`#mod-menu .opt[data-id="\${id}"]\`).click(); await sleep(1800);
  }
  await sleep(1500);
  const msRow = [...document.querySelectorAll('#stats-rows .srow')]
    .find((r) => /Multishot/i.test(r.querySelector('.sk')?.textContent || ''));
  out.msLocked = msRow ? msRow.textContent.replace(/\\s+/g, ' ').trim() : '';
  out.msDead = msRow
    ? [...msRow.querySelectorAll('.ssrc')].map(e => e.classList.contains('sdead')) : [];
  out.msBucket = !!(msRow && msRow.querySelector('.sbucket'));
  // Frenzy is Dual Toxocyst's fire-rate passive, so under the lock it has
  // nothing to grant and no card to configure.
  await go('/weapons/Dual_Toxocyst/simulator');
  out.buffs = (document.getElementById('sim-buffs') || {}).textContent || '';
  return out;
})()`);

check("a bare Dual Toxocyst is offered the Cannonade", r.bare === true);
check("it equips", r.equipped === true);
check("the cycle is greyed while it is worn",
  r.formOff.includes("cycle"), JSON.stringify(r.formOff));
check("the base form stays available", r.formOn.includes("base"), JSON.stringify(r.formOn));
check("the mode lives in the BUILDER", r.modeIsOneDropdown === true);
check("...and the fight does not offer it at all", r.fightOffersForm === false);
check("a playable mode can be picked", r.movedToPlayable === "base", String(r.movedToPlayable));
check("...and a greyed option cannot be clicked into the build",
  r.greyedTried !== null && r.clickTook === null,
  `tried ${r.greyedTried}, mode moved to ${r.clickTook}`);
check("...each saying why", /trigger on every firing mode/.test(r.offText), JSON.stringify(r.offText.slice(0, 120)));
check("the reason is on screen", /firing mode/.test(r.why), JSON.stringify(r.why));
check("the sim refuses the pair", /firing mode/.test(r.simSaid), JSON.stringify(r.simSaid));
check("installing the form unequips it", r.evicted === true);
check("...and says so", /firing mode/.test(r.said), JSON.stringify(r.said));
check("the picker stops offering it", r.installedOffered === false);
check("removing the form offers it again", r.backOffered === true);
check("the panel pins Fire Rate at the weapon's default",
  /locked at the weapon's default by/.test(r.frRow) && /Semi-Pistol Cannonade/.test(r.frRow),
  JSON.stringify(r.frRow));
// The row must not argue with itself: the number stays the weapon's, and the
// bonus it is ignoring is struck through and SAID, not silently listed as a
// contribution (owner — the question was whether the lock works at
// all, asked of a panel that showed both answers at once).
console.log("locked row:", JSON.stringify(r.frLocked), JSON.stringify(r.frDead));
const frOf = (s) => (String(s).match(/([\d.]+)\/s/) || [])[1];
check("a -20% fire-rate mod under the lock does not move the number",
  frOf(r.frLocked) && frOf(r.frLocked) === frOf(r.frRow) && !/→/.test(r.frLocked),
  `${JSON.stringify(r.frRow)} vs ${JSON.stringify(r.frLocked)}`);
check("...and its line is marked ignored, not listed as a contribution",
  r.frDead.length > 0 && r.frDead.every((x) => x.dead) && /ignored/i.test(r.frLocked),
  JSON.stringify(r.frDead));
check("...and no bucket arithmetic is drawn for a stat with an empty bucket",
  r.frBucket === false);
// The Acuity twins lock Multishot with the same sentence, so they answer to
// the same three claims — and two multishot mods under one is where a drawn
// bucket would be a false equation rather than just a confusing row.
console.log("locked multishot:", JSON.stringify(r.msLocked), JSON.stringify(r.msDead));
check("Multishot locks the same way, naming what pinned it",
  /locked at the weapon's default by/.test(r.msLocked) && /Pistol Acuity/.test(r.msLocked)
    && !/→/.test(r.msLocked),
  JSON.stringify(r.msLocked));
check("...with both ignored mods marked, and no arithmetic drawn",
  r.msDead.length === 2 && r.msDead.every(Boolean) && r.msBucket === false,
  `${JSON.stringify(r.msDead)} bucket ${r.msBucket}`);
check("...and Frenzy is not offered as a buff to configure",
  !/Frenzy/i.test(r.buffs), JSON.stringify(r.buffs.slice(0, 200)));

await app.finish("a card's equip rule reaches the screen, both ways");
