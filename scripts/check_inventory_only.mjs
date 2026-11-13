// Inventory publication can run without intake or later queue mutations.
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { runInNewContext } from "node:vm";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const SOURCE = resolve(process.env.WFSIM_INVENTORY_ONLY_SOURCE || ROOT);
const workflow = readFileSync(join(process.env.WFSIM_WORKFLOW_DIR || join(SOURCE, ".github/workflows"), "queue.yml"), "utf8");
const temporary = mkdtempSync(join(tmpdir(), "wfsim-inventory-only-"));
let failed = 0;
const check = (ok, name, detail = "") => {
  console.log(`  ${ok ? "ok  " : "FAIL"}  ${name}${ok || !detail ? "" : ` — ${detail}`}`);
  if (!ok) failed++;
};

function phaseTool() {
  const fs = require("node:fs"), path = require("node:path"), { spawnSync } = require("node:child_process");
  const own = path.basename(process.argv[1]);
  const command = own === "phase-tool.cjs" ? process.argv[2] : own;
  const args = process.argv.slice(own === "phase-tool.cjs" ? 3 : 2);
  const note = phase => fs.appendFileSync(process.env.WFSIM_TEST_LOG, JSON.stringify({ phase, args }) + "\n");
  const put = (name, text = "") => { fs.mkdirSync(path.dirname(path.resolve(name)), { recursive: true }); fs.writeFileSync(name, text); };
  if (command === "python3" && args[0].endsWith("inventory_phase.py")) {
    const at = args.indexOf("--");
    process.exit(spawnSync(args[at + 1], args.slice(at + 2), { stdio: "inherit", timeout: 3000 }).status ?? 1);
  }
  if (command === "node") {
    const [script, ...files] = args;
    if (script.endsWith("capture_compute_inventory.mjs")) {
      note("capture"); const [library, facts, queue, prefix] = files;
      put(library, "[]\n"); put(facts); put(queue);
      for (const suffix of ["", ".builds", ".tasks", ".json"]) put(prefix + suffix);
    } else if (script.endsWith("bind_inventory_builds.mjs")) note("bind");
    else if (script.endsWith("compute_inventory.mjs")) { note("publish"); if (process.env.WFSIM_TEST_FAIL_PUBLISH) process.exit(17); }
    else throw new Error(`unexpected node phase ${script}`);
  } else if (command === "board") {
    if (args[0] === "--rulers") { note("roster"); process.stdout.write("ruler_one\r\nruler_two\r\n"); }
    else {
      note("enumerate");
      for (const flag of ["--queue-missing", "--queue-orphans"]) for (const id of ["ruler_one", "ruler_two"])
        put(args[args.indexOf(flag) + 1].replace("{id}", id), flag === "--queue-missing" ? JSON.stringify({ build_id: id, ruler: id, mode: "base" }) + "\n" : "");
      put(args[args.indexOf("--inventory-builds") + 1]);
    }
  } else if (command === "curl") { note("popularity"); process.stdout.write("{}\n"); }
  else if (command === "python3" && args[0].endsWith("scene_rows.py")) { note("pace"); process.stdout.write(fs.readFileSync(args[3])); }
  else if (command === "python3" && args[0].endsWith("popular_order.py")) { note("order"); process.stdout.write(fs.readFileSync(0)); }
  else if (command === "ship_queue") note(args[0] === "--admit" ? "admission" : "dispatch");
  else if (["purge_queue", "purge_orphans", "fetch_queue"].includes(command)) note(command);
  else throw new Error(`unexpected phase ${command}`);
}

function reconcile(mode, failPublication = false, workers) {
  const work = mkdtempSync(join(temporary, "case-")), bin = join(work, "bin"), scripts = join(work, "scripts");
  mkdirSync(bin); mkdirSync(scripts); mkdirSync(join(work, "tmp"));
  const tool = `#!${process.execPath}\n(${phaseTool.toString()})();\n`;
  for (const name of ["node", "python3", "board", "curl"]) writeFileSync(join(bin, name), tool, { mode: 0o755 });
  writeFileSync(join(work, "phase-tool.cjs"), tool);
  for (const name of ["ship_queue", "purge_queue", "purge_orphans", "fetch_queue"])
    writeFileSync(join(scripts, name + ".sh"), `#!/usr/bin/env bash\nexec "$WFSIM_TEST_NODE" "$WFSIM_TEST_TOOL" ${name} "$@"\n`);
  const log = join(work, "phases.jsonl"), env = { ...process.env, PATH: `${bin}:${process.env.PATH}`, TMPDIR: join(work, "tmp"),
    WFSIM_BOARD: join(bin, "board"), WFSIM_TEST_LOG: log, WFSIM_TEST_NODE: process.execPath,
    WFSIM_TEST_TOOL: join(work, "phase-tool.cjs") };
  delete env.INVENTORY_ONLY; delete env.WFSIM_TEST_FAIL_PUBLISH; delete env.WFSIM_INVENTORY_WORKERS;
  if (workers !== undefined) env.WFSIM_INVENTORY_WORKERS = workers;
  if (mode !== undefined) env.INVENTORY_ONLY = mode;
  if (failPublication) env.WFSIM_TEST_FAIL_PUBLISH = "1";
  const result = spawnSync("bash", [join(SOURCE, "scripts/reconcile_queue.sh"), "library.json", "facts.ndjson", "queue.ndjson"],
    { cwd: work, env, encoding: "utf8", timeout: 10000 });
  const trace = existsSync(log) ? readFileSync(log, "utf8").trim().split("\n").filter(Boolean).map(line => JSON.parse(line)) : [];
  return { ...result, trace, phases: trace.map(row => row.phase), work };
}

const field = (step, name) => step.split("\n").find(line => new RegExp(`^ {6}- ${name}:|^ {8}${name}:`).test(line))
  ?.replace(/^\s*(?:- )?[^:]+:\s*/, "") || "";
const lines = workflow.split("\n"), starts = lines.flatMap((line, i) => /^ {6}- /.test(line) ? [i] : []);
const steps = starts.map((at, i) => lines.slice(at, starts[i + 1] ?? lines.length).join("\n"));
const evaluate = (expression, inventoryOnly) => runInNewContext(expression.replace(/^\$\{\{\s*|\s*\}\}$/g, "")
  .replace(/inputs\.inventory_only\b/g, String(inventoryOnly))
  .replace(/inputs\.[A-Za-z0-9_]+/g, "''")
  .replace(/steps\.wired\.outputs\.yes/g, "'true'")
  .replace(/steps\.intake\.outcome/g, inventoryOnly ? "'skipped'" : "'success'"), {}, { timeout: 20 });

try {
  const group = lines.find(line => /^  group:/.test(line))?.replace(/^  group:\s*/, "") || "";
  const groupOf = mode => group.startsWith("${{") ? evaluate(group, mode) : group.replace(/^["']|["']$/g, "");
  check(groupOf(true) === "board-inventory" && groupOf(false) === "board-queue",
    "inventory-only has a stable independent slot while normal queue runs keep theirs");
  const only = reconcile("1"), expected = ["capture", "enumerate", "roster", "bind", "publish"];
  check(only.status === 0 && JSON.stringify(only.phases) === JSON.stringify(expected),
    "inventory-only captures, enumerates, binds and publishes once without pacing, dispatch or purge", JSON.stringify(only.phases) + only.stderr);
  const capture = only.trace.find(row => row.phase === "capture")?.args || [];
  const enumerate = only.trace.find(row => row.phase === "enumerate")?.args || [];
  const bind = only.trace.find(row => row.phase === "bind")?.args || [], publish = only.trace.find(row => row.phase === "publish")?.args || [];
  const prefix = capture[4], roster = bind[6], missing = bind[3], orphans = bind[5];
  check(capture.slice(1, 4).join("|") === "library.json|facts.ndjson|queue.ndjson"
    && ["--each-ruler", "--dry-run", "--gate"].every(flag => enumerate.includes(flag))
    && enumerate[enumerate.indexOf("--facts-in") + 1] === "facts.ndjson"
    && enumerate[enumerate.indexOf("--queue-in") + 1] === "queue.ndjson"
    && enumerate[enumerate.indexOf("--inventory-builds") + 1] === prefix + ".canonical"
    && JSON.stringify(bind.slice(1)) === JSON.stringify([prefix + ".builds", prefix + ".canonical", missing, "queue.ndjson", orphans, roster])
    && JSON.stringify(publish.slice(1)) === JSON.stringify(["queue.ndjson", missing, orphans, "facts.ndjson", roster, prefix]),
  "inventory-only keeps the existing snapshot and enumeration arguments");
  check(enumerate[enumerate.indexOf("--inventory-workers") + 1] === "2", "full reconciliation defaults to two concurrent ruler walks");
  const override = reconcile("1", false, "1").trace.find(row => row.phase === "enumerate")?.args || [];
  check(override[override.indexOf("--inventory-workers") + 1] === "1", "an explicit worker budget reaches the native walk");
  const removed = prefix ? [prefix, ...[".builds", ".canonical", ".tasks", ".json"].map(suffix => prefix + suffix), roster, missing, orphans] : [];
  check(removed.length === 8 && removed.every(path => !existsSync(path)) && existsSync(join(only.work, "queue.ndjson")),
    "inventory-only cleans its snapshot, roster and temporary candidates after publication");
  const refusal = reconcile("1", true);
  check(refusal.status !== 0 && refusal.phases.at(-1) === "publish" && !refusal.phases.some(phase => ["pace", "dispatch", "purge_queue", "purge_orphans"].includes(phase)),
    "failed inventory publication stays failed and never reaches queue mutations", refusal.stderr);
  const invalid = ["2", "true", "bogus"].map(mode => reconcile(mode));
  check(invalid.every(result => result.status !== 0 && result.phases.length === 0), "invalid nonempty inventory modes fail before capture");
  const normal = [undefined, "0", ""].map(mode => reconcile(mode));
  const normalPhases = ["admission"];
  check(normal.every(result => result.status === 0 && JSON.stringify(result.phases) === JSON.stringify(normalPhases)),
    "ordinary reconciliation only recovers existing work and never enumerates, creates or purges tasks", JSON.stringify(normal.map(result => result.phases)));

  check(/\n      inventory_only:\n(?: {8}[^\n]*\n)*?        type: boolean\n(?: {8}[^\n]*\n)*?        default: false/.test(workflow),
    "inventory-only is an explicit boolean dispatch input defaulting to false");
  const modeExpression = lines.find(line => /^  INVENTORY_ONLY:/.test(line))?.replace(/^  INVENTORY_ONLY:\s*/, "") || "''";
  check(evaluate(modeExpression, true) === "1" && evaluate(modeExpression, false) === "0", "the workflow passes the requested mode to reconciliation");
  const intake = steps.find(step => field(step, "name") === "take in what has arrived") || "";
  check(!evaluate(field(intake, "if") || "true", true) && evaluate(field(intake, "if") || "true", false),
    "inventory-only does not schedule the real intake/family step");
  const scan = steps.find(step => step.includes("bash scripts/reconcile_queue.sh")) || "";
  check(scan && evaluate(field(scan, "if") || "true", true) && evaluate(field(scan, "if") || "true", false),
    "both workflow modes reach their selected reconciliation behavior");
  const preflight = steps.find(step => field(step, "name") === "validate inventory-only request") || "";
  check(preflight && steps.indexOf(preflight) < steps.indexOf(intake)
    && steps.indexOf(preflight) < steps.findIndex(step => step.includes("uses: ./.github/actions/databases")),
    "conflicting inventory-only flags are checked before intake or database setup");
  const body = preflight.split("\n").slice(preflight.split("\n").findIndex(line => /^ {8}run: \|/.test(line)) + 1)
    .filter(line => !line.trim() || /^ {10}/.test(line)).map(line => line.slice(10)).join("\n");
  const flags = ["RESCORE", "WEAPONS", "WHY", "DAY", "CANCEL"];
  const guard = (mode, selected) => spawnSync("bash", ["-c", body], { encoding: "utf8", timeout: 1000,
    env: { ...process.env, INVENTORY_ONLY: mode, ...Object.fromEntries(flags.map(flag => [flag, flag === selected ? "requested" : ""])) } });
  check(flags.every(flag => guard("1", flag).status !== 0) && guard("1").status === 0 && flags.every(flag => guard("0", flag).status === 0),
    "the actual preflight rejects each mixed action while allowing inventory-only and normal requests");
} finally { rmSync(temporary, { recursive: true, force: true }); }

if (failed) throw new Error(`${failed} inventory-only checks failed`);
