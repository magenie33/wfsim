// THE ONLY FETCH IN NONA. Every request leaves from here, to the address the
// reader typed and nowhere else; the key goes in its headers and in no other
// request. Which protocol an address speaks is the settings' `proto`, found by
// `detect` — never a branch on the provider's name inside the loop.

import { sseSplit } from "../core/protocols/sse.js";
import * as openai from "../core/protocols/openai.js";
import * as anthropic from "../core/protocols/anthropic.js";

const PROTOCOLS = { openai, anthropic };
const protocol = (cfg) => PROTOCOLS[cfg.proto] || openai;

/// A PROVIDER'S REFUSAL IN ITS OWN WORDS. Every provider puts the reason in
/// `error.message`; the status alone would say only that it failed.
async function jsonOf(res) {
  let body = null;
  try { body = await res.json(); } catch (_) { /* reported below */ }
  if (!res.ok) {
    const why = body && ((body.error && (body.error.message || body.error)) || body.message);
    const e = new Error(`${res.status} ${typeof why === "string" ? why : res.statusText}`);
    e.status = res.status;
    throw e;
  }
  return body || {};
}

const isStream = (res) => /event-stream/.test(res.headers.get("content-type") || "");

/// ONE MODEL TURN: the view encoded for the address, streamed back through the
/// protocol's decoder. `onText` gets the reply so far as it grows. An address
/// that answers with one JSON body instead of a stream is read the same way.
export async function send(cfg, view, { signal, onText }) {
  const p = protocol(cfg);
  const res = await fetch(p.chatUrl(cfg.base), {
    method: "POST", signal, headers: p.headers(cfg.key), body: JSON.stringify(p.encode(view, { model: cfg.model })),
  });
  if (!res.ok || !isStream(res)) {
    const out = p.decodeJson(await jsonOf(res));
    if (out.text) onText(out.text);
    return out;
  }
  const dec = p.streamDecoder();
  const reader = res.body.getReader();
  const td = new TextDecoder();
  let rest = "";
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    const split = sseSplit(rest + td.decode(value, { stream: true }));
    rest = split.rest;
    const before = dec.text();
    split.payloads.forEach((ev) => dec.push(ev));
    if (dec.text() !== before) onText(dec.text());
  }
  return dec.done();
}

/// ONE PLAIN ANSWER, no tools and no stream — what the summary asks for.
export async function complete(cfg, system, text, signal) {
  const p = protocol(cfg);
  const res = await fetch(p.chatUrl(cfg.base), {
    method: "POST", signal, headers: p.headers(cfg.key), body: JSON.stringify(p.encodePlain(system, text, { model: cfg.model })),
  });
  return p.decodePlain(await jsonOf(res));
}

/// WHAT AN ADDRESS SPEAKS AND SERVES, asked of the address itself: its model
/// list, in whichever protocol answers, the one its host names tried first. A
/// GET carries only the key's own headers, so no provider is asked to allow one
/// it does not know.
export async function detect(base, key) {
  const order = /anthropic\.com/.test(base) ? ["anthropic", "openai"] : ["openai", "anthropic"];
  let last = null;
  for (const proto of order) {
    const p = PROTOCOLS[proto];
    const { "Content-Type": _, "X-Title": __, ...headers } = p.headers(key);
    try {
      const models = p.parseModels(await jsonOf(await fetch(p.modelsUrl(base), { headers })));
      if (!models.length) { last = Object.assign(new Error("the address answered, but listed no models"), { empty: true }); continue; }
      return { proto, models: models.sort((a, b) => a.id.localeCompare(b.id)) };
    } catch (e) { last = e; }
  }
  throw last || new Error("no answer");
}

/// "The conversation no longer fits", in any provider's words.
export const tooLong = (e) => !!e && (e.status === 400 || e.status === 413)
  && /context|too long|maximum|token/i.test(e.message || "");

/// The browser could not reach the address at all — the network, or CORS.
export const unreachable = (e) => e instanceof TypeError;
