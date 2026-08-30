// A MODE IS EXPLAINED, NOT JUST NAMED — and its name is DERIVED.
//
// A dropdown of names is enough while every weapon's second mode is the same
// mechanic, and stopped being enough the day two weapons earned a form by
// KILLING rather than by hitting: "cycle" does not say what fills the gauge,
// how many it takes, or what the earned form fires — the numbers that decide
// whether the mode is worth picking. A Torid pays 5 direct hits for 170 rounds;
// a Mausolon pays 5 kills for ONE.
//
// The other half is the NAME, and this carries a MATCHED PAIR because neither
// half passes alone: the Mausolon must NOT be told it has an Incarnon anything,
// and the Torid, which does, must still say so. A check asserting only the
// first passes on a page that dropped the word entirely.
//
// It runs the whole pass in BOTH languages, because the sentences are TEMPLATES
// with `{named}` holes — a hole filled into an untranslated string is invisible
// in English and half an English sentence on a Chinese page.
//
//   node scripts/check_mode_def.mjs
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
//   Ballistica Prime  TWO CYCLES — a weapon whose gauge can be filled by
//             either of two shots has two of everything, and both pairs
//             share a form name
const WEAPONS = [
  "Mausolon", "Torid", "Cortege", "Lex", "Kuva_Hind", "Magistar", "Ballistica_Prime",
];

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
    // **THE MODE YOU ARE IN, AND NOT THE OTHER SIX**. This
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

    // **NO TWO MODES OF ONE WEAPON SHARE A NAME.** Every assertion above
    // passes on a list that says one word three times: one entry per mode, a
    // sentence each, one marked, the right language. Whether the names TELL
    // THE MODES APART is the only thing a name is for, and it is asked of the
    // CONTROL, because the block draws the picked mode alone — one entry
    // cannot repeat a name, and the dropdown is where all of them appear.
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
