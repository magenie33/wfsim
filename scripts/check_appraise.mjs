// RIVEN APPRAISAL'S DOOR (worker/appraise.js), with no network: only a bot opens
// one, the page reads it without the chat it came from, every build handed back
// is kept, the first one the bot accepts wins once, and each channel's bot sees
// only its own.
//   node scripts/check_appraise.mjs
import { appraiseRoute } from "../worker/appraise.js";
import { DatabaseSync } from "node:sqlite";
import { readFileSync } from "node:fs";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

const db = new DatabaseSync(":memory:");
db.exec(readFileSync(new URL("../worker/schema.sql", import.meta.url), "utf8"));
// D1's surface, on node's sqlite: `first`, `all`, `run` with `meta.changes`, and
// a batch that answers each statement the way D1 does.
const stmt = (sql, args = []) => ({
  bind: (...a) => stmt(sql, a),
  first: async () => db.prepare(sql).get(...args) ?? null,
  all: async () => ({ results: db.prepare(sql).all(...args) }),
  run: async () => ({ meta: { changes: Number(db.prepare(sql).run(...args).changes) } }),
  each: async () => (/^\s*select/i.test(sql) ? { results: db.prepare(sql).all(...args) } : { meta: { changes: Number(db.prepare(sql).run(...args).changes) } }),
});
const LIBRARY = { prepare: (sql) => stmt(sql), batch: async (ss) => Promise.all(ss.map((x) => x.each())) };
const env = { BOT_RELAY_TOKEN: "relay", LIBRARY };
const call = async (method, path, body, headers = {}) => {
  const r = await appraiseRoute(new Request(`https://x${path}`, { method, headers,
    ...(body === undefined ? {} : { body: typeof body === "string" ? body : JSON.stringify(body) }) }), env, path.split("?")[0]);
  return { status: r.status, body: await r.json() };
};
const bot = { authorization: "Bearer relay" };
const riven = { bonuses: [{ id: "critical_damage", roll: 1.1 }, { id: "multishot", roll: 1.1 }], malus: { id: "zoom", roll: 0.9 }, rank: 8 };
const ask = (asker, room = "g1", channel = "qq") => ({ channel, chat: { group_openid: room, msg_id: "m-" + asker }, asker, room,
  weapon: "torid", ruler: "standard_single_target", riven });

check("only a bot opens an appraisal", (await call("POST", "/api/appraise/new", ask("a"), { authorization: "Bearer no" })).status === 401);
const opened = await call("POST", "/api/appraise/new", ask("a"), bot);
const code = opened.body.code;
check("a bot opens one and gets a short code", opened.body.ok && /^[2-9A-HJKMNP-Z]{5}$/.test(code || ""), JSON.stringify(opened.body));

const page = await call("GET", `/api/appraise/${code.toLowerCase()}`);
check("the page reads it, in any case", page.body.ok && page.body.weapon === "torid" && page.body.riven.malus.id === "zoom" && page.body.done === false);
check("…and never where the chat is or who asked", !("chat" in page.body) && !("asker" in page.body) && !JSON.stringify(page.body).includes("m-a"));
check("an unknown code is a 404", (await call("GET", "/api/appraise/ZZZZZ")).status === 404);

for (let i = 0; i < 4; i++) await call("POST", "/api/appraise/new", ask("a"), bot);
const sixth = await call("POST", "/api/appraise/new", ask("a"), bot);
check("one asker opens at most five an hour", sixth.status === 429 && sixth.body.error === "asker_limit", JSON.stringify(sixth.body));

const build = { benchmark: "standard_single_target", weapon: "torid", mods: ["serration", "riven"], riven_pos: ["critical_damage", "multishot"], riven_neg: "zoom" };
check("a hand-back needs a build", (await call("POST", `/api/appraise/${code}/result`, { thanks: "x" })).status === 400);
const b1 = await call("POST", `/api/appraise/${code}/result`, { build, thanks: "  Kai  see https://evil.example/x  and evil.com  " });
const b2 = await call("POST", `/api/appraise/${code}/result`, { build: { ...build, mods: ["hornet_strike", "riven"] }, thanks: "B" });
check("every build handed back is kept, each told it may be first", b1.body.ok && b1.body.first && b2.body.first);
const thanked = db.prepare("SELECT thanks FROM appraisal_results ORDER BY id").all().map((r) => r.thanks);
check("a name to thank loses its links and its length", thanked[0] === "Kai see and" && !/https?:|evil/.test(thanked[0]), JSON.stringify(thanked));

const c1 = (await call("POST", "/api/appraise/claim", { channel: "qq" }, bot)).body;
check("the bot is handed both builds to judge, with the chat to answer", c1.judge.length === 2 && c1.judge[0].chat.msg_id === "m-a" && c1.judge[0].build.mods[0] === "serration");
const c2 = (await call("POST", "/api/appraise/claim", { channel: "qq" }, bot)).body;
check("…once, while it is judging them", c2.judge.length === 0);
check("another channel's bot sees none of them", (await call("POST", "/api/appraise/claim", { channel: "discord" }, bot)).body.judge.length === 0);

const [r1, r2] = c1.judge.map((j) => j.id);
const c3 = (await call("POST", "/api/appraise/claim", { channel: "qq", judged: [{ id: r2, ok: true, verdict: { score: 87.3 } }] }, bot)).body;
check("the first build the bot accepts wins", c3.tell.length === 1 && c3.tell[0].code === code && c3.tell[0].result_id === r2 && c3.tell[0].thanks === "B" && c3.tell[0].verdict.score === 87.3, JSON.stringify(c3.tell));
const c4 = (await call("POST", "/api/appraise/claim", { channel: "qq", judged: [{ id: r1, ok: true }], told: [code] }, bot)).body;
const row = db.prepare("SELECT winner, told_at FROM appraisals WHERE code = ?").get(code);
check("…once: a later acceptance changes nothing", row.winner === r2, JSON.stringify(row));
check("…and a told appraisal is not handed out to tell again", c4.tell.length === 0 && Date.parse(row.told_at) > 0);
const late = await call("POST", `/api/appraise/${code}/result`, { build, thanks: "C" });
check("a build handed back after the win is still kept, and told it was not first", late.body.ok && late.body.first === false);
const one = await call("GET", `/api/appraise/${code}?result=${r2}`);
check("one handed-back build reads by its id, with whom to thank", one.body.result && one.body.result.id === r2
  && one.body.result.build.mods[0] === "hornet_strike" && one.body.result.thanks === "B");
check("…and only under its own appraisal", !(await call("GET", `/api/appraise/${opened.code}?result=999`)).body.result);
const done = await call("GET", `/api/appraise/${code}`);
check("the page then reads it done, and whom it thanked", done.body.done === true && done.body.thanked === "B");

const other = (await call("POST", "/api/appraise/new", ask("names"), bot)).body.code;
await call("POST", `/api/appraise/${other}/result`, { build, thanks: "a 片达人" });
check("a name carrying a listed word is not carried into a chat at all, though the build counts",
  db.prepare("SELECT thanks FROM appraisal_results WHERE code = ?").get(other).thanks === "");

if (failures) { console.log(`\n${failures} failed`); process.exit(1); }
console.log("\nan appraisal is asked once, answered once");
