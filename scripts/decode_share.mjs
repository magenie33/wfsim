// A SHARE LINK, READ BACK INTO THE BUILD IT NAMES — for debugging a build
// somebody posted.
//
//   node scripts/decode_share.mjs <link | short id | code> [--json]
//
// Takes a short link (`/weapons/<Name>/s/<id>`), a long one (`?b=<code>`), a
// bare short id or a bare code. A short id is looked up on the live store
// (`/api/s/<id>`, WFSIM_SHARE_API to point elsewhere); everything else is
// local. The grammar is the worker's own copy of the page's codec and the id
// table is `data/share_order.yaml`, so this reads a link exactly as the page
// does. A link carries the build and nothing else: the fight it was run in is
// not in it.
import { decodeShare, useShareHost } from "../worker/share_codec.js";
import { shareHostOf } from "../worker/index.js";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const API = process.env.WFSIM_SHARE_API || "https://wfsim.app";
const args = process.argv.slice(2);
const asJson = args.includes("--json");
const input = args.find((a) => !a.startsWith("--"));
if (!input) {
  console.error("usage: node scripts/decode_share.mjs <link | short id | code> [--json]");
  process.exit(2);
}

/// Every yaml under `dir`, recursively.
function yamls(dir) {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    return statSync(p).isDirectory() ? yamls(p) : p.endsWith(".yaml") ? [p] : [];
  });
}

/// The table `site/share-names.json` is built from (`build_site_app.py`): the
/// frozen order, English names, and the weapons whose evolution ids carry the
/// weapon's prefix.
function shareTable() {
  const order = readFileSync(join(ROOT, "data", "share_order.yaml"), "utf8")
    .split("\n").filter((l) => l.trim().startsWith("- ")).map((l) => l.trim().slice(2).trim());
  const names = {}, prefixed = new Set();
  for (const family of ["weapons", "mods", "arcanes", "evolutions"]) {
    for (const f of yamls(join(ROOT, "data", family))) {
      const t = readFileSync(f, "utf8");
      const id = t.match(/^id:\s*(\S+)/m)?.[1];
      const name = t.match(/^name:\s*"?(.+?)"?\s*$/m)?.[1];
      if (id && name && !(id in names)) names[id] = name;
      const weapon = t.match(/^weapon:\s*(\S+)/m)?.[1];
      if (family === "evolutions" && id && weapon && id.startsWith(weapon + "_")) prefixed.add(weapon);
    }
  }
  // A RIVEN STAT is named by its card line without the hole and the unit.
  for (const f of yamls(join(ROOT, "data", "rivens"))) {
    for (const m of readFileSync(f, "utf8").matchAll(/^\s*- id:\s*(\S+)[\s\S]*?^\s*text:\s*"(.*)"/gm)) {
      if (!(m[1] in names)) names[m[1]] = m[2].replace("|val|", "").replace(/^\s*[%x+-]?\s*/, "").trim();
    }
  }
  return { order, names, evolution_prefixed: [...prefixed] };
}

/// The share code `input` names.
async function codeOf(s) {
  const short = s.match(/\/s\/([A-Za-z0-9_-]+)/)?.[1] ?? (/^[A-Za-z0-9_-]{6,16}$/.test(s) ? s : null);
  const long = s.match(/[?&]b=([^&#]+)/)?.[1];
  if (long) return decodeURIComponent(long);
  if (short && !s.includes("~")) {
    const r = await fetch(`${API}/api/s/${short}`);
    const j = r.ok ? await r.json() : null;
    if (!j || !j.ok) throw new Error(`no share stored under ${short} (${r.status})`);
    return j.c;
  }
  return s;
}

const table = shareTable();
useShareHost(shareHostOf(table));
// SET, NEVER `process.exit`: exiting with a fetch's handle still closing
// trips a libuv assertion on Windows.
const fail = (msg) => { console.error(msg); process.exitCode = 1; };
let code = null, d = null;
try {
  code = await codeOf(input);
  d = await decodeShare(code);
} catch (e) {
  fail(e.message);
}
if (code && !process.exitCode && (!d || !d.w)) fail("not a share code this codec reads");
if (d && d.w && asJson) console.log(JSON.stringify({ code, build: d }, null, 2));
if (d && d.w && !asJson) print(d);

function print(d) {
  const nm = (id) => table.names[id] || id;
  const pct = (r) => `roll ${r}`;
  console.log(`code     ${code}`);
  console.log(`weapon   ${nm(d.w)} (${d.w})${d.mode ? `  mode ${d.mode}` : ""}`);
  if (d.valence) console.log(`valence  ${d.valence.element} ${d.valence.bonus}`);
  if (d.assembly) console.log(`parts    grip ${d.assembly.grip}, loader ${d.assembly.loader}`);
  console.log("slots");
  d.slots.forEach((s, i) => {
    if (!s.mod) return;
    const riven = String(s.mod).startsWith("~") ? d.rivens?.[Number(String(s.mod).slice(1))] : null;
    const what = riven ? `riven ${String(s.mod).slice(1)}` : `${nm(s.mod)} (${s.mod})`;
    console.log(`  ${String(i).padStart(2)}  ${what}${s.rank != null ? ` rank ${s.rank}` : ""}${s.pol ? `  [${s.pol}]` : ""}`);
  });
  (d.rivens || []).forEach((r, i) => {
    const sp = r.s || {};
    const stat = (x) => `${nm(x.id)} ${pct(x.roll)}`;
    console.log(`riven ${i}  ${sp.shape || ""} rank ${sp.rank ?? "?"} ${sp.polarity || ""}`);
    (sp.bonuses || []).forEach((b) => console.log(`    + ${stat(b)}`));
    if (sp.malus) console.log(`    - ${stat(sp.malus)}`);
  });
  (d.arcane || []).forEach((a, i) => {
    if (a) console.log(`arcane   ${nm(a)} (${a})${d.arcaneRank?.[i] != null ? ` rank ${d.arcaneRank[i]}` : ""}`);
  });
  if ((d.evos || []).length) console.log(`evos     ${d.evos.map((e) => `${nm(e)} (${e})`).join(", ")}`);
}
