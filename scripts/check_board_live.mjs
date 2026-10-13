// THE BOARD THE SITE READS (worker/live_board.js), with no network: board paths
// map to their own public paths as keys in the board's bucket, and nothing else
// does; the committed board answers only until the first stamp lands; after
// that a file is the bucket's or a 404; a push needs the token, a board file's
// name and JSON, and lands with the freshness the CDN in front of it keeps.
//   node scripts/check_board_live.mjs
import { liveKey, serveLive, pushLive } from "../worker/live_board.js";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

const objects = new Map();
const meta = new Map();
const PUBLIC = {
  head: async (k) => (objects.has(k) ? {} : null),
  get: async (k) => (objects.has(k) ? { body: objects.get(k), httpEtag: '"e"' } : null),
  put: async (k, v, o) => { objects.set(k, new TextDecoder().decode(v)); meta.set(k, o && o.httpMetadata); },
};
const ASSETS = { fetch: async () => new Response("[\"committed\"]", { headers: { "content-type": "application/json" } }) };
globalThis.caches = { default: { match: async () => undefined, put: async () => {} } };
const env = { PUBLIC, ASSETS, BOARD_PUSH_TOKEN: "t" };
const get = async (path) => {
  const r = await serveLive(new Request(`https://x${path}`), env, null, liveKey(path));
  return { status: r.status, text: await r.text() };
};
const put = (name, body, token = "t") => pushLive(new Request(`https://x/api/board/live/${name}`,
  { method: "PUT", headers: { authorization: `Bearer ${token}` }, body }), env, name);

check("a weapon's board is a live key", liveKey("/board/torid.json") === "board/torid.json");
check("...and so are the index and the stamp",
  liveKey("/board/index.json") === "board/index.json" && liveKey("/board.meta.json") === "board.meta.json");
check("...and a scene's file", liveKey("/board/sp200/torid.json") === "board/sp200/torid.json");
check("a path that is not a board file is not", liveKey("/board/x/y/z.json") === null && liveKey("/board/../a.json") === null
  && liveKey("/board/../x/a.json") === null && liveKey("/board/x/../a.json") === null
  && liveKey("/boards.json") === null && liveKey("/board/meta.json") === null);

check("before the first stamp, the committed board answers", (await get("/board/torid.json")).text === "[\"committed\"]");
check("a push without the token is refused", (await put("torid.json", "[1]", "x")).status === 401);
check("a push that is not json is refused", (await put("torid.json", "nope")).status === 400);
check("a push under a name that is no board file is refused", (await put("../x.json", "[]")).status === 400);
check("a weapon is pushed", (await put("torid.json", "[1,2]")).status === 200);
check("...under its public path, kept a cache's half minute and named as JSON",
  objects.get("board/torid.json") === "[1,2]" && meta.get("board/torid.json").cacheControl === "public, max-age=30"
  && /^application\/json/.test(meta.get("board/torid.json").contentType), JSON.stringify(meta.get("board/torid.json")));
check("...and is still not served before the stamp", (await get("/board/torid.json")).text === "[\"committed\"]");
await put("meta.json", "{\"digest\":\"d\"}");
const torid = await get("/board/torid.json");
check("once stamped, the live file answers", torid.status === 200 && torid.text === "[1,2]", JSON.stringify(torid));
check("...and the stamp itself, kept as the public path names it", (await get("/board.meta.json")).text === "{\"digest\":\"d\"}"
  && objects.has("board.meta.json"));
check("a weapon the live board does not hold is a 404, never the committed copy",
  (await get("/board/braton.json")).status === 404);

console.log(failures ? `\n${failures} failed` : "\nthe site reads the live board, and the committed one only until it exists");
process.exitCode = failures ? 1 : 0;
