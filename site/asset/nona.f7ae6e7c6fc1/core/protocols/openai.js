// THE OPENAI CHAT-COMPLETIONS SHAPE — spoken by OpenAI and by OpenRouter,
// DeepSeek, DashScope, Kimi, SiliconFlow and any address that copies it.
// Pure: encoders build bodies, decoders read answers; the fetch is runtime's.

const root = (base) => base.replace(/\/+$/, "");
export const chatUrl = (base) => root(base) + "/chat/completions";
export const modelsUrl = (base) => root(base) + "/models";
export const headers = (key) => ({ "Content-Type": "application/json", Authorization: `Bearer ${key}`, "X-Title": "WFSim" });

export function encode(v, { model }) {
  const messages = v.system.map((content) => ({ role: "system", content }));
  for (const t of v.turns) {
    if (t.role === "user") messages.push({ role: "user", content: t.text });
    else if (t.role === "assistant") {
      messages.push({ role: "assistant", content: t.text || null,
        ...(t.calls.length ? { tool_calls: t.calls.map((c) => ({ id: c.id, type: "function",
          function: { name: c.name, arguments: JSON.stringify(c.args || {}) } })) } : {}) });
    } else messages.push({ role: "tool", tool_call_id: t.id, content: t.text });
  }
  return { model, messages, stream: true, stream_options: { include_usage: true },
    tools: v.tools.map((t) => ({ type: "function", function: { name: t.name, description: t.description, parameters: t.input_schema } })) };
}

/// One plain answer, no tools and no stream — what the summary asks for.
export const encodePlain = (system, text, { model }) =>
  ({ model, messages: [{ role: "system", content: system }, { role: "user", content: text }] });
export const decodePlain = (body) => (((body.choices || [])[0] || {}).message || {}).content || "";

export const usage = (u) => (u ? { input: u.prompt_tokens || 0, output: u.completion_tokens || 0,
  cached: (u.prompt_tokens_details && u.prompt_tokens_details.cached_tokens) || u.prompt_cache_hit_tokens || 0 } : null);

const args = (s) => { try { return JSON.parse(s || "{}"); } catch (_) { return {}; } };

export function decodeJson(body) {
  const msg = ((body.choices || [])[0] || {}).message || {};
  return { text: msg.content || "", usage: usage(body.usage),
    calls: (msg.tool_calls || []).map((c) => ({ id: c.id, name: c.function.name, args: args(c.function.arguments) })) };
}

/// A STREAM, assembled: text deltas appended, a tool call's name and arguments
/// joined by index across chunks, usage from the last chunk. `push` throws on
/// an error event, which a stream reports in-band.
export function streamDecoder() {
  let text = "", u = null;
  const calls = [];
  return {
    push(ev) {
      if (ev.error) throw new Error(ev.error.message || String(ev.error));
      if (ev.usage) u = usage(ev.usage);
      const d = ((ev.choices || [])[0] || {}).delta || {};
      if (d.content) text += d.content;
      for (const tc of d.tool_calls || []) {
        const i = tc.index ?? calls.length;
        const at = calls[i] || (calls[i] = { id: "", name: "", args: "" });
        if (tc.id) at.id = tc.id;
        if (tc.function && tc.function.name) at.name += tc.function.name;
        if (tc.function && tc.function.arguments) at.args += tc.function.arguments;
      }
    },
    text: () => text,
    done: () => ({ text, usage: u, calls: calls.filter(Boolean).map((c) => ({ ...c, args: args(c.args) })) }),
  };
}

/// A model list, in the shape OpenRouter and its imitators give it.
export function parseModels(body) {
  const list = Array.isArray(body.data) ? body.data : Array.isArray(body) ? body : [];
  return list.map((m) => ({
    id: m.id, name: m.name || m.id, context: m.context_length || null,
    price: m.pricing && Number(m.pricing.prompt) >= 0 ? [Number(m.pricing.prompt) * 1e6, Number(m.pricing.completion) * 1e6] : null,
    tools: Array.isArray(m.supported_parameters) ? m.supported_parameters.includes("tools") : null,
  }));
}
