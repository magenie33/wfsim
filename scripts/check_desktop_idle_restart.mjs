/// A READY UPDATE IS APPLIED BY ITSELF AS SOON AS IT LOSES NOTHING (docs/DESKTOP.md
/// §"The content update"). The window is made to look like the client, its
/// updater stubbed to say an update is ready, and every `update_apply` written
/// down where a reload keeps it: none while the window was touched lately, none
/// while a row is mid-measure, a field has focus or a panel holds the page, and
/// one once none of these is so.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000, lang: "en", base: process.env.WFSIM_BASE });
const { evaluate, check, send, sleep } = app;

await send("Page.addScriptToEvaluateOnNewDocument", { source: `
  window.__WFSIM_DESKTOP__ = true;
  window.__WFSIM_SHELL__ = "2026.10.12 7f87e1da";
  window.__TAURI_INTERNALS__ = { invoke: async (cmd) => {
    if (cmd === "update_status") return { phase: "ready", version: "v2", files_done: 0, files_total: 0, bytes_done: 0, bytes_total: 0, message: "" };
    if (cmd === "app_version") return "v1";
    if (cmd === "update_apply") localStorage.setItem("__test_applied", String(Number(localStorage.getItem("__test_applied") || 0) + 1));
    return null;
  } };` });

const applied = () => evaluate(`Number(localStorage.getItem("__test_applied") || 0)`);
// LONGER THAN ONE LOOK OF `restartWhenIdle`, so each case below was looked at.
const LOOK = 35_000;

await app.load("/", 12000);
await evaluate(`localStorage.removeItem("__test_applied")`);
await sleep(LOOK);
check("a window touched lately is not restarted", await applied() === 0);

// QUIET LONGER THAN `RELOAD_QUIET_MS`, re-set before each case: the window
// cannot be touched while the check waits, but say so rather than rely on it.
const quiet = `lastTouched = Date.now() - RELOAD_QUIET_MS - 60_000;`;
await evaluate(`(() => { ${quiet} computeNow = { kind: "board", done: 1, total: 10, started: Date.now() }; })()`);
await sleep(LOOK);
check("a window idle but measuring a row is not restarted", await applied() === 0);

await evaluate(`(() => { ${quiet} computeNow = null; const i = document.createElement("input"); i.id = "__test_field"; document.body.append(i); i.focus(); })()`);
await sleep(LOOK);
check("a window idle with a field in focus is not restarted", await applied() === 0);

await evaluate(`(() => { ${quiet} document.getElementById("__test_field").remove(); window.__test_release = window.wfsim.hold(); })()`);
await sleep(LOOK);
check("a window a panel holds is not restarted", await applied() === 0);

await evaluate(`(() => { ${quiet} window.__test_release(); })()`);
await sleep(LOOK);
check("a window that would lose nothing applies the ready update by itself", await applied() === 1, String(await applied()));

await app.finish("a ready update waits until a reload loses nothing, and then applies itself");
