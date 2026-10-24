// ---- The long image: a weapon's answer laid out to be sent as one picture ----
//
// docs/AGENT.md §"The QQ bot". `/weapons/<Name>/card?kind=pz|zk&ruler=&n=`, a
// zk narrowed by `&has=<stat ids>&malus=<id|any|none>`, a pz of riven builds by
// `&rv=1`. The card itself is `headlessLongCard` (87-headless.js), the one the
// bot draws with no page; this route fetches what it shows, draws it in the
// light theme with nothing else on the page, and sets `body[data-card-ready]`
// once every number in it has arrived.

/// A BUILD'S OWN RECORD (`headlessMeasuredRecord`).
const measuredRecordHtml = (row, words = true) => headlessMeasuredRecord(pageSay(), row, words);

/// …SAID ONCE BESIDE A LIST OF THEM, so nobody reads an old record as today's.
function measuredNote() { return tr("Every build shows when it was measured and by which WFSim engine. The game and WFSim both change, so an older record may be behind."); }

/// What the address asks for (`headlessCardAsk`).
const cardParams = () => headlessCardAsk(location.search);

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
  const riven = (loadPresetList(RIVENS).find((p) => p.id === id) || {}).state || {};
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
  return { ok: true, score, shown: fmtScore(score), state, riven, metric: met.id,
    top: free ? headlessCardShown(free.row) : null, gain: free ? score / free.row.score - 1 : null,
    of: rivens.length, rank: above + 1, runs,
    starts: builtinSearchStarts(w.id, RIVEN_PREFIX + id).length, candidate_runs: preset.candidate_runs || null,
    board_at: boardMeasuredAt(rows) || null, at: new Date().toISOString(),
    ...headlessRivenGrade(score, free ? free.row.score : 0, rivens.length ? rivens[0].row.score : 0) };
}

const cardBox = (head, body) => headlessCardBox(head, body);

/// `ask` is `cardParams()` read BEFORE the weapon opened: opening one restores
/// its saved build, which rewrites the address and takes the query with it.
async function renderCardPage(w, ask) {
  const box = $("card-page");
  if (!box || !w) return;
  document.body.removeAttribute("data-card-ready");
  const { kind, code, result } = ask;
  // AN APPRAISAL NAMES ITS OWN RULER (81-appraisal.js), read from its door.
  const job = kind === "appraise" ? await fetch(`/api/appraise/${encodeURIComponent(code)}${result ? `?result=${result}` : ""}`)
    .then((r) => (r.ok ? r.json() : null)).catch(() => null) : null;
  // AN APPRAISAL'S ANSWER, replayed here with the asker's real rolls — the
  // number a chat is told is this page's, never the searcher's.
  const verdict = kind === "appraise" && job && job.result ? await appraisalReplay(w, job) : null;
  if (verdict) {
    const { ok, score, shown, gain, top, of, rank, reason, grade, best, best_riven, runs, starts, candidate_runs, board_at, at } = verdict;
    document.body.dataset.verdict = JSON.stringify({ ok, score, shown, gain, top, of, rank, reason,
      grade, best, best_riven, runs, starts, candidate_runs, board_at, at });
  }
  const card = headlessLongCard(pageSay(), w, ask, { rows: BOARD[w.id] || [], job, verdict, origin: LIVE_ORIGIN,
    riven: rivenCardGot });
  const qr = await api("/api/qr", { text: card.link });
  // THE CARDS ARE DRAWN ONLY WHEN THEY GO STRAIGHT ONTO THE PAGE: a riven's
  // values arrive into the elements already there, so nothing may sit between.
  box.classList.toggle("lc-way-in", card.wayIn);
  box.innerHTML = card.draw(qr && qr.svg);
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
  const n = Math.max(1, Math.min(HEADLESS_CARD_MAX, Math.round(Number(p.get("n"))) || 5));
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
