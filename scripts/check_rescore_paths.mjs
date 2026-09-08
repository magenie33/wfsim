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
// A CODE CHANGE DOES NOT REACH THE BOARD ON ITS OWN. The score store keys a
// stored number by what it READ, never by which binary wrote it, so a run
// reuses every score whose data still matches — measured, 10,348 reused and 0
// scored. `audit.yml` is the only thing that finds a number the code no longer
// computes, and it finds it by MEASURING a slice rather than by asking a hash.
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

const wf = readFileSync(resolve(ROOT, ".github/workflows/board.yml"), "utf8").split(NL);
// The `on:` block, which is every line up to the next top-level key.
const from = wf.findIndex((l) => l.trim() === "on:");
const to = wf.findIndex((l, i) => i > from && /^[a-z]/.test(l));
const triggers = wf.slice(from + 1, to < 0 ? wf.length : to);
const keys = triggers.filter((l) => /^ {2}\S/.test(l)).map((l) => l.trim().replace(":", ""));

check(!keys.includes("push"), `the board is not woken by a push (${keys.join(" ") || "nothing"})`,
  "a pushed run duplicates the scheduled one and is cancelled by the next push; "
    + "the fingerprint is what carries a change to the board");
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
  "the fingerprint decides what a change reached; a path list is a second "
    + "answer to that question and the two can disagree");

// A STEP MAY NOT READ AN OUTPUT NOTHING HAS SET YET.
//
// `if: steps.x.outputs.y == '...'` above the step that defines `id: x` reads an
// empty string, so the condition is false, the step is SKIPPED, and everything
// downstream quietly gets nothing. It cost the score store its first run: the
// fetch sat above the check that gates it, never ran, and three jobs pointed
// `--scores` at a directory nobody had filled. Every run stayed green.
for (const wfName of ["board.yml", "audit.yml"]) {
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

// BOTH BACKLOGS ARE BOUNDED AND THE CLOCK IS BEHIND THEM, or the run does not
// fit the cadence that starts the next one. `--refresh` bounds the repair of
// rows the board already holds, in seconds the last run MEASURED; `--new-limit`
// bounds the builds it has no score for at all, and can only count them, since
// a row nobody has scored has no cost. `--deadline` is what turns that count
// into a promise: rows differ 79x, so 150 of them is nine minutes or fifty.
//
// Leaving the backlog unbounded is what kept the board from publishing for 35
// hours — a run had 4,570 never-scored rows to clear before `publish` assembled
// anything, and was cancelled before it got there, every run green.
const scoring = wf.filter((l) => /(slice|SLICE)="--new-limit/.test(l));
check(scoring.length > 0
  && scoring.every((l) => l.includes("--deadline")),
  `both backlogs are bounded, and the clock is behind them (${scoring.length} scoring call${scoring.length === 1 ? "" : "s"})`,
  "a run must bound its repair slice, its never-scored rows AND its wall clock: "
    + "the first is spent in measured seconds, the second can only COUNT rows "
    + "that differ 79x in cost, and the deadline is what makes the second a "
    + "promise rather than a guess");

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
const assembleStep = wf
  .slice(wf.findIndex((l) => l.includes("- name: assemble every benchmark")))
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
const pub = wf.slice(wf.findIndex((l) => /^ {2}publish:/.test(l)));
check(pub.some((l) => /group:\s*board-publish/.test(l)),
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

// THE BOARD PUBLISHES; THE AUDIT INSPECTS. `--verify` re-fights published rows
// and compares them, which is an inspector's job and costs 25 minutes of wall
// clock — on the board's critical path it delayed every publish to decide a
// priority hint, and the run behind it was cancelled while it worked. The audit
// asks the same question hourly, gating nothing.
const verifies = wf.filter((l) => l.includes("--verify"));
check(verifies.length === 0, "the board does not stop to verify itself",
  `the pipeline re-fights published rows (${verifies.map((l) => l.trim()).join(" | ")}) `
    + "instead of leaving that to audit.yml");

console.log(NL + (bad ? `${bad} failed` : "only the clock and a person start a board run"));
process.exit(bad ? 1 : 0);
