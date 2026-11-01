// SPDX-License-Identifier: AGPL-3.0-or-later
// Transfer requests land before intake spends their source inbox rows.
import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

export async function shipFamily(builds, { origin = process.env.FACTORY_ORIGIN || "https://wfsim.app", token = process.env.FACTORY_TOKEN,
  send = fetch } = {}) {
  const jobs = builds.flatMap(build => build.family || []);
  if (!jobs.length) return;
  if (!token) throw new Error("family transfers need FACTORY_TOKEN");
  for (let i = 0; i < jobs.length; i += 100) {
    const response = await send(`${origin}/api/board/family`, { method: "POST", headers: {
      authorization: `Bearer ${token}`, "content-type": "application/json" }, body: JSON.stringify({ jobs: jobs.slice(i, i + 100) }),
      signal: AbortSignal.timeout(60_000) });
    if (!response.ok || !(await response.json()).ok) throw new Error(`family requests refused (${response.status})`);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const builds = readFileSync(process.argv[2], "utf8").split("\n").filter(Boolean).map(line => JSON.parse(line));
  await shipFamily(builds);
}
