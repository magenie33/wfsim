// Audit a bounded queue page; removal rechecks the durable sources under its row lock.
import { query, close } from "./db.mjs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

export async function queueSourcePage(db, { after = "", limit = 100, apply = false } = {}) {
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 1000 || typeof after !== "string")
    throw new Error("a queue source page requires a limit of 1–1000 and a text cursor");
  const rows = (await db.prepare(`SELECT q.id,q.task_id,q.producer,compute_task_has_source(q.id) AS traced,
    to_char(clock_timestamp() AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS observed_at
    FROM state_compute_scheduling q WHERE q.state='open' AND q.id>?1::text ORDER BY q.id LIMIT ?2::bigint`)
    .bind(after, limit).all()).results;
  const counts = {};
  let removed = 0;
  for (const row of rows) {
    const count = counts[row.producer || "unknown"] ||= { traced: 0, untraceable: 0, removed: 0 };
    count[row.traced ? "traced" : "untraceable"]++;
    if (apply && !row.traced) {
      const changed = await db.prepare("SELECT compute_remove_untraced(?1::text,?2::bigint,?3::text) AS removed")
        .bind(row.id, row.task_id, row.observed_at).first();
      if (changed.removed) { removed++; count.removed++; }
    }
  }
  return { apply, scanned: rows.length, removed, counts, after: rows.at(-1)?.id || after,
    observed_at: rows.at(-1)?.observed_at || null, page_full: rows.length === limit };
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  const options = {};
  for (const arg of process.argv.slice(2)) {
    if (arg === "--apply") options.apply = true;
    else if (arg.startsWith("--limit=")) options.limit = Number(arg.slice(8));
    else if (arg.startsWith("--after=")) options.after = arg.slice(8);
    else throw new Error("usage: node scripts/compute_queue_sources.mjs [--apply] [--limit=100] [--after=id]");
  }
  const db = { prepare(text) { return { bind(...args) { return {
    all: async () => ({ results: await query(text, args) }),
    first: async () => (await query(text, args))[0] || null,
  }; } }; } };
  try { console.log(JSON.stringify(await queueSourcePage(db, options))); }
  finally { await close(); }
}
