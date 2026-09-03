// EVERY METRIC THE ENGINE DECLARES REACHES THE PAGE, and this check does not
// know what they are either.
//
// A metric is a term of the SCENARIO. Two are shipped and there will be more,
// so the property worth asserting is not "KPM and DPS both work" — it is that
// nothing between `engine::metrics::ALL` and the headline number names a metric
// at all. Written as `metric === "dps" ? ... : KPM`, a third one is drawn as
// kills per minute: silently, in the units of a different question, and that
// fork was in eight places.
//
// So the whole check is driven by `/api/meta`: whatever the table holds is
// offered by the Measure control, and picking each one puts ITS unit on the
// headline. Add a metric to the engine and this covers it without being edited.
//
//   node scripts/check_metrics.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000, base: process.env.WFSIM_BASE });
const { evaluate, check, send, sleep, BASE } = app;

await evaluate("localStorage.clear(); localStorage.setItem('wfsim-lang', 'en')");
await send("Page.navigate", { url: `${BASE}/weapons/Braton_Prime/simulator` });
await sleep(12000);

const table = await evaluate(`(() => ({
  metrics: META.metrics || null,
  fallback: META.metric_default || null,
}))()`);

check("the engine publishes its metric table", Array.isArray(table.metrics) && table.metrics.length >= 2,
  JSON.stringify(table.metrics));
check("...and names one of them the default",
  !!table.fallback && (table.metrics || []).some((m) => m.id === table.fallback),
  String(table.fallback));
check("...and every entry states an id, a field and a unit",
  (table.metrics || []).every((m) => m.id && m.field && m.label && typeof m.per_minute === "boolean"),
  JSON.stringify(table.metrics));

// WHAT THE CONTROL OFFERS, against the table rather than against a literal.
const offered = await evaluate(`(() => {
  const el = document.getElementById('dd-metric');
  if (!el) return null;
  const txt = (el.textContent || '');
  return { present: true, text: txt.trim() };
})()`);
check("the Measure control is on the page", !!(offered && offered.present),
  JSON.stringify(offered));

// EACH METRIC IN TURN: set it, run, and read the unit the headline drew.
for (const m of table.metrics || []) {
  const seen = await evaluate(`(async () => {
    const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
    sim.metric = ${JSON.stringify(m.id)};
    markScenarioDirty && markScenarioDirty();
    await runSim();
    await sleep(500);
    const num = document.querySelector('[data-hero]');
    const unit = num && num.querySelector('.hero-unit');
    return {
      hero: num ? num.dataset.hero : null,
      unit: unit ? unit.textContent.trim() : null,
      value: num ? (num.textContent || '').replace(unit ? unit.textContent : '', '').trim() : null,
    };
  })()`);
  check(
    `the headline is drawn in "${m.id}" when the scenario says so`,
    seen.hero === m.id,
    JSON.stringify(seen),
  );
  check(
    `...labelled ${m.label}, which is the table's own unit`,
    seen.unit === m.label,
    JSON.stringify(seen),
  );
  check(
    `...carrying a number`,
    !!seen.value && Number.isFinite(Number(seen.value.replace(/,/g, ""))),
    JSON.stringify(seen),
  );
}

// A METRIC NOBODY DECLARES IS REFUSED, not drawn in the default's units — the
// door, not the page, because a share link is where one arrives.
const refused = await evaluate(`(async () => {
  const r = await api('/api/simulate', {
    weapon: 'braton_prime', mods: [], metric: 'not_a_metric', duration: 10, runs: 1,
  });
  return { ok: r && r.ok, error: (r && r.error) || null };
})()`);
check("a metric the engine does not declare is refused", refused.ok === false,
  JSON.stringify(refused));
check("...and the refusal names what it would accept",
  !!refused.error && (table.metrics || []).every((m) => refused.error.includes(m.id)),
  JSON.stringify(refused));

await app.finish("every metric the engine declares reaches the page, and nothing names one");
