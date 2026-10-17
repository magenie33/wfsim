// THE MCP SERVER'S DOOR: who is calling, and how often (docs/AGENT.md §"The MCP
// server"). `mcp/index.js` itself, with its engine stubbed — nothing here runs a
// query — the site's worker stubbed at `SITE`, and each limiter a counter.
// What it holds: the server describes itself as a protected resource, a tool
// call without a key spends the address's allowance and with one the key's, the
// handshake and the list spend nothing, a key that is not one is refused rather
// than read as none, and a key is looked up once and not on every call.
// No network, no browser.
import { register } from "node:module";

register("data:text/javascript," + encodeURIComponent(`
export async function resolve(spec, ctx, next) {
  if (spec === "./engine.js" && ctx.parentURL && ctx.parentURL.endsWith("/mcp/index.js")) {
    return { url: "data:text/javascript," + encodeURIComponent(
      "const META = { weapons: [{ id: 'torid', name: 'Torid', evolutions: [], arcane_pools: ['primary'] }]," +
      " mod_pools: { rifle: [{ id: 'serration', max_rank: 10 }, { id: 'split_chamber', max_rank: 5 }] } };" +
      "export const wasm_bindgen = { initSync() {}, install_data() { return ''; }, api(path) { return path === '/api/meta' ? JSON.stringify(META) : '{}'; } };" +
      "export const module = null; export const ENGINE_DIGEST = 'test'; export const ENGINE_PACKS = [];"), shortCircuit: true };
  }
  return next(spec, ctx);
}`));

const { default: mcp } = await import("../mcp/index.js");

let failed = 0;
const check = (name, ok, detail = "") => {
  console.log(`${ok ? "  ok" : "FAIL"}  ${name}${ok ? "" : `  — ${String(detail).slice(0, 300)}`}`);
  if (!ok) failed++;
};

const limiter = (limit) => {
  const used = new Map();
  return { used, limit: async ({ key }) => { used.set(key, (used.get(key) || 0) + 1); return { success: used.get(key) <= limit }; } };
};
let lookups = 0;
// THE SYNC STORE, as the site forwards it: a claimed key is its member, an
// unclaimed one nobody, and `member` says whether the account holds the feature.
const store = new Map();
let member = true;
const synced = [];
async function cloud(req) {
  const b = JSON.parse(await req.text());
  const reply = (j, status = 200) => new Response(JSON.stringify(j), { status });
  if (req.headers.get("authorization") !== "Bearer wfa_good") return reply({ ok: false, reason: "not_signed_in" }, 401);
  if (!member) return reply({ ok: false, reason: "not_included" }, 403);
  for (const c of b.changes || []) { synced.push(c); store.set(c.id, { id: c.id, list: c.list, body: c.body, updated_at: c.updated_at }); }
  if (b.pull === false) return reply({ ok: true });
  return reply({ ok: true, entries: [...store.values()], next: null, cursor: 1 });
}
const env = {
  ADDRESS_LIMIT: limiter(2),
  KEY_LIMIT: limiter(3),
  SITE: { fetch: async (req) => {
    if (new URL(req.url).pathname === "/api/cloud/sync") return cloud(req);
    lookups++;
    const ok = req.headers.get("authorization") === "Bearer wfa_good";
    return new Response(JSON.stringify(ok ? { ok: true, agent: { id: "ag1", name: "A" } } : { ok: false, reason: "bad_key" }),
      { status: ok ? 200 : 401 });
  } },
};
let n = 0;
const rpc = (method, { key, ip = "1.1.1.1" } = {}) => mcp.fetch(new Request("https://mcp.wfsim.app/mcp", {
  method: "POST",
  headers: { "content-type": "application/json", "cf-connecting-ip": ip, ...(key ? { authorization: `Bearer ${key}` } : {}) },
  body: JSON.stringify({ jsonrpc: "2.0", id: ++n, method, params: method === "tools/call" ? { name: "no_such_tool", arguments: {} } : {} }),
}), env);

const prm = await (await mcp.fetch(new Request("https://mcp.wfsim.app/.well-known/oauth-protected-resource"), env)).json();
check("the server describes itself as a protected resource the site issues keys for",
  prm.resource === "https://mcp.wfsim.app/mcp" && prm.authorization_servers[0] === "https://wfsim.app"
    && prm.bearer_methods_supported.includes("header"), JSON.stringify(prm));

const codes = [];
for (let i = 0; i < 3; i++) codes.push((await rpc("tools/call")).status);
check("without a key, tool calls spend the address's allowance", JSON.stringify(codes) === "[200,200,429]", JSON.stringify(codes));
const over = await rpc("tools/call");
check("...and the refusal says when to come back", over.status === 429 && over.headers.get("retry-after") === "60");
check("the handshake and the list spend nothing", (await rpc("initialize")).status === 200 && (await rpc("tools/list")).status === 200);
check("another address has its own allowance", (await rpc("tools/call", { ip: "2.2.2.2" })).status === 200);

const withKey = [];
for (let i = 0; i < 4; i++) withKey.push((await rpc("tools/call", { key: "wfa_good" })).status);
check("with a key, calls spend the key's allowance, not the address's", JSON.stringify(withKey) === "[200,200,200,429]",
  JSON.stringify(withKey));
check("...and the key is looked up once, not on every call", lookups === 1, lookups);

const bad = await rpc("tools/list", { key: "wfa_revoked" });
check("a key that is not one is refused, pointing at the metadata",
  bad.status === 401 && /resource_metadata=/.test(bad.headers.get("www-authenticate") || ""), bad.status);

// THE PERSON'S OWN BUILDS, through the same sync store the page uses.
const tool = async (name, args, key) => {
  const r = await mcp.fetch(new Request("https://mcp.wfsim.app/mcp", { method: "POST",
    headers: { "content-type": "application/json", "cf-connecting-ip": "9.9.9.9", ...(key ? { authorization: `Bearer ${key}` } : {}) },
    body: JSON.stringify({ jsonrpc: "2.0", id: ++n, method: "tools/call", params: { name, arguments: args } }) }), env);
  return (await r.json()).result.structuredContent;
};
env.KEY_LIMIT = limiter(100);
const listed = await (await rpc("tools/list")).json();
const names = listed.result.tools.map((t) => t.name);
check("the build tools are listed, the save one not read-only",
  names.includes("account_builds_list") && names.includes("account_builds_save")
    && listed.result.tools.find((t) => t.name === "account_builds_save").annotations.readOnlyHint === false, JSON.stringify(names));
check("without a key they say how to get one", (await tool("account_builds_list", {})).reason === "needs_key");
let saved = await tool("account_builds_save", { build: { weapon: "torid", mods: ["serration@7", "split_chamber"] }, name: "from the agent" }, "wfa_good");
const row = synced.at(-1);
check("a build saves as a preset of the build list, filed under its weapon",
  saved.ok && row.list === "wfsim-presets-builder-builds" && row.body.scope === "torid" && row.body.name === "from the agent" && row.body.id === saved.id,
  JSON.stringify([saved, row && row.list]));
check("...its state in the page's own shape, ranks apart from cards",
  row.body.state.weapon === "torid" && row.body.state.slots[0].mod === "serration" && row.body.state.slots[0].rank === 7
    && row.body.state.slots[1].rank === 5 && "wielder" in row.body.state, JSON.stringify(row.body.state.slots.slice(0, 2)));
check("...with the link that opens it", /\/weapons\/Torid\?build=/.test(saved.link), saved.link);
const got = await tool("account_builds_list", { weapon: "torid" }, "wfa_good");
check("the list reads it back as the build it was, which the stats read takes",
  got.ok && got.found === 1 && JSON.stringify(got.rows[0].build.mods) === JSON.stringify(["serration@7", "split_chamber"]),
  JSON.stringify(got));
saved = await tool("account_builds_save", { id: saved.id, build: { weapon: "torid", mods: ["serration"] } }, "wfa_good");
check("naming an id replaces that build, keeping its name", saved.ok && saved.replaced === true
  && synced.at(-1).body.name === "from the agent" && store.size === 1, JSON.stringify(saved));
check("a build of a weapon WFSim does not have is refused",
  (await tool("account_builds_save", { build: { weapon: "nope" } }, "wfa_good")).reason === "unknown_weapon");
member = false;
check("an account without the membership is told so", (await tool("account_builds_list", {}, "wfa_good")).reason === "not_a_member");

// THE A2A AGENT: a card naming the skills the endpoint runs, and the endpoint.
const { HEADLESS_QUERIES } = await import("../mcp/headless.js");
const card = await (await mcp.fetch(new Request("https://mcp.wfsim.app/.well-known/agent-card.json"), env)).json();
const need = ["name", "description", "version", "supportedInterfaces", "capabilities", "defaultInputModes", "defaultOutputModes", "skills"];
check("the agent card carries every field A2A requires",
  need.every((k) => card[k] !== undefined) && card.supportedInterfaces[0].url === "https://mcp.wfsim.app/a2a"
    && card.supportedInterfaces[0].protocolBinding === "JSONRPC" && card.supportedInterfaces[0].protocolVersion === "1.0",
  JSON.stringify(Object.keys(card)));
check("...its skills are the queries the endpoint runs, each with an id, a name, a description and tags",
  JSON.stringify(card.skills.map((x) => x.id)) === JSON.stringify(HEADLESS_QUERIES.map((q) => q.id))
    && card.skills.every((x) => x.id && x.name && x.description && Array.isArray(x.tags) && x.tags.length), JSON.stringify(card.skills.map((x) => x.id)));
env.ADDRESS_LIMIT = limiter(100);
const a2a = async (method, params, headers = {}) => (await mcp.fetch(new Request("https://mcp.wfsim.app/a2a", {
  method: "POST", headers: { "content-type": "application/json", "cf-connecting-ip": "8.8.8.8", ...headers },
  body: JSON.stringify({ jsonrpc: "2.0", id: ++n, method, params }) }), env)).json();
const msg = (parts) => ({ message: { messageId: "m1", role: "ROLE_USER", parts } });
let a = await a2a("SendMessage", msg([{ data: { skill: "builder.weapons.find", args: { query: "tor" } } }]));
const dataOf = (m) => (m.parts.find((p) => p.data) || {}).data;
check("a data part naming a skill runs it, answered as a message from the agent",
  a.result && a.result.message.role === "ROLE_AGENT" && dataOf(a.result.message).rows[0].id === "torid", JSON.stringify(a));
a = await a2a("SendMessage", msg([{ text: "torid" }]));
check("plain text is read as a weapon to find", dataOf(a.result.message).rows[0].id === "torid", JSON.stringify(a));
a = await a2a("message/send", { message: { messageId: "m2", role: "user", kind: "message", parts: [{ kind: "data", data: { skill: "builder_weapons_find", args: { query: "tor" } } }] } });
check("a 0.3 caller is answered in 0.3's shape", a.result && a.result.kind === "message" && a.result.role === "agent"
  && dataOf(a.result).rows[0].id === "torid", JSON.stringify(a));
a = await a2a("SendMessage", msg([{ data: { skill: "builder.nothing" } }]));
check("a skill the card does not name is refused in the answer, naming the ones it does",
  dataOf(a.result.message).reason === "unknown_skill" && dataOf(a.result.message).skills.length === HEADLESS_QUERIES.length, JSON.stringify(a));
check("no task is kept, so asking for one finds none", (await a2a("GetTask", { id: "t1" })).error.code === -32001);
check("streaming is not offered", (await a2a("SendStreamingMessage", msg([{ text: "x" }]))).error.code === -32004);
check("a protocol version it does not speak is refused",
  (await a2a("SendMessage", msg([{ text: "x" }]), { "a2a-version": "9.9" })).error.code === -32009);
env.ADDRESS_LIMIT = limiter(1);
await a2a("SendMessage", msg([{ text: "tor" }]));
check("a message spends the same allowance as a tool call",
  (await mcp.fetch(new Request("https://mcp.wfsim.app/a2a", { method: "POST", headers: { "content-type": "application/json", "cf-connecting-ip": "8.8.8.8" },
    body: JSON.stringify({ jsonrpc: "2.0", id: ++n, method: "SendMessage", params: msg([{ text: "tor" }]) }) }), env)).status === 429);

console.log(failed ? `\n${failed} failed` : "\nthe MCP server knows who calls, and how often");
process.exit(failed ? 1 : 0);
