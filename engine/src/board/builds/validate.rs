use super::*;

/// THE BOARD'S ADMISSION RULE: a legal build, and the shape THIS BENCHMARK asks
/// for.
///
/// `validate` answers whether a build could be equipped; this answers whether
/// it belongs on a leaderboard. Four mods is a legal build and a meaningless
/// board row.
///
/// THE RULE IS THE BENCHMARK'S, not a global constant: a benchmark owns its
/// fight, so it owns what it admits. What it must NOT own is identity —
/// `canonical_mods` is universal, because two boards disagreeing about whether
/// two builds are the same would break dedup and displacement on both.
///
/// "FULL" IS COMPUTED PER WEAPON: eight main slots for everything, plus the
/// evolution tiers and arcane seats this weapon has. A weapon with nothing to
/// fill is complete by having filled it.
///
/// THE EXILUS SLOT IS OUTSIDE IT in both directions — a build is not more
/// complete for having one and not less for lacking one — and a submission
/// arriving with one is accepted with the exilus DROPPED rather than refused.
/// The dropping happens in the client and has to: this payload is a flat list
/// with no slot positions, and an exilus-eligible mod is legal in a main slot.
pub fn validate_for_board(
    benchmark: &str,
    weapon: &str,
    mods: &[String],
    evolutions: &[String],
    arcanes: &[String],
    valence: &str,
) -> Result<ValidBuild, String> {
    validate_for_board_with(
        benchmark, weapon, mods, evolutions, arcanes, valence, None, None, None, None)
}

/// [`validate_for_board`], for a build carrying a riven of known SHAPE.
///
/// A riven OCCUPIES A MAIN SLOT like any other mod, so a benchmark that wants
/// all eight still wants all eight — seven cards and the riven. Nothing about
/// admission changes; what changes is that one of the eight is priced from a
/// shape rather than from the pool.
// EIGHT ARGUMENTS, AND EACH IS A DISTINCT BUILD AXIS. Bundling them into a
// struct would satisfy the lint and hide the thing that matters here: every
// caller has to name every axis, which is the property `board::builds::BUILD_AXES`
// states and the one that has been broken four times by a producer that simply
// did not mention one.
#[allow(clippy::too_many_arguments)]
pub fn validate_for_board_with(
    benchmark: &str,
    weapon: &str,
    mods: &[String],
    evolutions: &[String],
    arcanes: &[String],
    valence: &str,
    riven: Option<&crate::build::rivens::RivenShape>,
    exilus: Option<&str>,
    assembly: Option<&crate::data::weapons::kitguns::Assembly>,
    wielder: Option<&crate::data::warframes::Build>,
) -> Result<ValidBuild, String> {
    // WHO SUPPLIES THE WARFRAME — `with_wielder`, which is where that rule
    // lives so this door and `wfsim-intake` cannot answer it differently.
    let b = validate_with(weapon, mods, evolutions, arcanes, valence, riven, exilus, assembly)?
        .with_wielder(wielder)?;
    let req = match crate::board::benchmarks::get(benchmark) {
        Some(bm) => bm.build.clone(),
        // An unknown benchmark admits nothing: scoring a build against a ruler
        // that does not exist would publish a number with no standard behind it.
        None => return Err(format!("unknown benchmark: {benchmark}")),
    };
    use crate::board::benchmarks::BuildRequirement as R;

    // MODS. The floor that stops an empty build being a weapon's only row —
    // there the single row IS the board, the builder presents it as "Benchmark
    // build #1" with a ⧉ that copies it, and an unmodded build in that position
    // is misinformation with nothing to displace it.
    // …AND THE STANCE IS NOT A MAIN SLOT. A melee build carrying one is `8 + 1`
    // in the same list, so counting the list would refuse a full build for
    // having filled the slot the game gave it.
    let pool = crate::data::mods::pool_naming(&b.weapon, &b.mods);
    let main = b
        .mods
        .iter()
        .filter(|id| {
            !pool.iter().any(|m| m.id == id.as_str() && m.stance.is_some())
        })
        .count();
    if R::requires_full(&req.mods) && main != MAIN_SLOTS {
        return Err(format!(
            "{main} mods, and this benchmark wants all {MAIN_SLOTS} main slots"
        ));
    }

    // THE EXILUS SLOT, which is OPTIONAL rather than excluded as of 2026-08-25.
    //
    // OPTIONAL AND NOT `full`, deliberately. Requiring one would force a choice
    // that is worth nothing on most weapons — and the board would then rank
    // whichever exilus mod the dice favoured, which is the thing the quick
    // calc's `tied` marking exists to admit rather than to publish. Leaving it
    // out is a real build a player can have; so is filling it. Both are rows,
    // and the floor decides which is worth listing.
    //
    // A RULER MAY STILL EXCLUDE IT and the rule is the ruler's, not this
    // function's — the same way `mods`/`evolutions`/`arcanes` admission is.
    if b.exilus.is_some() && !R::allows_exilus(&req.exilus) {
        return Err("this benchmark does not count the exilus slot".to_string());
    }

    // EVOLUTIONS. The same argument, and stronger: an Incarnon weapon with no
    // evolutions is not a weaker build of that weapon, it is the BASE FORM — a
    // different gun. `tier_count` is keyed on the TRANSFORM GROUP, since the two
    // entries of a two-weapon pair share one ladder.
    if R::requires_full(&req.evolutions) {
        let group = crate::data::weapons::spec(weapon)
            .and_then(|s| s.transform_group.clone())
            .unwrap_or_else(|| weapon.to_string());
        let want = crate::data::evolutions::tier_count(&group) as usize;
        if b.evolutions.len() != want {
            return Err(format!(
                "{} of {want} evolution tiers, and this benchmark wants every one",
                b.evolutions.len()
            ));
        }
    }

    // ARCANES. Seats come from the engine's own rule so the page and the board
    // cannot disagree about how many a weapon has. `none` is a filled slot only
    // in the sense that it holds a position — it is not an arcane.
    if R::requires_full(&req.arcanes) {
        let want = crate::data::weapons::arcane_pools(weapon).len();
        let have = b.arcanes.iter().filter(|a| a.as_str() != "none").count();
        if have != want {
            return Err(format!(
                "{have} of {want} arcane seats filled, and this benchmark wants every one"
            ));
        }
    }

    // NO VALENCE CLAUSE HERE. A ruler has nothing to have an opinion about:
    // an adversary weapon with no progenitor element is not a build this board
    // declines, it is not a build.
    // `validate` refuses it for every caller, which is where a legality rule
    // belongs.

    Ok(b)
}

/// Could a player have this in the arsenal?
///
/// The checks, and each one is a rule the game enforces at the slot:
/// the mod is in this weapon's pool; no two mods share a family; the set fits
/// 8 main slots plus at most one EXILUS-eligible mod in the exilus slot; and
/// some polarity layout fits it into [`CAPACITY`].
pub fn validate(
    weapon: &str,
    mods: &[String],
    evolutions: &[String],
    arcanes: &[String],
    valence: &str,
) -> Result<ValidBuild, String> {
    validate_with(weapon, mods, evolutions, arcanes, valence, None, None, None)
}

/// [`validate`], for a build carrying a riven of known SHAPE.
///
/// The riven rides in `mods` as [`RIVEN_SLOT`] so its POSITION is kept, and the
/// shape says what that slot actually holds — which pool it must be rollable
/// from, what it drains, and which elements it brings to the pairing.
// EIGHT ARGUMENTS, AND EACH IS A DISTINCT BUILD AXIS — the same reason
// `validate_for_board_with` carries nine. Bundling them into a struct would
// satisfy the lint and hide the property that matters: every caller has to NAME
// every axis, which is what turned "the scorer silently drops the assembly"
// into six compiler errors instead of one wrong number.
#[allow(clippy::too_many_arguments)]
pub fn validate_with(
    weapon: &str,
    mods: &[String],
    evolutions: &[String],
    arcanes: &[String],
    valence: &str,
    riven: Option<&crate::build::rivens::RivenShape>,
    // The EXILUS slot's mod, when the build wears one. Its own argument for the
    // reason `ValidBuild::exilus` is its own field: nothing downstream could
    // tell a ninth entry in `mods` from a main-slot one.
    exilus: Option<&str>,
    // THE PARTS, on a weapon that takes them. Its own argument for the same
    // reason: the parts are not mods, nothing in `mods` could stand for them,
    // and a build scored with a different grip is a different weapon.
    assembly: Option<&crate::data::weapons::kitguns::Assembly>,
) -> Result<ValidBuild, String> {
    let spec = crate::data::weapons::spec(weapon)
        .ok_or_else(|| format!("unknown weapon: {weapon}"))?;
    // THE SHAPE AND THE SLOT MUST AGREE, both ways. A shape with no slot is a
    // riven nobody is wearing; a slot with no shape is a mod nothing can price.
    // Either one silently scores a different build from the one submitted.
    let wears = mods.iter().any(|m| m == RIVEN_SLOT);
    match (riven, wears) {
        (Some(_), false) => return Err("a riven was named and no slot holds it".into()),
        (None, true) => return Err("a riven slot with no riven in it".into()),
        _ => {}
    }
    if let Some(shape) = riven {
        check_riven_shape(weapon, shape)?;
    }
    // DUPLICATES first, and separately: `normalize` collapses them, so a build
    // listing one mod nine times would otherwise be reported as "eight mods are
    // not in the pool" — a true count attached to the wrong reason, which is
    // worse than no reason at all.
    let mut uniq: Vec<&String> = mods.iter().collect();
    uniq.sort();
    uniq.dedup();
    if uniq.len() != mods.len() {
        return Err(format!("{} of {} mods are listed twice", mods.len() - uniq.len(), mods.len()));
    }
    // THE RIVEN JOINS THE POOL for the rest of this function, because every
    // question below is asked THROUGH the pool — is it in it, what family is it,
    // what does it drain. A riven has no family and drains 18 at max rank, and
    // both of those are the mod's own answer rather than a special case here.
    let riven_def = riven.map(|shape| {
        crate::build::rivens::god_roll(shape, riven_class(weapon))
            .to_mod_def(RIVEN_SLOT, spec.disposition.unwrap_or(1.0))
    });
    let (multishot, evos) = normalize_with(weapon, mods, evolutions, riven);
    if multishot.len() != mods.len() {
        // Loud, because a silently dropped mod is a build the submitter did not
        // send being scored under their name.
        return Err(format!(
            "{} of {} mods are not in {}'s pool",
            mods.len() - multishot.len(),
            mods.len(),
            spec.name
        ));
    }
    let mut pool = crate::data::mods::pool_naming(weapon, mods);
    crate::data::mods::with_ranks(&mut pool, exilus);
    pool.extend(riven_def.clone());
    let def = |id: &str| pool.iter().find(|m| m.id == id).expect("normalised into the pool");

    // THE EXILUS MOD, resolved against this weapon's own pool.
    //
    // IT IS A REAL BUILD AXIS, and it was excluded from the board because
    // "exilus mods are handling and mobility, with no single-target damage
    // model". That was true of most of them and is not true of the pool: BEAM
    // RANGE is exilus (`sinister_reach`, `ruinous_extension`,
    // `galvanized_acceleration`), and beam range decides how many bodies a beam
    // reaches — which on a 19x19 group ruler is most of the damage. Excluding
    // the slot put those mods out of reach of every board row.
    //
    // THE SLOT ONLY TAKES AN EXILUS MOD, which is the one rule the game
    // enforces here that a main slot does not.
    let exilus_id = match exilus.filter(|x| !x.is_empty()) {
        None => None,
        Some(id) => {
            let Some(m) = pool.iter().find(|m| m.id == id).cloned() else {
                return Err(format!("{id} is not a mod this weapon can hold"));
            };
            if !m.exilus {
                return Err(format!("{id} is not an exilus mod, so the exilus slot cannot take it"));
            }
            Some(m)
        }
    };

    // FAMILIES. Two mods of one family cannot be equipped together — and the
    // exilus slot is not a way around that, so it joins the same list.
    let mut fams: Vec<&str> = multishot.iter().filter_map(|id| def(id).family).collect();
    if let Some(m) = &exilus_id {
        if let Some(f) = m.family {
            fams.push(f);
        }
    }
    fams.sort_unstable();
    for w in fams.windows(2) {
        if w[0] == w[1] {
            return Err(format!("two mods of the {} family", w[0]));
        }
    }

    // EIGHT SLOTS. The exilus slot is OUT OF SCOPE for a benchmark build, and the reason is that it does not measure
    // anything: exilus mods are handling and mobility, with no single-target
    // damage model — the optimizer already excludes them from its pool for
    // exactly that reason. It also costs a separate adapter, so
    // counting it would price a build against a resource the ranking cannot
    // see the value of.
    //
    // An exilus MOD is still legal here: the game lets one sit in a regular
    // slot, and spending a main slot on it is the submitter's business.
    //
    // AT MOST eight. Whether a build must be FULL is a board policy and not a
    // legality fact — four mods is a legal build in the game — so it lives in
    // `validate_for_board` rather than here.
    // …AND A STANCE IS NOT ONE OF THEM. A melee weapon has a slot of its own for
    // it — eight main, one exilus, one STANCE — and a stance mod is legal in
    // that slot and NOWHERE else, so a flat list can say which entry it is by
    // looking at it. That is exactly what the exilus slot could not do, which
    // is why THAT one travels in a field of its own and this one
    // does not need to.
    //
    // AT MOST ONE, because there is one slot: two stances in a list is a build
    // nobody can hold, and admitting it would let a submission ship two combo
    // scripts with only the first ever read.
    let stances = multishot.iter().filter(|id| def(id).stance.is_some()).count();
    if stances > 1 {
        return Err(format!("{stances} stances, and a melee weapon has one stance slot"));
    }
    // …AND A FIXED STANCE IS ALWAYS THE ONE. It cannot be taken off (Valkyr
    // Talons' Hysteria, MEASUREMENTS M94), so a list without it is a build
    // nobody can hold — and one that reads 10 capacity short.
    if let Some(fixed) = crate::data::weapons::spec(weapon).and_then(|s| s.fixed_stance.as_deref()) {
        if !multishot.iter().any(|id| def(id).id == fixed) {
            return Err(format!("{fixed} is fixed on this weapon and cannot be removed"));
        }
    }
    if multishot.len() - stances > MAIN_SLOTS {
        return Err(format!(
            "{} mods, and a benchmark build has {MAIN_SLOTS}",
            multishot.len() - stances
        ));
    }

    // CAPACITY, with Forma unlimited. `plan_forma` answers both halves at once:
    // whether ANY layout fits, and how many Forma the cheapest one costs.
    //
    // NINE POLARITIES FOR EIGHT SLOTS, and the exilus one is in the pool even
    // though the exilus SLOT is out of scope.
    //
    // A POLARITY BELONGS TO THE WEAPON, NOT TO THE SLOT IT SITS ON. It can be swapped with another slot's without changing what
    // either slot IS — the exilus slot stays exilus — so a build with no exilus
    // mod at all can still spend that polarity: swap it onto a main slot, and
    // the exilus slot carries whatever came back and sits empty.
    //
    // This line read the other way and said "the slot is out of scope, so its
    // polarity is not a discount this build gets to spend". The `so` was the
    // error: it assumed the polarity was attached to the slot. And the board
    // already assumes the Exilus adapter is installed (docs/INVESTMENT.md), so
    // the slot exists and its polarity is reachable.
    //
    // It over-charged 699 of the 928 stored rows by one Forma each — three
    // quarters of the board, the Torid alone 95 of them.
    let mut innate: Vec<Option<crate::rules::capacity::Polarity>> =
        crate::data::weapons::innate_slots(weapon).to_vec();
    innate.push(crate::data::weapons::exilus_polarity(weapon));
    // THE STANCE IS NOT ONE OF THE NINE. It has a slot of its own — which is the
    // whole reason it hands capacity back below rather than taking it — so
    // pricing it here billed it twice and made a FULL melee build, eight mains
    // and an exilus and a stance, ten mods for nine slots. The board refused
    // exactly the builds it exists to rank.
    let mut planned: Vec<PlannedMod> = multishot
        .iter()
        .map(|id| def(id))
        .filter(|m| m.stance.is_none())
        .map(|m| PlannedMod { base_drain: m.base_drain, polarity: m.polarity })
        .collect();
    // …AND THE EXILUS MOD IS PRICED WITH THEM. Its slot's polarity was already
    // in `innate` above, for the reason written there: a polarity belongs to
    // the weapon, not to the slot it sits on. What is new is that there can now
    // be a NINTH mod to spend it on.
    if let Some(m) = &exilus_id {
        planned.push(PlannedMod { base_drain: m.base_drain, polarity: m.polarity });
    }
    // …AND THE STANCE HANDS CAPACITY BACK. It is an AURA, not a cost — the
    // slot's own polarity decides whether the grant doubles — so it is added to
    // what the weapon has rather than subtracted from it, and a melee build
    // that carries one fits five to ten points more than its rank allows.
    let stance = multishot
        .iter()
        .map(|id| def(id))
        .find(|m| m.stance.is_some())
        .map(|m| crate::rules::capacity::StanceSlot {
            mod_polarity: m.polarity,
            slot_polarity: crate::data::weapons::stance_polarity(weapon),
        });
    let plan = crate::rules::capacity::fit(spec.max_rank, &innate, &planned, BENCHMARK_INVESTMENT, stance)
        .map_err(|e| format!("does not fit this weapon's capacity even with Forma: {e}"))?;

    // ARCANES: one per pool THIS WEAPON seats, and each from that pool.
    //
    // NOT `data::arcanes::slots()` — every arcane DIRECTORY that exists,
    // sorted — which makes seat 0 "primary" on every weapon in the roster. A
    // secondary weapon's arcane is then checked against the primary pool and
    // refused (`secondary_deadhead is not an arcane Dual Toxocyst can seat`),
    // and a submission is thrown away silently, because the
    // scorer counted refusals without printing them.
    //
    // `data::weapons::arcane_pools` is the same answer the page shows, which is
    // the point of having moved it into the engine.
    let seats: Vec<&str> = crate::data::weapons::arcane_pools(weapon);
    if arcanes.iter().filter(|a| a.as_str() != "none" && !a.is_empty()).count() > seats.len() {
        return Err(format!(
            "{} arcanes, and {} seats {}",
            arcanes.len(),
            spec.name,
            seats.len()
        ));
    }
    let mut arcs = Vec::new();
    for (i, a) in arcanes.iter().enumerate() {
        if a == "none" || a.is_empty() {
            arcs.push("none".to_string());
            continue;
        }
        let seat = seats.get(i).copied().unwrap_or("");
        if crate::data::arcanes::for_slot(seat, a).is_none() {
            return Err(format!("{a} is not an arcane {} can seat", spec.name));
        }
        arcs.push(a.clone());
    }

    // THE VALENCE ELEMENT, checked against the weapon's own spec in both
    // directions: an adversary weapon may only take one of ITS progenitor
    // elements, and an ordinary weapon may not take one at all. A silent drop
    // would let a submission claim a bonus the game never hands out.
    let val = match crate::data::weapons::valence_of(weapon) {
        Some(s) => {
            if valence.is_empty() {
                // AND IT IS MANDATORY. Every copy of an
                // adversary weapon comes out of a Lich carrying an element, so
                // a build with none is not a weaker build of that weapon — it
                // is a weapon nobody has. Accepting it here and refusing it
                // one layer up — by the board, and only when the ruler asks —
                // puts a legality rule where an opinion belongs, so it lives
                // here instead.
                return Err(format!(
                    "{} has no Valence element, and every copy of it comes out of a Lich with one ({})",
                    spec.name,
                    s.elements.join(", ")
                ));
            } else if s.elements.iter().any(|e| e == valence) {
                valence.to_string()
            } else {
                return Err(format!(
                    "{valence} is not a progenitor element of {} ({})",
                    spec.name,
                    s.elements.join(", ")
                ));
            }
        }
        None if valence.is_empty() => String::new(),
        None => {
            return Err(format!("{} has no Valence bonus to set", spec.name));
        }
    };

    // THE PARTS MUST COMPOSE, and a weapon that takes none must be given none.
    // A REFUSAL rather than a repair: `assembly_of` repairs a stale link for a
    // FIGHT, where there has to be something to draw; a board record is a
    // statement about one build, and quietly scoring a different assembly is
    // exactly the divergence a record exists to prevent.
    let asm = match (spec.kitgun.as_deref(), assembly) {
        (None, None) => None,
        (None, Some(_)) => {
            return Err(format!("{} is not assembled from parts", spec.name));
        }
        // NAMED NOTHING? THE WEAPON'S OWN DEFAULT, which is what an arsenal
        // slot holds before anyone touches it — a Kitgun is never unassembled.
        (Some(record), None) => crate::data::weapons::kitguns::default_assembly(record),
        (Some(_), Some(a)) => {
            if crate::data::weapons::spec_assembled(spec, Some(a)).is_none() {
                return Err(format!(
                    "{} and {} do not make a {}",
                    a.grip, a.loader, spec.name
                ));
            }
            Some(a.clone())
        }
    };

    Ok(ValidBuild {
        // WHAT IT ROLLED IS NOT A LEGALITY QUESTION, so nothing here has one.
        // `with_riven_rolls` is where a caller that knows them says so.
        riven_rolls: Vec::new(),
        weapon: weapon.to_string(),
        exilus: exilus_id.map(|m| m.id.to_string()),
        mods: multishot,
        evolutions: evos,
        arcanes: arcs,
        forma: plan.cost.total(),
        drain: plan.drain,
        valence: val,
        riven: riven.cloned(),
        assembly: asm,
        // THE DOOR FILLS THIS, not the builder: which weapons carry a wielder
        // is a rule about the BOARD, and this function answers to the builder
        // too, where every build has one and none of them is a term.
        wielder: None,
    })
}

/// The riven pool this weapon rolls from — empty when it rolls from none, which
/// no roster weapon does today and a companion weapon might.
pub(super) fn riven_class(weapon: &str) -> &'static str {
    crate::build::rivens::class_for_weapon(weapon).unwrap_or("")
}

/// IS THIS SHAPE A RIVEN THIS WEAPON CAN ACTUALLY ROLL?
///
/// The board may not rank an item the game cannot produce, and this is the one
/// place that decides it. `build::rivens`'s own rules answer it — the derived
/// pool for the weapon's riven FAMILY, plus the per-family exceptions — so a
/// stat that stops being rollable in a hotfix stops being rankable in the same
/// data change, with no list here to remember.
pub(super) fn check_riven_shape(
    weapon: &str,
    shape: &crate::build::rivens::RivenShape,
) -> Result<(), String> {
    let class = riven_class(weapon);
    if class.is_empty() {
        return Err(format!("{weapon} takes no riven"));
    }
    // SHAPE FIRST: two or three bonuses, at most one malus. Asked through
    // `RivenSpec::illegal` so there is one answer to "could this exist", and
    // this function only has to carry the part that is about the WEAPON.
    let spec = crate::build::rivens::god_roll(shape, class);
    let bad = spec.illegal();
    if !bad.is_empty() {
        return Err(bad.join("; "));
    }
    // **WHAT THE EDITOR OFFERS IS WHAT THE BOARD MUST ACCEPT**, and the two
    // disagreed completely: the class pool MINUS what this
    // weapon cannot roll, which is exactly `rivenPool()` on the page —
    // `rivenPoolAll()` filtered by the `riven_excludes` this same engine
    // serves.
    //
    // IT WAS `derived_for` READ AS A WHITELIST, which is its exact inverse.
    // That function's name and its doc both read as "the stats this weapon
    // rolls" and its OUTPUT is the EXCLUSIONS — a Braton Prime answers
    // `["projectile_speed"]`, being hit-scan, and a Torid answers the three
    // physical types, being pure Toxin. So this loop refused every legal stat
    // and could only have accepted illegal ones: EVERY riven build submitted to
    // the board was rejected with "a X riven does not roll Y", which is why the
    // board has never carried one. `excluded_for` is the same list with the
    // family's exceptions applied and is what the page is served, so reading it
    // here is what makes the two surfaces one answer.
    let excluded = crate::build::rivens::excluded_for(weapon);
    let class_pool = crate::build::rivens::pool(class);
    for id in shape.bonuses.iter().chain(shape.malus.iter()) {
        let offered = class_pool.iter().any(|x| &x.id == id)
            && !excluded.iter().any(|x| x == id);
        if !offered {
            return Err(format!("a {} riven does not roll {id}", crate::data::weapons::spec(weapon)
                .map_or(weapon, |w| w.name.as_str())));
        }
    }
    if let Some(e) = crate::build::rivens::too_many_spliced(
        class,
        shape.bonuses.iter().chain(shape.malus.iter()).map(String::as_str),
    ) {
        return Err(e);
    }
    // A MALUS IS NOT ANY STAT. Five are bonus-only, and the pool says which.
    if let Some(m) = &shape.malus {
        let ok = crate::build::rivens::pool(class).iter().any(|x| &x.id == m && x.malus);
        if !ok {
            return Err(format!("{m} cannot be a riven's negative"));
        }
    }
    Ok(())
}
