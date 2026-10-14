// MERIT (docs/BOARD.md §"Merit"): what an account's verified compute buys back
// from the community's compute. Minted from contribution points, never bought,
// never moved between accounts, never decaying; held under a cap that grows
// with the contribution rank, and minted at most a day's cap a UTC day.
//
//   GET  /api/account/merit  → { balance, rank, daily, cap, minted_today, auto_priority, ledger: [{ at, kind, amount, balance }] }
//   POST /api/account/merit  { auto_priority: bool } → the same
//   GET  /api/merit          → { minted, granted, spent, refunded, accounts } — sums only, never an account
import { contributorRank, standings } from "./contribution.js";
import { sessionAccount, sameSite, initiatorCode } from "./accounts.js";
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
  return { ok: true, on: !!d.merit_on, priority: !!d.merit_priority, balance: row ? Number(row.balance) : 0, ...caps,
    minted_today: row && row.day === today ? Number(row.minted_today) : 0, auto_priority: !!(row && Number(row.auto_priority)),
    ledger: l.results.map((r) => ({ at: r.at, kind: r.kind, amount: Number(r.amount), balance: Number(r.balance) })) };
}

/// THE PUBLIC LEDGER: sums by kind and how many accounts hold any, never a row.
export async function meritTotals(env) {
  if (!env.ACCOUNTS) return null;
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
    return json(await meritTotals(env));
  }
  const account = await sessionAccount(env, request);
  if (!account) return json({ ok: false, error: "not_signed_in" }, 401);
  if (request.method === "POST") {
    if (!sameSite(request)) return json({ ok: false, error: "cross_site" }, 403);
    let b;
    try { b = await request.json(); } catch (_) { return json({ ok: false, error: "not_json" }, 400); }
    if (typeof b.auto_priority !== "boolean") return json({ ok: false, error: "auto_priority" }, 400);
    // WHOSE CODE IS WHOSE is kept while the switch is on, and only then (`merit_codes`).
    const code = await initiatorCode(env, "user", account);
    await env.ACCOUNTS.batch([
      env.ACCOUNTS.prepare(`INSERT INTO merit (account, day, since, auto_priority) VALUES (?1, ?2, ?3, ?4)
          ON CONFLICT (account) DO UPDATE SET auto_priority = excluded.auto_priority`)
        .bind(account, iso(Date.now()).slice(0, 10), iso(Date.now()), b.auto_priority ? 1 : 0),
      b.auto_priority ? env.ACCOUNTS.prepare("INSERT INTO merit_codes (code, account) VALUES (?, ?) ON CONFLICT (code) DO NOTHING").bind(code, account)
        : env.ACCOUNTS.prepare("DELETE FROM merit_codes WHERE account = ?").bind(account),
    ]);
  } else if (request.method !== "GET") return json({ ok: false, error: "method" }, 405);
  return json(await mine(env, account));
}

// ---- Merit priority (docs/BOARD.md §"Merit priority") ---------------------------

/// THE ANSWERS A ROW'S PRICE IS READ FROM, back from the newest by id.
const PRICE_SCAN = 200_000;
/// WHAT A ROW COSTS: the median work of the recent paid answers on its ruler
/// and mode, in points, up to the next ten; dial `merit_price_default` where
/// none is recent.
async function pricesOf(db, keys, d) {
  const { results } = await db.prepare(`WITH s AS (SELECT question, work FROM answers
        WHERE id > (SELECT coalesce(max(id), 0) FROM answers) - ?2::bigint AND official = 0 AND credited = 1)
      SELECT k.ruler, k.mode, percentile_cont(0.5) WITHIN GROUP (ORDER BY s.work) AS w
        FROM s JOIN questions q ON q.id = s.question AND q.verb = 'simulate'
        JOIN jsonb_to_recordset((?1::text)::jsonb) AS k(ruler text, mode text)
          ON (q.request::jsonb ->> 'ruler') = k.ruler AND (q.request::jsonb ->> 'mode') = k.mode
      GROUP BY k.ruler, k.mode`).bind(JSON.stringify(keys), PRICE_SCAN).all();
  const by = new Map(results.map((r) => [`${r.ruler}|${r.mode}`, Math.max(10, Math.ceil(Number(r.w) / 1e9 / 10) * 10)]));
  return (ruler, mode) => by.get(`${ruler}|${mode}`) || d.merit_price_default;
}

/// A SIGNED-IN PLAYER'S ARRIVALS, MOVED WITH THEIR MERIT where they asked for it:
/// an account with `auto_priority` pays each row's price into a hold and the
/// row is due `merit_priority_pct` sooner — up to `merit_rows_per_hour` an
/// account, while the holds are under `merit_share_pct` of the board's live
/// rows, and as far as its balance goes; the rest go as they would. Returns the
/// asks, the moved ones with their sooner deadline. `kinds`: the board's
/// producers as an SQL list.
export async function holdPriority(env, batch, asks, kinds, now = Date.now()) {
  const d = await dialsOf(env, now);
  if (!d.merit_priority || !env.ACCOUNTS || !batch.startsWith("arrivals-")) return asks;
  const coded = [...new Set(asks.map((a) => a.initiator).filter((c) => typeof c === "string" && c.startsWith("user:")))];
  if (!coded.length) return asks;
  const A = env.ACCOUNTS, L = env.LIBRARY, at = iso(now);
  const hourNow = at.slice(0, 13);
  const { results: payers } = await A.prepare(`SELECT c.code, c.account, m.balance, CASE WHEN m.prio_hour = ?2::text THEN m.prio_rows ELSE 0 END AS used
      FROM merit_codes c JOIN merit m ON m.account = c.account
      WHERE c.code IN (SELECT jsonb_array_elements_text((?1::text)::jsonb)) AND m.auto_priority = 1 AND m.balance > 0`).bind(JSON.stringify(coded), hourNow).all();
  if (!payers.length) return asks;
  const [held, live] = await L.batch([
    L.prepare("SELECT count(*) AS n FROM merit_holds WHERE state = 'held'"),
    L.prepare(`SELECT count(*) AS n FROM demands WHERE state = 'live' AND producer IN ${kinds}`),
  ]);
  let room = Math.floor((Number(live.results[0].n) + asks.length) * d.merit_share_pct / 100) - Number(held.results[0].n);
  const price = await pricesOf(L, [...new Map(asks.filter((a) => coded.includes(a.initiator))
    .map((a) => [`${a.request.ruler}|${a.request.mode}`, { ruler: a.request.ruler, mode: a.request.mode }])).values()], d);
  const out = asks.slice();
  for (const p of payers) {
    let left = Number(p.balance), n = 0, total = 0;
    const mine = [];
    out.forEach((a, k) => {
      if (a.initiator !== p.code || room <= 0 || Number(p.used) + n >= d.merit_rows_per_hour) return;
      const cost = price(a.request.ruler, a.request.mode);
      if (cost > left) return;
      left -= cost; total += cost; n++; room--;
      const sooner = Math.round(a.due_in_ms * (1 - d.merit_priority_pct / 100));
      mine.push({ k, hold: { producer: a.producer, ref: a.ref, code: p.code, price: cost,
        natural_due: iso(now + a.due_in_ms), boosted_due: iso(now + sooner), at }, sooner });
    });
    if (!n) continue;
    const paid = await A.prepare(`WITH m AS (UPDATE merit SET balance = balance - ?2::bigint,
            prio_rows = CASE WHEN prio_hour = ?5::text THEN prio_rows ELSE 0 END + ?6::bigint, prio_hour = ?5::text
          WHERE account = ?1::text AND balance >= ?2::bigint RETURNING balance)
        INSERT INTO merit_ledger (account, at, kind, amount, balance, note) SELECT ?1::text, ?3::text, 'spend', ?2::bigint, balance, ?4::text FROM m
        RETURNING balance`).bind(p.account, total, at, `${n} rows`, hourNow, n).all();
    if (!paid.results.length) continue;
    try {
      await L.prepare(`INSERT INTO merit_holds (producer, ref, code, price, natural_due, boosted_due, at)
          SELECT producer, ref, code, price, natural_due, boosted_due, at FROM jsonb_to_recordset((?::text)::jsonb)
            AS x(producer text, ref text, code text, price bigint, natural_due text, boosted_due text, at text)`)
        .bind(JSON.stringify(mine.map((m) => m.hold))).run();
    } catch (e) {
      // NOT HELD, NOT PAID: the balance goes back and the rows go as they would.
      await A.prepare(`WITH r AS (INSERT INTO merit_ledger (account, at, kind, amount, balance, note)
            SELECT ?1::text, ?2::text, 'refund', ?3::bigint, balance + ?3::bigint, ?4::text FROM merit WHERE account = ?1::text
            ON CONFLICT (note) WHERE kind = 'refund' DO NOTHING RETURNING 1)
          UPDATE merit SET balance = balance + ?3::bigint WHERE account = ?1::text AND EXISTS (SELECT 1 FROM r)`)
        .bind(p.account, at, total, `ask:${at}:${p.code}`).run();
      continue;
    }
    for (const m of mine) out[m.k] = { ...out[m.k], due_in_ms: m.sooner };
  }
  return out;
}

/// EVERY HOLD SETTLED once its row is taken or gone (the ten-minute cron): CHARGED
/// where, when a computer first took it, an open row of its producer nobody paid
/// for was due between its new deadline and its old one — it went ahead of
/// someone — and REFUNDED where it went ahead of nobody, where its ask was
/// withdrawn, or past `merit_hold_max_ms`. A refund is one ledger row a hold.
const SETTLE_HOLDS = 500;
export async function settleMerit(env, now = Date.now()) {
  if (!env.ACCOUNTS || !env.LIBRARY) return 0;
  const d = await dialsOf(env, now), L = env.LIBRARY, at = iso(now);
  const { results } = await L.prepare(`SELECT h.id, h.code, h.price, h.at,
        (d.state IS NULL OR d.state = 'withdrawn' OR q.state = 'withdrawn' OR h.at < ?1::text) AS gone,
        (q.leased_at IS NOT NULL OR q.state <> 'open') AS taken,
        EXISTS (SELECT 1 FROM questions o WHERE o.state = 'open' AND o.producer = q.producer AND o.waits = 0
                  AND o.due_at > h.boosted_due AND o.due_at < h.natural_due AND o.opened_at <= h.at AND o.id <> q.id
                  AND NOT EXISTS (SELECT 1 FROM demands od JOIN merit_holds oh ON oh.producer = od.producer AND oh.ref = od.ref
                                  WHERE od.question = o.id AND oh.state <> 'refunded')) AS jumped
      FROM merit_holds h LEFT JOIN demands d ON d.producer = h.producer AND d.ref = h.ref LEFT JOIN questions q ON q.id = d.question
      WHERE h.state = 'held' ORDER BY h.id LIMIT ?2::bigint`).bind(iso(now - d.merit_hold_max_ms), SETTLE_HOLDS).all();
  const charge = results.filter((r) => !r.gone && r.taken && r.jumped).map((r) => r.id);
  const refund = results.filter((r) => r.gone || (r.taken && !r.jumped));
  if (charge.length) {
    await L.prepare("UPDATE merit_holds SET state = 'charged', code = '' WHERE id IN (SELECT value::bigint FROM jsonb_array_elements_text((?::text)::jsonb)) AND state = 'held'")
      .bind(JSON.stringify(charge.map(String))).run();
  }
  if (refund.length) {
    // THE LEDGER FIRST, keyed by the hold: a pass dying between the two writes refunds nothing twice.
    await env.ACCOUNTS.prepare(`WITH x AS (SELECT * FROM jsonb_to_recordset((?1::text)::jsonb) AS x(id bigint, code text, price bigint)),
        r AS (INSERT INTO merit_ledger (account, at, kind, amount, balance, note)
              SELECT c.account, ?2::text, 'refund', x.price, m.balance + x.price, 'hold:' || x.id FROM x
                JOIN merit_codes c ON c.code = x.code JOIN merit m ON m.account = c.account
              ON CONFLICT (note) WHERE kind = 'refund' DO NOTHING RETURNING account, amount),
        s AS (SELECT account, sum(amount) AS amount FROM r GROUP BY account)
      UPDATE merit m SET balance = m.balance + s.amount FROM s WHERE m.account = s.account`)
      .bind(JSON.stringify(refund.map((r) => ({ id: Number(r.id), code: r.code, price: Number(r.price) }))), at).run();
    await L.prepare("UPDATE merit_holds SET state = 'refunded', code = '' WHERE id IN (SELECT value::bigint FROM jsonb_array_elements_text((?::text)::jsonb)) AND state = 'held'")
      .bind(JSON.stringify(refund.map((r) => String(r.id)))).run();
  }
  return charge.length + refund.length;
}

