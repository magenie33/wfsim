// ---- The long image: a weapon's answer laid out to be sent as one picture ----
//
// docs/AGENT.md §"The QQ bot". `/weapons/<Name>/card?kind=pz|zk&ruler=&n=`,
// a zk narrowed by `&has=<stat ids>&malus=<id|any|none>`, a pz of riven builds by `&rv=1`, draws the bots' reply image from the page's own components — the simulator's
// build card and the board's published numbers — so a chat image and the site
// are one design. Every mode of the ruler is one ranking (`pooledRanking`). Phone width, the light theme, nothing else on the page;
// `body[data-card-ready]` is set once every number in it has arrived, which is
// when a screenshot may be taken. The footer's code opens the page it is about.

const CARD_MAX = 10;
const WFSIM_COMMIT = "https://github.com/magenie33/wfsim/commit/";

/// A BUILD'S OWN RECORD: when it was measured and by which version of WFSim.
/// Each build carries its own — a weapon's builds are measured at different
/// times by different versions, and a rescore replaces them one by one.
function measuredRecordHtml(row, link = true, words = true) {
  const r = row || {};
  const at = r.measured_at ? measuredText(r.measured_at) : "";
  return [
    at ? escHtml(words ? trF("measured {t}", { t: at }) : at) : "",
    r.measured_by ? `${words ? escHtml(tr("version")) + " " : ""}${link
      ? `<a href="${WFSIM_COMMIT}${escHtml(r.measured_by)}" target="_blank" rel="noopener">${escHtml(r.measured_by)}</a>`
      : escHtml(r.measured_by)}` : "",
  ].filter(Boolean).join(" · ");
}

/// …SAID ONCE BESIDE A LIST OF THEM, so nobody reads an old record as today's.
function measuredNote() { return tr("Every build shows when it was measured and by which version of WFSim. The game and WFSim both change, so an older record may be behind."); }

/// What the address asks for, clamped to what a card shows.
function cardParams() {
  const p = new URLSearchParams(location.search);
  const n = Math.max(1, Math.min(CARD_MAX, Math.round(Number(p.get("n"))) || 3));
  const kind = ["zk", "appraise"].includes(p.get("kind")) ? p.get("kind") : "pz";
  return { kind, ruler: p.get("ruler") || "", n, code: (p.get("code") || "").toUpperCase(), result: Number(p.get("result")) || 0,
    riven: { bonuses: (p.get("has") || "").split(",").filter(Boolean), malus: p.get("malus") || null },
    withRiven: p.get("rv") === "1" };
}

/// THE REPLAY: the board's own door judges the build, then the official fight
/// runs it with the asker's card at its real rolls, scored the way the board
/// scores (`metricValue`). Against the board: its riven-free leader for this
/// ruler, and where the card would stand among the rivens it has measured.
async function appraisalReplay(w, job) {
  const build = job.result.build || {};
  const check = await api("/api/board/check", build).catch(() => null);
  if (!check || check.ok === false || check.accepted === false) {
    return { ok: false, reason: (check && check.reason) || tr("the board could not judge this build") };
  }
  await agentDo("shell.preset.open", { bar: "scenario", preset: job.ruler });
  const rv = job.riven || {};
  const id = newRiven({ bonuses: rv.bonuses || [], malus: rv.malus || null,
    rank: rv.rank != null ? rv.rank : rivenRules().max_rank, polarity: rv.polarity || "madurai" });
  // NAMELESS, so the card reads "Riven" the way a board row's does, not a
  // name this page made up.
  storePresetList(RIVENS, loadPresetList(RIVENS).map((p) => (p.id === id ? { ...p, name: "" } : p)));
  const state = boardRowState(w, { ...build, mods: (build.mods || []).map((m) => (m === BOARD_RIVEN_SLOT ? RIVEN_PREFIX + id : m)) });
  // THE RULER'S OWN RUNS, not the reader's setting: the grade divides this
  // number by the board's, and a board row is fought at the ruler's runs.
  const runs = (((scenarioNamed(activeScenario) || {}).state) || {}).runs || simRuns();
  const r = await simulateFleet({ ...tennoPayload(), ...seatPayload(state), rivens: rivenPayload(), ...theFight({ runs }) });
  if (!r || r.ok === false) return { ok: false, reason: (r && r.error) || tr("the replay did not run") };
  const met = metricOf((scenarioNamed(activeScenario) || {}).metric);
  const score = metricValue(met, r);
  const rows = (BOARD[w.id] || []).filter((x) => x.benchmark === job.ruler);
  const free = pooledRanking(rankBoard(rows.filter((x) => !x.riven), [job.ruler], w.modes))[0];
  const rivens = pooledRivens(META, w, rows, job.ruler).rivens;
  const above = rivens.filter((x) => x.row.score > score).length;
  const preset = ((META.search_presets || {}).presets || []).find((p) => p.riven) || {};
  return { ok: true, score, shown: fmtScore(score), state, metric: met.id,
    top: free ? cardShown(free.row) : null, gain: free ? score / free.row.score - 1 : null,
    of: rivens.length, rank: above + 1, runs,
    starts: builtinSearchStarts(w.id, RIVEN_PREFIX + id).length, candidate_runs: preset.candidate_runs || null,
    board_at: boardMeasuredAt(rows) || null, at: new Date().toISOString(),
    ...rivenGrade(score, free ? free.row.score : 0, rivens.length ? rivens[0].row.score : 0) };
}

/// THE GRADE, 0 to 100 and linear in the gain: 100 × (card − the riven-free
/// leader) ÷ (the board's #1 − that leader), rounded down. At or past #1 is
/// 100, so the first riven measured where the board has none is 100 too; no
/// better than going without one is 0. `best` is the #1 the grade was read against.
function rivenGrade(score, free, riven) {
  const best = Math.max(free, riven);
  const grade = score >= best ? 100 : score <= free ? 0 : Math.floor(100 * (score - free) / (best - free));
  return { grade, best: fmtScore(best), best_riven: riven > free };
}

/// WHAT THE GRADE STOOD ON, every term of it: the formula, the two numbers it
/// divides, how many rivens the board had, when, and what is approximate.
function rivenGradeNotes(v) {
  const when = (t) => {
    const zone = new Intl.DateTimeFormat(accountLocale(), { timeZoneName: "short" }).formatToParts(new Date(t)).find((x) => x.type === "timeZoneName");
    return `${measuredText(t)}${zone ? ` (${zone.value})` : ""}`;
  };
  return [
    tr("Grade = 100 × (this card − the best build without a riven) ÷ (the board's #1 − the best build without a riven), linear and rounded down; at or past the #1 is 100, no better than going without a riven is 0."),
    !v.of ? tr("The board has measured no riven of this weapon under this ruler: this card is the first, so it is 100.")
      : trF("Read against: the board's #1 {best} ({which}), the best build without a riven {free}, and the {n} rivens the board has measured under this ruler, each at its god roll (every positive stat at its ceiling, the negative at its floor).",
        { best: v.best, which: v.best_riven ? tr("with a riven") : tr("without a riven"), free: v.top == null ? "—" : v.top, n: v.of }),
    v.board_at ? trF("The board as last measured {t}; graded {g}.", { t: when(v.board_at), g: when(v.at) }) : trF("Graded {g}.", { g: when(v.at) }),
    trF("Approximate: this card's build is a quick search ({k} starts, {c} fights a candidate, one build kept), not proven its best, so the grade is a floor. Its number is the official fight, {runs} fights, at the card's own rolls, the same fight the board's rows are measured in.",
      { k: v.starts, c: v.candidate_runs, runs: v.runs }),
  ];
}

const cardShown = (r) => String(r.shown != null ? r.shown : (r.score || 0).toFixed(4));
const cardRecord = (row) => {
  const t = measuredRecordHtml(row, false);
  return t ? `<span class="lc-when">${t}</span>` : "";
};
const cardBox = (head, body) => `<section class="lc-box"><div class="lc-head">${head}</div>${body}</section>`;

/// `ask` is `cardParams()` read BEFORE the weapon opened: opening one restores
/// its saved build, which rewrites the address and takes the query with it.
async function renderCardPage(w, ask) {
  const box = $("card-page");
  if (!box || !w) return;
  document.body.removeAttribute("data-card-ready");
  const { kind, n, riven: rivenAsk, withRiven, code, result } = ask;
  // AN APPRAISAL NAMES ITS OWN RULER (81-appraisal.js), read from its door.
  const job = kind === "appraise" ? await fetch(`/api/appraise/${encodeURIComponent(code)}${result ? `?result=${result}` : ""}`)
    .then((r) => (r.ok ? r.json() : null)).catch(() => null) : null;
  const want = job ? job.ruler : ask.ruler;
  const rows = BOARD[w.id] || [];
  const benches = META.benchmarks || [];
  const bench = benches.find((b) => b.id === want) || benches.find((b) => b.primary) || benches[0];
  const ruler = bench ? bench.id : "";
  const metric = metricLabel(metricOf(((bench || {}).scenario || {}).metric));
  const pct = (x) => (x >= 0 ? "+" : "−") + Math.abs(x * 100).toFixed(1) + "%";
  // THE CARDS ARE DRAWN ONLY WHEN THEY GO STRAIGHT ONTO THE PAGE: a riven's
  // values arrive into the elements already there, so nothing may sit between.
  let title = "", link = "", body = () => "";
  // AN APPRAISAL'S ANSWER, replayed here with the asker's real rolls — the
  // number a chat is told is this page's, never the searcher's.
  const verdict = kind === "appraise" && job && job.result ? await appraisalReplay(w, job) : null;
  if (verdict) {
    const { ok, score, shown, gain, top, of, rank, reason, grade, best, best_riven, runs, starts, candidate_runs, board_at, at } = verdict;
    document.body.dataset.verdict = JSON.stringify({ ok, score, shown, gain, top, of, rank, reason,
      grade, best, best_riven, runs, starts, candidate_runs, board_at, at });
  }
  if (verdict) {
    title = trF("{w} · Riven gain", { w: w.name });
    link = `${LIVE_ORIGIN}${weaponPath(w.id)}/riven-analyst`;
    const who = job.result.thanks || tr("a kind someone in the chat");
    body = () => !verdict.ok ? `<p class="sim-empty">${escHtml(verdict.reason || "")}</p>`
      : cardBox(`<b class="lc-score">${escHtml(verdict.shown)}</b><span class="sb-empty">${escHtml(metric)}</span>${
        verdict.gain == null ? "" : `<b class="lc-gain">${pct(verdict.gain)}</b>`}`, cardOfState(verdict.state, w))
      + cardBox(`<span class="sb-h">${escHtml(tr("Riven grade"))}</span><b class="lc-score">${verdict.grade}</b><span class="sb-empty">/ 100</span>`,
        rivenGradeNotes(verdict).map((x) => `<p class="lc-note">${escHtml(x)}</p>`).join(""))
      + cardBox(`<span class="sb-h">${escHtml(tr("Against the board"))}</span>`,
        `<p class="lc-note">${escHtml(verdict.top == null ? tr("The board has no riven-free build of this weapon yet.")
          : trF("The board's best riven-free build: {s}", { s: verdict.top }))}</p>`
        + (verdict.of ? `<p class="lc-note">${escHtml(trF("Among the {n} rivens the board has measured (each at its best rolls), this card would rank #{k}.", { n: verdict.of, k: verdict.rank }))}</p>` : "")
        + `<p class="sb-empty">${escHtml(trF("Searched by {who}. Thank you!", { who }))}</p>`);
  } else if (kind === "appraise") {
    // THE ASKER'S CARD AND THE WAY IN: whoever scans the code searches it.
    title = trF("{w} · Riven gain", { w: w.name });
    link = `${LIVE_ORIGIN}/appraise/${code}`;
    body = () => !job ? "" : cardBox(`<span class="sb-h">${escHtml(tr("Riven gain code"))}</span><b class="lc-score">${escHtml(code)}</b>`,
      rivenSpecCardHtml(w, { name: "", spec: { ...job.riven, polarity: job.riven.polarity || "madurai" } })
      + `<p class="lc-note">${escHtml(tr("Scan the code below: your own browser searches this riven's best build, and Nona posts the result in the chat."))}</p>`
      + `<p class="sb-empty">${escHtml(link.replace(/^https?:\/\//, ""))}</p>`);
  } else if (kind === "pz") {
    const ranked = pooledRanking(rankBoard(rows.filter((r) => r.benchmark === ruler && !r.riven === !withRiven), [ruler], w.modes));
    const shown = distinctTop(ranked, n, () => "");
    const mode = (shown[0] || {}).mode;
    title = trF(withRiven ? "{w} · top {n} riven builds" : "{w} · top {n} builds", { w: w.name, n: shown.length });
    link = `${LIVE_ORIGIN}${weaponPath(w.id)}?bench=${encodeURIComponent(ruler)}&mode=${encodeURIComponent(mode || "base")}&riven=${withRiven ? 1 : 0}`;
    body = () => shown.map((x) => cardBox(`<b class="lc-rank">#${x.rank}</b><b class="lc-score">${escHtml(cardShown(x.row))}</b><span class="sb-empty">${escHtml(metric)}${
      x.tied > 1 ? ` · ${escHtml(trF("{n} builds tie", { n: x.tied }))}` : ""}</span>${cardRecord(x.row)}`,
      cardOfState(boardRowState(w, x.row), w))).join("");
  } else {
    const g = pooledRivens(META, w, rows, ruler);
    title = trF("{w} · Riven Analyst", { w: w.name });
    link = `${LIVE_ORIGIN}${weaponPath(w.id)}/riven-analyst`;
    body = () => !g.rivens.length ? "" : (g.top ? cardBox(`<span class="sb-h">${escHtml(tr("The board's best riven-free build"))}</span><b class="lc-score">${escHtml(cardShown(g.top.row))}</b>${cardRecord(g.top.row)}`,
      cardOfState(boardRowState(w, g.top.row), w)) : "")
      + distinctTop(placed(g.rivens.filter((x) => rivenMatches(x.row.riven, rivenAsk)), (x) => x.row.score || 0), n, () => "")
        .map((x) => cardBox(`<b class="lc-rank">#${x.rank}</b><b class="lc-score">${escHtml(cardShown(x.row))}</b>${
        x.gain == null ? "" : `<b class="lc-gain">${pct(x.gain)}</b>`}${cardRecord(x.row)}`, cardOfState(boardRowState(w, x.row), w))).join("");
  }
  const updated = kind === "appraise" ? "" : measuredText(boardMeasuredAt(rows.filter((r) => r.benchmark === ruler)));
  const qr = await api("/api/qr", { text: link });
  const cards = body();
  // THE SITE'S OWN WORDMARK, as the topbar draws it: the brand is WFSim, and
  // Nona speaks in the line the bot sends with the image, not on it.
  // THE CODE IS THE POINT of an appraisal's first picture — people long-press it
  // on a phone — so there, and only there, it is drawn large.
  box.classList.toggle("lc-way-in", kind === "appraise" && !verdict);
  box.innerHTML = `<header class="lc-top"><span class="brand">WF<span>Sim</span></span><span class="sb-empty">wfsim.app</span></header>
    <h1 class="lc-title">${escHtml(title)}</h1>
    <div class="sb-empty lc-sub">${escHtml([bench ? tr(bench.name) : "", updated ? trF("updated {t}", { t: updated }) : ""]
      .filter(Boolean).join(" · "))}</div>
    ${cards || `<p class="sim-empty">${escHtml(tr("Nothing measured for this weapon yet."))}</p>`}
    <footer class="lc-foot"><div class="lc-qr">${qr && qr.svg ? qr.svg.replace(/^<\?xml[^>]*>/, "") : ""}</div>
      <div><b class="lc-slogan">${escHtml(tr("The real Simulacrum Prime."))}</b><div class="sb-empty">wfsim.app</div></div></footer>`;
  // READY WHEN NOTHING IS STILL ARRIVING: every riven's values, every picture —
  // fetched NOW, since a lazy picture below the fold of a screenshot never is.
  while (rivenCardPending.size) await Promise.allSettled([...rivenCardPending]);
  box.querySelectorAll("img").forEach((im) => { im.loading = "eager"; });
  await Promise.all([...box.querySelectorAll("img")].map((im) => (im.complete ? null
    : new Promise((ok) => { im.onload = ok; im.onerror = ok; }))));
  document.body.dataset.cardReady = "1";
}

/// THE CONTRIBUTORS' LONG IMAGE: `/contributors/card?n=` the three rankings'
/// first `n`, or `?name=` one person's place and points on each — the rows
/// drawn as the ranking page draws them (`17-account.js`), a name only for an
/// account that agreed to show it.
async function renderContributorsCard() {
  const box = $("card-page");
  if (!box) return;
  document.body.removeAttribute("data-card-ready");
  const p = new URLSearchParams(location.search);
  const name = (p.get("name") || "").trim();
  const n = Math.max(1, Math.min(CARD_MAX, Math.round(Number(p.get("n"))) || 5));
  const get = (q) => fetch(`/api/contributors${q}`).then((r) => (r.ok ? r.json() : null)).catch(() => null);
  const periods = [["week", "week", "Weekly ranking"], ["recent", "recent", "Monthly ranking"], ["all", "points", "All-time ranking"]];
  const num = (x) => escHtml((x || 0).toLocaleString(accountLocale()));
  const who = (c) => (c.name === null ? `<span class="rank-name muted">${aT("Anonymous contributor")}</span>`
    : `<span class="rank-name">${escHtml(c.name)}</span>${extHookNow("contributorMark", c.mark) || ""}`);
  let title, body, computing = 0;
  if (name) {
    const r = await get(`?name=${encodeURIComponent(name)}`);
    const me = r && r.person;
    computing = (r && r.computing) || 0;
    title = trF("{name}'s contribution", { name });
    body = !me ? `<p class="sim-empty">${escHtml(tr("Nobody on the ranking shows that name."))}</p>`
      : `<div class="lc-sub">${contributorRankBar(me.contributor_rank)}</div><div class="rank-who lc-sub">${extHookNow("contributorMark", me.mark) || ""}</div>`
        + periods.map(([id, key, label]) => cardBox(`<span class="sb-h">${aT(label)}</span>`,
          me.ranks[id] ? `<div class="lc-head"><b class="lc-rank">#${me.ranks[id]}</b><b class="lc-score">${num(me[key])}</b><span class="sb-empty">${aT("Points")}</span></div>`
            : `<p class="sb-empty">${aT("Not on this ranking yet.")}</p>`)).join("");
  } else {
    const all = await Promise.all(periods.map(([id]) => get(id === "all" ? "" : `?period=${id}`)));
    computing = (all[0] && all[0].computing) || 0;
    title = tr("Contribution ranking");
    body = periods.map(([, key, label], i) => {
      const rows = ((all[i] && all[i].contributors) || []).slice(0, n);
      return cardBox(`<span class="sb-h">${aT(label)}</span>`, rows.length
        ? `<ol class="rank-list">${rows.map((c, k) => `<li class="rank-row${k < 3 ? " top" : ""}${contributorRowMark(c)}"><span class="rank-n">${k + 1}</span>`
          + `<span class="rank-who">${contributorRankBadge(c.contributor_rank)}${who(c)}</span><span class="rank-pts">${num(c[key])}</span></li>`).join("")}</ol>`
        : `<p class="sb-empty">${aT("Nobody yet.")}</p>`);
    }).join("");
  }
  const link = `${LIVE_ORIGIN}/contributors`;
  const qr = await api("/api/qr", { text: link });
  box.innerHTML = `<header class="lc-top"><span class="brand">WF<span>Sim</span></span><span class="sb-empty">wfsim.app</span></header>
    <h1 class="lc-title">${escHtml(title)}</h1>
    <div class="sb-empty lc-sub">${computing ? escHtml(trF("{n} devices are computing WFSim together right now", { n: computing })) : ""}</div>
    ${body}
    <footer class="lc-foot"><div class="lc-qr">${qr && qr.svg ? qr.svg.replace(/^<\?xml[^>]*>/, "") : ""}</div>
      <div><b class="lc-slogan">${escHtml(tr("The real Simulacrum Prime."))}</b><div class="sb-empty">wfsim.app</div></div></footer>`;
  document.body.dataset.cardReady = "1";
}
