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
  };
})()`;

// FOUR WEAPONS, four different answers — a derivation is worth nothing if it
// derives the same sentence for everyone.
//   Mausolon  gauge fed by KILLS, no adapter, one round, no transition
//   Torid     gauge fed by DIRECT hits, an adapter, 170 rounds, two animations
//   Cortege   gauge fed by kills on a HELD beam, and its earned form is an
//             alt-fire rather than a charged shot
//   Lex       the ordinary case: weakpoint hits
const WEAPONS = ["Mausolon", "Torid", "Cortege", "Lex"];

for (const lang of ["en", "zh"]) {
  await evaluate(
    `localStorage.clear(); localStorage.setItem('wfsim-lang', ${JSON.stringify(lang)})`);
  // LANG is read once at module evaluation, so the language only takes on a
  // real navigation — setting it and re-rendering leaves the old one up.
  await send("Page.navigate", { url: BASE });
  await sleep(12000);

  for (const w of WEAPONS) {
    const r = await evaluate(grab(w));
    const names = r.names.join(" | ");
    check(`${w}/${lang}: the block is drawn`, r.present === true);
    check(`${w}/${lang}: ...and folds`, r.foldable === true);
    check(`${w}/${lang}: one entry per mode`, r.names.length === r.modes.length,
      `${r.names.length} entries for ${r.modes.length} modes`);
    check(`${w}/${lang}: every entry got a sentence`, r.lines.length >= r.names.length,
      `${r.lines.length} lines for ${r.names.length} entries`);
    check(`${w}/${lang}: the one you are in is marked, once`, r.marked === 1, `${r.marked}`);

    // THE NUMBERS ARE ON SCREEN. A cycle whose text carries no digits is the
    // old dropdown with more words: this is the assertion that the gauge
    // economy actually reached the page.
    if (r.modes.includes("cycle")) {
      check(`${w}/${lang}: the gauge economy is stated`,
        /\d/.test(r.lines.join(" ")), r.lines.join(" ").slice(0, 80));
    }

    // THE MATCHED PAIR. Both directions, because either alone passes on a bug.
    const saysIncarnon = /incarnon/i.test(names) || names.includes("灵化");
    if (w === "Mausolon" || w === "Cortege") {
      check(`${w}/${lang}: a weapon with no adapter is not told it has one`,
        !saysIncarnon, names);
    }
    if (w === "Torid" || w === "Lex") {
      check(`${w}/${lang}: ...and a weapon that HAS one still says so`, saysIncarnon, names);
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
