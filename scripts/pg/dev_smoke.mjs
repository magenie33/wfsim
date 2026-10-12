// The worker in workerd (`wrangler dev`) with FACTORY_DB=pg, its Hyperdrive a
// local connection to the server's scratch database through the ssh tunnel on
// localhost:15432: the vendored Cloudflare build of postgres.js, asked a few
// read doors. Secrets read, never printed.
import { readFileSync, writeFileSync, rmSync } from "node:fs";
import { spawn, execFileSync } from "node:child_process";
const read = (f) => Object.fromEntries(readFileSync(f, "utf8").split(/\r?\n/).filter((l) => l.includes("="))
  .map((l) => [l.slice(0, l.indexOf("=")).trim(), l.slice(l.indexOf("=") + 1).trim()]));
const pg = read(process.argv[2]);
const url = `postgres://${pg.PG_USER}:${encodeURIComponent(pg.PG_PASSWORD)}@localhost:15432/wfsim_scratch`;
const npx = process.platform === "win32" ? "npx.cmd" : "npx";
execFileSync(npx, ["wrangler", "d1", "execute", "wfsim", "--local", "--file", "worker/schema.sql"], { stdio: "ignore", shell: true });
execFileSync(npx, ["wrangler", "d1", "execute", "wfsim-accounts", "--local", "--file", "worker/accounts.sql"], { stdio: "ignore", shell: true });
const config = "wrangler.smoke.jsonc";
writeFileSync(config, readFileSync("wrangler.jsonc", "utf8").replace('"main": "worker/index.js"', '"main": "scripts/pg/smoke/entry.js"'));
const dev = spawn(npx, ["wrangler", "dev", "--local", "--config", config, "--port", "41399", "--inspector-port", "41400", "--var", "FACTORY_DB:pg"], { shell: true,
  env: { ...process.env, CLOUDFLARE_HYPERDRIVE_LOCAL_CONNECTION_STRING_HYPERDRIVE: url } });
let log = "";
dev.stdout.on("data", (d) => { log += d; });
dev.stderr.on("data", (d) => { log += d; });
const until = Date.now() + 120_000;
while (!/Ready on/.test(log) && Date.now() < until) await new Promise((r) => setTimeout(r, 1000));
const hide = (s) => String(s).replaceAll(pg.PG_PASSWORD, "<hidden>");
try {
  const post = async (path, body) => (await fetch(`http://localhost:41399${path}`, { method: "POST", headers: { "content-type": "application/json", origin: "http://localhost:41399" }, body: JSON.stringify(body) })).text();
  const get = async (path) => (await fetch(`http://localhost:41399${path}`)).text();
  const [v] = (await (await import("./node_modules/postgres/src/index.js")).default(url).unsafe("SELECT id FROM verifiers WHERE work > 0 ORDER BY work DESC LIMIT 1"));
  console.log("points    ", hide(await post("/api/board/points", { verifier: v ? v.id : "0".repeat(24) })).slice(0, 300));
  console.log("ledger    ", hide(await post("/api/board/ledger", { verifier: v ? v.id : "0".repeat(24), from: "" })).slice(0, 300));
  console.log("computing ", hide(await get("/api/board/computing")).slice(0, 300));
  console.log("work      ", hide(await post("/api/board/work", { verifier: "a".repeat(24), engine: "e1", protocol: 7, consent: { v: 1, at: "2026-10-01T00:00:00.000Z" }, lanes: 1 })).slice(0, 300));
} catch (e) {
  console.log("ERROR", hide(e.message));
} finally {
  if (!/Ready on/.test(log)) console.log(hide(log).slice(-1500));
  const errs = hide(log).split("\n").filter((l) => /error|Error/.test(l)).slice(0, 8);
  if (errs.length) console.log("dev log errors:\n" + errs.join("\n"));
  if (process.platform === "win32") execFileSync("taskkill", ["/T", "/F", "/PID", String(dev.pid)], { stdio: "ignore" });
  else dev.kill();
  rmSync(config, { force: true });
  process.exit(0);
}
