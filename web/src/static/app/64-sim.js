// ---- Sim: scenario/buff settings + run against an enemy -----------------
// The build (mods/arcane/evolutions) comes from buildPayload(); this block
// only owns the scenario + engine-modeled buff levers (`sim`). Run POSTs to
// /api/simulate and renders a summary card + an illustrative arena replay.
// The SIMULATOR tab's read-only build summary — the sim always tests the
// ACTIVE preset's build, and this card shows exactly what that is (mods
// at rank, arcane, evolutions). Editing happens in the Builder; the
// button jumps there.
function renderSimBuild() {
  const box = $("sim-build-info");
  if (!box || !META) return;
  const sub = $("sim-build-sub");
  // The LABEL, never the id: an official build's id carries its ruler and rank
  // (`standard_single_target#cycle#1`) and is not a thing to show anyone.
  const activeLabel = presetLabel(buildNamed(activePreset));
  if (sub) sub.textContent = activeLabel ? `${tr("testing build")}: ${activeLabel}` : "";
  const w = weaponInfo($("weapon").value);
  box.innerHTML = cardOfState(snapshotState(), w)
    + aplHtml(w) + `<div class="sb-wielder"></div><a class="ghost-btn small sb-edit" href="${weaponPath($("weapon").value)}">${tr("edit in Builder")}</a>`;
  renderSimWielder(box.querySelector(".sb-wielder"));
  // The part value analysis lists THIS build's parts, so it follows the card.
  renderShapley();
}

/// **THE LIST THE FIGHT RAN**, in the order it is scanned.
///
/// A mode has always BEEN an action priority list — `play_modes` calls it "a
/// policy over its forms" — and it was only ever written in Rust. This is that
/// policy shown, in the vocabulary the combat record uses for what a fight does
/// (a press, a reload, a transmute's two ends) and the shape SimC writes one
/// in: top down, the first rule that holds is what the player does.
///
/// **THE FIGHT'S OWN ANSWER, NEVER COMPOSED HERE.** Which press a mode is
/// played on is the FORM's, and whether the Tennokai flash converts a swing is
/// the BUILD's — so only a run can say, and what it says comes back on the
/// response. Nothing to show before the first run, which is honest: no fight
/// has happened.
function aplHtml(w) {
  const lines = (shownResult && shownResult.r && shownResult.r.apl) || [];
  if (!lines.length) return "";
  return `<div class="sb-h">${escHtml(tr("Action priority"))} · ${escHtml(modeLabel(w, mode) || "")}</div>`
    + `<ol class="sb-apl">${lines.map((l) => {
      const [act, cond] = l.split(",if=");
      return `<li><code>${escHtml(act)}</code>${cond ? ` <span class="sb-empty">if ${escHtml(cond)}</span>` : ""}</li>`;
    }).join("")}</ol>`;
}

/// THE WIELDER AS THE SIMULATOR SHOWS IT: read-only, the Warframe with the
/// Operator it holds nested under it — the nesting is the model — and each with
/// the way to the page that edits it. The Builder's Wielder block is the
/// complete editor; this is the summary of what is being tested.
async function renderSimWielder(host) {
  await loadWarframeCatalog();
  const held = buildWielder;
  if (!host || !host.isConnected) return;
  const title = (label, at, href) => `<div class="wld-title"><b>${escHtml(label)}</b>${at ? ` <span class="sb-empty">${escHtml(at)}</span>` : ""}
    <a class="ghost-btn small" href="${href}">${escHtml(tr("edit on its page"))}</a></div>`;
  // A ROBOTIC WEAPON IS HELD BY A COMPANION, which seats nothing the engine reads
  // yet — so the summary is the host and its build, and there is no frame under it.
  if (isHostId(held.frame)) {
    const cb = wielderBuild(held);
    host.innerHTML = `<div class="sb-h">${escHtml(tr("Wielder"))}</div><div class="wld-nest">`
      + title(holderName(held.frame), cb ? cb.name : "",
        `${holderPath(held.frame)}?build=${encodeURIComponent(wielderIdOf(held))}`)
      + `<div class="sb-chips"><span class="sb-empty">${escHtml(tr("A companion's own mods are not modelled yet."))}</span></div></div>`;
    return;
  }
  const f = (META.warframes || []).find((x) => x.id === held.frame);
  if (!f) return;
  const b = wielderBuild(held);
  const st = wfNormalize(b ? b.state : null, f.id);
  const opBuild = opBuildOf(st.operator);
  const os = opBuild && opNormalize(opBuild.state);
  const school = os && focusSchool(os.school);
  const chips = (head, list, empty) => `<div class="sb-h">${escHtml(head)}</div><div class="sb-chips">${
    list.map((label) => `<span class="sb-chip"><span>${escHtml(label)}</span></span>`).join("")
    || `<span class="sb-empty">${escHtml(empty)}</span>`}</div>`;
  const name = (m) => (m || {}).name;
  const shards = st.shards.filter(Boolean).map((p) => {
    const d = SHARDS().find((x) => x.id === p.shard);
    const o = d && d.options.find((x) => x.id === p.effect);
    return d && o ? wfShardLine(d, o, p.tauforged) : null;
  }).filter(Boolean);
  host.innerHTML = `<div class="sb-h">${escHtml(tr("Wielder"))}</div><div class="wld-nest">`
    + title(f.name, b ? b.name : "", `${warframePath(f)}?build=${encodeURIComponent(wielderIdOf(held))}`)
    + chips(tr("Aura"), [name(wfMod(st.slots[WF_AURA].mod))].filter(Boolean), tr("no aura"))
    + chips(`${tr("Mods")}`, st.slots.slice(0, 9).map((s) => name(wfMod(s.mod))).filter(Boolean), tr("no mods equipped"))
    + chips(tr("Archon shards"), shards, tr("empty socket"))
    + `<div class="wld-op">`
    + title(tr("Operator"), opBuild ? opBuild.name : "", opBuild ? `/operator?build=${encodeURIComponent(opBuild.id)}` : "/operator")
    + chips(tr("School"), school ? [school.name] : [], tr("no linked Operator"))
    + (school ? chips(tr("Artifact"), (os.artifact.mods || []).map((id) => name(opAMod(id))).filter(Boolean), tr("no mods equipped")) : "")
    + `</div></div>`;
}

/// A MODULAR WEAPON'S TWO PARTS AS CHIPS, named from its own assembly spec.
function partChipsOf(weaponId, grip, loader) {
  const s = assemblySpec(weaponId) || {};
  const name = (list, id) => ((list || []).find((x) => x.id === id) || {}).name || prettify(id);
  return [grip && { label: name(s.grips, grip), key: "part:" + grip },
    loader && { label: name(s.loaders, loader), key: "part:" + loader }].filter(Boolean);
}

/// THE CURRENT WEAPON'S EVOLUTIONS AS CHIPS, tier first — by id, so a board
/// row's list and the live selection read the same way.
function evoChipsOf(ids) {
  return weaponEvos().map((t) => {
    const o = t.options.find((x) => ids.includes(x.id));
    return o ? { img: o.icon ? IMG(o.icon) : null, label: `${t.tier} · ${o.name}`, key: "evo:" + o.id } : null;
  }).filter(Boolean);
}

/// A BUILD AS CHIPS — the simulator's "what is being tested" card, and every
/// row of the build finder. It takes a DESCRIPTOR, never live state, so the two
/// cannot drift into two pictures of one build. A section given as null is not
/// drawn (a weapon without that axis); an empty one says so. A chip's `cls`
/// carries the finder's comparison marks.
///
/// THE BUILDER'S OWN ORDER, so the two read line for line: mode, mods, parts,
/// arcane, evolutions, and the valence LAST — which on an adversary weapon, with
/// no evolutions, makes it the fourth block. The mode is stated even where the
/// weapon has one: a summary that drops a field the build has is not a summary.
/// A BUILD STATE AS THE SIMULATOR'S CARD — the one brief every surface that
/// shows a build draws: the simulator's own, and the optimizer's starts. `fixed`
/// (a set of `mods:i` / `arcane:i` keys) marks the positions a start pins.
function cardOfState(st, w, fixed) {
  const pinned = (k) => (fixed && fixed.has(k) ? "fixed" : "");
  const arcs = st.arcane || [];
  const ranks = st.arcaneRank || [];
  return buildCardHtml({
    // A START (the call that passes pins) that names no mode shows the one the
    // builder opens this weapon in; the simulator's build shows the builder's.
    mode: modeLabel(w, st.mode || (fixed ? defaultMode(w.id, null) : mode)),
    mods: (st.slots || []).map((s, i) => {
      const m = s && s.mod && modById(s.mod);
      return m ? { img: IMG(m.image), label: m.name, rank: s.rank == null ? m.max_rank : s.rank,
        cls: pinned("mods:" + i) } : null;
    }).filter(Boolean),
    arcanes: (w.arcane_slots || 0) >= 1
      ? arcs.map((id, i) => {
        const a = id !== "none" && arcaneById(id);
        return a ? { img: IMG(a.image), label: a.name, rank: ranks[i] ?? ((a.ranks || []).length - 1),
          cls: pinned("arcane:" + i) } : null;
      }).filter(Boolean)
      : null,
    parts: st.assembly ? partChipsOf(w.id, st.assembly.grip, st.assembly.loader) : null,
    evolutions: w.uses_evo2 ? evoChipsOf(Object.values(st.evoSel || {}).filter(Boolean)) : null,
    valence: valenceSpec(w.id) && st.valence
      ? `${DT(st.valence.element)} +${Math.round(st.valence.bonus * 1000) / 10}%` : null,
  });
}

function buildCardHtml(d) {
  const chip = (c) => `<span class="sb-chip${c.cls ? " " + c.cls : ""}"${c.title ? ` title="${escHtml(c.title)}"` : ""}>` +
    `${c.img ? imgTag(c.img, "sb-img") : ""}<span>${escHtml(c.label)}</span>${c.rank != null ? `<span class="rk">R${c.rank}</span>` : ""}</span>`;
  const section = (head, chips, empty) => chips == null ? ""
    : `<div class="sb-h">${head}</div><div class="sb-chips">${chips.map(chip).join("") || `<span class="sb-empty">${empty}</span>`}</div>`;
  return [
    d.mode != null ? section(tr("Mode"), [{ label: d.mode }]) : "",
    section(`${tr("Mods")} · ${d.mods.length}`, d.mods, tr("no mods equipped")),
    d.parts ? section(tr("Parts"), d.parts) : "",
    section(tr("Arcane"), d.arcanes, tr("no arcane")),
    section(tr("Evolutions"), d.evolutions, tr("none selected")),
    // Its percentage rides with the element: 60% Heat and 25% Heat are two builds.
    d.valence ? section(tr("Valence"), [{ label: d.valence }]) : "",
  ].join("");
}

/// THE FIGHT AS A CARD — the scenario read back in the build card's own chips,
/// for a surface that runs the simulator's fight and does not edit it. A section
/// the fight leaves empty says so, the way the build card does.
function fightCardHtml() {
  const chip = (label, title) => `<span class="sb-chip"${title ? ` title="${escHtml(title)}"` : ""}><span>${escHtml(label)}</span></span>`;
  const section = (head, chips, empty) => `<div class="sb-h">${escHtml(head)}</div><div class="sb-chips">${
    chips.join("") || `<span class="sb-empty">${escHtml(empty)}</span>`}</div>`;
  const en = allEnemies().find((e) => e.id === sim.enemy) || {};
  const gap = Math.hypot((sim.target_at || [0, 0])[0] - (sim.player_at || [0, 0])[0],
    (sim.target_at || [0, 0])[1] - (sim.player_at || [0, 0])[1]);
  const enemy = [chip(`${en.name || sim.enemy} Lv ${sim.level}${sim.steel_path ? " (SP)" : ""}`)];
  if (sim.eximus) enemy.push(chip(tr("Eximus")));
  const play = [
    chip(`${sim.duration} s`),
    chip(`${sim.headshot_pct}% ${tr("headshots")}`),
    chip(tr(sim.aiming ? "Aiming" : "hip-fire")),
    chip(`${Math.round(gap * 10) / 10} m`),
    chip(metricLabel(metricOf(sim.metric))),
  ];
  if (sim.invisible) play.push(chip(tr("invisible")));
  if (sim.airborne) play.push(chip(tr("airborne")));
  if (!sim.infinite_ammo) play.push(chip(tr("ammo counts")));
  const squad = alsoActing().map((ref) => chip(weaponExists(ref.weapon) ? tf(weaponInfo(ref.weapon).name) : ref.weapon));
  const frame = (META.warframes || []).find((f) => f.id === sim.frame);
  const wf = [
    ...(frame ? [chip(frame.name)] : []),
    ...wfAbilities().filter((a) => wfPick(a.id)).map((a) => chip(wfName(a), wfValueLabel(a))),
    ...(sim.auras || []).map((a) => chip(`${(AURAS().find((x) => x.id === a.id) || {}).name || a.id}${a.count > 1 ? ` ×${a.count}` : ""}`)),
    ...(sim.shards || []).map((p) => {
      const d = SHARDS().find((x) => x.id === p.shard);
      const o = d && d.options.find((x) => x.id === p.effect);
      return d && o ? chip(wfShardLine(d, o, p.tauforged)) : "";
    }),
  ];
  if (wf.length && Math.round((Number(sim.ability_strength) || 0) * 100) !== 100) {
    wf.push(chip(`${Math.round(sim.ability_strength * 100)}% ${tr("Ability Strength")}`));
  }
  const extra = EXTRA_STAT_KEYS.filter(([k]) => Number((sim.extra_stats || {})[k]))
    .map(([k, label]) => chip(`${tr(label)} +${Math.round(sim.extra_stats[k] * 1000) / 10}%`));
  const buffs = Object.entries(sim.buffs || {}).filter(([, c]) => c && c.stacks > 0).map(([id, c]) => {
    const b = buffList.find((x) => x.id === id) || {};
    return chip(`${b.name || prettify(id)} ${c.stacks}${b.max_stacks > 1 ? `/${b.max_stacks}` : ""}`);
  });
  return section(tr("Enemy"), enemy)
    + section(tr("The Tenno"), play)
    + section(tr("Who else is firing"), squad, tr("nobody else"))
    + section(tr("Warframe buffs"), wf, tr("none"))
    + section(tr("Extra stats"), extra, tr("none"))
    + section(tr("Buffs"), buffs, tr("none start stacked"));
}

// The headshot rate a weapon is played at. A SENTINEL is fired by the
// companion, which picks its own targets and does not aim for the head, so it
// starts at 0 rather than the player's 100. Still a knob
// here — the engine is what PINS a sentinel at 0 whatever a request says, so
// this only decides where the control opens.
const defaultHeadshotPct = (w) => ((w || {}).sentinel ? 0 : META.defaults.headshot_pct);

/// THE WIELDER'S FLOOR, from `/api/meta` — `data/tenno/default.yaml`, the worst
/// max-rank frame that does not exist.
/// WHOSE FLOOR THIS WEAPON'S FIGHT STARTS FROM — a Warframe's, or a SENTINEL's
/// for the 21 companion weapons. Two rosters, two lowest
/// values: 450/130/80 against 250/0/105, so showing one for the other told a
/// reader their Artax had a Warframe's armor.
/// THE WIELDER BEFORE THE OVERRIDES, as the server resolved it on the last panel —
/// it is the server that resolves a linked Warframe build. The Prototype (or the
/// Sentinel) until a panel has answered.
let panelWielder = null;
const tennoFloor = () => {
  if (panelWielder) return panelWielder;
  const w = (META.weapons || []).find((x) => x.id === ($("weapon") || {}).value);
  const f = (w && w.sentinel ? META.sentinel_floor : META.tenno_floor);
  return f || { name: "", health: 0, shield: 0, armor: 0, energy: 0, sprint: 0.9 };
};

/// ONE OVERRIDABLE WARFRAME STAT — a tick and a number.
///
/// A BARE NUMBER CANNOT SAY "LEAVE IT ALONE". Sent on every fight, it
/// overwrites the floor in `data/tenno/default.yaml` before it is read, so "the
/// neutral Tenno has 105 armor" is true of the data and false of what runs. Nor
/// can it say the other thing: four released frames have NO energy pool, so 0
/// is a real value that "unset" cannot be spelled as.
///
/// So the tick IS the override: unticked, the key is dropped and the server
/// falls back to the floor; ticked, the number is sent and starts AT the floor,
/// so ticking alone changes nothing.
/// WHAT THE FIGHT STARTS FROM, stated before the boxes that change it.
///
/// The four controls under this line are OVERRIDES, and an override shown
/// alone cannot be read: nothing tells "0 because there is no frame" from "0
/// because that IS the floor", and the second is true of two of these stats.
///
/// IT NAMES WHAT IT IS. "The worst max-rank frame that does not exist" is
/// `data/tenno/default.yaml`'s claim, and it is why a number here means
/// something: a bonus this player pays is a bonus EVERY frame pays.
const wfFloorLine = () => {
  const f = tennoFloor();
  // WHOLE NUMBERS STAY WHOLE. Four of these five are integers and `sig2` wrote
  // "250.00" for them, which reads as a measurement rather than as a stat.
  const num = (v) => (Number.isInteger(v) ? String(v) : sig2(v));
  const cell = (l, v) => `<span><b>${escHtml(l)}</b> ${num(v)}</span>`;
  return `<div class="wffloor" title="${escHtml(tr(
    "the fight's wielder before any override: each stat is the LOWEST any released Warframe has at rank 30, so a bonus it pays is one every frame pays. EVERY OFFICIAL BOARD IS SCORED ON THIS WIELDER — tick a box below to test a real frame instead"))}">`
    + `<span class="wffloor-h">${escHtml(
        (f.name === "Prototype Companion" ? tr("Companion floor") : tr(f.name || "Prototype"))
        + " · " + tr("the wielder before the overrides below — every board is scored on the Prototype"))}</span>`
    + cell(tr("Health"), f.health) + cell(tr("Shield"), f.shield)
    + cell(tr("Armor"), f.armor) + cell(tr("Max energy"), f.energy)
    + (f.name === "Prototype Companion" ? "" : cell(tr("Sprint"), f.sprint))
    + `</div>`;
};

const wfOverride = (key, label, floorKey, min, max, step, why) => {
  const floor = tennoFloor()[floorKey];
  const on = sim[key] !== undefined && sim[key] !== null;
  const shown = on ? sim[key] : floor;
  return `<label class="wfov ${on ? "" : "off"}" title="${escHtml(why)}">`
    + `<input type="checkbox" data-k="${key}" data-wfov="${floorKey}"${on ? " checked" : ""}>`
    + ` ${escHtml(label)} `
    + `<input type="number" data-k="${key}" data-wfovnum="1" min="${min}" max="${max}" step="${step}"`
    + ` value="${shown}"${on ? "" : " disabled"}>`
    + `</label>`;
};

// The SCENARIO — enemy, technique, measurement — is the SIMULATOR's, and the
// optimizer borrows it rather than keeping a lookalike: one state, one
// renderer, so a search is scored under the fight you are simulating with by
// construction. `ids` names the host per section and a section with no host is
// not drawn. ENGAGEMENT LENGTH sits with the enemy rather than the measurement,
// which is why the optimizer needs it and needs neither Runs nor Measure.
/// **THE WHOLE FIGHT — every axis, including the ones this weapon hides.**
///
/// The blocks above show what APPLIES to the weapon in front of you, which
/// leaves a field this weapon FORCES and one it merely does not use invisible;
/// both still travel. SO IT IS A READING SURFACE FIRST: a forced row says what
/// it will run as and WHY — `sim.headshot_pct` can be 100 on a Verglas while
/// the run is computed at 0. EDITABLE WHERE A CONTROL CAN MEAN IT: a flag and a
/// number here, while a formation or a buff map is edited in its own panel.
/// **EVERY RULE THIS FIGHT MAKES, FOR EVERY CLASS** — the one surface where a
/// scenario is edited as the whole document it is. DERIVED FROM THE ENGINE'S
/// OWN LEGALITY TABLE: exactly the (class, axis) pairs
/// `META.class_rules.overridable` lists. A CLASS WITH NOTHING TO ARGUE ABOUT IS
/// STILL LISTED, saying so — dropping it would read as "we did not check".
function houseRulesRow(w) {
  const classes = ((META && META.class_rules) || {}).classes || [];
  const here = w && w.weapon_class;
  const NAMES = {
    primary: tr("Primary"), secondary: tr("Secondary"),
    archgun: tr("Arch-Gun"), sentinel: tr("Companion"),
  };
  const cols = classes.map((cls) => {
    const pairs = overridablePairs().filter((p) => p[0] === cls);
    const body = pairs.length
      ? pairs.map(([, id]) => {
          const rule = classRuleOf(cls, id);
          const f = settledAxis((META.weapons || []).find((x) => x.weapon_class === cls), id);
          const dflt = classRuleDefault(cls, id);
          const on = rule === undefined ? !!dflt : !!rule;
          return `<label class="check" title="${escHtml(f ? tr(f.why) : "")}">`
            + `<input type="checkbox" data-cr="${escHtml(cls)}|${escHtml(id)}" data-crdef="${!!dflt}"`
            + `${on ? " checked" : ""}> <code>${escHtml(id)}</code>`
            + (rule === undefined
                ? ` <span class="wf-was">${escHtml(tr("default"))}</span>`
                : ` <b>${escHtml(tr("ruled by this fight"))}</b>`)
            + `</label>`;
        }).join("")
      : `<i class="wf-absent">${escHtml(tr("nothing to decide"))}</i>`;
    return `<div class="hr-col${cls === here ? " here" : ""}">`
      + `<div class="hr-h">${escHtml(NAMES[cls] || cls)}`
      + (cls === here ? ` <span class="wf-was">${escHtml(tr("this weapon"))}</span>` : "")
      + `</div>${body}</div>`;
  }).join("");
  return `<div class="wf-row wf-hr"><code>class_rules</code>`
    + `<div class="hr-grid">${cols}</div>`
    + `<span class="wf-why">${escHtml(tr(
        "rules this fight makes for a whole weapon class, so any weapon can be measured against this one document. Only what the sim SIMPLIFIES can be ruled on — the game's own limits are not offered"))}</span>`
    + `</div>`;
}

function renderWholeFight() {
  const host = $("sim-whole-fight-body");
  if (!host) return;
  const axes = (META && META.scenario_axes) || [];
  const w = weaponInfo($("weapon").value) || {};
  const GROUPS = [["target", tr("The target")], ["engagement", tr("The engagement")],
                  ["wielder", tr("The wielder")], ["squad", tr("What the Warframe brings")]];
  const shown = (v) => {
    if (v === undefined) return `<i class="wf-absent">${escHtml(tr("not set — the default"))}</i>`;
    if (v === null) return "null";
    if (typeof v === "object") {
      const n = Array.isArray(v) ? v.length : Object.keys(v).length;
      return `<i>${escHtml(n ? tr("{n} entries").replace("{n}", n) : tr("empty"))}</i>`;
    }
    return escHtml(String(v));
  };
  host.innerHTML = GROUPS.map(([g, label]) => {
    const rows = axes.filter((a) => a.group === g).map((a) => {
      // THE HOUSE RULES ARE THE "GLOBAL EDIT", and they are
      // the reason this panel is more than a reading surface: the ordinary
      // blocks show the fight AS IT APPLIES TO THE WEAPON IN FRONT OF YOU,
      // which is the right default and cannot say what this same document does
      // to an Arch-Gun. This row is where a reader edits the classes they are
      // not currently pointed at.
      if (a.id === "class_rules") return houseRulesRow(w);
      const f = settledAxis(w, a.id);
      const live = sim[a.id];
      // THE FORCED ROW SHOWS BOTH NUMBERS. What the document says and what the
      // run will use are different facts, and the gap between them is the one
      // thing this panel exists to make visible.
      // A CLASS RULE BEATS THE CAPABILITY where the capability's absence was
      // ours to begin with, so the row must show the RULED value — otherwise
      // the one panel whose job is "what will actually run" would be the last
      // place still reporting the default.
      const cls = w && w.weapon_class;
      const rule = f && f.overridable && cls ? classRuleOf(cls, a.id) : undefined;
      const runs = rule === undefined ? (f ? f.value : undefined) : rule;
      const val = f
        ? `<b>${escHtml(String(runs))}</b> <span class="wf-was">${
            rule === undefined
              ? escHtml(tr("document says")) + " " + shown(live)
              : escHtml(tr("ruled by this fight for the {c} class").replace("{c}", cls))
          }</span>`
        : shown(live);
      const editable = !f && (a.kind.t === "flag" || a.kind.t === "number");
      const ctl = editable
        ? (a.kind.t === "flag"
            ? `<input type="checkbox" data-k="${a.id}"${live ? " checked" : ""}>`
            : `<input type="number" data-k="${a.id}" min="${a.kind.min}" max="${a.kind.max}" value="${
                live === undefined || live === null ? "" : live}">`)
        : "";
      return `<div class="wf-row${f ? " forced" : ""}">`
        + `<code>${escHtml(a.id)}</code>`
        + `<span class="wf-val">${val}</span>`
        + `<span class="wf-ctl">${ctl}</span>`
        + (f ? `<span class="wf-why">${escHtml(tr(f.why))}</span>` : "")
        + `</div>`;
    }).join("");
    return `<div class="wf-group"><div class="wf-g-h">${escHtml(label)}</div>${rows}</div>`;
  }).join("");
  // The same generic binding every other scenario field uses, so a change here
  // is a change to the fight and nothing else has to know about this panel.
  host.querySelectorAll("[data-cr]").forEach((el) => {
    el.addEventListener("change", () => { applyClassRule(el); renderSim(); });
  });
  host.querySelectorAll("[data-k]").forEach((el) => {
    el.addEventListener("change", () => {
      const k = el.dataset.k;
      // See the other binding below for why an empty number DELETES the key.
      writeScenarioFields({ [k]: el.type === "number" && el.value === "" ? null
        : el.type === "checkbox" ? el.checked : Number(el.value) });
      markScenarioDirty();
      renderSim();
    });
  });
}

function renderScenarioFields(ids, opts = {}) {
  const w = weaponInfo($("weapon").value);
  const enemies = allEnemies();
  const en = enemies.find((e) => e.id === sim.enemy) || enemies[0];
  if (en) sim.enemy = en.id;

  // ---- 1. THE FIGHT: who, where, how strong, how long ------------------
  // Target and arena in ONE block, not two: an enemy is
  // not a name, it is a name plus everything about the encounter, and the
  // enemies still to come bring their own assortment of those — a level, an
  // arena, an Eximus flag, a faction override. They all answer "what am I
  // shooting at, under what conditions", so they belong in one place that can
  // grow rather than in a grid that has to be re-cut every time.
  if (ids.target) {
    // The wiki link sits OUTSIDE the picker button — an <a> inside a <button>
    // is not valid HTML, and the two are different actions anyway: one
    // changes the fight, the other reads about the unit. Built from the
    // ENGLISH name like every other wiki link, and absent for a synthetic
    // target, which has no page to land on.
    const wiki = en && !en.synthetic
      ? `<a class="en-wiki" href="${wikiUrl(en.name_en || en.name)}" target="_blank" rel="noopener"
            title="${escHtml(tr("open the wiki page"))}">${escHtml(tr("wiki"))} ↗</a>`
      : "";
    // WHAT THIS CARD REACHES, said out loud once there is more than one body.
    //
    // It is the AIMED body's unit and it is the BRUSH for the next placement —
    // two jobs that were one thing while a fight had one target, and are still
    // one control because the aimed body IS the thing you place a copy of. What
    // it is NOT is a control over the floor: a body keeps the unit it was
    // placed with (`placeAt`), which is the whole point of a formation built
    // unit by unit and is invisible until somebody switches and watches nothing
    // change. So the count is on screen rather than left to be discovered.
    const brushNote = (s) => {
      const n = (s.formation || []).length;
      if (!n || opts.readonly) return "";
      const kept = (s.formation || []).filter((f) => f.enemy && f.enemy !== s.enemy).length;
      return `<p class="arc-note">${escHtml(kept
        ? trF("this is the aimed enemy, and what the place tool puts down. {n} already on the floor keep the unit they were placed with.", { n: kept })
        : tr("this is the aimed enemy, and what the place tool puts down. Enemies already on the floor keep the unit they were placed with."))}</p>`;
    };
    const card =
      `<div class="en-row">
         <button class="en-card" id="${ids.target}-pick" title="${escHtml(tr("choose the target"))}">
           ${enemyImg(en, "en-img")}
           <span class="en-txt">
             <span class="en-name">${en
               ? `<i class="ai-sw" style="background:${unitColor(en.id)}"></i>` : ""
             }${escHtml(en ? en.name : tr("Enemy"))}</span>
             <span class="en-meta">${escHtml(enemyMeta(en))}</span>
             ${enemyVuln(en)}${enemyStatusImmune(en)}${enemyEffectsNulled(en)}
             ${enemyCaveat(en)}
           </span>
         </button>
         ${wiki}
       </div>`;
    // EVERYTHING ABOUT THE TARGET LIVES IN THE CANVAS, and
    // only the DURATION stays outside: how long the fight runs is a property of
    // the ENGAGEMENT rather than of anybody standing on the floor, and it is the
    // one field in this block that is not about the target.
    //
    // The card and the fields are rendered INTO the arena host and then MOVED —
    // not re-serialised — into the scene by `mountArenaCanvas`. Moving the nodes
    // is what keeps `pick.onclick` bound, every `data-k` listener live, and
    // every selector pointing at the element it always did.
    $(ids.target).innerHTML =
      `<div class="arena" id="${ids.target}-arena">
        <div class="arc-side">${card}
          <div class="arc-fields">
            <label>${escHtml(tr("Level"))} <input type="number" data-k="level" min="1" max="9999" value="${sim.level}"></label>
            <label class="check"><input type="checkbox" data-k="steel_path" ${sim.steel_path ? "checked" : ""}> Steel Path</label>
            ${eximusField(en)}
            ${spectralField(en)}
            ${guardianField()}
            ${protectorField()}
            ${squadField(en)}
          </div>
          ${brushNote(sim)}
        </div>
      </div>` +
      `<div class="field-grid">
        ${deployField(w, sim)}
        <label>${escHtml(tr("Duration (s)"))} <input type="number" data-k="duration" min="1" max="3600" value="${sim.duration}"></label>
      </div>`;
    const pick = $(`${ids.target}-pick`);
    if (pick && !opts.readonly) pick.onclick = (e) => { e.stopPropagation(); openEnemyPicker(pick); };
    if (pick && opts.readonly) pick.disabled = true;
    // THE ARENA, drawn last so it can measure the width it was given.
    mountArena($(`${ids.target}-arena`), sim, en, opts);
  }

  // ---- 2. THE WIELDER: whoever is holding the weapon, and what they do ---
  // Block 1 is the other actor; this is your side of the fight. It grew from
  // "technique" when the engine got a second actor: the
  // form you fire, how you fire it, the states a mod card gates on, and the
  // Warframe behind it are all one answer to "who is shooting". A neutral
  // frame wears nothing and is doing nothing, which is why the states are off
  // and the stats are 0 — and why the mods and arcanes that read them
  // contribute nothing until you say otherwise, on the panel, in the sim and
  // in the search alike.
  //
  // NOT "the Tenno". A ROBOTIC weapon is not held by one:
  // the wiki is explicit that MOAs "share Robotic weapons with Sentinels and
  // equip their weapons", so the wielder of Verglas Prime is a Sentinel or a
  // MOA. Naming the section after the commonest case made the model say
  // something false about every companion weapon.
  // NO FORM CONTROL HERE. How the weapon is played is part of the BUILD and
  // lives in the builder — a fight that decided it could only ever measure
  // whichever way the ruler happened to pin, which is what kept "the Torid
  // that never transmutes" unaskable.

  if (ids.technique) {
    $(ids.technique).innerHTML = `
      ${aimField(w, sim)}
      ${headshotField(w, sim)}
      <label class="check" title="${escHtml(tr("the wielder's state: mods that only pay while Invisible (Spectral Serration) grant nothing when this is off"))}"><input type="checkbox" data-k="invisible"${sim.invisible ? " checked" : ""}> ${escHtml(tr("Invisible"))}</label>
      <label class="check" title="${escHtml(tr("the wielder's state: what a card means by \"while Airborne\""))}"><input type="checkbox" data-k="airborne"${sim.airborne ? " checked" : ""}> ${escHtml(tr("Airborne"))}</label>
      <label class="check" title="${escHtml(tr("the wielder's state: what a card means by \"With Overshields\". Nothing here takes them away, so it is a declaration"))}"><input type="checkbox" data-k="overshields"${sim.overshields ? " checked" : ""}> ${escHtml(tr("Overshields"))}</label>
      <label class="check" title="${escHtml(tr("the wielder's state: what a card means by \"With Channeled Ability active\". The ability must DRAIN ENERGY over time — Desecrate, Haven and an empty Gloom do not count"))}"><input type="checkbox" data-k="channeling"${sim.channeling ? " checked" : ""}> ${escHtml(tr("Channeled ability"))}</label>
      <label class="check" title="${escHtml(tr("the wielder's state: what a card means by \"With Melee Weapon Equipped\". It means the weapon is DRAWN — a quick-melee swing out of a gun does not satisfy it. Every ruler runs it drawn"))}"><input type="checkbox" data-k="melee_equipped"${sim.melee_equipped !== false ? " checked" : ""}> ${escHtml(tr("Melee drawn"))}</label>
      <label class="check" title="${escHtml(tr("the LOADOUT, not what the wielder is doing: off means a full one, which is what the board is scored under. On, this weapon is the only one carried — the Vasto's Lone Gun pays its \"With No Primary Equipped\" half, and every \"On Equip from Primary\" or \"while Holstered\" clause becomes impossible rather than merely unmodelled"))}"><input type="checkbox" data-k="solo_weapon"${sim.solo_weapon ? " checked" : ""}> ${escHtml(tr("Only this weapon"))}</label>
      ${wfFloorLine()}
      ${wfOverride("wf_health", tr("Health"), "health", 1, 100000, 1,
        tr("your Warframe's health, buffs included — the Basmu's Dreadful Killshot pays +20% damage and status chance for every 75 of it"))}
      ${wfOverride("wf_shield", tr("Shield"), "shield", 0, 100000, 1,
        tr("your Warframe's shields, buffs included"))}
      ${wfOverride("wf_armor", tr("Armor"), "armor", 0, 100000, 1,
        tr("your Warframe's armor, buffs included — Primary Bulwark pays +1% damage per point past 1,000"))}
      ${wfOverride("wf_energy", tr("Max energy"), "energy", 0, 100000, 1,
        tr("your Warframe's MAX energy — Primary Overcharge turns 35% of it into multishot"))}
      ${wfOverride("wf_sprint", tr("Sprint"), "sprint", 0, 3, 0.05,
        tr("your Warframe's sprint speed — several Incarnon perks pay only at 1.2 or higher"))}`;
  }

  // ---- 2b. WHAT THE WARFRAME BRINGS ------------------------------------
  // Its own renderer because it is a LIST rather than a field: two of them, one
  // capped at five, each row a control with its own remove. It sits in the
  // wielder block because that is whose it is.
  if (ids.squad) renderSquad(ids.squad, w, sim, opts);

  // ---- 3. LIMITS: what the simulation is allowed to assume -------------
  // Infinite ammo is NOT a technique — nobody plays it. It
  // is a statement about what this run does not model, and the things that
  // will join it are the same kind of statement.
  if (ids.limits) {
    $(ids.limits).innerHTML = ammoField(w, sim) + pickupFields(sim) + buffTriggersField();
  }

  // ---- 3b. THE FIGHT'S OWN STAT BONUSES --------------------------------
  //
  // Everything this weapon is handed by something that is not its build. They
  // land in the same ADDITIVE buckets the mods feed — "效果等于又塞mod" — so a
  // player who knows what their squad, their frame or an ability is worth types
  // the number and the whole app treats it as one more card: the panel's own
  // arithmetic, the sim, the optimizer's scoring, and every lock.
  //
  // BLANK IS ZERO, not empty: a fight hands this weapon nothing unless someone
  // says otherwise, which is what every ruler and every stored scenario means.
  if (ids.extra) {
    const ex = sim.extra_stats || {};
    $(ids.extra).innerHTML = EXTRA_STAT_KEYS.map(([k, label, hint]) =>
      `<label title="${escHtml(tr(hint || "a percentage, into the same bucket a mod of this stat feeds — permanent, no trigger and no clock"))}">${escHtml(tr(label))} <span class="unit">%</span> <input type="number" data-xk="${k}" step="1" value="${ex[k] ? r3(ex[k] * 100) : ""}" placeholder="0"></label>`
    ).join("");
  }

  // ---- 4. MEASUREMENT: nothing the player does in-game -----------------
  if (ids.run) {
    $(ids.run).innerHTML = `
      <label title="${escHtml(tr("what the run is judged by — the headline number and the picker's gain scan both follow it"))}">${escHtml(tr("Measure"))} ${
        ddButton("dd-metric", {
          value: sim.metric,
          dataK: "metric",
          // FROM THE ENGINE'S TABLE, so a metric it declares is offered here
          // without this list being edited — two declarations of one set is
          // one that goes stale.
          items: (META.metrics || []).map((m) => ({
            value: m.id, label: tr(m.label), hint: tr(m.hint),
          })),
        })}</label>`;
  }

  // …AND THE WHOLE-FIGHT PANEL, drawn from the same state as the blocks
  // above so the two can never disagree about what the fight holds.
  renderWholeFight();

  const boxes = [ids.target, ids.technique, ids.limits, ids.extra, ids.run]
    .filter(Boolean)
    .map($);
  // READ-ONLY hosts get the same fields, in the same order, showing the same
  // values — and no way to change them. A preset is edited in exactly ONE
  // place: two editors over one document is how a document
  // gets edited twice and saved once. The optimizer therefore SHOWS the fight
  // and links to the module that owns it.
  if (opts.readonly) {
    boxes.forEach((box) => box.querySelectorAll("[data-k],[data-xk],[data-bev],[data-bevg]").forEach((el) => {
      el.disabled = true;
      el.title = tr("edit this in the Simulator");
    }));
    return;
  }
  // The pools at THIS fight's level, fetched once per (level, Steel Path) and
  // painted in when they arrive. Fired from here because this is the one
  // function that draws the target card, on either tab.
  loadTargetStats().then((changed) => { if (changed) paintTargetMeta(); });
  boxes.forEach((box) =>
    box.querySelectorAll("[data-cr]").forEach((el) =>
      el.addEventListener("change", () => { applyClassRule(el); renderSim(); })));
  // A LIST, so it cannot ride the generic `data-k` binding, which stores a
  // scalar. It is still a scenario field in every other way.
  boxes.forEach((box) => {
    box.querySelectorAll("[data-bev]").forEach((el) =>
      el.addEventListener("change", () => toggleBuffTrigger(el.dataset.bev, el.checked)));
    box.querySelectorAll("[data-bevg]").forEach((el) => {
      // HALF-TICKED, which HTML has no attribute for: the header shows it and
      // a click from there switches the whole group ON, because that is what a
      // reader reaching for a partly-ticked group means.
      if (el.dataset.some) el.indeterminate = true;
      el.addEventListener("change", () => toggleBuffTriggerGroup(el.dataset.bevg, el.checked));
    });
  });
  boxes.forEach((box) =>
    box.querySelectorAll("[data-k]").forEach((el) => {
      el.addEventListener("change", () => {
        const k = el.dataset.k;
        // A WARFRAME OVERRIDE IS TWO CONTROLS FOR ONE FACT, so it cannot use
        // the generic binding: the tick and the number share a `data-k`, and
        // reading `el.checked` into it would store `true` where a number goes.
        //
        // THE TICK DECIDES WHETHER THE KEY EXISTS AT ALL. Deleting it is the
        // whole mechanism — an absent key is what makes the server fall back to
        // the floor, and a `0` would be an override that happens to say zero.
        if (el.dataset.wfov) {
          if (el.checked) sim[k] = tennoFloor()[el.dataset.wfov];
          else delete sim[k];
          renderSim();
          markScenarioDirty();
          return;
        }
        if (applyClassRule(el)) { renderSim(); return; }
        // A NUMBER THAT IS NOT A NUMBER INPUT SAYS SO. A `<select>` reaches the
        // `else` below as a STRING, and a scenario field the server reads with
        // `get_u32` would silently fall back to its default — the control would
        // move and the fight would not.
        // AN EMPTY NUMBER IS AN ABSENT KEY, not a zero. The pickup reach is
        // the field this exists for: blank means "any distance", which the
        // server reads off the missing key, where a 0 would mean a radius that
        // collects nothing.
        writeScenarioFields({ [k]: el.dataset.wfovnum || el.dataset.num ? Number(el.value)
          : el.type === "checkbox" ? el.checked
          : el.type === "number" && el.value === "" ? null
          : el.type === "number" ? Number(el.value)
          : el.value });
        // No `enemy` case here: the target is the picker's, not a field's, and
        // it repaints the arena through renderSim() like everything else.
        // ONLY the scenario. A build carries no copy of the fight, so a sim
        // knob does not dirty the build preset: this is the scenario's edit
        // and nobody else's.
        markScenarioDirty();
        // Whichever tab drew the field, both are looking at this one state.
        if (opts.after) opts.after();
      });
    }));
  // …AND THE FIGHT'S OWN STAT BONUSES, whose own map they write into. A
  // separate attribute rather than a `data-k` per stat because they are ONE
  // scenario field: nine numbers in one object, so a blank one is absent rather
  // than a zero nobody typed, and the share link carries only what was set.
  boxes.forEach((box) =>
    box.querySelectorAll("[data-xk]").forEach((el) => {
      el.addEventListener("change", () => {
        // TYPED IN PERCENT, stored as the fraction every bucket in the engine
        // holds — the same units a mod's `rankMax` is in.
        setExtraStat(el.dataset.xk, Number(el.value) / 100);
        if (opts.after) opts.after();
      });
    }));
}

/// ONE OF THE FIGHT'S OWN STAT BONUSES, as a fraction; zero or not a number
/// clears it. It changes what the BUILD is worth, so the panel is asked again —
/// the same reason a Tenno field is.
function setExtraStat(k, v) {
  const next = { ...(sim.extra_stats || {}) };
  if (!Number.isFinite(v) || v === 0) delete next[k]; else next[k] = v;
  sim.extra_stats = next;
  refreshPanel();
  markScenarioDirty();
}

// WHAT THE WARFRAME BRINGS: the squad's AURA and the frame's ARCHON SHARDS.
//
// Neither is the weapon's and neither is the build's, which is why they are
// drawn inside the wielder block and travel with the FIGHT — the same place
// and the same reason as the ability buffs. An aura can no more reach the
// board than Roar can.
//
// THEY ARE OFFERED, NEVER TYPED. The Extra stats grid above already accepts any
// number a player wants into any bucket; what it cannot do is say WHERE the
// number came from. A named shard has a source that can be checked against the
// wiki and updated when DE moves it; a typed +45% has nothing behind it.
const AURAS = () => (META && META.auras) || [];
const SHARDS = () => (META && META.shards) || [];
// FIVE SOCKETS. The frame has five and no amount of anything adds a sixth.
const SHARD_SLOTS = 5;
// A SOCKET IS ONE CHOICE, not two. `shard/effect` in one dropdown, grouped by
// colour, because "which colour" is never the question a player has — they know
// the effect they want and the colour follows from it.
const shardKey = (x) => `${x.shard}/${x.effect}`;

// WHY AN AURA IS GREY, said on the row that OFFERS it rather than after it is
// picked. The amp family is the whole reason: three of them are gated on a mod
// POOL and Dead Eye on a CLASS, so "this is a rifle, why is Dead Eye dead" is a
// question the list has to answer where it is asked.
const paidHint = (a, paid) => (paid.has(a.id) ? null : tr("pays this weapon nothing"));

// THE NUMBER IS PRINTED, NOT TRANSLATED. A card reads "+25% (+37.5%)" and the
// bracket is the TAUFORGED value, which is how both the wiki and the game
// present it — so a value DE moves costs zero translation, and the pair is
// visible before the checkbox is ticked.
const shardValue = (o) => {
  const f = (v) => (o.unit === "pct" ? `${r3(v * 100)}%` : String(r3(v)));
  return `+${f(o.value)} (+${f(o.tauforged)})`;
};

function shardItems() {
  const out = [];
  SHARDS().forEach((d) => d.options.forEach((o) => out.push({
    value: `${d.id}/${o.id}`,
    label: `${shardValue(o)} ${LN("shards", `${d.id}/${o.id}`, o.text)}`,
    group: LN("shards", d.id, d.name),
    // WHAT IT PAYS HERE, on the row, because twenty of the twenty-seven pay
    // nothing in this arena and a socket that quietly does nothing is worse
    // than one that says so.
    hint: o.modelled ? null : `${tr("not modelled")} — ${tr(o.why_not || "")}`,
  })));
  return out;
}

const shardOption = (key) => {
  const [sid, eid] = String(key || "").split("/");
  const d = SHARDS().find((x) => x.id === sid);
  return d && d.options.find((o) => o.id === eid);
};

function renderSquad(host, w, sim, opts) {
  const box = $(host);
  if (!box || !META) return;
  // THE SQUAD'S AURAS AND NOTHING ELSE: the archon shards are the wielder's,
  // and come with the Warframe build the weapon's wielder links.
  const auras = sim.auras || [];
  // WHICH AURAS PAY THIS WEAPON is the ENGINE's answer, served per weapon — the
  // amp family does not share one gate, so nothing here re-derives it.
  const paid = new Set((w && w.auras) || []);
  const taken = new Set(auras.map((a) => a.id));

  const auraRow = (a, i) => {
    const d = AURAS().find((x) => x.id === a.id);
    if (!d) return "";
    const pays = paid.has(d.id);
    return `<div class="sq-row${pays ? "" : " sq-dead"}">
      ${ddButton(`sq-aura-${i}`, {
        value: d.id, data: { sqaura: i }, search: true,
        items: AURAS().filter((x) => x.id === d.id || !taken.has(x.id))
          .map((x) => ({ value: x.id, label: LN("auras", x.id, x.name),
            hint: paidHint(x, paid) })),
      })}
      ${d.squad_stacking
        ? `<label class="sq-n" title="${escHtml(tr("how many of the squad are running it — Corrosive Projection reaches 72% four-handed"))}">&times;<input type="number" min="1" max="4" step="1" value="${a.count || 1}" data-sqcount="${i}"></label>`
        : `<span class="sq-n sq-mute" title="${escHtml(tr("this aura does not stack with itself across the squad"))}">&times;1</span>`}
      <button type="button" class="ghost-btn small" data-sqdrop="${i}" title="${escHtml(tr("remove"))}">&#10005;</button>
      ${pays ? "" : `<span class="sq-why">${escHtml(tr("pays this weapon nothing"))}</span>`}
    </div>`;
  };

  const free = AURAS().filter((x) => !taken.has(x.id));
  box.innerHTML = `
    <div class="sq-h">${escHtml(tr("Squad aura"))} <span class="sim-hint">${escHtml(tr("the squad's mod — it is the fight's, never the build's, so it cannot reach the board"))}</span></div>
    ${auras.map(auraRow).join("") || `<div class="sq-none">${escHtml(tr("none — nobody in the squad is running an aura"))}</div>`}
    ${free.length ? `<div class="sq-add">${ddButton("sq-aura-add", {
      value: "", placeholder: `+ ${tr("add an aura")}`, search: true, data: { sqadd: "1" },
      items: free.map((x) => ({ value: x.id, label: LN("auras", x.id, x.name),
        hint: paidHint(x, paid) })),
    })}</div>` : ""}`;

  if (opts && opts.readonly) {
    box.querySelectorAll("button,input").forEach((el) => {
      el.disabled = true;
      el.title = tr("edit this in the Simulator");
    });
    return;
  }
  // A CHANGE HERE IS A CHANGE TO THE FIGHT, and to what the BUILD is worth — an
  // aura lands in the base-damage bucket beside Serration and a shard in the
  // crit one — so the panel is asked again exactly as a Tenno field does.
  const touched = () => {
    renderSquad(host, w, sim, opts);
    refreshPanel();
    markScenarioDirty();
    if (opts && opts.after) opts.after();
  };
  const num = (el, k) => {
    const i = Number(el.dataset[k]);
    return Number.isFinite(i) ? i : -1;
  };
  box.querySelectorAll("[data-sqaura]").forEach((el) => el.addEventListener("change", () => {
    const i = num(el, "sqaura");
    if (i >= 0 && sim.auras[i]) { sim.auras[i] = { ...sim.auras[i], id: el.value }; touched(); }
  }));
  box.querySelectorAll("[data-sqcount]").forEach((el) => el.addEventListener("change", () => {
    const i = num(el, "sqcount");
    if (i >= 0 && sim.auras[i]) { setSquadAura(sim.auras[i].id, Math.max(1, Number(el.value) || 1)); touched(); }
  }));
  box.querySelectorAll("[data-sqdrop]").forEach((el) => el.addEventListener("click", () => {
    const a = auras[num(el, "sqdrop")];
    if (a) { setSquadAura(a.id, 0); touched(); }
  }));
  box.querySelectorAll("[data-sqadd]").forEach((el) => el.addEventListener("change", () => {
    if (!el.value) return;
    setSquadAura(el.value, 1);
    touched();
  }));
}

/// HOW MANY OF THE SQUAD RUN AN AURA — 1 to 4, because a squad is four people;
/// 0 removes it. An aura that does not stack with itself counts once whatever
/// is asked. Its place in the list is kept, so the rows do not jump.
function setSquadAura(id, count) {
  const list = sim.auras || [];
  if (!count) { sim.auras = list.filter((a) => a.id !== id); return; }
  const d = AURAS().find((x) => x.id === id);
  const n = d && d.squad_stacking ? Math.min(4, Math.max(1, Math.round(count))) : 1;
  sim.auras = list.some((a) => a.id === id)
    ? list.map((a) => (a.id === id ? { ...a, count: n } : a))
    : list.concat([{ id, count: n }]);
}

// The unit's portrait, or NOTHING — never an empty box holding its place.
// `imgTag` renders a placeholder span when the src is null, which is right
// for a mod grid (the slots must stay aligned) and wrong here: an enemy with
// no art yet should read as a name, not as a name with a hole beside it.
const enemyImg = (en, cls) => {
  const src = IMG(en && en.image);
  return src ? `<img class="${cls}" src="${src}" alt="" onerror="this.style.display='none'"/>` : "";
};

// What a run against this unit does not account for (`unmodeled` in its data
// file). An Acolyte carries damage attenuation whose constants DE has never
// published, so the number this app reports against one is too HIGH — that is
// a thing to say on the card, not a thing to leave the reader to discover.
const enemyCaveat = (en) => {
  const gaps = (en && en.unmodeled) || [];
  return gaps.length
    ? `<span class="en-gap" title="${escHtml(tr("the sim does not model this yet, so its number against this target is optimistic"))}">⚠ ${
        escHtml(tr("not modeled") + ": " + gaps.map((g) => tr(g)).join(", "))}</span>`
    : "";
};

// What this unit takes MORE and LESS of — the post-U36 faction column
// (`FactionDamageOverride ?? Faction`), which is half of what picks a build's
// elements. Its own line, not another entry in the meta run-on: a reader
// scanning for "what do I bring" should find it in one place, and up and down
// have to look different at a glance. A unit with a neutral column shows
// nothing at all — no line is the honest rendering of "takes damage as
// written", and an empty "Vulnerabilities:" label would not be.
const enemyVuln = (en) => {
  // What to BRING before what to avoid: the server sends them in damage-type
  // order, which interleaves the two answers.
  const mods = [...((en && en.type_modifiers) || [])].sort((a, b) => b.mult - a.mult);
  return mods.length
    ? `<span class="en-vuln" title="${escHtml(tr("this unit's faction takes more or less of these damage types, whatever its armor and shields do"))}">${
        mods.map((m) => `<span class="${m.mult > 1 ? "up" : "dn"}">${escHtml(DT(m.type))} ×${escHtml(String(m.mult))}</span>`).join("")}</span>`
    : "";
};

// WHAT CANNOT BE PROC'D ON IT, which is a different line because it is a
// different mechanic. A vulnerability says what a hit DEALS; a status immunity
// says what it PROCS, and it moves the whole distribution rather than the one
// entry — the immune type leaves the denominator and the others take over its
// share of the roll (wiki `Status_Effect` §Status Immunity Interactions). A
// reader who saw only "Heat ×0" would conclude the Heat procs stopped too, and
// they did not.
const enemyStatusImmune = (en) => {
  const im = (en && en.status_immunities) || [];
  return im.length
    ? `<span class="en-vuln" title="${escHtml(tr("these procs cannot land on this unit — the other damage types take over their share of the status roll"))}">${
        im.map((k) => `<span class="dn">${escHtml(DT(k))} ⃠</span>`).join("")}</span>`
    : "";
};

/// …AND THE THIRD LINE, which is neither of the two above. The proc LANDS, and
/// what it does is nothing.
///
/// It has to be its own line because the arithmetic differs: an immune type
/// leaves the status roll and makes every other type MORE likely, while this
/// one keeps its share of the roll and still counts as a type for Condition
/// Overload. Showing them together would tell a reader the opposite of both.
///
/// `cannot_be_frozen` rides here for the same reason and reads the other way —
/// Cold is worth MORE on such a unit, because the ladder never spends itself.
const enemyEffectsNulled = (en) => {
  const out = [];
  const nn = (en && en.nullified_status_effects) || [];
  if (nn.length) {
    out.push(`<span class="en-vuln" title="${escHtml(
      tr("these procs still land and still count for Condition Overload — this unit simply ignores what they do"))}">${
      nn.map((k) => `<span class="dim">${escHtml(DT(k))} ${escHtml(tr("no effect"))}</span>`).join("")}</span>`);
  }
  if (en && en.cannot_be_frozen) {
    out.push(`<span class="en-vuln" title="${escHtml(
      tr("Cold never converts on this unit, so the stacks climb to their cap and STAY — the bonus is up all fight instead of being spent on a 3-second Frozen window"))}">${
      `<span class="up">${escHtml(tr("never Frozen — Cold stacks hold"))}</span>`}</span>`);
  }
  return out.join("");
};

// THE POOLS AT THE FIGHT'S LEVEL, from the engine.
//
// A unit's OWN base level — a Corrupted Heavy Gunner as "700 Health · 500
// Armor" — is a number nobody fights: the scenario runs at 9999 Steel Path,
// where the same unit is millions of health and the armour figure has stopped
// meaning what
// the raw number suggests. Choosing a target on those is choosing on the wrong
// axis.
//
// Fetched, not computed here: the level curves belong to the engine and a
// second implementation in JavaScript is a second answer waiting to drift.
// Keyed on the two inputs, so it costs one call per level change and nothing
// per repaint.
let targetStats = { key: null, by: {} };
async function loadTargetStats() {
  const key = `${sim.level}|${sim.steel_path ? 1 : 0}`;
  if (targetStats.key === key) return false;
  const r = await api("/api/targets", { level: sim.level, steel_path: sim.steel_path });
  if (!r || !r.targets) return false;
  const by = {};
  r.targets.forEach((t) => { by[t.id] = t; });
  // Written together with the key: a half-applied cache would serve one
  // level's numbers under another's label.
  targetStats = { key, by };
  return true;
}

// Repaint the target cards' meta line IN PLACE when the numbers land.
//
// Not a re-render: the renderer is what asked for them, so calling it back
// would recurse. One line of text is the whole difference the answer makes,
// and both tabs draw the same card from the same state, so both are patched
// by one selector.
function paintTargetMeta() {
  const en = allEnemies().find((e) => e.id === sim.enemy);
  if (!en) return;
  document
    .querySelectorAll("#sim-target .en-meta, #opt-target .en-meta")
    .forEach((el) => { el.textContent = enemyMeta(en); });
}

// What the target card says under the name. The enemy's own facts, plus what
// it is MADE OF at the level this fight runs at.
function enemyMeta(en) {
  if (!en) return "";
  const bits = [];
  if (en.faction && en.faction !== "unknown") bits.push(tr(cap1(en.faction)));
  // The pools a build has to get through. AT THE FIGHT'S LEVEL when the engine
  // has told us (`loadTargetStats`), falling back to the unit's base numbers
  // before that answer arrives — never a mix of the two, because a row reading
  // "4.9M Health · 500 Armor" would be a lie about both.
  const at = targetStats.by[en.id];
  const src = at && !at.error ? at : en;
  const pools = [
    src.health ? `${Math.round(src.health).toLocaleString()} ${tr("Health")}` : null,
    src.shield ? `${Math.round(src.shield).toLocaleString()} ${tr("Shield")}` : null,
    src.armor ? `${Math.round(src.armor).toLocaleString()} ${tr("Armor")}` : null,
    src.overguard ? `${Math.round(src.overguard).toLocaleString()} ${tr("Overguard")}` : null,
  ].filter(Boolean);
  if (pools.length) {
    // SAY WHICH LEVEL, always. The same list of numbers means something
    // completely different at 9999 than at the unit's base level, and the
    // fallback above can serve either — so the label is not decoration, it is
    // what makes the row readable.
    // "Lv" stays untranslated, the way every other level label in the app is.
    const lv = src === en
      ? `Lv ${en.base_level ?? 1}`
      : `Lv ${sim.level}${sim.steel_path ? " SP" : ""}${at.eximus ? ` ${tr("Eximus")}` : ""}`;
    bits.push(`${lv}: ${pools.join(" · ")}`);
    // What the armour is WORTH — at these levels the raw figure says little
    // and the reduction it buys says everything.
    if (src !== en && at.armor_dr > 0) {
      bits.push(`${Math.round(at.armor_dr * 1000) / 10}% ${tr("damage reduction")}`);
    }
  }
  const head = (en.parts || []).find((p) => p.is_head);
  if (head) bits.push(`${tr("Headshot")} ×${head.multiplier}`);
  // A UNIT'S OWN MULTIPLIER, and it is said as BOTH numbers because one of
  // them is the surprise: it rides the faction bracket, so a status carries
  // its square. A card printing only "×0.8" would have a reader off by a
  // fifth on every DoT.
  const fb = en.faction_bracket_multiplier;
  if (fb != null && fb !== 1) {
    bits.push(`×${fb} ${tr("all damage")} (×${Math.round(fb * fb * 1000) / 1000} ${tr("on status")})`);
  }
  // The bare word "Eximus" is now a claim about WHAT YOU ARE SHOOTING and is
  // made above, beside that variant's own pools — there is a switch for it and
  // it defaults on. This line stays only for the case the numbers have not
  // arrived yet, where nothing else would mention the variant at all.
  if (en.can_be_eximus && !targetStats.by[en.id]) bits.push(tr("has an Eximus variant"));
  return bits.join(" · ");
}

// THE TARGET PICKER — the same component as the mod and arcane pickers, on
// the same rule: search matches anything the reader can see. One enemy in the
// roster today, so this is the shape the roster grows into rather than a
// convenience over a two-item list.
function openEnemyPicker(anchor) {
  closePopovers();
  const pop = $("enemy-popover");
  place(pop, anchor);
  const search = $("enemy-search");
  search.value = "";
  search.oninput = () => renderEnemyMenu(search.value);
  renderEnemyMenu("");
  search.focus();
  // Then again with the real numbers. Drawn twice rather than awaited, so the
  // menu opens at once on a cold cache instead of after a round trip — and the
  // first pass is honest either way, because it labels the base level it is
  // actually showing.
  loadTargetStats().then(() => {
    if (pop.classList.contains("open") || pop.style.display !== "none") {
      renderEnemyMenu(search.value);
    }
  });
}

function renderEnemyMenu(query) {
  const menu = $("enemy-menu");
  const q = (query || "").trim().toLowerCase();
  const blob = (e) => [e.name, e.name_en, e.id, e.faction, e.scaling]
    .filter(Boolean).join(" ").toLowerCase();
  const hits = allEnemies().filter((e) => !q || blob(e).includes(q));
  menu.innerHTML = hits.length
    ? hits.map((e) => `<div class="opt ${e.id === sim.enemy ? "sel" : ""}" data-e="${escHtml(e.id)}">
         ${enemyImg(e, "en-thumb")}
         <div class="info"><div class="mn">${escHtml(e.name)}</div>
         <div class="me">${escHtml(enemyMeta(e))}</div>${enemyVuln(e)}${enemyStatusImmune(e)}${enemyEffectsNulled(e)}${enemyCaveat(e)}</div>
       </div>`).join("")
    : `<div class="sim-empty">${escHtml(tr("no enemy matches"))}</div>`;
  menu.querySelectorAll("[data-e]").forEach((el) => el.onclick = () => {
    closePopovers();
    setScenarioFields({ enemy: el.dataset.e });
    renderOptFight();
  });
}

// Which forms this weapon offers, and the reseed when it does not offer the
// one currently chosen.
//
// The FORMS are the weapon's own (registered in data/weapons, served by
// /api/meta) — not a hardcoded Incarnon triple. The two-form CYCLE is not a
// form but a MODE over them, so it is listed first and only when the weapon
// has something to transform into (`has_cycle`). A weapon with one form and
// no cycle has nothing to choose, so no selector is drawn.
//
// The cycle is offered — and defaulted to — whenever the WEAPON has one,
// installed perk or not. It stays honest because the sim
// falls back to the base form when the unlock is missing, and it stays
// STABLE, which is the point: re-seeding the choice every time tier 1 is
// touched would move the selection under someone who had already made it.

// THERE IS NO ACTOR STRIP: Tenno and enemy markers on a bar are meaningless
// beside a drawn fight. A dot, a lane and a portrait stand in for a fight the
// page cannot draw — and the page draws the fight, at
// the engine's own scale, with every body in it. Two pictures of one fight is
// one too many, and the strip was the one that could not be edited, could not
// be measured off, and showed one enemy however many were standing there.
//
// The enemy's identity did not go with it: it is on the brush card in the
// scene (which is where you pick it) and in the result panel's own copy.


