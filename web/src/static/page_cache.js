// SPDX-License-Identifier: AGPL-3.0-or-later
// THE PAGE CACHE — a service worker for wfsim.app alone (docs/DISTRIBUTION.md
// §"The page cache"). A page is asked of the network first and answered from
// the copy kept here once the network has taken PAGE_WAIT_MS; a content-
// addressed file is answered from here first. Nothing else is touched: the api,
// the board and the art pass through as if this file did not exist.

/// THE WAY BACK: shipped as `true`, every browser that checks for an update
/// (each visit, and at least daily) drops both stores and unregisters this
/// worker, and the site is then served exactly as if it had never existed.
const RETIRED = false;
const PAGES = "wfsim-pages-1";
const FILES = "wfsim-files-1";
/// HOW LONG A PAGE WAITS ON THE NETWORK before the kept copy answers. Above a
/// sound road's whole answer from any continent, so only a dropping road falls
/// back, and the network's answer still replaces the copy for next time.
const PAGE_WAIT_MS = 1500;
/// A COPY OLDER THAN THIS IS NOT ANSWERED: the files it names may be gone from
/// the site, and a page whose app.js is a 404 never starts.
const PAGE_KEEP_MS = 7 * 86_400_000;
/// THE PREFIXES THAT ARE CONTENT-ADDRESSED (worker/pages.js `HASHED`): a name
/// there never changes meaning, so a kept copy is the file.
const HASHED = ["/asset/", "/pkg/", "/lib/"];
/// HOW MANY FILES are kept, the oldest put dropped first: a release adds about
/// ten, so the engines of the last few releases stay.
const FILES_KEEP = 40;
/// WHICH RELEASE A PAGE IS, by the app.js it names. Pages are kept for ONE
/// release: a copy of another names files this store may no longer hold.
const APP_JS = /\/asset\/app\.[0-9a-f]+\.js/;
const KEPT_AT = "x-wfsim-kept-at";
const KEPT_APP = "x-wfsim-kept-app";

self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (event) => {
  event.waitUntil((async () => {
    for (const name of await caches.keys()) {
      if (name.startsWith("wfsim-") && (RETIRED || (name !== PAGES && name !== FILES))) await caches.delete(name);
    }
    if (RETIRED) return self.registration.unregister();
    if (self.registration.navigationPreload) await self.registration.navigationPreload.enable();
    await self.clients.claim();
  })());
});

const keepable = (r) => r && r.status === 200 && !r.redirected && r.type !== "opaque";

async function keepPage(key, response) {
  if (!keepable(response) || !(response.headers.get("content-type") || "").includes("text/html")) return;
  const body = await response.text();
  const app = (body.match(APP_JS) || [""])[0];
  if (!app) return;
  const headers = new Headers(response.headers);
  headers.set(KEPT_AT, String(Date.now()));
  headers.set(KEPT_APP, app);
  const cache = await caches.open(PAGES);
  for (const k of await cache.keys()) {
    const old = await cache.match(k);
    if (!old || old.headers.get(KEPT_APP) !== app) await cache.delete(k);
  }
  await cache.put(key, new Response(body, { status: 200, headers }));
}

/// …AND ONLY WITH ITS app.js KEPT BESIDE IT, which is what makes the copy start
/// with no network at all.
async function keptPage(key) {
  const kept = await caches.match(key, { cacheName: PAGES, ignoreVary: true });
  if (!kept || !(Date.now() - Number(kept.headers.get(KEPT_AT)) < PAGE_KEEP_MS)) return null;
  const app = new URL(kept.headers.get(KEPT_APP), self.location.origin).href;
  return (await caches.match(app, { cacheName: FILES })) ? kept : null;
}

/// A PAGE: the network's answer if it comes within PAGE_WAIT_MS or there is no
/// copy, else the copy — and the network's answer, whenever it comes, is kept.
function page(event) {
  const url = new URL(event.request.url);
  const key = url.origin + url.pathname;
  const network = (async () => {
    const r = (await event.preloadResponse) || (await fetch(event.request));
    event.waitUntil(keepPage(key, r.clone()).catch(() => {}));
    return r;
  })();
  event.waitUntil(network.catch(() => {}));
  return (async () => {
    const kept = await keptPage(key);
    if (!kept) return network;
    let timer;
    const late = new Promise((resolve) => { timer = setTimeout(() => resolve(kept), PAGE_WAIT_MS); });
    return Promise.race([network.then((r) => (r.ok ? r : kept), () => kept), late]).finally(() => clearTimeout(timer));
  })();
}

async function keepFile(key, response) {
  if (!keepable(response)) return;
  const cache = await caches.open(FILES);
  await cache.put(key, response);
  const keys = await cache.keys();
  for (const k of keys.slice(0, Math.max(0, keys.length - FILES_KEEP))) await cache.delete(k);
}

async function file(event) {
  const kept = await caches.match(event.request.url, { cacheName: FILES, ignoreVary: true });
  if (kept) return kept;
  const r = await fetch(event.request);
  event.waitUntil(keepFile(event.request.url, r.clone()).catch(() => {}));
  return r;
}

self.addEventListener("fetch", (event) => {
  const req = event.request;
  if (RETIRED || req.method !== "GET") return;
  const url = new URL(req.url);
  if (url.origin !== self.location.origin) return;
  if (req.mode === "navigate") return event.respondWith(page(event));
  if (HASHED.some((p) => url.pathname.startsWith(p)) && !req.headers.has("range")) event.respondWith(file(event));
});
