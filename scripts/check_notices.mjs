// WHAT THE SITE TELLS A READER (worker/notices.js), on the real schemas in a
// test Postgres, no network: what is said to everyone reaches anyone and
// nothing said to one account leaks to another; reading moves one mark that
// never goes back; a feature's notice is written once however often it runs;
// a device's cool-down is told while it lasts and a ban never; what is kept
// goes when its time is up, with the account, and into its export.
//   node scripts/check_notices.mjs
import { noticesRoute, noticeWrite, purgeNotices } from "../worker/notices.js";
import { accountRoute, sha256 } from "../worker/accounts.js";
import { testDb } from "./pg/test_db.mjs";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${String(detail).slice(0, 300)}`}`);
  if (!ok) failures++;
};

globalThis.fetch = async () => new Response(JSON.stringify({ ok: true }));
const env = { ACCOUNTS: await testDb("accounts"), LIBRARY: await testDb("library"), AUTH_SECRET: "test" };
const A = env.ACCOUNTS;
const iso = (ms) => new Date(ms).toISOString();
const now = Date.now();

async function person(id) {
  await A.prepare("INSERT INTO accounts (id, created_at, username) VALUES (?1, ?2, ?3)").bind(id, iso(now), id).run();
  await A.prepare("INSERT INTO identities (provider, subject, account, label, linked_at) VALUES ('email', ?1, ?1, ?1, ?2)").bind(id, iso(now)).run();
  const token = `token-${id}`;
  await A.prepare("INSERT INTO sessions (token_hash, account, expires_at) VALUES (?1, ?2, ?3)")
    .bind(await sha256(token), id, iso(now + 86400_000)).run();
  const headers = { cookie: `wfsim_session=${token}`, "content-type": "application/json", origin: "https://wfsim.app" };
  const call = (path, body) => noticesRoute(new Request(`https://wfsim.app${path}`,
    body ? { method: "POST", headers, body: JSON.stringify(body) } : { headers }), env, path);
  return { get: (p) => call(p).then((r) => r.json()), post: (p, b) => call(p, b), raw: call };
}
const anon = (path) => noticesRoute(new Request(`https://wfsim.app${path}`), env, path);
const write = (row) => A.prepare(`INSERT INTO notices (account, kind, text_en, text_zh, created_at, expires_at)
    VALUES (?1, ?2, ?3, ?4, ?5, ?6)`).bind(row.account ?? null, row.kind, row.en ?? null, row.zh ?? null, iso(row.at ?? now), row.until ?? null).run();

const ann = await person("ann"), bob = await person("bob");
await write({ kind: "announcement", en: "Hello all", zh: "大家好", at: now - 3000 });
await write({ kind: "announcement", en: "Over", zh: "已过期", at: now - 4000, until: iso(now - 1000) });
await write({ account: "ann", kind: "message", en: "Hi Ann", zh: "你好", at: now - 2000 });

const everyone = anon("/api/notices/everyone");
const e = await (await everyone).json();
check("what is said to everyone reaches a reader with no account", e.notices.length === 1 && e.notices[0].text.zh === "大家好", JSON.stringify(e));
check("…cached at the edge", (await anon("/api/notices/everyone")).headers.get("cache-control").includes("public"));
check("…and an expired one is not said", !e.notices.some((n) => n.text.en === "Over"));
check("a reader with no account has no notices of their own", (await anon("/api/notices")).status === 401);

const a = await ann.get("/api/notices"), b = await bob.get("/api/notices");
check("an account sees what is said to it and to everyone, newest first",
  a.notices.map((n) => n.text.en).join() === "Hi Ann,Hello all" && a.notices[0].audience === "you", JSON.stringify(a.notices));
check("…and never what is said to another account", b.notices.map((n) => n.text.en).join() === "Hello all", JSON.stringify(b.notices));
check("…and nothing is read yet", a.read_at === null);

await ann.post("/api/notices/read", { at: iso(now) });
const read1 = (await ann.get("/api/notices")).read_at;
await ann.post("/api/notices/read", { at: iso(now - 60_000) });
check("reading moves the mark, and an older mark never moves it back", read1 === iso(now) && (await ann.get("/api/notices")).read_at === read1,
  JSON.stringify([read1]));
await ann.post("/api/notices/read", { at: iso(now + 86400_000) });
check("…nor past the server's now", Date.parse((await ann.get("/api/notices")).read_at) <= Date.now() + 1000);
const cross = await noticesRoute(new Request("https://wfsim.app/api/notices/read", { method: "POST",
  headers: { cookie: "wfsim_session=token-ann", "content-type": "application/json", origin: "https://evil.example" }, body: "{}" }), env, "/api/notices/read");
check("a read from another site is refused", cross.status === 403);

for (let i = 0; i < 2; i++) {
  await noticeWrite(A, { account: "bob", kind: "membership_ending", params: { ends_at: "2026-11-01T00:00:00.000Z" }, dedupe_key: "membership_ending:2026-11-01" });
}
const bobs = (await bob.get("/api/notices")).notices.filter((n) => n.kind === "membership_ending");
check("a feature's notice is written once however often the feature runs", bobs.length === 1 && bobs[0].params.ends_at === "2026-11-01T00:00:00.000Z",
  JSON.stringify(bobs));
let threw = false;
try { await noticeWrite(A, { account: "bob", kind: "made_up" }); } catch (_) { threw = true; }
check("…and a kind the page cannot word is refused", threw);

// A DEVICE'S COOL-DOWN, from the library's verifiers through the account's devices.
await A.prepare("INSERT INTO devices (verifier, account, claimed_at) VALUES ('dev1', 'bob', ?1), ('dev2', 'bob', ?1)").bind(iso(now)).run();
const L = env.LIBRARY;
const until = iso(now + 3 * 3600_000);
await L.prepare("INSERT INTO verifiers (id, seen, refused_until) VALUES ('dev1', ?1, ?2)").bind(iso(now), until).run();
await L.prepare("INSERT INTO verifiers (id, seen, banned, refused_until) VALUES ('dev2', ?1, 1, ?2)").bind(iso(now), iso(now + 9e9)).run();
const st = (await bob.get("/api/notices")).state;
check("a device in its cool-down is told, with when it ends", st.length === 1 && st[0].kind === "device_cooling"
  && st[0].params.until === until && st[0].params.devices === 1, JSON.stringify(st));
check("…a banned one is not, and another account hears of neither", (await ann.get("/api/notices")).state.length === 0);
await L.prepare("UPDATE verifiers SET refused_until = ?1 WHERE id = 'dev1'").bind(iso(now - 1000)).run();
check("…and once it ends, nothing is said", (await bob.get("/api/notices")).state.length === 0);

// WHAT IS KEPT, AND FOR HOW LONG.
await write({ account: "ann", kind: "message", en: "Old", at: now - 181 * 86400_000 });
await purgeNotices(env);
const left = (await A.prepare("SELECT text_en FROM notices ORDER BY id").all()).results.map((r) => r.text_en);
check("a purge drops what passed 180 days and what expired, and keeps the rest", !left.includes("Old") && !left.includes("Over")
  && left.includes("Hi Ann") && left.includes("Hello all"), JSON.stringify(left));

const exp = await (await accountRoute(new Request("https://wfsim.app/api/account/export", { method: "POST",
  headers: { cookie: "wfsim_session=token-ann", "content-type": "application/json", origin: "https://wfsim.app" }, body: "{}" }), env, "/api/account/export")).json();
check("an account's export carries its notices and its mark", exp.notices?.length === 1 && exp.notices[0].text_en === "Hi Ann"
  && typeof exp.notices_read_at === "string", JSON.stringify([exp.notices, exp.notices_read_at]));
await A.prepare("DELETE FROM accounts WHERE id = 'ann'").run();
const gone = await A.prepare("SELECT (SELECT COUNT(*) FROM notices WHERE account = 'ann') AS n, (SELECT COUNT(*) FROM notice_marks WHERE account = 'ann') AS m").first();
check("a deleted account takes its notices and its mark with it", Number(gone.n) === 0 && Number(gone.m) === 0, JSON.stringify(gone));
check("…and what is said to everyone stays", (await (await anon("/api/notices/everyone")).json()).notices.length === 1);

console.log(failures ? `\n${failures} FAILED` : "\nall ok");
process.exit(failures ? 1 : 0);
