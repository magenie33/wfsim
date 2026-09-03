// EVERY WEAPON HAS AN ADDRESS, AND NO TWO SHARE ONE.
//
// URLs mirror wiki page names, which is right until two roster entries mirror
// the SAME page: a Kitgun is one wiki page and two entries (one per slot), so
// the rule maps both onto `/weapons/Tombfinger` and the loser has no URL at
// all — nothing links to it, nothing prerenders it, and a player who wants the
// secondary can only reach it by switching slots from the primary's page.
// `urlSlug` hands the loser its id; this is what says it worked.
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

const out = await evaluate(`(() => {
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
      || ws.find((x) => wikiSlug(x).toLowerCase() === slug);
    return w ? w.id : null;
  };
  const wrong = paths.filter((p) => resolve(p.path) !== p.id);
  // The shared-name case, named, so a roster that loses it says so.
  const shared = paths.filter((p) =>
    ws.filter((x) => wikiSlug(x) === wikiSlug(ws.find((y) => y.id === p.id))).length > 1);
  return {
    total: paths.length,
    clash,
    wrong: wrong.map((p) => [p.id, p.path, resolve(p.path)]),
    shared: shared.map((p) => [p.id, p.path]),
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

await app.finish("every weapon has a URL of its own");
