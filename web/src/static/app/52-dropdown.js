// ---- popovers ----
// EVERY popover, found by class rather than by a list that a new one has to
// remember to join — the enemy picker opened and would not close, because it
// was not on the list.
//
// `keep` is the node the new panel is anchored to: a popover that CONTAINS it
// stays open, which is what lets a dropdown live inside a picker (the mod
// picker's Sort control is inside `#mod-popover`) without the act of opening
// it closing the thing it belongs to.
function closePopovers(keep) {
  document.querySelectorAll(".popover").forEach((p) => {
    if (keep && p.contains(keep)) return;
    p.hidden = true;
  });
}

// ---- THE dropdown -------------------------------------------------------
//
// ONE choose-one control for the whole site, and not a new component: the
// panel is `.popover`, the search bar `.addbar`, the rows `.combo-menu .opt`,
// so a dropdown cannot drift from the pickers because it IS them.
//
// The SEARCH BAR appears on its own rule rather than always: a list of two is
// read at a glance and a search box over it is furniture, while six is where
// scanning stops being instant. `search: true` forces it for a growing list.
/// The trigger. Emits a button that LOOKS like the select it replaces, and
/// registers what opening it should show. Callers re-render by innerHTML, so
/// registration happens on every draw rather than once.
// EVERYTHING THIS COMPONENT IMPLEMENTS, named once.
//
// A native `<select>` gives `option.disabled` away for free, and a component
// that silently IGNORES the field keeps every caller passing it while the
// options stay clickable — an extra key costs the author nothing, which makes
// such a loss silent. So an unknown key is an ERROR rather than a no-op: it
// means the author expected a behaviour this component does not have, and loud
// is cheap here.
const DD_CFG_KEYS = ["value", "items", "dataK", "title", "placeholder", "onPick", "search",
  "data", "disabled", "axis", "axisLabel"];
// `badge` IS THE ONE RAW-HTML FIELD, and it exists because escaped text cannot
// carry a quick-calc gain. A gain is MARKUP — a class that colours
// it, a title that explains the band it was measured to, and a "…" while it is
// still being measured — so flattening it into `hint` would print "≈+3.1% ±7.2%"
// as words and lose the three shapes that make a chip readable at a glance.
//
// Raw means the CALLER escapes. The only producer is `gainChipFor`, which
// builds its own markup and escapes its own inputs; anything else passed here
// is an injection the component cannot see.
// `key` IS THE SCAN'S NAME FOR THIS ROW, and it is what makes the gain chip
// LIVE. Baking the chip into `badge` when the list is registered puts it
// before the scan that fills it has started, so the rows keep their first
// answer for ever and a freshly opened list shows none at all. It is computed
// at RENDER, like the mod picker's, and the order with it.
const DD_ITEM_KEYS = ["value", "label", "hint", "disabled", "group", "badge", "key"];

function ddCheck(id, cfg) {
  const stray = (obj, known) => Object.keys(obj).filter((k) => !known.includes(k));
  const bad = stray(cfg, DD_CFG_KEYS);
  if (bad.length) throw new Error(`dd "${id}": unknown config ${bad.join(", ")}`);
  (cfg.items || []).forEach((i, n) => {
    const b = stray(i, DD_ITEM_KEYS);
    if (b.length) throw new Error(`dd "${id}" item ${n}: unknown field ${b.join(", ")} — the component does not implement it`);
  });
}

function ddButton(id, cfg) {
  ddCheck(id, cfg);
  ddReg.set(id, cfg);
  const cur = cfg.items.find((i) => String(i.value) === String(cfg.value));
  // `value=` is not decoration: `HTMLButtonElement.value` REFLECTS it, so the
  // scenario panel's generic `[data-k]` binding — which reads `el.value` and
  // listens for `change` — keeps working unchanged across the swap. That is
  // also what keeps `el.disabled = true` meaningful on the optimizer tab,
  // where the whole fight is drawn read-only.
  // `data:` is the general form of `dataK` — a caller whose binding reads some
  // OTHER attribute (the ability element list reads `data-wfel`) declares it
  // here rather than rewriting the markup afterwards, which is what the first
  // swap did and is how a component grows a caller that escapes its own values. `disabled` is the same argument: the read-only optimizer tab
  // sets it on the ELEMENT, and a control that is born disabled had no way to
  // say so.
  const data = Object.entries(cfg.data || {})
    .map(([k, v]) => ` data-${k}="${escHtml(String(v))}"`).join("");
  return `<button type="button" class="dd" id="${id}" data-dd="${id}" value="${
    escHtml(String(cfg.value ?? ""))}"${cfg.dataK ? ` data-k="${escHtml(cfg.dataK)}"` : ""}${
    data}${cfg.disabled ? " disabled" : ""}${
    cfg.title ? ` title="${escHtml(cfg.title)}"` : ""}><span class="dd-v">${
    escHtml(cur ? cur.label : (cfg.placeholder || "—"))}</span><span class="dd-c">▾</span></button>`;
}

function ddRender(id, query) {
  const cfg = ddReg.get(id);
  if (!cfg) return;
  const q = (query || "").trim().toLowerCase();
  // SEARCH SPANS EVERYTHING THE ROW SHOWS — its label, its hint and its group.
  // A list of official builds is `#3 · Incarnon cycle` under a ruler whose name
  // carries the enemy, the level and the metric, so "no aim", "thrax" and
  // "cycle" all have to find rows or the search only works for people who
  // already know where things are.
  // RANKED AT RENDER, so a scan that lands while the list is open re-orders it
  // — the mod picker's own behaviour, and the reason a half-filled ranking
  // marks itself with "…" rather than pretending to be final.
  const items = cfg.axis
    ? [...cfg.items]
        .map((i) => ({ ...i, id: i.key, name: i.label }))
        .sort((a, b) => gainSort(a, b, ["gain", "name"]))
    : cfg.items;
  const hits = items.filter((i) => {
    if (!q) return true;
    const blob = [i.label, i.hint, i.group].filter(Boolean).join(" ").toLowerCase();
    // …and space-insensitively, for the same reason the mod list is: a
    // localized label carries spaces the player does not type.
    return blob.includes(q) || squash(blob).includes(squash(q));
  });
  // A DISABLED item stays LISTED and greyed: "the weapon has no Incarnon form
  // while that mod is on it" is information, and a vanished option is not. It
  // is `.dis`, and `.dis` is what the click binding below skips — the one
  // native `<select>` behaviour this component has to reproduce by hand, and
  // the one it silently dropped when the selects were replaced.
  //
  // IT KEEPS ITS `data-v`. Dropping the value was the first way this was
  // written, and it left a greyed row identifiable only by the words on it —
  // which is the thing that broke `check_opt_gain` the day an evolution got a
  // Chinese name. An option carries its identity whether or not it can be
  // clicked; being clickable is a separate fact and lives in the class.
  // GROUPED, where the data has a grouping. A flat list is right up to about a
  // screenful; the official builds are rulers x modes x ten and the rulers are
  // meant to reach dozens, and past that a reader cannot SCAN even a list they
  // can search. The header is emitted when the group CHANGES, so grouping costs
  // nothing when no item carries one, and it survives filtering — a search that
  // leaves two rulers standing still says which is which.
  let group = null;
  // THE SCAN'S PROGRESS BELONGS WHERE THE WORK IS BEING READ, which for a
  // ranked axis is this list — the same place the mod and arcane pickers put
  // theirs. `scanStrip` draws nothing unless this axis is the one running, and
  // a dropdown that ranks nothing declares no `axis` and gets none.
  const strip = cfg.axis
    ? scanStrip(gainScan, cfg.axis, hits.map((i) => i.key)) : "";
  $("dd-menu").innerHTML = strip + (hits.length
    ? hits.map((i) => {
      const head = (i.group && i.group !== group)
        ? `<div class="ddgroup">${escHtml(i.group)}</div>` : "";
      group = i.group || group;
      const chip = cfg.axis && i.key ? gainChipFor(i.key, cfg.axisLabel || "") : "";
      return head + `<div class="opt${String(i.value) === String(cfg.value) ? " cur" : ""}${
        i.disabled ? " dis" : ""}" data-v="${escHtml(String(i.value))}">
        <div class="info"><div class="mn">${escHtml(i.label)}${chip}${i.badge || ""}</div>${
          i.hint ? `<div class="me"><div>${escHtml(i.hint)}</div></div>` : ""}</div>
      </div>`;
    }).join("")
    : `<div class="opt dis">${escHtml(tr("no matches"))}</div>`);
  $("dd-menu").querySelectorAll(".opt[data-v]:not(.dis)").forEach((el) => {
    el.onclick = (e) => {
      e.stopPropagation();
      $("dd-popover").hidden = true;   // only THIS panel — a parent picker stays
      // The TRIGGER THAT OPENED THIS, not `$(id)`: the scenario's fields are
      // drawn twice — once by the simulator, once read-only by the optimizer —
      // so an id resolves to whichever copy is earlier in the document, which
      // is not necessarily the one being used. The anchor is never ambiguous.
      const btn = $("dd-popover")._anchor;
      if (btn) btn.value = el.dataset.v;
      // IT ANNOUNCES ITSELF, ALWAYS — the same event a native `<select>` fires,
      // which is what every binding this component replaced was written against.
      //
      // It was `if (cfg.dataK)`, on the reasoning that the scenario's generic
      // binding was the only listener. It was not: the ability element list
      // binds `change` on `[data-wfel]`, so converting that select produced a
      // control that opened, closed, showed the picked element on its face and
      // changed nothing. A component that reflects `value` but
      // withholds the event is one every future caller has to be told about.
      //
      // Every listener on this page is bound per element by attribute, so a
      // bubbling `change` from a dropdown nobody listens to reaches nothing.
      if (btn) btn.dispatchEvent(new Event("change", { bubbles: true }));
      if (cfg.onPick) cfg.onPick(el.dataset.v);
    };
  });
}

function ddOpen(id, anchor) {
  const cfg = ddReg.get(id);
  if (!cfg) return;
  closePopovers(anchor);
  const pop = $("dd-popover");
  pop._anchor = anchor;
  place(pop, anchor);
  // Match the trigger's width where it is wider than the panel's default, so
  // the panel reads as belonging to the control rather than floating near it.
  pop.style.minWidth = `${Math.max(anchor.getBoundingClientRect().width, 180)}px`;
  const wantSearch = cfg.search === true || cfg.items.length >= DD_SEARCH_MIN;
  $("dd-addbar").hidden = !wantSearch;
  const s = $("dd-search");
  s.value = "";
  s.oninput = () => ddRender(id, s.value);
  ddRender(id, "");
  if (wantSearch) s.focus();
  // A LIST THAT DECLARES AN AXIS MEASURES ITSELF, and it is the DECLARATION
  // that does it rather than the control: a mode is a plain dropdown and a
  // valence is a card, and both are one axis of the same build.
  //
  // ON OPEN, not on render. The options live inside a closed list, so an eager
  // scan spends a fight per candidate on numbers nobody can see, and does it
  // again on every repaint. The list opens FIRST and the scan starts after, so
  // the rows are there immediately carrying their "…" chips.
  if (cfg.axis) {
    ensureGains(cfg.axis, () => {
      // REPAINT THE OPEN LIST, and only while it is open — the mod picker's
      // own rule. A closed list has nobody reading it, and its next open
      // re-renders.
      if (!$("dd-popover").hidden) ddRender(id, $("dd-search").value);
    }, true);
  }
}

// Delegated, because every caller re-renders its trigger by innerHTML — a
// listener bound to the node would be thrown away with it, and rebinding after
// each draw is the kind of thing that gets forgotten on the eighth dropdown.
// CAPTURE, not bubble: several containers call `stopPropagation` on click to
// survive their own innerHTML redraws (`#quick-calc` and `.picker-tools` both
// do), and a bubbling listener would never see a trigger inside one of them.
document.addEventListener("click", (e) => {
  const t = e.target.closest("[data-dd]");
  if (!t || t.disabled) return;
  e.stopPropagation();
  if (!$("dd-popover").hidden && $("dd-popover")._anchor === t) {
    $("dd-popover").hidden = true;    // clicking the open trigger closes it
    return;
  }
  ddOpen(t.id, t);
}, true);
/// PUT A POPOVER UNDER ITS ANCHOR — AND INSIDE THE SCREEN.
///
/// The anchor's left edge and nothing else is right on a desktop and is
/// HORIZONTAL OVERFLOW on a phone: a mod slot's ⋯ sits at x=295 of a 360px
/// screen, so its 200px menu runs to 495, the DOCUMENT becomes 495 wide, the
/// browser fits that into 360, and the Swap/Remove the reader was reaching for
/// is off the right edge. "The card is too long to reach its top right" and
/// "the menu makes the page smaller" are one bug, which is why the fix is here
/// rather than in either surface.
///
/// THE WIDTH IS CAPPED BEFORE THE CLAMP, because a popover wider than the
/// screen cannot be clamped into it: the mod picker is 331px and a 360px phone
/// has 348 to give. Both are measured after `hidden` is cleared, since a hidden
/// element has no size — and from a known left, since a width read against the
/// right edge comes back squeezed.
///
/// EVERY popover goes through this, so all six are covered by the one change.
/// `check_mobile` asserts it with each of them OPEN, a page at rest being its
/// blind spot.
function place(pop, anchor) {
  const r = anchor.getBoundingClientRect();
  pop.hidden = false;
  const margin = 6;
  const view_width = document.documentElement.clientWidth;
  pop.style.maxWidth = (view_width - 2 * margin) + "px";
  pop.style.left = margin + "px";
  const width = pop.offsetWidth;
  const left = Math.max(margin, Math.min(r.left, view_width - margin - width));
  pop.style.top = (window.scrollY + r.bottom + 4) + "px";
  pop.style.left = (window.scrollX + left) + "px";
}

function openPicker(slotIdx, anchor) {
  closePopovers();
  pickerSlot = slotIdx;
  const pop = $("mod-popover");
  place(pop, anchor);
  const search = $("mod-search");
  search.value = "";
  search.oninput = () => renderMenu(slotIdx, search.value);
  renderTools();
  renderMenu(slotIdx, "");
  // Sorted by EFFECT by default — which means computing it.
  ensureGains({ kind: "mods", idx: slotIdx },
    () => { if (!$("mod-popover").hidden) renderMenu(pickerSlot, $("mod-search").value); },
    true);
  search.focus();
}

function renderTools() {
  const t = $("picker-tools");
  const pols = ["Madurai", "Naramon", "Vazarin", "Umbra"].filter((p) => currentPool.some((m) => m.polarity === p));
  t.innerHTML =
    `<label>${escHtml(tr("Sort"))} ` + ddButton("pk-sort", {
      value: pickerPrefs.sort,
      items: [{ value: "name", label: tr("Name") }, { value: "drain", label: tr("Drain") }]
        .concat(gainPrefs.on === false ? [] : [{ value: "gain", label: tr("Gain") }]),
      onPick: (v) => { pickerPrefs.sort = v; savePickerPrefs(); renderTools(); renderMenu(pickerSlot, $("mod-search").value); },
    }) + `</label>` +
    `<button id="pk-dir" class="ghost-btn small" title="direction">${pickerPrefs.dir === "asc" ? "▲" : "▼"}</button>` +
    `<span class="pk-pols"><span class="pk-pol ${!pickerPrefs.pol ? "sel" : ""}" data-p="">all</span>` +
    pols.map((p) => `<span class="pk-pol ${pickerPrefs.pol === p ? "sel" : ""}" data-p="${p}" title="${p}">${imgTag(POL(p), "pol")}</span>`).join("") +
    `</span>`;
  // redraw() re-renders these tools via innerHTML, which DETACHES the clicked
  // node; without stopPropagation the click would bubble to the document
  // outside-click handler, whose closest(".popover") now fails on the detached
  // target → the picker would wrongly close. Keep every tool click inside.
  const redraw = () => { savePickerPrefs(); renderTools(); renderMenu(pickerSlot, $("mod-search").value); };
  $("pk-dir").onclick = (e) => { e.stopPropagation(); pickerPrefs.dir = pickerPrefs.dir === "asc" ? "desc" : "asc"; redraw(); };
  t.querySelectorAll(".pk-pol").forEach((o) => o.onclick = (e) => { e.stopPropagation(); pickerPrefs.pol = o.dataset.p || null; redraw(); });
}

