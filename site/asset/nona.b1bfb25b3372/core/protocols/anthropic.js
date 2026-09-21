// THE ANTHROPIC MESSAGES SHAPE. Pure: encoders build bodies, decoders read
// answers; the fetch is runtime's.

/// The API's root, whether or not the address was typed with /v1.
export const root = (base) => base.replace(/\/+$/, "").replace(/\/v1$/, "");
export const chatUrl = (base) => root(base) + "/v1/messages";
export const modelsUrl = (base) => root(base) + "/v1/models?limit=1000";
export const headers = (key) => ({ "Content-Type": "application/json", "x-api-key": key,
  "anthropic-version": "2023-06-01", "anthropic-dangerous-direct-browser-access": "true" });
export const MAX_TOKENS = 4096;

const EPHEMERAL = { type: "ephemeral" };

/// THREE CACHE BREAKPOINTS: the last system block, the last tool, and the
/// newest block — the last one follows the conversation so each turn reads the
/// one before it from the cache.
export function encode(v, { model }) {
  const messages = [];
  const push = (role, block) => {
    const last = messages[messages.length - 1];
    if (last && last.role === role) last.content.push(block); else messages.push({ role, content: [block] });
  };
  for (const t of v.turns) {
    if (t.role === "user") push("user", { type: "text", text: t.text });
    else if (t.role === "assistant") {
      if (t.text) push("assistant", { type: "text", text: t.text });
      for (const c of t.calls) push("assistant", { type: "tool_use", id: c.id, name: c.name, input: c.args || {} });
    } else push("user", { type: "tool_result", tool_use_id: t.id, content: t.text });
  }
  const last = messages[messages.length - 1];
  if (last) last.content[last.content.length - 1] = { ...last.content[last.content.length - 1], cache_control: EPHEMERAL };
  const tools = v.tools.map((t, i) => (i === v.tools.length - 1 ? { ...t, cache_control: EPHEMERAL } : t));
  const system = v.system.map((text, i) => (i === v.system.length - 1
    ? { type: "text", text, cache_control: EPHEMERAL } : { type: "text", text }));
  return { model, max_tokens: MAX_TOKENS, stream: true, system, messages, tools };
}

export const encodePlain = (system, text, { model }) =>
  ({ model, max_tokens: 2048, system, messages: [{ role: "user", content: text }] });
export const decodePlain = (body) => (body.content || []).filter((b) => b.type === "text").map((b) => b.text).join("\n");

export const usage = (u) => (u ? { input: (u.input_tokens || 0) + (u.cache_read_input_tokens || 0) + (u.cache_creation_input_tokens || 0),
  output: u.output_tokens || 0, cached: u.cache_read_input_tokens || 0 } : null);

const args = (s) => { try { return JSON.parse(s || "{}"); } catch (_) { return {}; } };

export function decodeJson(body) {
  const blocks = body.content || [];
  return { text: blocks.filter((b) => b.type === "text").map((b) => b.text).join("\n"), usage: usage(body.usage),
    calls: blocks.filter((b) => b.type === "tool_use").map((b) => ({ id: b.id, name: b.name, args: b.input || {} })) };
}

export function streamDecoder() {
  let text = "", u = {};
  const blocks = [];
  return {
    push(ev) {
      if (ev.type === "error") throw new Error((ev.error && ev.error.message) || "error");
      if (ev.type === "message_start") u = { ...(ev.message && ev.message.usage) };
      if (ev.type === "message_delta" && ev.usage) u.output_tokens = ev.usage.output_tokens;
      if (ev.type === "content_block_start") blocks[ev.index] = { ...ev.content_block, json: "" };
      if (ev.type === "content_block_delta") {
        const b = blocks[ev.index];
        if (ev.delta.type === "text_delta") text += ev.delta.text;
        if (ev.delta.type === "input_json_delta" && b) b.json += ev.delta.partial_json;
      }
    },
    text: () => text,
    done: () => ({ text, usage: usage(u),
      calls: blocks.filter((b) => b && b.type === "tool_use").map((b) => ({ id: b.id, name: b.name, args: args(b.json) })) }),
  };
}

export function parseModels(body) {
  const list = Array.isArray(body.data) ? body.data : [];
  return list.map((m) => ({ id: m.id, name: m.display_name || m.id, context: m.context_window || null, price: null, tools: null }));
}
