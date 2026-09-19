/// Two arcanes on one weapon are ONE effect set — which is why the sim
/// never learns that an Arch-Gun seats two ("Archguns possess two Arcane
/// Enhancement slots to equip one Primary Arcane and one Secondary
/// Arcane", wiki Arch-Gun). Each field folds the way its own mechanic
/// does, and the merge is lossless: a buff still names the arcane that
/// granted it, so a per-buff config key does not move when a second
/// arcane joins.
#[test]
fn two_arcanes_fold_into_one_effect_set() {
    let a = ArcaneFx {
        id: "a".into(),
        crit_chance_relative: 0.3,
        reload_bonus: 0.2,
        final_multiplier: 2.0,
        buffs: vec![ArcBuffSpec {
            owner: "a".into(),
            grant: ArcGrant::BaseDamage,
            trigger: ArcTrigger::Kill,
            per_stack: 0.1,
            max_stacks: 3,
            duration: 5.0,
            all_drop: false,
            one_per_instance: false,
            initial_stacks: 3,
        }],
        ..ArcaneFx::none()
    };
    let b = ArcaneFx {
        id: "b".into(),
        crit_chance_relative: 0.5,
        final_multiplier: 3.0,
        enervate_rank: Some(5),
        ..ArcaneFx::none()
    };

    let m = ArcaneFx::merged(&[a.clone(), b.clone()]);
    assert!((m.crit_chance_relative - 0.8).abs() < 1e-9, "additive buckets SUM");
    assert!((m.reload_bonus - 0.2).abs() < 1e-9);
    assert!((m.final_multiplier - 6.0).abs() < 1e-9, "multipliers MULTIPLY");
    assert_eq!(m.enervate_rank, Some(5), "a perk is whichever states one");
    assert_eq!(m.buffs.len(), 1);
    assert_eq!(m.buffs[0].owner, "a", "the buff still names its arcane");

    // One arcane is itself, untouched — the common case must not go
    // through a fold that could round or rename anything.
    let one = ArcaneFx::merged(&[a.clone(), ArcaneFx::none()]);
    assert_eq!(one.id, "a");
    assert!((one.final_multiplier - 2.0).abs() < 1e-9);

    // None at all is none, not an identity element with a joined name.
    assert_eq!(ArcaneFx::merged(&[]).id, "");
    assert_eq!(ArcaneFx::merged(&[ArcaneFx::none()]).id, "");

    // Order does not change the result.
    let rev = ArcaneFx::merged(&[b, a]);
    assert!((rev.crit_chance_relative - m.crit_chance_relative).abs() < 1e-9);
    assert!((rev.final_multiplier - m.final_multiplier).abs() < 1e-9);
}

use super::*;

const NO_TRAITS: &[&str] = &[];

#[test]
fn loads_all_26_secondary_arcanes() {
    let pool = secondary_pool();
    // 18 ordinary secondaries plus the EIGHT Kitgun arcanes — four Pax and
    // four Residual — which are FILED here and SEATED elsewhere. Ids are
    // globally unique across slots, so a directory is a filing decision;
    // `seats` is the equip rule, and theirs is a seat of their own.
    assert_eq!(pool.len(), 26, "expected the full 26-arcane pool");
    let kit = pool.iter().filter(|a| a.seats == ["kitgun"]).count();
    assert_eq!(kit, 8, "the four Pax and four Residual arcanes");
    // …AND EVERY OTHER ARCANE IS SEATED WHERE IT IS FILED. Asserted over
    // the whole pool rather than by naming the eight, so an arcane added
    // tomorrow with a stray `seats:` is caught by nobody.
    assert!(
        pool.iter().all(|a| a.seats == ["kitgun"] || a.seats == ["secondary"]),
        "a secondary arcane is seated somewhere it is not filed"
    );
}

/// SIXTEEN, and the last two are the reason this count is worth asserting.
/// The wiki types Shotgun Vendetta as `Shotgun` and Longbow Sharpshot as
/// `Bow` rather than `Primary` — the only two class-typed arcanes in the
/// game — so an import filtering on Type == "Primary" skips exactly them,
/// which is what happened until a player noticed.
#[test]
fn loads_all_16_primary_arcanes() {
    let pool = slot_pool("primary");
    assert_eq!(pool.len(), 16, "expected the full 16-arcane primary pool");
    // The two class-typed ones are an EQUIP rule, so they are gated by
    // `equip_classes` and not offered elsewhere at all.
    for (id, class) in [("shotgun_vendetta", "shotgun"), ("longbow_sharpshot", "bow")] {
        let a = pool.iter().find(|a| a.id == id).expect(id);
        assert_eq!(a.equip_classes, vec![class], "{id} states which class equips it");
    }
    // ...and the pool a weapon is OFFERED narrows accordingly.
    let on = |w: &str| {
        pool_for_weapon(w, "primary")
            .iter()
            .map(|a| a.id.as_str())
            .collect::<Vec<_>>()
    };
    assert!(on("boar_prime").contains(&"shotgun_vendetta"), "a shotgun takes it");
    assert!(!on("boar_prime").contains(&"longbow_sharpshot"), "a shotgun is not a bow");
    assert!(on("cernos_prime").contains(&"longbow_sharpshot"), "a bow takes it");
    assert!(!on("cernos_prime").contains(&"shotgun_vendetta"), "a bow is not a shotgun");
    assert!(!on("torid").contains(&"shotgun_vendetta"), "a launcher takes neither");
    assert!(!on("torid").contains(&"longbow_sharpshot"));
    // The slot registry discovers directories, so primary must show up
    // next to secondary with no code change.
    assert!(slots().contains(&"primary"), "slots(): {:?}", slots());
}

/// An arcane is not a preference, it is a SLOT. The display lookup finds
/// one anywhere; the equipping lookup refuses one from another slot, which
/// is what stops a saved secondary build putting Cascadia Flare on a rifle.
#[test]
fn an_arcane_only_resolves_into_its_own_slot() {
    assert!(secondary("cascadia_flare").is_some(), "display lookup finds it");
    assert!(
        for_slot("secondary", "cascadia_flare").is_some(),
        "a secondary arcane resolves into the secondary slot"
    );
    assert!(
        for_slot("primary", "cascadia_flare").is_none(),
        "a secondary arcane must NOT resolve onto a primary weapon"
    );
    assert!(
        for_slot("secondary", "primary_blight").is_none(),
        "and not the other way round either"
    );
}

/// Primary Blight is the Torid-relevant one: a Toxin weapon feeds it
/// constantly. Two grants on ONE trigger, 40 stacks — the same shape as
/// Conjunction Voltage — and BOTH grants stay plain RELATIVE ratios, so
/// each attack part can scale its own base by them (the explosion's base
/// crit damage is not the direct hit's).
#[test]
fn primary_blight_stacks_crit_damage_and_multishot_on_toxin() {
    let a = slot_pool("primary")
        .iter()
        .find(|x| x.id == "primary_blight")
        .expect("primary_blight");
    let fx = a.fx(a.max_rank, StackPolicy::Emergent, NO_TRAITS, crate::data::tenno::default_tenno());
    assert_eq!(fx.buffs.len(), 2, "crit damage + multishot");
    for b in &fx.buffs {
        assert_eq!(b.trigger, ArcTrigger::ToxinStatus);
        assert_eq!(b.max_stacks, 40);
        assert!(b.all_drop, "on-status family: ALL stacks drop on timeout");
    }
    let cd = fx
        .buffs
        .iter()
        .find(|b| b.grant == ArcGrant::CritDamage)
        .expect("crit damage grant");
    // +3.6% per stack, stored RELATIVE: at 40 stacks that is +144% of
    // whichever part's base crit damage the sim is resolving.
    assert!(
        (cd.per_stack - 0.036).abs() < 1e-9,
        "per stack {} vs 0.036",
        cd.per_stack
    );
    let multishot = fx
        .buffs
        .iter()
        .find(|b| b.grant == ArcGrant::Multishot)
        .expect("multishot grant");
    assert!((multishot.per_stack - 0.018).abs() < 1e-9, "multishot stays a ratio");
}

/// Primary Crux: two grants on a weak-point HIT, 10 stacks, all-drop.
/// Both per-stack values stay plain RATIOS — the status-chance one is
/// relative to the ATTACK PART's base, which only the sim knows (the
/// explosion's differs), unlike `CritDamage`, resolved absolute here.
#[test]
fn primary_crux_grants_status_chance_and_ammo_efficiency_on_weakpoint_hits() {
    let a = secondary("primary_crux").expect("primary_crux");
    let fx = a.fx(a.max_rank, StackPolicy::Emergent, NO_TRAITS, crate::data::tenno::default_tenno());
    assert_eq!(fx.buffs.len(), 2, "status chance + ammo efficiency");
    for b in &fx.buffs {
        assert_eq!(b.trigger, ArcTrigger::WeakpointHit);
        assert_eq!((b.max_stacks, b.all_drop), (10, true));
        assert!((b.duration - 10.0).abs() < 1e-9);
    }
    let g = |grant| fx.buffs.iter().find(|b| b.grant == grant).map(|b| b.per_stack);
    assert_eq!(g(ArcGrant::StatusChance), Some(0.3));
    assert_eq!(g(ArcGrant::AmmoEfficiency), Some(0.06));
    // Rank 0 (both ramps are linear from a fifth of max).
    let fx0 = a.fx(0, StackPolicy::Emergent, NO_TRAITS, crate::data::tenno::default_tenno());
    assert!((fx0.buffs[0].per_stack - 0.05).abs() < 1e-9);
    assert!((fx0.buffs[1].per_stack - 0.01).abs() < 1e-9);
    // The static `ammo_efficiency` field belongs to the assumed-max
    // conditionals (Akimbo Slip Shot) — Crux's is a live buff, not that.
    assert_eq!(fx.ammo_efficiency, 0.0);
    assert_eq!(
        a.desc_at(5),
        "On Weak Point Hit: Gain +30% Status Chance and +6% Ammo Efficiency for 10s. Stacks up to 10x."
    );
}

/// The X-fill invariant, for the primary pool too (it holds for the
/// secondary pool above): every rank of every arcane renders.
#[test]
fn primary_desc_at_fills_every_x() {
    for a in slot_pool("primary") {
        for r in 0..=a.max_rank {
            let d = a.desc_at(r);
            assert_eq!(
                crate::model::count_x(&d),
                0,
                "{} rank {r}: unfilled X in {d:?}",
                a.id
            );
        }
    }
}

#[test]
fn merciless_resolves_kill_family_buff_plus_rank5_reload() {
    let a = secondary("secondary_merciless").unwrap();
    let fx = a.fx(5, StackPolicy::Emergent, NO_TRAITS, crate::data::tenno::default_tenno());
    let b = &fx.buffs[0];
    assert_eq!(b.trigger, ArcTrigger::Kill);
    assert_eq!(b.grant, ArcGrant::BaseDamage);
    assert!((b.per_stack - 0.30).abs() < 1e-9);
    assert_eq!(b.max_stacks, 12);
    assert!(!b.all_drop); // kill family: lose one + reset
    assert!((fx.reload_bonus - 0.30).abs() < 1e-9);
    // Rank 4: no reload passive; per-stack 25% (linear).
    let fx4 = a.fx(4, StackPolicy::Emergent, NO_TRAITS, crate::data::tenno::default_tenno());
    assert_eq!(fx4.reload_bonus, 0.0);
    assert!((fx4.buffs[0].per_stack - 0.25).abs() < 1e-9);
}

#[test]
fn deadhead_and_flare_match_the_historical_hardcoded_specs() {
    let d = secondary("secondary_deadhead")
        .unwrap()
        .fx(5, StackPolicy::Emergent, NO_TRAITS, crate::data::tenno::default_tenno());
    let b = &d.buffs[0];
    assert_eq!(b.trigger, ArcTrigger::HeadshotKill);
    assert!((b.per_stack - 1.20).abs() < 1e-9);
    assert_eq!((b.max_stacks, b.all_drop), (3, false));
    assert!((b.duration - 24.0).abs() < 1e-9);
    assert!((d.headshot_multiplier_bonus - 0.30).abs() < 1e-9);

    let fl = secondary("cascadia_flare")
        .unwrap()
        .fx(5, StackPolicy::Emergent, NO_TRAITS, crate::data::tenno::default_tenno());
    let b = &fl.buffs[0];
    assert_eq!(b.trigger, ArcTrigger::HeatStatus);
    assert!((b.per_stack - 0.12).abs() < 1e-9);
    assert_eq!((b.max_stacks, b.all_drop), (40, true));
    // Flare is the one page in this family that states the per-instance
    // cap, so it is the one entry that carries it. The other three are
    // asserted FALSE rather than left unsaid: absence of the rule on their
    // pages is not evidence of it, and a later copy-paste that spread the
    // flag would otherwise pass unnoticed.
    assert!(b.one_per_instance, "wiki: one stack per damage instance");
    for (pool, id) in [
        ("primary", "primary_blight"),
        ("primary", "primary_frostbite"),
        ("secondary", "conjunction_voltage"),
    ] {
        let a = for_slot(pool, id).unwrap_or_else(|| panic!("{id} exists"));
        let fx = a.fx(5, StackPolicy::Emergent, NO_TRAITS, crate::data::tenno::default_tenno());
        assert!(
            fx.buffs.iter().all(|b| !b.one_per_instance),
            "{id}: its page does not state the rule"
        );
    }
}

#[test]
fn nonlinear_ranks_use_the_explicit_table() {
    // Kinship is inert (team context), but Cryogenic's stack table is
    // 1,1,2,2,3,3 — rank 3 must be 2, not a lerp of 1..3.
    let c = secondary("secondary_cryogenic").unwrap();
    assert_eq!(
        c.fx(3, StackPolicy::Emergent, NO_TRAITS, crate::data::tenno::default_tenno()).cold_bursts_on_puncture,
        2
    );
}

/// EVERY `condition:` IN THE ROSTER IS ONE THIS LOADER KNOWS.
///
/// The bug this exists to prevent is not a wrong number, it is SILENCE.
/// Secondary Irradiate carried `condition: target_has_10_radiation_stacks`
/// from the day it was written, and the loader read `trigger`/`grants` and
/// the per-rank values and nothing else — so the gate was dropped without a
/// warning anywhere, the echo fired off targets with no Radiation on them,
/// and the only thing that caught it was a player noticing.
///
/// A data file that states a rule the engine does not implement is worse
/// than one that omits it: it reads, to anyone auditing, as if the rule
/// were being applied. So a string nobody has taught this loader fails
/// here, and adding one means deciding on purpose whether it is a target
/// state (simulate it) or a Tenno state (assume it, and disclose).
#[test]
fn every_condition_in_the_roster_is_known() {
    // FROM THE YAML, not from the parsed roster: an effect the loader skips
    // entirely has no parsed form to inspect, and that is precisely the
    // shape of the bug — a rule written down and never reached.
    let mut seen: Vec<(String, String)> = Vec::new();
    for (path, text) in crate::data::files_under("arcanes/") {
        let v: Value = serde_norway::from_str(text).expect("an arcane file parses");
        let Some(effects) = v.get("effects").and_then(Value::as_sequence) else {
            continue;
        };
        for e in effects {
            match arc_condition(e) {
                None => {}
                Some(ArcCondition::Unknown(s)) => panic!(
                    "{path}: condition `{s}` is not known to `arc_condition` — teach it,                          or the rule is stated in yaml and applied nowhere"
                ),
                Some(_) => seen.push((
                    path.to_string(),
                    e.get("condition")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                )),
            }
        }
    }
    // …and there IS one, so a loader that stopped finding any at all — a
    // renamed key, a parser that no longer reaches the effect list — fails
    // here rather than passing vacuously.
    assert!(
        seen.iter().any(|(_, c)| c == "target_has_10_radiation_stacks"),
        "the roster's one simulated condition is still found: {seen:?}"
    );
}

/// EVERY CONDITION IS HONOURED SOMEWHERE, and the roster is DERIVED.
///
/// There are exactly two places a condition can be honoured, and a
/// condition honoured in neither is the Secondary Irradiate bug:
///
///   · AT RESOLVE, by the POLICY. A Tenno state this sim does not model
///     must pay NOTHING under `Emergent`, which is the policy the app runs
///     by default — `fx(Emergent)` has to be indistinguishable from
///     `fx(BaseOnly)`, where no conditional arms at all. That is the claim
///     worth defending: not "the two policies differ" (Secondary Kinship's
///     stacks are zero under both, because a solo fight has no allies to
///     buff, and that is the honest answer rather than a broken gate) but
///     "an unmodelled condition never hands out its bonus for free".
///   · AT THE HIT, by the SIM. A condition about the TARGET is checked
///     when the shot lands, so the two `fx` are legitimately identical and
///     what must be non-zero is the gate the sim reads.
///
/// DERIVED, because a hand list cannot report what is not on it — which is
/// how the third arcane's broken gate sat there for six months.
#[test]
fn every_condition_is_honoured_at_resolve_or_at_the_hit() {
    let tenno = crate::data::tenno::default_tenno();
    let mut checked = 0;
    for (path, text) in crate::data::files_under("arcanes/") {
        let v: Value = serde_norway::from_str(text).expect("an arcane file parses");
        let Some(effects) = v.get("effects").and_then(Value::as_sequence) else {
            continue;
        };
        let Some(id) = v.get("id").and_then(Value::as_str) else {
            continue;
        };
        for e in effects {
            let Some(cond) = arc_condition(e) else { continue };
            let def = secondary(id).expect("a roster arcane");
            // AN ARCANE THAT DEMANDS A WEAPON TRAIT GETS IT. Akimbo Slip
            // Shot is `requires: dual_pistols` and is inert without it, so
            // asking about its condition on a weapon it cannot go on would
            // compare two zeroes and call the gate broken — which the first
            // run of this test did.
            let req = v.get("requires").and_then(Value::as_str);
            let traits: Vec<&str> = req.into_iter().collect();
            let em = def.fx(def.max_rank, StackPolicy::Emergent, &traits, tenno);
            // AGAINST NOTHING, not against another policy of the same code.
            // Comparing `Emergent` to `BaseOnly` cannot see a guard removed
            // from BOTH — the first version of this test did exactly that
            // and passed on a sabotaged build. The claim is absolute: with
            // every effect in this file conditional and none of those
            // conditions modelled, the arcane is worth what having no
            // arcane is worth.
            let none = {
                let mut n = ArcaneFx::none();
                n.id = def.id.clone();
                n
            };
            let all_conditional = effects.iter().all(|e| arc_condition(e).is_some());
            checked += 1;
            match cond {
                ArcCondition::AssumedTennoState => {
                    if all_conditional {
                        assert_eq!(
                            format!("{em:?}"),
                            format!("{none:?}"),
                            "{path}: an unmodelled Tenno-state condition pays NOTHING                                  under Emergent, which is the policy the app runs by default"
                        );
                    }
                }
                ArcCondition::TargetRadiationStacks(n) => assert_eq!(
                    em.echo_needs_radiation_stacks, n,
                    "{path}: a target-state condition must be the SIM's gate"
                ),
                ArcCondition::Unknown(s) => {
                    panic!("{path}: condition `{s}` is not known to `arc_condition`")
                }
            }
        }
    }
    // …and it FOUND them, so a loader that stopped reaching the effect list
    // fails here rather than passing over an empty roster.
    assert!(checked >= 4, "every declared condition was reached: {checked}");
}

#[test]
fn assumed_max_only_conditionals_are_emergent_noops() {
    let o = secondary("cascadia_overcharge").unwrap();
    let em = o.fx(5, StackPolicy::Emergent, NO_TRAITS, crate::data::tenno::default_tenno());
    assert_eq!(em.crit_chance_relative, 0.0);
    let am = o.fx(5, StackPolicy::AssumedMax, NO_TRAITS, crate::data::tenno::default_tenno());
    // RELATIVE now: +300% of whichever part's base the sim resolves.
    assert!((am.crit_chance_relative - 3.0).abs() < 1e-9);

    // Surge: ×8 cap under AssumedMax, no-op under Emergent.
    let s = secondary("secondary_surge").unwrap();
    assert_eq!(s.fx(5, StackPolicy::Emergent, NO_TRAITS, crate::data::tenno::default_tenno()).final_multiplier, 1.0);
    assert!((s.fx(5, StackPolicy::AssumedMax, NO_TRAITS, crate::data::tenno::default_tenno()).final_multiplier - 8.0).abs() < 1e-9);
}

#[test]
fn requires_gates_akimbo_on_the_dual_pistols_trait() {
    let a = secondary("akimbo_slip_shot").unwrap();
    let off = a.fx(5, StackPolicy::AssumedMax, NO_TRAITS, crate::data::tenno::default_tenno());
    assert_eq!(off.ammo_efficiency, 0.0);
    let on = a.fx(5, StackPolicy::AssumedMax, &["dual_pistols"], crate::data::tenno::default_tenno());
    assert!((on.ammo_efficiency - 0.65).abs() < 1e-9);
}

#[test]
fn shiver_fortifier_encumber_empowered_cryogenic_resolve() {
    let sh = secondary("secondary_shiver")
        .unwrap()
        .fx(5, StackPolicy::Emergent, NO_TRAITS, crate::data::tenno::default_tenno());
    assert!((sh.per_cold_base_damage - 0.45).abs() < 1e-9);
    assert_eq!(sh.cold_cap, 10);
    let ft = secondary("secondary_fortifier")
        .unwrap()
        .fx(5, StackPolicy::Emergent, NO_TRAITS, crate::data::tenno::default_tenno());
    // ×9, not ×8: the card's "x8" is the EXTRA (M38,).
    assert!((ft.overguard_multiplier - 9.0).abs() < 1e-9);
    let en = secondary("secondary_encumber")
        .unwrap()
        .fx(5, StackPolicy::Emergent, NO_TRAITS, crate::data::tenno::default_tenno());
    assert!((en.encumber_chance - 0.24).abs() < 1e-9);
    let em = secondary("cascadia_empowered")
        .unwrap()
        .fx(5, StackPolicy::Emergent, NO_TRAITS, crate::data::tenno::default_tenno());
    assert!((em.flat_damage_on_status - 750.0).abs() < 1e-9);
    // Voltage: two status-family buffs sharing the 40-stack pool.
    let cv = secondary("conjunction_voltage")
        .unwrap()
        .fx(5, StackPolicy::Emergent, NO_TRAITS, crate::data::tenno::default_tenno());
    assert_eq!(cv.buffs.len(), 2);
    assert!(cv.buffs.iter().all(|b| b.all_drop && b.max_stacks == 40));
}

#[test]
fn desc_at_fills_every_x_for_the_whole_pool() {
    for a in secondary_pool() {
        for r in 0..=a.max_rank {
            let d = a.desc_at(r);
            assert_eq!(
                crate::model::count_x(&d),
                0,
                "{} rank {r}: unfilled X in {d:?}",
                a.id
            );
        }
    }
}

#[test]
fn desc_at_spot_checks() {
    let d = |id: &str, r: u32| secondary(id).unwrap().desc_at(r);
    // Linear percent fill.
    assert_eq!(d("secondary_merciless", 5), "On Kill:\n+30% Damage for 4s. Stacks up to 12x.\n+30% Reload Speed");
    assert_eq!(d("secondary_merciless", 0), "On Kill:\n+5% Damage for 4s. Stacks up to 12x.\n+30% Reload Speed");
    // Flare: per-stack AND the derived stack cap (per × 40).
    assert_eq!(d("cascadia_flare", 0), "On Heat Status Effect:\n+2% Damage for 10s. Stacks up to 80%.");
    assert_eq!(d("cascadia_flare", 5), "On Heat Status Effect:\n+12% Damage for 10s. Stacks up to 480%.");
    // Voltage: two Xs from the two buffs, in order.
    assert_eq!(d("conjunction_voltage", 5), "On Electricity Status Effect:\n+1.5% Reload Speed and +3% Multishot for 12s. Stacks up to 40x.");
    // Outburst: cc + cd collapse onto the single X (non-linear table).
    assert_eq!(d("secondary_outburst", 3), "On swapping to Secondary Weapon, consume all Combo Multipliers to increase Secondary Weapon Critical Chance and Critical Damage by 12% per Combo consumed for 30s.");
    // Cryogenic: non-linear stack table + radius lerp ("X ... Xm").
    assert_eq!(d("secondary_cryogenic", 2), "On Puncture: Apply 2 Cold stacks on targets within 12m.");
    // Multiplier form xX: stored bonus renders as the multiplier.
    assert_eq!(d("secondary_surge", 5), "On Ability Cast: Next shot gains a Damage Multiplier for every 200 current Energy, up to x8.");
    assert_eq!(d("secondary_fortifier", 0), "Gain 1 Overguard for every 100 Damage dealt to an enemy's Overguard.\nDeals x3 Extra Damage to Overguard.");
    // Enervate: the perk's reset threshold is the only varying number.
    assert_eq!(d("secondary_enervate", 3), "On Hit: Increase Critical Chance by 10%. Resets after 4 Big Critical Hit.");
    // Flat (non-%) fill.
    assert_eq!(d("cascadia_empowered", 5), "On Status Effect:\nDeals +750 Damage matching the Damage Type of the Status Effect");
}

#[test]
fn enervate_runs_as_the_perk() {
    let e = secondary("secondary_enervate")
        .unwrap()
        .fx(3, StackPolicy::Emergent, NO_TRAITS, crate::data::tenno::default_tenno());
    assert_eq!(e.enervate_rank, Some(3));
    assert!(e.buffs.is_empty()); // the on_hit buff is perk-implemented
}
/// A WARFRAME STAT reaches the weapon, and nothing else does.
///
/// Primary Bulwark and Primary Overcharge were both `unmodeled` because
/// "the value depends on the Warframe, which a weapon calc has no model
/// of". The fight now carries a Tenno, so it does: the arcane reads the
/// stat off it, and the NEUTRAL Tenno — no frame, no pool — makes both
/// resolve to nothing, which is the honest answer rather than a zero
/// invented to dodge the question.
#[test]
fn an_arcane_that_scales_off_a_warframe_reads_the_fights_tenno() {
    let bulwark = for_slot("primary", "primary_bulwark").expect("primary_bulwark");
    let overcharge = for_slot("primary", "primary_overcharge").expect("primary_overcharge");
    let frame = |armor: f64, energy: f64, pct: f64| {
        let mut t = crate::data::tenno::default_tenno().clone();
        t.armor = armor;
        t.energy = energy;
        t.state.energy_pct = pct;
        t
    };
    let base_damage = |t: &crate::data::tenno::Tenno| {
        bulwark
            .fx(5, StackPolicy::Emergent, NO_TRAITS, t)
            .buffs
            .iter()
            .map(|b| b.per_stack)
            .sum::<f64>()
    };
    let multishot = |t: &crate::data::tenno::Tenno| {
        overcharge
            .fx(5, StackPolicy::Emergent, NO_TRAITS, t)
            .buffs
            .iter()
            .map(|b| b.per_stack)
            .sum::<f64>()
    };

    // THE NEUTRAL FRAME IS THE FLOOR OF EVERY RELEASED ONE, not a blank, so these two arcanes now answer rather than
    // abstaining — which is the point of pinning it.
    let neutral = crate::data::tenno::default_tenno();
    // Bulwark pays past 1,000 armor and the floor is 105, so it is still
    // silent — and silent means NO buff, not a buff worth zero: a zero
    // stack would list in the picker and invite someone to turn it up.
    assert!(bulwark.fx(5, StackPolicy::Emergent, NO_TRAITS, neutral).buffs.is_empty());
    // Overcharge reads the POOL, and the floor of that pool is ZERO: four
    // frames have no energy at all — Hildryn and Lavos pay for their
    // abilities out of shields and with cooldowns — so "the weakest frame in
    // the game" has none of it. A floor of 150 is a floor no frame sets —
    // it comes of reading a zero as missing data rather than as the
    // value.
    //
    // SILENT MEANS NO BUFF, not a buff worth zero — the same rule Bulwark
    // follows one line above, and for the same reason: a zero stack would
    // list in the picker and invite someone to turn it up.
    assert!(
        overcharge.fx(5, StackPolicy::Emergent, NO_TRAITS, neutral).buffs.is_empty(),
        "no pool, no multishot"
    );
    // …and it pays the moment a fight says which frame is holding the gun.
    assert!((multishot(&frame(0.0, 150.0, 1.0)) - 0.525).abs() < 1e-9,
        "{}", multishot(&frame(0.0, 150.0, 1.0)));

    // Bulwark: +1% per point PAST 1,000 — so 1,000 armor still pays
    // nothing, 1,200 pays +200%, and the rank-5 cap of +500% is reached at
    // 1,500 and never exceeded.
    assert!(bulwark.fx(5, StackPolicy::Emergent, NO_TRAITS, &frame(1000.0, 0.0, 1.0)).buffs.is_empty());
    assert!((base_damage(&frame(1200.0, 0.0, 1.0)) - 2.0).abs() < 1e-9);
    assert!((base_damage(&frame(1500.0, 0.0, 1.0)) - 5.0).abs() < 1e-9);
    assert!((base_damage(&frame(9000.0, 0.0, 1.0)) - 5.0).abs() < 1e-9, "capped at +500%");
    // It is a BASE DAMAGE grant, pinned at its one stack: a Warframe stat
    // does not decay mid-fight, and no event grants it.
    let b = &bulwark.fx(5, StackPolicy::Emergent, NO_TRAITS, &frame(1200.0, 0.0, 1.0)).buffs[0];
    assert_eq!(b.grant, ArcGrant::BaseDamage);
    assert_eq!(b.trigger, ArcTrigger::Passive);
    assert_eq!((b.max_stacks, b.initial_stacks), (1, 1));
    assert_eq!(b.duration, crate::model::NO_TIMEOUT, "a stat has no clock");

    // Overcharge: 35% of MAX energy, and the gate is on how FULL the pool
    // is — 300 energy at 100% pays +105%, the same frame at 50% pays
    // nothing, and the cap needs 1,000 energy.
    assert!((multishot(&frame(0.0, 300.0, 1.0)) - 1.05).abs() < 1e-9);
    assert!((multishot(&frame(0.0, 300.0, 0.9)) - 1.05).abs() < 1e-9, "at exactly 90%");
    assert!(overcharge.fx(5, StackPolicy::Emergent, NO_TRAITS, &frame(0.0, 300.0, 0.5)).buffs.is_empty());
    assert!((multishot(&frame(0.0, 1000.0, 1.0)) - 3.5).abs() < 1e-9);
    assert!((multishot(&frame(0.0, 5000.0, 1.0)) - 3.5).abs() < 1e-9, "capped at +350%");
    assert_eq!(
        overcharge.fx(5, StackPolicy::Emergent, NO_TRAITS, &frame(0.0, 300.0, 1.0)).buffs[0].grant,
        ArcGrant::Multishot
    );

    // A SENTINEL fires under BaseOnly, where no conditional arms — there is
    // no Tenno standing behind a companion's gun either.
    assert!(bulwark.fx(5, StackPolicy::BaseOnly, NO_TRAITS, &frame(1500.0, 0.0, 1.0)).buffs.is_empty());
}
