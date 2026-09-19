// NONA'S BOUNDARY, READ OFF THE FILES. No browser: every assertion here is a
// fact about the source, so it runs in milliseconds.
//
// A classic script's top-level `const` and `function` are visible to every
// module on the page, so a file under `nona/` that names one of `app.js`'s
// works — until `app.js` renames it, and then fails only when that line runs.
// Invariant 1 of docs/NONA.md is that she reaches the page through
// `window.wfsim` and nothing else; this is where that fails instead of drifting.
//
//   node scripts/check_nona_boundary.mjs
import fs from "node:fs";
import path from "node:path";

const STATIC = "web/src/static";
const NONA = path.join(STATIC, "nona");

let failed = 0;
const check = (name, ok, detail = "") => {
  if (!ok) failed++;
  console.log(`${ok ? "  ok" : "FAIL"}  ${name}${ok ? "" : `  — ${detail}`}`);
};

const walk = (d) => fs.readdirSync(d, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))
  .flatMap((e) => (e.isDirectory() ? walk(path.join(d, e.name)) : [path.join(d, e.name)]));
const files = walk(NONA).map((f) => path.relative(NONA, f).split(path.sep).join("/"));
const source = Object.fromEntries(files.map((f) => [f, fs.readFileSync(path.join(NONA, f), "utf8")]));

/// THE CODE OF A FILE with its comments and the text of its strings blanked,
/// template substitutions kept: a name in prose or in a message is not a use.
/// A `/` after a value is division, anywhere else it opens a regex literal.
function code(src) {
  let out = "", i = 0, prev = "";
  const depth = [];
  const blank = (s) => s.replace(/[^\n]/g, " ");
  while (i < src.length) {
    const c = src[i], n = src[i + 1];
    if (c === "/" && n === "/") { const j = src.indexOf("\n", i); const e = j < 0 ? src.length : j; out += blank(src.slice(i, e)); i = e; continue; }
    if (c === "/" && n === "*") { const e = src.indexOf("*/", i + 2) + 2; out += blank(src.slice(i, e)); i = e; continue; }
    if (c === "}" && depth.length && depth[depth.length - 1] === 0) { depth.pop(); i = template(i + 1, "}"); continue; }
    if (c === "{" && depth.length) depth[depth.length - 1]++;
    if (c === "}" && depth.length) depth[depth.length - 1]--;
    if (c === '"' || c === "'") {
      let j = i + 1;
      while (j < src.length && src[j] !== c) j += src[j] === "\\" ? 2 : 1;
      out += c + blank(src.slice(i + 1, j)) + c; i = j + 1; prev = "v"; continue;
    }
    if (c === "`") { i = template(i + 1, "`"); continue; }
    if (c === "/" && !/[\w$)\]]/.test(prev)) {
      let j = i + 1, cls = false;
      while (j < src.length && (cls || src[j] !== "/")) {
        if (src[j] === "\\") j++; else if (src[j] === "[") cls = true; else if (src[j] === "]") cls = false;
        j++;
      }
      j++;
      while (/[a-z]/.test(src[j] || "")) j++;
      out += blank(src.slice(i, j)); i = j; prev = "v"; continue;
    }
    out += c;
    if (!/\s/.test(c)) prev = /[\w$]/.test(c) ? (/^(return|typeof|case|in|of)$/.test(lastWord()) ? "(" : c) : c;
    i++;
  }
  return out;
  function lastWord() { const m = out.match(/([\w$]+)$/); return m ? m[1] : ""; }
  // The text of a template literal up to its end or its next `${`.
  function template(j, open) {
    out += open;
    while (j < src.length && src[j] !== "`") {
      if (src[j] === "\\") { out += "  "; j += 2; continue; }
      if (src[j] === "$" && src[j + 1] === "{") { out += "  "; depth.push(0); prev = "("; return j + 2; }
      out += src[j] === "\n" ? "\n" : " "; j++;
    }
    out += "`"; prev = "v";
    return j + 1;
  }
}

/// Every name bound anywhere in a file: imports, declarations, parameters and
/// destructured names. A module's own binding shadows the page's.
function bound(c) {
  const names = new Set();
  for (const m of c.matchAll(/\b(?:const|let|var|function|class)\s+([\w$]+)/g)) names.add(m[1]);
  for (const m of c.matchAll(/\b(?:const|let|var)\s*[{[]([^}\]=]*)[}\]]/g)) for (const x of m[1].matchAll(/(?:\.\.\.)?([\w$]+)\s*(?=[,}\]]|$|:)/g)) names.add(x[1]);
  for (const m of c.matchAll(/\bimport\s*\{([^}]*)\}/g)) for (const x of m[1].split(",")) names.add(x.trim().split(/\s+as\s+/).pop());
  for (const m of c.matchAll(/\bimport\s*\*\s*as\s+([\w$]+)/g)) names.add(m[1]);
  for (const m of c.matchAll(/\(([^()]*)\)\s*(?:=>|\{)/g)) for (const x of m[1].matchAll(/([\w$]+)\s*(?=[,=})]|$)/g)) names.add(x[1]);
  for (const m of c.matchAll(/([\w$]+)\s*=>/g)) names.add(m[1]);
  for (const m of c.matchAll(/\bcatch\s*\(\s*([\w$]+)/g)) names.add(m[1]);
  return names;
}

/// The names a file USES as free references: not after a `.`, not an object
/// key, not bound in the file.
function freeNames(c) {
  const own = bound(c);
  const used = new Set();
  for (const m of c.matchAll(/[A-Za-z_$][\w$]*/g)) {
    const before = c.slice(0, m.index).replace(/\s+$/, "");
    const after = c.slice(m.index + m[0].length).replace(/^\s+/, "");
    if (before.endsWith(".") && !before.endsWith("...")) continue;
    if (after.startsWith(":") && /[{,]$/.test(before)) continue;
    if (!own.has(m[0])) used.add(m[0]);
  }
  return used;
}

// ---- 1: no name of app.js ---------------------------------------------------------

const app = fs.readFileSync(path.join(STATIC, "app.js"), "utf8");
const PAGE = new Set();
for (const m of app.matchAll(/^(?:async\s+)?(?:function\*?|const|let|var|class)\s+([\w$]+)/gm)) PAGE.add(m[1]);
for (const m of app.matchAll(/^(?:const|let|var)\s*\{([^}]*)\}/gm)) for (const x of m[1].matchAll(/([\w$]+)\s*(?=[,}]|$)/g)) PAGE.add(x[1]);
check("app.js's own top-level names were found", PAGE.size > 500 && PAGE.has("escHtml") && PAGE.has("AGENT_ACTIONS"), String(PAGE.size));

const CODE = Object.fromEntries(files.filter((f) => f.endsWith(".js")).map((f) => [f, code(source[f])]));
const reaches = Object.entries(CODE).flatMap(([f, c]) => [...freeNames(c)].filter((n) => PAGE.has(n)).map((n) => `${f}: ${n}`));
check("no file under nona/ names one of app.js's", !reaches.length, reaches.join(", "));

// ---- the layers: imports only point down, and each keeps to what it may touch ---

const LAYER = (f) => (f === "index.js" ? 3 : f.startsWith("ui/") ? 2 : f.startsWith("runtime/") ? 1 : f.startsWith("core/") ? 0 : -1);
check("every file sits in a layer", files.every((f) => LAYER(f) >= 0), files.filter((f) => LAYER(f) < 0).join(", "));
const upward = [];
for (const f of Object.keys(CODE)) {
  for (const m of source[f].matchAll(/^\s*import\s[^;]*?from\s*"([^"]+)"/gm)) {
    const to = path.posix.normalize(path.posix.join(path.posix.dirname(f), m[1]));
    if (!files.includes(to)) upward.push(`${f} → ${m[1]} (no such file)`);
    else if (LAYER(to) > LAYER(f)) upward.push(`${f} → ${to}`);
  }
}
check("imports point only down: core ← runtime ← ui ← index", !upward.length, upward.join(", "));

/// What a layer may not touch, by name, in its code.
const BANNED = {
  0: ["window", "document", "fetch", "localStorage", "sessionStorage", "indexedDB", "navigator", "Date", "Math.random", "setTimeout"],
  1: ["document"],
  2: ["fetch", "localStorage", "sessionStorage", "indexedDB"],
  3: ["fetch", "localStorage", "sessionStorage", "indexedDB", "document"],
};
const breaches = [];
for (const [f, c] of Object.entries(CODE)) {
  for (const word of BANNED[LAYER(f)] || []) {
    const [head, prop] = word.split(".");
    const re = new RegExp(`(?<![.\\w$])${head}\\b${prop ? `\\s*\\.\\s*${prop}\\b` : ""}`);
    if (re.test(c)) breaches.push(`${f}: ${word}`);
  }
}
check("core touches no browser, clock or chance; runtime no DOM; ui no network or storage", !breaches.length, breaches.join(", "));
const fetching = Object.entries(CODE).filter(([, c]) => /(?<![.\w$])fetch\s*\(/.test(c)).map(([f]) => f);
check("the one fetch is runtime/transport.js", fetching.length === 1 && fetching[0] === "runtime/transport.js", fetching.join(", "));

// ---- every file is served, and the page loads her -------------------------------

const rs = fs.readFileSync("web/src/main.rs", "utf8");
const listed = [...rs.matchAll(/\("([^"]+)",\s*include_str!\("static\/nona\/([^"]+)"\)\)/g)];
const bad = listed.filter((m) => m[1] !== m[2]).map((m) => m[1]);
const names = listed.map((m) => m[1]);
check("the dev server lists every file under nona/, each at its own path",
  !bad.length && files.every((f) => names.includes(f)) && names.every((n) => files.includes(n)),
  [...bad, ...files.filter((f) => !names.includes(f)).map((f) => `missing ${f}`), ...names.filter((n) => !files.includes(n)).map((n) => `gone ${n}`)].join(", "));
const build = fs.readFileSync("scripts/build_site_app.py", "utf8");
check("the site build publishes the directory whole", /shutil\.copytree\(src, out \/ name\)/.test(build) && /STATIC \/ "nona"/.test(build));
const html = fs.readFileSync(path.join(STATIC, "index.html"), "utf8");
check("the page loads her as a module after app.js",
  /<script src="\/app\.js"><\/script>\s*<script type="module" src="\/nona\/index\.js"><\/script>/.test(html));

console.log(failed ? `\n${failed} failed` : "\nnona keeps to her side of the door");
process.exit(failed ? 1 : 0);
