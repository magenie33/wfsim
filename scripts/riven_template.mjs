// SPDX-License-Identifier: AGPL-3.0-or-later
// ONE WEAPON'S FROZEN SEARCH, WITH ANOTHER CARD IN IT — docs/AGENT.md §"Riven
// survey". A survey's shapes of one weapon freeze to one search that differs
// only in the riven: its id, its spec, and the board context naming it. So one
// clean page per weapon freezes a TEMPLATE and every other card is written
// into it here, as the page would write it (20-rivens.js `withDrafts`,
// 68-board-submit.js `boardRivenShape`). `riven_freeze.mjs` checks a share of
// them against clean pages and stops on the first that differs.

/// A riven id as the page makes one (18-riven-mods.js `newRivenId`).
export const newRivenId = () => "r" + Date.now().toString(36) + Math.random().toString(36).slice(2, 5);

const shapeOf = (card) => `${card.bonuses.length}${card.malus ? "+1" : ""}`;
/// An empty draft of a shape: its bonuses and malus with no stat chosen.
const blank = (shape) => {
  const [n, m] = shape.split("+");
  return { bonuses: Array.from({ length: Number(n) }, () => ({ id: null, roll: 1 })), malus: m ? { id: null, roll: 1 } : null };
};
const stat = (s) => ({ id: s.id, roll: s.roll });

/// `frozen` ({ engine, request, context }) with `card` in place of its riven,
/// under the id `id`. The template is never changed.
export function withCard(frozen, card, id = newRivenId()) {
  const [rv] = frozen.request.rivens;
  if (frozen.request.rivens.length !== 1) throw new Error("a template holds exactly one riven");
  const old = "riven:" + rv.id, now = "riven:" + id;
  const out = JSON.parse(JSON.stringify(frozen).split(`"${old}"`).join(`"${now}"`));
  const r = out.request.rivens[0];
  const bonuses = card.bonuses.map(stat), malus = card.malus ? stat(card.malus) : null, shape = shapeOf(card);
  r.id = id;
  if (!Number.isInteger(card.rank)) throw new Error("a card states its rank");
  Object.assign(r.spec, { bonuses, malus, rank: card.rank, polarity: card.polarity || "madurai", shape });
  for (const k of Object.keys(r.spec.drafts)) {
    r.spec.drafts[k] = k === shape ? { bonuses: bonuses.map(stat), malus: malus && stat(malus) } : blank(k);
  }
  if (!(shape in r.spec.drafts)) throw new Error(`the template has no ${shape} draft`);
  // SORTED, as the board context states a riven (boardRivenShape).
  out.context.rivens = { [now]: { riven_pos: bonuses.map((b) => b.id).sort(), riven_neg: malus ? malus.id : "" } };
  return out;
}

/// Two frozen searches are the same search when they agree with their riven ids
/// set aside: an id is the page's own and names nothing.
export function sameSearch(a, b) {
  const plain = (f) => {
    const id = f.request.rivens.map((r) => r.id);
    return id.reduce((s, x) => s.split(x).join("RIVEN"), JSON.stringify(f));
  };
  return plain(a) === plain(b);
}
