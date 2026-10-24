// SPDX-License-Identifier: AGPL-3.0-or-later
// THE QQ BOT SERVER — docs/AGENT.md §"The QQ bot". On the one address QQ's
// whitelist holds: pull what chats sent from the site's door, answer it, reply
// through QQ's API with the message's own id (a passive reply, inside QQ's
// five minutes). Secrets come from the environment file, never from the repo.
//   node bot/qq.mjs            (systemd: deploy/wfsim-bot.service)
import { readFileSync } from "node:fs";
import { loadEngine, SITE } from "./engine.mjs";
import { makeAnswer } from "./answer.mjs";
import { renderCard, renderCardWith, renderDocument } from "./render.mjs";
import { cardDocument } from "./card.mjs";
import { relayWorld } from "./world.mjs";

const ENV_FILE = process.env.BOT_ENV || "/etc/wfsim-bot.env";
const env = Object.fromEntries(readFileSync(ENV_FILE, "utf8").split(/\r?\n/)
  .filter((l) => l.includes("=") && !l.startsWith("#")).map((l) => [l.slice(0, l.indexOf("=")).trim(), l.slice(l.indexOf("=") + 1).trim()]));
const API = env.QQ_API || "https://api.sgroup.qq.com";
const IDLE_MS = 1000;
/// A call to QQ or the site that has not answered in this has failed: the chat loop never waits unbounded.
const FETCH_MS = 30_000;
/// How long an appraisal's answer may take to draw: a replay is a whole fight.
const REPLAY_MS = 300_000;

let token = null;
async function accessToken() {
  if (token && Date.now() < token.until) return token.value;
  const r = await fetch("https://bots.qq.com/app/getAppAccessToken", { method: "POST",
    headers: { "content-type": "application/json" }, body: JSON.stringify({ appId: env.QQ_APP_ID, clientSecret: env.QQ_APP_SECRET }),
    signal: AbortSignal.timeout(FETCH_MS) });
  const j = await r.json();
  if (!j.access_token) throw new Error(`token: ${JSON.stringify(j).slice(0, 200)}`);
  // RENEWED A MINUTE EARLY, so a reply never carries one that just expired.
  token = { value: j.access_token, until: Date.now() + (Number(j.expires_in) - 60) * 1000 };
  return token.value;
}

/// A GROUP'S MESSAGE: an @, or any message the worker kept from a group that
/// gives the bot all of them (worker/qq.js `qqAddressed`).
const inGroup = (row) => row.kind === "GROUP_AT_MESSAGE_CREATE" || row.kind === "GROUP_MESSAGE_CREATE";

/// THE REPLY'S ADDRESS: a private chat answers its user, a group its group.
function chatPath(row) {
  const d = row.body;
  if (row.kind === "C2C_MESSAGE_CREATE") return `/v2/users/${d.author.user_openid}`;
  if (inGroup(row)) return `/v2/groups/${d.group_openid}`;
  return null;
}

async function qq(path, body) {
  const r = await fetch(API + path, { method: "POST", headers: { "content-type": "application/json",
    authorization: `QQBot ${await accessToken()}`, "x-union-appid": env.QQ_APP_ID }, body: JSON.stringify(body),
    signal: AbortSignal.timeout(FETCH_MS) });
  const text = await r.text();
  if (!r.ok) throw new Error(`${path}: ${r.status} ${text.slice(0, 300)}`);
  return text ? JSON.parse(text) : {};
}

/// A CARD ALREADY DRAWN is the same picture while the published board is: kept
/// by its address and the board's `digest`, read at most once a minute, and for
/// ten minutes at most, since a card prints how long ago it was measured.
const CARD_KEEP_MS = 600_000, CARD_KEEP_MAX = 64, STAMP_EVERY_MS = 60_000;
const cards = new Map();
let stamp = { digest: "", at: 0 };
async function boardDigest() {
  if (Date.now() - stamp.at < STAMP_EVERY_MS) return stamp.digest;
  const digest = await fetch(`${SITE}/board.meta.json`).then((r) => r.json()).then((j) => String(j.digest || "")).catch(() => "");
  stamp = { digest, at: Date.now() };
  return digest;
}
async function cardFor(url) {
  const digest = await boardDigest();
  const hit = digest && cards.get(url);
  if (hit && hit.digest === digest && Date.now() - hit.at < CARD_KEEP_MS) return hit.png;
  // DRAWN HERE, and only a card the bot cannot draw opens the page (bot/card.mjs).
  const html = await cardDocument(engine, url);
  const png = html ? await renderDocument(html) : await renderCard(url);
  if (digest) {
    cards.delete(url);
    cards.set(url, { png, digest, at: Date.now() });
    if (cards.size > CARD_KEEP_MAX) cards.delete(cards.keys().next().value);
  }
  return png;
}

/// THE ANSWER, SENT: the long image with its line, or the answer in words when
/// the image cannot be made or taken — a reader is never left with nothing.
async function send(row, ans) {
  const chat = chatPath(row);
  if (!chat) return;
  const reply = { msg_id: row.body.id, msg_seq: 1 };
  if (ans.card) {
    try {
      const png = await cardFor(ans.card);
      const media = await qq(`${chat}/files`, { file_type: 1, file_data: png.toString("base64"), srv_send_msg: false });
      await qq(`${chat}/messages`, { ...reply, msg_type: 7, content: ans.line, media: { file_info: media.file_info } });
      return;
    } catch (e) {
      console.error(`image ${row.id}: ${e && e.message || e}`);
    }
  }
  await qq(`${chat}/messages`, { ...reply, msg_type: 0, content: ans.text });
}

/// A RIVEN APPRAISAL, opened for the message that asked: the chat to answer is
/// kept with it, and only this bot reads that back (worker/appraise.js).
async function openAppraisal(row, ask) {
  const d = row.body;
  const group = inGroup(row);
  const r = await fetch(`${SITE}/api/appraise/new`, { method: "POST", headers: { "content-type": "application/json",
    authorization: `Bearer ${env.BOT_RELAY_TOKEN}` }, body: JSON.stringify({ ...ask, channel: "qq",
    chat: { kind: row.kind, msg_id: d.id, msg_at: row.at, ...(group ? { group_openid: d.group_openid } : { user_openid: d.author.user_openid }) },
    asker: (d.author && (d.author.member_openid || d.author.user_openid)) || "unknown", room: group ? d.group_openid : "" }) });
  const opened = await r.json().catch(() => ({ ok: false, error: String(r.status) }));
  if (opened.ok && opened.code) freezeAppraisal(opened.code).catch((e) => console.error("freeze", opened.code, e && e.message));
  return opened;
}

/// …AND ITS QUESTION FROZEN, so the community's computers can run it: the
/// search `headlessRivenGainRequest` builds from the appraisal — the one its
/// page sends — on this bot's own engine, which ships with the site, stored
/// with the appraisal (worker/appraise.js §"Volunteer work"). Until it is
/// stored nobody but a scan can answer it, so one still unfrozen a minute on
/// comes back from the claim and is frozen again.
const FREEZE_AGAIN_MS = 60_000;
/// When each code's last freeze failed; Infinity while one runs, or for good
/// once the engine has refused it.
const freezing = new Map();
async function freezeAppraisal(code) {
  if (Date.now() - (freezing.get(code) || 0) < FREEZE_AGAIN_MS) return;
  freezing.set(code, Infinity);
  try {
    const call = (path, init) => fetch(`${SITE}${path}`, { ...init, signal: AbortSignal.timeout(FETCH_MS) })
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error(`${path}: ${r.status}`))));
    const [job, release] = await Promise.all([call(`/api/appraise/${code}`), call("/release.json")]);
    const asked = engine.headless.headlessRivenGainRequest(engine.meta, { weapon: job.weapon, ruler: job.ruler, card: job.riven || {} });
    if (!asked) {
      console.error(`freeze ${code}: no search for ${job.weapon} under ${job.ruler}`);
      return;
    }
    await call(`/api/appraise/${code}/request`, { method: "POST", headers: { "content-type": "application/json",
      authorization: `Bearer ${env.BOT_RELAY_TOKEN}` }, body: JSON.stringify({ engine: release.engine, ...asked }) });
    freezing.delete(code);
  } catch (e) {
    freezing.set(code, Date.now());
    throw e;
  }
}

// ---- riven appraisals: judge what came back, tell the chat once ----------------
//
// docs/AGENT.md §"Riven appraisal". A loop of its own beside the messages, since
// one replay is a whole official fight and a chat must not wait behind it.

/// Where an answer goes: the group, or the private chat.
const roomOf = (chat) => chat.group_openid ? `g:${chat.group_openid}` : `u:${chat.user_openid}`;
const chatPathOf = (chat) => chat.group_openid ? `/v2/groups/${chat.group_openid}` : `/v2/users/${chat.user_openid}`;
/// Answers drawn while judging, by appraisal code, so telling does not draw twice.
const drawn = new Map();
/// Answers whose asker's window closed, by room, until that room says anything.
const held = new Map();
let judged = [], told = [], startedTold = [];

/// TO THE CHAT THAT ASKED: a reply to the asking message, and an active message
/// when QQ refuses the reply. QQ's stated reply window is not trusted either
/// way — its refusal is the measurement, logged with the message's age.
async function postToChat(chat, seq, body) {
  const path = `${chatPathOf(chat)}/messages`;
  const age = Math.round((Date.now() - Number(chat.msg_at || 0)) / 1000);
  try {
    return await qq(path, { ...body, msg_id: chat.msg_id, msg_seq: seq });
  } catch (e) {
    console.error(`reply refused ${age}s after the message, sending active: ${e && e.message || e}`);
  }
  return qq(path, body);
}

async function tellAppraisal(item, chatPath, post, late) {
  let png = (drawn.get(item.code) || {}).png;
  if (!png) png = (await renderCardWith(answer.answerCard(item, item.result_id), { wait: REPLAY_MS })).png;
  const media = await qq(`${chatPath}/files`, { file_type: 1, file_data: png.toString("base64"), srv_send_msg: false });
  await post({ msg_type: 7, content: answer.told(item, late), media: { file_info: media.file_info } });
  drawn.delete(item.code);
}

async function appraisalTick() {
  const r = await fetch(`${SITE}/api/appraise/claim`, { method: "POST", headers: { "content-type": "application/json",
    authorization: `Bearer ${env.BOT_RELAY_TOKEN}` }, body: JSON.stringify({ channel: "qq", judged, told, started_told: startedTold }) });
  if (!r.ok) throw new Error(`appraise claim: ${r.status}`);
  const { judge = [], tell = [], started = [], freeze = [] } = await r.json();
  judged = []; told = []; startedTold = [];
  // AN ORDER NO COMPUTER CAN SEE YET, frozen again beside this loop.
  for (const code of freeze) freezeAppraisal(code).catch((e) => console.error("freeze", code, e && e.message));
  // A COMPUTER TOOK IT: said once, however long the asker has waited.
  for (const s of started) {
    try {
      await postToChat(s.chat, 2, { msg_type: 0, content: answer.started(s) });
    } catch (e) {
      console.error(`started ${s.code}: ${e && e.message || e}`);
    }
    startedTold.push(s.code);
  }
  for (const j of judge) {
    try {
      const { png, verdict } = await renderCardWith(answer.answerCard(j, j.id), { wait: REPLAY_MS });
      const ok = !!(verdict && verdict.ok);
      judged.push({ id: j.id, ok, verdict: verdict || {} });
      if (ok && !drawn.has(j.code)) drawn.set(j.code, { png });
    } catch (e) {
      console.error(`judge ${j.code}/${j.id}: ${e && e.message || e}`);
      judged.push({ id: j.id, ok: false, verdict: { reason: String(e && e.message || e).slice(0, 200) } });
    }
  }
  for (const t of tell) {
    if ([...held.values()].flat().some((x) => x.code === t.code)) continue;
    try {
      await tellAppraisal(t, chatPathOf(t.chat), (body) => postToChat(t.chat, 3, body), false);
      told.push(t.code);
      continue;
    } catch (e) {
      console.error(`tell ${t.code}: ${e && e.message || e}`);
    }
    // NEITHER WENT (the chat turned active messages off, say): it rides on the
    // room's next message.
    const k = roomOf(t.chat);
    held.set(k, (held.get(k) || []).concat([t]));
  }
}

/// A ROOM SPOKE: what it was owed goes out on that message, after its answer.
async function tellHeld(row) {
  const d = row.body;
  const k = inGroup(row) ? `g:${d.group_openid}` : `u:${d.author.user_openid}`;
  const owed = held.get(k);
  if (!owed || !owed.length) return;
  held.delete(k);
  let seq = 2;
  for (const t of owed) {
    try {
      const reply = { msg_id: d.id, msg_seq: seq++ };
      await tellAppraisal(t, chatPath(row), (body) => qq(`${chatPath(row)}/messages`, { ...body, ...reply }), true);
      told.push(t.code);
    } catch (e) {
      console.error(`tell held ${t.code}: ${e && e.message || e}`);
    }
  }
}

async function claim(done) {
  const r = await fetch(`${SITE}/api/qq/claim`, { method: "POST", headers: { "content-type": "application/json",
    authorization: `Bearer ${env.BOT_RELAY_TOKEN}` }, body: JSON.stringify({ done }),
    signal: AbortSignal.timeout(FETCH_MS) });
  if (!r.ok) throw new Error(`claim: ${r.status}`);
  return (await r.json()).rows;
}

const engine = await loadEngine();
const answer = makeAnswer(engine);
console.log("wfsim-bot: up");
relayWorld(env.BOT_RELAY_TOKEN);
(async () => {
  for (;;) {
    try { await appraisalTick(); } catch (e) { console.error(String(e)); }
    await new Promise((r) => setTimeout(r, 3000));
  }
})();
/// THE CHAT LOOP MUST MOVE: one that has not in this long is stuck on something
/// no timeout covers, and exiting hands it to systemd's restart.
const STALL_MS = 300_000;
let beat = Date.now();
setInterval(() => {
  if (Date.now() - beat < STALL_MS) return;
  console.error(`chat loop stalled ${Math.round((Date.now() - beat) / 1000)} s, exiting for a restart`);
  process.exit(1);
}, 30_000);
let done = [];
for (;;) {
  beat = Date.now();
  let rows = [];
  try {
    rows = await claim(done);
    done = [];
  } catch (e) {
    console.error(String(e));
  }
  for (const row of rows) {
    beat = Date.now();
    try {
      await send(row, await answer(row.body.content, { openAppraisal: (ask) => openAppraisal(row, ask) }));
      await tellHeld(row);
    } catch (e) {
      console.error(`answer ${row.id}: ${e && e.stack || e}`);
    }
    // MARKED EITHER WAY: a message that cannot be answered must not block the rest.
    done.push(row.id);
  }
  if (!rows.length) await new Promise((r) => setTimeout(r, IDLE_MS));
}
