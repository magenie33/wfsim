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

/// The page at /download: the offer, above the questions the markup asks.
function renderDownloadPage() {
  const host = document.getElementById("dl-offer");
  if (!host) return;
  // ALREADY RUNNING IT. The URL is typed by hand and read off a video, so it
  // has to answer inside the client too — as the fact that there is nothing
  // here to install rather than as a blank.
  if (window.__WFSIM_DESKTOP__) {
    host.innerHTML = `<span class="dl-why">${escHtml(
      tr("You are running the Windows app. It updates itself — there is nothing to download here."))}</span>`;
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

function mountDesktopUpdater() {
  if (!window.__WFSIM_DESKTOP__ || !window.__TAURI_INTERNALS__) return;
  const invoke = (cmd, args = {}) => window.__TAURI_INTERNALS__.invoke(cmd, args);

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
  setInterval(look, DESKTOP_CHECK_MS);
}

