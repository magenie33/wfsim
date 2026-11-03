// Signed interactions and the common clan application path, with no network.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import discord from "../worker/discord.js";
import { requestClanInvite } from "../worker/clan_request.js";

const keys = await crypto.subtle.generateKey("Ed25519", true, ["sign", "verify"]);
const publicKey = Buffer.from(await crypto.subtle.exportKey("raw", keys.publicKey)).toString("hex");
const told = [], receipts = [], pending = [];
let allowance = 2, serviceWorks = true;
const env = { DISCORD_APPLICATION: "1554007319967436875", DISCORD_GUILD: "1532898211663057138",
  DISCORD_PUBLIC_KEY: publicKey, CLAN_LIMIT: { limit: async () => ({ success: allowance-- > 0 }) },
  CLOUD: { fetch: async (request) => {
    assert.equal(new URL(request.url).pathname, "/internal/tell");
    told.push(await request.json());
    return Response.json({ ok: serviceWorks });
  } },
};
globalThis.fetch = async (url, init) => {
  assert.match(url, /^https:\/\/discord\.com\/api\/v10\/webhooks\/\d+\/test-token\/messages\/@original$/);
  receipts.push(JSON.parse(init.body));
  return Response.json({ id: "receipt" });
};
const ctx = { waitUntil: (promise) => pending.push(promise) };
const base = { application_id: env.DISCORD_APPLICATION, guild_id: env.DISCORD_GUILD,
  member: { user: { id: "605578251485708309", username: "tenno" } },
  token: "test-token", locale: "zh-CN" };
async function request(interaction, options = {}) {
  const body = JSON.stringify(interaction), timestamp = options.timestamp || String(Math.floor(Date.now() / 1000));
  const signature = Buffer.from(await crypto.subtle.sign("Ed25519", keys.privateKey,
    new TextEncoder().encode(timestamp + body))).toString("hex");
  return discord.fetch(new Request("https://discord.example/interactions", { method: "POST",
    body: options.body || body, headers: { "x-signature-ed25519": signature,
      "x-signature-timestamp": timestamp, ...options.headers } }), env, ctx);
}
const join = { ...base, type: 3, data: { custom_id: "clan:join" } };
const submit = (value, id = "clan:request") => ({ ...base, type: 5, data: { custom_id: id,
  components: [{ type: 18, component: { type: 4, custom_id: "name", value } }] } });

assert.deepEqual(await (await request({ ...base, type: 1 })).json(), { type: 1 });
for (const options of [{ body: JSON.stringify({ ...join, guild_id: "attacker" }) },
  { timestamp: String(Math.floor(Date.now() / 1000) - 600) },
  { timestamp: String(Math.floor(Date.now() / 1000) + 600) },
  { headers: { "x-signature-ed25519": "00".repeat(64) } }]) {
  assert.equal((await request(join, options)).status, 401);
}
for (const changed of [{ guild_id: "another-guild" }, { application_id: "another-app" },
  { member: { user: { id: base.member.user.id }, pending: true } },
  { member: { user: { id: base.member.user.id, bot: true } } }, { member: null }]) {
  assert.equal((await request({ ...join, ...changed })).status, 403);
}
const opened = await (await request(join)).json();
assert.equal(opened.type, 9);
assert.equal(opened.data.components[0].label, "In-game name");
assert.equal(told.length, 0);
for (const value of [" ", "x".repeat(41)]) {
  const refused = await (await request(submit(value))).json();
  assert.equal(refused.type, 4);
  assert.equal(refused.data.flags, 64);
  assert.equal(told.length, 0);
}
const deferred = await (await request(submit(" Te`nno\n "))).json();
assert.deepEqual(deferred, { type: 5, data: { flags: 64 } });
await Promise.all(pending.splice(0));
assert.equal(told.length, 1);
assert.equal(told[0].text, `Clan invite: \`Tenno\` — Discord user ${base.member.user.id} (zh)`);
assert.match(receipts.at(-1).content, /^Request sent: `Tenno`/);
assert.deepEqual(receipts.at(-1).allowed_mentions, { parse: [] });
const changed = await (await request({ ...join, data: { custom_id: "clan:change" },
  message: { content: receipts.at(-1).content } })).json();
assert.equal(changed.data.custom_id, "clan:replace");
assert.equal(changed.data.components[0].component.value, "Tenno");
await request(submit("Tenn0", "clan:replace"));
await Promise.all(pending.splice(0));
assert.match(told.at(-1).text, /replaces their previous one/);
assert.match((await (await request(submit("Third"))).json()).data.content, /wait a minute/);
assert.equal(told.length, 2);
allowance = 2;
serviceWorks = false;
await request(submit("Tenno"));
await Promise.all(pending.splice(0));
assert.match(receipts.at(-1).content, /could not be sent/);
assert.doesNotMatch(receipts.at(-1).content, /Request sent/);
serviceWorks = true;
const website = await requestClanInvite(env, { name: " Ten`no ", identity: "WFSim user player", lang: "en" });
assert.equal(website.name, "Tenno");
assert.equal(told.at(-1).text, "Clan invite: `Tenno` — WFSim user player (en)");
assert.equal((await requestClanInvite({}, { name: "Tenno", identity: "WFSim user player" })).reason, "tell_failed");
assert.equal((await discord.fetch(new Request("https://discord.example/interactions"), env, ctx)).status, 405);
assert.equal((await discord.fetch(new Request("https://discord.example/other"), env, ctx)).status, 404);
const config = JSON.parse(readFileSync(new URL("../wrangler.discord.jsonc", import.meta.url), "utf8").replace(/^\/\/.*$/gm, ""));
assert.equal(config.ratelimits[0].simple.limit, 2);
assert.equal(config.services[0].service, "wfsim-cloud");
assert.ok(!config.hyperdrive && !config.d1_databases);
console.log("Discord: signed requests, guild scope, private forms/receipts, corrections, limits and shared clan delivery passed.");
