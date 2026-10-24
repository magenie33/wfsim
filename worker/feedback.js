// SPDX-License-Identifier: AGPL-3.0-or-later
// WHAT A READER TELLS THE OWNER — docs/ACCOUNTS.md §"Feedback". A signed-in
// reader's words, where they were, and up to three game screenshots: stored
// for the owner and the account alone, then sent to the owner as one Discord
// message by the paid half's bot (`cloudTell`). The words are kept as written:
// what they are about is read by the owner afterwards, never asked of the
// reader.
//
//   POST /api/feedback  { text, route?, share?, release?, lang?, images?: [jpeg data url] } → { ok, id }
import { sessionAccount, sameSite, json, no, now } from "./accounts.js";
import { cloudTell } from "./cloud.js";

export const FEEDBACK_TEXT_MAX = 4000;
export const FEEDBACK_IMAGES_MAX = 3;
/// The same ceiling as an offered riven screenshot (worker/ocr_samples.js).
export const FEEDBACK_IMAGE_BYTES_MAX = 1_500_000;
/// A share link's long form carries the whole build; past this it is not one.
const SHARE_MAX = 8000;

export const feedbackImageKey = (id, n) => `feedback/${id}/${n}.jpg`;

const jpeg = (url) => {
  const m = /^data:image\/jpeg;base64,([A-Za-z0-9+/=]+)$/.exec(String(url || ""));
  if (!m) return null;
  try { return Uint8Array.from(atob(m[1]), (c) => c.charCodeAt(0)); } catch (_) { return null; }
};
const line = (s, max) => (typeof s === "string" && s.length <= max && !/[\u0000-\u001f\u007f]/.test(s) ? s : null);

export async function feedbackRoute(request, env, path) {
  if (path !== "/api/feedback") return null;
  if (request.method !== "POST") return no("method", 405);
  if (!sameSite(request)) return no("cross_site", 403);
  if (!env.ACCOUNTS) return no("unavailable", 503);
  const account = await sessionAccount(env, request);
  if (!account) return no("not_signed_in", 401);
  // A HELD KEY CANNOT FLOOD THE OWNER'S DMS; a reader writing one after another
  // never meets it.
  if (env.FEEDBACK_LIMIT && !(await env.FEEDBACK_LIMIT.limit({ key: "feedback" + account })).success) {
    return no("rate_limited", 429);
  }
  let b = {};
  try { b = JSON.parse((await request.text()) || "{}"); } catch (_) { return no("not_json"); }
  const text = typeof b.text === "string" ? b.text.replace(/\r\n?/g, "\n").trim() : "";
  if (!text || text.length > FEEDBACK_TEXT_MAX) return no("bad_text");
  const raw = Array.isArray(b.images) ? b.images : [];
  if (raw.length > FEEDBACK_IMAGES_MAX) return no("too_many_images");
  if (raw.length && !env.UPLOADS) return no("images_unavailable", 501);
  const images = raw.map(jpeg);
  if (images.some((x) => !x || x.length > FEEDBACK_IMAGE_BYTES_MAX)) return no("bad_image");
  const route = line(b.route, 300);
  const share = line(b.share, SHARE_MAX);
  const release = line(b.release, 64);
  const lang = b.lang === "zh" ? "zh" : "en";

  const row = await env.ACCOUNTS.prepare(`INSERT INTO feedback (account, created_at, text, route, share, release, lang, images)
      VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) RETURNING id`)
    .bind(account, now(), text, route, share, release, lang, images.length).first();
  const id = Number(row.id);
  for (let n = 0; n < images.length; n++) {
    await env.UPLOADS.put(feedbackImageKey(id, n), images[n], { httpMetadata: { contentType: "image/jpeg" } });
  }

  // STORED IS SENT: the row is the record, so a message that fails to go out
  // still answers ok and the owner reads it where it is kept.
  const who = await env.ACCOUNTS.prepare("SELECT username FROM accounts WHERE id = ?1").bind(account).first();
  const head = `Feedback #${id} from ${who ? who.username : "?"} (${lang})${route ? ` on ${route}` : ""}${release ? `, release ${release}` : ""}`;
  const quoted = text.split("\n").map((l) => `> ${l}`).join("\n");
  const link = share ? `\n${share.length <= 1000 ? share : "(share link in the record)"}` : "";
  await cloudTell(env, `${head}${link}\n${quoted}`,
    images.map((bytes, n) => ({ name: `feedback-${id}-${n}.jpg`, type: "image/jpeg", bytes })));
  return json({ ok: true, id });
}

/// AN ACCOUNT'S OWN FEEDBACK, in its export: the words and where they were; the
/// screenshots by count.
export async function feedbackExport(env, account) {
  const { results } = await env.ACCOUNTS.prepare("SELECT id, created_at, text, route, share, release, lang, images FROM feedback WHERE account = ?1 ORDER BY id")
    .bind(account).all();
  return { feedback: results.map((r) => ({ ...r, id: Number(r.id), images: Number(r.images) })) };
}

/// BEFORE AN ACCOUNT IS DELETED its screenshots go from the bucket: the rows go
/// with the account by cascade, and nothing else would ever reach these.
export async function feedbackPurge(env, account) {
  if (!env.UPLOADS) return;
  const { results } = await env.ACCOUNTS.prepare("SELECT id, images FROM feedback WHERE account = ?1 AND images > 0").bind(account).all();
  const keys = results.flatMap((r) => Array.from({ length: Number(r.images) }, (_, n) => feedbackImageKey(Number(r.id), n)));
  if (keys.length) await env.UPLOADS.delete(keys);
}
