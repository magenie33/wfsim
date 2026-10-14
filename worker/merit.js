// MERIT (docs/BOARD.md §"Merit"): what an account's verified compute buys back
// from the community's compute. Minted from contribution points, never bought,
// never moved between accounts, never decaying; held under a cap that grows
// with the contribution rank, and minted at most a day's cap a UTC day.
//
//   GET  /api/account/merit  → { balance, rank, daily, cap, minted_today, auto_priority, ledger: [{ at, kind, amount, balance }] }
//   POST /api/account/merit  { auto_priority: bool } → the same
//   GET  /api/merit          → { minted, granted, spent, refunded, accounts } — sums only, never an account
import { contributorRank, standings } from "./contribution.js";
import { sessionAccount, sameSite } from "./accounts.js";
import { dialsOf } from "./dials.js";
import { iso } from "./instant.js";

const json = (body, status = 200) => new Response(JSON.stringify(body), {
  status, headers: { "content-type": "application/json", "cache-control": "no-store" } });

/// AN ACCOUNT'S CAPS at `points`: the day's mint and the balance's, both linear
/// in the contribution rank (dials `merit_daily_base`, `merit_daily_per_rank`,
/// `merit_balance_days`).
export function meritCaps(points, d) {
  const { rank } = contributorRank(points);
  const daily = d.merit_daily_base + d.merit_daily_per_rank * rank;
  return { rank, daily, cap: daily * d.merit_balance_days };
}

/// THE MINT, every account at once (the worker's ten-minute cron): an account
/// first seen is granted all its points so far up to its balance cap; after
/// that, each pass adds the points earned since, within what the day's cap and
/// the balance cap leave. A point past either cap is still a contribution
/// point, and mints nothing later: `points_seen` moves past it.
export async function mintMerit(env, now = Date.now()) {
  if (!env.ACCOUNTS || !env.LIBRARY) return 0;
  const d = await dialsOf(env, now);
  if (!d.merit_on) return 0;
  const rows = (await standings(env)).filter((e) => e.points > 0)
    .map((e) => ({ a: e.id, p: e.points, ...meritCaps(e.points, d) })).map(({ a, p, daily, cap }) => ({ a, p, daily, cap }));
  if (!rows.length) return 0;
  const A = env.ACCOUNTS, at = iso(now), day = at.slice(0, 10), x = JSON.stringify(rows);
  const input = `x AS (SELECT * FROM jsonb_to_recordset((?1::text)::jsonb) AS x(a text, p bigint, daily bigint, cap bigint))`;
  const [granted, minted] = await A.batch([
    A.prepare(`WITH ${input},
        g AS (INSERT INTO merit AS t (account, balance, points_seen, day, minted_today, since, granted_at)
                SELECT x.a, LEAST(x.p, x.cap), x.p, ?2::text, 0, ?3::text, ?3::text FROM x JOIN accounts ac ON ac.id = x.a
              ON CONFLICT (account) DO UPDATE SET balance = excluded.balance, points_seen = excluded.points_seen, granted_at = excluded.granted_at
                WHERE t.granted_at IS NULL
              RETURNING account, balance)
      INSERT INTO merit_ledger (account, at, kind, amount, balance) SELECT account, ?3::text, 'grant', balance, balance FROM g WHERE balance > 0
      RETURNING amount`).bind(x, day, at),
    A.prepare(`WITH ${input},
        m AS (UPDATE merit m SET
                last_mint = merit_room(x.p, m.points_seen, x.daily, CASE WHEN m.day = ?2::text THEN m.minted_today ELSE 0 END, x.cap, m.balance),
                balance = m.balance + merit_room(x.p, m.points_seen, x.daily, CASE WHEN m.day = ?2::text THEN m.minted_today ELSE 0 END, x.cap, m.balance),
                minted_today = CASE WHEN m.day = ?2::text THEN m.minted_today ELSE 0 END
                  + merit_room(x.p, m.points_seen, x.daily, CASE WHEN m.day = ?2::text THEN m.minted_today ELSE 0 END, x.cap, m.balance),
                day = ?2::text, points_seen = x.p
              FROM x WHERE m.account = x.a AND m.granted_at IS NOT NULL AND x.p > m.points_seen RETURNING m.account, m.last_mint, m.balance)
      INSERT INTO merit_ledger (account, at, kind, amount, balance) SELECT account, ?3::text, 'mint', last_mint, balance FROM m WHERE last_mint > 0
      RETURNING amount`).bind(x, day, at),
  ]);
  return granted.results.length + minted.results.length;
}

/// WHAT AN ACCOUNT SEES OF ITS OWN: its balance, rank, caps and last movements.
const LEDGER_SHOWN = 50;
async function mine(env, account, now = Date.now()) {
  const d = await dialsOf(env, now), today = iso(now).slice(0, 10);
  const [m, l] = await env.ACCOUNTS.batch([
    env.ACCOUNTS.prepare("SELECT balance, points_seen, day, minted_today, auto_priority FROM merit WHERE account = ?").bind(account),
    env.ACCOUNTS.prepare("SELECT at, kind, amount, balance FROM merit_ledger WHERE account = ? ORDER BY id DESC LIMIT ?").bind(account, LEDGER_SHOWN),
  ]);
  const row = m.results[0] || null;
  const caps = meritCaps(row ? Number(row.points_seen) : 0, d);
  return { ok: true, on: !!d.merit_on, balance: row ? Number(row.balance) : 0, ...caps,
    minted_today: row && row.day === today ? Number(row.minted_today) : 0, auto_priority: !!(row && Number(row.auto_priority)),
    ledger: l.results.map((r) => ({ at: r.at, kind: r.kind, amount: Number(r.amount), balance: Number(r.balance) })) };
}

/// THE PUBLIC LEDGER: sums by kind and how many accounts hold any, never a row.
async function totals(env) {
  const { results } = await env.ACCOUNTS.prepare("SELECT kind, coalesce(sum(amount), 0) AS n FROM merit_ledger GROUP BY kind").all();
  const by = Object.fromEntries(results.map((r) => [r.kind, Number(r.n)]));
  const held = await env.ACCOUNTS.prepare("SELECT count(*) AS n, coalesce(sum(balance), 0) AS b FROM merit WHERE balance > 0").first();
  return { ok: true, granted: by.grant || 0, minted: by.mint || 0, spent: by.spend || 0, refunded: by.refund || 0,
    held: Number(held.b), accounts: Number(held.n) };
}

export async function meritRoute(request, env, path) {
  if (!env.ACCOUNTS) return json({ ok: false, error: "not_configured" }, 503);
  if (path === "/api/merit") {
    if (request.method !== "GET") return json({ ok: false, error: "method" }, 405);
    return json(await totals(env));
  }
  const account = await sessionAccount(env, request);
  if (!account) return json({ ok: false, error: "not_signed_in" }, 401);
  if (request.method === "POST") {
    if (!sameSite(request)) return json({ ok: false, error: "cross_site" }, 403);
    let b;
    try { b = await request.json(); } catch (_) { return json({ ok: false, error: "not_json" }, 400); }
    if (typeof b.auto_priority !== "boolean") return json({ ok: false, error: "auto_priority" }, 400);
    await env.ACCOUNTS.prepare(`INSERT INTO merit (account, day, since, auto_priority) VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT (account) DO UPDATE SET auto_priority = excluded.auto_priority`)
      .bind(account, iso(Date.now()).slice(0, 10), iso(Date.now()), b.auto_priority ? 1 : 0).run();
  } else if (request.method !== "GET") return json({ ok: false, error: "method" }, 405);
  return json(await mine(env, account));
}
