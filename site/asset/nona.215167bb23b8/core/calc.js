// ARITHMETIC ON MEASURED NUMBERS. A difference between two measured scores is
// itself measured — but only when both were measured: every number in the
// expression must be one she was sent (`measured`, the same test the panel's
// mark uses), so a figure she made up cannot be laundered through a sum.
// Parsed by hand, never `eval`'d.

import { measured } from "./measure.js";

/// Tokens: numbers (with an optional %, k, 万 or M, as the mark reads them),
/// the four operators in either script, and brackets.
function tokens(src) {
  const out = [];
  const re = /\s*(?:(\d[\d,]*(?:\.\d+)?)(%|k|K|万|M)?|([-+*/×÷()]))/y;
  let at = 0;
  while (at < src.length) {
    re.lastIndex = at;
    const m = re.exec(src);
    if (!m) { if (/^\s*$/.test(src.slice(at))) break; throw new Error(`cannot read "${src.slice(at, at + 12)}"`); }
    at = re.lastIndex;
    if (m[1] != null) out.push({ num: m[1], suffix: m[2] || "" });
    else out.push({ op: m[3] === "×" ? "*" : m[3] === "÷" ? "/" : m[3] });
  }
  return out;
}

const SCALE = { "%": 0.01, k: 1e3, K: 1e3, "万": 1e4, M: 1e6 };

/// `expression` evaluated, or why not. `numbers` is what she was sent.
export function calc(expression, numbers) {
  let ts;
  try { ts = tokens(String(expression || "")); } catch (e) { return { ok: false, reason: "bad_expression", because: e.message }; }
  if (!ts.length) return { ok: false, reason: "missing_argument", argument: "expression" };
  // A CONSTANT IS NOT A CLAIM: a whole number under 100, or a power of ten —
  // what turns a fraction into a percentage — needs no source.
  const constant = (t) => !t.suffix && /^(\d{1,2}|10+)$/.test(t.num);
  const unsent = ts.filter((t) => t.num != null && !constant(t) && !measured(t.num, t.suffix, numbers)).map((t) => t.num + t.suffix);
  if (unsent.length) {
    return { ok: false, reason: "unmeasured", numbers: unsent,
      because: "these were never sent to you — measure them first, or use numbers a tool, the page, the reader or the summary gave" };
  }
  let i = 0;
  const peek = () => ts[i] || {};
  const expr = () => {
    let v = term();
    while (peek().op === "+" || peek().op === "-") v = ts[i++].op === "+" ? v + term() : v - term();
    return v;
  };
  const term = () => {
    let v = unary();
    while (peek().op === "*" || peek().op === "/") v = ts[i++].op === "*" ? v * unary() : v / unary();
    return v;
  };
  const unary = () => {
    if (peek().op === "-") { i++; return -unary(); }
    if (peek().op === "+") { i++; return unary(); }
    if (peek().op === "(") {
      i++;
      const v = expr();
      if (peek().op !== ")") throw new Error("a bracket is not closed");
      i++;
      return v;
    }
    const t = ts[i++];
    if (!t || t.num == null) throw new Error("a number is missing");
    return Number(t.num.replace(/,/g, "")) * (SCALE[t.suffix] || 1);
  };
  let value;
  try {
    value = expr();
    if (i < ts.length) throw new Error("something follows the end");
  } catch (e) { return { ok: false, reason: "bad_expression", because: e.message }; }
  if (!Number.isFinite(value)) return { ok: false, reason: "not_a_number", because: "division by zero" };
  const shown = Number(value.toPrecision(6));
  return { ok: true, expression: String(expression), value: shown, text: `${expression} = ${shown}` };
}
