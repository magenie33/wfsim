// ---- topbar weapon search: one box, rows navigate ----------------------
/// SOLO AND TOGETHER, TWO SLIDERS (docs/UI.md §"Compute: solo and together"): the share of the
/// device the reader's own computing takes, and the share computing together
/// takes, each 10–100%, together with its switch and the battery choice. One
/// control wherever a `.cs` host is — the top bar's menu and the compute page —
/// drawn once into an empty host and PAINTED in place after, so a slider being
/// dragged is never redrawn under the pointer. A percentage, never a core
/// count: the together share is the share on any device (`communityPower`).
function computeSharesHtml() {
  const row = (key, name, note, extra = "") => `<div class="cs-row" data-cs-row="${key}">
    <div class="cs-h"><b>${escHtml(tr(name))}</b>${extra}<span class="cs-v" data-cs-v="${key}"></span></div>
    <input type="range" min="10" max="100" step="10" data-cs="${key}" aria-label="${escHtml(tr(name))}">
    <small>${escHtml(tr(note))}</small></div>`;
  return row("solo", "Solo", "When you press Run, it computes your own builds")
    + (WASM ? row("together", "Compute together", "With the volunteers, it runs simulation and optimization tasks for every player",
      `<label class="cs-on"><input type="checkbox" data-cs="on">${escHtml(tr("on"))}</label>`)
      + `<label class="cs-bat"><input type="checkbox" data-cs="battery"> ${escHtml(tr("Also compute together on battery"))}</label>
        <div class="cs-sum" data-cs-sum></div>` : "");
}
function paintComputeShares(host, solo = computePct, together = communityShare()) {
  const on = boardVerifyOn();
  const set = (sel, f) => host.querySelectorAll(sel).forEach(f);
  set('[data-cs="solo"]', (el) => { if (document.activeElement !== el) el.value = String(solo); });
  set('[data-cs="together"]', (el) => { if (document.activeElement !== el) el.value = String(together); el.disabled = !on; });
  set('[data-cs="on"]', (el) => { el.checked = on; });
  set('[data-cs="battery"]', (el) => { el.checked = computeOnBattery(); el.disabled = !on; });
  set('[data-cs-v="solo"]', (el) => { el.textContent = `${solo}%`; });
  set('[data-cs-v="together"]', (el) => { el.textContent = on ? `${together}%` : tr("off"); });
  set('[data-cs-row="together"]', (el) => el.classList.toggle("cs-off", !on));
  const sum = solo + (on ? together : 0);
  set("[data-cs-sum]", (el) => {
    el.textContent = !on ? "" : tr(sum <= 100 ? "{sum}% in all · the two never touch" : "{sum}% in all · while both run, they share in proportion")
      .replace("{sum}", sum);
  });
}
/// DRAWN AGAIN only when its words changed — the language's strings arrive after
/// the menu is first drawn — so a drag in progress is never cut off by a paint.
function renderComputePicker() {
  const words = tr("Solo");
  document.querySelectorAll(".cs").forEach((host) => {
    if (!host.firstElementChild || host.dataset.words !== words) { host.innerHTML = computeSharesHtml(); host.dataset.words = words; }
    paintComputeShares(host);
  });
}
/// …AND WHAT IT DOES: a slider moved is said at once, and set when it is let go
/// — solo's share drops the reader's pool (`setComputePct`), so not on every
/// step of a drag. Switching together on is the yes to the statement the card
/// shows (`setBoardVerify`), as it always was in this menu.
document.addEventListener("input", (e) => {
  const el = e.target.closest && e.target.closest('.cs input[type="range"]');
  if (!el) return;
  const host = el.closest(".cs"), v = Number(el.value);
  paintComputeShares(host, el.dataset.cs === "solo" ? v : computePct, el.dataset.cs === "together" ? v : communityShare());
});
document.addEventListener("change", (e) => {
  const el = e.target.closest && e.target.closest(".cs [data-cs]");
  if (!el) return;
  const k = el.dataset.cs;
  if (k === "solo") setComputePct(Number(el.value));
  else if (k === "together") setCommunityShare(Number(el.value));
  else if (k === "on") setBoardVerify(el.checked);
  else if (k === "battery") setComputeOnBattery(el.checked);
  renderComputePicker();
});

/// How long typing must pause before the list is drawn again: a list of every
/// weapon redrawn per keystroke is what made typing stutter on a phone.
const WSEARCH_DEBOUNCE_MS = 120;

function initWeaponSearch() {
  const input = $("wsearch-input"), panel = $("wsearch-panel"), listEl = $("wsearch-list");
  if (!input) return;
  input.placeholder = tr("Search…");
  const countEl = $("wsearch-count"), closeEl = $("wsearch-close");
  if (closeEl) closeEl.textContent = tr("Close");
  // ONE BOX, NO FILTER ROW: a kind is a word in the box like a name is — the
  // search text carries every weapon's type — and on a phone a row of chips
  // above the list was the list's own room.
  const renderList = () => {
    const q = input.value.trim().toLowerCase();
    // ONE ROW PER MODULAR WEAPON, for the reason the home grid shows one card:
    // two entries with one name, one picture and one destination is a list that
    // looks like it has a bug in it.
    const list = oneCardPerChamber(META.weapons || [])
      .filter((w) => searchHit(w, q))
      .sort((a, b) => a.name.localeCompare(b.name));
    if (countEl) countEl.textContent = tr("{n} results").replace("{n}", list.length);
    listEl.innerHTML = list.map((w) => `
      <div class="opt" data-id="${w.id}">
        ${imgTag(IMG(w.image), "mod")}
        <div class="info"><div class="mn">${w.name}</div><div class="me"><div>${escHtml(tr(w.subtype || ""))}</div></div></div>
      </div>`).join("") || `<div class="sim-empty">${tr("No matches")}</div>`;
  };
  // WHILE THE RESULTS ARE OPEN the page says so (`wsearch-open`), so what
  // floats over a phone's screen can step aside for them.
  let timer = null;
  const shut = () => { clearTimeout(timer); panel.hidden = true; document.body.classList.remove("wsearch-open"); };
  // AN EMPTY BOX OPENS NOTHING: every weapon at once answers no question, and
  // on a phone it covered the page the moment the box was touched.
  const open = () => {
    if (!input.value.trim()) return shut();
    panel.hidden = false; document.body.classList.add("wsearch-open"); renderList();
  };
  input.addEventListener("focus", open);
  input.addEventListener("input", () => {
    clearTimeout(timer);
    if (!input.value.trim()) return shut();
    timer = setTimeout(open, WSEARCH_DEBOUNCE_MS);
  });
  // THE WAY OUT where the list covers the screen: closed, emptied, and the
  // keyboard put away.
  if (closeEl) closeEl.addEventListener("click", (e) => { e.stopPropagation(); shut(); input.value = ""; input.blur(); });
  listEl.addEventListener("click", (e) => {
    const row = e.target.closest(".opt");
    if (!row) return;
    shut();
    input.value = "";
    switchWeapon(row.dataset.id);
    nav(weaponModPath(row.dataset.id));
  });
  document.addEventListener("click", (e) => {
    if (!e.target.closest(".wsearch")) shut();
  });
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Escape" || panel.hidden || imeComposing(e)) return;
    shut();
    input.blur();
  });
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
  document.addEventListener("keydown", (e) => { if (e.key === "Escape" && !imeComposing(e)) set(false); });
  // A DESTINATION closes it — the page moves and the menu would be left open
  // over the new one. A CONTROL does not: after switching the theme you can
  // still want the language, and neither moves the page.
  document.querySelector("#topmenu .topnav")
    .addEventListener("click", () => set(false));
  // Above the breakpoint the panel is `display:contents` again and the class
  // means nothing — but it would still be there on the way back down.
  addEventListener("resize", () => { if (innerWidth > 768) set(false); });
})();

// THE READER'S OWN GROUP FIRST in the community menu. A Chinese reader will
// never click Discord and an English reader will never click QQ, so the one
// that matches the display language leads — ordered, never dropped. English is
// the source everywhere in this repo, so the markup ships the English order and
// this swaps it; a language change reloads the page, so it runs exactly once.
function applyCommunityOrder() {
  const primary = $("community-primary"), alt = $("community-alt");
  const qq = document.querySelector(".qq-link"), dc = document.querySelector(".dc-link");
  if (!primary || !alt || !qq || !dc) return;
  const [near, far] = LANG === "zh" ? [qq, dc] : [dc, qq];
  primary.appendChild(near);
  alt.appendChild(far);
}
applyCommunityOrder();

// THE TOPBAR'S GROUP MENUS (`.tbmore`): community and settings. At 768px and
// below each is `display:contents` and its button is not drawn, so this only
// does anything on a desktop — but it binds either way, because a resize
// crosses the breakpoint without reloading. One open at a time.
(function () {
  const boxes = [...document.querySelectorAll(".topbar .tbmore")];
  const set = (box, open) => {
    box.classList.toggle("open", open);
    const btn = box.querySelector(".tbmore-btn");
    if (btn) btn.setAttribute("aria-expanded", open ? "true" : "false");
  };
  for (const box of boxes) {
    const btn = box.querySelector(".tbmore-btn");
    if (!btn) continue;
    btn.addEventListener("click", (e) => {
      e.stopPropagation();
      const open = !box.classList.contains("open");
      boxes.forEach((b) => set(b, b === box && open));
    });
    // A DESTINATION closes it; a control inside does not, so the language can
    // follow the theme without reopening the menu.
    box.querySelector(".tbmore-panel").addEventListener("click", (e) => {
      if (e.target.closest("a[href]")) set(box, false);
    });
  }
  // `#dd-popover` counts as INSIDE: the language and compute pickers draw into
  // the shared popover, which is a SIBLING of the panel in the DOM, so picking
  // from one would close the panel out from under the control being used.
  document.addEventListener("click", (e) => {
    for (const box of boxes) if (!box.contains(e.target) && !e.target.closest("#dd-popover")) set(box, false);
  });
  document.addEventListener("keydown", (e) => { if (e.key === "Escape" && !imeComposing(e)) boxes.forEach((b) => set(b, false)); });
  addEventListener("resize", () => boxes.forEach((b) => set(b, false)));
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

/// THE ENGINE'S FIRST ANSWER, asked until it comes or plainly cannot.
///
/// `api` already re-asks on a fresh lane, but a download the network dropped
/// fails every lane in the same second; a pause between rounds outlasts a
/// blip. Read without an answer, `META.defaults` threw a TypeError the reader
/// saw as "could not start" over a stack, so the failure says what it is.
async function bootMeta() {
  let r = null;
  for (const wait of [0, 2000, 5000]) {
    if (wait) await new Promise((res) => setTimeout(res, wait));
    r = await api("/api/meta");
    if (r && Array.isArray(r.weapons) && r.defaults) return r;
  }
  bootReported = true;
  if (window.__wfsimBootFailed) {
    window.__wfsimBootFailed(
      "The engine could not start — usually the network dropped its download. Reload the page to try again. / 计算引擎没能启动，通常是网络中断了它的下载。请刷新页面重试。",
      String((r && r.error) || "no answer from the engine"),
    );
  }
  throw new Error("engine did not answer /api/meta");
}

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
  META = await bootMeta();
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
  // THE ENGINE ANSWERED: the denominator every other point is read against,
  // with how the page was reached and how long the reader waited for it.
  track("app.boot", usageArrival(), Math.round(performance.now()));
  const device = usageDevice();
  track("app.device", device.subject, device.cores);
  // HOW MUCH IS SAVED HERE, which is what a sync allowance would be measured in.
  const saved = savedCounts();
  track("presets.saved", "presets", saved.presets);
  track("presets.saved", "customs", saved.customs);
  const boardBoot = loadBoard(bootWeapon);
  applyI18n();
  fillSelect("weapon", META.weapons);
  initWeaponSearch();
  const d = META.defaults;
  $("weapon").value = bootWeapon;
  arcanes = arcanesFor(bootWeapon, d.arcane);
  sim = defaultScenario();
  await boardBoot;            // before presets: the board's rows ARE build presets
  applyWeapon(bootWeapon, d.mods);

  // THROUGH THE DOOR, like every other consumer — see "The agent door". The
  // control and an agent must move the page the same way, and the only thing
  // that keeps them the same is that there is one implementation to move it.
  $("weapon").addEventListener("change", () => { wfsim.do("builder.weapon.set", { weapon: $("weapon").value }); });
  $("run-sim").addEventListener("click", runSim);
  $("run-opt").addEventListener("click", runOptimize);
  // updateOptEstimate is also the search's auto-save, so these land in the
  // active preset the same way the starts do.
  $("opt-cand-runs").value = String(optRun.candidate_runs);
  $("opt-cand-runs").title = tr("how many fights each candidate gets while the search compares them. 1 is ten times faster and noisier; the answers are re-measured in the simulator either way");
  $("opt-cand-runs").addEventListener("change", () => setOptSizes({ candidate_runs: Number($("opt-cand-runs").value) || 10 }));
  $("opt-finalists").value = String(optRun.finalists);
  $("opt-finalists").title = tr("how many builds the search answers with: the best that many of every build it scored, re-measured in the simulator");
  $("opt-finalists").addEventListener("change", () => setOptSizes({ finalists: Number($("opt-finalists").value) }));
  // (The final round runs at the simulator's Runs, so it has no box here; nor
  // do threads, because the topbar's compute picker is the one place that
  // question is answered.)
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
  document.addEventListener("keydown", (e) => { if (e.key === "Escape" && !imeComposing(e)) closePopovers(); });
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
    // `data-native` leaves the app — a sign-in round trip, a page outside it.
    const a = e.target.closest('a[href^="/"]:not([data-native])');
    if (!a || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey || e.button !== 0) return;
    e.preventDefault();
    nav(a.getAttribute("href"));
  });
  // EVERY BLOCK AND EVERY SECTION FOLDS, and the menu that indexes them is one
  // panel for the whole page — so both are wired once, before the first route
  // draws anything for them to report on.
  wireStaticFolds();
  wireJump();
  bootRouted = true;
  route();
  // A language switch reloads the page; the pre-switch build is stashed in
  // sessionStorage and restored here so nothing is lost.
  const stash = sessionStorage.getItem("wfsim-lang-stash");
  if (stash) {
    sessionStorage.removeItem("wfsim-lang-stash");
    try { restoreState(JSON.parse(stash)); } catch (_) {}
  }
}
