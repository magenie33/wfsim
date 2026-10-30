// A TIER WITH ONE OPTION IS NOT A CHOICE. Every Genesis's tier 1 (the form) is
// always installed, so no surface offers to swap it, exclude it or switch it
// off — and the Shapley analysis still installs it in every subset, or the full
// build it reports on would not be the build on screen.
//
//   node scripts/check_fixed_tier.mjs
import { openApp } from "./cdp.mjs";
const app = await openApp({ boot: 11000 });
const { evaluate, check, sleep, send, BASE } = app;

await send("Page.navigate", { url: BASE + "/weapons/Torid/builder" });
await sleep(11000);
const r = await evaluate(String.raw`(async () => {
  const nap = (ms) => new Promise((r) => setTimeout(r, ms));
  const tiers = weaponEvos();
  const fixed = tiers.filter((t) => t.options.length === 1).map((t) => t.tier);
  const open = tiers.filter((t) => t.options.length > 1).map((t) => t.tier);
  const plate = (t) => document.querySelector('#evo-rows [data-slot="dd-evo-' + t + '"]');
  const out = { fixed, open, unlock: (weaponInfo($('weapon').value) || {}).unlock_evo };
  out.fixedDots = fixed.map((t) => !!(plate(t) && plate(t).querySelector('.dots')));
  out.openDots = open.map((t) => !!(plate(t) && plate(t).querySelector('.dots')));
  // A click on the plate opens nothing.
  if (fixed.length && plate(fixed[0])) {
    plate(fixed[0]).click(); await nap(400);
    out.fixedOpened = !!document.querySelector('#dd-menu .opt');
    closePopovers();
  }
  renderOptLimits();
  out.limitTiers = [...document.querySelectorAll('#opt-limits .opt-limit-tier')].map((e) => e.textContent.trim());
  out.limitIds = [...document.querySelectorAll('#opt-limits .opt[data-axis="evolutions"]')].map((e) => e.dataset.id);
  const parts = shapleyParts();
  out.parts = parts.map((p) => p.key);
  out.none = shapleyPayload(parts, 0).evolutions;
  out.all = shapleyPayload(parts, (1 << parts.length) - 1).evolutions;
  out.build = Object.values(evoSel).filter(Boolean);
  return out;
})()`);

const unlockTier = r.fixed[0];
check("the Torid has a one-option tier and tiers with a choice", r.fixed.length === 1 && r.open.length >= 2, JSON.stringify(r));
check("the builder draws the one-option tier with no ⋯, the others with one",
  r.fixedDots.every((d) => !d) && r.openDots.every(Boolean), JSON.stringify([r.fixedDots, r.openDots]));
check("...and a click on it opens no list", r.fixedOpened === false, String(r.fixedOpened));
check("the optimizer's limits list no one-option tier and no unlock",
  r.limitTiers.length === r.open.length && !r.limitIds.includes(r.unlock) && r.limitIds.length > 0,
  JSON.stringify([r.limitTiers, r.limitIds.length]));
check("the Shapley analysis has no part for it", !r.parts.includes("evo:" + unlockTier) && !r.parts.includes("form"), JSON.stringify(r.parts));
check("...and installs it in every subset, so the full subset is the build on screen",
  r.none.includes(r.unlock) && JSON.stringify([...r.all].sort()) === JSON.stringify([...r.build].sort()),
  JSON.stringify({ none: r.none, all: r.all, build: r.build }));

await app.finish("a tier with one option is installed and never offered");
