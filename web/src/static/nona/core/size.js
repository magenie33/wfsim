// HOW BIG EACH PIECE OF A REQUEST MAY BE, in characters. Nothing here guesses
// tokens: the provider states what a request cost, and docs/NONA.md §"The
// context budget" compacts on that.

/// THE CAPS. S, T, M and K bound her rules, the tool table, the reader's memory
/// and the skill documents in view; `summary`, `page`, `text`, `result` and
/// `skill` bound one piece each.
export const CAPS = {
  S: 4500, T: 6000, M: 1500, K: 15000,
  summary: 2000, page: 1500, text: 4500, result: 6000, skill: 7500,
};
/// COMPACT PAST THIS SHARE OF THE WINDOW, measured on the last reply: the next
/// request adds at most one message or result, and the room left must hold it.
export const HIGH = 0.7;

/// TEXT CUT TO A SIZE, saying so and saying how much is missing — so a list cut
/// short reads as cut, and the tool's own limit can narrow it.
export function clip(text, chars) {
  const s = String(text);
  if (s.length <= chars) return s;
  const tail = (n) => ` …[cut: ${n} more characters]`;
  let keep = chars - tail(s.length).length;
  keep = Math.max(0, keep);
  return s.slice(0, keep) + tail(s.length - keep);
}
