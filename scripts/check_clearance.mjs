// Clearance uses one complete inventory and two whole-day capacity windows.
import assert from "node:assert/strict";
import { clearanceOf } from "../worker/clearance.js";

const inventory = { engine: "d8787cb351c5", snapshot: "42:42:", started_at: "2026-11-01T00:00:00Z",
  total: 12, verbs: { simulate: 10, optimize: 2 } };
const sample = { msOf: { simulate: 2000, optimize: 60000 }, factsOf: { simulate: 2, optimize: 2 },
  perHour: 14000, previousPerHour: 28000, at: "2026-11-02T00:00:00Z", sampleHours: 24 };
const ready = clearanceOf(inventory, sample);
assert.equal(ready.state, "ready");
assert.equal(ready.hours, 5);
assert.deepEqual(ready.range_hours, [2.5, 5]);
assert.equal(ready.started_at, inventory.started_at);
assert.equal(ready.snapshot, inventory.snapshot);
assert.equal(ready.priced, 12);
assert.equal(ready.unpriced, 0);
const partial = clearanceOf(inventory, { ...sample, factsOf: { simulate: 2 } });
assert.equal(partial.state, "partial");
assert.equal(partial.hours, null);
assert.equal(partial.known_hours, 10000 / 14000);
assert.equal(partial.priced, 10);
assert.equal(partial.unpriced, 2);
assert.equal(clearanceOf(inventory, { ...sample, factsOf: {} }).state, "no_samples");
assert.equal(clearanceOf(inventory, { ...sample, perHour: 0 }).state, "no_capacity");
assert.equal(clearanceOf(inventory, { ...sample, perHour: 0 }).hours, null);
assert.deepEqual(clearanceOf(inventory, { ...sample, previousPerHour: 0 }).range_hours, [5, 5]);
assert.equal(clearanceOf(null, sample).state, "unavailable");
assert.equal(clearanceOf({ ...inventory, total: 0, verbs: {} }, { ...sample, perHour: 0 }).hours, 0);
assert.equal(clearanceOf({ ...inventory, total: 0, verbs: {} }, sample).state, "empty");
assert.equal(clearanceOf(inventory, { ...sample, msOf: { simulate: -1 } }).state, "no_samples");
assert.equal(clearanceOf(inventory, { ...sample, msOf: { simulate: 0, optimize: 0 } }).hours, null);
console.log("clearance preserves snapshot age, prices distinct task kinds, states missing samples and compares whole-day capacity");
