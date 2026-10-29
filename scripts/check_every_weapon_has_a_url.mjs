// EVERY WEAPON HAS AN ADDRESS, AND NO TWO SHARE ONE.
//
// Unique wiki names are paths; shared wiki names use each weapon's id.
// Primary and secondary Kitguns therefore have symmetric, independent paths.
//
// ASSERTED OVER THE WHOLE ROSTER rather than over Tombfinger, because the fix
// is a rule and the next Kitgun to land is the case it has to survive.
//
//   node scripts/check_every_weapon_has_a_url.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000, base: process.env.WFSIM_BASE });
const { evaluate, check } = app;

const out = await evaluate(`(async () => {
  const ws = META.weapons || [];
  const paths = ws.map((w) => ({ id: w.id, name: w.name, path: weaponPath(w.id) }));
  const seen = new Map();
  const clash = [];
  for (const p of paths) {
    if (seen.has(p.path)) clash.push([seen.get(p.path), p.id, p.path]);
    else seen.set(p.path, p.id);
  }
  // WHAT THE PATH RESOLVES BACK TO, by the router's own rule — a path that
  // is unique and lands on a DIFFERENT weapon is the same bug wearing a
  // different shape.
  const resolve = (path) => {
    // NO BACKSLASH IN THIS REGEX. The body is a template literal, so a
    // written-out escape reaches the page one level shorter and a class
    // meaning "whitespace or dash" quietly became "the letter s or a dash".
    const slug = decodeURIComponent(path.replace('/weapons/', ''))
      .trim().toLowerCase().split(/[ -]+/).join('_');
    const w = ws.find((x) => x.id === slug)
      || ws.filter((x) => wikiSlug(x).toLowerCase() === slug).sort((a, b) => a.id.localeCompare(b.id))[0];
    return w ? w.id : null;
  };
  const wrong = paths.filter((p) => resolve(p.path) !== p.id);
  // The shared-name case, named, so a roster that loses it says so.
  const shared = paths.filter((p) =>
    ws.filter((x) => wikiSlug(x) === wikiSlug(ws.find((y) => y.id === p.id))).length > 1);
  const opened = [], aliases = [];
  for (const p of shared) {
    history.pushState(null, '', p.path); await route();
    if ($('weapon').value !== p.id || location.pathname !== p.path) opened.push([p.id, location.pathname]);
    const w = ws.find(x => x.id === p.id), legacy = '/weapons/' + wikiSlug(w);
    const owner = ws.filter(x => wikiSlug(x) === wikiSlug(w)).map(x => x.id).sort()[0];
    if (owner !== p.id) continue;
    history.pushState(null, '', legacy); await route();
    if ($('weapon').value !== p.id || location.pathname !== p.path) aliases.push([p.id, location.pathname]);
  }
  return {
    total: paths.length,
    clash,
    wrong: wrong.map((p) => [p.id, p.path, resolve(p.path)]),
    shared: shared.map((p) => [p.id, p.path]),
    symmetric: shared.every(p => p.path === '/weapons/' + p.id),
    opened, aliases,
  };
})()`);

check("the roster loaded", out.total > 100, `${out.total} weapons`);
check(
  "no two weapons answer to the same URL",
  out.clash.length === 0,
  JSON.stringify(out.clash),
);
check(
  "every weapon's URL resolves back to that weapon",
  out.wrong.length === 0,
  JSON.stringify(out.wrong.slice(0, 6)),
);
// …AND THE ROSTER STILL EXERCISES THE RULE. Without this the three checks
// above pass on a roster where no name is shared at all, which is the state
// they were written to survive rather than the state they are asserting.
check(
  "some display name is still shared, so the rule above is being tested",
  out.shared.length >= 2,
  JSON.stringify(out.shared),
);
check("weapons sharing a wiki name each use their own id as the path", out.symmetric, JSON.stringify(out.shared));
check("each shared-name entry opens its own weapon page", out.opened.length === 0, JSON.stringify(out.opened));
check("old shared wiki-name links open their original weapon at its canonical path", out.aliases.length === 0, JSON.stringify(out.aliases));

await app.finish("every weapon has a URL of its own");
