// SHORT SHARE LINKS — the worker's half, against a stub store.
//
// A short link is `/weapons/<name>/s/<id>`, and the id names a share code the
// worker stored (worker/index.js §"SHORT SHARE LINKS"). What must hold:
//   - the id is the WORKER's hash of (weapon, code): the same build is one id,
//     another weapon or another build is another, and no client picks one;
//   - what goes in comes back out, byte for byte;
//   - nothing that is not a share code is stored — a link is a build, never a
//     place to park arbitrary text;
//   - the short path serves the weapon's OWN page, so a pasted link previews
//     as that weapon.
//
//   node scripts/check_share_short.mjs
import worker, { SHARE_CODE, SHARE_ID, shareId, shareHostOf, sharePreviewText } from "../worker/index.js";
import { decodeShare, useShareHost } from "../worker/share_codec.js";
import { shareCardSvg } from "../worker/share_card.js";
import { existsSync, readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

// THE STORE, as much of the library as the endpoint touches: `ON CONFLICT DO NOTHING` keeps
// the first row under a key, and `SELECT … WHERE id = ?` reads it.
const shares = new Map();
const LIBRARY = {
  prepare: (sql) => ({
    bind: (...a) => ({
      run: async () => {
        if (/INSERT INTO shares .*ON CONFLICT DO NOTHING/.test(sql) && !shares.has(a[0])) {
          shares.set(a[0], { weapon: a[1], code: a[2], at: a[3], claim: a[4] ?? null });
        }
      },
      first: async () => (/FROM shares/.test(sql) ? shares.get(a[0]) || null : null),
    }),
  }),
};
const served = [];
const ASSETS = {
  fetch: async (req) => {
    served.push(new URL(req.url).pathname);
    return new Response("<html>weapon page</html>", { headers: { "content-type": "text/html" } });
  },
};
const env = { LIBRARY, ASSETS };
const call = (path, init) => worker.fetch(new Request(`https://wfsim.app${path}`, init), env);
const store = (w, c) => call("/api/s", {
  method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ w, c }),
}).then(async (r) => ({ status: r.status, j: await r.json() }));

console.log("short share links");

// A v4 code as the page writes one: version, then the compact alphabet.
const CODE = "42f~B2AXCUAWAbB6BD-0CS--~EO~HfHhHiHe~;b;8;M;P43EP03ESk3E";
const a = await store("Dual_Toxocyst", CODE);
check("a share code is stored and named by a short id", a.status === 200 && a.j.ok && SHARE_ID.test(a.j.id),
  JSON.stringify(a));
check("the id is the worker's own hash of (weapon, code)", a.j.id === await shareId("Dual_Toxocyst", CODE));
const again = await store("Dual_Toxocyst", CODE);
check("the same build is the same id, and one row", again.j.id === a.j.id && shares.size === 1);
const other = await store("Braton_Prime", CODE);
check("another weapon is another id", other.j.ok && other.j.id !== a.j.id);

const got = await call(`/api/s/${a.j.id}`);
const back = await got.json();
check("what went in comes back out", back.ok && back.w === "Dual_Toxocyst" && back.c === CODE, JSON.stringify(back));
check("…and may be cached for ever, since the id is its content",
  /immutable/.test(got.headers.get("cache-control") || ""));
check("any origin may ask (the desktop shell does)", got.headers.get("access-control-allow-origin") === "*");
const missing = await call("/api/s/AAAAAAAAAA");
check("an unknown id is a 404", missing.status === 404);

for (const [what, w, c] of [
  ["text with a space", "Braton", "4hello world"],
  ["markup", "Braton", "4<script>alert(1)</script>"],
  ["no version character", "Braton", "xB2AX"],
  ["a weapon slug with a slash in it", "Braton/../x", CODE],
  ["a code past the ceiling", "Braton", "4" + "A".repeat(2000)],
]) {
  const r = await store(w, c);
  check(`refused: ${what}`, r.status === 400 && !r.j.ok, JSON.stringify(r));
}
check("…and nothing refused was stored", shares.size === 2);

// Every form the page may send fits the worker's alphabet: v4/v3 text and the
// two base64url forms.
for (const c of [CODE, "3102.4.5~~", "1eJyrVkrLz1eyUkpKLFKqBQAdegQp", "0W1sxLDJd"]) {
  check(`the worker accepts the page's form ${c[0]}`, SHARE_CODE.test(c));
}

served.length = 0;
const page = await call(`/weapons/Dual_Toxocyst/s/${a.j.id}`);
check("the short path serves the weapon's own page", page.status === 200 && served[0] === "/weapons/Dual_Toxocyst",
  `${page.status} ${served.join(",")}`);

// THE PREVIEW: a chat reads the head, so the head must describe THIS build.
// Against a names table of six, through the whole worker path.
const TABLE = {
  order: ["torid", "serration", "split_chamber", "primary_merciless", "critical_chance", "zoom"],
  names: { torid: "Torid", serration: "Serration", split_chamber: "Split Chamber",
    primary_merciless: "Primary Merciless", critical_chance: "Critical Chance", zoom: "Zoom" },
  evolution_prefixed: [],
  scenarios: { standard_single_target: "Standard Single Target" },
  enemies: { thrax_centurion: "Thrax Centurion" },
};
// v4: weapon 00, slots 01 02 and riven 0, arcane 03, a riven of +04 −05 at roll 1.0.
const TINY = "400~0102-0~03~~;a;8;M;041c;051c";
const HEAD = `<html><head><title>Torid — board</title>
<meta name="description" content="the board's best" />
<meta property="og:title" content="Torid — board" />
<meta property="og:description" content="the board's best" />
<meta property="og:url" content="https://wfsim.app/weapons/Torid/builder" />
</head><body></body></html>`;
const envP = {
  LIBRARY,
  ASSETS: { fetch: async (req) => (new URL(req.url).pathname === "/share-names.json"
    ? new Response(JSON.stringify(TABLE)) : new Response(HEAD, { headers: { "content-type": "text/html", etag: "x" } })) },
};
const tiny = await store("Torid", TINY);
const shown = await (await worker.fetch(new Request(`https://wfsim.app/weapons/Torid/s/${tiny.j.id}`), envP)).text();
const og = (p) => (shown.match(new RegExp(`<meta (?:property|name)="${p}" content="([^"]*)"`)) || [])[1];
check("a short link's preview is titled by its weapon", og("og:title") === "Torid build | WFSim", og("og:title"));
check("…and describes the build it carries, not the board's",
  og("og:description") === "Mods: Serration · Split Chamber. Riven: +Critical Chance −Zoom. Arcane: Primary Merciless.",
  og("og:description"));
check("…names the link itself as its url", og("og:url") === `https://wfsim.app/weapons/Torid/s/${tiny.j.id}`, og("og:url"));
check("…and asks not to be indexed apart from the weapon", og("robots") === "noindex");
check("…in the page's <title> too", /<title>Torid build \| WFSim<\/title>/.test(shown));
check("…with a large card image drawn for this link",
  og("og:image") === `https://wfsim.app/og/s/${tiny.j.id}.png?v=1` && og("twitter:card") === "summary_large_image",
  `${og("og:image")} ${og("twitter:card")}`);
const noCard = await worker.fetch(new Request("https://wfsim.app/og/s/BBBBBBBBBB.png"), envP);
check("a card for a link that does not exist is a 404, not a broken image", noCard.status === 404);
const svg = shareCardSvg({ weapon: "Torid <x>", mods: ["Serration", "Split Chamber"], rivenSlots: 1,
  riven: "+Critical Chance −Zoom", arcanes: ["Primary Merciless"], evolutions: [],
  claim: { headline: "79.3116 KPM", line: "Standard Single Target benchmark" } });
check("the card states the weapon, escaped", svg.includes("Torid &lt;x&gt;") && !svg.includes("<x>"));
check("…the result and where it was measured", svg.includes(">79.3116 KPM<") && svg.includes("Standard Single Target benchmark"));
check("…every mod, and the riven as a chip of its own", svg.includes(">Split Chamber<") && svg.includes(">Riven<") && svg.includes("+Critical Chance"));
const gone = await (await worker.fetch(new Request("https://wfsim.app/weapons/Torid/s/BBBBBBBBBB"), envP)).text();
check("an unknown id serves the weapon page unchanged", gone === HEAD);

// A MEASUREMENT, when the sharer sends one: stored beside the build, in the id,
// and stated in the preview — and nothing typed gets through.
const storeP = (w, c, m) => worker.fetch(new Request("https://wfsim.app/api/s", {
  method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ w, c, m }),
}), envP).then(async (r) => ({ status: r.status, j: await r.json() }));
const headOf = async (id) => (await (await worker.fetch(new Request(`https://wfsim.app/weapons/Torid/s/${id}`), envP)).text());
const ogOf = (html, p) => (html.match(new RegExp(`<meta (?:property|name)="${p}" content="([^"]*)"`)) || [])[1];

const official = await storeP("Torid", TINY, { s: "standard_single_target", k: "kpm", v: "79.3116" });
check("a build with a measurement is another link than the build alone",
  official.j.ok && official.j.id !== tiny.j.id, JSON.stringify(official));
check("…and a build without one keeps the id it always had", tiny.j.id === await shareId("Torid", TINY));
const oh = await headOf(official.j.id);
check("an official scenario's result heads the preview", ogOf(oh, "og:title") === "Torid build — 79.3116 KPM | WFSim", ogOf(oh, "og:title"));
check("…and is named as the benchmark it was measured in",
  (ogOf(oh, "og:description") || "").startsWith("79.3116 KPM in the Standard Single Target benchmark. Mods: Serration"),
  ogOf(oh, "og:description"));
const claimBack = await (await worker.fetch(new Request(`https://wfsim.app/api/s/${official.j.id}`), envP)).json();
check("…and comes back with the build when the link is opened", claimBack.m && claimBack.m.s === "standard_single_target" && claimBack.m.v === "79.3116");

const own = await storeP("Torid", TINY, { k: "dps", v: "12345.0000", e: "thrax_centurion", l: 9999, sp: 1, d: 180, name: "ignored" });
const ownHead = await headOf(own.j.id);
check("a fight of the sharer's own is stated by its terms, and marked as theirs",
  (ogOf(ownHead, "og:description") || "").startsWith("12345.0000 DPS vs Thrax Centurion Lv 9999 SP, 180 s — the sharer's own fight."),
  ogOf(ownHead, "og:description"));
const ownBack = await (await worker.fetch(new Request(`https://wfsim.app/api/s/${own.j.id}`), envP)).json();
check("…and a field nobody declared is not stored", ownBack.m && !("name" in ownBack.m), JSON.stringify(ownBack.m));

for (const [what, m] of [
  ["a value that is words", { k: "kpm", v: "the best", s: "standard_single_target" }],
  ["a scenario the site does not have", { k: "kpm", v: "1.0", s: "my_scenario" }],
  ["an enemy the site does not have", { k: "kpm", v: "1.0", e: "<b>x</b>", l: 1, d: 10 }],
  ["a level past the game's", { k: "kpm", v: "1.0", l: 10000, d: 10 }],
  ["a metric that is not an id", { k: "K P M", v: "1.0", s: "standard_single_target" }],
]) {
  const r = await storeP("Torid", TINY, m);
  check(`refused, and no link made: ${what}`, r.status === 400 && !r.j.ok, JSON.stringify(r));
}

// AND AGAINST THE REAL TABLE the build writes, with a code the page wrote: the
// generated codec and the names file have to agree with each other.
const real = resolve(dirname(fileURLToPath(import.meta.url)), "../site/share-names.json");
if (!existsSync(real)) check("site/share-names.json exists — run build_site_app.py", false);
else {
  const table = JSON.parse(readFileSync(real, "utf8"));
  const host = shareHostOf(table);
  useShareHost(host);
  const p = sharePreviewText(await decodeShare(CODE), table, host);
  check("a real code previews under its real weapon", p.title === "Dual Toxocyst build | WFSim", p.title);
  check("…with its mods and its riven named in English",
    /^Mods: [A-Z][^.]+\. Riven: \+[A-Z]/.test(p.description) && !/undefined|\b[a-z]+_[a-z]+\b/.test(p.description),
    p.description);
}

// Exercise both compact encoders with indexed parts as well as today's literal-id fallback.
const codecSource = readFileSync(new URL('../worker/share_codec.js', import.meta.url), 'utf8');
const codec = await import('data:text/javascript;base64,' + Buffer.from(codecSource
  + '\nexport { packV3, packV4 };').toString('base64'));
const ids = ['sepfahn_one_handed', 'peye', 'vargeet_ii_jai', 'tombfinger_secondary', 'haymaker', 'killstream'];
codec.useShareHost({ index: { to: new Map(ids.map((v, i) => [v, i])), from: new Map(ids.map((v, i) => [i, v])) },
  evoPrefixFor: () => '', rivenName: () => '' });
for (const [weapon, parts, expected] of [
  ['sepfahn_one_handed', ['peye', 'vargeet_ii_jai', 'link'], { grip: 'peye', link: 'vargeet_ii_jai' }],
  ['tombfinger_secondary', ['haymaker', 'killstream'], { grip: 'haymaker', loader: 'killstream' }],
]) {
  const payload = [2, weapon, 0, [], [], [], [], 0, 0, 0, 0, parts];
  for (const [version, pack] of [['3', codec.packV3], ['4', codec.packV4]]) {
    const packed = pack(payload);
    const back = await codec.decodeShare(version + packed);
    check(version + ' preserves ' + (parts[2] || 'loader') + ' as its component kind',
      JSON.stringify(back.assembly) === JSON.stringify(expected), JSON.stringify(back.assembly));
    if (version === '4' && parts.length === 2) {
      check('legacy Kitgun v4 bytes stay fixed', packed === '03~~~~~~~~0405', packed);
    }
  }
}

console.log(failures ? `\n${failures} failed` : "\nshort links store a build and nothing else");
process.exit(failures ? 1 : 0);
