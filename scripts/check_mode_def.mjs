// A MODE IS EXPLAINED, NOT JUST NAMED — and its name is DERIVED.
//
// The builder's Mode control was a dropdown of names and nothing else. That is
// enough while every weapon's second mode is the same mechanic, and it stopped
// being enough the day two weapons earned a form by KILLING rather than by
// hitting (owner, 2026-08-15): "cycle" does not say what fills the gauge, how
// many of them it takes, or what the earned form gets to fire — and those are
// exactly the numbers that decide whether the mode is worth picking. A Torid
// pays 5 direct hits for 170 rounds; a Mausolon pays 5 kills for ONE.
//
// The other half is the NAME. `modeLabel` returned a hardcoded "Incarnon
// cycle", which was right for sixty-nine weapons and wrong for the first one
// that earns a form with no adapter anywhere on it. So this check carries a
// matched pair, and neither half passes alone:
//
//   * the Mausolon must NOT be told it has an Incarnon anything;
//   * the Torid, which does, must still say so.
//
// A check that only asserted the first would pass on a page that had dropped
// the word entirely, which is a different bug with the same symptom.
//
// It runs the whole pass in BOTH languages, because the sentences are
// TEMPLATES with `{named}` holes: a hole filled into an untranslated string is
// invisible in English and is half an English sentence on a Chinese page.
//
//   node scripts/check_mode_def.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, send, sleep, BASE } = app;

const grab = (weapon) => `(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  history.pushState({}, '', '/weapons/${weapon}'); route(); await sleep(4000);
  const box = document.querySelector('.fold[data-fold="mode-def"]');
  return {
    present: !!box,
    // The block FOLDS and remembers, like every other block on the page.
    foldable: !!(box && box.querySelector('.fold-h') && box.querySelector('.fold-b')),
    names: [...document.querySelectorAll('.modedef-n')].map(x => x.textContent.trim()),
    lines: [...document.querySelectorAll('.modedef-l')].map(x => x.textContent.trim()),
    // Exactly one entry is marked as the one you are in.
    marked: document.querySelectorAll('.modedef.on').length,
    modes: (weaponInfo('${weapon}'.toLowerCase()) || {}).modes || [],
    picked: mode,
    // WHAT THE CONTROL OFFERS, which is where every mode of this weapon is
    // still named. The dropdown is a popover, so its labels are read from the
    // source renderMode builds them from rather than from a menu that is
    // shut — the same list, one step earlier. (No backticks in here: this whole
    // body is a template literal.)
    offered: modeOpts(weaponInfo('${weapon}'.toLowerCase()) || {}).map(o => o[1]),
    // The picked mode's own name, so the one entry drawn can be checked to BE
    // it rather than merely to exist.
    label: modeLabel(weaponInfo('${weapon}'.toLowerCase()) || {}, mode),
  };
})()`;

// FOUR WEAPONS, four different answers — a derivation is worth nothing if it
// derives the same sentence for everyone.
//   Mausolon  gauge fed by KILLS, no adapter, one round, no transition
//   Torid     gauge fed by DIRECT hits, an adapter, 170 rounds, two animations
//   Cortege   gauge fed by kills on a HELD beam, and its earned form is an
//             alt-fire rather than a charged shot
//   Lex       the ordinary case: weakpoint hits
//   Kuva Hind three FREE modes, whose ids are their FORMS' ids — the shape
//             that broke, see below
//   Magistar  seven of them, all melee
const WEAPONS = ["Mausolon", "Torid", "Cortege", "Lex", "Kuva_Hind", "Magistar"];

for (const lang of ["en", "zh"]) {
  await evaluate(
    `localStorage.clear(); localStorage.setItem('wfsim-lang', ${JSON.stringify(lang)})`);
  // LANG is read once at module evaluation, so the language only takes on a
  // real navigation — setting it and re-rendering leaves the old one up.
  await send("Page.navigate", { url: BASE });
  await sleep(12000);

  for (const w of WEAPONS) {
    const r = await evaluate(grab(w));
    check(`${w}/${lang}: the block is drawn`, r.present === true);
    check(`${w}/${lang}: ...and folds`, r.foldable === true);
    // **THE MODE YOU ARE IN, AND NOT THE OTHER SIX** (owner, 2026-08-29). This
    // asserted one entry PER MODE while the block listed them all, which was
    // right for a weapon with two or three and became a wall at seven. The
    // block draws the picked one now, so the count is one — and the assertion
    // that it is the RIGHT one is the half that carries the meaning, since
    // "exactly one entry" passes just as well on a block permanently showing
    // `base`.
    check(`${w}/${lang}: exactly one entry, and it is the one you are in`,
      r.names.length === 1 && r.marked === 1, `${r.names.length} entries, ${r.marked} marked`);
    check(`${w}/${lang}: ...and it names the picked mode`,
      r.names[0] === r.label, `${r.names[0]} vs ${r.label}`);
    check(`${w}/${lang}: it got a sentence`, r.lines.length >= 1,
      `${r.lines.length} lines`);

    // **NO TWO MODES OF ONE WEAPON SHARE A NAME**, which is the assertion this
    // check was missing and the reason a real bug shipped for two weeks.
    //
    // `modeLabel` had branches for `cycle`, `alternate` and `transformed` and
    // fell through to "the default form" for anything else — so a mode whose id
    // IS a form's id matched nothing and took the default form's name. The Kuva
    // Hind drew all three of its modes as "Base Form" from the day its third
    // trigger landed (2026-08-14), and melee made it seven identical entries
    // before anyone saw it (owner, 2026-08-29).
    //
    // EVERY ASSERTION ABOVE PASSED THE WHOLE TIME. One entry per mode, a
    // sentence each, one marked, the right language — all true of a list that
    // says the same word three times. What none of them asked is whether the
    // names TELL THE MODES APART, which is the only thing a name is for.
    //
    // IT MOVED TO THE CONTROL when the block stopped listing every mode
    // (2026-08-29): one entry cannot repeat a name, so asking the block would
    // have retired the assertion by making it vacuous. The dropdown is where
    // all seven names still appear, and it is the surface the bug was actually
    // about.
    const dupes = r.offered.filter((n, i) => r.offered.indexOf(n) !== i);
    check(`${w}/${lang}: the mode control's names tell the modes apart`,
      dupes.length === 0,
      `${dupes.length} repeated of ${r.offered.length}: ${r.offered.join(" | ")}`);
    check(`${w}/${lang}: ...one option per mode`,
      r.offered.length === r.modes.length || r.modes.length <= 1,
      `${r.offered.length} options for ${r.modes.length} modes`);

    // THE NUMBERS ARE ON SCREEN. A cycle whose text carries no digits is the
    // old dropdown with more words: this is the assertion that the gauge
    // economy actually reached the page.
    if (r.modes.includes("cycle")) {
      check(`${w}/${lang}: the gauge economy is stated`,
        /\d/.test(r.lines.join(" ")), r.lines.join(" ").slice(0, 80));
    }

    // THE MATCHED PAIR. Both directions, because either alone passes on a bug.
    const offered = r.offered.join(" | ");
    const saysIncarnon = /incarnon/i.test(offered) || offered.includes("灵化");
    if (w === "Mausolon" || w === "Cortege") {
      check(`${w}/${lang}: a weapon with no adapter is not told it has one`,
        !saysIncarnon, offered);
    }
    if (w === "Torid" || w === "Lex") {
      check(`${w}/${lang}: ...and a weapon that HAS one still says so`, saysIncarnon, offered);
    }

    // THE DISPLAY LANGUAGE, both ways. A template that was never translated
    // comes back English on a Chinese page; a zh string leaking into the
    // English one is the same bug mirrored.
    const cjk = /[一-鿿]/.test(r.lines.join(" "));
    check(`${w}/${lang}: the sentences are in the display language`,
      lang === "zh" ? cjk : !cjk, (r.lines[0] || "").slice(0, 60));
  }
}

await app.finish("what each mode is, on the page, in both languages");
