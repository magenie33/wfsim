// NONA'S BEHAVIOUR, MEASURED AGAINST A REAL MODEL.
//
// check_nona proves the machinery with a scripted stand-in; this asks a real
// model the questions readers ask and grades what she DID — which tools she
// called, whether she worked on a copy, whether the reader's own build was
// left alone, how many numbers in her answer she was never sent — and what
// each case cost. Run it after changing her prompt, her tools, her context or
// the door, with `--against=` an earlier record, which compares case by case.
// It spends the key it is given; it is not in CI.
//
//   NONA_BASE=https://openrouter.ai/api/v1 NONA_KEY=sk-… NONA_MODEL=deepseek/deepseek-chat \
//   [NONA_PROTO=openai|anthropic] [WFSIM_BASE=http://127.0.0.1:8813] \
//   node scripts/nona_eval.mjs [case-id …] [--against=private/nona-eval/<earlier>.json]
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
    // THE READER'S NUMBERS GO IN AS THEY GAVE THEM. A card holding other
    // values replaced what they said; asking first is the other right answer.
    grade: (r) => {
      const card = r.rivens.find((x) => r.madeRivens.includes(x.id));
      const nums = card ? card.lines.map((l) => Math.abs(parseFloat(l))).sort((a, b) => a - b) : null;
      return [
        !card && !/[?？]/.test(r.reply) && "neither wrote the card nor asked",
        card && JSON.stringify(nums) !== "[30,120,150]" && `wrote ${card.lines.join(" / ")} for +120% / +150% / -30%`,
      ];
    },
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

/// WHAT A CASE COST, from the provider's own usage on each reply: how many
/// requests, their input (and the largest one), how much of it the provider's
/// cache served, and the output.
function billOf(record) {
  const us = record.filter((m) => m.role === "assistant" && m.usage).map((m) => m.usage);
  const input = us.reduce((a, u) => a + u.input, 0), cached = us.reduce((a, u) => a + u.cached, 0);
  return { requests: us.length, input, cached, output: us.reduce((a, u) => a + u.output, 0),
    max_input: Math.max(0, ...us.map((u) => u.input)), cached_pct: Math.round((100 * cached) / (input || 1)) };
}
const k = (n) => `${(n / 1000).toFixed(1)}k`;

const AGAINST = (process.argv.find((a) => a.startsWith("--against=")) || "").slice(10);
const only = process.argv.slice(2).filter((a) => !a.startsWith("--"));
const run = CASES.filter((c) => !only.length || only.includes(c.id));
const app = await openApp({ boot: 13000, base: process.env.WFSIM_BASE });
const results = [];

for (const c of run) {
  await app.load(`/weapons/${c.weapon}`);
  const r = await app.evaluate(`(async () => {
    const wait = (ms) => new Promise(r => setTimeout(r, ms));
    localStorage.setItem("wfsim-nona", ${JSON.stringify(JSON.stringify(cfg))});
    ${c.setup || ""}
    // READ THROUGH THE DOOR AND THE PAGE ONLY, as a reader's tools would: the
    // build by its query, the conversation from her own store.
    const door = window.wfsim;
    const readerBuild = door.observe().open.build;
    const read = async (id) => JSON.stringify(((await door.do("shell.preset.read", { bar: "build", preset: id })) || {}).mods || null);
    const readerState = readerBuild ? await read(readerBuild) : null;
    document.getElementById("nona-fab").click();
    document.getElementById("nona-new").click();
    const t0 = Date.now();
    document.getElementById("nona-input").value = ${JSON.stringify(c.ask)};
    document.getElementById("nona-send").click();
    await wait(300);
    const busy = () => document.getElementById("nona-send").classList.contains("busy");
    while (busy() && Date.now() - t0 < 240000) await wait(500);
    const all = await new Promise((ok) => {
      const req = indexedDB.open("wfsim-nona", 1);
      req.onsuccess = () => { const g = req.result.transaction("conversations").objectStore("conversations").getAll(); g.onsuccess = () => ok(g.result); };
      req.onerror = () => ok([]);
    });
    const conv = all.sort((a, b) => b.updated_at - a.updated_at)[0] || { messages: [], made: [], usage: null };
    const m = conv.messages;
    const replies = m.filter(x => x.role === "assistant" && x.text).map(x => x.text);
    const readerAfter = readerBuild ? await read(readerBuild) : null;
    const o = door.observe();
    return {
      calls: m.filter(x => x.role === "tool").map(x => {
        const id = x.name.replace(/_/g, ".");
        const a = door.actions.find(y => y.id === id);
        return { id, ok: x.ok, query: !a || !!a.query };
      }),
      reply: replies[replies.length - 1] || "",
      unmeasured: document.querySelectorAll("#nona-log .nona-unmeasured").length,
      copied: (conv.made || []).length > 0,
      madeRivens: (conv.made || []).filter(x => x.startsWith("riven:")).map(x => x.slice(6)),
      rivens: ((await door.do("rivens.cards.list", {})) || {}).rivens || [],
      readerIntact: readerBuild === null || readerState === readerAfter,
      officialAfter: !!o.official_scenario && !!o.scenario && o.scenario.level === 200,
      usage: conv.usage, seconds: Math.round((Date.now() - t0) / 1000),
      errors: [...document.querySelectorAll("#nona-log .error")].map(e => e.textContent),
      record: m,
    };
  })()`, { awaitPromise: true });
  const why = c.grade(r).filter(Boolean).concat(r.errors);
  const bill = billOf(r.record);
  results.push({ id: c.id, ask: c.ask, pass: !why.length, why, bill, ...r });
  console.log(`${why.length ? "FAIL" : "  ok"}  ${c.id.padEnd(24)} ${String(r.calls.length).padStart(2)} calls  ${String(bill.requests).padStart(2)} req  `
    + `in ${k(bill.input)} (max ${k(bill.max_input)}, cached ${bill.cached_pct}%)  out ${k(bill.output)}  ${r.seconds}s  ${why.join(" · ")}`);
}

const sum = (key) => results.reduce((a, r) => a + r.bill[key], 0);
const summary = {
  model: cfg.model, at: new Date().toISOString(),
  passed: results.filter((r) => r.pass).length, of: results.length,
  unmeasured: results.reduce((a, r) => a + r.unmeasured, 0),
  calls: results.reduce((a, r) => a + r.calls.length, 0),
  requests: sum("requests"), input: sum("input"), cached: sum("cached"), output: sum("output"),
  tokens: sum("input") + sum("output"), cost: results.reduce((a, r) => a + ((r.usage || {}).cost || 0), 0),
};
summary.cached_pct = Math.round((100 * summary.cached) / (summary.input || 1));
console.log(`\n${summary.passed}/${summary.of} passed · ${summary.unmeasured} unmeasured · ${summary.calls} calls · `
  + `${summary.requests} requests · input ${k(summary.input)} (avg ${k(summary.input / (summary.requests || 1))}/request, `
  + `cached ${summary.cached_pct}%) · output ${k(summary.output)}${summary.cost ? ` · ≈$${summary.cost.toFixed(3)}` : ""}`);

// A COMPARISON IS PAIRED: the same case against the same case of an earlier
// record, never one run's total against another's.
if (AGAINST) {
  const before = JSON.parse(readFileSync(AGAINST, "utf8"));
  console.log(`\nagainst ${before.summary.model} · ${before.summary.at}`);
  for (const r of results) {
    const b = before.results.find((x) => x.id === r.id);
    if (!b) { console.log(`  ${r.id.padEnd(24)} (not in the earlier record)`); continue; }
    const bb = b.bill || billOf(b.record);
    const pct = bb.input ? Math.round((100 * (r.bill.input - bb.input)) / bb.input) : 0;
    console.log(`  ${r.id.padEnd(24)} ${b.pass ? "ok" : "FAIL"} → ${r.pass ? "ok" : "FAIL"}  input ${k(bb.input)} → ${k(r.bill.input)} (${pct > 0 ? "+" : ""}${pct}%)`
      + `  requests ${bb.requests} → ${r.bill.requests}  unmeasured ${b.unmeasured} → ${r.unmeasured}`);
  }
}
mkdirSync(new URL("../private/nona-eval/", import.meta.url), { recursive: true });
const out = new URL(`../private/nona-eval/${summary.at.replace(/[:.]/g, "-")}.json`, import.meta.url);
writeFileSync(out, JSON.stringify({ summary, results }, null, 1));
console.log(`record: ${out.pathname}`);
await app.finish("nona eval");
