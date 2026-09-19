// THE ENGINE'S MODULES DEPEND IN ONE DIRECTION, DOWN THE LAYERS BELOW.
//
// A cycle is what makes a module impossible to read, test or move alone: every
// member reaches every other, so understanding one means understanding all of
// them. The compiler accepts a cycle inside a crate, so nothing else notices one
// being added. Production code only: a test may reach anywhere.
//
// A new module is placed in LAYERS by whoever adds it — an unplaced one fails,
// because where it sits is the decision this check exists to make visible.
import { readFileSync, readdirSync, statSync } from "node:fs";
import { resolve, dirname, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

// Bottom first. A module may name its own layer and the ones before it. A
// layer that is a folder (`rules/`, `data/`, `build/`, `board/`) holds its own
// modules; the rest are named here one by one.
const LAYERS = [
  // RULES: primitives and the game's formulas, pure functions over numbers.
  ["rules", "naming"],
  // MODEL: the vocabulary — what a card, a buff, a weapon IS.
  ["model", "model"],
  // DATA: the catalogs, yaml read into the vocabulary.
  ["data", ""],
  // BUILD: a loadout resolved into the numbers a fight reads.
  ["build", ""],
  // FIGHT: who is shot, where, the simulation itself and what it recorded.
  ["fight", "target formation arena record fight"],
  // BOARD: build identity and the rulers every row is measured by.
  ["board", ""],
];

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const SRC = resolve(ROOT, "engine/src");
const NL = String.fromCharCode(10);
let bad = 0;
const check = (ok, name, detail) => {
  console.log(`  ${ok ? "ok  " : "FAIL"}  ${name}${ok || !detail ? "" : `  — ${detail}`}`);
  if (!ok) bad += 1;
};

const layerOf = new Map();
LAYERS.forEach(([, mods], i) => mods.split(" ").filter(Boolean).forEach((m) => layerOf.set(m, i)));
const FOLDERS = new Map(LAYERS.map(([name], i) => [name, i]));
// `model/` and `fight/` are each ONE module, split into files — not folders of modules.
FOLDERS.delete("model");
FOLDERS.delete("fight");

const walk = (dir) => readdirSync(dir).flatMap((f) => {
  const p = resolve(dir, f);
  return statSync(p).isDirectory() ? walk(p) : p.endsWith(".rs") ? [p] : [];
});

// The module a file belongs to: `fight/run.rs` is `fight`, `data/weapons/kitguns.rs`
// is `data::weapons` — a layer folder's children are its modules.
const owner = (p) => {
  const parts = relative(SRC, p).split(sep).map((x) => x.replace(/\.rs$/, ""));
  if (FOLDERS.has(parts[0]) && parts.length > 1 && parts[1] !== "mod") return `${parts[0]}::${parts[1]}`;
  return parts[0];
};
const layerOfModule = (m) => FOLDERS.get(m.split("::")[0]) ?? layerOf.get(m);

// Production lines only: drop comments, a `tests` directory, and every inline
// `#[cfg(test)] mod x { … }` (closed by the `}` at its own indent).
const production = (text) => {
  const out = [];
  const lines = text.split(NL);
  for (let i = 0; i < lines.length; i += 1) {
    const l = lines[i];
    const m = /^(\s*)#\[cfg\(test\)\]\s*$/.exec(l);
    if (m && /^\s*(pub(\([^)]*\))? )?mod \w+ \{/.test(lines[i + 1] ?? "")) {
      const close = `${m[1]}}`;
      i += 2;
      while (i < lines.length && lines[i] !== close) i += 1;
      continue;
    }
    out.push(l.replace(/\/\/.*$/, ""));
  }
  return out.join(NL);
};

// `data::weapons::spec` names the module `data::weapons`; `fight::X` names `fight`.
const groupEntry = (w) => {
  const seg = w.split("::").filter(Boolean);
  return FOLDERS.has(seg[0]) && seg[1] ? `${seg[0]}::${seg[1]}` : seg[0];
};

// Every module named after `crate::`, including inside a `{…}` group.
const references = (text) => {
  const found = new Set();
  for (const m of text.matchAll(/crate::([a-z_][a-z0-9_]*)(?:::([a-z_][a-z0-9_]*))?/g)) {
    found.add(FOLDERS.has(m[1]) && m[2] ? `${m[1]}::${m[2]}` : m[1]);
  }
  for (const m of text.matchAll(/crate::\{/g)) {
    let depth = 1;
    let word = "";
    let atTop = true;
    for (let i = m.index + m[0].length; i < text.length && depth > 0; i += 1) {
      const c = text[i];
      if (c === "{") depth += 1;
      else if (c === "}") depth -= 1;
      if (depth === 1 && /[a-z0-9_:]/.test(c) && atTop) word += c;
      else if (word) { found.add(groupEntry(word)); word = ""; }
      if (depth === 1 && c === ",") atTop = true;
    }
  }
  return found;
};

const graph = new Map();
for (const p of walk(SRC)) {
  const rel = relative(SRC, p);
  if (rel === "lib.rs" || rel.startsWith(`bin${sep}`) || rel.split(sep).includes("tests")) continue;
  const mod = owner(p);
  const deps = graph.get(mod) ?? new Set();
  for (const d of references(production(readFileSync(p, "utf8")))) if (d !== mod) deps.add(d);
  graph.set(mod, deps);
}
for (const deps of graph.values()) {
  for (const d of [...deps]) {
    // `crate::data::file` is the data layer's own root; the rest must be modules.
    if (FOLDERS.has(d) && graph.has(d)) continue;
    if (!graph.has(d)) deps.delete(d);
  }
}
if (process.env.GRAPH) for (const [m, deps] of [...graph].sort()) console.log(`${m} -> ${[...deps].sort().join(" ")}`);

const unplaced = [...graph.keys()].filter((m) => layerOfModule(m) === undefined).sort();
check(unplaced.length === 0, `every module has a layer (${graph.size})`,
  `place ${unplaced.join(", ")} in LAYERS — the lowest layer that holds everything it names`);
const stale = [...layerOf.keys()].filter((m) => !graph.has(m)).sort();
check(stale.length === 0, "...and every layer entry is a module", `no module named ${stale.join(", ")}`);

const upward = [];
for (const [m, deps] of graph) {
  for (const d of deps) {
    const lm = layerOfModule(m);
    const ld = layerOfModule(d);
    if (lm !== undefined && ld !== undefined && ld > lm) {
      upward.push(`${m} (${LAYERS[lm][0]}) -> ${d} (${LAYERS[ld][0]})`);
    }
  }
}
check(upward.length === 0, "no module names a layer above its own", upward.sort().join("; "));

// Tarjan's strongly connected components: a loop inside one layer.
let index = 0;
const idx = new Map(), low = new Map(), stack = [], on = new Set(), loops = [];
const visit = (v) => {
  idx.set(v, index); low.set(v, index); index += 1;
  stack.push(v); on.add(v);
  for (const w of graph.get(v)) {
    if (!idx.has(w)) { visit(w); low.set(v, Math.min(low.get(v), low.get(w))); }
    else if (on.has(w)) low.set(v, Math.min(low.get(v), idx.get(w)));
  }
  if (low.get(v) === idx.get(v)) {
    const c = [];
    let w;
    do { w = stack.pop(); on.delete(w); c.push(w); } while (w !== v);
    if (c.length > 1) loops.push(c.sort().join(" "));
  }
};
for (const v of [...graph.keys()].sort()) if (!idx.has(v)) visit(v);
check(loops.length === 0, "no two modules reach each other",
  `${loops.join("; ")} — move the type both sides name down into the lower one`);

console.log(bad ? `${NL}${bad} failed` : `${NL}the engine's modules depend one way`);
process.exit(bad ? 1 : 0);
