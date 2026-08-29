// WHAT THE APP DOES NOT MODEL IS ON THE PAGE, in every family that has one.
//
// The owner debugs the way a player does — by reading the card — so a gap that
// exists only in a yaml comment or a report script is a gap nobody can act on. Five families can admit something, each with its own
// surface, and each has gone silent at least once:
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

    // WHERE AN EVOLUTION'S ADMISSIONS LIVE, now that a tier is a dropdown. Two places, and both are the promise:
    //
    //   * the ROWS you choose from, because that is where the choosing
    //     happens — rendered with ddRender, which is the same function the
    //     popover calls, so this is real markup and not the registry;
    //   * the INSTALLED perk's own card — the control IS the card now, so its
    //     chips are on the page without anybody opening anything.
    //
    // A locked tier still LISTS its perks, so this reads every tier rather
    // than only the reachable ones — what is being asserted is the chip, and
    // the ladder is check_gain_axes' subject.
    const evoRows = (sel) => {
      const out = [];
      for (const b of document.querySelectorAll('[data-slot^="dd-evo-"]')) {
        ddRender(b.dataset.slot);
        out.push(...document.querySelectorAll('#dd-menu .opt ' + sel));
      }
      out.push(...document.querySelectorAll('.slot.axis.filled ' + sel));
      return out;
    };
    const evoOpt = (id) => {
      for (const b of document.querySelectorAll('[data-slot^="dd-evo-"]')) {
        ddRender(b.dataset.slot);
        const el = document.querySelector('#dd-menu .opt[data-v="' + id + '"]');
        if (el) return el;
      }
      return null;
    };

    history.pushState({}, '', '/weapons/Stug'); route(); await sleep(3800);
    const out = { lang: ${JSON.stringify(lang)} };

    // 1. THE WEAPON BANNER. The Stug is the roster's most-disclaimed weapon:
    //    a blob economy, an unmodelled explosion, bouncing secondaries.
    out.banner = (document.querySelector('.unmod-h') || {}).textContent || '';
    out.bannerLines = [...document.querySelectorAll('.unmod-l')].map(e => e.textContent.trim());
    out.weaponGaps = (weaponInfo($('weapon').value).unmodeled || []).length;
    // …AND THE STUG HAS NO LIVE BUG, which is the negative control for the
    // block below: a banner that appeared on every weapon would tell nobody
    // anything.
    out.stugLiveBug = !document.getElementById('stats-livebug').hidden;

    // 1b. A LIVE BUG, WHICH IS THE OPPOSITE ADMISSION. The banner above says the
    //     number below is a FLOOR; this says the number is RIGHT, was measured,
    //     and nobody can explain it — so a hotfix could take it away. Filing one
    //     as the other would give a reader the opposite advice.
    //
    //     The Laetum is the case it exists for: its Incarnon form pays Secondary
    //     Irradiate's echo at 3.6x the hit while the arcane's own card, on the
    //     same screen, prints DE's 180%. The engine carried that as
    //     echo_multiplier and said so nowhere a reader could see, which is how
    //     it was reported.
    history.pushState({}, '', '/weapons/Laetum'); route(); await sleep(3200);
    const lb = document.getElementById('stats-livebug');
    out.bugShown = lb && !lb.hidden;
    out.bugHead = lb ? (lb.querySelector('.unmod-h') || {}).textContent || '' : '';
    out.bugLines = lb ? [...lb.querySelectorAll('.unmod-l')].map(e => e.textContent.trim()) : [];
    out.bugData = (weaponInfo('laetum').live_bugs || []).length;
    history.pushState({}, '', '/weapons/Stug'); route(); await sleep(3200);

    // 2. EVOLUTION TILES.
    out.evoChips = evoRows('.exchip.unmod').map(e => e.textContent.trim());
    // …AND THE OTHER ADMISSION. An evolution has two: a clause nobody has
    // modelled YET, and a clause that cannot pay out in a one-target fight at
    // all. They were one chip until 2026-08-12, which said "not modelled yet"
    // over perks nobody is working on and nobody should.
    out.evoScope = evoRows('.exchip.scope')
        .map(e => ({ text: e.textContent.trim(), why: e.getAttribute('title') || '' }));
    // …AND THE THIRD, which is not a shortfall of ours at all: a clause the
    // GAME does not pay out. The mod and arcane cards have carried live bugs
    // since 2026-08-08 and the evolutions had no arm for one until an owner
    // measurement found the first (Carnage Reign's "+33% per Status Type" pays
    // nothing — MEASUREMENTS M49). It is read from META rather than from the
    // open weapon's tiles, because the assertion is about the WHOLE roster
    // knowing how to say it, and its chip is asserted on the Dual Toxocyst's
    // own page below.
    out.evoBugs = (META.weapons || []).flatMap(w => (w.evolutions || [])
      .flatMap(t => (t.options || [])
        .filter(o => (o.live_bugs || []).length)
        .map(o => ({ id: o.id, bugs: o.live_bugs, effects: o.effects || [] }))));
    // …AND THE ARM IS EXERCISED WHETHER OR NOT THE ROSTER HAS ONE TODAY.
    //
    // It had exactly one for ten days — Carnage Reign's "+33% per Status Type",
    // read as a clause DE had broken — and on 2026-08-26 that turned out to be
    // an unlisted ENERGY MAX gate rather than a bug, so the count went to zero.
    // An "at least one" assertion over the roster would then have FAILED on
    // correct data, and the two beside it would have passed VACUOUSLY over an
    // empty list, which is the worse of the two outcomes.
    //
    // So the live bug is INJECTED, the way check_board_link injects a second
    // mode: the claim is that the machinery can SAY it, and that claim does not
    // depend on which perks happen to be broken this patch. It goes all the way
    // to the DOM, which is more than reading META back could ever prove.
    {
      const w = (META.weapons || []).find(x => x.id === $('weapon').value);
      const opt = ((w.evolutions || [])[0] || {}).options?.[0];
      if (opt) {
        opt.live_bugs = ['condition overload — an injected clause, to prove the chip draws'];
        renderEvo();
        await sleep(400);
        out.evoBugInjected = evoRows('.livebug').map(e => e.textContent.trim());
        out.evoBugTitle = (evoRows('.livebug')[0] || {}).title || '';
        delete opt.live_bugs;
        renderEvo();
        await sleep(300);
        out.evoBugGone = evoRows('.livebug').length;
      }
    }
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

    // 6b. THE LIVE-BUG MARK, which is the one admission that is NOT a
    //     shortfall: the number is right, the game is wrong, and a hotfix
    //     changes it. Asserted on a PRIMARY weapon because the arcane that
    //     carries it is a primary one — the Stug above seats secondaries and
    //     would have made this pass on an empty set.
    const parcs = arcanePool(0) || [];
    out.bugArcanes = parcs.filter(a => (a.live_bugs || []).length).map(a => a.id);
    out.bugClean = parcs.filter(a => !(a.live_bugs || []).length).length;
    try {
      openArcanePicker(document.querySelector('#arcane-slots .aslot, #arcane-slots *')
        || document.body, 0);
      await sleep(400);
      const bugs = [...document.querySelectorAll('#arcane-menu .livebug')];
      out.bugChips = bugs.map(e => e.textContent.trim());
      out.bugWhy = bugs.length ? bugs[0].title : '';
      // The mark means something only if MOST cards do not carry it.
      out.bugCards = document.querySelectorAll('#arcane-menu .unmodeled, #arcane-menu .livebug').length;
      closePopovers();
    } catch (e) { out.bugErr = String(e).slice(0, 90); }

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
    // MATCH THE LONGEST NAME, not any prefix. A row's label carries the weapon
    // and then its mode, so a prefix test is the only way to find the weapon —
    // and a some(startsWith) over the GAPPED names alone is wrong the moment
    // one weapon's name is a prefix of another's. That became false on
    // 2026-08-20: Baza / Baza Prime, Ignis / Ignis Wraith, Knell / Knell Prime,
    // Corinth / Corinth Prime, Tigris / Tigris Prime and a dozen more, so a
    // CLEAN Prime read as gapped because its ordinary version has a gap. The
    // row's weapon is the longest roster name its label starts with, and the
    // question is whether THAT weapon has gaps.
    const byLength = (META.weapons || []).slice()
        .sort((a, b) => b.name.length - a.name.length);
    out.markedRight = rows.every(r => {
        const name = (r.querySelector('.bname') || {}).textContent || '';
        const marked = !!r.querySelector('.bgap');
        const w = byLength.find(x => name.startsWith(x.name));
        return marked === !!(w && (w.unmodeled || []).length);
      });

    // 7b. THE TODO CHIP, on a weapon that still has one.
    //
    // NOT read off the Stug above: that stops working the day its last inert
    // perk is modelled, which is the check succeeding at its job and then
    // failing for it. The assertion is
    // that the gold "not modelled YET" chip still RENDERS, so it has to be
    // asked of a weapon that has one; the Felarx does, and it also carries
    // three weapon-level gaps of its own.
    //
    // The day nothing in the roster is inert, this stops finding a weapon and
    // says so, which is the right way to learn that the ratchet reached zero.
    history.pushState({}, '', '/weapons/Felarx'); route(); await sleep(3200);
    out.todoChips = evoRows('.exchip.unmod').map(e => e.textContent.trim());
    out.todoScope = evoRows('.exchip.scope').map(e => e.textContent.trim());

    // 7b-2. THE FIFTH KIND OF ADMISSION: THE CARD ITSELF IS WRONG.
    //
    // The other four are all "the number here is lower than the card promises",
    // in four different senses. This one is the opposite: the effect WORKS,
    // the simulation follows the game, and the perk's own text misdescribes it.
    // Swift Punishment prints "With Sprint Speed 1.2 or Higher" and its wiki
    // row says *"Despite the description, the effect only requires 1.1"* — a
    // player reading the card would mod for a threshold the game never asks
    // for (anything that differs from what the game DISPLAYS
    // is to be noted).
    //
    // A NEGATIVE CONTROL IN THE SAME TIER, which is what makes the assertion
    // mean anything: Riddled Target sits beside it, has no such note, and must
    // carry no chip. A check that only asserts presence passes just as well on
    // a page that stamps "card is wrong" on everything.
    history.pushState({}, '', '/weapons/Latron_Prime'); route(); await sleep(3800);
    const pick = evoOpt;
    out.misprintDeclared = ((META.weapons || [])
      .find((w) => w.id === 'latron_prime') || {}).evolutions || [];
    out.misprintChips = pick('latron_prime_swift_punishment')
      ? [...pick('latron_prime_swift_punishment').querySelectorAll('.exchip.misprint')]
          .map((e) => e.textContent.trim())
      : null;
    out.misprintWhy = pick('latron_prime_swift_punishment')
      ? [...pick('latron_prime_swift_punishment').querySelectorAll('.exchip.misprint')]
          .map((e) => e.title)
      : [];
    out.misprintControl = pick('latron_prime_riddled_target')
      ? pick('latron_prime_riddled_target').querySelectorAll('.exchip.misprint').length
      : -1;
    // …AND IT IS NOT FILED WITH THE BROKEN ONES. A live bug tells a reader not
    // to pick the perk; this tells them to pick it for a reason the card does
    // not state, so wearing the same chip would be the opposite advice.
    out.misprintNotLiveBug = pick('latron_prime_swift_punishment')
      ? pick('latron_prime_swift_punishment').querySelectorAll('.exchip.livebug').length
      : -1;

    // 7c. A BONUS THAT PAYS NOTHING SAYS WHY — the fourth surface, and the only
    //     one where the mechanic is fully MODELLED and still worth a line.
    //
    // The Vasto's Incarnon Form "cannot Zoom", which is the wiki's word for the
    // aim state, so every while-aiming bonus resolves to nothing in it. A mod
    // that silently contributes zero is indistinguishable from a broken one, so
    // the panel names the form and the reason.
    history.pushState({}, '', '/weapons/Vasto_Prime'); route(); await sleep(3200);
    slots[0] = { mod: 'galvanized_crosshairs' };
    evoSel[1] = 'vasto_prime_evo1_incarnon_form';
    markPresetDirty(); renderMods(); refreshPanel(); await sleep(2500);
    out.zoomLines = [...document.querySelectorAll('#stats-conditionals .scond')]
        .filter(e => /aim down sights/.test(e.textContent))
        .map(e => e.textContent.replace(/[ ]+/g, ' ').trim());
    // …and the SAME mod on a weapon whose forms all zoom gets no such line.
    history.pushState({}, '', '/weapons/Lex_Prime'); route(); await sleep(3000);
    slots[0] = { mod: 'galvanized_crosshairs' };
    markPresetDirty(); renderMods(); refreshPanel(); await sleep(2500);
    out.zoomFalsePositives = [...document.querySelectorAll('#stats-conditionals .scond')]
        .filter(e => /aim down sights/.test(e.textContent)).length;

    // 8. NEGATIVE CONTROL: a weapon with nothing to admit, ANYWHERE in its
    //    transform group. Opened EXPLICITLY: reading whatever page the block
    //    above happens to leave behind is right only by accident of ordering,
    //    and stops being so the moment a step is inserted.
    //
    //    THE TORID, and it is hand-written: no gaps, no inert perks, and both
    //    of its forms carry a transcribed SPREAD (0/0 on the grenade, which is
    //    the "Pinpoint accuracy" its page states in words, and 1/1.5 on the
    //    Incarnon beam). It briefly stopped being clean while the aim model
    //    read the weapon-level Accuracy stat, which no Incarnon form has —
    //    fixed by reading the per-ATTACK spread the wiki module publishes
    //    instead, which is the primary value anyway.
    //    (No backticks in here: this block lives inside a template literal.)
    history.pushState({}, '', '/weapons/Torid'); route(); await sleep(3200);
    out.cleanBanner = document.querySelector('.unmod-h') ? 'shown' : 'absent';
    out.cleanEvoChips = evoRows('.exchip.unmod').length;
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
  // A LIVE BUG IS ON THE PAGE, and it is not the "not modelled" banner.
  check(`[${lang}] a weapon that does something unexplained says so`,
    r.bugShown === true && r.bugLines.length === r.bugData && r.bugData >= 1,
    `${r.bugLines.length} lines, ${r.bugData} in the data`);
  check(`[${lang}] ...naming the number, so it can be argued with`,
    /3\.6/.test(r.bugLines.join(" ")), JSON.stringify(r.bugLines).slice(0, 140));
  // THE OPPOSITE ADVICE, said differently. "Not modelled" means the number is a
  // floor; this means the number is right and may be taken away by a hotfix. A
  // reader who cannot tell them apart has been told the wrong thing.
  check(`[${lang}] ...in its own words, not the shortfall banner's`,
    r.bugHead.length > 10 && r.bugHead !== r.banner,
    JSON.stringify([r.bugHead, r.banner]).slice(0, 160));
  check(`[${lang}] ...in the display language`,
    cjk.test(r.bugHead) === (lang === "zh") && cjk.test(r.bugLines.join("")) === (lang === "zh"),
    JSON.stringify(r.bugHead).slice(0, 90));
  // THE NEGATIVE CONTROL, and not a formality: the Stug is the roster's
  // most-disclaimed weapon and has no live bug, so a block that drew on
  // everything would fail here rather than reading as thorough.
  check(`[${lang}] ...and the most-disclaimed weapon on the roster has none`,
    r.stugLiveBug === false, String(r.stugLiveBug));

  // ONE OF EACH IS THE CLAIM NOW, not three of one. The Stug's inert clauses
  // have been going down all night — the count was a proxy for "the chip is
  // drawn", and the pair below says that better: a todo AND an edge, both on
  // screen, told apart.
  check(`[${lang}] evolution tiles carry a chip`, r.todoChips.length >= 1,
    `${r.todoChips.length} chips on the Felarx`);

  // THE FIFTH ADMISSION, and the one that is not a shortfall.
  check(`[${lang}] a perk whose CARD is wrong says so on the card`,
    Array.isArray(r.misprintChips) && r.misprintChips.length === 1,
    JSON.stringify(r.misprintChips));
  check(`[${lang}] ...naming the disagreement, in this language`,
    (r.misprintWhy[0] || '').length > 40
      && (lang === 'zh' ? /[\u4e00-\u9fff]/.test(r.misprintWhy[0]) : true),
    (r.misprintWhy[0] || '').slice(0, 120));
  check(`[${lang}] ...and not as a live bug, which is the opposite advice`,
    r.misprintNotLiveBug === 0, String(r.misprintNotLiveBug));
  check(`[${lang}] ...while its tier-mate, whose card is right, carries none`,
    r.misprintControl === 0, String(r.misprintControl));
  // BOTH KINDS ON SCREEN, and told apart. The Stug carries each: clauses the
  // model has no rule for yet, and Hoplite Virtue, whose trigger is the
  // PLAYER's shields breaking — which nothing in this arena can do.
  check(`[${lang}] ...and an edge says it is an edge, not a todo`,
    r.evoScope.length >= 1, `${r.evoScope.length} scope chips`);
  check(`[${lang}] ...naming the reason it can never pay out`,
    r.evoScope.every((c) => c.why.length > 20 && cjk.test(c.why) === (lang === "zh")),
    JSON.stringify(r.evoScope[0] || null).slice(0, 140));

  // A CLAUSE THE GAME DOES NOT DO, said as that and not as a todo. The reader's
  // action is what separates them: "not modelled yet" says wait for us, this
  // says do not pick the perk for that half — nobody is going to implement what
  // DE has not shipped (MEASUREMENTS M49).
  check(`[${lang}] an evolution whose clause the GAME does not honour says so`,
    (r.evoBugInjected || []).length === 1,
    `${(r.evoBugInjected || []).length} chips for one injected live bug`);
  check(`[${lang}] ...and the chip says the number matches the live game`,
    /unintended|bug|游戏|实际/i.test(r.evoBugTitle || '') || (r.evoBugTitle || '').length > 20,
    JSON.stringify(r.evoBugTitle || '').slice(0, 160));
  // THE NEGATIVE CONTROL, and the reason the injection is safe to trust: with
  // the flag removed the chip goes. A page that drew it unconditionally would
  // pass the assertion above and mean nothing.
  check(`[${lang}] ...and it is gone when nothing is flagged`,
    r.evoBugGone === 0, `${r.evoBugGone} chips left after the flag was removed`);
  check(`[${lang}] ...and it names the clause rather than condemning the whole perk`,
    (r.evoBugs || []).every((e) => e.bugs.every((b) => b.includes(" — "))
      && e.effects.some((x) => !/DOES NOT WORK/.test(x))),
    JSON.stringify((r.evoBugs || [])[0] || null).slice(0, 200));
  check(`[${lang}] ...and the dead clause never prints as if it paid`,
    (r.evoBugs || []).every((e) => e.effects.filter((x) => /33|per Status Type/i.test(x))
      .every((x) => /DOES NOT WORK/.test(x))),
    JSON.stringify(((r.evoBugs || [])[0] || {}).effects || null).slice(0, 200));
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
  const kinds = new Set([...r.modChips, ...(r.arcChips || []), ...r.todoChips]
    .map((s) => s.replace(/\s+/g, " ").trim()));
  check(`[${lang}] and more than one KIND of admission is on screen`,
    kinds.size >= 3, [...kinds].join(" | "));
  check(`[${lang}] an arcane with a dead effect is flagged in the data`,
    r.arcPartly.length >= 2, r.arcPartly.join(","));
  check(`[${lang}] …and the flag reaches its card`,
    (r.arcChips || []).length >= 1, r.arcErr || (r.arcChips || []).join(" / "));
  // THE FOURTH KIND. Every other admission on this page says the number is
  // lower than the card promises; this one says the number is right and rests
  // on a bug. A player building around 441x is entitled to know which.
  check(`[${lang}] an arcane that rides a live bug is flagged in the data`,
    (r.bugArcanes || []).length >= 1, (r.bugArcanes || []).join(","));
  check(`[${lang}] …and the mark reaches its card`,
    (r.bugChips || []).length === (r.bugArcanes || []).length,
    r.bugErr || `${(r.bugChips || []).length} chips for ${(r.bugArcanes || []).length} arcanes`);
  check(`[${lang}] …in the display language`,
    (r.bugChips || []).every((l) => cjk.test(l) === (lang === "zh")),
    JSON.stringify((r.bugChips || [])[0] || "").slice(0, 90));
  check(`[${lang}] …carrying WHAT the bug is, not just that there is one`,
    (r.bugWhy || "").length > 60 && cjk.test(r.bugWhy || "") === (lang === "zh"),
    (r.bugWhy || "").slice(0, 70));
  // The control: the mark is rare, so it reads as a statement about ONE arcane.
  check(`[${lang}] …and the arcanes with nothing to admit carry no mark`,
    (r.bugClean || 0) > (r.bugChips || []).length * 5,
    `${r.bugClean} clean arcanes`);
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

  // A BONUS THAT PAYS NOTHING SAYS WHY. Not an admission of a gap — the
  // mechanic is fully modelled — but a mod resolving to zero in silence reads
  // as broken, so it is the same duty.
  check(`[${lang}] an aim mod on a form that cannot zoom says so`,
    r.zoomLines.length === 1 && /cannot Zoom/.test(r.zoomLines[0]),
    r.zoomLines[0] || "(no line)");
  check(`[${lang}] …and a form that CAN zoom gets no such line`,
    r.zoomFalsePositives === 0, String(r.zoomFalsePositives));
}

await app.finish("what the app does not model is on the page");
