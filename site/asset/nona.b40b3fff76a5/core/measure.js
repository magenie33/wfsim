// A NUMBER SHE DID NOT MEASURE IS MARKED, not hidden: "every number from a
// tool" is a promise, and this is where the reader can see it kept.

/// EVERY NUMBER SHE WAS SENT before message `upto`: the summary, the tool
/// results and pages still in view, and the reader's own words — repeating
/// what the reader said is not inventing it. A result or page set aside has
/// left the request, so its numbers no longer count. docs/NONA.md §"What
/// measured means".
export function numbersIn(record, upto = record.messages.length) {
  const texts = [];
  const from = record.summary && record.summary.upto <= upto ? record.summary.upto : 0;
  if (from) texts.push(record.summary.text);
  for (const m of record.messages.slice(from, upto)) {
    if (m.role === "tool" && !m.masked) texts.push(m.result || "");
    if (m.role === "user") texts.push(m.text || "", m.pageMasked ? "" : m.page || "");
  }
  return texts.flatMap((t) => (t.match(/\d[\d,]*(?:\.\d+)?/g) || []).map((x) => Number(x.replace(/,/g, ""))));
}

/// Is this figure one she was sent, to the precision it was written at — read
/// as written, as a percentage of a fraction, or scaled by k / 万 / M? Signs
/// are not compared: "−34.7% zoom" and "zoom −34.7%" put the minus in different
/// places. Small whole numbers (slot 3, 4 Forma) pass: counts and labels far
/// more often than measurements.
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
