// THE CONTEXT BUDGET: every zone of a request under its cap, and the whole
// under W, however long the conversation — docs/NONA.md §"The context budget".
// `plan` measures the zones and says what to do; the agent does it and asks
// again. Nothing here removes a message: setting aside is a mark, summarising
// moves where the request starts, and the record stays whole.

import { CAPS, LOW, WINDOW, OUTPUT, MARGIN, BUDGET, estimate } from "./size.js";
import { sizeOf, summaryText } from "./view.js";

export { calibrate, estimate } from "./size.js";

/// W: the input one request may hold — the window less the reply's reserve and
/// a margin, and never more than the cost budget.
export const room = ({ context, budget } = {}) =>
  Math.min((context || WINDOW) - OUTPUT - MARGIN, budget || BUDGET);

/// Where this turn starts: the reader's newest message.
function turnStart(record) {
  for (let i = record.messages.length - 1; i >= 0; i--) if (record.messages[i].role === "user") return i;
  return record.messages.length;
}

/// THE ZONES AS THEY STAND. `fixed` is {rules, memory, tools, skills}, the
/// texts of S, M, T and K. H is the summary and every message from it to this
/// turn; P is this turn.
export function measure(record, fixed) {
  const from = record.summary ? record.summary.upto : 0;
  const start = Math.max(from, turnStart(record));
  // ONLY WHAT IS SENT IS MEASURED: a message before the summary costs nothing,
  // and measuring it would make every request slower the longer the record.
  const sizes = record.messages.map((m, i) => (i < from ? 0 : sizeOf(m)));
  const add = (a, b) => sizes.slice(a, b).reduce((x, y) => x + y, 0);
  return {
    S: estimate(fixed.rules || ""), T: estimate(JSON.stringify(fixed.tools || [])),
    M: estimate(fixed.memory || ""), K: estimate(fixed.skills || ""),
    H: (record.summary ? estimate(summaryText(record.summary)) : 0) + add(from, start), P: add(start, sizes.length),
    from, start, sizes,
  };
}

/// THE CAPS FOR THIS REQUEST. S, T, M and K are what they are; H and P share
/// what is left of W, each up to its own cap, this turn first. A model whose W
/// leaves no working room is refused rather than run badly.
export function caps(z, W) {
  const avail = W - z.S - z.T - z.M - z.K;
  const P = Math.min(CAPS.P, Math.floor(avail / 2));
  const H = Math.min(CAPS.H, avail - P);
  return { W, P, H, refuse: P < 1500 || H < CAPS.summary + 500 };
}

/// The size of message `i` once set aside as `kind`.
const asideSize = (m, kind) => sizeOf({ ...m, ...(kind === "tools" ? { masked: true } : { pageMasked: true }) });

/// WHAT TO DO BEFORE THIS REQUEST, cheapest first:
/// - `marks`: results and pages to set aside (this turn's older results beyond
///   the newest two; then earlier turns' results, then their pages);
/// - `summarize`: the index to summarise up to, when setting aside is not
///   enough to bring H down to its low water mark;
/// - `stop`: this turn alone is over its cap with nothing left to set aside —
///   the turn ends, and the next one can summarise it.
/// A zone under its cap is left alone, so the prefix does not move.
export function plan(record, fixed, { context, budget, ratio = 1 } = {}) {
  const z = measure(record, fixed);
  const c = caps(z, Math.floor(room({ context, budget }) / (ratio || 1)));
  const out = { zones: z, caps: c, marks: { tools: [], pages: [] }, summarize: null, stop: false, refuse: c.refuse };
  if (c.refuse) return out;
  const msgs = record.messages;
  const aside = new Map();
  const setAside = (i, kind) => {
    out.marks[kind].push(i);
    aside.set(i, kind);
    return z.sizes[i] - asideSize(msgs[i], kind);
  };

  // P: this turn's older results, keeping the newest two — or the newest one,
  // when two do not fit under the cap.
  let P = z.P;
  if (P > c.P) {
    const results = [];
    for (let i = z.start; i < msgs.length; i++) if (msgs[i].role === "tool" && !msgs[i].masked) results.push(i);
    for (const i of results.slice(0, Math.max(0, results.length - 2))) {
      if (P <= c.P * LOW) break;
      P -= setAside(i, "tools");
    }
    if (P > c.P && results.length >= 2) P -= setAside(results[results.length - 2], "tools");
    if (P > c.P) out.stop = true;
  }

  // H: earlier results, then earlier pages, then a summary.
  let H = z.H;
  if (H > c.H) {
    const at = (pred) => { const xs = []; for (let i = z.from; i < z.start; i++) if (pred(msgs[i])) xs.push(i); return xs; };
    for (const i of at((m) => m.role === "tool" && !m.masked)) { if (H <= c.H * LOW) break; H -= setAside(i, "tools"); }
    for (const i of at((m) => m.role === "user" && m.page && !m.pageMasked)) { if (H <= c.H * LOW) break; H -= setAside(i, "pages"); }
    if (H > c.H * LOW) {
      // THE LEAST THAT IS ENOUGH: the earliest reader turn from which what is
      // left, plus a full summary, fits the low water mark — or all of it.
      const tail = (u) => {
        let s = CAPS.summary;
        for (let i = u; i < z.start; i++) s += aside.has(i) ? asideSize(msgs[i], aside.get(i)) : z.sizes[i];
        return s;
      };
      let cut = z.start;
      for (let u = z.from + 1; u < z.start; u++) {
        if (msgs[u].role === "user" && tail(u) <= c.H * LOW) { cut = u; break; }
      }
      if (cut > z.from) out.summarize = cut;
    }
  }
  return out;
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
