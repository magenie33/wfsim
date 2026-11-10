// ---- AGENTS ---------------------------------------------------------------------------
//
// docs/ACCOUNTS.md §"Agents". An AI agent registers with nothing and gets a key
// at once; the key raises its allowance on the MCP server. Claimed — a code
// mailed to an address a WFSim account holds, read back to the agent by its
// person — the key acts for that account in what the account may do.
//
// ONE SOURCE FOR EVERY DOCUMENT AN AGENT READS: the two OAuth metadata files
// and `/auth.md` are drawn here from the same constants the endpoints use, so
// no copy of an endpoint can drift from the endpoint.

import { EMAIL_FROM, emailOf, emailSlot, hmac, json, later, limited, no, now, random, sameSite, sessionAccount, sha256 }
  from "./accounts.js";

const KEY_PREFIX = "wfa_";
export const SCOPES_BEFORE_CLAIM = ["read"];
export const SCOPES_CLAIMED = ["read", "builds"];
/// THE MCP SERVER'S ALLOWANCE, per minute — enforced by `mcp/wrangler.jsonc`'s
/// limiters and stated from here; `check_agents` holds the two equal.
export const MCP_LIMITS = { per_address: 30, per_key: 120 };
const NAME_MAX = 64;
const CLAIM_SECONDS = 600;
const CLAIM_RESEND_SECONDS = 60;
const CLAIM_ATTEMPTS = 5;
/// An unclaimed key nobody used for this long is dropped.
const IDLE_SECONDS = 90 * 24 * 3600;
/// How stale `last_used_at` may be before a use writes it again.
const TOUCH_SECONDS = 600;

export const AGENT_PATHS = {
  register: "/api/agent/auth",
  claim: "/api/agent/auth/claim",
  claimComplete: "/api/agent/auth/claim/complete",
  revoke: "/api/agent/auth/revoke",
  whoami: "/api/agent/whoami",
};
const MCP_URL = "https://mcp.wfsim.app/mcp";

// ---- what an agent reads ------------------------------------------------------------------

/// RFC 9728: this site as a protected resource, and who issues its keys.
export const protectedResource = (origin, resource = origin) => ({
  resource,
  authorization_servers: [origin],
  scopes_supported: SCOPES_CLAIMED,
  bearer_methods_supported: ["header"],
  resource_name: "WFSim",
  resource_documentation: `${origin}/auth.md`,
});

/// RFC 8414, with the `agent_auth` block auth.md defines. Both spellings of the
/// two endpoint names are given: `identity_endpoint` and `claim_endpoint` are
/// the current ones, `register_uri` and `claim_uri` what older readers look for.
export function authorizationServer(origin) {
  const at = (p) => origin + AGENT_PATHS[p];
  return {
    issuer: origin,
    revocation_endpoint: at("revoke"),
    response_types_supported: [],
    grant_types_supported: [],
    scopes_supported: SCOPES_CLAIMED,
    service_documentation: `${origin}/auth.md`,
    agent_auth: {
      skill: `${origin}/auth.md`,
      identity_endpoint: at("register"),
      register_uri: at("register"),
      claim_endpoint: at("claim"),
      claim_uri: at("claim"),
      claim_complete_endpoint: at("claimComplete"),
      revocation_uri: at("revoke"),
      identity_types_supported: ["anonymous"],
      anonymous: {
        credential_types_supported: ["api_key"],
        pre_claim_scopes: SCOPES_BEFORE_CLAIM,
        claim_uri: at("claim"),
      },
      claim_methods_supported: ["email_otp"],
    },
  };
}

export function authMd(origin) {
  const at = (p) => origin + AGENT_PATHS[p];
  return `# WFSim auth.md

WFSim (${origin}) is a Warframe calculator: builds, fights and a build
optimizer, with numbers checked against in-game measurements. An AI agent
answering a Warframe player can use it with no key at all; a key raises the
agent's allowance, and a key its person claims can read and save that person's
builds.

## Without a key

Everything a player can look up is public:

- the MCP server: ${MCP_URL} (Streamable HTTP);
- the A2A agent: https://mcp.wfsim.app/a2a (its card: ${origin}/.well-known/agent-card.json);
- any page as markdown: ${origin}/weapons/<Wiki_Name> with \`Accept: text/markdown\`,
  or the same address with \`.md\` appended;
- the published board: ${origin}/board/<weapon_id>.json.

The site's edge refuses the \`Python-urllib\` default user agent; send any other.

## Discover

- Protected resource: ${origin}/.well-known/oauth-protected-resource
- Authorization server: ${origin}/.well-known/oauth-authorization-server (its \`agent_auth\` block)
- MCP server: ${MCP_URL} (Streamable HTTP)

## Pick a method

One method: **anonymous**. Register, and use the key at once. To act for a
person — read or save their builds — claim the key with their email address.

## Register

\`\`\`http
POST ${at("register")}
Content-Type: application/json

{"type": "anonymous", "name": "<your agent's name, shown to the person>"}
\`\`\`

\`\`\`json
{"ok": true, "agent_id": "…", "credential": {"type": "api_key", "api_key": "${KEY_PREFIX}…"},
 "scopes": ${JSON.stringify(SCOPES_BEFORE_CLAIM)}, "claim_uri": "${at("claim")}"}
\`\`\`

Keep the key: it is shown once. Registering is limited per address.

## Claim

Ask your person for the email address of their WFSim account, then:

\`\`\`http
POST ${at("claim")}
Authorization: Bearer <api_key>
Content-Type: application/json

{"email": "<their address>"}
\`\`\`

A six-digit code is mailed to the address; it works for 10 minutes. The answer
is the same whether or not an account holds the address, so tell your person
to check their mail: an address with no account gets a mail saying so, and they
sign up at ${origin}/signup first. Then, with the code they read back:

\`\`\`http
POST ${at("claimComplete")}
Authorization: Bearer <api_key>
Content-Type: application/json

{"code": "123456"}
\`\`\`

\`\`\`json
{"ok": true, "account": {"username": "…"}, "scopes": ${JSON.stringify(SCOPES_CLAIMED)}}
\`\`\`

## Use the credential

A claimed key can also read its account's recommendation history at
\`GET ${origin}/api/account/recommendations\`. Read current state with
\`POST ${origin}/api/recommendations/state\` and \`{"identities": [...]}\`;
\`POST ${origin}/api/recommendations\` accepts \`{identity, day, action, ruler, mode}\`.
Use the returned UTC \`day\`; \`action\` is \`recommend\` or \`withdraw\`.
Each account has one recommendation per build per UTC day. Requests are
limited to 120 a minute per account and address; excess returns 429 with
\`Retry-After: 60\`. Pending recommendations may be withdrawn before settlement.

Send \`Authorization: Bearer <api_key>\` to the MCP server. Without a key it
allows ${MCP_LIMITS.per_address} tool calls a minute per address; with one,
${MCP_LIMITS.per_key} a minute per key. Over it, the answer is HTTP 429 with
\`Retry-After\`.

\`\`\`http
GET ${at("whoami")}
Authorization: Bearer <api_key>
\`\`\`

says who the key is and what it may do. A claimed key's \`builds\` scope reads
and writes the person's saved builds — on the MCP server as the tools
\`account_builds_list\` and \`account_builds_save\`, which hand back the link
that opens each build, or directly through \`POST ${origin}/api/cloud/sync\`.

## Revoke

\`\`\`http
POST ${at("revoke")}
Authorization: Bearer <api_key>
\`\`\`

Your person can also revoke the key from ${origin}/account.

## Errors

| reason | meaning |
| --- | --- |
| \`bad_key\` (401) | no key, or not one this server issued; register again |
| \`rate_limited\` (429) | too many registrations or claims from this address |
| \`already_claimed\` | the key already acts for an account |
| \`too_soon\` (429) | a code was sent under a minute ago |
| \`bad_email\`, \`bad_code\` | not an address; not six digits |
| \`wrong_code\`, \`expired\`, \`too_many_attempts\` | claim again for a new code |
`;
}

// ---- keys -----------------------------------------------------------------------------------

const bearer = (request) => {
  const m = /^Bearer\s+(\S+)$/i.exec(request.headers.get("authorization") || "");
  return m && m[1].startsWith(KEY_PREFIX) ? m[1] : null;
};

/// THE KEY ON THIS REQUEST, as its row, or null. A use writes `last_used_at`
/// when it has gone stale, so reading a key costs no write on most calls.
export async function agentOf(env, request) {
  const key = bearer(request);
  if (!key || !env.ACCOUNTS) return null;
  const row = await env.ACCOUNTS.prepare("SELECT * FROM entity_agent_keys WHERE key_hash = ?1").bind(await sha256(key)).first();
  if (!row) return null;
  if (!row.last_used_at || Date.parse(row.last_used_at) < Date.now() - TOUCH_SECONDS * 1000) {
    await env.ACCOUNTS.prepare("UPDATE entity_agent_keys SET last_used_at = ?1 WHERE id = ?2").bind(now(), row.id).run();
  }
  return row;
}

/// The account a claimed key acts for on authenticated feature routes.
export async function agentAccount(env, request) {
  const row = await agentOf(env, request);
  return row && row.account ? row.account : null;
}

async function register(request, env, b) {
  if (await limited(env, request, "agent:")) return no("rate_limited", 429);
  if (b.type !== undefined && b.type !== "anonymous") return no("unsupported_type");
  const name = String(b.name ?? "").replace(/[\u0000-\u001f\u007f]/g, "").trim().slice(0, NAME_MAX) || "Unnamed agent";
  const db = env.ACCOUNTS;
  // AN UNCLAIMED KEY NOBODY USES IS DROPPED, here where keys are made.
  await db.prepare("DELETE FROM entity_agent_keys WHERE account IS NULL AND COALESCE(last_used_at, created_at) < ?1")
    .bind(new Date(Date.now() - IDLE_SECONDS * 1000).toISOString()).run();
  const id = crypto.randomUUID();
  const key = KEY_PREFIX + random(32);
  await db.prepare("INSERT INTO entity_agent_keys (id, key_hash, name, created_at) VALUES (?1, ?2, ?3, ?4)")
    .bind(id, await sha256(key), name, now()).run();
  return json({ ok: true, agent_id: id, credential: { type: "api_key", api_key: key },
    scopes: SCOPES_BEFORE_CLAIM, claim_uri: new URL(AGENT_PATHS.claim, request.url).toString() }, 201);
}

/// A CODE, MAILED — the same answer whether or not an account holds the
/// address, so the endpoint tells nobody who has one. An address with no
/// account gets a mail that says so instead of a code.
async function claim(request, env, agent, b) {
  if (agent.account) return no("already_claimed", 409);
  if (await limited(env, request, "claim:")) return no("rate_limited", 429);
  const email = emailOf(b.email);
  if (!email) return no("bad_email");
  if (agent.claim_sent_at && Date.parse(agent.claim_sent_at) > Date.now() - CLAIM_RESEND_SECONDS * 1000) return no("too_soon", 429);
  const slot = await emailSlot(env, email);
  const code = String(crypto.getRandomValues(new Uint32Array(1))[0] % 1000000).padStart(6, "0");
  await env.ACCOUNTS.prepare(
    `UPDATE entity_agent_keys SET claim_email = ?1, claim_code_hash = ?2, claim_expires_at = ?3, claim_sent_at = ?4,
       claim_attempts = 0 WHERE id = ?5`,
  ).bind(email, slot ? await hmac(env.AUTH_SECRET, `agent:${agent.id}:${email}:${code}`) : null,
    later(CLAIM_SECONDS), now(), agent.id).run();
  const who = agent.name;
  try {
    await env.EMAIL.send(slot ? {
      to: email, from: EMAIL_FROM, subject: `${code} connects ${who} to WFSim`,
      text: `${who} asked to act for your WFSim account. The code is ${code}\n\nGive it to the agent only if you asked it to connect. It works for 10 minutes. You can disconnect the agent at any time from your account page.\n\n${who} 请求代表你的 WFSim 账号操作，验证码是 ${code}。只有在你确实让它连接时才把验证码告诉它，10 分钟内有效。你可以随时在账号页断开这个 agent。\n`,
    } : {
      to: email, from: EMAIL_FROM, subject: `No WFSim account uses this address`,
      text: `${who} asked to connect to a WFSim account at this address, and none uses it. Sign up at https://wfsim.app/signup first, then ask the agent again. If you did not ask for this, ignore this mail.\n\n${who} 请求连接这个邮箱的 WFSim 账号，但这个邮箱还没有注册。请先到 https://wfsim.app/signup 注册，再让 agent 重试。如果不是你本人操作，请忽略本邮件。\n`,
    });
  } catch (_) {
    return no("send_failed", 502);
  }
  return json({ ok: true, sent: true, expires_in: CLAIM_SECONDS });
}

async function claimComplete(env, agent, b) {
  if (agent.account) return no("already_claimed", 409);
  const code = String(b.code ?? "").trim();
  if (!/^\d{6}$/.test(code)) return no("bad_code");
  if (!agent.claim_email || !agent.claim_expires_at || Date.parse(agent.claim_expires_at) < Date.now()) return no("expired");
  if (agent.claim_attempts >= CLAIM_ATTEMPTS) return no("too_many_attempts", 429);
  const db = env.ACCOUNTS;
  const slot = await emailSlot(env, agent.claim_email);
  if (!slot || !agent.claim_code_hash
      || agent.claim_code_hash !== await hmac(env.AUTH_SECRET, `agent:${agent.id}:${agent.claim_email}:${code}`)) {
    await db.prepare("UPDATE entity_agent_keys SET claim_attempts = claim_attempts + 1 WHERE id = ?1").bind(agent.id).run();
    return no("wrong_code");
  }
  await db.prepare(
    `UPDATE entity_agent_keys SET account = ?1, claimed_at = ?2, claim_email = NULL, claim_code_hash = NULL,
       claim_expires_at = NULL, claim_attempts = 0 WHERE id = ?3`,
  ).bind(slot.account, now(), agent.id).run();
  const a = await db.prepare("SELECT username FROM entity_accounts WHERE id = ?1").bind(slot.account).first();
  return json({ ok: true, account: { username: a && a.username }, scopes: SCOPES_CLAIMED });
}

async function whoami(env, agent) {
  const a = agent.account
    ? await env.ACCOUNTS.prepare("SELECT username, display_name FROM entity_accounts WHERE id = ?1").bind(agent.account).first()
    : null;
  return json({ ok: true, agent: { id: agent.id, name: agent.name, created_at: agent.created_at },
    account: a ? { username: a.username, display_name: a.display_name } : null,
    scopes: agent.account ? SCOPES_CLAIMED : SCOPES_BEFORE_CLAIM, limits: MCP_LIMITS });
}

/// An account's agents, as its page and its export show them.
export async function agentsOf(env, account) {
  const { results } = await env.ACCOUNTS.prepare(
    "SELECT id, name, created_at, claimed_at, last_used_at FROM entity_agent_keys WHERE account = ?1 ORDER BY claimed_at",
  ).bind(account).all();
  return results;
}

// ---- the router -------------------------------------------------------------------------------

/// The response for an agent path, or null for a path that is not one.
export async function agentRoute(request, env, path) {
  const origin = new URL(request.url).origin;
  const get = request.method === "GET" || request.method === "HEAD";
  if (path === "/.well-known/oauth-protected-resource") return get ? json(protectedResource(origin)) : no("method", 405);
  if (path === "/.well-known/oauth-authorization-server") return get ? json(authorizationServer(origin)) : no("method", 405);
  if (path === "/auth.md") {
    return get ? new Response(authMd(origin), { headers: { "content-type": "text/markdown; charset=utf-8", "cache-control": "public, max-age=300" } })
      : no("method", 405);
  }
  if (path === "/api/account/agents" || path === "/api/account/agents/revoke") {
    if (!env.ACCOUNTS) return no("unavailable", 503);
    const account = await sessionAccount(env, request);
    if (!account) return no("not_signed_in", 401);
    if (path === "/api/account/agents") return get ? json({ ok: true, agents: await agentsOf(env, account) }) : no("method", 405);
    if (request.method !== "POST") return no("method", 405);
    if (!sameSite(request)) return no("cross_site", 403);
    let b = {};
    try { b = JSON.parse((await request.text()) || "{}"); } catch (_) { return no("not_json"); }
    await env.ACCOUNTS.prepare("DELETE FROM entity_agent_keys WHERE id = ?1 AND account = ?2").bind(String(b.id || ""), account).run();
    return json({ ok: true });
  }
  if (!Object.values(AGENT_PATHS).includes(path)) return null;
  if (!env.ACCOUNTS || !env.AUTH_SECRET) return no("unavailable", 503);
  if (path === AGENT_PATHS.whoami) {
    if (!get) return no("method", 405);
    const agent = await agentOf(env, request);
    return agent ? whoami(env, agent) : no("bad_key", 401);
  }
  if (request.method !== "POST") return no("method", 405);
  let b = {};
  try { b = JSON.parse((await request.text()) || "{}"); } catch (_) { return no("not_json"); }
  if (path === AGENT_PATHS.register) return register(request, env, b);
  const agent = await agentOf(env, request);
  if (!agent) return no("bad_key", 401);
  if (path === AGENT_PATHS.revoke) {
    await env.ACCOUNTS.prepare("DELETE FROM entity_agent_keys WHERE id = ?1").bind(agent.id).run();
    return json({ ok: true, revoked: true });
  }
  if (!env.EMAIL && path === AGENT_PATHS.claim) return no("unavailable", 503);
  return path === AGENT_PATHS.claim ? claim(request, env, agent, b) : claimComplete(env, agent, b);
}
