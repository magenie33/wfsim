// WHAT A MOD'S CARD SAYS THE WEAPON MAY DO — the equip rule, and the LOCK.
//
// Two claims, one pair of mod families (the Cannonades and the Acuity twins),
// and both are about a weapon the game will not let you build or a number it
// will not let you move.
//
// "Weapons with an Incarnon mode must have Semi-Auto trigger type for both
// firing modes in order to equip this mod" (wiki, Semi-Pistol_Cannonade). Dual
// Toxocyst is semi-auto and transforms into a full-auto form, so the mod fits
// while the Genesis is not installed and does not the moment tier 1 is (user,
// 2026-08-04: "只要没点第一个 evo 就视为还是纯半自动，那就可以带，如果装上了就
// 不可以带").
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
  const evoCard = (id) => document.querySelector(\`.evopick[data-id="\${id}"]\`);

  out.bare = (await offered()).includes('${MOD}');

  // Equip it through the picker, the way a visitor does.
  const slot = document.querySelector('#mod-slots .slot.empty');
  slot.click(); await sleep(600);
  document.querySelector('#mod-menu .opt[data-id="${MOD}"]').click(); await sleep(600);
  out.equipped = slots.some(s => s.mod === '${MOD}');

  // The FORM control: the Incarnon options are greyed, the reason is on screen,
  // and the scenario's own selection is untouched.
  //
  // It is the site's ONE dropdown (ddButton), not a native <select>, so its
  // options exist only while the panel is open and a greyed one is .dis with
  // no data-v — carrying no value is what takes its click handler away. The
  // fight also arrives LOCKED on the official ruler, so it is copied first,
  // which is the real flow for editing a fight and the only one that can open
  // this control at all.
  await go('/weapons/Dual_Toxocyst/simulator');
  if (officialScenarioActive()) { copyActiveScenario(); await sleep(1200); }
  const trigger = document.querySelector('#sim-technique [data-dd][data-k="form"]');
  out.formIsOneDropdown = !!trigger && trigger.tagName === 'BUTTON';
  document.querySelectorAll('.popover').forEach((x) => { if (x.id !== 'dd-popover') x.hidden = true; });
  if (trigger) trigger.click();
  await sleep(900);
  const opts = [...document.querySelectorAll('#dd-menu .opt')];
  out.formOff = opts.filter((o) => o.classList.contains('dis')).map((o) => o.dataset.v);
  out.formOn = opts.filter((o) => !o.classList.contains('dis')).map((o) => o.dataset.v);
  // ...and a greyed row shows WHY, which is the half of the rule a visitor acts
  // on: an option that vanished would need no reason.
  out.offText = opts.filter((o) => o.classList.contains('dis')).map((o) => o.textContent.trim()).join(' | ');
  // GREYED MUST MEAN UNCLICKABLE, not merely grey. Clicking a forbidden form
  // and having it take is the failure the colour is supposed to prevent, and
  // the colour cannot prevent it.
  const before = sim.form;
  // NOT the one already selected — clicking the current value moves nothing
  // whatever the rule says, so it cannot tell a working guard from a missing
  // one. This picks a greyed option the fight is not already on.
  const greyed = opts.find((o) => o.classList.contains('dis') && o.dataset.v !== before);
  out.greyedTried = greyed ? greyed.dataset.v : null;
  if (greyed) greyed.click();
  await sleep(700);
  out.clickTook = sim.form !== before ? sim.form : null;
  closePopovers();
  out.why = (document.querySelector('#sim-technique .warn') || {}).textContent || '';
  // The scenario still says what it said: a build may report that an option is
  // unavailable to it, and may not move a selection the visitor owns.
  out.formKept = sim.form === 'incarnon_cycle';

  // ...and the sim REFUSES the pair rather than reporting a number for it, in
  // the shipping wasm build. Run it for real — the form is still the cycle,
  // because nothing moved it — and read what lands ON SCREEN.
  document.getElementById('run-sim').click();
  for (let i = 0; i < 60 && !document.querySelector('#sim-results .error'); i++) await sleep(500);
  out.simSaid = (document.querySelector('#sim-results .error') || {}).textContent || '';

  // Installing the form takes the mod off, out loud.
  await go('/weapons/Dual_Toxocyst');
  evoCard('${EVO1}').click(); await sleep(900);
  out.evicted = !slots.some(s => s.mod === '${MOD}');
  out.said = (document.getElementById('toast') || {}).textContent || '';
  out.installedOffered = (await offered()).includes('${MOD}');

  // Taking the form back off gives it back — this excludes, it does not delete.
  evoCard('${EVO1}').parentElement.querySelector('.evopick.empty').click(); await sleep(900);
  out.backOffered = (await offered()).includes('${MOD}');

  // ---- THE LOCK. Re-equip it and read the panel: Fire Rate must sit at the
  // weapon's own value and say what pinned it there.
  const slot2 = document.querySelector('#mod-slots .slot.empty');
  slot2.click(); await sleep(600);
  document.querySelector('#mod-menu .opt[data-id="${MOD}"]').click(); await sleep(2500);
  const frRow = [...document.querySelectorAll('#stats-rows .srow')]
    .find((r) => /Fire Rate/i.test(r.querySelector('.sk')?.textContent || ''));
  out.frRow = frRow ? frRow.textContent.replace(/\\s+/g, ' ').trim() : '';
  // Frenzy is Dual Toxocyst's fire-rate passive, so under the lock it has
  // nothing to grant and no card to configure.
  await go('/weapons/Dual_Toxocyst/simulator');
  out.buffs = (document.getElementById('sim-buffs') || {}).textContent || '';
  return out;
})()`);

check("a bare Dual Toxocyst is offered the Cannonade", r.bare === true);
check("it equips", r.equipped === true);
check("the Incarnon form options are greyed while it is worn",
  r.formOff.includes("incarnon_cycle") && r.formOff.includes("incarnon"), JSON.stringify(r.formOff));
check("the base form stays available", r.formOn.includes("base"), JSON.stringify(r.formOn));
check("it is the site's one dropdown, not a native select", r.formIsOneDropdown === true);
check("...and a greyed option cannot be clicked into the fight",
  r.greyedTried !== null && r.clickTook === null,
  `tried ${r.greyedTried}, form moved to ${r.clickTook}`);
check("...each saying why", /trigger on every firing mode/.test(r.offText), JSON.stringify(r.offText.slice(0, 120)));
check("the reason is on screen", /firing mode/.test(r.why), JSON.stringify(r.why));
check("the build does not move the scenario's own selection", r.formKept === true);
check("the sim refuses the pair", /firing mode/.test(r.simSaid), JSON.stringify(r.simSaid));
check("installing the form unequips it", r.evicted === true);
check("...and says so", /firing mode/.test(r.said), JSON.stringify(r.said));
check("the picker stops offering it", r.installedOffered === false);
check("removing the form offers it again", r.backOffered === true);
check("the panel pins Fire Rate at the weapon's default",
  /locked at the weapon's default by/.test(r.frRow) && /Semi-Pistol Cannonade/.test(r.frRow),
  JSON.stringify(r.frRow));
check("...and Frenzy is not offered as a buff to configure",
  !/Frenzy/i.test(r.buffs), JSON.stringify(r.buffs.slice(0, 200)));

await app.finish("a card's equip rule reaches the screen, both ways");
