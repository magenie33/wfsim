// ---- A SECTION WIDER THAN THE SCREEN SCROLLS, AND SAYS SO --------------------
//
// docs/UI.md §"A section wider than the screen". `.block` clips its corners, so
// a body wider than a phone would vanish past the right edge. A body scrolls
// sideways instead (`.block > .bb`), and a phone's overlay scrollbar is
// invisible until touched — so each overflowing body gets this bar, pinned to
// the bottom of the screen while its section is on it, with a thumb a finger
// can drag. A layout that fits never shows it; the bar is the backstop.

/// The bar of one section body, made on first overflow and kept.
function hscrollBar(bb) {
  if (bb.__hbar) return bb.__hbar;
  const bar = document.createElement("div");
  bar.className = "hbar";
  bar.hidden = true;
  bar.innerHTML = `<button type="button" class="hbar-step" data-d="-1" aria-label="${escHtml(tr("scroll left"))}">‹</button>`
    + `<div class="hbar-track"><div class="hbar-thumb"></div></div>`
    + `<button type="button" class="hbar-step" data-d="1" aria-label="${escHtml(tr("scroll right"))}">›</button>`;
  bb.after(bar);
  const track = bar.querySelector(".hbar-track");
  const thumb = bar.querySelector(".hbar-thumb");
  // Track pixels to body pixels: the thumb is the visible fraction of the body.
  const ratio = () => bb.scrollWidth / Math.max(1, track.clientWidth);
  bar.querySelectorAll(".hbar-step").forEach((b) => b.addEventListener("click", () =>
    bb.scrollBy({ left: Number(b.dataset.d) * bb.clientWidth * 0.8, behavior: "smooth" })));
  thumb.addEventListener("pointerdown", (e) => {
    e.preventDefault();
    e.stopPropagation();
    thumb.setPointerCapture(e.pointerId);
    const x0 = e.clientX, left0 = bb.scrollLeft;
    const move = (m) => { bb.scrollLeft = left0 + (m.clientX - x0) * ratio(); };
    const up = () => {
      thumb.removeEventListener("pointermove", move);
      thumb.removeEventListener("pointerup", up);
      thumb.removeEventListener("pointercancel", up);
      bar.classList.remove("drag");
    };
    bar.classList.add("drag");
    thumb.addEventListener("pointermove", move);
    thumb.addEventListener("pointerup", up);
    thumb.addEventListener("pointercancel", up);
  });
  // A tap on the track puts the thumb's middle where the finger is.
  track.addEventListener("pointerdown", (e) => {
    const r = track.getBoundingClientRect();
    bb.scrollTo({ left: (e.clientX - r.left) * ratio() - bb.clientWidth / 2, behavior: "smooth" });
  });
  bb.addEventListener("scroll", () => hscrollPaint(bb), { passive: true });
  bb.__hbar = bar;
  return bar;
}

function hscrollPaint(bb) {
  const bar = bb.__hbar;
  if (!bar || bar.hidden) return;
  const track = bar.querySelector(".hbar-track");
  const tw = track.clientWidth;
  const w = Math.max(28, tw * bb.clientWidth / bb.scrollWidth);
  const room = bb.scrollWidth - bb.clientWidth;
  const thumb = bar.querySelector(".hbar-thumb");
  thumb.style.width = `${w}px`;
  thumb.style.transform = `translateX(${room > 0 ? (tw - w) * bb.scrollLeft / room : 0}px)`;
  bar.querySelector('[data-d="-1"]').disabled = bb.scrollLeft <= 0;
  bar.querySelector('[data-d="1"]').disabled = bb.scrollLeft >= room - 1;
}

function hscrollAll() {
  for (const bb of document.querySelectorAll(".block > .bb")) {
    const over = bb.scrollWidth > bb.clientWidth + 1 && bb.getClientRects().length > 0;
    if (!over && !bb.__hbar) continue;
    const bar = hscrollBar(bb);
    bar.hidden = !over;
    hscrollPaint(bb);
  }
}

// ONE PASS A FRAME, however many nodes a redraw touched or however the window
// moved — the same debounce the start pins use.
let hscrollFrame = 0;
const hscrollSoon = () => {
  if (!hscrollFrame) hscrollFrame = requestAnimationFrame(() => { hscrollFrame = 0; hscrollAll(); });
};
new MutationObserver(hscrollSoon).observe(document.documentElement, { childList: true, subtree: true });
addEventListener("resize", hscrollSoon);

// One measured offset keeps page tabs and local sticky controls below the shell.
let stickyHeaderFrame = 0;
function refreshStickyHeader() {
  if (stickyHeaderFrame) return;
  stickyHeaderFrame = requestAnimationFrame(() => {
    stickyHeaderFrame = 0;
    const root = document.documentElement;
    const header = document.querySelector('.topbar');
    const height = header?.offsetHeight || 0;
    const tabs = [...document.querySelectorAll('.module-tabs')].find(el => el.offsetParent !== null && el.children.length);
    const tabHeight = tabs?.offsetHeight || 0;
    root.style.setProperty('--shell-header-height', height + 'px');
    root.style.setProperty('--page-tabs-height', tabHeight + 'px');
    root.classList.toggle('page-tabs-stuck', !!tabs && tabs.getBoundingClientRect().top <= height + 1);
    const selected = tabs?.querySelector('.sel');
    if (selected && selected !== tabs.stickySelected) {
      const r = selected.getBoundingClientRect(), host = tabs.getBoundingClientRect();
      if (r.left < host.left) tabs.scrollLeft += r.left - host.left - 8;
      else if (r.right > host.right) tabs.scrollLeft += r.right - host.right + 8;
      tabs.stickySelected = selected;
    }
  });
}
addEventListener('scroll', refreshStickyHeader, {passive:true});
addEventListener('resize', refreshStickyHeader);
new MutationObserver(refreshStickyHeader).observe(document.body, {subtree:true,childList:true,attributes:true,attributeFilter:['hidden','class']});
const stickyShell = document.querySelector('.topbar');
if ('ResizeObserver' in window && stickyShell) new ResizeObserver(refreshStickyHeader).observe(stickyShell);
refreshStickyHeader();

// Layout preferences move the existing cards and keep their authored default slots.
(() => {
  const key = 'wfsim-layout';
  let previous = {};
  try { previous = JSON.parse(localStorage.getItem(key)) || {}; } catch {}
  const saved = previous.schema === 2 && previous.cards && previous.orders ? previous : {schema:2,cards:{},orders:{}};
  const groups = new Map();
  let editing = false, pending = false, dragging = null;
  const changed = () => Object.keys(saved.cards).length || Object.keys(saved.orders).length;
  const save = () => { localStorage.setItem(key, JSON.stringify(saved)); schedule(); };
  const text = (node, value) => { if (node.textContent !== value) node.textContent = value; };
  function apply(group) {
    const ids = group.rows.map(({card}) => card.id), requested = saved.orders[group.key] || ids;
    const order = [...requested.filter(id => ids.includes(id)), ...ids.filter(id => !requested.includes(id))];
    group.rows.forEach(({slot}, index) => {
      const card = group.rows.find(row => row.card.id === order[index]).card;
      if (slot.nextSibling !== card) slot.after(card);
    });
  }
  function ordered(group) {
    return [...group.parent.children].filter(card => group.rows.some(row => row.card === card));
  }
  function move(card, before) {
    const group = groups.get(card.dataset.layoutGroup);
    const order = ordered(group).filter(other => other !== card);
    const index = before ? order.indexOf(before) : order.length;
    order.splice(index < 0 ? order.length : index, 0, card);
    const ids = order.map(node => node.id);
    if (ids.every((id, i) => id === group.rows[i].card.id)) delete saved.orders[group.key];
    else saved.orders[group.key] = ids;
    apply(group); save();
  }
  function step(card, direction) {
    const group = groups.get(card.dataset.layoutGroup), cards = ordered(group).filter(node => node.offsetParent !== null);
    const index = cards.indexOf(card), target = index + direction;
    if (target < 0 || target >= cards.length) return;
    move(card, direction < 0 ? cards[target] : cards[target + 1]);
    card.scrollIntoView({block:'nearest'});
  }
  function clearDrop() {
    document.querySelectorAll('.layout-drop-before,.layout-drop-after').forEach(node => node.classList.remove('layout-drop-before','layout-drop-after'));
  }
  function dropTarget(card, y) {
    const group = groups.get(card.dataset.layoutGroup), cards = ordered(group).filter(node => node !== card && node.offsetParent !== null);
    const before = cards.find(node => { const r = node.getBoundingClientRect(); return y < r.top + r.height / 2; });
    clearDrop();
    if (before) before.classList.add('layout-drop-before');
    else cards.at(-1)?.classList.add('layout-drop-after');
    return before;
  }
  function drag(handle, card) {
    handle.onpointerdown = event => {
      if (event.button !== 0) return;
      event.preventDefault(); handle.setPointerCapture(event.pointerId);
      dragging = {card, start:event.clientY, y:event.clientY, active:false, before:null};
      let frame;
      const tick = () => {
        if (!dragging) return;
        if (dragging.active) {
          const edge = 100, y = dragging.y;
          const speed = y < edge ? -Math.ceil((edge-y)/7) : y > innerHeight-edge ? Math.ceil((y-innerHeight+edge)/7) : 0;
          if (speed) window.scrollBy(0, Math.max(-20, Math.min(20, speed)));
          dragging.before = dropTarget(card, y);
        }
        frame = requestAnimationFrame(tick);
      };
      frame = requestAnimationFrame(tick);
      handle.onpointermove = next => {
        if (!dragging) return;
        dragging.y = next.clientY;
        if (Math.abs(next.clientY - dragging.start) > 5) { dragging.active = true; card.classList.add('layout-dragging'); }
      };
      const finish = cancelled => {
        if (!dragging) return;
        const current = dragging; dragging = null; cancelAnimationFrame(frame);
        handle.onpointermove = null; clearDrop(); card.classList.remove('layout-dragging');
        if (current.active && !cancelled) { move(card, current.before); card.scrollIntoView({block:'nearest'}); }
      };
      handle.onpointerup = () => finish(false); handle.onpointercancel = () => finish(true);
    };
    handle.onkeydown = event => {
      if (!['ArrowUp','ArrowDown'].includes(event.key)) return;
      event.preventDefault(); step(card, event.key === 'ArrowUp' ? -1 : 1);
    };
  }
  function wireCard(card, page) {
    card.classList.add('layout-card');
    const dimensions = saved.cards[card.id];
    if (dimensions) {
      card.style.width = dimensions.width + 'px'; card.style.height = dimensions.height + 'px'; card.classList.add('layout-sized');
    }
    const header = card.querySelector(':scope > .bh, :scope > .fold-h, :scope > .calculation-heading, :scope > .library-panel-heading, :scope > .library-toolbar .library-toolbar-row, :scope > h2');
    if (header && !header.querySelector('.layout-move-tools')) {
      const tools = document.createElement('span'); tools.className = 'layout-move-tools';
      tools.innerHTML = `<button type="button" data-layout-drag title="${escHtml(tr('Drag to reorder; arrow keys move the card'))}" aria-label="${escHtml(tr('Reorder card'))}">⠿</button><button type="button" data-layout-up title="${escHtml(tr('Move up'))}" aria-label="${escHtml(tr('Move up'))}">↑</button><button type="button" data-layout-down title="${escHtml(tr('Move down'))}" aria-label="${escHtml(tr('Move down'))}">↓</button>`;
      header.append(tools); drag(tools.querySelector('[data-layout-drag]'), card);
      tools.querySelector('[data-layout-up]').onclick = () => step(card,-1);
      tools.querySelector('[data-layout-down]').onclick = () => step(card,1);
    }
    if (card.querySelector(':scope > .layout-size-handle')) return;
    const handle = document.createElement('button'); handle.type = 'button'; handle.className = 'layout-size-handle';
    handle.textContent = '◢'; handle.title = tr('Drag or use arrow keys to resize; double-click restores size'); handle.setAttribute('aria-label',tr('Resize card')); card.append(handle);
    handle.ondblclick = () => { delete saved.cards[card.id]; card.style.width = ''; card.style.height = ''; card.classList.remove('layout-sized'); save(); };
    const size = (width,height) => {
      width = Math.min(page.clientWidth - 36, Math.max(Math.min(280,page.clientWidth - 36),width)); height = Math.max(100,height);
      card.style.width = width + 'px'; card.style.height = height + 'px'; card.classList.add('layout-sized'); saved.cards[card.id] = {width,height};
    };
    handle.onpointerdown = event => {
      if (event.button !== 0) return;
      event.preventDefault(); handle.setPointerCapture(event.pointerId);
      const box = card.getBoundingClientRect(), x = event.clientX, y = event.clientY;
      handle.onpointermove = next => size(box.width + (next.clientX-x)*2,box.height+next.clientY-y);
      const finish = () => { handle.onpointermove = null; save(); };
      handle.onpointerup = finish; handle.onpointercancel = finish;
    };
    handle.onkeydown = event => {
      if (!['ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(event.key)) return;
      event.preventDefault(); const box = card.getBoundingClientRect(), amount = event.shiftKey ? 50 : 10;
      size(box.width + (event.key === 'ArrowRight' ? amount : event.key === 'ArrowLeft' ? -amount : 0),box.height + (event.key === 'ArrowDown' ? amount : event.key === 'ArrowUp' ? -amount : 0)); save();
    };
  }
  function refresh() {
    if (EMBED || dragging) return;
    for (const page of document.querySelectorAll('.config-page')) {
      if (page.hidden) continue;
      let tools = page.querySelector(':scope > .layout-tools');
      if (!tools) {
        tools = document.createElement('div'); tools.className = 'layout-tools';
        tools.innerHTML = `<span>${escHtml(tr('Layout'))}</span><span class="layout-status"></span><button type="button" class="ghost-btn small" data-layout-edit>${escHtml(tr('Edit layout'))}</button><button type="button" class="ghost-btn small" data-layout-reset>${escHtml(tr('Restore default layout'))}</button>`;
        const tabs = page.querySelector('.module-tabs'); if (tabs) tabs.after(tools); else page.prepend(tools);
        tools.querySelector('[data-layout-edit]').onclick = () => { editing = !editing; refresh(); };
        tools.querySelector('[data-layout-reset]').onclick = () => {
          saved.cards = {}; saved.orders = {};
          document.querySelectorAll('.layout-card').forEach(card => { card.style.width = ''; card.style.height = ''; card.classList.remove('layout-sized'); });
          for (const group of groups.values()) if (group.rows.every(({slot}) => slot.isConnected)) apply(group);
          save(); refresh();
        };
      }
      text(tools.querySelector('.layout-status'),changed() ? tr('Custom layout') + ' · ' + tr('Saved') : tr('Default layout'));
      const edit = tools.querySelector('[data-layout-edit]'); text(edit,editing ? tr('Done') : tr('Edit layout')); edit.setAttribute('aria-pressed',editing);
      page.classList.toggle('layout-active',editing || !!changed()); page.classList.toggle('layout-editing',editing);
      const candidates = [...page.querySelectorAll(':scope > .block, :scope > .library-panel, :scope > .calculation-conditions, #build-finder > .library-panel, #calculation-scenario-editor > .scenario-section')].filter(card => card.id);
      const byGroup = new Map();
      for (const card of candidates) {
        const name = (card.parentElement.id || page.id || 'weapon') + ':' + (card.dataset.module || (card.classList.contains('calculation-conditions') ? 'builder' : 'page'));
        if (!byGroup.has(name)) byGroup.set(name,[]); byGroup.get(name).push(card); card.dataset.layoutGroup = name;
      }
      for (const [name,cards] of byGroup) {
        let group = groups.get(name);
        if (!group || group.rows.length !== cards.length || group.rows.some(({card,slot}) => !cards.includes(card) || !slot.isConnected)) {
          group?.rows.forEach(({slot}) => slot.remove());
          group = {key:name,parent:cards[0].parentElement,rows:cards.map(card => { const slot = document.createComment('layout slot'); card.before(slot); return {card,slot}; })};
          groups.set(name,group);
        }
        apply(group);
        for (const card of cards) wireCard(card,page);
        const shown = ordered(group).filter(node => node.offsetParent !== null);
        shown.forEach((card,index) => {
          const up = card.querySelector('[data-layout-up]'), down = card.querySelector('[data-layout-down]');
          if (up) up.disabled = index === 0; if (down) down.disabled = index === shown.length-1;
        });
      }
    }
  }
  function schedule() {
    if (pending) return; pending = true;
    requestAnimationFrame(() => { pending = false; refresh(); });
  }
  new MutationObserver(schedule).observe(document.body,{subtree:true,childList:true});
  window.addEventListener('wfsim:ready',schedule); schedule();
})();
