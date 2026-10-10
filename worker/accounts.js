// ---- ACCOUNTS -----------------------------------------------------------------
//
// docs/ACCOUNTS.md. An account is a UUID, and a person reaches it through four
// SLOTS — Google, Discord, GitHub, and an email address with a password — at
// most one of each. Mail goes out only to prove an address: to register, to
// link one, to reset a password. Signing in with a password sends nothing.
// The UUID lives while a slot is filled: removing the last one deletes it, and
// the schema's trigger holds that whoever deletes (`worker/accounts.sql`).
//
// THE SERVER NEVER MERGES. An identity already on another account is refused,
// never moved; two accounts become one only by their owner emptying one of them.
//
// Nothing here is needed to use WFSim: a reader who never signs in has the site
// exactly as it was. Every endpoint answers 503 while its binding or its
// provider's secrets are absent, and `/api/account` says which are configured,
// so the page offers only the ways in that work.

import { nameBlocked } from "./names.js";
import { cloudEnd, cloudExport } from "./cloud.js";
import { agentsOf } from "./agents.js";

export const SLOTS = ["google", "discord", "github", "email"];

/// THE THIRD-PARTY WAYS IN. Each asks for the least that names a person: the
/// provider's own id, and the name shown back to them on the account page.
const PROVIDERS = {
  google: {
    authorize: "https://accounts.google.com/o/oauth2/v2/auth",
    token: "https://oauth2.googleapis.com/token",
    scope: "openid profile email",
    user: "https://openidconnect.googleapis.com/v1/userinfo",
    who: (u) => ({ subject: String(u.sub), label: u.email || u.name || "Google" }),
  },
  discord: {
    authorize: "https://discord.com/oauth2/authorize",
    token: "https://discord.com/api/oauth2/token",
    scope: "identify",
    user: "https://discord.com/api/users/@me",
    who: (u) => ({ subject: String(u.id), label: u.global_name || u.username || "Discord" }),
  },
  github: {
    authorize: "https://github.com/login/oauth/authorize",
    token: "https://github.com/login/oauth/access_token",
    scope: "",
    user: "https://api.github.com/user",
    who: (u) => ({ subject: String(u.id), label: u.login || "GitHub" }),
  },
};

const SESSION_COOKIE = "wfsim_session";
const OAUTH_COOKIE = "wfsim_oauth";
const SESSION_SECONDS = 90 * 24 * 3600;
const OAUTH_SECONDS = 600;
const CODE_SECONDS = 600;
const CODE_RESEND_SECONDS = 60;
const CODE_ATTEMPTS = 5;
export const EMAIL_FROM = "WFSim <login@wfsim.app>";
const PASSWORD_MIN = 8;
const PASSWORD_MAX = 128;
const LOGIN_FAILURES = 5;
const LOGIN_WINDOW_SECONDS = 15 * 60;
/// A USERNAME: lowercase letters, digits and `_`, typed in any case. `user_` is
/// the system's — every account is born with one and nobody picks one — so a
/// `user_` name always means "not chosen yet".
const USERNAME = /^[a-z0-9_]{3,20}$/;
const USERNAME_BORN = "user_";
const USERNAMES_RESERVED = new Set(["wfsim", "nona", "admin", "administrator", "official", "support", "help",
  "root", "system", "staff", "mod", "moderator", "de", "digital_extremes", "warframe", "api", "www", "account",
  "login", "signup", "billing", "privacy", "terms", "null", "undefined"]);
const RENAME_COOLDOWN_SECONDS = 24 * 3600;
const USERNAME_HOLD_SECONDS = 7 * 24 * 3600;
const DISPLAY_NAME_MAX = 32;

const secretOf = (env, p) => ({ id: env[`${p.toUpperCase()}_CLIENT_ID`], secret: env[`${p.toUpperCase()}_CLIENT_SECRET`] });
const configured = (env) => [
  ...Object.keys(PROVIDERS).filter((p) => secretOf(env, p).id && secretOf(env, p).secret),
  ...(env.EMAIL ? ["email"] : []),
].filter(() => env.ACCOUNTS && env.AUTH_SECRET);

// ---- small pieces -------------------------------------------------------------

const enc = new TextEncoder();
const b64url = (bytes) => btoa(String.fromCharCode(...new Uint8Array(bytes)))
  .replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
export const random = (n = 32) => b64url(crypto.getRandomValues(new Uint8Array(n)));
export const sha256 = async (s) => b64url(await crypto.subtle.digest("SHA-256", enc.encode(s)));
export async function hmac(key, s) {
  const k = await crypto.subtle.importKey("raw", enc.encode(key), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
  return b64url(await crypto.subtle.sign("HMAC", k, enc.encode(s)));
}
export const now = () => new Date().toISOString();
/// `user_` and six random characters, free of every account's name.
async function bornUsername(db) {
  for (;;) {
    const tail = Array.from(crypto.getRandomValues(new Uint8Array(6)), (b) => "abcdefghijklmnopqrstuvwxyz0123456789"[b % 36]).join("");
    const name = USERNAME_BORN + tail;
    if (!(await db.prepare("SELECT 1 FROM accounts WHERE username = ?1").bind(name).first())) return name;
  }
}
export const later = (seconds) => new Date(Date.now() + seconds * 1000).toISOString();

export const json = (obj, status = 200, headers = {}) =>
  new Response(JSON.stringify(obj), { status, headers: { "content-type": "application/json", "cache-control": "no-store", ...headers } });
export const no = (reason, status = 400, extra = {}) => json({ ok: false, reason, ...extra }, status);

function cookies(request) {
  const out = {};
  for (const part of (request.headers.get("cookie") || "").split(";")) {
    const i = part.indexOf("=");
    if (i > 0) out[part.slice(0, i).trim()] = part.slice(i + 1).trim();
  }
  return out;
}
const setCookie = (name, value, seconds, path = "/") =>
  `${name}=${value}; Path=${path}; Max-Age=${seconds}; HttpOnly; Secure; SameSite=Lax`;

/// A path on this site to come back to, or `/`: an OAuth round trip carries it,
/// and anything else would make this an open redirect.
const safeReturn = (r) => (typeof r === "string" && /^\/(?!\/)[^\\\s]*$/.test(r) ? r : "/");

/// A STATE-CHANGING CALL COMES FROM THIS SITE, as JSON. The session cookie is
/// SameSite=Lax, and this is the second half: a cross-site form cannot send
/// JSON, and a cross-site fetch carries a foreign Origin.
export function sameSite(request) {
  const origin = request.headers.get("origin");
  return (!origin || origin === new URL(request.url).origin)
    && (request.headers.get("content-type") || "").startsWith("application/json");
}

/// Over the per-address allowance for sign-in calls; `what` gives a kind of
/// call a counter of its own.
export async function limited(env, request, what = "") {
  if (!env.AUTH_LIMIT) return false;
  const { success } = await env.AUTH_LIMIT.limit({ key: what + (request.headers.get("cf-connecting-ip") || "unknown") });
  return !success;
}

// ---- sessions -------------------------------------------------------------------

/// The account signed in on this request, or null. The cookie holds a random
/// token; the table holds only its hash, so a copy of the table signs nobody in.
export async function sessionAccount(env, request) {
  const token = cookies(request)[SESSION_COOKIE];
  if (!token || !env.ACCOUNTS) return null;
  const row = await env.ACCOUNTS.prepare(
    "SELECT account FROM sessions WHERE token_hash = ?1 AND expires_at > ?2",
  ).bind(await sha256(token), now()).first();
  return row ? row.account : null;
}

async function newSession(env, account) {
  const token = random();
  await env.ACCOUNTS.prepare("INSERT INTO sessions (token_hash, account, expires_at) VALUES (?1, ?2, ?3)")
    .bind(await sha256(token), account, later(SESSION_SECONDS)).run();
  return setCookie(SESSION_COOKIE, token, SESSION_SECONDS);
}
const endSession = () => setCookie(SESSION_COOKIE, "", 0);

// ---- the one decision: an identity arrives --------------------------------------

/// A VERIFIED IDENTITY, to sign in with or to fill a slot — the same decision
/// whichever of the four ways it came by, so no way in can merge or steal.
///
///   login: its account, or a new account holding it
///   link:  into the signed-in account's slot for its provider — refused if
///          another account holds it, a replacement if the slot is filled
///
/// Returns `{ ok, outcome, account, cookie? }` or `{ ok: false, reason }`.
export async function arrive(env, request, { provider, subject, label, password_hash = null }, intent) {
  const db = env.ACCOUNTS;
  const holder = await db.prepare("SELECT account FROM identities WHERE provider = ?1 AND subject = ?2")
    .bind(provider, subject).first();
  const signedIn = await sessionAccount(env, request);

  if (intent === "link") {
    if (!signedIn) return { ok: false, reason: "not_signed_in" };
    if (holder && holder.account !== signedIn) return { ok: false, reason: "taken" };
    if (holder) {
      // The same address again: its password is the one just chosen.
      await db.prepare("UPDATE identities SET label = ?1, password_hash = COALESCE(?2, password_hash) WHERE provider = ?3 AND subject = ?4")
        .bind(label, password_hash, provider, subject).run();
      return { ok: true, outcome: "linked", account: signedIn };
    }
    const slot = await db.prepare("SELECT subject FROM identities WHERE account = ?1 AND provider = ?2")
      .bind(signedIn, provider).first();
    if (slot) {
      // A REPLACEMENT IS AN UPDATE, not a delete and an insert: the slot is
      // never empty, so the last-slot trigger can never fire half way through.
      await db.prepare("UPDATE identities SET subject = ?1, label = ?2, linked_at = ?3, password_hash = ?4 WHERE account = ?5 AND provider = ?6")
        .bind(subject, label, now(), password_hash, signedIn, provider).run();
      return { ok: true, outcome: "replaced", account: signedIn };
    }
    await db.prepare("INSERT INTO identities (provider, subject, account, label, linked_at, password_hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")
      .bind(provider, subject, signedIn, label, now(), password_hash).run();
    return { ok: true, outcome: "linked", account: signedIn };
  }

  if (holder) {
    await db.prepare("UPDATE identities SET label = ?1 WHERE provider = ?2 AND subject = ?3")
      .bind(label, provider, subject).run();
    return { ok: true, outcome: "signed_in", account: holder.account, cookie: await newSession(env, holder.account) };
  }
  // A NEW ACCOUNT ARRIVES WITH ITS FIRST SLOT, in one batch: there is no moment
  // at which a UUID exists with nothing to reach it by.
  const account = crypto.randomUUID();
  await db.batch([
    db.prepare("INSERT INTO accounts (id, created_at, username) VALUES (?1, ?2, ?3)").bind(account, now(), await bornUsername(db)),
    db.prepare("INSERT INTO identities (provider, subject, account, label, linked_at, password_hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")
      .bind(provider, subject, account, label, now(), password_hash),
  ]);
  return { ok: true, outcome: "created", account, cookie: await newSession(env, account) };
}

// ---- OAuth ------------------------------------------------------------------------

async function oauthStart(request, env, provider) {
  const url = new URL(request.url);
  const back = safeReturn(url.searchParams.get("return"));
  const intent = url.searchParams.get("intent") === "link" ? "link" : "login";
  const fail = (reason) => Response.redirect(`${url.origin}${back}${back.includes("?") ? "&" : "?"}auth_error=${reason}`, 302);
  if (!configured(env).includes(provider)) return fail("unavailable");
  const p = PROVIDERS[provider];
  const state = random(16);
  const verifier = random(32);
  const carried = b64url(enc.encode(JSON.stringify({ provider, state, verifier, intent, back })));
  const q = new URLSearchParams({
    client_id: secretOf(env, provider).id,
    redirect_uri: `${url.origin}/api/auth/${provider}/callback`,
    response_type: "code",
    state,
    code_challenge: await sha256(verifier),
    code_challenge_method: "S256",
    ...(p.scope ? { scope: p.scope } : {}),
    ...(provider === "google" ? { prompt: "select_account" } : {}),
  });
  return new Response(null, { status: 302, headers: {
    location: `${p.authorize}?${q}`,
    "set-cookie": setCookie(OAUTH_COOKIE, `${carried}.${await hmac(env.AUTH_SECRET, carried)}`, OAUTH_SECONDS, "/api/auth"),
  } });
}

async function oauthCallback(request, env, provider) {
  const url = new URL(request.url);
  const raw = cookies(request)[OAUTH_COOKIE] || "";
  const [carried, sig] = raw.split(".");
  let flow = null;
  if (carried && sig && env.AUTH_SECRET && sig === await hmac(env.AUTH_SECRET, carried)) {
    try { flow = JSON.parse(new TextDecoder().decode(Uint8Array.from(
      atob(carried.replace(/-/g, "+").replace(/_/g, "/")), (c) => c.charCodeAt(0)))); } catch (_) { flow = null; }
  }
  const back = flow ? safeReturn(flow.back) : "/";
  const done = (param, cookie) => {
    const h = new Headers({ location: `${url.origin}${back}${back.includes("?") ? "&" : "?"}${param}` });
    h.append("set-cookie", setCookie(OAUTH_COOKIE, "", 0, "/api/auth"));
    if (cookie) h.append("set-cookie", cookie);
    return new Response(null, { status: 302, headers: h });
  };
  if (!flow || flow.provider !== provider || flow.state !== url.searchParams.get("state")) return done("auth_error=state");
  if (url.searchParams.get("error")) return done("auth_error=cancelled");
  const code = url.searchParams.get("code");
  if (!code || !configured(env).includes(provider)) return done("auth_error=unavailable");

  const p = PROVIDERS[provider];
  const { id, secret } = secretOf(env, provider);
  let who;
  try {
    const tok = await fetch(p.token, {
      method: "POST",
      headers: { "content-type": "application/x-www-form-urlencoded", accept: "application/json" },
      body: new URLSearchParams({
        grant_type: "authorization_code", code, client_id: id, client_secret: secret,
        redirect_uri: `${url.origin}/api/auth/${provider}/callback`, code_verifier: flow.verifier,
      }),
    }).then((r) => r.json());
    if (!tok.access_token) return done("auth_error=provider");
    const user = await fetch(p.user, {
      headers: { authorization: `Bearer ${tok.access_token}`, accept: "application/json", "user-agent": "wfsim" },
    }).then((r) => r.json());
    who = p.who(user);
    if (!who.subject || who.subject === "undefined") return done("auth_error=provider");
  } catch (_) {
    return done("auth_error=provider");
  }
  const r = await arrive(env, request, { provider, ...who }, flow.intent);
  return r.ok ? done(`auth=${r.outcome}`, r.cookie) : done(`auth_error=${r.reason}`);
}

// ---- passwords ------------------------------------------------------------------

// PBKDF2-SHA256 AT 100,000 ROUNDS, the ceiling a Worker's WebCrypto allows —
// below what is advised for PBKDF2 alone, so the password is first keyed with a
// secret only the worker holds: a copy of the table without it cannot test a
// single guess. The stored string names its scheme and rounds, so either can
// move without breaking a stored hash.
const PBKDF2_ROUNDS = 100000;

async function stretch(env, password, salt, rounds) {
  const keyed = await hmac(`${env.AUTH_SECRET}:password`, password);
  const k = await crypto.subtle.importKey("raw", enc.encode(keyed), "PBKDF2", false, ["deriveBits"]);
  return b64url(await crypto.subtle.deriveBits({ name: "PBKDF2", hash: "SHA-256", salt, iterations: rounds }, k, 256));
}

async function hashPassword(env, password) {
  const salt = crypto.getRandomValues(new Uint8Array(16));
  return `pbkdf2-sha256$${PBKDF2_ROUNDS}$${b64url(salt)}$${await stretch(env, password, salt, PBKDF2_ROUNDS)}`;
}

async function passwordMatches(env, password, stored) {
  const [scheme, rounds, salt, hash] = String(stored || "").split("$");
  if (scheme !== "pbkdf2-sha256" || !hash) return false;
  const raw = Uint8Array.from(atob(salt.replace(/-/g, "+").replace(/_/g, "/")), (c) => c.charCodeAt(0));
  const got = await stretch(env, password, raw, Number(rounds));
  let diff = got.length ^ hash.length;
  for (let i = 0; i < Math.min(got.length, hash.length); i++) diff |= got.charCodeAt(i) ^ hash.charCodeAt(i);
  return diff === 0;
}

/// Eight to 128 characters, and nothing else asked of it.
const passwordOf = (p) => (typeof p === "string" && p.length >= PASSWORD_MIN && p.length <= PASSWORD_MAX ? p : null);

// ---- email: the first-party way in --------------------------------------------------

export const emailOf = (s) => {
  const e = String(s || "").trim().toLowerCase();
  return /^[^\s@]{1,64}@[^\s@]{1,253}\.[^\s@]{2,}$/.test(e) && e.length <= 254 ? e : null;
};
export const emailSlot = (env, email) => env.ACCOUNTS.prepare(
  "SELECT account, password_hash FROM identities WHERE provider = 'email' AND subject = ?1").bind(email).first();

const CODE_SUBJECTS = { register: "is your WFSim code", link: "is your WFSim code", reset: "resets your WFSim password" };

/// A SIX-DIGIT CODE, mailed, for one purpose. A code rather than a link, so it
/// works on whichever device the reader is on. The table keeps the code's HMAC
/// and, for a password chosen up front, that password's hash — neither in the
/// clear.
async function mailCode(env, email, purpose, { password_hash = null, account = null } = {}) {
  const prior = await env.ACCOUNTS.prepare("SELECT sent_at FROM email_codes WHERE email = ?1").bind(email).first();
  if (prior && Date.parse(prior.sent_at) > Date.now() - CODE_RESEND_SECONDS * 1000) return no("too_soon", 429);
  const code = String(crypto.getRandomValues(new Uint32Array(1))[0] % 1000000).padStart(6, "0");
  await env.ACCOUNTS.prepare(
    `INSERT INTO email_codes (email, code_hash, expires_at, attempts, sent_at, purpose, password_hash, account)
     VALUES (?1, ?2, ?3, 0, ?4, ?5, ?6, ?7)
     ON CONFLICT (email) DO UPDATE SET code_hash = ?2, expires_at = ?3, attempts = 0, sent_at = ?4,
       purpose = ?5, password_hash = ?6, account = ?7`,
  ).bind(email, await hmac(env.AUTH_SECRET, `${email}:${code}`), later(CODE_SECONDS), now(), purpose,
    password_hash, account).run();
  try {
    await env.EMAIL.send({
      to: email,
      from: EMAIL_FROM,
      subject: `${code} ${CODE_SUBJECTS[purpose]}`,
      text: `Your WFSim code is ${code}\n\nIt works for 10 minutes. If you did not ask for it, ignore this mail.\n\n你的 WFSim 验证码是 ${code}，10 分钟内有效。如果不是你本人操作，请忽略本邮件。\n`,
    });
  } catch (_) {
    return no("send_failed", 502);
  }
  return json({ ok: true, sent: true });
}

/// REGISTER: an address nobody holds, and a password for it. Nothing is
/// created until the code comes back.
async function emailRegister(env, b) {
  const email = emailOf(b.email);
  if (!email) return no("bad_email");
  const password = passwordOf(b.password);
  if (!password) return no("bad_password");
  if (await emailSlot(env, email)) return no("email_taken", 409);
  return mailCode(env, email, "register", { password_hash: await hashPassword(env, password) });
}

/// LINK: the signed-in account's email slot, filled or replaced once the
/// address proves itself. The password travels with the address.
async function emailLink(env, account, b) {
  const email = emailOf(b.email);
  if (!email) return no("bad_email");
  const password = passwordOf(b.password);
  if (!password) return no("bad_password");
  const held = await emailSlot(env, email);
  if (held && held.account !== account) return no("taken", 409);
  return mailCode(env, email, "link", { password_hash: await hashPassword(env, password), account });
}

/// RESET: a code to whoever holds the address, answered the same whether or
/// not anyone does, so the form tells nobody who has an account.
async function emailReset(env, b) {
  const email = emailOf(b.email);
  if (!email) return no("bad_email");
  if (!(await emailSlot(env, email))) return json({ ok: true, sent: true });
  return mailCode(env, email, "reset");
}

/// THE CODE COMES BACK, and completes what it was mailed for.
async function emailVerify(request, env, b) {
  const email = emailOf(b.email);
  const code = String(b.code || "").trim();
  if (!email || !/^\d{6}$/.test(code)) return no("bad_code");
  const db = env.ACCOUNTS;
  const row = await db.prepare(
    "SELECT code_hash, expires_at, attempts, purpose, password_hash, account FROM email_codes WHERE email = ?1",
  ).bind(email).first();
  if (!row || Date.parse(row.expires_at) < Date.now()) return no("expired");
  if (row.attempts >= CODE_ATTEMPTS) return no("too_many_attempts", 429);
  if (row.code_hash !== await hmac(env.AUTH_SECRET, `${email}:${code}`)) {
    await db.prepare("UPDATE email_codes SET attempts = attempts + 1 WHERE email = ?1").bind(email).run();
    return no("wrong_code");
  }
  if (row.purpose === "reset") {
    const password = passwordOf(b.password);
    if (!password) return no("bad_password");
    const slot = await emailSlot(env, email);
    await db.prepare("DELETE FROM email_codes WHERE email = ?1").bind(email).run();
    if (!slot) return no("expired");
    // A NEW PASSWORD ENDS EVERY OTHER SIGN-IN: whoever knew the old one is out.
    await db.batch([
      db.prepare("UPDATE identities SET password_hash = ?1 WHERE provider = 'email' AND subject = ?2")
        .bind(await hashPassword(env, password), email),
      db.prepare("DELETE FROM sessions WHERE account = ?1").bind(slot.account),
      db.prepare("DELETE FROM login_failures WHERE email = ?1").bind(email),
    ]);
    return json({ ok: true, outcome: "password_reset" }, 200, { "set-cookie": await newSession(env, slot.account) });
  }
  if (row.purpose === "link" && row.account !== await sessionAccount(env, request)) return no("not_signed_in", 401);
  await db.prepare("DELETE FROM email_codes WHERE email = ?1").bind(email).run();
  const r = await arrive(env, request, { provider: "email", subject: email, label: email, password_hash: row.password_hash },
    row.purpose === "link" ? "link" : "login");
  return r.ok ? json({ ok: true, outcome: r.outcome }, 200, r.cookie ? { "set-cookie": r.cookie } : {}) : no(r.reason, 409);
}

/// SIGNING IN WITH A PASSWORD sends no mail. A wrong address and a wrong
/// password read the same, and too many wrong ones close the address to
/// passwords for the rest of the window.
async function emailLogin(env, b) {
  const email = emailOf(b.email);
  const password = typeof b.password === "string" ? b.password : "";
  if (!email || !password) return no("wrong_credentials", 401);
  const db = env.ACCOUNTS;
  const f = await db.prepare("SELECT failures, first_at FROM login_failures WHERE email = ?1").bind(email).first();
  const lapsed = !f || Date.parse(f.first_at) < Date.now() - LOGIN_WINDOW_SECONDS * 1000;
  if (!lapsed && f.failures >= LOGIN_FAILURES) return no("locked", 429);
  const slot = await emailSlot(env, email);
  if (slot && slot.password_hash && await passwordMatches(env, password, slot.password_hash)) {
    await db.prepare("DELETE FROM login_failures WHERE email = ?1").bind(email).run();
    return json({ ok: true, outcome: "signed_in" }, 200, { "set-cookie": await newSession(env, slot.account) });
  }
  await db.prepare(
    `INSERT INTO login_failures (email, failures, first_at) VALUES (?1, 1, ?2)
     ON CONFLICT (email) DO UPDATE SET failures = CASE WHEN ?3 THEN 1 ELSE failures + 1 END,
       first_at = CASE WHEN ?3 THEN ?2 ELSE first_at END`,
  ).bind(email, now(), lapsed ? 1 : 0).run();
  return no("wrong_credentials", 401);
}

/// A NEW PASSWORD FOR THE SIGNED-IN ACCOUNT'S EMAIL SLOT. The old one is asked
/// for where there is one; an address linked before passwords has none to ask.
/// Every other sign-in ends.
async function passwordChange(request, env, account, b) {
  const db = env.ACCOUNTS;
  const slot = await db.prepare("SELECT password_hash FROM identities WHERE account = ?1 AND provider = 'email'")
    .bind(account).first();
  if (!slot) return no("not_linked");
  const password = passwordOf(b.password);
  if (!password) return no("bad_password");
  if (slot.password_hash && !(await passwordMatches(env, String(b.current || ""), slot.password_hash))) {
    return no("wrong_password", 403);
  }
  const token = cookies(request)[SESSION_COOKIE] || "";
  await db.batch([
    db.prepare("UPDATE identities SET password_hash = ?1 WHERE account = ?2 AND provider = 'email'")
      .bind(await hashPassword(env, password), account),
    db.prepare("DELETE FROM sessions WHERE account = ?1 AND token_hash != ?2").bind(account, await sha256(token)),
  ]);
  return json({ ok: true, outcome: "password_changed" });
}

// ---- the account itself -------------------------------------------------------------

async function accountView(env, account) {
  const a = await env.ACCOUNTS.prepare(
    "SELECT id, created_at, username, display_name, username_changed_at FROM accounts WHERE id = ?1",
  ).bind(account).first();
  if (!a) return null;
  const { results } = await env.ACCOUNTS.prepare(
    "SELECT provider, label, linked_at, password_hash IS NOT NULL AS has_password FROM identities WHERE account = ?1",
  ).bind(account).all();
  return { id: a.id, created_at: a.created_at, username: a.username, display_name: a.display_name,
    display_name_hidden: !!a.display_name && nameBlocked(a.display_name), rename_after: renameAfter(a),
    identities: SLOTS.map((s) => results.find((r) => r.provider === s)).filter(Boolean)
      .map(({ has_password, ...r }) => (r.provider === "email" ? { ...r, has_password: !!has_password } : r)) };
}

/// When this account may next change its username, or null for now: the first
/// change is free, each later one waits a day after the last.
const renameAfter = (a) => {
  if (!a.username_changed_at) return null;
  const at = new Date(Date.parse(a.username_changed_at) + RENAME_COOLDOWN_SECONDS * 1000).toISOString();
  return at > now() ? at : null;
};

/// A GIVEN-UP NAME IS HELD for its old owner, so nobody takes it at once and
/// passes as them. A `user_` name was never chosen and is not held.
const holdUsername = (db, name, account) => (name.startsWith(USERNAME_BORN) ? null : db.prepare(
  `INSERT INTO username_holds (username, account, held_until) VALUES (?1, ?2, ?3)
   ON CONFLICT (username) DO UPDATE SET account = ?2, held_until = ?3`,
).bind(name, account, later(USERNAME_HOLD_SECONDS)));

/// THE NAME THE ACCOUNT GOES BY: `username` and `display_name`, either or both.
async function profile(env, account, b) {
  const db = env.ACCOUNTS;
  const a = await db.prepare("SELECT username, username_changed_at FROM accounts WHERE id = ?1").bind(account).first();
  if (!a) return no("not_signed_in", 401);
  const writes = [];
  if ("display_name" in b) {
    const shown = String(b.display_name ?? "").replace(/[\u0000-\u001f\u007f]/g, "").trim();
    if ([...shown].length > DISPLAY_NAME_MAX) return no("bad_display_name");
    if (nameBlocked(shown)) return no("name_not_allowed");
    writes.push(db.prepare("UPDATE accounts SET display_name = ?1 WHERE id = ?2").bind(shown || null, account));
  }
  if ("username" in b) {
    const name = String(b.username ?? "").trim().toLowerCase();
    if (name !== a.username) {
      if (!USERNAME.test(name)) return no("bad_username");
      if (name.startsWith(USERNAME_BORN) || USERNAMES_RESERVED.has(name)) return no("username_reserved");
      if (nameBlocked(name)) return no("name_not_allowed");
      const after = renameAfter(a);
      if (after) return no("rename_too_soon", 429, { rename_after: after });
      const held = await db.prepare("SELECT account FROM username_holds WHERE username = ?1 AND held_until > ?2")
        .bind(name, now()).first();
      if (held && held.account !== account) return no("username_taken", 409);
      if (await db.prepare("SELECT 1 FROM accounts WHERE username = ?1").bind(name).first()) return no("username_taken", 409);
      writes.push(
        db.prepare("UPDATE accounts SET username = ?1, username_changed_at = ?2 WHERE id = ?3").bind(name, now(), account),
        db.prepare("DELETE FROM username_holds WHERE username = ?1 OR held_until <= ?2").bind(name, now()),
      );
      const hold = holdUsername(db, a.username, account);
      if (hold) writes.push(hold);
    }
  }
  // TWO READERS RACING FOR ONE NAME: the unique index lets one through.
  try { if (writes.length) await db.batch(writes); } catch (_) { return no("username_taken", 409); }
  return json({ ok: true, account: await accountView(env, account) });
}

/// A DELETED ACCOUNT'S NAME IS HELD like a given-up one.
async function holdOnDelete(env, account) {
  const a = await env.ACCOUNTS.prepare("SELECT username FROM accounts WHERE id = ?1").bind(account).first();
  const hold = a && holdUsername(env.ACCOUNTS, a.username, account);
  if (hold) await hold.run();
}

/// EMPTYING A SLOT. The last one is the account itself, so it is refused
/// unless the caller says it means that — and then the trigger takes the UUID,
/// its sessions and its data with it.
async function unlink(request, env, account, b) {
  if (!SLOTS.includes(b.provider)) return no("bad_provider");
  const { n } = await env.ACCOUNTS.prepare("SELECT COUNT(*) AS n FROM identities WHERE account = ?1").bind(account).first();
  const has = await env.ACCOUNTS.prepare("SELECT 1 FROM identities WHERE account = ?1 AND provider = ?2")
    .bind(account, b.provider).first();
  if (!has) return no("not_linked");
  if (n <= 1 && b.delete_account !== true) return no("last_slot", 409);
  if (n <= 1 && !(await cloudEnd(env, account))) return no("billing_open", 409);
  if (n <= 1) await holdOnDelete(env, account);
  await env.ACCOUNTS.prepare("DELETE FROM identities WHERE account = ?1 AND provider = ?2").bind(account, b.provider).run();
  const gone = n <= 1;
  return json({ ok: true, deleted: gone }, 200, gone ? { "set-cookie": endSession() } : {});
}

// ---- the router -----------------------------------------------------------------------

/// The response for an account path, or null for a path that is not one.
export async function accountRoute(request, env, path) {
  const m = path.match(/^\/api\/auth\/(google|discord|github)\/(start|callback)$/);
  if (m) {
    if (request.method !== "GET") return no("method", 405);
    if (!env.ACCOUNTS || !env.AUTH_SECRET) return no("unavailable", 503);
    return m[2] === "start" ? oauthStart(request, env, m[1]) : oauthCallback(request, env, m[1]);
  }
  if (path === "/api/account") {
    if (request.method !== "GET") return no("method", 405);
    const providers = configured(env);
    if (!providers.length) return json({ ok: true, account: null, providers });
    const account = await sessionAccount(env, request);
    return json({ ok: true, account: account ? await accountView(env, account) : null, providers });
  }
  const post = ["/api/auth/email/register", "/api/auth/email/verify", "/api/auth/email/login",
    "/api/auth/email/reset", "/api/auth/email/link", "/api/auth/logout",
    "/api/account/password", "/api/account/unlink", "/api/account/delete", "/api/account/export", "/api/account/profile"];
  if (!post.includes(path)) return null;
  if (request.method !== "POST") return no("method", 405);
  if (!sameSite(request)) return no("cross_site", 403);
  if (!env.ACCOUNTS || !env.AUTH_SECRET) return no("unavailable", 503);
  let b = {};
  try { b = JSON.parse((await request.text()) || "{}"); } catch (_) { return no("not_json"); }

  if (path.startsWith("/api/auth/email/")) {
    if (!env.EMAIL) return no("unavailable", 503);
    if (await limited(env, request)) return no("rate_limited", 429);
    if (path === "/api/auth/email/register") return emailRegister(env, b);
    if (path === "/api/auth/email/verify") return emailVerify(request, env, b);
    if (path === "/api/auth/email/login") return emailLogin(env, b);
    if (path === "/api/auth/email/reset") return emailReset(env, b);
    const signedIn = await sessionAccount(env, request);
    if (!signedIn) return no("not_signed_in", 401);
    return emailLink(env, signedIn, b);
  }
  if (path === "/api/auth/logout") {
    const token = cookies(request)[SESSION_COOKIE];
    if (token) await env.ACCOUNTS.prepare("DELETE FROM sessions WHERE token_hash = ?1").bind(await sha256(token)).run();
    return json({ ok: true }, 200, { "set-cookie": endSession() });
  }
  const account = await sessionAccount(env, request);
  if (!account) return no("not_signed_in", 401);
  if (path === "/api/account/unlink") return unlink(request, env, account, b);
  if (path === "/api/account/password") return passwordChange(request, env, account, b);
  if (path === "/api/account/profile") return profile(env, account, b);
  if (path === "/api/account/delete") {
    if (!(await cloudEnd(env, account))) return no("billing_open", 409);
    await holdOnDelete(env, account);
    await env.ACCOUNTS.prepare("DELETE FROM accounts WHERE id = ?1").bind(account).run();
    return json({ ok: true, deleted: true }, 200, { "set-cookie": endSession() });
  }
  // EVERYTHING HELD ABOUT THIS ACCOUNT, as it is held — docs/ACCOUNTS.md.
  const devices = (await env.ACCOUNTS.prepare("SELECT verifier, claimed_at FROM devices WHERE account = ?1")
    .bind(account).all()).results;
  const choice = await env.ACCOUNTS.prepare("SELECT named, chosen_at FROM contribution_choice WHERE account = ?1").bind(account).first();
  return json({ ok: true, account: await accountView(env, account), agents: await agentsOf(env, account), devices,
    contribution_choice: choice || null,
    ...(await cloudExport(env, account)),
    exported_at: now() }, 200,
    { "content-disposition": 'attachment; filename="wfsim-account.json"' });
}
