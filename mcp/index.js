// THE MCP SERVER — the headless queries (`headless.js`, the page's own table,
// generated) over the Model Context Protocol, at https://mcp.wfsim.app/mcp.
//
// A WORKER OF ITS OWN, not a path on the site's: it bundles the engine, and a
// site worker that carried it would start every page load that much colder.
// It holds no state between requests (Streamable HTTP without sessions). It
// offers the headless queries, and — to a key its person claimed — that
// person's saved builds (docs/AGENT.md §"The MCP server").
import { wasm_bindgen, module, ENGINE_DIGEST } from "./engine.js";
import { HEADLESS_ABOUT, HEADLESS_QUERIES, HEADLESS_RETIRED, headlessCheckArgs, headlessNo, headlessSchema, headlessToolName,
  headlessUnknown } from "./headless.js";
import { ACCOUNT_TOOLS, callAccount } from "./account.js";
import { a2aAnswer, agentCard } from "./a2a.js";

const SITE = "https://wfsim.app";
const PROTOCOLS = ["2025-06-18", "2025-03-26"];
const CORS = {
  "access-control-allow-origin": "*",
  "access-control-allow-methods": "POST, OPTIONS",
  "access-control-allow-headers": "authorization, content-type, mcp-protocol-version, mcp-session-id",
};
const RESOURCE = "https://mcp.wfsim.app/mcp";
const METADATA = "/.well-known/oauth-protected-resource";

/// WHO IS CALLING, as the site's worker says: an agent key's row, or null for
/// none. A key is asked about once a minute per isolate, so a busy agent costs
/// the site one lookup, not one per call.
const KEY_TTL_MS = 60000;
const keys = new Map();
async function agentKey(env, request) {
  const auth = request.headers.get("authorization") || "";
  if (!auth) return null;
  const hit = keys.get(auth);
  if (hit && hit.until > Date.now()) return hit.agent;
  let agent = false;
  try {
    const r = await env.SITE.fetch(new Request(`${SITE}/api/agent/whoami`, { headers: { authorization: auth } }));
    const j = await r.json();
    agent = r.ok && j.ok ? j.agent : false;
  } catch (_) {
    agent = false;
  }
  if (keys.size > 5000) keys.clear();
  keys.set(auth, { agent, until: Date.now() + KEY_TTL_MS });
  return agent;
}

/// THE ALLOWANCE (worker/agents.js MCP_LIMITS): per address without a key, per
/// key with one. Only a tool call counts; the handshake and the list are free.
async function overLimit(env, request, agent) {
  const limiter = agent ? env.KEY_LIMIT : env.ADDRESS_LIMIT;
  if (!limiter) return false;
  const key = agent ? `key:${agent.id}` : `ip:${request.headers.get("cf-connecting-ip") || "unknown"}`;
  const { success } = await limiter.limit({ key });
  return !success;
}

// THE ENGINE STARTS ON THE FIRST CALL THAT NEEDS IT, not at load: a worker's
// global scope has a startup budget, and `tools/list` needs no engine at all.
let engine = null;
function api(path, body) {
  if (!engine) {
    wasm_bindgen.initSync({ module });
    engine = wasm_bindgen;
  }
  return JSON.parse(engine.api(path, JSON.stringify(body ?? {})));
}

let meta = null;
let names = null;
const boardFile = async (env, url) => {
  const r = await (env.SITE ? env.SITE.fetch(url) : fetch(url));
  return r.ok ? r.json() : null;
};

/// The worker as a headless host: the engine's own meta and names, the board
/// as the site publishes it right now, English, and no screen.
const host = (env) => ({
  meta: () => (meta ??= api("/api/meta")),
  names: () => (names ??= Object.values(api("/api/i18n"))),
  async board(id) {
    return boardFile(env, `${SITE}/board/${encodeURIComponent(id)}.json`);
  },
  // THE HEADS, for a top-N answer (docs/BOARD.md §"The heads"); every row
  // where a weapon has none yet.
  async boardHeads(id) {
    return (await boardFile(env, `${SITE}/board/heads/${encodeURIComponent(id)}.json`)) || this.board(id);
  },
  api: async (path, body) => api(path, body),
  tr: (s) => s,
  origin: SITE,
  screen_weapon: () => null,
  screen_build: () => null,
});

const TOOLS = HEADLESS_QUERIES.map((q) => ({ q, name: headlessToolName(q.id) }));

async function call(env, name, args, auth) {
  const own = ACCOUNT_TOOLS.find((x) => x.name === name);
  if (own) return callAccount({ env, auth, host: host(env), api }, own, args);
  const t = TOOLS.find((x) => x.name === name);
  if (!t) {
    const old = Object.keys(HEADLESS_RETIRED).find((id) => headlessToolName(id) === name);
    return (old && headlessUnknown(old)) || headlessNo("unknown_tool", { alternatives: TOOLS.map((x) => x.name) });
  }
  const bad = headlessCheckArgs(t.q, args || {});
  if (bad) return bad;
  try {
    const out = await t.q.run(args || {}, host(env));
    return out && out.ok === false ? out : { ok: true, ...out };
  } catch (e) {
    return headlessNo("query_failed", { because: String((e && e.message) || e) });
  }
}

async function answer(env, msg, auth) {
  const { id, method, params } = msg;
  switch (method) {
    case "initialize": {
      const asked = params && params.protocolVersion;
      return {
        protocolVersion: PROTOCOLS.includes(asked) ? asked : PROTOCOLS[0],
        capabilities: { tools: { listChanged: false } },
        serverInfo: { name: "wfsim", title: "WFSim", version: ENGINE_DIGEST },
        instructions: HEADLESS_ABOUT,
      };
    }
    case "ping":
      return {};
    case "tools/list":
      return { tools: TOOLS.map(({ q, name }) => ({
        name, title: q.id, description: q.what, inputSchema: headlessSchema(q.args),
        annotations: { readOnlyHint: true, openWorldHint: false },
      })).concat(ACCOUNT_TOOLS.map((t) => ({
        name: t.name, title: t.title, description: t.what, inputSchema: headlessSchema(t.args),
        annotations: { readOnlyHint: t.readOnly, destructiveHint: false, openWorldHint: false },
      }))) };
    case "tools/call": {
      const out = await call(env, params && params.name, params && params.arguments, auth);
      return { content: [{ type: "text", text: JSON.stringify(out) }], structuredContent: out, isError: out.ok === false };
    }
    default:
      return { error: { code: -32601, message: `no method ${method}` } };
  }
}

const json = (body, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json", ...CORS } });

/// THE A2A ENDPOINT (`a2a.js`): the same key, the same allowance as a tool call.
const A2A_VERSIONS = new Set(["", "1.0", "0.3"]);
async function a2a(request, env) {
  if (request.method === "OPTIONS") return new Response(null, { status: 204, headers: { ...CORS, "access-control-allow-headers": `${CORS["access-control-allow-headers"]}, a2a-version` } });
  if (request.method !== "POST") return new Response(null, { status: 405, headers: { allow: "POST", ...CORS } });
  let msg;
  try { msg = await request.json(); } catch (_) {
    return json({ jsonrpc: "2.0", id: null, error: { code: -32700, message: "not JSON" } }, 400);
  }
  if (!msg || typeof msg !== "object" || msg.jsonrpc !== "2.0" || !msg.method) {
    return json({ jsonrpc: "2.0", id: null, error: { code: -32600, message: "one JSON-RPC 2.0 request" } }, 400);
  }
  const version = (request.headers.get("a2a-version") || "").trim();
  if (!A2A_VERSIONS.has(version)) {
    return json({ jsonrpc: "2.0", id: msg.id ?? null, error: { code: -32009, message: `A2A ${version} is not supported; 1.0 is` } });
  }
  const agent = await agentKey(env, request);
  if (agent === false) {
    return json({ jsonrpc: "2.0", id: msg.id ?? null, error: { code: -32001, message: `bad_key: register again at ${SITE}/auth.md` } }, 401);
  }
  if (await overLimit(env, request, agent)) {
    return new Response(JSON.stringify({ jsonrpc: "2.0", id: msg.id ?? null, error: { code: -32029, message: `rate_limited: see ${SITE}/auth.md` } }),
      { status: 429, headers: { "content-type": "application/json", "retry-after": "60", ...CORS } });
  }
  const out = await a2aAnswer(msg, (q, args) => q.run(args, host(env)));
  return json({ jsonrpc: "2.0", id: msg.id ?? null, ...out });
}

export default {
  async fetch(request, env) {
    const path = new URL(request.url).pathname;
    // RFC 9728: this server as a protected resource. A key is optional here;
    // the site's worker issues it (its /auth.md).
    if (path === METADATA) {
      return json({ resource: RESOURCE, authorization_servers: [SITE], scopes_supported: ["read", "builds"],
        bearer_methods_supported: ["header"], resource_name: "WFSim MCP", resource_documentation: `${SITE}/auth.md` });
    }
    if (path === "/.well-known/agent-card.json") {
      return new Response(JSON.stringify(agentCard(ENGINE_DIGEST)), { headers: { "content-type": "application/json",
        "cache-control": "public, max-age=3600", etag: `"${ENGINE_DIGEST}"`, ...CORS } });
    }
    if (path === "/a2a") return a2a(request, env);
    if (path !== "/mcp") {
      return path === "/" ? Response.redirect(`${SITE}/llms.txt`, 302) : new Response("not found", { status: 404 });
    }
    if (request.method === "OPTIONS") return new Response(null, { status: 204, headers: CORS });
    // NO STREAM TO OPEN: every answer is one JSON response, so the GET a client
    // uses to listen for server messages is refused, as the transport allows.
    if (request.method !== "POST") return new Response(null, { status: 405, headers: { allow: "POST", ...CORS } });
    let msg;
    try { msg = await request.json(); } catch (_) {
      return json({ jsonrpc: "2.0", id: null, error: { code: -32700, message: "not JSON" } }, 400);
    }
    if (!msg || typeof msg !== "object" || Array.isArray(msg) || msg.jsonrpc !== "2.0" || !msg.method) {
      return json({ jsonrpc: "2.0", id: null, error: { code: -32600, message: "one JSON-RPC 2.0 request" } }, 400);
    }
    // A NOTIFICATION HAS NO ID AND GETS NO ANSWER.
    if (msg.id === undefined) return new Response(null, { status: 202, headers: CORS });
    // A KEY THAT IS NOT ONE is refused rather than read as none, so an agent
    // learns its key is gone instead of quietly running on the smaller allowance.
    const agent = await agentKey(env, request);
    if (agent === false) {
      return new Response(JSON.stringify({ jsonrpc: "2.0", id: msg.id, error: { code: -32001, message: "bad_key: register again at " + SITE + "/auth.md" } }),
        { status: 401, headers: { "content-type": "application/json", "www-authenticate": `Bearer resource_metadata="https://mcp.wfsim.app${METADATA}"`, ...CORS } });
    }
    if (msg.method === "tools/call" && await overLimit(env, request, agent)) {
      return new Response(JSON.stringify({ jsonrpc: "2.0", id: msg.id, error: { code: -32029, message: `rate_limited: see ${SITE}/auth.md` } }),
        { status: 429, headers: { "content-type": "application/json", "retry-after": "60", ...CORS } });
    }
    const out = await answer(env, msg, agent ? request.headers.get("authorization") : null);
    return json(out.error ? { jsonrpc: "2.0", id: msg.id, error: out.error } : { jsonrpc: "2.0", id: msg.id, result: out });
  },
};
