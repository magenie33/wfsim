// "NONA REMEMBERED …" — every write to memory is seen and undone in one tap; a
// proposed one waits for the reader to take it or refuse it.

import { SLOT_LABELS } from "../core/memory.js";
import * as store from "../runtime/store.js";
import { tr, esc, $ } from "./kit.js";

export const memoryLabel = (it) => `${it.key ? tr(SLOT_LABELS[it.key]) + ": " : ""}${it.value}`;

export function drawChip(id) {
  const log = $("nona-log");
  if (!log || !store.memory.get().items.some((x) => x.id === id)) return;
  const el = document.createElement("div");
  el.className = "nona-msg memory";
  const paint = () => {
    const cur = store.memory.get().items.find((x) => x.id === id);
    if (!cur) { el.innerHTML = `<span>${esc(tr("forgotten"))}</span>`; return; }
    el.innerHTML = cur.status === "proposed"
      ? `<span>${esc(tr("Nona would like to remember"))}: <b>${esc(memoryLabel(cur))}</b></span>`
        + `<button class="ghost-btn small" data-mem="keep">${esc(tr("Remember"))}</button>`
        + `<button class="ghost-btn small" data-mem="drop">${esc(tr("Don't"))}</button>`
      : `<span>${esc(tr("Nona remembered"))}: <b>${esc(memoryLabel(cur))}</b></span>`
        + `<button class="ghost-btn small" data-mem="undo">${esc(tr("Undo"))}</button>`;
    const on = (key, f) => { const b = el.querySelector(`[data-mem="${key}"]`); if (b) b.onclick = () => { f(); paint(); }; };
    on("keep", () => store.memory.keep(id));
    on("drop", () => store.memory.forget(id));
    on("undo", () => store.memory.undo(id));
  };
  paint();
  log.appendChild(el);
  log.scrollTop = log.scrollHeight;
}
