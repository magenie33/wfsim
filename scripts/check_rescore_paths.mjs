// EVERYTHING THAT CAN MOVE THE FINGERPRINT MUST TRIGGER A RESCORE.
//
// Two lists decide when the board re-scores and they are written in different
// files. `scripts/engine_fingerprint.sh` says what a score depends on; the
// `push.paths` in `.github/workflows/board.yml` say what wakes the board. A
// path in the first and not the second moves every stored score without asking
// anyone to recompute them — so the board keeps publishing numbers under a
// fingerprint that no longer matches, and the next run triggered for some
// unrelated reason does a full rescore nobody asked for.
//
// It is not symmetric and must not be. `data/**` wakes the board and is NOT in
// the code fingerprint, because a data change is asked PER ROW inside the
// scorer (`engine::data_fingerprint`) — waking on more than the hash covers is
// the safe direction and the one this repository already chose.
//
// THE CASE IT WAS WRITTEN FOR: the fingerprint hashed all of `cli` while the
// trigger named only `cli/src/bin/wfsim-board.rs`. `cli/src/main.rs` is a demo
// that shoots a training dummy and cannot reach the board, and editing it
// invalidated every row on the next run that happened for any other reason.
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

// The pathspec the fingerprint hashes: everything after `git ls-files -s --`.
const fp = readFileSync(resolve(ROOT, "scripts/engine_fingerprint.sh"), "utf8");
const spec = fp.split(NL).find((l) => l.includes("git ls-files") && l.includes("--"));
const hashed = spec
  ? spec.slice(spec.indexOf("--") + 2).split("|")[0].trim().split(/\s+/).filter(Boolean)
  : [];
check(hashed.length > 0, `the fingerprint names its paths (${hashed.join(" ") || "none found"})`);

// The board's own push trigger, read out of the workflow rather than restated.
const wf = readFileSync(resolve(ROOT, ".github/workflows/board.yml"), "utf8").split(NL);
const at = wf.findIndex((l) => l.trim() === "paths:");
const triggers = [];
for (let i = at + 1; i < wf.length && at >= 0; i += 1) {
  const m = wf[i].match(/^\s+- "?([^"]+)"?\s*$/);
  if (!m) break;
  triggers.push(m[1]);
}
check(triggers.length > 0, `the board names its triggers (${triggers.length} paths)`);

// A trigger covers a path when it IS that path or when its `/**` prefix is a
// parent of it. Both directions of "the same directory" count, since a hashed
// `engine` is covered by a trigger on `engine/**`.
const covers = (trigger, path) => {
  const t = trigger.replace(/\/\*\*$/, "").replace(/\/$/, "");
  const p = path.replace(/\/$/, "");
  return t === p || p.startsWith(`${t}/`);
};
for (const p of hashed) {
  const by = triggers.find((t) => covers(t, p));
  check(
    Boolean(by),
    `a change to \`${p}\` wakes the board`,
    "it moves the fingerprint and triggers nothing, so the rescore it buys "
      + "lands on whichever run happens next",
  );
}

console.log(NL + (bad ? `${bad} failed` : "the fingerprint and the trigger agree"));
process.exit(bad ? 1 : 0);
