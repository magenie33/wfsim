// SPDX-License-Identifier: AGPL-3.0-or-later
export function rivenRequestCases(meta, everyRulerOn, shard, shards, cards) {
  if (![shard, shards, cards].every(Number.isInteger) || shards < 1 || shard < 0 || shard >= shards || cards < 1) {
    throw new Error('riven request shard and card count must be valid integers');
  }
  if (!meta.weapons?.length || !meta.benchmarks?.length) throw new Error('riven request catalog is empty');
  const primary = (meta.benchmarks.find(b => b.primary) || meta.benchmarks[0]).id;
  const cases = [];
  meta.weapons.forEach((weapon, i) => {
    if (i % shards === shard) {
      const index = Math.floor(i / shards);
      cases.push({ weapon: weapon.id, ruler: primary, card: index % cards, index });
    }
  });
  const base = Math.ceil(meta.weapons.length / shards);
  let extra = 0;
  for (const weapon of everyRulerOn) for (const ruler of meta.benchmarks) if (ruler.id !== primary) {
    const index = base + extra;
    if (extra % shards === shard) cases.push({ weapon, ruler: ruler.id, card: index % cards, index });
    extra++;
  }
  return cases;
}
