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
    { kind: "board", weapon: "torid", ruler: "standard_single_target", mode: "base", mods: ["secret"], at: Date.now() - 120000, started: Date.now() - 125000, ms: 32000, work: 3400000000,
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
  out.took = (page().querySelector("#rt-live .rt-detail") || { textContent: "" }).textContent.replace(/\\s+/g, " ");
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
check("a task says how long it took by the clock, its cores' summed time beside it as core time",
  r.recent[0].includes("· 5 s") && r.took.includes("5 s · 32 s of core time"), JSON.stringify([r.recent[0], r.took.slice(0, 200)]));
check("every block of the compute page folds as the site's do", r.folds.length >= 3 && r.folds.every(([, c]) => c), JSON.stringify(r.folds));
check("...and a folded one stays folded, with one caret, when the page is drawn again", r.shut && r.stillShut);
check("a riven gain asks the server where it stands: one credited says confirmed, one not yet waits for another computer",
  r.recent[2] && r.recent[2].includes("confirmed +1") && r.recent[3] && r.recent[3].includes("waiting for another device"), JSON.stringify(r.recent));
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
// whether a task runs or not. Solo and together are two sliders in the menu.
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
  computeEnd(null);
  out.after = bar(); out.idle = mark().dataset.state;
  setBoardVerify(false); out.off = mark().hidden; setBoardVerify(true);
  computeBattery = { charging: false }; out.battery = computeHeld(); computeBattery = null;
  computeBattery = { charging: false }; setComputeOnBattery(true); out.batteryAllowed = computeHeld();
  setComputeOnBattery(false); computeBattery = null;
  // HOW MUCH (69-board-work.js \`communityPower\`): the together share of the
  // device whether or not the reader is at it, as lanes each running a share of
  // the time; past 100% with solo, while the reader computes, the two in proportion.
  const cores = detectedCores().n;
  localStorage.removeItem("wfsim-community-share");
  lastTouched = Date.now(); out.power30 = communityPower(); out.want30 = cores * 0.3;
  out.lanes30 = communityLanes(); out.duty30 = communityDuty();
  setCommunityShare(100); setComputePct(100);
  out.alone100 = communityPower();
  readerInFlight += 1; out.shared100 = communityPower(); readerInFlight -= 1;
  setCommunityShare(30); setComputePct(50);
  readerInFlight += 1; out.under100 = communityPower(); readerInFlight -= 1;
  out.cores = cores;
  // THE TWO SLIDERS (10-weapon-search.js \`computeSharesHtml\`), in the menu.
  renderComputePicker();
  const cs = document.getElementById("compute-select");
  out.sliders = cs ? cs.querySelectorAll('input[type="range"]').length : 0;
  const tog = cs && cs.querySelector('[data-cs="together"]');
  if (tog) { tog.value = "60"; tog.dispatchEvent(new Event("change", { bubbles: true })); }
  out.set60 = communityShare() === 60;
  out.sum = cs ? (cs.querySelector("[data-cs-sum]") || {}).textContent : "";
  localStorage.removeItem("wfsim-community-share"); renderComputePicker();
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
check("while it runs the top bar's ring says so", /Computing now/.test(k.mark), c);
check("...the ring keeps its size when the task ends, so nothing on the bar moves",
  k.during[0] > 0 && k.during[0] === k.after[0] && k.during[1] === k.after[1] && k.idle === "waiting", c);
check("...it leaves only when computing is turned off, and a battery holds it unless the reader allows it",
  k.off && k.battery === "battery" && k.batteryAllowed === "", c);
check("together takes its share of the device whether or not the reader is at it, as lanes running that share of the time",
  Math.abs(k.power30 - k.want30) < 1e-9 && k.lanes30 === Math.ceil(k.want30 - 1e-9) && Math.abs(k.duty30 * k.lanes30 - k.want30) < 1e-9, c);
check("...past 100% with solo, while the reader computes, the two share the device in proportion; within it, they never touch",
  k.alone100 === k.cores && Math.abs(k.shared100 - k.cores / 2) < 1e-9 && Math.abs(k.under100 - k.want30) < 1e-9, c);
check("solo and together are two sliders in the menu, together's set where it is let go, and the sum said",
  k.sliders === 2 && k.set60 && /110% in all/.test(k.sum), c);
check("...and a yes to an older statement is asked again", k.oldYes, c);

// WHEN THE READER COMPUTES, WHATEVER THEY RUN (69-board-work.js `readerBusy`),
// which is when solo and together past 100% share in proportion: a call of
// theirs counts and a together call does not, a search between calls counts,
// and so does another tab's word.
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
  return JSON.stringify(out);
})()`);
const yr = JSON.parse(y);
check("[built site] the reader's own call counts as their computing, and a together call does not",
  yr.idle && yr.reader && yr.after && yr.community, y);
check("...a search between calls counts, and so does another tab computing", yr.search && yr.otherTab, y);
check("...but a scan or a search that stopped answering never counts for good, in this tab or any other",
  yr.searchLatched && yr.scanLatched, y);

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
  (await boardLanes(communityLanes())).forEach((l, i) => {
    const call = l.call;
    l.call = (p, b, x, c) => { if (p === "/api/board/runs") used.add(i); return call(p, b, x, c); };
  });
  const got = await measureRow(request, "standard_single_target", () => true);
  localStorage.removeItem("wfsim-community-share"); lastTouched = Date.now();
  out.lanes = communityLanes(); out.used = used.size;
  out.same = !!got && got.score === want.score && got.metric === want.metric && got.work === want.work;
  out.got = got && [got.score, got.metric, got.work]; out.want = [want.score, want.metric, want.work];
  return JSON.stringify(out);
})()`);
const fr = JSON.parse(f);
check("[built site] a board order fights on more than one lane when its share buys them",
  !fr.wasm || fr.lanes < 2 || fr.used >= 2, f);
check("...and its score, metric and work are the single fold's to the bit", !fr.wasm || fr.same, f);

// A PAGE THE BROWSER FREEZES (69-board-work.js `computeFrozen`): what it holds
// goes back at once, by beacon, and woken it asks for work at once and /compute
// says the browser put it to sleep, with how to keep the site awake. Frozen and
// woken here as the browser does, by its own lifecycle.
await evaluate(`(() => {
  window.__beacons = [];
  navigator.sendBeacon = (path, body) => { window.__beacons.push([path, String(body)]); return true; };
  localStorage.setItem("wfsim-compute-consent", JSON.stringify({ v: COMPUTE_CONSENT_V, on: true, at: new Date().toISOString() }));
  heldLease = { lease: "f".repeat(32), verifier: "z".repeat(24) };
  window.__woke = false; computeWake = () => { window.__woke = true; };
  return true;
})()`);
await app.send("Page.setWebLifecycleState", { state: "frozen" });
await app.sleep(300);
await app.send("Page.setWebLifecycleState", { state: "active" });
await app.sleep(300);
const z = JSON.parse(await evaluate(`(async () => {
  const out = { beacons: window.__beacons, frozen: computeFrozen, woke: window.__woke, slept: computeSlept, held: heldLease };
  history.pushState({}, "", "/compute"); route(); await new Promise((ok) => setTimeout(ok, 400));
  out.note = (document.getElementById("compute-browser") || { textContent: "" }).textContent.includes("put this page to sleep");
  localStorage.removeItem("wfsim-compute-consent");
  return JSON.stringify(out);
})()`));
check("a page the browser freezes hands back what it holds at once",
  z.held === null && z.beacons.some(([p, b]) => p === "/api/board/release" && b.includes("f".repeat(32))), JSON.stringify(z));
check("...woken, it asks for work at once, and /compute says the browser put it to sleep", z.frozen && z.woke && z.slept && z.note, JSON.stringify(z));

// WHAT A DEVICE IS CALLED until its owner names it (73-compute.js
// \`deviceGuessReady\`): the pinned library, served from this origin, reads the
// system, the browser and a phone's model — a browser inside WeChat too.
const WECHAT = "Mozilla/5.0 (Linux; Android 13; V2227A Build/TP1A.220624.014; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/111.0.5563.116 Mobile Safari/537.36 XWEB/5235 MMWEBSDK/20230805 MicroMessenger/8.0.42.2460(0x28002A35) WeChat/arm64 Weixin NetType/WIFI Language/zh_CN ABI/arm64";
const here = JSON.parse(await evaluate(`(async () => JSON.stringify({ label: await deviceGuessReady(), lib: typeof UAParser === "function" }))()`));
await app.send("Emulation.setUserAgentOverride", { userAgent: WECHAT });
const wx = await evaluate(`(async () => { deviceGuess = null; deviceGuessing = null; return await deviceGuessReady(); })()`);
check("a device is first called by its system and browser, read by the library this site serves itself",
  here.lib && / · /.test(here.label), JSON.stringify(here));
check("...and a phone by its maker and model, a browser inside WeChat by WeChat", wx === "Vivo V2227A · WeChat", wx);

await app.finish("the compute page shows what each device does, by kind, and nothing private");
