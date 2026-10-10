// ---------------------------------------------------------------------------
// THE DESKTOP UPDATE NOTICE
//
// IT LIVES HERE AND NOT IN THE SHELL, and that one decision is the whole update
// strategy. The shell is a binary a reader replaces by running an installer;
// this file is a thing the updater itself replaces. So an updater compiled into
// the shell cannot fix its own bugs — every reader would be frozen on the
// broken version, and the only way out is asking them to download an installer
// by hand, which is precisely what the desktop build exists to avoid. Putting
// it in `app.js` means a mistake here is repairable the same quiet way as any
// other: change a file, push, done.
//
// A NO-OP IN A BROWSER. `__WFSIM_DESKTOP__` is set by the shell and by nothing
// else, so this costs the web build one comparison at boot.
//
// IT NEVER RESTARTS BY ITSELF. A download is silent and a restart is asked for,
// because a page that reloads on its own throws away whatever the reader was
// in the middle of — an hour-long search, a fight half set up — to deliver a
// change they had not asked for yet.
const DESKTOP_POLL_MS = 700;
/// Hourly. The client is not a page a reader refreshes, so it has to look on
/// its own; hourly is often enough that a fix lands the day it ships and rare
/// enough to be invisible.
const DESKTOP_CHECK_MS = 60 * 60 * 1000;

// ---------------------------------------------------------------------------
// THE DESKTOP DOWNLOAD — one platform, and the SOURCE it comes from.
//
// WINDOWS ONLY, FOR NOW. The Linux entry was built and
// listed before anyone had asked for it, and it cost the offer its shape: the
// hero drew a Windows button, a GitHub link beside it, and then a second row
// saying "Linux · GitHub" — four things for what is one decision. Warframe on
// Linux is Proton, which is a smaller audience than the row it was taking.
//
// THE SOURCE IS NAMED BESIDE THE BUTTON, which is the whole point of the badge. The file is not hosted by this site: it lives on a Quark
// network drive, and a reader about to run a downloaded executable is entitled
// to see where it is coming from BEFORE they click rather than after the tab
// has opened. That drive is also the reason the client exists — measured from
// Shanghai, it serves 4.6x faster than Cloudflare does (AGENTS.md), and the
// readers this project is mostly for are exactly the ones GitHub is slowest
// for.
const DOWNLOADS = [
  {
    os: "Windows",
    // Anything that says Windows and is not a phone claiming to be one.
    detect: (ua) => /Windows NT/i.test(ua),
    file: "WFSim.exe",
    source: {
      name: "夸克网盘",
      url: "https://pan.quark.cn/s/dc8fe9046d6a",
      // A GENERIC DRIVE MARK, not Quark's own. The badge has to SAY which
      // service this is, which the name does; reproducing somebody else's
      // logo to do it would be borrowing their trademark for a claim they
      // have not made about this file.
      icon: '<svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true">'
        + '<path fill="none" stroke="currentColor" stroke-width="1.7"'
        + ' stroke-linecap="round" stroke-linejoin="round"'
        + ' d="M6.5 18.5a4 4 0 0 1-.4-8A5.5 5.5 0 0 1 17 10.2a3.6 3.6 0 0 1 .3 8.3z'
        + 'M12 9.5v6m0 0-2.4-2.4M12 15.5l2.4-2.4"/></svg>',
    },
  },
];

/// The button and the source it came from, as one piece of markup.
///
/// THE SOURCE IS NAMED BESIDE THE BUTTON. This site does not host the
/// executable, and a reader about to run a downloaded binary is entitled to see
/// whose drive it is on before they click rather than after the tab has opened.
/// The badge links to the same place, so "where does this come from" and "take
/// me there to look first" are one click apart.
// A CLICK ON EITHER LINK is a download asked for; the file itself is not ours to count.
document.addEventListener("click", (e) => {
  if (e.target.closest && e.target.closest(".dl-btn, .dl-src")) track("desktop.download");
});
function downloadOffer(d) {
  const src = d.source;
  return `<a class="dl-btn" href="${escHtml(src.url)}" target="_blank" rel="noopener">`
    + `${escHtml(tr("Download for {os}").replace("{os}", d.os))}</a>`
    + `<a class="dl-src" href="${escHtml(src.url)}" target="_blank" rel="noopener"`
    + ` title="${escHtml(trF("hosted on {name} — this is where the file comes from",
      { name: src.name }))}">${src.icon}<span>${escHtml(src.name)}</span></a>`;
}

/// The machine asking, in one answer for both surfaces to use. `null` means a
/// phone: it is already running what a download would install.
function downloadFor(ua) {
  if (/Android|iPhone|iPad|iPod/i.test(ua || "")) return null;
  return DOWNLOADS.find((d) => d.detect(ua || "")) || false;
}

/// THE OLDEST SHELL THIS CONTENT DOES NOT ASK TO BE REPLACED, as the build date
/// `window.__WFSIM_SHELL__` starts with. Content reaches every shell and a shell
/// never replaces itself, so this is the only way an old one hears that a new
/// download fixes something only a download can. Raise it when a released shell
/// changes behaviour a reader would notice, and only once that file is on the
/// drive `DOWNLOADS` points at — or the notice sends readers to the old one.
const SHELL_MINIMUM = [2026, 10, 10];
/// Whether the running shell predates it. A shell too old to state its build
/// predates everything.
function shellIsOld() {
  if (!window.__WFSIM_DESKTOP__) return false;
  const m = /^(\d+)\.(\d+)\.(\d+)/.exec(String(window.__WFSIM_SHELL__ || ""));
  if (!m) return true;
  const have = [Number(m[1]), Number(m[2]), Number(m[3])];
  for (let i = 0; i < 3; i++) if (have[i] !== SHELL_MINIMUM[i]) return have[i] < SHELL_MINIMUM[i];
  return false;
}

/// The page at /download: the offer, above the questions the markup asks.
function renderDownloadPage() {
  const host = document.getElementById("dl-offer");
  if (!host) return;
  // ALREADY RUNNING IT. The URL is typed by hand and read off a video, so it
  // has to answer inside the client too — as the fact that there is nothing
  // here to install rather than as a blank.
  if (window.__WFSIM_DESKTOP__ && shellIsOld()) {
    host.innerHTML = downloadOffer(DOWNLOADS[0]) + `<span class="dl-why">${escHtml(
      tr("This copy of the program is older than the current one. Download the new WFSim.exe and use it in place of this one — your saved builds stay."))}</span>`;
    renderDesktopSettings();
    return;
  }
  if (window.__WFSIM_DESKTOP__) {
    host.innerHTML = `<span class="dl-why">${escHtml(
      tr("You are running the Windows app. It updates itself — there is nothing to download here."))}</span>`;
    renderDesktopSettings();
    return;
  }
  const mine = downloadFor(navigator.userAgent);
  // A PHONE READS THE PAGE. The hero shows a phone nothing because an
  // executable is noise on the one screen with the least room for it; a reader
  // who has navigated HERE asked, and is answered with the button they cannot
  // use today plus the reason.
  const d = mine || DOWNLOADS[0];
  host.innerHTML = downloadOffer(d)
    + (mine ? "" : `<span class="dl-why">${escHtml(
      tr("This is a Windows program — it will not run on the machine you are reading this on."))}</span>`);
}

/// INSIDE THE CLIENT, /download IS ITS SETTINGS, and the way in says so: the
/// settings menu's row, its icon and the page's heading. Rewritten in the
/// English source while app.js loads, before the page is translated, so the
/// one translation path covers it.
if (window.__WFSIM_DESKTOP__) {
  const link = document.querySelector(".dl-link"), label = link && link.querySelector(".tbl");
  if (link) {
    link.title = "Desktop settings";
    const svg = link.querySelector("svg path");
    if (svg) svg.setAttribute("d", "M12 15.2a3.2 3.2 0 1 0 0-6.4 3.2 3.2 0 0 0 0 6.4zM19.4 13.5l1.6 1.2-1.8 3.1-1.9-.7a7 7 0 0 1-1.7 1l-.3 2h-3.6l-.3-2a7 7 0 0 1-1.7-1l-1.9.7-1.8-3.1 1.6-1.2a7 7 0 0 1 0-2l-1.6-1.2 1.8-3.1 1.9.7a7 7 0 0 1 1.7-1l.3-2h3.6l.3 2a7 7 0 0 1 1.7 1l1.9-.7 1.8 3.1-1.6 1.2a7 7 0 0 1 0 2z");
  }
  if (label) label.textContent = "Desktop settings";
  const h = $("h-download");
  if (h) h.textContent = "Desktop settings";
}

/// THE PROGRAM'S OWN SETTINGS, on the client's /download — the page a browser
/// reader downloads from is, inside the client, where it is configured. The
/// shell holds them (`desktop/src/settings.rs`), because starting hidden is
/// decided before any page exists; a shell too old to have them says so.
const DESKTOP_SWITCHES = [
  ["autostart", "Start with Windows", "Opens WFSim when you sign in to Windows."],
  ["start_minimized", "Start in the tray", "When Windows starts it, stay in the tray instead of opening a window."],
  ["close_to_tray", "Close to the tray",
    "The close button keeps WFSim running in the tray, and computing together goes on. Quit from the tray icon's menu."],
];
/// What the shell last said: null before asking, false for a shell without them.
let desktopPrefs = null;
let desktopError = "";
/// THE UNINSTALL IS TWO CLICKS, the second beside what it destroys: it
/// deletes every build saved in this profile, and no native dialog is allowed.
let uninstallAsked = false;

async function renderDesktopSettings() {
  const host = $("dl-settings");
  if (!host || !window.__TAURI_INTERNALS__) return;
  const invoke = (cmd, args = {}) => window.__TAURI_INTERNALS__.invoke(cmd, args);
  for (const el of document.querySelectorAll("#download-page [data-dl-web]")) el.hidden = true;
  host.hidden = false;
  if (desktopPrefs === null) {
    try { desktopPrefs = await invoke("desktop_settings"); } catch (_) { desktopPrefs = false; }
  }
  const section = (title, body) => `<section class="block"><div class="bh"><h2>${escHtml(tr(title))}</h2></div><div class="bb">${body}</div></section>`;
  if (!desktopPrefs) {
    host.innerHTML = section("Settings", `<p class="dl-why">${escHtml(tr("These settings need the new version of the program."))}</p>`
      + `<div class="dl-offer">${downloadOffer(DOWNLOADS[0])}</div>`);
    return;
  }
  const p = desktopPrefs;
  const switches = DESKTOP_SWITCHES.map(([key, name, says]) => {
    const off = key === "start_minimized" && !p.autostart;
    return `<label class="dt-sw"><input type="checkbox" data-dt="${key}"${p[key] ? " checked" : ""}${off ? " disabled" : ""}>`
      + `<b>${escHtml(tr(name))}</b><small>${escHtml(tr(says))}</small></label>`;
  }).join("");
  const uninstall = uninstallAsked
    ? `<p class="dt-err">${escHtml(tr("This turns off starting with Windows and deletes the program, its data and everything saved in it. Builds, scenarios and rivens not synced to an account are lost: to keep them, export them first from Saved items in the settings menu."))}</p>`
      + `<div class="dt-acts"><button type="button" class="btn-danger" data-dt-a="uninstall-go">${escHtml(tr("Uninstall for good"))}</button>`
      + `<button type="button" class="ghost-btn btn-sm" data-dt-a="uninstall-no">${escHtml(tr("Cancel"))}</button></div>`
    : `<div class="dt-acts"><button type="button" class="ghost-btn btn-sm" data-dt-a="uninstall">${escHtml(tr("Uninstall"))}</button></div>`;
  host.innerHTML = section("Settings", switches
      + `<div class="dt-acts"><button type="button" class="ghost-btn btn-sm" data-dt-a="folder">${escHtml(tr("Open the data folder"))}</button></div>`
      + (desktopError ? `<p class="dt-err">${escHtml(desktopError)}</p>` : ""))
    + section("Uninstall", `<p class="dl-why">${escHtml(tr("Removes this program and everything it keeps on this computer."))}</p>${uninstall}`);
  host.onchange = async (e) => {
    const box = e.target.closest && e.target.closest("[data-dt]");
    if (!box) return;
    try {
      desktopPrefs = await invoke("desktop_set", { key: box.getAttribute("data-dt"), on: box.checked });
      desktopError = "";
    } catch (err) { desktopError = String(err); }
    renderDesktopSettings();
  };
  host.onclick = async (e) => {
    const b = e.target.closest && e.target.closest("[data-dt-a]");
    if (!b) return;
    const a = b.getAttribute("data-dt-a");
    if (a === "folder") invoke("open_data_dir").catch((err) => { desktopError = String(err); renderDesktopSettings(); });
    if (a === "uninstall" || a === "uninstall-no") { uninstallAsked = a === "uninstall"; renderDesktopSettings(); }
    if (a === "uninstall-go") {
      try { await invoke("uninstall"); } catch (err) { desktopError = String(err); uninstallAsked = false; renderDesktopSettings(); }
    }
  };
}

function mountDesktopUpdater() {
  if (!window.__WFSIM_DESKTOP__ || !window.__TAURI_INTERNALS__) return;
  const invoke = (cmd, args = {}) => window.__TAURI_INTERNALS__.invoke(cmd, args);

  invoke("tray_labels", { open: tr("Open WFSim"), settings: tr("Desktop settings"), quit: tr("Quit") }).catch(() => { /* an older shell has no tray */ });

  const bar = document.createElement("div");
  bar.className = "dtup";
  bar.hidden = true;
  document.body.appendChild(bar);

  let dismissed = "";
  const hide = () => { bar.hidden = true; };

  // ONE QUESTION, AND IT IS THE ONLY ONE THE READER CAN ANSWER: when to
  // restart. Whether to spend 12 MB on a calculator they already have open is
  // not a decision they have anything to decide it with, and asking it made
  // the update wait behind a button — so the fetch happens on its own and the
  // notice appears when there is something a click can finish.
  const render = (s) => {
    // A version the reader has already waved away stays away until the NEXT
    // one — a notice that comes back every hour is a notice people learn to
    // close without reading. A status with NO version was never dismissed:
    // a check that fails before it reads one carries the empty string, and
    // matching that against a fresh `dismissed` would hide the failure.
    if (s.version && dismissed === s.version) return hide();
    let html = "";
    if (s.phase === "ready") {
      html = `<span class="dtup-t">${escHtml(tr("Update ready — restart to finish"))}</span>`
        + `<button class="dtup-b" data-a="go">${escHtml(tr("Restart now"))}</button>`
        + `<button class="dtup-x" data-a="no" title="${escHtml(tr("Later"))}">×</button>`;
    } else if (s.phase === "failed") {
      // THE ONE FAILURE THAT IS WORTH INTERRUPTING FOR, even unasked: a client
      // that cannot update is a client frozen for good, and silence here is
      // indistinguishable from being up to date.
      html = `<span class="dtup-t dtup-e">${escHtml(tr("Update failed"))}: ${escHtml(s.message || "")}</span>`
        + `<button class="dtup-x" data-a="no">×</button>`;
    } else {
      return hide();
    }
    bar.innerHTML = html;
    bar.hidden = false;
  };

  let polling = false;
  const poll = async () => {
    if (polling) return;
    polling = true;
    try {
      const s = await invoke("update_status");
      if (s.phase === "available") {
        await invoke("update_download");
        polling = false;
        return poll();
      }
      render(s);
      if (s.phase === "checking" || s.phase === "downloading") setTimeout(() => { polling = false; poll(); }, DESKTOP_POLL_MS);
      else polling = false;
    } catch (_) { polling = false; }
  };

  bar.addEventListener("click", async (e) => {
    const b = e.target.closest && e.target.closest("[data-a]");
    if (!b) return;
    const a = b.getAttribute("data-a");
    if (a === "no") {
      try { const s = await invoke("update_status"); dismissed = s.version; } catch (_) { /* hide anyway */ }
      return hide();
    }
    if (a === "go") {
      try {
        await invoke("update_apply");
        // The swap is done; the page is now serving files from a directory
        // that no longer matches what this document was loaded from.
        location.reload();
      } catch (err) {
        render({ phase: "failed", message: String(err), version: "" });
      }
    }
  });

  // THE SOURCE LINK IS PART OF SHIPPING THIS AT ALL. AGPL asks for the source
  // corresponding to the binary being conveyed, and this client conveys itself
  // a new one on every update — so the link has to name the version in the
  // footer beside it, not "the latest", which is the wrong tree for anyone who
  // has not updated yet. The archive for every version is kept on the same
  // bucket the update came from, so this is the same "same place" the licence
  // means. It goes next to the build stamp because that is where a reader
  // already looks to find out what they are running.
  invoke("source_url").then((url) => {
    const stamp = document.getElementById("build-stamp");
    if (!stamp || !url) return;
    const a = document.createElement("a");
    a.href = url;
    a.className = "wl";
    a.textContent = tr("source");
    a.title = "AGPL-3.0";
    stamp.after(document.createTextNode(" · "), a);
  }).catch(() => { /* an older shell without the command still runs */ });

  // AN ASSEMBLED UPDATE IS THE END OF THE LINE UNTIL A RESTART. `check`
  // compares the channel against `current/`, which the download did not touch,
  // so checking again reports the same update as available — and now that the
  // fetch is automatic, that is 12 MB pulled down every hour behind a notice
  // already on screen saying it is ready.
  const look = async () => {
    try {
      const s = await invoke("update_status");
      if (s.phase === "ready" || s.phase === "downloading") return poll();
      await invoke("update_check");
      poll();
    } catch (_) { /* offline is normal */ }
  };
  // Not at boot: the first seconds belong to the page the reader opened.
  setTimeout(look, 8000);
  setTimeout(offerNewShell, 8000);
  setInterval(look, DESKTOP_CHECK_MS);
}

/// A SHELL OLDER THAN `SHELL_MINIMUM` IS TOLD A NEW DOWNLOAD EXISTS — once per
/// minimum: closed, it stays closed until a newer shell raises it again. Its
/// own corner, so it never covers the restart notice beside it.
const SHELL_DISMISSED = "wfsim-shell-dismissed";
function offerNewShell() {
  if (!shellIsOld()) return;
  const key = SHELL_MINIMUM.join(".");
  try { if (localStorage.getItem(SHELL_DISMISSED) === key) return; } catch (_) { /* ask anyway */ }
  const d = DOWNLOADS[0];
  const bar = document.createElement("div");
  bar.className = "dtup dtup-shell";
  bar.innerHTML = `<span class="dtup-t">${escHtml(tr("A new version of the program is out — you can sign in to your account, and it can start with Windows, sit in the tray and uninstall itself."))}</span>`
    + `<a class="dtup-b" href="${escHtml(d.source.url)}" target="_blank" rel="noopener">${escHtml(tr("Download"))}</a>`
    + `<button class="dtup-x" title="${escHtml(tr("Later"))}">×</button>`;
  bar.querySelector(".dtup-b").addEventListener("click", () => track("desktop.download"));
  bar.querySelector(".dtup-x").addEventListener("click", () => {
    try { localStorage.setItem(SHELL_DISMISSED, key); } catch (_) { /* this session only */ }
    bar.remove();
  });
  document.body.appendChild(bar);
}
