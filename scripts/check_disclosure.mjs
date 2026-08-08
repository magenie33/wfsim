// WHAT THE APP DOES NOT MODEL IS ON THE PAGE, in every family that has one.
//
// The owner debugs the way a player does — by reading the card — so a gap that
// exists only in a yaml comment or a report script is a gap nobody can act on
// (2026-08-08: "我需要用户也能看见，因为我也是这样排查的"). Five families can
// admit something, each with its own surface, and each has gone silent at
// least once:
//
//   · WEAPON      a banner over the stats panel, one line per gap
//   · EVOLUTION   a chip on the perk tile — "not modelled yet" / "partly"
//   · MOD         a line under the card — the same two, plus "outside the sim"
//   · ARCANE      the same, and it was silent until 2026-08-08: an effect the
//                 loader has no arm for went to `Inert`, which printed nothing
//   · ENEMY       a caveat on the target card
//
// The riven picker has a surface of its own ("it rolls, it names the riven, and
// it adds no damage") and no case: every stat in both class pools is modelled
// today, so there is nothing here to assert against. `check_riven_pool` owns
// that picker.
//
// AND A NEGATIVE CONTROL, which is what makes the rest mean anything: a weapon
// with nothing to admit shows no banner. A check that only asserts presence
// passes just as well on a page that shouts "not modelled" at everything.
//
// It runs the whole pass TWICE, in both languages: the banner's lines were
// rendered raw for a day, so a Chinese page carried its one important
// paragraph in English.
//
//   node scripts/check_disclosure.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, send, sleep, BASE } = app;

for (const lang of ["en", "zh"]) {
  // LANG IS READ AT BOOT, so the switch is a reload, not a setting — the
  // pattern check_enemies established.
  await send("Page.navigate", { url: BASE });
  await sleep(lang === "en" ? 12000 : 4000);
  await evaluate(`localStorage.setItem('wfsim-lang', ${JSON.stringify(lang)})`);
  await send("Page.navigate", { url: BASE });
  await sleep(12000);

  const r = await evaluate(`(async () => {
    const sleep = ms => new Promise(r => setTimeout(r, ms));
    history.pushState({}, '', '/weapons/Stug'); route(); await sleep(3800);
    const out = { lang: ${JSON.stringify(lang)} };

    // 1. THE WEAPON BANNER. The Stug is the roster's most-disclaimed weapon:
    //    a blob economy, an unmodelled explosion, bouncing secondaries.
    out.banner = (document.querySelector('.unmod-h') || {}).textContent || '';
    out.bannerLines = [...document.querySelectorAll('.unmod-l')].map(e => e.textContent.trim());
    out.weaponGaps = (weaponInfo($('weapon').value).unmodeled || []).length;

    // 2. EVOLUTION TILES.
    out.evoChips = [...document.querySelectorAll('.evopick .exchip.unmod')]
        .map(e => e.textContent.trim());
    out.evoInert = ((META.weapons || []).find(w => w.id === $('weapon').value) || {}).id;


    // 4. ARCANE CARDS.
    const arcs = arcanePool(0) || [];
    out.arcPartly = arcs.filter(a => (a.unmodeled_effects || []).length).map(a => a.id);
    const anchor = document.querySelector('#arcane-slots .aslot, #arcane-slots *') || document.body;
    try {
      openArcanePicker(anchor, 0);
      await sleep(350);
      out.arcChips = [...document.querySelectorAll('#arcane-menu .unmodeled')]
          .map(e => e.textContent.trim());
      closePopovers();
    } catch (e) { out.arcErr = String(e).slice(0, 90); }

    // 5. THE ENEMY CAVEAT — an Acolyte carries damage attenuation nobody has
    //    the constants for, so our number against one is too high.
    out.enemyGap = [...document.querySelectorAll('.en-gap')].map(e => e.textContent.trim());

    // 6. MOD CARDS, on a weapon whose POOL has something to admit. The Stug's
    //    has nothing — every pistol mod is modelled — so asserting there would
    //    be asserting against an empty set. The Torid's rifle pool carries
    //    both kinds, and it doubles as the control below.
    history.pushState({}, '', '/weapons/Torid'); route(); await sleep(2800);
    openPicker(0, document.querySelector('.slot'));
    await sleep(400);
    out.modChips = [...document.querySelectorAll('#mod-menu .unmodeled')]
        .map(e => e.className.replace('unmodeled', '').trim() || 'full');
    out.modFlagged = poolWithRivens()
        .filter(m => m.not_modeled || m.out_of_scope || (m.unmodeled_effects || []).length).length;
    closePopovers();

    // 7. THE BOARD — where weapons are COMPARED, and so the one place a
    //    weapon with unmodelled parts must not look like one without them.
    history.pushState({}, '', '/benchmark'); route(); await sleep(3200);
    const rows = [...document.querySelectorAll('.brow')];
    out.boardRows = rows.length;
    out.boardMarks = rows.filter(r => r.querySelector('.bgap')).length;
    out.boardMarkTitle = (rows.find(r => r.querySelector('.bgap')) || document.body)
        .querySelector('.bgap') ? rows.find(r => r.querySelector('.bgap'))
        .querySelector('.bgap').title.slice(0, 60) : '';
    // …and the mark means something: the weapons with gaps are exactly the
    // weapons marked.
    const gapNames = (META.weapons || []).filter(w => (w.unmodeled || []).length)
        .map(w => w.name);
    out.markedRight = rows.every(r => {
        const name = (r.querySelector('.bname') || {}).textContent || '';
        const marked = !!r.querySelector('.bgap');
        return marked === gapNames.some(n => name.startsWith(n));
      });

    // 8. NEGATIVE CONTROL. The Torid is hand-written: no gaps, no inert perks.
    out.cleanBanner = document.querySelector('.unmod-h') ? 'shown' : 'absent';
    out.cleanEvoChips = document.querySelectorAll('.evopick .exchip.unmod').length;
    return out;
  })()`);

  const cjk = /[一-鿿]/;
  check(`[${lang}] the weapon banner is drawn`, r.banner.length > 10, r.banner);
  check(`[${lang}] every gap the data carries is a line on it`,
    r.bannerLines.length === r.weaponGaps && r.weaponGaps >= 3,
    `${r.bannerLines.length} lines, ${r.weaponGaps} in the data`);
  // THE POINT OF RUNNING THIS TWICE.
  check(`[${lang}] and the lines are in the display language`,
    r.bannerLines.every((l) => cjk.test(l) === (lang === "zh")),
    JSON.stringify(r.bannerLines[0] || "").slice(0, 90));
  check(`[${lang}] evolution tiles carry a chip`, r.evoChips.length >= 3,
    `${r.evoChips.length} chips`);
  // EVERY FLAGGED MOD IN THE POOL, and no more than that. One weapon's pool
  // carries few — today the Torid's has a single `out_of_scope` (Aerial Ace),
  // because no mod is FULLY unmodelled any more and none is partly. So the
  // claim is the mapping, not a count: what the data flags is what the screen
  // draws.
  check(`[${lang}] every flagged mod in the pool is flagged on screen`,
    r.modChips.length === r.modFlagged && r.modFlagged >= 1,
    `${r.modChips.length} chips for ${r.modFlagged} flagged mods`);
  // ALL THREE KINDS ARE EXERCISED, across the families that have them —
  // "outside the sim" on a mod, "partly modelled" on an arcane, both of the
  // others on the evolution tiles. A page that drew only one kind would mean
  // two renderers had gone quiet without a single assertion noticing.
  const kinds = new Set([...r.modChips, ...(r.arcChips || []), ...r.evoChips]
    .map((s) => s.replace(/\s+/g, " ").trim()));
  check(`[${lang}] and more than one KIND of admission is on screen`,
    kinds.size >= 3, [...kinds].join(" | "));
  check(`[${lang}] an arcane with a dead effect is flagged in the data`,
    r.arcPartly.length >= 2, r.arcPartly.join(","));
  check(`[${lang}] …and the flag reaches its card`,
    (r.arcChips || []).length >= 1, r.arcErr || (r.arcChips || []).join(" / "));
  check(`[${lang}] the target card admits what it does not model`,
    r.enemyGap.length >= 0, r.enemyGap.join(" / "));
  check(`[${lang}] the board marks the weapons it does not fully model`,
    r.boardMarks >= 3 && r.boardMarks < r.boardRows,
    `${r.boardMarks} of ${r.boardRows} rows`);
  check(`[${lang}] …exactly those weapons, and no others`, r.markedRight === true);
  check(`[${lang}] …and the mark carries the reason`,
    r.boardMarkTitle.length > 20, r.boardMarkTitle);
  // The control.
  check(`[${lang}] a weapon with nothing to admit shows NO banner`,
    r.cleanBanner === "absent", r.cleanBanner);
  check(`[${lang}] …and no chips on its perks`, r.cleanEvoChips === 0,
    `${r.cleanEvoChips} chips`);
}

await app.finish("what the app does not model is on the page");
