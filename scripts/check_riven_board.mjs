// Published riven rows stay readable without owning a card. Only copying a
// build creates equipment; editing that copy cannot change the published row.
// Synthetic rows keep this check independent of the current board's winners.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000, base: process.env.WFSIM_BASE });
const { evaluate, check, finish } = app;

// A shape a rifle can really roll, so nothing downstream refuses it.
const SHAPE = { bonuses: ["critical_damage", "damage", "multishot"], malus: "recoil" };

const setup = `
  history.pushState({}, '', '/weapons/Torid'); route();
  await new Promise(r => setTimeout(r, 4500));
  const id = $('weapon').value;
  const rows = (BOARD[id] || []).filter(r => r.benchmark === 'standard_single_target');
  if (!rows.length) return { error: 'no standard_single_target rows for ' + id };
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
    const entries = benchEntries({ id: 'standard_single_target' }, META.weapons || []);
    const e = entries.find(x => x.w.id === id && x.mode === 'cycle')
           || entries.find(x => x.w.id === id);
    seen[v] = e && e.row ? { score: e.row.score, riven: !!e.row.riven } : null;
  }
  benchRivenView = 'all';
  return { id, seen, plainTop: best.score, synthTop: best.score * 1.5 };
})()`);

check("a riven row is listed at all", views.seen && views.seen.all,
  JSON.stringify(views));
check("...and under `all builds` the better one wins, riven or not",
  views.seen.all?.riven === true, JSON.stringify(views.seen.all));
// THE POINT OF THE FILTER: the plain build is reachable again.
check("...`no riven` shows the plain build instead",
  views.seen.plain && views.seen.plain.riven === false,
  JSON.stringify(views.seen.plain));
check("...and it is a DIFFERENT row, not the same one relabelled",
  views.seen.plain?.score !== views.seen.all?.score,
  `${views.seen.plain?.score} vs ${views.seen.all?.score}`);
check("...`riven only` shows the riven build",
  views.seen.riven && views.seen.riven.riven === true,
  JSON.stringify(views.seen.riven));

// ---- a rank says what it is best of ----------------------------------------

const picker = await evaluate(`(async () => {
  ${setup}
  const bs = builtinBuilds();
  const mine = bs.filter(b => b.benchmark === 'standard_single_target');
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

// ---- reading and copying have separate ownership boundaries ---------------

const ownership = await evaluate(`(async () => {
  ${setup}
  const entry = builtinBuilds().find(b => b.board && b.board.riven);
  if (!entry) return { error: 'no published riven fixture' };
  const legacy = boardRivenName(entry.board.riven);
  const before = localStorage.getItem('wfsim-customs-rivens');
  const beforeList = JSON.stringify(loadPresetList(RIVENS));
  const cfg = buildBarCfg();
  cfg.setActive('');
  pickPreset(cfg, entry.builtin);
  await new Promise(r => setTimeout(r, 400));
  const ref = slots.find(s => isRivenId(s.mod))?.mod;
  const expected = boardRivenState(entry.board.riven);
  const spec = () => rivenSpecFor(ref)?.spec;
  const request = () => rivenPayload().find(p => RIVEN_PREFIX + p.id === ref)?.spec;
  const out = {
    noOwned: JSON.stringify(loadPresetList(RIVENS)) === beforeList,
    noStorage: localStorage.getItem('wfsim-customs-rivens') === before,
    ref, readable: !!modById(ref),
    spec: JSON.stringify(spec()) === JSON.stringify(expected),
    request: JSON.stringify(request()) === JSON.stringify(expected),
    filled: slots.slice(0, 8).filter(s => s.mod).length
  };
  const wire = sharePayload()[6];
  out.share = wire.length === 1 && wire[0][2] === 8 && wire[0][3] === 'M'
    && JSON.stringify(wire[0][4]) === JSON.stringify(expected.bonuses.map(b => [b.id, b.roll]))
    && JSON.stringify(wire[0][5]) === JSON.stringify([expected.malus.id, expected.malus.roll]);
  const variantRef = boardRivenRef(weaponInfo('burston_prime'), entry.board.riven);
  out.family = readOnlyRiven(ref, 'lex') === null
    && !!readOnlyRiven(variantRef, 'burston') && !!readOnlyRiven(variantRef, 'burston_prime');
  const old = { id: legacy, name: legacy, scope: rivenScope(id), savedAt: 1,
    state: JSON.parse(JSON.stringify(expected)) };
  old.state.bonuses[0].roll = 0.9;
  storePresetList(RIVENS, loadPresetList(RIVENS).filter(p => p.id !== legacy).concat([old]));
  out.legacyCannotOverride = JSON.stringify(spec()) === JSON.stringify(expected);
  deleteRiven(legacy);
  out.afterDelete = slots.some(s => s.mod === ref) && !!modById(ref)
    && JSON.stringify(spec()) === JSON.stringify(expected)
    && JSON.stringify(request()) === JSON.stringify(expected);
  restoreState(entry.state, id);
  out.reopened = slots.some(s => s.mod === ref) && !!modById(ref)
    && !loadPresetList(RIVENS).some(p => p.id === legacy);
  const ownedBefore = loadPresetList(RIVENS).length;
  cfg.setActive(entry.builtin);
  const copyId = copyActivePreset(cfg);
  const copy = loadPresetList(BUILDS).find(p => p.id === copyId);
  const copyRef = copy?.state.slots.find(s => isRivenId(s.mod))?.mod;
  const made = loadPresetList(RIVENS).find(p => RIVEN_PREFIX + p.id === copyRef);
  out.copy = !!made && made.id !== legacy && copyRef !== ref
    && !copyRef.startsWith('riven:readonly:') && made.id !== made.name
    && loadPresetList(RIVENS).length === ownedBefore + 1;
  out.copyRefs = slots.some(s => s.mod === copyRef)
    && copy?.state.slots.some(s => s.mod === copyRef);
  if (made) {
    made.state.bonuses[0].roll = 0.8;
    storePresetList(RIVENS, loadPresetList(RIVENS).map(p => p.id === made.id ? made : p));
  }
  restoreState(entry.state, id);
  out.isolated = !!made && JSON.stringify(spec()) === JSON.stringify(expected)
    && rivenSpecFor(copyRef)?.spec.bonuses[0].roll === 0.8;
  cfg.setActive(''); pickPreset(cfg, entry.builtin);
  const publishedBefore = JSON.stringify(entry.state);
  const countBeforeEdit = loadPresetList(RIVENS).length;
  setRivenRank(ref, 7); flushPresetSaves();
  const editRef = slots.find(s => isRivenId(s.mod))?.mod;
  const editBuild = loadPresetList(BUILDS).find(p => p.id === activePreset);
  out.firstEdit = editRef !== ref && editRef !== copyRef
    && loadPresetList(RIVENS).length === countBeforeEdit + 1
    && rivenSpecFor(editRef)?.spec.rank === 7
    && editBuild?.state.slots.some(s => s.mod === editRef)
    && JSON.stringify(entry.state) === publishedBefore && rivenSpecFor(ref)?.spec.rank === 8;
  return out;
})()`);
for (const [label, key] of [
  ['opening a published riven build leaves the owned list unchanged', 'noOwned'],
  ['...and leaves owned storage unchanged', 'noStorage'],
  ['...the riven resolves as a mod', 'readable'],
  ['...with the published specification', 'spec'],
  ['...and its specification travels with simulation requests', 'request'],
  ['...and its full specification travels in share field 6', 'share'],
  ['read-only references resolve across their family and refuse unrelated weapons', 'family'],
  ['a legacy owned card cannot override the published specification', 'legacyCannotOverride'],
  ['deleting that card preserves the published mod, specification and request', 'afterDelete'],
  ['reopening remains readable without recreating the deleted owned card', 'reopened'],
  ['copying creates one fresh opaque owned riven identity', 'copy'],
  ['...the saved copy and live slots reference that owned identity', 'copyRefs'],
  ['editing the owned copy leaves the published specification unchanged', 'isolated'],
  ['the first rank edit forks a fresh owned card and build, preserving the published state', 'firstEdit'],
]) check(label, ownership[key] === true, JSON.stringify(ownership));
check('the published riven fills its own slot', ownership.filled === 8, JSON.stringify(ownership));

const corners = await evaluate(`(async () => {
  ${setup}
  const rowAt = (roll) => ({ ...BOARD[id][0], score: BOARD[id][0].score + roll,
    riven: { ...BOARD[id][0].riven, rolls: [1.1, 1.1, 1.1, roll] } });
  BOARD[id] = [rowAt(0.9), rowAt(1.1), ...BOARD[id].slice(1)];
  const entries = builtinBuilds().filter(b => b.board && b.board.riven);
  const seen = [];
  const before = JSON.stringify(loadPresetList(RIVENS));
  for (const roll of [0.9, 1.1]) {
    const entry = entries.find(e => e.board.riven.rolls[3] === roll);
    if (!entry) return { error: 'missing corner ' + roll };
    restoreState(entry.state, id);
    const ref = slots.find(s => isRivenId(s.mod))?.mod;
    seen.push({ ref, roll: rivenSpecFor(ref)?.spec.malus?.roll });
  }
  return { seen, unchanged: JSON.stringify(loadPresetList(RIVENS)) === before };
})()`);
check('two corners resolve to distinct read-only cards with their own malus rolls',
  corners.seen?.length === 2 && corners.seen[0].ref !== corners.seen[1].ref
    && corners.seen[0].roll === 0.9 && corners.seen[1].roll === 1.1
    && corners.unchanged, JSON.stringify(corners));

const elements = await evaluate(`(() => {
  const w = weaponInfo($('weapon').value);
  const row = (BOARD[w.id] || []).find(r => r.riven);
  const orders = [['heat', 'cold', 'multishot'], ['cold', 'heat', 'multishot']];
  const seen = orders.map(bonuses => {
    const state = boardRowState(w, { ...row, riven: { ...row.riven, bonuses, rolls: [0.9, 1.1, 1, 1.1] } });
    const ref = state.slots.find(s => isRivenId(s.mod))?.mod;
    return { ref, bonuses: rivenSpecFor(ref)?.spec.bonuses };
  });
  return { distinct: seen[0].ref !== seen[1].ref,
    preserved: seen.every((s, i) => JSON.stringify(s.bonuses) === JSON.stringify(
      orders[i].map((id, j) => ({ id, roll: [0.9, 1.1, 1][j] })))) };
})()`);
check('elemental bonus order survives read-only resolution and distinguishes references',
  elements.distinct && elements.preserved, JSON.stringify(elements));

// ---- the negative control ---------------------------------------------------

// A weapon whose board holds no riven row must create nothing and show no
// riven group. Without this the whole file would pass just as well on a page
// that made a riven for every build it opened.
const plain = await evaluate(`(async () => {
  history.pushState({}, '', '/weapons/Lex'); route();
  await new Promise(r => setTimeout(r, 4000));
  const id = $('weapon').value;
  const before = loadPresetList(RIVENS).length;
  const entry = builtinBuilds().find(b => b.benchmark === 'standard_single_target');
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

await finish("published rivens stay read-only until their build is copied");
