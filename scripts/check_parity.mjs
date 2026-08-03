// Do the BUILDER and the OPTIMIZER offer the same thing?
//
// They are the same question asked twice — the builder fills a weapon's slots,
// the optimizer searches them — so every axis must present the same options
// and the same visibility on both sides. `weaponAxes()` in app.js exists to
// make that true by construction; this checks that it stayed true.
//
// It has caught, in the two hours it took to write the thing it checks:
//   - the optimizer offering an Exilus and an Arcanes scope on a sentinel
//     weapon, which has neither
//   - the Larkspur being given an exilus slot with no mod that can enter it
//   - the two modules computing the exilus pool from different sources
//     (`poolWithRivens()` vs `currentPool`), agreeing only by coincidence
//
// Usage:
//   node scripts/check_parity.mjs                 serves site/ itself
//   node scripts/check_parity.mjs http://host:port  against a running server
//
// Exits non-zero on any mismatch, so it can gate a push.
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { extname, join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

// Where Chrome lives differs per platform and per CI image, so try the known
// places rather than betting on one. `CHROME=` overrides all of it.
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
const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "site");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ---- a static server for site/, with the SPA fallback the app needs -------
const MIME = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css",
  ".json": "application/json", ".wasm": "application/wasm", ".svg": "image/svg+xml",
  ".png": "image/png", ".jpg": "image/jpeg", ".ico": "image/x-icon" };
async function serve() {
  const srv = createServer(async (req, res) => {
    const p = decodeURIComponent(req.url.split("?")[0]);
    let file = join(ROOT, p);
    try {
      const body = await readFile(file);
      res.writeHead(200, { "content-type": MIME[extname(file)] || "application/octet-stream",
        "cache-control": "no-store" });
      res.end(body);
    } catch {
      // Unknown path with no extension = an SPA route.
      try {
        res.writeHead(200, { "content-type": "text/html", "cache-control": "no-store" });
        res.end(await readFile(join(ROOT, "index.html")));
      } catch { res.writeHead(404).end(); }
    }
  });
  await new Promise((r) => srv.listen(0, "127.0.0.1", r));
  return { url: `http://127.0.0.1:${srv.address().port}`, close: () => srv.close() };
}

// ---- headless Chrome over CDP --------------------------------------------
async function browser(port) {
  const proc = spawn(CHROME, [`--remote-debugging-port=${port}`, "--headless=new",
    "--disable-gpu", "--no-first-run", "--no-default-browser-check",
    // A CI runner has no usable user namespace for Chrome's sandbox. The page
    // is local content we generated, so this costs nothing there and is not
    // enabled anywhere else.
    ...(process.env.CI ? ["--no-sandbox", "--disable-dev-shm-usage"] : []),
    `--user-data-dir=${process.env.TEMP || "/tmp"}/wfsim-parity-${port}`, "about:blank"],
    { stdio: "ignore" });
  let page = null;
  for (let i = 0; i < 80 && !page; i++) {
    try {
      const r = await fetch(`http://127.0.0.1:${port}/json/list`);
      if (r.ok) page = (await r.json()).find((t) => t.type === "page");
    } catch { /* not up yet */ }
    if (!page) await sleep(250);
  }
  if (!page) {
    throw new Error(`chrome did not start (tried ${CHROME}) — set CHROME to its path`);
  }
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  let id = 0;
  const pending = new Map();
  ws.onmessage = (e) => {
    const m = JSON.parse(e.data);
    if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
  };
  await new Promise((r) => (ws.onopen = r));
  const send = (method, params = {}) =>
    new Promise((res) => { const i = ++id; pending.set(i, res); ws.send(JSON.stringify({ id: i, method, params })); });
  await send("Page.enable");
  await send("Runtime.enable");
  const evaluate = async (expression) => {
    const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    if (r.result?.exceptionDetails) {
      throw new Error(String(r.result.exceptionDetails.exception?.description || "").slice(0, 300));
    }
    return r.result?.result?.value;
  };
  return { send, evaluate, close: () => { ws.close(); proc.kill(); } };
}

// ---- the comparison ------------------------------------------------------
// Both sides are read from the SAME page by calling the functions each module
// actually calls, so this compares behaviour and not a screenshot.
const PROBE = `(async () => {
  const S = (a) => [...new Set(a)].sort();
  const out = [];
  for (const w of META.weapons) {
    switchWeapon(w.id);
    await new Promise((r) => setTimeout(r, 120));
    const AX = weaponAxes(w.id);
    out.push({
      weapon: w.id,
      axes: {
        mods: S(AX.mods.map((m) => m.id)),
        exilus: S(AX.exilus.map((m) => m.id)),
        arcanes: AX.arcanes.map((a) => S(a.options.map((x) => x.id))),
        evolutions: AX.evolutions.map((t) => S(t.options.map((o) => o.id))),
      },
      // THE EXILUS SLOT'S OWN POLARITY. The server sends nine innate slots —
      // eight main plus the exilus — and the client used to slice the ninth
      // off and pad a null over it, so a weapon that comes with an exilus
      // polarity showed an empty one: full drain for the mod in it, and a
      // Forma charged for something the weapon already has.
      innate: { client: innate.slice(), served: (w.innate_polarities || []).slice() },
      // What each module INDEPENDENTLY decides to show.
      shown: {
        builder: { exilus: AX.hasExilus, arcanes: AX.arcanes.length > 0,
                   evolutions: AX.evolutions.length > 0 },
      },
    });
  }
  return out; })()`;

const VISIBLE = `(() => {
  const v = (id) => { const e = document.getElementById(id); return !!e && !e.hidden; };
  return { exilus: v("exilus-block"), arcanes: v("arcane-block"), evolutions: v("evo-block") };
})()`;
const VISIBLE_OPT = `(() => {
  const v = (id) => { const e = document.getElementById(id); return !!e && !e.hidden; };
  return { exilus: v("opt-exilus-sect"), arcanes: v("opt-arcanes-sect"), evolutions: v("opt-evos-sect") };
})()`;

const base = process.argv[2];
const server = base ? null : await serve();
const url = base || server.url;
const b = await browser(9333);
let bad = 0;
try {
  await b.send("Page.navigate", { url: url + "/" });
  await sleep(13000);
  const rows = await b.evaluate(PROBE);
  for (const r of rows) {
    const notes = [];
    for (const [k, v] of Object.entries(r.axes)) {
      const n = Array.isArray(v[0]) ? v.map((x) => x.length).join(",") : v.length;
      notes.push(`${k} ${n}`);
    }
    // The two modules render their own visibility; read BOTH pages for real.
    await b.send("Page.navigate", { url: `${url}/weapons/${r.weapon}` });
    await sleep(1500);
    const shownBuilder = await b.evaluate(VISIBLE);
    await b.send("Page.navigate", { url: `${url}/weapons/${r.weapon}/optimizer` });
    await sleep(1500);
    const shownOpt = await b.evaluate(VISIBLE_OPT);
    const diffs = Object.keys(shownBuilder)
      .filter((k) => shownBuilder[k] !== shownOpt[k])
      .map((k) => `${k}: builder ${shownBuilder[k]} vs optimizer ${shownOpt[k]}`);
    // An axis that is SHOWN must have options, and one with options must show.
    for (const k of ["exilus", "arcanes", "evolutions"]) {
      const has = k === "exilus" ? r.axes.exilus.length > 0 : r.axes[k].length > 0;
      if (has !== shownBuilder[k]) diffs.push(`${k}: has options ${has} but builder shows ${shownBuilder[k]}`);
    }
    // Nothing the server said about polarities may be lost on the way in.
    const { client, served } = r.innate;
    if (JSON.stringify(client) !== JSON.stringify(served)) {
      diffs.push(`innate polarities: client ${JSON.stringify(client)} vs served ${JSON.stringify(served)}`);
    }
    notes.push(`exilus pol ${client[8] || "—"}`);
    console.log(`${r.weapon.padEnd(20)} ${notes.join("  ").padEnd(66)} ${diffs.length ? "MISMATCH" : "ok"}`);
    diffs.forEach((d) => console.log("    " + d));
    bad += diffs.length;
  }
} finally {
  b.close();
  server?.close();
}
console.log(bad ? `\n${bad} mismatch(es)` : "\nbuilder and optimizer agree on every axis");
process.exit(bad ? 1 : 0);
