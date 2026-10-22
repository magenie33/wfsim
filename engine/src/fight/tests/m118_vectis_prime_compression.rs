//! VECTIS PRIME against the owner's readings (MEASUREMENTS M118): Primary
//! Compression on the Incarnon form, Galvanized Aptitude's term on the round
//! and its orb, and the shot combo counter counting a hit before paying it.
use super::*;
use crate::record::{Kind, Origin};

/// What each part of a shot rolls: the round, the orb's hits, the explosion.
#[derive(Clone, Copy)]
struct Crits {
    direct: bool,
    orb: bool,
    boom: bool,
}

const NONE: Crits = Crits { direct: false, orb: false, boom: false };

/// The Heavy Gunner fight the readings were taken in, stripped of armour.
/// `compression` equips a rank-5 Primary Compression; `co` is Galvanized
/// Aptitude's rate per status type (two stacks: 0.8); `gap` is the seconds
/// between rounds, so a second round finds the first one's Cold. Returns
/// every number by its time and whose it is: `(t, own?, raw)`.
#[allow(clippy::too_many_arguments)]
fn fight(
    weapon: &str,
    evos: &[&str],
    mods: &[&str],
    aiming: bool,
    zoom: f64,
    compression: bool,
    head: bool,
    crits: Crits,
    combo: (u32, bool),
    co: f64,
    gap: f64,
) -> Vec<(f64, bool, f64)> {
    let base = crate::model::WeaponBase::from_data(weapon, false, evos);
    let family = weapon.trim_end_matches("_incarnon");
    let pool = crate::data::mods::pool_for_weapon(family);
    let cards: Vec<&crate::model::ModDef> =
        mods.iter().map(|id| pool.iter().find(|m| m.id == *id).expect("a card for this weapon")).collect();
    let mut tenno = crate::data::tenno::default_tenno().clone();
    tenno.state.aiming = aiming;
    tenno.state.scope_zoom = zoom;
    let panel = crate::build::loadout::resolve_for(&base, &cards, crate::model::StackPolicy::Emergent, &tenno);
    let arcane = if compression {
        crate::data::arcanes::for_slot("primary", "primary_compression")
            .expect("the arcane")
            .fx(5, crate::model::StackPolicy::Emergent, &[], &tenno)
    } else {
        ArcaneFx::none()
    };
    let unit = crate::data::enemies::all().into_iter().find(|e| e.id == "corrupted_heavy_gunner").expect("the target");
    let mut p = FightParams::from_panel(&panel, &crate::arena::Arena::training(3.0), &arcane);
    p.foe = unit.target_params(8, false, false, TargetMode::InfiniteHealth).expect("a legal unit");
    p.foe.base_armor = 0.0;
    p.base_crit_chance = if crits.direct { 1.0 } else { 0.0 };
    if let Some(e) = p.embed.as_mut() {
        e.hit.crit_chance = if crits.orb { 1.0 } else { 0.0 };
        e.explosion.crit_chance = if crits.boom { 1.0 } else { 0.0 };
    }
    p.status_chance = 0.0;
    p.base_status_chance = 0.0;
    p.co_per_type = co;
    p.fire_rate = 1.0 / gap;
    (p.combo_initial, p.combo_held) = combo;
    let part = unit.body_parts.iter().find(|b| b.is_head == head).expect("the part");
    p.body_parts = vec![BodyPart {
        name: part.name.clone(),
        aim_weight: 1.0,
        multiplier: part.multiplier,
        is_head: head,
        is_weak_point: head,
        crit_bonus: part.crit_bonus,
    }];
    let rec = record(&p, 3, 0.0, f64::INFINITY, 10_000, 0);
    let mut out: Vec<(f64, bool, f64)> = Vec::new();
    for e in rec.events() {
        if let Kind::Damage(d) = &e.kind {
            let own = match d.origin {
                Origin::Own => true,
                Origin::Embed => false,
                _ => continue,
            };
            match out.last_mut() {
                Some((t, o, sum)) if (*t - e.t).abs() < 1e-9 && *o == own && own => *sum += d.effective,
                _ => out.push((e.t, own, d.effective)),
            }
        }
    }
    out.into_iter().map(|(t, own, x)| ((t * 100.0).round() / 100.0, own, (x + 1e-6).round())).collect()
}

const INCARNON: [&str; 3] =
    ["vectis_prime_evo1_incarnon_form", "vectis_prime_inciting_incident", "vectis_prime_critical_parallel"];

/// The Incarnon form, scoped, Serration, two stacks of Galvanized Aptitude.
fn incarnon(zoom: f64, compression: bool, head: bool, crits: Crits, combo: u32, gap: f64) -> Vec<(f64, bool, f64)> {
    fight(
        "vectis_prime_incarnon", &INCARNON, &["serration"], true, zoom, compression, head, crits, (combo, true),
        0.8, gap,
    )
}

/// What landed in `[from, from + 1)`.
fn from(got: &[(f64, bool, f64)], t: f64) -> Vec<(f64, bool, f64)> {
    got.iter().copied().filter(|&(at, _, _)| at >= t - 1e-9 && at < t + 1.0 - 1e-9).collect()
}

/// A BODY SHOT ON A CLEAN TARGET: 3637 (crit) beside 668, then 788 and 788.
/// The round and its orb's first hit count nothing — the round's own Cold is
/// not yet on the target — and the orb's later hits take 150 x 0.8 = 120
/// outside the compression's x1.12: 788 = 596.25 x 1.12 + 120.
#[test]
fn m118_the_orbs_later_hits_take_the_rounds_galvanized_term_outside_compression() {
    let got = incarnon(0.0, true, false, Crits { direct: true, ..NONE }, 0, 10.0);
    assert_eq!(got, [(0.0, true, 3637.0), (0.0, false, 668.0), (1.0, false, 788.0), (2.0, false, 788.0)]);
}

/// THE NEXT ROUND FINDS THE COLD: 1159 = 927.5 x 1.12 + 120, beside its orb's
/// 788 — compression does not reach the Adding term (inside it, 1173).
#[test]
fn m118_a_round_on_a_cold_target_reads_1159_beside_788() {
    let got = incarnon(0.0, true, false, NONE, 0, 1.5);
    assert_eq!(from(&got, 1.5)[..2], [(1.5, true, 1159.0), (1.5, false, 788.0)]);
}

/// A SCOPED HEADSHOT AT x2 UNDER COMPRESSION: 11124 beside the orb's 5516
/// (crit), then the explosion 2968 x 2 — the scope's x1.6 and the combo, no
/// compression and no Galvanized term. At 3.5x the bracket is x1.4: 9734, 2597.
#[test]
fn m118_the_explosion_takes_the_scope_and_not_the_compression() {
    let crits = Crits { orb: true, ..NONE };
    let six = from(&incarnon(0.0, true, true, crits, 15, 1.5), 1.5);
    assert_eq!(six, [(1.5, true, 11124.0), (1.5, false, 5516.0), (1.9, false, 2968.0), (1.9, false, 2968.0)]);
    let low = from(&incarnon(3.5, true, true, crits, 15, 1.5), 1.5);
    assert_eq!(low, [(1.5, true, 9734.0), (1.5, false, 5516.0), (1.9, false, 2597.0), (1.9, false, 2597.0)]);
}

/// WITHOUT COMPRESSION the orb's term is the same 120: 10056 beside 5015
/// (crit), the explosion 2968.
#[test]
fn m118_without_compression_the_orb_takes_the_same_term() {
    let got = from(&incarnon(0.0, false, true, Crits { orb: true, ..NONE }, 15, 1.5), 1.5);
    assert_eq!(got, [(1.5, true, 10056.0), (1.5, false, 5015.0), (1.9, false, 2968.0), (1.9, false, 2968.0)]);
}

/// THE SHOT COMBO COUNTS A HIT BEFORE PAYING IT: the Rubico Prime's Minimum
/// Combo is 1, so from an empty counter its first scoped hit pays x1.5 — 311 —
/// where the hip, which builds nothing, reads 207 (26 / 5 / 2 units of 187/32,
/// Puncture x1.5 on the Corrupted).
#[test]
fn m118_the_rubico_primes_first_scoped_hit_pays_its_own_count() {
    let shot = |aiming: bool| {
        fight("rubico_prime", &[], &[], aiming, 0.0, false, false, NONE, (0, false), 0.0, 10.0)[0]
    };
    assert_eq!(shot(false), (0.0, true, 207.0));
    assert_eq!(shot(true), (0.0, true, 311.0));
}
