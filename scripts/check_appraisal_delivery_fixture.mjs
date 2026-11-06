import assert from "node:assert/strict";
import { appraisalDeliveryFixture } from "./appraisal_delivery_fixture.mjs";

const fixture = appraisalDeliveryFixture({ engine: "check-engine", code: "TEST7", weapon: "torid",
  request: { weapon: "torid", strategy: "quick", search_strategy: "four_elements", starts: [{ slots: [] }] } });
const grant = await fixture.submit({ builds: [], harvest: { code: "TEST7", engine: "check-engine" } });
assert.equal(grant.status, 200);
assert.match(grant.body.receipt, /^[A-Za-z0-9_-]+\.[A-Za-z0-9_-]{43}$/);
const harvest = { receipt: grant.body.receipt, question: grant.body.question, engine: grant.body.engine };
assert.equal((await fixture.submit({ builds: [{ weapon: "torid" }], harvest })).body.results[0].ok, true);
assert.equal(fixture.delivered.length, 1);
for (const wrong of [undefined, { ...harvest, receipt: harvest.receipt + "x" },
  { ...harvest, engine: "wrong-engine" }, { code: "WRONG", engine: "check-engine" }]) {
  assert.ok((await fixture.submit({ builds: [{ weapon: "torid" }], harvest: wrong })).status >= 400);
}
assert.equal((await fixture.submit({ builds: [{ weapon: "hek" }], harvest })).body.results[0].ok, false);
assert.equal(fixture.delivered.length, 1);
const leased = await fixture.submit({ builds: [], harvest: { engine: "check-engine", lease: "0".repeat(32), verifier: "a".repeat(24) } });
assert.equal(leased.status, 200);
assert.match(leased.body.receipt, /^[A-Za-z0-9_-]+\.[A-Za-z0-9_-]{43}$/);
console.log("  ok  appraisal fixture issues real signed receipts and refuses missing, forged, wrong-scope and wrong-weapon authorization");
