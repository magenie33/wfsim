// THE BOT'S CARD IS THE PAGE'S CARD, pixel for pixel — docs/AGENT.md §"The QQ
// bot". The bot draws a weapon's long image itself (`bot/card.mjs`, the card
// page's own `headlessLongCard`) and Chrome only rasterizes the document; the
// page draws it on `/weapons/<Name>/card`. Each fixture is taken both ways in
// each language and the two PNGs must be the same bytes: a wrapper that drifts
// from index.html, a style or picture the document lost, a number the bot reads
// from other rows, each moves a pixel. An appraisal's answer is replayed both
// ways (`headlessAppraisalReplay`, on the page's fleet and on the bot's
// threads) and the two verdicts must be equal, field for field.
//
//   node scripts/check_card_document.mjs              serves site/ itself
//   WFSIM_BASE=http://127.0.0.1:<port> node scripts/check_card_document.mjs
import { createServer } from "node:http";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { CHROME, serveSite } from "./cdp.mjs";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
process.env.CHROME = CHROME;
const { renderCardWith, renderDocument } = await import("../bot/render.mjs");
const { loadEngine } = await import("../bot/engine.mjs");
const { cardDocument } = await import("../bot/card.mjs");

/// Every shape a chat asks for: a plain gun, riven builds, the analyst with and
/// without a filter, a melee, a weapon with modes, an Incarnon, a kitgun, and an
/// appraisal's way in and its answer.
const FIXTURES = [
  "Torid/card?kind=pz",
  "Torid/card?kind=pz&rv=1",
  "Torid/card?kind=zk",
  "Torid/card?kind=zk&has=critical_chance&malus=any",
  "Laetum/card?kind=zk&n=5",
  "Nikana_Prime/card?kind=pz",
  "Haalvu/card?kind=pz",
  "Catchmoon/card?kind=pz",
  "Laetum/card?kind=appraise&code=CHECK1",
  "Laetum/card?kind=appraise&code=CHECK1&result=1",
];
/// A PAGE PICTURE THAT FAILED TO ARRIVE is hidden, not an error, so a page shot
/// that differs is taken again before it counts.
const PAGE_TRIES = 3;

const srv = process.env.WFSIM_BASE ? null : await serveSite(join(ROOT, "site"));
const BASE = process.env.WFSIM_BASE || `http://127.0.0.1:${srv.address().port}`;
const engine = await loadEngine({ site: BASE });

// THE APPRAISAL, answered on the wire: the asker's card is a riven row of the
// board under test, rolls and all, and the build that came back is that row's.
const primary = (engine.meta.benchmarks.find((b) => b.primary) || engine.meta.benchmarks[0]).id;
const row = ((await engine.host.board("laetum")) || []).find((x) => x.riven && x.benchmark === primary);
if (!row) throw new Error(`no laetum riven row under ${primary} on ${BASE}`);
const rv = row.riven;
const JOB = { ok: true, code: "CHECK1", weapon: "laetum", ruler: primary, at: 0, done: true,
  riven: { bonuses: rv.bonuses.map((id, i) => ({ id, roll: rv.rolls[i] })),
    malus: rv.malus ? { id: rv.malus, roll: rv.rolls[rv.bonuses.length] } : null, rank: 7 } };
const RESULT = { id: 1, thanks: "check", build: { benchmark: primary, weapon: "laetum", mode: row.mode, mods: row.mods,
  evolutions: row.evolutions, arcanes: row.arcanes, exilus: row.exilus, valence: row.valence,
  riven_pos: [...rv.bonuses].sort(), riven_neg: rv.malus || "" } };
const appraisal = async (code, result) => (code === JOB.code ? { ...JOB, ...(result === RESULT.id ? { result: RESULT } : {}) } : null);

// …THROUGH A DOOR IN FRONT OF THE SITE: the appraisal from here, the rest from it.
const door = createServer(async (q, s) => {
  const u = new URL(q.url, BASE);
  const m = /^\/api\/appraise\/([^/]+)$/.exec(u.pathname);
  if (m) {
    const job = await appraisal(m[1], Number(u.searchParams.get("result")) || 0);
    s.writeHead(job ? 200 : 404, { "content-type": "application/json" });
    return s.end(JSON.stringify(job || { ok: false }));
  }
  const body = ["GET", "HEAD"].includes(q.method) ? undefined : await new Promise((ok) => {
    const parts = []; q.on("data", (c) => parts.push(c)); q.on("end", () => ok(Buffer.concat(parts)));
  });
  const r = await fetch(new URL(u.pathname + u.search, BASE), { method: q.method, body,
    headers: q.headers["content-type"] ? { "content-type": q.headers["content-type"] } : {} });
  const headers = Object.fromEntries([...r.headers].filter(([k]) => !["content-encoding", "content-length", "transfer-encoding"].includes(k)));
  s.writeHead(r.status, headers);
  s.end(Buffer.from(await r.arrayBuffer()));
});
await new Promise((ok) => door.listen(0, "127.0.0.1", ok));
const PAGE = `http://127.0.0.1:${door.address().port}`;

// THE STYLE THE PAGE UNDER TEST LINKS, so a dev server's unbuilt one is compared too.
const shell = await (await fetch(`${BASE}/`)).text();
const href = (/<link rel="stylesheet" href="([^"]+)"/.exec(shell) || [])[1];
if (!href) throw new Error(`no stylesheet linked from ${BASE}/`);
const css = await (await fetch(new URL(href, BASE))).text();
const img = join(ROOT, "site", "img");

/// TWO VERDICTS ARE ONE when every field is, the raw numbers to their last bits
/// but one: the page's fleet folds its shards in whatever pieces its lanes took,
/// and a float sum grouped otherwise moves the final bit (the bot folds a run at
/// a time, bot/fleet.mjs). What is printed — `shown`, the grade — is exact.
const RAW = ["score", "gain"];
const sameVerdict = (a, b) => !a === !b && (!a || Object.keys({ ...a, ...b }).every((k) => (RAW.includes(k)
  && typeof a[k] === "number" && typeof b[k] === "number" ? Math.abs(a[k] - b[k]) <= 1e-12 * Math.max(1, Math.abs(a[k]))
  : JSON.stringify(a[k]) === JSON.stringify(b[k]))));

const out = mkdtempSync(join(tmpdir(), "wfsim-card-document-"));
let failures = 0, compared = 0;
const fail = (what) => { failures++; console.log(`  FAIL  ${what}`); };
for (const f of FIXTURES) for (const lang of ["zh", "en"]) {
  const url = `${PAGE}/weapons/${f}`;
  let page = null, doc = null, same = false;
  for (let i = 0; i < PAGE_TRIES && !same; i++) {
    page = await renderCardWith(url, { lang, wait: 600_000 });
    // A VERDICT IS DATED BY ITS REPLAY: the bot's is dated by the page's, so the
    // two cards print one time.
    const at = page.verdict && page.verdict.at;
    doc = await cardDocument(engine, url, { lang, css, img, appraisal, ...(at ? { now: () => new Date(at) } : {}) });
    same = Buffer.compare(page.png, await renderDocument(doc.html)) === 0;
  }
  compared++;
  const tag = `${f.replace(/[^a-z0-9]+/gi, "_")}_${lang}`;
  if (!sameVerdict(page.verdict, doc.verdict)) {
    fail(`${f} (${lang}): the bot's verdict is not the page's\n        page ${JSON.stringify(page.verdict)}\n        bot  ${JSON.stringify(doc.verdict)}`);
  }
  if (f.includes("result=") && !(doc.verdict && doc.verdict.ok)) fail(`${f} (${lang}): the replay did not answer (${JSON.stringify(doc.verdict)})`);
  if (same) continue;
  writeFileSync(join(out, `${tag}.page.png`), page.png);
  writeFileSync(join(out, `${tag}.bot.png`), await renderDocument(doc.html));
  fail(`${f} (${lang}): the bot's picture is not the page's — ${out}/${tag}.{page,bot}.png`);
}
door.close();
srv && srv.close();
console.log(failures ? `${failures} failures over ${compared} cards` : `ok  ${compared} cards, the bot's picture and verdict are the page's`);
process.exit(failures ? 1 : 0);
