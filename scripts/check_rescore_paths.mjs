// THE BOARD WAKES ON EVERYTHING IT DOES NOT ITSELF WRITE.
//
// Deciding when to re-score had two answers — a trigger list and a fingerprint
// — and they could disagree. A path in the fingerprint and not the trigger
// moves every stored score without waking anyone, so the full rescore it bought
// lands on whichever run happens next, for a reason that run has nothing to do
// with. `cli` was such a path: `cli/src/main.rs` is a demo that shoots a
// training dummy, and editing it invalidated the whole board in silence.
//
// AN EXCLUSION LIST INVERTS THAT FAILURE, which is the only reason to prefer
// it: forgetting an entry costs one wasted run, where forgetting an entry in an
// inclusion list costs a board that quietly stops moving. So this asserts the
// SHAPE — the board must name what to skip, never what to catch — and then that
// the skipped set is the board's own output and nothing else.
//
// It is affordable because a run with nothing to do is one job: the
// `is there anything to score` step asks the scorer by walking the boards, and
// the fan-out does not start when the answer is none.
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

// The list under a key, as workflow yaml writes one: `- "value"` lines until
// something that is not one.
const listAt = (key) => {
  const at = wf.findIndex((l) => l.trim() === key);
  if (at < 0) return null;
  const out = [];
  for (let i = at + 1; i < wf.length; i += 1) {
    const m = wf[i].match(/^\s+- "?([^"]+)"?\s*$/);
    if (!m) break;
    out.push(m[1]);
  }
  return out;
};

const skipped = listAt("paths-ignore:");
check(listAt("paths:") === null, "the board names what to SKIP, not what to catch",
  "an inclusion list cannot wake the board for a path nobody remembered to add");
check(Array.isArray(skipped) && skipped.length > 0,
  `the board names what it skips (${(skipped || []).join(" ") || "nothing"})`);

// WHAT THE RUN GENERATES, read from the step that commits it rather than
// restated — the two lists are the same claim about the same files.
const gen = wf.find((l) => l.trim().startsWith("GENERATED="));
const generated = gen
  ? gen.slice(gen.indexOf("=") + 1).replace(/"/g, "").trim().split(/\s+/).filter(Boolean)
  : [];
check(generated.length > 0, `publish names what it writes (${generated.join(" ") || "none"})`);

// A generated path covers a skipped one when it IS that path or a parent of it.
const covers = (g, p) => {
  const a = g.replace(/\/\*\*$/, "").replace(/\/$/, "");
  const b = p.replace(/\/\*\*$/, "").replace(/\/$/, "");
  return a === b || b.startsWith(`${a}/`);
};
for (const p of skipped || []) {
  check(
    generated.some((g) => covers(g, p)),
    `\`${p}\` is skipped because this workflow writes it`,
    "nothing else may be skipped: a path the board does not generate is a path "
      + "whose change it has to look at, and skipping it is how a board stops moving",
  );
}

console.log(NL + (bad ? `${bad} failed` : "the board skips only what it writes"));
process.exit(bad ? 1 : 0);
