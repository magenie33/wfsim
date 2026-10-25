// THE BUILT-IN SEARCHES (80-optimizer-preset.js, data/search/presets.yaml): a
// reader who owns no search has the first built-in — four starts, each the open
// weapon's 60/60 card of one element — read-only like an official ruler: its
// controls are inert, a click in them or ⧉ makes the reader's own copy, nothing
// it shows is stored, deleting the copy brings the built-in back, and the run
// button sends the search the page states.
//   node scripts/check_search_presets.mjs
import { openApp, sleep } from "./cdp.mjs";

const app = await openApp({ boot: 13000, base: process.env.WFSIM_BASE });
const { evaluate, check, finish } = app;
const cards = () => evaluate(`JSON.stringify(opt.starts.map((s) => startPayload(s).slots.filter(Boolean)))`).then(JSON.parse);

await app.load("/weapons/Torid/optimizer", 7000);
check("owning no search, a reader has the four-element built-in", await evaluate(`activeOptPreset === "search:four_elements"`),
  await evaluate("activeOptPreset"));
check("…four starts, each this weapon's 60/60 card of one element",
  JSON.stringify(await cards()) === JSON.stringify([["thermite_rounds"], ["rime_rounds"], ["high_voltage"], ["malignant_force"]]),
  JSON.stringify(await cards()));
check("…answering the run terms it states", await evaluate("optRun.finalists === 10 && optRun.candidate_runs === 10"));
check("…read-only on screen: its note shows and its controls are inert", await evaluate(`!$("opt-builtin").hidden
  && $("opt-finalists").disabled && [...$("opt-starts").querySelectorAll("button")].every((b) => b.disabled)`));
check("…and the run button is not among them", await evaluate(`!$("run-opt").disabled`));
// A CLICK IS NOT A REQUEST: the listener is handed the event, and a search that
// took it for a whole request searched nothing and listed one empty build.
const sent = JSON.parse(await evaluate(`(async () => {
  const real = window.fetch;
  let body = null;
  window.fetch = (url, init) => {
    if (!String(url).endsWith("/api/optimize")) return real(url, init);
    body = JSON.parse(init.body);
    return Promise.resolve(new Response(JSON.stringify({ ok: false, error: "held by the check" })));
  };
  try {
    $("run-opt").click();
    await new Promise((r) => setTimeout(r, 500));
  } finally {
    window.fetch = real;
  }
  return JSON.stringify(body && { weapon: body.weapon, strategy: body.strategy, starts: (body.starts || []).length });
})()`));
check("…and clicked, it sends the search the page states",
  !!sent && sent.weapon === await evaluate(`$("weapon").value`) && sent.strategy === "quick" && sent.starts === 4,
  JSON.stringify(sent));
check("…nor the limits' mod filter, which edits nothing", await evaluate(`!$("opt-limit-filter").disabled`));
// A CLICK IN A READ-ONLY REGION MAKES THE READER'S OWN COPY (`lockReadOnly`),
// and the click itself toggles nothing in it.
const deleteCopy = () => evaluate(`(() => { const del = $("preset-bar-optimizer").querySelector(".pop.del"); del.click(); if (loadOptPresets().length) del.click(); })()`);
check("…a limit row clicked makes the reader's own copy, and toggles nothing", await evaluate(`(() => {
  const was = JSON.stringify(opt.limits);
  const row = document.querySelector("#opt-limit-mods .opt[data-axis]");
  if (row) row.click();
  return !!row && JSON.stringify(opt.limits) === was && loadOptPresets().length === 1 && !builtinSearchActive();
})()`), await evaluate("activeOptPreset + ' / ' + loadOptPresets().length"));
await deleteCopy();
await sleep(800);
check("…and deleting it opens the built-in again", await evaluate(`loadOptPresets().length === 0 && activeOptPreset === "search:four_elements"`),
  await evaluate("activeOptPreset + ' / ' + loadOptPresets().length"));
check("…and the limits stay inert when they redraw", await evaluate(`(() => {
  renderOptLimits();
  return [...$("opt-limits").querySelectorAll("select,input[type=checkbox]")].every((el) => el.disabled)
    && !$("opt-limit-filter").disabled;
})()`));
// AN EDIT THAT REACHES IT ANYWAY — the door, say — is not written anywhere.
await evaluate(`setOptSizes({ finalists: 3 })`);
await sleep(1200);
check("nothing it shows is stored", await evaluate("loadOptPresets().length === 0"), await evaluate("JSON.stringify(loadOptPresets())"));

await evaluate(`$("opt-builtin-copy").click()`);
await sleep(800);
check("⧉ makes a search of the reader's own, editable", await evaluate(`loadOptPresets().length === 1 && !builtinSearchActive()
  && $("opt-builtin").hidden && !$("opt-finalists").disabled`));
check("…holding the same four starts", JSON.stringify(await cards()) === JSON.stringify([["thermite_rounds"], ["rime_rounds"], ["high_voltage"], ["malignant_force"]]));

await deleteCopy();
await sleep(800);
check("deleting it brings the built-in back", await evaluate(`loadOptPresets().length === 0 && activeOptPreset === "search:four_elements"`),
  await evaluate("activeOptPreset + ' / ' + loadOptPresets().length"));

await app.load("/weapons/Hek/optimizer", 7000);
check("a shotgun's built-in starts are the shotgun's 60/60 cards",
  JSON.stringify(await cards()) === JSON.stringify([["scattering_inferno"], ["frigid_blast"], ["shell_shock"], ["toxic_barrage"]]),
  JSON.stringify(await cards()));
await finish("the built-in searches are read-only and come back");
