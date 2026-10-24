// WHAT A READER TELLS THE OWNER (worker/feedback.js), on the real schemas in a
// test Postgres, no network: only a signed-in reader on this site is heard;
// the words are kept as written with where they were and reach the owner with
// the screenshots attached; a message that fails to go out is still kept; an
// account's feedback is in its own export and nobody else's, and its deletion
// takes the screenshots from the bucket.
//   node scripts/check_feedback.mjs
import { feedbackRoute, feedbackImageKey } from "../worker/feedback.js";
import { accountRoute, purgeDeleted, sha256 } from "../worker/accounts.js";
import { testDb } from "./pg/test_db.mjs";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${String(detail).slice(0, 300)}`}`);
  if (!ok) failures++;
};

const told = [];
let tellOk = true;
const bucket = new Map();
const env = {
  ACCOUNTS: await testDb("accounts"), LIBRARY: await testDb("library"), AUTH_SECRET: "test",
  CLOUD: { fetch: async (req) => {
    if (new URL(req.url).pathname === "/internal/tell") told.push(await req.json());
    return new Response(JSON.stringify({ ok: tellOk }));
  } },
  UPLOADS: {
    put: async (k, v) => { bucket.set(k, v); },
    delete: async (keys) => { for (const k of [].concat(keys)) bucket.delete(k); },
  },
};
const A = env.ACCOUNTS;
const iso = (ms) => new Date(ms).toISOString();
const now = Date.now();

async function person(id) {
  await A.prepare("INSERT INTO accounts (id, created_at, username) VALUES (?1, ?2, ?3)").bind(id, iso(now), id).run();
  await A.prepare("INSERT INTO identities (provider, subject, account, label, linked_at) VALUES ('email', ?1, ?1, ?1, ?2)").bind(id, iso(now)).run();
  const token = `token-${id}`;
  await A.prepare("INSERT INTO sessions (token_hash, account, expires_at) VALUES (?1, ?2, ?3)")
    .bind(await sha256(token), id, iso(now + 86400_000)).run();
  return { cookie: `wfsim_session=${token}` };
}
const send = (who, body, extra = {}) => feedbackRoute(new Request("https://wfsim.app/api/feedback", {
  method: "POST", body: JSON.stringify(body),
  headers: { "content-type": "application/json", origin: "https://wfsim.app", ...(who ? { cookie: who.cookie } : {}), ...extra } }),
  env, "/api/feedback");
const exportOf = async (who) => (await accountRoute(new Request("https://wfsim.app/api/account/export", { method: "POST", body: "{}",
  headers: { cookie: who.cookie, "content-type": "application/json", origin: "https://wfsim.app" } }), env, "/api/account/export")).json();
const jpeg = (n) => "data:image/jpeg;base64," + Buffer.from(new Uint8Array(n).fill(7)).toString("base64");

const ann = await person("ann"), bob = await person("bob");

check("another path is not this route's", (await feedbackRoute(new Request("https://wfsim.app/api/x"), env, "/api/x")) === null);
check("a reader with no account is refused", (await send(null, { text: "hi" })).status === 401);
check("another site is refused", (await send(ann, { text: "hi" }, { origin: "https://evil.example" })).status === 403);
check("empty words are refused", (await send(ann, { text: "   " })).status === 400);
check("words past the limit are refused", (await send(ann, { text: "x".repeat(4001) })).status === 400);
check("four screenshots are refused", (await send(ann, { text: "hi", images: [jpeg(9), jpeg(9), jpeg(9), jpeg(9)] })).status === 400);
check("a png is refused", (await send(ann, { text: "hi", images: ["data:image/png;base64,AAAA"] })).status === 400);
check("a screenshot past 1.5 MB is refused", (await send(ann, { text: "hi", images: [jpeg(1_500_001)] })).status === 400);
check("nothing refused was kept or sent", (await A.prepare("SELECT COUNT(*) AS n FROM feedback").first()).n == 0 && told.length === 0);

const words = "Hunter Munitions here reads 42.5%,\r\nthe game says 40%";
const r = await send(ann, { text: words, route: "/weapons/Rubico_Prime", share: "https://wfsim.app/weapons/Rubico_Prime?b=abc",
  release: "e36e8563", lang: "zh", images: [jpeg(20), jpeg(30)] });
const j = await r.json();
check("a signed-in reader is heard", r.status === 200 && j.ok === true && j.id > 0, JSON.stringify(j));
const row = await A.prepare("SELECT * FROM feedback WHERE id = ?1").bind(j.id).first();
check("the words are kept as written, line ends made one", row && row.text === words.replace("\r\n", "\n"), row && row.text);
check("where they were is kept", row && row.route === "/weapons/Rubico_Prime" && row.share.endsWith("?b=abc") && row.release === "e36e8563" && row.lang === "zh");
check("the screenshots are in the bucket under its id", bucket.get(feedbackImageKey(j.id, 0))?.length === 20 && bucket.get(feedbackImageKey(j.id, 1))?.length === 30 && Number(row.images) === 2);
const t = told.at(-1);
check("the owner is told who, where and the words", t && t.text.includes(`#${j.id}`) && t.text.includes("ann") && t.text.includes("/weapons/Rubico_Prime")
  && t.text.includes("> the game says 40%"), t && t.text);
check("the screenshots go with the message", t && t.files?.length === 2 && Buffer.from(t.files[1].base64, "base64").length === 30 && t.files[0].type === "image/jpeg");

tellOk = false;
const r2 = await send(bob, { text: "the font here is ugly", route: "/" });
check("a message that cannot go out is still kept, and answered ok", r2.status === 200 && (await r2.json()).ok === true
  && (await A.prepare("SELECT COUNT(*) AS n FROM feedback WHERE account = 'bob'").first()).n == 1);
check("no screenshot sends no files", !("files" in told.at(-1)));

tellOk = true;
const ea = await exportOf(ann), eb = await exportOf(bob);
check("an account's export holds its own feedback", ea.feedback?.length === 1 && ea.feedback[0].text === row.text && ea.feedback[0].images === 2, JSON.stringify(ea.feedback));
check("and nobody else's", eb.feedback?.length === 1 && eb.feedback[0].text === "the font here is ugly");

await A.prepare("UPDATE accounts SET delete_after = ?1 WHERE id = 'ann'").bind(iso(now - 1000)).run();
await purgeDeleted(env);
check("a deleted account's feedback goes with it", (await A.prepare("SELECT COUNT(*) AS n FROM feedback WHERE account = 'ann'").first()).n == 0);
check("and its screenshots leave the bucket", !bucket.has(feedbackImageKey(j.id, 0)) && !bucket.has(feedbackImageKey(j.id, 1)));
check("another account's feedback stays", (await A.prepare("SELECT COUNT(*) AS n FROM feedback WHERE account = 'bob'").first()).n == 1);

console.log(failures ? `\n${failures} failed` : "\nall ok");
process.exit(failures ? 1 : 0);
