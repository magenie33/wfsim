// ---- views: '/' = the weapon list (home); '/weapons/<Wiki_Name>' = the
// BUILDER; '/weapons/<Wiki_Name>/simulator' = the SIMULATOR (tests the
// current build); '/weapons/<Wiki_Name>/optimizer' = the OPTIMIZER — one
// tab per module (the page's three modules, "Simulator
// sits in the middle"). URLs mirror wiki page names
// (display name, spaces → '_'); an id appears only where that name is shared
// by two weapons — see `urlSlug`.
// The weapon <select> stays the internal source of truth; the home grid
// and the path just drive it.
// The path rules — `wikiWeaponName`, `wikiSlug`, `weaponSlug` — are the
// headless part's (`87-headless.js`), so a link a query returns is this one.
const urlSlug = (w) => weaponSlug(META.weapons || [], w);
const weaponPath = (id) => headlessWeaponPath(META.weapons || [], id);
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
  // BOOT DRAWS THE FIRST PAGE, from whatever address a redirect left by then.
  if (!bootRouted) return;
  // AN EXTENSION'S PAGES ARE ROUTES TOO, so nothing is routed before it mounts.
  if (!extSettled) await extReady;
  leaveStartEdit();
  // A RIVEN APPRAISAL'S LINK (81-appraisal.js) becomes its weapon's optimizer.
  const appraiseCode = location.pathname.match(/^\/appraise\/([A-Za-z0-9]{3,12})\/?$/);
  if (appraiseCode) { await openAppraisal(appraiseCode[1], new URLSearchParams(location.search).has("freeze")); return; }
  // THE CONTRIBUTORS' LONG IMAGE (45-card-page.js), a card of no weapon.
  if (/^\/contributors\/card\/?$/.test(location.pathname)) {
    document.body.classList.add("on-card");
    document.documentElement.dataset.theme = "light";
    await renderContributorsCard();
    return;
  }
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
  // …AND A SHORT ONE, whose id boot already took off the address. The page is
  // drawn first on the same terms; the build lands when the store answers.
  if (SHARE_ENABLED && SHORT_SHARE_ID) {
    const id = SHORT_SHARE_ID, sig = SHORT_SHARE_SIG;
    SHORT_SHARE_ID = SHORT_SHARE_SIG = null;
    route();
    shortShareCode(id).then(async (code) => {
      if (!code) { presetToast(tr("that share link could not be found")); return; }
      if (await importShare(code) && sig) showShareBy(id, sig);
    });
    return;
  }
  // `/support` is a page of the SHELL, not a fourth module and not a weapon's
  // tab: it belongs to no weapon, so it sits beside the home grid rather than
  // under /weapons/<name>.
  const support = /^\/support\/?$/.test(location.pathname);
  const bench = /^\/benchmark\/?$/.test(location.pathname);
  const dl = /^\/download\/?$/.test(location.pathname);
  // `/utility/<tab>` — the game's live state and the reader's reminders on it.
  // Bare `/utility` is its first tab.
  const utilRoute = location.pathname.match(/^\/utility(?:\/([a-z]+))?\/?$/);
  const util = utilRoute && UTILITY_TABS.some(([t]) => t === (utilRoute[1] || UTILITY_TABS[0][0]))
    ? (utilRoute[1] || UTILITY_TABS[0][0]) : null;
  if (util && !utilRoute[1]) history.replaceState(history.state, "", `/utility/${util}`);
  // `/login`, `/signup`, `/reset`, `/account` — the account's pages, which
  // belong to no weapon (`17-account.js`) — and any page an extension mounts.
  const authKind = authKindOf(location.pathname);
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
  const m = (support || bench || dl || util || wfHit || opRoute || compHit || authKind) ? null : location.pathname.match(/^\/weapons\/([^/]+?)(\/simulator|\/optimizer|\/rivens|\/riven-analyst|\/enemies|\/benchmark|\/card)?\/?$/);
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
  // WHICH PAGE, as a kind and never an address: one point per kind per load.
  track("app.view", w ? `weapon_${mod || "builder"}` : support ? "support" : bench ? "benchmark"
    : dl ? "download" : util ? `utility_${util}` : wfHit ? "warframe" : opRoute ? "operator"
    : compHit ? "companion" : authKind ? authView(authKind) : "home");
  // WHICH WAY IN, where the link that led here names it (`?from=`): said once,
  // then taken off the address so a copied link does not carry it on.
  const door = new URLSearchParams(location.search).get("from");
  if (door !== null) {
    if (/^[a-z_]{1,24}$/.test(door)) track("door.open", door);
    const u = new URL(location.href);
    u.searchParams.delete("from");
    history.replaceState(history.state, "", u.pathname + u.search + u.hash);
  }
  document.body.classList.toggle("on-home", !w && !support && !bench && !dl && !util && !wfHit && !opRoute && !compHit && !authKind);
  document.body.classList.toggle("on-auth", !!authKind);
  $("auth-page").hidden = !authKind;
  document.body.classList.toggle("on-warframe", !!wfHit);
  document.body.classList.toggle("on-operator", opRoute);
  document.body.classList.toggle("on-companion", !!compHit);
  $("warframe-page").hidden = !wfHit;
  $("operator-page").hidden = !opRoute;
  $("companion-page").hidden = !compHit;
  document.body.classList.toggle("on-support", support);
  document.body.classList.toggle("on-benchmark", bench);
  document.body.classList.toggle("on-download", dl);
  document.body.classList.toggle("on-simulator", mod === "simulator");
  document.body.classList.toggle("on-optimizer", mod === "optimizer");
  document.body.classList.toggle("on-rivens", mod === "rivens");
  document.body.classList.toggle("on-ranalyst", mod === "riven-analyst");
  // THE LONG IMAGE is the page and nothing else, in the light theme whatever
  // the reader's choice: a chat shows it on either background.
  document.body.classList.toggle("on-card", mod === "card");
  if (mod === "card") document.documentElement.dataset.theme = "light";
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
    : bench ? "bench-page" : null;
  if (away) {
    const ask = ensurePageBodies(away);
    if (ask) ask.then(() => route());
  }
  $("home-page").hidden = !!w || support || bench || dl || util || !!wfHit || opRoute || !!compHit || !!authKind;
  $("support-page").hidden = !support;
  $("bench-page").hidden = !bench;
  $("download-page").hidden = !dl;
  $("utility-page").hidden = !util;
  // The nav says where you are. `data-nav` rather than a path compare: the
  // roster lives at "/" and a path compare there matches every page.
  const here = bench ? "benchmark" : util ? "utility" : (!w && !support && !dl && !wfHit && !opRoute && !compHit && !authKind) ? "home" : "";
  document.querySelectorAll(".tnav").forEach((a) => {
    a.classList.toggle("sel", a.dataset.nav === here);
  });
  document.querySelector(".config-page").hidden = !w;
  const modTitle = { simulator: " · Simulator", optimizer: " · Optimizer", rivens: " · Rivens", "riven-analyst": " · Riven Analyst", enemies: " · Enemies", benchmark: " · Benchmark" }[mod] || "";
  // The home title carries the SEARCH TERMS, not the headline: nobody looks
  // for "Simulacrum Prime", and the tab/result/share-card is the one place
  // that has to be found rather than enjoyed. The joke
  // stays on the page, which is where a player meets it.
  document.title = authKind ? `${tr({ login: "Sign in", signup: "Create an account", reset: "Reset your password",
    account: "Account settings", sync: "Cloud sync" }[authKind] || (EXT.pages[authKind] || {}).title || "")} — WFSim`
    : support ? `${tr("Support")} — WFSim`
    : dl ? `${tr("WFSim for Windows")} — WFSim`
    : bench ? `${tr("Benchmark")} — WFSim`
    : util ? `${tr(utilityTitle(util))} — WFSim`
    : wfHit ? `${wfHit.name} — WFSim`
    : compHit ? `${compHit.name} — WFSim`
    : opRoute ? `${tr("Operator")} — WFSim`
    : w ? `${w.name}${modTitle} — WFSim` : "WFSim — Warframe Calculator";
  trailPush();
  // A REMINDER IS WATCHED FROM EVERY PAGE, not only from /utility.
  reminderWatch();
  // The top bar's "Sign in" carries this page as where to come back to.
  renderAccountEntry();
  if (wfHit) {
    await showWarframe(wfHit.id);
    if (gen !== routeGen) return;
  } else if (compHit) {
    await showCompanion(compHit.id);
    if (gen !== routeGen) return;
  } else if (opRoute) {
    await showOperator();
    if (gen !== routeGen) return;
  } else if (authKind) {
    renderAuthPage(authKind);
  } else if (support) {
    renderSupport();
  } else if (dl) {
    renderDownloadPage();
  } else if (util) {
    showUtility(util);
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
    // …AND THE CARD'S OWN QUESTION, for the same reason (45-card-page.js).
    const cardAsk = mod === "card" ? cardParams() : null;
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
      `<a class="mtab ${mod === "riven-analyst" ? "sel" : ""}" href="${weaponPath(w.id)}/riven-analyst">${tr("Riven Analyst")}</a>` +
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
    if (mod === "riven-analyst") renderRivenAnalyst();
    if (mod === "card") renderCardPage(w, cardAsk);
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
const modSuffix = () => (location.pathname.match(/\/(simulator|optimizer|rivens|riven-analyst|benchmark)\/?$/) || [null, ""])[1];
const weaponModPath = (id) => weaponPath(id) + (modSuffix() ? "/" + modSuffix() : "");

// WHAT AN ARCANE SEAT IS CALLED: the pool it draws, which is a weapon type or
// a Kitgun's own. "add Kitgun arcane" is the sentence, so the Kitgun label is
// singular here.
const ARC_POOL_LABEL = { primary: "Primary", secondary: "Secondary", melee: "Melee",
  kitgun: "Kitgun", archgun: "Arch-Gun" };

/// THE FILTERS INSIDE ONE SLOT — the weapon type where the slot is not one, the
/// class, the tags — one state shape and one drawing, read by the home page's
/// sections and by the board, so a chip means the same thing on both pages.
const slotFilterNew = () => ({ type: "", cls: "", tags: [] });
const weaponClassKey = (w) => w.subtype || w.mod_class || "";
const weaponTagLabel = (id) => tr(((META && META.weapon_tags) || []).find((t) => t.id === id)?.name || id);
const slotFilterMatch = (st, w) => (!st.type || w.weapon_type === st.type)
  && (!st.cls || weaponClassKey(w) === st.cls)
  && st.tags.every((t) => (w.tags || []).includes(t));
const filterChip = (label, sel, attrs, n) => `<button type="button" class="bchip${sel ? " sel" : ""}" ${attrs}>${
  escHtml(label)}${n != null ? ` <span class="bcnt">${n}</span>` : ""}</button>`;
/// A ROW ONLY WHERE IT HAS A CHOICE TO OFFER: one type, one class or no tag in
/// the list is nothing to filter by, and a row of one chip reads as broken.
function slotFilterRows(ws, st) {
  const count = (f) => ws.filter(f).length;
  const chip = (label, sel, k, v, n) => filterChip(label, sel, `data-sf="${k}" data-v="${escHtml(v)}"`, n);
  const types = [...new Set(ws.map((w) => w.weapon_type).filter(Boolean))];
  const classes = [...new Set(ws.map(weaponClassKey).filter(Boolean))]
    .sort((a, b) => count((w) => weaponClassKey(w) === b) - count((w) => weaponClassKey(w) === a));
  const tags = ((META && META.weapon_tags) || []).map((t) => t.id)
    .filter((id) => ws.some((w) => (w.tags || []).includes(id)));
  const row = (label, body) => `<div class="slotf-row"><span class="slotf-lab">${escHtml(tr(label))}</span>${body}</div>`;
  return (types.length > 1 ? row("Type", chip(tr("All"), !st.type, "type", "")
      + types.map((t) => chip(tr(ARC_POOL_LABEL[t] || t), st.type === t, "type", t, count((w) => w.weapon_type === t))).join("")) : "")
    + (classes.length > 1 ? row("Class", chip(tr("All"), !st.cls, "cls", "")
      + classes.map((c) => chip(tr(c), st.cls === c, "cls", c, count((w) => weaponClassKey(w) === c))).join("")) : "")
    + (tags.length ? row("Tags", tags.map((t) => chip(weaponTagLabel(t), st.tags.includes(t), "tag", t,
      count((w) => (w.tags || []).includes(t)))).join("")) : "");
}
/// A CLICK ON ONE OF THOSE CHIPS: type and class are single choices a second
/// click clears; tags stack.
function slotFilterClick(st, el) {
  const k = el.dataset.sf, v = el.dataset.v;
  if (k === "tag") st.tags = st.tags.includes(v) ? st.tags.filter((t) => t !== v) : [...st.tags, v];
  else st[k] = st[k] === v ? "" : v;
}
/// A SEARCH MATCHES EITHER NAME: a reader on the Chinese page types "Braton" as
/// often as 布莱顿.
const nameMatches = (q, ...names) => !q || names.some((n) => n && String(n).toLowerCase().includes(q.trim().toLowerCase()));


/// ONE ROW PER MODULAR WEAPON, in a list that picks a WEAPON (search, the fight
/// roster). The chamber IS the weapon — one mastery track, one riven, one wiki
/// page — so its two slot entries are one row there, and which one it opens is
/// settled on the page by the Slot control. The home page groups by SLOT
/// instead, so a Kitgun appears there once per slot.
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

/// WHAT THE HOME PAGE IS FILTERED BY, kept for the visit: one search across
/// every slot, and each weapon slot's own chips.
const homeFilters = {};
let homeQuery = "";
/// A SLOT'S FILTERS START OPEN ON A WIDE SCREEN AND SHUT ON A PHONE, where
/// open chips would push the cards below the fold; a reader's own toggle wins.
const homeOpen = {};
const homeFiltersOpen = (id) => homeOpen[id] ?? !matchMedia("(max-width: 700px)").matches;

function renderHome() {
  renderHomeFacts();
  renderHomeHot();
  const box = $("home-sections");
  if (!box) return;
  const tag = (t) => `<span class="tag">${escHtml(tr(t))}</span>`;
  const frameCard = (f) => `<a class="wcard" href="${warframePath(f)}">
      ${imgTag(IMG(f.image), "wc-img")}
      <div class="wc-info"><div class="wc-name">${escHtml(f.name)}</div>
      <div class="wc-tags">${tag("Builder")}</div></div></a>`;
  const companionCard = (c) => `<a class="wcard" href="${companionPath(c)}">
      ${imgTag(null, "wc-img")}
      <div class="wc-info"><div class="wc-name">${escHtml(c.name)}</div>
      <div class="wc-tags">${tag("Builder")}</div></div></a>`;
  // THE OPERATOR IS ITS OWN SLOT, never a Warframe: a player has exactly one,
  // where a Warframe is one of many they can own.
  const operatorCard = () => `<a class="wcard" href="/operator">
      ${imgTag(IMG(META && META.operator_image), "wc-img")}
      <div class="wc-info"><div class="wc-name">${escHtml(tr("Operator"))}</div>
      <div class="wc-tags">${tag("Focus school")}</div></div></a>`;
  // A WEAPON CARD'S TAGS say what differs INSIDE its slot: its class, its tags
  // (`WeaponSpec::tags`), and its weapon type wherever the slot is not one. A
  // Kitgun sits in each slot its grips reach — one card per slot, each opening
  // that slot's entry. "Prime" is in the name already, so it is not repeated.
  const weaponCard = (w) => {
    const tags = [
      tag(weaponClassKey(w)),
      ...(w.tags || []).filter((t) => t !== "prime" && !(t === "kitgun" && weaponClassKey(w) === "Kitgun"))
        .map((t) => `<span class="tag">${escHtml(weaponTagLabel(t))}</span>`),
      w.weapon_type && w.weapon_type !== w.slot ? tag(ARC_POOL_LABEL[w.weapon_type] || w.weapon_type) : "",
    ].join("");
    return `<a class="wcard" href="/weapons/${urlSlug(w)}">
      ${imgTag(IMG(w.image), "wc-img")}
      <div class="wc-info">
        <div class="wc-name">${w.name}</div>
        <div class="wc-tags">${tags}</div>
      </div>
    </a>`;
  };
  // A SLOT'S FILTERS appear once it holds enough to need them.
  const FILTER_FROM = 7;
  const sections = ((META && META.equipment_slots) || []).map((slot) => {
    if (slot.holds === "warframe") {
      const all = wfFrames();
      return { slot, total: all.length, cards: all.filter((f) => nameMatches(homeQuery, f.name)).map(frameCard) };
    }
    if (slot.holds === "companion") {
      const all = compHosts();
      return { slot, total: all.length, cards: all.filter((c) => nameMatches(homeQuery, c.name)).map(companionCard) };
    }
    if (slot.holds === "operator") {
      return { slot, total: 1, cards: nameMatches(homeQuery, tr("Operator"), "Operator") ? [operatorCard()] : [] };
    }
    const all = (META.weapons || []).filter((w) => w.slot === slot.id);
    const st = homeFilters[slot.id] || (homeFilters[slot.id] = slotFilterNew());
    const kept = all.filter((w) => slotFilterMatch(st, w) && nameMatches(homeQuery, w.name, w.name_en));
    const rows = all.length >= FILTER_FROM ? slotFilterRows(all, st) : "";
    return { slot, total: all.length, cards: kept.map(weaponCard), filters: rows,
      active: !!st.type + !!st.cls + st.tags.length };
  }).filter((x) => x.total);
  // THE NAV NAMES EVERY SLOT ON THE PAGE with how many it holds, and jumps to
  // it; the search beside it narrows every slot at once.
  const nav = $("home-nav");
  if (nav) {
    const q = $("home-q");
    const typing = q && document.activeElement === q;
    nav.innerHTML = `<div class="slotf-row"><input id="home-q" class="slotf-search" type="search"
      placeholder="${escHtml(tr("Search by name"))}" value="${escHtml(homeQuery)}"></div>
      <div class="slotf-row">${sections.map((x) => filterChip(tr(x.slot.name), false,
      `data-jump="${escHtml(x.slot.id)}"`, x.total)).join("")}</div>`;
    nav.querySelectorAll("[data-jump]").forEach((el) => {
      el.onclick = () => { const t = $("home-" + el.dataset.jump); if (t) t.scrollIntoView(); };
    });
    const input = $("home-q");
    // ON A PHONE THE HERO FILLS THE SCREEN, so typing brings the search and
    // its results to the top rather than leaving them below the fold.
    onTyped(input, () => {
      homeQuery = input.value;
      renderHome();
      if (matchMedia("(max-width: 700px)").matches) $("home-nav").scrollIntoView({ block: "start" });
    });
    if (typing) { input.focus(); input.setSelectionRange(input.value.length, input.value.length); }
  }
  // A SLOT WITH NOTHING IN IT IS NOT LISTED: a heading over an empty grid
  // promises a roster that is not there. A search that empties a slot hides it
  // too; its own chips that empty it say so instead.
  box.innerHTML = sections.filter((x) => x.cards.length || (x.filters && !homeQuery)).map((x) => `
    <section class="wgroup" id="home-${x.slot.id}">
      <h2 class="home-h">${escHtml(tr(x.slot.name))} <span class="muted">${
        x.cards.length === x.total ? x.total : `${x.cards.length} / ${x.total}`}</span></h2>
      ${x.filters ? `<details class="slotf-box" data-slot="${escHtml(x.slot.id)}"${
        homeFiltersOpen(x.slot.id) ? " open" : ""}><summary>${escHtml(tr("Filters"))}${
        x.active ? ` <span class="bcnt">${x.active}</span>` : ""}</summary>${x.filters}</details>` : ""}
      ${x.cards.length ? `<div class="wgrid">${x.cards.join("")}</div>`
        : `<div class="sim-empty">${escHtml(tr("No weapon matches these filters."))}</div>`}
    </section>`).join("");
  box.querySelectorAll("details[data-slot]").forEach((d) => {
    d.ontoggle = () => { homeOpen[d.dataset.slot] = d.open; };
  });
  box.querySelectorAll("[data-sf]").forEach((el) => {
    el.onclick = () => { slotFilterClick(homeFilters[el.closest("[data-slot]").dataset.slot], el); renderHome(); };
  });
}

