// SPDX-License-Identifier: AGPL-3.0-or-later
// THE QQ BOT'S DOOR IN — docs/AGENT.md §"The QQ bot". QQ calls back here; this proves the
// address (op 13), checks every event's signature, and keeps the messages the
// bot answers in `bot_inbox`. It sends nothing: QQ only takes calls from the
// whitelisted bot server, which claims the rows over `/api/qq/claim`.

const KINDS = new Set(["C2C_MESSAGE_CREATE", "GROUP_AT_MESSAGE_CREATE", "GROUP_MESSAGE_CREATE"]);
/// A COMMAND WITHOUT THE @: the word, then a space or the weapon's own script —
/// "fx 托里德", "zk托里德" — so "fxxk" and a bare "fx" in passing are chat.
/// `gx` is also chat's "恭喜", so without the @ only its slash form is one.
const COMMAND = /^\s*[/／]?\s*(?:fx|zk|pz)(?:\s+\S|[^\x00-\x7F])|^\s*[/／]\s*(?:帮助|gx\b)/i;

/// WHAT IS KEPT: a private chat, an @, and — in a group whose owner gave the bot
/// every message, where an @ arrives as GROUP_MESSAGE_CREATE too — an @ of this
/// bot or a command. The rest of the room's talk is never stored.
export function qqAddressed(kind, d) {
  if (!KINDS.has(kind) || !d || !d.id) return false;
  if (kind !== "GROUP_MESSAGE_CREATE") return true;
  return (d.mentions || []).some((m) => m && m.is_you) || COMMAND.test(String(d.content || ""));
}
const CLAIM_MAX = 20;
/// A claim not marked done within this is handed out again.
const RECLAIM_MS = 90_000;
const KEEP_MS = 86_400_000;

const json = (body, status = 200) => new Response(JSON.stringify(body), {
  status, headers: { "content-type": "application/json; charset=utf-8" } });
const hex = (buf) => [...new Uint8Array(buf)].map((b) => b.toString(16).padStart(2, "0")).join("");
const unhex = (s) => Uint8Array.from((String(s).match(/../g) || []).map((h) => parseInt(h, 16)));

/// THE BOT'S KEY, as QQ derives it: the AppSecret repeated to 32 bytes is the
/// Ed25519 seed. Imported as PKCS#8, the one form WebCrypto takes a bare seed in.
async function qqKeys(secret) {
  let seed = secret;
  while (seed.length < 32) seed = seed.repeat(2);
  const raw = new TextEncoder().encode(seed.slice(0, 32));
  const der = new Uint8Array([...unhex("302e020100300506032b657004220420"), ...raw]);
  const priv = await crypto.subtle.importKey("pkcs8", der, { name: "Ed25519" }, true, ["sign"]);
  const { x } = await crypto.subtle.exportKey("jwk", priv);
  const pub = await crypto.subtle.importKey("jwk", { kty: "OKP", crv: "Ed25519", x }, { name: "Ed25519" }, false, ["verify"]);
  return { priv, pub };
}

export async function qqRoute(request, env, path) {
  if (path === "/api/qq") return qqCallback(request, env);
  if (path === "/api/qq/claim") return qqClaim(request, env);
  return json({ ok: false, error: "not found" }, 404);
}

async function qqCallback(request, env) {
  if (request.method !== "POST") return json({ ok: false, error: "POST only" }, 405);
  if (!env.QQ_APP_SECRET || !env.LIBRARY) return json({ ok: false, error: "not offered here" }, 501);
  const body = await request.text();
  let ev;
  try { ev = JSON.parse(body); } catch (_) { return json({ ok: false, error: "not json" }, 400); }
  const keys = await qqKeys(env.QQ_APP_SECRET);
  // THE ADDRESS CHECK: sign what QQ sent, so it knows this door is ours.
  if (ev.op === 13) {
    const d = ev.d || {};
    const sig = await crypto.subtle.sign({ name: "Ed25519" }, keys.priv,
      new TextEncoder().encode(String(d.event_ts) + String(d.plain_token)));
    return json({ plain_token: d.plain_token, signature: hex(sig) });
  }
  // EVERY EVENT IS QQ'S, or nothing is kept: the signature covers the
  // timestamp and the body exactly as they arrived.
  const ts = request.headers.get("x-signature-timestamp") || "";
  const sig = unhex(request.headers.get("x-signature-ed25519") || "");
  const ok = sig.length === 64 && await crypto.subtle.verify({ name: "Ed25519" }, keys.pub, sig,
    new TextEncoder().encode(ts + body));
  if (!ok) return json({ ok: false, error: "bad signature" }, 401);
  if (ev.op === 0 && qqAddressed(ev.t, ev.d)) {
    await env.LIBRARY.prepare("INSERT OR IGNORE INTO bot_inbox (id, channel, kind, body, at) VALUES (?, 'qq', ?, ?, ?)")
      .bind(String(ev.d.id), ev.t, JSON.stringify(ev.d), Date.now()).run();
  }
  return json({ op: 12 });
}

/// THE SERVER'S PULL: the rows it has not answered, each handed to one claim
/// at a time; a body of `{ done: [ids] }` marks those answered first.
async function qqClaim(request, env) {
  if (request.method !== "POST") return json({ ok: false, error: "POST only" }, 405);
  const auth = request.headers.get("authorization") || "";
  if (!env.BOT_RELAY_TOKEN || auth !== `Bearer ${env.BOT_RELAY_TOKEN}`) return json({ ok: false, error: "unauthorized" }, 401);
  const now = Date.now();
  let b = {};
  try { b = await request.json(); } catch (_) {}
  const done = Array.isArray(b.done) ? b.done.map(String).slice(0, 100) : [];
  const db = env.LIBRARY;
  const writes = [db.prepare("DELETE FROM bot_inbox WHERE at < ?").bind(now - KEEP_MS)];
  for (const id of done) writes.push(db.prepare("UPDATE bot_inbox SET done_at = ? WHERE id = ?").bind(now, id));
  await db.batch(writes);
  const { results } = await db.prepare(`SELECT id, channel, kind, body, at FROM bot_inbox
    WHERE done_at IS NULL AND (claimed_at IS NULL OR claimed_at < ?) ORDER BY at LIMIT ?`)
    .bind(now - RECLAIM_MS, CLAIM_MAX).all();
  if (results.length) {
    await db.batch(results.map((r) => db.prepare("UPDATE bot_inbox SET claimed_at = ? WHERE id = ?").bind(now, r.id)));
  }
  return json({ ok: true, rows: results.map((r) => ({ ...r, body: JSON.parse(r.body) })) });
}
