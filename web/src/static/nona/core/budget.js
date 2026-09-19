// THE CONTEXT BUDGET, in the order it is spent. docs/NONA.md, and the research
// behind it in the plan's research page: setting old tool results aside beats
// summarising them for an agent whose tools can be called again.

import { sent, pageText, toolText } from "./view.js";

/// A window the address does not state is assumed small, so a model with more
/// room is under-used rather than overrun. Past half the window the oldest tool
/// results and page snapshots are set aside, the newest kept verbatim; they go
/// in batches so each breaks the provider's prompt cache once rather than every
/// turn. The ratios are starting points, tuned against `nona_eval`.
export const BUDGET = { window: 64000, output: 4096, margin: 1500, mask_at: 0.5, force_at: 0.7,
  keep_tools: 6, keep_pages: 2, batch: 15000 };

/// TOKENS, ESTIMATED WITHOUT A TOKENIZER: a CJK character is about 0.6, any
/// other about 0.3 (DeepSeek's published rule of thumb), JSON a little more,
/// times the model's own correction (`calibrate`).
export function estimate(s, ratio = 1) {
  const cjk = (String(s).match(/[⺀-鿿가-힯＀-￯]/g) || []).length;
  return Math.ceil((cjk * 0.6 + (s.length - cjk) * 0.3) * 1.1 * ratio);
}

/// The next ratio, from what a provider billed against what was guessed.
export function calibrate(prev, estimated, billed) {
  if (!estimated || !billed) return prev || 1;
  const r = billed / (estimated / (prev || 1));
  return prev ? prev * 0.7 + r * 0.3 : r;
}

export const room = (window) => (window || BUDGET.window) - BUDGET.output - BUDGET.margin;

/// What the record costs to send, given the fixed part (rules, memory, tools).
export function recordCost(record, fixed, ratio) {
  return estimate(fixed + sent(record).map((m) => (m.role === "tool" ? toolText(m)
    : (m.text || "") + pageText(m) + JSON.stringify(m.calls || []))).join(""), ratio);
}

/// WHICH MESSAGES TO SET ASIDE now, by index — nothing while under half the
/// window; the oldest tool results and snapshots past it, once enough would go
/// (or the window is nearly full); all but the newest two and one when `force`,
/// which is what an address saying "too long" gets.
export function decide(record, { window, fixed, ratio, force }) {
  const est = recordCost(record, fixed, ratio);
  const none = { est, tools: [], pages: [] };
  const limit = room(window);
  if (!force && est < limit * BUDGET.mask_at) return none;
  const from = record.summary ? record.summary.upto : 0;
  const idx = (pred) => record.messages.map((m, i) => (i >= from && pred(m) ? i : -1)).filter((i) => i >= 0);
  const tools = idx((m) => m.role === "tool" && !m.masked);
  const pages = idx((m) => m.role === "user" && m.page && !m.pageMasked);
  const oldTools = tools.slice(0, Math.max(0, tools.length - (force ? 2 : BUDGET.keep_tools)));
  const oldPages = pages.slice(0, Math.max(0, pages.length - (force ? 1 : BUDGET.keep_pages)));
  const freed = estimate(oldTools.map((i) => record.messages[i].result).join("")
    + oldPages.map((i) => record.messages[i].page).join(""), ratio);
  if (!force && est < limit * BUDGET.force_at && freed < Math.min(BUDGET.batch, limit * 0.15)) return none;
  return { est: est - freed, tools: oldTools, pages: oldPages };
}

/// The record with those marks written in — a new record; the old is untouched.
/// Written INTO the record so the next request is sent the same prefix.
export function applyMarks(record, { tools, pages }) {
  if (!tools.length && !pages.length) return record;
  const t = new Set(tools), p = new Set(pages);
  return { ...record, messages: record.messages.map((m, i) => (t.has(i) ? { ...m, masked: true }
    : p.has(i) ? { ...m, pageMasked: true } : m)) };
}

/// What a reply cost, where the address states prices: the part read from the
/// provider's cache is counted at a tenth, which is what most of them charge.
export function cost(price, u) {
  if (!u || !price) return null;
  return ((u.input - u.cached) * price[0] + u.cached * price[0] * 0.1 + u.output * price[1]) / 1e6;
}
