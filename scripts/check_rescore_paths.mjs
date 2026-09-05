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
// Ordinary work still reaches the board. The FINGERPRINT carries it: a change
// marks the rows it reached as unverified, and the slice repairs them at the
// rate `REFRESH_MINUTES` sets.
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
check(keys.includes("schedule"), "the clock wakes it",
  "with no schedule and no push, nothing automatic updates the board at all");
check(!triggers.some((l) => l.trim() === "paths:" || l.trim() === "paths-ignore:"),
  "no path list decides whether the board runs",
  "the fingerprint decides what a change reached; a path list is a second "
    + "answer to that question and the two can disagree");

// THE BOARD PUBLISHES; THE AUDIT INSPECTS. `--verify` re-fights published rows
// and compares them, which is an inspector's job and costs 25 minutes of wall
// clock — on the board's critical path it delayed every publish to decide a
// priority hint, and the run behind it was cancelled while it worked. The audit
// asks the same question hourly, gating nothing.
const verifies = wf.filter((l) => l.includes("--verify"));
check(verifies.length === 0, "the board does not stop to verify itself",
  `the pipeline re-fights published rows (${verifies.map((l) => l.trim()).join(" | ")}) `
    + "instead of leaving that to audit.yml");

// THE REPAIR SLICE ADVANCES BY WHAT IT TOOK, and the workflow may not say
// otherwise. `--refresh-from` pins the offset, and pinning it to the run number
// is the shape that stalled the board for 35 hours: a slice is hundreds of rows
// wide and the run number steps by one, so consecutive runs re-fought the same
// rows and a board of 7,659 would have needed 7,659 runs to cross itself once.
// The cursor in `data/board_state.yaml` is the only source.
const pinned = wf.filter((l) => l.includes("--refresh-from"));
check(pinned.length === 0, "the repair slice starts where the last one stopped",
  `the workflow pins the offset (${pinned.map((l) => l.trim()).join(" | ")}) instead of `
    + "letting it come from the stored cursor");

console.log(NL + (bad ? `${bad} failed` : "only the clock and a person start a board run"));
process.exit(bad ? 1 : 0);
