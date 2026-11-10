// THE HOP NOBODY WAS WATCHING — `worker/index.js`, the one link in the chain
// that is neither the engine nor the page.
//
// A build reaches the board through three things: the PAGE builds a payload,
// the WORKER writes it to KV, the SCORER reads it back and validates it. The
// outer two are covered by `check_parity.mjs` and by
// `engine::board::builds::validate_for_board`; the middle one is where builds were
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
import worker, { AXES, MAX_MODS, BOARD_SUBMIT_BATCH_MAX, BOARD_SUBMIT_BATCH_BYTES } from "../worker/index.js";
import { initiatorCode, hmac } from "../worker/accounts.js";
import { USER_TASK_KEEP_MS } from "../worker/user_tasks.js";
import { questionId } from "../worker/task_contract.js";
import fs from "node:fs";
import { appSource } from "./app_source.mjs";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

// A LIBRARY STUB, AND IT IS THE WHOLE STORE NOW. `prepare(sql).bind(...).run()` and
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
  tables.set("inbox", rows);
  return {
    rows,
    tables,
    calls,
    prepare: (sql) => ({
      bind: (...args) => ({
        run: async () => {
          if (fail) throw new Error("the library is down");
          calls.push({ sql, args });
          // ONE MAP PER TABLE, keyed the way the table is: the assertions ask
          // "how many rows does `builds` hold", which is a question about a
          // table and not about a call log.
          const into = sql.match(/INTO ([a-z_]+)/);
          if (into) {
            if (!tables.has(into[1])) tables.set(into[1], new Map());
            const t = tables.get(into[1]);
            if (into[1] === "inbox") {
              const width = sql.includes("family_source") ? 5 : 4;
              for (let at = 0; at < args.length; at += width) t.set(args[at], JSON.parse(args[at + 2]));
            } else t.set(args[0], args);
          }
          return { success: true };
        },
      }),
      first: async () => {
        if (fail) throw new Error("the library is down");
        calls.push({ sql, args: [] });
        // EVERY TABLE THE STATEMENT COUNTS, summed. The library is `builds`
        // plus what has arrived and not been through intake yet, so the count
        // is a sum and a stub that answered one table would make the second
        // half untestable.
        const of = [...sql.matchAll(/COUNT\(\*\)(?: AS n)? FROM ([a-z_]+)/g)].map((m) => m[1]);
        if (!of.length) return null;
        return { n: of.reduce((n, t) => n + (tables.get(t) || new Map()).size, 0) };
      },
    }),
  };
};

const post = async (body, db, options = {}) =>
  worker.fetch(
    new Request("https://wfsim.app/api/board/submit", {
      method: "POST",
      headers: { "content-type": "application/json", ...options.headers },
      body: JSON.stringify(body),
    }),
    { LIBRARY: db, ASSETS: { fetch: async () => new Response("site") }, ...options.env },
  );

// THE PAYLOAD THE PAGE ACTUALLY SENDS, field for field — `boardPayload()` in
// web/src/static/app/. An adversary weapon, because that is the case with the
// most axes on it.
const PAYLOAD = {
  benchmark: "standard_single_target_v1",
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
  const src = appSource();
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

// ---- 2. THE DOOR KEEPS NO KEY OF ITS OWN ----------------------------------
//
// Telling two builds apart needs the mod POOL — an elemental card enters the
// element sequence and a plain one does not, and the sequence decides the
// pairing — and this service has no game data. It stores the record verbatim
// under an id that means nothing, and `wfsim-intake` derives the real key with
// the engine. Two builds differing in one axis are two rows, and one build
// twice is one row: both are asserted in that binary's own tests, where the
// answer is the engine's.
//
// The failure this refuses: a key computed here would be a SECOND answer to the
// question that must have one, from the half with no evidence. The last one had
// `mode` in it, so the same cards sent from two modes were two rows.
{
  const kv = database();
  await post(PAYLOAD, kv);
  await post(PAYLOAD, kv);
  check("the same build twice is two queue rows, not one",
    kv.rows.size === 2, `${kv.rows.size} row(s)`);
  const keys = [...kv.rows.keys()];
  check("...under ids that say nothing about the build",
    keys.every((k) => /^[0-9a-f-]{36}$/i.test(k)), keys.join("  |  "));
  const sql = kv.calls.map((c) => c.sql).join(" ");
  check("...and nothing is written to `builds`",
    !/INTO entity_weapon_builds/.test(sql) && /INTO record_build_submissions/.test(sql), sql);
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
// THERE WILL BE MANY RULERS. `standard_single_target` was alone for months, then a
// companion, then `standard_multi_target` — and the point of this block is
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

  // …AND THE RULER IS NOT BAKED INTO WHAT IS STORED. The store is a LIBRARY OF
  // BUILDS and every ruler crosses the whole of it, so where the submitter was
  // standing is PROVENANCE — the same cards from two fights are one build, and
  // it is `wfsim-intake` that says so, from records that differ only in a field
  // no build has.
  await post(PAYLOAD, kv);
  const rows = [...kv.rows.values()];
  check("...and a second submission of it is a second queue row",
    rows.length === 2, `${rows.length} rows`);
  check("...differing in the ruler and in nothing a build is made of",
    rows[0].benchmark !== rows[1].benchmark
      && JSON.stringify(rows.map((r) => [r.weapon, r.mods]))
        === JSON.stringify(rows.map(() => [PAYLOAD.weapon, PAYLOAD.mods])),
    JSON.stringify(rows));

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

  // A RESUBMISSION MOVES IT, and that is the honest answer at the door. The
  // sentence this feeds is "N have arrived since this board was scored", and
  // what has arrived is what has arrived: whether two of them are one build is
  // a question about the mod pool, which is `wfsim-intake`'s and not this
  // service's.
  await post(PAYLOAD, db);
  check("...and every arrival is counted, because that is what it counts",
    (await ask()) === 3, String(await ask()));

}


// ---- A MOD BELOW MAX RANK IS AN ID OF ITS OWN ----------------------------
//
// `<card>@<rank>` (`engine::data::mods::RANK_MARK`) rides `mods` and `exilus`,
// and no other axis: an arcane's rank is scored at its ceiling.
{
  const db = database();
  const ranked = await post({ ...PAYLOAD, mods: ["hunter_track@0", ...PAYLOAD.mods.slice(1)] }, db);
  const kept = [...db.rows.values()].map((r) => r.mods).flat();
  check("a ranked mod id is taken and kept", ranked.status === 200 && kept.includes("hunter_track@0"),
    `${ranked.status} ${JSON.stringify(kept)}`);
  const arc = await post({ ...PAYLOAD, arcanes: ["secondary_deadhead@2"] }, database());
  check("...and a ranked arcane id is refused", arc.status === 400, String(arc.status));
}

// ---- THE LIMIT IS THE ENGINE'S, and this file cannot derive it ----------
//
// `MAX_MODS` is `MAIN_SLOTS + 1`: eight main slots and the STANCE, the one card
// that rides `mods` rather than a key of its own. The worker has no game data,
// so nothing but this line stops the two drifting — and a worker one short
// refuses every full MELEE build with "bad mods", which is a legal build lost
// at the one hop neither the engine nor the page is watching.
{
  const src = fs.readFileSync(new URL("../engine/src/board/builds/axes.rs", import.meta.url), "utf8");
  const m = src.match(/pub const MAIN_SLOTS: usize = (\d+);/);
  const mainSlots = m ? Number(m[1]) : NaN;
  check("the engine's MAIN_SLOTS is readable", Number.isFinite(mainSlots), String(mainSlots));
  check(
    "the worker's mod limit is the engine's main slots plus the stance",
    MAX_MODS === mainSlots + 1,
    `worker ${MAX_MODS}, engine ${mainSlots} + 1`,
  );
}

// Each batch uses the same record door and one atomic inbox write.
{
  const db = database();
  const first = { ...PAYLOAD, score: 999999, initiator: "system", id: "chosen", family_source: "fake" };
  const second = { ...PAYLOAD, weapon: "braton_prime", mods: ["serration", "split_chamber"], valence: "" };
  const response = await post({ builds: [first, { ...PAYLOAD, valence: "not an id!" }, second], initiator: "system" }, db);
  const result = await response.json();
  check("a mixed batch acknowledges each item in input order", response.ok && JSON.stringify(result.results) === JSON.stringify([
    { ok: true }, { ok: false, error: "bad valence", retryable: false }, { ok: true },
  ]), JSON.stringify(result));
  check("the batch writes all accepted records in one statement", db.rows.size === 2 && db.calls.length === 1,
    `${db.rows.size} rows, ${db.calls.length} statements`);
  const stored = [...db.rows.values()];
  check("batch records retain their own build axes", stored[0]?.weapon === PAYLOAD.weapon && stored[1]?.weapon === second.weapon
    && JSON.stringify(stored[0].mods) === JSON.stringify(PAYLOAD.mods) && JSON.stringify(stored[1].mods) === JSON.stringify(second.mods), JSON.stringify(stored));
  check("batch submissions carry no score or caller-supplied provenance", stored.every(record =>
    !["score", "initiator", "id", "family_source"].some(key => key in record))
    && db.calls[0].args.filter((_, index) => index % 4 === 3).every(value => value === "anonymous"), JSON.stringify(db.calls));
  check("the batch writes only the ordinary inbox", db.calls.every(call => /INSERT INTO record_build_submissions /.test(call.sql)
    && !/INTO (builds|scores)/.test(call.sql)), JSON.stringify(db.calls));
}
{
  let sessionReads = 0;
  const env = { AUTH_SECRET: "submission-check-only", ACCOUNTS: { prepare: () => ({ bind: () => ({ first: async () => {
    sessionReads++; return { account: "reader-account" };
  } }) }) } };
  const db = database();
  const response = await post({ builds: [PAYLOAD, PAYLOAD], initiator: "bot:forged" }, db,
    { headers: { cookie: "wfsim_session=reader-token" }, env });
  const expected = await initiatorCode(env, "user", "reader-account");
  check("one signed-in session determines the entire batch's initiator", response.ok && sessionReads === 1
    && db.calls[0]?.args.filter((_, index) => index % 4 === 3).every(value => value === expected), JSON.stringify(db.calls));
  check("duplicate builds remain separate inbox arrivals", db.rows.size === 2 && [...db.rows.keys()].every(key => /^[0-9a-f-]{36}$/i.test(key)), String(db.rows.size));
}
{
  const db = database();
  const full = Array.from({ length: BOARD_SUBMIT_BATCH_MAX }, () => PAYLOAD);
  const first = await post({ builds: full }, db);
  const second = await post({ builds: full }, db);
  check("the batch bound is per request, not a total build cap", first.ok && second.ok
    && db.rows.size === BOARD_SUBMIT_BATCH_MAX * 2 && db.calls.length === 2, `${db.rows.size} rows, ${db.calls.length} statements`);
  const tooMany = await post({ builds: [...full, PAYLOAD] }, database());
  check("a batch over its item bound is rejected", tooMany.status === 413, String(tooMany.status));
  const tooLarge = await post({ builds: [PAYLOAD], padding: "界".repeat(Math.ceil(BOARD_SUBMIT_BATCH_BYTES / 2)) }, database());
  check("the batch body bound counts UTF-8 bytes", tooLarge.status === 413, String(tooLarge.status));
  for (const builds of [[], null, {}, "bad"]) {
    const response = await post({ builds }, database());
    check(`an invalid batch envelope is refused (${JSON.stringify(builds)})`, response.status === 400, String(response.status));
  }
  const malformed = await post({ builds: [null, 4, [], { ...PAYLOAD, padding: "x".repeat(4096) }] }, database());
  const refused = await malformed.json();
  check("invalid batch items are permanently refused without breaking their siblings", malformed.ok
    && refused.results.length === 4 && refused.results.every(item => item.ok === false && item.retryable === false), JSON.stringify(refused));
  const failed = await post({ builds: [PAYLOAD, PAYLOAD] }, database(true));
  check("a failed batch write is retryable and acknowledges no item", failed.status === 503 && !(await failed.json()).results, String(failed.status));
}

// The receipt retains only the authority proved by a search lease or appraisal.
const HARVEST_LEASE = "a".repeat(32), HARVEST_VERIFIER = "b".repeat(24), HARVEST_QUESTION = "c".repeat(32);
function harvestFixture(producer = "user_calculation") {
  const db = database(), original = db.prepare;
  db.held = true;
  db.leaseUntil = Date.now() + 60_000;
  db.search = { id: HARVEST_QUESTION, verb: "optimize_report", request: JSON.stringify({ weapon: PAYLOAD.weapon, strategy: "quick", __board_upload: true }),
    producer, ref: "trusted_transfer", initiator: "user:original_asker" };
  db.appraisal = { code: "TEST7", channel: "qq", asker: "original-chat-asker", weapon: PAYLOAD.weapon,
    engine: "check-engine", at: new Date().toISOString(),
    request: JSON.stringify({ request: { weapon: PAYLOAD.weapon, strategy: "quick" }, context: { weapon: PAYLOAD.weapon } }) };
  db.prepare = sql => {
    if (sql.startsWith("SELECT q.id, q.verb")) return { bind: (...args) => ({ first: async () => {
      db.calls.push({ sql, args });
      return db.held && Date.parse(args[2]) <= db.leaseUntil && args[0] === HARVEST_LEASE && args[1] === HARVEST_VERIFIER
        && (!args[3] || args[3] === db.search.id) ? db.search : null;
    } }) };
    if (sql.startsWith("SELECT code, channel, asker, weapon, engine")) return { bind: code => ({ first: async () => {
      db.calls.push({ sql, args: [code] }); return code === db.appraisal.code ? db.appraisal : null;
    } }) };
    return original(sql);
  };
  const env = { AUTH_SECRET: "submission-harvest-check-only", ASSETS: { fetch: async () => Response.json({ engine: "check-engine" }) } };
  const source = { lease: HARVEST_LEASE, verifier: HARVEST_VERIFIER, question: HARVEST_QUESTION, engine: "check-engine" };
  return { db, env, source, post: body => post(body, db, { env }) };
}
{
  const fixture = harvestFixture();
  const issuedAt = Date.now();
  const response = await fixture.post({ builds: [], harvest: { ...fixture.source, initiator: "system" } });
  const granted = await response.json();
  check("a live search lease grants delivery without writing a build", response.ok && typeof granted.receipt === "string"
    && granted.question === HARVEST_QUESTION && granted.weapon === PAYLOAD.weapon && fixture.db.rows.size === 0, JSON.stringify(granted));
  check("the receipt uses the existing user-task retention window", Math.abs(Date.parse(granted.expires_at) - issuedAt - USER_TASK_KEEP_MS) < 1000,
    granted.expires_at);
  fixture.db.held = false;
  const harvest = { receipt: granted.receipt, question: granted.question, engine: granted.engine, initiator: "system" };
  const first = await fixture.post({ builds: [{ ...PAYLOAD, score: 999999 }], harvest, initiator: "system" });
  const second = await fixture.post({ builds: [PAYLOAD], harvest });
  const writes = fixture.db.calls.filter(call => /INSERT INTO record_build_submissions/.test(call.sql));
  check("a completed lease's outbox still delivers and retries", first.ok && second.ok && fixture.db.rows.size === 2, `${first.status}/${second.status}`);
  check("the receipt retains the server's original asker without crediting the executor", writes.length === 2
    && writes.every(call => call.args[3] === fixture.db.search.initiator && !/INTO (scores|answers|verifiers)/.test(call.sql))
    && [...fixture.db.rows.values()].every(record => !("score" in record)), JSON.stringify(writes));
  const crossWeapon = await fixture.post({ builds: [{ ...PAYLOAD, weapon: "torid" }], harvest });
  const crossResult = await crossWeapon.json();
  check("a receipt refuses another weapon without writing it", crossWeapon.ok && crossResult.results[0].ok === false
    && crossResult.results[0].retryable === false && fixture.db.rows.size === 2, JSON.stringify(crossResult));
  const [payload, signature] = harvest.receipt.split(".");
  const forged = `${payload}.${signature[0] === "A" ? "B" : "A"}${signature.slice(1)}`;
  for (const wrong of [{ ...harvest, question: "d".repeat(32) }, { ...harvest, engine: "another-engine" },
    { ...harvest, receipt: forged }]) {
    const refused = await fixture.post({ builds: [PAYLOAD], harvest: wrong });
    const detail = await refused.json();
    check("a changed receipt or search scope cannot authorize a build", [401, 403].includes(refused.status)
      && detail.retryable === false && fixture.db.rows.size === 2, `${refused.status}: ${JSON.stringify(detail)}`);
  }
  const claims = JSON.parse(Buffer.from(granted.receipt.split(".")[0], "base64url").toString());
  claims.expires_at = new Date(Date.now() - 1000).toISOString();
  const carried = Buffer.from(JSON.stringify(claims)).toString("base64url");
  const expired = await fixture.post({ builds: [PAYLOAD], harvest: { receipt: `${carried}.${await hmac(`${fixture.env.AUTH_SECRET}:board-harvest`, carried)}` } });
  const expiredBody = await expired.json();
  check("an expired receipt is a distinguishable permanent refusal", expired.status === 410 && expiredBody.retryable === false
    && fixture.db.rows.size === 2, JSON.stringify(expiredBody));
}
{
  for (const strategy of ["descent", "exhaust", undefined]) {
    const fixture = harvestFixture();
    fixture.db.search.request = JSON.stringify({ weapon: PAYLOAD.weapon, strategy, __board_upload: true });
    const response = await fixture.post({ builds: [], harvest: fixture.source });
    const grant = await response.json();
    const uploaded = await fixture.post({ builds: [PAYLOAD], harvest: { receipt: grant.receipt } });
    const write = fixture.db.calls.find(call => /INSERT INTO record_build_submissions/.test(call.sql));
    check("trusted optimizer leases do not duplicate the engine's strategy rules", response.ok && grant.receipt && uploaded.ok
      && write?.args[3] === fixture.db.search.initiator, `${strategy}: ${response.status}/${uploaded.status}`);
  }
  for (const flag of [false, undefined]) {
    const fixture = harvestFixture();
    fixture.db.search.request = JSON.stringify({ weapon: PAYLOAD.weapon, strategy: "quick", __board_upload: flag });
    const response = await fixture.post({ builds: [], harvest: { ...fixture.source, __board_upload: true } });
    const result = await response.json();
    check("a personal search without captured consent can compute without a build receipt", response.ok && result.ok
      && result.disabled === true && result.receipt === undefined && fixture.db.rows.size === 0, JSON.stringify(result));
    const bypass = await fixture.post({ builds: [PAYLOAD], harvest: { ...fixture.source, __board_upload: true } });
    const denied = await bypass.json();
    check("client flags cannot bypass the personal search's captured opt-out", bypass.ok && denied.results[0].ok === false
      && denied.results[0].retryable === false && fixture.db.rows.size === 0, JSON.stringify(denied));
  }
  for (const changed of [{ lease: "d".repeat(32) }, { verifier: "d".repeat(24) }, { question: "d".repeat(32) }]) {
    const fixture = harvestFixture();
    const response = await fixture.post({ builds: [], harvest: { ...fixture.source, ...changed } });
    check("another lease, witness or question cannot issue a receipt", response.status === 403 && fixture.db.rows.size === 0, String(response.status));
  }
  const fixture = harvestFixture();
  fixture.db.leaseUntil = Date.now() - 1000;
  const expired = await fixture.post({ builds: [], harvest: fixture.source });
  check("an expired lease cannot issue a new receipt", expired.status === 403, String(expired.status));
  const family = harvestFixture("family_build");
  const grant = await (await family.post({ builds: [], harvest: family.source })).json();
  const response = await family.post({ builds: [PAYLOAD, PAYLOAD], harvest: { receipt: grant.receipt } });
  const write = family.db.calls.find(call => /INSERT INTO record_build_submissions/.test(call.sql));
  check("family-search candidates retain trusted derivation provenance", response.ok && family.db.rows.size === 2
    && /family_source/.test(write?.sql || "") && write.args[3] === "system" && write.args[4] === family.db.search.ref
    && write.args[8] === "system" && write.args[9] === family.db.search.ref, JSON.stringify(write));
}
{
  const fixture = harvestFixture();
  const response = await fixture.post({ builds: [], harvest: { code: "TEST7", engine: "check-engine", initiator: "forged" } });
  const grant = await response.json();
  const expectedQuestion = await questionId("optimize", JSON.parse(fixture.db.appraisal.request));
  const uploaded = await fixture.post({ builds: [PAYLOAD], harvest: { receipt: grant.receipt, question: expectedQuestion } });
  const expectedInitiator = await initiatorCode(fixture.env, "bot", "qq:original-chat-asker");
  const write = fixture.db.calls.find(call => /INSERT INTO record_build_submissions/.test(call.sql));
  check("an opened appraisal keeps its existing code-based build authority", response.ok && grant.question === expectedQuestion
    && uploaded.ok && write?.args[3] === expectedInitiator, `${response.status}/${uploaded.status}: ${JSON.stringify(write)}`);
  const alternate = harvestFixture();
  alternate.db.appraisal.request = JSON.stringify({ request: { weapon: PAYLOAD.weapon, strategy: "exhaust" }, context: { weapon: PAYLOAD.weapon } });
  const nonQuick = await alternate.post({ builds: [], harvest: { code: "TEST7", engine: "check-engine" } });
  check("a frozen appraisal's strategy remains the engine's decision", nonQuick.ok && !!(await nonQuick.json()).receipt, String(nonQuick.status));
  const older = harvestFixture();
  older.db.appraisal.engine = "older-engine";
  const frozenBefore = JSON.stringify(older.db.appraisal);
  const olderQuestion = await questionId("optimize", JSON.parse(older.db.appraisal.request));
  const continued = await older.post({ builds: [], harvest: { code: "TEST7", engine: "check-engine" } });
  const continuedGrant = await continued.json();
  check("the current execution engine can continue an appraisal frozen before a release", continued.ok
    && typeof continuedGrant.receipt === "string" && continuedGrant.engine === "check-engine"
    && continuedGrant.question === olderQuestion, `${continued.status}: ${JSON.stringify(continuedGrant)}`);
  if (continuedGrant.receipt) {
    const claims = JSON.parse(Buffer.from(continuedGrant.receipt.split(".")[0], "base64url").toString());
    const delivered = await older.post({ builds: [PAYLOAD], harvest: { receipt: continuedGrant.receipt,
      question: olderQuestion, engine: "check-engine" } });
    const stored = older.db.calls.find(call => /INSERT INTO record_build_submissions/.test(call.sql));
    check("a continued appraisal retains its frozen question and original asker without rewriting it", delivered.ok
      && claims.engine === "check-engine" && claims.question === olderQuestion && stored?.args[3] === expectedInitiator
      && JSON.stringify(older.db.appraisal) === frozenBefore && !older.db.calls.some(call => /UPDATE entity_appraisal_requests/.test(call.sql)), JSON.stringify(claims));
  }
  const staleEngine = await older.post({ builds: [], harvest: { code: "TEST7", engine: "older-engine" } });
  check("an appraisal still requires the currently served execution engine", staleEngine.status === 409
    && (await staleEngine.json()).retryable === false, String(staleEngine.status));
  for (const change of [{ request: null }, { at: new Date(0).toISOString() }]) {
    const denied = harvestFixture(); Object.assign(denied.db.appraisal, change);
    const refused = await denied.post({ builds: [], harvest: { code: "TEST7", engine: "check-engine" } });
    const detail = await refused.json();
    check("an unfrozen or expired appraisal cannot issue a receipt", [404, 409].includes(refused.status)
      && detail.retryable === false && denied.db.rows.size === 0, `${refused.status}: ${JSON.stringify(detail)}`);
  }
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
