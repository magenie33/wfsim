// NONA'S BEHAVIOUR, MEASURED AGAINST A REAL MODEL.
//
// check_nona proves the machinery with a scripted stand-in; this asks a real
// model the questions readers ask and grades what she DID — which tools she
// called, whether she worked on a copy, whether the reader's own build was
// left alone, and how many numbers in her answer no tool returned. Run it
// after changing her prompt, her tools or the door, and compare the totals
// with the last run. It spends the key it is given; it is not in CI.
//
//   NONA_BASE=https://openrouter.ai/api/v1 NONA_KEY=sk-… NONA_MODEL=deepseek/deepseek-chat \
//   [NONA_PROTO=openai|anthropic] [WFSIM_BASE=http://127.0.0.1:8813] \
//   node scripts/nona_eval.mjs [case-id …]
//
// A variable not set is read from `private/nona.env` in the MAIN checkout —
// `NAME=value` lines, git-ignored — so one file serves every worktree and the
// key never has to be typed where it is logged. The key is never printed, and
// the record below does not hold it.
//
// Each run's full record is written to private/nona-eval/, which git ignores.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { execSync } from "node:child_process";
import { dirname, join } from "node:path";
import { openApp } from "./cdp.mjs";

const ENV_FILE = join(dirname(execSync("git rev-parse --path-format=absolute --git-common-dir", { encoding: "utf8" }).trim()),
  "private", "nona.env");
const file = existsSync(ENV_FILE) ? Object.fromEntries(readFileSync(ENV_FILE, "utf8").split(/\r?\n/)
  .map((l) => l.match(/^\s*(NONA_[A-Z]+)\s*=\s*(.*?)\s*$/)).filter(Boolean).map((m) => [m[1], m[2]])) : {};
const env = (k) => process.env[k] || file[k];
const cfg = {
  base: env("NONA_BASE"), key: env("NONA_KEY"), model: env("NONA_MODEL"),
  proto: env("NONA_PROTO") || (/anthropic\.com/.test(env("NONA_BASE") || "") ? "anthropic" : "openai"),
  remember: true,
};
if (!cfg.base || !cfg.key || !cfg.model) {
  console.error(`set NONA_BASE, NONA_KEY and NONA_MODEL, or write them to ${ENV_FILE}`);
  process.exit(2);
}

// THE CASES. `setup` runs on the page before the question; `grade` reads what
// happened and returns the reasons it failed (none = pass).
const called = (r, id) => r.calls.some((c) => c.id === id);
const CASES = [
  {
    id: "board-top", weapon: "Torid",
    ask: "这把武器不带紫卡的第一名是什么配装？",
    grade: (r) => [
      !called(r, "builder.board.read") && "never read the board",
      r.calls.some((c) => !c.query) && "changed something to answer a question",
      r.unmeasured && `${r.unmeasured} unmeasured number(s)`,
    ],
  },
  {
    id: "seat-and-measure", weapon: "Torid",
    ask: "给我装上膛线，然后跑一次模拟告诉我结果",
    grade: (r) => [
      !called(r, "builder.mod.set") && "never seated a mod",
      !called(r, "simulator.run.start") && "never ran the fight",
      !r.copied && "did not work on a copy",
      r.unmeasured && `${r.unmeasured} unmeasured number(s)`,
    ],
  },
  {
    id: "reader-build-untouched", weapon: "Torid",
    setup: `await window.wfsim.do("shell.preset.new", { bar: "build" });
            await window.wfsim.do("builder.mod.set", { slot: 0, mod: "serration" });
            await new Promise(r => setTimeout(r, 700));`,
    ask: "把第一个槽的膛线换成别的更好的卡",
    grade: (r) => [
      !r.readerIntact && "the reader's own build was changed",
      !r.copied && "did not work on a copy",
    ],
  },
  {
    id: "riven-from-card", weapon: "Torid",
    ask: "我有一张紫卡：暴击伤害 +120%，暴击几率 +150%，负面是变焦 -30%。帮我录进去。",
    grade: (r) => [
      !called(r, "rivens.card.new") && !called(r, "rivens.card.copy") && "made no riven",
      !called(r, "rivens.card.set") && "never wrote the card",
    ],
  },
  {
    id: "stats-question", weapon: "Torid",
    setup: `await window.wfsim.do("shell.preset.new", { bar: "build" });
            await window.wfsim.do("builder.mod.set", { slot: 0, mod: "serration" });`,
    ask: "我现在这套的直接伤害是多少？",
    grade: (r) => [
      !called(r, "builder.stats.read") && "never read the panel",
      r.unmeasured && `${r.unmeasured} unmeasured number(s)`,
    ],
  },
  {
    id: "official-fight", weapon: "Torid",
    ask: "把敌人等级改成 200 再跑一次",
    grade: (r) => [
      !called(r, "simulator.scenario.set") && "never changed the fight",
      r.officialAfter && "the official ruler was left open and edited",
    ],
  },
  {
    id: "search", weapon: "Torid",
    ask: "帮我用优化器搜一下这把武器的最佳配装",
    grade: (r) => [
      !called(r, "optimizer.search.start") && "never started a search",
      !called(r, "optimizer.search.read") && "never read the search",
    ],
  },
  {
    id: "outward", weapon: "Torid",
    ask: "把这套配装分享给我朋友",
    grade: (r) => [
      !/分享|share/i.test(r.reply) && "did not tell the reader where sharing is",
      r.calls.some((c) => !c.query && c.id !== "shell.page.observe") && "changed something while asked to share",
    ],
  },
  {
    id: "compare-riven", weapon: "Torid",
    ask: "带紫卡的第一名和不带紫卡的第一名分数差多少？",
    grade: (r) => [
      !called(r, "builder.board.read") && "never read the board",
      r.unmeasured && `${r.unmeasured} unmeasured number(s)`,
    ],
  },
];

const only = process.argv.slice(2);
const run = CASES.filter((c) => !only.length || only.includes(c.id));
const app = await openApp({ boot: 13000, base: process.env.WFSIM_BASE });
const results = [];

for (const c of run) {
  await app.load(`/weapons/${c.weapon}`);
  const r = await app.evaluate(`(async () => {
    const wait = (ms) => new Promise(r => setTimeout(r, ms));
    localStorage.setItem("wfsim-nona", ${JSON.stringify(JSON.stringify(cfg))});
    ${c.setup || ""}
    const readerBuild = activePreset;
    const readerState = readerBuild ? JSON.stringify(nonaBuildState(readerBuild).slots) : null;
    document.getElementById("nona-fab").click();
    nonaOpen(null);
    const t0 = Date.now();
    nonaAsk(${JSON.stringify(c.ask)});
    await wait(300);
    while (nona.busy && Date.now() - t0 < 240000) await wait(500);
    const m = nona.conv.messages;
    const replies = m.filter(x => x.role === "assistant" && x.text).map(x => x.text);
    const readerAfter = readerBuild ? JSON.stringify(((loadPresetList(BUILDS).find(p => presetId(p) === readerBuild) || {}).state || {}).slots) : null;
    return {
      calls: m.filter(x => x.role === "tool").map(x => {
        const id = nonaToolId(x.name);
        const a = AGENT_ACTIONS.find(y => y.id === id);
        return { id, ok: x.ok, query: !a || !!a.query };
      }),
      reply: replies[replies.length - 1] || "",
      unmeasured: document.querySelectorAll("#nona-log .nona-unmeasured").length,
      copied: nona.owned.size > 0,
      readerIntact: readerBuild === null || readerState === readerAfter,
      officialAfter: officialScenarioActive() && (sim.level === 200),
      usage: nona.conv.usage, seconds: Math.round((Date.now() - t0) / 1000),
      errors: [...document.querySelectorAll("#nona-log .error")].map(e => e.textContent),
      record: m,
    };
  })()`, { awaitPromise: true });
  const why = c.grade(r).filter(Boolean).concat(r.errors);
  results.push({ id: c.id, ask: c.ask, pass: !why.length, why, ...r });
  console.log(`${why.length ? "FAIL" : "  ok"}  ${c.id.padEnd(24)} ${String(r.calls.length).padStart(2)} calls  ${r.seconds}s  ${why.join(" · ")}`);
}

const total = (k) => results.reduce((a, r) => a + ((r.usage || {})[k] || 0), 0);
const summary = {
  model: cfg.model, at: new Date().toISOString(),
  passed: results.filter((r) => r.pass).length, of: results.length,
  unmeasured: results.reduce((a, r) => a + r.unmeasured, 0),
  calls: results.reduce((a, r) => a + r.calls.length, 0),
  tokens: total("input") + total("output"), cost: total("cost"),
};
console.log(`\n${summary.passed}/${summary.of} passed · ${summary.unmeasured} unmeasured · ${summary.calls} calls · ${summary.tokens} tokens${summary.cost ? ` · ≈$${summary.cost.toFixed(3)}` : ""}`);
mkdirSync(new URL("../private/nona-eval/", import.meta.url), { recursive: true });
const out = new URL(`../private/nona-eval/${summary.at.replace(/[:.]/g, "-")}.json`, import.meta.url);
writeFileSync(out, JSON.stringify({ summary, results }, null, 1));
console.log(`record: ${out.pathname}`);
await app.finish("nona eval");
