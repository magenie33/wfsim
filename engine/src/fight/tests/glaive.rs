//! A GLAIVE'S THROW — docs/MELEE.md §10.
use super::*;
use crate::data::apl::{Action, Apl, Rule, When};
use crate::rules::space::Vec2;

/// One throw form, played with or without `detonate` on its list, `gap_m` from
/// the target and with `others` standing where they are given.
fn throw_among(form: &str, mods: &[&str], detonate: bool, gap_m: f64, others: &[Vec2]) -> Summary {
    monte_carlo(&throw_params(form, mods, detonate, gap_m, others), 20, 7)
}

fn throw_params(form: &str, mods: &[&str], detonate: bool, gap_m: f64, others: &[Vec2]) -> FightParams {
    throw_params_with_arcane(form, mods, detonate, gap_m, others, None)
}

fn throw_params_with_arcane(form: &str, mods: &[&str], detonate: bool, gap_m: f64, others: &[Vec2], arcane: Option<&str>) -> FightParams {
    let base = crate::model::WeaponBase::from_data(form, true, &[]);
    let pool = crate::data::mods::pool_for_weapon(form);
    let refs: Vec<&crate::model::ModDef> = mods
        .iter()
        .map(|id| pool.iter().find(|m| m.id == *id).unwrap_or_else(|| panic!("{id} not in the pool")))
        .collect();
    let panel = crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
    let mut arena = crate::arena::Arena::training(60.0);
    arena.target_at = Vec2::new(0.0, crate::rules::space::CONTACT_RANGE_M + gap_m);
    if detonate {
        arena.apl = Apl(vec![Rule { action: Action::Detonate, when: When::Always }]);
    }
    let fx = arcane.map_or_else(crate::data::arcanes::ArcaneFx::none, |id| {
        let card = crate::data::arcanes::pool_for_weapon(form, "melee")
            .into_iter().find(|a| a.id == id).expect("the weapon can equip the arcane");
        card.fx(card.max_rank, crate::model::StackPolicy::Emergent, base.traits,
            crate::data::tenno::default_tenno())
    });
    let mut p = FightParams::for_panel(&panel, &arena, &fx, || panel.clone());
    p.others = others
        .iter()
        .map(|&at| crate::formation::FoeSpec {
            id: String::new(),
            params: p.foe.clone(),
            body_parts: p.body_parts.clone(),
            at,
        })
        .collect();
    p
}

fn throw(form: &str, mods: &[&str], detonate: bool, gap_m: f64) -> Summary {
    throw_among(form, mods, detonate, gap_m, &[])
}

/// **A THROW NOBODY DETONATES EXPLODES NOT AT ALL**, and `detonate` is the
/// only difference between the two: the explosion is several times the throw.
#[test]
fn the_explosion_goes_off_only_when_the_list_detonates() {
    let left = throw("glaive_prime_charged_throw", &[], false, 0.0);
    let set_off = throw("glaive_prime_charged_throw", &[], true, 0.0);
    // 360 thrown against 360 + 1184 Blast — less than its face, since the
    // throw's forced Slash bleeds in both.
    let ratio = set_off.damage_per_shot / left.damage_per_shot;
    assert!(ratio > 1.8, "detonated / left {ratio}");
}

/// **THE DETONATION TAKES THE COUNTER AND THE THROW DOES NOT** — *"Only
/// explosive damage will use the Combo Multiplier"* (wiki, Melee). Corrupt
/// Charge's +30 initial combo is a 2x floor, worth nothing to a throw left to
/// come back and close to double to one that is set off.
#[test]
fn initial_combo_pays_the_detonation_and_not_the_throw() {
    let plain = throw("glaive_prime_charged_throw", &[], false, 0.0).damage_per_shot;
    let plain_floor = throw("glaive_prime_charged_throw", &["corrupt_charge"], false, 0.0).damage_per_shot;
    assert!((plain_floor / plain - 1.0).abs() < 0.02, "a throw left to return reads the counter: {}", plain_floor / plain);
    let boom = throw("glaive_prime_charged_throw", &[], true, 0.0).damage_per_shot;
    let boom_floor = throw("glaive_prime_charged_throw", &["corrupt_charge"], true, 0.0).damage_per_shot;
    assert!(boom_floor / boom > 1.5, "the detonation ignores the counter: {}", boom_floor / boom);
}

/// **KILLING BLOW REACHES THE EXPLOSION AND NOT THE THROW** — *"increase the
/// damage of their mid-air explosion, but will not affect the damage of the
/// thrown weapon itself"* (wiki). Its damage half, that is: its wind-up half
/// shortens the charge, so the comparison is per THROW, never per second.
#[test]
fn killing_blow_pays_the_explosion_alone() {
    // WHAT THE EXPLOSION ADDS to a throw, with and without the card: the
    // throw's own share (and the bleeds it forces, which a faster loop leaves
    // ticking into more throws) cancels out of the difference.
    let plain = throw("glaive_prime_charged_throw", &[], false, 0.0).damage_per_shot;
    let kb = throw("glaive_prime_charged_throw", &["killing_blow"], false, 0.0).damage_per_shot;
    assert!((kb / plain - 1.0).abs() < 0.06, "Killing Blow reached the throw: {}", kb / plain);
    let boom = throw("glaive_prime_charged_throw", &[], true, 0.0).damage_per_shot - plain;
    let boom_kb = throw("glaive_prime_charged_throw", &["killing_blow"], true, 0.0).damage_per_shot - kb;
    assert!(boom_kb / boom > 1.25, "Killing Blow missed the explosion: {}", boom_kb / boom);
}

/// **A GLAIVE IN THE AIR HOLDS THE NEXT THROW**: one detonated for its flight
/// out to the target and back, one left to return for its flight out to where it turns
/// and back. Glaive Prime's quick throw: 0.8 s a throw, 40 m/s, 30 m of range.
#[test]
fn the_glaive_in_the_air_holds_the_next_throw() {
    let at_hand = throw("glaive_prime_throw", &[], true, 0.0).mean_shots;
    // M129: 20 m out and back at 40 m/s adds one second.
    let far_set_off = throw("glaive_prime_throw", &[], true, 20.0).mean_shots;
    assert!((at_hand / far_set_off - 1.8 / 0.8).abs() < 0.1, "out and back: {at_hand} / {far_set_off}");
    // ITS PUNCH THROUGH CARRIES IT THROUGH THE TARGET, and with no wall here it
    // flies its whole 30 m before it turns: 1.5 s out and back.
    let passed = throw("glaive_prime_throw", &[], false, 0.0).mean_shots;
    assert!((at_hand / passed - 2.3 / 0.8).abs() < 0.15, "its range and back: {at_hand} / {passed}");
    // …AND ONE THAT CANNOT PASS turns at the target: 20 m out and back, 1 s.
    let bounced = throw("glaive_prime_throw", &["volatile_rebound"], false, 20.0).mean_shots;
    assert!((at_hand / bounced - 1.8 / 0.8).abs() < 0.1, "out to the body and back: {at_hand} / {bounced}");
}

/// **WHIRLWIND SPEEDS THE FLIGHT AND STRETCHES THE RANGE TOGETHER** — *"As a
/// direct result, this mod also increases throwing distance"* (wiki). So a
/// glaive that flies its whole range comes back no sooner, and one detonated
/// at a distant target does.
#[test]
fn whirlwind_is_worth_time_only_where_the_glaive_turns_short() {
    let passed = throw("glaive_prime_throw", &[], false, 0.0).mean_shots;
    let passed_ww = throw("glaive_prime_throw", &["whirlwind"], false, 0.0).mean_shots;
    assert!((passed_ww / passed - 1.0).abs() < 0.03, "its range grew with its speed: {passed} / {passed_ww}");
    let far = throw("glaive_prime_throw", &[], true, 20.0).mean_shots;
    let far_ww = throw("glaive_prime_throw", &["whirlwind"], true, 20.0).mean_shots;
    // One second out and back becomes 1.0 / 2.8.
    assert!((far_ww / far - 1.8 / (0.8 + 1.0 / 2.8)).abs() < 0.1, "a faster flight out: {far} / {far_ww}");
}

/// **POWER THROW PAYS THE THROW AND NOT ITS EXPLOSION**: *"On Consecutive
/// throw (Max stacks 3): +100% Throw Damage"*, and a loop buffers every throw,
/// so all but the first carry up to +300% — sixteen throws a minute here, so
/// `(0 + 1 + 2 + 3 x 13) / 16 + 1` = 3.6x on the hits.
#[test]
fn power_throw_pays_the_thrown_glaive_alone() {
    let hits = |s: &Summary| s.mean_damage - s.mean_dot_damage;
    let left = throw("glaive_prime_charged_throw", &[], false, 0.0);
    let left_pt = throw("glaive_prime_charged_throw", &["power_throw"], false, 0.0);
    assert_eq!(left.mean_shots, left_pt.mean_shots);
    let expected = 4.0 - 6.0 / left.mean_shots;
    assert!((hits(&left_pt) / hits(&left) - expected).abs() < 0.1, "three stacks on the throw: {}", hits(&left_pt) / hits(&left));
    let plain = left.damage_per_shot;
    let pt = left_pt.damage_per_shot;
    let boom = throw("glaive_prime_charged_throw", &[], true, 0.0).damage_per_shot - plain;
    let boom_pt = throw("glaive_prime_charged_throw", &["power_throw"], true, 0.0).damage_per_shot - pt;
    assert!((boom_pt / boom - 1.0).abs() < 0.15, "Power Throw reached the explosion: {}", boom_pt / boom);
}

/// **A VOLATILE CARD TAKES THE PUNCH THROUGH AND EXPLODES ON THE BOUNCE**:
/// the glaive turns off the body it was thrown into, and that turn explodes.
#[test]
fn a_volatile_glaive_explodes_off_the_body_it_cannot_pass() {
    // Left to return, so the only explosion is the bounce's: 296 Blast beside
    // the throw's 180, on the hits (the bleeds the throw forces are in both),
    // and less than that face since Blast and Slash meet the target differently.
    let hits = |s: &Summary| (s.mean_damage - s.mean_dot_damage) / s.mean_shots;
    let plain = throw("glaive_prime_throw", &[], false, 0.0);
    let volatile = throw("glaive_prime_throw", &["volatile_rebound"], false, 0.0);
    let ratio = hits(&volatile) / hits(&plain);
    assert!(ratio > 1.5, "the bounce explosion went off: {ratio}");
}

/// **A BOUNCE GOES WHERE REFLECTION TAKES IT, AND THE COUNT IS A CEILING** —
/// three bounces into a ring of bodies reach bodies the explosion at the target
/// does not, and Quick Return's none reach no one.
#[test]
fn rebound_bounces_into_the_crowd_and_quick_return_turns_home() {
    let t = Vec2::new(0.0, crate::rules::space::CONTACT_RANGE_M + 8.0);
    // A RING 7 m OUT, past the 4.8 m explosion (7.8 m with Volatile Quick
    // Return's +3), so only a bounce reaches it.
    let ring: Vec<Vec2> = (0..36)
        .map(|i| {
            let a = f64::from(i) * std::f64::consts::TAU / 36.0;
            Vec2::new(t.x + 8.5 * a.cos(), t.y + 8.5 * a.sin())
        })
        .collect();
    let others = |s: &Summary| s.mean_damage_by_body.0[1..].iter().sum::<f64>();
    let rebound = throw_among("glaive_prime_charged_throw", &["volatile_rebound", "rebound"], false, 8.0, &ring);
    let quick = throw_among("glaive_prime_charged_throw", &["volatile_quick_return"], false, 8.0, &ring);
    assert!(others(&rebound) > 0.0, "the bounces found the ring");
    assert_eq!(others(&quick), 0.0, "no bounce after the first");
}

/// **A DETONATABLE THROW IS ITS OWN MODE**, the throw's form with one more line
/// on the list — and a quick throw that is not a strike has no form at all.
#[test]
fn a_detonatable_throw_is_played_both_ways() {
    let ids = |w: &str| {
        crate::data::weapons::play_modes(w).iter().map(|m| (m.id, m.detonates)).collect::<Vec<_>>()
    };
    let gp = ids("glaive_prime");
    for want in [("throw", false), ("throw_detonate", true), ("charged_throw", false), ("charged_throw_detonate", true)] {
        assert!(gp.contains(&want), "glaive_prime: {want:?} in {gp:?}");
    }
    let orvius = ids("orvius");
    assert!(!orvius.iter().any(|(id, _)| *id == "throw"), "orvius: {orvius:?}");
    let mode = crate::data::weapons::play_modes("glaive_prime").into_iter().find(|m| m.id == "throw_detonate").unwrap();
    assert_eq!(mode.rules().to_simc(), "detonate");
}

/// Package projectile/recall procs; the bounce retains its own wiki rule.
#[test]
fn every_glaive_keeps_each_instances_own_forced_procs() {
    use DamageType::{Cold, Electricity, Impact, Slash, Toxin, Viral};
    type Procs<'a> = (&'a str, &'a [DamageType], &'a [DamageType], &'a [DamageType]);
    let cases: &[Procs<'_>] = &[
        ("cerata", &[], &[Impact], &[Impact, Toxin]),
        ("coda_pathocyst", &[], &[], &[Impact, Viral]),
        ("falcor", &[Impact], &[Electricity], &[Electricity]),
        ("glaive", &[], &[Impact], &[Impact]),
        ("glaive_prime", &[Impact, Slash], &[Impact, Slash], &[Impact, Slash]),
        ("halikar", &[Impact], &[Impact], &[Impact]),
        ("halikar_wraith", &[Impact], &[Impact], &[Impact]),
        ("kestrel", &[], &[], &[Impact]),
        ("kestrel_prime", &[], &[], &[]),
        ("orvius", &[], &[Impact], &[Cold]),
        ("pathocyst", &[], &[], &[Impact, Viral]),
        ("xoris", &[Impact], &[Impact], &[Impact, Electricity]),
    ];
    for &(weapon, quick, charged, recall) in cases {
        for (suffix, direct) in [("throw", quick), ("charged_throw", charged)] {
            if weapon == "orvius" && suffix == "throw" {
                continue;
            }
            let id = format!("{weapon}_{suffix}");
            let p = throw_params(&id, &[], true, 0.0, &[]);
            let swing = p.combo_script.first().cloned();
            let (types, _) = swing.as_ref().unwrap().split_forced();
            let mut buf = [Impact; DamageType::ALL.len()];
            let n = forced_procs(&p, &None, true, &swing, &types, &[], 0, &mut buf);
            buf[..n].sort_by_key(|ty| *ty as usize);
            assert_eq!(&buf[..n], direct, "{id}: direct");
            let n = forced_procs(&p, &p.radial, false, &swing, &types, &[], 0, &mut buf);
            assert_eq!(&buf[..n], recall, "{id}: recall");
            assert_eq!(p.radial.unwrap().forced_procs.independent("knockdown"), weapon == "kestrel_prime");
            let bounce = p.thrown_bounce.unwrap().explosion;
            let n = forced_procs(&p, &Some(bounce), false, &swing, &types, &[], 0, &mut buf);
            let expected: &[DamageType] = if weapon == "glaive_prime" && suffix == "throw" { &[Impact] } else { &[] };
            assert_eq!(&buf[..n], expected, "{id}: bounce must not inherit the recall's procs");
        }
    }
}

#[test]
fn glaive_projectile_recall_and_bounce_numbers_match_the_pinned_package() {
    let raw: serde_norway::Value = serde_norway::from_str(include_str!(
        "../../../../tests/fixtures/melee/package-2026-10-08.json")).unwrap();
    let mut checked = 0;
    for s in crate::data::weapons::all().iter().filter(|s| s.form_kind().is_throw()) {
        let path = format!("weapons/melee/{}.yaml", s.group());
        let metadata: serde_norway::Value = serde_norway::from_str(crate::data::file(&path).unwrap()).unwrap();
        let name = metadata["internal_name"].as_str().unwrap();
        let projectile = &raw["weapons"][name]["glaive"][if s.form_kind() == crate::model::FormKind::ChargedThrow { "turbo" } else { "projectile" }];
        let near = |got: f64, key: &str| {
            let want = projectile[key].as_f64().unwrap();
            assert!((got - want).abs() < 1e-4, "{} {key}: {got} != {want}", s.id);
        };
        near(s.attack.damage.values().sum(), "amount");
        near(s.attack.crit_chance, "crit");
        near(s.attack.status_chance, "procChance");
        near(s.attack.projectile_speed_mps.unwrap(), "speed");
        let recall = s.attack.radial.as_ref().unwrap();
        let want = projectile["explosion"].as_f64().unwrap() * projectile["heavyRecallAtten"].as_f64().unwrap();
        assert!((recall.damage.values().sum::<f64>() - want).abs() < 1e-4, "{} recall", s.id);
        near(s.attack.thrown_bounce.as_ref().unwrap().explosion.damage.values().sum(), "explosion");
        if s.form_kind() == crate::model::FormKind::ChargedThrow {
            assert!(s.attack.combo_script.iter().all(|h| h.event_timing.is_some()));
            assert_eq!(s.attack.combo_script.last().unwrap().next_swing_index, Some(1));
        }
        checked += 1;
    }
    assert_eq!(checked, 23);
}

#[test]
fn a_kestrel_prime_recall_knocks_down_without_giving_its_collision_that_proc() {
    let damage = |detonate, co| {
        let mods: &[&str] = if co { &["condition_overload"] } else { &[] };
        let mut p = throw_params("kestrel_prime_charged_throw", mods, detonate, 0.0, &[]);
        p.duration_seconds = 1.0;
        p.fire_rate = 10.0;
        p.status_chance = 0.0;
        p.base_status_chance = 0.0;
        p.base_crit_chance = 0.0;
        for h in &mut p.combo_script { h.windup_seconds = 0.0; }
        if let Some(r) = &mut p.radial {
            r.status_chance = 0.0;
            r.base_status_chance = 0.0;
            r.crit_chance = 0.0;
            r.base_crit_chance = 0.0;
        }
        monte_carlo(&p, 1, 7).mean_damage
    };
    let collision = damage(false, true) / damage(false, false);
    assert!((collision - 1.0).abs() < 1e-9, "collision forced a status: x{collision}");
    let recall = damage(true, true) / damage(true, false);
    assert!(recall > 1.1, "the recall's independent Knockdown never fed CO: x{recall}");
}

/// Forced Electricity survives elemental combinations and zero random status;
/// Falcor's charged collision and Xoris's recall can both open Influence.
#[test]
fn a_glaives_forced_electricity_opens_influence_without_electric_damage() {
    for (form, detonate) in [("falcor_charged_throw", false), ("xoris_charged_throw", true)] {
        let mut p = throw_params_with_arcane(form, &["fever_strike"], detonate, 0.0, &[], Some("melee_influence"));
        assert_eq!(p.arcane.influence_chance, 0.2, "{form}: the equipped arcane must survive the weapon trait gate");
        p.status_chance = 0.0;
        p.base_status_chance = 0.0;
        if let Some(r) = &mut p.radial {
            r.status_chance = 0.0;
            r.base_status_chance = 0.0;
            r.crit_chance = 0.0;
            assert_eq!(r.damage.get(DamageType::Electricity), 0.0);
        }
        assert_eq!(p.damage.get(DamageType::Electricity), 0.0);
        let rec = record(&p, 7, 0.0, f64::INFINITY, 10_000, 0);
        assert!(rec.events().iter().any(|e| matches!(&e.kind,
            crate::record::Kind::Damage(d) if d.procs.contains(&DamageType::Electricity)
        )), "{form}: the forced proc disappeared with the damage type");
        // A rolled Corrosive proc makes the open window observable even when
        // Electricity's own share of the hit is zero.
        p.status_chance = 1.0;
        if let Some(r) = &mut p.radial {
            r.status_chance = 1.0;
        }
        let spread = record(&p, 7, 0.0, f64::INFINITY, 10_000, 0);
        assert!(spread.events().iter().any(|e| matches!(&e.kind,
            crate::record::Kind::Damage(d) if d.origin == crate::record::Origin::Influence
        )), "{form}: forced Electricity did not open Influence");
        for hit in &mut p.combo_script {
            hit.forced_procs.retain(|ty| ty != "electricity");
        }
        p.forced_procs.retain(|ty| *ty != DamageType::Electricity);
        if let Some(r) = &mut p.radial {
            let mut buf = [DamageType::Impact; DamageType::ALL.len()];
            let n = r.forced_procs.fill(&mut buf);
            r.forced_procs = crate::rules::damage::ForcedProcs::from_types(
                buf[..n].iter().copied().filter(|ty| *ty != DamageType::Electricity),
            );
        }
        let control = record(&p, 7, 0.0, f64::INFINITY, 10_000, 0);
        assert!(!control.events().iter().any(|e| matches!(&e.kind,
            crate::record::Kind::Damage(d) if d.origin == crate::record::Origin::Influence
        )), "{form}: Influence opened without an Electricity proc");
    }
}

/// M129: charge uses wind-up speed, release uses attack speed, and flight
/// uses projectile speed.
#[test]
fn a_throw_keeps_its_charge_and_release_on_separate_clocks() {
    for form in ["glaive_prime_throw", "glaive_prime_charged_throw"] {
        let plain = throw_params(form, &[], true, 0.0, &[]);
        let fury = throw_params(form, &["fury"], true, 0.0, &[]);
        let windup = throw_params(form, &["killing_blow"], true, 0.0, &[]);
        let a = &plain.combo_script[0];
        let b = &windup.combo_script[0];
        assert_eq!(a.delay_seconds, b.delay_seconds, "{form}: wind-up speed changed the release animation");
        if a.windup_seconds > 0.0 {
            assert!((a.windup_seconds / b.windup_seconds - 1.6).abs() < 1e-9);
        }
        assert_eq!(a.windup_seconds, fury.combo_script[0].windup_seconds);
        assert_eq!(a.delay_seconds, fury.combo_script[0].delay_seconds);
        let bare = monte_carlo(&plain, 1, 7);
        let fast = monte_carlo(&fury, 1, 7);
        assert!(fast.mean_shots > bare.mean_shots, "{form}: Fury did not shorten the release animation");
    }
}

/// M129: a recall is an immediate input, with the same return flight as a
/// detonation and no explosion or heavy-attack combo spend.
#[test]
fn a_glaive_recall_returns_without_detonating() {
    let mut p = throw_params("glaive_prime_throw", &[], false, 20.0, &[]);
    let natural = monte_carlo(&p, 1, 7);
    p.apl_inserted = Apl(vec![Rule { action: Action::Recall, when: When::Always }]);
    let recalled = monte_carlo(&p, 1, 7);
    let detonated = throw("glaive_prime_throw", &[], true, 20.0);
    assert_eq!(recalled.mean_shots, detonated.mean_shots, "both inputs return at the outbound speed");
    assert!(recalled.mean_shots > natural.mean_shots, "recall must turn before the full range");
    assert!(detonated.damage_per_shot > recalled.damage_per_shot, "recall must not detonate");
    let mode = crate::data::weapons::play_modes("glaive_prime").into_iter()
        .find(|m| m.id == "throw").unwrap();
    assert_eq!(mode.rules().to_simc(), "recall");
}
