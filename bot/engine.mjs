// SPDX-License-Identifier: AGPL-3.0-or-later
// THE ENGINE AND THE HEADLESS TABLE, as the bot server runs them — the same
// `mcp/headless.js` and wasm the MCP server bundles, loaded the way
// `scripts/check_mcp_tools.mjs` loads them. The board is the server's live one when
// it keeps one, and the published one otherwise.
import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const SITE = "https://wfsim.app";
const BOARD_TTL_MS = 600_000;
const LIVE_BOARD = process.env.WFSIM_LIVE_BOARD || "";

export async function loadEngine() {
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
  const zh = i18n.zh || {};
  const host = {
    meta: () => meta,
    names: () => Object.values(i18n),
    async board(id) {
      // THE OWNER'S LIVE BOARD WHEN THIS SERVER KEEPS ONE (`scripts/live_board.sh`,
      // swapped in whole), read on every ask because it moves by the minute;
      // the published one otherwise.
      if (LIVE_BOARD) {
        try {
          return JSON.parse(readFileSync(resolve(LIVE_BOARD, `${id}.json`), "utf8"));
        } catch (_) { /* not projected yet — the published board answers */ }
      }
      const hit = boards.get(id);
      if (hit && Date.now() - hit.at < BOARD_TTL_MS) return hit.rows;
      const r = await fetch(`${SITE}/board/${id}.json`);
      const rows = r.ok ? await r.json() : null;
      boards.set(id, { at: Date.now(), rows });
      return rows;
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
  return { host, run, meta, zh, headless };
}
