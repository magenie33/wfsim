// A CARD ON THE EVERY-RANK LIST IS A ROW PER RANK, and its rank reaches the
// fight.
//
// The list (`/api/meta.every_rank`, editable in the quick calc) names cards
// whose lower ranks can beat their max. Each named card is offered once per
// rank in the builder's picker and in the quick calc's candidates; picking a
// lower rank seats the card at that rank; the build leaves the page as
// `<card>@<rank>` and comes back from that spelling as the same slot.
//
//   node scripts/check_every_rank.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000, base: process.env.WFSIM_BASE });
const { evaluate, check, send, sleep, BASE } = app;

await evaluate("localStorage.clear(); localStorage.setItem('wfsim-lang', 'en')");
await send("Page.navigate", { url: `${BASE}/weapons/Burston_Prime` });
await sleep(14000);

const out = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const r = {};
  r.listed = everyRank().mods.includes('hunter_track');
  // THE PICKER, for main slot 0, without a scan behind it.
  gainPrefs = { ...gainPrefs, on: false };
  openPicker(0, document.querySelectorAll('#mod-slots > .slot')[0]);
  await sleep(300);
  const rows = Array.from(document.querySelectorAll('#mod-menu .opt[data-id]')).map((o) => o.dataset.id);
  r.rows = rows.filter((id) => id.startsWith('hunter_track'));
  // PICKING RANK 0 seats the card at rank 0.
  const row = document.querySelector('#mod-menu .opt[data-id="hunter_track@0"]');
  if (row) row.click();
  await sleep(300);
  r.slot = { mod: slots[0].mod, rank: slots[0].rank };
  r.sent = buildPayload().mods;
  // …AND THE SPELLING COMES BACK AS THE SAME SLOT.
  const back = stateFromBuild(buildPayload(), 'burston_prime');
  r.back = { mod: back.slots[0].mod, rank: back.slots[0].rank };
  // THE QUICK CALC ASKS ABOUT EVERY OTHER RANK of the card this slot holds.
  r.cands = gainCandidates({ kind: 'mods', idx: 0 }).map((c) => c.id).filter((id) => id.startsWith('hunter_track'));
  // TAKING THE CARD OFF THE LIST takes its rows away; the default brings them back.
  setEveryRank({ ...everyRank(), mods: everyRank().mods.filter((x) => x !== 'hunter_track') });
  r.afterRemove = gainCandidates({ kind: 'mods', idx: 0 }).map((c) => c.id).filter((id) => id.startsWith('hunter_track@')).length;
  setEveryRank(null);
  r.afterReset = everyRank().mods.includes('hunter_track');
  closePopovers();
  return r;
})()`);

check("hunter_track is on the default every-rank list", out.listed, JSON.stringify(out));
check(
  "the picker offers it at max and at each lower rank",
  JSON.stringify(out.rows.slice().sort())
    === JSON.stringify(["hunter_track", "hunter_track@0", "hunter_track@1", "hunter_track@2", "hunter_track@3", "hunter_track@4"]),
  JSON.stringify(out.rows),
);
check("picking rank 0 seats the card at rank 0",
  out.slot.mod === "hunter_track" && out.slot.rank === 0, JSON.stringify(out.slot));
check("the build leaves the page as hunter_track@0",
  (out.sent || []).includes("hunter_track@0"), JSON.stringify(out.sent));
check("...and that spelling comes back as the same slot",
  out.back.mod === "hunter_track" && out.back.rank === 0, JSON.stringify(out.back));
check("the quick calc asks about every other rank",
  JSON.stringify(out.cands.slice().sort())
    === JSON.stringify(["hunter_track", "hunter_track@1", "hunter_track@2", "hunter_track@3", "hunter_track@4"]),
  JSON.stringify(out.cands));
check("a card taken off the list is asked at max only", out.afterRemove === 0, String(out.afterRemove));
check("the default list comes back", out.afterReset === true);

await app.finish("a card on the every-rank list is a row per rank, and its rank reaches the fight");
