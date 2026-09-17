// A BOARD ROW MAY WEAR A RIVEN, AND TAKING IT GIVES YOU THE RIVEN.
//
// A riven is an item that exists on one machine, so what a row holds is a
// SHAPE — which stats, which is the malus — scored at that shape's own ceiling,
// the way every row is scored at full Forma. Three things have to be true on
// the page, each with a way of being subtly wrong that looks fine:
//
//   * THE RANKING IS ONE LIST AND THE VIEW NARROWS IT. A riven build does not
//     always beat a plain one, so they rank together — but under "all builds" a
//     weapon whose riven build wins hides its plain one, which is the build
//     most readers can actually make. A filter that quietly showed the same row
//     in all three positions would look like it worked.
//   * A RANK SAYS WHAT IT IS BEST OF: the floor treats riven and plain as
//     separate fields, so a weapon has two leaders and a `#1` that could mean
//     either is the one number on the page not saying what it ranks among.
//   * TAKING THE ROW GIVES YOU THE RIVEN. The record carries the bare `riven`
//     and a mod id has to name an ITEM, so one is created once, idempotently,
//     only when the build is taken — without it the slot is dropped.
//
// The row is SYNTHETIC, because the live board has no riven build yet and a
// check waiting for one would pass by doing nothing for weeks.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000, base: process.env.WFSIM_BASE });
const { evaluate, check, finish } = app;

// A shape a rifle can really roll, so nothing downstream refuses it.
const SHAPE = { bonuses: ["critical_damage", "damage", "multishot"], malus: "recoil" };

const setup = `
  history.pushState({}, '', '/weapons/Torid'); route();
  await new Promise(r => setTimeout(r, 4500));
  const id = $('weapon').value;
  const rows = (BOARD[id] || []).filter(r => r.benchmark === 'single_target');
  if (!rows.length) return { error: 'no single_target rows for ' + id };
  const best = rows.reduce((a, r) => (r.score > a.score ? r : a), rows[0]);
  // A RIVEN ROW BEATING THE PLAIN LEADER, which is the case the "no riven"
  // view exists for: under "all builds" it hides the plain one.
  const synth = {
    ...best,
    score: best.score * 1.5,
    // THE SLOT AS WELL AS THE SHAPE. A record carries the bare riven id at the
    // riven own position — position is the build — so a fixture that named a
    // shape and no slot would be testing a row the endpoint cannot produce.
    mods: [...(best.mods || [])].map((m, i) => (i === 0 ? 'riven' : m)),
    riven: {
      bonuses: ${JSON.stringify(SHAPE.bonuses)},
      malus: ${JSON.stringify(SHAPE.malus)},
      rolls: [1.1, 1.1, 1.1, 1.1],
    },
  };
  BOARD[id] = [synth, ...rows];`;

// ---- the view narrows, and the ranking is one list -------------------------

const views = await evaluate(`(async () => {
  ${setup}
  const seen = {};
  for (const v of ['all', 'plain', 'riven']) {
    benchRivenView = v;
    const e = benchEntries('single_target').find(x => x.w.id === id && x.mode === 'cycle')
           || benchEntries('single_target').find(x => x.w.id === id);
    seen[v] = e && e.row ? { score: e.row.score, riven: !!e.row.riven } : null;
  }
  benchRivenView = 'all';
  return { id, seen, plainTop: best.score, synthTop: best.score * 1.5 };
})()`);

check("a riven row is listed at all", views.seen && views.seen.all,
  JSON.stringify(views));
check("...and under `all builds` the better one wins, riven or not",
  views.seen.all.riven === true, JSON.stringify(views.seen.all));
// THE POINT OF THE FILTER: the plain build is reachable again.
check("...`no riven` shows the plain build instead",
  views.seen.plain && views.seen.plain.riven === false,
  JSON.stringify(views.seen.plain));
check("...and it is a DIFFERENT row, not the same one relabelled",
  views.seen.plain.score !== views.seen.all.score,
  `${views.seen.plain.score} vs ${views.seen.all.score}`);
check("...`riven only` shows the riven build",
  views.seen.riven && views.seen.riven.riven === true,
  JSON.stringify(views.seen.riven));

// ---- a rank says what it is best of ----------------------------------------

const picker = await evaluate(`(async () => {
  ${setup}
  const bs = builtinBuilds();
  const mine = bs.filter(b => b.benchmark === 'single_target');
  const riven = mine.filter(b => b.board && b.board.riven);
  const plain = mine.filter(b => !(b.board && b.board.riven));
  return {
    rivenFirstRank: riven.length ? riven[0].rank : null,
    plainFirstRank: plain.length ? plain[0].rank : null,
    rivenName: riven.length ? riven[0].name : null,
    plainName: plain.length ? plain[0].name : null,
    // THE WORD THE PAGE ITSELF USES. Asserting the English one would pass only
    // on an English page, and this app's default is not English — DE's own
    // Chinese for a riven is 裂罅, which no English regex would have found.
    word: tr('riven'),
    // Two entries may not share a builtin id: it is what the active pointer
    // stores, so a collision makes one of them unreachable.
    ids: mine.map(b => b.builtin),
    unique: new Set(mine.map(b => b.builtin)).size === mine.length,
    // The riven rows must be CONTIGUOUS INSIDE A MODE — a mode is a control of
    // its own and holds its own two rankings. Across the weapon they are not.
    contiguous: (() => {
      const byMode = {};
      for (const b of mine) {
        (byMode[b.mode] = byMode[b.mode] || []).push(!!(b.board && b.board.riven));
      }
      return Object.values(byMode)
        .every(fs => fs.filter((v, i) => i && v !== fs[i - 1]).length <= 1);
    })(),
  };
})()`);

check("a riven row and a plain row each start at #1 of their own group",
  picker.rivenFirstRank === 1 && picker.plainFirstRank === 1,
  JSON.stringify(picker));
check("...and the riven one says so in its name, in the page's own language",
  (picker.rivenName || "").includes(picker.word)
    && !(picker.plainName || "").includes(picker.word),
  `"${picker.word}": riven "${picker.rivenName}" vs plain "${picker.plainName}"`);
check("...no two entries share a builtin id", picker.unique, picker.ids.join(" | "));
check("...and the riven rows are contiguous inside each mode",
  picker.contiguous, JSON.stringify(picker.ids));

// ---- taking the row gives you the riven ------------------------------------

const taken = await evaluate(`(async () => {
  ${setup}
  const before = loadPresetList(RIVENS).length;
  const entry = builtinBuilds().find(b => b.board && b.board.riven);
  if (!entry) return { error: 'no riven entry to take' };

  restoreState(entry.state, id);
  await new Promise(r => setTimeout(r, 900));
  const after = loadPresetList(RIVENS);
  const slot = slots.find(s => isRivenId(s.mod));

  // TAKEN TWICE is the sharp half: the name is derived from the SHAPE, so the
  // second take must find the first copy rather than stack another behind it.
  restoreState(entry.state, id);
  await new Promise(r => setTimeout(r, 900));
  const twice = loadPresetList(RIVENS).length;

  const made = after.find(p => RIVEN_PREFIX + p.name === (slot || {}).mod);
  return {
    before, after: after.length, twice,
    equipped: (slot || {}).mod || null,
    stats: made ? (made.state.bonuses || []).map(b => b.id) : null,
    malus: made && made.state.malus ? made.state.malus.id : null,
    rolls: made ? (made.state.bonuses || []).map(b => b.roll) : null,
    // The build must still be eight mods — the riven filling its own slot.
    filled: slots.slice(0, 8).filter(s => s.mod).length,
  };
})()`);

check("taking a riven row creates the riven locally",
  taken.after === taken.before + 1, JSON.stringify(taken));
check("...and the build actually wears it",
  !!taken.equipped && taken.filled === 8, JSON.stringify(taken));
check("...with the row's own stats and rolls",
  JSON.stringify(taken.stats) === JSON.stringify(SHAPE.bonuses)
    && taken.malus === SHAPE.malus
    && (taken.rolls || []).every((r) => r === 1.1),
  JSON.stringify(taken));
check("...and taking it twice does not stack a second copy",
  taken.twice === taken.after, `${taken.before} -> ${taken.after} -> ${taken.twice}`);

// ---- two corners of one shape are two cards --------------------------------

// ONE SHAPE, TWO CORNERS. The fight can store a shape at its god roll on one
// row and with the malus at its other end on another; each row must hand over
// ITS OWN riven, and a copy an older page filed under the god roll's name with
// the other corner's numbers must come back corrected.
const corners = await evaluate(`(async () => {
  ${setup}
  const rowAt = (roll) => ({ ...BOARD[id][0], score: BOARD[id][0].score + roll,
    riven: { ...BOARD[id][0].riven, rolls: [1.1, 1.1, 1.1, roll] } });
  BOARD[id] = [rowAt(0.9), rowAt(1.1), ...BOARD[id].slice(1)];
  const entries = builtinBuilds().filter(b => b.board && b.board.riven);
  const malusOf = async (entry) => {
    restoreState(entry.state, id);
    await new Promise(r => setTimeout(r, 900));
    const slot = slots.find(s => isRivenId(s.mod));
    const made = loadPresetList(RIVENS).find(p => RIVEN_PREFIX + p.id === (slot || {}).mod);
    return { slot: (slot || {}).mod || null, roll: made && made.state.malus ? made.state.malus.roll : null };
  };
  const god = entries.find(e => e.board.riven.rolls[3] === 0.9);
  const deep = entries.find(e => e.board.riven.rolls[3] === 1.1);
  if (!god || !deep) return { error: 'the two corners were not listed', n: entries.length };
  // A STALE COPY under the god roll's id, holding the deep corner's numbers.
  const godId = boardRivenName(god.board.riven);
  const ps = loadPresetList(RIVENS).filter(p => p.id !== godId);
  ps.push({ id: godId, name: godId, savedAt: 1, state: boardRivenState(deep.board.riven) });
  storePresetList(RIVENS, ps);
  const g = await malusOf(god);
  const d = await malusOf(deep);
  return { god: g, deep: d };
})()`);
check("two corners of one shape are two cards",
  !!corners.god && !!corners.deep && corners.god.slot !== corners.deep.slot, JSON.stringify(corners));
check("...and each card's name states every stat's roll",
  ["god", "deep"].every((k) => corners[k] && ["critical_damage x1.1", "damage x1.1", "multishot x1.1"]
    .every((t) => corners[k].slot.includes(t)))
    && corners.god.slot.endsWith("recoil x0.9") && corners.deep.slot.endsWith("recoil x1.1"),
  JSON.stringify(corners));
check("...each with its own malus roll, a stale copy corrected",
  corners.god && corners.god.roll === 0.9 && corners.deep && corners.deep.roll === 1.1,
  JSON.stringify(corners));

// ---- the negative control ---------------------------------------------------

// A weapon whose board holds no riven row must create nothing and show no
// riven group. Without this the whole file would pass just as well on a page
// that made a riven for every build it opened.
const plain = await evaluate(`(async () => {
  history.pushState({}, '', '/weapons/Lex'); route();
  await new Promise(r => setTimeout(r, 4000));
  const id = $('weapon').value;
  const before = loadPresetList(RIVENS).length;
  const entry = builtinBuilds().find(b => b.benchmark === 'single_target');
  if (entry) { restoreState(entry.state, id); await new Promise(r => setTimeout(r, 800)); }
  return {
    took: !!entry,
    before, after: loadPresetList(RIVENS).length,
    rivenGroups: builtinBuilds().filter(b => b.board && b.board.riven).length,
    wearing: slots.filter(s => isRivenId(s.mod)).length,
  };
})()`);
check("a weapon with no riven row makes no riven", plain.after === plain.before,
  JSON.stringify(plain));
check("...and shows no riven group and wears nothing",
  plain.rivenGroups === 0 && plain.wearing === 0, JSON.stringify(plain));

await finish("a board row may wear a riven, and taking it gives you the riven");
