// The community goal reads complete inventory, independently of dispatch pacing.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 11000, base: process.env.WFSIM_BASE, lang: "en" });
try {
  const now = Date.now(), engine = await app.evaluate('ENGINE_ID');
  const goal = { v: 1, at: new Date(now - 60_000).toISOString(), started_at: new Date(now - 3600000).toISOString(),
    expires_at: new Date(now + 2 * 3600000).toISOString(), snapshot: '100:100:', engine, total: 1000, registered: 20,
    orders: [{ kind: "board", total: 900 }, { kind: "future_scope", total: 100 }], verbs: { simulate: 1000 }, clear_hours: 7.7 };
  const demand = { ok: true, owed: { new_builds: 20 }, orders: [{ kind: "new_build", total: 20, dispatched: 10 }],
    goal, per_hour: { volunteers: 100, official: 10 }, clear_hours: 0.1, surveys: [] };
  const draw = async (d, at = now) => app.evaluate(`(() => {
    computeDemand = ${JSON.stringify(d)};
    const actualNow = Date.now;
    const box = document.createElement('div');
    try { Date.now = () => ${at}; box.innerHTML = computeDemandHtml(); } finally { Date.now = actualNow; }
    const row = [...box.querySelectorAll('.kv')].find(k => k.querySelector('dt').textContent === 'Total orders');
    return { total: row?.querySelector('b')?.textContent,
      rows: [...box.querySelectorAll('.order-list li')].map(li => li.textContent.trim()),
      note: box.querySelector('.bh .set-note')?.textContent,
      eta: [...box.querySelectorAll('dt')].some(n => n.textContent === 'Cleared in') };
  })()`);
  let state = await draw(demand);
  app.check("the goal includes work not admitted to the queue", state.total === "1,000", JSON.stringify(state));
  app.check("every task kind is listed, including a newly registered kind", state.rows.join('|') === "leaderboard calculations 900|future_scope 100", JSON.stringify(state));
  const originalNote = state.note;
  const cut = await app.evaluate(`new Date(${JSON.stringify(goal.started_at)}).toLocaleString(accountLocale())`);
  app.check("the complete scan names its original input time and its own clearance estimate", state.note === 'Last complete count · as of ' + cut && state.eta, JSON.stringify(state));
  state = await draw({ ...demand, orders: [{ kind: "new_build", total: 200, dispatched: 100 }] });
  app.check("admitting more work does not inflate the goal", state.total === "1,000", JSON.stringify(state));
  state = await draw({ ...demand, goal: { ...goal, total: 950, orders: [{ kind: "board", total: 850 }, { kind: "future_scope", total: 100 }], verbs: { simulate: 950 } } });
  app.check("confirmed completion reduces the next complete inventory", state.total === "950", JSON.stringify(state));
  for (const incomplete of [null, { ...goal, expires_at: new Date(Date.now() - 1).toISOString() }]) {
    state = await draw({ ...demand, goal: incomplete });
    app.check("an absent or expired scan cannot present the partial queue as full demand", state.total === "—" && !state.eta
      && state.rows.length === 0 && state.note === "Checking total demand", JSON.stringify(state));
  }
  state = await draw({ ...demand, last_complete: goal }, now + 3 * 3600000);
  app.check("a cached response crossing expiry retains the original same-engine complete count and timestamp without an ETA",
    state.total === '1,000' && state.note === originalNote && !state.eta && state.rows.length === 2, JSON.stringify(state));
  state = await draw({ ...demand, goal: null, last_complete: goal }, now + 3 * 3600000);
  app.check("a later demand response with no fresh goal retains that same complete count", state.total === '1,000' && state.note === originalNote && !state.eta, JSON.stringify(state));
  const clockSkew = { ...goal, at: new Date(now + 1000).toISOString() };
  state = await draw({ ...demand, goal: clockSkew });
  app.check("a valid fresh goal preserves the server's clock tolerance", state.total === '1,000' && state.eta, JSON.stringify(state));
  state = await draw({ ...demand, goal: null, last_complete: clockSkew });
  app.check("the display-only fallback still refuses a future publication", state.total === '—' && !state.eta, JSON.stringify(state));
  const obsolete = { ...goal, engine: '95d73da96acb', total: 17976524, orders: [{ kind: 'board', total: 17976524 }], verbs: { simulate: 17976524 } };
  for (const incomplete of [obsolete, { ...goal, engine: null }, { ...goal, at: new Date(now + 60_001).toISOString() },
    { ...goal, orders: [null] }, { ...goal, verbs: { simulate: 999 } }, { ...goal, registered: 1001 },
    { ...goal, expires_at: new Date(Date.parse(goal.expires_at) + 60_000).toISOString() }]) {
    state = await draw({ ...demand, goal: incomplete, last_complete: incomplete });
    app.check("an obsolete, unknown-engine, future or malformed snapshot cannot render a count", state.total === '—' && !state.eta
      && state.rows.length === 0 && state.note === 'Checking total demand', JSON.stringify(state));
  }
  state = await draw({ ...demand, goal: { ...goal, total: 0, registered: 0, orders: [], verbs: {}, clear_hours: null } });
  app.check("a confirmed empty inventory is shown as zero", state.total === "0" && !state.eta, JSON.stringify(state));
  state = await draw({ ...demand, goal: null, last_complete: { ...goal, total: 0, registered: 0, orders: [], verbs: {} } }, now + 3 * 3600000);
  app.check("an expired complete empty inventory also remains zero", state.total === "0" && state.note === originalNote && !state.eta, JSON.stringify(state));
  const stale = { ...goal, started_at: new Date(now - 5 * 3600000).toISOString(), at: new Date(now - 4 * 3600000).toISOString(), expires_at: new Date(now - 2 * 3600000).toISOString() };
  const localized = await app.evaluate(`(async () => {
    const language = LANG, overlay = I18N;
    try {
      const all = await api('/api/i18n'); LANG = 'zh'; I18N = all.zh;
      computeDemand = ${JSON.stringify({ ...demand, goal: null, last_complete: stale })};
      const box = document.createElement('div'); box.innerHTML = computeDemandHtml();
      return { note: box.querySelector('.bh .set-note')?.textContent,
        expected: '上次完整统计 · 截至 ' + new Date(${JSON.stringify(stale.started_at)}).toLocaleString(accountLocale()) };
    } finally { LANG = language; I18N = overlay; }
  })()`);
  app.check("the retained count labels its original cut in Chinese too", localized.note === localized.expected, JSON.stringify(localized));
} catch (error) { app.check("complete demand renders without errors", false, String(error)); }
finally { await app.finish("complete compute demand is independent of admission"); }
