// ---- topbar weapon search: filter chips + sort, rows navigate ----------
/// THE COMPUTE PICKER, in the topbar beside the language.
///
/// In the TOPBAR because it is the page's setting rather than any module's —
/// the same place the language and the theme live, and the same reason.
///
/// EVERY ROW NAMES THE LANE COUNT IT BUYS, because a percentage alone is not
/// something a reader can act on: "50%" says nothing until it says "4 of 8".
/// The button shows the LANES rather than the share, for the same reason —
/// that is the number that decides how hot the phone gets.
///
/// …AND IT SAYS WHEN IT IS GUESSING. A browser that will not report its core
/// count (iOS Safari 11–15.3, or a privacy mode) gets a fallback of 4 and a
/// line saying so, rather than a confident number nobody measured.
function renderComputePicker() {
  const host = $("compute-select");
  if (!host) return;
  const { n: cores, known } = detectedCores();
  const steps = computeSteps();
  // LANES OVER CORES on the face — `14/28` states the setting and its ceiling
  // in one, and it is plain text: an emoji or a dingbat is the one glyph the
  // platform draws in its own colour and its own shape, which is why this
  // topbar has none.
  const face = known ? `${poolSize()}/${cores}` : `${poolSize()}/?`;
  host.outerHTML = ddButton("compute-select", {
    value: String(computePct),
    title: known
      ? tr("how much of this machine the page may use — it changes how FAST an answer arrives, never what the answer is")
      : tr("this browser will not say how many cores it has, so 4 is assumed — the share still applies"),
    // A LANE COUNT, not a share: it is what the setting actually does.
    placeholder: face,
    items: steps.map((s) => ({
      value: String(s.pct),
      label: `${s.pct}%`,
      hint: known
        ? tr("{n} of {c} cores").replace("{n}", s.lanes).replace("{c}", cores)
        : tr("{n} lanes · cores unknown, assuming {c}")
            .replace("{n}", s.lanes).replace("{c}", cores),
    })),
    onPick: (v) => setComputePct(v),
  });
  // The face shows the LANES; the list shows the shares.
  const btn = $("compute-select");
  const el = btn && btn.querySelector(".dd-v");
  if (el) el.textContent = face;
}

function initWeaponSearch() {
  const input = $("wsearch-input"), panel = $("wsearch-panel"),
        tools = $("wsearch-tools"), listEl = $("wsearch-list");
  if (!input) return;
  input.placeholder = tr("Search…");
  let flt = "all", srt = "az";
  const cats = [...new Set((META.weapons || []).map((w) => w.subtype || w.mod_class))];
  tools.innerHTML =
    `<span class="pchip sel" data-f="all">${tr("All")}</span>` +
    cats.map((c) => `<span class="pchip" data-f="${c}">${c}</span>`).join("") +
    ddButton("wsearch-sort", {
      value: srt,
      items: [{ value: "az", label: tr("Name A→Z") }, { value: "za", label: tr("Name Z→A") }],
      onPick: (v) => { srt = v; renderList(); },
    });
  const renderList = () => {
    const q = input.value.trim().toLowerCase();
    // ONE ROW PER MODULAR WEAPON, for the reason the home grid shows one card:
    // two entries with one name, one picture and one destination is a list that
    // looks like it has a bug in it.
    const list = oneCardPerChamber(META.weapons || [])
      .filter((w) => flt === "all" || (w.subtype || w.mod_class) === flt)
      .filter((w) => searchHit(w, q))
      .sort((a, b) => (srt === "za" ? -1 : 1) * a.name.localeCompare(b.name));
    listEl.innerHTML = list.map((w) => `
      <div class="opt" data-id="${w.id}">
        ${imgTag(IMG(w.image), "mod")}
        <div class="info"><div class="mn">${w.name}</div><div class="me"><div>${w.subtype || ""}</div></div></div>
      </div>`).join("") || `<div class="sim-empty">${tr("No matches")}</div>`;
  };
  const open = () => { panel.hidden = false; renderList(); };
  input.addEventListener("focus", open);
  input.addEventListener("input", open);
  tools.addEventListener("click", (e) => {
    const chip = e.target.closest(".pchip");
    if (!chip) return;
    flt = chip.dataset.f;
    tools.querySelectorAll(".pchip").forEach((c) => c.classList.toggle("sel", c === chip));
    renderList();
  });
  tools.addEventListener("change", (e) => {

  });
  listEl.addEventListener("click", (e) => {
    const row = e.target.closest(".opt");
    if (!row) return;
    panel.hidden = true;
    input.value = "";
    switchWeapon(row.dataset.id);
    nav(weaponModPath(row.dataset.id));
  });
  document.addEventListener("click", (e) => {
    if (!e.target.closest(".wsearch")) panel.hidden = true;
  });
  document.addEventListener("keydown", (e) => { if (e.key === "Escape") panel.hidden = true; });
}

// language dropdown (top right, beside the theme toggle): switching
// reloads with the current build stashed and restored.
(function () {
  // What the DOCUMENT says it is, so a screen reader picks the right voice
  // and a crawler indexes the page under the language it is actually in.
  // The tag ships as `en` and the page may be zh from the first paint.
  document.documentElement.lang = LANG;
  const host = $("lang-select");
  if (!host) return;
  // The topbar's language control is a dropdown like every other, so it is
  // drawn by the same component — the LAST native select on the page, and the
  // most visible one.
  //
  // DEFERRED by a microtask, because this block runs DURING script evaluation
  // and the component it calls is declared further down: `const` and `function
  // expression` bindings are in their temporal dead zone until the line that
  // creates them runs, so drawing here directly threw. A microtask runs after
  // the whole script has evaluated, which is the first moment any part of the
  // file may call any other part.
  queueMicrotask(() => {
    const el = $("lang-select");
    if (!el) return;
    el.outerHTML = ddButton("lang-select", {
      value: LANG,
      title: "Language / 语言",
      items: [{ value: "en", label: "English" }, { value: "zh", label: "中文" }],
      onPick: (v) => {
        localStorage.setItem("wfsim-lang", v);
        try { sessionStorage.setItem("wfsim-lang-stash", JSON.stringify(snapshotState())); } catch (_) {}
        location.reload();
      },
    });
    // …AND THE COMPUTE PICKER BESIDE IT, drawn in the same deferred block and
    // for the same reason: both are the page's own settings and both call a
    // component declared further down this file.
    renderComputePicker();
  });
})();

// Official QQ community group: the topbar mark and the footer entry LINK
// to the join page (qm.qq.com deep-links into the QQ app, Discord-invite
// style); the footer's ⧉ copies the raw number for manual in-QQ search.
// Feedback is inline: no native dialogs.
const QQ_GROUP = "995078378";
(function () {
  const btn = $("qq-copy-foot");
  if (!btn) return;
  btn.addEventListener("click", async () => {
    try { await navigator.clipboard.writeText(QQ_GROUP); } catch (_) {
      const ta = document.createElement("textarea");
      ta.value = QQ_GROUP; document.body.appendChild(ta);
      ta.select(); document.execCommand("copy"); ta.remove();
    }
    btn.textContent = "✓";
    setTimeout(() => { btn.textContent = "⧉"; }, 1200);
  });
})();

// The phone's topbar menu. It opens ONE container that holds the real
// controls — see index.html — so there is nothing here to keep in sync with a
// second copy; this only decides when the container is a box.
(function () {
  const bar = document.querySelector(".topbar"), btn = $("menu-toggle");
  if (!bar || !btn) return;
  const set = (open) => {
    bar.classList.toggle("menu-open", open);
    btn.setAttribute("aria-expanded", open ? "true" : "false");
  };
  btn.addEventListener("click", (e) => {
    e.stopPropagation();
    set(!bar.classList.contains("menu-open"));
  });
  // `#dd-popover` counts as INSIDE: the language dropdown draws into the
  // shared popover, which is a sibling of the menu in the DOM, so a click on
  // "中文" would otherwise close the panel out from under the control.
  document.addEventListener("click", (e) => {
    if (!e.target.closest("#topmenu, #menu-toggle, #dd-popover")) set(false);
  });
  document.addEventListener("keydown", (e) => { if (e.key === "Escape") set(false); });
  // A DESTINATION closes it — the page moves and the menu would be left open
  // over the new one. A CONTROL does not: after switching the theme you can
  // still want the language, and neither moves the page.
  document.querySelector("#topmenu .topnav")
    .addEventListener("click", () => set(false));
  // Above the breakpoint the panel is `display:contents` again and the class
  // means nothing — but it would still be there on the way back down.
  addEventListener("resize", () => { if (innerWidth > 700) set(false); });
})();

// THE COMMUNITY LINK A READER CAN ACT ON GOES ON THE BAR;
// the other one is in the overflow. A Chinese reader will never click Discord
// and an English reader will never click QQ, so putting both on the bar spent
// two of its slots to serve half a reader each.
//
// ORDERED, NEVER DROPPED — an English reader still finds the QQ group one
// click away. English is the source everywhere in this repo, so the markup
// ships the English order and this swaps it; a language change reloads the
// page, so it runs exactly once.
function applyCommunityOrder() {
  const primary = $("community-primary"), alt = $("community-alt");
  const qq = document.querySelector(".qq-link"), dc = document.querySelector(".dc-link");
  if (!primary || !alt || !qq || !dc) return;
  const [near, far] = LANG === "zh" ? [qq, dc] : [dc, qq];
  primary.appendChild(near);
  alt.appendChild(far);
}
applyCommunityOrder();

// The topbar overflow. Below 700px it is `display:contents` and the button is
// not drawn, so this only does anything on a desktop — but it binds either
// way, because a resize crosses the breakpoint without reloading.
(function () {
  const box = $("tbmore"), btn = $("tbmore-toggle");
  if (!box || !btn) return;
  const set = (open) => {
    box.classList.toggle("open", open);
    btn.setAttribute("aria-expanded", open ? "true" : "false");
  };
  btn.addEventListener("click", (e) => {
    e.stopPropagation();
    set(!box.classList.contains("open"));
  });
  // `#dd-popover` counts as INSIDE, for the same reason the phone menu counts
  // it: the compute picker draws into the shared popover, which is a SIBLING
  // of this panel in the DOM, so picking a share would close the panel out
  // from under the control that is being used.
  document.addEventListener("click", (e) => {
    if (!e.target.closest("#tbmore, #dd-popover")) set(false);
  });
  document.addEventListener("keydown", (e) => { if (e.key === "Escape") set(false); });
  addEventListener("resize", () => set(false));
})();

// theme
(function () {
  const saved = localStorage.getItem("wfsim-theme");
  if (saved) document.documentElement.setAttribute("data-theme", saved);
  // ONE SETTING, WHEREVER IT IS THROWN. The record's own window is a second
  // document of this app, so the flip reaches it too — and it carries the same
  // button, which calls this. A window that came up light under a page the
  // reader had set to dark is the same setting answered twice.
  window.flipTheme = () => {
    const cur = document.documentElement.getAttribute("data-theme");
    const dark = cur === "dark" || (!cur && matchMedia("(prefers-color-scheme: dark)").matches);
    const next = dark ? "light" : "dark";
    document.documentElement.setAttribute("data-theme", next);
    localStorage.setItem("wfsim-theme", next);
    syncRecordChrome();
  };
  $("theme-toggle").addEventListener("click", () => flipTheme());
})();

async function init() {
  // BEFORE ANYTHING TOUCHES THE DOM: a page from another build has markup
  // this file does not know, and the failure that produces is unreadable.
  checkBuildMatches();
  // …AND THEN THE FOOTER SAYS THE WHOLE THING. After the guard, never before:
  // the guard reads the token the page was SERVED with, and this overwrites it.
  {
    const el = $("build-stamp");
    if (el && BUILD_SHA !== "dev") {
      el.textContent = `${BUILD_SHA} · ${RELEASE_ID}`;
      el.title = `commit ${BUILD_SHA} · release ${RELEASE_ID} · page ${BUILD_ID}`;
    }
  }
  META = await api("/api/meta");
  // …AND THE FLOOR THE ARENA DRAWS ON IS THE ENGINE'S, before anything is drawn.
  adoptBodyRadius(META);
  {
    let all = null;
    try { all = await api("/api/i18n"); } catch (_) { all = null; }
    I18N = (LANG !== "en" && all && all[LANG]) || null;
    // Keep the OTHER locales' name tables for the search blob. English is not
    // among them — it is already on every entity as `name` or `name_en`.
    ALT_NAMES = all
      ? Object.entries(all).filter(([l]) => l !== LANG).map(([, v]) => v)
      : null;
    applyNameOverlay();
  }
  // THE BOARD NEEDS NEITHER WASM NOR i18n, so it is in flight while both
  // finish rather than queued behind the whole engine boot.
  //
  // AND IT IS THE ROUTE'S WEAPON, not the default one: the board's rows ARE
  // build presets, so the file has to be in hand before `initPresets` runs for
  // the weapon actually being opened. Fetching the default's would leave every
  // deep link a page whose benchmark builds arrive after the presets that were
  // supposed to contain them.
  //
  // …AND THE EDITOR BOOTS INTO THAT SAME WEAPON. Booting the default and
  // letting `route` switch drew the roster's first weapon on screen for as long
  // as anything in between waited on the network.
  const bootWeapon = routeWeaponId() || META.defaults.weapon;
  const boardBoot = loadBoard(bootWeapon);
  applyI18n();
  fillSelect("weapon", META.weapons);
  initWeaponSearch();
  const d = META.defaults;
  $("weapon").value = bootWeapon;
  arcanes = arcanesFor(bootWeapon, d.arcane);
  evoSel = { 1: null, 2: null, 3: null, 4: null, ...(d.evolutions || {}) };
  sim = defaultScenario();
  await boardBoot;            // before presets: the board's rows ARE build presets
  applyWeapon(bootWeapon, d.mods);

  // THROUGH THE DOOR, like every other consumer — see "The agent door". The
  // control and an agent must move the page the same way, and the only thing
  // that keeps them the same is that there is one implementation to move it.
  $("weapon").addEventListener("change", () => { wfsim.do("builder.weapon.set", { weapon: $("weapon").value }); });
  $("run-sim").addEventListener("click", runSim);
  $("run-opt").addEventListener("click", runOptimize);
  $("opt-mod-filter").addEventListener("input", renderOptModList);
  $("opt-arc-filter").addEventListener("input", renderOptArcanes);
  // How full a build must be, as a RANGE. The two ends are one setting: a
  // ceiling below the floor is not a scope, so each end pushes the other.
  // THE CEILING MAY BE 0, like every other axis's: 0–0 is "search it empty,
  // and keep the marks" — the bare weapon, without unmarking everything.
  $("opt-size").addEventListener("input", () => setOptSizes({ size: Number($("opt-size").value) || 0 }));
  // THE FLOOR STARTS AT 0, AND 0 IS THE DEFAULT. Every
  // other axis here treats "nothing marked" as the EMPTY option — an unmarked
  // exilus slot stays empty, an unmarked arcane seat searches no arcane — and
  // the mods axis alone answered it with an error. `updateOptEstimate` has
  // claimed "an empty scope = the bare weapon, still a legal search" since it
  // was written, and a floor of 1 made that sentence false.
  // It costs nothing anywhere else: the moment anything is marked, the DERIVED
  // floor (every required mod, plus one pooled) is at least 1 and wins, so 0
  // and 1 differ in exactly the one case above.
  $("opt-min").addEventListener("input", () => setOptSizes({ min: Number($("opt-min").value) || 0 }));
  // updateOptEstimate is also the scope's auto-save, so finalists lands in the
  // active preset the same way every other search setting does.
  $("opt-finalists").value = optRun.finalists;
  $("opt-finalists").title = tr("how many builds survive to the last round — each is then run at the final-round run count beside this");
  $("opt-finalists").addEventListener("input", () => setOptSizes({ finalists: Number($("opt-finalists").value) || 10 }));
  // (The final-round run count is not wired here: it is a PREFERENCE and
  // draws itself — `renderOptRuns`, outside both halves because it is in
  // neither preset. There is no CPU-thread box, because the topbar's compute
  // picker is the one place that question is answered.)
  // BEFORE ANY LIST IS READ: a riven is the FAMILY's as of 2026-08-25, and the
  // lists already on this machine are filed per weapon. It needs `META`, which
  // is why it is called here rather than beside the migrations it belongs with.
  foldRivensIntoOneList();
  initPresets();
  reattachOptimize(); // resume progress display if a server-side job survives a reload
  $("auto-forma").addEventListener("click", () => autoForma().then(() => renderMods()));
  $("clear-mods").addEventListener("click", () => { clearMods(); });
  document.addEventListener("click", (e) => {
    // `.rv-pick` opens the same popover from the riven tab, so its own click
    // must not be the click that closes it again.
    if (!e.target.closest(".popover") && !e.target.closest(".slot") && !e.target.closest(".rv-pick")) closePopovers();
  });
  document.addEventListener("keydown", (e) => { if (e.key === "Escape") closePopovers(); });
  window.addEventListener("popstate", route);
  // Reloading or closing the tab KILLS a run in progress: the worker dies with
  // the page. That is not a limitation we can engineer around — measured
  // 2026-07-30, a SharedWorker is terminated too, the moment its last client
  // disconnects, whether or not it is busy.
  //
  // The browser's own unload prompt is the only guard that runs before the page
  // goes, and it cannot be replaced by an inline one — so this is the single
  // place the project's no-native-dialogs rule does not reach. It only fires
  // while something is actually running.
  window.addEventListener("beforeunload", (e) => {
    if (optJobId == null) return;
    e.preventDefault();
    e.returnValue = ""; // required by older engines to trigger the prompt
  });
  // In-app navigation: any same-origin root-relative link routes client-side
  // (modified clicks — new tab etc. — keep native behavior; a full page load
  // also works thanks to the server's SPA fallback).
  document.addEventListener("click", (e) => {
    const a = e.target.closest('a[href^="/"]');
    if (!a || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey || e.button !== 0) return;
    e.preventDefault();
    nav(a.getAttribute("href"));
  });
  // EVERY BLOCK AND EVERY SECTION FOLDS, and the menu that indexes them is one
  // panel for the whole page — so both are wired once, before the first route
  // draws anything for them to report on.
  wireStaticFolds();
  wireJump();
  route();
  // A language switch reloads the page; the pre-switch build is stashed in
  // sessionStorage and restored here so nothing is lost.
  const stash = sessionStorage.getItem("wfsim-lang-stash");
  if (stash) {
    sessionStorage.removeItem("wfsim-lang-stash");
    try { restoreState(JSON.parse(stash)); } catch (_) {}
  }
}

