//! FIRED UP ON A VERGLAS: the Sentinel card's heat pile (W`Fired_Up`).
//!
//! NO IN-GAME READING STANDS BEHIND THE COMBINATION. The wiki states that the
//! heat "will combine with elements on the weapon" and no order; the order
//! asserted here is the owner's rule — after every element on the weapon,
//! pooling into a mod of its own element — pending MEASUREMENTS §Open.
use super::*;
use crate::record::{Kind, Origin};
use crate::rules::damage::DamageType::{Blast, Cold, Electricity, Heat, Magnetic};

fn fired_up(rank: u32) -> crate::model::ElementPerHit {
    crate::data::companions::companion_mod("fired_up")
        .and_then(|m| m.element_per_hit_at(rank))
        .expect("Fired Up pays")
}

/// A Verglas with `mods`, held by a Sentinel seating Fired Up at `rank`.
fn verglas(mods: &[&str], rank: Option<u32>) -> crate::build::loadout::ResolvedPanel {
    let base = crate::model::WeaponBase::from_data("verglas", false, &[]);
    let pool = crate::data::mods::pool_for_weapon("verglas");
    let cards: Vec<&crate::model::ModDef> = mods
        .iter()
        .map(|id| pool.iter().find(|m| m.id == *id).unwrap_or_else(|| panic!("{id} on the Verglas")))
        .collect();
    let mut tenno = crate::data::tenno::sentinel_wielder().clone();
    tenno.element_per_hit = rank.map(fired_up);
    crate::build::loadout::resolve_for(&base, &cards, crate::model::StackPolicy::Emergent, &tenno)
}

/// THE LADDER: rate and ceiling climb together, so every rank fills on the
/// 20th hit; the wiki prints 0.8/1.7/2.5/3.4/4.2/5 and 16/33/50/67/84/100.
#[test]
fn every_rank_fills_on_the_twentieth_hit() {
    for (rank, per, cap) in [(0, 0.05 / 6.0, 1.0 / 6.0), (2, 0.025, 0.5), (5, 0.05, 1.0)] {
        let e = fired_up(rank);
        assert!((e.per_stack - per).abs() < 1e-12 && (e.max_bonus - cap).abs() < 1e-12, "rank {rank}: {e:?}");
        assert_eq!(e.max_stacks(), 20, "rank {rank}");
        assert!((e.bonus(20) - cap).abs() < 1e-12 && (e.bonus(400) - cap).abs() < 1e-12, "rank {rank}");
    }
    assert_eq!(fired_up(5).element, Heat);
    assert_eq!(fired_up(5).reset_seconds, 5.0);
}

/// THE PILE: a hit read at the next shot, the ceiling, and five seconds with
/// no hit emptying it.
#[test]
fn the_pile_climbs_caps_and_lapses_after_five_seconds() {
    let spec = fired_up(5);
    let mut pile = crate::fight::state::ElementPile::default();
    assert_eq!(pile.stacks_at(&spec, 0.0), 0);
    pile.settle(&spec, 1, 0.0);
    assert_eq!(pile.stacks_at(&spec, 0.1), 1);
    pile.settle(&spec, 30, 1.0);
    assert_eq!(pile.stacks_at(&spec, 1.1), 20, "29 more hits stop at the ceiling");
    assert!((pile.bonus_at(&spec, 1.1) - 1.0).abs() < 1e-12);
    assert_eq!(pile.stacks_at(&spec, 5.99), 20, "4.99 s after the last hit it stands");
    assert_eq!(pile.stacks_at(&spec, 6.0), 0, "5 s with no hit resets it");
    // …and the next hit starts again from nothing, not from 20.
    pile.settle(&spec, 31, 7.0);
    assert_eq!(pile.stacks_at(&spec, 7.0), 1);
    // A shot that landed nothing changes nothing.
    pile.settle(&spec, 31, 8.0);
    assert_eq!((pile.stacks_at(&spec, 8.0), pile.last_hit), (1, 7.0));
}

/// THE OWNER'S ORDER, at the ceiling: the heat comes after every element on
/// the weapon. Alone with the Verglas's innate Cold it makes Blast; behind an
/// Electricity mod the Cold pairs first (Magnetic) and the heat is left pure;
/// beside Hellfire it pools into the mod's heat, additive with it.
#[test]
fn at_the_ceiling_the_heat_combines_last() {
    let at_cap = |mods: &[&str]| {
        let p = verglas(mods, Some(5));
        p.per_hit_element.expect("a pile").damage[19]
    };
    let v = at_cap(&[]);
    assert!((v.get(Blast) - 52.0).abs() < 1e-9 && v.get(Cold) == 0.0 && v.get(Heat) == 0.0, "{v:?}");
    let v = at_cap(&["stormbringer"]);
    assert!((v.get(Magnetic) - 26.0 * 1.9).abs() < 1e-9, "{v:?}");
    assert!((v.get(Heat) - 26.0).abs() < 1e-9 && v.get(Electricity) == 0.0 && v.get(Blast) == 0.0, "{v:?}");
    let v = at_cap(&["hellfire"]);
    assert!((v.get(Blast) - 26.0 * 2.9).abs() < 1e-9 && v.get(Heat) == 0.0, "{v:?}");
    // …and with no card seated the panel carries no pile at all.
    assert!(verglas(&[], None).per_hit_element.is_none());
}

/// Each trigger pull's own-weapon damage events, as (type, raw) per pull.
fn pulls(panel: &crate::build::loadout::ResolvedPanel, n: usize) -> Vec<Vec<(crate::rules::damage::DamageType, f64)>> {
    let mut p = FightParams::from_panel(panel, &crate::arena::Arena::training(30.0), &ArcaneFx::none());
    p.foe.base_health = 1e15;
    p.base_crit_chance = 0.0;
    p.status_chance = 0.0;
    p.base_status_chance = 0.0;
    p.body_parts = mono_body(1.0);
    // THE BEAM AT FULL from the first tick, so every pull reads its vector.
    p.beam_ramp_floor = 1.0;
    let rec = record(&p, 3, 0.0, f64::INFINITY, 10_000, 0);
    let mut out: Vec<(f64, Vec<_>)> = Vec::new();
    for e in rec.events() {
        if let Kind::Damage(d) = &e.kind {
            if d.origin != Origin::Own || d.pellet.is_none() {
                continue;
            }
            match out.last_mut() {
                Some((t, v)) if (*t - e.t).abs() < 1e-9 => v.push((d.dtype, d.raw)),
                _ => out.push((e.t, vec![(d.dtype, d.raw)])),
            }
        }
    }
    assert!(out.len() >= n, "{} pulls recorded", out.len());
    out.into_iter().take(n).map(|(_, v)| v).collect()
}

/// IN THE FIGHT: the first pull is the bare Cold 26, each landed hit adds 5%
/// Heat that pairs with it into Blast, and the 21st pull — 20 hits behind it —
/// is the ceiling's 26 + 26 Blast. Every pull after holds there.
#[test]
fn a_verglas_reaches_the_full_hundred_percent_after_twenty_hits() {
    let got = pulls(&verglas(&[], Some(5)), 24);
    let sum = |v: &Vec<(crate::rules::damage::DamageType, f64)>| v.iter().map(|x| x.1).sum::<f64>();
    assert!(got[0].iter().all(|(t, _)| *t == Cold) && (sum(&got[0]) - 26.0).abs() < 1e-9, "{:?}", got[0]);
    for (k, pull) in got.iter().enumerate().skip(1) {
        assert!(pull.iter().all(|(t, _)| *t == Blast), "pull {}: {pull:?}", k + 1);
    }
    assert!(sum(&got[1]) > 26.0 && sum(&got[1]) < sum(&got[10]), "{got:?}");
    for pull in &got[20..] {
        assert!((sum(pull) - 52.0).abs() < 1e-9, "{pull:?}");
    }
    // …AND A VERGLAS WITHOUT THE CARD IS THE FLOOR IT WAS: Cold 26, every pull.
    for pull in pulls(&verglas(&[], None), 24) {
        assert!(pull.iter().all(|(t, _)| *t == Cold) && (sum(&pull) - 26.0).abs() < 1e-9, "{pull:?}");
    }
}
