// ---- USAGE ------------------------------------------------------------------
//
// WHAT READERS DID, one point per thing, to `POST /api/e` (worker/index.js
// §USAGE). Everything is computed on the device, so this is the only way the
// site learns whether a visit produced a result. docs/ANALYTICS.md.

/// THE LIVE SITE, and the desktop shell that serves the same files. A dev
/// server or a check (127.0.0.1) is neither, so no test ever reaches a live
/// store — the short-link store and this one both ask here.
const LIVE_ORIGIN = "https://wfsim.app";
const LIVE_HOSTS = ["wfsim.app", "wfsim.localhost"];

/// A random id per browser, and nothing else about the reader. Clearable like
/// every other `wfsim-*` key; a browser that cannot store one gets one per page.
const USAGE_CID = "wfsim-cid";
let usageCid = null;
function usageVisitor() {
  if (usageCid) return usageCid;
  try { usageCid = localStorage.getItem(USAGE_CID); } catch (_) { /* private mode */ }
  if (!/^[0-9a-f]{32}$/.test(usageCid || "")) {
    const b = crypto.getRandomValues(new Uint8Array(16));
    usageCid = Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
    try { localStorage.setItem(USAGE_CID, usageCid); } catch (_) { /* per page, then */ }
  }
  return usageCid;
}

/// THE READER'S OWN SWITCH, on /support beside what is counted. Browser storage,
/// so a browser that cannot keep it is counted — and says so by showing the id.
const USAGE_OFF = "wfsim-usage-off";
const usageOff = () => { try { return localStorage.getItem(USAGE_OFF) === "1"; } catch (_) { return false; } };

/// /support's "what is counted": this browser's id, and the switch.
function renderUsageNote() {
  const id = $("usage-id"), btn = $("usage-off");
  if (!id || !btn) return;
  id.textContent = usageVisitor().slice(0, 8);
  btn.textContent = tr(usageOff() ? "count this browser again" : "stop counting this browser");
  btn.onclick = () => {
    try { localStorage.setItem(USAGE_OFF, usageOff() ? "0" : "1"); } catch (_) { /* nothing to keep it in */ }
    renderUsageNote();
  };
}

/// HOW THIS PAGE WAS REACHED, as `app.boot`'s subject: the browser's own
/// navigation type when it is not a fresh one ("reload", "back_forward"), else
/// where the reader came from — "from_site", "direct", or "from_<host>". The
/// referrer's HOST and never its path: which site sent someone, not which page.
/// A reload a new release caused says so (`reloadForRelease`), read once.
const USAGE_ARRIVAL = "wfsim-arrival";
function usageArrival() {
  const nav = (performance.getEntriesByType("navigation")[0] || {}).type || "navigate";
  let told = null;
  try { told = sessionStorage.getItem(USAGE_ARRIVAL); sessionStorage.removeItem(USAGE_ARRIVAL); } catch (_) { /* none */ }
  if (nav === "reload" && /^release_[a-z]+$/.test(told || "")) return told;
  if (nav !== "navigate") return nav.replace(/[^a-z_]/g, "_");
  let host = "";
  try { host = document.referrer ? new URL(document.referrer).hostname : ""; } catch (_) { /* none */ }
  if (!host) return "direct";
  if (LIVE_HOSTS.includes(host)) return "from_site";
  return ("from_" + host.replace(/^www\./, "").replace(/[^a-z0-9]+/g, "_")).slice(0, 64);
}

/// WHAT THIS DEVICE CAN DO, never what it is: a model or a user agent goes stale
/// with every new device, and the questions are about traits — tapped or
/// clicked, room for the editor, cores for the community's work, an in-app
/// browser that limits downloads. COARSE CLASSES ONLY, so the point cannot
/// single a device out: `<input>_<width>_<browser>` as `app.device`'s subject,
/// the cores rounded down to a step as its `n`.
const USAGE_CORES = [1, 2, 4, 6, 8, 12, 16, 24, 32];
function usageDevice() {
  const q = (m) => !!(window.matchMedia && matchMedia(m).matches);
  const coarse = q("(any-pointer: coarse)"), fine = q("(any-pointer: fine)");
  const input = q("(pointer: none)") ? "nopointer"
    : q("(pointer: coarse)") ? (fine ? "touchmouse" : "touch")
    : coarse ? "mousetouch" : "mouse";
  const w = innerWidth;
  const width = w <= 640 ? "narrow" : w <= 1024 ? "medium" : "wide";
  const ua = navigator.userAgent || "";
  const browser = /MicroMessenger\//.test(ua) ? "wechat" : /\sQQ\//.test(ua) ? "qq" : "browser";
  const cores = USAGE_CORES.filter((c) => c <= (navigator.hardwareConcurrency || 0)).pop() || 0;
  return { subject: `${input}_${width}_${browser}`, cores };
}

/// AN UNCAUGHT FAILURE IN THIS PAGE'S OWN CODE, as `app.error`: `boot` before the
/// app is ready (the reader sees "could not start"), `script` or `promise`
/// after — and WHERE in our own file, as `<kind>_<file>_<line>_<column>`, which
/// with the point's release names the line in that build's committed bundle.
/// A PLACE, NEVER THE MESSAGE — that can carry what the reader typed. Only this
/// origin's files count, so a browser extension's errors are not ours.
const USAGE_FILES = [["/asset/app.", "app"], ["/asset/nona.", "nona"]];
function usagePlace(text) {
  const at = typeof text === "string" ? text.indexOf(location.origin + "/") : -1;
  const m = at < 0 ? null : /^(\/[^\s:)]*):(\d+):(\d+)/.exec(text.slice(at + location.origin.length));
  if (!m) return null;
  const file = (USAGE_FILES.find(([p]) => m[1].startsWith(p)) || [, "page"])[1];
  return `${file}_${m[2]}_${m[3]}`;
}
/// `place` is null for another origin's failure, "" for ours with no position.
function usageError(kind, place) {
  if (place === null) return;
  const k = window.__wfsimReady ? kind : "boot";
  track("app.error", (place ? `${k}_${place}` : k).slice(0, 64), Math.round(performance.now()));
}
window.addEventListener("error", (e) => usageError("script",
  e && typeof e.filename === "string" && e.filename.startsWith(location.origin + "/")
    ? usagePlace(`${e.filename}:${e.lineno}:${e.colno}`) || "" : null));
window.addEventListener("unhandledrejection", (e) => usageError("promise",
  usagePlace(e && e.reason && e.reason.stack)));

/// ONCE PER (event, subject) PER PAGE LOAD. A point says a reader got this far
/// with this thing, not how many times: forty edits to one build are one build.
const usageSent = new Set();

/// Fire and forget. Never awaited, never throws, never delays a render — a
/// failed point is a point missing, and nothing the reader can see.
/// A text/plain body is a SIMPLE request: the desktop shell posts cross-origin
/// without a preflight.
function track(event, subject = "", n) { usageSend(event, subject, n); }
/// NONA'S POINTS, which reach the counter through the door (`wfsim.usage`):
/// her own events only, named as literals in `nona/`, where
/// `check_usage_events` reads them beside the page's.
function trackNona(event, subject, n) { if (/^nona\.[a-z]+$/.test(event)) usageSend(event, subject, n); }
function usageSend(event, subject, n) {
  try {
    if (!LIVE_HOSTS.includes(location.hostname)) return;
    // A reader who has asked not to be tracked is not.
    if (navigator.globalPrivacyControl || navigator.doNotTrack === "1" || usageOff()) return;
    // A PAGE THE BROWSER RENDERS AHEAD, in case it is opened, is not yet a
    // visit: the point waits for the reader to actually arrive.
    if (document.prerendering) {
      document.addEventListener("prerenderingchange", () => usageSend(event, subject, n), { once: true });
      return;
    }
    const key = `${event}\n${subject}`;
    if (usageSent.has(key)) return;
    usageSent.add(key);
    const seg = location.pathname.split("/")[1] || "";
    const point = {
      v: 1, e: event, cid: usageVisitor(), subject: subject || "",
      route: !seg ? "home" : /^[a-z0-9_-]{1,32}$/.test(seg) ? seg : "other",
      lang: typeof LANG === "string" ? LANG : "en",
      shell: window.__WFSIM_DESKTOP__ ? "desktop" : "web",
      release: RELEASE_ID,
      ...(typeof n === "number" ? { n } : {}),
    };
    const at = location.origin === LIVE_ORIGIN ? "" : LIVE_ORIGIN;
    fetch(`${at}/api/e`, { method: "POST", body: JSON.stringify(point), keepalive: true })
      .catch(() => {});
  } catch (_) { /* never the page's problem */ }
}
