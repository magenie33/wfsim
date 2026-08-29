// THE DOWNLOAD OFFER ANSWERS THE MACHINE ASKING, and has three answers.
//
// The home page offers the desktop build, and what it should say depends on
// where it is read: the platform we build for gets its own button, a desktop we
// do NOT build for is told so instead of being shown an executable it cannot
// run, and a phone is shown nothing at all — it is already running the thing a
// download would install.
//
// Only ONE of those is visible on the machine this is written on, so checking
// by looking is checking a third of it. Worse, the failure modes are silent and
// plausible: a Mac reader shown `WFSim.exe`, a phone reader shown a 34 MB
// download. Each looks like a working page.
//
// IT PINS THE SOURCE BADGE, which is the offer's other half. This site does not host the executable — it lives on a Quark
// network drive — and a reader about to run a downloaded binary is entitled to
// see whose drive it is on BEFORE they click. A badge that drew an icon and no
// NAME would look finished and identify nothing, which is why the name is
// asserted rather than the element.
//
// WINDOWS ONLY, FOR NOW. The Linux row was built before anyone asked for it and
// cost the offer its shape; the AppImage is still cut by the release workflow
// and still on the GitHub release page, it is simply not what the hero is
// about. The assertions that used to cover it are now the negative control:
// a Linux reader must be TOLD, not handed an .exe.
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
    links: [...host.querySelectorAll("a")].map((a) => a.href),
    // NOT NORMALISED, deliberately. A whitespace regex here sits inside a
    // TEMPLATE LITERAL, which eats the backslash — the page then runs /s+/g
    // and replaces every letter s in the sentence with a space, so the
    // assertion below read "only supports Window ." and failed on a correct
    // page. It only greps this string, so there is nothing to normalise for.
    text: host.innerText || "",
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

// ---- 1. A PHONE IS SHOWN NOTHING ----------------------------------------
// Not "a smaller button": an executable a phone cannot execute is noise on the
// one screen with the least room for it, and the reader is already using the
// thing the download would give them.
check("a phone is offered nothing at all",
  seen.Android.hidden === true,
  JSON.stringify(seen.Android).slice(0, 160));

// ---- 2. THE PLATFORM WE BUILD FOR GETS ITS OWN BUTTON --------------------
check("Windows is offered Windows",
  /Windows/.test(seen.Windows.button || ""),
  `button ${JSON.stringify(seen.Windows.button)}`);
check("...and it goes to the network drive, not GitHub",
  (seen.Windows.buttonHref || "").includes("pan.quark.cn"),
  seen.Windows.buttonHref || "(no button)");

// ---- 3. THE SOURCE IS NAMED BESIDE IT -----------------------------------
// The NAME is the assertion, not the element: an icon alone identifies nothing,
// which is the whole failure this badge exists to prevent.
check("the source is named beside the button",
  (seen.Windows.srcName || "").includes("夸克网盘"),
  `badge ${JSON.stringify(seen.Windows.srcName)}`);
check("...with a mark, and pointing at the same place",
  seen.Windows.srcIcon === true
    && (seen.Windows.srcHref || "") === (seen.Windows.buttonHref || "x"),
  `icon ${seen.Windows.srcIcon}, href ${seen.Windows.srcHref}`);

// ---- 4. A PLATFORM WE DO NOT BUILD FOR IS TOLD SO ------------------------
// The negative control, and the one a check that only tested Windows would
// pass while the page handed a Mac or Linux reader an .exe.
for (const os of ["macOS", "Linux"]) {
  check(`${os} is offered no button`,
    seen[os].button === null && seen[os].hidden === false,
    `button ${JSON.stringify(seen[os].button)}, hidden ${seen[os].hidden}`);
  check(`...and ${os} is told the desktop build is Windows`,
    /Windows/.test(seen[os].text || ""),
    (seen[os].text || "").slice(0, 120));
  // AND IS HANDED NOTHING IT CANNOT RUN. Saying the right sentence beside a
  // live .exe link would read as a pass on every assertion above.
  check(`...and ${os} is handed no executable`,
    !(seen[os].links || []).some((u) => /\.exe|pan\.quark\.cn/.test(u)),
    JSON.stringify(seen[os].links));
}

await app.finish("the download offer answers the machine asking");
