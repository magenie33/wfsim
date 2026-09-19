// HOW BIG THINGS ARE, and how big each part of a request may be.
// docs/NONA.md §"The context budget" is the rule; these are its numbers, which
// are starting values tuned against `nona_eval`.

/// THE CAPS, in estimated tokens. S, T, M, K, H and P are the zones; `summary`,
/// `page`, `text`, `result` and `skill` bound the pieces inside them.
export const CAPS = {
  S: 1500, T: 2000, M: 800, K: 4000, H: 6000, P: 6000,
  summary: 1500, page: 500, text: 1500, result: 2000, skill: 2500,
};
/// A zone over its cap is brought down to this share of it, not just under the
/// line: each compaction breaks the provider's cache once, and the room left is
/// what makes the next one rare.
export const LOW = 0.6;
/// The window assumed when the address does not state one, the reply's
/// reserve, a margin for the estimate being wrong, and the cost budget B.
export const WINDOW = 64000;
export const OUTPUT = 4096;
export const MARGIN = 1500;
export const BUDGET = 24000;

/// TOKENS, ESTIMATED WITHOUT A TOKENIZER: a CJK character is about 0.6, any
/// other about 0.3 (DeepSeek's published rule of thumb), JSON a little more,
/// times the model's own correction (`calibrate`).
export function estimate(s, ratio = 1) {
  const str = String(s);
  const cjk = (str.match(/[⺀-鿿가-힯＀-￯]/g) || []).length;
  return Math.ceil((cjk * 0.6 + (str.length - cjk) * 0.3) * 1.1 * ratio);
}

/// The next ratio, from what a provider billed against what was guessed.
export function calibrate(prev, estimated, billed) {
  if (!estimated || !billed) return prev || 1;
  const r = billed / (estimated / (prev || 1));
  return prev ? prev * 0.7 + r * 0.3 : r;
}

/// TEXT CUT TO A SIZE, saying so and saying how much is missing — so a list cut
/// short reads as cut, and the tool's own limit can narrow it.
export function clip(text, tokens) {
  const s = String(text);
  if (estimate(s) <= tokens) return s;
  const tail = (n) => ` …[cut: ${n} more characters]`;
  let lo = 0, hi = s.length;
  while (lo < hi) {
    const mid = Math.ceil((lo + hi) / 2);
    if (estimate(s.slice(0, mid) + tail(s.length - mid)) <= tokens) lo = mid; else hi = mid - 1;
  }
  return s.slice(0, lo) + tail(s.length - lo);
}
