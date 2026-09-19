// ---- Evolution ----
// Every tier (EVO I–IV) renders its options as CARDS — icon, name, and the
// verbatim effect text, like the mod/arcane cards — PLUS an explicit None
// card (nothing installed). Wiki-flagged broken evolutions carry a red
// BROKEN badge, and selecting one shows a red note: the engine really
// computes them as NO EFFECT. Deselecting tier 1 (the Incarnon Form
// unlock) drops the weapon to its base form.
// Evolution tiers are per weapon, not a fixed four: Zariman weapons run
// I-V (Laetum), Incarnon Genesis adapters I-IV. Build the numeral instead
// of indexing a table that stops at IV.
const ROMAN = (n) => {
  const T = [[10, "X"], [9, "IX"], [5, "V"], [4, "IV"], [1, "I"]];
  let out = "";
  for (const [v, sym] of T) while (n >= v) { out += sym; n -= v; }
  return out;
};
/// The modes this build may be played in, each with the reason it cannot be —
/// `[id, label, offReason]`, the same shape the Form control always took.
///
/// A mode a BUILD rules out is offered DISABLED, not dropped: "the weapon has
/// no Incarnon form while that mod is on it" is information, and a vanished
/// option is not. Asking for a cycle implies its unlock, and installing that
/// unlock takes a Cannonade off the weapon — so a build wearing one has no
/// cycle to run and the sim refuses it.
/// HOW A FORM IS FIRED, in one word. The trigger is weapon data; this is the
/// English for it, and a trigger with no entry simply contributes nothing
/// rather than printing its yaml token at a reader.
// WHAT A FORCED PROC IS CALLED. Two of them are not damage types at all —
// Lifted and Knockdown are their own procs — and a reader should not have to
// know which of the two machines carries which.
const PROC_WORD = {
  impact: "Impact", puncture: "Puncture", slash: "Slash",
  heat: "Heat", cold: "Cold", electricity: "Electricity", toxin: "Toxin",
  blast: "Blast", corrosive: "Corrosive", gas: "Gas", magnetic: "Magnetic",
  radiation: "Radiation", viral: "Viral",
  lifted: "Lifted", knockdown: "Knockdown",
};
const TRIGGER_WORD = {
  auto: "fully automatic", semi_auto: "semi-auto", burst: "burst",
  charge: "charged", held: "held", projectile: "projectile",
};
/// WHAT FILLS A GAUGE, in words. Mirrors `build::loadout::ChargeOn` — a member the
/// page has no word for falls back to the id, which reads as obviously missing
/// rather than as a silently empty sentence.
const CHARGE_WORD = {
  weakpoint_hits: "weakpoint hits", direct_hits: "direct hits", kills: "kills",
};

/// A FORM, NAMED WITH WHAT TELLS IT FROM THE FORM BESIDE IT: its trigger, or
/// its draw.
const firedForm = (f) => {
  const word = f && TRIGGER_WORD[f.trigger];
  const nm = tr((f || {}).name || "");
  if (!word) return nm;
  if (f.charge_seconds) {
    return trF("{form} ({secs}s charge)", { form: nm, secs: (+f.charge_seconds).toFixed(2) });
  }
  // A FORM NAMED AFTER ITS TRIGGER DOES NOT SAY IT TWICE — the Kuva Hind's
  // read "Semi-Auto (semi-auto)". Compared on the SOURCE strings, because a
  // form name may not be translated yet and the rendered pair ("Semi-Auto"
  // against "半自动") overlaps in neither language; and with BOTH SIDES
  // checked non-empty, since a Chinese string strips to "" and
  // `"".includes("")` is true, which swallowed every translated one.
  const flat = (x) => String(x || "").toLowerCase().replace(/[^a-z]/g, "");
  const said = (a, b) => !!flat(a) && !!flat(b) && flat(a).includes(flat(b));
  const how = tr(word);
  return said(f.name, word) || said(nm, how)
    ? nm
    : trF("{form} ({how})", { form: nm, how });
};

/// Is this mode a CYCLE — one form filling a gauge the other spends?
const isCycleMode = (id) => id === "cycle" || String(id).endsWith("_cycle");

/// THE FORM A MODE IS ABOUT: the one it fires, and for a cycle the one it
/// FILLS THE GAUGE IN. A cycle is named for its feeder — `<mode>_cycle`, the
/// bare `cycle` being the arsenal's own (engine `play_modes`).
const modeForm = (w, id) => {
  const forms = (w || {}).forms || [];
  const def = forms.find((x) => x.is_default) || forms[0] || null;
  const fed = id === "cycle" ? "base" : String(id).replace(/_cycle$/, "");
  if (fed === "base") return def;
  if (fed === "transformed") return earnedForm(w);
  // A WEAPON CAN HAVE MORE THAN ONE ALTERNATE — a bow with an adapter has a
  // tapped shot AND an Incarnon form — told apart by the gauge, as the engine
  // splits them.
  if (fed === "alternate") {
    return forms.find((x) => !x.is_default && !x.gauge_switched) || null;
  }
  // A MODE ID THAT IS A FORM ID — melee's seven, the Hind's two extra
  // triggers. LAST: the form KIND `base` is a bow's TAPPED shot while the
  // `base` MODE is the drawn one, so a direct match would swap them.
  return forms.find((x) => x.id === fed) || def;
};

/// WHAT A MODE ACTUALLY IS, in sentences, derived from the weapon's own forms.
///
/// A mode was a NAME in a dropdown and nothing else, which is fine while every
/// weapon's second mode is the same mechanic — and stopped being fine the day
/// two weapons earned a form by killing rather than by hitting. "Cycle" does not say what fills the gauge, how much of it there
/// is, or what the earned form gets to fire, and those are precisely the
/// numbers that decide whether the mode is worth picking.
///
/// DERIVED, never written per weapon: the forms carry their trigger and their
/// gauge economy (`/api/meta`), so a weapon that arrives tomorrow explains
/// itself. Each sentence is ONE translated template with `{named}` holes, so a
/// weapon with different numbers costs no translation at all.
/// WHAT THIS MODE SWINGS, in the three numbers that decide between the seven:
/// how many swings, what they come to, and how long they take.
///
/// FROM THE STANCE WHEN ONE IS EQUIPPED, because that is the card that moves
/// them — a mode's NAME is fixed and its STRENGTH is not.
/// The form's own is what an empty stance slot fires.
const comboOf = (w, id) => {
  const f = ((w || {}).forms || []).find((x) => x.id === id);
  if (!f) return null;
  const sid = (slots[STANCE] || {}).mod;
  const m = sid ? modById(sid) : null;
  const fromStance = m && m.stance_combos ? m.stance_combos[id] : null;
  return fromStance || f.combo || null;
};

function modeExplain(w, id) {
  const forms = (w || {}).forms || [];
  const def = forms.find((x) => x.is_default) || forms[0] || {};
  const earned = earnedForm(w);
  const alt = forms.find((x) => !x.is_default && !x.gauge_switched);
  // EVERY LINE HAS TO EARN ITS PLACE. "Nothing is spent to
  // be in it, so it can be held forever — a ruler ranks it" was true of the
  // mode it was printed under AND of every mode beside it, which is the
  // definition of a sentence that distinguishes nothing; the same goes for
  // "swung as its neutral combo for the whole engagement" under a heading that
  // already reads "Neutral Combo". What is left either carries a NUMBER or
  // names something this mode does and its neighbours do not.
  const swung = (f) => {
    const c = comboOf(w, f.id);
    if (!c) return [];
    const out = [];
    // THE THREE NUMBERS, all in ONE unit — a share of the WEAPON's base, which
    // is what makes the seven modes comparable at a glance.
    //
    // TWO CONVERSIONS GET THEM THERE. A script's multipliers are relative to
    // the ENTRY they are written in (`swing_share` carries it back, and it is
    // 1.0 on every entry but the slam, whose vector is zero because the whole
    // attack is its explosion), and the explosion is not in the script at all.
    // Without both the heavy slam read 100% for an attack that deals 300%.
    const share = f.swing_share === undefined ? 1 : f.swing_share;
    out.push(trF("{n} swings coming to {total}% of base, over {secs}s at 1.0x attack speed.", {
      n: c.swings, secs: (+c.seconds).toFixed(2),
      total: Math.round((c.total * share + (f.radial_share || 0)) * 100),
    }));
    // A SPIN REACHES THE WHOLE ROOM and an ordinary sweep reaches one body —
    // the one spatial fact separating two combos of the same weapon.
    if (c.spins > 0) {
      out.push(c.spins === c.swings
        ? tr("Every swing is a spin, reaching everything within the weapon's range.")
        : trF("{n} of them are spins, reaching everything within the weapon's range.",
          { n: c.spins }));
    }
    // A SLAM IS NOT BOUNDED BY REACH, which is what makes it the melee mode
    // that answers a crowd.
    if (f.slam) {
      out.push(tr("Its damage is an explosion centred on your own feet, so the weapon's reach does not bound it."));
    }
    // THE COUNTER IS SPENT HERE, and that makes this mode a different BUILD:
    // Blood Rush and Weeping Wounds read the counter this one empties.
    if (f.spends_combo) {
      out.push(tr("It spends the combo counter and is multiplied by it, so the cards that read that counter are worth far less here."));
    }
    if ((c.procs || []).length) {
      out.push(trF("Forces {procs}, whatever the status roll says.",
        { procs: c.procs.map((x) => tr(PROC_WORD[x] || x)).join(tr(", ")) }));
    }
    return out;
  };
  const out = [];
  if (id === "base") {
    // …AND A MELEE WEAPON'S `base` IS A COMBO, whose FORM id is `neutral` and
    // therefore never equals the mode id.
    const lines = swung(def);
    if (lines.length) {
      out.push(...lines);
    } else {
      // "IT NEVER TRANSFORMS" IS ONLY WORTH SAYING WHERE IT COULD BE.
      out.push(forms.some((x) => x.gauge_switched)
        ? trF("Fired in its {form} for the whole engagement — it never transforms.",
          { form: firedForm(def) })
        : trF("Fired in its {form} for the whole engagement.", { form: firedForm(def) }));
    }
  } else if (id === "alternate") {
    out.push(trF("Fired in its {form} for the whole engagement.", { form: firedForm(alt) }));
  } else if (id === "transformed") {
    // THE ONE CONSTRAINT THAT IS NOT TRUE OF ITS NEIGHBOURS stays, because it
    // is the only mode here you cannot actually play: it exists so a form's own
    // numbers can be read.
    out.push(trF("Its {form} alone, from the first second — a form that has to be bought and runs out, so this is here to read its numbers rather than to play.",
      { form: firedForm(earned) }));
  } else if (forms.some((x) => x.id === id)) {
    // **A MODE WHOSE ID IS A FORM'S ID**, which is every melee mode and the
    // Kuva Hind's two extra triggers. `renderMode` DROPS an entry with no
    // lines, so a mode with no arm here is a mode that never draws.
    const f = forms.find((x) => x.id === id) || {};
    const lines = swung(f);
    // A GUN WHOSE MODE IS A FORM has no combo to state, so its trigger is what
    // there is to say — this arm may not REQUIRE a combo.
    out.push(...(lines.length ? lines
      : [trF("Fired in its {form} for the whole engagement.", { form: firedForm(f) })]));
  } else if (isCycleMode(id) && earned) {
    const g = earned.gauge || {};
    // WHICH FORM FILLS THE GAUGE is all one cycle has that the other does not.
    out.push(trF(
      "Fire {base} until the gauge fills, spend it in {other}, come back — and again.",
      { base: firedForm(modeForm(w, id)), other: firedForm(earned) }));
    out.push(trF("{n} {what} fill it, and the earned form fires {rounds} before it ends.", {
      n: g.charges_to_fill, rounds: g.max_rounds,
      what: tr(CHARGE_WORD[g.charge_on] || g.charge_on || ""),
    }));
    // THE TRANSITIONS, and the ZERO CASE IS A FACT. An Incarnon plays an
    // animation in each direction; a form you simply hold the other button for
    // does not, and saying nothing there would read as "unknown" rather than
    // as "none".
    out.push(g.transmute_in > 0 || g.transmute_out > 0
      ? trF("Each way costs an animation — {in}s in, {out}s out, both scaled by reload speed.",
        { in: (+g.transmute_in).toFixed(2), out: (+g.transmute_out).toFixed(2) })
      : tr("There is no transition animation either way."));
    // WHY A KILL-FED GAUGE IS DIFFERENT, said once and only where it applies:
    // it is the only source that the TARGET can refuse.
    if (g.charge_on === "kills") {
      out.push(tr("A gauge bought with kills is worth what the fight lets you earn: against something that will not die, this mode is the base one."));
    }
  }
  return out;
}

function modeOpts(w) {
  const ids = (w.modes || []);
  if (ids.length < 2) return [];
  const cost = (w.unlock_evo && (w.evo_forbids || {})[w.unlock_evo]) || [];
  const blocker = slots
    .map((s) => s.mod)
    .filter((m) => m && cost.includes(m))
    .map((m) => (modById(m) || {}).name || m)[0];
  const off = blocker
    ? `${blocker} ${tr("needs the same trigger on every firing mode, so this build has no Incarnon form")}`
    : null;
  // EVERY cycle: the mod takes the Incarnon form off the weapon, so none of
  // them has anything left to transform into.
  return ids.map((id) => [id, modeLabel(w, id), isCycleMode(id) ? off : null]);
}

/// The mode control, in the BUILDER, in a block of its own above the mods.
///
/// ALWAYS DRAWN. A weapon with one way to be fired is still being fired in it,
/// and a panel that says nothing leaves the reader to assume — which is the
/// same rule the Form control carried, and the reason a single form was stated
/// rather than hidden. Several modes
/// is a dropdown; one is a value.
/// THE VALENCE THIS WEAPON MAY HAVE, or null. One place asks the question, so a
/// control, a payload and a reset cannot disagree about whether the axis exists.
const valenceSpec = (id) => (weaponInfo(id) || {}).valence || null;

/// A build's valence, cleaned against the weapon it is being opened on.
///
/// An element the spec does not offer is DROPPED rather than kept — a preset
/// imported from another weapon carries one, and so does a stale one written
/// before an element was removed. The percentage is clamped to the roll's own
/// range for the same reason.
function defaultValence(id, st) {
  const s = valenceSpec(id);
  if (!s) return { element: "", bonus: 0 };
  // 60% IMPACT, not "none". Every copy of an adversary
  // weapon in the game comes out of its Lich carrying a bonus, so "no element"
  // is not a weaker build of it — it is a weapon nobody has, and the panel
  // would print numbers no player can reproduce.
  //
  // The FIRST element and the roll's CEILING: the ceiling because every player
  // can Valence-fuse to it and it is what the board scores, the first because
  // the wiki lists them in the game's own order and Impact is where it starts.
  const el = st && s.elements.includes(st.element) ? st.element : s.elements[0];
  const b = st && Number.isFinite(Number(st.bonus)) ? Number(st.bonus) : s.max;
  return { element: el, bonus: Math.min(Math.max(b, s.min), s.max) };
}

/// The parts `/api/meta` publishes for a weapon, or `null` if it takes none.
const assemblySpec = (id) => (weaponInfo(id) || {}).assembly || null;

/// THE OTHER SLOT OF THE SAME CHAMBER, or `null`.
///
/// A Kitgun is ONE weapon — one mastery track, one riven, one wiki page, and
/// therefore ONE URL — and it is TWO roster entries, because the slot decides
/// which mods it may hold and that is a question with a static answer. Those
/// two facts meet here: `/weapons/Tombfinger` resolves to one entry and the
/// Slot control moves to the other WITHOUT changing the address, which is what
/// makes the two a page rather than two pages that happen to share a name. The
/// second entry still needs an address of its OWN to be linkable at all, and
/// `urlSlug` gives it one; nothing on the page navigates to it.
///
/// Switching is switching WEAPONS in every other sense — its own build, its own
/// riven slots, its own board rows — which is exactly what "nothing crosses
/// between weapons" already guarantees, so nothing extra is needed for it.
const slotSibling = (id) => {
  const s = assemblySpec(id);
  if (!s) return null;
  const w = (META.weapons || []).find(
    (x) => x.id !== id && x.assembly && x.assembly.chamber === s.chamber);
  return w ? w.id : null;
};

/// THIS WEAPON'S PARTS, repaired against what it can actually take.
///
/// A build with no assembly, or one carrying a part from the sibling slot,
/// lands on the server's own default rather than on nothing — the same repair
/// `assembly_of` does on the wire, and it has to agree with it: a page showing
/// one assembly while the server simulates another is the exact shape of bug
/// that had a player measuring 26 KPM on a ranking and 15 in the simulator.
/// That is why the default comes from `/api/meta` and is not computed here.
function defaultAssembly(id, st) {
  const s = assemblySpec(id);
  if (!s) return null;
  const d = s.default || {};
  const has = (list, v) => list.some((x) => x.id === v);
  return {
    grip: st && has(s.grips, st.grip) ? st.grip : d.grip,
    loader: st && has(s.loaders, st.loader) ? st.loader : d.loader,
  };
}

/// THE PARTS BLOCK — two dropdowns, and what each part is worth beside it.
///
/// The block is HIDDEN on everything that is not modular, where Mode is always
/// drawn: every weapon is fired in some mode, and only a Kitgun has parts.
///
/// EACH OPTION CARRIES ITS OWN NUMBERS, because that is the whole decision. A
/// grip is damage, fire rate or a beam's reach, and (on a charge chamber) charge
/// time; a loader is
/// three additive deltas that may be NEGATIVE, plus a magazine and a reload.
/// A list of twenty names would say none of it, which is the Mode control's own
/// lesson.
/// ONE PART OF A KITGUN, swapped. A part is never absent, so there is no null.
function setAssemblyPart(part, v) {
  assembly = { ...assembly, [part]: v };
  markPresetDirty();
  renderAssembly();
  refreshPanel();
}

function renderAssembly() {
  const block = $("assembly-block");
  const box = $("assembly-row");
  if (!block || !box || !META) return;
  const w = $("weapon").value;
  const s = assemblySpec(w);
  block.hidden = !s;
  if (!s) return;
  if (!assembly) assembly = defaultAssembly(w, null);
  const sub = $("assembly-sub");
  if (sub) sub.textContent = tr("what this weapon is built from");

  // A ROUND COUNT AND A RELOAD ARE NOT THE SAME KIND OF NUMBER, so the block
  // trims its own rather than borrowing the results panel's two-significant
  // figures: "23 rounds" and "1.70 s" is what a card would say.
  const n = (v) => String(Math.round(Number(v) * 100) / 100);
  const sign = (v, unit) => (v > 0 ? "+" : "") + n(v) + (unit || "");
  const pc = (v) => Math.round(Number(v) * 1000) / 10;
  const gripItem = (g) => {
    const d = s.grip_stats && s.grip_stats[g.id];
    // A BEAM GRIP'S TRADE IS REACH, and its fire rate is the same on all five,
    // so the reach is what the hint shows in its place.
    const bits = d
      ? [`${n(d.damage)} ${tr("damage")}`,
         d.range_m != null ? `${n(d.range_m)} m` : `${n(d.fire_rate)}/s`,
         d.charge_seconds != null ? `${n(d.charge_seconds)} s ${tr("charge")}` : ""]
        .filter(Boolean)
      : [];
    return { id: g.id, label: g.name, hint: bits.join(" · ") };
  };
  const loaderItem = (l) => {
    // ONLY WHAT IT MOVES. Two of the three deltas are zero on most loaders and
    // six are zero on all three, so printing every one would bury the ones that
    // decide the pick.
    const bits = [
      `${n(l.rounds)} ${tr("rounds")}`,
      `${n(l.reload_seconds)} s`,
      l.crit_chance ? `${sign(pc(l.crit_chance), "%")} ${tr("crit")}` : "",
      l.crit_multiplier ? `${sign(l.crit_multiplier)}x` : "",
      l.status_chance ? `${sign(pc(l.status_chance), "%")} ${tr("status")}` : "",
    ].filter(Boolean);
    return { id: l.id, label: l.name, hint: bits.join(" · ") };
  };

  // THE PARTS, on the page's one ranked control — see `rankedSlot`. A part can
  // never be absent (a Kitgun without a grip is not a Kitgun), so the card has
  // no Remove.
  const partCfgs = {};
  const pick = (part, label, items, hint) => {
    const id = "dd-" + part;
    const cur = items.find((it) => it.id === assembly[part]);
    partCfgs[id] = {
      axis,
      label: tr(label),
      title: hint,
      value: assembly[part],
      removable: false,
      // A grip and a loader may share a name, so the SCAN's key carries the
      // part it belongs to.
      items: items.map((it) => ({ key: part + ":" + it.id, value: it.id, label: it.label, hint: it.hint })),
      card: cur ? { name: cur.label, lines: cur.hint ? [cur.hint] : [], title: hint } : null,
      onPick: (v) => { if (v && assembly[part] !== v) setAssemblyPart(part, v); },
    };
    return rankedSlot(id, partCfgs[id]);
  };

  // THE SLOT. In game the GRIP decides it; here it is picked first, because it
  // decides the mod pool and therefore which build you are even editing. So the
  // two are one control read in the other direction: choose the slot, then a
  // grip from that slot's five.
  const sibling = slotSibling(w);
  const wSlot = (weaponInfo(w) || {}).slot;
  const slotPick = sibling
    ? `<label title="${escHtml(tr("the grip decides this in game; here it comes first, because it decides which mods the weapon can hold — and switching is switching weapons, so each slot keeps its own build"))}">${
        escHtml(tr("Slot"))} ${ddButton("dd-kitslot", {
        value: wSlot,
        items: [w, sibling].map((id) => ({
          value: (weaponInfo(id) || {}).slot,
          label: tr(cap1((weaponInfo(id) || {}).slot)),
        })).sort((a, b) => a.value.localeCompare(b.value)),
        // The address stays put: `restoreState` knows a Kitgun's two slots
        // are one page, so switching the ENTRY does not move it.
        onPick: (v) => {
          if (v === wSlot) return;
          switchWeapon(sibling);
        },
      })}</label>`
    : "";

  // THE CHAMBER IS NOT HERE AT ALL. It was stated as a
  // read-only value on the reasoning that a reader should see what is missing
  // on purpose — but the chamber IS the weapon, whose name is already at the
  // top of the page, so the row said the same word twice and took the first
  // line of the block to do it. What belongs in that line is the one thing
  // about this weapon that IS a choice and is not offered anywhere else: the
  // SLOT.
  //
  // NO SUMMARY LINE: the card each part draws carries its own stats, which is
  // all a line under the shut dropdowns could say.
  const axis = { kind: "assembly", idx: 0 };
  box.innerHTML =
    slotPick +
    pick("grip", "Grip", s.grips.map(gripItem),
      tr("the grip sets damage, fire rate and the charge — only this slot's five are offered, because a grip is what decides the slot")) +
    pick("loader", "Loader", s.loaders.map(loaderItem),
      tr("the loader sets the magazine and the reload, and adds three deltas that can be negative"));
  // THE SCAN FIRES WHEN A LIST IS OPENED — see `openRanked`. The strip that
  // reports it lives inside that list, where the ranking is being read.
  bindRankedSlots(box, partCfgs);
}

/// THE VALENCE BLOCK. Two controls, because a Lich hands you two facts: which
/// element, and how big the roll was.
///
/// THE ELEMENT IS MANDATORY. Every copy of an adversary
/// weapon comes out of a Lich carrying one, so there is no "none" to pick and
/// no empty state to open on: `defaultValence` starts on the first element at
/// the roll's ceiling. The weapon's own printed panel — the wiki infobox's
/// figure — is not a build anyone can play, and offering it as one put a
/// number nobody can reproduce on the same row as six that they can.
function renderValence() {
  const box = $("element-cfg");
  if (!box || !META) return;
  const w = weaponInfo($("weapon").value) || {};
  const s = valenceSpec(w.id);
  const sub = $("valence-sub");
  if (!s) { box.innerHTML = ""; if (sub) sub.textContent = ""; return; }
  if (sub) sub.textContent = tr("the bonus this copy came out of its Lich with — added as the weapon's own BASE damage, so elemental mods and status scale with it");
  const pct = (x) => Math.round(x * 1000) / 10;
  // ON THE PAGE'S ONE RANKED CONTROL — see `rankedSlot`. Which element wins is
  // the question a scan is worth the most on: a progenitor element is a whole
  // element entering the hierarchy, so the answer depends on the mods around it
  // and on the target, and no card states it.
  //
  // THE NAME AND THE NUMBER, and nothing else. An
  // evolution's description earns its space because the perks differ from each
  // other; seven elements do not — every one says the same sentence, so seven
  // copies of it is a wall of text between the reader and the seven numbers
  // that are the actual answer. What the bonus DOES is said once, by the
  // block's own subtitle.
  //
  // NO "NONE" OPTION. Every copy of an adversary weapon comes out of a Lich
  // carrying an element, so an empty valence is not a weaker build of this
  // weapon — it is a weapon nobody has, and a number nobody can reproduce.
  const axis = { kind: "valence", idx: 0 };
  const cfgs = {
    "dd-valence": {
      axis,
      label: tr("Element"),
      value: valence.element,
      // NO REMOVE. Every copy of an adversary weapon comes out of a Lich
      // carrying an element, so there is no empty state to offer.
      removable: false,
      items: s.elements.map((e) => ({ key: e, value: e, label: DT(e) })),
      card: { name: DT(valence.element) },
      onPick: (v) => { if (v && valence.element !== v) setValence({ element: v }); },
    },
  };
  box.innerHTML =
    rankedSlot("dd-valence", cfgs["dd-valence"]) +
    `<div class="runs-row"><label title="${escHtml(tr("how big the roll was, as a share of base damage — a Lich rolls it randomly and Valence Fusion raises it, capping at the number on the right"))}">${escHtml(tr("Valence bonus"))} <span class="unit">%</span> <input type="number" id="valence-bonus" min="${pct(s.min)}" max="${pct(s.max)}" step="0.5" value="${pct(valence.bonus)}"></label>` +
    `<span class="sim-hint">${escHtml(tr("rolls") + ` ${pct(s.min)}–${pct(s.max)}%`)}</span></div>`;
  bindRankedSlots(box, cfgs);
  const inp = $("valence-bonus");
  if (inp) {
    inp.addEventListener("change", () => setValence({ bonus: Number(inp.value) / 100 }));
  }
}

/// AN ADVERSARY WEAPON'S VALENCE — its element, and a bonus clamped to what a
/// Lich can roll, because a bonus past the cap is a weapon nobody can own.
function setValence({ element, bonus }) {
  const s = valenceSpec($("weapon").value);
  if (element != null) valence.element = element;
  if (bonus != null) valence.bonus = Math.min(Math.max(Number.isFinite(bonus) ? bonus : s.min, s.min), s.max);
  markPresetDirty(); renderValence(); refreshPanel();
}

/// HOW THE BUILD IS PLAYED, set by the dropdown and the agent door alike.
function setMode(v) {
  mode = v;
  // Keep the address bar honest: it may have said a mode, and it is not
  // saying this one any more.
  const q = new URLSearchParams(location.search);
  if (q.get("mode") && q.get("mode") !== v) {
    q.set("mode", v);
    history.replaceState(null, "", `${location.pathname}?${q}`);
  }
  markPresetDirty(); renderMode(); refreshPanel();
}

function renderMode() {
  const box = $("mode-row");
  if (!box || !META) return;
  const w = weaponInfo($("weapon").value) || {};
  const opts = modeOpts(w);
  const sub = $("mode-sub");
  // **THE MODE YOU ARE IN, and not the other six.** A dropdown names the
  // choices and says nothing about them, and the numbers that decide between
  // them — what fills the gauge, how much of it there is, what the earned form
  // gets to fire — are on no other screen. It FOLDS and remembers, because a
  // reader who has learned this weapon does not need it again.
  //
  // LISTING THEM ALL IS A WALL at seven modes: six blocks about something the
  // reader did not pick, with the one that IS theirs no longer findable. The
  // reason to list them was comparison, and comparison is what the BOARD does —
  // it ranks every mode of every weapon as its own row. So the picked one gets
  // its sentences; `modeExplain` still answers for any mode, and the control
  // beside it still names all seven.
  const explain = (id) => {
    const lines = modeExplain(w, id);
    if (!lines.length) return "";
    return foldBlock("mode-def", tr("What this mode is"), "",
      `<div class="modedef on">
        <div class="modedef-n">${escHtml(modeLabel(w, id))}</div>
        ${lines.map((s) => `<div class="modedef-l">${escHtml(s)}</div>`).join("")}
      </div>`);
  };

  if (!opts.length) {
    // ONE WAY TO FIRE IT. Named from the weapon's own form, so it reads as a
    // fact about the weapon rather than as a control somebody disabled — and
    // it still gets its sentence: "base" is a claim (it never transforms) and
    // not merely the absence of a choice.
    const only = (w.modes || ["base"])[0];
    box.innerHTML = `<label>${escHtml(tr("Mode"))} <span class="fixed-val">${
      escHtml(modeLabel(w, only))}</span></label>${explain(only)}`;
    if (sub) sub.textContent = tr("one firing mode");
    wireFolds(box);
    return;
  }
  // A build naming a mode this weapon does not offer falls back to how the
  // weapon is played — a stale preset, or one copied from another weapon.
  if (!opts.some(([id]) => id === mode)) mode = defaultMode(w.id, null);
  const why = (opts.find(([id]) => id === mode) || [])[2];
  if (sub) sub.textContent = tr("how this build is played");
  // RANKED, because a mode is a build axis like every other
  // (`engine::board::builds::BUILD_AXES`) and the cheapest of them to try: it moves
  // no other part of the build, so each one is a single simulation against the
  // same baseline. STILL A DROPDOWN — the axis is what measures a list, not
  // the shape of the control that opens it, so how a weapon is played stays
  // one click away.
  box.innerHTML = `<label>${escHtml(tr("Mode"))} ${ddButton("dd-mode", {
    axis: { kind: "mode", idx: 0 },
    axisLabel: tr("Mode"),
    value: mode,
    items: opts.map(([id, label, offReason]) => ({
      key: id, value: id, label: label + (offReason ? " ⊘" : ""),
      hint: offReason || "", disabled: !!offReason,
    })),
    onPick: (v) => setMode(v),
  })}</label>${why ? `<span class="warn">⊘ ${escHtml(why)}</span>` : ""}${
    explain(mode)}`;
  wireFolds(box);
}

function renderEvo() {
  const tiers = weaponEvos();
  const rows = [];
  // TIERS UNLOCK IN ORDER, as they do in game: tier N is reachable only once
  // tier N-1 is installed. Without it the whole branch is
  // void — a tier-2 perk with no tier 1 is not a weaker build, it is not a
  // build — so the later rows are DISABLED rather than silently contributing
  // to a number nobody could reach.
  const openTo = evoOpenTo();
  const genesis = wikiUrl(wikiWeaponName(weaponInfo($("weapon").value)));
  const cfgs = {};
  for (const t of tiers) {
    const sel = evoSel[t.tier] || null;
    const locked = t.tier > openTo;
    const id = "dd-evo-" + t.tier;
    const o = (t.options || []).find((x) => x.id === sel);
    cfgs[id] = {
      axis: { kind: "evo", idx: 0 },
      label: `EVO ${ROMAN(t.tier)}`,
      addLabel: tr("add evolution"),
      value: sel || "",
      // A TIER CAN BE EMPTY — a bare weapon is every tier empty — so this one
      // HAS a Remove. That is the whole of the difference from a part or an
      // element, and it is one menu item rather than a second shape.
      removable: !!sel,
      locked,
      lockedWhy: locked ? tr("install the previous tier first") : "",
      items: t.options.map((x) => ({
        key: x.id,
        value: x.id,
        label: x.name,
        // The perk's own lines, flattened: a list row is one line, and the full
        // text is on the card of whichever perk is installed.
        hint: evoLines(x).join(" · "),
        // WHAT THE SIM DOES NOT MODEL, ON THE ROW YOU CHOOSE FROM — the whole
        // point of these chips is to be readable while deciding, which is
        // exactly when the list is open.
        extra: (x.broken ? ' <i class="bx">BROKEN</i>' : "") + evoGapChips(x, "i"),
      })),
      card: o ? evoCardOf(o, genesis) : null,
      onPick: (v) => pickEvolution(t.tier, v || null),
    };
    rows.push(rankedSlot(id, cfgs[id]));
  }
  $("evo-rows").innerHTML = rows.join("");
  // ONE AXIS FOR EVERY TIER, still: they are scanned in a single pass — a dozen
  // candidates, not seventy — so opening any tier's list measures them all and
  // the next tier's opens already answered. What changed is WHEN. The pass used
  // to run from this render, on the reasoning that the options were all on
  // screen at once; they are inside a closed list now, so it runs on open
  // (`openRanked`).
  bindRankedSlots($("evo-rows"), cfgs);
}

/// THE INSTALLED PERK, as its own card — the pieces `rankedSlot` draws.
///
/// A gap this app admits has to be ON THE PAGE, and a chip that appears only
/// inside an open list is a weaker promise than that (`check_disclosure`'s
/// subject). The list carries every option's chips because that is where the
/// choosing happens; this carries the chosen one's, plus the text and the wiki
/// link a list row cannot hold, and it is there without anybody opening
/// anything.
function evoCardOf(o, genesis) {
  // The broken warning lives INSIDE the card, so it never straddles the
  // divider into the next tier.
  const warn = o.broken
    ? `<span class="ed warn">⚠ ${escHtml(tr("does not work in-game (wiki) — the simulation computes it as NO EFFECT"))}</span>`
    : "";
  // THE CONDITION OVERLOAD CAVEAT. A perk flagged here raises base damage
  // WITHOUT feeding the CO term, so it reads strictly better than the tier's
  // other option and is not — every status is worth less, and past a couple of
  // statuses the other one overtakes it. Reported as a bug
  // precisely because the only place saying so was the CO row in the stats
  // panel, which is not where the comparison happens.
  const coNote = o.co_excluded
    ? `<span class="ed caveat" title="${escHtml(
        tr("Condition Overload computes on this weapon's ORIGINAL base damage — this perk's added base is excluded, so every status type is worth less than the card implies"),
      )}">◈ ${escHtml(tr("its added base does not feed Condition Overload"))}</span>`
    : "";
  return {
    name: o.name,
    // Evolutions have no standalone wiki pages, so they link to the WEAPON's —
    // which carries the same evolution tables and is where you wanted to end up
    // anyway.
    href: genesis,
    icon: o.icon,
    broken: o.broken,
    lines: evoLines(o),
    title: (o.effects || []).join(String.fromCharCode(10)),
    chips: (o.broken ? ' <i class="bx">BROKEN</i>' : "") + evoGapChips(o, "i"),
    notes: coNote + warn,
  };
}

/// INSTALLING OR REMOVING ONE TIER, and everything that follows from it.
function pickEvolution(tier, id) {
  const tiers = weaponEvos();
  evoSel[tier] = id;
  // Removing a tier removes everything that stood on it. Leaving them
  // selected-but-void would show a build the game cannot make, and the engine
  // would price perks the weapon never reached.
  if (!id) tiers.forEach((x) => { if (x.tier > tier) evoSel[x.tier] = null; });
  // ...and an installed form takes with it every mod that needed the weapon
  // not to have it (a Cannonade under the Incarnon form). Said out loud, never
  // silently: the slot emptying under you is exactly the kind of change a
  // build must not make without telling you.
  const no = forbiddenByEvos();
  const evicted = slots
    .filter((s) => s.mod && no.has(s.mod))
    .map((s) => (modById(s.mod) || {}).name || s.mod);
  if (evicted.length) {
    slots = slots.map((s) => (s.mod && no.has(s.mod) ? { ...s, mod: null, rank: null } : s));
    presetToast(`${tr("unequipped")}: ${evicted.join(", ")} — ${
      tr("it needs the same trigger on every firing mode")}`);
    renderMods();
  }
  // Redraw the whole ladder, not just this row: a pick opens (or a removal
  // shuts) every tier below it.
  renderEvo(); renderMode(); refreshPanel();
}

