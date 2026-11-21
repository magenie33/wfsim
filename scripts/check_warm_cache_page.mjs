// SPDX-License-Identifier: AGPL-3.0-or-later
// node scripts/check_warm_cache_page.mjs [local-release-site]
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile, readdir, mkdir, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { resolve, join } from "node:path";
import { fileURLToPath } from "node:url";
import { isDeepStrictEqual } from "node:util";
import { gzipSync } from "node:zlib";
import { openApp, serveSite, sleep } from "./cdp.mjs";
import { officialSearchRequest } from "./bake_search_cache_request.mjs";

const repo = fileURLToPath(new URL("../", import.meta.url));
const site = resolve(process.argv[2] || join(repo, "site"));
const ruler = "standard_multi_target__aimed_half";
const weapons = { ocucor: "Ocucor", torid: "Torid", kuva_nukor: "Kuva_Nukor" };
const scenarioBar = "#preset-bar-simulator-scenarios";
const sha = bytes => createHash("sha256").update(bytes).digest("hex");
const read = path => readFile(join(site, path), "utf8");
const [html, generation] = await Promise.all([read("index.html"), read("pkg/generation.json").then(JSON.parse)]);
const appPath = html.match(/<script\s+[^>]*?src="(\/asset\/app\.[a-f0-9]+\.js)"/)?.[1];
const glue = generation.names.find(name => /^wfsim_wasm\.[a-f0-9]+\.js$/.test(name));
assert(appPath && glue, "Expected the release's app and WASM glue");
const published = await read(appPath);

function replaceOnce(text, needle, replacement) {
  const first = text.indexOf(needle);
  assert(first !== -1 && text.indexOf(needle, first + needle.length) === -1, `Expected one ${needle}`);
  return text.slice(0, first) + replacement + text.slice(first + needle.length);
}

const appDir = join(repo, "web/src/static/app");
const parts = (await readdir(appDir)).filter(name => name.endsWith(".js")).sort();
let candidate = (await Promise.all(parts.map(name => readFile(join(appDir, name), "utf8")))).join("");
for (const name of ["BUILD_ID", "BUILD_SHA", "PROJECT_FACTS", "RELEASE_ID", "ENGINE_ID",
  "ENGINE_WASM", "ENGINE_WASM_BYTES", "ENGINE_PACKS", "ENGINE_VIEWS"]) {
  const pattern = new RegExp(`^const ${name} = .*;$`, "m");
  const local = candidate.match(pattern)?.[0], shipped = published.match(pattern)?.[0];
  assert(local && shipped, `Missing release binding ${name}`);
  candidate = replaceOnce(candidate, local, shipped);
}
const buildId = published.match(/^const RELEASE_ID = "([a-f0-9]+)";/m)?.[1];
assert(buildId, "Missing release cache namespace");

// Run the release's real lightweight APIs on the page's main thread.
// The closed endpoint list and Worker guard prevent fight work.
function installAudit(glue, warm) {
  const audit = window.warmPageAudit = { raw: [], sent: [], calls: [], blocked: [], workers: [] };
  window.Worker = new Proxy(window.Worker, { construct(_target, args) {
    audit.workers.push(String(args[0]));
    throw Error("Worker creation forbidden by warm-cache page check");
  } });
  const allowed = new Set(["/api/auto-forma", "/api/catalogue", "/api/computation/input", "/api/i18n",
    "/api/meta", "/api/opt-buffs", "/api/optimize/check", "/api/panel", "/api/warframe/catalog", "/api/warframe/panel"]);
  let loading;
  const load = () => loading ??= (async () => {
    const script = document.createElement("script");
    const inserted = new Promise((ok, fail) => { script.onload = ok; script.onerror = fail; });
    script.src = glue;
    document.head.append(script);
    const [{ module, packs }] = await Promise.all([engineLoad(), inserted]);
    await wasm_bindgen({ module_or_path: module });
    for (const pack of packs) {
      const err = wasm_bindgen.install_data_bytes(pack);
      if (err) throw Error(err);
    }
  })();
  audit.engine = async (path, body) => {
    if (!allowed.has(path)) {
      audit.blocked.push(path);
      return { ok: false, error: "Endpoint disabled by warm-cache page check" };
    }
    audit.calls.push(path);
    if (ENGINE_VIEWS[path]) {
      const response = await fetch(ENGINE_VIEWS[path]);
      if (response.ok) return response.json();
    }
    await load();
    return JSON.parse(wasm_bindgen.api(path, JSON.stringify(body ?? {})));
  };
  const realApi = api;
  api = async (path, body, ...rest) => {
    if (["/api/optimize", "/api/optimize/status", "/api/optimize/cancel"].includes(path)) return realApi(path, body, ...rest);
    const answer = await audit.engine(path, body);
    if (path === "/api/optimize/check") audit.raw.push({ body: structuredClone(body), answer });
    return answer;
  };
  const start = woptStart;
  woptStart = (body, checkpoint, _onCheckpoint, _onBuilds, options) => {
    const task = userTasks.view().find(t => t.kind === "optimization" && t.state === "running");
    audit.sent.push({ task: structuredClone(task), body: structuredClone(body), checkpoint: checkpoint ?? null, options: options ?? null });
    if (warm && body.strategy === "quick" && !checkpoint) return start(body, checkpoint, _onCheckpoint, _onBuilds, options);
    return { ok: false, error: "Dispatch outside the quick fleet stopped by warm-cache page check" };
  };
  if (warm) {
    audit.identity = searchWarmIdentity;
    audit.hash = searchWarmIdentityHash;
    audit.scene = body => Object.fromEntries(Object.entries(body).filter(([key]) => !searchWarmExcluded.has(key)));
    audit.cacheBase = searchWarmBase().pathname;
    audit.stopFleet = async (cache, body, options) => {
      await cache?.flush();
      const id = options && await audit.identity(body, options);
      const key = id && await audit.hash(id);
      const storage = searchWarmStorage(indexedDB, 1500, searchWarmLimits.local, searchWarmLimits.disk);
      const asset = key && await storage.read("asset:" + key);
      audit.sent.at(-1).cache = { asset: !!asset?.asset, pack: cache?.exportPack() ?? null };
      throw Error("First optimizer dispatch stopped by warm-cache page check");
    };
  }
  localStorage.setItem("wfsim-lang", "en");
}

const cachePath = `/search-cache/v1/${buildId}/`;
const manifest = { version: 1, packs: {} }, packs = new Map(), requests = [];
const staticSite = await serveSite(site);
const server = createServer(async (req, res) => {
  try {
    const path = new URL(req.url, "http://127.0.0.1").pathname;
    if (path === appPath || path.startsWith(cachePath)) {
      let data, type;
      if (path === appPath) {
        data = servedApp;
        type = "text/javascript";
      } else {
        requests.push(path);
        data = path === `${cachePath}manifest.json` ? JSON.stringify(manifest) : packs.get(path);
        type = path.endsWith(".gz") ? "application/gzip" : "application/json";
      }
      if (data === undefined) { res.writeHead(404).end(); return; }
      res.writeHead(200, { "content-type": type, "content-length": Buffer.byteLength(data), "cache-control": "no-store" });
      res.end(data);
      return;
    }
    if (req.method !== "GET" && req.method !== "HEAD") { res.writeHead(405).end(); return; }
    const upstream = await fetch(`http://127.0.0.1:${staticSite.address().port}${req.url}`);
    res.writeHead(upstream.status, Object.fromEntries(upstream.headers));
    res.end(Buffer.from(await upstream.arrayBuffer()));
  } catch (error) {
    if (!res.headersSent) res.writeHead(500);
    res.end(String(error));
  }
});
function withAudit(source, warm) {
  if (warm) {
    // Let the actual fleet open the cache, then stop before the first job.
    source = replaceOnce(source, "\n    grow(job.shards);\n    show();", "\n    show();");
    source = replaceOnce(source, "\n    const cacheFlag = cache ?", "\n    await window.warmPageAudit.stopFleet(cache, body, searchWarm);\n    const cacheFlag = cache ?");
  }
  return replaceOnce(source, "\ninit()\n", `\n;(${installAudit.toString()})(${JSON.stringify(`/pkg/${glue}`)}, ${warm});\ninit()\n`);
}
let servedApp = withAudit(published, false);
const evidence = { release: buildId, app: appPath, appSha256: sha(published), candidateSha256: sha(candidate), cases: [] };
let browser, failures = 0;

async function until(expr, label, timeout = 12000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    const result = await browser.evaluate(expr);
    if (result) return result;
    await sleep(50);
  }
  const state = await browser.evaluate(`({ href: location.pathname, ready: window.__wfsimReady, note: $("boot-note")?.textContent,
    weapon: $("weapon")?.value, scenario: activeScenario, userTaskError, raw: warmPageAudit.raw.at(-1)?.answer,
    tasks: userTasks?.view().map(t => [t.state, t.error]), calls: [...new Set(warmPageAudit.calls)], blocked: warmPageAudit.blocked })`);
  throw Error(`${label}: ${JSON.stringify(state)}`);
}

const query = selector => `document.querySelector(${JSON.stringify(selector)})`;
async function click(selector) {
  await until(`(() => {
    const el = ${query(selector)};
    if (!el || el.disabled || !el.getClientRects().length) return false;
    el.click(); return true;
  })()`, `click ${selector}`);
}

async function pickDropdown(id, value) {
  await click(`#${id}`);
  await click(`#dd-menu [data-v=${JSON.stringify(value)}]:not(.dis)`);
}

async function selectOfficial() {
  await click(`${scenarioBar} [data-board-open]`);
  const pop = "#board-choice-popover";
  if (!await browser.evaluate(`!!${query(`${pop} [data-board-choice="${ruler}"]`)}`)) {
    await click(`${pop} [data-board-axis="base"] [data-board-choice^="standard_multi_target"]`);
  }
  const choice = `${pop} [data-board-choice="${ruler}"]`;
  if (await browser.evaluate(`${query(`${pop} [data-board-selected]`)}?.dataset.boardSelected !== ${JSON.stringify(ruler)}`)) await click(choice);
  assert.equal(await browser.evaluate(`${query(`${pop} [data-board-selected]`)}?.dataset.boardSelected`), ruler);
  await click(`${pop} [data-board-apply]`);
  return until(`${query(`${scenarioBar} .preset-current`)}?.dataset.name === ${JSON.stringify(ruler)} && ${query(`${scenarioBar} .preset-current`)}.classList.contains("ro")`, "official scenario chip");
}

async function loadWeapon(weapon, module) {
  await browser.load(`/weapons/${weapons[weapon]}/${module}`);
  await until(`document.body.dataset.panelFor === ${JSON.stringify(weapon)} && modSuffix() === ${JSON.stringify(module === "builder" ? "" : module)}`, `${weapon} ${module}`);
}

async function capture(label) {
  const index = await browser.evaluate(`warmPageAudit.sent.length`);
  const first = requests.length;
  await click("#run-opt");
  const sent = await until(`warmPageAudit.sent[${index}]?.cache && warmPageAudit.sent[${index}]`, `${label} frozen request and fleet cache`);
  const raw = await browser.evaluate(`warmPageAudit.raw.at(-1)`);
  assert(raw?.answer.compatible, `${label} used real compatibility validation`);
  assert.deepEqual(sent.body, sent.task.request, `${label} dispatched the frozen request unchanged`);
  assert.equal(sent.checkpoint, null);
  const id = sent.task.id;
  await until(`!userTasks.view().some(t => t.id === ${JSON.stringify(id)} && userTaskActive(t))`, `${label} task stopped`);
  return { ...sent, raw: raw.body, requests: requests.slice(first) };
}

const evaluateBody = (code, ...args) => browser.evaluate(`(${code.toString()})(...${JSON.stringify(args)})`);
const check = (label, ok, detail) => {
  browser.check(label, ok, detail);
  if (!ok) failures++;
};

function differences(a, b) {
  return Object.fromEntries([...new Set([...Object.keys(a), ...Object.keys(b)])].sort()
    .filter(key => !isDeepStrictEqual(a[key], b[key]))
    .map(key => [key, [Object.hasOwn(a, key) ? a[key] : "<absent>", Object.hasOwn(b, key) ? b[key] : "<absent>"]]));
}

async function identity(body, options) {
  return evaluateBody(async (body, options) => {
    const id = await warmPageAudit.identity(body, options);
    return { value: id, key: id && await warmPageAudit.hash(id), scene: warmPageAudit.scene(body) };
  }, body, options);
}

function publishEmptyPack(id) {
  assert(id.key);
  const bytes = gzipSync(JSON.stringify({ version: 1, identity: id.value, entries: [] }));
  const digest = sha(bytes), path = `${cachePath}${digest}.json.gz`;
  manifest.packs[id.key] = { bytes: bytes.length, sha256: digest };
  packs.set(path, bytes);
  return path;
}

try {
  await new Promise((ready, fail) => server.once("error", fail).listen(0, "127.0.0.1", ready));
  const base = `http://127.0.0.1:${server.address().port}`;
  browser = await openApp({ base, profile: "wfsim-warm-page-release" });
  await loadWeapon("torid", "optimizer");
  check("published release starts on home and the optimizer deep link", await browser.evaluate(`WASM && !!$("run-opt") && !$("boot-note")`));
  assert.deepEqual(await browser.evaluate(`warmPageAudit.workers`), []);
  await browser.close();

  servedApp = withAudit(candidate, true);
  browser = await openApp({ base, profile: "wfsim-warm-page-candidate" });
  check("source app boots with the release's WASM, views and cache namespace",
    await browser.evaluate(`WASM && !!META.weapons.length && !$("boot-note") && warmPageAudit.cacheBase === ${JSON.stringify(cachePath)}`));

  for (const weapon of Object.keys(weapons)) {
    await loadWeapon(weapon, "simulator");
    await selectOfficial();
    await loadWeapon(weapon, "optimizer");
    const hostRaw = await officialSearchRequest({ site, weapon, ruler });
    const hostAnswer = await evaluateBody(request => warmPageAudit.engine("/api/computation/input", { kind: "optimization", request }), hostRaw);
    assert(hostAnswer.ok, JSON.stringify(hostAnswer));
    const host = hostAnswer.request;
    const official = { official: ruler };
    const [rawId, hostId] = await Promise.all([identity(hostRaw, official), identity(host, official)]);
    const packPath = publishEmptyPack(hostId);
    const sent = await capture(weapon);
    const pageId = await identity(sent.body, sent.options);
    const diff = differences(pageId.scene, hostId.scene);
    const rawDiff = differences(rawId.scene, pageId.scene);
    const axes = Object.fromEntries(["weapon", "mode", "valence_element", "valence_bonus", "assembly"].map(k => [k, [sent.body[k], host[k]]]));
    check(`${weapon} captured the selected official ID in the frozen task and wopt`,
      sent.task.origin.official_scenario === ruler && sent.options?.official === ruler && sent.body.headshot_pct === 50);
    check(`${weapon} normalized browser and host cache identity and weapon axes match`,
      isDeepStrictEqual(pageId.value, hostId.value) && Object.values(axes).every(([a, b]) => isDeepStrictEqual(a, b)), JSON.stringify({ diff, axes }));
    check(`${weapon} freezes default Eximus, landscape and ammo drops`, sent.body.eximus === false && sent.body.landscape === false && sent.body.ammo_drops === true);
    check(`${weapon} reads and persists the host-keyed pack`, sent.cache.asset && sent.requests.includes(packPath), JSON.stringify({ cache: sent.cache, requests: sent.requests }));
    console.log(`       ${weapon} raw host -> frozen scene: ${JSON.stringify(rawDiff)}`);
    evidence.cases.push({ weapon, sent, hostRaw, host, pageId, rawId, hostId, diff, rawDiff, axes });
  }

  const ocucor = evidence.cases.find(entry => entry.weapon === "ocucor");
  await loadWeapon("ocucor", "optimizer");
  await click("[data-opt-board-consent]");
  await click('[data-fold="opt-limits"].shut > .fold-h');
  await click('#opt-limit-mods [data-id="galvanized_crosshairs"] .mn');
  await click('#opt-limits [data-axis="arcanes"][data-id="secondary_fortifier"] .mn');
  const personal = await capture("personal search with exclusions");
  const personalId = await identity(personal.body, personal.options);
  check("personal official search keeps its edited exclusions and reuses the cache from IndexedDB",
    personal.task.origin.board_upload === false && personal.options?.official === ruler &&
    personal.body.limits.exclude.mods.includes("galvanized_crosshairs") && personal.body.limits.exclude.arcanes.includes("secondary_fortifier") &&
    personalId.key === ocucor.pageId.key && personal.cache.asset && personal.requests.length === 0,
    JSON.stringify({ limits: personal.body.limits, options: personal.options, requests: personal.requests }));
  evidence.personal = { sent: personal, identity: personalId };

  const torid = evidence.cases.find(entry => entry.weapon === "torid");
  await loadWeapon("torid", "builder");
  await pickDropdown("dd-mode", "base");
  await click('#module-tabs a[href$="/optimizer"]');
  await until('modSuffix() === "optimizer"', "Torid optimizer tab");
  const modeEdit = await capture("Torid base mode");
  const modeId = await identity(modeEdit.body, modeEdit.options);
  check("Torid mode edit survives freezing and keeps the published scene key",
    modeEdit.raw.mode === "base" && modeEdit.body.mode === "base" && torid.sent.body.mode === "cycle" &&
    modeId.key === torid.pageId.key && modeEdit.cache.asset, JSON.stringify({ mode: modeEdit.body.mode, identity: modeId.value }));
  evidence.mode = { sent: modeEdit, identity: modeId };

  await click('#opt-scenario-ref [data-global-scenario]');
  await click("#sim-official-copy");
  const copied = await until(`${query(`${scenarioBar} .preset-current:not(.ro)`)}?.dataset.name`, "editable copy in scenario bar");
  await click('#module-tabs a[href$="/optimizer"]');
  await until('modSuffix() === "optimizer"', "optimizer after copying");
  const custom = await capture("copied custom scenario");
  const customId = await identity(custom.body, custom.options);
  check("an unchanged editable copy preserves the scene hash and never fetches an official pack",
    copied !== ruler && custom.task.origin.scenario === copied && custom.task.origin.official_scenario === null &&
    custom.options?.official === null && customId.value?.scenario === torid.pageId.value?.scenario &&
    customId.key !== torid.pageId.key && !custom.cache.asset && custom.requests.length === 0,
    JSON.stringify({ copied, origin: custom.task.origin.scenario, identity: customId.value, requests: custom.requests }));
  evidence.custom = { sent: custom, identity: customId };

  await browser.load("/warframes/Excalibur/builder");
  await click("#preset-bar-warframes .pchip.add");
  const framePreset = await until(`$("preset-bar-warframes").querySelector(".preset-current:not(.ro)")?.dataset.name`, "editable Warframe build");
  await pickDropdown("dd-wf-shard-0", "crimson_archon_shard");
  await pickDropdown("dd-wf-shard-effect-0", "secondary_crit_chance");
  await click('[data-wf-tau="0"]');
  await until(`wfBarCfg().load().find(p => p.id === ${JSON.stringify(framePreset)})?.state.shards[0]?.tauforged`, "Warframe shard autosave");

  await loadWeapon("ocucor", "builder");
  await click("#preset-bar-builder-builds .pchip.add");
  const seatPreset = await until(`$("preset-bar-builder-builds").querySelector(".preset-current:not(.ro)")?.dataset.name`, "editable seat build");
  await click("#wielder-block.shut > .bh");
  await pickDropdown("dd-wielder", "excalibur");
  await pickDropdown("dd-wielder-preset", framePreset);
  await until(`presetListWithIds(BUILDS, "ocucor").find(p => p.id === ${JSON.stringify(seatPreset)})?.state.wielder?.preset === ${JSON.stringify(framePreset)}`, "seat wielder autosave");

  await loadWeapon("torid", "simulator");
  await click("#rs-add");
  await pickDropdown("dd-seat-0", "ocucor");
  await pickDropdown("dd-seat-preset-0", seatPreset);
  await click('#module-tabs a[href$="/optimizer"]');
  await until('modSuffix() === "optimizer"', "optimizer after adding a seat");
  const withSeat = await capture("custom scenario with Archon Shard on another seat");
  const seatId = await identity(withSeat.body, withSeat.options);
  const expectedShard = { shard: "crimson_archon_shard", effect: "secondary_crit_chance", tauforged: true };
  check("the seat's UI-equipped Archon Shard survives freezing and changes the custom scene key",
    isDeepStrictEqual(withSeat.raw.also_acting[0]?.wielder?.shards, [expectedShard]) &&
    isDeepStrictEqual(withSeat.body.also_acting[0]?.wielder?.shards, [expectedShard]) &&
    seatId.value?.scenario !== customId.value?.scenario && withSeat.options?.official === null && withSeat.requests.length === 0,
    JSON.stringify({ seat: withSeat.body.also_acting[0], options: withSeat.options, requests: withSeat.requests }));
  evidence.customSeat = { sent: withSeat, identity: seatId };

  const legacyProbe = await evaluateBody(request => warmPageAudit.engine("/api/computation/input",
    { kind: "optimization", request }), { ...ocucor.sent.raw, shards: [expectedShard] });
  assert(legacyProbe.ok, JSON.stringify(legacyProbe));
  evidence.shards = { uiRaw: ocucor.sent.raw.shards, uiFrozen: ocucor.sent.body.shards, legacyFrozen: legacyProbe.request.shards };
  console.log(`       top-level shards: ${JSON.stringify(evidence.shards)}`);
  evidence.workerAttempts = await browser.evaluate(`warmPageAudit.workers`);
  assert.deepEqual(evidence.workerAttempts, []);
  failures += Number(!!browser.bootBroken);
  await browser.close();

  // A new profile also discards the previous manifest result and its retry TTL.
  manifest.packs = {};
  packs.clear();
  const wrongPack = publishEmptyPack(ocucor.rawId);
  browser = await openApp({ base, profile: "wfsim-warm-page-raw-host" });
  await loadWeapon("ocucor", "simulator");
  await selectOfficial();
  await loadWeapon("ocucor", "optimizer");
  const miss = await capture("raw-host negative control");
  check("a manifest indexed only by the raw host identity misses the normal UI request",
    !miss.cache.asset && miss.requests.includes(`${cachePath}manifest.json`) && !miss.requests.includes(wrongPack), JSON.stringify(miss.requests));
  evidence.rawNegative = { sent: miss, wrongPack };
  assert.deepEqual(await browser.evaluate(`warmPageAudit.workers`), []);
} finally {
  if (browser) await browser.close();
  server.closeAllConnections();
  server.close();
  staticSite.closeAllConnections();
  staticSite.close();
  const output = join(repo, "target", `check-warm-cache-page-${process.pid}.json`);
  await mkdir(join(repo, "target"), { recursive: true });
  await writeFile(output, JSON.stringify(evidence, null, 2) + "\n");
  console.log(`Evidence: ${output}`);
}
process.exitCode = failures || browser?.failures ? 1 : 0;
