// THE HOP NOBODY WAS WATCHING — `worker/index.js`, the one link in the chain
// that is neither the engine nor the page.
//
// A build reaches the board through three things: the PAGE builds a payload,
// the WORKER writes it to KV, the SCORER reads it back and validates it. The
// outer two are covered by `check_parity.mjs` and by
// `engine::builds::validate_for_board`; the middle one is where builds were
// being lost.
//
// TWICE, the same way. `mode` was sent by the page and never written down, so
// every Incarnon weapon's row said `cycle`; then `valence`, where seven Kuva
// Nukor submissions were refused on every scoring run since they arrived while
// the panel had told each submitter "sent" — dropped AFTER `/api/board/check`
// approved the payload carrying it.
//
// So this asserts the property rather than the two fields: EVERY KEY THE PAGE
// SENDS SURVIVES INTO STORAGE, and the key a record hashes to tells two builds
// apart whenever any of those keys differs. A third axis added tomorrow fails
// this without anyone remembering to come back.
//
//   node scripts/check_board_submit.mjs
import worker, { AXES, MAX_MODS } from "../worker/index.js";
import fs from "node:fs";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

// A KV stub that records what it was asked to store. The worker touches `put`,
// `get` — it asks whether a build is already held before bumping the counter —
// and `list`.
//
// `get` RETURNS A STRING, like KV does, and the counter is stored as one:
// a stub that handed back a number would let `Number.parseInt` be dropped and
// nothing would notice until the endpoint answered `null` in production.
const store = () => {
  const rows = new Map();
  return {
    rows,
    // A VALUE THAT IS NOT JSON IS STILL A VALUE. The build records are objects
    // and the assertions below read them as objects, but a supporter key holds
    // the empty string — so what does not parse is kept as it arrived.
    put: async (k, v) => { let p; try { p = JSON.parse(v); } catch { p = v; } rows.set(k, p); },
    // …AND GIVES BACK A STRING, like KV does. The rows are held parsed for the
    // assertions below; what `get` hands the worker is what KV would.
    get: async (k) => {
      if (!rows.has(k)) return null;
      const v = rows.get(k);
      return typeof v === "string" ? v : JSON.stringify(v);
    },
    // KV LISTS IN PAGES and a caller may page through them; the stub answers in
    // one page, which is the shape a store this size really has. The PREFIX is
    // honoured because the supporter count leans on it to leave its own counter
    // key out of what it is counting.
    list: async (o) => ({
      keys: [...rows.keys()]
        .filter((k) => !(o && o.prefix) || k.startsWith(o.prefix))
        .map((name) => ({ name })),
      list_complete: true,
    }),
  };
};

// A D1 STUB, AND IT IS THE WHOLE STORE NOW. `prepare(sql).bind(...).run()` and
// `.first()` are the whole surface the worker touches.
//
// IT KEEPS WHAT IT WAS WRITTEN, twice over: `rows` is the table as the
// assertions want to read it — identity to record, so "is this one row or two"
// is a question about the table and not about a call log — and `calls` is the
// statements, for the assertions that are about the SQL.
//
// `fail` makes every write throw, because the property that matters about an
// authoritative write is what the submitter is told when it does not land.
const database = (fail = false) => {
  const tables = new Map();
  const calls = [];
  const rows = new Map();
  tables.set("builds", rows);
  return {
    rows,
    tables,
    calls,
    prepare: (sql) => ({
      bind: (...args) => ({
        run: async () => {
          if (fail) throw new Error("D1 is down");
          calls.push({ sql, args });
          // ONE MAP PER TABLE, keyed the way the table is: the assertions ask
          // "how many rows does `builds` hold", which is a question about a
          // table and not about a call log.
          const into = sql.match(/INTO ([a-z_]+)/);
          if (into) {
            if (!tables.has(into[1])) tables.set(into[1], new Map());
            const t = tables.get(into[1]);
            t.set(args[0], into[1] === "builds" ? JSON.parse(args[2]) : args);
          }
          return { success: true };
        },
      }),
      first: async () => {
        if (fail) throw new Error("D1 is down");
        calls.push({ sql, args: [] });
        const from = sql.match(/COUNT\(\*\) AS n FROM ([a-z_]+)/);
        return from ? { n: (tables.get(from[1]) || new Map()).size } : null;
      },
    }),
  };
};

const post = async (body, db) =>
  worker.fetch(
    new Request("https://wfsim.app/api/board/submit", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    }),
    { LIBRARY: db, ASSETS: { fetch: async () => new Response("site") } },
  );

// THE PAYLOAD THE PAGE ACTUALLY SENDS, field for field — `boardPayload()` in
// web/src/static/app.js. An adversary weapon, because that is the case with the
// most axes on it.
const PAYLOAD = {
  benchmark: "single_target_v1",
  weapon: "kuva_nukor",
  mode: "base",
  mods: ["hornet_strike", "barrel_diffusion", "primed_target_cracker",
         "primed_heated_charge", "convulsion", "pathogen_rounds",
         "galvanized_diffusion", "galvanized_shot"],
  evolutions: [],
  arcanes: ["secondary_deadhead"],
  valence: "magnetic",
  // STATED AND EMPTY, exactly as the page sends them on a build with no riven.
  // The rule the endpoint applies to both is `valence`'s: an empty optional
  // axis is ABSENT rather than empty, so neither reaches storage here.
  riven_pos: [],
  riven_neg: "",
};

console.log("the board's submission endpoint\n");

// ---- 0. THE WORKER KNOWS EVERY AXIS THE PAGE SENDS ------------------------
//
// THE ASSERTION THAT WOULD HAVE CAUGHT BOTH LOSSES ON THE DAY THEY HAPPENED.
// Everything below tests the worker against a payload written HERE, which is
// only ever as current as this file — and this file did not exist when either
// axis was added. So the first thing checked is the two lists against each
// other: `boardPayload()` in the page is the definition of what a submission
// is, `AXES` is what the endpoint knows how to keep, and a name in one and not
// the other is the bug, before any request is made.
{
  const src = fs.readFileSync("web/src/static/app.js", "utf8");
  const body = src.slice(src.indexOf("function boardPayload()"));
  const ret = body.slice(body.indexOf("return {"), body.indexOf("\n  };"));
  // Keys of the returned literal sit at four spaces, as `name:` or bare
  // `name,`. Anything deeper belongs to a value expression, and a `//` line is
  // a comment — this file has more comment than code.
  const sent = [...ret.matchAll(/^ {4}([A-Za-z_$][\w$]*)\s*[:,]/gm)].map((m) => m[1]);
  const known = AXES.map((a) => a.key);
  const missing = sent.filter((k) => !known.includes(k));
  const extra = known.filter((k) => !sent.includes(k));
  check("the page sends a payload this file could read at all", sent.length > 3, sent.join(","));
  check("every axis the PAGE sends is an axis the WORKER knows",
    missing.length === 0, `worker has no: ${missing.join(", ")}   (page sends ${sent.join(", ")})`);
  check("...and the worker knows no axis the page never sends",
    extra.length === 0, `page never sends: ${extra.join(", ")}`);
}

// ---- 1. EVERY KEY SURVIVES ------------------------------------------------
{
  const kv = database();
  const res = await post(PAYLOAD, kv);
  const body = await res.json();
  check("a complete submission is accepted", res.ok && body.ok === true, JSON.stringify(body));
  check("...and exactly one record is written", kv.rows.size === 1, String(kv.rows.size));
  const rec = [...kv.rows.values()][0];
  // DERIVED, not listed: whatever the page sends is what has to arrive.
  //
  // AN EMPTY AXIS IS EXPECTED TO BE ABSENT, which is the endpoint's own rule
  // and not a concession — a record should no more carry an empty riven than
  // an empty valence, and the assertion below is the other half of it.
  const said = (v) => (Array.isArray(v) ? v.length > 0 : v !== "" && v != null);
  const lost = Object.keys(PAYLOAD).filter(
    (k) => said(PAYLOAD[k]) && JSON.stringify(rec[k]) !== JSON.stringify(PAYLOAD[k]),
  );
  check("every axis the page sends is written down", lost.length === 0,
    `lost: ${lost.join(", ")} — stored ${JSON.stringify(rec)}`);
  const kept_empty = Object.keys(PAYLOAD).filter((k) => !said(PAYLOAD[k]) && k in rec);
  check("...and an axis it has nothing for is absent, not empty",
    kept_empty.length === 0, `stored empty: ${kept_empty.join(", ")}`);
}

// ---- 2. THE KEY TELLS TWO BUILDS APART ------------------------------------
//
// One per axis, walked rather than spelled out. Two submissions differing in
// ANY axis must land on two keys — a shared key is a silent overwrite, and the
// build that loses is the one the player submitted second.
{
  const variants = {
    weapon: "kuva_bramma",
    mode: "cycle",
    valence: "toxin",
    arcanes: ["secondary_encumber"],
    evolutions: ["laetum_devastating_attrition"],
    // The ORDER is the build: the same mods in two orders combine to two
    // different elements, so this must key differently too.
    mods: [PAYLOAD.mods[1], PAYLOAD.mods[0], ...PAYLOAD.mods.slice(2)],
    // A RIVEN'S SHAPE. Two players who rolled different stats did not submit
    // the same build, and a key that could not tell them apart would file the
    // second under the first's number — the failure this file exists for.
    riven_pos: ["critical_damage", "multishot"],
    riven_neg: "recoil",
  };
  for (const [axis, value] of Object.entries(variants)) {
    const kv = database();
    await post(PAYLOAD, kv);
    await post({ ...PAYLOAD, [axis]: value }, kv);
    check(`two builds differing only in \`${axis}\` are two records`,
      kv.rows.size === 2, `${kv.rows.size} record(s): ${[...kv.rows.keys()].join("  |  ")}`);
  }
  // ...and the control: the SAME build twice is one record, or the board would
  // fill with duplicates of whoever pressed the button twice.
  const kv = database();
  await post(PAYLOAD, kv);
  await post(PAYLOAD, kv);
  check("the same build twice is one record", kv.rows.size === 1, String(kv.rows.size));
}

// ---- 3. AN ORDINARY WEAPON IS UNAFFECTED ----------------------------------
//
// Most of the roster has no valence, and it sends the field EMPTY rather than
// omitting it. A shape guard written as `!== undefined` would have turned the
// whole roster away — which is the failure mode of fixing this in a hurry.
{
  const kv = database();
  const plain = { ...PAYLOAD, weapon: "torid", valence: "" };
  const res = await post(plain, kv);
  check("a weapon with no valence still submits", res.ok, String(res.status));
  const rec = [...kv.rows.values()][0];
  check("...and carries no empty valence into storage", rec && !("valence" in rec),
    JSON.stringify(rec));
}

// ---- 4. MALFORMED INPUT IS STILL TURNED AWAY ------------------------------
{
  const kv = database();
  const res = await post({ ...PAYLOAD, valence: "not an id!" }, kv);
  check("a malformed valence is rejected", !res.ok && kv.rows.size === 0, String(res.status));
}

// ---- 5. A BENCHMARK THE WORKER HAS NEVER HEARD OF -------------------------
//
// THERE WILL BE MANY RULERS. `single_target` was alone for months, then a
// companion, then `group_clear` — and the point of this block is
// that adding the FOURTH costs nothing here. The worker validates `benchmark`
// as an ID and holds no LIST of them, which is what makes a ruler a data file
// rather than a deploy; this asserts it stays that way.
//
// A NAME NOBODY HAS SEEN, deliberately: submitting to an existing ruler would
// prove only that the existing rulers work.
{
  const kv = database();
  const fresh = { ...PAYLOAD, benchmark: "a_ruler_invented_by_this_check_v3" };
  const res = await post(fresh, kv);
  check("a benchmark the worker has never heard of is accepted", res.ok, String(res.status));
  const rec = [...kv.rows.values()][0];
  check("...and reaches storage under its own name",
    rec && rec.benchmark === fresh.benchmark, JSON.stringify(rec && rec.benchmark));

  // …AND ONE BUILD IS ONE RECORD, whichever ruler it arrived from. Two rulers
  // scoring one build are two records only while the identity key carries the
  // benchmark, which is right only while a submission is bound to the fight it
  // was measured under. It is not: the store is a LIBRARY OF BUILDS and every
  // ruler
  // crosses the whole of it, so the ruler is provenance (`identity: false`) and
  // the same build arriving from two fights is the same build.
  //
  // LAST WRITE WINS, which is what "the same build always produces the same
  // key" has always meant here — a resubmission overwrites the record rather
  // than adding one. That is the right way round for the two fields that are
  // not the build: `at` is the day, so a build somebody is still submitting
  // stays current, and `benchmark` is only where the last submitter happened to
  // be standing. Neither is ranked.
  await post(PAYLOAD, kv);
  check("...and the same build from another ruler is the SAME record",
    kv.rows.size === 1, `${kv.rows.size} records`);
  const only = [...kv.rows.values()][0];
  check("...still carrying a provenance, and the same build",
    !!only && !!only.benchmark && only.weapon === PAYLOAD.weapon,
    JSON.stringify(only && { benchmark: only.benchmark, weapon: only.weapon }));

  // THE NEGATIVE CONTROL FOR THAT COLLAPSE: dropping the ruler must not make
  // every build one record. A build with no ruler at all is legal now — that is
  // what an upload from a scenario of the player's own is — and it is still a
  // record of its own build.
  const kv2 = database();
  const nameless = { ...PAYLOAD };
  delete nameless.benchmark;
  const res2 = await post(nameless, kv2);
  check("a build uploaded from no ruler at all is accepted", res2.ok, String(res2.status));
  await post({ ...nameless, weapon: "braton_prime" }, kv2);
  check("...and two different builds are still two records",
    kv2.rows.size === 2, `${kv2.rows.size} records`);
}

// ---- and the library can say how big it is -----------------------------------
//
// THE BOARD IS A STATIC FILE and always will be — committed to the repo, served
// from the CDN, unblockable and free. What it cannot carry is how far behind it
// is, so the page asks the library for the one number that says: how many builds
// it holds.
//
// ONE QUERY. It was a walk of a key namespace metered a thousand operations a
// DAY, and then a counter key with an hourly corrector to avoid the walk. Both
// are gone, and with them every way the count could disagree with the table.
{
  const db = database();
  const ask = async () => {
    const r = await worker.fetch(
      new Request("https://wfsim.app/api/board/pending"),
      { LIBRARY: db, ASSETS: { fetch: async () => new Response("site") } },
    );
    return r.ok ? (await r.json()).count : `not ok ${r.status}`;
  };
  check("an empty library says so", (await ask()) === 0, String(await ask()));
  await post(PAYLOAD, db);
  await post({ ...PAYLOAD, weapon: "braton_prime" }, db);
  check("...and otherwise counts what it holds", (await ask()) === 2, String(await ask()));

  // A RESUBMISSION IS NOT A SECOND BUILD. The key IS the build, so the count
  // cannot drift up for ever the way a bumped counter could.
  await post(PAYLOAD, db);
  check("...and a resubmission does not move it", (await ask()) === 2, String(await ask()));

  const res = await worker.fetch(
    new Request("https://wfsim.app/api/board/pending"),
    { LIBRARY: db, ASSETS: { fetch: async () => new Response("site") } },
  );
  const body = res.ok ? await res.json() : null;
  check("...and reports nothing else about it",
    !!body && Object.keys(body).sort().join(",") === "capped,count,ok",
    JSON.stringify(body && Object.keys(body)));
  const post_ = await worker.fetch(
    new Request("https://wfsim.app/api/board/pending", { method: "POST" }),
    { LIBRARY: db, ASSETS: { fetch: async () => new Response("site") } },
  );
  check("...and it is a READ, so a POST is refused", post_.status === 405,
    String(post_.status));

  // A DATABASE THAT WILL NOT ANSWER SAYS "UNKNOWN", never a number. The page
  // draws no footnote on a null and a wrong number is worse than none.
  const dead = database(true);
  const r = await worker.fetch(
    new Request("https://wfsim.app/api/board/pending"),
    { LIBRARY: dead, ASSETS: { fetch: async () => new Response("site") } },
  );
  const j = r.ok ? await r.json() : null;
  check("a database that will not answer says it does not know",
    !!j && j.ok === true && j.count === null, JSON.stringify(j));
}


// ---- and the supporter count is the same shape ------------------------------
//
// THE SAME QUESTION, THE SAME ANSWER. A supporter row holds a Ko-fi message id
// and a DAY and cannot hold more — no amount, no name, no email — so the count
// is the only figure about this project's funding that exists, and it is a
// `COUNT(*)` like the library's.
//
// IDEMPOTENT ON THE MESSAGE ID, because Ko-fi redelivers what it did not see
// acknowledged and a retry must not be a second supporter.
{
  const db = database();
  const env = { LIBRARY: db, KOFI_TOKEN: "t", ASSETS: { fetch: async () => new Response("site") } };
  const ask = async () => {
    const r = await worker.fetch(new Request("https://wfsim.app/api/support/count"), env);
    return r.ok ? (await r.json()).count : `not ok ${r.status}`;
  };
  const kofi = async (id) => worker.fetch(
    new Request("https://wfsim.app/api/support/kofi", {
      method: "POST",
      body: new URLSearchParams({
        data: JSON.stringify({ verification_token: "t", message_id: id }),
      }),
    }),
    env,
  );
  check("an empty supporter table counts zero", (await ask()) === 0, String(await ask()));
  await kofi("bbbbbbbb-1");
  await kofi("bbbbbbbb-2");
  check("...and two deliveries count two", (await ask()) === 2, String(await ask()));
  await kofi("bbbbbbbb-1");
  check("...and a redelivery of the same message does not",
    (await ask()) === 2, String(await ask()));

  // THE TOKEN IS THE WHOLE OF THE AUTHENTICATION, so a payload without it is
  // refused before anything is written: an endpoint that counts anonymous POSTs
  // is a counter anybody can drive.
  const bogus = await worker.fetch(
    new Request("https://wfsim.app/api/support/kofi", {
      method: "POST",
      body: new URLSearchParams({
        data: JSON.stringify({ verification_token: "wrong", message_id: "cccccccc-1" }),
      }),
    }),
    env,
  );
  check("a delivery with the wrong token is refused", bogus.status === 403,
    String(bogus.status));
  check("...and counted as nothing", (await ask()) === 2, String(await ask()));
}


// ---- THE LIMIT IS THE ENGINE'S, and this file cannot derive it ----------
//
// `MAX_MODS` is `MAIN_SLOTS + 1`: eight main slots and the STANCE, the one card
// that rides `mods` rather than a key of its own. The worker has no game data,
// so nothing but this line stops the two drifting — and a worker one short
// refuses every full MELEE build with "bad mods", which is a legal build lost
// at the one hop neither the engine nor the page is watching.
{
  const src = fs.readFileSync(new URL("../engine/src/builds.rs", import.meta.url), "utf8");
  const m = src.match(/pub const MAIN_SLOTS: usize = (\d+);/);
  const mainSlots = m ? Number(m[1]) : NaN;
  check("the engine's MAIN_SLOTS is readable", Number.isFinite(mainSlots), String(mainSlots));
  check(
    "the worker's mod limit is the engine's main slots plus the stance",
    MAX_MODS === mainSlots + 1,
    `worker ${MAX_MODS}, engine ${mainSlots} + 1`,
  );
}

// ---- ...AND THE DEPLOYED ONE IS THIS ONE --------------------------------
//
// `site/` deploys on a push and the worker does NOT, so the code above can be
// right while wfsim.app runs last month's. Probed WITHOUT WRITING: the shape
// pass walks `AXES` in order and stops at the first bad field, so a payload
// carrying a full mod list AND a deliberately malformed arcane answers "bad
// mods" from a worker whose limit is too low and "bad arcanes" from one that
// agrees with this file. Nothing is stored either way.
//
// SKIPPED WHEN THE NETWORK IS NOT THERE. It is a fact about the DEPLOYMENT,
// not about the tree, so an offline run must not fail on it.
if (process.env.WFSIM_SKIP_LIVE !== "1") {
  const ids = Array.from({ length: MAX_MODS }, (_, i) => `probe_mod_${i}`);
  const ask = async (mods) => {
    const r = await fetch("https://wfsim.app/api/board/submit", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ weapon: "praedos", mods, arcanes: ["NOT A VALID ID"] }),
    });
    return (await r.json()).error;
  };
  try {
    const full = await ask(ids);
    const over = await ask([...ids, "probe_mod_over"]);
    check(
      `the deployed worker takes ${MAX_MODS} mods`,
      full === "bad arcanes",
      `answered ${JSON.stringify(full)} - a worker limited below ${MAX_MODS} says "bad mods"`,
    );
    check(
      `...and refuses ${MAX_MODS + 1}`,
      over === "bad mods",
      `answered ${JSON.stringify(over)}`,
    );
  } catch (e) {
    console.log(`  --   the deployed worker was unreachable (${e.message}) - not asked`);
  }
}

console.log(
  failures
    ? `\n${failures} failed`
    : "\nevery axis a build has survives the hop into storage",
);
// EXIT CODE, NOT `process.exit`. The live probe above leaves a keep-alive
// socket in fetch's pool, and tearing the process down on top of it aborts node
// with a libuv assertion — a check that CRASHES on a clean run reports 127,
// which reads as a failure of the thing it was checking. Setting the code lets
// the loop drain and exit on its own.
process.exitCode = failures ? 1 : 0;
