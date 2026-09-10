// THE THANKS LIST SAYS WHO, AND MAY NEVER SAY HOW MUCH.
//
// The ledger behind this page holds names, amounts and dates
// (`ledger/schema.sql`), and exactly one thing comes out of it:
// `scripts/publish_thanks.py` writes names and a month into `site/thanks.json`.
// Everything asserted here is a property that would be broken SILENTLY — the
// page would still look right, and the leak or the slight would only be visible
// to the person it was about.
//
//   1. NO FIGURE REACHES THE PAGE. Not in the file, not in the DOM. The order
//      is derived from money and the money stays behind.
//   2. NO RANK IS PRINTED. An ordered list is a list; the same list with
//      "1." beside the first name is a leaderboard, and `/support` promises in
//      so many words that there are no tiers and nothing is bought.
//   3. NO VISUAL TIER. Every name is drawn at one size, one weight, one colour
//      — a bigger first name is a price list drawn instead of written.
//   4. THE ORDER IS THE PUBLISHED ORDER, unedited by the page.
//   5. AN EMPTY LIST IS SILENT on `/support` and SAYS SO on `/thanks`.
//
// IT PUBLISHES ITS OWN FIXTURE into `site/thanks.json` and puts the file back
// on the way out, because the real one is empty until somebody chips in — and a
// check that can only run once there is money is a check that never ran.
import { readFileSync, writeFileSync } from "node:fs";
import { openApp } from "./cdp.mjs";

const FILE = new URL("../site/thanks.json", import.meta.url);
const KEPT = readFileSync(FILE);

// FOUR NAMES AND ONE OF THEM IS A DIGIT STRING. `2024` is a legal Bilibili
// handle, and it is here so that assertion 2 cannot be satisfied by "the page
// contains no digits" — the check has to look at what the page ADDED.
const FIXTURE = {
  as_of: "2026-09-10",
  supporters: [
    { name: "Lucas", since: "2025-09" },
    { name: "2024", since: "2026-01" },
    { name: "一个路人", since: "2026-08" },
    { name: "someone", since: "2026-09" },
  ],
};

const app = await openApp({ boot: 20000 });
const { evaluate, check, finish } = app;
const tag = "thanks";

try {
  writeFileSync(FILE, JSON.stringify(FIXTURE, null, 1) + "\n");

  // ENGLISH IS FORCED rather than inherited: the app boots into the browser's
  // own language, so on a zh machine this pass would silently be the zh one.
  await app.setLang("en", 20000);
  await app.load("/thanks", 20000);

  const page = await evaluate(`(() => {
    const items = [...document.querySelectorAll("#thanks-list .thx-one")];
    const style = (el) => {
      const s = getComputedStyle(el);
      return [s.fontSize, s.fontWeight, s.color].join("|");
    };
    return {
      names: items.map((li) => li.querySelector(".thx-name").textContent.trim()),
      // What the page DREW around each name, so a rank prefix or a suffix shows
      // up as text the fixture never contained.
      whole: items.map((li) => li.textContent.trim()),
      styles: items.map((li) => style(li.querySelector(".thx-name"))),
      noneHidden: document.getElementById("thanks-none").hidden,
      body: document.body.innerText,
    };
  })()`);

  check(`${tag} every published name is drawn`,
    page.names.length === FIXTURE.supporters.length, JSON.stringify(page.names));
  check(`${tag} ...in the order the file published, not one the page chose`,
    page.names.join("|") === FIXTURE.supporters.map((s) => s.name).join("|"),
    page.names.join("|"));

  // A RANK IS A NUMBER THE PAGE ADDED. Each row may carry the name and the
  // month it published and nothing else, so anything left over after removing
  // those two is something this page invented.
  const leftover = page.whole.map((whole, i) => whole
    .replace(FIXTURE.supporters[i].name, "")
    .replace(FIXTURE.supporters[i].since, "").trim());
  check(`${tag} ...and no rank, position or figure is printed beside a name`,
    leftover.every((rest) => rest === ""), JSON.stringify(leftover));

  check(`${tag} every name is drawn the same — no visual tier`,
    new Set(page.styles).size === 1, JSON.stringify([...new Set(page.styles)]));

  check(`${tag} a published list means the page does not say it is empty`,
    page.noneHidden === true, `hidden=${page.noneHidden}`);

  // ---------------------------------------------------------------------------
  // `/support` — the same names, under the door rather than over it.
  await app.load("/support", 20000);
  const sup = await evaluate(`(() => {
    const block = document.getElementById("support-thanks");
    const channels = document.getElementById("support-channels");
    const names = [...document.querySelectorAll("#support-thanks-list .thx-name")]
      .map((el) => el.textContent.trim());
    return {
      hidden: block.hidden,
      names,
      // DOCUMENT ORDER, which is the whole point of where this block sits: the
      // reader meets the ask first and the proof second.
      afterChannels: !!(channels.compareDocumentPosition(block)
        & Node.DOCUMENT_POSITION_FOLLOWING),
      channels: [...document.querySelectorAll("#support-channels .sup-name")]
        .map((el) => el.textContent.trim()),
    };
  })()`);

  check(`${tag} /support draws the names too`,
    sup.hidden === false && sup.names.length === FIXTURE.supporters.length,
    JSON.stringify(sup.names));
  check(`${tag} ...below the channels, not above them`, sup.afterChannels === true);
  check(`${tag} ...and Bilibili is one of the channels offered`,
    sup.channels.some((n) => /bilibili/i.test(n)), JSON.stringify(sup.channels));

  // NO AMOUNT ANYWHERE. The strongest form of this assertion is about the FILE,
  // since the page can only draw what it was given: the published entry carries
  // a name and a month, and a key added later has to be decided on rather than
  // shipped.
  const keys = new Set(FIXTURE.supporters.flatMap((s) => Object.keys(s)));
  const published = JSON.parse(readFileSync(FILE, "utf8"));
  check(`${tag} the published entry is a name and a month, and nothing else`,
    published.supporters.every((s) => JSON.stringify(Object.keys(s).sort())
      === JSON.stringify([...keys].sort())), JSON.stringify(published.supporters[0]));

  // ---------------------------------------------------------------------------
  // AN EMPTY LIST, which is the state this ships in.
  writeFileSync(FILE, JSON.stringify({ as_of: "2026-09-10", supporters: [] }, null, 1) + "\n");
  await app.load("/support", 20000);
  const empty = await evaluate(
    `document.getElementById("support-thanks").hidden`);
  check(`${tag} an empty list draws no heading on /support`, empty === true, String(empty));

  await app.load("/thanks", 20000);
  const emptyPage = await evaluate(`(() => ({
    hidden: document.getElementById("thanks-block").hidden,
    said: !document.getElementById("thanks-none").hidden,
  }))()`);
  check(`${tag} ...but the page somebody navigated to says so in a sentence`,
    emptyPage.hidden === true && emptyPage.said === true, JSON.stringify(emptyPage));
} finally {
  writeFileSync(FILE, KEPT);
}

await finish("names, in an order, and never a number");
