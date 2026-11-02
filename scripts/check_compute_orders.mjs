// The community goal reads complete inventory, independently of dispatch pacing.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 11000, base: process.env.WFSIM_BASE, lang: "en" });
try {
  const goal = { v: 1, at: new Date().toISOString(), expires_at: new Date(Date.now() + 3600000).toISOString(), total: 1000,
    orders: [{ kind: "board", total: 900 }, { kind: "future_scope", total: 100 }], clear_hours: 7.7 };
  const demand = { ok: true, owed: { new_builds: 20 }, orders: [{ kind: "new_build", total: 20, dispatched: 10 }],
    goal, per_hour: { volunteers: 100, official: 10 }, clear_hours: 0.1, surveys: [] };
  const draw = async d => app.evaluate(`(() => {
    computeDemand = ${JSON.stringify(d)};
    const box = document.createElement('div'); box.innerHTML = computeDemandHtml();
    const row = [...box.querySelectorAll('.kv')].find(k => k.querySelector('dt').textContent === 'Total orders');
    return { total: row?.querySelector('b')?.textContent,
      rows: [...box.querySelectorAll('.order-list li')].map(li => li.textContent.trim()),
      note: box.querySelector('.bh .set-note')?.textContent,
      eta: [...box.querySelectorAll('dt')].some(n => n.textContent === 'Cleared in') };
  })()`);
  let state = await draw(demand);
  app.check("the goal includes work not admitted to the queue", state.total === "1,000", JSON.stringify(state));
  app.check("every task kind is listed, including a newly registered kind", state.rows.join('|') === "leaderboard calculations 900|future_scope 100", JSON.stringify(state));
  app.check("the complete scan has an update time and its own clearance estimate", state.note.startsWith("Updated ") && state.eta, JSON.stringify(state));
  state = await draw({ ...demand, orders: [{ kind: "new_build", total: 200, dispatched: 100 }] });
  app.check("admitting more work does not inflate the goal", state.total === "1,000", JSON.stringify(state));
  state = await draw({ ...demand, goal: { ...goal, total: 950, orders: [{ kind: "board", total: 850 }, { kind: "future_scope", total: 100 }] } });
  app.check("confirmed completion reduces the next complete inventory", state.total === "950", JSON.stringify(state));
  for (const incomplete of [null, { ...goal, expires_at: new Date(Date.now() - 1).toISOString() }]) {
    state = await draw({ ...demand, goal: incomplete });
    app.check("an absent or expired scan cannot present the partial queue as full demand", state.total === "—" && !state.eta
      && state.rows.length === 0 && state.note === "Checking total demand", JSON.stringify(state));
  }
  state = await draw({ ...demand, goal: { ...goal, total: 0, orders: [], clear_hours: null } });
  app.check("a confirmed empty inventory is shown as zero", state.total === "0" && !state.eta, JSON.stringify(state));
} catch (error) { app.check("complete demand renders without errors", false, String(error)); }
finally { await app.finish("complete compute demand is independent of admission"); }
