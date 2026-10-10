/// AN OLD SHELL IS TOLD A NEW DOWNLOAD EXISTS (docs/DESKTOP.md §A new shell is
/// offered by the content). The window is made to look like the client — the
/// globals the shell declares before boot and a stub of its updater commands —
/// and the build it states is varied: older than `SHELL_MINIMUM` and too old to
/// state one are both offered the download, in the corner and on /download; the
/// current shell is offered nothing; and a notice closed stays closed.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000, lang: "en", base: process.env.WFSIM_BASE });
const { evaluate, check, send, sleep } = app;

// THE SHELL'S OWN PREAMBLE, with the build read from a test-only key so each
// load below can state a different one.
await send("Page.addScriptToEvaluateOnNewDocument", { source: `
  window.__WFSIM_DESKTOP__ = true;
  const shell = localStorage.getItem("__test_shell");
  if (shell !== "none") window.__WFSIM_SHELL__ = shell;
  window.__TAURI_INTERNALS__ = { invoke: async (cmd) => cmd === "update_status"
    ? { phase: "uptodate", version: "", files_done: 0, files_total: 0, bytes_done: 0, bytes_total: 0, message: "" }
    : null };` });

const as = async (shell, path = "/") => {
  await evaluate(`localStorage.setItem("__test_shell", ${JSON.stringify(shell)})`);
  await app.load(path, 12000);
  await sleep(9500);
  return evaluate(`(() => {
    const bar = document.querySelector(".dtup-shell");
    const offer = document.getElementById("dl-offer");
    return { bar: !!bar, href: bar ? bar.querySelector("a").href : "",
      offer: offer ? offer.innerHTML : null };
  })()`);
};

await app.load("/", 12000);
await evaluate(`localStorage.removeItem("wfsim-shell-dismissed")`);
const old = await as("2026.9.20 52e99d11");
check("a shell older than the minimum is offered the download", old.bar && /pan\.quark\.cn/.test(old.href), JSON.stringify(old));
const bare = await as("none");
check("...and so is one too old to state its build", bare.bar, JSON.stringify(bare));
const now = await as("2026.10.10 7f87e1da");
check("the current shell is offered nothing", !now.bar, JSON.stringify(now));

const dlOld = await as("2026.9.20 52e99d11", "/download");
check("/download in an old shell offers the file", /dl-btn/.test(dlOld.offer || ""), dlOld.offer);
const dlNow = await as("2026.10.10 7f87e1da", "/download");
check("/download in the current shell says there is nothing to install",
  !/dl-btn/.test(dlNow.offer || "") && /nothing to download/.test(dlNow.offer || ""), dlNow.offer);

await as("2026.9.20 52e99d11");
await evaluate(`document.querySelector(".dtup-shell .dtup-x").click()`);
const closed = await as("2026.9.20 52e99d11");
check("a notice closed stays closed", !closed.bar, JSON.stringify(closed));

await app.finish("an old shell hears that a new one is out, and only an old one");
