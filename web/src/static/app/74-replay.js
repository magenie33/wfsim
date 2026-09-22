/// WHERE A COUNTER WENT UP, as percentages along the rail.
///
/// A cumulative series is what the frames carry, so an EVENT is a frame whose
/// count exceeds the one before it. Marks closer together than `MARK_MIN_GAP`
/// are dropped: at a tick apiece they would not be told apart anyway, and a
/// dense reloader would otherwise put six hundred elements in the DOM. What
/// survives merges into a band, which is what that fight looks like.
const MARK_MIN_GAP = 0.3;
function risesIn(series, last) {
  const at = [];
  let prevPct = -MARK_MIN_GAP;
  for (let i = 1; i <= last; i++) {
    if (!((series[i] || 0) > (series[i - 1] || 0))) continue;
    const pct = (i / last) * 100;
    if (pct - prevPct < MARK_MIN_GAP) continue;
    at.push(pct);
    prevPct = pct;
  }
  return at;
}

/// The marks themselves: kills tall and red, reloads short and recessive.
function scrubMarks(rp) {
  const last = rp.t.length - 1;
  if (last < 1) return "";
  const kills = risesIn(rp.kills || [], last);
  const reloads = risesIn((rp.kpi || {}).reloads || [], last);
  return kills.map((p) => `<i class="scrub-kill" style="left:${p.toFixed(2)}%"></i>`).join("")
    + reloads.map((p) => `<i class="scrub-reload" style="left:${p.toFixed(2)}%"></i>`).join("");
}

/// …and what they mean, drawn only for the marks this fight actually has. A
/// legend entry for a mark that is not on the rail is a key to nothing.
function scrubLegend(rp) {
  const last = rp.t.length - 1;
  const has = (s) => (s || []).some((v, i) => i > 0 && v > s[i - 1]);
  const items = [];
  if (last >= 1 && has(rp.kills)) {
    items.push(`<span class="sl"><i class="sl-kill"></i>${escHtml(tr("a kill"))}</span>`);
  }
  if (last >= 1 && has((rp.kpi || {}).reloads)) {
    items.push(`<span class="sl"><i class="sl-reload"></i>${escHtml(tr("a reload"))}</span>`);
  }
  if (!items.length) return "";
  items.push(`<span class="sl-note">${escHtml(tr("drag onto a mark to land on that instant"))}</span>`);
  return `<div class="scrub-legend">${items.join("")}</div>`;
}

function replayMarkup(r) {
  const rp = r && r.replay;
  if (!rp || !rp.t || rp.t.length < 2) return { bar: "", where: "", curves: "" };
  const named = buffRosterName;
  const dbName = debuffRosterName;
  // WHOSE DEBUFFS. `rp.tracked` names the bodies the replay followed — the
  // aimed one first — and this is the index into it. The selection lives
  // OUTSIDE the render (`replayFoe`) so picking an enemy survives a scrub and
  // a re-run, which is the same rule every fold on this panel follows.
  const dBody = replayFoeIdx(rp);
  // WHICH ENEMY, as chips. Drawn only when there is a choice — one body is the
  // fight this app has always run and needs no control.
  const foeChips = (rp, sel) => {
    const ids = rp.tracked || [];
    if (ids.length < 2) return "";
    const hit = (r.bodies || []).length;
    const more = Math.max(0, hit - ids.length);
    // BY DAMAGE, HARDEST HIT FIRST. The engine follows the
    // aimed body plus the hardest-hit few, and `tracked` came back in ITS
    // order, which is an implementation detail of who got a slot rather than an
    // answer to the question a reader is asking — "who took the most" is the
    // reason to open this table at all.
    //
    // THE ORDER IS THE DISPLAY'S, NOT THE DATA'S. `data-rpfoe` stays the index
    // into `tracked`, because `dstacks[k]` is that body's series and reordering
    // the list itself would draw one body's chip over another's debuffs.
    const dmgOf = Object.fromEntries((r.bodies || []).map((b) => [b.id, b.damage || 0]));
    const order = ids.map((id, k) => k)
      .sort((a, b) => (dmgOf[ids[b]] || 0) - (dmgOf[ids[a]] || 0));
    return `<div class="rp-foes">${
      order.map((k) => `<button type="button" class="rp-foe${k === sel ? " sel" : ""}" data-rpfoe="${
        k}" title="${escHtml(k === 0
          ? tr("the body the weapon was on")
          : tr("an enemy the shot reached"))}">${escHtml(ids[k])} <span class="sm">${
        // THE NUMBER IT IS SORTED BY, on the chip. An order nobody can check is
        // an order nobody trusts, and this is the whole reason the chips exist.
        escHtml(sig2(dmgOf[ids[k]] || 0))}${
        k === 0 ? ` · ${escHtml(tr("aimed"))}` : ""}</span></button>`).join("")
    }${more > 0
      ? `<span class="rp-foe-more">${escHtml(
          tr("+{n} more took damage and are not followed").replace("{n}", more))}</span>`
      : ""}</div>`;
  };
  const dRoster = rp.debuffs || [];
  const dSeries = (rp.dstacks || [])[dBody] || [];
  // THE CHART IS 120 UNITS TALL, not 28.
  //
  // At 28 px a stack count spends its whole life inside two pixels: the ramp,
  // the dip when a body dies, the plateau against the ceiling and the dead
  // bands all collapse into one wobbling line, and the row is decoration
  // beside its own header. These curves are the one thing on this panel no
  // other calculator has — a state this fight DERIVED rather than one the
  // reader typed — so they are drawn at a size where that is legible.
  //
  // The coordinate space is the chart's, and `preserveAspectRatio="none"`
  // stretches it to whatever width the column is, so nothing here depends on
  // the pixel height the CSS happens to give it.
  const W = 600, H = 120;
  const curveRows = (roster, series, name, kind) => roster.map((b, i) => {
    // WHAT IS DRAWN, which is the stack count on an ordinary row and the number
    // the stacks are worth on a value-capped one. Everything below — the area,
    // the average, the dead bands, the ramp — is computed on this, so a clamped
    // pile draws the flat plateau it actually is instead of a line climbing
    // past its own ceiling.
    const raw = series[i] || [];
    const s = b && b.value ? raw.map((n) => rpValueOf(b, n)) : raw;
    const max = b && b.value ? b.value.max
      : Math.max(1, rpUncapped(b) ? Math.max(...s) : b.max);
    const px = (j) => (j / (s.length - 1)) * W;
    const py = (v) => H - 1 - (v / max) * (H - 2);
    const pts = s.map((v, j) => `${px(j).toFixed(1)},${py(v).toFixed(1)}`).join(" ");
    const mean = s.reduce((a, v) => a + v, 0) / (s.length || 1);
    const up = s.filter((v) => v > 0).length / (s.length || 1);
    // TWO DECIMALS, which is what made the "impossible 100%" go away for
    // real: 99.83% is the truth, and rounding it to a whole number was the
    // only thing that ever made it look like a perfect run. `100.00%` now
    // means every single frame was up, and nothing else prints it.
    const upPct = (up * 100).toFixed(2);
    const offPct = (100 - up * 100).toFixed(2);
    // ...and the thing the 100% was hiding: how long the ramp took. This is
    // the answer to "初始肯定要花时间" as a number rather than a rounding.
    const iFull = rpUncapped(b) && !(b && b.value) ? -1 : s.findIndex((v) => v >= rpCeil(b));
    const ramp = iFull < 0
      ? tr("never full")
      : `${tr("full at")} ${(iFull * rp.frame_seconds).toFixed(2)}s`;
    // WHERE IT WAS OFF. A buff at zero is not a low buff, it is an absent one,
    // and a flat line along the axis says that far too quietly — the run that
    // never earned a stack and the run that lost them all draw the same
    // picture. Every zero stretch gets a band.
    const dead = [];
    for (let j = 0; j < s.length; j++) {
      if (s[j] > 0) continue;
      const from = j;
      while (j + 1 < s.length && s[j + 1] === 0) j++;
      dead.push(`<rect class="rp-dead" x="${px(from).toFixed(1)}" y="0" width="${Math.max(1, px(j) - px(from)).toFixed(1)}" height="${H}"/>`);
    }
    return `<div class="rp-row" data-${kind}="${i}">
      <div class="rp-head">
        <span class="rp-caret">▾</span>
        <span class="rp-name">${escHtml(name(b.id))}</span>
        <span class="rp-stat">${escHtml(tr("avg"))} ${b && b.value ? rpFmtVal(b, mean) : mean.toFixed(2)}/${rpCap(b)} · ${escHtml(tr("uptime"))} ${upPct}%${up < 1 ? ` · <span class="rp-off">${escHtml(tr("inactive"))} ${offPct}%</span>` : ""} · ${escHtml(ramp)}</span>
        <span class="rp-now" data-now="${i}" data-series="${kind}">${b && b.value ? rpFmtVal(b, s[s.length - 1]) : s[s.length - 1]}/${rpCap(b)}</span>
      </div>
      <div class="rp-chart">
        <svg viewBox="0 0 ${W} ${H}" preserveAspectRatio="none">
          ${[0.25, 0.5, 0.75].map((f) =>
            `<line class="tl-grid" x1="0" x2="${W}" y1="${py(max * f).toFixed(1)}" y2="${py(max * f).toFixed(1)}"/>`).join("")}
          ${dead.join("")}
          <polygon class="rp-area" points="0,${H} ${pts} ${W},${H}"/>
          <line class="rp-mean" x1="0" x2="${W}" y1="${py(mean).toFixed(1)}" y2="${py(mean).toFixed(1)}"><title>${escHtml(tr("avg"))} ${mean.toFixed(2)}</title></line>
          <polyline class="rp-line" points="${pts}"/>
          <rect class="rp-ahead" data-ahead="${i}" data-series="${kind}" x="${W}" y="0" width="0" height="${H}"/>
          <line class="rp-cur" data-cur="${i}" data-series="${kind}" y1="0" y2="${H}" x1="${W}" x2="${W}"/>
        </svg>
      </div>
      <div class="rp-x"><span>0s</span><span>${(rp.t[rp.t.length - 1] || 0).toFixed(0)}s</span></div>
    </div>`;
  }).join("");
  const rows = curveRows(rp.buffs, rp.stacks, named, "buff");
  // THE TARGET'S SIDE OF THE SAME FIGHT. Symmetric with the buff table on
  // purpose — same rows, same uptime, same dead bands. A
  // DEATH IS NOT A NEW SERIES: the arena replaces the body it kills and every
  // stack goes with it, so a respawn reads as the curve dropping to zero and
  // climbing again, and the gap counts
  // against uptime. That is what makes the table worth reading — the ramp you
  // pay for on every body is the thing a single averaged number hides.
  //
  // Rows the run never touched are dropped rather than drawn flat: the roster
  // is every status the engine models, and thirteen empty charts would bury the
  // three that moved. A buff row is kept even at zero because the BUILD claimed
  // it; nothing claims a debuff except the fight.
  const dRows = curveRows(
    // A STORED RESULT PREDATES THIS TABLE and carries no debuff series at all —
    // `lastResult` is saved in the scenario preset and replayed on boot, so a
    // payload written by yesterday's build is the FIRST thing this code sees on
    // a returning visitor's machine. It cost the whole app: an unguarded
    // `.filter` on `undefined` threw inside `restoreState`, which is upstream
    // of everything, so the page did not fail to draw a table — it failed to
    // start (reported 2026-08-11).
    dRoster.filter((_, i) => (dSeries[i] || []).some((v) => v > 0)),
    dSeries.filter((s) => (s || []).some((v) => v > 0)),
    dbName, "debuff");
  // …AND HOW MANY NEVER MOVED, said out loud.
  //
  // A row the run never touched is dropped, because seventeen flat lines bury
  // the three that moved. That is a real filter and it looks exactly like a
  // CAP from the reader's side — which is how it was reported: DoTs that were
  // on the enemy and not on the chart. They were missing for a different
  // reason (two whole families had no row at all — `DEBUFF_ROSTER`), but the
  // silence was the same either way.
  //
  // So the omission is counted rather than hidden, which is the rule the body
  // roll call already follows one panel up.
  const dQuiet = dRoster.length - dRoster
    .filter((_, i) => (dSeries[i] || []).some((v) => v > 0)).length;
  // TWO pieces, deliberately far apart. The transport
  // belongs at the top, next to the numbers it drives; the CURVES are charts
  // and belong with the other chart, under the DPS curve. Moving both up put
  // a wall of graphs above the result they explain.
  const bar = `
      <h3>${escHtml(tr("Replay"))}</h3>
      <div class="rp-bar">
        <button id="rp-play" class="ghost-btn small rp-play">▶ ${escHtml(tr("play"))}</button>
        <!-- ■ IS A STOP, AND A STOP RETURNS THE PANEL TO ITS RESTING STATE —
             which here is the FINISHED fight, the numbers the result reports.
             Pause leaves the playhead where it stands; this is the one click
             back to the answer, and dragging the scrubber to its far end was
             the only way there. -->
        <button id="rp-stop" class="ghost-btn small rp-stop" title="${escHtml(tr("back to the finished fight"))}">■ ${escHtml(tr("stop"))}</button>
        ${ddButton("rp-speed", {
          value: 5,
          items: REPLAY_SPEEDS.map((sp) => ({ value: sp, label: `${sp}x` })),
        })}
        <!-- THE SCRUBBER CARRIES THE FIGHT'S OWN EVENTS. A rail that knows only
             "0 to 180 seconds" makes a reader hunt for the instant something
             died; with the marks on it, that instant is a place to drag TO.
             Both series are already on the wire — no engine asked. -->
        <div class="scrub">
          <div class="scrub-rail"><i id="rp-done" class="scrub-done"></i></div>
          ${scrubMarks(rp)}
          <input id="rp-scrub" class="rp-scrub" type="range" min="0" max="${rp.t.length - 1}" value="${rp.t.length - 1}">
        </div>
        <span id="rp-clock" class="rp-clock">${rp.t[rp.t.length - 1].toFixed(0)}s / ${rp.t[rp.t.length - 1].toFixed(0)}s</span>
      </div>
      ${scrubLegend(rp)}
      <!-- THE FIGHT'S OWN NUMBERS, and only those. Damage and kills are the
           whole engagement's however many bodies are in it; a POOL belongs to
           one body, and a body's number sitting here reads as the crowd's.
           The pools are down in "Where the damage went", beside the bodies. -->
      <div class="rp-pools" id="rp-pools"></div>`;
  // ---- WHERE THE DAMAGE WENT ------------------------------------------
  //
  // A SECOND COPY OF THE SCENE, and that is the point:
  // setting up a fight and reading one are two things. The scenario's canvas is
  // where a body is PLACED — draggable, with its distance shortcuts and its
  // +1/+8. This one is read-only, coloured by what each body TOOK, and clicking
  // a body picks it. Neither is the other's control and neither can be mistaken
  // for it.
  //
  // Only when there is a crowd: one body is the fight this app has always run
  // and its picture is already on the scenario.
  // THE ROLL CALL IS THE RESULT's, not the replay's: `bodies` is a mean over
  // every run, while a replay is ONE engagement. Two different questions and
  // two different objects, joined here by the id the page gave each body.
  // HARDEST HIT FIRST, like the chips above it — the two are the same list
  // read two ways, so they are ordered the same way. A copy
  // rather than a sort in place: `r.bodies` is the stored result and a render
  // does not get to reorder it.
  const bodyRows = [...(r.bodies || [])].sort((a, b) => (b.damage || 0) - (a.damage || 0));
  // ONE TEMPLATE, WHATEVER THE FIGHT IS. This block was
  // gated on `bodyRows.length > 1` and called `crowd`, so a single-target
  // simulation drew no scene, no roll call — and no DAMAGE POPS, since the
  // layer they float in is appended to the scene. The one output that is a
  // discrete thing that happened at a place at a time was invisible in the
  // commonest fight this app runs.
  //
  // THE SPECIAL CASE WAS THE MISTAKE, not the missing feature. A fight is a
  // scene with N bodies and N=1 is just N=1: it has a shooter, a target, a
  // distance and an aim point, which is exactly what the SCENARIO's own canvas
  // draws for it. A result that changes shape with the body count is a second
  // template nobody asked for, and the reader has to learn both.
  // WHOSE POOLS THE REPLAY CARRIES. The engine traces ONE body's overguard,
  // shield and health — the one the weapon is on (`fight::run`) — so the row
  // names it instead of letting a crowd read three numbers as everyone's.
  // Following a second body's pools is a second series, not a second label.
  const aimedId = (bodyRows.find((b) => b.aimed) || {}).id || (rp.tracked || [])[0] || "";
  const crowd = bodyRows.length
    ? `<div class="rp-pools-head">${escHtml(tr("the body the weapon was on"))}${
        aimedId ? ` · ${escHtml(aimedId)}` : ""}</div>`
      + `<div class="rp-pools" id="rp-foe-pools"></div>`
      + `<div class="rp-scene" id="rp-scene"></div>`
      + `<table class="rp-roll"><tbody>${bodyRows.map((b) => {
          const top = Math.max(...bodyRows.map((x) => x.damage)) || 1;
          const share = b.damage / top;
          const on = (rp.tracked || []).indexOf(b.id);
          // A ROW THE REPLAY DID NOT FOLLOW IS STILL A QUESTION, and it has an
          // answer: `data-rpask` re-runs the SAME fight following that body.
          // `off` stays as the styling hook for "no series in hand yet".
          return `<tr class="rp-rollrow${on === dBody ? " sel" : ""}${
            on < 0 ? " off" : ""}"${
            on >= 0 ? ` data-rpfoe="${on}"` : ` data-rpask="${escHtml(b.id)}"`}>`
            + `<td class="nm">${escHtml(b.id)}${
              b.aimed ? ` <span class="sm">${escHtml(tr("aimed"))}</span>` : ""}</td>`
            + `<td class="bar"><span style="width:${(share * 100).toFixed(1)}%"></span></td>`
            + `<td class="num">${Math.round(b.damage).toLocaleString()}</td></tr>`;
        }).join("")}</tbody></table>`
      // WHAT IS NOT IN HAND, and what a click there costs. The replay follows
      // a few bodies because a series is 18 KB and a 19x19 ruler would be 6.5
      // MB of them; the rest are one engagement away, re-run from the state
      // this fight started at, so what comes back is THIS fight's series and
      // not a second fight's.
      + ((rp.tracked || []).length < bodyRows.length
        ? `<div class="rp-foe-more">${escHtml(
            tr("+{n} more took damage — click one to follow it too")
              .replace("{n}", bodyRows.length - (rp.tracked || []).length))}</div>`
        : "")
    : "";

  // WHERE IT WENT is a CUT OF THE TOTAL — the same damage sorted by body — so
  // it belongs with the other cuts, in the composition zone. The coverage
  // CURVES are evidence of a state this fight derived, which is a different
  // question, so they are returned separately and land in the evidence zone.
  const where = crowd
    ? foldBlock("where", tr("Where the damage went"),
        tr("every body that took something, and how much — click one to read its debuffs"),
        crowd)
    : "";
  const curves =
    foldBlock("buffs", tr("Buff coverage"), tr("live stacks through the engagement"), rows)
    + (dRows
      ? foldBlock("debuffs", tr("Debuff coverage"),
          tr("what was on the target — a respawn is the same target, so its stacks drop to zero and climb again"),
          // WHOSE, and it is a control rather than a caption. One body is one
          // chip; the fight this app ran until 2026-08-17 has exactly one and
          // draws no chips at all, so a single-target replay looks the way it
          // always did.
          //
          // THE CAP IS SAID OUT LOUD. A replay follows the aimed body plus the
          // hardest-hit few (`REPLAY_TRACKED`) because a series is 18 KB and a
          // 19x19 ruler would be 6.5 MB — so when more bodies took damage than
          // were followed, the line says how many. An absence would read as
          // "that is everyone".
          foeChips(rp, dBody) + dRows + (dQuiet > 0
            ? `<div class="rp-foe-more">${escHtml(
                tr("{n} more statuses this engine models never landed and are not drawn")
                  .replace("{n}", dQuiet))}</div>`
            : ""))
      : "");
  return { bar, where, curves };
}

// Re-read the WHOLE result panel at frame `i` — KPIs, the damage meter, the
// DPS curve, the buff curves, the target's pools.
//
// This is what makes it a replay rather than a cursor. The
// panel is rendered ONCE at its final state and then re-read in place:
// rebuilding the markup sixty times a second would drop every open sub-row,
// every scroll position and the caret you just clicked.
//
// It starts at the LAST frame, which is the finished fight — the same numbers
// the panel would show with no replay at all. Playing rewinds to 0 and walks
// forward; stopping anywhere leaves the panel reading that instant.
function replayApply(rp, i) {
  const n = (x) => Math.round(x || 0).toLocaleString();
  const pc = (x) => `${((x || 0) * 100).toFixed(1)}%`;
  const last = rp.t.length - 1;
  const frac = last > 0 ? i / last : 1;

  $("rp-clock").textContent = `${rp.t[i].toFixed(1)}s / ${rp.t[last].toFixed(0)}s`;
  $("rp-scrub").value = i;
  // THE RAIL FILLS BEHIND THE THUMB. A range input paints one track, so the
  // "played" half is an element of its own under it — which is also what lets
  // the marks sit between the rail and the thumb.
  const done = $("rp-done");
  if (done) done.style.width = `${(frac * 100).toFixed(2)}%`;
  // A FIXED GRID, not a flowing row. Every value here
  // changes on every frame, and a flex row re-measures itself each time — the
  // labels slide left and right for the whole playback, which reads as the
  // page shaking. Fixed columns and tabular figures hold still, and the grid
  // is what lets a second and third enemy join without a re-layout.
  const cell = (label, v) =>
    `<span class="rp-cell"><i>${escHtml(label)}</i><b>${v}</b></span>`;
  // THE TOP ROW IS THE FIGHT: what it dealt and what it killed, true of a
  // crowd and of one body alike.
  $("rp-pools").innerHTML =
    cell(tr("Damage"), n(rp.dmg[i])) +
    cell(tr("Kills"), rp.kills[i]);
  // …AND THE POOLS WHERE THE BODIES ARE, under the heading that names whose.
  const foePools = $("rp-foe-pools");
  if (foePools) {
    foePools.innerHTML =
      cell(tr("Overguard"), n(rp.og[i])) +
      cell(tr("Shield"), n(rp.sh[i])) +
      cell(tr("Health"), n(rp.hp[i]));
  }

  // The headline. KPM is `kill_progress / minutes`, and `kill_progress` is
  // kills plus the fraction of the CURRENT target's pool already gone — which
  // the frames carry directly, so it is derived here rather than shipped as a
  // fourth series that could disagree with them.
  const hero = document.querySelector("[data-hero]");
  if (hero) {
    const pool0 = (rp.og[0] || 0) + (rp.hp[0] || 0) + (rp.sh[0] || 0);
    const left = (rp.og[i] || 0) + (rp.hp[i] || 0) + (rp.sh[i] || 0);
    const progress = (rp.kills[i] || 0) + (pool0 > 0 ? 1 - left / pool0 : 0);
    const mins = rp.t[i] / 60;
    // THE SAME METRIC THE HEADLINE WAS DRAWN IN, read back by id rather than
    // asked "is it dps": the ticker has to agree with the number it replaces.
    // The replay's own series is what it reads, so the field is looked up on it.
    const hm = metricOf(hero.dataset.hero);
    const v = hm.per_minute
      ? fmtScore(mins > 0 ? progress / mins : 0)
      : fmtScore(rp.kpi && rp.kpi[hm.field] ? rp.kpi[hm.field][i] : 0);
    const unit = hero.querySelector(".hero-unit");
    hero.textContent = v;
    if (unit) hero.appendChild(unit);
  }

  // KPIs. Rates are fractions, counters are counts, DPS is a number — the
  // key says which, so a new KPI needs no new branch here.
  const k = rp.kpi || {};
  document.querySelectorAll("[data-kpi]").forEach((el) => {
    const key = el.dataset.kpi, s = k[key];
    if (!s) return;
    el.textContent = key === "crit_tier" ? (s[i] || 0).toFixed(2)
      : /_rate$/.test(key) ? pc(s[i])
      : n(s[i]);
  });

  // The damage meter, on ONE FIXED SCALE — every bar is a length against the
  // biggest source AT THE END, so a bar only ever grows and the picture fills
  // in as the fight runs. Rescaling each frame against that frame's own
  // leader pinned the top bar full from the first shot, which is a chart that
  // never moves: the only thing a reader could see was the text. The SHARES
  // stay live, because a share is about the instant.
  //
  // One scale for sub-rows too — the same one the panel is drawn with
  // (`srcMax` in `76-damage-pops.js`), so the last frame is the finished
  // panel and not a second chart that happens to carry the same numbers.
  const byKey = {};
  (rp.sources || []).forEach((s) => {
    byKey[s.source] = s.dmg;
    (s.by_type || []).forEach((ty) => { byKey[`${s.source}::${ty.type}`] = ty.dmg; });
  });
  let total = 0, scale = 0;
  (rp.sources || []).forEach((s) => { total += s.dmg[i]; scale = Math.max(scale, s.dmg[last]); });
  scale = scale || 1;
  document.querySelectorAll("#sim-results [data-mk]").forEach((el) => {
    const s = byKey[el.dataset.mk];
    if (!s) return;
    const v = s[i];
    const bar = el.querySelector(".mbar i");
    if (bar) bar.style.width = `${Math.max(0, (v / scale) * 100).toFixed(1)}%`;
    const val = el.querySelector(".mval");
    if (val) val.textContent = `${n(v)} · ${total > 0 ? ((v / total) * 100).toFixed(1) : "0.0"}%`;
  });

  // …AND THE TYPE BAR WITH IT. It is the same damage the meter just drew,
  // counted a second way, so a bar frozen on the finished fight beside a meter
  // following the playhead is two answers to one question. A stacked
  // composition fills its width by construction, so what moves here is the
  // SHARES — the segments the ramping elements own widen as they ramp.
  //
  // THE ORDER IS THE ONE IT WAS DRAWN IN, which is why the page is re-read by
  // type rather than rebuilt: re-sorting each frame would have segments
  // overtaking each other sixty times a second. A type with nothing yet is
  // hidden rather than left at its 3px minimum, so an empty bar reads empty.
  const typeNow = {};
  (rp.sources || []).forEach((s) => {
    const parts = s.by_type && s.by_type.length ? s.by_type : [{ type: s.source, dmg: s.dmg }];
    parts.forEach((p) => {
      const k = dtKey(p.type);
      if (k) typeNow[k] = (typeNow[k] || 0) + (p.dmg[i] || 0);
    });
  });
  const typeSum = Object.values(typeNow).reduce((a, v) => a + v, 0);
  document.querySelectorAll("#sim-results [data-dk]").forEach((el) => {
    const share = typeSum > 0 ? (typeNow[el.dataset.dk] || 0) / typeSum : 0;
    if (el.classList.contains("dmg-seg")) {
      el.hidden = share <= 0;
      el.style.flex = share.toFixed(5);
      el.title = `${DT(el.dataset.dk)} ${pct2(share)}`;
      return;
    }
    const lv = el.querySelector(".lv");
    if (lv) lv.textContent = pct2(share);
  });

  // The DPS curve: everything past `t` is greyed rather than removed, so the
  // shape you are walking through stays legible.
  const svg = $("tl-svg");
  if (svg) {
    const w = Number(svg.viewBox.baseVal.width) || 600;
    const now = $("tl-now"), ahead = $("tl-ahead");
    if (now) { now.hidden = false; now.setAttribute("x1", w * frac); now.setAttribute("x2", w * frac); }
    if (ahead) {
      ahead.hidden = false;
      ahead.setAttribute("x", w * frac);
      ahead.setAttribute("width", w * (1 - frac));
    }
  }

  // The buff curves, same treatment, plus the live count in each header.
  document.querySelectorAll("[data-cur]").forEach((el) => {
    el.setAttribute("x1", 600 * frac); el.setAttribute("x2", 600 * frac);
  });
  document.querySelectorAll("[data-ahead]").forEach((el) => {
    el.setAttribute("x", 600 * frac); el.setAttribute("width", 600 * (1 - frac));
  });
  // THE LIVE COUNT in each header, from whichever side of the fight the row
  // belongs to. The DEBUFF rows index a FILTERED roster — the statuses this run
  // never applied are not drawn — so the row rebuilds the same filter rather
  // than indexing the full one and reading somebody else's series.
  // WHOSE, again — this re-reads the panel at a frame and has to pick the same
  // body the table was DRAWN for, or the live count in each header would belong
  // to somebody else.
  const aBody = replayFoeIdx(rp);
  const dLive = (rp.debuffs || [])
    .map((b, k) => [b, ((rp.dstacks || [])[aBody] || [])[k] || []])
    .filter(([, s]) => s.some((v) => v > 0));
  document.querySelectorAll("[data-now]").forEach((el) => {
    const j = Number(el.dataset.now);
    if (el.dataset.series === "debuff") {
      const [b, s] = dLive[j] || [];
      if (b) el.textContent = `${rpFmt(b, s[i])}/${rpCap(b)}`;
      return;
    }
    // THE SAME QUANTITY THE ROW WAS DRAWN IN — a header that scrubs from a
    // percentage to a stack count would be two charts wearing one label.
    el.textContent = `${rpFmt(rp.buffs[j], rp.stacks[j][i])}/${rpCap(rp.buffs[j])}`;
  });
}

function wireReplay(r) {
  const rp = r && r.replay;
  if (replayState && replayState.raf) cancelAnimationFrame(replayState.raf);
  replayState = null;
  if (!rp || !$("rp-scrub")) return;
  // `pos` is a FLOAT cursor in frames — playback advances it by fractions of
  // a frame per animation tick. Every array read goes through the rounded
  // index: `rp.t[3.7]` is `undefined`, which threw inside the animation
  // callback and killed the loop with no console entry to show for it.
  const st = { data: rp, pos: rp.t.length - 1, i: rp.t.length - 1, playing: false, speed: 5, raf: 0, last: 0 };
  replayState = st;

  const draw = () => {
    const was = st.i;
    st.i = Math.max(0, Math.min(rp.t.length - 1, Math.round(st.pos)));
    replayApply(rp, st.i);
    // THE NUMBERS, and only when the frame actually changed. `tick` runs at the
    // browser's refresh rate and the clock at 1x advances a frame every 300 ms,
    // so spawning per paint would draw the same twelve numbers twenty times.
    if (st.playing) {
      if (st.i !== was) popsDraw(rp, st.i, true);
    } else {
      popsDraw(rp, st.i, false);
    }
  };
  const stop = () => {
    st.playing = false; st.last = 0;
    $("rp-play").textContent = `▶ ${tr("play")}`;
  };
  // THE NUMBERS ARE THE RECORD, so reaching for the replay is what asks for
  // it — and it asks for the WINDOW it is looking at. It is deliberately NOT
  // fetched with the result: a dense fight's stream is megabytes and most runs
  // are never replayed at all, which is the same reason `/api/log` is a query
  // rather than a field on `/api/simulate`.
  //
  // A RECORD IS A WINDOW, AND THE PLAYHEAD SETS IT. The cap is real and it
  // bites on exactly the builds people argue about: the board's leading Laetum
  // deals ~230,000 damage instances over 180 s, so a stream asked for from zero
  // runs out after 14 seconds — 8% of the fight, with the other 92% drawing no
  // numbers at all and nothing on screen saying why. Following the playhead is
  // what makes "the numbers are the record" survive the cap, rather than being
  // true only for the opening.
  let loading = false;
  const recordFor = () => (recordState && shownResult
    && recordState.key === recordKey(r) ? recordState : null);
  /// Is `t` inside the stream we hold? A CUT stream covers only as far as it
  /// actually got, which is the last event in it — not the window it asked for.
  const covers = (t) => {
    const rst = recordFor();
    if (!rst || !rst.events || !rst.events.length) return false;
    const lo = rst.from || 0;
    const hi = rst.dropped
      ? rst.events[rst.events.length - 1].t
      : (rst.to == null ? Infinity : rst.to);
    return t >= lo - 1e-9 && t <= hi + 1e-9;
  };
  const withRecord = async () => {
    const at = rp.t[st.i] || 0;
    if (loading || covers(at)) return;
    loading = true;
    // A SECOND BEFORE THE PLAYHEAD, running forward as far as the cap allows.
    // The lead-in is there so a scrub that lands just after a shot still shows
    // the shot's own row rather than starting mid-volley.
    await loadRecord(r, Math.max(0, at - 1));
    loading = false;
    // The reader may have moved on — another run, another weapon — while the
    // engagement was being re-run.
    if (replayState === st) draw();
  };
  const tick = (now) => {
    if (!st.playing) return;
    // Wall-clock paced, so `speed` means what it says however fast the
    // browser paints: 5x is five seconds of fight per second of watching.
    const dtms = st.last ? now - st.last : 16;
    st.last = now;
    st.pos += (dtms / 1000) * st.speed / rp.frame_seconds;
    if (st.pos >= rp.t.length - 1) { st.pos = rp.t.length - 1; stop(); }
    draw();
    // …AND PLAYING OFF THE END OF THE WINDOW FETCHES THE NEXT ONE. Cheap to
    // ask: `covers` is two comparisons, and `withRecord` returns immediately
    // unless the playhead has actually left the stream we hold.
    withRecord();
    if (st.playing) st.raf = requestAnimationFrame(tick);
  };
  // PICK AN ENEMY — from a chip, from a roll-call row, or from the map. Three
  // views of ONE selection, which is why they share the attribute.
  //
  // The debuff table is the only thing that changes: the buffs are the
  // PLAYER's and belong to no body, and the clock does not move — you are
  // asking "what was on THAT one at this instant", not replaying anything.
  /// FOLLOW A BODY THE REPLAY DID NOT — one engagement, and the same one.
  ///
  /// `run` PINS THE FIGHT. The endpoint answers with the state its median run
  /// started from, and handing that back makes this a question about the report
  /// on screen rather than a second report that merely agrees with it. With
  /// `runs: 1` beside it there is no Monte Carlo to pay for: the aggregate that
  /// comes back is one run's and is thrown away — only the frames are kept.
  ///
  /// THE ROW SAYS IT IS WORKING and says so if it fails. A click that produces
  /// nothing and no reason is the worst of the three outcomes.
  const askFoe = async (id, el) => {
    if (!shownResult || !id || el.dataset.busy) return;
    el.dataset.busy = "1";
    const was = el.querySelector(".nm").innerHTML;
    el.querySelector(".nm").textContent = tr("following…");
    try {
      const r = await api("/api/simulate", {
        ...buildPayload(),
        ...theFight({ replay: true, replay_follow: [id], runs: 1, run: shownResult.r.run }),
      });
      const rp = r && r.replay;
      const k = rp && (rp.tracked || []).indexOf(id);
      if (!rp || k < 0) throw new Error("not followed");
      // MERGED INTO THE RESULT IN HAND, never swapped for it: everything else
      // on screen belongs to the run the reader asked for, and this call's
      // single run has no business replacing any of it.
      const cur = shownResult.r.replay;
      cur.tracked = [...(cur.tracked || []), id];
      cur.dstacks = [...(cur.dstacks || []), rp.dstacks[k]];
      replayFoe = cur.tracked.length - 1;
      renderResults(shownResult.r, shownResult.at);
    } catch (_) {
      el.querySelector(".nm").innerHTML = was;
      el.dataset.busy = "";
      presetToast(tr("could not follow that one — try the run again"));
    }
  };

  const pickFoe = (k) => {
    replayFoe = k;
    // FROM THE RESULT IN HAND. No simulation, and no storage lookup either —
    // see `shownResult`. Picking an enemy is a question about a run that has
    // already happened and cannot be a reason to lose it.
    if (shownResult) renderResults(shownResult.r, shownResult.at);
    else renderStoredSimResult();
  };
  document.querySelectorAll("[data-rpfoe]").forEach((el) => {
    el.onclick = () => pickFoe(Number(el.dataset.rpfoe));
  });
  document.querySelectorAll("[data-rpask]").forEach((el) => {
    el.onclick = () => askFoe(el.dataset.rpask, el);
  });
  // THE MAP, mounted last because it measures the box it was given. It is the
  // RESULT's copy of the scene: read-only, shaded by what each body took, and
  // it picks rather than drags (`mountArena`'s analysis mount).
  const sceneEl = $("rp-scene");
  if (sceneEl && (r.bodies || []).length) {
    // BY ARENA INDEX, which is what the scene draws: 0 is the aimed body and
    // `i + 1` is `formation[i]`. The result names bodies by ID, so the two are
    // joined on the name the page itself gave them.
    const idAt = (i) => (i === 0
      ? (rp.tracked || [])[0]
      : ((sim.formation || [])[i - 1] || {}).id || `e${i + 1}`);
    const top = Math.max(...r.bodies.map((b) => b.damage)) || 1;
    const byId = Object.fromEntries(r.bodies.map((b) => [b.id, b.damage / top]));
    const n = 1 + (sim.formation || []).length;
    const heat = Array.from({ length: n }, (_, i) => byId[idAt(i)] || 0);
    const selIdx = Array.from({ length: n }, (_, i) => i)
      .find((i) => idAt(i) === (rp.tracked || [])[replayFoeIdx(rp)]);
    mountArena(sceneEl, sim, (allEnemies().find((e) => e.id === sim.enemy) || allEnemies()[0]), {
      readonly: true,
      heat,
      selected: selIdx,
      onPick: (i) => {
        const k = (rp.tracked || []).indexOf(idAt(i));
        // A BODY THE REPLAY DID NOT FOLLOW has no series to show, so the click
        // does nothing rather than showing somebody else's.
        if (k >= 0) pickFoe(k);
      },
    });
    // THE LAYER THE NUMBERS FLOAT IN, appended AFTER the mount and not before:
    // `mountArena` takes the host over and rewrites its contents, so a layer
    // created first is wiped by the scene it was meant to sit on. It survives
    // the arena's own repaints because those redraw the CANVAS, not the host.
    const layer = document.createElement("div");
    layer.className = "rp-pops";
    sceneEl.appendChild(layer);
  }
  $("rp-play").onclick = () => {
    if (st.playing) { stop(); return; }
    // Pressing play on a FINISHED fight rewinds it — that is what the button
    // means, and leaving it stuck at the end made it look broken.
    if (st.pos >= rp.t.length - 1) { st.pos = 0; draw(); }
    withRecord();
    st.playing = true; st.last = 0;
    $("rp-play").textContent = `❚❚ ${tr("pause")}`;
    st.raf = requestAnimationFrame(tick);
  };
  // STOP — playback ends and the panel goes back to the finished fight. The
  // record window follows it, because the numbers at the end are a window like
  // any other and a stop that left the ledger on second 12 would be two
  // instants on one screen.
  $("rp-stop").onclick = () => {
    stop();
    st.pos = rp.t.length - 1;
    draw();
    withRecord();
  };
  ddReg.get("rp-speed").onPick = (v) => { st.speed = Number(v) || 1; };
  $("rp-scrub").oninput = () => {
    // AFTER the playhead moves, never before: `withRecord` asks for the window
    // around `st.i`, so asking first fetches the window the reader just left.
    stop(); st.pos = Number($("rp-scrub").value); draw();
    withRecord();
  };
  // Collapse one row without losing its place in the group.
  document.querySelectorAll(".rp-row .rp-head").forEach((h) => {
    h.onclick = () => {
      const chart = h.parentElement.querySelector(".rp-chart");
      const open = chart.hidden;
      chart.hidden = !open;
      h.querySelector(".rp-caret").textContent = open ? "▾" : "▸";
    };
  });
  draw();
}

