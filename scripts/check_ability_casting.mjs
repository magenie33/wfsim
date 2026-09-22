// SPDX-License-Identifier: AGPL-3.0-or-later
// CASTING IS AN ACTION, SO THE ACTION PRIORITY LIST DECIDES IT. An ability the
// list never names is handed to you — what every board row was measured under —
// and one it names is cast: energy out of the frame's pool, and the shooting
// the cast interrupts.
//
//   node scripts/check_ability_casting.mjs
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;
const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  localStorage.clear(); sessionStorage.clear();
  history.pushState({}, '', '/weapons/Valkyr_Talons/simulator'); route(); await sleep(4500);
  // WARCRY UP, the way the buff block hands it to you: no rule names it.
  setWfAbility('warcry', true); await sleep(1200);
  const base = theFight({ runs: 1 });
  const assumed = await api('/api/simulate', base);
  // …AND THE SAME FIGHT WITH ONE RULE INSERTED: cast it whenever it is down.
  const rule = { action: { do: 'cast', ability: 'warcry' },
                 when: { if: 'buff_remains_under', ability: 'warcry', seconds: 0 } };
  const cast = await api('/api/simulate', { ...base, apl: [rule] });
  // A RULE THIS ENGINE CANNOT READ IS REFUSED, not dropped.
  const bad = await api('/api/simulate',
    { ...base, apl: [{ action: { do: 'channel', ability: 'warcry' }, when: { if: 'always' } }] });
  // WHAT THE PAGE SHOWS IS THE FIGHT'S OWN LIST while nothing is inserted.
  const shown = [...document.querySelectorAll('#sim-build-info .sb-apl li')]
    .map((li) => li.textContent.replace(/\\s+/g, ' ').trim().replace(' if ', ',if='));
  return { assumed: { apl: assumed.apl, score: assumed.score },
           cast: { apl: cast.apl, score: cast.score },
           bad: { ok: !!bad.ok, error: bad.error || null }, shown };
})()`);
console.log(JSON.stringify(r, null, 1));
check("a fight that inserts nothing runs the mode's own list",
  Array.isArray(r.assumed.apl) && !r.assumed.apl.some((l) => l.startsWith("warcry")),
  JSON.stringify(r.assumed.apl));
check("...and the page shows that list, line for line",
  JSON.stringify(r.shown) === JSON.stringify(r.assumed.apl),
  JSON.stringify(r.shown));
check("an inserted rule goes above the mode's, where it can fire",
  r.cast.apl[0] === "warcry,if=buff.warcry.remains<0",
  JSON.stringify(r.cast.apl));
check("...and casting costs the fight something an assumed buff never did",
  r.cast.score < r.assumed.score,
  `${r.cast.score} < ${r.assumed.score}`);
check("a rule this engine cannot read is refused rather than dropped",
  !r.bad.ok && /action priority list/.test(r.bad.error || ""), JSON.stringify(r.bad));
await app.finish("the list says who is cast");
