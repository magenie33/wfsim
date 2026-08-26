// THE DOWNLOAD OFFER ANSWERS THE MACHINE ASKING, and has four answers.
//
// The home page offers the desktop build, and what it should say depends on
// where it is read: a platform we build for gets its own button, a desktop we
// do NOT build for is told so instead of being shown an executable it cannot
// run, and a phone is shown nothing at all — it is already running the thing a
// download would install.
//
// Only ONE of those four is visible on the machine this is written on, so
// checking by looking is checking a quarter of it. Worse, three of the four
// failure modes are silent and plausible: a Mac reader shown `WFSim.exe`, a
// phone reader shown a 34 MB download, a Linux reader shown Quark (which has
// no Linux build in it). Each looks like a working page.
//
// IT ALSO PINS THE ORDER OF SOURCES, because that is the whole reason the table
// is two levels deep (owner, 2026-08-26): which PLATFORM someone needs is
// decided by their machine, which SOURCE is best is decided by where they are,
// and Quark leads on Windows precisely because the readers this project is
// mostly for are the ones GitHub is slowest for.
import { openApp } from "./cdp.mjs";

const app = await openApp({ base: process.argv[2] });
const { check } = app;

const UAS = {
  Windows: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36",
  Linux: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36",
  macOS: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36",
  Android: "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Mobile Safari/537.36",
};

/// What the offer looks like to one machine. Read from the rendered DOM rather
/// than from the table it was built from — the table being right is not the
/// claim, the page being right is.
const READ = `(() => {
  const host = document.getElementById("hero-dl");
  if (!host) return { missing: true };
  const btn = host.querySelector(".dl-btn");
  const links = [...host.querySelectorAll("a")].map((a) => a.href);
  return {
    hidden: !!host.hidden,
    button: btn ? btn.textContent.trim() : null,
    buttonHref: btn ? btn.href : null,
    links,
    rows: [...host.querySelectorAll(".dl-row .dl-os")].map((e) => e.textContent.trim()),
    text: (host.innerText || "").replace(/\\s+/g, " ").trim(),
  };
})()`;

const seen = {};
for (const [name, ua] of Object.entries(UAS)) {
  // Before the load, so the page reads it during its own boot rather than
  // being told afterwards and never asked again.
  await app.send("Emulation.setUserAgentOverride", { userAgent: ua });
  await app.load("/");
  seen[name] = await app.evaluate(READ);
}

const has = (v, s) => (v.links || []).some((u) => u.includes(s));

// ---- 1. A PHONE IS SHOWN NOTHING ----------------------------------------
// Not "a smaller button": an executable a phone cannot execute is noise on the
// one screen with the least room for it, and the reader is already using the
// thing the download would give them.
check("a phone is offered nothing at all",
  seen.Android.hidden === true,
  JSON.stringify(seen.Android).slice(0, 160));

// ---- 2. EACH BUILT PLATFORM GETS ITS OWN BUTTON --------------------------
check("Windows is offered Windows",
  /Windows/.test(seen.Windows.button || ""),
  `button ${JSON.stringify(seen.Windows.button)}`);
check("...and its first source is Quark, not GitHub",
  (seen.Windows.buttonHref || "").includes("pan.quark.cn"),
  seen.Windows.buttonHref || "(no button)");

check("Linux is offered Linux",
  /Linux/.test(seen.Linux.button || ""),
  `button ${JSON.stringify(seen.Linux.button)}`);
check("...and its source is the AppImage",
  (seen.Linux.buttonHref || "").endsWith("WFSim.AppImage"),
  seen.Linux.buttonHref || "(no button)");

// ---- 3. THE PLATFORM IT IS NOT STILL REACHABLE ---------------------------
// Somebody downloading for another machine is being deliberate; the other rows
// are listed rather than hidden behind a control they would have to find.
check("Windows can still reach the Linux build",
  has(seen.Windows, "WFSim.AppImage") && seen.Windows.rows.includes("Linux"),
  `rows ${JSON.stringify(seen.Windows.rows)}`);
check("Linux can still reach the Windows build",
  has(seen.Linux, "pan.quark.cn") && seen.Linux.rows.includes("Windows"),
  `rows ${JSON.stringify(seen.Linux.rows)}`);

// ---- 4. A PLATFORM WE DO NOT BUILD FOR IS TOLD SO ------------------------
// The negative control, and the one a check that only tested Windows would
// pass while the page handed a Mac reader an .exe. Warframe has no macOS
// client either, so this is rarely anyone — which is exactly why it would go
// unnoticed.
check("macOS is offered no button",
  seen.macOS.button === null && seen.macOS.hidden === false,
  `button ${JSON.stringify(seen.macOS.button)}, hidden ${seen.macOS.hidden}`);
check("...and is told which platforms exist",
  seen.macOS.rows.includes("Windows") && seen.macOS.rows.includes("Linux"),
  `rows ${JSON.stringify(seen.macOS.rows)}`);

// ---- 5. EVERY SOURCE IN THE TABLE IS REACHABLE FROM SOMEWHERE ------------
// A source added to `DOWNLOADS` and rendered by nothing is the failure this
// two-level shape exists to prevent, and it would look exactly like a working
// page from whichever machine the author happened to test on.
const everywhere = [...new Set([...seen.Windows.links, ...seen.Linux.links, ...seen.macOS.links])];
check("every configured source appears on the page",
  everywhere.some((u) => u.includes("pan.quark.cn"))
    && everywhere.some((u) => u.endsWith("WFSim.exe"))
    && everywhere.some((u) => u.endsWith("WFSim.AppImage")),
  everywhere.join(" "));

await app.finish("the download offer answers the machine asking, on all four");
