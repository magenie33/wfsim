// Local-only account service for browser checks and feature review.
import { createServer } from "node:http";
import { Readable } from "node:stream";
import { testDb } from "./pg/test_db.mjs";
import { accountRoute, sha256 } from "../worker/accounts.js";
import { agentRoute } from "../worker/agents.js";
import { recommendationsRoute } from "../worker/recommendations.js";

export async function recommendationFixture(base, port = 0, controls = false) {
  const env = { ACCOUNTS: await testDb("accounts"), LIBRARY: await testDb("library"), AUTH_SECRET: "local-feature-review", EMAIL: { send: async () => ({ messageId: "local" }) } };
  const token = "local-recommendation-review", account = "local-review";
  await env.ACCOUNTS.run("INSERT INTO entity_accounts (id, created_at, username) VALUES (?, ?, ?)", account, new Date().toISOString(), "review");
  await env.ACCOUNTS.run("INSERT INTO relation_account_login_identities (provider, subject, account, label, linked_at) VALUES (?, ?, ?, ?, ?)", "email", "review@example.invalid", account, "review@example.invalid", new Date().toISOString());
  await env.ACCOUNTS.run("INSERT INTO state_login_sessions (token_hash, account, expires_at) VALUES (?, ?, ?)", await sha256(token), account, "2099-01-01T00:00:00.000Z");
  let greeted = false;
  const server = createServer(async (incoming, outgoing) => {
    try {
      const url = new URL(incoming.url, "http://" + incoming.headers.host);
      const headers = new Headers();
      for (const [name, value] of Object.entries(incoming.headers)) if (value != null) headers.set(name, Array.isArray(value) ? value.join(", ") : value);
      const request = new Request(url, { method: incoming.method, headers,
        ...(!["GET", "HEAD"].includes(incoming.method) ? { body: Readable.toWeb(incoming), duplex: "half" } : {}) });
      let response;
      if (url.pathname === "/__fixture/settle" && controls) {
        const { day } = await env.ACCOUNTS.get("SELECT (recommendation_day() + 1)::text AS day");
        await env.ACCOUNTS.exec("CREATE OR REPLACE FUNCTION recommendation_day() RETURNS date LANGUAGE sql STABLE AS $$ SELECT '" + day + "'::date $$");
        response = new Response(null, { status: 302, headers: { location: "/account/recommendations" } });
      } else if (url.pathname === "/__fixture/login") {
        response = new Response(null, { status: 302, headers: { location: "/weapons/Laetum", "set-cookie": "wfsim_session=" + token + "; Path=/; HttpOnly; SameSite=Lax" } });
      } else if (url.pathname === "/api/account/devices") {
        response = Response.json({ ok: true, points: 128640, recent: 128640, week: 128640, devices: [], ranks: {}, named: false, decided: true });
      } else if (url.pathname.startsWith("/api/recommendations") || url.pathname.startsWith("/api/account/recommendations")) {
        response = await recommendationsRoute(request, env, url.pathname);
      } else if (url.pathname === "/api/account/agents") response = await agentRoute(request, env, url.pathname);
      else if (url.pathname.startsWith("/api/auth/") || url.pathname === "/api/account") response = await accountRoute(request, env, url.pathname);
      if (!response) {
        const proxyHeaders = new Headers(headers); proxyHeaders.delete("host");
        response = await fetch(new URL(incoming.url, base), { method: incoming.method, headers: proxyHeaders,
          ...(!["GET", "HEAD"].includes(incoming.method) ? { body: request.body, duplex: "half" } : {}), redirect: "manual" });
      }
      const responseHeaders = new Headers(response.headers);
      let bytes = Buffer.from(await response.arrayBuffer());
      if (controls && responseHeaders.get("content-type")?.includes("text/html")) {
        const tools = `<aside style="position:fixed;bottom:10px;left:50%;transform:translateX(-50%);z-index:999;display:flex;flex-wrap:wrap;gap:8px;align-items:center;max-width:calc(100vw - 20px);padding:8px 12px;border:1px solid var(--line);border-radius:9px;background:var(--surface);font-size:11px;color:var(--muted)">本地测试数据 <a class="ghost-btn small" href="/__fixture/settle">模拟 UTC 0 点</a><a class="ghost-btn small" href="/__fixture/login">测试登录</a></aside>`;
        bytes = Buffer.from(bytes.toString().replace("</body>", tools + "</body>"));
        responseHeaders.delete("content-length"); responseHeaders.delete("content-encoding");
      }
      if (controls && url.pathname.startsWith("/api/recommendations/heat/")) responseHeaders.set("cache-control", "no-store");
      if (!greeted && responseHeaders.get("content-type")?.includes("text/html")) {
        greeted = true;
        responseHeaders.append("set-cookie", "wfsim_session=" + token + "; Path=/; HttpOnly; SameSite=Lax");
      }
      outgoing.writeHead(response.status, Object.fromEntries(responseHeaders));
      outgoing.end(bytes);
    } catch (error) {
      console.error(error);
      outgoing.writeHead(500, { "content-type": "application/json" });
      outgoing.end(JSON.stringify({ ok: false, error: String(error.message || error) }));
    }
  });
  await new Promise(resolve => server.listen(port, "127.0.0.1", resolve));
  return {
    base: "http://127.0.0.1:" + server.address().port, env, token, account,
    async seed(identity, record, ruler, mode) {
      await env.LIBRARY.run("INSERT INTO entity_weapon_builds (id, at, record) VALUES (?, ?, ?) ON CONFLICT DO NOTHING", identity, new Date().toISOString().slice(0, 10), JSON.stringify(record));
      await env.LIBRARY.run(`INSERT INTO state_board_measurement_selection (identity, ruler, mode, measured_by, score, metric, cost_seconds, started_at, finished_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT DO NOTHING`, identity, ruler, mode, "local-review", 100, "kpm", 0, "2026-11-04", "2026-11-04");
    },
    async day(day) {
      if (!/^\d{4}-\d\d-\d\d$/.test(day)) throw new Error("Invalid fixture day");
      await env.ACCOUNTS.exec("CREATE OR REPLACE FUNCTION recommendation_day() RETURNS date LANGUAGE sql STABLE AS $$ SELECT '" + day + "'::date $$");
    },
    async close() {
      server.closeAllConnections(); await new Promise(resolve => server.close(resolve));
      await env.ACCOUNTS.close(); await env.LIBRARY.close();
    },
  };
}
