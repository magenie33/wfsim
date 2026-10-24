// SPDX-License-Identifier: AGPL-3.0-or-later
// THE ENGINE AND THE HEADLESS TABLE, as the bot server runs them — the same
// `mcp/headless.js` and wasm the MCP server bundles, loaded the way
// `scripts/check_mcp_tools.mjs` loads them. The board is the published one, the
// rows the card page draws from.
import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const SITE = "https://wfsim.app";
const BOARD_TTL_MS = 600_000;

/// `site` is where the board is read from: the live site, or a check's server.
export async function loadEngine({ site = SITE } = {}) {
  const headless = await import(pathToFileURL(resolve(ROOT, "mcp/headless.js")));
  const src = readFileSync(resolve(ROOT, "mcp/engine.js"), "utf8");
  const wasmPath = resolve(ROOT, "mcp", src.match(/^import module from "([^"]+)";$/m)[1]);
  const glue = src.split("\n").filter((l) => !/^(import|export) /.test(l)).join("\n");
  const engine = new Function(`${glue}\nreturn wasm_bindgen;`)();
  engine.initSync({ module: new WebAssembly.Module(readFileSync(wasmPath)) });
  // …AND ITS DATA, the packs `engine.js` imports beside the module.
  for (const [, p] of src.matchAll(/^import pack\d+ from "([^"]+)";$/gm)) {
    const err = engine.install_data(readFileSync(resolve(ROOT, "mcp", p), "utf8"));
    if (err) throw new Error(err);
  }
  const api = (path, body) => JSON.parse(engine.api(path, JSON.stringify(body ?? {})));
  const meta = api("/api/meta");
  const i18n = api("/api/i18n");
  const boards = new Map();
  const kept = async (url) => {
    const hit = boards.get(url);
    if (hit && Date.now() - hit.at < BOARD_TTL_MS) return hit.rows;
    const r = await fetch(url);
    const rows = r.ok ? await r.json() : null;
    boards.set(url, { at: Date.now(), rows });
    return rows;
  };
  const zh = i18n.zh || {};
  const host = {
    meta: () => meta,
    names: () => Object.values(i18n),
    async board(id) {
      return kept(`${site}/board/${id}.json`);
    },
    // THE HEADS, for a top-N answer (docs/BOARD.md §"The heads"); every row
    // where a weapon has none yet.
    async boardHeads(id) {
      return (await kept(`${site}/board/heads/${id}.json`)) || host.board(id);
    },
    api: async (path, body) => api(path, body),
    tr: (s) => (zh.ui && zh.ui[s]) || s,
    origin: SITE,
    screen_weapon: () => null,
    screen_build: () => null,
  };
  const run = async (id, args) => {
    const q = headless.HEADLESS_QUERIES.find((x) => x.id === id);
    return headless.headlessCheckArgs(q, args) || q.run(args, host);
  };
  return { host, run, meta, i18n, zh, headless, api, site };
}
