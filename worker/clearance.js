// SPDX-License-Identifier: AGPL-3.0-or-later
// Time to process one complete workload snapshot at observed compute capacity.
export function clearanceOf(inventory, { msOf, factsOf, perHour, previousPerHour, at, sampleHours }) {
  if (!inventory) return { state: "unavailable", at };
  let work = 0, priced = 0, unpriced = 0;
  for (const [verb, count] of Object.entries(inventory.verbs)) {
    if (!count) continue;
    if (!(Number.isFinite(msOf[verb]) && Number.isFinite(factsOf[verb]) && msOf[verb] > 0 && factsOf[verb] > 0)) { unpriced += count; continue; }
    work += count * msOf[verb] / factsOf[verb];
    priced += count;
  }
  const rates = [perHour, previousPerHour].filter(n => Number.isFinite(n) && n > 0);
  const active = Number.isFinite(perHour) && perHour > 0;
  const hours = active ? work / perHour : null;
  const range = active ? [work / Math.max(...rates), work / Math.min(...rates)] : null;
  return { state: !inventory.total ? "empty" : !active ? "no_capacity" : !priced ? "no_samples" : unpriced ? "partial" : "ready",
    at, engine: inventory.engine, snapshot: inventory.snapshot, started_at: inventory.started_at,
    total: inventory.total, priced, unpriced, sample_hours: sampleHours,
    hours: !inventory.total ? 0 : unpriced ? null : hours, known_hours: hours, range_hours: range };
}
