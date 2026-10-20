//! VECTIS PRIME against the owner's readings (MEASUREMENTS M117): Charged
//! Chamber's first round, the shot combo counter, and the Incarnon form's
//! embedded orb.
use super::*;
use crate::record::{Kind, Origin};

/// The panel the readings were taken on: Inciting Incident's +200 base
/// damage, Serration, and the chambers named.
fn vectis_prime(mods: &[&str]) -> crate::build::loadout::ResolvedPanel {
    vectis_prime_aimed(mods, true)
}

fn vectis_prime_aimed(mods: &[&str], aiming: bool) -> crate::build::loadout::ResolvedPanel {
    let base = crate::model::WeaponBase::from_data("vectis_prime", false, &["vectis_prime_inciting_incident"]);
    let pool = crate::data::mods::pool_for_weapon("vectis_prime");
    let cards: Vec<&crate::model::ModDef> = mods
        .iter()
        .map(|id| pool.iter().find(|m| m.id == *id).unwrap_or_else(|| panic!("{id} on the Vectis Prime")))
        .collect();
    let mut tenno = crate::data::tenno::default_tenno().clone();
    tenno.state.aiming = aiming;
    crate::build::loadout::resolve_for(&base, &cards, crate::model::StackPolicy::Emergent, &tenno)
}

/// The number the game draws: halves round up, and a product that lands a
/// hair under one (2040.4999…) is still the half it means.
fn shown(x: f64) -> f64 {
    (x + 1e-6).round()
}

/// Each trigger pull's number, summed over its damage types, non-crit body
/// shots on a dummy that takes damage as written — the reading the game pops.
fn shots(panel: &crate::build::loadout::ResolvedPanel, efficiency: f64, n: usize) -> Vec<f64> {
    shots_from(panel, efficiency, (0, false), n)
}

/// …opening with `combo` = (count, held) on the shot combo counter.
fn shots_from(
    panel: &crate::build::loadout::ResolvedPanel,
    efficiency: f64,
    combo: (u32, bool),
    n: usize,
) -> Vec<f64> {
    let arcane = ArcaneFx { ammo_efficiency: efficiency, ..ArcaneFx::none() };
    let mut p = FightParams::from_panel(panel, &crate::arena::Arena::training(30.0), &arcane);
    p.ammo_efficiency_applies = efficiency > 0.0;
    p.foe.base_health = 1e15;
    p.base_crit_chance = 0.0;
    p.status_chance = 0.0;
    p.base_status_chance = 0.0;
    p.body_parts = mono_body(1.0);
    (p.combo_initial, p.combo_held) = combo;
    let rec = record(&p, 3, 0.0, f64::INFINITY, 10_000, 0);
    let mut out: Vec<(f64, f64)> = Vec::new();
    for e in rec.events() {
        if let Kind::Damage(d) = &e.kind {
            if d.origin != Origin::Own || d.pellet.is_none() || d.radial {
                continue;
            }
            match out.last_mut() {
                Some((t, sum)) if (*t - e.t).abs() < 1e-9 => *sum += d.raw,
                _ => out.push((e.t, d.raw)),
            }
        }
    }
    assert!(out.len() >= n, "{} shots recorded", out.len());
    out.into_iter().take(n).map(|(_, s)| s).collect()
}

/// `(350 + 200) x 2.65 = 1457.5`, read 1458; the first round of each
/// magazine `x 1.4 = 2040.5`, read 2041. A two-round magazine: the bonus
/// on rounds 1 and 3, never on 2 and 4.
#[test]
fn m117_charged_chamber_is_forty_percent_on_the_first_round_beside_serration() {
    let panel = vectis_prime(&["serration", "charged_chamber"]);
    assert!((panel.first_round_damage - 0.4).abs() < 1e-9);
    let got = shots(&panel, 0.0, 4);
    for (k, want) in [2041.0, 1458.0, 2041.0, 1458.0].into_iter().enumerate() {
        assert_eq!(shown(got[k]), want, "round {}: {} vs {want} ({got:?})", k + 1, got[k]);
    }
}

/// UNDER ENERGIZED MUNITIONS (75%, a round costs 0.25) THE CHAMBER PAYS
/// ROUNDS 4 TO 7: the counter the HUD draws is the fraction rounded up, so
/// 1.0, 0.75, 0.5 and 0.25 left all read max - 1, while 1.75 to 1.25 still
/// read full and round 8 empties it. Fired from the hip, as measured, so the
/// shot combo counter stays out of the eight readings.
#[test]
fn m117_energized_munitions_pays_the_chamber_on_rounds_four_to_seven() {
    let panel = vectis_prime_aimed(&["serration", "charged_chamber"], false);
    let got: Vec<f64> = shots(&panel, 0.75, 8).into_iter().map(shown).collect();
    assert_eq!(got, [1458.0, 1458.0, 1458.0, 2041.0, 2041.0, 2041.0, 2041.0, 1458.0]);
}

/// LAVOS'S VALENCE FORMATION, +200% Heat: the shot and its heat tick, one
/// shot at a time so each stack's tick reads alone. The chamber reaches the
/// tick as it reaches the hit — `x 1.4` on both, 6122 / 3062 against
/// 4373 / 2188 — which is the wiki's "applies … to Status Damage" holding
/// for Charged Chamber as well as Primed. The tick is `(1457.5 + 1) x 0.5 x
/// 3`, the seed M113 measured.
#[test]
fn m117_the_chamber_reaches_the_heat_tick_under_valence_formation() {
    for (mods, hit, tick) in [
        (&["serration", "charged_chamber"][..], 6122.0, 3062.0),
        (&["serration"][..], 4373.0, 2188.0),
    ] {
        let mut p = FightParams::from_panel(&vectis_prime(mods), &crate::arena::Arena::training(3.0), &ArcaneFx::none());
        p.abilities = crate::data::abilities::resolve(
            &[crate::data::abilities::AbilityPick { id: "valence_formation", duration_seconds: None, element: Some("heat") }],
            &crate::data::abilities::Caster::default(),
            "",
            "melee",
        );
        p.foe.base_health = 1e15;
        p.base_crit_chance = 0.0;
        p.body_parts = mono_body(1.0);
        // ONE SHOT in the window, so the tick is that shot's stack alone.
        p.fire_rate = 0.1;
        let rec = record(&p, 3, 0.0, f64::INFINITY, 10_000, 0);
        let (mut hits, mut ticks) = (Vec::new(), Vec::new());
        for e in rec.events() {
            if let Kind::Damage(d) = &e.kind {
                match d.origin {
                    Origin::Own => hits.push(shown(d.raw)),
                    Origin::Status => ticks.push(shown(d.raw)),
                    _ => {}
                }
            }
        }
        assert_eq!(hits, [hit], "{mods:?}");
        assert!(!ticks.is_empty() && ticks.iter().all(|t| *t == tick), "{mods:?}: {ticks:?}");
    }
}

/// THE SHOT COMBO COUNTER MULTIPLIES THE CHAMBERED SHOT: at x1.5 the pair
/// reads 3061 / 2186, at x2 4081 / 2915 — each the plain pair times the tier.
#[test]
fn m117_the_combo_multiplies_the_chambered_round_and_the_plain_one() {
    let panel = vectis_prime(&["serration", "charged_chamber"]);
    for (count, want) in [(5, [3061.0, 2186.0]), (15, [4081.0, 2915.0])] {
        let got = shots_from(&panel, 0.0, (count, true), 4);
        for k in 0..4 {
            assert_eq!(shown(got[k]), want[k % 2], "combo {count}, round {}: {got:?}", k + 1);
        }
    }
}

/// FROM THE HIP THE COUNTER PAYS AND DOES NOT BUILD (M117). Opening at 14,
/// one hit short of x2: scoped in, the first hit takes it to 15 and the
/// second round reads 2915; from the hip it stays at 14 and reads 2186.
#[test]
fn m117_a_hip_fired_hit_pays_the_combo_and_adds_nothing_to_it() {
    let read = |aiming: bool| shots_from(&vectis_prime_aimed(&["serration", "charged_chamber"], aiming), 0.0, (14, false), 4);
    let (scoped, hip) = (read(true), read(false));
    assert_eq!(scoped.iter().map(|x| shown(*x)).collect::<Vec<_>>(), [3061.0, 2915.0, 4081.0, 2915.0]);
    assert_eq!(hip.iter().map(|x| shown(*x)).collect::<Vec<_>>(), [3061.0, 2186.0, 3061.0, 2186.0]);
}

/// A MISS TAKES ONE POINT, AND ONLY SCOPED IN (M117). At 25 m a 5 degree cone
/// drops some rounds; the counter opens at 45 (x2.5) and never times out, so
/// each landed hit reads `45 + hits - misses` before it — and from the hip
/// it reads 45 throughout. Reset-to-zero would put x1.0 on the first hit
/// after a miss.
#[test]
fn m117_a_scoped_miss_takes_one_point_and_a_hip_miss_takes_none() {
    use crate::record::Factor;
    let combo = crate::model::SniperCombo { min: 5, seconds: 2.0 };
    for aiming in [true, false] {
        let mut arena = crate::arena::Arena::training(20.0);
        arena.target_at = crate::rules::space::Vec2::new(0.0, 25.0);
        let panel = vectis_prime_aimed(&["serration"], aiming);
        let mut p = FightParams::from_panel(&panel, &arena, &ArcaneFx::none());
        p.projectile_speed_mps = None;
        // A CONE THAT MISSES: the Vectis Prime.s own first shot is pinpoint.
        p.spread = Some(crate::model::Spread { min_deg: 5.0, max_deg: 5.0 });
        p.foe.base_health = 1e15;
        (p.combo_initial, p.combo_held) = (45, true);
        let rec = record(&p, 3, 0.0, f64::INFINITY, 100_000, 0);
        let (mut count, mut misses, mut hits) = (45i64, 0, 0);
        for e in rec.events() {
            match &e.kind {
                Kind::Miss { .. } => {
                    misses += 1;
                    if aiming {
                        count = (count - 1).max(0);
                    }
                }
                Kind::Damage(d) if d.origin == Origin::Own && d.pellet.is_some() && !d.radial => {
                    let got: f64 = d
                        .layers
                        .iter()
                        .filter_map(|l| match l {
                            crate::record::Layer::Mul { factor: Factor::SniperCombo, value, .. } => Some(*value),
                            _ => None,
                        })
                        .product();
                    assert_eq!(got, combo.multiplier(count as u32), "aiming {aiming}, hit {hits}");
                    hits += 1;
                    if aiming {
                        count += 1;
                    }
                }
                _ => {}
            }
        }
        assert!(misses > 0 && hits > 0, "aiming {aiming}: {hits} hits, {misses} misses");
    }
}

/// One Incarnon shot at a stripped Corrupted Heavy Gunner — the readings'
/// target — with Inciting Incident and Critical Parallel, Serration and the
/// cards named. `head` aims every round at the head (x3); `crit` forces or
/// forbids the roll; `combo` opens the counter, held. Returns each number by
/// its time and origin: `(t, own?, raw)`.
fn incarnon_shot(
    mods: &[&str],
    evos: &[&str],
    aiming: bool,
    head: bool,
    crit: bool,
    combo: u32,
) -> Vec<(f64, bool, f64)> {
    let mut all = vec!["vectis_prime_evo1_incarnon_form", "vectis_prime_inciting_incident", "vectis_prime_critical_parallel"];
    all.extend_from_slice(evos);
    let base = crate::model::WeaponBase::from_data("vectis_prime_incarnon", false, &all);
    let pool = crate::data::mods::pool_for_weapon("vectis_prime");
    let cards: Vec<&crate::model::ModDef> =
        mods.iter().map(|id| pool.iter().find(|m| m.id == *id).expect("a Vectis Prime card")).collect();
    let mut tenno = crate::data::tenno::default_tenno().clone();
    tenno.state.aiming = aiming;
    let panel = crate::build::loadout::resolve_for(&base, &cards, crate::model::StackPolicy::Emergent, &tenno);
    let unit = crate::data::enemies::all().into_iter().find(|e| e.id == "corrupted_heavy_gunner").expect("the target");
    let mut p = FightParams::from_panel(&panel, &crate::arena::Arena::training(3.0), &ArcaneFx::none());
    p.foe = unit.target_params(8, false, false, TargetMode::InfiniteHealth).expect("a legal unit");
    p.foe.base_armor = 0.0;
    // EVERY PART'S OWN ROLL, fixed: the orb and its explosion carry their own.
    let cc = if crit { 1.0 } else { 0.0 };
    p.base_crit_chance = cc;
    if let Some(e) = p.embed.as_mut() {
        e.hit.crit_chance = cc;
        e.explosion.crit_chance = cc;
    }
    p.status_chance = 0.0;
    p.base_status_chance = 0.0;
    // ONE SHOT in the window, so every number is that shot's.
    p.fire_rate = 0.1;
    (p.combo_initial, p.combo_held) = (combo, true);
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
                Some((t, o, sum)) if (*t - e.t).abs() < 1e-9 && *o == own && own => *sum += d.raw,
                _ => out.push((e.t, own, d.raw)),
            }
        }
    }
    out
}

fn shown_at(got: &[(f64, bool, f64)]) -> Vec<(f64, bool, f64)> {
    got.iter().map(|&(t, own, x)| ((t * 100.0).round() / 100.0, own, shown(x))).collect()
}

/// IN A BODY THE ORB HITS THREE TIMES, a second apart — 928 and 596, then 596
/// and 596 — and the first round of a full Incarnon magazine carries the
/// chamber to all three of them (835) and to the direct hit (1299).
#[test]
fn m117_a_body_shots_orb_hits_three_times_and_carries_the_first_round() {
    let plain = shown_at(&incarnon_shot(&["serration"], &[], true, false, false, 0));
    assert_eq!(plain, [(0.0, true, 928.0), (0.0, false, 596.0), (1.0, false, 596.0), (2.0, false, 596.0)]);
    let first = shown_at(&incarnon_shot(&["serration", "charged_chamber"], &[], true, false, false, 0));
    assert_eq!(first, [(0.0, true, 1299.0), (0.0, false, 835.0), (1.0, false, 835.0), (2.0, false, 835.0)]);
}

/// IN A SCOPED HEAD THE ORB HITS ONCE AND EXPLODES 0.4 s LATER, TWICE: 4452
/// (x3 head, x1.6 scope) beside 596, then 1484 and 1484 — the explosion takes
/// the scope's bracket and not the head's x3. With Deadhead the bracket is
/// x1.9 (1762); from the hip it is x1.
#[test]
fn m117_a_head_shots_orb_explodes_twice_under_the_headshot_bracket() {
    let scoped = shown_at(&incarnon_shot(&["serration"], &[], true, true, false, 0));
    assert_eq!(scoped, [(0.0, true, 4452.0), (0.0, false, 596.0), (0.4, false, 1484.0), (0.4, false, 1484.0)]);
    let deadhead = shown_at(&incarnon_shot(&["serration"], &["vectis_prime_deadhead"], true, true, false, 0));
    assert_eq!(deadhead[2..], [(0.4, false, 1762.0), (0.4, false, 1762.0)]);
    let hip = shown_at(&incarnon_shot(&["serration"], &[], false, true, false, 0));
    assert_eq!(hip[2..], [(0.4, false, 928.0), (0.4, false, 928.0)]);
}

/// THE SHOT COMBO COUNTER PAYS EVERY PART: at x2, 8904 beside 1193, then 2968
/// and 2968. From the hip at x2 a critical explosion reads 6494 = 927.5 x 2 x
/// 3.50085 — the quantized 3.5 and no Cold, whatever the stacks.
#[test]
fn m117_the_combo_pays_every_part_and_the_explosion_crits_without_cold() {
    let x2 = shown_at(&incarnon_shot(&["serration"], &[], true, true, false, 15));
    assert_eq!(x2, [(0.0, true, 8904.0), (0.0, false, 1193.0), (0.4, false, 2968.0), (0.4, false, 2968.0)]);
    let hip = shown_at(&incarnon_shot(&["serration"], &[], false, true, true, 15));
    assert_eq!(hip[2..], [(0.4, false, 6494.0), (0.4, false, 6494.0)]);
    // A scoped one at x2: 10391 = 2968 x 3.50085, Cold or no Cold.
    let crit = shown_at(&incarnon_shot(&["serration"], &[], true, true, true, 15));
    assert_eq!(crit[2..], [(0.4, false, 10391.0), (0.4, false, 10391.0)]);
}
