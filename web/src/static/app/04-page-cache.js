// ---- THE PAGE CACHE -----------------------------------------------------------
//
// `/page_cache.js` keeps this page and its content-addressed files, and answers
// with them once the network has taken too long (docs/DISTRIBUTION.md §"The page
// cache"). Registered on wfsim.app alone: the desktop shell serves its own files
// and a dev server or a check must always see the files on disk.

addEventListener("load", () => {
  if (location.hostname !== "wfsim.app" || window.__WFSIM_DESKTOP__ || !navigator.serviceWorker) return;
  navigator.serviceWorker.register("/page_cache.js").catch(() => {});
});

/// THE KEPT PAGES ARE DROPPED before a reload that follows a newer release: on a
/// slow road the reload would be answered by the kept copy, which is the release
/// being left, and the page would reload into itself for ever.
async function forgetKeptPages() {
  try {
    for (const name of await caches.keys()) if (name.startsWith("wfsim-pages-")) await caches.delete(name);
  } catch (_) { /* no cache storage: nothing kept */ }
}
