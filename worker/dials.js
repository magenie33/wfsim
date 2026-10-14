// SPDX-License-Identifier: AGPL-3.0-or-later
// THE FACTORY'S DIALS — docs/BOARD.md §"The dials". Every number that decides how
// work is handed out is set by the owner in `factory_dials` and logged in
// `factory_dial_log`; the code holds only its default and its bounds, so a dial
// moves without a deploy and a value outside its bounds is never used.
// `CLIENTS_PER_FACT` is not a dial: two owners per fact is the board's promise,
// not a capacity knob (worker/verify.js).
import { iso } from "./instant.js";

/// EVERY DIAL, its default (what the code did before it was a dial), its bounds
/// and what it decides. Whole numbers all; `_ms` are milliseconds.
export const DIALS = {
  lease_ms: { default: 1_800_000, min: 600_000, max: 7_200_000, unit: "ms",
    says: "how long a protocol 6 row, or a question that goes alone, is leased" },
  pack_ms: { default: 60_000, min: 10_000, max: 1_800_000, unit: "ms",
    says: "how much of a computer's time one pack is, by its reported pace" },
  pack_first: { default: 3, min: 1, max: 20, unit: "rows",
    says: "the pack of a computer whose pace is not known yet" },
  pack_max: { default: 20, min: 1, max: 200, unit: "rows",
    says: "the most rows one pack holds" },
  pack_lease_ms: { default: 300_000, min: 60_000, max: 1_800_000, unit: "ms",
    says: "how long a pack is leased before its rows must be renewed" },
  packs_held: { default: 2, min: 1, max: 4, unit: "packs",
    says: "how many packs one computer holds at once, asking ahead included" },
  renew_ms: { default: 300_000, min: 60_000, max: 1_800_000, unit: "ms",
    says: "how far one renewal moves a row's lease" },
  row_ceiling_ms: { default: 7_200_000, min: 1_800_000, max: 86_400_000, unit: "ms",
    says: "how long a row is one computer's before another may take it too" },
  asked_every_ms: { default: 60_000, min: 10_000, max: 600_000, unit: "ms",
    says: "how often an ask is written down as a computer being on" },
  compute_cap: { default: 400, min: 0, max: 1_000_000, unit: "computers",
    says: "past this many asking, one that answered nothing lately waits; 0 is no cap" },
  // PROTOCOL 8 (worker/sync.js, docs/CLIENTS.md §"The cycle, protocol 8").
  pack8_ms: { default: 600_000, min: 60_000, max: 3_600_000, unit: "ms",
    says: "how much of a computer's time one protocol 8 pack is, while work is plentiful" },
  pack8_max: { default: 200, min: 1, max: 500, unit: "rows",
    says: "the most rows one protocol 8 pack holds" },
  sync_lease_ms: { default: 1_500_000, min: 300_000, max: 7_200_000, unit: "ms",
    says: "how long a protocol 8 pack is leased while work is plentiful; short work leases pack_lease_ms" },
  sync_low_ms: { default: 120_000, min: 10_000, max: 1_800_000, unit: "ms",
    says: "a protocol 8 client syncs for more once this little of its work is left" },
  sync_floor_ms: { default: 60_000, min: 5_000, max: 3_600_000, unit: "ms",
    says: "the soonest a protocol 8 client is handed work again after a sync that handed it some" },
  sync_idle_ms: { default: 20_000, min: 5_000, max: 3_600_000, unit: "ms",
    says: "the soonest a protocol 8 client is handed work again after a sync that found none" },
  flush_ms: { default: 600_000, min: 60_000, max: 3_600_000, unit: "ms",
    says: "a protocol 8 client sends the results it keeps at least this often" },
  poll_ms: { default: 10_000, min: 2_000, max: 600_000, unit: "ms",
    says: "how often an idle protocol 8 client reads work.json" },
  jitter_ms: { default: 5_000, min: 0, max: 120_000, unit: "ms",
    says: "the most an idle client waits at random before syncing when work.json shows work" },
  urgent_jitter_ms: { default: 2_000, min: 0, max: 60_000, unit: "ms",
    says: "the same wait when work.json shows a question someone waits on" },
  report_ms: { default: 3_600_000, min: 0, max: 86_400_000, unit: "ms",
    says: "how often an idle protocol 8 client syncs only to say it is on; 0 never" },
  sync_scale_pct: { default: 100, min: 50, max: 1_000, unit: "%",
    says: "every next_ms is multiplied by this: one dial to slow every client down" },
  refusals_to_withdraw: { default: 3, min: 1, max: 20, unit: "owners",
    says: "this many owners refusing a question on the served engine withdraw it" },
  sync_off: { default: 0, min: 0, max: 1, unit: "switch",
    says: "1 answers every protocol 8 sync with fallback 7" },
  sync_proc: { default: 0, min: 0, max: 1, unit: "switch",
    says: "1: a protocol 8 sync's database work is one CALL board_sync (worker/pg/sync.sql); 0: the worker's statements one by one" },
  signal_mode: { default: 0, min: 0, max: 1, unit: "mode",
    says: "0: computers wait on the signal; 1: they ask the worker directly, the signal unused" },
  // THE SETTLER (worker/settler.js `settleBatch`, docs/BOARD.md §"The settler").
  settle_async: { default: 0, min: 0, max: 1, unit: "switch",
    says: "1: a volunteer's board answers are queued and the settler beside the database settles them; 0: settled in the request" },
  settle_grace_ms: { default: 120_000, min: 10_000, max: 3_600_000, unit: "ms",
    says: "a queued question is handed to nobody for this long, then to anyone again should the settler not have come" },
  settle_batch: { default: 200, min: 1, max: 2_000, unit: "questions",
    says: "the most queued questions one settler pass takes" },
  // WHO ASKED (docs/BOARD.md §"Who asked"): read by worker/pg/sync.sql `factory_capped` too.
  initiator_share_pct: { default: 0, min: 0, max: 100, unit: "%",
    says: "one person or chat holding more than this share of the live leases is passed over until it holds less; 0 off" },
  initiator_floor_rows: { default: 50, min: 1, max: 100_000, unit: "rows",
    says: "nobody is passed over for their share while holding this many leases or fewer" },
  initiator_forget_ms: { default: 600_000, min: 60_000, max: 86_400_000, unit: "ms",
    says: "how often the settler erases the code of an ask that is over" },
  // MERIT (docs/BOARD.md §"Merit"), worker/merit.js.
  merit_on: { default: 1, min: 0, max: 1, unit: "switch",
    says: "1 mints merit from contribution points each ten-minute cron; 0 mints nothing (balances stay)" },
  merit_daily_base: { default: 16_000, min: 0, max: 10_000_000, unit: "merit",
    says: "the most merit an account mints a UTC day, at rank 0" },
  merit_daily_per_rank: { default: 500, min: 0, max: 1_000_000, unit: "merit",
    says: "…and this much more per contribution rank" },
  merit_balance_days: { default: 10, min: 1, max: 365, unit: "days",
    says: "the balance an account may hold, in days of its daily cap" },
  // THE PUBLIC STATS (docs/BOARD.md §"The public stats"), written to the public bucket.
  stats_tally_ms: { default: 60_000, min: 10_000, max: 3_600_000, unit: "ms",
    says: "how often the settler writes stats/tally.json, the home page's count" },
  stats_contributors_ms: { default: 600_000, min: 600_000, max: 86_400_000, unit: "ms",
    says: "how often the worker's ten-minute cron writes stats/contributors.json, the ranking" },
};
/// …AND THE ONES A CLIENT READS, sent with every hand-out so no client
/// hard-codes them (docs/CLIENTS.md §"Values").
export const CLIENT_DIALS = ["lease_ms", "pack_ms", "pack_max", "pack_lease_ms", "packs_held", "renew_ms", "row_ceiling_ms"];
/// …A PROTOCOL 8 CLIENT'S, in every sync reply and in work.json.
export const SYNC_CLIENT_DIALS = ["pack8_ms", "pack8_max", "sync_lease_ms", "pack_lease_ms", "sync_low_ms", "sync_floor_ms",
  "sync_idle_ms", "flush_ms", "packs_held", "row_ceiling_ms", "poll_ms", "jitter_ms", "urgent_jitter_ms", "report_ms"];

export const DIAL_DEFAULTS = Object.freeze(Object.fromEntries(Object.entries(DIALS).map(([k, d]) => [k, d.default])));
const inBounds = (name, v) => Number.isSafeInteger(v) && v >= DIALS[name].min && v <= DIALS[name].max;

/// HOW LONG ONE ISOLATE KEEPS WHAT IT READ: a change is in force within this.
export const DIALS_FRESH_MS = 60_000;
let held = null;
/// THE DIALS AS SET, read at most once a `DIALS_FRESH_MS` per isolate. A dial
/// never set, unreadable, or set out of its bounds is its default, so a missing
/// table or a bad row changes nothing.
export async function dialsOf(env, now = Date.now()) {
  if (held && now - held.at < DIALS_FRESH_MS) return held.values;
  const values = { ...DIAL_DEFAULTS };
  try {
    const { results } = await env.LIBRARY.prepare("SELECT name, value FROM factory_dials").all();
    for (const r of results) if (DIALS[r.name] && inBounds(r.name, Number(r.value))) values[r.name] = Number(r.value);
  } catch { /* defaults */ }
  held = { at: now, values: Object.freeze(values) };
  return held.values;
}
/// What this isolate read is dropped: the next read is the table's.
export const forgetDials = () => { held = null; };

/// SET A DIAL, and log it, in one transaction. Refused — nothing written — for
/// a name that is not a dial or a value outside its bounds.
export async function setDial(env, name, value, by, now = Date.now()) {
  if (!DIALS[name]) return { ok: false, error: "not a dial" };
  if (!inBounds(name, value)) return { ok: false, error: `outside ${DIALS[name].min}..${DIALS[name].max}` };
  if (typeof by !== "string" || !by) return { ok: false, error: "who set it is required" };
  const db = env.LIBRARY, at = iso(now);
  await db.batch([
    db.prepare(`INSERT INTO factory_dial_log (name, value_before, value_after, set_by, set_at)
        VALUES (?1::text, (SELECT value FROM factory_dials WHERE name = ?1::text), ?2::double precision, ?3::text, ?4::text)`)
      .bind(name, value, by, at),
    db.prepare(`INSERT INTO factory_dials (name, value, set_by, set_at) VALUES (?1::text, ?2::double precision, ?3::text, ?4::text)
        ON CONFLICT (name) DO UPDATE SET value = excluded.value, set_by = excluded.set_by, set_at = excluded.set_at`)
      .bind(name, value, by, at),
  ]);
  forgetDials();
  return { ok: true, name, value };
}

/// THE DIALS A CLIENT IS TOLD, from what is in force.
export const clientDials = (d, names = CLIENT_DIALS) => Object.fromEntries(names.map((k) => [k, d[k]]));
