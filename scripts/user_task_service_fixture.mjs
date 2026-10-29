// The real task service and factory, isolated from production behind a static frontend.
import { createServer } from "node:http";
import { resolve } from "node:path";
import { serveSite } from "./cdp.mjs";
import { testDb } from "./pg/test_db.mjs";
import { userTasksRoute } from "../worker/user_tasks.js";
import { syncRoute } from "../worker/sync.js";
import { verifyRoute } from "../worker/verify.js";
import { facts, workOf } from "../worker/factory.js";

export async function taskServiceFixture() {
  const frontend = process.env.WFSIM_BASE ? null : await serveSite(resolve("site"));
  const upstream = process.env.WFSIM_BASE || `http://127.0.0.1:${frontend.address().port}`;
  const release = await fetch(upstream + "/release.json").then((response) => response.json());
  const db = await testDb("library");
  const env = { LIBRARY: db, ASSETS: { fetch: async () => Response.json(release) } };
  let base;
  const server = createServer(async (request, response) => {
    try {
      const path = new URL(request.url, "http://localhost").pathname;
      if (path.startsWith("/api/user-tasks/") || ["/test/claim", "/api/board/sync", "/api/board/work", "/api/board/renew", "/api/board/release"].includes(path)) {
        const chunks = []; for await (const chunk of request) chunks.push(chunk);
        const raw = Buffer.concat(chunks).toString();
        let body = {}; try { body = JSON.parse(raw); } catch (_) { /* GET has no body */ }
        // Two test witnesses model separate devices and networks.
        if (path === "/test/claim") {
          const leased = await facts().next(env, { device: body.verifier, owner: body.verifier,
            siblings: [body.verifier, "cap:calculation-reports-v1"], net: "test-" + body.verifier }, release.engine);
          response.writeHead(200, { "content-type": "application/json" }); response.end(JSON.stringify(leased ? workOf(leased) : null)); return;
        }
        const headers = new Headers(request.headers);
        if (body.verifier) headers.set("cf-connecting-ip", "test-" + body.verifier);
        const incoming = new Request(base + request.url, { method: request.method, headers, ...(request.method === "POST" ? { body: raw } : {}) });
        const reply = path.startsWith("/api/user-tasks/") ? await userTasksRoute(incoming, env, path)
          : path === "/api/board/sync" ? await syncRoute(incoming, env) : await verifyRoute(incoming, env, path);
        response.writeHead(reply.status, Object.fromEntries(reply.headers)); response.end(Buffer.from(await reply.arrayBuffer())); return;
      }
      if (path.startsWith("/api/") && request.method !== "GET") { response.writeHead(404); response.end(); return; }
      const reply = await fetch(upstream + request.url, { method: request.method });
      response.writeHead(reply.status, Object.fromEntries([...reply.headers].filter(([key]) => !["content-encoding", "content-length"].includes(key))));
      response.end(Buffer.from(await reply.arrayBuffer()));
    } catch (error) { response.writeHead(500, { "content-type": "application/json" }); response.end(JSON.stringify({ error: error.message })); }
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  base = `http://127.0.0.1:${server.address().port}`;
  return { base, async close() { server.closeAllConnections(); server.close(); frontend?.closeAllConnections(); frontend?.close(); await db.close(); } };
}
