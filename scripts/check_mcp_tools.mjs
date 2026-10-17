// THE MCP SERVER'S QUERIES RUN WITH NO PAGE. `mcp/headless.js` is the page's
// headless part, copied; this runs every query in it against the engine
// `mcp/engine.js` bundles, with a host like the worker's — the published board
// read off `site/board/`, English, no screen. A query that reaches for a page
// global, or answers only with a build on screen, fails here and not in front
// of an agent. No browser; seconds.
import { readFileSync, existsSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const { HEADLESS_QUERIES, HEADLESS_RETIRED, headlessCheckArgs, headlessSchema, headlessToolName, headlessUnknown,
  headlessSeat, headlessStateAxes } =
  await import(pathToFileURL(resolve(ROOT, "mcp/headless.js")));

let failed = 0;
const check = (name, ok, detail = "") => {
  console.log(`${ok ? "  ok" : "FAIL"}  ${name}${ok ? "" : `  — ${String(detail).slice(0, 300)}`}`);
  if (!ok) failed++;
};

// THE ENGINE AS THE WORKER LOADS IT: the page's glue, and the wasm the site
// serves, by the path `engine.js` imports it from.
const src = readFileSync(resolve(ROOT, "mcp/engine.js"), "utf8");
const wasmPath = resolve(ROOT, "mcp", src.match(/^import module from "([^"]+)";$/m)[1]);
check("the engine the worker bundles is a file the site serves", existsSync(wasmPath), wasmPath);
const glue = src.split("\n").filter((l) => !/^(import|export) /.test(l)).join("\n");
const engine = new Function(`${glue}\nreturn wasm_bindgen;`)();
engine.initSync({ module: new WebAssembly.Module(readFileSync(wasmPath)) });
const packs = [...src.matchAll(/^import pack\d+ from "([^"]+)";$/gm)].map(([, p]) => resolve(ROOT, "mcp", p));
check("the engine bundles its data packs, each a file the site serves", packs.length > 0 && packs.every(existsSync), packs.join(", "));
for (const p of packs) {
  const err = engine.install_data(readFileSync(p, "utf8"));
  check(`the pack ${p.split(/[\\/]/).pop()} installs`, !err, err);
}
const api = (path, body) => JSON.parse(engine.api(path, JSON.stringify(body ?? {})));

const meta = api("/api/meta");
const names = Object.values(api("/api/i18n"));
const host = {
  meta: () => meta,
  names: () => names,
  async board(id) {
    const f = resolve(ROOT, "site/board", `${id}.json`);
    return existsSync(f) ? JSON.parse(readFileSync(f, "utf8")) : null;
  },
  api: async (path, body) => api(path, body),
  tr: (s) => s,
  origin: "https://wfsim.app",
  screen_weapon: () => null,
  screen_build: () => null,
};
const run = async (id, args) => {
  const q = HEADLESS_QUERIES.find((x) => x.id === id);
  return headlessCheckArgs(q, args) || q.run(args, host);
};

for (const q of HEADLESS_QUERIES) {
  check(`${q.id}: a tool name MCP accepts`, /^[a-zA-Z0-9_-]{1,64}$/.test(headlessToolName(q.id)));
  check(`${q.id}: its schema is an object`, headlessSchema(q.args).type === "object");
}

const found = await run("builder.weapons.find", { query: "月神" });
check("a weapon is found by its Chinese name", found.rows && found.rows.some((r) => r.id === "soma"), JSON.stringify(found));
check("...with the address its page lives at", found.rows && found.rows.every((r) => r.url.startsWith("https://wfsim.app/weapons/")),
  JSON.stringify(found.rows));

const board = await run("builder.board.read", { weapon: "soma_prime", limit: 1 });
const top = (board.rows || []).find((r) => r.build);
check("a board reads with no weapon on screen", !!top, JSON.stringify(board).slice(0, 300));
check("...each row with the link that opens it", (board.rows || []).every((r) => /\?bench=.+&mode=.+&riven=[01]$/.test(r.link)),
  JSON.stringify((board.rows || []).map((r) => r.link)));
check("...and its cards by name, not id", top && top.mods.every((m) => !/^[a-z0-9_]+$/.test(m)), top && top.mods);

const stats = top && await run("builder.stats.read", { build: top.build });
check("a row's build reads the stats panel", !!stats && Array.isArray(stats.forms) && stats.forms.length > 0,
  JSON.stringify(stats).slice(0, 300));

// THE RIVEN ANALYST WITH NO PAGE: published rows against the riven-free #1.
const rvBoard = (await host.board("acceltra_prime")) || [];
const rv = await run("builder.rivens.read", { weapon: "acceltra_prime" });
const rvGroup = (rv.groups || []).find((g) => g.riven_free && g.rivens.length);
const rvFree = rvGroup && Math.max(...rvBoard.filter((r) => !r.riven && r.benchmark === rvGroup.ruler_id
  && (r.mode || "base") === rvGroup.mode).map((r) => r.score));
const rvTop = rvGroup && Math.max(...rvBoard.filter((r) => r.riven && r.benchmark === rvGroup.ruler_id
  && (r.mode || "base") === rvGroup.mode).map((r) => r.score));
check("a riven's gain is its published score over the riven-free #1's", !!rvGroup
  && Math.abs(rvGroup.rivens[0].gain - (rvTop / rvFree - 1)) < 1e-9, JSON.stringify(rv).slice(0, 300));
check("...and its stats by name, not id", !!rvGroup && rvGroup.rivens.every((x) => x.bonuses.every((b) => !/^[a-z_]+$/.test(b))),
  JSON.stringify(rvGroup && rvGroup.rivens[0]));

const nobody = await run("builder.stats.read", {});
check("with no screen, a missing build is refused, not guessed", nobody.ok === false && nobody.reason === "missing_argument",
  JSON.stringify(nobody));
const typo = await run("builder.board.read", { weapon: "somaa" });
check("a mistyped weapon is refused with what was meant", typo.ok === false && typo.alternatives.includes("soma"),
  JSON.stringify(typo));

// A RETIRED NAME POINTS SOMEWHERE REAL: it is no longer a query, and what
// replaces it is one. The mechanism is exercised with a name of its own, so the
// check reads the same whether or not anything has been retired yet.
const live = new Set(HEADLESS_QUERIES.map((q) => q.id));
for (const [old, use] of Object.entries(HEADLESS_RETIRED)) {
  check(`retired ${old} is not still a query`, !live.has(old));
  check(`retired ${old} points at a query that exists`, use === null || live.has(use), use);
}
HEADLESS_RETIRED["builder.check.retired"] = "builder.board.read";
const moved = headlessUnknown("builder.check.retired");
delete HEADLESS_RETIRED["builder.check.retired"];
check("a retired name is refused with where it went",
  moved && moved.reason === "retired" && moved.use === "builder.board.read" && moved.tool === "builder_board_read",
  JSON.stringify(moved));
check("a name nobody retired is not answered as retired", headlessUnknown("builder.never.was") === null);

// A SAVED BUILD AND THE WIRE ARE ONE TRANSLATION BOTH WAYS: every riven-free
// board row's build, stored as a saved build's state and read back, is the
// build it was — across every weapon with a board, so an exilus, a stance, an
// evolution, a valence, a Kitgun's parts and a second arcane seat all pass
// through it.
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const norm = (p) => ({
  weapon: p.weapon, mods: (p.mods || []).filter(Boolean).slice().sort(),
  arcane: (Array.isArray(p.arcane) ? p.arcane : [p.arcane]).filter((x) => x && x !== "none"),
  evolutions: (p.evolutions || []).filter(Boolean).slice().sort(), mode: p.mode || null,
  valence: p.valence_element ? [p.valence_element, p.valence_bonus] : null, assembly: p.assembly || null,
});
let trips = 0;
const broken = [];
for (const w of meta.weapons) {
  const b = await run("builder.board.read", { weapon: w.id, limit: 1 });
  for (const r of (b.rows || []).filter((x) => x.build)) {
    trips++;
    const back = headlessSeat(meta, { weapon: w.id, ...headlessStateAxes(meta, r.build, w.id) });
    if (!same(norm(back), norm(r.build))) broken.push({ weapon: w.id, sent: norm(r.build), back: norm(back) });
  }
}
check(`${trips} board builds, saved and read back, are the builds they were`, trips > 100 && broken.length === 0,
  JSON.stringify(broken.slice(0, 2)));

console.log(failed ? `\n${failed} failed` : "\nthe MCP server's queries run with no page");
process.exit(failed ? 1 : 0);
