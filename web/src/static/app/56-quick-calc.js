// ---- the quick calc runs WIDE ------------------------------------------
//
// A scan is one simulate per candidate and a mod slot has ~80 candidates once
// family conflicts are dropped, each at 10 runs — so ~810 full engagements.
//
// A nearly-full build is far worse than a bare one because the candidate COUNT
// barely moves while each ENGAGEMENT does: seven mods means multishot, status
// and elements all live, so one fight generates several times the procs, DoT
// stacks and ticks. The per-sim cost is a function of how much is HAPPENING.
//
// The lever is PARALLELISM and only that — the run count is 10 by decision and
// the engagement DURATION is never the lever. A lane is THE PAGE'S POOL, the
// same lanes the simulator shards across and `api` picks from; on the native
// server there is no worker to own, so a lane is plain `api` and the fetches
// parallelise by themselves.
//
// PINNABLE, because a worker is opaque from the page: `check_one_fight` and
// `check_gain_freshness` point the scan back through `api` to read what it
// actually sent.
let gainPool = null;
let nativeLanes = null;
async function gainLanes() {
  if (gainPool) return gainPool;
  if (WASM) return lanes();
  if (!nativeLanes) {
    nativeLanes = Array.from({ length: poolSize() },
      () => ({ call: (p, b) => api(p, b) }));
  }
  return nativeLanes;
}

// The generation of the scan that is allowed to write `gainScan`. Bumped on
// every start, so an older scan discovers at its next await that it has been
// superseded and stands down.
//
// WHY it has to be interruptible: a scan is ~90 SERIAL simulate calls, and the
// old guard was "a scan is running, so ignore this". Editing the fight midway
// therefore did not queue and did not cancel — it was DROPPED, the whole stale
// scan ran to the end under the config you had just left, and only then did
// the refresh notice the key had moved and start again. Every rapid edit paid
// for a full measurement of a question nobody was asking any more. Cancelling at the next await bounds that to one sim.
// WHICH ENEMY THE DEBUFF TABLE IS ABOUT — an index into `replay.tracked`, 0
// being the body the weapon was on.
//
// OUTSIDE the render for the reason every fold on that panel is: a panel that
// resets your selection on every Run Sim is a panel you re-select on every Run
// Sim.
let replayFoe = 0;
/// …CLAMPED TO WHAT THIS REPLAY ACTUALLY FOLLOWED. Three functions ask it —
/// the markup, the frame reader and the wiring — and they run at different
/// times, so it is derived rather than passed. It was written out three times
/// for an hour and the third copy was in a function the other two's binding
/// could not reach, which took the whole panel down with "dBody is not
/// defined" rather than drawing a wrong number: it announced itself.
const replayFoeIdx = (rp) =>
  Math.min(replayFoe, Math.max(0, ((rp && rp.tracked) || [""]).length - 1));
// WHOSE BUFFS THE BUFF TABLE IS ABOUT — an index into `replay.combatants`, 0
// being the wielder. The mirror of `replayFoe` on the other side of the fight,
// and outside the render for the same reason.
//
// A BUFF IS A SEAT'S OR IT IS NOBODY'S: two seats are two builds, so they have
// two rosters and two piles, and one table can only ever be one of them.
let replaySeat = 0;
const replaySeatIdx = (rp) =>
  Math.min(replaySeat, Math.max(0, ((rp && rp.buffs) || [[]]).length - 1));

let gainGen = 0;
// An axis whose scan was dropped because another axis was mid-flight. One slot:
// the newest asker wins, and it is consumed when the running scan finishes.
let gainPending = null;

/// THE FIGHT THIS SCAN IS ABOUT — the one it answered, or the one it is
/// answering. Two states, because the key is now stamped on COMPLETION: a scan
/// in flight has only a `want`, and a scan that died has neither.
const gainAbout = () => gainScan.key || (gainScan.running ? gainScan.want : null);

/// END A SCAN AND SAY WHY, which is the whole of what was missing. Every exit
/// dropped `running` and left the reader with a frozen counter and no reason —
/// so a scan that lost its lanes and one that finished looked the same.
/// THE ONE PLACE THE QUICK CALC IS DRAWN.
///
/// NOTHING THAT CHANGES THE SCAN DRAWS IT. Every mutation marks the surface
/// dirty and this runs on a TRAILING timer, whose defining property is that it
/// always runs once more AFTER the last change — a throttle applied at the
/// write cannot, which is how a finished scan kept a frame several answers
/// short while its own books balanced. Three surfaces depend on the scan and
/// the bugs were all the same shape: a new exit path repainted two of them, or
/// none. A mutation site that cannot paint cannot forget to.
let calcPainter = null, calcPaintAt = 0;
const CALC_FRAME_MS = 250;
function paintCalc() {
  calcPaintAt = Date.now();
  renderQuickCalc();
  renderCalcStatus();
  const r = gainScan.repaint;
  if (r) { try { r(); } catch (_) { /* the list it belongs to is gone */ } }
  // …AND THE ONE THAT GAVE WAY TAKES ITS TURN HERE, in the frame that is
  // guaranteed to happen, rather than inside a callback that a throttle can
  // swallow — which is how a parked axis was never started at all.
  if (!gainScan.running && gainPending) {
    const p = gainPending;
    gainPending = null;
    ensureGains(p.axis, p.repaint);
  }
}
function calcDirty() {
  gainScan.beat = Date.now();
  if (calcPainter) return;
  const wait = Math.max(0, CALC_FRAME_MS - (Date.now() - calcPaintAt));
  calcPainter = setTimeout(() => { calcPainter = null; paintCalc(); }, wait);
}

/// IS A SCAN ACTUALLY WORKING — asked instead of `gainScan.running`.
///
/// `running` is asserted by one site and has to be cleared by EVERY exit, so
/// any exit nobody thought of latches it, and a latched flag refused that list
/// for the life of the page. Every hang this file has had was one cause of
/// that; there is always another cause. A refusal that expires ends the whole
/// CLASS: whatever latches it, the calculator answers again a few seconds
/// later. The watchdog on a lane is then about being PROMPT, not about being
/// correct.
///
/// A working scan beats through `calcDirty` — per candidate, and through the
/// baseline's own progress, so a long fight is live rather than merely slow.
const CALC_STALE_MS = 20000;
const scanIsLive = () => gainScan.running
  && Date.now() - (gainScan.beat || 0) < CALC_STALE_MS;

function gainStop(why) {
  gainScan.running = false;
  gainScan.note = why || gainScan.note;
  gainScan.failed = !!why;
  calcDirty();
}

async function scanGains(axis, repaint) {
  const gen = ++gainGen;
  const live = () => gen === gainGen;
  gainAxis = axis;
  const { name, scenario, refine } = gainScenario();
  // The note is WHICH FIGHT this was measured in, and nothing else: each
  // chip's tooltip states its own run count, which is the only place the
  // number changes how a reading should be taken.
  // THE CANDIDATES ARE ENUMERATED BEFORE THE FIRST RUN, so a list drawn while
  // the base fight is still going already knows how many of ITS OWN rows are
  // coming. One request, no simulation — it costs a round trip and no runs.
  const cands = await gainCandidates(axis);
  if (!live()) return;
  // THE KEY IS NOT STAMPED YET. `ensureGains` returns early when the key
  // matches, so stamping it at the START meant a scan that died half way —
  // lanes gone, an exception — left the page believing this fight was already
  // answered, and no later request ever re-asked. It is written at the end, and
  // only when the scan actually finished; `want` is what a live scan is FOR,
  // which is what the interrupt check compares against.
  gainScan = { key: null, want: gainKey(), axis, running: true, base: 0, floor: 0,
    phase: "", by: {}, refused: {}, done: 0, repaint, beat: Date.now(),
    total: cands.length + (refine ? Math.min(GAIN_REFINE_TOP, cands.length) + 1 : 0),
    ids: new Set(cands.map((c) => c.id)), note: name, metric: "", lanesLost: 0 };
  // THE BAR APPEARS WHEN THE WORK STARTS, not when the first candidate lands.
  // The BASE fight runs before any candidate does, and on a crowd at a real run
  // count that is tens of seconds — throughout which `running` is already true
  // and nothing had repainted the list. So the list a reader had just opened
  // sat silent through exactly the wait the strip exists to explain, and the
  // bar turned up only once there was already an answer to show.
  calcDirty();
  // THE SCENARIO'S OWN METRIC, AND NOTHING ELSE.
  //
  // There is no fallback to another one, and there must not be: a ruler
  // declares what it measures, and a ranking quietly produced in a different
  // unit answers a question nobody asked. It cost a second full baseline to do
  // it, and the day a ruler measures something that is neither — how fast a
  // build applies status TYPES, say — a hard-coded pair is not a fallback, it
  // is a wrong answer.
  //
  // IT NEVER HAD ANYTHING TO FALL BACK FOR. Kill progress is not a kill COUNT:
  // it is "kills plus the depleted fraction of the current target's pool", so a
  // build that kills nothing still scores what it drained. Zero means the build
  // dealt no damage at all — and a build that deals no damage has no DPS
  // either, so the second baseline measured the same nothing.
  // BY THE SCENARIO'S OWN METRIC, whatever it is. `per_minute` is the question
  // that decides which baseline is read — a rate over the engagement against a
  // field the run reports directly — and it is the metric's to answer.
  const useKills = metricOf(scenario.metric).per_minute;
  // THE BASELINE IS SHARDED, like every other simulation the page runs.
  //
  // It went through `api`, which takes ONE lane — so the step every candidate
  // waits on ran single-threaded while the rest of the pool idled, and the
  // candidates that follow it use the whole machine. On a crowd at a real run
  // count that is tens of seconds of a fifteen-core machine doing nothing, and
  // it is the whole of what "stuck at 0/77" was.
  //
  // `simulateFleet` falls back to one call where sharding would not pay, and
  // the merge is `simulate_merged` — exactly what one worker would have
  // produced — so this is the same answer, sooner.
  const run = async (override, phase) => {
    const label = phase || "";
    gainScan.phase = label;
    calcDirty();
    // THE BASELINE SAYS HOW FAR IT HAS GOT. Every candidate waits on this one
    // call, so a scan sitting at 0/77 is not stuck — it is measuring the thing
    // the whole ranking is relative to, and saying nothing through it is what
    // made that read as a hang. The beat is also what keeps `scanIsLive` true
    // across a long fight, so this is load-bearing rather than decoration.
    const r = await simulateFleet(
      { ...buildPayload(), ...scenario, ...override },
      (d, total) => {
        gainScan.phase = label ? `${label} ${d}/${total}` : `${d}/${total}`;
        calcDirty();
      },
      { background: true });
    if (!r || !r.ok) return null;
    return readGain(r, useKills);
  };
  let base = await run({}, tr("measuring the baseline"));
  if (!live()) return;
  if (!base?.v) { gainStop("the base fight did not measure"); return; }
  gainScan.base = base.v;
  gainScan.metric = useKills ? tr("kill rate") : tr("DPS");
  // HOW LONG THE REST WILL TAKE, from the rate this scan is actually going.
  //
  // NOT FROM THE BASELINE, which is the first real fight of the page and pays
  // for wasm tiering up on a hot numeric loop — measured at about 4x, so an
  // estimate built on it over-predicts fivefold. Nor from a model of the pool:
  // throughput already contains the lane count, the machine, the fight and
  // whatever else the browser is doing, and it corrects itself as any of them
  // change.
  //
  // AFTER A FEW, because one candidate's wall time is mostly the round trip.
  const candAt = Date.now();
  const reckon = () => {
    if (gainScan.done < 3) return;
    const per = (Date.now() - candAt) / gainScan.done;
    gainScan.etaMs = Math.round(per * Math.max(0, gainScan.total - gainScan.done));
  };
  // ...and how far this same build moves on luck alone — the RESOLUTION the
  // server measured across the runs it was already paid for, not a second run
  // at another seed. See `readGain`.
  gainScan.floor = base.se / base.v;
  // …AND THE CANDIDATES ARE THE COUNTER'S OWN, so the phase line steps aside.
  gainScan.phase = "";
  // One shared cursor, every lane pulling the next candidate as it frees up —
  // so a slow candidate delays itself and nothing else. `live()` is checked
  // after each await, which is what bounds an interrupted scan to one
  // outstanding sim per lane rather than the whole queue.
  let cursor = 0;
  await Promise.all((await gainLanes()).map(async (lane) => {
    for (;;) {
      if (!live()) return;
      // A PERSON'S SIMULATE GOES FIRST. Checked before the candidate is taken,
      // so the scan gives up the machine between candidates rather than holding
      // a lane through one.
      await yieldToForeground(live);
      if (!live()) return;
      const c = cands[cursor++];
      if (!c) return;
      const r = await laneAsk(lane, "/api/simulate",
        { ...buildPayload(), ...scenario, ...c.payload }, live);
      if (!live()) return;               // the fight moved — this answer is stale
      // THE POOL REFUSED TWICE. Counted rather than silent: this lane's share
      // of the candidates goes unmeasured, and a ranking with holes in it has
      // to say so — the sweep below re-asks for them on a fresh worker.
      if (r === null) { gainScan.lanesLost++; return; }
      const g = readGain(r, useKills);
      gainScan.done++;
      reckon();
      if (g) {
        gainScan.by[c.id] = { ...gainOver(g, base), runs: scenario.runs };
      } else if (r.error) {
        // THE ENGINE ANSWERED, AND THE ANSWER IS NO — which is not a failure.
        //
        // A mod the weapon cannot take with these evolutions ("it needs the
        // same trigger on every firing mode") was counted as a HOLE, so the
        // scan could never be complete, never stamped its key, and re-ran on
        // every request for the life of the page. One legal refusal in eighty
        // was enough: the Torid's list never settled.
        //
        // Recorded rather than dropped, because the reader asked what this
        // option is worth and "you cannot equip it" is the answer.
        gainScan.refused[c.id] = String(r.error);
      }
      calcDirty();
    }
  }));
  if (!live()) return;
  // SECOND PASS. One run ranks the field cheaply but cannot separate its top
  // few — so the leaders are asked again with more, against a baseline
  // measured the same way. Everything below them keeps its first answer,
  // which is all a position near the bottom needs to be right about.
  if (refine) {
    const deep = { ...scenario, runs: refine };
    // …AND THE SECOND PASS IS SHARDED FOR THE SAME REASON, at a HIGHER run
    // count than the first: one lane there is the slowest call in the scan.
    const runDeep = async (override) => {
      const r = await simulateFleet(
        { ...buildPayload(), ...deep, ...override },
        () => { calcDirty(); },
        { background: true });
      if (!r || !r.ok) return null;
      return readGain(r, useKills);
    };
    gainScan.phase = tr("re-measuring the leaders");
    calcDirty();
    const deepBase = await runDeep({});
    if (!live()) return;
    gainScan.done++;
    calcDirty();
    if (deepBase?.v) {
      const top = cands
        .filter((c) => gainScan.by[c.id])
        .sort((a, b) => gainScan.by[b.id].pct - gainScan.by[a.id].pct)
        .slice(0, GAIN_REFINE_TOP);
      for (const c of top) {
        const v = await runDeep(c.payload);
        if (!live()) return;
        gainScan.done++;
        if (v) {
          gainScan.by[c.id] = { ...gainOver(v, deepBase), runs: refine };
        }
        calcDirty();
      }
    }
  }
  // WHAT THE LOST LANES NEVER ASKED, asked once more.
  //
  // A lane stands down when the pool refuses it twice, and every candidate it
  // had not reached went unmeasured — silently, because the counter only ever
  // counted answers. `laneAt` builds a fresh worker now, so the retry has
  // somewhere to run; ONCE, because a second refusal means the pool is not
  // coming back and a loop against that is a hang rather than a recovery.
  if (gainScan.lanesLost) {
    for (const c of cands.filter((x) => !gainScan.by[x.id] && !gainScan.refused[x.id])) {
      if (!live()) return;
      const r = await laneAsk(freeLane(), "/api/simulate",
        { ...buildPayload(), ...scenario, ...c.payload }, live);
      if (!live()) return;
      if (r === null) break;             // still gone: stop rather than spin
      const g = readGain(r, useKills);
      if (g) gainScan.by[c.id] = { ...gainOver(g, base), runs: scenario.runs };
      else if (r.error) gainScan.refused[c.id] = String(r.error);
      calcDirty();
    }
  }
  // A SCAN WITH HOLES IS NOT AN ANSWER, so the key stays null and the next
  // request re-asks. The reader is told how many are missing rather than shown
  // a ranking that is quietly short.
  // AN ANSWER OF "NO" IS STILL AN ANSWER, so a refused option is not a hole.
  // Only a candidate nobody ever heard back about is.
  const holes = cands.filter((c) => !gainScan.by[c.id] && !gainScan.refused[c.id]).length;
  if (holes) {
    // …AND THE REASON GIVEN IS THE REASON. Blaming a lost worker when none was
    // lost sends the reader to rebuild a pool that was never the problem.
    gainStop(gainScan.lanesLost
      ? tr("{n} of {total} could not be measured — the calculator lost a worker")
        .replace("{n}", holes).replace("{total}", cands.length)
      : tr("{n} of {total} could not be measured")
        .replace("{n}", holes).replace("{total}", cands.length));
    return;
  }
  // FINISHED, so the fight it answers is recorded — and only now.
  gainScan.key = gainScan.want;
  gainStop("");
}

/// The gain chip: what this option is worth, once scanned. Shared by all
/// three lists — a mod, an arcane and an evolution are the same question
/// asked of different axes, so they say it the same way.
/// HOW WIDE this gain's answer is, as a fraction — 0 when the comparison is
/// exact.
///
/// It is the comparison's OWN error and nothing else. `gainOver` pairs the two
/// builds run by run, so a candidate that scales this fight proportionally
/// leaves every paired difference at zero and lands here as a plain 0: exact,
/// derived, not asserted.
///
/// NOT a PROXY — "did the median run's proc count change?" — which is what two
/// independent summaries reduce you to. The proxy is wrong in the direction
/// that matters: it says "same fight" whenever the count happens to coincide,
/// and on the Kuva Nukor all seven progenitor elements report 6079
/// while their fights differ by up to 30%. Seven chips claimed an exactness
/// none of them had, and the ranking between two of them was a coin flip
/// printed as a fact.
const gainBand = (g) => g.se || 0;

/// The chip. A gain the scan cannot resolve STATES ITS WIDTH rather than
/// collapsing to "about nothing". "≈0%" was one string for
/// two different findings — a mod that does nothing, and a mod nobody measured
/// hard enough — and the difference between them is the only thing a reader
/// can act on: the first says pick something else, the second says raise the
/// runs. A band says which: `+0.1%` is
/// worthless, `≈+3.1% ±7.2%` is unmeasured.
const gainChip = (g, why, tied) => {
  // TIED WITH THE LEADER, said on the chip. The list always produces an order;
  // this is where it admits the order is not one. See `gainTied`.
  const tie = tied
    ? ` <span class="gtie" title="${escHtml(tr("this is as good as the top option — the gap between them is smaller than either answer's own width, so the order between them is the dice"))}">${tr("tied")}</span>`
    : "";
  const band = gainBand(g);
  if (!band) {
    // MEASURED AND MOVED NOTHING — which is a NUMBER, and the number is what
    // is printed. A worded verdict here would be about the reader's own fight
    // rather than a measurement of it: the same card is worth nothing against
    // one standing target and a great deal in a crowd, or the moment a build
    // finally kills something an on-kill trigger is waiting for. The app states
    // what it MEASURED and marks only what it does not MODEL.
    if (g.pct === 0) {
      return `<span class="gainchip flat" title="${escHtml(
        `${tr("this option did not re-roll the fight — run for run it scaled the same engagement, so this comparison is exact")} · ${why}`
      )}">${gainPct(g.pct)}</span>`;
    }
    // Paired exactly — the fight did not re-roll, so this is not an estimate.
    return `<span class="gainchip ${g.pct >= 0 ? "up" : "down"}" title="${escHtml(
      `${tr("this option did not re-roll the fight — run for run it scaled the same engagement, so this comparison is exact")} · ${why}`
    )}">${gainPct(g.pct)}${tie}</span>`;
  }
  const cls = Math.abs(g.pct) < band ? "flat" : (g.pct >= 0 ? "up" : "down");
  return `<span class="gainchip ${cls}" title="${escHtml(
    `${tr("this option re-rolls the fight, so its answer is only good to ±{x} — raise the run count to narrow it")
      .replace("{x}", sig2(band * 100) + "%")} · ${why}`
  )}">≈${gainPct(g.pct)} ±${sig2(band * 100)}%${tie}</span>`;
};

/// IS THIS OPTION TELLING THE READER APART FROM THE BEST ONE?
///
/// A chip answers "what is this worth against the build you have". The LIST
/// answers a second question nobody wrote down — which of these to pick — and
/// it answers it by sorting, which always produces an order even when there is
/// none. Two options whose gains differ by less than the two bands together are
/// the same answer; printing one above the other says otherwise, and the reader
/// acts on the order, so the top one can measure worse than the one under
/// it.
///
/// So an option that is not SEPARATED from the leader is marked as tied with
/// it. Not the leader's neighbour — the leader, because "which do I pick" is
/// asked of the top of the list and every option tied with it is an equally
/// good answer.
const gainTied = (g) => {
  const all = Object.values(gainScan.by || {});
  if (all.length < 2) return false;
  const best = all.reduce((a, b) => (b.pct > a.pct ? b : a));
  const near = (x, y) => x.pct - y.pct < Math.hypot(x.se || 0, y.se || 0);
  // THE LEADER IS ONLY TIED IF SOMETHING ELSE IS TIED WITH IT. Marking it
  // unconditionally put "tied" on the first row of every ranking, including the
  // ones with a clear winner — a caveat printed where there is nothing to
  // caveat, which is noise on every list.
  if (best === g) return all.some((x) => x !== g && near(g, x));
  return near(best, g);
};

// PROGRESS BELONGS WHERE THE WORK IS BEING READ, not only where it was started.
//
// The quick calc counted itself in one place — the panel at the top of the page
// — and the LIST it was ranking said nothing. A pool of 90 mods at 200 runs is
// tens of seconds of a list that does not move, and a list that does not move is
// read as broken. The per-row "…" chip was already there and
// was not enough: it says THIS row has no answer yet, and says nothing about
// whether anything is still happening.
//
// So the strip goes at the top of every list a scan ranks — the mod picker, the
// arcane picker, the evolution tiers, the valence row, and the optimizer's own
// list — and it is the SAME component fed from whichever scan state that list
// reads, because "how far along is it" is one question with one answer.
//
// It draws NOTHING when nothing is running, so a finished list is a finished
// list: a bar sitting at 100% would be one more thing to read and dismiss.
function scanStrip(st, axis, rows) {
  if (!st) return "";
  // AN AXIS ONLY SHOWS ITS OWN. Two lists can be open at once (a picker over
  // the evolution rows), and the scan belongs to exactly one of them — showing
  // it on both would say the other one is being measured when it is waiting.
  if (axis && JSON.stringify(st.axis) !== JSON.stringify(axis)) return "";
  if (!st.running) {
    // …EXCEPT WHEN EVERY ANSWER WAS ZERO, which is a FINDING and not a bar at
    // 100%. A list of "+0.00%" and a list nobody measured are the same picture,
    // and the reader who opens the exilus slot under a single-target ruler gets
    // the first and reads the second — reported as the calculator being stuck
    // when it had measured all eleven correctly. Said only on a COMPLETE scan,
    // so a run with holes in it cannot claim nothing mattered.
    const own = rows && st.ids ? rows.filter((k) => k && st.ids.has(k)) : null;
    const got = (own || Object.keys(st.by || {}))
      .map((k) => (st.by || {})[k]).filter(Boolean);
    if (!st.key || !got.length || got.some((g) => g.pct !== 0)) return "";
    return `<div class="scan-strip flat" role="status">` +
      `<span class="scan-txt">${escHtml(
        tr("all {n} measured — none of them changes this fight")
          .replace("{n}", got.length))}</span></div>`;
  }
  // THE COUNT IS THE LIST'S OWN, and it is DERIVED FROM THE ROWS ON SCREEN. `st.total` is how many SIMULATIONS this scan will run,
  // which is a different number from how many options the reader is looking at
  // — it carries the refine pass, and on an axis whose candidates are split
  // across several lists it carries the other lists too. Both cases read as a
  // denominator that does not match what is in front of you: a Kitgun's grip
  // list is five rows and said `12/23`, because one scan covers the grip and
  // the loader and only one of the two is ever open.
  //
  // So a caller that has rows hands them over and the strip counts those: how
  // many of THESE options have an answer, over how many are coming. It is
  // derived from the list rather than from the axis, so a list added tomorrow
  // — or one an axis splits into three — is right without being taught.
  const mine = rows && st.ids
    ? rows.filter((k) => k && st.ids.has(k)) : null;
  const total = mine ? Math.max(1, mine.length)
    : Math.max(1, st.total || 1);
  const done = mine ? mine.filter((k) => gainOf(k)).length
    : Math.min(st.done || 0, total);
  const pct = Math.round((done / total) * 100);
  return `<div class="scan-strip" role="status" aria-live="polite" title="${escHtml(
    tr("the ranking is still being measured — the order moves until it finishes"))}">` +
    `<span class="scan-bar"><i style="width:${pct}%"></i></span>` +
    `<span class="scan-txt">⚡ ${escHtml(tr("ranking"))} ${done}/${total}</span></div>`;
}

const gainChipFor = (id, where) => {
  const g = gainOf(id);
  // A HALF-FILLED RANKING HAS TO LOOK LIKE ONE.
  //
  // An option with no answer YET rendered exactly like one that finished with
  // nothing to say — no chip at all — while the list re-sorts on every result
  // that lands (an unranked option sorts last, so whatever arrives first sits
  // at the top). Read mid-scan that is a ranking which keeps changing its mind,
  // and the only way to find out it was not final was to click away and back
  // (report, 2026-08-13).
  //
  // The scan already counts itself; it just said so nowhere near the list being
  // read. `gainOf` has checked the key, so this only marks rows on the axis
  // actually being measured.
  // REFUSED, AND THE ROW SAYS WHY. A row with no chip reads as one nobody got
  // to; this one was measured and cannot be equipped, which is a different
  // thing and the only one the reader can act on.
  const no = gainAbout() === gainKey() ? (gainScan.refused || {})[id] : null;
  if (no) {
    return `<span class="gainchip no" title="${escHtml(no)}">${
      escHtml(tr("cannot equip"))}</span>`;
  }
  if (!g) {
    return gainScan.running && gainScan.want === gainKey()
      ? `<span class="gainchip pend" title="${escHtml(
          tr("still measuring — {a} of {b} options ranked so far, and the order moves until it finishes")
            .replace("{a}", gainScan.done).replace("{b}", gainScan.total))}">…</span>`
      : "";
  }
  // A ONE-RUN number is a SCREEN, not a measurement, and it has to read like
  // one. Measured across three seeds on a status mod, a single run lands
  // anywhere in a ±39-point band — wide enough to print a minus sign in front
  // of a mod worth +40%, which reads as adding status chance LOWERING the
  // damage. A damage mod barely moves, because paired seeds
  // cancel almost everything about it; a status mod's payoff is decided by
  // which procs land, which is the one thing the dice still choose.
  //
  // So the screen says "about", and only the second pass — the leaders, run
  // again with a tenth of the scenario's count — prints a bare number.
  // EVERY number here is an average of a few runs, so every number reads the
  // same way: approximate, and said to be. A status mod's payoff is decided by
  // which procs land, and ten runs narrows that without settling it.
  const why = tr("averaged over {n} runs — this number moves between scans, most of all for status mods")
    .replace("{n}", g.runs);
  return gainChip(g, `${where} · ${gainScan.metric} · ${gainScan.note} · ${why}`,
    gainTied(g));
};

