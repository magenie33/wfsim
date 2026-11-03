// SPDX-License-Identifier: AGPL-3.0-or-later
// Publish the classified inventory captured before admission pacing.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { execFileSync } from "node:child_process";
import { classifyInventory } from "./classify_compute_inventory.mjs";

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
  const plan = await classifyInventory({ queue, missing, orphans, builds: `${prefix}.builds`, tasks: `${prefix}.tasks`,
    canonical: `${prefix}.canonical`,
    meta: JSON.parse(readFileSync(`${prefix}.json`, "utf8")), rulers: readFileSync(roster, "utf8").trim().split(/\s+/) });
  plan.source = execFileSync("git", ["rev-parse", "HEAD"], { encoding: "utf8" }).trim();
  await publishInventory(plan);
  console.log(`inventory: ${plan.rows} distinct board tasks, including work not yet admitted`);
}
