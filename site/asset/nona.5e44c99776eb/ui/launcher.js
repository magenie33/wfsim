// THE LAUNCHER MOVES. A reader drags it off whatever it covers; let go, it
// settles on the nearer side edge at the height it was left, and stays there
// on this browser. A press that barely moves is a click, so opening Nona is
// unchanged. Never moved, it sits where the stylesheet puts it.

import * as store from "../runtime/store.js";

/// How far a press may wander and still be a click, in CSS pixels.
const DRAG_PX = 6;
/// The gap kept to the screen's edges, the stylesheet's own 18px.
const EDGE_PX = 18;
/// Kept below the sticky topbar, so it never sits on the search.
const TOP_PX = 72;

export function makeDraggable(fab) {
  let press = null;
  let dragged = false;

  /// Place it from `{ side, y }`: `y` is how far up the screen its bottom sits,
  /// as a share of the height, so a resized window keeps it where it was.
  /// Pinned by the edge it sits on, so its width never has to be known — it
  /// is hidden, and measures nothing, while the panel is open.
  const place = (p) => {
    if (!p) return;
    const h = fab.offsetHeight || 44;
    const maxBottom = Math.max(EDGE_PX, innerHeight - TOP_PX - h);
    fab.style.left = p.side === "left" ? `${EDGE_PX}px` : "auto";
    fab.style.right = p.side === "left" ? "auto" : `${EDGE_PX}px`;
    fab.style.bottom = `${Math.min(maxBottom, Math.max(EDGE_PX, p.y * innerHeight))}px`;
  };

  // THE PRESS IS FOLLOWED ON THE WINDOW, not on the button: a quick flick
  // leaves the button before its first move arrives, and the button then
  // hears nothing more.
  const move = (e) => {
    if (!press || e.pointerId !== press.id) return;
    if (!dragged && Math.hypot(e.clientX - press.x, e.clientY - press.y) < DRAG_PX) return;
    if (!dragged) { dragged = true; fab.classList.add("dragging"); }
    const w = fab.offsetWidth, h = fab.offsetHeight;
    const left = Math.min(innerWidth - w, Math.max(0, e.clientX - press.dx));
    const top = Math.min(innerHeight - h, Math.max(0, e.clientY - press.dy));
    fab.style.right = "auto";
    fab.style.left = `${left}px`;
    fab.style.bottom = `${innerHeight - top - h}px`;
  };
  const release = (e) => {
    if (!press || e.pointerId !== press.id) return;
    press = null;
    removeEventListener("pointermove", move);
    removeEventListener("pointerup", release);
    removeEventListener("pointercancel", release);
    if (!dragged) return;
    fab.classList.remove("dragging");
    const r = fab.getBoundingClientRect();
    const p = { side: r.left + r.width / 2 < innerWidth / 2 ? "left" : "right", y: (innerHeight - r.bottom) / innerHeight };
    place(p);
    store.launcher.put(p);
    // A release off the button fires no click, and the mark would then eat
    // the next real one: it lasts only until the click that ends this drag.
    setTimeout(() => { dragged = false; }, 0);
  };
  fab.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    const r = fab.getBoundingClientRect();
    press = { id: e.pointerId, x: e.clientX, y: e.clientY, dx: e.clientX - r.left, dy: e.clientY - r.top };
    dragged = false;
    addEventListener("pointermove", move);
    addEventListener("pointerup", release);
    addEventListener("pointercancel", release);
  });
  // A DRAG IS NOT A CLICK: the click that ends one is swallowed before Nona opens.
  fab.addEventListener("click", (e) => {
    if (!dragged) return;
    e.stopImmediatePropagation();
    e.preventDefault();
  }, true);

  place(store.launcher.get());
  addEventListener("resize", () => place(store.launcher.get()));
}
