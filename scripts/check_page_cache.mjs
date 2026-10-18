// THE PAGE CACHE ANSWERS A SLOW ROAD AND NEVER A SOUND ONE (docs/DISTRIBUTION.md
// §"The page cache"). A server of its own serves a page naming one app.js and
// the real `web/src/static/page_cache.js`; the page is then held back on the
// network and the check reads which release answered and how soon.
//
//   node scripts/check_page_cache.mjs
import { createServer } from "node:http";
import { existsSync, readFileSync } from "node:fs";
import { openApp, sleep } from "./cdp.mjs";

const WORKER = readFileSync(new URL("../web/src/static/page_cache.js", import.meta.url), "utf8");
const SLOW_MS = 6000;
let servedWorker = WORKER;
const site = { release: "r1", app: "aaaaaaaaaaaa", pageDelay: 0 };
const html = () => `<!doctype html><title>t</title><body data-release="${site.release}">`
  + `<script src="/asset/app.${site.app}.js"></script></body>`;

const srv = createServer(async (q, s) => {
  const path = q.url.split("?")[0];
  if (path === "/page_cache.js") {
    s.writeHead(200, { "content-type": "text/javascript", "cache-control": "no-store" });
    return s.end(servedWorker);
  }
  if (path.startsWith("/asset/app.")) {
    s.writeHead(200, { "content-type": "text/javascript", "cache-control": "no-store" });
    return s.end(`window.APP_RAN = ${JSON.stringify(path)};`);
  }
  if (site.pageDelay) await sleep(site.pageDelay);
  s.writeHead(200, { "content-type": "text/html", "cache-control": "no-store" });
  s.end(html());
});
await new Promise((r) => srv.listen(0, "127.0.0.1", r));
const app = await openApp({ base: `http://127.0.0.1:${srv.address().port}` });
// THE BUILT SITE SERVES THIS FILE AT ITS ROOT, where a scope of `/` needs it.
const built = new URL("../site/page_cache.js", import.meta.url);
if (existsSync(new URL("../site/index.html", import.meta.url))) {
  app.check("site/ serves the page cache as written", existsSync(built) && readFileSync(built, "utf8") === WORKER);
}

/// Navigate and return which release answered and how long it took.
async function visit() {
  const t0 = Date.now();
  await app.send("Page.navigate", { url: `${app.BASE}/` });
  for (;;) {
    const r = await app.evaluate(`document.body && document.body.dataset.release || ""`).catch(() => "");
    if (r) return { release: r, ms: Date.now() - t0 };
    await sleep(50);
  }
}
const kept = () => app.evaluate(`(async () => {
  const out = {};
  for (const name of await caches.keys()) out[name] = (await (await caches.open(name)).keys()).map((r) => new URL(r.url).pathname);
  return out;
})()`);

await visit();
await app.evaluate(`navigator.serviceWorker.register("/page_cache.js").then(() => navigator.serviceWorker.ready).then(() => true)`);
await visit();
await sleep(300);
let held = await kept();
app.check("a page visited under the worker is kept with its app.js",
  (held["wfsim-pages-1"] || []).includes("/") && (held["wfsim-files-1"] || []).includes(`/asset/app.${site.app}.js`),
  JSON.stringify(held));

site.pageDelay = 0;
let v = await visit();
app.check("a sound road is answered by the network", v.release === "r1" && v.ms < 1500, JSON.stringify(v));

site.release = "r1-slow";
site.pageDelay = SLOW_MS;
v = await visit();
app.check("a slow road is answered by the kept copy, well before the network",
  v.release === "r1" && v.ms < SLOW_MS - 2000, JSON.stringify(v));
await sleep(SLOW_MS);

site.pageDelay = 0;
site.release = "r2";
site.app = "bbbbbbbbbbbb";
v = await visit();
await sleep(300);
held = await kept();
app.check("a new release replaces the copy and its app.js is kept",
  v.release === "r2" && (held["wfsim-files-1"] || []).includes(`/asset/app.${site.app}.js`), JSON.stringify({ v, held }));

site.pageDelay = SLOW_MS;
v = await visit();
app.check("the slow road then gets the new release's copy", v.release === "r2" && v.ms < SLOW_MS - 2000, JSON.stringify(v));
await sleep(SLOW_MS);

await app.evaluate(`caches.open("wfsim-files-1").then((c) => c.delete("/asset/app.${site.app}.js"))`);
site.release = "r2-network";
v = await visit();
app.check("a copy whose app.js is not kept is never answered: the page waits for the network",
  v.release === "r2-network" && v.ms >= SLOW_MS - 500, JSON.stringify(v));

site.pageDelay = 0;
await visit();
await sleep(300);
await app.evaluate(`(async () => { for (const n of await caches.keys()) if (n.startsWith("wfsim-pages-")) await caches.delete(n); })()`);
site.pageDelay = SLOW_MS;
site.release = "r2-forgotten";
v = await visit();
app.check("with the kept pages forgotten, as before a release reload, the page waits for the network",
  v.release === "r2-forgotten" && v.ms >= SLOW_MS - 500, JSON.stringify(v));

// THE WAY BACK: the same file shipped with RETIRED true leaves nothing behind.
const retired = WORKER.replace("const RETIRED = false;", "const RETIRED = true;");
app.check("the worker states its way back", retired !== WORKER);
servedWorker = retired;
site.pageDelay = 0;
await visit();
await app.evaluate(`navigator.serviceWorker.getRegistration().then((r) => r && r.update())`);
await sleep(1500);
const left = await app.evaluate(`(async () => ({
  registrations: (await navigator.serviceWorker.getRegistrations()).length,
  stores: (await caches.keys()).filter((n) => n.startsWith("wfsim-")),
}))()`);
app.check("a retired worker unregisters itself and drops both stores",
  left.registrations === 0 && left.stores.length === 0, JSON.stringify(left));

srv.close();
await app.finish("the page cache answers a slow road and never a sound one");
