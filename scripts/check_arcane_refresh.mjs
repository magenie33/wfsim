// FOURTEENTH CHECK — changing an arcane reaches the panel and the buff bar.
//
// Reported 2026-08-05: "切换赋能不会刷新缓存，需要切换一下mod才能刷新可以正确
// 显示". The arcane picker redrew its own slots and stopped — `refreshPanel` is
// the funnel every build change is supposed to go through, and the arcane path
// skipped it, so the stats and the SIM'S BUFF BAR kept showing the previous
// arcane until an unrelated edit happened to refresh them. Toggling a mod was
// the usual accident.
//
// The fix put the refresh inside the mutation, so this asserts the OBSERVABLE:
// pick an arcane, touch nothing else, and its buff card is on screen with its
// name in the display language.
//
//   node scripts/check_arcane_refresh.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";
const app = await openApp({ boot: 12000 });
const { evaluate, check, sleep, send, BASE } = app;
for (const lang of ["en", "zh"]) {
  // A FULL RELOAD per language. The display language is read from storage at
  // BOOT, and the buff bar keeps whatever the last pass left in it — running
  // both passes in one page made the second one compare a card against itself.
  await evaluate(`localStorage.clear(); localStorage.setItem('wfsim-lang', ${JSON.stringify(lang)})`);
  await send("Page.navigate", { url: BASE }); await sleep(11000);
  const r = await evaluate(`(async () => {
    const sleep=ms=>new Promise(r=>setTimeout(r,ms));
    // A SHOTGUN, because Shotgun Vendetta is class-gated to one.
    history.pushState({},'','/weapons/Boar_Prime'); route(); await sleep(4500);
    const out = { lang: '${lang}' };
    // BY ID, not by rendered text: the card's .bn carries the grants too, so
    // matching on text compares a name against a sentence.
    const ids = () => (buffList || []).map(b => b.id);
    out.before = ids();
    // PICK THE ARCANE AND TOUCH NOTHING ELSE. No mod edit, no tab switch —
    // the whole bug was that something else had to happen.
    setArcane('shotgun_vendetta', 0);
    renderArcanes();
    await sleep(1800);
    out.after = ids();
    out.appeared = out.after.filter(n => !out.before.includes(n));
    // THE GENERAL MECHANISM, proved by BYPASSING the fix. Assign the state
    // directly — no setArcane, no render call, nothing that could have been
    // taught to refresh — and then do the one thing a user always does: click.
    // The panel must catch up on its own, because the trigger is derived from
    // the build rather than fired by whoever changed it.
    arcanes[0] = 'none';
    document.body.click();
    await sleep(1600);
    out.afterRawEdit = ids();
    out.watchdogCaughtIt = !out.afterRawEdit.includes('arcane:shotgun_vendetta');
    // Put it back for the language assertion below.
    setArcane('shotgun_vendetta', 0); renderArcanes(); await sleep(1500);
    // ...and what that card actually READS on screen, for the language check.
    const el = document.querySelector('#sim-buffs [data-b="arcane:shotgun_vendetta"]');
    out.shown = el ? (el.closest('.buff-card') || el).textContent.replace(/\s+/g,' ').trim() : '';
    // ...and the card the panel built for it.
    const b = (buffList || []).find(x => x.id === 'arcane:shotgun_vendetta');
    out.buff = b ? { name: b.name, grants: b.grants, max: b.max_stacks, kind: b.kind } : null;
    return out;
  })()`);
  console.log(`\n[${lang}]`);
  check("the arcane's buff card appears with no other edit",
    r.appeared.length >= 1, `before ${JSON.stringify(r.before)} after ${JSON.stringify(r.after)}`);
  check("the panel built a card for it", !!r.buff, JSON.stringify(r.buff));
  if (r.buff) {
    check("...granting both halves", /Multishot/i.test(r.buff.grants) && /Reload/i.test(r.buff.grants),
      r.buff.grants);
    check("...as a single toggle", r.buff.max === 1, String(r.buff.max));
  }
  // The NAME is DE's, in the display language — 霰弹·仇杀 in Chinese.
  check("a RAW state edit is caught too — the trigger is derived, not fired",
    r.watchdogCaughtIt === true, JSON.stringify(r.afterRawEdit));
  check("its card is named in the display language",
    lang === "zh" ? /霰弹|仇杀/.test(r.shown) : /Vendetta/i.test(r.shown), r.shown);
}

await app.finish("all good");
