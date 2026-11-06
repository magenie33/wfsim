// SPDX-License-Identifier: AGPL-3.0-or-later
import { CLAN_NAME_MAX, clanName, requestClanInvite } from "./clan_request.js";

const json = (body, status = 200) => new Response(JSON.stringify(body), {
  status, headers: { "content-type": "application/json", "cache-control": "no-store" },
});
const reply = (content, components = []) => ({ type: 4, data: {
  content, flags: 64, allowed_mentions: { parse: [] }, components,
} });
const changeButton = [{ type: 1, components: [{ type: 2, style: 2,
  label: "Change name", custom_id: "clan:change" }] }];
const hex = (raw) => Uint8Array.from(raw.match(/../g), (part) => parseInt(part, 16));

// Discord signs timestamp + raw body; freshness limits replay of signed forms.
export async function verifyDiscord(request, body, publicKey, now = Date.now()) {
  const signature = request.headers.get("x-signature-ed25519") || "";
  const timestamp = request.headers.get("x-signature-timestamp") || "";
  if (!/^[a-f0-9]{128}$/i.test(signature) || !/^[a-f0-9]{64}$/i.test(publicKey || "")
      || !/^\d{10}$/.test(timestamp) || Math.abs(now / 1000 - Number(timestamp)) > 300) return false;
  try {
    const key = await crypto.subtle.importKey("raw", hex(publicKey), "Ed25519", false, ["verify"]);
    return await crypto.subtle.verify("Ed25519", key, hex(signature), new TextEncoder().encode(timestamp + body));
  } catch (_) { return false; }
}

function modal(replaces, previous = "") {
  return { type: 9, data: { custom_id: replaces ? "clan:replace" : "clan:request",
    title: "Join WFSim#582", components: [{ type: 18, label: "In-game name",
      description: "Copy the name shown in Warframe.", component: {
        type: 4, custom_id: "name", style: 1, required: true, min_length: 1,
        max_length: CLAN_NAME_MAX, ...(previous ? { value: previous } : {}),
      } }] } };
}

export async function discordInteraction(interaction, env, ctx) {
  if (interaction.application_id !== env.DISCORD_APPLICATION) return json({ error: "application" }, 403);
  if (interaction.type === 1) return json({ type: 1 });
  const user = interaction.member?.user;
  if (interaction.guild_id !== env.DISCORD_GUILD || !/^\d{5,25}$/.test(user?.id || "")
      || user.bot || interaction.member?.pending) return json({ error: "member" }, 403);
  const id = interaction.data?.custom_id;
  if (interaction.type === 3 && (id === "clan:join" || id === "clan:change")) {
    const previous = id === "clan:change"
      ? /^Request sent: `([^`]+)`/.exec(interaction.message?.content || "")?.[1] || "" : "";
    return json(modal(id === "clan:change", previous));
  }
  if (interaction.type !== 5 || !["clan:request", "clan:replace"].includes(id)) {
    return json(reply("This action is no longer available. Open the clan channel to try again."));
  }
  const fields = (interaction.data.components || []).flatMap((row) => row.components || [row.component]);
  const name = clanName(fields.find((field) => field?.custom_id === "name")?.value);
  if (!name || name.length > CLAN_NAME_MAX) return json(reply("Please enter a valid in-game name.", changeButton));
  if (!env.CLAN_LIMIT || !(await env.CLAN_LIMIT.limit({ key: "discord-clan:" + user.id })).success) {
    return json(reply("Please wait a minute, then try again.", changeButton));
  }
  // Acknowledge within Discord's three seconds; the owner notification may retry.
  const finish = async () => {
    let result;
    try {
      result = await requestClanInvite(env, { name, identity: `Discord user ${user.id}`,
        lang: String(interaction.locale || "").startsWith("zh") ? "zh" : "en", replaces: id === "clan:replace" });
    } catch (_) { result = { ok: false }; }
    const response = result.ok
      ? `Request sent: \`${name}\`\nWe’ll invite you in game.`
      : "Your request could not be sent. Please try again, or apply at https://wfsim.app/clan.";
    const url = `https://discord.com/api/v10/webhooks/${env.DISCORD_APPLICATION}/${interaction.token}/messages/@original`;
    const sent = await fetch(url, { method: "PATCH", headers: { "content-type": "application/json" },
      body: JSON.stringify({ content: response, allowed_mentions: { parse: [] }, components: changeButton }) });
    if (!sent.ok) console.error("Discord clan receipt failed", sent.status);
  };
  ctx.waitUntil(finish());
  return json({ type: 5, data: { flags: 64 } });
}

export default {
  async fetch(request, env, ctx) {
    if (new URL(request.url).pathname !== "/interactions") return json({ error: "not_found" }, 404);
    if (request.method !== "POST") return json({ error: "method" }, 405);
    if (Number(request.headers.get("content-length") || 0) > 65536) return json({ error: "too_large" }, 413);
    const body = await request.text();
    if (body.length > 65536) return json({ error: "too_large" }, 413);
    if (!await verifyDiscord(request, body, env.DISCORD_PUBLIC_KEY)) return json({ error: "signature" }, 401);
    let interaction;
    try { interaction = JSON.parse(body); } catch (_) { return json({ error: "json" }, 400); }
    return discordInteraction(interaction, env, ctx);
  },
};
