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
// that says "Ultimate Warframe Calculator" and opens on an executable download
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

/// What the offer looks like to one machine. Read from the rendered DOM rather
/// than from the table it was built from — the table being right is not the
/// claim, the page being right is.
const READ = `(() => {
  const host = document.getElementById("hero-dl");
  if (!host) return { missing: true };
  const btn = host.querySelector(".dl-btn");
  const src = host.querySelector(".dl-src");
  return {
    hidden: !!host.hidden,
    button: btn ? btn.textContent.trim() : null,
    buttonHref: btn ? btn.href : null,
    srcName: src ? src.textContent.trim() : null,
    srcHref: src ? src.href : null,
    srcIcon: !!(src && src.querySelector("svg")),
    heroText: host.querySelector(".dl-line")
      ? host.querySelector(".dl-line").textContent.trim() : null,
    heroHref: host.querySelector(".dl-line")
      ? host.querySelector(".dl-line").getAttribute("href") : null,
    links: [...host.querySelectorAll("a")].map((a) => a.href),
    // NOT NORMALISED, deliberately. A whitespace regex here sits inside a
    // TEMPLATE LITERAL, which eats the backslash — the page then runs /s+/g
    // and replaces every letter s in the sentence with a space, so the
    // assertion below read "only supports Window ." and failed on a correct
    // page. It only greps this string, so there is nothing to normalise for.
    text: host.innerText || "",
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

// ---- 1. THE HOME HERO IS A POINTER ------------------------------------
// One line to the page, not the offer. A button here would be the offer in two
// places, and the one on the hero could not carry the answers.
check("Windows is pointed at the download page",
  (seen.Windows.heroHref || "").endsWith("/download"),
  `hero ${JSON.stringify(seen.Windows.heroText)} -> ${seen.Windows.heroHref}`);
check("...and the hero holds no download button",
  seen.Windows.button === null,
  `button ${JSON.stringify(seen.Windows.button)}`);

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

// ---- 4. A PLATFORM WE DO NOT BUILD FOR IS TOLD SO ------------------------
// The negative control, and the one a check that only tested Windows would
// pass while the page handed a Mac or Linux reader an .exe.
for (const os of ["macOS", "Linux"]) {
  check(`${os} is offered no download on the home page`,
    !(seen[os].links || []).some((u) => /pan\.quark\.cn/.test(u)),
    JSON.stringify(seen[os].links));
  check(`...and ${os} is told the desktop build is Windows`,
    /Windows/.test(seen[os].text || ""),
    (seen[os].text || "").slice(0, 120));
}

// ---- 5. A PHONE IS SHOWN NOTHING ON THE HERO ----------------------------
// Not "a smaller button": an executable a phone cannot execute is noise on the
// one screen with the least room for it, and the reader is already using the
// thing the download would give them.
check("a phone is offered nothing at all",
  seen.Android.hidden === true,
  JSON.stringify(seen.Android).slice(0, 160));

// ---- 6. THE PAGE IS PRERENDERED -----------------------------------------
// Its own title, description and canonical, none of which a browser assertion
// can see: the SPA replaces the head on boot, so this reads the built file.
let built = "";
try {
  built = readFileSync(new URL("../site/download/index.html", import.meta.url), "utf8");
} catch {
  built = "";
}
check("/download is prerendered with its own head", built.length > 0, "site/download missing");
if (built) {
  check("...with a title that is not the app's headline",
    /<title>[^<]*Windows[^<]*<\/title>/.test(built)
      && !/<title>WFSim — Ultimate/.test(built),
    (built.match(/<title>[^<]*<\/title>/) || ["(none)"])[0]);
  check("...a canonical pointing at itself",
    /rel="canonical"[^>]*\/download/.test(built),
    (built.match(/rel="canonical"[^>]*>/) || ["(none)"])[0]);
  check("...and a description about the app, not the calculator",
    /name="description"[^>]*Windows app/.test(built),
    (built.match(/name="description"[^>]*>/) || ["(none)"])[0].slice(0, 120));
}

await app.finish("the download offer answers the machine asking, and its page answers the questions");
