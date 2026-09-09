// NOTHING BUT THE CLOCK AND A PERSON MAY START A BOARD RUN.
//
// A push does not wake the board. Under convergence a pushed run does exactly
// what the next scheduled one does — score what is NEW, repair a bounded slice,
// publish — so it duplicates a run that is coming anyway while competing for
// the same forty slots. Measured over 95 runs: 21 of 22 pushed runs were
// cancelled by the next push, and the board published nothing at all.
//
// RE-ADDING THE TRIGGER FAILS SILENTLY, which is why it is a check and not a
// comment: every run still goes green, and what you lose is the board moving.
// So does pinning the repair slice's offset, the other way the same wheel spins
// without turning.
//
// A CODE CHANGE DOES NOT REACH THE BOARD ON ITS OWN. A stored score is reused
// because it EXISTS — no clock, no fingerprint, no binary in the key — so a run
// that changed a formula reuses every row it already has. What retires a fact
// is a person deleting its row.
import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const NL = String.fromCharCode(10);
let bad = 0;
const check = (ok, name, detail) => {
  console.log(`  ${ok ? "ok  " : "FAIL"}  ${name}${ok || !detail ? "" : `  — ${detail}`}`);
  if (!ok) bad += 1;
};

const wf = readFileSync(resolve(ROOT, ".github/workflows/scores.yml"), "utf8").split(NL);
// The `on:` block, which is every line up to the next top-level key.
const from = wf.findIndex((l) => l.trim() === "on:");
const to = wf.findIndex((l, i) => i > from && /^[a-z]/.test(l));
const triggers = wf.slice(from + 1, to < 0 ? wf.length : to);
const keys = triggers.filter((l) => /^ {2}\S/.test(l)).map((l) => l.trim().replace(":", ""));

check(!keys.includes("push"), `the board is not woken by a push (${keys.join(" ") || "nothing"})`,
  "a pushed run duplicates the one a person is about to start and is cancelled "
    + "by the next push");
// THE CLOCK, OR A DECLARED HOLD — never neither, and never a hold by accident.
// A board nothing wakes is the state that goes green in every check and is
// discovered by a reader asking why the numbers stopped, so the hold has to be
// a line somebody wrote on purpose. Deleting the line without restoring the
// cron fails here.
const HELD = "AUTOMATIC-UPDATES: HELD";
const held = triggers.some((l) => l.includes(HELD));
check(keys.includes("schedule") || held, "the clock wakes it, or the hold is declared",
  "with no schedule and no push, nothing automatic updates the board at all — "
    + `say so with a ${HELD} line in the on: block if that is deliberate`);
if (held) {
  console.log(`  --    the clock is HELD, so only the button updates the board`);
  check(!keys.includes("schedule"), "...and a held clock is not also running",
    "the hold line says the board is manual; a schedule beside it says it is not");
}
check(!triggers.some((l) => l.trim() === "paths:" || l.trim() === "paths-ignore:"),
  "no path list decides whether the board runs",
  "what a run does is decided by what the database holds; a path list is a "
    + "second answer to that question and the two can disagree");

// A STEP MAY NOT READ AN OUTPUT NOTHING HAS SET YET.
//
// `if: steps.x.outputs.y == '...'` above the step that defines `id: x` reads an
// empty string, so the condition is false, the step is SKIPPED, and everything
// downstream quietly gets nothing. It cost the score store its first run: the
// fetch sat above the check that gates it, never ran, and three jobs pointed
// `--scores` at a directory nobody had filled. Every run stayed green.
for (const wfName of ["scores.yml", "queue.yml"]) {
  const lines = readFileSync(resolve(ROOT, ".github/workflows", wfName), "utf8").split(NL);
  const definedAt = new Map();
  lines.forEach((l, i) => {
    const m = l.match(/^\s+id:\s*([A-Za-z0-9_-]+)\s*$/);
    if (m && !definedAt.has(m[1])) definedAt.set(m[1], i);
  });
  const early = [];
  lines.forEach((l, i) => {
    // A STEP CONDITION AND NOTHING ELSE. A job's own `outputs:` block names
    // steps below it on purpose — those are read when the job ENDS — and an
    // `if:` is read the moment the runner reaches it.
    if (!l.trim().startsWith("if:")) return;
    for (const m of l.matchAll(/steps\.([A-Za-z0-9_-]+)\.outputs/g)) {
      const at = definedAt.get(m[1]);
      if (at === undefined || at > i) early.push(`${wfName}:${i + 1} reads steps.${m[1]}`);
    }
  });
  check(early.length === 0, `${wfName} gates no step on an output set below it`,
    early.join(" | "));
}

// THE CLOCK IS UNCONDITIONAL, AND THE COUNT ONLY EXTENDS IT.
//
// `--deadline` is the bound that always holds: rows differ 79x in cost, so a
// COUNT of them is nine minutes or fifty depending on which builds arrived, and
// only the clock turns a bound into a promise. `--new-limit` bounds the backlog
// of never-measured rows on top of it, and is left off a run that NAMED rows —
// a person who asked for a weapon should get the weapon, not the first 3,000
// rows of it.
//
// ASSERTED AS AN ORDER, not as a line: the deadline is assigned first and the
// count PREPENDS itself to the same variable, so a line carrying `--new-limit`
// carries the clock by reference. Checking each line for both strings would
// pass a version that dropped the clock on the branch that sets the count.
//
// Leaving the backlog unbounded is what kept the board from publishing for 35
// hours — a run had 4,570 never-scored rows to clear before `publish` assembled
// anything, and was cancelled before it got there, every run green.
const assigns = wf
  .map((l, i) => [l, i])
  .filter(([l]) => /^\s*(slice|SLICE)="/.test(l));
const firstOf = new Map();
for (const [l, i] of assigns) {
  const name = l.trim().startsWith("SLICE") ? "SLICE" : "slice";
  if (!firstOf.has(name)) firstOf.set(name, l);
}
// ASSERTED ON THE PROPERTY AND NOT ON THE SHAPE: what has to hold is that the
// CLOCK is in every bound, whether one assignment carries both or a later one
// extends an earlier that has it. Insisting on the two-step form outlawed the
// simpler single assignment that satisfies it outright.
const carriesClock = assigns.every(([l]) =>
  l.includes("--deadline") || /\$\{?(slice|SLICE)\}?/.test(l));
const counted = assigns.some(([l]) => l.includes("--new-limit"));
check(firstOf.size > 0 && carriesClock && counted,
  `the clock bounds every scoring call, and the count rides with it (${firstOf.size} of them)`,
  "a bound naming --new-limit without the clock is a run bounded by a number "
    + "that means nothing: rows differ 79x in cost, so a count of 150 is nine "
    + "minutes or fifty");

// THE MATRIX AND THE DENOMINATOR ARE ONE NUMBER. A shard is told `i/N` while
// the matrix is a list, so a list of 32 against an N of 128 tells 32 jobs they
// are one of 128 and three quarters of the board is never scored — nothing
// fails, the rows just keep their old numbers. The denominator must therefore
// name the same output the matrix is built from.
const denom = wf.filter((l) => l.includes("--shard ") && !l.trim().startsWith("#"));
check(denom.length > 0 && denom.every((l) => l.includes("outputs.shard_count")),
  "the shard denominator comes from the matrix's own count",
  `a shard is told a count the matrix does not set (${denom.map((l) => l.trim()).join(" | ") || "no --shard at all"})`);

// THE ASSEMBLY HAS ONE SOURCE, and a publisher with more than one needs a rule
// for which of them wins. Every defect this pipeline has produced has lived in
// that rule: a merge decided by filename order, an artifact whose absence
// skipped the assembly, a prior board republished beside rows half its size.
//
// Read over the whole step, because the flags sit on their own lines.
const pubWf = readFileSync(resolve(ROOT, ".github/workflows/publish.yml"), "utf8").split(NL);
const assembleStep = pubWf
  .slice(pubWf.findIndex((l) => l.includes("- name: assemble every benchmark")))
  .slice(0, 80)
  .filter((l) => !l.trim().startsWith("#"))
  .join(NL);
check(assembleStep.includes("--project"), "the assembly does not fight a row",
  "the pass that publishes must not also be a pass that scores");
check(assembleStep.includes("--facts-in"), "…and it reads the generation's facts",
  "a publisher with no facts publishes whatever else it was handed");
for (const flag of ["--scores", "--scored-here", "--emit-scores", "--reuse"]) {
  check(!assembleStep.includes(flag), `…and nothing else (${flag})`,
    "a second source needs a rule about which one wins, and that rule is where "
      + "every defect in this pipeline has lived");
}

// …AND THE FULL BUTTON FORCES ROWS RATHER THAN WITHHOLDING A PRIOR BOARD.
// There is no prior to withhold any more — the facts of the open generation are
// the only source — so "full" can only mean every row forced, and the facts it
// drops still answer what each row cost, which is what the split packs by.
//
// READ OVER THE CODE LINES ALONE. A prose block between the condition and the
// assignment is not distance, and counting it as distance is how the check
// would pass on the shape it exists to refuse.
const code = wf.filter((l) => l.trim() && !l.trim().startsWith("#"));
const gated = code
  .map((l, i) => [l, i])
  .filter(([l]) => l.includes('prior="--reuse'))
  .filter(([, i]) => code.slice(Math.max(0, i - 3), i + 1)
    .some((l) => /(inputs|outputs)\.full/.test(l)));
check(gated.length === 0, "a full rescore is every row forced, not a missing prior",
  `the prior board is withheld on \`full\` (${gated.map(([l]) => l.trim()).join(" | ")}) — `
    + "which forces nothing, since reuse is decided per row and the store answers "
    + "first, and loses the costs the split packs by and the leaders it screens against");

// AND IT REACHES A BOARD NOBODY IS OVERWRITING. Two runs assembling at once is
// last-writer-wins over a whole run's KNOWLEDGE, not over one file: the loser is
// whichever read the score store first, and what it publishes is the store as it
// was then — the board and the store both rolled back. Measured: a repaired row
// landed at 18:02 and a run that had read the store at 18:01 published over it
// at 18:07, twice, so the button looked like it did nothing.
//
// So the assembly is serialised and reads the store LAST. The scoring may
// overlap freely; it only ever adds.
const pub = pubWf.slice(pubWf.findIndex((l) => /^ {2}publish:/.test(l)));
// A FILE A JOB IS TOLD TO READ MUST REACH IT. `--queue-in queue.ndjson` in a job
// that downloads no `queue` artifact reads nothing, and reading nothing is
// indistinguishable from an empty queue: every shard reports "0 scored here",
// the run goes green and the board does not move. Measured — it is how the
// first run of the queue went.
const scoreJob = wf
  .slice(wf.findIndex((l) => /^  score:$/.test(l)))
  .join(NL);
for (const [flag, artifact] of [["--queue-in", "queue"], ["--facts-in", "facts"],
                                ["< library.json", "library"]]) {
  check(!scoreJob.includes(flag) || new RegExp(`name: ${artifact}\s*$`, "m").test(scoreJob),
    `a shard given ${flag} downloads the ${artifact} it names`,
    "a file the scorer cannot read is an empty one, and an empty one is silent");
}

// THE CLOCK IS ON THE PUBLISH, and it is what makes the board move at all now
// that scoring is a button. Without it nothing writes `site/board` ever again,
// which goes green everywhere and is discovered by a reader asking why the
// numbers stopped.
check(pubWf.some((l) => /^\s+- cron:/.test(l)),
  "the publish runs on a clock",
  "nothing else writes site/board, so with no schedule the board never moves");
// …AND IT PUBLISHES WHAT THE TABLE HOLDS, waiting for no scoring run. `needs:`
// would make the board's freshness a property of whether somebody pressed a
// button, which is the coupling this file was split to remove.
check(!pub.some((l) => /^\s+needs:/.test(l)),
  "…and waits for no scoring run",
  "a publish gated on a scorer publishes only when somebody scored");
// ASKED OF THE WHOLE FILE, because a workflow-level `concurrency:` is what
// serialises across RUNS; one declared inside the job only serialises the job.
check(pubWf.some((l) => /group:\s*board-publish/.test(l)),
  "one assembly at a time, across every run",
  "two publishes overlap, and the later one writes the store it read at its own start");
check(pub.some((l) => l.includes("fetch_facts.sh")),
  "…and it reads the generation itself, at its own start",
  "an assembly handed a copy of the facts read earlier in the run publishes what "
    + "was true then, and a run's own shards are still shipping while it waits");
check(!pub.some((l, i) => /name:\s*store/.test(l)
  && pub.slice(Math.max(0, i - 4), i).some((p) => p.includes("download-artifact"))),
  "…rather than the snapshot the scorers were handed",
  "the artifact is as old as the run; the assembly is the last thing it does");

console.log(NL + (bad ? `${bad} failed` : "only the clock and a person start a board run"));
process.exit(bad ? 1 : 0);
