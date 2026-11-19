// A SHARE BESIDE A BUILD IS THE ONE PANEL, FOR THAT BUILD (docs/UI.md §"A share
// link is a build"). Each entry — the bar's button, an opened finder row, a
// search's finalist, the simulator's result — must share THAT build through the
// one panel, where the reader can see it, and say which entry it was as
// `share.entry`. A board row is opened first; a finalist is not, because opening
// a build resets the search, so its panel sits under its row and encodes it.
//
//   node scripts/check_share_entries.mjs
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000, base: process.env.WFSIM_BASE });
const { evaluate, check } = app;

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  localStorage.clear();
  const sent = [];
  window.track = (e, subject) => { sent.push(e + ':' + (subject ?? '')); };
  const bar = () => document.getElementById('preset-bar-builder-builds');
  let host = bar;
  const panel = () => host().querySelector('.pshare');
  const shown = () => !!panel() && !panel().hidden && panel().offsetParent !== null;
  // The panel draws its link asynchronously; copy once it is there.
  const copy = async () => {
    for (let i = 0; i < 40 && !panel().querySelector('.sh-copy'); i++) await sleep(100);
    const b = panel().querySelector('.sh-copy');
    if (b) b.click();
    await sleep(200);
  };
  const entries = () => sent.filter((x) => x.startsWith('share.entry:')).map((x) => x.slice(12));
  const out = {};
  let copied = "";
  Object.defineProperty(navigator, "clipboard", {configurable: true, value: {
    writeText: async value => { copied = value; },
  }});
  const copyText = async () => {
    copied = "";
    await panel().querySelector(".sh-text").onclick();
    return copied;
  };

  // THE BAR'S OWN BUTTON.
  history.pushState({}, '', '/weapons/Ballistica_Prime/builder'); route(); await sleep(3500);
  bar().querySelector('.pchip.share').click(); await sleep(300);
  out.barShown = shown();
  await copy();
  out.barEntry = entries().at(-1);
  panel().hidden = true;

  // AN OPENED FINDER ROW: the second row, so it is not already the open build.
  history.pushState({}, '', '/weapons/Ballistica_Prime/build-library'); route();
  for (let i = 0; i < 50 && document.querySelectorAll('#build-finder tr.fr[data-frow]').length < 2; i++) await sleep(200);
  const rows = Array.from(document.querySelectorAll('#build-finder tr.fr[data-frow]')).map((t) => t.dataset.frow);
  finder.open = rows[1]; renderBuildFinder(); await sleep(200);
  const fs = document.querySelector('#build-finder [data-fshare]');
  out.finderButton = !!fs;
  if (fs) fs.click();
  await sleep(300);
  out.finderActive = activePreset === rows[1];
  out.finderShown = shown();
  await copy();
  out.finderEntry = entries().at(-1);
  const source = buildNamed(activePreset).board;
  const urlText = panel().querySelector(".sh-url").value;
  const citation = (await copyText()).split(String.fromCharCode(10))[0];
  const scene = META.benchmarks.find(b => b.id === source.benchmark);
  const conditions = tr(scene.name).split(" · ");
  const unit = conditions.pop();
  out.citation = [tr("Published leaderboard result"), conditions.join(" · "), (source.shown ?? source.score.toFixed(4)) + " " + unit,
    source.measured_by || tr("Engine version unavailable"), "WFSim", urlText].every(value => value && citation.includes(value));
  out.citationDetail = citation;
  const mods = s => s.slots.map(mod => mod?.mod).filter(Boolean).sort();
  const decoded = await decodeShare(new URL(urlText, location.origin).searchParams.get(SHARE_PARAM));
  const plainMods = s => mods(s).filter(id => !id.startsWith("riven:") && !id.startsWith("~"));
  out.boardMods = JSON.stringify(plainMods(decoded)) === JSON.stringify(plainMods(buildNamed(activePreset).state));
  const realClipboard = navigator.clipboard.writeText;
  navigator.clipboard.writeText = async () => { throw new Error("clipboard denied"); };
  await panel().querySelector(".sh-text").onclick();
  const selected = panel().querySelector("textarea");
  out.manualCopy = selected?.value.startsWith(citation) && selected.selectionEnd - selected.selectionStart === selected.value.length;
  navigator.clipboard.writeText = realClipboard;
  const legacy = shareBoardCitation({ ...source, measured_by: "", measured_at: "" });
  out.legacy = legacy.includes(tr("Engine version unavailable")) && (!source.measured_by || !legacy.includes(source.measured_by));
  panel().hidden = true;
  await openSharePanel(bar());
  out.pinnedBoard = (await copyText()).startsWith(citation);
  panel().hidden = true;
  const preset = buildNamed(activePreset);
  buildBarCfg().share(preset, bar());
  await copy();
  out.libraryBoard = (await copyText()).startsWith(citation);
  panel().hidden = true;

  // THE SIMULATOR'S RESULT: the open build, as it is.
  const before = activePreset;
  history.pushState({}, '', '/weapons/Ballistica_Prime/simulator'); route(); await sleep(1500);
  document.getElementById('sim-share').click(); await sleep(300);
  out.simKept = activePreset === before;
  out.simShown = shown();
  await copy();
  out.simEntry = entries().at(-1);
  const resultRead = window.resultForShare;
  window.resultForShare = async () => ({engine:"test-player-engine", r:{[metricOf(sim.metric).field]:2, duration:60}});
  try {
    panel().querySelector(".sh-result").click();
    for (let i=0; i<40 && !panel().querySelector(".sh-result:checked"); i++) await sleep(100);
    const personal = await copyText();
    out.personalUnpublished = personal.includes(tr("Player simulation")) && personal.includes("test-player-engine") &&
      !personal.includes(tr("Published leaderboard result"));
  } finally { window.resultForShare = resultRead; }
  panel().hidden = true;

  // A SEARCH'S FINALIST, from a search small enough to finish in seconds.
  history.pushState({}, '', '/weapons/Verglas_Prime/optimizer'); route(); await sleep(3500);
  await api('/api/optimize', { weapon: 'verglas_prime',
    mods: Object.fromEntries(['serration','split_chamber','point_strike','vital_sense','cryo_rounds','hellfire'].map((id) => [id, 'search'])),
    build_size: 6, build_min: 6, enemy: 'thrax_centurion', level: 200, steel_path: true,
    duration: 8, runs: 3, finalists: 3 });
  let res = null;
  for (let i = 0; i < 600 && !res; i++) {
    const st = await api('/api/optimize/status', {});
    if (st && ['done', 'cancelled', 'error'].includes(st.phase)) res = st.result;
    else await sleep(500);
  }
  out.searched = !!(res && (res.results || []).length);
  if (res) renderOptResults(res);
  await sleep(300);
  const n0 = loadPresetList(BUILDS).length, open0 = activePreset;
  const rows0 = document.querySelectorAll('#opt-results .opt-row').length;
  const os = () => document.querySelector('#opt-results .opt-share');
  out.optButton = !!os();
  if (os()) os().click();
  await sleep(800);
  host = () => os().closest('.opt-row');
  out.optUntouched = [loadPresetList(BUILDS).length === n0, activePreset === open0,
    document.querySelectorAll('#opt-results .opt-row').length === rows0];
  out.optShown = shown();
  out.optNoResultNoCard = !panel().querySelector('.sh-result') && !panel().querySelector('.sh-full');
  const url = (panel().querySelector('.sh-url') || {}).value || '';
  const code = new URL(url, location.origin).searchParams.get(SHARE_PARAM);
  const d = code ? await decodeShare(code) : null;
  const linked = d ? d.slots.map((x) => x && x.mod).filter(Boolean).sort() : [];
  out.optLinked = JSON.stringify(linked) === JSON.stringify([...res.results[0].mods].sort());
  out.optLinkedWhat = [linked, res.results[0].mods];
  await copy();
  out.optEntry = entries().at(-1);
  return out;
})()`);

const ok = (x) => JSON.stringify(x);
check("the bar's own button opens the panel, and says so", r.barShown && r.barEntry === "bar", ok([r.barShown, r.barEntry]));
check("an opened finder row carries a share", r.finderButton === true);
check("...which makes that board build the open one", r.finderActive === true);
check("...and opens the bar's panel in view, saying it came from the finder",
  r.finderShown && r.finderEntry === "finder", ok([r.finderShown, r.finderEntry]));
check("the simulator's result shares the open build as it is, from the bar's panel",
  r.simKept && r.simShown && r.simEntry === "simulator", ok([r.simKept, r.simShown, r.simEntry]));
check("the copied board citation includes its published score, fight, recorded engine and build link", r.citation, r.citationDetail);
check("the link carries the cited build and denied clipboard access selects the citation", r.boardMods && r.manualCopy);
check("the pinned build bar and the preset library cite the same published row", r.pinnedBoard && r.libraryBoard);
check("an older published row reports its missing version", r.legacy);
check("the simulator does not present the open build as a leaderboard result", r.personalUnpublished);
check("a small search finishes, so a finalist can be shared", r.searched === true);
check("a finalist carries a share", r.optButton === true);
check("...which saves nothing, opens nothing, and leaves the search's results",
  ok(r.optUntouched) === ok([true, true, true]), ok(r.optUntouched));
check("...and opens the panel under it in view, saying it came from the search",
  r.optShown && r.optEntry === "optimizer", ok([r.optShown, r.optEntry]));
check("...whose link is that finalist's build", r.optLinked === true, ok(r.optLinkedWhat));
check("...without the reader's result or the card, which describe the open build",
  r.optNoResultNoCard === true);

await app.finish("a share beside a build is the one panel, for that build");
