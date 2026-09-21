// ---- THE TRAIL ---------------------------------------------------------
//
// The last few pages THIS TAB visited, as a list in the top bar. A configuration
// lives on the page of its own module, and a weapon page sends you there and
// wants you back: one "back" link is the last page only, and where you came from
// three pages ago is the one you want. Per tab (`sessionStorage`), never
// shared: another tab's trail is not this one's.
const TRAIL_KEY = "wfsim-trail";
const TRAIL_LEN = 5;

function trailRead() {
  try {
    const a = JSON.parse(sessionStorage.getItem(TRAIL_KEY));
    return Array.isArray(a) ? a : [];
  } catch (_) { return []; }
}

/// THIS PAGE, first. A page visited again moves to the front rather than
/// appearing twice, and a framed page (`?embed`) is somebody else's pane, not a
/// place this tab went.
function trailPush() {
  if (EMBED) return;
  const path = location.pathname;
  const title = path === "/" ? tr("Home") : document.title.replace(/ — WFSim$/, "");
  const list = [{ path, title }, ...trailRead().filter((x) => x.path !== path)];
  try { sessionStorage.setItem(TRAIL_KEY, JSON.stringify(list.slice(0, TRAIL_LEN + 1))); } catch (_) { /* no trail */ }
  renderTrail();
}

function renderTrail() {
  const host = $("trail");
  if (!host) return;
  const back = trailRead().slice(1);
  host.hidden = !back.length;
  host.innerHTML = back.length ? ddButton("dd-trail", {
    value: "", placeholder: `↩ ${tr("Recent")}`,
    items: back.map((x) => ({ value: x.path, label: x.title })),
    onPick: (p) => nav(p),
  }) : "";
}
