// A RIVEN GAIN'S SEARCH IS THE ONE ITS PAGE SHOWS — docs/AGENT.md §"Riven
// appraisal". The bot and the appraisal page both send
// `headlessRivenGainRequest` (87-headless.js); the page also sets the search up
// through its own controls, the ruler, the riven and the built-in search, and
// what those controls would send must be that request key for key. Each pair
// is one clean boot of `/appraise/<code>?freeze=1` (81-appraisal.js), the
// appraisal it reads answered on the wire — every weapon under the primary
// ruler, every ruler on a few weapons, across the card's four shapes.
//
//   node scripts/check_riven_request.mjs                 serves site/ itself
//   WFSIM_BASE=http://127.0.0.1:<port> node scripts/check_riven_request.mjs
//   WFSIM_RIVEN_SHARD=1/4 …   every fourth weapon, from the second
//   WFSIM_RIVEN_LANES=4 …     clean pages at once (default 4)
import { openApp } from "./cdp.mjs";

const [SHARD, SHARDS] = (process.env.WFSIM_RIVEN_SHARD || "0/1").split("/").map(Number);
if (!(SHARDS >= 1 && SHARD >= 0 && SHARD < SHARDS)) {
  console.error(`WFSIM_RIVEN_SHARD must be i/n with 0 <= i < n, not ${process.env.WFSIM_RIVEN_SHARD}`);
  process.exit(2);
}
const LANES = Math.max(1, Number(process.env.WFSIM_RIVEN_LANES) || 4);
/// The weapons every ruler is checked on: a plain gun, a melee, an Incarnon.
const EVERY_RULER_ON = ["torid", "laetum", "nikana_prime"];
/// The card's four shapes, rotated through the pairs.
const CARDS = [
  { bonuses: [{ id: "multishot", roll: 1.066 }, { id: "toxin", roll: 1.06 }, { id: "critical_damage", roll: 1.063 }], malus: { id: "zoom", roll: 0.931 }, rank: 8 },
  { bonuses: [{ id: "critical_chance", roll: 1.1 }, { id: "base_damage", roll: 0.95 }], malus: null, rank: 8 },
  { bonuses: [{ id: "critical_damage", roll: 1.0 }, { id: "status_chance", roll: 1.02 }], malus: { id: "impact", roll: 0.9 }, rank: 6 },
  { bonuses: [{ id: "multishot", roll: 1.1 }, { id: "fire_rate", roll: 1.1 }, { id: "heat", roll: 1.1 }], malus: null, rank: 8 },
];


let failures = 0, compared = 0, refused = 0;
const fail = (what) => { failures++; console.log(`  FAIL  ${what}`); };

const canon = (v) => Array.isArray(v) ? v.map(canon) : v && typeof v === "object"
  ? Object.fromEntries(Object.keys(v).sort().map((k) => [k, canon(v[k])])) : v;
/// Where two searches part, as `path: controls | sent`, at most a few.
function parts(a, b, path = "", out = []) {
  if (out.length >= 4 || JSON.stringify(canon(a)) === JSON.stringify(canon(b))) return out;
  if (a && b && typeof a === "object" && typeof b === "object") {
    for (const k of new Set([...Object.keys(a), ...Object.keys(b)])) parts(a[k], b[k], `${path}.${k}`, out);
  } else {
    out.push(`${path}: ${JSON.stringify(a)} | ${JSON.stringify(b)}`.slice(0, 300));
  }
  return out;
}

const first = await openApp({ profile: `wfsim-riven-request-0-${Date.now()}` });
const meta = await first.evaluate(`(async () => { for (let i = 0; i < 200 && !(window.META && META.weapons); i++) await new Promise((r) => setTimeout(r, 100)); return META; })()`);
const primary = (meta.benchmarks.find((b) => b.primary) || meta.benchmarks[0]).id;
const pairs = [];
meta.weapons.forEach((w, i) => { if (i % SHARDS === SHARD) pairs.push([w.id, primary]); });
if (SHARD === 0) {
  for (const w of EVERY_RULER_ON) for (const b of meta.benchmarks) if (b.id !== primary) pairs.push([w, b.id]);
}
console.log(`${pairs.length} pairs, ${LANES} lanes`);

const apps = [first];
for (let i = 1; i < LANES; i++) apps.push(await openApp({ profile: `wfsim-riven-request-${i}-${Date.now()}` }));
// THE APPRAISAL, ANSWERED ON THE WIRE: a static site has no worker behind
// `/api/appraise/<code>`, and the page reads nothing else of the appraisal. The
// service worker is bypassed so the request reaches the wire to be answered.
for (const app of apps) {
  await app.send("Network.enable");
  await app.send("Network.setBypassServiceWorker", { bypass: true });
  await app.send("Fetch.enable", { patterns: [{ urlPattern: "*/api/appraise/*" }] });
  app.on("Fetch.requestPaused", (p) => app.send("Fetch.fulfillRequest", { requestId: p.requestId, responseCode: 200,
    responseHeaders: [{ name: "content-type", value: "application/json" }],
    body: Buffer.from(JSON.stringify(app.job || { ok: false })).toString("base64") }));
}

const t0 = Date.now();
let next = 0;
await Promise.all(apps.map(async (app) => {
  while (next < pairs.length) {
    const k = next++;
    const [weapon, ruler] = pairs[k];
    const card = CARDS[k % CARDS.length];
    // A CODE IS THREE TO TWELVE CHARACTERS, or the link is not an appraisal's.
    const code = `T${String(k).padStart(4, "0")}`;
    app.job = { ok: true, code, weapon, ruler, riven: card, at: Date.now(), done: false };
    // A CLEAN BOOT: nothing an earlier pair stored is read by the next.
    await app.send("Storage.clearDataForOrigin", { origin: new URL(app.BASE).origin, storageTypes: "local_storage,indexeddb,session_storage" });
    await app.send("Page.navigate", { url: `${app.BASE}/appraise/${code}?freeze=1` });
    let got = "";
    for (let i = 0; i < 600 && !got; i++) {
      await app.sleep(100);
      try { got = await app.evaluate(`document.body && document.body.dataset.request || ""`); } catch { got = ""; }
    }
    if (!got) { fail(`${weapon} under ${ruler}: the page froze nothing in 60 s`); continue; }
    const page = JSON.parse(got);
    // A PAGE THAT REFUSES THE SEARCH has nothing to compare; the bot then stores
    // a search the page would not run, so a refusal is counted and printed.
    if (page.error) { refused++; console.log(`  note  ${weapon} under ${ruler}: the page refuses it (${page.error})`); continue; }
    const shown = JSON.parse(await app.evaluate(`JSON.stringify({ request: optimizeBody(),
      context: { ...boardBuildContext(), rivens: { [appraisal.rivenMod]: boardRivenShape(appraisal.rivenMod) } } })`));
    const diff = parts(shown, { request: page.request, context: page.context });
    compared++;
    if (diff.length) fail(`${weapon} under ${ruler}:\n        ${diff.join("\n        ")}`);
    if (compared % 50 === 0) console.log(`  ${compared}/${pairs.length} in ${((Date.now() - t0) / 1000).toFixed(0)} s`);
  }
}));
for (const app of apps) await app.close();

console.log(`\n${compared} compared, ${refused} refused by the page, ${((Date.now() - t0) / 1000).toFixed(0)} s`);
if (failures) { console.log(`${failures} failed`); process.exit(1); }
console.log("a riven gain sends the search its page shows");
