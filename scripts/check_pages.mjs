// EVERY WEAPON, EVERY TAB, BOTH LANGUAGES — does the page come up at all?
//
// The other checks each go deep on one thing. This one goes wide and shallow,
// and it is the only one that would catch a weapon whose page throws on the way
// in: a data field the renderer does not expect, an evolution with no tier, a
// dropdown handed something it cannot draw. Those break one weapon and leave
// the other sixteen fine, so nothing aimed at Torid or the Laetum sees them.
//
// It asserts four things per page, and the last two are the ones that matter:
//
//   · nothing threw while rendering (page-side exceptions and console errors)
//   · nothing failed to load (art, wasm, json)
//   · the URL actually RESOLVED to the weapon it names
//   · ...and to the RIGHT weapon
//
// The last two exist because the first version of this check asked only
// "did something draw", and the home grid answers yes. Its Chinese pass had
// been building URLs from the LOCALIZED name, so every one of them fell through
// to the home page and the check called it a clean sweep. A page that renders
// is not a page that rendered what you asked for.
//
//   node scripts/check_pages.mjs
//
// Exits non-zero if any page throws, fails a load, or is not the one asked for.
import { openApp } from "./cdp.mjs";

// boot: 0 — the language passes below each load the app themselves.
const app = await openApp({ boot: 0 });
const { evaluate, check, sleep, send } = app;

const problems = [];
let current = "boot";
await send("Network.enable");
app.on("Runtime.exceptionThrown", (p) => {
  const d = p.exceptionDetails;
  problems.push(`${current} EXCEPTION ${(d.exception && (d.exception.description || d.exception.value)) || d.text}`);
});
app.on("Runtime.consoleAPICalled", (p) => {
  if (p.type !== "error") return;
  problems.push(`${current} CONSOLE ${p.args.map((a) => a.value || a.description || "").join(" ")}`);
});
app.on("Network.loadingFailed", (p) => {
  problems.push(`${current} FAILED TO LOAD ${p.errorText}`);
});

await app.load("/", 13000);
// The ENGLISH name, always: a URL mirrors the English wiki page name, and the
// display name is Chinese in the second pass (docs/DATA_SOURCES: a localized
// name in a wiki URL lands on garbage — the same is true of our own routes).
const weapons = await evaluate(
  `META.weapons.map((w) => ({ id: w.id, slug: (w.name_en || w.wiki_name || w.name).replace(/ /g, "_") }))`,
);
// EVERY PAGE WAITS 1400 ms, so the sweep costs `weapons x 6 x 1.4 s` and that
// grew with the roster: 130 entries was 18 minutes and 382 is over an hour. A
// check that prints nothing for an hour reads as HUNG, and on 2026-08-20 it was
// read as one — so it says where it is and what it has left.
const total = weapons.length * 6;
const began = Date.now();
let done = 0;
console.log(`sweeping ${weapons.length} weapons x 3 tabs, both languages `
  + `— ${total} page loads at 1.4 s each, about ${Math.round(total * 1.5 / 60)} min`);

for (const lang of ["en", "zh"]) {
  await app.setLang(lang, 13000);
  for (const w of weapons) {
    for (const tab of ["", "/simulator", "/optimizer"]) {
      const url = `/weapons/${w.slug}${tab}`;
      current = `${lang} ${w.id}${tab}`;
      const v = await evaluate(`(async () => { try {
        history.pushState({}, '', ${JSON.stringify(url)}); route();
        await new Promise((r) => setTimeout(r, 1400));
        const panel = document.querySelector('#panel, #sim, #opt, main');
        const on = document.getElementById('weapon');
        return {
          drew: !!panel && panel.textContent.trim().length > 40,
          home: document.body.classList.contains('on-home'),
          weapon: on ? on.value : null,
        };
      } catch (e) { return { threw: String((e && e.stack) || e).slice(0, 240) }; } })()`);
      if (!v) problems.push(`${current} the evaluate itself rejected`);
      else if (v.threw) problems.push(`${current} THREW ${v.threw}`);
      else if (v.home) problems.push(`${current} FELL HOME — ${url} resolved to no weapon`);
      else if (!v.drew) problems.push(`${current} BLANK`);
      else if (v.weapon !== w.id) problems.push(`${current} WRONG WEAPON ${v.weapon}`);
      done += 1;
      if (done % 120 === 0) {
        const secs = (Date.now() - began) / 1000;
        const left = Math.round((secs / done) * (total - done) / 60);
        console.log(`  … ${done}/${total} pages, ${problems.length} problem(s), `
          + `about ${left} min left`);
      }
    }
  }
}

// One line per distinct problem: a broken render usually reports itself on
// every tab of the weapon, and three copies of one stack are not three bugs.
const distinct = [...new Set(problems.map((p) => p.slice(0, 240)))];
check(`${weapons.length * 6} pages came up clean`, distinct.length === 0);
distinct.slice(0, 30).forEach((p) => console.log("      " + p));
if (distinct.length > 30) console.log(`      …and ${distinct.length - 30} more`);

await app.finish("every weapon page draws, in both languages");
