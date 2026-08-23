// A BLOCK BELONGS TO ONE MODULE, AND IT APPEARS ON THAT MODULE'S TAB ONLY.
//
// The page is three modules — Builder | Simulator | Optimizer — plus the
// editors that feed them, and the chain runs one way: builder → simulator →
// optimizer, each reading upstream and writing nothing. A BUILD is the
// builder's, so a control that edits one has no business on the two tabs that
// do not own a build.
//
// THIS HAS GONE WRONG TWICE, THE SAME WAY BOTH TIMES. The Mode control was the
// builder's, drawn on the optimizer tab "only because nothing hid it"
// (check_opt_modes, 2026-08-11) — the page offered a choice it never sent. Then
// the Parts block, added 2026-08-23, drew on the Simulator and the Optimizer
// and was numbered nowhere (owner, 2026-08-24).
//
// Both times the mechanism was a LIST. There were four of them naming the
// builder's blocks by id — two CSS rules, the step numbering, and the
// official-build lock — and a new block was added to none. A list cannot
// report what is not on it, so the fix was to make every block DECLARE its
// module (`data-module`) and derive all four from that.
//
// So this check holds the declaration rather than any block:
//
//   * every block in the config page declares a module — one added tomorrow
//     with none fails here, which is the whole point;
//   * on each tab, every VISIBLE block belongs to that tab's module;
//   * ...and the builder's blocks are visible on the builder, which is the
//     negative control: a page that hid everything would pass the first two.
//
// It runs on a KITGUN, because the Parts block is the one that was missing and
// it is hidden outright on the other 134 weapons — checking this on an ordinary
// rifle would pass without ever looking at it.
//
//   node scripts/check_module_blocks.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, finish } = app;

// THE FOUR TABS A WEAPON PAGE HAS, and the module each one owns. `enemies` is
// reachable only from the simulator's target card, so it is not a tab a URL
// lands on here; its blocks are covered by the "declares a module" sweep.
const TABS = [["builder", ""], ["simulator", "/simulator"],
              ["optimizer", "/optimizer"], ["rivens", "/rivens"]];

const r = await evaluate(`(async () => {
  const out = { tabs: {}, undeclared: [], modules: [] };
  for (const [name, suffix] of ${JSON.stringify(TABS)}) {
    history.pushState({}, '', '/weapons/Tombfinger' + suffix); route();
    await new Promise(r => setTimeout(r, 3500));
    // OFFSETPARENT, not the class or the style: what is being asserted is what
    // a reader SEES, and a rule that stopped applying would leave every class
    // exactly as it is.
    out.tabs[name] = [...document.querySelectorAll('[data-module]')]
      .filter(b => b.offsetParent !== null)
      .map(b => ({ id: b.id, module: b.dataset.module }));
  }
  // EVERY block in the config page, declared or not — the sweep that catches
  // one added tomorrow. Read on the last tab, since a block's ATTRIBUTES are
  // there whether or not it is on screen.
  out.undeclared = [...document.querySelectorAll('.config-page .block')]
    .filter(b => !b.dataset.module)
    .map(b => b.id || ('(no id) ' + b.textContent.trim().slice(0, 24)));
  out.modules = [...new Set([...document.querySelectorAll('[data-module]')]
    .map(b => b.dataset.module))].sort();
  return out;
})()`);

// ---- every block says whose it is -------------------------------------------

check("every block in the config page declares its module",
  r.undeclared.length === 0,
  `undeclared: ${JSON.stringify(r.undeclared)}`);
// The modules that exist. Asserted so that a typo — `data-module="build"` —
// fails here rather than silently making a block belong to nobody and
// therefore show everywhere.
const KNOWN = ["builder", "enemies", "optimizer", "rivens", "simulator"];
check("...and every module named is one the page has",
  r.modules.every((m) => KNOWN.includes(m)),
  JSON.stringify(r.modules));

// ---- and appears only where it belongs --------------------------------------

for (const [tab] of TABS) {
  const shown = r.tabs[tab] || [];
  const strays = shown.filter((b) => b.module !== tab);
  check(`the ${tab} tab shows only ${tab} blocks`,
    strays.length === 0,
    `strays: ${JSON.stringify(strays)}`);
}

// THE NEGATIVE CONTROL, and it is not a formality: every assertion above is
// satisfied by a page that renders nothing at all. The builder has to be
// showing its own blocks, and the Parts block among them — that is the one the
// lists forgot, and the reason this check runs on a Kitgun.
const builder = r.tabs.builder || [];
check("...and the builder shows its own",
  builder.length >= 3 && builder.every((b) => b.module === "builder"),
  JSON.stringify(builder));
check("...including the Parts block, which is the one the lists forgot",
  builder.some((b) => b.id === "assembly-block"),
  JSON.stringify(builder.map((b) => b.id)));
// …AND THE OTHER TABS ARE NOT EMPTY EITHER, so "shows only its own" cannot be
// passing because a tab renders nothing.
for (const [tab] of TABS) {
  check(`...and the ${tab} tab is not simply blank`,
    (r.tabs[tab] || []).length > 0,
    JSON.stringify(r.tabs[tab]));
}

await finish();
console.log("\na block belongs to one module, and appears on that module's tab only");
