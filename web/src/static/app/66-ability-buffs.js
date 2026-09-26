// ---- WARFRAME ABILITY BUFFS (scenario section 3) ------------------------
//
// ABILITY STRENGTH IS THE WIELDER'S — the linked Warframe build's — unless the
// fight types one over it (`sim.ability_strength`, null = the wielder's).
//
// It is the SCENARIO's, not the build's: a thing done TO this weapon for a
// while. That is what puts it in section 3 beside the wielder, what carries it
// into the optimizer read-only, and what keeps it off the board — a board row
// is a statement about the weapon, and no ruler casts Roar.
const wfAbilities = () => (META && META.abilities) || [];
// DE'S OWN NAME for the ability, never a translation of ours (the house rule).
// The FRAME keeps its English name because DE's Chinese client does too.
const wfName = (a) => ((I18N && I18N.abilities) || {})[a.id] || a.name;
const wfPick = (id) => (sim.abilities || []).find((a) => a.id === id);
// The strength-scaled value, which is the number the card shows. Linear, and
// the engine agrees by construction: `data::abilities::at_strength` is the same
// multiply, and `check_wf_buffs.mjs` asserts the screen and the sim match.
// …AND THE ONES THE KNOB DOES NOT MOVE. Energized Munitions' ammo efficiency is
// a flat 75% — its wiki row carries no Ability Strength icon — so multiplying it
// here would print a number the game never gives, at every strength but 100%.
// The server states which per ability (`scales_with_strength`), so the page
// cannot disagree with the sim about it.
const wfValue = (a) =>
  a.value * (a.scales_with_strength === false ? 1 : simStrength());

/// THE WIELDER'S OWN ABILITY STRENGTH, as the server resolved it on the last
/// panel (`panelWielder`) — the same resolve the fight makes.
const wielderStrength = () => {
  const f = typeof tennoFloor === "function" ? tennoFloor() : null;
  return f && typeof f.ability_strength === "number" ? f.ability_strength : 1;
};
/// The strength the fight runs at: the typed override, else the wielder's.
const simStrength = () => (typeof sim.ability_strength === "number" ? sim.ability_strength : wielderStrength());

// WHICH PICKS ARE ACTUALLY RUNNING. Same family, only the strongest — the
// wiki's own rule ("Multiple Freeze Forces do not stack; the buff with the
// highest Ability Strength will take effect") and the owner's ask for Roar vs
// Roar (Helminth). Computed here TOO, rather than only in the engine, because
// a page that showed both as active would be lying about a number it printed.
function wfRunning() {
  const best = new Map();
  for (const p of sim.abilities || []) {
    const def = wfAbilities().find((a) => a.id === p.id);
    if (!def) continue;
    const cur = best.get(def.family);
    if (!cur || wfValue(def) > wfValue(cur)) best.set(def.family, def);
  }
  return new Set([...best.values()].map((a) => a.id));
}

// WHAT IT IS WORTH, at the strength you set — the number, on its own, big
// enough to read. A catalogue that showed only the
// ability's name would make you do the multiply the sim is doing. THE ELEMENT
// THIS BUFF IS ACTUALLY SET TO — the picked one where the ability
// offers a choice (Resupply's gear wheel of ten), its own otherwise.
const wfElement = (a) => {
  const p = wfPick(a.id);
  return (p && p.element) || a.element;
};
// `DT` is the one place a damage type is named, and it reads the locale:
// reaching into META instead prints `corrosive` on a Chinese page.
const wfElementName = (id) => DT(id);

function wfValueLabel(a) {
  // A MULTIPLIER, written the way the card writes it ("+3x"), never as a percent.
  if (a.kind === "final_crit_damage") return `+${Math.round(wfValue(a) * 100) / 100}x`;
  const pct = Math.round(wfValue(a) * 1000) / 10;
  if (a.kind === "add_element" || a.kind === "extra_hit") {
    return `+${pct}% ${wfElementName(wfElement(a))}`;
  }
  return `+${pct}%`;
}

// …and WHERE it lands, which is the half a number cannot say. Two buffs both
// reading "+50%" are worth different amounts on a DoT weapon, and this line is
// the difference.
function wfEffectLine(a) {
  if (a.kind === "faction_damage") {
    return tr("faction damage — the bracket a Bane mod is in, so a status tick takes it twice");
  }
  if (a.kind === "final_crit_damage") {
    return tr("critical damage added flat to the finished multiplier, after the mods and before the crit tier — the same +x on every weapon, so it is worth more the lower the weapon's own crit damage");
  }
  if (a.kind === "final_damage") {
    return tr("damage on its own multiplier — applied once, to the hit and to the status alike");
  }
  // AN EXTRA HIT IS NOT A MULTIPLIER, and the line has to say so or the card
  // reads as "+26% damage" — which is not what it is worth. It is a second
  // damage instance, so it takes the faction bonus and the headshot multiplier
  // a SECOND time, and 26% on the card is more like 40% on the number.
  if (a.kind === "extra_hit") {
    return tr("a second damage instance, not a multiplier — it takes faction damage and the headshot multiplier one more time than the hit it copies");
  }
  // NOT A DAMAGE BRACKET AT ALL. What it buys is reloads not taken, so on a
  // weapon that never reloads it is worth nothing and the line has to say
  // where to look instead of implying a damage number.
  if (a.kind === "ammo_efficiency") {
    return tr("ammo efficiency — it does not touch damage: it divides what a shot costs the magazine, so what it buys is reloads not taken. Multiplicative with other ammo-efficiency sources");
  }
  // NOT A DAMAGE BRACKET EITHER, and worth saying where it lands: it buys
  // swings, so on a weapon whose fight is not rate-bound it is worth less than
  // the percentage reads.
  if (a.kind === "fire_rate") {
    return tr("attack speed, in the same sum a fire-rate mod is in — it buys attacks rather than damage, so what it is worth is whatever more attacks are worth in this fight");
  }
  return tr("added on top and NOT combined with the weapon's own elements");
}

function renderWfBuffs(host, readonly) {
  const box = $(host);
  if (!box) return;
  const list = wfAbilities();
  if (!list.length) { box.innerHTML = ""; return; }
  const running = wfRunning();
  const typed = typeof sim.ability_strength === "number";
  const strength = typed ? Math.round(sim.ability_strength * 100) : "";
  const own = Math.round(wielderStrength() * 100);
  // THE TARGET GETS A SAY. A Demolisher pulses every 5 s and dispels every
  // Warframe ability in range — so against one, nothing ticked here is up, and
  // the sim scores it that way. A section that let you tick Roar and quietly
  // ignored it would be the worst kind of wrong: a number you cannot explain.
  const nulled = (allEnemies().find((e) => e.id === sim.enemy) || {}).nullifies_abilities;
  const rows = list.map((a) => {
    const pick = wfPick(a.id);
    const on = !!pick;
    // SUPERSEDED, not off: you ticked it and something stronger is running.
    // Saying nothing here is how a player ends up believing they have +80%.
    const dead = on && !running.has(a.id);
    return `<div class="wfb${on ? " on" : ""}${(dead || (on && nulled)) ? " dead" : ""}">
      <label class="check wfb-pick"><input type="checkbox" data-wf="${escHtml(a.id)}"${on ? " checked" : ""}${readonly ? " disabled" : ""}>
        <span class="wfb-n">${escHtml(wfName(a))}</span>
        <span class="wfb-f">${escHtml(tr(a.frame))}</span></label>
      <div class="wfb-v">${escHtml(wfValueLabel(a))}</div>
      ${/* A CHOSEN ELEMENT, where the ability offers one. Drawn from the data's
            own list in the game's own order, so the day a member gains or loses
            a choice this follows without being told. */ ""}
      ${(a.elements || []).length
        ? `<label class="wfb-el" title="${escHtml(tr("this ability lets you pick the element — the gear wheel in game"))}">${
            escHtml(tr("element"))} ${ddButton(`wfel-${a.id}`, {
            value: wfElement(a),
            items: a.elements.map((e) => ({ value: e, label: wfElementName(e) })),
            // THE SAME HANDLER, because `ddButton` reflects `value` on the
            // button and fires `change` — which is what let every other native
            // select on this page be swapped without touching its wiring.
            data: { wfel: a.id },
            disabled: !on || readonly,
          })}</label>`
        : ""}
      <div class="wfb-e">${escHtml(wfEffectLine(a))}</div>
      ${/* WHAT IT DOES NOT DO, in the same chips a mod and an arcane card use —
            `notModeledLines` reads `unmodeled_effects` and `live_bugs` off any
            object, and an ability publishes both under those names. The owner
            debugs by reading the card, so a Bullet Attractor this sim has
            nothing to point at has to say so HERE. */ ""}
      <div class="wfb-u">${notModeledLines(a).join("")}</div>
      ${dead ? `<div class="wfb-dead">${escHtml(tr("a stronger buff of the same kind is running — this one adds nothing"))}</div>` : ""}
      ${on && nulled ? `<div class="wfb-dead">${escHtml(tr("this target dispels it — nothing ticked here is running against it"))}</div>` : ""}
      ${a.url ? `<a class="wfb-w" href="${escHtml(a.url)}" target="_blank" rel="noopener">${escHtml(tr("wiki"))} ↗</a>` : ""}
    </div>`;
  }).join("");
  const sub = $(host === "sim-wfbuffs" ? "wfbuff-sub" : "");
  if (sub) {
    sub.textContent = nulled
      ? tr("none — this target dispels them")
      : running.size
        ? `${running.size} ${tr("running")}`
        : tr("none — the weapon on its own");
  }
  box.innerHTML =
    `<div class="wfb-head">
       <label title="${escHtml(tr("empty is your Warframe build's own Ability Strength; a number here overrides it for this fight — every value below is this times the wiki's max-rank number"))}">${escHtml(tr("Ability Strength %"))}
         <input type="number" id="${host}-str" min="0" max="1000" step="1" value="${strength}" placeholder="${own}"${readonly ? " disabled" : ""}></label>
       <span class="sb-empty wfb-src">${escHtml(typed ? tr("typed for this fight") : tr("from the Warframe build"))}</span>
       <span class="wfb-early">${escHtml(tr("what the fight hands this weapon — a squadmate's Roar, your own frame's, an arcane or a companion's precept — assumed up while ticked. WHEN an ability is cast is the action priority list's question and not this block's"))}</span>
     </div>
     ${nulled ? `<div class="wfb-null">${escHtml(
        tr("this target nullifies Warframe abilities — it pulses every 5 seconds and dispels everything in range, so none of these are running and the sim scores it that way"))}</div>` : ""}
     <div class="wfb-grid">${rows}</div>`;
  if (readonly) {
    box.querySelectorAll("input").forEach((el) => {
      el.disabled = true;
      el.title = tr("edit this in the Simulator");
    });
    return;
  }
  const touched = () => { markScenarioDirty(); renderSim(); };
  const str = $(`${host}-str`);
  if (str) str.addEventListener("change", () => { setAbilityStrength(str.value.trim() === "" ? null : Number(str.value) || 0); touched(); });
  box.querySelectorAll("[data-wfel]").forEach((el) => el.addEventListener("change", () => {
    setWfAbility(el.dataset.wfel, true, el.value);
    touched();
  }));
  box.querySelectorAll("[data-wf]").forEach((el) => el.addEventListener("change", () => {
    setWfAbility(el.dataset.wf, el.checked);
    touched();
  }));
}

/// The fight's Ability Strength, typed in percent as the arsenal shows it —
/// null hands it back to the wielder's build.
function setAbilityStrength(percent) {
  sim.ability_strength = percent == null ? null : Math.max(0, percent) / 100;
}

/// A WARFRAME ABILITY RUNNING IN THE FIGHT, or not. `secs: null` is the whole
/// engagement — the one question the page asks today: what is this weapon
/// worth UNDER the buff. Where the ability offers an element, a pick always
/// names one (the definition's first by default), so what the card shows is
/// what the sim runs; an element given to one already running changes it.
function setWfAbility(id, on, element) {
  const was = wfPick(id);
  if (on && was) { if (element) was.element = element; return; }
  sim.abilities = (sim.abilities || []).filter((a) => a.id !== id);
  if (!on) return;
  const def = wfAbilities().find((a) => a.id === id);
  const el = element || (def && (def.elements || [])[0]);
  sim.abilities.push(el ? { id, secs: null, element: el } : { id, secs: null });
}

/// THE RUN COUNT, on its own — the page's, not the fight's.
///
/// GLOBAL, and it stays put: switching weapons, switching fights and opening an
/// OFFICIAL ruler all leave it alone, because how long you are willing to wait
/// is not one of a board fight's terms. The METRIC is the opposite and sits in
/// the block above — KPM is a term of the scenario, so a ruler pins it.
/// THE BUILDER'S STEP NUMBERS, recomputed from what is on screen.
///
/// A block whose axis this weapon does not have is hidden, and the visible ones
/// are then 1..n in document order. Only badges that are already a NUMBER are
/// touched: `Σ`, `≡` and `▶` mark panels that are not steps in the build and
/// say so by not being counted.
/// The BUILDER's blocks, in document order — DERIVED, never listed.
///
/// A list of ids is the right answer to the wrong question: `.config-page` also
/// holds the Sim, Rivens, Enemies and Optimizer pages, so a sweep over `.block`
/// renumbers the Rivens editor as step 5 of building a gun. What needs saying
/// is which MODULE a block belongs to, and once every block says it
/// (`data-module`) this, the CSS and the official-build lock are one query —
/// where four hand lists left the Parts block out of all of them.
const builderBlocks = () => [...document.querySelectorAll('[data-module="builder"]')];

/// …and of those, the ones that are a STEP rather than a read-out.
///
/// The badge already says which, and `renumberBlocks` already reads it: a step
/// carries a NUMBER, while `Σ`, `≡` and `▶` mark panels that only report. So
/// the lock derives from the same fact the numbering does, instead of naming
/// four blocks and going stale on the fifth.
const builderSteps = () => builderBlocks().filter((b) => {
  const n = b.querySelector(".bh .n");
  return !!n && /^\d+$/.test(n.textContent.trim());
});

function renumberBlocks() {
  let n = 0;
  // THE STEPS, not every builder block — see `builderSteps`. The doc above has
  // always said "only badges that are already a NUMBER are touched", and the
  // code never did: it was true because the LIST left the read-outs out. Derive
  // the list and the claim has to be implemented, which is how the Stats
  // panel's `Σ` came back as "5" the first time this was a query
  // (caught by check_valence, 2026-08-24).
  builderSteps().forEach((b) => {
    if (b.hidden) return;
    b.querySelector(".bh .n").textContent = String(++n);
  });
}

function renderSimRuns() {
  const box = $("sim-runs-block");
  if (!box) return;
  box.innerHTML =
    `<label title="${escHtml(tr("how many times to replay this fight HERE. Not part of the scenario: it follows you across fights and weapons, and an official ruler does not pin it. The boards themselves are scored at 1,000 by the server whatever this says"))}">${
      escHtml(tr("Runs"))} <input type="number" id="sim-runs-input" min="1" max="20000" step="10" value="${simRuns()}"></label>` +
    `<span class="sim-hint">${escHtml(tr("yours, not the fight's — the boards score at 1,000"))}</span>`;
  const el = $("sim-runs-input");
  el.addEventListener("change", () => {
    setSimRuns(el.value);
    el.value = String(simRuns());
  });
}

function renderSim() {
  if (!META) return;
  renderSimBuild();
  const enemies = allEnemies();
  const en = enemies.find((e) => e.id === sim.enemy) || enemies[0];
  renderScenarioFields({ target: "sim-target", technique: "sim-technique",
    limits: "sim-limits", extra: "sim-extra", run: "sim-run", squad: "sim-squad" });
  renderRoster();
  renderSimRuns();
  renderWfBuffs("sim-wfbuffs", false);
  // …AND THE OPTIMIZER'S FIGHT CARD, whenever the fight is redrawn.
  renderOptFight();
  renderScenarioBar();
  $("sim-sub").textContent = "current build vs the enemy";
  renderSimBuffs();
  lockOfficialScenario();
  renderBoardConsent();
  refreshBoardDoor();
}

