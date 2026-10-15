// SPDX-License-Identifier: AGPL-3.0-or-later
// THE FLEET'S PULSE, read the same way every time: facts made per five minutes
// over two hours, the questions someone waits on and how long they took, the
// ones still open, and who is computing. Run it before and after any change to
// what the factory hands out — a change is judged on the whole fleet's facts,
// not only on the rows it meant to move. Every window is the DATABASE's clock.
//
//   PG_URL=… node scripts/fleet_pulse.mjs
//   node scripts/fleet_pulse.mjs --sql          the statements, for psql anywhere
//   node scripts/fleet_pulse.mjs --self-test    they run on the schema
import { query, close } from "./db.mjs";

const NOW = `(now() at time zone 'utc')`;
const AGO = (span) => `to_char(${NOW} - interval '${span}', 'YYYY-MM-DD"T"HH24:MI:SS')`;
export const PULSE = {
  facts_per_5_min: `SELECT to_char(date_trunc('hour', fact_at::timestamptz) + floor(extract(minute FROM fact_at::timestamptz) / 5) * interval '5 min', 'HH24:MI') AS at,
      count(*) AS facts FROM questions WHERE fact_at >= ${AGO("2 hours")} GROUP BY 1 ORDER BY 1`,
  waited_by_hour: `SELECT to_char(date_trunc('hour', d.asked_at::timestamptz), 'HH24:MI') AS hour, count(*) AS asked, count(q.fact_at) AS landed,
      round((percentile_cont(0.5) WITHIN GROUP (ORDER BY extract(epoch FROM (q.fact_at::timestamptz - d.asked_at::timestamptz)) / 60))::numeric, 1) AS p50_min,
      round((percentile_cont(0.9) WITHIN GROUP (ORDER BY extract(epoch FROM (q.fact_at::timestamptz - d.asked_at::timestamptz)) / 60))::numeric, 1) AS p90_min
    FROM demands d JOIN questions q ON q.id = d.question
    WHERE d.waits > 0 AND d.asked_at >= ${AGO("2 hours")} GROUP BY 1 ORDER BY 1`,
  waited_open: `SELECT count(*) AS open, min(opened_at) AS oldest,
      count(*) FILTER (WHERE opened_at < ${AGO("10 minutes")}) AS older_than_10_min
    FROM questions WHERE state = 'open' AND waits > 0`,
  fleet: `SELECT at, open, urgent, asking, computing FROM factory_snapshot`,
};

async function run(q) {
  const out = {};
  for (const [name, sql] of Object.entries(PULSE)) out[name] = await q(sql);
  return out;
}

const mode = process.argv[2];
if (mode === "--sql") {
  for (const [name, sql] of Object.entries(PULSE)) console.log(`-- ${name}\n${sql};\n`);
} else if (mode === "--self-test") {
  const { testDb } = await import("./pg/test_db.mjs");
  const db = await testDb("library");
  let bad = 0;
  for (const [name, sql] of Object.entries(PULSE)) {
    try { await db.all(sql); console.log(`  ok    ${name} runs on the schema`); }
    catch (e) { bad++; console.log(`  FAIL  ${name}: ${e.message}`); }
  }
  console.log(bad ? `\n${bad} failed` : "\nthe pulse reads the schema it is run on");
  process.exit(bad ? 1 : 0);
} else {
  const r = await run((sql) => query(sql));
  await close();
  for (const [name, rows] of Object.entries(r)) {
    console.log(`\n${name}`);
    if (!rows.length) { console.log("  (none)"); continue; }
    const cols = Object.keys(rows[0]);
    console.log("  " + cols.join("\t"));
    for (const row of rows) console.log("  " + cols.map((c) => row[c] ?? "").join("\t"));
  }
}
