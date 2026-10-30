// A SEARCH BOX TAKES A CHINESE INPUT METHOD.
//
// An IME composes: "jue" is typed, then 绝路 is committed, and every keystroke
// of the composing fires `input` with `isComposing` set. A box whose handler
// redraws the box replaces the element under the composition and kills it — on
// a phone or a desktop IME nothing can be typed at all, while Latin typing
// looks fine because a lone letter has nothing to lose. Chrome's own IME
// emulation (`Input.imeSetComposition`, `Input.insertText`) drives each box
// that filters as it is typed in, and asserts the box survives the composing,
// takes the committed text, and filters by it.
//
//   node scripts/check_ime_search.mjs
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000, lang: "zh" });
const { evaluate, check, send, sleep } = app;

/// One box: `prepare` opens the page it lives on, `filtered` says (page side)
/// whether the results now answer the committed text.
async function ime(name, route, selector, prepare, text, filtered) {
  await app.load(route);
  await sleep(1500);
  if (prepare) await evaluate(`(async () => { ${prepare} })()`);
  const found = await evaluate(`(() => {
    const el = document.querySelector(${JSON.stringify(selector)});
    if (!el) return false;
    el.focus(); el.value = ""; window.__imeBox = el;
    return document.activeElement === el;
  })()`);
  check(`${name}: the box is on the page and takes focus`, found === true);
  if (!found) return;
  await send("Input.imeSetComposition", { text: "jue", selectionStart: 3, selectionEnd: 3 });
  await sleep(500);
  const during = await evaluate(`(() => ({
    same: document.querySelector(${JSON.stringify(selector)}) === window.__imeBox,
    focused: document.activeElement === window.__imeBox,
  }))()`);
  check(`${name}: composing does not replace the box`, during.same && during.focused, JSON.stringify(during));
  await send("Input.insertText", { text });
  await sleep(900);
  const after = await evaluate(`(() => ({
    value: (document.querySelector(${JSON.stringify(selector)}) || {}).value,
    filtered: (() => { ${filtered} })(),
  }))()`);
  check(`${name}: the committed text is in the box`, after.value === text, JSON.stringify(after.value));
  check(`${name}: the results answer it`, after.filtered === true, JSON.stringify(after.filtered));
}

await ime("home search", "/", "#home-q", "", "绝路", `
  const cards = [...document.querySelectorAll("#home-grid .wcard, .wgrid .wcard")];
  return cards.length > 0 && cards.every((c) => c.textContent.includes("绝路"));`);

await ime("board search", "/benchmark", "#bench-q", "", "绝路", `
  const t = (document.getElementById("bench-board") || {}).textContent || "";
  return t.includes("绝路");`);

await ime("optimizer mod filter", "/weapons/Rubico_Prime/optimizer", "#opt-limit-filter", `
  const h = document.querySelector('[data-fold="opt-limits"].shut > .fold-h');
  if (h) h.click();
  await new Promise((r) => setTimeout(r, 300));`, "膛线", `
  const rows = [...document.querySelectorAll("#opt-limit-mods .opt")];
  return rows.length > 0 && rows.length < 10 && rows.some((r) => r.textContent.includes("膛线"));`);

// THE SYNC PAGE, against an account and a sync server faked in the page as
// check_cloud_page fakes them.
await ime("cloud list search", "/", "#cloud-q", `
  const realFetch = window.fetch;
  window.fetch = async (url, o = {}) => {
    const reply = (j) => new Response(JSON.stringify(j), { headers: { "content-type": "application/json" } });
    const path = String(url);
    if (path === "/api/account") return reply({ ok: true, providers: ["email"], account: { id: "acc1", created_at: "2026-09-01",
      identities: [{ provider: "email", label: "a@x" }] } });
    if (path === "/api/billing") return reply({ ok: true, configured: false });
    if (path === "/api/account/agents") return reply({ ok: true, agents: [] });
    if (path === "/api/cloud/sync") return reply({ ok: true, full: false, entries: [], next: null, cursor: 0, devices: [] });
    return realFetch(url, o);
  };
  localStorage.setItem("wfsim-presets-builder-builds", JSON.stringify([
    { id: "t1", scope: "torid", name: "绝路 torid", savedAt: Date.now(), state: {} },
    { id: "t2", scope: "torid", name: "crit torid", savedAt: Date.now(), state: {} }]));
  history.pushState({}, "", "/account/sync"); route(); await loadAccount();
  await new Promise((r) => setTimeout(r, 800));`, "绝路", `
  const names = [...document.querySelectorAll("#cloud-items tbody tr:not(.grp) .nm")].map((x) => x.textContent);
  return names.length === 1 && names[0].includes("绝路");`);

await ime("preset bar filter", "/weapons/Rubico_Prime/builder", "#preset-bar-builder-builds .pfilter", `
  for (let i = 0; i < 11; i++) await window.wfsim.do("shell.preset.new", { bar: "build" });
  await new Promise((r) => setTimeout(r, 800));`, "绝路", `
  // The preset being edited stays listed whatever the filter says.
  return document.querySelectorAll("#preset-bar-builder-builds .pchip[data-name]:not(.sel)").length === 0;`);

await app.finish("a search box takes a Chinese input method");
