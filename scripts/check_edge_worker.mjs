// THE TWO WORKERS SERVE ONE SITE (docs/WASM.md §"Two workers"), offline.
// wrangler.edge.jsonc's assets block is wrangler.jsonc's, the edge is never
// placed and binds the worker the other config names, and `edgeServe` answers
// the site's files without the backend while passing on everything else.
import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { edgeServe } from "../worker/pages.js";
import edge from "../worker/edge.js";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
let failed = 0;
const check = (name, ok, detail = "") => {
  console.log(`${ok ? "  ok" : "FAIL"}  ${name}${ok ? "" : `  — ${String(detail).slice(0, 300)}`}`);
  if (!ok) failed++;
};

/// JSONC as JSON: a `//` outside a string starts a comment.
function jsonc(file) {
  const text = readFileSync(resolve(ROOT, file), "utf8");
  let out = "", str = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (str) {
      out += c;
      if (c === "\\") out += text[++i];
      else if (c === '"') str = false;
    } else if (c === '"') { str = true; out += c; }
    else if (c === "/" && text[i + 1] === "/") { while (i < text.length && text[i] !== "\n") i++; out += "\n"; }
    else out += c;
  }
  return JSON.parse(out);
}

const back = jsonc("wrangler.jsonc");
const front = jsonc("wrangler.edge.jsonc");
check("the edge serves the same assets, routed the same way",
  JSON.stringify(front.assets) === JSON.stringify(back.assets),
  `${JSON.stringify(front.assets)} against ${JSON.stringify(back.assets)}`);
check("the edge is not placed", !front.placement, JSON.stringify(front.placement));
const binding = (front.services || []).find((s) => s.binding === "BACKEND");
check("the edge passes on to the worker wrangler.jsonc deploys", binding && binding.service === back.name,
  JSON.stringify(front.services));
check("…which is the one placed near its database", back.placement && back.placement.mode === "smart",
  JSON.stringify(back.placement));
check("the two agree on the runtime", front.compatibility_date === back.compatibility_date,
  `${front.compatibility_date} against ${back.compatibility_date}`);

// A FAKE ASSET LAYER: a known file is itself, anything else is the SPA's html.
const FILES = { "/index.md": "text/markdown", "/pkg/live.wasm": "application/wasm", "/": "text/html" };
const assets = {
  async fetch(req) {
    const p = new URL(req.url).pathname;
    const type = FILES[p] || "text/html";
    return new Response(p, { status: 200, headers: { "content-type": type } });
  },
};
const asked = [];
const env = { ASSETS: assets, BACKEND: { async fetch(req) { asked.push(new URL(req.url).pathname); return new Response("backend"); } } };
const get = (path, headers = {}) => new Request(`https://wfsim.app${path}`, { headers });

const home = await edgeServe(get("/"), env);
check("the home page is the edge's, with its twin named",
  home && home.status === 200 && (home.headers.get("link") || "").includes("/index.md"), home && home.headers.get("link"));
const md = await edgeServe(get("/", { accept: "text/markdown" }), env);
check("…and its twin to a reader that asks for markdown",
  md && (md.headers.get("content-type") || "").startsWith("text/markdown"), md && md.headers.get("content-type"));
const wasm = await edgeServe(get("/pkg/live.wasm"), env);
check("a hashed file is the edge's", wasm && wasm.status === 200, wasm && wasm.status);
const gone = await edgeServe(get("/pkg/gone.wasm"), env);
check("a hashed file that is gone is a 404, not the app", gone && gone.status === 404, gone && gone.status);
check("an endpoint is not the edge's", (await edgeServe(get("/api/board/ping"), env)) === null);

for (const p of ["/api/board/ping", "/board.meta.json", "/.well-known/api-catalog", "/og/s/abc"]) {
  const r = await edge.fetch(get(p), env);
  check(`${p} goes to the placed worker`, (await r.text()) === "backend" && asked.includes(p), asked.join(" "));
}
const before = asked.length;
await edge.fetch(get("/pkg/live.wasm"), env);
await edge.fetch(get("/weapons/Braton"), env);
check("the page and the engine never do", asked.length === before, asked.slice(before).join(" "));

console.log(failed ? `\n${failed} failed` : "\nthe edge serves the site, and the placed worker everything else");
process.exit(failed ? 1 : 0);
