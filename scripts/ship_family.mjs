// SPDX-License-Identifier: AGPL-3.0-or-later
// Transfer requests land before intake spends their source inbox rows.
import { createReadStream } from "node:fs";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const MAX_JOBS = 100, MAX_BYTES = 512 * 1024, TRIES = 3;
const networkFailure = error => ["AbortError", "TimeoutError"].includes(error?.name)
  || ["ECONNRESET", "ECONNREFUSED", "EPIPE", "ETIMEDOUT", "ENETUNREACH", "EAI_AGAIN", "UND_ERR_SOCKET", "UND_ERR_CONNECT_TIMEOUT"]
    .includes(error?.code || error?.cause?.code)
  || (error?.name === "TypeError" && !error?.cause?.code);

export async function shipFamily(builds, { origin = process.env.FACTORY_ORIGIN || "https://wfsim.app", token = process.env.FACTORY_TOKEN,
  send = fetch, wait = ms => new Promise(resolve => setTimeout(resolve, ms)) } = {}) {
  const request = async (path, options) => {
    for (let attempt = 1; ; attempt++) {
      let response;
      try { response = await send(`${origin}${path}`, { ...options, signal: AbortSignal.timeout(60_000) }); }
      catch (error) {
        if (attempt >= TRIES || !networkFailure(error)) throw error;
        await wait(2000 * attempt); continue;
      }
      if (response.status === 429 || (response.status >= 500 && response.status < 600)) {
        await response.body?.cancel().catch(() => {});
        if (attempt >= TRIES) throw new Error(`family requests refused (${response.status})`);
        const after = Number(response.headers.get("retry-after"));
        await wait(Math.min(30_000, Number.isFinite(after) && after > 0 ? after * 1000 : 2000 * attempt));
        continue;
      }
      if (!response.ok) throw new Error(`family requests refused (${response.status})`);
      try { return await response.json(); }
      catch (error) {
        if (attempt >= TRIES || !networkFailure(error)) throw error;
        await wait(2000 * attempt);
      }
    }
  };
  const size = text => new TextEncoder().encode(text).byteLength;
  const unique = new Set(), suffix = "]}";
  let engine = null, prefix = "", overhead = 0, batch = [], bytes = 0, requests = 0;
  const flush = async () => {
    if (!batch.length) return;
    const body = prefix + batch.join(",") + suffix;
    const ack = await request("/api/board/family", { method: "POST", headers: {
      authorization: `Bearer ${token}`, "content-type": "application/json" }, body });
    if (!ack.ok || ack.engine !== engine || ack.accepted !== batch.length) throw new Error("family admission was not acknowledged");
    requests++; batch = []; bytes = overhead;
  };
  for await (const build of builds) for (const job of build.family || []) {
    const key = JSON.stringify([job.source, job.weapon, job.ruler]);
    if (unique.has(key)) continue;
    unique.add(key);
    if (!engine) {
      if (!token) throw new Error("family transfers need FACTORY_TOKEN");
      ({ engine } = await request("/release.json", { headers: { "cache-control": "no-cache" } }));
      if (typeof engine !== "string" || !engine) throw new Error("family transfers need a served engine");
      prefix = `{"engine":${JSON.stringify(engine)},"jobs":[`;
      overhead = size(prefix) + size(suffix); bytes = overhead;
    }
    const text = JSON.stringify(job), n = size(text);
    if (overhead + n > MAX_BYTES) throw new Error("family request exceeds the admission byte limit");
    if (batch.length === MAX_JOBS || bytes + n + (batch.length ? 1 : 0) > MAX_BYTES) await flush();
    bytes += n + (batch.length ? 1 : 0); batch.push(text);
  }
  if (!unique.size) return;
  await flush();
  return { jobs: unique.size, requests, engine };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  async function* builds() {
    const lines = createInterface({ input: createReadStream(process.argv[2]), crlfDelay: Infinity });
    for await (const line of lines) if (line.trim()) yield JSON.parse(line);
  }
  const sent = await shipFamily(builds());
  if (sent) console.log(`family: ${sent.jobs} request(s) admitted in ${sent.requests} batch(es), engine ${sent.engine}`);
}
