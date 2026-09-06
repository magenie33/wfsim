// A SHARE LINK, end to end, in a browser that has never seen the build.
//
// Exists because this path broke twice in ways a state check could not see.
// Both times the presets landed correctly and `slots` held the right mods —
// and the visitor still stared at an empty page, because what is VISIBLE is
// decided somewhere else. So this asserts the screen, not the variables:
// the builder is shown, the home grid is not, and the stats panel the
// recipient renders is the one the sender saw, character for character.
//
//   node scripts/check_share.mjs        (serves site/, drives headless Chrome)
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";
const app = await openApp({ boot: 12000 });
const { evaluate, check, sleep, send, BASE } = app;

// A RIVEN'S ID IS MINTED ON ARRIVAL, and that is the feature: the copy the
// reader gets is a card of THEIRS with an identity of its own. So sender and
// recipient never agree on it, and anything comparing the two has to say so —
// comparing the raw id asserts that the copy was NOT made.
const sameRiven = (s) => String(s).replace(/riven:[a-z0-9]+/g, "riven:*");

// SHARING CAN BE SWITCHED OFF, and while it is, THIS is what has to hold: no
// way to make a new link, and every link already posted still opens a page.
// A blank is the one outcome a posted URL must never reach — it is what got the
// feature switched off.
//
// The round trip below runs again the moment the flag goes back to true, so
// this file is the check for both states rather than a file to remember to
// restore.
const on = await evaluate("typeof SHARE_ENABLED === 'undefined' ? true : SHARE_ENABLED");
if (!on) {
  const off = await evaluate(`(async () => {
    const s = (ms) => new Promise(r => setTimeout(r, ms));
    localStorage.clear();
    history.pushState({}, '', '/weapons/Torid'); route(); await s(2500);
    const out = { button: !!document.querySelector('.pchip.share') };
    // A link from when it was on: the query goes, a page arrives, and it says why.
    history.pushState({}, '', '/weapons/Torid?b=1abcNOTAREALCODE'); route(); await s(2500);
    out.query = location.search;
    out.drew = document.body.innerText.trim().length;
    out.onWeapon = !document.querySelector('.config-page').hidden;
    await s(1200);
    out.said = (document.getElementById('toast') || {}).textContent || '';
    return out;
  })()`);
  check("no way to make a new link while sharing is off", off.button === false);
  check("a link already posted still opens a page", off.drew > 400 && off.onWeapon === true,
    `${off.drew} chars, weapon page ${off.onWeapon}`);
  check("...the query is stripped, so a refresh cannot retry it", off.query === "");
  check("...and it says why", /sharing is off|分享功能暂时关闭/.test(off.said), JSON.stringify(off.said));
  await app.finish("sharing is off, and every posted link still opens a page");
}

// ---- the SENDER: a build with a riven and a non-default scenario ---------
const sent = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Torid/rivens'); route(); await sleep(1800);
  document.querySelector('.cu-new').click(); await sleep(700);
  riven.bonuses[0] = { id: 'damage', roll: 1.05 };
  riven.bonuses[1] = { id: 'critical_damage', roll: 1.0 };
  riven.bonuses[2] = { id: 'multishot', roll: 0.95 };
  riven.malus = { id: 'zoom', roll: 1.0 };
  markRivenDirty(); await sleep(1000);
  history.pushState({}, '', '/weapons/Torid'); route(); await sleep(1400);
  ['serration','split_chamber','point_strike','vital_sense','hellfire']
    .forEach((m, i) => { if (modById(m)) { slots[i].mod = m; slots[i].rank = modById(m).max_rank; } });
  slots[6].mod = rivenMods()[0].id;
  arcanes = ['primary_deadhead'];
  evoSel = { 1: 'torid_evo1_incarnon_form', 2: 'torid_final_fusillade' };
  sim.level = 155; sim.steel_path = false; sim.headshot_pct = 40;
  markPresetDirty(); markScenarioDirty(); renderMods(); refreshPanel(); await sleep(2000);
  const panel = ['stats-rows','stats-damage']
    .map(id => (document.getElementById(id) || {}).textContent || '').join(' | ').replace(/\\s+/g,' ').trim();
  // AND WHAT THE PANEL ACTUALLY OFFERS. Everything else here calls the codec
  // directly, so the panel could be wired to the wrong one of the two and no
  // assertion would notice — which is exactly what a sabotage of the panel
  // proved on the way in: three assertions stayed green while the button
  // handed out a link carrying the fight.
  const bar = document.querySelector('#preset-bar-builder-builds');
  await openSharePanel(bar); await sleep(900);
  const shown = ((bar.querySelector('.pshare .sh-url') || {}).value) || '';
  const dec = shown.includes('?b=') ? await decodeShare(shown.split('?b=')[1]) : null;
  // A LEGACY LINK, BUILT BY HAND from this very build: the same tuple with a
  // fight in field 7 and a measurement in field 8, encoded the way v1/v2 links
  // in the wild are. Nothing can produce one any more, which is exactly why it
  // has to be made here — the links it stands for are already posted.
  const legacy = (() => {
    const a = sharePayload();
    while (a.length < 9) a.push(0);
    a[7] = { level: 155, headshot_pct: 40 };
    a[8] = [12345, 60, 999];
    return a;
  })();
  const lj = new TextEncoder().encode(JSON.stringify(legacy));
  const lz = await deflate(lj);
  const legacyCode = lz && lz.length < lj.length ? '1' + b64urlEnc(lz) : '0' + b64urlEnc(lj);
  const ldec = await decodeShare(legacyCode);
  return { url: await shareUrl(), panel, mods: slots.map(s => s.mod),
           legacyUrl: location.origin + weaponPath('torid') + '?b=' + legacyCode,
           legacyDec: { sc: ldec && ldec.sc, m: ldec && ldec.m,
                        mods: (ldec && ldec.slots || []).filter(x => x && x.mod).length },
           card: !!bar.querySelector('.pshare .sh-full'),
           shownIsBuildOnly: !!dec && dec.sc === null && dec.m === null,
           shownHasMods: !!dec && (dec.slots || []).some(x => x && x.mod) };
})()`);
// WHAT THE PANEL HANDS OUT IS A BUILD. Asserted on the panel's own field
// rather than on `shareUrl`, because the button could be wired to something
// else and every other assertion here would stay green — which is what a
// sabotage of the panel proved on the way in.
check("the share panel hands out a BUILD", sent.shownIsBuildOnly === true,
  `decoded sc/m from the panel's own link`);
check("...and it is a real build, not an empty one", sent.shownHasMods === true);
check("a link is produced", !!sent.url, sent.url);
check("the link is under 600 characters", sent.url.length < 600, `${sent.url.length} chars`);
// **THE PANEL OFFERS ONE THING**. There is no second link and no card entry
// beside it: a card states a MEASUREMENT, and what may be shared is a build.
check("the panel offers no card", sent.card === false);
// AND A LEGACY LINK IS READ AS A BUILD. The decoder is where a fight stops —
// one place rather than a guard at every use — so the assertion is on what the
// DECODER returns for a link that really does carry one.
check("a legacy link's fight is not decoded", sent.legacyDec.sc === null,
  JSON.stringify(sent.legacyDec.sc));
check("...nor its measurement", sent.legacyDec.m === null, JSON.stringify(sent.legacyDec.m));
check("...and its build still is", sent.legacyDec.mods >= 5, `${sent.legacyDec.mods} mods`);
// ---- the RECIPIENT: a real navigation, in a browser with nothing ---------
await evaluate(`(() => { localStorage.clear(); location.href = ${JSON.stringify("__URL__").replace("__URL__", sent.url)}; })()`);
await sleep(12000);
const got = await evaluate(`(async () => {
  const q = (s) => document.querySelector(s);
  await new Promise(r => setTimeout(r, 2500));
  const panel = ['stats-rows','stats-damage']
    .map(id => (document.getElementById(id) || {}).textContent || '').join(' | ').replace(/\\s+/g,' ').trim();
  return {
    search: location.search,
    homeVisible: !q('#home-page').hidden,
    configVisible: !q('.config-page').hidden,
    slotsDrawn: document.querySelectorAll('#mod-slots .slot').length,
    mods: slots.map(s => s.mod),
    rivens: loadPresetList('rivens').map(p => p.name),
    scenarioLevel: sim.level, headshot: sim.headshot_pct,
    activeBuild: activePreset,
    panel,
  };
})()`);
check("the builder is on screen without a refresh", got.configVisible && !got.homeVisible,
  `config=${got.configVisible} home=${got.homeVisible}`);
check("the mod slots are drawn", got.slotsDrawn === 8, `${got.slotsDrawn} slots`);
check("the query is stripped", got.search === "", got.search);
check("the build is the shared one", /\(shared\)/.test(got.activeBuild || ""), got.activeBuild);
check("the riven travelled", got.rivens.length === 1, JSON.stringify(got.rivens));
check("the riven is equipped in its slot", /^riven:/.test(got.mods[6] || ""), got.mods[6]);
// AND THE SCENARIO DOES NOT TRAVEL, which is the inversion of what this line
// would assert. The sender was on level 155 at 40% headshots; the recipient
// keeps whatever fight they were already in, and 155/40 arriving would mean a
// link had moved it. That is the one thing a build link must never do.
check("the scenario does NOT travel — the reader's own fight is untouched",
  got.scenarioLevel !== 155 || got.headshot !== 40,
  `level=${got.scenarioLevel} headshot=${got.headshot}`);
check("the panel reproduces the sender's exactly", sameRiven(got.panel) === sameRiven(sent.panel),
  sameRiven(got.panel) === sameRiven(sent.panel) ? ""
    : `\n    sent: ${sameRiven(sent.panel).slice(0, 140)}\n    got : ${sameRiven(got.panel).slice(0, 140)}`);

// ---- ACT THREE: a BUILD is not a CLAIM, and it moves nobody's fight -------
//
// The sharp case, and why the split exists: a build link posted into a chat is
// clicked by people in the middle of their own measurement, and landing a
// scenario preset in their collection is not a thing a build may do.
// `importShare` skips the whole scenario arm when no fight travelled, and this
// asserts the CONSEQUENCE rather than the branch — a reader's own level, their
// own active scenario, and the length of their own list, before and after.
//
// The recipient is set up on a DIFFERENT weapon with a DISTINCTIVE fight,
// because a scenario is shared across the roster (SHARED_DOMAINS) and the
// thing being tested is that switching weapons through a share link does not
// disturb it.
//
// A SCENARIO OF THEIR OWN FIRST. The app lands a first-time visitor on the
// OFFICIAL ruler, whose fight is PINNED, so writing `sim.level` on it changes
// an in-memory object that is never saved and a setup assertion passes by
// reading back what it just wrote. Same trap `check_arena.mjs` names: an
// editable fight has to be MADE, and the check has to assert that it was.
await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear();
  history.pushState({}, '', '/weapons/Braton/simulator'); route(); await sleep(3000);
  const bar = document.querySelector('#preset-bar-simulator-scenarios');
  const add = bar && bar.querySelector('.pchip.add');
  if (add) { add.click(); await sleep(1600); }
  sim.level = 90; sim.headshot_pct = 0; markScenarioDirty(); await sleep(1800);
})()`);
const mine = await evaluate(`({
  level: sim.level,
  official: typeof officialScenarioActive === 'function' ? officialScenarioActive() : null,
  active: activeScenario,
  scenarios: loadPresetList('simulator-scenarios').map(p => p.name),
})`);
// BOTH HALVES, because the in-memory value alone is what made the first
// version of this pass on a fight it could not actually have edited.
check("the reader has a fight of their own to disturb",
  mine.level === 90 && mine.official === false,
  `level=${mine.level} official=${mine.official} active=${mine.active}`);

// THE LINK NAVIGATED TO IS THE LEGACY ONE — the tuple with a fight at 155/40
// in field 7 and a measurement in field 8. A link that carries nothing cannot
// prove that nothing lands; this one can, and it is the shape already posted in
// chat windows, so it is the shape that has to be safe to click.
await evaluate(`(() => { location.href = ${JSON.stringify(sent.legacyUrl)}; })()`);
await sleep(12000);
const bo = await evaluate(`(async () => {
  await new Promise(r => setTimeout(r, 2500));
  const b = loadPresetList('builder-builds').find(p => p.name === activePreset);
  return {
    weapon: document.getElementById('weapon').value,
    activeBuild: activePreset,
    mods: slots.map(s => s.mod),
    rivens: loadPresetList('rivens').map(p => p.name),
    level: sim.level, headshot: sim.headshot_pct,
    active: activeScenario,
    scenarios: loadPresetList('simulator-scenarios').map(p => p.name),
    landedResult: !!(b && b.lastResult),
    said: (document.querySelector('.preset-toast, .ptoast') || {}).textContent || '',
  };
})()`);
check("a legacy link still lands the build", /\(shared\)/.test(bo.activeBuild || "")
  && bo.weapon === "torid", `weapon=${bo.weapon} build=${bo.activeBuild}`);
check("...and its riven with it", bo.rivens.length === 1 && /^riven:/.test(bo.mods[6] || ""),
  `${JSON.stringify(bo.rivens)} slot6=${bo.mods[6]}`);
check("...and the mods are the sender's",
  sameRiven(JSON.stringify(bo.mods)) === sameRiven(JSON.stringify(sent.mods)),
  `
    sent: ${sameRiven(JSON.stringify(sent.mods))}
    got : ${sameRiven(JSON.stringify(bo.mods))}`);
// THE FOUR THAT MUST NOT HAVE MOVED — asserted against a link that was really
// carrying all of them.
check("the reader's fight is untouched", bo.level === 90 && bo.headshot === 0,
  `level=${bo.level} headshot=${bo.headshot} — the link carried 155/40`);
check("...no scenario was planted in their list", bo.scenarios.length === mine.scenarios.length,
  `${JSON.stringify(mine.scenarios)} -> ${JSON.stringify(bo.scenarios)}`);
check("...and they are still on their own", bo.active === mine.active,
  `${mine.active} -> ${bo.active}`);
// A NUMBER IS NOT A BUILD'S EITHER. The link carried one; the build lands with
// no result at all, so the first run on this machine is its first number.
check("...and the sender's measurement did not land", bo.landedResult === false);

// ---- THE COMPACT FORMS: SAME BUILD, FEWER CHARACTERS ----------------------
//
// v2 spells every id out and deflates the result. v3 sends each one's place in a
// frozen manifest; v4 spells that same place in base62 at a fixed two
// characters, so nothing separates one id from the next. Each is a renumbering
// or a respelling of EVERY id in the payload, laid on a format that has already
// dropped an axis silently once (`mode` and `valence` were in the state and not
// in the tuple, so a shared Kuva Nukor reopened on the default element).
//
// SO THE ASSERTION IS AN ANSWER, NOT A FIELD LIST: the same build encoded every
// way must decode to the same object. It cannot go stale — an axis added
// tomorrow is covered without editing this file, because neither side of the
// comparison knows what an axis is.
{
  const enc = await evaluate(`(async () => {
    const s = (ms) => new Promise(r => setTimeout(r, ms));
    localStorage.clear();
    history.pushState({}, '', '/weapons/Dual_Toxocyst?bench=single_target&mode=cycle&riven=1');
    route(); await s(4000);
    const out = { build: activePreset };
    out.hasRiven = slots.some(x => String(x.mod || '').startsWith('riven'));
    // A NAME A PERSON TYPED, and not one this app would ever generate: a space
    // and a non-ASCII character are exactly what a compact form's alphabet does
    // not hold, and the name is the one field that may contain them.
    //
    // SAVED AS THE READER'S OWN, because the board build this arrived as is
    // BUILTIN — and a builtin's name never travels whatever it is called, so
    // renaming it in place would assert nothing about names at all.
    const ps = loadPresetList('builder-builds');
    ps.push({ name: '我的 build', savedAt: Date.now(), state: snapshotState() });
    storePresetList('builder-builds', ps);
    activePreset = '我的 build';
    await s(600);

    // ONE payload, every framing — so the comparison is about the framing and
    // never about two different builds.
    const payload = sharePayload();
    out.name = payload[2];
    const json = new TextEncoder().encode(JSON.stringify(payload));
    const z = await deflate(json);
    const codes = {
      v2: (z && z.length < json.length ? '1' + b64urlEnc(z) : '0' + b64urlEnc(json)),
      v3: packV3(payload) === null ? null : '3' + packV3(payload),
      v4: packV4(payload) === null ? null : '4' + packV4(payload),
    };
    out.expressed = Object.fromEntries(Object.entries(codes).map(([k, v]) => [k, v !== null]));
    out.len = Object.fromEntries(Object.entries(codes).map(([k, v]) => [k, v && v.length]));
    const want = JSON.stringify(await decodeShare(codes.v2));
    out.same = {};
    out.got = {};
    for (const [k, v] of Object.entries(codes)) {
      if (v === null) continue;
      const got = JSON.stringify(await decodeShare(v));
      out.same[k] = got === want;
      if (got !== want) {
        // WHERE THEY PART, as data — two equal 300-character prefixes say
        // nothing. Formatted on the node side, because a newline escape inside
        // this body is eaten by the template literal carrying it.
        let i = 0;
        while (i < got.length && i < want.length && got[i] === want[i]) i++;
        out.got[k] = { at: i, want: want.slice(Math.max(0, i - 40), i + 90),
                       got: got.slice(Math.max(0, i - 40), i + 90) };
      }
    }
    out.want = want.slice(0, 200);

    // ...AND THROUGH THE THING THAT ACTUALLY CARRIES IT. shareUrl builds the
    // URL by CONCATENATION, so what has to survive is a raw code sitting in a
    // real query and read back the way the router reads it — not
    // 'searchParams.set', which escapes the code first and would prove nothing
    // about the path production takes.
    out.viaUrl = {};
    for (const [k, v] of Object.entries(codes)) {
      if (v === null) continue;
      const href = 'https://wfsim.app/weapons/Dual_Toxocyst?b=' + v;
      const back = new URL(href).searchParams.get('b');
      out.viaUrl[k] = JSON.stringify(await decodeShare(back)) === want;
    }
    return out;
  })()`);
  check("the case runs on a build with a riven, which is the hard one",
    enc.hasRiven === true, `${enc.build}, riven ${enc.hasRiven}`);
  check("...and on a name a PERSON typed, with a space and a non-ASCII character",
    enc.name === "我的 build", JSON.stringify(enc.name));
  for (const v of ["v3", "v4"]) {
    check(`...${v} can express it`, enc.expressed[v] === true);
    check(`...${v} decodes to the SAME build as v2`, enc.same[v] === true,
      enc.same[v] ? "" : `they part at character ${enc.got[v].at}
      want …${enc.got[v].want}
      got  …${enc.got[v].got}`);
    check(`...${v} survives a real URL unchanged`, enc.viaUrl[v] === true);
  }
  // AND EACH FORM IS SHORTER THAN THE ONE IT REPLACES. Stated as an ordering
  // rather than as a number: a manifest that grows moves every figure here, and
  // what must hold is that the denser spelling is still denser.
  check(`v4 < v3 < v2 (${enc.len.v4} < ${enc.len.v3} < ${enc.len.v2})`,
    enc.len.v4 < enc.len.v3 && enc.len.v3 < enc.len.v2,
    `v4=${enc.len.v4} v3=${enc.len.v3} v2=${enc.len.v2}`);
}

await app.finish("a shared link lands whole, on screen, first time");
