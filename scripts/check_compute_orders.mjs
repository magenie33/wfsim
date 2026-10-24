// THE ORDERS ON THE COMPUTE PAGE (73-compute.js `computeDemandHtml`, docs/BOARD.md
// §"Demand"): the total is every order there is now, said as what a device has
// taken and what waits on the server, over a bar of the share taken; each kind
// is a row of one list, a kind the page does not know shown by its id; an
// older object without `orders` still lists its kinds and claims no split; and
// the countdown stays.
//   node scripts/check_compute_orders.mjs [shots=<dir>]   (WFSIM_BASE to test a server)
import { openApp } from "./cdp.mjs";
import { writeFileSync } from "node:fs";
import { join } from "node:path";

const shots = (process.argv.find((a) => a.startsWith("shots=")) || "").slice(6);
const app = await openApp({ boot: 11000, base: process.env.WFSIM_BASE, lang: "en" });
const { evaluate, check, send, sleep } = app;
const page = (body) => evaluate(`(async () => { ${body} })()`);

const DEMAND = { ok: true, v: 1, at: new Date().toISOString(), computing: 76,
  owed: { new_builds: 156, rescores: 51390, sweeps: 333901, refreshes: 0 }, riven_gains: 0, surveys: [],
  orders: [{ kind: "rescore", total: 51390, dispatched: 38791 }, { kind: "sweep", total: 333901, dispatched: 21265 },
    { kind: "new_build", total: 156, dispatched: 66 }, { kind: "scene_audit", total: 40, dispatched: 0 }],
  per_hour: { volunteers: 49461, official: 494 }, clear_hours: 7.7 };

const drawn = (demand) => page(`
  computeDemand = ${JSON.stringify(demand)};
  let box = document.getElementById('orders-probe');
  if (!box) { box = document.createElement('div'); box.id = 'orders-probe'; document.querySelector('main, body').prepend(box); }
  box.innerHTML = computeDemandHtml();
  const kv = [...box.querySelectorAll('.kv')].find((k) => k.querySelector('dt').textContent === 'Total orders');
  const bar = box.querySelector('#orders-bar > div');
  return { total: kv ? kv.querySelector('b').textContent : null,
    split: kv && kv.querySelector('.set-note') ? kv.querySelector('.set-note').textContent.replace(/\\s+/g, ' ').trim() : null,
    bar: bar ? bar.style.width : null,
    rows: kv ? [...kv.querySelectorAll('.order-list li')].map((li) => [...li.children].map((c) => c.textContent.trim()).join(' ')) : [],
    queued: [...box.querySelectorAll('dt')].some((t) => t.textContent === 'Queued'),
    eta: [...box.querySelectorAll('.kv')].some((k) => k.querySelector('dt').textContent === 'Cleared in') };
`);

await sleep(1500);
let s = await drawn(DEMAND);
check("the total is every order there is now", s.total === "385,487", JSON.stringify(s));
check("...said as what a device has taken and what waits on the server", s.split === "dispatched 60,122 · waiting on the server 325,365", s.split);
check("...over a bar of the share taken", s.bar === "16%", s.bar);
check("each kind is a row of one list, with what was taken of it", s.rows.length === 4
  && s.rows[0] === "rescores after a model change 51,390 dispatched 38,791" && s.rows[2] === "new builds 156 dispatched 66", JSON.stringify(s.rows));
check("...a kind the page does not know shown by its id", s.rows[3] === "scene_audit 40 dispatched 0", s.rows[3]);
check("the old Queued row is gone, and the countdown stays", !s.queued && s.eta, JSON.stringify(s));
if (shots) {
  await send("Emulation.setDeviceMetricsOverride", { width: 390, height: 800, deviceScaleFactor: 2, mobile: true });
  await sleep(400);
  const m = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(join(shots, "orders.png"), Buffer.from(m.result.data, "base64"));
}

const { orders, ...older } = DEMAND;
s = await drawn(older);
check("an older object without the list still lists its kinds, and claims no split", s.total === "385,447" && s.split === null && s.bar === null
  && s.rows.length === 3 && s.rows[1] === "a ruler's first pass over older builds 333,901", JSON.stringify(s));

await app.close();
