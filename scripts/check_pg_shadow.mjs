// EVERY CHECK THAT RUNS THE WORKER ON node:sqlite, RUN AGAIN ON POSTGRES TOO
// (scripts/pg/shadow.mjs): each statement the worker issues is repeated on
// PGlite, in its Postgres text where it states one, and compared — rows, rows
// changed, failure. A check passes only if its own verdict holds and Postgres
// answered every statement as SQLite did.
//   npm install --prefix scripts/pg && node scripts/check_pg_shadow.mjs [check …]
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const CHECKS = process.argv.slice(2).length ? process.argv.slice(2) : [
  "check_accounts", "check_appraise", "check_board_verify", "check_contribution", "check_fair", "check_ledger",
  "check_qq", "check_renew", "check_riven_gain", "check_tasks", "check_usage_events",
];
const shadow = fileURLToPath(new URL("./pg/shadow.mjs", import.meta.url));
let failed = 0;
for (const c of CHECKS) {
  const r = spawnSync(process.execPath, ["--no-warnings", "--import", `file:///${shadow.replace(/\\/g, "/")}`, fileURLToPath(new URL(`./${c}.mjs`, import.meta.url))],
    { encoding: "utf8", maxBuffer: 256 << 20 });
  const out = `${r.stdout || ""}${r.stderr || ""}`;
  const verdict = (out.match(/shadow: .*/) || ["shadow: did not report"])[0];
  const ok = r.status === 0;
  console.log(`  ${ok ? "ok  " : "FAIL"}  ${c} — ${verdict}`);
  if (!ok) {
    failed++;
    const tail = out.slice(out.indexOf("shadow:") >= 0 ? out.indexOf("shadow:") : Math.max(0, out.length - 3000));
    console.log(tail.split("\n").slice(1, 60).map((l) => `      ${l}`).join("\n"));
    const own = out.split("\n").filter((l) => /^\s*FAIL\b/.test(l)).slice(0, 8);
    if (own.length) console.log(`      the check's own failures:\n${own.map((l) => `        ${l.trim()}`).join("\n")}`);
  }
}
console.log(failed ? `\n${failed} check(s) differ on Postgres` : "\nevery check answers alike on SQLite and on Postgres");
process.exitCode = failed ? 1 : 0;
