// THE QQ BOT'S DOOR (worker/qq.js), with no network: the address check signs
// QQ's own published example, a signed event is kept and a forged one is not,
// and the bot server's pull hands each message out once until it is answered.
//   node scripts/check_qq.mjs
import { qqRoute } from "../worker/qq.js";
import { DatabaseSync } from "node:sqlite";
import { readFileSync } from "node:fs";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

// QQ's webhook documentation: this secret, timestamp and token sign to this.
const SECRET = "DG5g3B4j9X2KOErG";
const want = "87befc99c42c651b3aac0278e71ada338433ae26fcb24307bdc5ad38c1adc2d01bcfcadc0842edac85e85205028a1132afe09280305f13aa6909ffc2d652c706";

const db = new DatabaseSync(":memory:");
db.exec(readFileSync(new URL("../worker/schema.sql", import.meta.url), "utf8"));
const stmt = (sql, args = []) => ({
  bind: (...a) => stmt(sql, a),
  all: async () => ({ results: db.prepare(sql).all(...args) }),
  run: async () => db.prepare(sql).run(...args),
});
const LIBRARY = { prepare: (sql) => stmt(sql), batch: async (ss) => { for (const x of ss) await x.run(); } };
const env = { QQ_APP_SECRET: SECRET, BOT_RELAY_TOKEN: "relay", LIBRARY };
const post = (path, body, headers = {}) => qqRoute(new Request(`https://x${path}`,
  { method: "POST", headers, body: typeof body === "string" ? body : JSON.stringify(body) }), env, path);

const v = await (await post("/api/qq", { op: 13, d: { plain_token: "Arq0D5A61EgUu4OxUvOp", event_ts: "1725442341" } })).json();
check("the address check signs QQ's published example", v.signature === want, v.signature);

// AN EVENT SIGNED THE WAY QQ SIGNS ONE: the same seed, over timestamp + body.
let seed = SECRET; while (seed.length < 32) seed = seed.repeat(2);
const der = new Uint8Array([...Buffer.from("302e020100300506032b657004220420", "hex"), ...Buffer.from(seed.slice(0, 32))]);
const key = await crypto.subtle.importKey("pkcs8", der, { name: "Ed25519" }, false, ["sign"]);
const sign = async (ts, body) => Buffer.from(await crypto.subtle.sign({ name: "Ed25519" }, key, new TextEncoder().encode(ts + body))).toString("hex");
const event = JSON.stringify({ op: 0, t: "C2C_MESSAGE_CREATE", d: { id: "m1", content: "zk 托里德", author: { user_openid: "u1" } } });
const ack = await (await post("/api/qq", event, { "x-signature-timestamp": "100", "x-signature-ed25519": await sign("100", event) })).json();
check("a signed message is acknowledged and kept", ack.op === 12 && db.prepare("SELECT COUNT(*) n FROM bot_inbox").get().n === 1);
const forged = event.replace("托里德", "盗贼");
const no = await post("/api/qq", forged, { "x-signature-timestamp": "100", "x-signature-ed25519": await sign("100", event) });
check("…and a message whose body was changed is refused", no.status === 401 && db.prepare("SELECT COUNT(*) n FROM bot_inbox").get().n === 1);
await post("/api/qq", event, { "x-signature-timestamp": "100", "x-signature-ed25519": await sign("100", event) });
check("…and a retried callback is kept once", db.prepare("SELECT COUNT(*) n FROM bot_inbox").get().n === 1);

// A GROUP THAT GIVES THE BOT EVERY MESSAGE sends an @ as GROUP_MESSAGE_CREATE
// too: an @ of this bot and a command are kept, the room's talk is not.
const room = async (id, d) => {
  const e = JSON.stringify({ op: 0, t: "GROUP_MESSAGE_CREATE", d: { id, group_openid: "g1", author: { member_openid: "a1" }, ...d } });
  await post("/api/qq", e, { "x-signature-timestamp": "100", "x-signature-ed25519": await sign("100", e) });
  return !!db.prepare("SELECT 1 FROM bot_inbox WHERE id = ?").get(id);
};
check("in a group giving every message, an @ of the bot is kept",
  await room("g-at", { content: "<@!b> zk 托里德", mentions: [{ is_you: true }] }));
check("…and a command without the @, spaced or not",
  await room("g-fx", { content: "fx 托里德 暴伤160.9 多重120.7" }) && await room("g-zk", { content: "zk托里德" }) && await room("g-slash", { content: "/帮助" })
  && await room("g-gx", { content: "/gx" }) && await room("g-gx-name", { content: "/gx 梅吉捏" }));
check("…and the room's own talk is not",
  !(await room("g-chat", { content: "今天刷什么" })) && !(await room("g-word", { content: "fxxk" })) && !(await room("g-bare", { content: "fx" }))
  && !(await room("g-congrats", { content: "gx" })) && !(await room("g-congrats-2", { content: "gx 大佬" }))
  && !(await room("g-other", { content: "<@!c> 你好", mentions: [{ is_you: false }] })));
db.prepare("DELETE FROM bot_inbox WHERE id LIKE 'g-%'").run();

check("the pull needs the server's token", (await post("/api/qq/claim", {}, { authorization: "Bearer wrong" })).status === 401);
const auth = { authorization: "Bearer relay" };
const first = await (await post("/api/qq/claim", {}, auth)).json();
check("the pull hands out what has not been answered", first.rows.length === 1 && first.rows[0].body.content === "zk 托里德");
const again = await (await post("/api/qq/claim", {}, auth)).json();
check("…once, while it is being answered", again.rows.length === 0);
db.prepare("UPDATE bot_inbox SET claimed_at = claimed_at - 100000").run();
const stale = await (await post("/api/qq/claim", {}, auth)).json();
check("…and again if it was never answered", stale.rows.length === 1);
await post("/api/qq/claim", { done: ["m1"] }, auth);
db.prepare("UPDATE bot_inbox SET claimed_at = claimed_at - 100000").run();
const after = await (await post("/api/qq/claim", {}, auth)).json();
check("…and never once it is", after.rows.length === 0);

console.log(failures ? `\n${failures} failed` : "\nthe qq door holds");
process.exit(failures ? 1 : 0);
