// SPDX-License-Identifier: AGPL-3.0-or-later
// THE OWNER'S DASHBOARD — the community's machines as they stand now: which are
// computing, which are idle, which are on an engine no longer served, and what
// the factory has waiting for them. Read-only, behind its own secret.
//
//   GET  /api/ops        → the page (it holds no data; it asks with the secret)
//   POST /api/ops/pulse  Authorization: Bearer <OPS_TOKEN> → the numbers below
//
// A machine is ON while it asked for work, answered or held a lease in the last
// `ON_MS`: an idle page asks every ten seconds (69-board-work.js
// `ASK_EVERY_MS`), recorded once a minute (worker/verify.js `askedNow`), and a
// computing one asks only between tasks, so a held lease is what shows it.
import { servedRelease } from "./verify.js";
import { iso } from "./instant.js";

const ON_MS = 3 * 60_000;
const HOURS = 24;

const json = (body, status = 200) => new Response(JSON.stringify(body), {
  status, headers: { "content-type": "application/json; charset=utf-8", "cache-control": "no-store" } });

export async function opsRoute(request, env, path) {
  if (path === "/api/ops") {
    return new Response(PAGE, { headers: { "content-type": "text/html; charset=utf-8", "cache-control": "no-store",
      "x-robots-tag": "noindex", "content-security-policy": "default-src 'self'; style-src 'unsafe-inline'; script-src 'unsafe-inline'" } });
  }
  if (path !== "/api/ops/pulse") return null;
  if (request.method !== "POST") return json({ ok: false, error: "POST only" }, 405);
  const auth = request.headers.get("authorization") || "";
  if (!env.OPS_TOKEN || auth !== `Bearer ${env.OPS_TOKEN}`) return json({ ok: false, error: "unauthorized" }, 401);
  if (!env.LIBRARY) return json({ ok: false, error: "the library is not configured" }, 503);
  return json({ ok: true, ...(await pulse(env, Date.now())) });
}

async function pulse(env, now) {
  const db = env.LIBRARY, at = iso(now), on = iso(now - ON_MS), hour = iso(now - 3_600_000);
  const { engine, release } = await servedRelease(env);
  const [machines, held, answered, queue, recent, hours] = (await db.batch([
    db.prepare(`SELECT id, asked_at, asked_engine, asked_lanes, last_at, refused_until FROM verifiers
                 WHERE asked_at > ?1 OR last_at > ?1`).bind(on),
    // WHAT EACH ONE HOLDS: a board question or a riven gain someone waits on.
    db.prepare(`SELECT leased_to AS id, 'board' AS kind, count(*) AS n FROM questions WHERE lease_until > ?1 GROUP BY 1
                UNION ALL SELECT leased_to, 'riven', count(*) FROM appraisals WHERE lease_until > ?1 AND leased_to IS NOT NULL GROUP BY 1`).bind(at),
    db.prepare(`SELECT device AS id, count(*) AS tasks, sum(compute_ms) AS ms, max(at) AS last FROM answers
                 WHERE at > ?1 AND official = 0 GROUP BY 1`).bind(hour),
    db.prepare(`SELECT d.producer AS kind, q.state, count(*) AS n, sum(q.due_at < ?1) AS past,
                       sum(coalesce(q.lease_until > ?1, 0)) AS leased, min(q.due_at) AS next_due
                  FROM questions q JOIN demands d ON d.question = q.id AND d.state = 'live'
                 WHERE q.state != 'fact' AND q.state != 'withdrawn' GROUP BY 1, 2`).bind(at),
    db.prepare(`SELECT a.official, count(*) AS n, count(DISTINCT a.device) AS devices, sum(a.credited) AS credited,
                       sum(q.fact_at IS NOT NULL AND a.at > q.fact_at AND a.official = 0) AS late
                  FROM answers a JOIN questions q ON q.id = a.question WHERE a.at > ?1 GROUP BY 1`).bind(hour),
    db.prepare(`SELECT hour, count(DISTINCT CASE WHEN tasks > 0 THEN verifier END) AS devices, sum(tasks) AS tasks, sum(work) AS work
                  FROM verifier_hours WHERE hour >= ?1 GROUP BY 1 ORDER BY 1`).bind(iso(now - HOURS * 3_600_000).slice(0, 13)),
  ])).map((r) => r.results);

  const holds = new Map();
  for (const h of held) {
    if (!h.id || h.id.startsWith("official:")) continue;
    const x = holds.get(h.id) || { board: 0, riven: 0 };
    x[h.kind] += h.n;
    holds.set(h.id, x);
  }
  const work = new Map(answered.map((a) => [a.id, a]));
  const ids = new Set([...machines.map((m) => m.id), ...holds.keys()]);
  const byId = new Map(machines.map((m) => [m.id, m]));
  const names = await namesOf(env, [...ids]);
  const devices = [...ids].map((id) => {
    const m = byId.get(id) || {}, h = holds.get(id) || { board: 0, riven: 0 }, w = work.get(id) || {};
    const refused = m.refused_until && Date.parse(m.refused_until) > now;
    const state = refused ? "refused" : h.board || h.riven ? "computing"
      : m.asked_engine && m.asked_engine !== engine && m.asked_at > on ? "stale" : m.asked_at > on ? "idle" : "gone";
    return { id: id.slice(0, 8), name: names.get(id) || null, state, lanes: m.asked_lanes ?? null, holds: h,
      engine: m.asked_engine || null, asked_at: m.asked_at || null, answered_at: m.last_at || null,
      tasks_hour: w.tasks || 0, ms_hour: w.ms || 0 };
  }).filter((d) => d.state !== "gone");
  const order = { computing: 0, idle: 1, stale: 2, refused: 3 };
  devices.sort((a, b) => order[a.state] - order[b.state] || b.tasks_hour - a.tasks_hour);

  const count = (s) => devices.filter((d) => d.state === s);
  const lanes = (ds) => ds.reduce((n, d) => n + (d.lanes || 0), 0);
  const summary = Object.fromEntries(["computing", "idle", "stale", "refused"].map((s) => [s, { machines: count(s).length, cores: lanes(count(s)) }]));
  const official = held.filter((h) => h.id && h.id.startsWith("official:")).reduce((n, h) => n + h.n, 0);
  return { at, engine, release, summary, official_held: official, devices, queue, recent, hours };
}

/// WHO A MACHINE IS: its owner's name and what they call it, for a claimed one.
async function namesOf(env, ids) {
  const out = new Map();
  if (!env.ACCOUNTS || !ids.length) return out;
  const { results } = await env.ACCOUNTS.prepare(
    `SELECT d.verifier, d.label, a.username FROM devices d JOIN accounts a ON a.id = d.account
      WHERE d.verifier IN (SELECT value FROM json_each(?))`).bind(JSON.stringify(ids)).all();
  for (const r of results) out.set(r.verifier, r.label ? `${r.username} · ${r.label}` : r.username);
  return out;
}

const PAGE = `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex"><title>WFSim Ops</title>
<style>
:root { --bg: #f6f7f9; --card: #fff; --ink: #1b1f24; --mute: #6a737d; --line: #e3e6ea;
  --run: #1f9d55; --idle: #d69e2e; --stale: #c53030; --ref: #805ad5; --bar: #3b82f6; }
@media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) { --bg: #0f1216; --card: #171b21; --ink: #e6e9ee;
  --mute: #8b949e; --line: #2a3038; --bar: #60a5fa; } }
:root[data-theme="dark"] { --bg: #0f1216; --card: #171b21; --ink: #e6e9ee; --mute: #8b949e; --line: #2a3038; --bar: #60a5fa; }
* { box-sizing: border-box; }
body { margin: 0; background: var(--bg); color: var(--ink); font: 14px/1.45 system-ui, -apple-system, "Segoe UI", sans-serif; }
main { max-width: 1100px; margin: 0 auto; padding: 16px; }
h1 { font-size: 18px; margin: 0 0 4px; } h2 { font-size: 14px; margin: 20px 0 8px; color: var(--mute); text-transform: uppercase; letter-spacing: .04em; }
.sub { color: var(--mute); font-size: 12px; }
.cards { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 10px; }
.card { background: var(--card); border: 1px solid var(--line); border-radius: 10px; padding: 12px; }
.card b { display: block; font-size: 28px; font-variant-numeric: tabular-nums; }
.card span { color: var(--mute); font-size: 12px; }
.dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; margin-right: 6px; vertical-align: middle; }
.computing { background: var(--run); } .idle { background: var(--idle); } .stale { background: var(--stale); } .refused { background: var(--ref); }
.wrap { overflow-x: auto; background: var(--card); border: 1px solid var(--line); border-radius: 10px; }
table { border-collapse: collapse; width: 100%; font-variant-numeric: tabular-nums; }
th, td { padding: 6px 10px; border-bottom: 1px solid var(--line); text-align: left; white-space: nowrap; }
th { color: var(--mute); font-weight: 500; font-size: 12px; } tr:last-child td { border-bottom: 0; }
td.n, th.n { text-align: right; }
code { font: 12px ui-monospace, Consolas, monospace; }
svg { display: block; width: 100%; height: 140px; }
form { display: flex; gap: 8px; flex-wrap: wrap; } input { flex: 1; min-width: 200px; padding: 8px; border: 1px solid var(--line);
  border-radius: 8px; background: var(--card); color: var(--ink); } button { padding: 8px 14px; border-radius: 8px; border: 1px solid var(--line);
  background: var(--card); color: var(--ink); cursor: pointer; }
.err { color: var(--stale); margin-top: 8px; }
</style></head><body><main>
<h1>Community machines</h1><div class="sub" id="sub">—</div>
<div id="gate" hidden><h2>Secret</h2><form id="f"><input id="t" type="password" autocomplete="off" placeholder="OPS_TOKEN">
<button>Open</button></form><div class="err" id="err"></div></div>
<div id="body" hidden>
<h2>Now</h2><div class="cards" id="cards"></div>
<h2>Queue</h2><div class="wrap"><table id="queue"></table></div>
<h2>Last hour</h2><div class="cards" id="hour"></div>
<h2>Last 24 hours — machines that sent results, per hour</h2><div class="card"><svg id="chart" preserveAspectRatio="none"></svg><div class="sub" id="chartsub"></div></div>
<h2>Machines</h2><div class="wrap"><table id="devices"></table></div>
</div></main>
<script>
const KEY = "wfsim.ops.token", EVERY_MS = 15000;
const $ = (id) => document.getElementById(id);
const esc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
const fmt = (n) => (n ?? 0).toLocaleString();
const ago = (t) => { if (!t) return "—"; const s = Math.round((Date.now() - Date.parse(t)) / 1000);
  return s < 60 ? s + "s" : s < 3600 ? Math.round(s / 60) + "m" : Math.round(s / 3600) + "h"; };
const local = (t) => t ? new Date(t).toLocaleString() : "—";
let token = ""; try { token = localStorage.getItem(KEY) || ""; } catch (_) {}
let timer = null;
$("f").onsubmit = (e) => { e.preventDefault(); token = $("t").value.trim(); try { localStorage.setItem(KEY, token); } catch (_) {} load(); };
async function load() {
  clearTimeout(timer);
  if (!token) { $("gate").hidden = false; return; }
  let r;
  try { r = await fetch("/api/ops/pulse", { method: "POST", headers: { authorization: "Bearer " + token } }); }
  catch (_) { $("sub").textContent = "offline — retrying"; timer = setTimeout(load, EVERY_MS); return; }
  if (r.status === 401) { token = ""; try { localStorage.removeItem(KEY); } catch (_) {} $("gate").hidden = false; $("body").hidden = true;
    $("err").textContent = "That secret was refused."; return; }
  const p = await r.json();
  $("gate").hidden = true; $("body").hidden = false; $("err").textContent = "";
  draw(p);
  timer = setTimeout(load, EVERY_MS);
}
const LABEL = { computing: "Computing", idle: "Idle, asking for work", stale: "On an old engine", refused: "Refused" };
function draw(p) {
  $("sub").innerHTML = "engine <code>" + esc(p.engine) + "</code> · release <code>" + esc(p.release) + "</code> · " + esc(local(p.at)) + " · refreshes every 15s";
  const s = p.summary;
  $("cards").innerHTML = ["computing", "idle", "stale", "refused"].map((k) =>
    '<div class="card"><span><i class="dot ' + k + '"></i>' + LABEL[k] + "</span><b>" + fmt(s[k].machines) + "</b><span>" + fmt(s[k].cores) + " cores</span></div>").join("")
    + '<div class="card"><span>Held by the official machines</span><b>' + fmt(p.official_held) + "</b><span>questions</span></div>";
  const q = p.queue.slice().sort((a, b) => a.kind.localeCompare(b.kind) || a.state.localeCompare(b.state));
  $("queue").innerHTML = '<tr><th>kind</th><th>state</th><th class="n">waiting</th><th class="n">being computed</th><th class="n">past due</th><th>next due</th></tr>'
    + (q.length ? q.map((r) => "<tr><td>" + esc(r.kind) + "</td><td>" + esc(r.state) + '</td><td class="n">' + fmt(r.n) + '</td><td class="n">' + fmt(r.leased)
      + '</td><td class="n">' + fmt(r.past) + "</td><td>" + esc(local(r.next_due)) + "</td></tr>").join("") : '<tr><td colspan="6">nothing waiting</td></tr>');
  const v = p.recent.find((r) => !r.official) || {}, o = p.recent.find((r) => r.official) || {};
  $("hour").innerHTML = [["Results from volunteers", v.n], ["Machines that sent one", v.devices], ["Credited", v.credited],
    ["Answered after the fact", v.late], ["Results from official machines", o.n]]
    .map(([k, n]) => '<div class="card"><span>' + k + "</span><b>" + fmt(n) + "</b></div>").join("");
  chart(p.hours);
  $("devices").innerHTML = '<tr><th>state</th><th>machine</th><th>who</th><th class="n">cores</th><th class="n">holds</th><th class="n">results / h</th><th class="n">compute / h</th><th>asked</th><th>answered</th><th>engine</th></tr>'
    + p.devices.map((d) => '<tr><td><i class="dot ' + d.state + '"></i>' + d.state + "</td><td><code>" + esc(d.id) + "</code></td><td>" + esc(d.name || "—")
      + '</td><td class="n">' + (d.lanes ?? "—") + '</td><td class="n">' + (d.holds.board + d.holds.riven || "") + (d.holds.riven ? " (riven)" : "")
      + '</td><td class="n">' + fmt(d.tasks_hour) + '</td><td class="n">' + Math.round(d.ms_hour / 60000) + "m</td><td>" + ago(d.asked_at)
      + "</td><td>" + ago(d.answered_at) + "</td><td><code>" + esc(d.engine === p.engine ? "current" : d.engine || "—") + "</code></td></tr>").join("");
}
function chart(hours) {
  const svg = $("chart"), W = 1000, H = 140, n = hours.length || 1, max = Math.max(1, ...hours.map((h) => h.devices));
  svg.setAttribute("viewBox", "0 0 " + W + " " + H);
  const bw = W / n;
  svg.innerHTML = hours.map((h, i) => { const bh = (h.devices / max) * (H - 18);
    return '<rect x="' + (i * bw + 1) + '" y="' + (H - bh) + '" width="' + Math.max(1, bw - 2) + '" height="' + bh + '" style="fill:var(--bar)"><title>'
      + esc(new Date(h.hour + ":00:00Z").toLocaleString()) + ": " + h.devices + " machines, " + h.tasks + " results</title></rect>"; }).join("")
    + '<text x="4" y="12" font-size="11" style="fill:var(--mute)">' + max + "</text>";
  const last = hours[hours.length - 1];
  $("chartsub").textContent = last ? "Most recent hour (so far): " + last.devices + " machines, " + fmt(last.tasks) + " results. Hover a bar for its hour." : "";
}
load();
</script></body></html>`;
