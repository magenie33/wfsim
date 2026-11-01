// ---- SAVED ITEMS, AS A FILE ---------------------------------------------------
//
// docs/UI.md §"Saved items as a file". Everything this browser saved — every
// collection `COLLECTIONS` names — written to one file, and read back from one,
// signed in or not. What a reader saved is theirs to take anywhere; sync is the
// convenience of not having to.
const SAVES_FORMAT = "wfsim-saves";
const SAVES_VERSION = 1;

/// WHAT TRAVELS: each entry as saved, less what belongs to this browser — its
/// sync choice — and less a measured result an older page left in it.
const savesBody = (p) => { const { cloud_sync, lastResult, ...rest } = p; return rest; };
/// Two entries are the same item when everything but when they were saved agrees.
const savesSame = (a, b) => { const { savedAt: x, ...ra } = savesBody(a); const { savedAt: y, ...rb } = savesBody(b); return JSON.stringify(ra) === JSON.stringify(rb); };

function savesRead(key) {
  try { const v = JSON.parse(localStorage.getItem(key)); return Array.isArray(v) ? v : []; } catch (_) { return []; }
}

function savesExport() {
  flushPresetSaves();
  const lists = {};
  let n = 0;
  for (const { domain } of COLLECTIONS) {
    const ps = savesRead(presetListKey(domain)).filter((p) => p && p.id && !p.builtin).map(savesBody);
    if (ps.length) { lists[domain] = ps; n += ps.length; }
  }
  if (!n) { noteInline(tr("Nothing is saved on this browser yet.")); return; }
  const file = { format: SAVES_FORMAT, version: SAVES_VERSION, exported_at: new Date().toISOString(), lists };
  const a = document.createElement("a");
  a.href = URL.createObjectURL(new Blob([JSON.stringify(file)], { type: "application/json" }));
  a.download = `wfsim-saves-${new Date().toISOString().slice(0, 10)}.json`;
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 1000);
  presetToast(tr("{n} saved items exported").replace("{n}", n));
}

/// READ BACK, NEVER OVER WHAT IS HERE. An item this browser lacks is added as
/// it was; one it holds unchanged is skipped; one it holds CHANGED is added
/// beside it as a copy, and a copied custom has what the file names it by
/// repointed at the copy — the rule a share link keeps.
function savesImport(text) {
  let file;
  try { file = JSON.parse(text); } catch (_) { file = null; }
  if (!file || file.format !== SAVES_FORMAT || !file.lists || typeof file.lists !== "object") {
    noteInline(tr("That file is not a WFSim export."));
    return;
  }
  if (file.version > SAVES_VERSION) {
    noteInline(tr("That file was written by a newer WFSim. Reload the page and try again."));
    return;
  }
  flushPresetSaves();
  const known = new Map(COLLECTIONS.map((c) => [c.domain, c]));
  const incoming = Object.entries(file.lists)
    .filter(([d, ps]) => known.has(d) && Array.isArray(ps))
    .map(([d, ps]) => [d, ps.filter((p) => p && typeof p === "object" && typeof p.id === "string" && p.id && !p.builtin)]);
  // CUSTOMS FIRST: a custom given a new id moves the references to it.
  incoming.sort(([a], [b]) => isCustomDomain(b) - isCustomDomain(a));
  const moved = new Map();
  const repoint = (p) => {
    if (!moved.size) return p;
    let s = JSON.stringify(p);
    for (const [from, to] of moved) s = s.split(JSON.stringify(from)).join(JSON.stringify(to));
    return JSON.parse(s);
  };
  const lists = new Set(), ids = new Set();
  let added = 0, copied = 0, same = 0;
  for (const [domain, ps] of incoming) {
    const key = presetListKey(domain);
    const here = savesRead(key);
    const ref = known.get(domain).ref;
    const before = here.length;
    for (const raw of ps) {
      const p = repoint(savesBody(raw));
      const had = here.find((x) => x && x.id === p.id);
      if (had && savesSame(isCustomDomain(domain) ? had : { ...had, name: p.name }, p)) { same++; continue; }
      if (had) {
        const id = presetNewId();
        if (ref) moved.set(ref + p.id, ref + id);
        here.push({ ...p, id, name: isCustomDomain(domain) ? tr("{name} (imported)").replace("{name}", p.name || "") : newPresetName(here) });
        ids.add(id);
        copied++;
      } else {
        here.push(isCustomDomain(domain) ? p : { ...p, name: newPresetName(here) });
        ids.add(p.id);
        added++;
      }
    }
    if (here.length > before) {
      try { localStorage.setItem(key, JSON.stringify(here)); }
      catch (_) { noteInline(tr("this browser's storage is full - the change is on screen but was not saved")); return; }
      lists.add(key);
    }
  }
  if (lists.size) {
    syncShow({ lists, ids, removed: new Set() });
    if (typeof refreshRivenNames === "function") refreshRivenNames();
    syncSoon(0);
  }
  noteInline(tr("Imported: {added} added, {copied} added as copies beside a changed one here, {same} already here.")
    .replace("{added}", added).replace("{copied}", copied).replace("{same}", same));
}

/// The file picker behind "Import": a hidden input, opened by the button.
function savesPick() {
  const inp = document.createElement("input");
  inp.type = "file";
  inp.accept = "application/json,.json";
  inp.onchange = () => {
    const f = inp.files && inp.files[0];
    if (f) f.text().then(savesImport, () => noteInline(tr("That file could not be read.")));
  };
  inp.click();
}

document.addEventListener("click", (e) => {
  const b = e.target.closest && e.target.closest("[data-saves]");
  if (!b) return;
  e.preventDefault();
  if (b.dataset.saves === "export") savesExport();
  else if (b.dataset.saves === "import") savesPick();
});
