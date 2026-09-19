// EVERY CONTROL A READER CAN USE IS ON THE AGENT DOOR, OR SAYS WHY NOT.
//
// Nona can use exactly what the door's table holds, so a control added to the
// page without a row there is a feature she silently cannot reach — nothing
// fails, she just never uses it. This makes that loud. A control is found the
// way the page itself makes one: an element that was handed a click, change or
// input listener (recorded by wrapping `addEventListener` before the boot), an
// `on*` property, or a `[data-dd]` dropdown, whose listener is delegated. It is COVERED when it sits inside the anchor of a door action,
// and EXEMPT when it sits inside an entry of `AGENT_EXEMPT` in `app.js`, which
// states the kind and the reason:
//
//   * outward — leaves the browser; a reader's gesture for ever (docs/AGENT.md)
//   * view    — changes how the page looks, not what is built or fought
//   * pref    — a preference of this browser, not part of any build
//   * reader  — the reader's own housekeeping (rename, delete); never an agent's
//   * todo    — a real feature not on the door yet; the count only goes down
//
// THE INNERMOST MATCH DECIDES, so a door region cannot swallow a control
// nested inside it that the door does not reach, nor an exemption a control
// the door does. An anchor may be any selector list, attributes included.
//
// Anything else fails, naming the control. An exemption that matches nothing on
// any page scanned is stale and fails too.
import { openApp } from "./cdp.mjs";

// THE TODO RATCHET: lower it when a feature reaches the door, never raise it to
// make a red run green.
const TODO_MAX = 7;

const app = await openApp({ boot: 13000, base: process.env.WFSIM_BASE });
const { evaluate, check, finish, send } = app;

await send("Page.addScriptToEvaluateOnNewDocument", {
  source: `(() => {
    const seen = new WeakSet();
    window.__wfsimListened = seen;
    const kinds = new Set(["click", "change", "input", "pointerdown", "mousedown", "keydown"]);
    const add = EventTarget.prototype.addEventListener;
    EventTarget.prototype.addEventListener = function (type, fn, opts) {
      if (kinds.has(type) && this instanceof Element) seen.add(this);
      return add.call(this, type, fn, opts);
    };
  })();`,
});

const SCAN = `(() => {
  const seen = window.__wfsimListened;
  const anchors = window.wfsim.actions.map(a => a.anchor);
  // How many steps up from el to the nearest element matching sel; -1 if none.
  const depth = (el, sel) => { let d = 0; for (let n = el; n && n !== document.body; n = n.parentElement, d++) if (n.matches(sel)) return d; return -1; };
  const visible = (el) => el.getClientRects().length > 0 && getComputedStyle(el).visibility !== "hidden";
  const name = (el) => {
    const parts = [];
    for (let n = el; n && n !== document.body && parts.length < 4; n = n.parentElement) {
      parts.unshift(n.id ? "#" + n.id : n.tagName.toLowerCase() + (n.classList.length ? "." + [...n.classList].slice(0, 2).join(".") : ""));
      if (n.id) break;
    }
    return parts.join(" > ");
  };
  const out = { uncovered: [], hit: [] };
  for (const el of document.querySelectorAll("body *")) {
    // A DROPDOWN IS DELEGATED: one document listener serves every [data-dd], so
    // the attribute is the only mark it leaves on the control itself.
    if (!(seen.has(el) || el.onclick || el.onchange || el.oninput || el.matches("[data-dd]"))) continue;
    if (!visible(el)) continue;
    const door = Math.min(...anchors.map(a => depth(el, a)).filter(d => d >= 0), Infinity);
    const ex = AGENT_EXEMPT.map(e => ({ e, d: depth(el, e.sel) })).filter(x => x.d >= 0).sort((a, b) => a.d - b.d)[0];
    if (ex && ex.d < door) { out.hit.push(ex.e.sel); continue; }
    if (door < Infinity) continue;
    out.uncovered.push(name(el));
  }
  out.hit = [...new Set(out.hit)];
  out.uncovered = [...new Set(out.uncovered)];
  return out;
})()`;

const WEAPONS = ["Torid", "Kuva_Bramma", "Catchmoon"];
const MODULES = ["builder", "simulator", "optimizer", "rivens", "enemies"];
const uncovered = new Map();
const hit = new Set();
for (const w of WEAPONS) {
  await app.load(`/weapons/${w}`);
  for (const m of MODULES) {
    await evaluate(`window.wfsim.do("shell.module.open", { module: ${JSON.stringify(m)} })`, { awaitPromise: true });
    // AN EDITOR'S CONTROLS EXIST ONLY WITH SOMETHING OPEN IN IT, so each gets
    // a document before the scan: a list with nothing open hides the editor.
    if (m === "rivens") await evaluate(`window.wfsim.do("rivens.card.new", {})`, { awaitPromise: true });
    if (m === "enemies") await evaluate(`(() => { const b = document.querySelector("#enemy-tools .cu-new"); if (b) b.click(); })()`);
    // EVERY FOLD OPEN: a control inside a shut block is still one a reader can
    // reach, and a block that ships shut would otherwise hide its whole feature.
    await evaluate(`document.querySelectorAll("[data-fold].shut").forEach(b => setFold(b, false))`);
    await new Promise((r) => setTimeout(r, 500));
    const r = await evaluate(SCAN);
    r.hit.forEach((s) => hit.add(s));
    r.uncovered.forEach((u) => { if (!uncovered.has(u)) uncovered.set(u, `${w}/${m}`); });
  }
}

const KINDS = ["outward", "view", "pref", "reader", "todo"];
const exempt = await evaluate(`AGENT_EXEMPT.map(e => ({ sel: e.sel, kind: e.kind, why: e.why }))`);
const list = [...uncovered].map(([u, where]) => `${u}   (${where})`);
if (process.env.DUMP) console.log(list.join("\n"));
check("every control is on the door or exempt with a reason", list.length === 0,
  list.length + " uncovered:\n    " + list.slice(0, 40).join("\n    "));
check("every exemption states a known kind and a reason",
  exempt.every((e) => KINDS.includes(e.kind) && e.why && e.why.length > 8),
  JSON.stringify(exempt.filter((e) => !KINDS.includes(e.kind) || !e.why)));
const stale = exempt.filter((e) => !hit.has(e.sel)).map((e) => e.sel);
check("no exemption is stale", stale.length === 0, stale.join(" · "));
const todo = exempt.filter((e) => e.kind === "todo").length;
check(`the not-yet-on-the-door list is not growing (${todo} ≤ ${TODO_MAX})`, todo <= TODO_MAX);

await finish("everything a reader can do, an agent can do or is told why not");
