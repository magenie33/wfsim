// SPDX-License-Identifier: AGPL-3.0-or-later
// NONA'S COMMAND PANELS AND HER PRIVATE-CHAT MENU, set through QQ's API
// (bot.q.qq.com, 菜单面板). A tapped command fills the reader's input box, so a
// command can be sent without typing it — docs/AGENT.md §"The QQ bot".
// Run on the bot server, the one address QQ takes calls from; it replaces the
// panels it made before (marked by `remark`) and leaves any others alone.
//   node bot/menus.mjs
import { readFileSync } from "node:fs";
import { loadEngine } from "./engine.mjs";

const env = Object.fromEntries(readFileSync(process.env.BOT_ENV || "/etc/wfsim-bot.env", "utf8").split(/\r?\n/)
  .filter((l) => l.includes("=") && !l.startsWith("#")).map((l) => [l.slice(0, l.indexOf("=")).trim(), l.slice(l.indexOf("=") + 1).trim()]));
const API = env.QQ_API || "https://api.sgroup.qq.com";
const MARK = "wfsim:nona";

// THE COMMANDS, in the language overlay's words; a command's name is what
// lands in the box, so it is never translated.
const { zh } = await loadEngine();
const t = (x) => (zh.ui && zh.ui[x]) || x;
const COMMANDS = [
  { type: "command", name: "pz", desc: t("the best builds on the board") },
  { type: "command", name: "zk", desc: t("rivens against the best build without one") },
  { type: "command", name: "fx", desc: t("the gain of your own riven") },
  { type: "command", name: "gx", desc: t("the compute contribution rankings") },
  { type: "command", name: "帮助", desc: t("what Nona can do") },
  { type: "link", name: t("WFSim, the site"), desc: t("open the site"), link: "https://wfsim.app" },
];
const MENU = [
  { type: "send_message", name: t("Builds"), send_message: "pz " },
  { type: "send_message", name: t("Rivens"), send_message: "zk " },
  { type: "send_message", name: t("Riven gain"), send_message: "fx " },
  { type: "send_message", name: t("Contribution"), send_message: "gx" },
  { type: "send_message", name: t("Help"), send_message: "帮助" },
  { type: "link", name: t("Site"), link: "https://wfsim.app" },
];

async function token() {
  const r = await fetch("https://bots.qq.com/app/getAppAccessToken", { method: "POST",
    headers: { "content-type": "application/json" }, body: JSON.stringify({ appId: env.QQ_APP_ID, clientSecret: env.QQ_APP_SECRET }) });
  const j = await r.json();
  if (!j.access_token) throw new Error(`token: ${JSON.stringify(j).slice(0, 200)}`);
  return j.access_token;
}

const tk = await token();
async function qq(method, path, body) {
  const r = await fetch(API + path, { method, headers: { "content-type": "application/json",
    authorization: `QQBot ${tk}`, "x-union-appid": env.QQ_APP_ID }, ...(body ? { body: JSON.stringify(body) } : {}) });
  const text = await r.text();
  console.log(`${method} ${path} → ${r.status} ${text.slice(0, 300)}`);
  if (!r.ok) throw new Error(`${method} ${path}: ${r.status}`);
  return text ? JSON.parse(text) : {};
}

// ONE SCENE AT A TIME: the list is asked per scene (`scope`), and only the
// panels this script made are replaced.
for (const scope of ["group", "c2c"]) {
  const listed = await qq("GET", `/v2/panels?scope=${scope}`).catch(() => ({}));
  const all = Array.isArray(listed) ? listed : listed.records || listed.panels || [];
  for (const p of all.filter((x) => String((x.panel || x).remark || "").startsWith(MARK))) {
    await qq("DELETE", `/v2/panels/${p.panel_id || p.id}`).catch(() => {});
  }
  await qq("POST", "/v2/panels", { scope, target_type: "all", panel: { items: COMMANDS, remark: `${MARK} ${scope}` } });
}
await qq("PUT", "/v2/menu", { menu: { items: MENU } });
