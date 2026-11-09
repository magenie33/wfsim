// SPDX-License-Identifier: AGPL-3.0-or-later
import { hmac, json, later, limited, no, now, random, sameSite, sessionAccount, sha256 } from "./accounts.js";
import { claimDevice } from "./contribution.js";

export const DEVICE_LINK_SECONDS = 600;
export const DEVICE_LINK_INTERVAL = 10;
const PREFIX = "/api/account/devices/link/";
const ALPHABET = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const codeOf = (s) => typeof s === "string" ? s.toUpperCase().replace(/[-\s]/g, "") : "";
const codeHash = (env, code) => hmac(env.AUTH_SECRET, `device-link:${code}`);

export async function deviceLinkRoute(request, env, path) {
  if (!path.startsWith(PREFIX)) return null;
  const action = path.slice(PREFIX.length);
  if (!["start", "poll", "review", "approve"].includes(action)) return no("not_found", 404);
  if (request.method !== "POST") return no("method", 405);
  if (!env.ACCOUNTS || !env.AUTH_SECRET) return no("unavailable", 503);
  if (!sameSite(request)) return no("cross_site", 403);
  const text = await request.text();
  if (text.length > 2048) return no("too_large", 413);
  let b;
  try { b = JSON.parse(text); } catch (_) { return no("not_json"); }
  if (!b || typeof b !== "object" || Array.isArray(b)) return no("bad_request");
  const pollKey = action === "poll" && typeof b.device_code === "string" && /^[A-Za-z0-9_-]{43}$/.test(b.device_code)
    ? `:${(await sha256(b.device_code)).slice(0, 16)}` : "";
  if (await limited(env, request, `device-link-${action}${pollKey}:`)) return no("rate_limited", 429, { retry_after: DEVICE_LINK_INTERVAL });
  const db = env.ACCOUNTS;
  if (action === "start") {
    if (!/^[0-9a-f]{24}$/.test(b.verifier || "")) return no("bad_device");
    const label = typeof b.label === "string" ? b.label.trim() : "";
    if (!/^[^\u0000-\u001f\u007f]{1,40}$/u.test(label)) return no("bad_label");
    await db.prepare("DELETE FROM device_links WHERE expires_at <= ?1").bind(now()).run();
    const bytes = crypto.getRandomValues(new Uint8Array(8));
    const code = Array.from(bytes, x => ALPHABET[x % ALPHABET.length]).join("");
    const token = random(32), expires = later(DEVICE_LINK_SECONDS);
    await db.prepare("INSERT INTO device_links (token_hash, code_hash, verifier, label, expires_at) VALUES (?1, ?2, ?3, ?4, ?5)")
      .bind(await sha256(token), await codeHash(env, code), b.verifier, label, expires).run();
    const userCode = `${code.slice(0, 4)}-${code.slice(4)}`;
    const uri = `${new URL(request.url).origin}/compute`;
    return json({ ok: true, device_code: token, user_code: userCode, expires_in: DEVICE_LINK_SECONDS,
      interval: DEVICE_LINK_INTERVAL, verification_uri: uri, verification_uri_complete: `${uri}?device_code=${userCode}` });
  }
  if (action === "poll") {
    if (typeof b.device_code !== "string" || !/^[A-Za-z0-9_-]{43}$/.test(b.device_code)) return no("bad_code");
    const row = await db.prepare("SELECT claimed_at, expires_at FROM device_links WHERE token_hash = ?1")
      .bind(await sha256(b.device_code)).first();
    return json({ ok: true, status: !row || row.expires_at <= now() ? "expired" : row.claimed_at ? "claimed" : "pending" });
  }
  const account = await sessionAccount(env, request);
  if (!account) return no("not_signed_in", 401);
  const code = codeOf(b.user_code);
  if (!/^[ABCDEFGHJKLMNPQRSTUVWXYZ23456789]{8}$/.test(code)) return no("bad_code");
  const hash = await codeHash(env, code);
  const row = await db.prepare("SELECT verifier, label, account, claimed_at FROM device_links WHERE code_hash = ?1 AND expires_at > ?2")
    .bind(hash, now()).first();
  if (!row || row.account && row.account !== account) return no("expired", 404);
  if (action === "review") return json({ ok: true, label: row.label, id: row.verifier.slice(0, 6), claimed: !!row.claimed_at });
  if (row.claimed_at) return json({ ok: true });
  // Reserve a code for one account before invoking the existing ownership rule.
  // Retrying that account can finish an interrupted claim; another cannot steal it.
  const reserved = await db.prepare(`UPDATE device_links SET account = ?1
    WHERE code_hash = ?2 AND expires_at > ?3 AND (account IS NULL OR account = ?1) RETURNING verifier, label`)
    .bind(account, hash, now()).first();
  if (!reserved) return no("expired", 404);
  const claimed = await claimDevice(env, account, reserved);
  if (!claimed.ok) return claimed;
  await db.prepare("UPDATE devices SET label = ?1 WHERE verifier = ?2 AND account = ?3 AND (label IS NULL OR label != ?1)")
    .bind(reserved.label, reserved.verifier, account).run();
  await db.prepare("UPDATE device_links SET claimed_at = ?1 WHERE code_hash = ?2 AND account = ?3")
    .bind(now(), hash, account).run();
  return json({ ok: true });
}
