// SPDX-License-Identifier: AGPL-3.0-or-later
// A BATCH IS ASKED IN POPULARITY ORDER — docs/BOARD.md §"Compute orders".
//
// `popular_order.py` puts the most popular weapon first and names each row's
// place; `rowAsk` turns a place into a deadline by the batch's day, so the
// factory hands the most popular weapon out first however many runs asked it.
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { rowAsk } from "../worker/board_tasks.js";

let failed = 0;
const check = (ok, what) => {
  console.log(`  ${ok ? "ok  " : "FAIL"}  ${what}`);
  if (!ok) failed++;
};

const record = JSON.stringify({ weapon: "laetum" });
const batch = "rescore-laetum-2026-10-22";
const row = (order) => ({ identity: "b", ruler: "standard_single_target", mode: "base", ...(order === undefined ? {} : { order }) });
const now = Date.parse("2026-10-22T08:00:00Z");
const due = (order, at = now) => at + rowAsk(batch, row(order), record, "2026-01-01T00:00:00Z", null, at).due_in_ms;

check(due(3) - due(0) === 3000, "a row three places down is due three seconds later");
check(due(2, now) === due(2, now + 3_600_000), "a place is due by its batch's day, whichever run asked it");
check(due(0) < due(undefined), "a placed row is due no later than an unplaced one asked with it");
check(rowAsk("rescore-laetum-2026-01-01", row(0), record, "2026-01-01T00:00:00Z", null, now).due_in_ms >= 60_000,
  "a batch's day long past is due a minute from now, not overdue");

const dir = fs.mkdtempSync(path.join(os.tmpdir(), "popular-"));
fs.writeFileSync(path.join(dir, "p.json"), JSON.stringify({ scores: { laetum: 110, hek: 5 } }));
fs.writeFileSync(path.join(dir, "f.ndjson"), '{"identity":"b2","ruler":"r","mode":"base","score":9}\n{"identity":"b1","ruler":"r","mode":"base","score":3}\n');
const rows = ["zarr:b9", "laetum:b1", "hek:b5", "laetum:b2"]
  .map((x) => JSON.stringify({ build_id: x.split(":")[1], ruler: "r", mode: "base", weapon: x.split(":")[0] })).join("\n") + "\n";
const out = execFileSync("python3", [path.join("scripts", "popular_order.py"), path.join(dir, "p.json"), path.join(dir, "f.ndjson")],
  { input: rows, encoding: "utf8" }).trim().split("\n").map((l) => JSON.parse(l));
fs.rmSync(dir, { recursive: true, force: true });
check(out.map((r) => r.build_id).join(",") === "b2,b1,b5,b9", "the most popular weapon first, its best-scored build first inside it");
check(out.map((r) => r.order).join(",") === "0,0,1,2", "every row names its weapon's place");
check(/\{ order \}/.test(fs.readFileSync("scripts/ship_queue.sh", "utf8")), "ship_queue.sh carries a row's place to the ask");

console.log(failed ? `\n${failed} failed` : "\nthe most popular weapon is asked first");
process.exit(failed ? 1 : 0);
