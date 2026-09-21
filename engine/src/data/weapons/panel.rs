use super::*;

/// Build the RAW (no evolutions, no mods) [`WeaponBase`] panel for a weapon
/// entry. `frenzy_active` folds passive-granted element injections in
/// (resolved from the transform group's base entry, where passives live).
/// THE SPEC A BUILD ACTUALLY FIRES, with a KITGUN's assembly composed into it.
///
/// Everything that is not modular comes back untouched. A modular weapon with
/// no assembly named is composed with its DEFAULT assembly.
///
/// WHY IT REWRITES THE SPEC RATHER THAN THE PANEL: `base_panel` derives a
/// great deal from `s.attack` on the way past — the cone, the falloff, the CO
/// class, the punch-through budget, the radial's crit and status defaults — so
/// a panel patched afterwards would have to re-derive every one, and the ones
/// nobody remembered would keep the preview's answer. Composing one layer
/// earlier means nothing downstream has to learn what a Kitgun is.
///
/// `None` when the assembly names parts that do not compose — a grip from the
/// other slot, a loader that does not exist. A Kitgun that is not assembled has
/// no numbers, and inventing some would be worse than saying so.
pub fn spec_assembled<'a>(
    s: &'a WeaponSpec,
    a: Option<&crate::data::weapons::kitguns::Assembly>,
) -> Option<std::borrow::Cow<'a, WeaponSpec>> {
    let Some(chamber_id) = s.kitgun.as_deref() else {
        return Some(std::borrow::Cow::Borrowed(s));
    };
    // A MODULAR WEAPON IS NEVER FIRED AS ITS PREVIEW. The `base` row is the
    // module's no-grip preview and is a stat line no player can reproduce, so
    // a caller that names no assembly gets the DEFAULT one rather than that —
    // which means no path anywhere can produce a preview-based panel by
    // forgetting to pass parts. Six call sites in `webapi` build a base for a
    // request and every one of them would otherwise have had to remember;
    // a shared helper is not enough when the DECISION around it is the thing
    // that goes missing (AGENTS.md, `parse_fight`).
    let owned;
    let a = match a {
        Some(a) => a,
        None => {
            owned = crate::data::weapons::kitguns::default_assembly(chamber_id)?;
            &owned
        }
    };
    let built = crate::data::weapons::kitguns::assemble(a)?;
    // THE ASSEMBLY MUST BE THIS ENTRY'S. The grip picks the slot and the slot
    // picks the chamber record, so a secondary grip on the primary entry
    // composes a real weapon that is the WRONG one — which is exactly the kind
    // of mismatch that reads as a working build.
    if built.chamber_record_id != chamber_id {
        return None;
    }

    let mut out = s.clone();
    // THE FORM DECIDES WHICH EXPLOSION. A Tombfinger primary explodes
    // differently on a quick shot and on a full charge, and this entry is one
    // of the two; the chamber keys its blasts by the same form ids.
    let blast = built.blasts.get(&s.form);
    if built.blasts.is_empty() != out.attack.radial.is_none() {
        // A chamber that explodes on an entry with no `radial:` (or the other
        // way round) is a roster entry and a parts file that disagree about
        // what the weapon IS, and neither one can be the answer.
        return None;
    }

    // THE DIRECT HIT IS WHAT THE EXPLOSION LEAVES — the whole shot where the
    // explosion is ADDED beside it, and short of it by the carve where it is
    // taken out of it. One field either way, so the two shapes need no branch.
    out.attack.damage = match blast {
        Some(b) => b.direct.clone(),
        None => built.damage.clone(),
    };
    if let (Some(r), Some(b)) = (out.attack.radial.as_mut(), blast) {
        r.damage = b.damage.clone();
        r.radius_m = b.radius_m;
        r.falloff_start_m = Some(0.0);
        r.falloff_reduction = Some(1.0 - b.falloff_to);
    }

    out.attack.fire_rate = built.fire_rate;
    // A CHARGE ONLY WHERE THE CHAMBER HAS ONE, and it is the GRIP's — the one
    // trigger in the roster whose charge belongs to a part rather than to the
    // weapon.
    if built.charge_seconds.is_some() {
        out.attack.charge_seconds = built.charge_seconds;
    }
    out.attack.crit_chance = built.crit_chance;
    out.attack.crit_multiplier = built.crit_multiplier;
    out.attack.status_chance = built.status_chance;
    out.attack.multishot = built.multishot;
    out.attack.ammo_cost = built.ammo_cost;
    // ONLY WHERE THE MODULE STATES A DEPTH. Catchmoon's page states infinite
    // body punch through and its module states none; a zero composed over the
    // entry would erase the page's number with the module's silence.
    if let Some(p) = built.punch_through_m {
        out.attack.punch_through_m = p;
    }
    // THE GRIP'S REACH, where the chamber publishes one. A beam carries its
    // reach in `beam:` (the radius and the chain read it too); anything else on
    // the attack.
    if let Some(r) = built.range_m {
        match out.attack.beam.as_mut() {
            Some(b) => b.range_m = r,
            None => out.attack.range_m = Some(RangeSpec::Metres(r)),
        }
    }
    out.attack.spread = Some(SpreadSpec {
        min_deg: built.spread.min_deg,
        max_deg: built.spread.max_deg,
    });
    out.recharge_per_second = built.recharge_per_second;
    out.magazine = Some(built.magazine);
    out.reload_seconds = Some(built.reload_seconds);
    out.ammo_max = Some(built.ammo_max);
    out.ammo_pickup = Some(built.ammo_pickup);
    // THE DISPOSITION IS THE ENTRY'S, not the assembly's: it is per chamber AND
    // per slot, and this entry already is one chamber in one slot. Restating it
    // from the parts would be the same number written twice.
    Some(std::borrow::Cow::Owned(out))
}

pub fn base_panel(id: &str, frenzy_active: bool) -> WeaponBase {
    base_panel_assembled(id, frenzy_active, None)
}

/// [`base_panel`], for a MODULAR weapon whose numbers are its assembly's.
///
/// Panics on an assembly that does not compose, the way this function already
/// panics on an unknown weapon id: both are a caller handing over a weapon that
/// does not exist, and a panel invented for one is a build nobody can reproduce.
pub fn base_panel_assembled(
    id: &str,
    frenzy_active: bool,
    assembly: Option<&crate::data::weapons::kitguns::Assembly>,
) -> WeaponBase {
    let raw = spec(id).unwrap_or_else(|| panic!("unknown weapon id: {id}"));
    let composed = spec_assembled(raw, assembly).unwrap_or_else(|| {
        panic!("weapon {id}: the assembly {assembly:?} does not compose into it")
    });
    let s = &*composed;

    let mut vector = DamageVector::new();
    for (name, amount) in &s.attack.damage {
        vector = vector.with(damage_type(name), *amount);
    }

    let injected_elements = if frenzy_active {
        s.perks
            .iter()
            .filter_map(|p| p.resolve().grants.as_ref())
            .filter_map(|g| g.injected_element.as_ref())
            .map(|inj| (damage_type(&inj.element), inj.amount))
            .collect()
    } else {
        Vec::new()
    };

    // EVERY spelling is named, and anything else is a LOAD ERROR.
    //
    // A `_ => Independent` arm swallows both a typo and an omission: a
    // misspelled `co_behavior` silently becomes Independent — the wiki's
    // "Multiplying", the EXCEPTION class — on a weapon the CO catalog does not
    // list at all, and the correct spelling works only by falling through to
    // the same arm.
    //
    // The default it implied is also backwards. The catalog "lists only
    // discrepant attacks. Anything not listed should be assumed to be Additive"
    // (docs/MECHANICS.md §Condition Overload), so the class an unlisted weapon
    // takes is ADDITIVE — there is no defensible default that is the exception.
    // Hence: state it, or fail.
    let co_behavior = match s.co_behavior.as_deref() {
        Some("additive_with_base_damage") => CoBehavior::AdditiveWithBaseDamage,
        Some("independent") => CoBehavior::Independent,
        Some("inert") => CoBehavior::Inert,
        other => panic!(
            "weapon {}: co_behavior must be additive_with_base_damage / independent / inert, got {other:?}.              The wiki CO catalog lists only DISCREPANT attacks — a weapon it does not list is              additive_with_base_damage, never independent.",
            s.id
        ),
    };

    // An Incarnon form's rounds are charge-backed; the locked-gauge
    // pseudo-reload supplies the sim's magazine/reload reduction.
    let (magazine_size, base_reload) = match &s.pseudo_reload {
        Some(pr) => (pr.magazine, pr.reload_seconds),
        // A WEAPON WITH NO MAGAZINE IS NOT ASKED FOR ONE.
        //
        // `no_magazine` is a flag an entry can carry while stating a magazine
        // anyway (the Grimoire's 1); melee is the family where the field is not
        // merely unused but has no value to state: a swing spends
        // nothing and there is no reload to time. Requiring the number would
        // mean writing a fiction into the data — which is the one thing
        // data/README.md forbids — so the requirement reads the declaration.
        //
        // The loop tops the magazine up silently on such an entry, so the value
        // is only ever the size of a batch it never runs out of; 1 is the
        // smallest honest one.
        None if s.attack.no_magazine => (s.magazine.unwrap_or(1.0), s.reload_seconds.unwrap_or(0.0)),
        None => (
            s.magazine.unwrap_or_else(|| panic!("{id}: magazine missing")),
            s.reload_seconds.unwrap_or_else(|| panic!("{id}: reload_seconds missing")),
        ),
    };

    // A BY-ROUND RELOAD, resolved into (start, per shell, end).
    //
    // Where the weapon's page states the three parts, they are used. Where it
    // does not, the per-shell time is DERIVED from the published total and the
    // base magazine with the fixed parts at zero: that reproduces the
    // published number exactly at a full magazine and scales correctly with
    // capacity, which is the behaviour that was missing. It understates a
    // PARTIAL reload by the fixed part, and this sim reloads from empty.
    //
    // A pseudo-reload (an Incarnon form's charge pool) is never by-round: it
    // is not a magazine, it is a resource that runs out.
    let by_round_reload = (s.reload_style.as_deref() == Some("by_round")
        && s.pseudo_reload.is_none())
        .then(|| {
            let start = s.reload_start_seconds.unwrap_or(0.0);
            let end = s.reload_end_seconds.unwrap_or(0.0);
            let per = s.reload_per_shell_seconds.unwrap_or_else(|| {
                ((base_reload - start - end) / magazine_size.max(1.0)).max(0.0)
            });
            (start, per, end)
        });

    let gauge_form = s.gauge_form.as_ref().map(|inc| GaugeForm {
        max_charges: inc.gauge.max_rounds,
        charge_on: match inc.gauge.charge_on.as_str() {
            "weakpoint_hits" => ChargeOn::WeakpointHits,
            "direct_hits" => ChargeOn::DirectHits,
            "kills" => ChargeOn::Kills,
            other => panic!("{id}: unknown gauge charge_on: {other}"),
        },
        charges_to_fill: inc.gauge.charges_to_fill,
        transmute_in: inc.transmute_in_seconds,
        transmute_out: inc.transmute_out_seconds,
        charge_rate: 0.0, // raised by evolutions (Incarnon Efficiency)
    });

    // The radial (AoE) attack part, when the weapon data declares one — and the
    // weapon's own SLAM, which is the same shape and a different attack: three
    // of Crushing Ruin's four combos end on one, and it is the only thing a
    // light combo has that reaches past the weapon's own reach.
    let a_radial = |r: &RadialSpec| {
        let mut v = DamageVector::new();
        for (t, val) in &r.damage {
            v.add(damage_type(t), *val);
        }
        RadialBase {
            base_vector: v,
            blast_kind: r.blast_kind,
            // Each stat falls back to the direct part's when unstated.
            base_crit_chance: r.crit_chance.unwrap_or(s.attack.crit_chance),
            base_crit_damage: r.crit_multiplier.unwrap_or(s.attack.crit_multiplier),
            base_status_chance: r.status_chance.unwrap_or(s.attack.status_chance),
            radius_m: r.radius_m,
            takes_blast_radius_mods: r.takes_blast_radius_mods,
            falloff_start_m: r.falloff_start_m.unwrap_or(0.0),
            falloff_reduction: r.falloff_reduction.unwrap_or(0.0),
            forced_procs: crate::rules::damage::ForcedProcs::from_types(
                r.forced_procs.iter().map(|t| damage_type(t)),
            ),
            takes_condition_overload: r.takes_condition_overload,
            takes_multishot: r.takes_multishot,
            // AN EXPLOSION STARTS WITH ITS OWN BASE as the CO base, and stays
            // there — see `WeaponBase::add_flat_base_damage`, where the radial's
            // never grows.
            co_base: v.total(),
        }
    };
    let radial = s.attack.radial.as_ref().map(&a_radial);
    let slam = s.attack.slam.as_ref().map(&a_radial);

    // THE BOMBLETS, both halves through the same builder — see `ClusterSpec`.
    // The CONTACT hit is a radial of one BODY RADIUS: the smallest sphere that
    // means "the body this bomblet touched and nobody else", since `falloff_at`
    // is exclusive at the edge and a zero would reach nobody at all.
    let cluster = s.attack.cluster.as_ref().map(|c| {
        let contact = a_radial(&RadialSpec {
            damage: c.damage.clone(),
            radius_m: crate::rules::space::BODY_RADIUS_M,
            blast_kind: BlastKind::default(),
            // A CONTACT HIT HAS NO RADIUS TO GROW. Firestorm pays on the
            // bomblet's explosion below and on nothing else here.
            takes_blast_radius_mods: false,
            crit_chance: c.crit_chance,
            crit_multiplier: c.crit_multiplier,
            status_chance: c.status_chance,
            falloff_start_m: None,
            falloff_reduction: None,
            forced_procs: c.forced_procs.clone(),
            takes_condition_overload: false,
            takes_multishot: false,
        });
        crate::model::ClusterBase {
            count: c.count,
            contact,
            // MULTISHOT DOES NOT RAISE THE COUNT, and two pages say so in
            // words: *"each main projectile will always produce 6 cluster
            // bombs"* (Zarr) and *"the number of bomblets generated per rocket
            // will always be 3"* (Kulstar). The module's `Multishot` on a
            // cluster attack IS that per-parent count, not a mod interaction —
            // so both halves read `false` and the count comes from `count:`.
            blast: a_radial(&RadialSpec {
                takes_multishot: false,
                takes_condition_overload: false,
                ..c.radial.clone()
            }),
        }
    });

    // The lingering FIELD (Torid's Toxin cloud). Each stat falls back to the
    // direct part's when unstated, same rule as the radial.
    let lingering = s.attack.lingering.as_ref().map(|f| {
        let mut v = DamageVector::new();
        for (t, val) in &f.damage {
            v.add(damage_type(t), *val);
        }
        LingeringBase {
            base_vector: v,
            base_crit_chance: f.crit_chance.unwrap_or(s.attack.crit_chance),
            base_crit_damage: f.crit_multiplier.unwrap_or(s.attack.crit_multiplier),
            base_status_chance: f.status_chance.unwrap_or(s.attack.status_chance),
            tick_rate: f.tick_rate,
            duration_seconds: f.duration_seconds,
            first_tick_delay_seconds: f.first_tick_delay_seconds,
            forced_procs: crate::rules::damage::ForcedProcs::from_types(
                f.forced_procs.iter().map(|t| damage_type(t)),
            ),
            radius_m: f.radius_m,
            falloff_start_m: f.falloff_start_m.unwrap_or(0.0),
            falloff_reduction: f.falloff_reduction.unwrap_or(0.0),
            takes_condition_overload: f.takes_condition_overload,
            elemental_mods_apply: f.elemental_mods_apply,
            can_crit: f.can_crit,
            status_mods_apply: f.status_mods_apply,
            stacking: match f.stacking.as_str() {
                "stack" => FieldStacking::Stack,
                "refresh" => FieldStacking::Refresh,
                other => panic!("{id}: unknown lingering stacking: {other}"),
            },
        }
    });

    WeaponBase {
        // FILLED IN BY THE EVOLUTIONS, and by nothing else: a melee Incarnon is
        // a Genesis tier rather than a weapon stat.
        melee_incarnon: None,
        // LEAKED ONCE so the panel can answer "what is this" and "what does it
        // draw" without a lookup — the two questions the Amp auras ask.
        class: Box::leak(s.class.clone().into_boxed_str()),
        slot: Box::leak(s.slot.clone().into_boxed_str()),
        // Filled in by `apply_valence`; zero until then, and zero forever on a
        // weapon that never came out of a Lich.
        valence_bonus: 0.0,
        exalted: s.exalted,
        // Zero until an evolution's flat add writes into it
        // (`add_flat_base_damage`), which is the only thing that does.
        unswung_base: 0.0,
        recharge_per_second: s.recharge_per_second,
        echo_multiplier: s.echo_multiplier,
        mod_pools: Box::leak(
            s.mod_pools
                .iter()
                .map(|p| &*Box::leak(p.clone().into_boxed_str()))
                .collect::<Vec<&'static str>>()
                .into_boxed_slice(),
        ),
        form: FormKind::parse(&s.form),
        // Empty until an evolution writes into it (data::evolutions::apply).
        indirect: Vec::new(),
        reload_damage_buff: 0.0,
        base_vector: vector,
        base_crit_chance: s.attack.crit_chance,
        base_crit_damage: s.attack.crit_multiplier,
        base_status_chance: s.attack.status_chance,
        base_fire_rate: s.attack.fire_rate,
        headshot_bonus_multiplicative: s.headshot_bonus_multiplicative,
        // Straight through: what a shot COSTS is a weapon constant, and no mod
        // in the roster changes it (ammo EFFICIENCY is its own, separate term).
        ammo_cost: s.attack.ammo_cost,
        charge_ammo_per_second: s.attack.charge_ammo_per_second,
        sustained_fire_rate: s.attack.sustained_fire_rate,
        // A BOW paces on draw + nock, every form of it (wiki Fire Rate's
        // bow-specific formula — see `AttackSpec::charge_seconds`), so a bow
        // states the draw even when it is 0.0 and anything else must not.
        // Silence here would mean falling back to the `1 / fire_rate` cadence,
        // which for a bow is the one reading the wiki rules out.
        charge_seconds: match (s.class.as_str(), s.attack.charge_seconds) {
            ("bow", Some(c)) => {
                // A zero draw makes the nock the whole cycle, which only reads
                // as a cadence while the magazine is the one nocked arrow.
                assert!(
                    c > 0.0 || s.magazine == Some(1.0),
                    "{id}: a 0.0 draw paces on the reload, so the magazine must be 1"
                );
                Some(c)
            }
            ("bow", None) => panic!("{id}: a bow's cadence is draw + nock — state charge_seconds"),
            // Every OTHER charge weapon uses the wiki's general formula,
            // which is a different sentence: "Effective Fire Rate =
            // 1 / (Modded Charge Time + 1 / Modded Fire Rate)". The listed
            // rate is not the whole cycle there — it is what happens AFTER
            // the charge completes — so the two are added, and the guard
            // guard routes this case rather than refusing it (Larkspur
            // Prime's alt-fire: 0.5 s draw + 1/2.0 s = 1.0 s per shot).
            (_, Some(c)) => {
                assert!(c > 0.0, "{id}: a 0.0 charge outside a bow is just a fire rate");
                Some(c)
            }
            (_, None) => None,
        },
        // WHICH of the wiki's two charge formulas paces this weapon. A bow's
        // draw IS its cycle; everything else pays the draw and then the
        // listed rate's interval. Carried as a fact about the weapon rather
        // than re-derived from the class in the sim, which has no business
        // knowing what a bow is.
        burst: s.attack.burst,
        charge_cadence: if s.class == "bow" {
            ChargeCadence::DrawOnly
        } else {
            ChargeCadence::DrawThenRate
        },
        // Does a FIRE-RATE bonus shorten the draw as well as the interval?
        //
        // The wiki's general charge formula says yes — "Charge Time = Base
        // Charge Time / (1 + Mod Bonus)". An ARCH-GUN is the exception: its fire rate governs only the interval between shots,
        // and the draw answers to a stat of its own. The mod cards are the
        // visible half of that split — Shell Rush is "+50% Charge Rate" where
        // Automatic Trigger is "+X% Fire Rate", and Archgun Ace grants
        // "Fire/Charge Rate", naming two things a single card would not.
        //
        // CHARGE-rate bonuses shorten the draw on every weapon; this flag is
        // only about fire rate. Kept beside `charge_cadence` because it is the
        // same kind of fact — how a weapon's draw and its rate compose — and
        // the sim has no business knowing what an Arch-Gun is.
        fire_rate_shortens_draw: s.class != "archgun",
        // "(x2 for Bows)" — the clause every fire-rate mod card carries
        // (Shred, Primed Shred, Speed Trigger, Vile Acceleration, Vigilante
        // Fervor, and the two DRAWBACK mods Critical Delay / Vile Precision,
        // so it doubles a penalty just as literally). It is keyed on the
        // weapon CLASS, not on a per-weapon flag: the cards say "for Bows",
        // and the wiki's rank tables carry a whole "Fire Rate (Bows)" column
        // at exactly twice the rifle one.
        //
        // A CROSSBOW IS A BOW FOR THIS ONE RULE. DE files it as its own class
        // and the Attica's and the Zhuge's pages both say, word for word,
        // "Counts as a bow in regards to fire rate mods, doubling the fire rate
        // bonus" — so the match is a SET rather than an equality, and the rest
        // of what `class == "bow"` decides (the draw-only charge cadence) stays
        // the bows' alone, since a crossbow does not charge.
        independent_procs: independent_procs_for(s),
        fire_rate_mod_multiplier: match s.class.as_str() {
            "bow" | "crossbow" => 2.0,
            _ => 1.0,
        },
        base_multishot: s.attack.multishot,
        buff_multishot_bonus: 0.0,
        buff_multishot_max_stacks: 0,
        magazine_size,
        // The reserve the sim may spend, and the two facts about it. HAVING
        // one is `ammo_max` — derived, because a weapon that states a reserve
        // has a reserve and there is nothing to declare twice. Being able to
        // REFILL it is the weapon's own business and is declared.
        ammo_reserve: s.ammo_max.unwrap_or(0.0),
        has_reserve: s.ammo_max.is_some_and(|a| a > 0.0),
        ammo_pickup: s.ammo_pickup.unwrap_or(0.0),
        super_crit_on_status: s.super_crit_on_status,
        weakpoint_stacks: s.weakpoint_stacks,
        spawn_on_kill: s.spawn_on_kill,
        kill_streak_summon: s.kill_streak_summon,
        tendril_max: s.tendrils.map_or(0, |t| t.max),
        tendril_range_m: s.tendrils.as_ref().map_or(0.0, |t| t.range_m),
        tendril_acquire_deg: s.tendrils.as_ref().map_or(0.0, |t| t.acquire_deg),
        sniper_combo: s.sniper_combo,
        // The scope's bonus rides HERE unconditionally and is spent by
        // `resolve`, which is the only layer that knows whether the Tenno is
        // aiming — and the only one that knows a form cannot zoom.
        scope_headshot_damage: s.scope.map_or(0.0, |z| z.headshot_damage),
        scope_crit_chance: s.scope.map_or(0.0, |z| z.crit_chance),
        scope_crit_multiplier: s.scope.map_or(0.0, |z| z.crit_multiplier),
        scope_crit_chance_post_mod: s.scope.map_or(0.0, |z| z.crit_chance_post_mod),
        // The DEFAULT lives with the ramp it belongs to, so "most weapons"
        // is stated once rather than copied into a second file that is free
        // to drift from it.
        beam_ramp_floor: s.beam_ramp_floor.unwrap_or(crate::model::BEAM_RAMP_FLOOR),
        applies_microwave: s.applies_microwave,
        battery: s.battery,
        forced_procs: s.attack.forced_procs.iter().map(|t| damage_type(t)).collect(),
        pellet_elements: s.attack.pellet_elements.iter().map(|t| damage_type(t)).collect(),
        multishot_adds_damage: s.attack.multishot_adds_damage,
        attractor_seconds: s.attack.attractor_seconds,
        no_resupply: s.no_resupply,
        base_reload,
        by_round_reload,
        innate_co_per_type: 0.0,
        gated: Vec::new(),
        tenno_scaled: Vec::new(),
        cannot_zoom: s.cannot_zoom,
        consecutive_hit_damage: None,
        consecutive_hit_radial_only: s.attack.consecutive_hit_radial_only,
        bodyshot_crit_chance_multiplier: 1.0,
        round_restore_on_status: None,
        instant_reload_on_kill: None,
        magazine_growth_on_empty_reload: None,
        evo_weakpoint_crit_chance_relative: 0.0,
        base_status_from_crit: None,
        base_crit_from_status: None,
        base_damage_below_half_health: 0.0,
        crit_chance_on_undamaged: 0.0,
        crit_damage_on_undamaged: 0.0,
        co_behavior,
        // THE ORIGINAL BASE, absolute. A weapon may DECLARE a fraction of its
        // own base here (0.5 on a bow's charged entry) and that is the only
        // place a fraction is written down, because it is how the catalog
        // prints it — everything downstream carries the absolute.
        co_base: vector.total() * s.co_base_fraction.unwrap_or(1.0),
        injected_elements,
        traits: traits_for(s),
        gauge_form,
        radial,
        cluster,
        slam,
        spread: s.attack.spread,
        // Only an EVOLUTION grants one (Lone Enforcer); no weapon declares it.
        multishot_beyond_range: None,
        falloff: s.attack.falloff.clone(),
        punch_through_m: s.attack.punch_through_m,
        projectile_width_m: s.attack.projectile_width_m,
        // ONE FACT, TWO SPELLINGS, resolved here rather than left to a reader
        // to notice. `beam.range_m` has carried a beam's reach since the block
        // existed — the Torid Incarnon's 37 m is asserted in this file's own
        // tests — and was read by nothing, so it recorded the number and
        // changed no answer. `attack.range_m` is the general form, for the
        // weapons that have a range and no `beam:` block (the Phantasma says
        // so in its own comment: *"What is lost by omitting it is the RANGE"*).
        // The explicit one wins; a beam's own is the fallback.
        range_m: s
            .attack
            .range_m
            .as_ref()
            .map(RangeSpec::metres)
            .or_else(|| s.attack.beam.as_ref().map(|b| b.range_m))
            .unwrap_or(f64::INFINITY),
        punch_through_mods: s.attack.punch_through_mods,
        compression: s.attack.compression.clone(),
        lingering,
        // The data module's Trigger for a beam. Not cosmetic: it decides
        // whether `fire_rate` means shots or TICKS and whether multishot merges.
        continuous: s.attack.trigger == "held",
        // A BOUNCE IS NOT SCALED BY ANYTHING, so it comes across as written.
        unaimed_headshot_chance: s.attack.unaimed_headshot_chance,
        windup_seconds: s.attack.windup_seconds,
        no_magazine: s.attack.no_magazine,
        combo_script: s.attack.combo_script.clone(),
        heavy: s.attack.heavy,
        // A GENESIS FILLS THESE IN, and an entry states none of them.
        evo_base_damage_bonus: 0.0,
        evo_combo_count_on_slam_hit: 0.0,
        evo_initial_combo: 0.0,
        evo_melee_range_m: 0.0,
        evo_follow_through_bonus: 0.0,
        evo_slam_radius_bonus: 0.0,
        evo_heavy_windup_speed: 0.0,
        evo_proc_conversion: None,
        follow_through: s.attack.follow_through,
        spends_combo: s.attack.spends_combo,
        combo_duration_seconds: s.combo_duration_seconds,
        orb: s.attack.orb,
        meter: s.attack.meter,
        ricochet: s.attack.ricochet.as_ref().map(|r| crate::model::Ricochet {
            bounces: r.bounces,
            headshot_chance: r.headshot_chance,
            // NO RANGE IS NOT ZERO RANGE — it is the page stating no limit but
            // the count, so an absent field means "the nearest body it has not
            // hit yet, however far".
            range_m: r.range_m.unwrap_or(f64::INFINITY),
        }),
        beam: s.attack.beam.as_ref().map(|b| crate::model::BeamGeometry {
            range_m: b.range_m,
            damage_radius_m: b.damage_radius_m,
            radius_takes_multishot: b.radius_takes_multishot,
            chain_hops: b.chain.hops,
            chain_range_m: b.chain.range_m,
            chain_damage_per_hop: b.chain.damage_per_hop,
            chain_compounds: b.chain.compounds,
            chain_takes_multishot: b.chain.takes_multishot,
            chain_nodes_have_radius: b.chain.nodes_have_radius,
            // ABSENT MEANS ONE, which is every weapon that fires where it is
            // pointed. A default of zero would read as "no beams at all".
            beams_count: b.beams.as_ref().map_or(1, |x| x.count.max(1)),
            beams_acquire_deg: b.beams.as_ref().map_or(0.0, |x| x.acquire_deg),
            beams_range_m: b.beams.as_ref().map_or(0.0, |x| x.range_m),
        }),
        field_duration_on_empty_reload: 1.0, // raised by Renewed Horror
        multishot_on_last_round: 0.0,
        base_multishot_on_last_round: 0.0,        // raised by Final Fusillade
        multishot_ammo_bonus: 0.0,           // raised by Plentiful Mayhem
        // Raised by evolutions, never by the raw weapon data.
        evo_fire_rate_bonus: 0.0,
        evo_reload_bonus: 0.0,
        rs_on_empty_reload: 0.0,
        armor_strip_per_puncture: 0.0,
        instant_reload_on_headshot: None,
        headshot_streak: None,
        crit_damage_below_status_count: None,
        // Set by Prelude of Might at `data::evolutions::apply`, read in `resolve`.
        crit_multiplier_below_crit_chance: None,
        // Set by Headcracker at `data::evolutions::apply`.
        // Filled by `data::evolutions::apply`; see `StackingBuff`.
        stacking_buffs: Vec::new(),
        post_mod_crit_chance: 0.0,
        post_mod_status_chance: 0.0,
        // Evolutions ADD to this (Caput Mortuum); a weapon's innate share is
        // the module's `ExtraHeadshotDmg`.
        headshot_damage_bonus: s.headshot_damage_bonus.unwrap_or(0.0),
        headshot_multiplier: s.headshot_multiplier,
        noncrit_bonus: None,
    }
}
