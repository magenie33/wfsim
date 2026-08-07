// The ELEVENTH check: the fight has a TARGET, and the target is a unit —
// a picture that actually loads, a wiki page that actually exists, and a
// statement of what the sim does not model about it.
//
// Three failure modes it exists for:
//   1. Art that ships in `data/` but not in `site/img/` — the enemy portrait
//      is declared in the enemy's own YAML (`image:`), NOT in assets.yaml,
//      so it rides a different path to the build than every mod card.
//   2. A wiki link built from the DISPLAY name. In Chinese the target reads
//      堕落重型机枪手, and a wiki URL built from that lands on nothing. The
//      link must come from the English name in EVERY language, which is why
//      the whole check runs twice.
//   3. A silent modelling gap. An Acolyte carries damage attenuation whose
//      constants DE has never published, so the number this app gives against
//      one is too HIGH — and a caveat nobody can see is a wrong number.
//   4. A vulnerability column that never arrives. The Thrax's Void x1.5 rides
//      a FactionDamageOverride, which spent this whole project being parsed
//      and dropped; the column is now what decides which elements a build
//      wants, so the card has to say it and the api has to send it.
import { openApp } from "./cdp.mjs";

// boot: 0 — this check boots once per LANGUAGE in its own loop below.
const app = await openApp({ boot: 0 });
const { evaluate, check, sleep, send, BASE } = app;

// One pass per language. Everything asserted here has to hold in both: the
// art is language-independent, and the wiki link must be.
for (const lang of ["en", "zh"]) {
  await send("Page.navigate", { url: BASE });
  await sleep(lang === "en" ? 12000 : 4000);
  await evaluate(`localStorage.setItem('wfsim-lang', ${JSON.stringify(lang)})`);
  await send("Page.navigate", { url: BASE });
  await sleep(12000);

  const r = await evaluate(`(async () => {
    const sleep = (ms) => new Promise(r => setTimeout(r, ms));
    history.pushState({}, '', '/weapons/Torid/simulator'); route(); await sleep(2800);
    // THE FIGHT IS READ-ONLY ON ARRIVAL (2026-08-05): the official benchmark is
    // now the default scenario, and its controls are locked — so the enemy
    // picker's own button is disabled and cannot be opened. Copying it is the
    // real user flow for changing the target, and it is what this check needs
    // before it can ask anything about the picker.
    if (typeof officialScenarioActive === 'function' && officialScenarioActive()) {
      copyActiveScenario(); await sleep(1200);
    }


    // Every portrait in the roster, fetched the way the page asks for it.
    const roster = (META.enemies || []).map(e => ({ id: e.id, name: e.name, name_en: e.name_en,
      image: e.image, unmodeled: e.unmodeled || [], mods: e.type_modifiers || [] }));
    const art = [];
    for (const e of roster) {
      if (!e.image) { art.push([e.id, 'no image declared']); continue; }
      const ok = await new Promise(res => {
        const i = new Image();
        i.onload = () => res(i.naturalWidth > 0);
        i.onerror = () => res(false);
        i.src = '/img/' + encodeURIComponent(e.image);
      });
      if (!ok) art.push([e.id, e.image]);
    }

    // The card, for a target that HAS a caveat and one that does not.
    const cardFor = async (id) => {
      sim.enemy = id; renderSim(); await sleep(700);
      const host = document.getElementById('sim-target');
      const img = host.querySelector('.en-img');
      const link = host.querySelector('.en-wiki');
      return {
        name: (host.querySelector('.en-name') || {}).textContent || '',
        imgSrc: img ? img.getAttribute('src') : null,
        imgShown: !!img && img.naturalWidth > 0,
        href: link ? link.getAttribute('href') : null,
        gap: (host.querySelector('.en-gap') || {}).textContent || '',
        vuln: [...host.querySelectorAll('.en-vuln span')]
          .map(e => e.className + ':' + e.textContent.trim()),
        arenaImg: document.getElementById('arena-eimg').hidden
          ? null : document.getElementById('arena-eimg').getAttribute('src'),
        arenaDot: document.getElementById('arena-edot').hidden,
        // WHAT IT IS MADE OF, and the Eximus switch beside it.
        meta: (host.querySelector('.en-meta') || {}).textContent || '',
        eximusBox: !!host.querySelector('[data-k=eximus]'),
        eximusOn: !!(host.querySelector('[data-k=eximus]') || {}).checked,
      };
    };
    const acolyte = await cardFor('angst');
    const gunner = await cardFor('corrupted_heavy_gunner');
    // The pools are fetched, so the card is repainted a beat after it is
    // drawn. Re-read rather than re-render: this is the state a player sees.
    await sleep(1400);
    const gunnerHost = document.getElementById('sim-target');
    const gunnerLate = {
      meta: (gunnerHost.querySelector('.en-meta') || {}).textContent || '',
      eximusBox: !!gunnerHost.querySelector('[data-k=eximus]'),
      eximusOn: !!(gunnerHost.querySelector('[data-k=eximus]') || {}).checked,
    };
    const thraxCard = await cardFor('thrax_centurion');
    await sleep(1400);
    const thraxLate = {
      meta: (document.querySelector('#sim-target .en-meta') || {}).textContent || '',
      eximusBox: !!document.querySelector('#sim-target [data-k=eximus]'),
    };

    // The picker lists them all, with their pictures.
    document.getElementById('sim-target-pick').click(); await sleep(500);
    const rows = [...document.querySelectorAll('#enemy-menu .opt')];
    const menu = { rows: rows.length, thumbs: rows.filter(o => o.querySelector('.en-thumb')).length };
    closePopovers();

    return { roster, art, acolyte, gunner, menu, gunnerLate, thraxLate, lang: LANG,
             baseHealth: (META.enemies.find(e => e.id === 'corrupted_heavy_gunner') || {}).health };
  })()`);

  const tag = `[${lang}]`;
  check(`${tag} the app is in ${lang}`, r.lang === lang, r.lang);
  check(`${tag} every target declares a portrait and it LOADS`, r.art.length === 0,
    JSON.stringify(r.art));
  check(`${tag} the roster is the whole data/enemies/ library`, r.roster.length >= 8,
    `${r.roster.length} targets`);

  for (const [who, card] of [["acolyte", r.acolyte], ["gunner", r.gunner]]) {
    check(`${tag} ${who}: the card shows the portrait`, card.imgShown, card.imgSrc);
    check(`${tag} ${who}: the arena shows it too, instead of the dot`,
      !!card.arenaImg && card.arenaDot, `${card.arenaImg} / dot hidden ${card.arenaDot}`);
    // The whole reason this runs in zh: the label is localized, the URL is not.
    const want = who === "acolyte" ? "Angst" : "Corrupted_Heavy_Gunner";
    check(`${tag} ${who}: the wiki link is the ENGLISH page`,
      card.href === `https://wiki.warframe.com/w/${want}`, card.href);
  }
  check(`${tag} the acolyte states what is not modeled`, /⚠/.test(r.acolyte.gap), r.acolyte.gap);
  // The column, end to end: data file -> override resolution -> api -> card.
  const thrax = r.roster.find((e) => e.id === "thrax_centurion");
  check(`${tag} the Thrax's OVERRIDE column arrives (Void ×1.5)`,
    thrax.mods.length === 1 && thrax.mods[0].type === "void" && thrax.mods[0].mult === 1.5,
    JSON.stringify(thrax.mods));
  check(`${tag} the gunner shows what to bring BEFORE what to avoid`,
    r.gunner.vuln.length === 3 && r.gunner.vuln[2].startsWith("dn")
      && r.gunner.vuln.slice(0, 2).every((v) => v.startsWith("up")),
    JSON.stringify(r.gunner.vuln));
  // Neutral is NOTHING on screen, not an empty label.
  check(`${tag} a neutral unit claims no vulnerability`, r.acolyte.vuln.length === 0,
    JSON.stringify(r.acolyte.vuln));
  check(`${tag} the gunner claims no caveat it does not have`, r.gunner.gap === "", r.gunner.gap);
  check(`${tag} the picker lists every target, each with its picture`,
    r.menu.rows === r.roster.length && r.menu.thumbs === r.menu.rows, JSON.stringify(r.menu));
  // THE POOLS ARE THE FIGHT'S, NOT THE UNIT'S BASE (owner, 2026-08-05).
  // A Corrupted Heavy Gunner is 700 health in the data module and tens of
  // millions at the level this scenario runs at; printing the former is
  // answering a question nobody asked. The level has to be ON the line too —
  // the same digits mean different things at 9999 and at base level 8.
  check(`${tag} the gunner's pools are stated AT THE FIGHT'S LEVEL`,
    // Asserted on the LEVEL LABEL, not on the base health: the fallback line
    // is "Lv 8: 700 Health · 500 Armor", and a scaled line legitimately
    // contains "700" inside "2,700 Armor" — so matching the number caught the
    // right answer as if it were the wrong one.
    /Lv 9999/.test(r.gunnerLate.meta) && !/Lv 8\b/.test(r.gunnerLate.meta),
    r.gunnerLate.meta);
  // Millions, not hundreds — a scaled number is unmistakable, so this cannot
  // pass on a label alone.
  const biggest = Math.max(...(r.gunnerLate.meta.match(/[\d,]{4,}/g) || ["0"])
    .map((x) => Number(x.replace(/,/g, ""))));
  check(`${tag} and they are the SCALED numbers`, biggest > 1e6, `largest figure ${biggest}`);

  // THE ELITE VARIANT, offered where it exists and DEFAULTED ON.
  check(`${tag} the gunner offers the Eximus switch, ticked by default`,
    r.gunnerLate.eximusBox && r.gunnerLate.eximusOn,
    `box ${r.gunnerLate.eximusBox} / on ${r.gunnerLate.eximusOn}`);
  check(`${tag} its pools are the EXIMUS ones`, /Eximus|精英/.test(r.gunnerLate.meta),
    r.gunnerLate.meta);
  // ...and NOT offered where no such unit exists: the engine refuses the
  // combination, so a control would be a promise the fight cannot keep.
  check(`${tag} the Thrax offers no Eximus switch (no such unit)`,
    !r.thraxLate.eximusBox, r.thraxLate.meta);

  // Localized names must actually arrive — otherwise "the link is English" is
  // passing for the boring reason that everything is.
  if (lang === "zh") {
    check(`${tag} the target is NAMED in Chinese`, /[\u4e00-\u9fff]/.test(r.acolyte.name),
      r.acolyte.name);
  }
}

await app.finish("all good");
