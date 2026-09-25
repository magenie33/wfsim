// ---- views: '/' = the weapon list (home); '/weapons/<Wiki_Name>' = the
// BUILDER; '/weapons/<Wiki_Name>/simulator' = the SIMULATOR (tests the
// current build); '/weapons/<Wiki_Name>/optimizer' = the OPTIMIZER — one
// tab per module (the page's three modules, "Simulator
// sits in the middle"). URLs mirror wiki page names
// (display name, spaces → '_'); an id appears only where that name is shared
// by two weapons — see `urlSlug`.
// The weapon <select> stays the internal source of truth; the home grid
// and the path just drive it.
// The WIKI PAGE name behind a weapon's display name. A parenthesised
// qualifier is OURS — "Larkspur Prime (Atmosphere)" is one wiki page with two
// stat columns, and we ship the ground one — so it never reaches a URL.
// `build_site_app.py`'s `wiki_name` splits on the same " (".
const wikiWeaponName = (w) => (w.name_en || w.name).split(" (")[0];
const wikiSlug = (w) => wikiWeaponName(w).replace(/ /g, "_");

/// THE PATH SEGMENT A WEAPON LIVES AT — the wiki page name, and the ID where
/// that name is not this weapon's alone.
///
/// URLs mirror wiki page names and ids never appear, which holds for every
/// weapon whose display name is its own. Two Kitgun slots are ONE wiki page
/// and two roster entries, so the rule maps them onto one address and the
/// loser of that collision has NO URL AT ALL — nothing to link, nothing to
/// prerender. An id is uglier than a wiki name and it is reachable.
///
/// THE LOWEST ID KEEPS THE WIKI NAME, so `/weapons/Tombfinger` is a stable
/// address rather than one that follows roster order, and it stays the one the
/// Slot control swaps INSIDE — switching slots does not navigate, which is
/// what makes a Kitgun one page. `url_slug` in `build_site_app.py` is the same
/// rule and has to stay it, or a link points at a page that was never written.
let SLUG_OWNER = null;
const urlSlug = (w) => {
  // NOT CACHED BEFORE THE ROSTER LANDS: an empty `META.weapons` would freeze an
  // empty map, after which every weapon's path is its id forever.
  if (!SLUG_OWNER && (META.weapons || []).length) {
    SLUG_OWNER = new Map();
    for (const x of META.weapons || []) {
      const g = wikiSlug(x);
      if (!SLUG_OWNER.has(g) || x.id < SLUG_OWNER.get(g)) SLUG_OWNER.set(g, x.id);
    }
  }
  const g = wikiSlug(w);
  return !SLUG_OWNER || SLUG_OWNER.get(g) === w.id ? g : w.id;
};
const weaponPath = (id) => {
  const w = (META.weapons || []).find((x) => x.id === id);
  return "/weapons/" + (w ? urlSlug(w) : id);
};
function nav(path) {
  const moved = location.pathname !== path;
  if (moved) history.pushState(null, "", path);
  route();
  // A NEW PAGE STARTS AT THE TOP.
  //
  // `pushState` does not touch the scroll position, so a click from halfway
  // down the home grid lands on a weapon page ALREADY scrolled into the middle
  // of a panel nobody chose — on every in-app link, since they all come here.
  //
  // ONLY ON A FORWARD MOVE, and only when the path actually changed. `route()`
  // is called on its own for a re-render — a language switch, a share import,
  // every check in `scripts/` — and jumping those to the top would throw away a
  // position the reader never left. BACK and FORWARD go through `popstate`,
  // which never reaches this line, so the browser's own restored position
  // stands and `history.scrollRestoration` is left alone.
  //
  // AFTER the `pushState`, never before. The entry being LEFT records its
  // scroll at the moment the new one is pushed, so scrolling first would file
  // "top of page" against the page the reader is about to come back to.
  //
  // `instant`, not the default `auto`: `auto` defers to CSS, and a smooth
  // scroll from the bottom of a 500-weapon grid is an animation nobody asked
  // for on top of a navigation.
  if (moved) window.scrollTo({ top: 0, left: 0, behavior: "instant" });
}
let routeGen = 0;
async function route() {
  leaveStartEdit();
  // A SHARED LINK is answered before anything else on the page is drawn for
  // it, and the query is stripped afterwards so a refresh does not import the
  // same build a second time. `?b=` only ever ADDS — see importShare.
  const shared = SHARE_ENABLED && new URLSearchParams(location.search).get(SHARE_PARAM);
  // A LINK POSTED WHILE SHARING WAS ON still has to open something. The query
  // is stripped either way, so a refresh cannot retry it; with sharing off the
  // visitor gets the weapon's own page and a line saying why, which is a page
  // rather than a blank.
  if (!SHARE_ENABLED && new URLSearchParams(location.search).get(SHARE_PARAM)) {
    history.replaceState(null, "", location.pathname);
    setTimeout(() => presetToast(tr("sharing is off for now — this link opened the weapon instead")), 900);
  }
  if (shared) {
    history.replaceState(null, "", location.pathname);
    // DRAW THE PAGE FIRST, then land the payload into it. Returning here
    // instead left the visitor on the home grid staring at nothing until they
    // refreshed: `importShare` fills the editor in, but which module is
    // VISIBLE is this function's job and it had been skipped. The query is
    // already stripped, so this re-entry takes the ordinary path.
    route();
    importShare(shared);
    return;
  }
  // `/support` is a page of the SHELL, not a fourth module and not a weapon's
  // tab: it belongs to no weapon, so it sits beside the home grid rather than
  // under /weapons/<name>.
  const support = /^\/support\/?$/.test(location.pathname);
  const bench = /^\/benchmark\/?$/.test(location.pathname);
  const dl = /^\/download\/?$/.test(location.pathname);
  // `/thanks` is a page of the SHELL like the three above it, and it is a URL
  // meant to be PASTED — into a video description, into the group — so it is a
  // real address rather than a section somebody has to scroll to.
  const thx = /^\/thanks\/?$/.test(location.pathname);
  // `/warframes/<Wiki_Name>` — the Warframe builder, a page of its own that
  // belongs to no weapon. Matched by id or by the wiki name, like a weapon.
  const wfRoute = location.pathname.match(/^\/warframes\/([^/]+?)\/?$/);
  const wfSlug = wfRoute && decodeURIComponent(wfRoute[1]).trim().toLowerCase().replace(/[\s-]+/g, "_");
  const wfHit = wfSlug && wfFrames().find((f) =>
    f.id === wfSlug || f.name.toLowerCase().replace(/[\s-]+/g, "_") === wfSlug) || null;
  const opRoute = /^\/operator\/?$/.test(location.pathname);
  // `/companions/<Name>` — a companion host, the layer between a robotic weapon
  // and the Warframe that owns it.
  const compRoute = location.pathname.match(/^\/companions\/([^/]+?)\/?$/);
  const compSlug = compRoute && decodeURIComponent(compRoute[1]).trim().toLowerCase().replace(/[\s-]+/g, "_");
  const compHit = compSlug && compHosts().find((c) =>
    c.id === compSlug || c.name.toLowerCase().replace(/[\s-]+/g, "_") === compSlug) || null;
  const m = (support || bench || dl || thx || wfHit || opRoute || compHit) ? null : location.pathname.match(/^\/weapons\/([^/]+?)(\/simulator|\/optimizer|\/rivens|\/enemies|\/benchmark)?\/?$/);
  // A hand-typed URL is not the canonical slug. Fold case and treat spaces
  // (and their %20) as underscores, so "/weapons/Dual Toxocyst" reaches the
  // same weapon as "/weapons/Dual_Toxocyst" instead of silently falling back
  // to the home grid — which reads as "the site sent me somewhere else".
  const slug = m && decodeURIComponent(m[1]).trim().toLowerCase().replace(/[\s-]+/g, "_");
  // AN ID ANSWERS FIRST. `/weapons/tombfinger_secondary` is that weapon's own
  // address; matching the wiki slug first would hand it to the entry that owns
  // the shared name and the second slot would be unreachable again.
  const w = slug && ((META.weapons || []).find((x) => x.id === slug)
    || (META.weapons || []).find((x) => wikiSlug(x).toLowerCase() === slug));
  // The active module: "" = builder, "simulator", "optimizer".
  const mod = (w && m[2]) ? m[2].slice(1) : "";
  // THE ROWS BEFORE ANYTHING IS SHOWN. Waiting after the page was unhidden put
  // the PREVIOUS weapon on screen for the whole round trip. A route that another
  // navigation overtook while it waited draws nothing.
  const gen = ++routeGen;
  if (w) await loadWeaponBoard(w.id);
  if (gen !== routeGen) return;
  document.body.classList.toggle("on-home", !w && !support && !bench && !dl && !thx && !wfHit && !opRoute && !compHit);
  document.body.classList.toggle("on-warframe", !!wfHit);
  document.body.classList.toggle("on-operator", opRoute);
  document.body.classList.toggle("on-companion", !!compHit);
  $("warframe-page").hidden = !wfHit;
  $("operator-page").hidden = !opRoute;
  $("companion-page").hidden = !compHit;
  document.body.classList.toggle("on-support", support);
  document.body.classList.toggle("on-thanks", thx);
  document.body.classList.toggle("on-benchmark", bench);
  document.body.classList.toggle("on-download", dl);
  document.body.classList.toggle("on-simulator", mod === "simulator");
  document.body.classList.toggle("on-optimizer", mod === "optimizer");
  document.body.classList.toggle("on-rivens", mod === "rivens");
  document.body.classList.toggle("on-enemies", mod === "enemies");
  // THE WEAPON'S OWN BOARD, and NOT `on-benchmark`: that class already
  // means the site-wide ranking page, and one class with two meanings is
  // two pages hiding each other's blocks.
  document.body.classList.toggle("on-wbench", mod === "benchmark");
  // THE BODY THIS DOCUMENT DID NOT SHIP, before anything draws into it. The
  // four pages below are the ones a document carries only when it IS one of
  // them (`11-page-bodies.js`), and the route runs again once the file lands —
  // `ensurePageBodies` answers null the second time, so it cannot loop.
  const away = dl ? "download-page" : support ? "support-page"
    : thx ? "thanks-page" : bench ? "bench-page" : null;
  if (away) {
    const ask = ensurePageBodies(away);
    if (ask) ask.then(() => route());
  }
  $("home-page").hidden = !!w || support || bench || dl || thx || !!wfHit || opRoute || !!compHit;
  $("support-page").hidden = !support;
  $("thanks-page").hidden = !thx;
  $("bench-page").hidden = !bench;
  $("download-page").hidden = !dl;
  // The nav says where you are. `data-nav` rather than a path compare: the
  // roster lives at "/" and a path compare there matches every page.
  const here = bench ? "benchmark" : (!w && !support && !dl && !thx && !wfHit && !opRoute && !compHit) ? "home" : "";
  document.querySelectorAll(".tnav").forEach((a) => {
    a.classList.toggle("sel", a.dataset.nav === here);
  });
  document.querySelector(".config-page").hidden = !w;
  const modTitle = { simulator: " · Simulator", optimizer: " · Optimizer", rivens: " · Rivens", enemies: " · Enemies", benchmark: " · Benchmark" }[mod] || "";
  // The home title carries the SEARCH TERMS, not the headline: nobody looks
  // for "Simulacrum Prime", and the tab/result/share-card is the one place
  // that has to be found rather than enjoyed. The joke
  // stays on the page, which is where a player meets it.
  document.title = support ? `${tr("Support")} — WFSim`
    : thx ? `${tr("Thank you")} — WFSim`
    : dl ? `${tr("WFSim for Windows")} — WFSim`
    : bench ? `${tr("Benchmark")} — WFSim`
    : wfHit ? `${wfHit.name} — WFSim`
    : compHit ? `${compHit.name} — WFSim`
    : opRoute ? `${tr("Operator")} — WFSim`
    : w ? `${w.name}${modTitle} — WFSim` : "WFSim — Warframe Calculator";
  trailPush();
  if (wfHit) {
    await showWarframe(wfHit.id);
    if (gen !== routeGen) return;
  } else if (compHit) {
    await showCompanion(compHit.id);
    if (gen !== routeGen) return;
  } else if (opRoute) {
    await showOperator();
    if (gen !== routeGen) return;
  } else if (support) {
    renderSupport();
  } else if (thx) {
    renderThanksPage();
  } else if (dl) {
    renderDownloadPage();
  } else if (bench) {
    // THE ONLY SURFACE THAT RANKS ACROSS WEAPONS, and therefore the only one
    // that needs every weapon's rows. It draws first with whatever is in hand
    // and redraws when the rest lands, so the page is never a spinner.
    showBenchBoard();
  } else if (w) {
    // `?mode=` — HOW the linked build is played, carried by the link that made
    // it. A board row is a weapon AND a mode ("Burston Prime, base form"), so a
    // link that dropped the second half landed on a page measuring something
    // else.
    //
    // The QUERY, not a path segment: the path mirrors the wiki's page name and
    // its next segment is already the module (`/simulator`), so a mode there
    // would be a third meaning for one slot.
    //
    // READ BEFORE THE SWITCH. Loading a weapon restores its preset, and that
    // rewrites the address to the weapon's plain path — so by the time the
    // weapon is on screen the query is already gone, and the mode with it.
    const wantMode = new URLSearchParams(location.search).get("mode");
    // WHICH RULER, from a board row. A row is a build AND the ruler it was
    // measured under; arriving with only the build gives you a number you
    // cannot reproduce, and arriving with neither gave you the FIRST ruler's
    // leader whichever board you clicked.
    const wantBench = new URLSearchParams(location.search).get("bench");
    // WHICH OF THE TWO LEADERS — see the link builder. Null when the link
    // predates the parameter, which means "whichever row leads".
    const rivenParam = new URLSearchParams(location.search).get("riven");
    const wantRiven = rivenParam === null ? null : rivenParam === "1";
    // THE ROWS BEFORE THE WEAPON, because everything below reads them
    // SYNCHRONOUSLY: `switchWeapon` builds this weapon's presets out of them
    // and `applyBenchLink` opens one BY NAME. A board that lands afterwards is
    // a deep link that opens the wrong build, which is what a board link exists
    // to stop. Awaited at the top of this function, before the page is shown.
    if ($("weapon").value !== w.id) {
      switchWeapon(w.id);
    }
    if (wantBench) applyBenchLink(w, wantBench, wantMode, wantRiven);
    if (wantMode && (w.modes || []).includes(wantMode)) {
      if (wantMode !== mode) {
        mode = wantMode;
        renderMode();
        renderMods();
        refreshPanel();
      }
      // ...and put it back on the address bar, which the restore just cleared.
      // Kept rather than stripped: the link has to survive a refresh and a
      // bookmark, and `renderMode` rewrites it when the visitor changes mode so
      // it never says something the page is not doing.
      if (new URLSearchParams(location.search).get("mode") !== wantMode) {
        const q = new URLSearchParams();
        if (wantBench) q.set("bench", wantBench);
        if (rivenParam !== null) q.set("riven", rivenParam);
        q.set("mode", wantMode);
        history.replaceState(null, "", `${location.pathname}?${q}`);
      }
    }
    $("module-tabs").innerHTML =
      `<a class="mtab ${mod === "" ? "sel" : ""}" href="${weaponPath(w.id)}">${tr("Builder")}</a>` +
      `<a class="mtab ${mod === "simulator" ? "sel" : ""}" href="${weaponPath(w.id)}/simulator">${tr("Simulator")}</a>` +
      `<a class="mtab ${mod === "optimizer" ? "sel" : ""}" href="${weaponPath(w.id)}/optimizer">${tr("Optimizer")}</a>` +
      `<a class="mtab ${mod === "rivens" ? "sel" : ""}" href="${weaponPath(w.id)}/rivens">${tr("Rivens")}</a>` +
      `<a class="mtab ${mod === "benchmark" ? "sel" : ""}" href="${weaponPath(w.id)}/benchmark">${tr("Benchmark")}</a>` +
      `<a class="mtab ${mod === "enemies" ? "sel" : ""}" href="${weaponPath(w.id)}/enemies">${tr("Enemies")}</a>`;
    // Arriving on the simulator: refresh its build summary (builder edits
    // don't re-render sim views while they are hidden). The SCENARIO is one
    // state shared with the optimizer, so each tab redraws its own copy of
    // those fields on arrival — the other tab may have moved them, and the
    // tabs are CSS-hidden rather than re-rendered.
    if (mod === "simulator") renderSim();
    if (mod === "optimizer") { renderOptFight(); updateOptEstimate(); }
    if (mod === "rivens") renderRivens();
    if (mod === "enemies") renderEnemies();
    if (mod === "benchmark") renderWeaponBench();
    // THE PAGE'S OWN ANSWER, on every module: it describes the WEAPON and
    // not the tab, so it stands above them rather than inside one.
    renderWeaponDoc();
  } else {
    renderHome();
  }
  // LAST: the jump menu indexes whatever the page has just become, and which
  // blocks a module shows is decided above.
  renderJump();
}

// The current module's path suffix — weapon switches (search, select,
// preset load) keep the visitor on the tab they are on.
const modSuffix = () => (location.pathname.match(/\/(simulator|optimizer|rivens|benchmark)\/?$/) || [null, ""])[1];
const weaponModPath = (id) => weaponPath(id) + (modSuffix() ? "/" + modSuffix() : "");

// The home grid groups by EQUIPMENT SLOT in loadout order:
// one flat list stops being readable as soon as the roster holds more than one
// slot's worth. A slot with no weapons renders nothing at all rather than an
// empty heading, and an unknown slot still gets its weapons shown.
// Equipment slots, in the order the arsenal shows them. "sentinel" is a real
// slot, not a kind of primary: a sentinel weapon rides the companion and draws
// from the rifle mod pool without ever occupying a weapon slot.
const SLOT_ORDER = ["primary", "secondary", "kitgun", "melee", "sentinel", "archgun"];
const SLOT_LABEL = { primary: "Primary", secondary: "Secondary", melee: "Melee",
  kitgun: "Kitguns", sentinel: "Sentinel Weapons", archgun: "Arch-Guns", other: "Other" };

// WHAT AN ARCANE SEAT IS CALLED, which is the weapon-group label everywhere but
// one: the group heading is the plural "Kitguns" because it heads a list of
// them, and "add Kitguns arcane" is not a sentence. One override rather than a
// second full table, so a seat added later still reads.
const ARC_POOL_LABEL = { ...SLOT_LABEL, kitgun: "Kitgun" };

/// WHICH GROUP A WEAPON IS LISTED UNDER — its slot, except that a MODULAR
/// weapon gets its own.
///
/// A Kitgun is one weapon with a roster entry per slot, so listing it by slot
/// puts the same name and the same picture in two groups, both linking to the
/// same page. Its own group says what it actually is, and says it once.
const weaponCategory = (w) => (w.assembly ? "kitgun" : (w.slot || ""));

/// ONE CARD PER MODULAR WEAPON. The chamber IS the weapon — one mastery track,
/// one riven, one wiki page — so its two slot entries are one entry here, and
/// which one the card opens is settled on the page by the Slot control.
///
/// IT OPENS THE PRIMARY, stated rather than left to roster ORDER — which
/// happens to be alphabetical and happens to put `_primary` before
/// `_secondary`. That is the right answer arrived at by accident, and an
/// accident stops being right the day a chamber is named so that it is not.
const oneCardPerChamber = (ws) => {
  const best = new Map();
  ws.forEach((w) => {
    const key = w.assembly ? w.assembly.chamber : w.id;
    const cur = best.get(key);
    if (!cur || (w.slot === "primary" && cur.slot !== "primary")) best.set(key, w);
  });
  // A Map keeps insertion order, so this is the roster's order with the
  // duplicates removed — the same list every caller sorted before.
  return [...best.values()];
};

function renderHome() {
  renderHomeFacts();
  const frames = $("warframe-grid");
  if (frames) {
    frames.innerHTML = `<section class="wgroup"><div class="wgrid">${wfFrames().map((f) => `<a class="wcard" href="${warframePath(f)}">
      ${imgTag(IMG(f.image), "wc-img")}
      <div class="wc-info"><div class="wc-name">${escHtml(f.name)}</div>
      <div class="wc-tags"><span class="tag">${escHtml(tr("Builder"))}</span></div></div></a>`).join("")}</div></section>`;
  }
  const companions = $("companion-grid");
  if (companions) {
    companions.innerHTML = `<section class="wgroup"><div class="wgrid">${compHosts().map((c) => `<a class="wcard" href="${companionPath(c)}">
      ${imgTag(null, "wc-img")}
      <div class="wc-info"><div class="wc-name">${escHtml(c.name)}</div>
      <div class="wc-tags"><span class="tag">${escHtml(tr("Builder"))}</span></div></div></a>`).join("")}</div></section>`;
  }
  // THE OPERATOR IS ITS OWN GROUP, never a Warframe: a player has exactly one,
  // where a Warframe is one of many they can own.
  const operator = $("operator-grid");
  if (operator) {
    operator.innerHTML = `<section class="wgroup"><div class="wgrid"><a class="wcard" href="/operator">
      ${imgTag(IMG(META && META.operator_image), "wc-img")}
      <div class="wc-info"><div class="wc-name">${escHtml(tr("Operator"))}</div>
      <div class="wc-tags"><span class="tag">${escHtml(tr("Focus school"))}</span></div></div></a></div></section>`;
  }
  const grid = $("weapon-grid");
  if (!grid) return;
  const card = (w) => {
    const tags = [
      // TRANSLATED, like the group heading above it. `tr` falls through to the
      // English for a subtype no overlay names, so this costs nothing on the
      // ones nobody has translated and stops the Kitgun card reading half in
      // one language.
      `<span class="tag">${escHtml(tr(w.subtype || w.mod_class))}</span>`,
      w.uses_evo2 ? `<span class="tag">Incarnon</span>` : "",
      w.sentinel ? `<span class="tag">Sentinel</span>` : "",
    ].join("");
    return `<a class="wcard" href="/weapons/${urlSlug(w)}">
      ${imgTag(IMG(w.image), "wc-img")}
      <div class="wc-info">
        <div class="wc-name">${w.name}</div>
        <div class="wc-tags">${tags}</div>
      </div>
    </a>`;
  };
  const all = oneCardPerChamber(META.weapons || []);
  const groups = SLOT_ORDER
    .map((s) => [s, all.filter((w) => weaponCategory(w) === s)])
    .filter(([, ws]) => ws.length);
  const rest = all.filter((w) => !SLOT_ORDER.includes(weaponCategory(w)));
  if (rest.length) groups.push(["other", rest]);
  grid.innerHTML = groups.map(([slot, ws]) => `
    <section class="wgroup">
      <h3 class="wgroup-h">${tr(SLOT_LABEL[slot] || slot)}</h3>
      <div class="wgrid">${ws.map(card).join("")}</div>
    </section>`).join("");
}

