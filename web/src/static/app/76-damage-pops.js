// THE NUMBERS A FIGHT POPS, over the bodies they landed on.
//
// Everything else the replay draws is a CURVE — a pool falling, a stack count,
// a damage total. This is the one thing that is an EVENT: a discrete number at
// a place at a time, which is how the game reports damage and the only view
// where "one big hit" and "twenty small ones" look different.
//
// THEY ARE THE COMBAT RECORD, REPLAYED, not a second account of it. Two lists
// filled from one `log_damage` call hold DIFFERENT SETS, so a number floats
// over a body with no row to explain it — and for a panel whose claim is "this
// is what happened", one-to-one IS the claim.
//
// So `/api/log` is the ledger, the table lists it, this draws it, and every
// number carries the id of its row. The CAP is a DISPLAY decision made here —
// twelve a frame, biggest kept, the rest counted, which is how the game caps
// its own and unavoidable besides at ~320,000 instances over 600 frames.
//
// A DOM OVERLAY RATHER THAN THE CANVAS: the arena repaints on every resize and
// heat change, and a number wants a CSS animation. `pointer-events: none`, so
// the scene underneath still picks.
const POP_KIND_CLASS = {
  direct: "p-direct", crit: "p-crit", head: "p-head", head_crit: "p-headcrit",
  status: "p-status", blast: "p-blast", blast_area: "p-area",
  field: "p-field", extra: "p-extra", arcane: "p-arcane",
};

// WHERE A BODY STANDS, in the scene's own metres. Index 0 is the aimed body and
// `i + 1` is `formation[i]` — the same numbering `damage_by_body`, the roll
// call and the heat map use, so a number lands on the body the rest of the
// panel is talking about.
const popBodyAt = (i) => (i === 0
  ? (sim.target_at || [0, 0.5])
  : (((sim.formation || [])[i - 1] || {}).at || null));

/// HOW MANY NUMBERS ONE FRAME MAY SHOW. Twelve, and the BIGGEST win: keeping
/// the first twelve would be arbitrary — which twelve depends on the order the
/// engine happened to settle them in — while a reader's eye goes to the big
/// one. What did not fit is counted, never swallowed.
const POPS_PER_FRAME = 12;

/// THE RECORD, BUCKETED INTO THE REPLAY'S OWN FRAMES.
///
/// The engine quantises a replay by pushing a frame every `frame_seconds`, so
/// frame `i` covers `(i x fs, (i+1) x fs]` — the same slice of the clock the
/// curves beside it are drawn from. Computed once per record and cached on it,
/// because a scrub redraws a frame on every pointer move.
///
/// It returns null when there is no record for the result on screen, which is
/// the honest answer rather than an empty overlay: the numbers ARE the record,
/// so before it is read there is nothing to draw. `wireReplay` asks for it.
function popFrames(rp) {
  const st = recordState && shownResult
    && recordState.key === recordKey(shownResult.r) ? recordState : null;
  if (!st || st.loading || !st.events) return null;
  const n = rp.t.length;
  if (st.popFrames && st.popFrames.length === n) return st.popFrames;
  const frames = Array.from({ length: n }, () => []);
  for (const e of st.events) {
    if (e.kind !== "damage" || e.body == null || !(e.effective > 0)) continue;
    const i = Math.min(n - 1, Math.max(0, Math.ceil(e.t / rp.frame_seconds) - 1));
    frames[i].push(e);
  }
  st.popFrames = frames.map((list) => {
    if (list.length <= POPS_PER_FRAME) return { v: list, n: 0 };
    const by = [...list].sort((a, b) => b.effective - a.effective);
    return { v: by.slice(0, POPS_PER_FRAME), n: list.length - POPS_PER_FRAME };
  });
  return st.popFrames;
}

/// Spawn one frame's numbers over the scene.
///
/// `live` distinguishes PLAYING from SCRUBBING: playing spawns each frame once
/// as the clock passes it, scrubbing replaces what is on screen with the frame
/// you landed on. Without that distinction a scrub would either stack hundreds
/// of numbers or show none.
function popsDraw(rp, i, live) {
  const scene = $("rp-scene");
  const host = scene && scene.querySelector(".rp-pops");
  if (!host || !rp) return;
  // VIEWBOX TO PIXELS. The result's scene is the SVG mount, which draws in a
  // fixed coordinate space and lets the browser fit it to the box — so a
  // number's place is that map's answer put through the same fit. Doing the
  // arithmetic here rather than reading a DOM box keeps ONE geometry: whatever
  // `__arena` says is where the body was drawn.
  const ar = scene.__arena;
  const svg = scene.querySelector(".ar-svg");
  if (!ar || !ar.vb || !svg) return;
  const sb = svg.getBoundingClientRect();
  if (!sb.width || !sb.height) return;
  // `xMidYMid meet`, which is the default and what `arenaSvg` relies on.
  const k = Math.min(sb.width / ar.vw, sb.height / ar.vh);
  const ox = (sb.width - ar.vw * k) / 2;
  const oy = (sb.height - ar.vh * k) / 2;
  const map = (m) => {
    const [vx, vy] = ar.vb(m);
    return [ox + vx * k, oy + vy * k];
  };
  // THE MODE IS ON THE LAYER, because it decides the ANIMATION and a CSS class
  // is the only thing that can. Parked numbers rise and hold; playing ones
  // float away.
  host.classList.toggle("live", !!live);
  if (!live) host.textContent = "";
  const all = popFrames(rp);
  if (!all) return;
  const frame = all[i];
  if (!frame) return;
  const box = scene.getBoundingClientRect();
  const dx = sb.left - box.left;
  const dy = sb.top - box.top;
  // FANNED, NOT JITTERED. Several numbers land on ONE body in the same instant —
  // a hit, its crit, three status ticks — and scattering them by a hash of their
  // own value still piles them up as often as not. Counting per BODY and
  // stepping each one out along a fixed fan is readable at any count and
  // identical every time the same frame is drawn, which is what a scrub needs.
  //
  // BIGGEST NEAREST THE BODY, because that is the one the reader came for; the
  // rest step outward and up from it.
  const perBody = new Map();
  // HOW MANY DID NOT FIT ON SCREEN, as opposed to how many the frame's own cap
  // dropped. They are the same fact to a reader — "there were more than this" —
  // so they are counted together and stated in one chip.
  let skipped = 0;
  const ordered = [...(frame.v || [])].sort((a, b) => b.effective - a.effective);
  for (const e of ordered) {
    const body = e.body;
    const amount = e.effective;
    const dtype = e.type;
    const kind = e.pop_kind || "direct";
    const at = popBodyAt(body);
    if (!at) continue;
    const [x, y] = map(at);
    const seen = perBody.get(body) || 0;
    perBody.set(body, seen + 1);
    // A RISING COLUMN, not a fan. A fan needs to clear the number's WIDTH and a
    // five-figure number is 45 px wide, so anything narrow enough to look tidy
    // still overlaps; stacking upward only has to clear the line height. The
    // small left/right alternation is there to break the column's edge so two
    // equal numbers are still visibly two.
    // …AND THE COLUMN STAYS INSIDE THE SCENE, or it is not drawn at all.
    //
    // Twelve numbers can all land on ONE body, and twelve is 250 px of column —
    // taller than the gap above a body standing near the top of the map, so the
    // biggest numbers, which are the ones a reader came for, climbed off the
    // panel entirely (seen on the board's leading Laetum, 2026-08-27). A second
    // COLUMN was the first fix and was worse: a six-figure number is ~110 px
    // wide, so the columns ran through each other and produced digits that
    // belonged to neither.
    //
    // So the geometry decides how many fit, and what does not fit joins the
    // count already beside them. That keeps the two claims this layer makes
    // both true: every number drawn names the row it is, and everything not
    // drawn is stated rather than silently missing.
    const fits = Math.max(1, Math.floor((dy + y - 36) / 21) + 1);
    if (seen >= fits) { skipped += 1; continue; }
    // The step clears the LARGEST line here: a headcrit is set at 19 px, so 17
    // put two of them on top of each other. Starting at -32 clears the body's
    // own name and distance labels rather than landing in them.
    const fx = seen === 0 ? 0 : (seen % 2 ? 11 : -11);
    const fy = -32 - seen * 21;
    const el = document.createElement("span");
    el.className = `rp-pop ${POP_KIND_CLASS[kind] || "p-direct"}`;
    el.style.left = `${dx + x + fx}px`;
    el.style.top = `${dy + y + fy}px`;
    const c = dtColor(dtype);
    if (c) el.style.setProperty("--pop-dt", c);
    el.textContent = Math.round(amount).toLocaleString();
    // THE ROW THIS NUMBER IS. `id` is the event's own place in the stream, so
    // a number on the scene and a line in the table below are the same thing
    // under the same name — which is what one-to-one has to mean to be worth
    // claiming.
    el.dataset.rpevent = e.id;
    el.title = `${DT(dtype)} · ${tr(kind)} · #${e.id}`;
    host.appendChild(el);
    if (live) el.addEventListener("animationend", () => el.remove());
  }
  // …AND WHAT DID NOT FIT. Never silent: a cap nobody is told about reads as
  // "that is all of them", which is the one thing it must not.
  const more = (frame.n || 0) + skipped;
  if (more > 0) {
    const chip = document.createElement("span");
    chip.className = "rp-pop p-more";
    // IN THE CORNER, not above the tallest column. One line above the topmost
    // number is one line above the last line that fits, so the chip itself
    // never does — and the one thing on this layer whose whole job is to say
    // "there were more" is the thing clipped off
    // the top. It is a statement about the FRAME rather than
    // about a body, so the frame's own corner is where it belongs.
    // `.rp-pop` is centred on its point, so the corner needs half a chip of
    // clearance or it hangs over the edge it was moved here to stay inside.
    chip.style.left = `${dx + 26}px`;
    chip.style.top = `${dy + 16}px`;
    chip.textContent = `+${more}`;
    chip.title = tr("more numbers than fit here — the biggest are drawn");
    host.appendChild(chip);
    if (live) chip.addEventListener("animationend", () => chip.remove());
  }
}

/// WHAT IS ON SCREEN RIGHT NOW, and the ONLY thing a redraw reads.
///
/// Picking an enemy in the result must not call `renderStoredSimResult`, which
/// looks the result up in a preset collection: a pick is then a bet that the
/// SAVE worked, and every way it could not have takes the result off screen
/// with it. A full disk is one way. A preset that
/// is not in the list is another, and `saveSimResult` returns early on it
/// without storing anywhere. Neither has anything to do with picking an enemy.
///
/// So a redraw takes the object the panel was drawn from. Storage is what
/// survives a RELOAD or a preset switch, and nothing else asks it a question.
let shownResult = null;

function renderResults(r, testedAt) {
  shownResult = { r, at: testedAt };
  const t = r.target || {};
  const pc = pct2; // 2 decimals, more when the value would otherwise vanish
  const n0 = (x) => Math.round(x || 0).toLocaleString();
  const n2 = sig2;
  const killed = (r.kills || 0) >= 1;
  // ONE scoring unit, killed or not: the KILL SCORE
  // (engine `kill_progress`) = whole kills + the fraction of the current
  // target's pool already drained. 0.85% of an EHP is 0.01; two kills and
  // 30% of the next is 2.30. The sub-line adds the context that differs.
  const ttk = killed ? r.duration / r.kills : Infinity;
  // The headline is a RATE, like DPS: kill score PER MINUTE, so a 20-second
  // run and a 120-second one produce comparable numbers.
  // The score itself is the total over the engagement and stays beside it,
  // the same way total damage sits beside DPS.
  const met = metricOf(sim.metric);
  const heroNum = fmtScore(metricValue(met, r));
  // The UNIT belongs beside the number, not under it: "5.29" on one line and
  // "KPM · …" starting the next read as two facts. Set
  // small and spaced away, so it labels the figure without competing with it.
  const heroUnit = metricLabel(met);
  // HOW FIRM THE NUMBER IS, said beside the number.
  //
  // A mean with no spread invites a reader to compare two builds that differ by
  // less than either one's own run-to-run swing. The range and the deviation
  // are what say whether the gap they are looking at is a gap — and they belong
  // ON the answer rather than folded away under a table nobody opens.
  //
  // ONLY WHERE IT MEANS SOMETHING: one run has no spread to report, and a fight
  // that kills nothing has no kill count to spread.
  const spread = killed && (r.runs || 0) > 1 && r.kills_max > r.kills_min
    ? ` · ${n0(r.kills_min)}–${n0(r.kills_max)} over ${n0(r.runs)} runs (±${sig2(r.kills_std)})`
    : "";
  // …AND THE DEFAULT MEASURE BESIDE IT, when the headline is not it. What a
  // build is FOR is the kill rate, so a reader judging by anything else still
  // wants it — named by the table rather than by the word "KPM", so a third
  // metric borrows the sentence instead of needing a new one.
  const alt = met.id === metricOf().id ? "" : (() => {
    const d = metricOf();
    return `${fmtScore(metricValue(d, r))} ${metricLabel(d)} · `;
  })();
  const heroSub = alt +
    `${n2(r.score)} kill score in ${n0(r.duration)}s · ` + (killed
    ? `${n2(r.kills)} killed · ~${isFinite(ttk) ? ttk.toFixed(2) : "∞"}s avg per kill`
    : `${pc(r.score)} of one ${LN("enemies", sim.enemy, t.name || "enemy")}'s EHP drained`)
    + spread;
  // No Forma/capacity here — the simulator reports EFFECTS only; build
  // legality is the Builder's business.
  // `k` names the replay series that re-reads this cell, and only a cell of the
  // BENCHMARK FIGHT carries it (`live`): the row of means is every run at once,
  // and a replay of one of them has no business rewriting it.
  const kpi = (l, v, k, live) => `<div class="kpi"><div class="kv"${live && k ? ` data-kpi="${k}"` : ""}>${v}</div><div class="kl">${tr(l)}</div></div>`;
  // KPI row: damage pace + crit feel + HANDLING feel (shots, reloads,
  // transforms). In THIS product "DPS" always means
  // EFFECTIVE dps — what the target actually lost, armor and on-target
  // amps included; the weapon-side raw number is out (user: in our
  // context every stat accounts for the enemy).
  const kpis = [
    kpi("DPS", n0(r.dps), "dps"),
    // The TIER leads, and the rate is renamed to what it actually measures.
    // "Crit rate" reads as "my crit chance", and it stops being that the
    // moment a build passes 100%: every pellet crits, so it pins at 100%
    // whether the build is at 110% or 410% (group, 2026-07-31). The tier is
    // the same number without that truncation — and the one that multiplies
    // the damage. 1 = yellow, 2 = orange, 3 = red, and it keeps going.
    kpi("Crit tier", (r.crit_tier ?? 0).toFixed(2), "crit_tier"),
    kpi("Pellets crit", pc(r.crit_rate), "crit_rate"), kpi("Orange+", pc(r.big_crit_rate), "big_crit_rate"),
    kpi("Procs", n0(r.procs), "procs"), kpi("Shots", n0(r.shots), "shots"),
    // WHAT THE FIGHT SPENT ON NOTHING. A unit dies once however far past zero
    // it goes, so the excess on a killing blow bought nothing — and a build
    // can deal MORE damage while killing FEWER units by spending the
    // difference on corpses. Read against what the kills actually cost, so it
    // is not capped at 100%.
    kpi("Overkill", pc(r.overkill_rate), "overkill_rate"),
    // THE AVERAGE VIRAL PILE THE DAMAGE WAS DEALT THROUGH — every body and
    // every run, where the debuff chart below follows eight bodies of one
    // engagement. Absent on a build that never applies Viral.
    r.virus_stacks == null ? "" : kpi("Viral stacks", r.virus_stacks.toFixed(2), "virus_stacks"),
    // …AND WHAT WAS LEFT OF THE ARMOUR when it arrived. The companion to the
    // tile above: a Corrosive pile that finishes after the kill stripped
    // nothing, and only this says so.
    r.armor_left == null ? "" : kpi("Armour left", pc(r.armor_left), "armor_left"),
    kpi("Reloads", n0(r.reloads), "reloads"), kpi("Transforms", n0(r.transforms), "transforms"),
    // COUNTED, NOT FOUGHT. The row is drawn only where a weapon leaves
    // something standing, and it says BOTH numbers because neither is the
    // mechanic on its own: how many were left, and how many stood at once.
    r.ghosts == null ? "" : kpi("Ghosts", `${n0(r.ghosts)} (${n0(r.ghosts_peak)} ${tr("at once")})`, "ghosts"),
  ].join("");
  // WoW-style damage meter: effective damage BY SOURCE
  // over the whole engagement — what actually hurt the target. The panel's
  // per-shot theory lives in the Builder's Stats, not here.
  const srcs = r.damage_sources || [];
  const srcTotal = srcs.reduce((a, x) => a + x.dmg, 0) || 1;
  const srcMax = (srcs[0] && srcs[0].dmg) || 1;
  // Bucket rows are named; a status row is named by its damage type. `field`
  // and `radial` fell through to `DT()`, which knows damage types only, so
  // they printed their raw wire key in every language.
  const SRC_LABEL = { direct: "Direct hits", radial: "Radial (AoE)",
    field: "Lingering field", arcane: "Arcane (on status)",
    syndicate: "Syndicate radial", "extra hit": "Extra hit (ability)" };
  const srcLabel = (k) => (SRC_LABEL[k] ? tr(SRC_LABEL[k]) : DT(k));
  // A WEAPON-damage row EXPANDS into the damage types it was dealt as — a
  // status row already IS one type, which is what a proc is. Both levels use
  // the same denominator, so every percentage in the meter reads against the
  // engagement total and the numbers still sum to 100%.
  //
  // The shares are the QUANTIZED ones (the vector that actually landed), so
  // they will not match the Builder's panel exactly — the Torid's 164.73
  // Corrosive / 52.02 Magnetic reads 76/24 there and 75/25 here, because
  // quantization snaps each component to a multiple of total/32.
  // A BAR IS COLOURED BY WHAT IT IS, not by where it sits.
  //
  // A damage TYPE gets DE's own colour (style.css, from the wiki's
  // `Module:DamageTypes/data`); a SOURCE — "Direct hits", "Radial (AoE)" — is
  // not a damage type and has no official colour, so it keeps a positional one
  // from the `--s1..8` ramp. That is the split the meter was missing: it
  // coloured everything positionally, so Heat was one colour under a direct hit
  // and another under a field, and neither was Heat's.
  const mbar = (w, ty, c, dim) => {
    const col = dtColor(ty) || `var(--s${c})`;
    return `<div class="mbar"><i style="width:${w.toFixed(1)}%;background:${col}${dim ? ";opacity:.65" : ""}"></i></div>`;
  };
  // The meter is re-read per frame too, so every row carries the key of the
  // series that feeds it — top-level by source, sub-rows by source+type.
  const mkey = (s, ty) => ` data-mk="${escHtml(ty ? `${s}::${ty}` : s)}"`;
  const meter = srcs.map((x, i) => {
    const c = (i % 8) + 1;
    const parts = x.by_type && x.by_type.length > 1 ? x.by_type : null;
    const open = !!parts && simMeterOpen.has(x.source);
    const head = `<div class="mrow${parts ? " exp" : ""}" data-src="${escHtml(x.source)}"${mkey(x.source)} data-c="${c}">
      <span class="mname">${parts ? `<span class="mcaret">${open ? "▾" : "▸"}</span>` : ""}${dtIcon(x.source)}${srcLabel(x.source)}</span>
      ${mbar(x.dmg / srcMax * 100, x.source, c, false)}
      <span class="mval">${n0(x.dmg)} · ${pct2(x.dmg / srcTotal)}</span>
    </div>`;
    if (!parts) return head;
    return head + parts.map((p) => `<div class="mrow sub" data-of="${escHtml(x.source)}"${mkey(x.source, p.type)} data-c="${c}"${open ? "" : " hidden"}>
      <span class="mname">${dtIcon(p.type)}${DT(p.type)}</span>
      ${mbar(p.dmg / srcMax * 100, p.type, c, true)}
      <span class="mval">${n0(p.dmg)} · ${pct2(p.dmg / srcTotal)}</span>
    </div>`).join("");
  }).join("");
  // WHAT THE DAMAGE WAS MADE OF — one stacked bar over the whole engagement,
  // in DE's own colours.
  //
  // The meter above answers "where did it come from" — direct hits, a cloud, a
  // proc. This answers a different question a build actually turns on: what
  // ELEMENTS is that, added up. They are the same damage counted two ways, so
  // both read against the same total and both come to 100%.
  //
  // Aggregated from the meter's own rows rather than from a second field, so
  // the bar cannot disagree with the list under it: a source that splits by
  // type contributes its split, and a source that IS a type (a status row)
  // contributes itself.
  const typeTotals = {};
  for (const x of srcs) {
    const parts = x.by_type && x.by_type.length ? x.by_type : [{ type: x.source, dmg: x.dmg }];
    for (const p of parts) {
      if (!dtKey(p.type)) continue;   // a source name is not a damage type
      typeTotals[dtKey(p.type)] = (typeTotals[dtKey(p.type)] || 0) + p.dmg;
    }
  }
  const typeRows = Object.entries(typeTotals)
    .filter(([, v]) => v > 0)
    .sort((a, b) => b[1] - a[1]);
  const typeSum = typeRows.reduce((a, [, v]) => a + v, 0);
  // Biggest first, so the bar reads left to right in the order the legend
  // does and the eye can match a segment to a line without hunting.
  const composition = typeRows.length ? `
      <h3>${tr("Damage by type")} <span class="sim-hint">${tr("share of the engagement")}</span></h3>
      <div class="dmg-bar">${typeRows.map(([ty, v]) =>
        `<i class="dmg-seg" style="flex:${(v / typeSum).toFixed(5)};background:${dtColor(ty)}" title="${escHtml(DT(ty))} ${pct2(v / typeSum)}"></i>`).join("")}</div>
      <div class="legend">${typeRows.map(([ty, v]) =>
        `<span class="li">${dtIcon(ty)}${escHtml(DT(ty))} <span class="lv">${pct2(v / typeSum)}</span></span>`).join("")}</div>` : "";

  // DPS-over-time curve: the MEDIAN run's per-bucket
  // EFFECTIVE dps. One series — the accent line, recessive grid, hover
  // crosshair + tooltip; no legend needed.
  const tl = r.timeline || [];
  const bucketSecs = (r.duration || 1) / (tl.length || 1);
  const dpsPts = tl.map((v) => v / bucketSecs);
  const tlMax = Math.max(1, ...dpsPts);
  const W = 600, H = 170, PADL = 8, PADR = 8, PADT = 8, PADB = 6;
  const px = (i) => PADL + ((i + 1) / tl.length) * (W - PADL - PADR);
  const py = (v) => PADT + (1 - v / tlMax) * (H - PADT - PADB);
  const pts = dpsPts.map((v, i) => `${px(i)},${py(v)}`).join(" ");
  const tlGrid = [0.25, 0.5, 0.75].map((f) =>
    `<line class="tl-grid" x1="${PADL}" x2="${W - PADR}" y1="${py(tlMax * f)}" y2="${py(tlMax * f)}"/>`).join("");
  const chart = tl.length ? `
      <h3>${tr("DPS over time")} <span class="sim-hint">${tr("benchmark fight")}</span></h3>
      <div class="tl-wrap">
        <svg id="tl-svg" viewBox="0 0 ${W} ${H}" preserveAspectRatio="none">
          ${tlGrid}
          <polyline class="tl-line" points="${pts}"/>
          <rect id="tl-ahead" class="tl-ahead" x="${W}" y="0" width="0" height="${H}" hidden/>
          <line id="tl-now" class="tl-now" y1="0" y2="${H}" x1="0" x2="0" hidden/>
          <line id="tl-cross" class="tl-cross" y1="${PADT}" y2="${H - PADB}" hidden/>
        </svg>
        <div class="tl-x"><span>0s</span><span>${n0(r.duration)}s</span></div>
        <div class="tl-ymax">${n0(tlMax)}</div>
        <div id="tl-tip" class="tl-tip" hidden></div>
      </div>` : "";
  const { bar: replayBar, curves: replayCurves } = replayMarkup(r);
  // THE BENCHMARK FIGHT — one of the runs, the middle one by the metric
  // (`Summary::median_run`). Everything above it is a MEAN and holds still;
  // everything in its block is that one run and replays. Its figures differ
  // from the means, so the block states both numbers side by side.
  const sm = r.sample;
  const rpk = (r.replay && r.replay.kpi) || null;
  const lastOf = (s) => (s && s.length ? s[s.length - 1] : 0);
  const sampleKpis = rpk ? [
    kpi("DPS", n0(lastOf(rpk.dps)), "dps", true),
    kpi("Crit tier", lastOf(rpk.crit_tier).toFixed(2), "crit_tier", true),
    kpi("Pellets crit", pc(lastOf(rpk.crit_rate)), "crit_rate", true),
    kpi("Orange+", pc(lastOf(rpk.big_crit_rate)), "big_crit_rate", true),
    kpi("Procs", n0(lastOf(rpk.procs)), "procs", true),
    kpi("Shots", n0(lastOf(rpk.shots)), "shots", true),
    kpi("Reloads", n0(lastOf(rpk.reloads)), "reloads", true),
    kpi("Transforms", n0(lastOf(rpk.transforms)), "transforms", true),
  ].join("") : "";
  const benchHead = sm ? `<div class="bench-head">
      <h3>${escHtml(tr("Benchmark fight"))}</h3>
      <div class="bench-num"><span data-hero="${met.id}">${fmtScore(metricValue(met, { ...sm, duration: r.duration }))}<span class="hero-unit">${heroUnit}</span></span>
        <span class="sim-hint">${escHtml(trF("average {v}", { v: `${heroNum} ${heroUnit}` }))}</span></div>
      <div class="sim-hint">${escHtml(trF("one of the {runs} runs, the middle one when ranked by {unit} — its numbers differ from the average", { runs: n0(r.runs), unit: heroUnit }))}</div>
    </div>` : "";
  // THE `Detail` TABLE IS GONE. Five of its six rows were the fight restated —
  // the target, its pools, its armour, the shot count — each of which the
  // scenario panel above states while it is being CHOSEN, which is when a
  // reader needs them. The sixth was the answer's own spread, and that has
  // moved onto the answer (`spread`), where a reader deciding between two
  // builds actually looks.
  // THE ASK, BENEATH THE ANSWER. The topbar is chrome and
  // the ask lived only there, which put it as far from the moment this tool
  // delivers anything as a page can put it. This is not an interruption: it is
  // a line of text below the last fold, after the result, and it never moves,
  // blocks, animates or asks twice.
  //
  // IT STATES THE WORK BEFORE IT STATES THE ASK, and both numbers are ones the
  // reader can check against the Detail table directly above it — `runs` is the
  // count they set, and `pellets` is PER RUN on the wire (webapi/src/lib.rs),
  // so the product is the damage instances this answer was actually averaged
  // over. A figure a reader can verify is the only kind worth printing here.
  const askRuns = Math.round(r.runs || 0), askRolls = Math.round(askRuns * (r.pellets || 0));
  const ask = `<p class="sim-ask">${escHtml(trF(
    "{runs} engagements · {rolls} damage instances rolled, on your own machine.",
    { runs: askRuns.toLocaleString(), rolls: askRolls.toLocaleString() }))} ${
    escHtml(tr("Builder, simulator and optimizer are free for everyone."))} <a href="/support">${
    escHtml(tr("Chip in ↗"))}</a></p>`;
  $("sim-results").innerHTML = `
    <!-- WHAT BECAME OF THIS RUN, above the result and OUTSIDE it. It sat inside the result, under the headline, and read as
         a footnote to the number — which it is not: it is about the RUN, not
         about the damage, and it carries the one sentence a submitter has to
         act on. Its own block, above the thing it is not part of. -->
    <div id="sim-board-outcome" class="board-outcome"></div>
    <div class="results">
      <div class="hero"><div><div class="hero-label">${escHtml(trF("Average of {runs} runs", { runs: n0(r.runs) }))}</div><div class="hero-num">${heroNum}<span class="hero-unit">${heroUnit}</span></div><div class="hero-sub">${heroSub}</div>${testedAt ? `<div class="hero-tested">${tr("last tested")} ${new Date(testedAt).toLocaleString()}</div>` : ""}</div></div>
      <div class="kpi-row">${kpis}</div>
      ${speedMarkup(r)}
      <div class="bench">
        ${benchHead}
        ${replayBar}
        ${sampleKpis ? `<div class="kpi-row">${sampleKpis}</div>` : ""}
        ${foldBlock("meter", tr("Damage by source"), "",
          `<div class="meter">${meter.length ? meter : `<div class="sb-empty">${tr("no damage dealt")}</div>`}</div>${composition}`)}
        ${recordMarkup(r)}${chart}${replayCurves}
      </div>
      ${ask}
    </div>`;
  // WHAT BECAME OF THIS RUN, drawn on EVERY result — a stored one re-rendered
  // after a reload, a pick in the roll call, and the fresh run that
  // `offerBoardSubmit` is about to repaint when its verdict lands. Filling it
  // only from there would leave "submitting…" standing for ever on the two
  // paths that submit nothing: a scenario of your own, and a board row.
  renderBoardOutcome();
  // Meter rows that carry a per-type split toggle theirs. The choice is kept
  // across runs — a player who opened Direct hits wants it open on the next
  // simulate, not to reopen it every time.
  $("sim-results").querySelectorAll(".mrow.exp").forEach((el) => {
    el.addEventListener("click", () => {
      const k = el.dataset.src;
      const open = !simMeterOpen.has(k);
      if (open) simMeterOpen.add(k);
      else simMeterOpen.delete(k);
      el.querySelector(".mcaret").textContent = open ? "▾" : "▸";
      $("sim-results")
        .querySelectorAll(`.mrow.sub[data-of="${CSS.escape(k)}"]`)
        .forEach((c) => { c.hidden = !open; });
    });
  });
  wireFolds();
  // THE RECORD'S OWN CONTROLS. Its body is rebuilt with the rest of the panel,
  // so its buttons are rebound here rather than kept — and the state that says
  // WHICH body and WHICH filter lives outside the markup, the same rule the
  // fold state follows one function over.
  paintRecord(r);
  wireReplay(r);
  // Chart hover: crosshair + tooltip on the nearest time bucket.
  const wrap = $("sim-results").querySelector(".tl-wrap");
  if (wrap) {
    const svg = wrap.querySelector("#tl-svg");
    const tip = wrap.querySelector("#tl-tip");
    const cross = wrap.querySelector("#tl-cross");
    wrap.addEventListener("mousemove", (ev) => {
      const b = svg.getBoundingClientRect();
      const fx = (ev.clientX - b.left) / b.width;
      const i = Math.max(0, Math.min(tl.length - 1, Math.round(fx * tl.length) - 1));
      cross.hidden = false;
      cross.setAttribute("x1", px(i)); cross.setAttribute("x2", px(i));
      tip.hidden = false;
      tip.textContent = `${(((i + 1) / tl.length) * r.duration).toFixed(1)}s · ${n0(dpsPts[i])} DPS`;
      tip.style.left = Math.min(b.width - 120, Math.max(0, ev.clientX - b.left + 10)) + "px";
    });
    wrap.addEventListener("mouseleave", () => { cross.hidden = true; tip.hidden = true; });
  }
}


