// THE COMPUTE PAGE (`/compute`, web/src/static/app/73-compute.js) — docs/BOARD.md
// §"Contribution". This browser's tasks are drawn from what it kept and by each
// task's KIND: a board order as its weapon, ruler and mode, linked to that
// board, never its mods; a kind the page does not know still draws. A task in
// progress shows how far it is. Signed in, every device of the account is
// listed by its name, this browser marked, with what each is doing or last
// did; one claimed before it had a name is given its guess; a device is renamed
// and removed inline, with no native dialog. The account is answered by
// interception.
//   node scripts/check_compute.mjs        (WFSIM_BASE=<origin> for a dev server)
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 13000, base: process.env.WFSIM_BASE });
const { evaluate, check } = app;
await app.setLang("en", 20000);

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((ok) => setTimeout(ok, ms));
  const realFetch = window.fetch;
  const own = "ownown" + "0".repeat(18);
  localStorage.setItem("wfsim-verifier", own);
  localStorage.setItem("wfsim-compute-log", JSON.stringify([
    { kind: "board", weapon: "torid", ruler: "standard_single_target", mode: "base", mods: ["secret"], at: Date.now() - 120000, ms: 12000, work: 3400000000,
      identity: "b1", state: "confirmed", score: 123.4567, metric: "kpm", record: { weapon: "torid", mods: ["serration"] } },
    { kind: "appraise", at: Date.now() - 7200000, ms: 90000, work: 1e9 },
    { kind: "riven_gain", weapon: "furis", ruler: "standard_single_target", at: Date.now() - 10800000, ms: 95000, work: 1e9,
      score: 254.3244, metric: "kpm", search: { builds: 1840, fights: 22080 } },
    { kind: "riven_gain", weapon: "furis", ruler: "standard_single_target", at: Date.now() - 14400000, ms: 95000, work: 1e9, score: 0, metric: "kpm" },
  ]));
  let account = null;
  let devices = [
    { id: "ownown", label: null, claimed_at: "2026-10-08", points: 12, recent: 12, last_at: null,
      now: { kind: "board", weapon: "furis", ruler: "standard_single_target", mode: "base" } },
    { id: "abcdef", label: "Study PC", claimed_at: "2026-10-08", points: 30, recent: 5, last_at: new Date(Date.now() - 600000).toISOString(), now: null },
  ];
  const posted = [];
  window.fetch = async (url, o = {}) => {
    const path = String(url);
    const reply = (j, status = 200) => new Response(JSON.stringify(j), { status, headers: { "content-type": "application/json" } });
    if (path === "/api/account") return reply({ ok: true, providers: ["email"], account });
    if (path === "/api/account/devices") return reply({ ok: true, named: false, decided: true, points: 42, recent: 17, devices });
    if (path === "/api/account/devices/label" || path === "/api/account/devices/remove") {
      const b = JSON.parse(o.body);
      posted.push([path.split("/").pop(), b]);
      if (path.endsWith("label")) devices = devices.map((d) => (d.id === b.id ? { ...d, label: b.label } : d));
      else devices = devices.filter((d) => d.id !== b.id);
      return reply({ ok: true });
    }
    // A RIVEN GAIN ASKS WHERE IT STANDS as a board order does: the newer one is credited.
    if (path === "/api/appraise/mine") {
      const b = JSON.parse(o.body);
      const newest = Math.max(...b.tasks.map((t) => t.at));
      return reply({ ok: true, tasks: b.tasks.map((t) => (t.at === newest
        ? { at: t.at, code: "AB12", state: "confirmed", agreed_at: new Date().toISOString() } : { at: t.at, code: "CD34", state: "waiting" })) });
    }
    if (path === "/api/board/points") return reply({ ok: true, points: 12, recent: 12, claimed: !!account });
    if (path === "/api/cloud/sync") return reply({ ok: true, full: false, entries: [], next: null, cursor: 0 });
    return realFetch(url, o);
  };
  const page = () => document.getElementById("auth-page");
  const rowsOf = (title) => {
    const block = [...page().querySelectorAll(".block")].find((b) => (b.querySelector(".bh h2") || {}).textContent === title);
    // THE DEVICES' ROWS: the honour below them is a row of its own.
    return block ? [...block.querySelectorAll(".kv")].filter((k) => k.querySelector("dt").textContent !== "Honour") : [];
  };
  const out = {};
  history.pushState({}, "", "/compute"); route(); await loadAccount(); await sleep(500);
  const cards = () => [...page().querySelectorAll("#rt-live .rt-card")];
  out.recent = cards().map((k) => k.textContent.replace(/\\s+/g, " ").trim());
  // EVERY BLOCK FOLDS, the site's way, and stays folded when the page is drawn again.
  out.folds = [...page().querySelectorAll(".block[id]")].map((b) => [b.id, !!b.querySelector(":scope > .bh > .fold-c")]);
  const demand = document.getElementById("compute-browser");
  demand.querySelector(":scope > .bh").click();
  out.shut = demand.classList.contains("shut");
  renderAuthPage("compute"); await sleep(200);
  out.stillShut = document.getElementById("compute-browser").classList.contains("shut")
    && document.getElementById("compute-browser").querySelectorAll(":scope > .bh > .fold-c").length === 1;
  document.getElementById("compute-browser").querySelector(":scope > .bh").click();
  out.secret = page().textContent.includes("secret");
  // OPENED, a task shows its build and the way to its weapon's board; the list
  // node is the same one after the page is drawn again around it.
  const live = document.getElementById("rt-live");
  cards()[0].click(); await sleep(300);
  out.link = (page().querySelector('#rt-live .rt-detail a[href$="/benchmark"]') || { getAttribute: () => null }).getAttribute("href");
  out.kept = document.getElementById("rt-live") === live;
  cards()[0].click(); await sleep(200);
  out.closed = !page().querySelector("#rt-live .rt-detail");
  out.signIn = !!page().querySelector('a[href^="/login"]');
  // THE NOW ROW IS DRAWN WHILE COMPUTING IS ON, so it is turned on for this.
  localStorage.setItem("wfsim-compute-consent", JSON.stringify({ v: COMPUTE_CONSENT_V, on: true, at: new Date().toISOString() }));
  computeStart({ kind: "board", weapon: "torid", ruler: "standard_single_target", mode: "base" });
  computeProgress(50, 100);
  computeRedraw();
  const pct = page().querySelector("#rt-live .rt-now .rt-pct");
  out.now = pct ? pct.textContent : null;
  out.bar = (page().querySelector("#rt-live .rt-now .rt-bar i") || { style: {} }).style.width;
  computeEnd(null);
  localStorage.removeItem("wfsim-compute-consent");

  account = { id: "acc1", created_at: "", identities: [{ provider: "email", label: "a@x" }] };
  history.pushState({}, "", "/"); route(); await sleep(100);
  history.pushState({}, "", "/compute"); route(); await loadAccount(); await sleep(800);
  const mine = rowsOf("Your devices");
  out.devices = mine.map((k) => k.querySelector("dt").textContent.replace(/\\s+/g, " ").trim());
  out.status = mine.map((k) => (k.querySelector("dd") || { textContent: "" }).textContent.replace(/\\s+/g, " ").trim());
  out.guessed = posted.find(([what, b]) => what === "label" && b.id === "ownown");
  page().querySelector('[data-auth="device-rename"][data-id="abcdef"]').click(); await sleep(200);
  const input = document.getElementById("device-label");
  out.editing = !!input;
  input.value = "Garage Mac";
  page().querySelector('[data-auth="device-rename-save"]').click(); await sleep(400);
  out.renamed = rowsOf("Your devices").map((k) => k.querySelector("dt").textContent.replace(/\\s+/g, " ").trim());
  page().querySelector('[data-auth="device-remove"][data-id="abcdef"]').click(); await sleep(200);
  out.asks = page().textContent.includes("Remove it from this list? Its points stay yours");
  page().querySelector('[data-auth="device-remove-confirm"]').click(); await sleep(400);
  out.after = rowsOf("Your devices").length;
  out.posted = posted;
  window.fetch = realFetch;
  localStorage.removeItem("wfsim-compute-log");
  return out;
})()`, 40000);

check("this browser's tasks are drawn by kind, a board order as its weapon and ruler, confirmed with its points",
  r.recent[0] && r.recent[0].startsWith("SimulateTorid") && r.recent[0].includes("Standard Single Target") && r.recent[0].includes("confirmed +3"), JSON.stringify(r.recent));
check("...a simulation says what it computed, a search the best it found among the builds it tried",
  r.recent[0] && r.recent[0].includes("computed 123.4567 KPM")
  && r.recent[2] && r.recent[2].startsWith("OptimizeFuris") && r.recent[2].includes("best 254.3244 KPM · 1,840 builds"), JSON.stringify(r.recent));
check("every block of the compute page folds as the site's do", r.folds.length >= 3 && r.folds.every(([, c]) => c), JSON.stringify(r.folds));
check("...and a folded one stays folded, with one caret, when the page is drawn again", r.shut && r.stillShut);
check("a riven gain asks the server where it stands: one credited says confirmed, one not yet waits for another computer",
  r.recent[2] && r.recent[2].includes("confirmed +1") && r.recent[3] && r.recent[3].includes("waiting for another computer"), JSON.stringify(r.recent));
check("...and a search's stored 0, a misread row, is drawn as no number at all", r.recent[3] && r.recent[3].startsWith("Optimize") && !r.recent[3].includes("KPM"),
  JSON.stringify(r.recent));
check("...opened, linked to that weapon's board, and its mods never drawn closed", r.link === "/weapons/Torid/benchmark" && !r.secret && r.closed,
  `${r.link} ${r.secret} ${r.closed}`);
check("...the list is changed in place, never drawn again", r.kept);
check("...and a kind the page does not know still draws", r.recent[1] && r.recent[1].startsWith("Task"), JSON.stringify(r.recent));
check("signed out, it says how to count the work under a name", r.signIn);
check("a task in progress shows how far it is", r.now === "50%" && r.bar === "50%", `${r.now} ${r.bar}`);
check("signed in, every device is listed by its name, this browser marked",
  / · .* \(this browser\)$/.test(r.devices[0]) && r.devices[1] === "Study PC" && r.devices.length === 2, JSON.stringify(r.devices));
check("...with what each is doing, or when it last answered",
  /^Computing: Leaderboard order · Furis/.test(r.status[0]) && /^Last answered 10 min ago/.test(r.status[1]), JSON.stringify(r.status));
check("this browser, claimed before it had a name, is given its guess", !!r.guessed && /·/.test(r.guessed[1].label),
  JSON.stringify(r.guessed));
check("a device is renamed inline", r.editing && r.renamed.includes("Garage Mac"), JSON.stringify(r.renamed));
check("...and removed after an inline question", r.asks && r.after === 1
  && r.posted.some(([what, b]) => what === "remove" && b.id === "abcdef"), JSON.stringify(r.posted));

// THE QUESTION BEFORE ANY COMPUTING (69-board-work.js `computeConsent`): on a
// computer that can compute and has not answered, a card asks once; an old
// default's "yes" is no answer; a no is kept and not asked again; a yes turns
// it on with the statement and the time; a card page a bot photographs never
// carries it; and while it is on a ring in the top bar says so, at one size
// whether a task runs or not, and the pause holds it.
const c = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((ok) => setTimeout(ok, ms));
  const out = { wasm: WASM };
  const card = () => !!document.getElementById("compute-ask");
  localStorage.removeItem("wfsim-compute-consent");
  localStorage.setItem("wfsim-board-verify", "yes");
  history.pushState({}, "", "/"); computeChrome();
  out.asked = card(); out.offBefore = !boardVerifyOn();
  document.querySelector('#compute-ask [data-compute-ask="no"]').click(); await sleep(50);
  const no = JSON.parse(localStorage.getItem("wfsim-compute-consent") || "null");
  computeChrome();
  out.no = !card() && no && no.on === false && no.v === COMPUTE_CONSENT_V && !boardVerifyOn();
  localStorage.removeItem("wfsim-compute-consent");
  history.pushState({}, "", "/weapons/Torid/card"); computeChrome();
  out.cardPage = !card();
  history.pushState({}, "", "/"); computeChrome();
  document.querySelector('#compute-ask [data-compute-ask="yes"]').click(); await sleep(50);
  const yes = JSON.parse(localStorage.getItem("wfsim-compute-consent") || "null");
  out.yes = !card() && boardVerifyOn() && yes.on === true && Number.isFinite(Date.parse(yes.at));
  const mark = () => document.getElementById("compute-mark");
  const bar = () => [mark().getBoundingClientRect().width, document.querySelector(".wsearch").getBoundingClientRect().width];
  computeStart({ kind: "board", weapon: "torid", ruler: "standard_single_target", mode: "base" });
  out.mark = mark().hidden ? "" : mark().getAttribute("aria-label");
  out.during = bar();
  computeTogglePause(); await sleep(50);
  out.paused = mark().getAttribute("aria-label"); out.held = computeHeld();
  computeTogglePause(); await sleep(50);
  computeEnd(null);
  out.after = bar(); out.idle = mark().dataset.state;
  setBoardVerify(false); out.off = mark().hidden; setBoardVerify(true);
  computeBattery = { charging: false }; out.battery = computeHeld(); computeBattery = null;
  // HOW MUCH: one core while the reader is at the computer, the share they
  // picked (30% unless they did) of its cores once it is idle.
  const cores = detectedCores().n;
  localStorage.removeItem("wfsim-community-share");
  lastTouched = Date.now(); out.busyLanes = communityLanes();
  lastTouched = 0; out.idleLanes = communityLanes(); out.want30 = Math.max(1, Math.ceil(cores * 0.3));
  setCommunityShare(50); out.idle50 = communityLanes(); out.want50 = Math.max(1, Math.ceil(cores * 0.5));
  localStorage.removeItem("wfsim-community-share"); lastTouched = Date.now();
  // AN OLDER YES IS NO ANSWER to a statement that now says more.
  localStorage.setItem("wfsim-compute-consent", JSON.stringify({ v: COMPUTE_CONSENT_V - 1, on: true, at: new Date().toISOString() }));
  out.oldYes = !boardVerifyOn();
  localStorage.removeItem("wfsim-compute-consent");
  return JSON.stringify(out);
})()`);
const k = JSON.parse(c);
check("[built site] nothing computes until asked: a card asks, and an old default's yes is no answer", k.wasm && k.asked && k.offBefore, c);
check("...a no is kept and not asked again", k.no, c);
check("...a card page a bot photographs never carries it", k.cardPage, c);
check("...a yes turns it on, kept with the statement and when", k.yes, c);
check("while it runs the top bar's ring says so, and the pause holds it", /Computing now/.test(k.mark) && /Paused/.test(k.paused) && k.held === "paused", c);
check("...the ring keeps its size when the task ends, so nothing on the bar moves",
  k.during[0] > 0 && k.during[0] === k.after[0] && k.during[1] === k.after[1] && k.idle === "waiting", c);
check("...it leaves only when computing is turned off, and a battery holds it", k.off && k.battery === "battery", c);
check("one core while the reader is at the computer, 30% of its cores once idle, or the share they picked",
  k.busyLanes === 1 && k.idleLanes === k.want30 && k.idle50 === k.want50, c);
check("...and a yes to an older statement is asked again", k.oldYes, c);

// THE READER GOES FIRST, WHATEVER THEY RUN (69-board-work.js `readerBusy`): a
// call of theirs on the pool holds the community's work and a community call
// does not, a search between calls holds it, another tab's word holds it, and
// work waiting on it resumes the moment nothing does.
const y = await evaluate(`(async () => {
  const sleep = (ms) => new Promise((ok) => setTimeout(ok, ms));
  const out = {};
  for (let i = 0; i < 200 && readerBusy(); i++) await sleep(50);
  out.idle = !readerBusy();
  const mine = api("/api/meta");
  out.reader = readerInFlight > 0 && readerBusy();
  await mine;
  out.after = !readerBusy();
  const theirs = api("/api/meta", null, null, { community: true });
  out.community = readerInFlight === 0 && !readerBusy();
  await theirs;
  optJobId = 12345; optHeardAt = Date.now(); out.search = readerBusy();
  optHeardAt = Date.now() - 60000; out.searchLatched = !readerBusy(); optJobId = null;
  const scan = gainScan.running; gainScan.running = true; gainScan.beat = 0;
  out.scanLatched = !readerBusy(); gainScan.running = scan;
  const other = new BroadcastChannel("wfsim-reader-busy");
  other.postMessage({ until: Date.now() + 2000 }); await sleep(100);
  out.otherTab = readerBusy(); other.close(); otherTabBusyUntil = 0;
  let waiting = true;
  optJobId = 1; optHeardAt = Date.now();
  const held = yieldToReader().then(() => { waiting = false; });
  await sleep(200); out.waits = waiting;
  optJobId = null; await held; out.resumes = !waiting;
  return JSON.stringify(out);
})()`);
const yr = JSON.parse(y);
check("[built site] the reader's own call on the pool holds the community's work, and a community call does not",
  yr.idle && yr.reader && yr.after && yr.community, y);
check("...a search between calls holds it, and so does another tab computing", yr.search && yr.otherTab, y);
check("...but a scan or a search that stopped answering never holds it for good, in this tab or any other",
  yr.searchLatched && yr.scanLatched, y);
check("...work waiting on the reader resumes the moment nothing of theirs runs", yr.waits && yr.resumes, y);

// A BOARD ORDER FIGHTS ON EVERY LANE ITS SHARE BUYS (69-board-work.js
// `measureRow`), and its score, metric and work are the single fold's to the
// bit: the shards are folded in run order however the lanes raced.
const f = await evaluate(`(async () => {
  const out = { wasm: !!WASM };
  if (!WASM) return JSON.stringify(out);
  const rows = await (await fetch("/board/braton_prime.json")).json();
  const row = rows.find((r) => r.benchmark === "standard_single_target" && (r.mode || "base") === "base" && !(r.mods || []).includes("riven"));
  const record = { weapon: "braton_prime" };
  for (const k of ["mods", "evolutions", "arcanes", "valence", "exilus"]) if (row[k] !== undefined) record[k] = row[k];
  const order = await api("/api/board/order", { record, ruler: "standard_single_target", mode: "base" });
  const request = { ...order.request, runs: 48 };
  const one = await api("/api/board/fold", { request, from: 0, count: 48 });
  const want = await api("/api/board/score", { ruler: "standard_single_target", request, acc: one.acc });
  setCommunityShare(100); lastTouched = 0;
  const used = new Set();
  (await lanes(communityLanes(true))).forEach((l, i) => {
    const call = l.call;
    l.call = (p, b, x, c) => { if (p === "/api/board/runs") used.add(i); return call(p, b, x, c); };
  });
  const got = await measureRow(request, "standard_single_target", () => true);
  localStorage.removeItem("wfsim-community-share"); lastTouched = Date.now();
  out.lanes = communityLanes(true); out.used = used.size;
  out.same = !!got && got.score === want.score && got.metric === want.metric && got.work === want.work;
  out.got = got && [got.score, got.metric, got.work]; out.want = [want.score, want.metric, want.work];
  return JSON.stringify(out);
})()`);
const fr = JSON.parse(f);
check("[built site] a board order fights on more than one lane when its share buys them",
  !fr.wasm || fr.lanes < 2 || fr.used >= 2, f);
check("...and its score, metric and work are the single fold's to the bit", !fr.wasm || fr.same, f);

await app.finish("the compute page shows what each device does, by kind, and nothing private");
