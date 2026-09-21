// THE PAGE'S LOOK, borrowed through the door: its translation, its dropdown and
// its escaping (`wfsim.ui`), so the panel reads like the page without reaching
// into it. Everything the ui files share is here.

import { markNumbers } from "../core/measure.js";

const kit = () => window.wfsim.ui;
export const tr = (s) => kit().tr(s);
export const esc = (s) => kit().esc(s);
export const dd = (id, cfg) => kit().dd(id, cfg);
export const $ = (id) => document.getElementById(id);

export const k = (n) => (n >= 1000 ? `${Math.round(n / 100) / 10}k` : String(n));
export const usd = (x) => (x < 0.01 ? `$${x.toFixed(4)}` : `$${x.toFixed(2)}`);

/// A REPLY'S MARKUP: escaped first, then the few marks a chat reply uses — a
/// model's text is not trusted as HTML. `measured` (the numbers the tools
/// returned) turns on the unmeasured-number mark.
export const markup = (s, measured) => markNumbers(esc(s), measured,
  (all) => `<span class="nona-unmeasured" title="${esc(tr("not measured in this conversation"))}">${all}</span>`)
  .replace(/`([^`]+)`/g, "<code>$1</code>")
  .replace(/\*\*([^*]+)\*\*/g, "<b>$1</b>")
  .split(/\n{2,}/).map((p) => (/^\s*[-*] /m.test(p)
    ? "<ul>" + p.split("\n").filter((l) => l.trim()).map((l) => `<li>${l.replace(/^\s*[-*] /, "")}</li>`).join("") + "</ul>"
    : `<p>${p.replace(/\n/g, "<br>")}</p>`)).join("");

export function usageLine(u) {
  return [`${k(u.input)} → ${k(u.output)} tokens`,
    u.input && u.cached ? `${tr("cached")} ${Math.round((u.cached / u.input) * 100)}%` : "",
    u.cost != null ? `≈${usd(u.cost)}` : ""].filter(Boolean).join(" · ");
}

/// One line in the log. The log only ever grows at its end.
export function line(kind, text) {
  const log = $("nona-log");
  const el = document.createElement("div");
  el.className = "nona-msg " + kind;
  el.textContent = text;
  log.appendChild(el);
  log.scrollTop = log.scrollHeight;
  return el;
}
