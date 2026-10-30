// Global scenario inputs stay editable; effective class values have separate readbacks.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, sleep } = app;

/// The three fields as the panel draws them, for whichever weapon is open.
const FIELDS = `(() => {
  // Two boxes: what the WIELDER is doing (technique) and what the fight is
  // LIMITED by (limits). Ammo is the second kind, which is why this looks in
  // both rather than assuming one panel holds all three.
  const boxes = ['sim-technique', 'sim-limits']
    .map((id) => document.getElementById(id)).filter(Boolean);
  if (!boxes.length) return { missing: true };
  const pick = (k) => {
    let el = null;
    for (const b of boxes) { el = el || b.querySelector('[data-k="' + k + '"]'); }
    if (!el) return { there: false };
    const lab = el.closest('label');
    return {
      there: true,
      disabled: !!el.disabled,
      checked: el.type === 'checkbox' ? !!el.checked : null,
      value: el.type === 'checkbox' ? null : el.value,
      why: lab?.querySelector('.scenario-effective')?.title || '',
      effective: lab?.querySelector('.scenario-effective')?.textContent || '',
    };
  };
  return { aiming: pick('aiming'), ammo: pick('infinite_ammo'), head: pick('headshot_pct') };
})()`;

await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  history.pushState({}, '', '/weapons/Torid/simulator'); route(); await sleep(3000);
  const bar = document.querySelector('#preset-bar-simulator-scenarios');
  const add = bar && bar.querySelector('.pchip.add');
  if (add) { add.click(); await sleep(1600); }
})()`);
const editable = await evaluate(
  `typeof officialScenarioActive === 'function' ? !officialScenarioActive() : null`);
check("the fight under test is the reader's own, not a pinned ruler", editable === true,
  `officialScenarioActive() says ${editable === true ? "no" : "YES — every field would read disabled"}`);

const open = async (path) => {
  await evaluate(`(async () => {
    const sleep = (ms) => new Promise(r => setTimeout(r, ms));
    history.pushState({}, '', ${JSON.stringify(path)}); route(); await sleep(2600);
  })()`);
  return evaluate(FIELDS);
};

const sent = await open("/weapons/Verglas_Prime/simulator");
check("a sentinel shows effective aim while leaving the global input editable", sent.aiming.there && !sent.aiming.disabled && /On|开/.test(sent.aiming.effective), JSON.stringify(sent.aiming));
check("...and its ammo is pinned ON — there is no reserve to run out of",
  !sent.ammo.disabled && /On|开/.test(sent.ammo.effective), JSON.stringify(sent.ammo));
check("...and its headshot rate is pinned at 0",
  !sent.head.disabled && /: 0$/.test(sent.head.effective), JSON.stringify(sent.head));
check("...and each says why, in words", [sent.aiming, sent.ammo, sent.head]
  .every((f) => f.there && f.why && f.why.length >= 8),
  [sent.aiming, sent.ammo, sent.head]
    .map((f) => `"${String(f.why || (f.there ? "" : "ABSENT")).slice(0, 40)}"`).join(" | "));
check("...and the headshot reason is the SENTINEL one, not the generic blurb",
  /sentinel|companion|守护/.test(sent.head.why), `"${sent.head.why.slice(0, 90)}"`);

const ag = await open("/weapons/Larkspur_Prime/simulator");
check("a ground Arch-Gun's ammo is pinned OFF — pickups it cannot receive",
  !ag.ammo.disabled && /Off|关/.test(ag.ammo.effective), JSON.stringify(ag.ammo));
check("...but its head is a head", ag.head.there && ag.head.disabled === false,
  JSON.stringify(ag.head));

const ord = await open("/weapons/Torid/simulator");
check("an ordinary weapon owns all three", ord.aiming.disabled === false
  && ord.ammo.disabled === false && ord.head.disabled === false,
  `aim=${ord.aiming.disabled} ammo=${ord.ammo.disabled} head=${ord.head.disabled}`);

const kept = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  sim.headshot_pct = 40; markScenarioDirty(); await sleep(1600);
  const before = sim.headshot_pct;
  history.pushState({}, '', '/weapons/Verglas_Prime/simulator'); route(); await sleep(2600);
  const onSentinel = sim.headshot_pct;
  const input = document.querySelector('#sim-technique [data-k="headshot_pct"]');
  const shown = input?.value;
  const effective = input?.closest('label')?.querySelector('.scenario-effective')?.textContent;
  history.pushState({}, '', '/weapons/Torid/simulator'); route(); await sleep(2600);
  return { before, onSentinel, shown, effective, after: sim.headshot_pct };
})()`);
check("the reader's own headshot rate survives a trip through a sentinel",
  kept.before === 40 && kept.after === 40,
  `set ${kept.before} -> on the sentinel ${kept.onSentinel} -> back ${kept.after}`);
check("...while the global input shows 40 and the effective value shows 0", Number(kept.shown) === 40 && /: 0$/.test(kept.effective), `showed ${kept.shown}`);

await app.finish("a question this weapon cannot be asked is pinned, and says why");
