/// THE SHARE CARD IS BUILT FROM THE BUILDER'S STEPS (docs/UI.md §The share
/// card): every step has a block, the blocks come in the order the page shows
/// the steps — move one there and the card follows — the head's backdrop word
/// is the weapon's own, and the share panel draws the card.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000, lang: "en", base: process.env.WFSIM_BASE });
const { evaluate, check } = app;

await app.load("/weapons/Torid/builder", 12000);
const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((res) => setTimeout(res, ms));
  ['serration', 'split_chamber', 'point_strike'].forEach((m, i) => { slots[i].mod = m; slots[i].rank = modById(m).max_rank; });
  arcanes = ['primary_deadhead'];
  evoSel = { 1: 'torid_evo1_incarnon_form', 2: 'torid_final_fusillade' };
  renderMods(); renderEvo(); await sleep(500);
  const out = {};
  out.unmapped = builderSteps().map((b) => b.id).filter((id) => !(id in CARD_BLOCKS));
  const ids = () => cardBlocks({ g: document.createElement('canvas').getContext('2d'), img: new Map() }).map((b) => b.id);
  const shown = () => builderSteps().filter((b) => !b.hidden).map((b) => b.id).filter((id) => typeof CARD_BLOCKS[id] === 'function');
  out.order = ids(); out.steps = shown();
  // MOVE A STEP ON THE PAGE: the evolutions ahead of the mods.
  const evo = document.getElementById('evo-block'), mods = document.getElementById('mod-block');
  const home = evo.nextElementSibling;
  mods.parentNode.insertBefore(evo, mods);
  out.moved = ids();
  home.parentNode.insertBefore(evo, home);
  out.keywords = ['Tenet Arca Plasmor', 'Nikana Prime', 'Lato Vandal', 'Kuva Bramma', 'Lex'].map((n) => cardKeyword(n).key);
  const c = document.createElement('canvas');
  await drawShareCard(c, 'https://wfsim.app/weapons/Torid/s/AAAAAAAAAA', {});
  out.size = [c.width, c.height];
  // THE PANEL'S ENTRY draws it.
  const bar = document.getElementById('preset-bar-builder-builds');
  await openSharePanel(bar); await sleep(1500);
  const full = bar.querySelector('.pshare .sh-full');
  if (full) { full.click(); await sleep(3000); }
  const cv = bar.querySelector('.pshare .sh-canvas');
  out.panel = cv ? [cv.tagName, cv.naturalWidth, cv.naturalHeight] : null;
  return out;
})()`);

check("every builder step has a card block", r.unmapped.length === 0, JSON.stringify(r.unmapped));
check("the card's blocks come in the builder's order", JSON.stringify(r.order) === JSON.stringify(r.steps),
  `${r.order.join(" ")} vs ${r.steps.join(" ")}`);
check("...and a step moved on the page moves on the card", r.moved.indexOf("evo-block") < r.moved.indexOf("mod-block"),
  r.moved.join(" "));
check("the backdrop word is the one fewest weapons share",
  JSON.stringify(r.keywords) === JSON.stringify(["PLASMOR", "NIKANA", "LATO", "BRAMMA", "LEX"]), JSON.stringify(r.keywords));
check("the card is drawn, 1080 wide at twice the pixels, as tall as its blocks", r.size[0] === 2160 && r.size[1] > 1200,
  JSON.stringify(r.size));
// AN <img>, NOT THE CANVAS: a phone saves a picture by long-pressing it.
check("the share panel shows it as a picture a phone can long-press", !!r.panel && r.panel[0] === "IMG" && r.panel[1] === 2160,
  JSON.stringify(r.panel));
await app.finish("the share card is the builder's steps, in the builder's order");
