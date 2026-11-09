// The community goal reads the current queue, without a library census.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 11000, base: process.env.WFSIM_BASE, lang: "en" });
try {
  const now = Date.now(), engine = await app.evaluate('ENGINE_ID');
  const goal = { v: 1, at: new Date(now - 60_000).toISOString(), started_at: new Date(now - 3600000).toISOString(),
    expires_at: new Date(now + 2 * 3600000).toISOString(), snapshot: '100:100:', engine, total: 1000, registered: 20,
    orders: [{ kind: "board", total: 1000 }], verbs: { simulate: 1000 }, clear_hours: 7.7 };
  const demand = { ok: true, at: new Date(now - 30_000).toISOString(), owed: { new_builds: 20 },
    orders: [{ kind: "new_build", total: 20, dispatched: 10 }, { kind: "future_scope", total: 3, dispatched: 0 }],
    goal, per_hour: { volunteers: 100, official: 10 }, clear_hours: 0.1, surveys: [] };
  const draw = async d => app.evaluate(`(() => {
    computeDemand = ${JSON.stringify(d)};
    const box = document.createElement('div'); box.innerHTML = computeDemandHtml();
    const row = [...box.querySelectorAll('.kv')].find(k => k.querySelector('dt').textContent === 'Total orders');
    return { total: row?.querySelector('b')?.textContent,
      rows: [...box.querySelectorAll('.order-list li')].map(li => li.textContent.trim()),
      note: box.querySelector('.bh .set-note')?.textContent,
      estimate: [...box.querySelectorAll('.kv')].find(k => k.querySelector('dt').textContent === 'Cleared in')?.textContent.trim() || null };
  })()`);
  let state = await draw(demand);
  app.check("total orders sums actual queued work, excluding prospective inventory", state.total === "23", JSON.stringify(state));
  app.check("queued task kinds are listed and unknown kinds retain their ids",
    state.rows.join('|') === "new builds 20|future_scope 3", JSON.stringify(state));
  const updated = await app.evaluate(`new Date(${JSON.stringify(demand.at)}).toLocaleString(accountLocale())`);
  app.check("the queue reading supplies its update time and clearance estimate",
    state.note === 'Updated ' + updated && state.estimate?.includes('6 min'), JSON.stringify(state));
  const minutes = await app.evaluate(`[0.4, 1, 59.6, 1439.6, 3064].map(m => demandHours(m / 60))`);
  app.check("clearance uses whole minutes and carries hours and days",
    minutes.join('|') === 'under a minute|1 min|1 h 0 min|1 d 0 h 0 min|2 d 3 h 4 min', JSON.stringify(minutes));
  state = await draw({ ...demand, orders: [{ kind: 'new_build', total: 200, dispatched: 100 }] });
  app.check("new queue entries increase the total", state.total === '200', JSON.stringify(state));
  state = await draw({ ...demand, orders: [{ kind: 'new_build', total: 15, dispatched: 10 }] });
  app.check("completed or cancelled queue entries reduce the total", state.total === '15', JSON.stringify(state));
  state = await draw({ ...demand, orders: demand.orders.map(o => ({ ...o, dispatched: o.total })) });
  app.check("leased work remains pending and is counted once", state.total === '23', JSON.stringify(state));
  for (const inventory of [null, { ...goal, engine: '95d73da96acb' }, { ...goal, expires_at: new Date(now - 1).toISOString() }, { total: 999999 }]) {
    state = await draw({ ...demand, goal: inventory, last_complete: inventory });
    app.check("missing, expired, different-engine or malformed inventory cannot hide the queue",
      state.total === '23' && state.note === 'Updated ' + updated, JSON.stringify(state));
  }
  state = await draw({ ...demand, orders: [], clear_hours: 7 });
  app.check("an empty queue is zero even while inventory still has work", state.total === '0' && !state.estimate && !state.rows.length, JSON.stringify(state));
  for (const orders of [undefined, null, [null], [{ kind: 'new_build', total: -1 }], [{ kind: 'new_build', total: '20' }]]) {
    state = await draw({ ...demand, orders });
    app.check("a missing or invalid queue reading cannot invent a zero or use inventory", state.total === '—' && !state.estimate && !state.rows.length, JSON.stringify(state));
  }
  for (const clear_hours of [null, -1, '1']) {
    state = await draw({ ...demand, clear_hours });
    app.check("missing or invalid queue cost estimates do not affect the count", state.total === '23' && !state.estimate, JSON.stringify(state));
  }
  const transport = await app.evaluate(`(async () => {
    const loaders = [fetchJson, accountCall], saved = computeDemand;
    const current = ${JSON.stringify(demand)};
    try {
      computeDemand = current;
      fetchJson = accountCall = async () => null;
      await loadComputeDemand(); const retained = computeDemand === current;
      const replacement = { ...current, orders: [] };
      accountCall = async () => replacement;
      await loadComputeDemand();
      return { retained, updated: computeDemand === replacement };
    } finally { [fetchJson, accountCall] = loaders; computeDemand = saved; }
  })()`);
  app.check("a failed refresh retains the queue reading and a later API fallback replaces it", transport.retained && transport.updated, JSON.stringify(transport));
  const localized = await app.evaluate(`(async () => {
    const language = LANG, overlay = I18N;
    try {
      const all = await api('/api/i18n'); LANG = 'zh'; I18N = all.zh;
      computeDemand = ${JSON.stringify({ ...demand, goal: null })};
      const box = document.createElement('div'); box.innerHTML = computeDemandHtml();
      return { text: box.textContent, note: box.querySelector('.bh .set-note')?.textContent,
        durations: [1, 60, 3064].map(m => demandHours(m / 60)),
        expected: '更新于 ' + new Date(${JSON.stringify(demand.at)}).toLocaleString(accountLocale()) };
    } finally { LANG = language; I18N = overlay; }
  })()`);
  app.check("the queue count and update time render in Chinese", localized.note === localized.expected
    && localized.text.includes('23') && localized.text.includes('future_scope 3'), JSON.stringify(localized));
  app.check("Chinese clearance spells out days, hours and minutes",
    localized.durations.join('|') === '1 分钟|1 小时 0 分钟|2 天 3 小时 4 分钟'
    && localized.text.includes('6 分钟'), JSON.stringify(localized));
} catch (error) { app.check("queue demand renders without errors", false, String(error)); }
finally { await app.finish("total orders follows the current queue"); }
