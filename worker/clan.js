// SPDX-License-Identifier: AGPL-3.0-or-later
// A CLAN INVITE ASKED FOR — docs/ACCOUNTS.md §"Clan invites". A signed-in reader
// leaves an in-game name; it goes to the owner as one Discord message, sent by
// the paid half's bot (`cloudTell`), and nothing of it is stored here.
import { sessionAccount, sameSite, json, no } from "./accounts.js";
import { cloudTell } from "./cloud.js";

/// Longer than any platform's in-game name; a name past it is not one.
const NAME_MAX = 40;

export async function clanRoute(request, env, path) {
  if (path !== "/api/clan/request") return null;
  if (request.method !== "POST") return no("method", 405);
  if (!sameSite(request)) return no("cross_site", 403);
  if (!env.ACCOUNTS) return no("unavailable", 503);
  const account = await sessionAccount(env, request);
  if (!account) return no("not_signed_in", 401);
  // TWO MESSAGES A MINUTE PER ACCOUNT (a name and its correction), so a held
  // key cannot flood the owner's DMs.
  if (env.CLAN_LIMIT && !(await env.CLAN_LIMIT.limit({ key: "clan" + account })).success) {
    return no("rate_limited", 429);
  }
  let b = {};
  try { b = JSON.parse((await request.text()) || "{}"); } catch (_) { return no("not_json"); }
  // A backtick would close the code span the name is shown in.
  const name = String(b.name || "").replace(/[\u0000-\u001f\u007f`]/g, "").trim();
  if (!name || name.length > NAME_MAX) return no("bad_ign");
  const row = await env.ACCOUNTS.prepare("SELECT username FROM accounts WHERE id = ?1").bind(account).first();
  const lang = b.lang === "zh" ? "zh" : "en";
  // A NAME SENT AGAIN from the page corrects the first, and says so.
  const again = b.replaces === true ? " — replaces their previous one" : "";
  const sent = await cloudTell(env, `Clan invite: \`${name}\` — WFSim user ${row ? row.username : "?"} (${lang})${again}`);
  return sent ? json({ ok: true }) : no("tell_failed", 502);
}
