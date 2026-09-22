// ---- The routes whose body is not in every document --------------------
// A DOCUMENT CARRIES ITS OWN PAGE AND NOBODY ELSE'S. The shell holds a `<main>`
// per route and the app shows one of them, so a document holding all of them
// carries six thousand characters of somebody else's text — on four hundred
// urls, which is four hundred near-copies of one document to anything that
// reads them. The build leaves each document the one body it is about and
// puts the rest in a file of their own; this fetches that file the first time
// the reader asks for a page whose body is not here.
//
// A DEV SERVER SHIPS THEM ALL, because it serves the shell as written rather
// than the build's output. An already-filled `<main>` is the test, so nothing
// here needs to know which of the two it is running in.

/// Replaced by the build with the hashed file's own name; the literal is what
/// the dev server serves, where there is nothing to fetch.
const PAGE_BODIES_URL = "/pages.html";

let pageBodiesAsk = null;
let pageBodiesSettled = false;

/// The bodies this document did not ship, injected once. Returns null when
/// there is nothing to wait for — the body is already here, or the file is
/// not reachable and the page has to draw without it.
///
/// ASKED ONCE, EVER. A file that never arrives leaves the body empty, and
/// emptiness is what a caller keys off — so answering with the settled promise
/// again hands the route that asked a `then` that re-runs the route, which
/// re-asks. The page does not break: it SPINS, which is the failure that takes
/// a machine down rather than a pixel.
function ensurePageBodies(id) {
  const el = document.getElementById(id);
  if (!el || el.childElementCount || pageBodiesSettled) return null;
  if (!pageBodiesAsk) {
    pageBodiesAsk = fetch(PAGE_BODIES_URL)
      .then((r) => (r.ok ? r.text() : ""))
      .then((html) => {
        if (!html) return;
        const doc = new DOMParser().parseFromString(html, "text/html");
        for (const t of doc.querySelectorAll("template[data-page]")) {
          const into = document.getElementById(t.dataset.page);
          // NEVER OVER A BODY THAT IS ALREADY THERE: the document that IS this
          // page shipped its own, and it is the one the reader was served.
          if (into && !into.childElementCount) into.append(t.content.cloneNode(true));
        }
        // THE INJECTED MARKUP IS ENGLISH, like the shell it came out of, so
        // the overlay has to reach it — it swept the document once, before
        // this existed.
        applyI18n();
      })
      .catch(() => { /* drawn empty; a reload is the reader's retry */ })
      .finally(() => { pageBodiesSettled = true; });
  }
  return pageBodiesAsk;
}
