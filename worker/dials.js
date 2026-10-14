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
};
/// …AND THE ONES A CLIENT READS, sent with every hand-out so no client
/// hard-codes them (docs/CLIENTS.md §"Values").
export const CLIENT_DIALS = ["lease_ms", "pack_ms", "pack_max", "pack_lease_ms", "packs_held", "renew_ms", "row_ceiling_ms"];

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
export const clientDials = (d) => Object.fromEntries(CLIENT_DIALS.map((k) => [k, d[k]]));
