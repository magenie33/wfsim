// THE DOWNLOAD OFFER ANSWERS THE MACHINE ASKING, AND ITS PAGE ANSWERS THE
// QUESTIONS.
//
// The offer is TWO SURFACES with one function behind each. The home hero is a
// POINTER — one line, because that page is read by someone who has not seen the
// tool work, the worst moment to ask them to run an unsigned executable.
// `/download` is the offer itself, and it is a page rather than a button
// because what it has to answer is a page: SmartScreen, why the program is
// unsigned, what updating costs, what uninstalling means, where the source is.
//
// Only ONE machine's answer is visible on the machine this is written on, so
// checking by looking is checking a third of it, and the failure modes are
// silent and plausible: a Mac reader handed an .exe, a phone reader handed a
// 34 MB download, a source badge that draws an icon and names nothing.
//
// THE PAGE IS PRERENDERED, which is the half no browser assertion can see. A
// URL people paste and read off a video must carry its own title, description
// and canonical; without them it previews as the app's own headline, and a link
// that says "WFSim — Warframe Calculator" and opens on an executable download
// is the kind of mismatch that reads as a scam.
import { readFileSync } from "node:fs";
import { openApp } from "./cdp.mjs";

const app = await openApp({ base: process.argv[2] });
const { check } = app;

const UAS = {
  Windows: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36",
  Linux: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36",
  macOS: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36",
  Android: "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Mobile Safari/537.36",
};

/// WHAT THE HOME PAGE SAYS ABOUT THE DOWNLOAD, which is: nothing, anywhere but
/// the topbar's overflow panel. The home page is read by somebody who has not
/// seen the tool work yet, and that is the worst moment to put an unsigned
/// executable in front of them — so the entry lives behind the overflow menu,
/// where a reader who wants the client goes looking for it.
const READ = `(() => {
  const main = document.querySelector("main:not([hidden])");
  const panel = document.querySelector(".tbmore, .tbmore-panel, #tbmore") || document.body;
  const dl = panel.querySelector("a.dl-link");
  return {
    // Every link the VISIBLE page offers, so a download entry anywhere outside
    // the panel shows up here whatever it is called.
    mainLinks: main ? [...main.querySelectorAll("a")].map((a) => a.getAttribute("href") || "") : [],
    mainText: main ? main.innerText || "" : "",
    panelHref: dl ? dl.getAttribute("href") : null,
    panelText: dl ? dl.textContent.trim() : null,
  };
})()`;

/// The offer as /download draws it, plus everything the page says.
const READ_PAGE = `(() => {
  const host = document.getElementById("dl-offer");
  const page = document.getElementById("download-page");
  const btn = host && host.querySelector(".dl-btn");
  const src = host && host.querySelector(".dl-src");
  return {
    drawn: !!(page && !page.hidden),
    button: btn ? btn.textContent.trim() : null,
    buttonHref: btn ? btn.href : null,
    srcName: src ? src.textContent.trim() : null,
    srcHref: src ? src.href : null,
    srcIcon: !!(src && src.querySelector("svg")),
    // THE ELEMENT, NOT THE SENTENCE. This file runs in whatever language the
    // browser reports, so grepping the wording asserts a translation instead of
    // the behaviour. The dl-why element IS "you cannot run this here".
    why: !!(host && host.querySelector(".dl-why")),
    text: page ? page.innerText : "",
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

// The page itself, read as the machine it is for.
await app.send("Emulation.setUserAgentOverride", { userAgent: UAS.Windows });
await app.load("/download");
const page = await app.evaluate(READ_PAGE);
check("/download draws its page", page.drawn === true, JSON.stringify(page).slice(0, 160));

// ---- 1. THE HOME PAGE OFFERS NOTHING, AND THE MENU DOES -----------------
// The rule this file exists to hold: no download entry on the page itself, on
// ANY machine, and exactly one behind the overflow menu.
for (const os of Object.keys(UAS)) {
  check(`${os} is offered no download on the home page`,
    !(seen[os].mainLinks || []).some((h) => /\/download|pan\.quark\.cn/.test(h)),
    JSON.stringify((seen[os].mainLinks || []).filter((h) => /download|quark/.test(h))));
}
check("...and the overflow menu carries the one entry",
  (seen.Windows.panelHref || "") === "/download",
  `panel ${JSON.stringify(seen.Windows.panelText)} -> ${seen.Windows.panelHref}`);

// ---- 2. THE PAGE CARRIES THE OFFER --------------------------------------
check("the page offers Windows",
  /Windows/.test(page.button || ""),
  `button ${JSON.stringify(page.button)}`);
check("...going to the network drive, not GitHub",
  (page.buttonHref || "").includes("pan.quark.cn"),
  page.buttonHref || "(no button)");

// THE SOURCE IS NAMED BESIDE IT. The NAME is the assertion, not the element:
// an icon alone identifies nothing, which is the whole failure this prevents.
check("...and names the drive the file is on",
  (page.srcName || "").includes("夸克网盘"),
  `badge ${JSON.stringify(page.srcName)}`);
check("...with a mark, pointing at the same place",
  page.srcIcon === true && (page.srcHref || "") === (page.buttonHref || "x"),
  `icon ${page.srcIcon}, href ${page.srcHref}`);

// ---- 3. AND IT ANSWERS THE QUESTIONS ------------------------------------
// The reason this is a page. Each of these is a real question a downloader
// asks, and a page carrying the button and none of them is the button with
// more scrolling.
for (const [what, needle] of [
  ["what SmartScreen does", /SmartScreen|protected your PC|已保护你的电脑/],
  ["why it is unsigned", /code-signing|代码签名/],
  ["how updating works", /updates itself|自己更新/],
  ["how to uninstall", /LOCALAPPDATA/],
  ["where the source is", /AGPL/],
]) {
  check(`the page says ${what}`, needle.test(page.text || ""),
    (page.text || "").slice(0, 100));
}

// ---- 4. A PLATFORM WE DO NOT BUILD FOR IS TOLD SO, ON THE PAGE ----------
// The negative control, and it moved WITH the offer: the home page no longer
// reads the user agent at all, so /download is the one surface that can say a
// Mac or Linux reader cannot run this. A page that handed them the button in
// silence is the failure.
for (const os of ["macOS", "Linux", "Android"]) {
  await app.send("Emulation.setUserAgentOverride", { userAgent: UAS[os] });
  await app.load("/download");
  const p = await app.evaluate(READ_PAGE);
  check(`${os} is told the desktop build is Windows`,
    /Windows/.test(p.text || ""), (p.text || "").slice(0, 120));
  check(`...and ${os} is told it will not run there`,
    p.why === true, `dl-why present: ${p.why}`);
}

await app.finish("the download offer answers the machine asking, and its page answers the questions");
