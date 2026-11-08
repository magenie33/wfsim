// SPDX-License-Identifier: AGPL-3.0-or-later
// Publish the classified inventory captured before admission pacing.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { execFileSync } from "node:child_process";
import { freemem } from "node:os";
import { classifyInventory, INVENTORY_MIN_CACHE_KIB } from "./classify_compute_inventory.mjs";

export function inventoryCacheKiB(availableBytes = typeof process.availableMemory === "function" ? process.availableMemory() : freemem()) {
  const quarter = Number.isFinite(availableBytes) && availableBytes > 0 ? Math.floor(availableBytes / 4096) : 0;
  return Math.max(INVENTORY_MIN_CACHE_KIB, Math.min(4 * 1024 * 1024, quarter));
}

export async function publishInventory(plan, { origin = process.env.FACTORY_ORIGIN || "https://wfsim.app", token = process.env.FACTORY_TOKEN,
  send = fetch } = {}) {
  if (!token) throw new Error("complete inventory needs FACTORY_TOKEN");
  const response = await send(`${origin}/api/board/inventory`, { method: "POST", headers: {
    authorization: `Bearer ${token}`, "content-type": "application/json", "user-agent": "wfsim-queue" },
    body: JSON.stringify(plan), signal: AbortSignal.timeout(180000) });
  if (!response.ok || !(await response.json()).ok) throw new Error(`complete inventory refused (${response.status})`);
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  const [queue, missing, orphans, facts, roster, prefix] = process.argv.slice(2);
  if (!prefix) throw new Error("usage: compute_inventory.mjs queue missing orphans facts roster snapshot-prefix");
  const cacheKiB = inventoryCacheKiB();
  console.error(`inventory: classifier cache budget ${cacheKiB} KiB`);
  const plan = await classifyInventory({ queue, missing, orphans, cacheKiB, builds: `${prefix}.builds`, tasks: `${prefix}.tasks`,
    canonical: `${prefix}.canonical`, questions: process.env.WFSIM_INVENTORY_QUESTIONS || undefined,
    meta: JSON.parse(readFileSync(`${prefix}.json`, "utf8")), rulers: readFileSync(roster, "utf8").trim().split(/\s+/) });
  plan.source = execFileSync("git", ["rev-parse", "HEAD"], { encoding: "utf8" }).trim();
  await publishInventory(plan);
  console.log(`inventory: ${plan.rows} distinct board tasks, including work not yet admitted`);
}
