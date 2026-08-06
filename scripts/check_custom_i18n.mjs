// I18N CHECK — the custom editors follow the language selector.
//   node scripts/check_custom_i18n.mjs
// Loads the custom weapon page + the custom mods page under zh and under en,
// and asserts the editor labels/kind names switch language with the page.
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve } from "node:path";
const EXE = resolve("target/debug/wfsim-web.exe");
const PORT = 8787;
const CDP_PORT = 9532;
const BASE = `http://127.0.0.1:${PORT}`;
const srv = spawn(EXE, [], { stdio: "ignore" });
const BIN = ["C:/Program Files/Google/Chrome/Application/chrome.exe", "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe", "C:/Program Files/Microsoft/Edge/Application/msedge.exe"].find((p) => existsSync(p));
if (!BIN) { console.error("FAIL  no Chrome/Edge"); process.exit(1); }
const proc = spawn(BIN, [`--remote-debugging-port=${CDP_PORT}`, "--headless=new", "--remote-allow-origins=*", "--disable-gpu", "--no-first-run", `--user-data-dir=${process.env.TEMP}/wfsim-i18n-${Date.now()}`, "about:blank"], { stdio: "ignore" });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function cdp(path) { for (let i = 0; i < 60; i++) { try { const r = await fetch(`http://127.0.0.1:${CDP_PORT}${path}`); if (r.ok) return r.json(); } catch {} await sleep(250); } throw new Error("no CDP"); }
const t = (await cdp("/json/list")).find((x) => x.type === "page");
const ws = new WebSocket(t.webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));
let id = 0; const waits = new Map();
ws.onmessage = (e) => { const m = JSON.parse(e.data); if (waits.has(m.id)) { waits.get(m.id)(m); waits.delete(m.id); } };
const send = (method, params = {}) => new Promise((r) => { const i = ++id; waits.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const evaluate = async (expr) => { const r = await send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true }); if (r.result?.exceptionDetails) throw new Error(String(r.result.exceptionDetails.exception?.description || "").slice(0, 600)); return r.result?.result?.value; };
let bad = 0;
const check = (what, ok, detail = "") => { console.log(`${ok ? "  ok" : "FAIL"}  ${what}${ok || !detail ? "" : "  — " + detail}`); if (!ok) bad++; };
await send("Page.enable");

async function snapshot() {
  // The custom weapon panel form labels + the first custom-mod effect kind.
  return evaluate(`(() => {
    const labels = [...document.querySelectorAll("#custom-weapon-form .cu-param span")].map((s) => s.textContent.trim());
    const kindSel = document.querySelector("#cm-edit .cm-kind");
    const kindOpts = kindSel ? [...kindSel.options].map((o) => o.textContent.trim()) : [];
    return { labels: labels.slice(0, 30), kindOpts: kindOpts.slice(0, 12), selKind: kindSel ? kindSel.value : null };
  })()`);
}

// zh: fields in Chinese (transcribed official terms).
await send("Page.navigate", { url: BASE + "/" }); await sleep(6000);
await evaluate(`localStorage.setItem("wfsim-lang", "zh")`); await sleep(500);
await send("Page.navigate", { url: BASE + "/weapons/primary" }); await sleep(7000);
const zh = await snapshot();
const zhHasCJK = zh.labels.join("").length > 0 && [...zh.labels.join("")].some((c) => c.charCodeAt(0) > 0x2e80);
check("zh: custom weapon panel fields are Chinese", zhHasCJK, zh.labels.slice(0, 6).join(" | "));
check("zh: Impact shows 冲击", zh.labels.includes("冲击"));
check("zh: Viral shows 病毒", zh.labels.includes("病毒"));
check("zh: Crit Chance shows 暴击几率", zh.labels.includes("暴击几率"));
check("zh: Type shows 类型", zh.labels.includes("类型"));
await send("Page.navigate", { url: BASE + "/weapons/primary/custommods" }); await sleep(6000);
await evaluate(`document.querySelector("#cm-list .btn").click()`); await sleep(1500);
const zhKinds = (await snapshot()).kindOpts;
check("zh: custom mod kind names are Chinese", zhKinds.some((k) => [...k].some((c) => c.charCodeAt(0) > 0x2e80)), zhKinds.slice(0, 4).join(" | "));
check("zh: Base Damage shows 基础伤害", zhKinds.includes("基础伤害"));
check("zh: Multishot shows 多重射击", zhKinds.includes("多重射击"));

// en: back to English.
await send("Page.navigate", { url: BASE + "/" }); await sleep(6000);
await evaluate(`localStorage.setItem("wfsim-lang", "en")`); await sleep(500);
await send("Page.navigate", { url: BASE + "/weapons/primary" }); await sleep(7000);
const en = await snapshot();
check("en: custom weapon panel fields back to English", en.labels.includes("Impact") && en.labels.includes("Viral") && en.labels.includes("Crit Chance"), en.labels.slice(0, 6).join(" | "));
await send("Page.navigate", { url: BASE + "/weapons/primary/custommods" }); await sleep(6000);
await evaluate(`document.querySelector("#cm-list .btn").click()`); await sleep(1500);
const enKinds = (await snapshot()).kindOpts;
check("en: custom mod kind names back to English", enKinds.includes("Base Damage") && enKinds.includes("Multishot"), enKinds.slice(0, 4).join(" | "));

proc.kill(); try { spawn("taskkill", ["//F", "//IM", "wfsim-web.exe"], { stdio: "ignore" }); } catch {}
console.log(bad ? `\n${bad} failure(s)` : "\nall i18n checks passed");
process.exit(bad ? 1 : 0);
