// A NUMBER SHE DID NOT MEASURE IS MARKED, not hidden: "every number from a
// tool" is a promise, and this is where the reader can see it kept.

/// Every number the tool results before `upto` hold, as plain values.
export function numbersIn(messages, upto = messages.length) {
  const out = [];
  for (const m of messages.slice(0, upto)) {
    if (m.role !== "tool" || !m.result) continue;
    for (const x of m.result.match(/-?\d+(?:\.\d+)?/g) || []) out.push(Number(x));
  }
  return out;
}

/// Is this figure one some tool result holds, to the precision it was written
/// at — read as written, as a percentage of a fraction, or scaled by k / 万 / M?
/// Small whole numbers (slot 3, 4 Forma) pass: counts and labels far more often
/// than measurements.
export function measured(raw, suffix, numbers) {
  const v = Number(raw.replace(/,/g, ""));
  if (!Number.isFinite(v)) return true;
  const decimals = (raw.split(".")[1] || "").length;
  if (!decimals && Math.abs(v) < 100 && suffix !== "%") return true;
  const scale = { k: 1e3, K: 1e3, "万": 1e4, M: 1e6 }[suffix] || 1;
  const want = [v * scale];
  if (suffix === "%") want.push(v / 100);
  const tol = (x) => Math.max(0.5 * 10 ** -decimals * (x === v / 100 ? 0.01 : scale), Math.abs(x) * 0.005);
  return numbers.some((t) => want.some((x) => Math.abs(t - x) <= tol(x)));
}

/// Mark the unmeasured numbers in already-escaped html. `wrap` draws the mark.
export function markNumbers(html, numbers, wrap) {
  if (!numbers) return html;
  return html.replace(/(\d[\d,]*(?:\.\d+)?)(%|k|K|万|M)?/g, (all, raw, suffix) =>
    (measured(raw, suffix, numbers) ? all : wrap(all)));
}
