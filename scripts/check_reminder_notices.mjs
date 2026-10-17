// WHERE A FIRED REMINDER IS SAID (27-utility.js): kept under the bell on the
// bar and counted there, said in a card in the corner that leaves on its own —
// never a strip over the page. Many firing at once is one card, a quiet
// reminder no card at all, and on a phone the bell stays on the bar.
//   node scripts/check_reminder_notices.mjs [shots=<dir>]   (WFSIM_BASE to test a server)
import { openApp } from "./cdp.mjs";
import { writeFileSync } from "node:fs";
import { join } from "node:path";

const shots = (process.argv.find((a) => a.startsWith("shots=")) || "").slice(6);
const app = await openApp({ boot: 11000, base: process.env.WFSIM_BASE, lang: "zh" });
const { evaluate, check, send, sleep } = app;
const shot = async (name) => {
  if (!shots) return;
  await sleep(300);
  const m = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(join(shots, `${name}.png`), Buffer.from(m.result.data, "base64"));
};
const page = (body) => evaluate(`(async () => { ${body} })()`);

await send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 800, deviceScaleFactor: 1, mobile: false });
// THE FIXTURE: reminders on an era no fissure holds, so the live feed never
// matches one, and items made to match them.
await page(`
  localStorage.removeItem(REMINDERS_KEY); localStorage.removeItem(REMINDERS_SEEN_KEY); localStorage.removeItem(REMINDERS_HITS_KEY);
  history.pushState({}, '', '/weapons'); route();
  const make = (tier, quiet) => { const r = reminderAdd('fissure', { tier }, {}); if (quiet) reminderQuiet(r.id); };
  ['T1', 'T2', 'T3', 'T4', 'T5', 'T6'].forEach((t) => make('Check' + t, false));
  make('CheckQuiet', true);
  window.__fire = (tiers) => {
    const at = Date.now() + 1;
    world = { read_at_ms: at, items: tiers.map((t, i) => ({ kind: 'fissure', id: 'check-' + t + '-' + at + '-' + i,
      attributes: { list: 'normal', tier: 'Check' + t, mission: 'MT_SURVIVAL', node: 'SolNode1' },
      names: { tier: { en: 'Era ' + t }, mission: { en: 'Survival' }, node: { en: 'Node' }, system: { en: 'Planet' } },
      started_at_ms: at + 5000, ends_at_ms: at + 3600000 })) };
    reminderCheck();
  };
  renderUtility();
`);

const state = () => page(`
  const bar = document.querySelector('.topbar').getBoundingClientRect();
  const bell = document.getElementById('rem-bell'), b = bell.getBoundingClientRect();
  const cards = [...document.querySelectorAll('#rem-toasts .rtoast')];
  return { bellShown: !bell.hidden && b.width > 0, bellInBar: b.top >= bar.top && b.bottom <= bar.bottom && b.right <= innerWidth,
    count: document.getElementById('rem-bell-count').hidden ? 0 : Number(document.getElementById('rem-bell-count').textContent),
    cards: cards.map((c) => c.querySelector('.rtoast-t').textContent),
    cardsBelowBar: cards.every((c) => c.getBoundingClientRect().top > bar.bottom),
    cardsInView: cards.every((c) => { const r = c.getBoundingClientRect(); return r.left >= 0 && r.right <= innerWidth && r.bottom <= innerHeight; }),
    cardsClearOfNona: (() => { const f = document.querySelector('.nona-fab'); if (!f || !f.offsetParent) return true;
      const n = f.getBoundingClientRect();
      return cards.every((c) => { const r = c.getBoundingClientRect(); return r.bottom <= n.top || r.right <= n.left || r.left >= n.right; }); })(),
    oldBanner: !!document.getElementById('reminder-note') };
`);

let s = await state();
check("the bell is on the bar once the reader has a reminder", s.bellShown && s.bellInBar, JSON.stringify(s));
await page(`__fire(['T1'])`);
s = await state();
check("one reminder firing is one card in the corner, and counted on the bell",
  s.cards.length === 1 && s.count === 1 && s.cardsBelowBar && s.cardsInView, JSON.stringify(s));
check("…and nothing is drawn over the top of the page, nor over Nona's button", !s.oldBanner && s.cardsClearOfNona, JSON.stringify(s));
await shot("desktop-one");
await page(`__fire(['T2', 'T3', 'T4', 'T5', 'T6'])`);
s = await state();
check("five firing at once is one more card that counts them, never five", s.cards.length === 2 && /5/.test(s.cards[1]) && s.count === 6,
  JSON.stringify(s));
await page(`__fire(['Quiet'])`);
s = await state();
check("a quiet reminder fires into the bell only", s.cards.length === 2 && s.count === 7, JSON.stringify(s));
for (const t of ["T1", "T2", "T3"]) await page(`__fire(['${t}'])`);
s = await state();
check("at most three cards stand at once", s.cards.length <= 3, JSON.stringify(s));

const panel = await page(`
  document.getElementById('rem-bell-btn').click(); await new Promise((r) => setTimeout(r, 200));
  const p = document.getElementById('rem-bell-panel'), r = p.getBoundingClientRect();
  return { open: r.height > 0, rows: p.querySelectorAll('.brow').length, inView: r.left >= 0 && r.right <= innerWidth,
    count: document.getElementById('rem-bell-count').hidden, all: !!p.querySelector('a[href="/utility/reminders"]') };
`);
check("the bell opens on what fired, newest first, with the way to every reminder", panel.open && panel.rows === 8 && panel.all && panel.inView,
  JSON.stringify(panel));
check("…and opening it reads them, the cards with them", panel.count === true && (await state()).cards.length === 0);
await shot("desktop-panel");
await page(`document.body.click()`);

// THE SWITCH on the Reminders tab: off is quiet, and quiet is what fires no card.
const sw = await page(`
  history.pushState({}, '', '/utility/reminders'); route(); await new Promise((r) => setTimeout(r, 600));
  const chips = () => [...document.querySelectorAll('[data-rquiet]')];
  const off = () => chips().filter((c) => !c.classList.contains('sel')).length;
  const before = [chips().length, off()];
  chips()[0].click(); await new Promise((r) => setTimeout(r, 100));
  return { before, quietNow: reminders().filter((r) => r.quiet).length };
`);
await shot("reminders-tab");
sw.cards = await page(`
  history.pushState({}, '', '/weapons'); route(); await new Promise((r) => setTimeout(r, 300));
  __fire(['T1']);
  return document.querySelectorAll('#rem-toasts .rtoast').length;
`);
check("each reminder has its pop-up switch, off for the quiet one", sw.before[0] === 7 && sw.before[1] === 1, JSON.stringify(sw));
check("…and switched off, it fires no card", sw.quietNow === 2 && sw.cards === 0, JSON.stringify(sw));

await page(`__fire(['T5'])`);
await sleep(8600);
s = await state();
check("a card leaves on its own", s.cards.length === 0, JSON.stringify(s));

// A PHONE: the bell stays on the bar, outside the folded menu, and its panel on screen.
await send("Emulation.setDeviceMetricsOverride", { width: 390, height: 800, deviceScaleFactor: 2, mobile: true });
await sleep(400);
await page(`__fire(['T4'])`);
s = await state();
check("on a phone the bell is on the bar and the card on screen", s.bellShown && s.bellInBar && s.cards.length === 1 && s.cardsInView,
  JSON.stringify(s));
await shot("phone-one");
const phonePanel = await page(`
  document.getElementById('rem-bell-btn').click(); await new Promise((r) => setTimeout(r, 200));
  const r = document.getElementById('rem-bell-panel').getBoundingClientRect();
  return { open: r.height > 0, inView: r.left >= 0 && r.right <= innerWidth };
`);
check("…and its panel opens on screen", phonePanel.open && phonePanel.inView, JSON.stringify(phonePanel));
await shot("phone-panel");

await app.finish("a fired reminder is a card in the corner and a count on the bell");
