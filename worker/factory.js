// SPDX-License-Identifier: AGPL-3.0-or-later
// THE SITE'S FACT FACTORY (worker/tasks.js): its verbs, the deadlines that ARE
// the dispatch policy, and the producers that ask. A new kind of task is one more
// producer here and in its own file; worker/tasks.js does not change.
import { factory, canon } from "./tasks.js";
import { RIVEN_PRODUCERS } from "./appraise.js";
import { CLIENTS_PER_FACT, SPOT_SHARE } from "./verify.js";
import { cloudTell } from "./cloud.js";
import { iso } from "./instant.js";

/// HOW LONG EACH ASK MAY WAIT, which orders every hand-out: earliest deadline
/// first, so a survey shape a day overdue goes before a new build's row due in an
/// hour, and nothing waits for ever behind a kind that keeps arriving.
export const DEADLINE_MS = {
  chat: 2 * 60_000,
  new_build: 3_600_000,
  survey: 86_400_000,
  sweep: 7 * 86_400_000,
};

/// HOW A COMPUTER ANSWERS. The canonical answer is all two equal answers share —
/// and so all of it must be deterministic under every split of the work
/// (`a_search_costs_the_same_work_however_the_fleet_splits_it`).
export const VERBS = {
  // A riven gain: the quick search's best build, its score and the search's work,
  // leased a quarter of an hour and renewed while it runs. Three answers without two agreeing is a dispute. No spot checks yet: no
  // official machine runs a search, and a check nobody makes is a question left waiting.
  optimize: { cap: 3, lease_ms: 15 * 60_000, spot: 0, canon: (r, work) => canon({ build: r.build, score: r.score, work }) },
};

let made = null;
/// THE FACTORY, made on first use: its producers' modules import this one.
export function facts() {
  made ||= factory({ verbs: VERBS, producers: { ...RIVEN_PRODUCERS }, threshold: CLIENTS_PER_FACT, spot: SPOT_SHARE });
  return made;
}

/// THE WATCHDOG'S SCHEDULE (wrangler.jsonc `triggers`), and how long one cause
/// stays said before it is said again.
export const WATCHDOG_CRON = "*/10 * * * *";
const RETELL_MS = 6 * 3_600_000;

/// WHAT IS WRONG IS SAID TO THE OWNER (worker/tasks.js `audit`), once a cause
/// until it has stood for `RETELL_MS`: a queue that stops moving is found by
/// this, within the hour, and not by a player asking why.
export async function watchdog(env, now = Date.now()) {
  if (!env.LIBRARY) return [];
  const found = await facts().audit(env, now);
  const db = env.LIBRARY;
  for (const f of found) {
    const told = await db.prepare("SELECT told_at FROM watchdog_told WHERE cause = ?").bind(f.kind).first();
    if (told && Date.parse(told.told_at) > now - RETELL_MS) continue;
    if (await cloudTell(env, `WFSim compute watchdog: ${f.kind} ${JSON.stringify(f)}`)) {
      await db.prepare(`INSERT INTO watchdog_told (cause, told_at) VALUES (?, ?)
                        ON CONFLICT (cause) DO UPDATE SET told_at = excluded.told_at`).bind(f.kind, iso(now)).run();
    }
  }
  return found;
}
