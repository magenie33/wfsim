// SPDX-License-Identifier: AGPL-3.0-or-later
import { cloudTell } from "./cloud_tell.js";

export const CLAN_NAME_MAX = 40;
export const clanName = (raw) => String(raw || "").replace(/[\u0000-\u001f\u007f`]/g, "").trim();

// Callers establish the identity before submitting; neither path stores names.
export async function requestClanInvite(env, { name: raw, identity, lang, replaces }) {
  const name = clanName(raw);
  if (!name || name.length > CLAN_NAME_MAX) return { ok: false, reason: "bad_ign" };
  const again = replaces === true ? " — replaces their previous one" : "";
  const sent = await cloudTell(env,
    `Clan invite: \`${name}\` — ${identity} (${lang === "zh" ? "zh" : "en"})${again}`);
  return sent ? { ok: true, name } : { ok: false, reason: "tell_failed" };
}
