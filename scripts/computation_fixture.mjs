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
  await env.ACCOUNTS.exec("SET TIME ZONE 'UTC'");
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
      else if (url.pathname.startsWith("/api/auth/") || ["/api/account", "/api/account/profile", "/api/account/identity"].includes(url.pathname)) response = await accountRoute(request, env, url.pathname);
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
    async retireBoard(value) {
      await env.LIBRARY.run(`INSERT INTO board_reviews (ref,id,round_at,identity,ruler,mode,engine,question,record,state,
        baseline_score,baseline_metric,baseline_cost_seconds,baseline_by,baseline_started_at,baseline_finished_at,created_at)
        SELECT 'profile-fixture','profile-fixture',finished_at,identity,ruler,mode,?,'profile-fixture',?,'succeeded',
          score,metric,cost_seconds,measured_by,started_at,finished_at,finished_at FROM scores WHERE identity=? AND ruler=? AND mode=?
        ON CONFLICT (id) DO NOTHING`, row.measured_by, JSON.stringify(record), identity, row.benchmark, row.mode || "base");
      await env.LIBRARY.run("UPDATE board_reviews SET state='succeeded',invalidated_at=NULL WHERE id='profile-fixture'");
      await env.LIBRARY.run("UPDATE scores SET review_id='profile-fixture' WHERE identity=? AND ruler=? AND mode=?", identity, row.benchmark, row.mode || "base");
      if (value) await env.LIBRARY.run("UPDATE board_reviews SET state='failed',invalidated_at=? WHERE id='profile-fixture'", new Date().toISOString());
    },
    hideBoard(value) { hidden = value; }, async close() { server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); await env.ACCOUNTS.close(); await env.LIBRARY.close(); } };
}
export async function seedHomePages(fixture, count = 23) {
  const rows = await fetch(fixture.base + "/board/laetum.json").then(response => response.json());
  const records = rows.slice(0, 200).map(row => ({ weapon: "laetum", ...row,
    ...(row.riven ? { riven_pos: row.riven.bonuses, riven_neg: row.riven.malus, riven_rolls: row.riven.rolls } : {}) }));
  const keys = loadEngine().api("/api/build/keys", { kind: "build", builds: records }).keys;
  const unique = new Map();
  keys.forEach((key, index) => { if (key && !unique.has(key)) unique.set(key, records[index]); });
  if (unique.size < count) throw new Error("Paging needs enough distinct canonical published builds.");
  const selected = [...unique].slice(0, count), device = "1".repeat(24);
  for (const [identity, record] of selected) {
    const measured = new Date(record.measured_at).toISOString(), mode = record.mode || "base";
    await fixture.env.LIBRARY.run("INSERT INTO builds (id,at,record) VALUES (?,?,?) ON CONFLICT DO NOTHING", identity, measured.slice(0, 10), JSON.stringify(record));
    await fixture.env.LIBRARY.run("INSERT INTO scores (identity,ruler,mode,measured_by,score,metric,cost_seconds,started_at,finished_at) VALUES (?,?,?,?,?,'kpm',0,?,?) ON CONFLICT DO NOTHING",
      identity, record.benchmark, mode, "verified:" + record.measured_by, record.score, measured, measured);
    await fixture.env.LIBRARY.run("INSERT INTO score_witnesses (identity,ruler,mode,measured_at,engine,owner,device,official,completed_at,credited_at,position) VALUES (?,?,?,?,?,?,?,0,?,?,0) ON CONFLICT DO NOTHING",
      identity, record.benchmark, mode, measured, record.measured_by, "local-aaa", device, measured, measured);
    await fixture.env.ACCOUNTS.run("INSERT INTO recommendation_targets (identity,weapon,record,ruler,mode) VALUES (?,'laetum',?,?,?) ON CONFLICT DO NOTHING", identity, JSON.stringify(record), record.benchmark, mode);
    await fixture.env.ACCOUNTS.run("INSERT INTO recommendations (account,identity,day) VALUES (?,?,CURRENT_DATE-1) ON CONFLICT DO NOTHING", fixture.account, identity);
  }
  const identity = selected.map(([id]) => id).sort()[0];
  for (let day = 2; day <= count; day++) await fixture.env.ACCOUNTS.run("INSERT INTO recommendations (account,identity,day) VALUES (?,?,CURRENT_DATE-?::integer) ON CONFLICT DO NOTHING", fixture.account, identity, day);
  const record = await fixture.env.LIBRARY.prepare("SELECT * FROM computation_records WHERE question=?").bind(fixture.question).first();
  for (let index = 1; index < count; index++) {
    const question = "f" + index.toString(16).padStart(31, "0"), at = new Date(Date.parse(record.completed_at) - index * 1000).toISOString();
    await fixture.env.LIBRARY.run("INSERT INTO computation_records (question,kind,engine,weapon,completed_at,expires_at) VALUES (?,'optimization',?,'laetum',?,?)", question, record.engine, at, record.expires_at);
    await fixture.env.LIBRARY.run("INSERT INTO computation_witnesses (question,owner,device,official,completed_at,credited_at,position) VALUES (?,?,?,0,?,?,0)", question, "local-aaa", device, at, at);
  }
  return { count, identity };
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const fixture = await computationFixture(process.env.WFSIM_BASE || "http://127.0.0.1:8934", 8935, true);
  console.log(fixture.base + "/people/me");
}
