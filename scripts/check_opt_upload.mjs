// EVERY FINALIST GOES TO THE BOARD, AND THE BOARD'S OWN DOOR DECIDES.
//
// A search that ranks twenty builds uploaded NONE of them: the only path to the
// store ran off a simulator run, one build at a time, so the strongest thing
// this app produces reached the board only if a player copied a row into the
// builder by hand and ran it again.
//
// TWO PROPERTIES, and the second is the one that is easy to get wrong:
//
//   * the payload comes off the ROW — its mods, its arcanes, its mode, its
//     valence — and not off the page, which holds a different build;
//   * NOTHING IS PRE-FILTERED HERE. How full a build must be is the searcher's
//     own setting, so a seven-mod scope produces seven-mod winners, and those
//     are refused by `/api/board/check` — which IS `validate_for_board` rather
//     than a copy of it. A second implementation is a second answer.
//
//   node scripts/check_opt_upload.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check, send, sleep, BASE } = app;

await evaluate(`localStorage.clear(); localStorage.setItem('wfsim-lang', 'en')`);
await send("Page.navigate", { url: BASE });
await sleep(12000);

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  history.pushState({}, '', '/weapons/Kuva_Nukor'); route(); await sleep(3500);
  const out = {};

  // THE PAYLOAD IS THE ROW'S. Built from a result that shares no axis with the
  // page's own build, so a builder reading page state cannot pass by accident.
  const res = {
    rank: 1,
    mods: ['hornet_strike', 'barrel_diffusion'],
    exilus: 'pistol_amp',
    arcane: ['secondary_deadhead'],
    arcane_rank: [5],
    evolutions: ['kuva_nukor_evo1_incarnon_form'],
    mode: 'cycle',
    valence: 'magnetic',
  };
  const p = boardPayloadFromResult(res);
  out.payload = p;
  out.pageMods = mainSlots().filter((s) => s.mod).map((s) => s.mod);

  // …AND THE DOOR IS ASKED, not second-guessed. A two-mod build is not a
  // complete one, and the answer has to come from the board rather than here.
  const verdict = await boardVerdict(p);
  out.verdict = verdict;

  // CONSENT STILL GATES IT. With uploading off nothing leaves, and the line
  // says so rather than staying blank.
  document.body.insertAdjacentHTML('beforeend', '<div id="opt-board"></div>');
  setBoardConsent && setBoardConsent('no');
  localStorage.setItem('wfsim-board-consent', 'no');
  await offerOptBoardSubmit({ results: [res] });
  out.offText = (document.getElementById('opt-board').textContent || '').trim();
  return out;
})()`);

check(
  "the payload is built from the ROW, not from the page",
  JSON.stringify(r.payload.mods) === JSON.stringify(["hornet_strike", "barrel_diffusion"]),
  JSON.stringify(r.payload.mods),
);
check(
  "…and the page's own build is a different one",
  JSON.stringify(r.pageMods) !== JSON.stringify(r.payload.mods),
  `${JSON.stringify(r.pageMods)} vs ${JSON.stringify(r.payload.mods)}`,
);
for (const [field, want] of [
  ["mode", "cycle"],
  ["valence", "magnetic"],
  ["exilus", "pistol_amp"],
]) {
  check(`the payload carries the row's ${field}`, r.payload[field] === want, String(r.payload[field]));
}
check(
  "…its arcanes, with `none` dropped",
  JSON.stringify(r.payload.arcanes) === JSON.stringify(["secondary_deadhead"]),
  JSON.stringify(r.payload.arcanes),
);
check(
  "…and its evolutions",
  JSON.stringify(r.payload.evolutions) === JSON.stringify(["kuva_nukor_evo1_incarnon_form"]),
  JSON.stringify(r.payload.evolutions),
);

// AN INCOMPLETE BUILD IS REFUSED BY THE BOARD, not by this page. Two mods is
// not eight, and what has to be true is that the ANSWER came from the door.
check(
  "a short build is refused, and the board is what refuses it",
  r.verdict && r.verdict.ok && r.verdict.accepted === false && !!r.verdict.reason,
  JSON.stringify(r.verdict),
);

check(
  "with uploading off, nothing is sent and the line says so",
  /not sent|off/i.test(r.offText),
  r.offText,
);

process.exit(0);
