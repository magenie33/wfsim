// Local account and publication fixture; every API uses the production route and schema.
import { createServer } from "node:http";
import { Readable } from "node:stream";
import { pathToFileURL } from "node:url";
import { testDb } from "./pg/test_db.mjs";
import { accountRoute, sha256 } from "../worker/accounts.js";
import { contributionRoute } from "../worker/contribution.js";
import { contributorProfilesRoute } from "../worker/contributor_profiles.js";
import { contributorAccounts } from "../worker/contributor_identity.js";
import { recommendationsRoute } from "../worker/recommendations.js";
import { loadEngine } from "./wasm_engine.mjs";

export async function computationFixture(upstream, port = 0, controls = false) {
  const env = { LIBRARY: await testDb("library"), ACCOUNTS: await testDb("accounts"), AUTH_SECRET: "local-attribution-fixture",
    EMAIL: { send: async () => ({ messageId: "local" }) } };
  const token = "local-computation-review", account = "local-aaa", engine = "123456789abc";
  const at = new Date().toISOString(), profileIds = {};
  for (const [i, name] of ["aaa", "bbb", "ccc"].entries()) {
    const owner = "local-" + name, device = String(i + 1).repeat(24);
    await env.ACCOUNTS.run("INSERT INTO accounts (id,created_at,username,display_name) VALUES (?,?,?,?)", owner, at, name, name);
    await env.ACCOUNTS.run("INSERT INTO contribution_choice (account,named,chosen_at) VALUES (?,1,?)", owner, at);
    await env.ACCOUNTS.run("INSERT INTO device_spans (verifier,account,from_hour) VALUES (?,?,'')", device, owner);
    await env.LIBRARY.run("INSERT INTO verifiers (id,seen,work,signatures) VALUES (?,?,?,?)", device, at, (128640 - i * 12000) * 1e9, 128 + i);
    profileIds[name] = (await contributorAccounts(env, [owner])).get(owner).profile_id;
  }
  await env.ACCOUNTS.run("INSERT INTO identities (provider,subject,account,label,linked_at) VALUES ('email','local@example.invalid',?,'local@example.invalid',?)", account, at);
  await env.ACCOUNTS.run("INSERT INTO sessions (token_hash,account,expires_at) VALUES (?,?,'2099-01-01T00:00:00.000Z')", await sha256(token), account);
  let publication = await fetch(upstream + "/board/laetum.json").then(r => r.json());
  if (!Array.isArray(publication) || !publication.length) throw new Error("The fixture needs the published Laetum board.");
  const row = publication[0], record = { weapon: "laetum", ...row,
    ...(row.riven ? { riven_pos: row.riven.bonuses, riven_neg: row.riven.malus, riven_rolls: row.riven.rolls } : {}) };
  const keyRequest = { kind: "build", builds: [record] };
  const keyResponse = await fetch(upstream + "/api/build/keys", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(keyRequest) });
  const keys = keyResponse.headers.get("content-type")?.includes("application/json")
    ? await keyResponse.json() : loadEngine().api("/api/build/keys", keyRequest);
  const identity = keys.keys?.[0];
  if (!/^[a-f0-9]{32}$/.test(identity || "")) throw new Error("The fixture build must have an engine identity.");
  const measured = new Date(row.measured_at).toISOString();
  await env.LIBRARY.run("INSERT INTO builds (id,at,record) VALUES (?,?,?)", identity, at.slice(0, 10), JSON.stringify(record));
  await env.ACCOUNTS.run("INSERT INTO recommendation_targets (identity,weapon,record,ruler,mode) VALUES (?,'laetum',?,?,?)", identity, JSON.stringify(record), row.benchmark, row.mode || "base");
  await env.ACCOUNTS.run("INSERT INTO recommendations (account,identity,day) VALUES (?,?,CURRENT_DATE-1)", account, identity);
  await env.LIBRARY.run("INSERT INTO scores (identity,ruler,mode,measured_by,score,metric,cost_seconds,started_at,finished_at) VALUES (?,?,?,?,?,'kpm',0,?,?)",
    identity, row.benchmark, row.mode || "base", "verified:" + row.measured_by, row.score, measured, measured);
  for (const [i, name] of ["aaa", "bbb"].entries()) await env.LIBRARY.run(`INSERT INTO score_witnesses
    (identity,ruler,mode,measured_at,engine,owner,device,official,completed_at,credited_at,position) VALUES (?,?,?,?,?,?,?,0,?,?,?)`,
    identity, row.benchmark, row.mode || "base", measured, row.measured_by, "local-" + name, String(i + 1).repeat(24), measured, measured, i);
  const question = "a".repeat(32), expires = new Date(Date.now() + 14 * 86400000).toISOString();
  await env.LIBRARY.run("INSERT INTO computation_records (question,kind,engine,weapon,completed_at,expires_at) VALUES (?,'optimization',?,'laetum',?,?)", question, engine, at, expires);
  for (const [i, name] of ["aaa", "bbb"].entries()) await env.LIBRARY.run("INSERT INTO computation_witnesses (question,owner,device,official,completed_at,credited_at,position) VALUES (?,?,?,0,?,?,?)",
    question, "local-" + name, String(i + 1).repeat(24), at, at, i);
  let greeted = false, hidden = false;
  const server = createServer(async (incoming, outgoing) => {
    try {
      const url = new URL(incoming.url, "http://" + incoming.headers.host), headers = new Headers();
      for (const [name, value] of Object.entries(incoming.headers)) if (value != null) headers.set(name, Array.isArray(value) ? value.join(", ") : value);
      const request = new Request(url, { method: incoming.method, headers,
        ...(!["GET", "HEAD"].includes(incoming.method) ? { body: Readable.toWeb(incoming), duplex: "half" } : {}) });
      let response;
      if (url.pathname.startsWith("/__fixture/") && controls) {
        const action = url.pathname.split("/").at(-1);
        if (action === "rename") await env.ACCOUNTS.run("UPDATE accounts SET display_name=CASE WHEN display_name='aaa' THEN 'aaa_new' ELSE 'aaa' END WHERE id=?", account);
        if (action === "visibility") await env.ACCOUNTS.run("UPDATE contribution_choice SET named=1-named WHERE account=?", account);
        if (action === "board") hidden = !hidden;
        response = new Response(null, { status: 302, headers: { location: request.headers.get("referer") || "/people/me" } });
      } else if (url.pathname === "/board/laetum.json") response = Response.json(hidden ? publication.filter(item => item !== row) : publication);
      else if (url.pathname.startsWith("/api/contributors/profile/") || url.pathname.startsWith("/api/computations/") || url.pathname === "/api/account/contribution-profile") response = await contributorProfilesRoute(request, env, url.pathname);
      else if (url.pathname === "/api/contributors" || url.pathname.startsWith("/api/account/devices") || url.pathname === "/api/account/contribution") response = await contributionRoute(request, env, url.pathname);
      else if (url.pathname.startsWith("/api/recommendations/") || url.pathname.startsWith("/api/account/recommendations")) response = await recommendationsRoute(request, env, url.pathname);
      else if (url.pathname.startsWith("/api/auth/") || ["/api/account", "/api/account/profile"].includes(url.pathname)) response = await accountRoute(request, env, url.pathname);
      else if (url.pathname === "/api/account/agents") response = Response.json({ ok: true, agents: [] });
      if (!response) {
        const proxyHeaders = new Headers(headers); proxyHeaders.delete("host");
        response = await fetch(new URL(incoming.url, upstream), { method: incoming.method, headers: proxyHeaders,
          ...(!["GET", "HEAD"].includes(incoming.method) ? { body: request.body, duplex: "half" } : {}), redirect: "manual" });
      }
      const responseHeaders = new Headers(response.headers);
      responseHeaders.delete("content-length"); responseHeaders.delete("content-encoding"); responseHeaders.set("cache-control", "no-store");
      let bytes = Buffer.from(await response.arrayBuffer());
      if (responseHeaders.get("content-type")?.includes("text/html")) {
        if (!greeted) { greeted = true; responseHeaders.append("set-cookie", "wfsim_session=" + token + "; Path=/; HttpOnly; SameSite=Lax"); }
        if (controls) bytes = Buffer.from(bytes.toString().replace("</body>", `<aside style="position:fixed;bottom:8px;left:8px;z-index:9999;background:var(--surface);border:1px solid var(--line);padding:8px;font-size:12px;display:flex;gap:12px;flex-wrap:wrap">本地测试数据 <a href="/people/me">我的主页</a><a href="/weapons/Laetum">配装库</a><a href="/__fixture/rename">切换改名</a><a href="/__fixture/visibility">切换匿名</a><a href="/__fixture/board">切换下榜</a></aside></body>`));
      }
      outgoing.writeHead(response.status, Object.fromEntries(responseHeaders)); outgoing.end(bytes);
    } catch (error) { outgoing.writeHead(500, { "content-type": "application/json" }); outgoing.end(JSON.stringify({ ok: false, error: error.message })); }
  });
  await new Promise(resolve => server.listen(port, "127.0.0.1", resolve));
  return { base: "http://127.0.0.1:" + server.address().port, env, token, account, profileIds, identity, row, question,
    hideBoard(value) { hidden = value; }, async close() { server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); await env.ACCOUNTS.close(); await env.LIBRARY.close(); } };
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const fixture = await computationFixture(process.env.WFSIM_BASE || "http://127.0.0.1:8934", 8935, true);
  console.log(fixture.base + "/people/me");
}
