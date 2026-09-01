// THE BENCHMARK BAR IS A RETRIEVER, AND A RETRIEVER'S SHAPE DOES NOT MOVE.
//
// Four controls — ruler, mode, riven, rank — on every weapon, whether it has a
// hundred board rows or none, and whether or not any of the four has more than
// one answer. A bar that grew a riven control on some weapons and dropped it on
// others taught the reader to read its SHAPE as information, and then "no riven
// control" and "no riven" were the same picture.
//
// …AND THE PAGE SAYS SEPARATELY WHAT IS OPEN. The bar falls back to the board's
// leader whenever nothing official is loaded, so a reader on their own build was
// shown a ruler, a mode and a rank belonging to a build that was NOT on the
// page. That is the sentence the bar was never in a position to say, and it is
// asserted here in all three of its states.
//
//   node scripts/check_build_retriever.mjs
//
// Exits non-zero on the first failure.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000 });
const { evaluate, check } = app;

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  localStorage.clear();
  const bar = () => document.getElementById('bench-bar-builder-builds');
  const dds = () => Array.from(bar().querySelectorAll('button.dd'));
  const shape = () => dds().map((d) => d.id.replace(/-builder-builds$/, ''));
  const values = () => dds().map((d) => (d.querySelector('.dd-v') || {}).textContent || '');
  const dead = () => dds().filter((d) => d.disabled).length;
  const line = () => {
    const el = document.getElementById('build-current');
    return el ? el.textContent.replace(/\\s+/g, ' ').trim() : '';
  };
  const open = async (name) => {
    history.pushState({}, '', '/weapons/' + name); route(); await sleep(3500);
  };
  const out = {};

  // A WEAPON WITH ROWS. The Ballistica Prime is the case the shape was written
  // for: four modes on the board and two rankings under some of them.
  await open('Ballistica_Prime');
  out.richRows = ((typeof BOARD !== 'undefined' && BOARD) ? (BOARD['ballistica_prime'] || []) : []).length;
  out.richHidden = !!bar().hidden;
  out.richShape = shape();
  out.richValues = values();
  out.richLine = line();

  // …AND ONE WITH NONE. Every control is dead and every value is the
  // placeholder, but the row is the same four questions in the same order.
  //
  // ASKED OF THE BAR, not of the fetched board. The bar draws whatever
  // builtinBuilds hands it, which is not keyed the way board.json is — picking
  // a weapon that looked bare there and finding four live controls is how this
  // was learned. So: try candidates until one is bare TO THE BAR.
  const rowsNow = () => buildBarCfg().load().filter((p) => p.builtin).length;
  out.bareId = '';
  for (const w of (META.weapons || [])) {
    if (((BOARD && BOARD[w.id]) || []).length) continue;
    await open(w.wiki_name || w.name.replace(/ /g, '_'));
    if (rowsNow()) continue;
    out.bareId = w.id;
    out.bareHidden = !!bar().hidden;
    out.bareShape = shape();
    out.bareDead = dead();
    out.bareValues = values();
    out.bareLine = line();
    break;
  }

  // …AND THE SCENARIO BAR ASKS ONE QUESTION, not four. It shares this
  // renderer, and an official scenario is one per ruler — a mode, a riven-ness
  // and a rank there are three controls answering nothing and a fourth
  // repeating the first, which is what a shared shape costs when the two
  // collections do not ask the same thing.
  await open('Ballistica_Prime');
  const sbar = document.getElementById('bench-bar-simulator-scenarios');
  out.scenarioShape = sbar
    ? Array.from(sbar.querySelectorAll('button.dd')).map((d) => d.id.replace(/-simulator-scenarios$/, ''))
    : [];

  // THE THREE STATES OF "WHAT IS OPEN". Landing on a weapon page opens NONE of
  // the board's rows — the bar shows the leader because that is where its
  // controls default, and the build on the page is an unsaved one. That gap is
  // the whole reason this line exists, so it is the first thing asserted.
  await open('Ballistica_Prime');
  out.lineLanding = line();
  const cfg = buildBarCfg();
  const rows = cfg.load().filter((p) => p.builtin);
  out.rowCount = rows.length;
  pickPreset(cfg, presetId(rows[0])); await sleep(1200);
  out.lineOfficial = line();
  copyActivePreset(buildBarCfg()); await sleep(800);
  out.lineOwn = line();
  return out;
})()`);

const FOUR = ["dd-bench", "dd-bench-mode", "dd-bench-kind", "dd-bench-row"];
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

// THE CASE HAS TO EXIST. `board.json` is fetched at runtime and the NATIVE dev
// server does not serve it, so a run pointed there sees an empty board and every
// assertion below passes on placeholder text. This is what makes that loud.
check(
  "the weapon under test actually has board rows",
  r.richRows > 0,
  `${r.richRows} rows — is this running against site/?`,
);
check("the retriever is drawn on a weapon that has rows", r.richHidden === false);
check(
  "...as four controls, in one order",
  same(r.richShape, FOUR),
  JSON.stringify(r.richShape),
);
check(
  "...each answering its own question, and none of them empty-handed",
  r.richValues.length === 4 && r.richValues.every((v) => v && v.trim() && v.trim() !== "—"),
  JSON.stringify(r.richValues),
);

check(
  `the same four are drawn on a weapon with NO rows (${r.bareId || "none found"})`,
  !!r.bareId && r.bareHidden === false && same(r.bareShape, FOUR),
  JSON.stringify(r.bareShape),
);
check(
  "...and every one of them is dead rather than absent",
  r.bareDead === 4,
  `${r.bareDead} of 4 disabled`,
);
check(
  "...each showing the placeholder rather than another weapon's answer",
  Array.isArray(r.bareValues) && r.bareValues.every((v) => v.trim() === "—"),
  JSON.stringify(r.bareValues),
);

// WHAT IS OPEN, in each of the three states it has. Asserted on the STATE the
// line names rather than on its wording, which is a translated string.
check(
  "a board row says it is one, and cannot be edited",
  /read-only|cannot be edited|不能编辑|榜单行/.test(r.lineOfficial),
  r.lineOfficial,
);
check(
  "...a copy of it says it is your own",
  /your own|你自己/.test(r.lineOwn),
  r.lineOwn,
);

check(
  "the SCENARIO bar asks one question, not the build bar's four",
  JSON.stringify(r.scenarioShape) === JSON.stringify(["dd-bench"]),
  JSON.stringify(r.scenarioShape),
);

// LANDING ON A WEAPON OPENS NO ROW, and the bar cannot say so: its controls
// default to the board's leader whether or not that leader is loaded. This is
// the case the line was added for, so it is asserted rather than assumed.
check(
  "landing on a weapon opens an UNSAVED build, whatever the bar is showing",
  /unsaved|尚未保存/.test(r.lineLanding),
  r.lineLanding,
);
check(
  "the line is not the bar: three states, three sentences",
  r.lineLanding !== r.lineOfficial && r.lineOwn !== r.lineOfficial,
  `${r.lineLanding} | ${r.lineOfficial} | ${r.lineOwn}`,
);

process.exit(0);
