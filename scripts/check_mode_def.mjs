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
import { readFileSync } from "node:fs";

const toridTransition = JSON.parse(readFileSync(new URL("../tests/fixtures/client/primary-five-2026-10-28.json", import.meta.url), "utf8"))
  .forms.find(form => form.id === "torid_incarnon").transition;

const app = await openApp({ boot: 12000 });
const { evaluate, check, send, sleep, BASE } = app;

if (process.env.WFSIM_MODE_BOARD_ONLY) {
  await checkBoardModes();
  await app.finish("the overview preserves each winning build's mode");
}

async function cycleDetails(weapon) {
  return evaluate(`(async () => {
    history.pushState({}, '', '/weapons/${weapon}/builder'); route();
    await new Promise(r => setTimeout(r, 800));
    setMode('cycle');
    for (let i = 0; i < 60; i++) {
      if (typeof modeApl !== 'undefined' && modeApl && modeApl.key === JSON.stringify(buildPayload())) break;
      await new Promise(r => setTimeout(r, 50));
    }
    const box = document.querySelector('[data-fold="mode-def"]');
    const row = document.getElementById('mode-row');
    const control = row.querySelector('label').getBoundingClientRect();
    const rect = box.getBoundingClientRect();
    const text = box.textContent.trim();
    const source = await api('/api/panel', buildPayload());
    const result = {
      shut: box.classList.contains('shut'), text,
      apl: (box.querySelector('[data-apl="combat"] code') || {}).textContent || '',
      source: (source.apl || []).join('\\n'),
      prose: box.querySelectorAll('.modedef-l').length,
      below: rect.top >= control.bottom - 1,
      full: Math.abs(rect.width - row.clientWidth) < 2,
      valid: !/undefined|NaN|\\{[a-z_]+\\}/i.test(text),
    };
    box.querySelector('.fold-h').click();
    result.expanded = !box.classList.contains('shut')
      && getComputedStyle(box.querySelector('.fold-b')).display !== 'none';
    renderMode();
    result.remembered = !document.querySelector('[data-fold="mode-def"]').classList.contains('shut');
    document.querySelector('[data-fold="mode-def"] .fold-h').click();
    return result;
  })()`);
}

for (const lang of ["en", "zh"]) {
  await evaluate(`localStorage.clear(); localStorage.setItem('wfsim-lang', ${JSON.stringify(lang)})`);
  await send("Page.navigate", { url: BASE });
  await sleep(4000);
  for (const weapon of ["Trumna_Prime", "Torid", "Nunchasa", "Grimoire"]) {
    const r = await cycleDetails(weapon);
    check(`${weapon}/${lang}: conditions start collapsed`, r.shut, r.text);
    check(`${weapon}/${lang}: conditions sit below Mode at full width`, r.below && r.full);
    check(`${weapon}/${lang}: expand and remember across rendering`, r.expanded && r.remembered);
    check(`${weapon}/${lang}: no missing condition values`, r.valid, r.text);
    check(`${weapon}/${lang}: actual APL code and prose are both present`,
      r.apl.length > 0 && r.apl === r.source && r.prose > 0, r.apl || "no APL code");
    if (weapon === "Trumna_Prime") {
      check(`${weapon}/${lang}: input priorities are reload, earned alternate, primary`,
        r.apl === 'reload,if=!can_fire\nalt_fire\nshoot', r.apl);
      check(`${weapon}/${lang}: empty opening, earned shot, return and ammo are explicit`,
        lang === "en"
          ? /empty gauge/i.test(r.text) && /5.*kills/.test(r.text)
            && /once.*return/i.test(r.text) && /0 ammo/.test(r.text) && /no reload/i.test(r.text)
          : /空.*充能|充能.*空/.test(r.text) && /5.*击杀/.test(r.text)
            && /一次.*返回/.test(r.text) && /0.*弹药/.test(r.text) && /不.*换弹/.test(r.text), r.text);
    }
    if (weapon === "Torid") check(`${weapon}/${lang}: magazine exhaustion and transition times`,
      /170/.test(r.text) && r.text.includes(toridTransition.entry_seconds.toFixed(2))
        && r.text.includes(toridTransition.exit_seconds.toFixed(2))
        && (lang === "en" ? /ammunition.*spent/i.test(r.text) : /弹药.*耗尽/.test(r.text)), r.text);
    if (weapon === "Nunchasa") check(`${weapon}/${lang}: weighted contacts and alternate draw`,
      /0\.5/.test(r.text) && /1\.5/.test(r.text), r.text);
    if (weapon === "Grimoire") check(`${weapon}/${lang}: opening full and recharge sources`,
      /45/.test(r.text) && /10/.test(r.text)
        && (lang === "en" ? /full meter/i.test(r.text) && /pickup/i.test(r.text) : /充能.*满/.test(r.text) && /拾取/.test(r.text)), r.text);
  }
  await send("Page.navigate", { url: BASE + "/weapons/Trumna_Prime/builder" });
  await sleep(4000);
  check(`${lang}: collapsed preference survives reload`, await evaluate(
    `document.querySelector('[data-fold="mode-def"]').classList.contains('shut')`));
  await evaluate(`document.querySelector('[data-fold="mode-def"] .fold-h').click()`);
  await send("Page.navigate", { url: BASE + "/weapons/Trumna_Prime/builder" });
  await sleep(4000);
  check(`${lang}: explicit open preference survives the closed default`, await evaluate(
    `!document.querySelector('[data-fold="mode-def"]').classList.contains('shut')`));
  await send("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 1, mobile: true });
  check(`${lang}: expanded mobile conditions stay below Mode without overflow`, await evaluate(`(() => {
    const row = document.getElementById('mode-row');
    const box = row.querySelector('[data-fold="mode-def"]').getBoundingClientRect();
    return box.top >= row.querySelector('label').getBoundingClientRect().bottom - 1
      && Math.abs(box.width - row.clientWidth) < 2
      && document.documentElement.scrollWidth <= innerWidth;
  })()`));
  await send("Emulation.clearDeviceMetricsOverride");
  const switches = await evaluate(`(async () => {
    const wait = async () => {
      for (let i = 0; i < 80; i++) {
        if (modeApl && modeApl.key === JSON.stringify(buildPayload())) return;
        await new Promise(r => setTimeout(r, 50));
      }
    };
    const code = () => document.querySelector('[data-apl="combat"] code').textContent;
    setMode('base'); await wait();
    const baseKey = JSON.stringify(buildPayload());
    const base = code();
    setMode('cycle');
    renderModeApl({ apl: ['stale_response'] }, baseKey);
    const refused = !document.getElementById('mode-apl').textContent.includes('stale_response');
    await wait();
    return { base, cycle: code(), refused };
  })()`);
  check(`${lang}: switching Mode replaces the executable APL`,
    !switches.base.includes('alt_fire') && switches.cycle === 'reload,if=!can_fire\nalt_fire\nshoot', JSON.stringify(switches));
  check(`${lang}: a stale build response cannot overwrite the APL`, switches.refused);
}

if (process.env.WFSIM_MODE_DETAILS_ONLY) {
  await app.finish("mode cycle conditions and folds in both languages");
}

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
//   Ocucor    ONE mode, which is most of the roster and the case every
//             "only where there is a choice" short-circuit was written for
const WEAPONS = [
  "Mausolon", "Torid", "Cortege", "Lex", "Kuva_Hind", "Magistar", "Ballistica_Prime",
  "Ocucor", "Nunchasa", "Higasa", "Battacor", "Trumna", "Trumna_Prime", "Morgha", "Sepulcrum",
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

    if (["Nunchasa", "Higasa", "Mausolon", "Cortege", "Morgha", "Battacor", "Trumna", "Trumna_Prime", "Sepulcrum"].includes(w)) {
      check(`${w}/${lang}: primary only or full-charge alternate cycle`,
        JSON.stringify(r.modes) === JSON.stringify(["base", "cycle"]), JSON.stringify(r.modes));
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

// ---------------------------------------------------------------------------
// A MODE IS NAMED WHEREVER A BUILD IS, INCLUDING ON A WEAPON THAT HAS ONE.
//
// Four surfaces drew it only where the weapon offered a second, so a blank
// beside a neighbour that names its mode read as "no mode", not "one mode".
//
// ASSERTED ON A WEAPON WITH ONE MODE AND ONE WITH FOUR, because either alone
// passes on the bug.
const surfaces = (weapon) => `(async () => {
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  history.pushState({}, '', '/weapons/${weapon}'); route(); await sleep(4000);
  const box = document.getElementById('build-finder');
  const seg = box && box.querySelector('[data-fseg="mo"]');
  return {
    modes: (weaponInfo('${weapon}'.toLowerCase()) || {}).modes || [],
    // THE FINDER NAMES IT — its mode segment is drawn with one button as
    // readily as with four.
    barHidden: !box || box.hidden || !!box.querySelector('.fd-empty:only-child, .fd-head + .fd-empty'),
    named: !!seg,
    namedText: ((seg || {}).textContent || '').trim(),
    // …AND SO DOES THE ROW'S OWN NAME, which travels to the simulator tab.
    rowNames: builtinBuilds().slice(0, 6).map(p => p.name),
  };
})()`;

for (const w of ["Ocucor", "Ballistica_Prime"]) {
  const r = await evaluate(surfaces(w));
  if (r.barHidden) {
    // Nobody has submitted a build for this weapon, which is ordinary — there
    // is no bar to assert about, and saying so beats a green tick that means
    // the check found nothing.
    check(`${w}: the build finder is empty (no submissions)`, true, "skipped");
  } else {
    check(`${w}: the build finder names the mode (${r.modes.length} mode(s))`,
      r.named === true, r.namedText || "nothing drawn");
    check(`${w}: ...and every row name carries it`,
      r.rowNames.length > 0 && r.rowNames.every((n) => / · /.test(n)),
      r.rowNames.join(" | "));
  }
}

// The overview names each weapon once; its link retains the winning build's
// ruler, mode and riven group. WFSIM_BREAK=mode|link corrupts a real row.
async function checkBoardModes() {
  for (const lang of ["en", "zh"]) {
    await evaluate(`localStorage.setItem('wfsim-lang', ${JSON.stringify(lang)})`);
    await send("Page.navigate", { url: BASE + "/benchmark" });
    await sleep(4500);
    const board = await evaluate(`(async () => {
      const cur = benchCurrent();
      await benchIndexOf(cur).load();
      renderBenchBoard();
      const entries = new Map(benchEntries(cur, benchPool()).map(e =>
        ['/weapons/' + urlSlug(e.w), e]));
      const rows = [...document.querySelectorAll('.bench-rows .brow')];
      const broken = ${JSON.stringify(process.env.WFSIM_BREAK || "")};
      if (rows.length && ['mode', 'link'].includes(broken)) {
        const target = rows.find(row => {
          const entry = entries.get(new URL(row.href).pathname);
          return entry?.w.modes.some(mode => mode !== entry.mode);
        }) || rows[0];
        const url = new URL(target.href), entry = entries.get(url.pathname);
        if (broken === 'mode') url.searchParams.set('mode',
          entry.w.modes.find(mode => mode !== entry.mode) || 'invalid_mode');
        else url.searchParams.delete('mode');
        target.setAttribute('href', url.pathname + url.search);
      }
      const paths = [], badLinks = [], badNames = [];
      for (const row of rows) {
        const url = new URL(row.href), entry = entries.get(url.pathname);
        paths.push(url.pathname);
        const name = row.querySelector('.bname')?.cloneNode(true);
        name?.querySelectorAll('.bgap').forEach(e => e.remove());
        if (!entry || !name || name.textContent.trim() !== weaponEntryName(entry.w)) {
          badNames.push(url.pathname);
        }
        if (!entry || url.searchParams.get('mode') !== entry.mode
            || url.searchParams.get('bench') !== cur.id
            || url.searchParams.get('riven') !== (entry.row ? (entry.row.riven ? '1' : '0') : null)) {
          badLinks.push(row.getAttribute('href'));
        }
      }
      return { total: rows.length, unique: new Set(paths).size,
        badges: rows.filter(row => row.querySelector('.bmode')).length,
        measured: rows.filter(row => !row.classList.contains('none')).length,
        badLinks, badNames };
    })()`);
    check(`${lang}: the overview lists measured weapons`, board.total > 50 && board.measured > 0,
      `${board.measured} measured of ${board.total} rows`);
    check(`${lang}: each visible weapon appears once`, board.unique === board.total,
      `${board.unique} weapons in ${board.total} rows`);
    check(`${lang}: weapon labels have no mode badges`, board.badges === 0 && board.badNames.length === 0,
      `${board.badges} badges; labels: ${board.badNames.join(', ')}`);
    check(`${lang}: winning links retain their ruler, mode and riven group`, board.badLinks.length === 0,
      board.badLinks.slice(0, 3).join(' | '));
  }
}

await checkBoardModes();

await app.finish("what each mode is, on the page, in both languages");
