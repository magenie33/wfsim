// ---- THE COMPANION HOST -------------------------------------------------
//
// `/companions/<Name>`: what carries a robotic weapon — a Sentinel or a MOA —
// and therefore the holder a Sentinel weapon's build links, exactly as any other
// weapon links a Warframe. Its stat block is the wielder floor
// (`META.sentinel_floor`). A companion's own mods are not modelled yet, so a
// build of one carries nothing else. docs/WARFRAMES.md §Companions.
const COMP_BUILDS = "companions";
let comp = null;
let compActive = "";

const compHosts = () => (META && META.companions) || [];
const compHost = (id) => compHosts().find((c) => c.id === id) || null;
const companionPath = (c) => "/companions/" + String(c.name).replace(/ /g, "_");
const compList = (id) => presetListWithIds(COMP_BUILDS, id);
const compBlank = (id) => ({ companion: id });
const compNormalize = (st, id) => ({ companion: id });

async function showCompanion(id) {
  if (!comp || comp.companion !== id) {
    const list = compList(id);
    const last = localStorage.getItem(presetActiveKey(COMP_BUILDS, id));
    const p = presetToOpen(list, new URLSearchParams(location.search).get("build"), last);
    compActive = p ? p.name : "";
    comp = compNormalize(p ? p.state : null, id);
  }
  renderCompanion();
}

function renderCompanion() {
  const c = compHost(comp.companion);
  const f = META.sentinel_floor || {};
  $("comp-name").textContent = c.name;
  $("comp-tags").innerHTML = [["Health", f.health], ["Shield", f.shield], ["Armor", f.armor]]
    .map(([k, v]) => `<span class="tag">${escHtml(tr(k))} ${v}</span>`).join("");
  renderCompPresetBar();
}

// ---- builds ----
/// FRAMED BY A WEAPON PAGE, which build is open is the weapon's link: told to the
/// parent whenever a real one is open.
function compAnnounceBuild() {
  if (!EMBED || window.parent === window || !comp) return;
  const p = compList(comp.companion).find((x) => x.name === compActive);
  if (p) window.parent.postMessage({ wfsim: "wielder-build", frame: comp.companion, id: p.id }, location.origin);
}

function compBarCfg() {
  return {
    domain: COMP_BUILDS,
    label: tr("Companion builds"),
    noun: "companion",
    load: () => compList(comp.companion),
    usedBy: (p) => linkersOfCompanionPreset(comp.companion, p.id),
    store: (ps) => storePresetList(COMP_BUILDS, opWithIds(ps), comp.companion),
    active: () => compActive,
    setActive: (n) => {
      compActive = n;
      if (EMBED) compAnnounceBuild();
      else localStorage.setItem(presetActiveKey(COMP_BUILDS, comp.companion), n);
    },
    snapshot: () => JSON.parse(JSON.stringify(comp)),
    apply: (st) => compApply(st),
    blank: () => compBlank(comp.companion),
    rerender: renderCompPresetBar,
  };
}
const renderCompPresetBar = () => renderPresetBarIn($("preset-bar-companions"), compBarCfg());
function compApply(st) {
  comp = compNormalize(st, comp.companion);
  renderCompanion();
}
