// THE SETTINGS: an address, a key, what the address serves, and a model — then
// everything she remembers. The draft is not saved until Save, so trying an
// address does not lose the one that works.

import { SLOT_LABELS } from "../core/memory.js";
import * as store from "../runtime/store.js";
import { detect, unreachable } from "../runtime/transport.js";
import { tr, esc, dd, $, k } from "./kit.js";

/// QUICK FILLS FOR THE ADDRESS, and nothing more: a provider is its base URL.
/// The reader may type any other, and which protocol it speaks and which models
/// it serves are DETECTED from the address and the key, never read off this list.
const PRESETS = [
  ["OpenRouter", "https://openrouter.ai/api/v1"],
  ["DeepSeek", "https://api.deepseek.com/v1"],
  ["OpenAI", "https://api.openai.com/v1"],
  ["Anthropic", "https://api.anthropic.com"],
  ["Qwen (DashScope)", "https://dashscope.aliyuncs.com/compatible-mode/v1"],
  ["Kimi", "https://api.moonshot.cn/v1"],
  ["SiliconFlow", "https://api.siliconflow.cn/v1"],
];
const CORS = "The provider could not be reached from the browser — the network failed, or it does not accept requests from a web page (CORS).";

const draft = { base: "", key: "", proto: null, model: "", models: null, state: "", error: "", remember: false, concise: false, asking: "" };

export function fillSettings(onSaved) {
  const s = store.settings();
  Object.assign(draft, { base: s.base || PRESETS[0][1], key: s.key || "", proto: s.proto || null,
    model: s.model || "", models: null, state: "", error: "", remember: !!s.remember, concise: !!s.concise });
  draft.onSaved = onSaved;
  paint();
  if (draft.key) redetect();
}

/// The model control: the page's own searchable dropdown once the address has
/// listed its models, a typed id while it has not (some addresses list none).
function modelControl() {
  const d = draft;
  if (!d.models) {
    return `<input id="nona-model" type="text" autocomplete="off" value="${esc(d.model)}" placeholder="${esc(tr("model id"))}">`;
  }
  const money = (x) => `$${x < 1 ? x.toFixed(2) : x.toFixed(1)}`;
  return dd("nona-model-dd", {
    value: d.model, search: true, placeholder: tr("choose a model"),
    items: d.models.map((m) => ({
      value: m.id, label: m.name === m.id ? m.id : `${m.name} · ${m.id}`,
      group: m.id.includes("/") ? m.id.split("/")[0] : undefined,
      hint: [m.context ? `${k(m.context)} ${tr("context")}` : "",
        m.price ? `${money(m.price[0])} / ${money(m.price[1])} ${tr("per million tokens in / out")}` : "",
        m.tools === false ? tr("cannot call tools — Nona needs them") : ""].filter(Boolean).join(" · ") || undefined,
      disabled: m.tools === false || undefined,
    })),
    onPick: (v) => { d.model = v; paint(); },
  });
}

function paint() {
  const d = draft;
  const box = $("nona-settings");
  const status = d.state === "working" ? tr("checking the address…")
    : d.state === "ok" ? `${tr("connected")} · ${d.proto === "anthropic" ? "Anthropic" : tr("OpenAI-compatible")} · ${d.models.length} ${tr("models")}`
    : d.state === "error" ? d.error : "";
  box.innerHTML = `
    <div class="nona-presets"><span class="nona-lbl">${esc(tr("Quick fill"))}</span>${PRESETS.map(([name, url]) =>
      `<span class="pchip${url === d.base ? " sel" : ""}" data-nona-base="${esc(url)}">${esc(tr(name))}</span>`).join("")}</div>
    <label>${esc(tr("Base URL"))}<input id="nona-base" type="url" autocomplete="off" value="${esc(d.base)}"></label>
    <label>${esc(tr("API key"))}<input id="nona-apikey" type="password" autocomplete="off" value="${esc(d.key)}"></label>
    <label class="check"><input type="checkbox" id="nona-remember"${d.remember ? " checked" : ""}> ${esc(tr("Remember the key in this browser"))}</label>
    <p class="nona-fine">${esc(tr("Otherwise it is forgotten when this tab closes. Set a spending limit on the key at your provider."))}</p>
    <div class="nona-detect"><button class="ghost-btn small" id="nona-check">${esc(tr("Check"))}</button>
      <span class="nona-status${d.state === "error" ? " bad" : d.state === "ok" ? " good" : ""}">${esc(status)}</span></div>
    <label>${esc(tr("Model"))}${modelControl()}</label>
    <label class="check"><input type="checkbox" id="nona-concise"${d.concise ? " checked" : ""}> ${esc(tr("Concise mode — no persona"))}</label>
    <p class="nona-fine">${esc(tr("Your key is stored only in this browser and is sent only to the address above."))}</p>
    <button class="run-btn" id="nona-save"${d.model && d.key && d.base ? "" : " disabled"}>${esc(tr("Save"))}</button>
    <div class="nona-memory" id="nona-memory"></div>`;
  paintMemory();
  box.querySelectorAll("[data-nona-base]").forEach((el) => el.addEventListener("click", () => {
    d.base = el.dataset.nonaBase; d.models = null; d.proto = null; d.state = "";
    paint();
    if (d.key) redetect();
  }));
  const base = $("nona-base"), key = $("nona-apikey"), typed = $("nona-model");
  base.addEventListener("change", () => { d.base = base.value.trim(); d.models = null; d.proto = null; if (d.key) redetect(); else paint(); });
  key.addEventListener("change", () => { d.key = key.value.trim(); if (d.key && d.base) redetect(); else paint(); });
  $("nona-remember").addEventListener("change", (e) => { d.remember = e.target.checked; });
  $("nona-concise").addEventListener("change", (e) => { d.concise = e.target.checked; });
  if (typed) typed.addEventListener("input", () => { d.model = typed.value.trim(); $("nona-save").disabled = !(d.model && d.key && d.base); });
  $("nona-check").addEventListener("click", () => { d.base = base.value.trim(); d.key = key.value.trim(); redetect(); });
  $("nona-save").addEventListener("click", () => {
    const m = (d.models || []).find((x) => x.id === d.model) || {};
    store.saveSettings({ base: d.base, key: d.key, remember: d.remember, concise: d.concise, model: d.model,
      proto: d.proto || (/anthropic\.com/.test(d.base) ? "anthropic" : "openai"),
      context: m.context || null, price: m.price || null });
    if (d.onSaved) d.onSaved();
  });
}

/// WHAT SHE REMEMBERS, all of it, where the reader can change or drop any of
/// it, pause memory, or take a copy.
function paintMemory() {
  const box = $("nona-memory");
  if (!box) return;
  const m = store.memory.get();
  box.innerHTML = `<div class="nona-lbl">${esc(tr("What Nona remembers"))} · ${esc(tr("kept only in this browser"))}</div>`
    + (m.items.length ? m.items.map((x) => `<div class="nona-mem-row" data-id="${esc(x.id)}">`
      + `<span class="nona-mem-k">${esc(x.key ? tr(SLOT_LABELS[x.key]) : tr("note"))}${x.status === "proposed" ? ` · ${esc(tr("unconfirmed"))}` : ""}</span>`
      + `<input type="text" value="${esc(x.value)}" data-mem-edit="${esc(x.id)}">`
      + `<button class="ghost-btn small" data-mem-del="${esc(x.id)}">✕</button></div>`).join("")
      : `<div class="nona-fine">${esc(tr("nothing yet"))}</div>`)
    + `<div class="nona-mem-b"><label class="check"><input type="checkbox" id="nona-mem-pause"${m.paused ? " checked" : ""}> ${esc(tr("Pause memory"))}</label>`
    + `<button class="ghost-btn small" id="nona-mem-copy">${esc(tr("Copy as JSON"))}</button>`
    + `<button class="ghost-btn small" id="nona-mem-clear">${esc(tr("Forget everything"))}</button></div>`;
  box.querySelectorAll("[data-mem-edit]").forEach((el) => el.addEventListener("change", () => {
    if (el.value.trim()) store.memory.edit(el.dataset.memEdit, el.value.trim());
  }));
  box.querySelectorAll("[data-mem-del]").forEach((el) => { el.onclick = () => { store.memory.forget(el.dataset.memDel); paintMemory(); }; });
  $("nona-mem-pause").onchange = (e) => store.memory.pause(e.target.checked);
  $("nona-mem-copy").onclick = async (e) => {
    try { await navigator.clipboard.writeText(JSON.stringify(store.memory.get(), null, 1)); e.target.textContent = `✓ ${tr("copied")}`; } catch (_) { /* no clipboard */ }
  };
  // FORGETTING EVERYTHING TAKES TWO CLICKS — no native dialog here.
  $("nona-mem-clear").onclick = (e) => {
    const b = e.currentTarget;
    if (!b.dataset.armed) { b.dataset.armed = "1"; b.textContent = tr("Forget everything?"); return; }
    store.memory.clear();
    paintMemory();
  };
}

/// Ask the address what it serves. A later answer to an earlier question is
/// dropped: the reader may have changed the address while it was out.
async function redetect() {
  const d = draft;
  if (!d.base || !d.key) return;
  const ask = JSON.stringify([d.base, d.key]);
  d.asking = ask; d.state = "working"; paint();
  try {
    const r = await detect(d.base, d.key);
    if (d.asking !== ask) return;
    Object.assign(d, { proto: r.proto, models: r.models, state: "ok", error: "" });
    if (d.model && !r.models.some((m) => m.id === d.model)) d.model = "";
  } catch (e) {
    if (d.asking !== ask) return;
    Object.assign(d, { models: null, state: "error",
      error: unreachable(e) ? tr(CORS) : e.empty ? tr("the address answered, but listed no models")
        : `${tr("could not list models")}: ${e.message || e}` });
  }
  paint();
}
