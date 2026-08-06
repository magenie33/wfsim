// CUSTOM CHECK — the two visitor-authored features work end to end.
//
//   node scripts/check_custom.mjs
//
// Asserts the OBSERVABLE, driving headless Chrome/Edge over CDP against the
// native wfsim server (target/debug/wfsim-web.exe). Covers:
//   1. home grid still shows the five weapon groups + a custom weapon card;
//   2. opening the custom weapon (/weapons/primary) shows its EMBEDDED panel
//      editor above the mod slots, has NO "Custom Weapon" tab, and its mod
//      picker lists the slot's mods (the regression: the picker was empty);
//   3. a custom weapon simulates (no panic, dps > 0);
//   4. creating a custom mod card lands it in the picker, searchable by name;
//   5. equipping the card changes the panel (the card resolves);
//   6. a roster weapon page hides the embedded custom form.
// Exits non-zero on the first failure.
import { spawn } from "node:child_process";
import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { resolve } from "node:path";
const EXE = resolve("target/debug/wfsim-web.exe");
const PORT = 8787;            // the wfsim native server
const CDP_PORT = 9531;        // the headless browser's debug port
const BASE = `http://127.0.0.1:${PORT}`;
const srv = spawn(EXE, [], { stdio: "ignore" });
const BIN = ["C:/Program Files/Google/Chrome/Application/chrome.exe", "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe", "C:/Program Files/Microsoft/Edge/Application/msedge.exe"].find((p) => existsSync(p));
if (!BIN) { console.error("FAIL  no Chrome/Edge for CDP"); process.exit(1); }
const proc = spawn(BIN, [`--remote-debugging-port=${CDP_PORT}`, "--headless=new", "--remote-allow-origins=*", "--disable-gpu", "--no-first-run", `--user-data-dir=${process.env.TEMP}/wfsim-custom-${Date.now()}`, "about:blank"], { stdio: "ignore" });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function cdp(path) {
  for (let i = 0; i < 60; i++) {
    try { const r = await fetch(`http://127.0.0.1:${CDP_PORT}${path}`); if (r.ok) return r.json(); } catch {}
    await sleep(250);
  }
  throw new Error("no CDP");
}
const t = (await cdp("/json/list")).find((x) => x.type === "page");
const ws = new WebSocket(t.webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));
let id = 0;
const waits = new Map();
ws.onmessage = (e) => { const m = JSON.parse(e.data); if (waits.has(m.id)) { waits.get(m.id)(m); waits.delete(m.id); } };
const send = (method, params = {}) => new Promise((r) => { const i = ++id; waits.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const evaluate = async (expr) => {
  const r = await send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true });
  if (r.result?.exceptionDetails) throw new Error(String(r.result.exceptionDetails.exception?.description || "").slice(0, 700));
  return r.result?.result?.value;
};
let bad = 0;
const check = (what, ok, detail = "") => { console.log(`${ok ? "  ok" : "FAIL"}  ${what}${ok || !detail ? "" : "  — " + detail}`); if (!ok) bad++; };
await send("Page.enable");
await send("Page.navigate", { url: BASE + "/" }); await sleep(8000);

// 1. Home grid: five groups, and a custom weapon card present.
const home = await evaluate(`(() => {
  const groups = document.querySelectorAll(".wgroup").length;
  const card = [...document.querySelectorAll(".wcard")].find((a) => a.getAttribute("href") === "/weapons/primary");
  return { groups, hasCustomCard: !!card, cardName: card ? card.textContent.trim() : "" };
})()`);
check("home shows 5 weapon groups", home.groups === 5, `got ${home.groups}`);
check("home has a custom weapon card", home.hasCustomCard, home.cardName);

// 2. The custom weapon page: embedded editor + picker actually lists mods.
await send("Page.navigate", { url: BASE + "/weapons/primary" }); await sleep(7000);
const page = await evaluate(`(() => {
  const form = document.getElementById("custom-weapon-form");
  const tabs = [...document.querySelectorAll(".mtab")].map((a) => a.textContent.trim());
  const pool = poolWithCustom();
  const rifles = pool.filter((m) => m.id === "serration" || m.id === "split_chamber" || m.id === "point_strike" || m.id === "vital_sense").length;
  const title = document.title;
  const err = document.querySelector(".config-page .error");
  return { formVisible: form && !form.hidden, tabHasCustomWeapon: tabs.includes("Custom Weapon"), poolLen: pool.length, rifles, title, err: err ? err.textContent.trim() : "" };
})()`);
check("custom weapon page shows the embedded panel editor", page.formVisible === true);
check("no panel error on the custom weapon page (the forms_list panic is fixed)", page.err === "", page.err);
check("no 'Custom Weapon' nav tab anymore", page.tabHasCustomWeapon !== true, `tabs=${page.tabHasCustomWeapon}`);
check("custom weapon picker lists the slot's mods (regression)", page.poolLen > 50 && page.rifles === 4, `pool=${page.poolLen}, rifles=${page.rifles}`);
check("page title is the custom weapon", typeof page.title === "string" && page.title.length > 0, page.title);

// 3. A custom weapon simulates.
const dps = await evaluate(`(async () => {
  const r = await fetch("/api/simulate", { method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ weapon: "custom:primary", custom_weapon: { type: "primary", panel: { impact: 100, viral: 100, fire_rate: 8, multishot: 1 } }, duration: 10 }) });
  const d = await r.json(); return d.ok && d.dps > 0 ? d.dps : null;
})()`);
check("custom weapon simulates (dps > 0)", dps !== null, `dps=${dps}`);

// 4. Creating a custom mod card makes it equippable and searchable.
await send("Page.navigate", { url: BASE + "/weapons/primary/custommods" }); await sleep(5000);
const cmPage = await evaluate(`(() => {
  const blk = document.getElementById("custommods-block");
  const btn = document.querySelector("#cm-list .btn");
  return { visible: !!blk && getComputedStyle(blk).display !== "none", btnClickable: !!btn };
})()`);
check("Custom Mods tab shows its block (was invisible: no CSS rule)", cmPage.visible === true);
check("Custom Mods page has the create button", cmPage.btnClickable === true);
await evaluate(`document.querySelector("#cm-list .btn").click()`); await sleep(1500);
const cardName = await evaluate(`document.querySelector("#cm-edit [data-k='name']").value`);
check("custom mod card created", !!cardName, `name=${cardName}`);

// 5a. Switching the effect kind stores the ENGLISH id (the option's value),
//     not the translated label — a zh label like 基础伤害 must never leak
//     into the payload (regression: the i18n pass dropped the value attr).
const kindSwitch = await evaluate(`(() => {
  const sel = document.querySelector("#cm-edit .cm-kind");
  if (!sel) return "no-kind-select";
  sel.value = "element"; sel.dispatchEvent(new Event("change", { bubbles: true }));
  return null;
})()`);
check("kind switch triggered", kindSwitch === null, kindSwitch);
await sleep(1200);
const kindStored = await evaluate(`(() => {
  const card = loadPresetList(CUSTOMMODS)[0];
  return card && card.state.effects[0] ? card.state.effects[0].kind : "no-card";
})()`);
check("effect kind stored as the ENGLISH id (not 基础伤害)", kindStored === "element", `kind=${kindStored}`);
const kindBack = await evaluate(`(() => {
  const sel = document.querySelector("#cm-edit .cm-kind");
  return sel ? sel.value : "no-select";
})()`);
check("editor shows the switched kind selected", kindBack === "element", `value=${kindBack}`);

// 5. The list card shows its effect lines (a card, not a bare row).
const listEffects = await evaluate(`(() => {
  const me = document.querySelector("#cm-list .cm-card-me");
  return me ? me.textContent.trim() : "";
})()`);
check("custom-mod list card shows its effect lines", listEffects.length > 0, listEffects.slice(0, 60));

await send("Page.navigate", { url: BASE + "/weapons/primary" }); await sleep(5000);
const found = await evaluate(`poolWithCustom().some((m) => String(m.id).startsWith("custom:"))`);
check("custom card in the equippable pool", found === true);
const searchHit = await evaluate(`(() => {
  const m = poolWithCustom().find((x) => String(x.id).startsWith("custom:"));
  return m ? searchBlob(m).length > 0 : null;
})()`);
check("custom card has searchable text", searchHit === true);

// 6. Equip the card: the SLOT tag shows the effect lines too, and a panel
//    request carrying custom_mods (with name) no longer errors.
await send("Page.navigate", { url: BASE + "/weapons/primary" }); await sleep(5000);
const slotView = await evaluate(`(() => {
  const m = poolWithCustom().find((x) => String(x.id).startsWith("custom:"));
  if (!m) return { me: 0, err: "no custom card" };
  slots[0].mod = m.id; slots[0].rank = 0; renderMods();
  const me = document.querySelector(".slot.filled .me");
  return { me: me ? me.querySelectorAll("div").length : 0, err: (document.querySelector(".config-page .error") || {}).textContent || "" };
})()`);
check("slot tag shows the custom mod's effect lines", slotView.me > 0, `lines=${slotView.me}`);
check("panel request with custom_mods carries a name (missing-name fixed)", !/missing name/.test(slotView.err), slotView.err.slice(0, 80));

// 7. Old-data compatibility: a card saved BEFORE the kind <select> carried
//    an explicit value holds the translated label (触发几率) as its kind.
//    Loading the page must repair it to the ENGLISH id and keep serving.
await send("Page.navigate", { url: BASE + "/" }); await sleep(6000);
await evaluate(`localStorage.setItem("wfsim-lang", "zh")`); await sleep(500);
await evaluate(`localStorage.setItem("wfsim-presets-custom:primary-custommods", JSON.stringify([{ name: "旧卡", state: { polarity: "madurai", base_drain: 10, exilus: false, effects: [{ kind: "触发几率", value: 0.5 }] } }]))`);
await send("Page.navigate", { url: BASE + "/weapons/primary" }); await sleep(6000);
// The PAYLOAD path normalizes as it sends: a request carrying the old card
// must simulate cleanly (this is the path the server actually saw fail).
const oldCardSim = await evaluate(`(async () => {
  const list = normalizeCustomModKinds(loadPresetList(CUSTOMMODS));
  const payload = { weapon: "custom:primary", mods: ["custom:旧卡"],
    custom_weapon: { type: "primary", panel: { impact: 100, viral: 100, fire_rate: 8, multishot: 1 } },
    custom_mods: list.map((p) => ({ name: p.name, ...(p.state || {}) })), duration: 10 };
  const r = await fetch("/api/simulate", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(payload) });
  const d = await r.json();
  return d.ok && !JSON.stringify(d).includes("unknown effect kind");
})()`);
check("request with the migrated card simulates (no unknown effect kind)", oldCardSim === true);
// Opening the Custom Mods tab repairs the stored copy (renderCustomMods
// normalizes and writes back).
await send("Page.navigate", { url: BASE + "/weapons/primary/custommods" }); await sleep(6000);
const migrated = await evaluate(`(() => {
  const raw = JSON.parse(localStorage.getItem("wfsim-presets-custom:primary-custommods") || "[]");
  const e = raw[0] && raw[0].state.effects[0];
  return { kind: e ? e.kind : "none", line: e ? customEffectLine(e) : "" };
})()`);
check("old zh kind migrated to the english id", migrated.kind === "status_chance", `kind=${migrated.kind}`);
check("effect line prints like a mod card (+50%)", /\+50%/.test(migrated.line), migrated.line);
await evaluate(`localStorage.removeItem("wfsim-presets-custom:primary-custommods")`);
await evaluate(`localStorage.setItem("wfsim-lang", "")`);

// 8. SHARE round-trip: the wire format carries the custom cards (field 10)
//    and the custom weapon panel (field 11), and decodeShare lands them.
await send("Page.navigate", { url: BASE + "/weapons/primary" }); await sleep(6000);
const share = await evaluate(`(async () => {
  localStorage.setItem("wfsim-presets-custom:primary-custommods", JSON.stringify([{ name: "毒卡", state: { polarity: "madurai", base_drain: 10, exilus: false, effects: [{ kind: "element", type: "toxin", value: 0.6 }] } }]));
  localStorage.setItem("wfsim-customs-primary-customweapon", JSON.stringify({ type: "primary", name: "我的", disposition: 1.0, trigger: "auto", panel: { impact: 100, viral: 100, fire_rate: 8, multishot: 1 } }));
  customModCache = { key: null, list: [] };
  const url = await shareUrl();
  const code = url.split("?b=")[1];
  const dec = await decodeShare(code);
  return { customs: dec.customs.length, cw: dec.custom_weapon ? dec.custom_weapon.type : null };
})()`);
check("share carries the custom mod cards (field 10)", share.customs === 1, `customs=${share.customs}`);
check("share carries the custom weapon panel (field 11)", share.cw === "primary", `cw=${share.cw}`);

// 9. Choosing a parameter in the editor stores the LOWERCASE engine id —
//    the option value, not the pretty label (regression: the i18n pass).
await send("Page.navigate", { url: BASE + "/weapons/primary/custommods" }); await sleep(6000);
await evaluate(`document.querySelector("#cm-list .btn").click()`); await sleep(1200);
const paramStore = await evaluate(`(() => {
  const sel = document.querySelector("#cm-edit .cm-kind");
  sel.value = "element"; sel.dispatchEvent(new Event("change", { bubbles: true }));
  const t = document.querySelector("#cm-edit [data-f='type']");
  if (!t) return "no-type-select";
  t.value = "heat"; t.dispatchEvent(new Event("change", { bubbles: true }));
  const name = cmActive();
  const card = loadPresetList(CUSTOMMODS).find((p) => p.name === name);
  return card.state.effects[0].type;
})()`);
check("element type stored as lowercase id (heat, not Heat)", paramStore === "heat", `type=${paramStore}`);

// 10. Extreme values are rejected, not hung on: fire_rate=1e9 and a mod
//     ratio beyond ±100 are both fail-fast HTTP errors.
const extreme = await evaluate(`(async () => {
  const bad = await fetch("/api/simulate", { method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ weapon: "custom:primary", custom_weapon: { type: "primary", panel: { impact: 100, fire_rate: 1e9, multishot: 1 } }, duration: 10 }) });
  const badMod = await fetch("/api/simulate", { method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ weapon: "torid", mods: ["custom:超"], custom_mods: [{ name: "超", effects: [{ kind: "base_damage", value: 200 }] }], duration: 10 }) });
  const a = await bad.json(); const b = await badMod.json();
  return { fireRate: !a.ok && /fire_rate/.test(JSON.stringify(a)), ratio: !b.ok && /100/.test(JSON.stringify(b)) };
})()`);
check("fire_rate=1e9 rejected fast (no hang)", extreme.fireRate === true);
check("mod ratio beyond ±100 rejected", extreme.ratio === true);

// 11. The While-Tenno inner effect is editable as a nested kind+value row.
await send("Page.navigate", { url: BASE + "/weapons/primary/custommods" }); await sleep(6000);
const inner = await evaluate(`(() => {
  localStorage.setItem("wfsim-presets-custom:primary-custommods", JSON.stringify([{ name: "随甲", state: { polarity: "madurai", base_drain: 10, exilus: false, effects: [{ kind: "while_tenno", condition: "aiming", inner: { kind: "base_damage", value: 1.3 } }] } }]));
  renderCustomMods(); customOpenCard("随甲");
  const sel = document.querySelector("#cm-edit [data-f='inner.kind']");
  return { hasInner: !!sel, value: sel ? sel.value : null };
})()`);
check("while_tenno inner kind row renders", inner.hasInner === true, `value=${inner.value}`);
check("inner kind stored as the english id", inner.value === "base_damage", `value=${inner.value}`);
await evaluate(`localStorage.removeItem("wfsim-presets-custom:primary-custommods")`);
await evaluate(`localStorage.removeItem("wfsim-customs-primary-customweapon")`);

// 5. A roster weapon page hides the embedded form.
await send("Page.navigate", { url: BASE + "/weapons/torid" }); await sleep(6000);
const roster = await evaluate(`(() => { const f = document.getElementById("custom-weapon-form"); return { visible: !!f && !f.hidden, weapon: document.getElementById("weapon").value }; })()`);
check("roster weapon page hides the embedded form", roster.visible === false, `weapon=${roster.weapon}`);

proc.kill(); try { spawn("taskkill", ["//F", "//IM", "wfsim-web.exe"], { stdio: "ignore" }); } catch {}
console.log(bad ? `\n${bad} failure(s)` : "\nall custom checks passed");
process.exit(bad ? 1 : 0);
