// THE CHANGE CARD: after she has worked on a copy of the reader's build, what
// differs, and three ways on — look at it, take it, or go back. Taking it is the
// reader's hand on the door's `shell.preset.adopt`, an action no model can call.

import { tr, esc, $ } from "./kit.js";

/// Mods and arcanes in and out, and whether the mode moved — between two
/// `shell.preset.read` answers.
function diff(a, b) {
  const ids = (st) => [...(st.mods || []), ...(st.arcanes || [])];
  const ia = ids(a).map((x) => x.id), ib = ids(b).map((x) => x.id);
  return {
    added: ids(b).filter((x) => !ia.includes(x.id)).map((x) => x.name),
    removed: ids(a).filter((x) => !ib.includes(x.id)).map((x) => x.name),
    mode: a.mode !== b.mode ? `${a.mode} → ${b.mode}` : null,
  };
}

/// The card for `pair` ({copy, from, state}), placed now so the log keeps its
/// order and filled once the reader's build has been read.
export function drawCard(door, pair) {
  const log = $("nona-log");
  const el = document.createElement("div");
  el.className = "nona-msg card";
  el.hidden = true;
  log.appendChild(el);
  door.do("shell.preset.read", { bar: "build", preset: pair.from }).then((mine) => {
    if (!mine || !mine.ok || !pair.state) { el.remove(); return; }
    const d = diff(mine, pair.state);
    if (!d.added.length && !d.removed.length && !d.mode) { el.remove(); return; }
    const row = (sign, xs) => (xs.length ? `<div class="nona-diff ${sign === "+" ? "add" : "del"}">${sign} ${xs.map(esc).join(" · ")}</div>` : "");
    el.innerHTML = `<div class="nona-card-h">${esc(tr("Changes on the copy"))} <b>${esc(pair.copy)}</b></div>`
      + row("+", d.added) + row("−", d.removed) + (d.mode ? `<div class="nona-diff">${esc(d.mode)}</div>` : "")
      + `<div class="nona-card-b"><button class="ghost-btn small" data-card="open">${esc(tr("Open the copy"))}</button>`
      + `<button class="ghost-btn small" data-card="apply">${esc(tr("Apply to my build"))} <b>${esc(pair.from)}</b></button>`
      + `<button class="ghost-btn small" data-card="back">${esc(tr("Back to my build"))}</button></div>`;
    el.hidden = false;
    el.querySelector('[data-card="open"]').onclick = () => door.do("shell.preset.open", { bar: "build", preset: pair.copy });
    el.querySelector('[data-card="back"]').onclick = () => door.do("shell.preset.open", { bar: "build", preset: pair.from });
    el.querySelector('[data-card="apply"]').onclick = async (e) => {
      const b = e.currentTarget;
      const r = await door.do("shell.preset.adopt", { bar: "build", from: pair.copy, into: pair.from }, { hand: true });
      if (r && r.ok) { b.textContent = `✓ ${tr("applied")}`; b.disabled = true; }
    };
  });
}
