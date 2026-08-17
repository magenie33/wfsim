// THE PLUMBING EVERY UI CHECK NEEDS, written once.
//
// Every `check_*.mjs` drives the SHIPPING build in headless Chrome over CDP and
// asserts real DOM state. Getting to the first assertion takes a static server
// for `site/`, a Chrome launch, a WebSocket, a request/response map, an
// `evaluate` helper and a `check` counter — thirty to ninety lines that were
// copied into eighteen files and then drifted: some `evaluate`s threw on a page
// exception and some returned undefined, some servers resolved a directory to
// its `index.html` and some 404'd it into the SPA fallback, and every file
// picked its own debugging port by hand, so two checks running at once could
// fight over one. None of that is what any check is about.
//
// The escaping is the other reason. A check's page-side code is a string inside
// a template literal, and a backslash that does not survive the trip turns
// `/\s+/` into `/s+/` — which silently rewrote "Winds" to "Wind " and looked
// like an app bug for as long as it took to find. One place to get that right
// is better than eighteen.
//
// Usage:
//
//   import { openApp } from "./cdp.mjs";
//   const app = await openApp({ lang: "zh" });
//   const v = await app.evaluate(`(async () => { ... })()`);
//   app.check("it says so", v.ok === true, JSON.stringify(v));
//   await app.finish("the thing holds");   // exits non-zero if anything failed
import { spawn, spawnSync } from "node:child_process";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { readFileSync, readdirSync, rmSync, existsSync, statSync } from "node:fs";
import { extname, join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const MIME = {
  ".html": "text/html", ".js": "text/javascript", ".css": "text/css",
  ".json": "application/json", ".wasm": "application/wasm", ".svg": "image/svg+xml",
  ".png": "image/png", ".jpg": "image/jpeg", ".ico": "image/x-icon",
  ".webmanifest": "application/manifest+json", ".txt": "text/plain", ".xml": "application/xml",
};

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// Where Chrome lives differs per platform and per CI image, so try the known
// places rather than betting on one. `CHROME=` overrides all of it. (Taken
// from `check_parity`, which was the only copy that got this right.)
const CHROME_CANDIDATES = process.platform === "win32"
  ? ["C:/Program Files/Google/Chrome/Application/chrome.exe",
     "C:/Program Files (x86)/Google/Chrome/Application/chrome.exe"]
  : process.platform === "darwin"
    ? ["/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"]
    : ["/usr/bin/google-chrome", "/usr/bin/google-chrome-stable",
       "/usr/bin/chromium-browser", "/usr/bin/chromium", "/snap/bin/chromium"];
const CHROME = process.env.CHROME
  || CHROME_CANDIDATES.find((p) => existsSync(p))
  || CHROME_CANDIDATES[0];

/// `site/` relative to THIS FILE, not to the shell's working directory — a
/// check should behave the same whether it is run from the repo root or not.
const SITE = resolve(dirname(fileURLToPath(import.meta.url)), "..", "site");

/// Serve `site/` the way Cloudflare Pages does: a real file wins, a directory
/// falls to its `index.html`, and anything else is the SPA shell. That last rule
/// is what lets a check push `/weapons/Torid` into the address bar; the
/// directory rule is what makes the PRERENDERED weapon pages reachable, which a
/// check of the crawler's view needs and a fallback-only server hides.
async function serveSite(root) {
  const srv = createServer(async (q, s) => {
    const p = decodeURIComponent(q.url.split("?")[0]);
    for (const c of [p, join(p, "index.html")]) {
      try {
        const b = await readFile(join(root, c));
        s.writeHead(200, {
          "content-type": MIME[extname(c)] || "application/octet-stream",
          "cache-control": "no-store",
        });
        return s.end(b);
      } catch { /* try the next candidate */ }
    }
    s.writeHead(200, { "content-type": "text/html" });
    s.end(await readFile(join(root, "index.html")));
  });
  await new Promise((r) => srv.listen(0, "127.0.0.1", r));
  return srv;
}

/// Chrome picks the debugging port and writes it into its own profile.
///
/// Every check used to hardcode one, which meant a fixed list nobody could keep
/// distinct and two checks running at once racing for the same number.
/// `--remote-debugging-port=0` plus `DevToolsActivePort` is Chrome's own answer.
async function debugPort(profile) {
  const file = join(profile, "DevToolsActivePort");
  for (let i = 0; i < 120; i++) {
    try {
      const line = readFileSync(file, "utf8").split("\n")[0].trim();
      if (line) return Number(line);
    } catch { /* not written yet */ }
    await sleep(250);
  }
  throw new Error("Chrome never reported a debugging port");
}

/// EVERY OLD PROFILE, ON THE WAY IN — the one cleanup that cannot be skipped.
///
/// `finish()` removes its own directory, and that was the whole plan until a
/// machine turned up with 644 of them and 17 GB of C: gone (owner,
/// 2026-08-10). Two ways they survive and neither is exotic:
///
/// - on Windows Chrome still holds handles for a moment after `kill()`, so the
///   `rmSync` in `finish` throws and the comment there says "it can stay" —
///   which on this platform is the NORMAL path, not the edge case;
/// - a script that throws, is interrupted, or is an ad-hoc probe that never
///   calls `finish()` never reaches the cleanup at all.
///
/// Sweeping on the way IN fixes both, because by then the run that made the
/// directory is long gone and nothing holds it. An hour is the grace period: a
/// check takes under a minute, so anything older than that belongs to a run
/// that is over, and a concurrent check's own directory is never old enough to
/// be caught.
function sweepStaleProfiles() {
  const tmp = process.env.TEMP || "/tmp";
  const cutoff = Date.now() - 60 * 60 * 1000;
  let names = [];
  try { names = readdirSync(tmp); } catch { return; }
  for (const n of names) {
    if (!n.startsWith("wfsim-")) continue;
    const p = join(tmp, n);
    try {
      if (statSync(p).mtimeMs > cutoff) continue;
      rmSync(p, { recursive: true, force: true });
    } catch { /* someone else's, or still held — the next run tries again */ }
  }
}

/**
 * Boot the shipping build in headless Chrome and hand back the handles a check
 * needs.
 *
 * @param {object}  [o]
 * @param {string}  [o.lang]    "en" | "zh" — set BEFORE the app boots, so the
 *                              page is never drawn once in the wrong language
 *                              and then re-rendered.
 * @param {number}  [o.boot]    ms to wait for the wasm to come up.
 * @param {string}  [o.profile] Chrome user-data-dir name (defaults to the
 *                              calling script's own, so checks never share).
 * @param {string}  [o.root]    directory to serve (defaults to `site/`).
 * @param {string}  [o.base]    an EXTERNAL origin to test instead of `site/` —
 *                              no server is started and `root` is ignored.
 */
export async function openApp(o = {}) {
  const root = o.root ? resolve(o.root) : SITE;
  const name = o.profile
    || `wfsim-${(process.argv[1] || "check").split(/[\\/]/).pop().replace(/\.mjs$/, "")}`;
  // A PROFILE PER RUN, removed on the way out. Reusing one by name looked
  // tidier and cost two failures that had nothing to do with any check: a
  // `DevToolsActivePort` left by the previous run got read before Chrome
  // rewrote it ("no CDP target"), and a Chrome still shutting down still held
  // the directory's lock when the next check claimed it. Neither can happen to
  // a directory nobody else has ever used.
  const profile = join(process.env.TEMP || "/tmp", `${name}-${process.pid}`);
  sweepStaleProfiles();
  // An EXTERNAL base skips the server entirely, so a check can be pointed at
  // wfsim.app (or a preview deploy) and assert the same things about it.
  const srv = o.base ? null : await serveSite(root);
  const BASE = o.base || `http://127.0.0.1:${srv.address().port}`;

  const proc = spawn(CHROME, [
    "--remote-debugging-port=0", "--headless=new", "--disable-gpu",
    "--no-first-run", "--no-default-browser-check",
    // A CI runner has no usable user namespace for Chrome's sandbox. The page
    // is local content we generated, so this costs nothing there and is not
    // enabled anywhere else.
    ...(process.env.CI ? ["--no-sandbox", "--disable-dev-shm-usage"] : []),
    `--user-data-dir=${profile}`, "about:blank",
  ], { stdio: "ignore" });

  const port = await debugPort(profile);
  let page = null;
  for (let i = 0; i < 60 && !page; i++) {
    try {
      const r = await fetch(`http://127.0.0.1:${port}/json/list`);
      if (r.ok) page = (await r.json()).find((t) => t.type === "page");
    } catch { /* still starting */ }
    if (!page) await sleep(250);
  }
  if (!page) {
    throw new Error(`chrome did not start (tried ${CHROME}) — set CHROME to its path`);
  }

  const ws = new WebSocket(page.webSocketDebuggerUrl);
  let id = 0;
  const pending = new Map();
  const listeners = [];
  ws.onmessage = (e) => {
    const m = JSON.parse(e.data);
    if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); return; }
    for (const [method, fn] of listeners) if (m.method === method) fn(m.params);
  };
  await new Promise((r) => { ws.onopen = r; });

  const send = (method, params = {}) => new Promise((res) => {
    const i = ++id;
    pending.set(i, res);
    ws.send(JSON.stringify({ id: i, method, params }));
  });

  await send("Page.enable");
  await send("Runtime.enable");
  if (o.lang) {
    // BEFORE the boot, not after: setting it afterwards draws the page once in
    // the wrong language, and a check that reads the first render sees it.
    await send("Page.addScriptToEvaluateOnNewDocument", {
      source: `localStorage.setItem("wfsim-lang", ${JSON.stringify(o.lang)})`,
    });
  }

  const app = {
    BASE, send, sleep, failures: 0,

    /// Listen for a CDP event (`Runtime.exceptionThrown`, `Network.loadingFailed`).
    /// The domain has to be enabled by the caller — `Network.enable` is not on
    /// by default because most checks do not want the traffic.
    on(method, fn) { listeners.push([method, fn]); },

    /// Evaluate page-side code and return its value.
    ///
    /// A page-side exception THROWS here rather than returning undefined. Half
    /// the copies of this helper did that and half did not, so the same bug
    /// read as "the assertion is false" in one check and "the value is
    /// missing" in another.
    async evaluate(expr) {
      const r = await send("Runtime.evaluate", {
        expression: expr, awaitPromise: true, returnByValue: true,
      });
      const ex = r.result?.exceptionDetails;
      if (ex) {
        const msg = String(ex.exception?.description || ex.text || "page threw").slice(0, 800);
        // THE ONE MISTAKE THIS FILE'S SHAPE INVITES, named where it is found.
        //
        // A check's page-side half is a template literal, so a backtick inside
        // it — in a COMMENT, which is where the repo's idiom puts them around
        // identifiers — ends the literal early and splices whatever follows
        // into the expression. The result usually still PARSES, so `node
        // --check` is clean and the only symptom is the page throwing about a
        // method nothing calls. It has cost a full check cycle seven times
        // (2026-08-18); the hint costs three lines and cannot false-positive,
        // because it only ever appends to a failure that already happened.
        throw new Error(`${msg}\n\n  hint: if that names something this check ` +
          `never wrote, look for a backtick or \${ inside the page-side body ` +
          `of ${process.argv[1]} — it ends the template literal early.`);
      }
      return r.result?.result?.value;
    },

    /// (Re)load the app and wait for it to come up.
    async load(path = "/", wait) {
      await send("Page.navigate", { url: BASE + path });
      await sleep(wait ?? o.boot ?? 13000);
    },

    /// Switch language the way a returning visitor does — the setting is
    /// stored, then the app is reloaded so it boots into it.
    async setLang(lang, wait) {
      await this.evaluate(`localStorage.setItem("wfsim-lang", ${JSON.stringify(lang)})`);
      await this.load("/", wait);
    },

    check(name, ok, detail) {
      console.log(`${ok ? "  ok  " : "FAIL  "}${name}${ok || detail === undefined ? "" : `  — ${detail}`}`);
      if (!ok) app.failures++;
    },

    /// Print the closing line and leave with the right exit code. Always call
    /// it — it is what shuts Chrome and the server down.
    async finish(message) {
      const failed = app.failures;
      console.log(failed ? `\n${failed} failed` : `\n${message}`);
      try { ws.close(); } catch { /* already gone */ }
      // THE WHOLE TREE, not the launcher. Chrome forks a renderer, a gpu
      // process and more, and on Windows `kill()` reaches only the one node
      // spawned here — the children go on holding the profile, which is why
      // eight headless Chromes were still alive with a directory open after a
      // check had exited cleanly (2026-08-10). `taskkill /T` is the platform's
      // own answer; everywhere else the signal already reaches the group.
      try {
        if (process.platform === "win32") {
          spawnSync("taskkill", ["/pid", String(proc.pid), "/T", "/F"], { stdio: "ignore" });
        } else {
          proc.kill();
        }
      } catch { /* already gone */ }
      try { srv?.close(); } catch { /* already gone */ }
      // WAIT FOR CHROME TO LET GO, then retry. `kill()` returns immediately and
      // Windows releases the profile's handles a moment later, so removing it
      // on the next line failed EVERY time — which is how 644 directories and
      // 17 GB accumulated while a comment said the leftovers were harmless.
      // A second is longer than Chrome needs and shorter than anyone notices;
      // `sweepStaleProfiles` is the backstop when even that is not enough.
      await Promise.race([
        new Promise((r) => proc.once("exit", r)),
        sleep(1000),
      ]);
      for (let i = 0; i < 5; i++) {
        try {
          rmSync(profile, { recursive: true, force: true });
          break;
        } catch {
          await sleep(200);
        }
      }
      process.exit(failed ? 1 : 0);
    },
  };

  await app.load("/");
  return app;
}
