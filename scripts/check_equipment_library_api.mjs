import assert from "node:assert/strict";
import { testDb } from "./pg/test_db.mjs";
import { equipmentLibraryRoute } from "../worker/equipment_library.js";

const db = await testDb("library"), env = {LIBRARY:db};
const call = async (path, body, method = "POST") => {
  const response = await equipmentLibraryRoute(new Request("https://example.test/api/equipment/" + path,
    {method, ...(method === "POST" ? {body:JSON.stringify(body)} : {})}), env, "/api/equipment/" + path);
  return {status:response.status, value:await response.json()};
};
const build = {frame:"volt", mods:[{id:"vitality"}], operator:{school:"vazarin"}};
assert.equal((await call("submit", {kind:"warframe", build, id:"forged", name:"private label"})).value.queued, true);
const stored = await db.prepare("SELECT id, record FROM record_equipment_submissions").first();
assert.match(stored.id, /^equipment:/);
assert.notEqual(stored.id, "forged");
assert.deepEqual(JSON.parse(stored.record).build, {frame:"volt", mods:[{id:"vitality"}]});
assert.equal(JSON.parse(stored.record).name, undefined);
assert.equal((await call("submit", {kind:"weapon", build})).status, 400);
assert.equal((await call("submit", {kind:"warframe", build:[]})).status, 400);
assert.equal((await call("submit", {}, "GET")).status, 405);
assert.equal((await call("library", {kind:"warframe", equipment:"volt' OR 1=1"})).status, 400);
assert.equal((await call("library", {kind:"warframe", equipment:"volt", after:"bad"})).status, 400);
for (let i = 0; i < 52; i++) {
  const record = {kind:"warframe", equipment:i === 51 ? "mag" : "volt", build};
  await db.prepare("INSERT INTO entity_equipment_builds (id, at, record) VALUES (?, ?, ?)")
    .bind(i.toString(16).padStart(32,"0"), "2026-11-06", JSON.stringify(record)).run();
}
const first = (await call("library", {kind:"warframe", equipment:"volt"})).value;
assert.equal(first.builds.length, 50);
assert.equal(first.next, first.entity_weapon_builds.at(-1).id);
const next = (await call("library", {kind:"warframe", equipment:"volt", after:first.next})).value;
assert.equal(next.builds.length, 1); assert.equal(next.next, null);
assert.equal(new Set(first.builds.concat(next.builds).map(b => b.id)).size, 51);
assert.equal((await call("library", {kind:"companion", equipment:"volt"})).value.builds.length, 0);
console.log("equipment intake preserves build data, strips private links and ignores client ids; scoped pagination is stable");
