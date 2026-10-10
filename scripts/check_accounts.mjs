// THE ACCOUNT RULES, RUN AGAINST THE REAL SCHEMA — docs/ACCOUNTS.md.
//
// `worker/accounts.sql` in node's own SQLite with foreign keys on, as D1 runs
// it, behind a stub of the D1 surface the worker touches; the three providers
// and the mailer are stubbed at `fetch` and `env.EMAIL`. What it holds is the
// model the owner fixed: one UUID, four slots, at most one of each; never a
// merge; the UUID gone the moment its last slot empties, whoever empties it.
// No network, no browser.
import { DatabaseSync } from "node:sqlite";
import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { accountRoute } from "../worker/accounts.js";
import { cloudRoute, cloudPath } from "../worker/cloud.js";
import { agentRoute, MCP_LIMITS } from "../worker/agents.js";
import { clanRoute } from "../worker/clan.js";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
let failed = 0;
const check = (name, ok, detail = "") => {
  console.log(`${ok ? "  ok" : "FAIL"}  ${name}${ok ? "" : `  — ${String(detail).slice(0, 300)}`}`);
  if (!ok) failed++;
};

// ---- D1, as far as the worker uses it ------------------------------------------

function d1() {
  const db = new DatabaseSync(":memory:");
  db.exec("PRAGMA foreign_keys = ON");
  db.exec(readFileSync(resolve(ROOT, "worker/accounts.sql"), "utf8"));
  const stmt = (sql, args = []) => ({
    sql, args,
    bind: (...a) => stmt(sql, a),
    first: async () => db.prepare(sql).get(...args) ?? null,
    all: async () => ({ results: db.prepare(sql).all(...args) }),
    run: async () => { db.prepare(sql).run(...args); return { success: true }; },
  });
  return {
    raw: db,
    prepare: (sql) => stmt(sql),
    async batch(list) {
      db.exec("BEGIN");
      try { for (const s of list) db.prepare(s.sql).run(...s.args); db.exec("COMMIT"); } catch (e) { db.exec("ROLLBACK"); throw e; }
    },
  };
}

// ---- three providers and a mailbox ------------------------------------------------

// WHO EACH CODE BELONGS TO: a test hands a provider a code, and the stub
// answers the token and user calls as that person.
const people = new Map();
globalThis.fetch = async (url, init = {}) => {
  const u = String(url);
  if (/oauth2\.googleapis|discord\.com\/api\/oauth2|github\.com\/login\/oauth/.test(u)) {
    const code = new URLSearchParams(String(init.body)).get("code");
    return new Response(JSON.stringify(people.has(code) ? { access_token: code } : { error: "bad" }));
  }
  const who = people.get((init.headers || {}).authorization?.slice(7));
  if (/openidconnect/.test(u)) return new Response(JSON.stringify({ sub: who.id, email: who.name }));
  if (/discord\.com\/api\/users/.test(u)) return new Response(JSON.stringify({ id: who.id, username: who.name }));
  if (/api\.github\.com\/user/.test(u)) return new Response(JSON.stringify({ id: Number(who.id), login: who.name }));
  throw new Error(`unexpected fetch ${u}`);
};
const mail = [];
const env = {
  ACCOUNTS: d1(),
  AUTH_SECRET: "test-secret",
  EMAIL: { send: async (m) => { mail.push(m); return { messageId: "x" }; } },
  GOOGLE_CLIENT_ID: "g", GOOGLE_CLIENT_SECRET: "gs",
  DISCORD_CLIENT_ID: "d", DISCORD_CLIENT_SECRET: "ds",
  GITHUB_CLIENT_ID: "h", GITHUB_CLIENT_SECRET: "hs",
};
const SITE = "https://wfsim.app";
const PASSWORD = "correct horse battery";

// ---- a browser: a cookie jar and the calls the page makes ----------------------------

function browser() {
  const jar = {};
  const keep = (res) => {
    for (const c of res.headers.getSetCookie ? res.headers.getSetCookie() : []) {
      const [pair, ...attrs] = c.split(";");
      const [k, v] = pair.split("=");
      if (/Max-Age=0/.test(attrs.join(";"))) delete jar[k.trim()]; else jar[k.trim()] = v;
    }
    return res;
  };
  const cookie = () => Object.entries(jar).map(([k, v]) => `${k}=${v}`).join("; ");
  const call = async (method, path, body, headers = {}) => {
    const req = new Request(SITE + path, {
      method, headers: { cookie: cookie(), ...(body ? { "content-type": "application/json", origin: SITE } : {}), ...headers },
      ...(body ? { body: JSON.stringify(body) } : {}),
    });
    return keep(await accountRoute(req, env, new URL(req.url).pathname));
  };
  return {
    jar,
    get: (path) => call("GET", path),
    post: (path, body, headers) => call("POST", path, body, headers),
    async me() { return (await (await call("GET", "/api/account")).json()).account; },
    /// A whole OAuth round trip as `person`, answered at the provider.
    async oauth(provider, person, intent = "login", tamper = null) {
      const code = `code-${Math.random()}`;
      people.set(code, person);
      const start = await call("GET", `/api/auth/${provider}/start?intent=${intent}&return=/weapons/Torid`);
      const state = new URL(start.headers.get("location")).searchParams.get("state");
      const back = await call("GET", `/api/auth/${provider}/callback?code=${code}&state=${tamper || state}`);
      return new URL(back.headers.get("location"));
    },
    /// Register or link an address with a password: the mail, then its code.
    async email(address, purpose = "register", { password = PASSWORD, wrong = false } = {}) {
      const s = await (await call("POST", `/api/auth/email/${purpose}`, { email: address, password })).json();
      if (!s.ok) return s;
      const code = mail.at(-1).subject.slice(0, 6);
      return (await call("POST", "/api/auth/email/verify",
        { email: address, code: wrong ? String((Number(code) + 1) % 1e6).padStart(6, "0") : code })).json();
    },
    login: async (address, password) =>
      (await call("POST", "/api/auth/email/login", { email: address, password })).json(),
    async reset(address, password) {
      const before = mail.length;
      const s = await (await call("POST", "/api/auth/email/reset", { email: address })).json();
      if (!s.ok || mail.length === before) return { ...s, mailed: false };
      return (await call("POST", "/api/auth/email/verify",
        { email: address, code: mail.at(-1).subject.slice(0, 6), password })).json();
    },
  };
}
const count = (sql, ...a) => env.ACCOUNTS.raw.prepare(sql).get(...a).n;

// ---- the rules ---------------------------------------------------------------------------

const a = browser();
const ada = { id: "g-ada", name: "ada@example.com" };
let back = await a.oauth("google", ada);
let me = await a.me();
check("a first sign-in makes an account holding that one slot",
  back.searchParams.get("auth") === "created" && me && me.identities.length === 1 && me.identities[0].provider === "google",
  back + " " + JSON.stringify(me));
check("...and returns to the page it left from", back.pathname === "/weapons/Torid", back);
const adaId = me.id;

back = await browser().oauth("google", ada);
check("the same Google person signs into the same account",
  back.searchParams.get("auth") === "signed_in" && count("SELECT COUNT(*) n FROM accounts") === 1, back);

back = await a.oauth("discord", { id: "d-ada", name: "ada#1" }, "link");
me = await a.me();
check("a second slot links onto the signed-in account",
  back.searchParams.get("auth") === "linked" && me.identities.map((i) => i.provider).join() === "google,discord", JSON.stringify(me));

const b = browser();
await b.oauth("github", { id: "77", name: "bob" });
const bobId = (await b.me()).id;
back = await b.oauth("discord", { id: "d-ada", name: "ada#1" }, "link");
check("an identity on another account is refused, never moved",
  back.searchParams.get("auth_error") === "taken"
    && count("SELECT COUNT(*) n FROM identities WHERE account = ? AND provider = 'discord'", adaId) === 1
    && count("SELECT COUNT(*) n FROM identities WHERE account = ?", bobId) === 1, back);

const sameMail = await browser().email("ada@example.com");
check("an email matching a Google address is its own account — nothing merges by address",
  sameMail.ok && sameMail.outcome === "created" && count("SELECT COUNT(*) n FROM accounts") === 3, JSON.stringify(sameMail));

back = await a.oauth("google", { id: "g-ada-2", name: "ada2@example.com" }, "link");
me = await a.me();
check("a filled slot is replaced, not doubled",
  back.searchParams.get("auth") === "replaced" && me.identities.length === 2
    && count("SELECT COUNT(*) n FROM identities WHERE subject = 'g-ada'") === 0, JSON.stringify(me));

const wrong = await a.email("ada.other@example.com", "link", { wrong: true });
check("a wrong code fills nothing", wrong.ok === false && wrong.reason === "wrong_code"
  && count("SELECT COUNT(*) n FROM identities WHERE provider = 'email' AND account = ?", adaId) === 0, JSON.stringify(wrong));
const tooSoon = await (await a.post("/api/auth/email/link", { email: "ada.other@example.com", password: PASSWORD })).json();
check("a second code inside a minute is refused", tooSoon.reason === "too_soon", JSON.stringify(tooSoon));
env.ACCOUNTS.raw.prepare("UPDATE email_codes SET sent_at = '2000-01-01T00:00:00.000Z'").run();
const linked = await a.email("Ada.Other@Example.com", "link");
check("an address links once its code is right, lowercased", linked.ok && linked.outcome === "linked"
  && count("SELECT COUNT(*) n FROM identities WHERE subject = 'ada.other@example.com' AND account = ?", adaId) === 1,
  JSON.stringify(linked));
check("the mail carries the code and no link", mail.length > 0 && /^\d{6} /.test(mail.at(-1).subject)
  && !/https?:\/\//.test(mail.at(-1).text), JSON.stringify(mail.at(-1)));

// ---- the password: signing in sends no mail -------------------------------------------

const mailed = mail.length;
const pw = await browser().login("ada.other@example.com", PASSWORD);
check("an address and its password sign in to its account, and nothing is mailed",
  pw.ok && pw.outcome === "signed_in" && mail.length === mailed, JSON.stringify(pw));
const badPw = await browser().login("ada.other@example.com", "not-the-password");
const nobody = await browser().login("nobody@example.com", PASSWORD);
check("a wrong password and an unknown address answer alike",
  badPw.reason === "wrong_credentials" && nobody.reason === "wrong_credentials", JSON.stringify([badPw, nobody]));
check("a password is never kept as itself", !env.ACCOUNTS.raw.prepare(
  "SELECT group_concat(password_hash) h FROM identities").get().h.includes(PASSWORD));
me = await a.me();
check("the account page says the address has a password, and never shows it",
  me.identities.find((i) => i.provider === "email").has_password === true && !JSON.stringify(me).includes("pbkdf2"),
  JSON.stringify(me));

const short = await browser().post("/api/auth/email/register", { email: "short@example.com", password: "1234567" });
check("a password under eight characters is refused", (await short.json()).reason === "bad_password");
const again = await (await browser().post("/api/auth/email/register", { email: "ADA@example.com", password: PASSWORD })).json();
check("registering an address that has an account says so", again.reason === "email_taken", JSON.stringify(again));

const other = browser();
const reg = await other.email("carol@example.com");
const carolId = (await other.me()).id;
check("registering is the only mail before the account exists", reg.ok && reg.outcome === "created", JSON.stringify(reg));
const elsewhere = browser();
await elsewhere.login("carol@example.com", PASSWORD);
for (let i = 0; i < 5; i++) await browser().login("carol@example.com", "wrong-wrong");
const locked = await browser().login("carol@example.com", PASSWORD);
check("five wrong passwords close the address to passwords for a while", locked.reason === "locked", JSON.stringify(locked));
const noMail = await browser().reset("nobody@example.com", "fresh-password");
check("a reset for an address nobody holds answers the same, and mails nothing",
  noMail.ok === true && noMail.mailed === false, JSON.stringify(noMail));
env.ACCOUNTS.raw.prepare("UPDATE email_codes SET sent_at = '2000-01-01T00:00:00.000Z'").run();
const reset = await browser().reset("carol@example.com", "a-new-password");
check("a reset sets the new password and lifts the lock",
  reset.ok && reset.outcome === "password_reset" && (await browser().login("carol@example.com", "a-new-password")).ok,
  JSON.stringify(reset));
check("...and signs every other browser out", (await elsewhere.me()) === null && (await other.me()) === null);

const keep = browser();
await keep.login("carol@example.com", "a-new-password");
const second = browser();
await second.login("carol@example.com", "a-new-password");
const refused = await (await keep.post("/api/account/password", { current: "wrong", password: "third-password" })).json();
check("changing the password asks for the old one", refused.reason === "wrong_password", JSON.stringify(refused));
const changed = await (await keep.post("/api/account/password", { current: "a-new-password", password: "third-password" })).json();
check("...and with it, changes it, keeps this browser and signs the others out",
  changed.ok && (await keep.me())?.id === carolId && (await second.me()) === null, JSON.stringify(changed));

env.ACCOUNTS.raw.prepare("UPDATE identities SET password_hash = NULL WHERE subject = 'carol@example.com'").run();
check("an address linked before passwords cannot sign in with one",
  (await browser().login("carol@example.com", "third-password")).reason === "wrong_credentials");
const set = await (await keep.post("/api/account/password", { password: "fourth-password" })).json();
check("...and its signed-in owner sets one without an old one to give",
  set.ok && (await browser().login("carol@example.com", "fourth-password")).ok, JSON.stringify(set));

let r = await (await a.post("/api/account/unlink", { provider: "discord" })).json();
check("a slot empties while another is filled", r.ok && r.deleted === false, JSON.stringify(r));
r = await (await a.post("/api/account/unlink", { provider: "email" })).json();
r = await (await a.post("/api/account/unlink", { provider: "google" })).json();
check("the last slot is not emptied by accident", r.ok === false && r.reason === "last_slot"
  && count("SELECT COUNT(*) n FROM accounts WHERE id = ?", adaId) === 1, JSON.stringify(r));
r = await (await a.post("/api/account/unlink", { provider: "google", delete_account: true })).json();
check("...and emptied on purpose, it takes the account and its sessions",
  r.ok && r.deleted === true && count("SELECT COUNT(*) n FROM accounts WHERE id = ?", adaId) === 0
    && count("SELECT COUNT(*) n FROM sessions WHERE account = ?", adaId) === 0 && (await a.me()) === null, JSON.stringify(r));

env.ACCOUNTS.raw.prepare("DELETE FROM identities WHERE account = ?").run(bobId);
check("the schema holds the rule too: a last slot deleted by hand takes its account",
  count("SELECT COUNT(*) n FROM accounts WHERE id = ?", bobId) === 0);

// ---- the doors a stranger tries ---------------------------------------------------------

const c = browser();
back = await c.oauth("github", { id: "88", name: "carol" }, "login", "forged-state");
check("a callback whose state is not this browser's is refused", back.searchParams.get("auth_error") === "state", back);
const odd = await c.get("/api/auth/github/start?return=//evil.example/x");
check("a return address off this site is not followed",
  (await (async () => { const code = "x"; people.set(code, { id: "88", name: "carol" });
    const st = new URL(odd.headers.get("location")).searchParams.get("state");
    const cb = await c.get(`/api/auth/github/callback?code=${code}&state=${st}`);
    return new URL(cb.headers.get("location")).host; })()) === "wfsim.app");
const cross = await c.post("/api/auth/logout", {}, { origin: "https://evil.example" });
check("a state-changing call from another site is refused", cross.status === 403, cross.status);
const notYet = await browser().oauth("discord", { id: "d-new", name: "new" }, "link");
check("linking needs a signed-in account", notYet.searchParams.get("auth_error") === "not_signed_in", notYet);

const bare = await (await accountRoute(new Request(SITE + "/api/account"), {}, "/api/account")).json();
check("with nothing configured the page is offered no way in", bare.ok && bare.providers.length === 0 && bare.account === null,
  JSON.stringify(bare));

// ---- the paid half: forwarded, never trusted from the browser ---------------------------

// A STAND-IN FOR THE PRIVATE WORKER: it records what reached it, and answers
// `/internal/end` as told.
const seen = [];
let endAnswer = true;
env.CLOUD = { fetch: async (req) => {
  seen.push({ path: new URL(req.url).pathname, account: req.headers.get("x-wfsim-account"),
    cookie: req.headers.get("cookie"), body: req.method === "POST" ? await req.text() : null });
  const path = new URL(req.url).pathname;
  if (path === "/internal/end") return new Response(JSON.stringify({ ok: endAnswer }));
  if (path === "/internal/export") return new Response(JSON.stringify({ ok: true, billing: { customer: "cus_1" }, sync: [{ id: "e1" }] }));
  return new Response(JSON.stringify({ ok: true, forwarded: true }));
} };
const cloud = (b, method, path, body, headers = {}) => cloudRoute(new Request(SITE + path, { method,
  headers: { cookie: Object.entries(b.jar).map(([k, v]) => `${k}=${v}`).join("; "),
    ...(body !== undefined ? { "content-type": "application/json", origin: SITE } : {}), ...headers },
  ...(body !== undefined ? { body: typeof body === "string" ? body : JSON.stringify(body) } : {}) }), env, path);

check("every paid path is the private worker's, and no other",
  ["/api/billing", "/api/billing/checkout", "/api/cloud/riven", "/api/stripe/webhook"].every(cloudPath)
    && !["/api/account", "/api/board/submit", "/api/billingx"].some(cloudPath));
const payer = browser();
await payer.oauth("github", { id: "99", name: "payer" });
const payerId = (await payer.me()).id;
await cloud(payer, "GET", "/api/billing");
check("a signed-in reader reaches the paid half as their account, with no cookie",
  seen.at(-1).account === payerId && seen.at(-1).cookie === null, JSON.stringify(seen.at(-1)));
await cloud(browser(), "GET", "/api/billing", undefined, { "x-wfsim-account": payerId });
check("an account header a browser sends is dropped", seen.at(-1).account === null, JSON.stringify(seen.at(-1)));
const before = seen.length;
const crossed = await cloud(payer, "POST", "/api/billing/checkout", { price: "member_month" }, { origin: "https://evil.example" });
check("a paid call from another site never reaches the paid half", crossed.status === 403 && seen.length === before);

// ---- a clan invite: one message to the owner, and nothing kept -----------------------

const clan = async (b, body, headers = {}) => {
  const r = await clanRoute(new Request(SITE + "/api/clan/request", { method: "POST",
    headers: { cookie: Object.entries(b.jar).map(([k, v]) => `${k}=${v}`).join("; "),
      "content-type": "application/json", origin: SITE, ...headers }, body: JSON.stringify(body) }), env, "/api/clan/request");
  return { status: r.status, ...(await r.json()) };
};
const told = () => seen.filter((x) => x.path === "/internal/tell");
const rows = () => env.ACCOUNTS.raw.prepare("SELECT name FROM sqlite_master WHERE type = 'table'").all()
  .reduce((n, t) => n + env.ACCOUNTS.raw.prepare(`SELECT COUNT(*) n FROM "${t.name}"`).get().n, 0);
const clanBefore = { told: told().length, rows: rows() };
check("a signed-out reader cannot ask for a clan invite",
  (await clan(browser(), { name: "Tenno" })).reason === "not_signed_in" && told().length === clanBefore.told);
check("a clan invite asked from another site is refused",
  (await clan(payer, { name: "Tenno" }, { origin: "https://evil.example" })).status === 403 && told().length === clanBefore.told);
check("an empty or over-long in-game name is refused",
  (await clan(payer, { name: "  " })).reason === "bad_ign" && (await clan(payer, { name: "x".repeat(41) })).reason === "bad_ign"
    && told().length === clanBefore.told);
const payerName = env.ACCOUNTS.raw.prepare("SELECT username FROM accounts WHERE id = ?").get(payerId).username;
const asked = await clan(payer, { name: " Te`nno ", lang: "zh" });
const said = told().at(-1) && JSON.parse(told().at(-1).body).text;
check("a clan invite reaches the owner with the name and the username",
  asked.ok && said && said.includes("`Tenno`") && said.includes(payerName) && said.includes("(zh)"), JSON.stringify([asked, said]));
check("a clan invite stores nothing", rows() === clanBefore.rows, `${clanBefore.rows} -> ${rows()}`);
const cloudFetch = env.CLOUD.fetch;
env.CLOUD = { fetch: async (req) => new URL(req.url).pathname === "/internal/tell"
  ? new Response(JSON.stringify({ ok: false })) : cloudFetch(req) };
check("a message the bot could not send is said to have failed", (await clan(payer, { name: "Tenno" })).reason === "tell_failed");
env.CLOUD = { fetch: cloudFetch };
let clanTaken = 0;
env.CLAN_LIMIT = { limit: async () => ({ success: clanTaken++ < 1 }) };
check("one account asks for a clan invite once a minute",
  (await clan(payer, { name: "Tenno" })).ok && (await clan(payer, { name: "Tenno" })).reason === "rate_limited");
delete env.CLAN_LIMIT;
const raw = '{"id":"evt_1","type":"invoice.paid"}';
await cloud(payer, "POST", "/api/stripe/webhook", raw, { "content-type": "application/json", origin: "https://stripe.com" });
check("Stripe's webhook goes through as sent, and as nobody",
  seen.at(-1).body === raw && seen.at(-1).account === null, JSON.stringify(seen.at(-1)));

const exported = await (await payer.post("/api/account/export", {})).json();
check("the account export carries what the paid half holds", exported.billing?.customer === "cus_1"
  && exported.sync?.[0]?.id === "e1", JSON.stringify([exported.billing, exported.sync]));
endAnswer = false;
r = await (await payer.post("/api/account/delete", {})).json();
check("an account whose subscription the paid half cannot end is kept",
  r.reason === "billing_open" && count("SELECT COUNT(*) n FROM accounts WHERE id = ?", payerId) === 1, JSON.stringify(r));
r = await (await payer.post("/api/account/unlink", { provider: "github", delete_account: true })).json();
check("...by either way of deleting it", r.reason === "billing_open" && count("SELECT COUNT(*) n FROM accounts WHERE id = ?", payerId) === 1,
  JSON.stringify(r));
endAnswer = true;
r = await (await payer.post("/api/account/delete", {})).json();
check("once it is ended the account goes", r.ok && count("SELECT COUNT(*) n FROM accounts WHERE id = ?", payerId) === 0
  && seen.filter((x) => x.path === "/internal/end" && x.account === payerId).length === 3, JSON.stringify(r));
delete env.CLOUD;
const off = await (await cloudRoute(new Request(SITE + "/api/billing"), env, "/api/billing")).json();
check("with no paid half bound, billing reads as off", off.ok && off.configured === false, JSON.stringify(off));

// ---- the name an account goes by -----------------------------------------------------------

const setName = async (b, body) => (await b.post("/api/account/profile", body)).json();
const ago = (id, hours) => env.ACCOUNTS.raw.prepare("UPDATE accounts SET username_changed_at = ? WHERE id = ?")
  .run(new Date(Date.now() - hours * 3600e3).toISOString(), id);
const n1 = browser(), n2 = browser();
await n1.oauth("discord", { id: "d-name-1", name: "one" });
await n2.oauth("discord", { id: "d-name-2", name: "two" });
const one = await n1.me(), two = await n2.me();
check("an account is born with a user_ name and no display name",
  /^user_[a-z0-9]{6}$/.test(one.username) && one.display_name === null && one.username !== two.username, JSON.stringify(one));
let nr = await setName(n1, { username: "Tenno_Ada" });
check("the first change is at once, and a name is kept lowercase", nr.ok && nr.account.username === "tenno_ada", JSON.stringify(nr));
nr = await setName(n1, { username: "ada_two" });
check("...the next waits a day", nr.reason === "rename_too_soon" && !!nr.rename_after, JSON.stringify(nr));
for (const [bad, why] of [["ab", "bad_username"], ["has space", "bad_username"], ["阿达", "bad_username"],
  ["user_abcdef", "username_reserved"], ["Admin", "username_reserved"]]) {
  nr = await setName(n2, { username: bad });
  check(`"${bad}" is refused as ${why}`, nr.reason === why, JSON.stringify(nr));
}
nr = await setName(n2, { username: "TENNO_ADA" });
check("a name another account holds is taken, whatever its case", nr.reason === "username_taken", JSON.stringify(nr));
ago(one.id, 25);
nr = await setName(n1, { username: "ada_two" });
check("a day later it changes again", nr.ok && nr.account.username === "ada_two", JSON.stringify(nr));
nr = await setName(n2, { username: "tenno_ada" });
check("the name given up is held from anyone else", nr.reason === "username_taken", JSON.stringify(nr));
ago(one.id, 25);
nr = await setName(n1, { username: "tenno_ada" });
check("...but its old owner may take it back", nr.ok && nr.account.username === "tenno_ada", JSON.stringify(nr));
ago(one.id, 25);
await setName(n1, { username: "ada_three" });
env.ACCOUNTS.raw.prepare("UPDATE username_holds SET held_until = ?").run(new Date(Date.now() - 1000).toISOString());
nr = await setName(n2, { username: "tenno_ada" });
check("once the hold lapses the name is free", nr.ok && nr.account.username === "tenno_ada", JSON.stringify(nr));
nr = await setName(n2, { display_name: "  阿达 · Ada  " });
check("a display name is free text, trimmed", nr.ok && nr.account.display_name === "阿达 · Ada", JSON.stringify(nr));
nr = await setName(n2, { display_name: "x".repeat(33) });
check("...of at most 32 characters", nr.reason === "bad_display_name", JSON.stringify(nr));
nr = await setName(n2, { display_name: "做 . 爱" });
check("...and never one carrying a listed word, however it is spaced out", nr.reason === "name_not_allowed", JSON.stringify(nr));
nr = await setName(n2, { display_name: "JS大佬" });
check("...while an ordinary name that only looks close passes", nr.ok && nr.account.display_name === "JS大佬", JSON.stringify(nr));
for (const bad of ["wf.jiuuvd.cn", "https://x.io/a", "jiuuvd点cn", "QQ 123456", "加群 7788990", "代刷白金", "WFSim官方"]) {
  nr = await setName(n2, { display_name: bad });
  check(`...nor one pointing elsewhere: ${bad}`, nr.reason === "name_not_allowed", JSON.stringify(nr));
}
for (const good of ["B站-星空笨鱼鱼", "dna980560", "SG91", "J.P."]) {
  nr = await setName(n2, { display_name: good });
  check(`...while a handle passes: ${good}`, nr.ok && nr.account.display_name === good, JSON.stringify(nr));
}
env.ACCOUNTS.raw.prepare("UPDATE accounts SET display_name = 'wf.jiuuvd.cn' WHERE username = ?").run(nr.account.username);
const hidden = await n2.me();
check("a name set before the rule that refuses it is kept, and the account is told others do not see it",
  hidden.display_name === "wf.jiuuvd.cn" && hidden.display_name_hidden === true, JSON.stringify(hidden));
nr = await setName(n2, { display_name: "" });
check("...and an empty one shows the username again", nr.ok && nr.account.display_name === null, JSON.stringify(nr));
const gone = (await n2.me()).username;
await n2.post("/api/account/delete", {});
const n3 = browser();
await n3.oauth("github", { id: "71", name: "three" });
nr = await setName(n3, { username: gone });
check("a deleted account's name is held too", nr.reason === "username_taken", JSON.stringify(nr));

// ---- agents: a key at once, and an account only when its person says so ------------------
{

const agentCall = async (method, path, { body, key, jar } = {}) => {
  const headers = {};
  if (key) headers.authorization = `Bearer ${key}`;
  if (jar) headers.cookie = Object.entries(jar).map(([k, v]) => `${k}=${v}`).join("; ");
  if (body !== undefined) { headers["content-type"] = "application/json"; if (jar) headers.origin = SITE; }
  const r = await agentRoute(new Request(SITE + path, { method, headers, ...(body !== undefined ? { body: JSON.stringify(body) } : {}) }), env, path);
  return { status: r.status, type: r.headers.get("content-type") || "", text: await r.text() };
};
const agentJson = async (...a) => { const r = await agentCall(...a); return { status: r.status, ...JSON.parse(r.text) }; };

const prm = await agentJson("GET", "/.well-known/oauth-protected-resource");
check("the protected resource names this site, its issuer and header bearers",
  prm.resource === SITE && prm.authorization_servers[0] === SITE && prm.bearer_methods_supported.includes("header")
    && prm.scopes_supported.length > 0, JSON.stringify(prm));
const asm = await agentJson("GET", "/.well-known/oauth-authorization-server");
const aa = asm.agent_auth || {};
check("the authorization server is the same issuer, with a complete anonymous method",
  asm.issuer === SITE && aa.skill === `${SITE}/auth.md` && aa.register_uri === aa.identity_endpoint
    && aa.claim_uri === aa.claim_endpoint && aa.identity_types_supported.includes("anonymous")
    && aa.anonymous.credential_types_supported.includes("api_key") && !!aa.anonymous.claim_uri && !!aa.revocation_uri,
  JSON.stringify(asm));
const md = await agentCall("GET", "/auth.md");
check("/auth.md is markdown under an auth.md heading, naming the endpoints the metadata names",
  md.type.startsWith("text/markdown") && /^# .*auth\.md/m.test(md.text)
    && md.text.includes(aa.register_uri) && md.text.includes(aa.claim_uri), md.text.slice(0, 80));
const agentBound = SITE + "/api/agent/auth";
check("the endpoints the metadata names are the ones served", aa.register_uri === agentBound, aa.register_uri);

const reg = await agentJson("POST", "/api/agent/auth", { body: { type: "anonymous", name: "Test Agent" } });
const key = reg.credential && reg.credential.api_key;
check("registering gives a key at once, before any claim", reg.status === 201 && /^wfa_/.test(key)
  && JSON.stringify(reg.scopes) === JSON.stringify(["read"]), JSON.stringify(reg));
check("...and only its hash is kept",
  count("SELECT COUNT(*) n FROM agent_keys WHERE key_hash = ? OR key_hash = ?", key, key) === 0
    && count("SELECT COUNT(*) n FROM agent_keys WHERE id = ?", reg.agent_id) === 1);
let who = await agentJson("GET", "/api/agent/whoami", { key });
check("the key says who it is, acting for nobody yet", who.ok && who.agent.name === "Test Agent" && who.account === null,
  JSON.stringify(who));
check("a key this server did not issue is refused",
  (await agentJson("GET", "/api/agent/whoami", { key: "wfa_forged" })).reason === "bad_key");

const before = mail.length;
let cl = await agentJson("POST", "/api/agent/auth/claim", { key, body: { email: "nobody-here@example.com" } });
check("a claim to an address no account holds answers as any other",
  cl.ok && cl.sent === true && mail.length === before + 1 && /No WFSim account/.test(mail.at(-1).subject), JSON.stringify(cl));
env.ACCOUNTS.raw.prepare("UPDATE agent_keys SET claim_sent_at = NULL WHERE id = ?").run(reg.agent_id);
check("...and no code completes it",
  (await agentJson("POST", "/api/agent/auth/claim/complete", { key, body: { code: "000000" } })).reason === "wrong_code");

const owner = browser();
await owner.email("agent-owner@example.com");
const ownerMe = await owner.me();
env.ACCOUNTS.raw.prepare("UPDATE agent_keys SET claim_sent_at = NULL, claim_attempts = 0 WHERE id = ?").run(reg.agent_id);
cl = await agentJson("POST", "/api/agent/auth/claim", { key, body: { email: "Agent-Owner@example.com" } });
const code = mail.at(-1).subject.slice(0, 6);
check("a claim to an account's address mails that address a code", cl.ok && /^\d{6}$/.test(code)
  && mail.at(-1).to === "agent-owner@example.com" && mail.at(-1).text.includes("Test Agent"), JSON.stringify(mail.at(-1)));
check("...and a second claim within the minute waits",
  (await agentJson("POST", "/api/agent/auth/claim", { key, body: { email: "agent-owner@example.com" } })).reason === "too_soon");
check("a wrong code does not claim",
  (await agentJson("POST", "/api/agent/auth/claim/complete", { key, body: { code: String((Number(code) + 1) % 1e6).padStart(6, "0") } })).reason === "wrong_code");
const done = await agentJson("POST", "/api/agent/auth/claim/complete", { key, body: { code } });
check("the right code claims the key for that account", done.ok && done.account.username === ownerMe.username
  && done.scopes.includes("builds"), JSON.stringify(done));
who = await agentJson("GET", "/api/agent/whoami", { key });
check("...which the key now says", who.account && who.account.username === ownerMe.username, JSON.stringify(who));
check("a claimed key is not claimed again",
  (await agentJson("POST", "/api/agent/auth/claim", { key, body: { email: "x@example.com" } })).reason === "already_claimed");

// THE KEY ON THE PAID HALF: a feature's route as its account, and nothing of billing.
env.CLOUD = { fetch: async (req) => {
  seen.push({ path: new URL(req.url).pathname, account: req.headers.get("x-wfsim-account"), auth: req.headers.get("authorization") });
  return new Response(JSON.stringify({ ok: true, forwarded: true }));
} };
const viaKey = (path, k) => cloudRoute(new Request(SITE + path, { method: "POST",
  headers: { "content-type": "application/json", authorization: `Bearer ${k}` }, body: "{}" }), env, path);
const reg2 = await agentJson("POST", "/api/agent/auth", { body: { name: "Unclaimed" } });
let n0 = seen.length;
await viaKey("/api/cloud/sync", key);
check("a claimed key reaches a feature as its account, and its key goes no further",
  seen.length === n0 + 1 && seen.at(-1).account === ownerMe.id && seen.at(-1).auth === null, JSON.stringify(seen.at(-1)));
n0 = seen.length;
await viaKey("/api/billing/checkout", key);
check("...but reaches billing as nobody", seen.length === n0 + 1 && seen.at(-1).account === null, JSON.stringify(seen.at(-1)));
await viaKey("/api/cloud/sync", reg2.credential.api_key);
check("an unclaimed key acts for nobody", seen.at(-1).account === null, JSON.stringify(seen.at(-1)));

const listed = await agentJson("GET", "/api/account/agents", { jar: owner.jar });
check("the account lists its agent", listed.ok && listed.agents.length === 1 && listed.agents[0].name === "Test Agent",
  JSON.stringify(listed));
const stranger = browser();
await stranger.oauth("github", { id: "4242", name: "stranger" });
check("another account cannot revoke it",
  (await agentJson("POST", "/api/account/agents/revoke", { jar: stranger.jar, body: { id: reg.agent_id } })).ok
    && (await agentJson("GET", "/api/agent/whoami", { key })).ok === true);
await agentJson("POST", "/api/account/agents/revoke", { jar: owner.jar, body: { id: reg.agent_id } });
check("its own account revokes it, and the key stops working",
  (await agentJson("GET", "/api/agent/whoami", { key })).reason === "bad_key");
const self = await agentJson("POST", "/api/agent/auth/revoke", { key: reg2.credential.api_key, body: {} });
check("an agent may revoke its own key", self.ok && (await agentJson("GET", "/api/agent/whoami", { key: reg2.credential.api_key })).reason === "bad_key");

const reg3 = await agentJson("POST", "/api/agent/auth", { body: { name: "Doomed" } });
env.ACCOUNTS.raw.prepare("UPDATE agent_keys SET account = ? WHERE id = ?").run(ownerMe.id, reg3.agent_id);
await owner.post("/api/account/delete", {});
check("deleting the account deletes its agents' keys", count("SELECT COUNT(*) n FROM agent_keys WHERE id = ?", reg3.agent_id) === 0);

// THE ALLOWANCE STATED IS THE ALLOWANCE ENFORCED.
const mcpConf = readFileSync(resolve(ROOT, "mcp/wrangler.jsonc"), "utf8");
const lim = (name) => Number((new RegExp(`"name": "${name}"[^}]*"limit": (\\d+)`).exec(mcpConf) || [])[1]);
check("the MCP allowance auth.md states is the one its limiters enforce",
  lim("ADDRESS_LIMIT") === MCP_LIMITS.per_address && lim("KEY_LIMIT") === MCP_LIMITS.per_key,
  JSON.stringify([lim("ADDRESS_LIMIT"), lim("KEY_LIMIT"), MCP_LIMITS]));
}

console.log(failed ? `\n${failed} failed` : "\nthe account rules hold");
process.exit(failed ? 1 : 0);
