// Independent scene and shooting choices preserve the other axis and the URL.
//   WFSIM_BASE=http://127.0.0.1:<port> node scripts/check_board_ladder.mjs
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 11000, lang: "en" });
const { evaluate, check, send, sleep, BASE } = app;
// THE BOARD'S OWN DOCUMENT, opened by its address: a built site carries only
// its route's page, so the board's controls exist only once it is the page.
await app.load("/benchmark");
const result = await evaluate(`(async () => {
  for (let i = 0; i < 100 && !document.getElementById('bench-scenes'); i++) await new Promise((r) => setTimeout(r, 100));
  const all = benchList(), bases = all.filter(b => b.id === b.base);
  const failures = [];
  const want = (ok, why) => { if (!ok) failures.push(why); };
  want(all.length === 48 && bases.length === 4, 'four fights, four scenes, three shooting conditions');
  for (const base of bases) {
    const mine = all.filter(b => b.base === base.id);
    want(mine.length === 12, base.id + ': twelve cells');
    for (const b of mine) {
      benchPick = b.id;
      renderBenchBoard();
      const scene = (b.ladder || []).find(r => r.axis === 'scene');
      const shot = (b.ladder || []).find(r => r.axis === 'shooting');
      const scenes = [...document.querySelectorAll('[data-axis="scene"]')];
      const shots = [...document.querySelectorAll('[data-axis="shooting"]')];
      want(scenes.length === 4 && shots.length === 3, b.id + ': separate rows');
      want(b.scenario.aiming === true && [100, 50, 0].includes(b.scenario.headshot_pct), b.id + ': shooting fields');
      if (!scene || !shot) { failures.push(b.id + ': missing axis metadata'); continue; }
      if (scene.tag === 'sp200' && b.scenario.headshot_pct === 50) {
        for (const next of scenes) {
          next.click();
          want(benchCurrent().ladder.find(r => r.axis === 'shooting').tag === shot.tag, b.id + ': scene keeps shooting');
        }
        benchPick = b.id;
        renderBenchBoard();
        for (const next of document.querySelectorAll('[data-axis="shooting"]')) {
          next.click();
          want(benchCurrent().ladder.find(r => r.axis === 'scene').tag === scene.tag, b.id + ': shooting keeps scene');
        }
        benchPick = b.id;
        renderBenchBoard();
        for (const next of document.querySelectorAll('[data-bench]')) {
          next.click();
          want(benchCurrent().cell_dir === b.cell_dir, b.id + ': fight keeps both axes');
        }
      }
      benchPick = b.id;
      benchWriteUrl();
      benchPick = null;
      benchReadUrl();
      want(benchCurrent().id === b.id, b.id + ': URL round trip');
    }
  }
  const pick = all.find(b => b.scenario.headshot_pct === 50 && b.cell && b.cell.scene);
  if (pick) { benchPick = pick.id; benchWriteUrl(); }
  return { failures, selected: pick && pick.id, path: location.pathname + location.search };
})()`);
check("all 48 cells have independent controls and stable URLs", result.failures.length === 0, JSON.stringify(result.failures));
if (result.selected) {
  await send("Page.navigate", { url: BASE + result.path });
  await sleep(11000);
  const cold = await evaluate(`benchCurrent().id`);
  check("a cold link restores both axes", cold === result.selected, cold);
}
await app.finish("scene and shooting choices remain independent across all 48 cells");
