// AN EQUIPPED RIVEN PRINTS ITS ROLLED STATS, on a phone and on a desktop.
//
// A riven's card text IS its roll, and the picker has always listed it — then
// equipping the thing dropped every line, which is the one place a player reads
// them while building. The slot required a `desc_ranks` or an official
// description and a riven has neither.
//
// ASSERTED FROM THE SLOT'S OWN MARKUP at both widths, because the block is
// styled per viewport and "it works on my screen" is how the phone regressed
// the last three times.
//
//   node scripts/check_riven_slot_lines.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000, base: process.env.WFSIM_BASE });
const { evaluate, check, send, sleep, BASE } = app;

await evaluate("localStorage.clear(); localStorage.setItem('wfsim-lang', 'en')");
// `?riven=1` seeds this weapon's board riven and equips it — the same link the
// board hands out, so this is the state a reader actually arrives in.
// The riven parameter rides WITH a bench link — that is the shape the board
// hands out, and the only one that seeds a riven on arrival.
await send("Page.navigate", {
  url: `${BASE}/weapons/Ballistica_Prime?bench=standard_single_target&riven=1`,
});
await sleep(14000);

const read = async (label, w, h) => {
  await send("Emulation.setDeviceMetricsOverride", {
    width: w, height: h, deviceScaleFactor: 1, mobile: w < 700,
  });
  await sleep(1200);
  return evaluate(`(async () => {
    const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
    await sleep(400);
    const out = { label: ${JSON.stringify(label)} };
    // WHICH SLOT HOLDS THE RIVEN, from the build rather than from the DOM.
    const at = slots.findIndex((s) => s.mod && String(s.mod).startsWith('riven:'));
    out.slotIdx = at;
    if (at < 0) return out;
    out.modId = slots[at].mod;
    // THE EIGHT MAIN SLOTS LIVE IN #mod-slots; the exilus has its own box.
    const cell = at === EXILUS
      ? (document.getElementById('exilus') || {}).firstElementChild
      : document.querySelectorAll('#mod-slots > .slot')[at];
    out.cellFound = !!cell;
    if (!cell) return out;
    // …AND WHERE ITS NAME POINTS. A riven's name is invented by the drop, so a
    // URL built from it 404s; the mechanic's page is what a click is for.
    const a = cell.querySelector('.mn a');
    out.href = a ? a.getAttribute('href') : null;
    const me = cell.querySelector('.me');
    out.hasBlock = !!me;
    out.lines = me ? Array.from(me.children).map((d) => d.textContent.trim()).filter(Boolean) : [];
    out.visible = me ? (me.getBoundingClientRect().height > 0) : false;
    // The same riven's lines as the PICKER prints them, to compare against.
    const m = modById(slots[at].mod);
    out.cardLines = m ? cardLines(m, m.max_rank).length : -1;
    return out;
  })()`);
};

const desk = await read("desktop", 1440, 900);
const phone = await read("phone", 390, 844);

check(
  "a riven really is equipped in the build",
  desk.slotIdx >= 0,
  `slot ${desk.slotIdx}, id ${desk.modId}`,
);
check(
  "the riven's name links to the mechanic, not to a page named after the roll",
  desk.href === "https://wiki.warframe.com/w/Riven_Mods",
  `href ${desk.href}`,
);
check(
  "the engine printed lines for it at all",
  desk.cardLines > 0,
  `cardLines ${desk.cardLines}`,
);

for (const v of (desk.slotIdx >= 0 ? [desk, phone] : [])) {
  check(
    `the equipped riven prints its stats in the slot — ${v.label}`,
    v.hasBlock === true && v.lines.length > 0,
    `block ${v.hasBlock}, lines ${JSON.stringify(v.lines)}`,
  );
  check(
    `…and the block is actually on screen — ${v.label}`,
    v.visible === true,
    `visible ${v.visible}`,
  );
  check(
    `…with every line the picker would print — ${v.label}`,
    v.lines.length >= v.cardLines,
    `slot ${v.lines.length}, picker ${v.cardLines}`,
  );
  // THE ROLL, NOT THE STAT'S NAME. The printed values arrive from `/api/riven`
  // AFTER the first render, so the slots are redrawn when they land. A slot
  // that is not redrawn keeps the fallback — "critical damage / multishot" with
  // no numbers on it, which is the half of the card a player builds around.
  check(
    `…and each line carries its rolled value — ${v.label}`,
    v.lines.length > 0 && v.lines.every((x) => /[0-9]/.test(x)),
    JSON.stringify(v.lines),
  );
}

// …AND ITS RANK STEPPER RANKS THE CARD. The engine prints and fights a riven at
// its spec's rank, so a stepper that wrote the slot moved the drain alone: the
// lines kept their max-rank values and so did the fight. Down to R0, each line
// is a ninth of what it was, the card itself says 0, and the drain is DE's 2.
const ranked = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const at = slots.findIndex((s) => s.mod && String(s.mod).startsWith('riven:'));
  if (at < 0) return null;
  const cell = () => document.querySelectorAll('#mod-slots > .slot')[at];
  const nums = () => Array.from(cell().querySelectorAll('.me > div'))
    .map((d) => parseFloat((d.textContent.match(/[0-9.]+/) || [])[0])).filter((x) => !isNaN(x));
  const before = nums();
  for (let k = 0; k < 8; k++) {
    cell().querySelector('.rk[data-d="-1"]').click();
    await sleep(600);
  }
  await sleep(1500);
  const id = slots[at].mod;
  const card = loadPresetList(RIVENS).find((p) => 'riven:' + p.id === id);
  const sent = rivenPayload().find((r) => 'riven:' + r.id === id);
  return { before, after: nums(), rank: card && card.state.rank, sent: sent && sent.spec.rank,
    drain: modDrain(modById(id), slots[at].rank),
    shown: (cell().querySelector('.rank b') || {}).textContent };
})()`);
check(
  "the riven slot's stepper takes the CARD to R0",
  ranked && ranked.rank === 0 && ranked.shown === "R0/8" && ranked.sent === 0,
  JSON.stringify(ranked && { rank: ranked.rank, shown: ranked.shown, sent: ranked.sent }),
);
check(
  "…and every printed line follows, at (0 + 1) / 9 of its max-rank value",
  ranked && ranked.before.length > 0 && ranked.after.length === ranked.before.length
    && ranked.before.every((v, k) => Math.abs(ranked.after[k] * 9 - v) <= Math.max(0.15, v * 0.01)),
  JSON.stringify(ranked && { before: ranked.before, after: ranked.after }),
);
check(
  "…and it drains 2, DE's rank-0 riven",
  ranked && ranked.drain === 2,
  `drain ${ranked && ranked.drain}`,
);

// …AND THE SAME PAGE RELOADED WITH THAT RIVEN ALREADY IN STORAGE.
//
// A DIFFERENT PATH, not a second opinion: with a riven saved,
// `refreshRivenNames` awaits the engine and whatever it redraws lands after
// `applyWeapon` has finished, where with none it runs straight through. Both
// orders have to draw a build. Whether the page BOOTED is not asked here —
// `cdp.mjs` asks it of every evaluate now, so a crash on either path fails this
// check without it carrying a rule of its own.
await send("Page.navigate", { url: `${BASE}/weapons/Ballistica_Prime` });
await sleep(14000);

const boot = await evaluate(`(() => ({
  saved: loadPresetList(RIVENS).length,
  drew: document.querySelectorAll('#mod-slots > .slot').length,
}))()`);

check(
  "the reload really had a riven in storage",
  boot.saved > 0,
  `saved ${boot.saved}`,
);
check(
  "…and the build is drawn on that path too",
  boot.drew === 8,
  `drew ${boot.drew}`,
);

await app.finish("an equipped riven prints its roll and links to the mechanic");
