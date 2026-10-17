// WHERE THE SITE TELLS THE READER SOMETHING (49-notices.js): kept under the bell
// on the bar and counted there, said in a card in the corner that leaves on its
// own — never a strip over the page. Many at once is one card, a quiet reminder
// no card at all, a message from the site a card once, and on a phone the bell
// stays on the bar.
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
// matches one, and items made to match them; the site's answer is stubbed.
await page(`
  for (const k of [REMINDERS_KEY, REMINDERS_SEEN_KEY, REMINDERS_HITS_KEY, NOTICES_READ_KEY, NOTICES_SAID_KEY, NOTICES_STATE_SEEN_KEY, NOTIFY_KEY]) localStorage.removeItem(k);
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
  window.__said = [];
  const real = window.fetch;
  window.fetch = (u, o) => String(u).includes('/api/notices/everyone')
    ? Promise.resolve(new Response(JSON.stringify({ ok: true, notices: window.__said })))
    : real(u, o);
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
      return cards.every((c) => { const r = c.getBoundingClientRect(); return r.bottom <= n.top || r.right <= n.left || r.left >= n.right; }); })() };
`);
const clearCards = () => page(`if (document.getElementById('rem-toasts')) document.getElementById('rem-toasts').replaceChildren();`);

let s = await state();
check("the bell is on the bar", s.bellShown && s.bellInBar, JSON.stringify(s));
await page(`__fire(['T1'])`);
s = await state();
check("one reminder firing is one card in the corner, and counted on the bell",
  s.cards.length === 1 && s.count === 1 && s.cardsBelowBar && s.cardsInView, JSON.stringify(s));
check("…below the bar and clear of Nona's button", s.cardsBelowBar && s.cardsClearOfNona, JSON.stringify(s));
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
await clearCards();

// WHAT THE SITE SAYS: the first answer a browser gets only counts; a message
// made after it is a card, once.
const old = new Date(Date.now() - 3600_000).toISOString();
await page(`window.__said = [{ id: 1, kind: 'announcement', audience: 'everyone', text: { en: 'Old news', zh: '旧公告' }, created_at: '${old}' }];
  await noticesLoad();`);
s = await state();
check("a browser's first answer from the site is counted, not carded", s.cards.length === 0 && s.count === 11, JSON.stringify(s));
await page(`window.__said = [{ id: 2, kind: 'announcement', audience: 'everyone', text: { en: 'Maintenance tonight', zh: '今晚维护' }, created_at: new Date().toISOString(), link: '/support' }, ...window.__said];
  await noticesLoad();`);
s = await state();
check("…and a message made after it is a card", s.cards.length === 1 && s.cards[0] === "今晚维护" && s.count === 12, JSON.stringify(s));
await page(`await noticesLoad();`);
s = await state();
check("…once, however often the site is asked", s.cards.length === 1, JSON.stringify(s));
await clearCards();

const panel = await page(`
  document.getElementById('rem-bell-btn').click(); await new Promise((r) => setTimeout(r, 200));
  const p = document.getElementById('rem-bell-panel'), r = p.getBoundingClientRect();
  const rows = [...p.querySelectorAll('.nrow')];
  return { open: r.height > 0, rows: rows.length, fresh: rows.filter((x) => x.classList.contains('fresh')).length,
    first: rows[0] && rows[0].textContent.trim().slice(0, 10), inView: r.left >= 0 && r.right <= innerWidth,
    count: document.getElementById('rem-bell-count').hidden, all: !!p.querySelector('a[href="/notifications"]') };
`);
check("the bell opens on everything, newest first, with the way to all of it", panel.open && panel.rows === 8 && panel.all && panel.inView
  && panel.first.startsWith("今晚维护"), JSON.stringify(panel));
check("…opening it reads them, and the rows still show what was new", panel.count === true && panel.fresh === 8, JSON.stringify(panel));
const filtered = await page(`
  document.querySelector('#rem-bell-panel [data-nfilter="messages"]').click(); await new Promise((r) => setTimeout(r, 100));
  const n = document.querySelectorAll('#rem-bell-panel .nrow').length;
  return { n, open: document.getElementById('rem-bell').classList.contains('open') };
`);
check("…and its filter shows the site's messages alone, the panel still open", filtered.n === 2 && filtered.open, JSON.stringify(filtered));
await shot("desktop-panel");
await page(`document.body.click(); noticesFilter = 'all';`);

// /notifications: the list, and the switches.
const pg = await page(`
  history.pushState({}, '', '/notifications'); route(); await new Promise((r) => setTimeout(r, 400));
  const rows = document.querySelectorAll('#auth-page .nrow').length;
  document.querySelector('[data-nset="messages:card"]').click(); await new Promise((r) => setTimeout(r, 100));
  return { rows, cardOff: notifyOn('messages', 'card') === false, chipOff: !document.querySelector('[data-nset="messages:card"]').classList.contains('sel') };
`);
check("/notifications lists everything", pg.rows === 12, JSON.stringify(pg));
check("…and turns a category's cards off", pg.cardOff && pg.chipOff, JSON.stringify(pg));
await shot("notifications-page");
await page(`window.__said = [{ id: 3, kind: 'announcement', audience: 'everyone', text: { en: 'Quiet one', zh: '静音的公告' }, created_at: new Date().toISOString() }, ...window.__said];
  history.pushState({}, '', '/weapons'); route(); await new Promise((r) => setTimeout(r, 300)); await noticesLoad();`);
s = await state();
check("…after which a message is counted and not carded", s.cards.length === 0 && s.count === 1, JSON.stringify(s));

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

await app.finish("what the site tells the reader is a card in the corner and a count on the bell");
