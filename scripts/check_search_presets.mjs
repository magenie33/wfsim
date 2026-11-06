// Ordinary searches begin as editable drafts; strategies are not presets.
import { openApp, sleep } from "./cdp.mjs";
import "./check_search_strategy.mjs";

const app = await openApp({ boot: 13000, base: process.env.WFSIM_BASE });
const { evaluate, check, finish } = app;
const blankDraft = `!activeOptPreset && loadOptPresets().length === 0
  && opt.starts.length === 1 && startPayload(opt.starts[0]).slots.every(s => !s)
  && opt.search_strategy === "four_elements" && $("opt-builtin").hidden`;

await app.load("/weapons/Torid/optimizer", 7000);
check("without saved searches, the editor has a blank draft, not a built-in preset",
  await evaluate(blankDraft));
check("the draft's starts, limits and run settings are editable", await evaluate(`
  !$("opt-finalists").disabled && !$("opt-cand-runs").disabled
  && [...$("opt-starts").querySelectorAll("button")].every(b => !b.disabled)
  && !optBarCfg().openable`));
check("the strategy selector explains the selected strategy", await evaluate(`(() => {
  const strategy = META.search_presets.strategies.find(s => s.id === $("opt-search-strategy").value);
  return !!strategy.description && $("opt-strategy-description").textContent === tr(strategy.description)
    && $("opt-search-strategy").options.length === 1;
})()`));
const sent = JSON.parse(await evaluate(`(async () => {
  const real = api;
  let body = null;
  api = (path, b, ...rest) => {
    if (path !== "/api/optimize") return real(path, b, ...rest);
    body = b;
    return Promise.resolve({ ok: false, error: "held by the check" });
  };
  try {
    $("run-opt").click();
    for (let i = 0; i < 100 && !body; i++) await new Promise(r => setTimeout(r, 50));
  } finally { api = real; }
  return JSON.stringify(body && { strategy: body.strategy, search_strategy: body.search_strategy, starts: body.starts.length });
})()`));
check("the editable draft runs with one start and the selected strategy",
  sent?.strategy === "quick" && sent.search_strategy === "four_elements" && sent.starts === 1, JSON.stringify(sent));
check("deleting the last start retains one blank start", await evaluate(`(() => {
  $("opt-starts").querySelector("[data-del]").click();
  return opt.starts.length === 1 && startPayload(opt.starts[0]).slots.every(s => !s);
})()`));

await evaluate(`$("preset-bar-optimizer").querySelector(".pchip.add").click()`);
await sleep(800);
check("users can still create their own saved searches", await evaluate(`
  !!activeOptPreset && !builtinSearchActive() && loadOptPresets().length === 1
  && snapshotOpt().search_strategy === "four_elements"`));
await evaluate(`(() => {
  const del = $("preset-bar-optimizer").querySelector(".pop.del");
  del.click(); if (loadOptPresets().length) del.click();
})()`);
await sleep(800);
check("deleting the last saved search returns to an editable blank draft",
  await evaluate(blankDraft + ` && !$("opt-finalists").disabled`));

await app.load("/weapons/Hek/optimizer", 7000);
check("another weapon also begins without a default preset", await evaluate(blankDraft));
await finish("search drafts and strategies are independent of saved presets");
