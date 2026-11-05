// Known portions of collected mods affect the real panel and remain distinct
// from their explicitly unsupported clauses.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000, lang: "en" });
const r = await app.evaluate(`(async () => {
  const card = (id) => Object.values(META.mod_pools).flat().find((m) => m.id === id);
  const partial = card('amalgam_furax_body_count');
  const unknown = card('burning_hate');
  const adaptive = card('sentient_incision');
  const lex = (mods) => api('/api/panel', { weapon: 'lex', mods, evolutions: [] });
  const bare = await lex([]), ranked = await lex(['no_return@0']);
  const damage = (p, type) => Number(p.forms?.[0]?.parts?.[0]?.damage?.find((d) => d.type === type)?.amount);
  await loadWarframeCatalog();
  const base = await api('/api/warframe/panel', { frame: 'zephyr', mods: [] });
  const added = await api('/api/warframe/panel', { frame: 'zephyr', mods: [],
    exilus: { id: 'anchored_glide', rank: 0 } });
  return {
    partial: { full: partial.not_modeled, clauses: partial.unmodeled_effects,
      label: notModeledLines(partial).join('') },
    unknown: unknown.not_modeled,
    adaptive: { full: adaptive.not_modeled, clauses: adaptive.unmodeled_effects,
      ranked: adaptive.lower_ranks_unmodelled === false },
    supportedRanks: card('no_return').lower_ranks_unmodelled === false,
    maxOnly: card('blade_of_truth').lower_ranks_unmodelled === true,
    lex: { bare: damage(bare, 'Puncture'), ranked: damage(ranked, 'Puncture') },
    frame: { base: base.stats.find((s) => s.id === 'ability_strength').value,
      added: added.stats.find((s) => s.id === 'ability_strength').value },
  };
})()`);
console.log(JSON.stringify(r, null, 1));
app.check("a partial weapon card names its missing clauses without hiding its working effects",
  !r.partial.full && r.partial.clauses.length === 2 && r.partial.label.includes("partly modelled"));
app.check("Burning Hate retains its unsupported target-vulnerability admission", r.unknown);
app.check("Sentient Incision supports its rank ladder and names only its remaining gaps",
  !r.adaptive.full && r.adaptive.ranked && r.adaptive.clauses.length === 2);
app.check("supported and max-only rank ladders are distinguished", r.supportedRanks && r.maxOnly);
app.check("rank-zero No Return changes Puncture through the real weapon panel",
  r.lex.ranked > r.lex.bare * 1.14 && r.lex.ranked < r.lex.bare * 1.16);
app.check("Anchored Glide rank zero adds 2.5 percentage points of Ability Strength",
  Math.abs(r.frame.added - r.frame.base - 0.025) < 1e-10);
await app.finish("collected mods compute their supported effects");
