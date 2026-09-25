
/// THE GOD ROLL IS THE DEFAULT, AND A FIGHT IS ASKED ONLY WHERE THE SIGN HAS
/// STOPPED ANSWERING. These pin the machinery before any weapon is involved:
/// the ends, which stats are asked about, and what it takes to move one.
#[test]
fn a_card_is_the_god_roll_unless_a_fight_can_prove_otherwise() {
let plain = RivenShape {
    bonuses: vec!["critical_damage".into(), "multishot".into(), "damage".into()],
    malus: Some("critical_chance".into()),
};
assert_eq!(plain.stat_count(), 4);

// EVERY BONUS AT ITS CEILING, THE MALUS AT ITS FLOOR — the card a player
// would want, and what all but a few hundred builds are stored with.
let god = god_roll(&plain, "rifle");
assert!(god.bonuses.iter().all(|b| b.roll == ROLL_MAX));
assert_eq!(god.malus.as_ref().unwrap().roll, ROLL_MIN);
// …at the ceiling of its investment, like every board row.
assert_eq!(god.rank, MAX_RANK);

// A CRIT MALUS IS NOT AMBIGUOUS ON ITS OWN. What makes it so is the WEAPON
// — one that pays for not critting, or whose crit multiplier is granted only
// below a crit chance threshold. The Lex Prime does neither, so nothing
// here is asked; the Braton Prime has a row for that threshold and the same
// shape on it names one.
let none = ambiguous_stats(&plain, "lex_prime");
assert!(none.is_empty(), "{none:?}");
assert_eq!(
    ambiguous_stats(&plain, "braton_prime").into_iter().collect::<Vec<_>>(),
    vec!["critical_chance".to_string()],
    "the same shape, on a weapon whose row names that stat"
);
let mut asked = 0;
let out = best_roll(&plain, "rifle", &none, |_| {
    asked += 1;
    Some((1.0, 0.0))
});
assert_eq!(asked, 0, "a shape with nothing ambiguous never reaches a fight");
assert_eq!(rolls(&out), rolls(&god));

// A PHYSICAL STAT IS ALWAYS AMBIGUOUS, on every weapon: it moves the SHARE
// each damage type holds of the total, and a status proc is drawn in
// proportion to that share. The Torid's own row names `magazine_capacity`,
// which this shape does not carry — a row only speaks about its stats.
let phys = RivenShape {
    bonuses: vec!["impact".into(), "damage".into()],
    malus: Some("zoom".into()),
};
assert_eq!(
    ambiguous_stats(&phys, "torid").into_iter().collect::<Vec<_>>(),
    vec!["impact".to_string()],
    "the physical stat, and only it"
);

// …AND ONLY THAT STAT LEAVES ITS END. The fight below wants everything at
// its floor; `damage` and `zoom` are not asked, so they do not move.
let asked_set = ambiguous_stats(&phys, "torid");
let low = best_roll(&phys, "rifle", &asked_set, |r| {
    Some((-r.bonuses.iter().map(|b| b.roll).sum::<f64>(), 0.0))
});
assert_eq!(rolls(&low), vec![ROLL_MIN, ROLL_MAX, ROLL_MIN], "{:?}", rolls(&low));

// A GAP THE RULER COULD NOT SEE IS NOT A GAP. The other end scores higher
// and the difference is inside the published measurement's own standard
// error, so the player keeps the better card — the same rule as a stat the
// fight cannot read at all, which returns an identical number twice.
let draw = best_roll(&phys, "rifle", &asked_set, |r| {
    let flipped = r.bonuses[0].roll == ROLL_MIN;
    Some((if flipped { 1.001 } else { 1.0 }, 0.01))
});
assert_eq!(rolls(&draw), rolls(&god_roll(&phys, "rifle")));
// …and the same gap against a sharper ruler DOES move it.
let seen = best_roll(&phys, "rifle", &asked_set, |r| {
    let flipped = r.bonuses[0].roll == ROLL_MIN;
    Some((if flipped { 1.001 } else { 1.0 }, 0.0001))
});
assert_eq!(rolls(&seen), vec![ROLL_MIN, ROLL_MAX, ROLL_MIN]);

// A FIGHT THAT DID NOT RUN LEAVES THE GOD ROLL STANDING.
let dead = best_roll(&phys, "rifle", &asked_set, |_| None);
assert_eq!(rolls(&dead), rolls(&god_roll(&phys, "rifle")));
}

/// AN ALTERNATIVE REPLACES THE DEFAULT ONLY BY MORE THAN TWO STANDARD ERRORS,
/// and then the best such one wins; a default that did not run stands.
#[test]
fn perfect_keeps_the_default_until_a_fight_separates_them() {
let table = |c: &u32| match c {
    0 => Some((10.0, 1.0)),
    1 => Some((12.0, 1.0)),  // inside 2·√2 of the default
    2 => Some((13.5, 1.0)),  // outside it
    3 => Some((15.0, 1.0)),  // outside it, and the best
    _ => None,
};
assert_eq!(perfect(0, [1], table), 0);
assert_eq!(perfect(0, [1, 2, 3, 9], table), 3);
assert_eq!(perfect(9, [3], table), 9, "a default that did not run stands");
assert_eq!(perfect(0, [], |_: &u32| unreachable!("nothing to compare, nothing run")), 0);
}

/// A STATUS DURATION MALUS IS ASKED AT ITS TWO ENDS, on any weapon. A best
/// inside the band (the edge of the -100% cliff) is not searched for.
#[test]
fn a_status_duration_malus_is_asked_at_its_ends_only() {
let shape = RivenShape {
    bonuses: vec!["damage".into(), "multishot".into()],
    malus: Some("status_duration".into()),
};
let asked = ambiguous_stats(&shape, "lex_prime");
assert_eq!(asked.iter().cloned().collect::<Vec<_>>(), vec!["status_duration".to_string()]);

// Rising with depth up to 1.04, then off the cliff: the deep end loses.
let mut seen = Vec::new();
let out = best_roll(&shape, "rifle", &asked, |r| {
    let m = r.malus.as_ref().unwrap().roll;
    seen.push(m);
    Some((if m > 1.045 { 0.0 } else { m }, 0.001))
});
assert_eq!(seen, vec![ROLL_MIN, ROLL_MAX], "the god roll and the other end");
assert_eq!(rolls(&out), vec![ROLL_MAX, ROLL_MAX, ROLL_MIN]);
}

/// Bonuses then the malus — the order `RivenShape::at` reads them back in.
#[cfg(test)]
fn rolls(spec: &RivenSpec) -> Vec<f64> {
spec.bonuses.iter().map(|b| b.roll).chain(spec.malus.iter().map(|m| m.roll)).collect()
}

/// A SHAPE IS THE LUCK REMOVED, and two rolls of one shape are one shape.
#[test]
fn a_shape_is_what_survives_when_the_roll_is_taken_away() {
let mk = |a: f64, b: f64| RivenSpec {
    class: "rifle".into(),
    bonuses: vec![
        RolledStat { id: "multishot".into(), roll: a },
        RolledStat { id: "damage".into(), roll: b },
    ],
    malus: Some(RolledStat { id: "recoil".into(), roll: a }),
    rank: 8,
    polarity: Polarity::Madurai,
};
assert_eq!(RivenShape::of(&mk(0.91, 1.07)), RivenShape::of(&mk(1.10, 0.90)));
// SORTED, because a riven's stats do not combine with each other — two
// players listing them in different orders described one riven.
let shape = RivenShape::of(&mk(1.0, 1.0));
assert_eq!(shape.bonuses, vec!["damage".to_string(), "multishot".to_string()]);
}

/// AN ELEMENTAL RIVEN PAIRS WITH THE BUILD, so the shape has to be able to say
/// which elements it brings — where it sits among the mods then changes the
/// combined element and therefore the fight.
#[test]
fn a_shape_names_the_elements_it_brings() {
let none = RivenShape { bonuses: vec!["multishot".into(), "damage".into()], malus: None };
assert!(none.elements("rifle").is_empty());

let heat = RivenShape {
    bonuses: vec!["heat".into(), "multishot".into()],
    malus: Some("recoil".into()),
};
assert_eq!(heat.elements("rifle"), vec![crate::rules::damage::DamageType::Heat]);

// TWO OF THEM IS LEGAL and the pair is what a board row has to keep apart
// from one: a riven bringing Heat AND Toxin pools two entries into the
// element sequence, not one.
let two = RivenShape {
    bonuses: vec!["cold".into(), "heat".into(), "multishot".into()],
    malus: None,
};
assert_eq!(two.elements("rifle").len(), 2);

// A PHYSICAL stat is not an element and never pairs.
let phys = RivenShape { bonuses: vec!["slash".into(), "damage".into()], malus: None };
assert!(phys.elements("rifle").is_empty());
}

/// NO RIVEN TAKES AN ELEMENT AS ITS MALUS, which is what lets `elements()` read
/// the bonuses alone. Asserted against the pool rather than assumed, so a data
/// change that made one malus-legal fails here instead of silently dropping an
/// element out of the pairing.
#[test]
fn an_element_is_never_a_malus() {
for class in crate::data::mods::classes() {
    for st in pool(class) {
        if st.kind == "elemental_damage_bonus" {
            assert!(!st.malus, "{class}/{}: an element may be a malus", st.id);
        }
    }
}
}
use super::*;

fn spec(ids: &[&str], malus: Option<&str>, rank: u32) -> RivenSpec {
    RivenSpec {
        class: "rifle".into(),
        bonuses: ids
            .iter()
            .map(|id| RolledStat { id: (*id).into(), roll: 1.0 })
            .collect(),
        malus: malus.map(|id| RolledStat { id: id.into(), roll: 1.0 }),
        rank,
        polarity: Polarity::Madurai,
    }
}

#[test]
fn both_pools_load() {
    // The ROLLED pools; the spliced stats are `spliced.rs`'s.
    let rolled = |c: &str| pool(c).iter().filter(|x| !x.spliced).count();
    assert_eq!(rolled("rifle"), 24);
    assert_eq!(rolled("pistol"), 24);
    assert_eq!(rolled("melee"), 24);
    assert!(pool("nonexistent").is_empty());
}

/// A MELEE WEAPON ROLLS THE MELEE POOL, and the pool is a different item
/// rather than the rifle's with a few rows crossed out: eleven of its
/// stats exist in no gun pool and eight gun stats exist in none of it.
#[test]
fn a_melee_weapon_rolls_a_pool_of_its_own() {
    assert_eq!(class_for_weapon("magistar"), Some("melee"));
    let ids = |c: &str| {
        pool(c).iter().map(|s| s.id.as_str()).collect::<std::collections::BTreeSet<_>>()
    };
    let (m, r) = (ids("melee"), ids("rifle"));
    for id in ["combo_duration", "initial_combo", "heavy_attack_efficiency", "range"] {
        assert!(m.contains(id) && !r.contains(id), "{id} is melee's own");
    }
    for id in ["multishot", "magazine_capacity", "reload_speed", "ammo_maximum", "zoom"] {
        assert!(!m.contains(id) && r.contains(id), "{id} is a gun's own");
    }
}

/// …AND THE WIKI'S MELEE COLUMN AGREES, on the same ugly entries the rifle
/// check uses: 164.7 and 73.44 are not round, and 24.5 is not a percentage
/// at all.
#[test]
fn the_wikis_melee_base_column_agrees() {
    let by = |id: &str| pool("melee").iter().find(|x| x.id == id).unwrap();
    for (id, wiki) in [
        ("melee_damage", 164.70),
        ("critical_chance", 180.00),
        ("critical_damage", 90.00),
        ("attack_speed", 54.90),
        ("heavy_attack_efficiency", 73.44),
        ("critical_chance_for_slide_attack", 120.00),
        ("additional_combo_count_chance", 58.77),
    ] {
        let ours = by(id).base * 90.0 * 100.0;
        assert!((ours - wiki).abs() < 0.01, "{id}: DE base x 90 = {ours:.4}, wiki publishes {wiki}");
    }
    // The three the wiki prints in the stat's OWN unit rather than as a
    // percentage — seconds, metres and combo points.
    for (id, wiki) in [("combo_duration", 8.1), ("range", 1.94), ("initial_combo", 24.5)] {
        let ours = by(id).base * 90.0;
        assert!((ours - wiki).abs() < 0.005, "{id}: DE base x 90 = {ours:.4}, wiki publishes {wiki}");
    }
}

/// A MALUS-ONLY STAT KEEPS THE SIGN DE GAVE IT, and Weapon Recoil is the
/// contrast that makes the rule visible: both bases are negative and only
/// one of them flips in the malus slot.
///
/// The band comes from live Magistar listings (disposition 1.35, rank 8,
/// three bonuses and a malus), which read -96.8 to -112.4 across eight
/// cards — `0.01165 x 90 x 1.35 x 0.75` is 106.2% at roll 1.0, so the
/// whole 0.9-1.1 band is 95.6 to 116.8 and every card sits inside it.
#[test]
fn a_malus_only_stat_is_negative_where_recoil_turns_positive() {
    let melee = |id: &str| pool("melee").iter().find(|x| x.id == id).unwrap();
    let s = RivenSpec {
        class: "melee".into(),
        ..spec(&["melee_damage", "critical_chance", "critical_damage"],
               Some("chance_to_gain_combo_count"), 8)
    };
    let v = s.value_of(melee("chance_to_gain_combo_count"), 1.0, false, 1.35);
    assert!((v + 1.0616).abs() < 5e-4, "the malus reads {v:.4}, the cards read -0.968 to -1.124");

    // …AND THE OTHER NEGATIVE BASE DOES FLIP. Its bonus is the good one, so
    // the malus slot is where it turns into recoil the weapon gains.
    let r = spec(&["damage", "critical_chance", "multishot"], Some("weapon_recoil"), 8);
    let rec = pool("rifle").iter().find(|x| x.id == "weapon_recoil").unwrap();
    assert!(r.value_of(rec, 1.0, true, 1.0) < 0.0, "the bonus reads -90% recoil");
    assert!(r.value_of(rec, 1.0, false, 1.0) > 0.0, "the malus reads +67.5% recoil");
}

/// …AND IT CANNOT BE A BONUS, which is the other half of the same fact.
/// Its partner stat is the bonus-only one, so the pair covers both refusals.
#[test]
fn the_combo_count_pair_rolls_one_slot_each() {
    let bonus_slot = RivenSpec {
        class: "melee".into(),
        ..spec(&["chance_to_gain_combo_count", "critical_chance"], None, 8)
    };
    assert!(
        bonus_slot.illegal().iter().any(|x| x.contains("malus-only")),
        "{:?}", bonus_slot.illegal()
    );
    let malus_slot = RivenSpec {
        class: "melee".into(),
        ..spec(&["melee_damage", "critical_chance"], Some("additional_combo_count_chance"), 8)
    };
    assert!(
        malus_slot.illegal().iter().any(|x| x.contains("bonus-only")),
        "{:?}", malus_slot.illegal()
    );
}

/// EVERY MELEE STAT LANDS IN THE BUCKET ITS MOD ALREADY LANDS IN — the
/// riven's Critical Chance carries True Steel's "(x2 for Heavy Attacks)",
/// its Range is Reach's flat metres, and its Finisher Damage pays nothing
/// because a finisher is an animation this arena has none of.
#[test]
fn a_melee_rivens_stats_reach_the_melee_buckets() {
    let by = |id: &str| pool("melee").iter().find(|x| x.id == id).unwrap();
    assert!(matches!(effect_of(by("critical_chance"), 1.0), Some(ModEffect::CritChanceHeavyDoubled(_))));
    assert!(matches!(effect_of(by("critical_chance_for_slide_attack"), 1.0), Some(ModEffect::CritChanceOnSlide(_))));
    assert!(matches!(effect_of(by("range"), 1.94), Some(ModEffect::MeleeRange(_))));
    assert!(matches!(effect_of(by("combo_duration"), 8.1), Some(ModEffect::MeleeComboDuration(_))));
    assert!(matches!(effect_of(by("initial_combo"), 24.5), Some(ModEffect::InitialCombo(_))));
    assert!(matches!(effect_of(by("heavy_attack_efficiency"), 0.73), Some(ModEffect::HeavyAttackEfficiency(_))));
    assert!(matches!(effect_of(by("additional_combo_count_chance"), 0.58), Some(ModEffect::ComboCountChance(_))));
    // …AND ITS MALUS TWIN IS A DIFFERENT MECHANIC, a gate (MEASUREMENTS M97).
    assert!(matches!(effect_of(by("chance_to_gain_combo_count"), -0.57), Some(ModEffect::ComboGainChance(_))));
    assert!(effect_of(by("finisher_damage"), 1.2).is_none(), "a finisher is out of this arena");
}

/// A MELEE CARD PRINTS ITS OWN UNITS. Combo Duration is seconds and Range
/// is metres, so neither may be printed as a percentage — and the unit
/// sits between the hole and the name, which is what `|val|s` means.
#[test]
fn a_melee_card_prints_seconds_and_metres() {
    let by = |id: &str| pool("melee").iter().find(|x| x.id == id).unwrap();
    assert_eq!(by("combo_duration").print(8.1), "+8.1s Combo Duration");
    assert_eq!(by("range").print(1.94), "+1.9 Range");
    assert_eq!(by("melee_damage").print(1.647), "+164.7% Melee Damage");
}

/// The scale is 90 at rank 8, and TWO sources say so. DE's export gives
/// the base; the wiki publishes its own "base value" column, and that
/// column IS `base x 90`. The check is the UGLY entries — 149.99 and
/// 60.03 are not round, so matching them to four figures is not luck.
///
/// Note what this test does NOT do: it does not apply a config
/// multiplier. These are BASE values, reached before the shape is known,
/// which is precisely why their roundness says nothing about whether the
/// two-bonus multiplier is 0.99 or 1.0.
#[test]
fn the_scale_is_90_and_the_wikis_base_column_agrees() {
    let by = |id: &str| pool("rifle").iter().find(|x| x.id == id).unwrap();
    for (id, wiki_pct) in [
        ("damage", 165.00),
        ("critical_chance", 149.99),
        ("fire_rate", 60.03),
        ("critical_damage", 120.00),
        ("multishot", 90.00),
    ] {
        let ours = by(id).base * 90.0 * 100.0;
        assert!(
            (ours - wiki_pct).abs() < 0.01,
            "{id}: DE base x 90 = {ours:.4}%, wiki publishes {wiki_pct}%"
        );
    }
}

/// Rank scales the value linearly in `(rank + 1) / 9` — rank 0 is a ninth
/// of rank 8, which is what the wiki's worked example shows.
#[test]
fn rank_scales_by_one_ninth_steps() {
    let by = |id: &str| pool("rifle").iter().find(|x| x.id == id).unwrap();
    let at = |r: u32| spec(&["damage", "multishot"], None, r).value_of(by("damage"), 1.0, true, 1.3);
    let full = at(8);
    assert!((at(0) - full / 9.0).abs() < 1e-9, "rank 0 is a ninth of rank 8");
    assert!((at(4) - full * 5.0 / 9.0).abs() < 1e-9);
}

/// The malus flips the sign, and a third bonus costs every stat 25%.
#[test]
fn the_shape_moves_every_stat() {
    let by = |id: &str| pool("rifle").iter().find(|x| x.id == id).unwrap();
    let two = spec(&["damage", "multishot"], None, 8);
    let two_with_malus = spec(&["damage", "multishot"], Some("weapon_recoil"), 8);
    let three = spec(&["damage", "multishot", "critical_chance"], None, 8);

    let d = |s: &RivenSpec| s.value_of(by("damage"), 1.0, true, 1.0);
    assert!((d(&two) - 1.65 * 0.99).abs() < 5e-4);
    // A malus pays the bonuses exactly 25%, in both shapes.
    assert!((d(&two_with_malus) - 1.65 * 0.99 * 1.25).abs() < 5e-4);
    assert!((d(&three) - 1.65 * 0.75).abs() < 5e-4, "a third bonus costs 25%");
    let three_with_malus = spec(&["damage", "multishot", "critical_chance"], Some("weapon_recoil"), 8);
    assert!((d(&three_with_malus) - 1.65 * 0.75 * 1.25).abs() < 5e-4);

    // The malus itself: negative multiplier, so the stat inverts.
    let c = two_with_malus.value_of(by("multishot"), 1.0, false, 1.0);
    assert!(c < 0.0, "a malus is negative: {c}");
    assert!((c + 0.90 * 0.495).abs() < 5e-4);
}

/// Disposition is the WEAPON's, so one riven reads differently on two.
#[test]
fn disposition_scales_the_whole_riven() {
    let by = |id: &str| pool("rifle").iter().find(|x| x.id == id).unwrap();
    let s = spec(&["damage", "multishot"], None, 8);
    let torid = s.value_of(by("damage"), 1.0, true, 1.3);
    assert!((torid - 1.65 * 0.99 * 1.3).abs() < 5e-4, "Torid at 1.3: {torid:.3}");
}

#[test]
fn a_riven_is_a_mod_the_rest_of_the_engine_understands() {
    let s = spec(&["damage", "multishot"], Some("weapon_recoil"), 8);
    let m = s.to_mod_def("riven_test", 1.3);
    assert_eq!(m.base_drain, 18, "rank 8 costs 18");
    assert_eq!(m.max_rank, MAX_RANK);
    assert_eq!(m.effects.len(), 3, "two bonuses and the malus");
    assert!(matches!(m.effects[0], ModEffect::BaseDamage(v) if (v - 1.65 * 1.2375 * 1.3).abs() < 1e-3));
}

/// Illegal rivens are refused by REASON, so the builder can say which
/// knob is wrong rather than just greying out.
#[test]
fn illegality_is_reported_per_reason() {
    assert!(spec(&["damage", "multishot"], None, 8).illegal().is_empty());
    // Four bonuses is not a riven.
    let too_many = spec(&["damage", "multishot", "critical_chance", "critical_damage"], None, 8);
    assert!(too_many.illegal().iter().any(|r| r.contains("2 or 3 bonuses")));
    // Toxin is bonus-only; it can never be the malus.
    let bad_malus = spec(&["damage", "multishot"], Some("toxin"), 8);
    assert!(
        bad_malus.illegal().iter().any(|r| r.contains("bonus-only")),
        "{:?}",
        bad_malus.illegal()
    );
    // A stat cannot appear twice.
    let dup = spec(&["damage", "damage"], None, 8);
    assert!(dup.illegal().iter().any(|r| r.contains("twice")));
    // Rolls live in 0.9-1.1.
    let mut wild = spec(&["damage", "multishot"], None, 8);
    wild.bonuses[0].roll = 1.5;
    assert!(wild.illegal().iter().any(|r| r.contains("outside")));
    // And a stat from the wrong pool.
    let mut alien = spec(&["damage", "multishot"], None, 8);
    alien.bonuses[1].id = "not_a_stat".into();
    assert!(alien.illegal().iter().any(|r| r.contains("not a rifle riven stat")));
}

/// A PHYSICAL STAT IS REFUSED BY EVIDENCE, NEVER BY THE SHARE RULE — and the
/// card's own check reads the same list the picker and the board read.
#[test]
fn a_physical_stat_is_refused_by_evidence_not_by_its_share() {
    // The Torid is pure Toxin and its family's cards carry no physical stat.
    for id in PHYSICAL {
        let s = spec(&["damage", id], None, 8);
        assert!(!s.illegal_for("torid").is_empty(), "{id} should be refused on the Torid");
    }
    // And it is the RIVEN that is fine — only the pairing is wrong.
    assert!(spec(&["damage", "slash"], None, 8).illegal().is_empty());

    // The Ocucor is 0% Slash, and real cards carry it as the malus: the card a
    // player sent, which this check refused while the picker offered it.
    let ocucor = RivenSpec { class: "pistol".into(), ..spec(&["damage", "critical_damage"], Some("slash"), 8) };
    assert!(ocucor.illegal_for("ocucor").is_empty(), "{:?}", ocucor.illegal_for("ocucor"));

    // A family nobody has looked at: the share rule would refuse the stat, so
    // it is OFFERED and marked unconfirmed rather than refused.
    let unseen = crate::data::weapons::all().iter().find(|w| {
        w.riven_family.as_deref().is_some_and(|f| survey(f).is_none()) && !unconfirmed_for(&w.id).is_empty()
    });
    if let Some(w) = unseen {
        let u = unconfirmed_for(&w.id);
        assert!(u.iter().all(|id| !excluded_for(&w.id).contains(id)), "{}: {u:?}", w.id);
    }
}

/// A value can be typed IN, not just rolled to — that is how a riven you
/// already own gets entered. Out of range snaps to the nearest legal end
/// rather than being refused, so typing is never a dead end.
#[test]
fn a_typed_value_becomes_the_roll_it_implies_and_stays_legal() {
    let by = |id: &str| pool("rifle").iter().find(|x| x.id == id).unwrap();
    let s = spec(&["damage", "multishot"], None, 8);
    let dmg = by("damage");
    let (lo, hi) = s.bounds_of(dmg, true, 1.3);
    // Torid, two bonuses, no malus: base x 90 x 1.3 x 0.99, at each
    // end. Written from the stored base rather than the round 1.65,
    // because the stored number is 0.018333299 and 90x it is 1.6499969.
    let unit = dmg.base * 90.0 * 1.3 * 0.99;
    assert!((lo - unit * ROLL_MIN).abs() < 1e-9);
    assert!((hi - unit * ROLL_MAX).abs() < 1e-9);

    // A value inside the band round-trips to itself.
    let want = (lo + hi) / 2.0;
    let r = s.roll_for_value(dmg, true, 1.3, want);
    assert!((s.value_of(dmg, r, true, 1.3) - want).abs() < 1e-9, "round trip");
    assert!((ROLL_MIN..=ROLL_MAX).contains(&r));

    // Outside it, each end. Note both clamps: asking for far too little
    // must land on the MINIMUM, not the maximum — the sign of the miss
    // has to be respected.
    assert!((s.roll_for_value(dmg, true, 1.3, 0.01) - ROLL_MIN).abs() < 1e-9);
    assert!((s.roll_for_value(dmg, true, 1.3, 99.0) - ROLL_MAX).abs() < 1e-9);

    // A MALUS flips its stat, and Weapon Recoil is stored NEGATIVE, so
    // the malus flips it upward — recoil going UP, which is the harm.
    // Bounds still come back ordered, and the smallest roll is still the
    // gentlest malus.
    let with_malus = spec(&["damage", "multishot"], Some("weapon_recoil"), 8);
    let rec = by("weapon_recoil");
    let (clo, chi) = with_malus.bounds_of(rec, false, 1.3);
    assert!(clo <= chi, "bounds come back ordered");
    assert!(clo > 0.0, "a malus flips a negative stat upward: {clo}");
    assert!(clo.abs() < chi.abs(), "the gentlest malus is the smallest");
    assert!((with_malus.roll_for_value(rec, false, 1.3, clo) - ROLL_MIN).abs() < 1e-9);
    assert!((with_malus.roll_for_value(rec, false, 1.3, chi) - ROLL_MAX).abs() < 1e-9);
}

/// A riven's Damage is IN SERRATION'S BUCKET, not a multiplier of its own.
///
/// Asked directly ("did you treat the riven's base
/// damage as an extra multiplicative bucket?"), and the answer has to be
/// a number rather than a reading of the code: if it were its own bucket,
/// a riven would be worth far more than the same percentage on a mod, and
/// every riven comparison downstream would be wrong in the same direction.
#[test]
fn a_rivens_damage_joins_serrations_bucket_and_does_not_multiply_it() {
    use crate::build::loadout::resolve;
    use crate::model::WeaponBase;
    use crate::model::StackPolicy;
    let base = WeaponBase::from_data("torid", true, &[]);
    let serration = crate::data::mods::class_pool("rifle")
        .into_iter()
        .find(|m| m.id == "serration")
        .expect("serration");
    // A riven whose Damage is deliberately NOT a round number, so an
    // accidental match cannot be luck.
    let riven = spec(&["damage", "multishot"], None, 8).to_mod_def("riven:x", 1.3);
    let bucket = |mods: &[&ModDef]| {
        resolve(&base, mods, StackPolicy::AssumedMax).base_damage_bonus
    };

    let s = bucket(&[&serration]);
    let r = bucket(&[&riven]);
    let both = bucket(&[&serration, &riven]);
    assert!(s > 1.0 && r > 1.0, "both are real bonuses: {s} {r}");
    // ADDITIVE: the bucket is a sum. A separate multiplicative bucket
    // would give (1+s)(1+r) - 1 = s + r + s*r, which for these two is
    // ~2.7 higher — not a rounding difference.
    assert!(
        (both - (s + r)).abs() < 1e-9,
        "riven + mod must SUM in one bucket: {both} vs {s} + {r} = {}",
        s + r
    );
    assert!(
        (both - (s + r + s * r)).abs() > 1.0,
        "and it is nowhere near the multiplicative reading"
    );
}

/// A weapon takes ONE riven, and the pool already has a word for that.
#[test]
fn two_rivens_cannot_be_equipped_together() {
    let a = spec(&["damage", "multishot"], None, 8).to_mod_def("riven:a", 1.3);
    let b = spec(&["critical_chance", "heat"], None, 8).to_mod_def("riven:b", 1.3);
    assert_eq!(a.family, Some("riven"));
    assert_eq!(a.family, b.family, "any two rivens exclude each other");
    // And it is the same mechanism ordinary mods use, not a parallel one:
    // the pool already excludes mods this way.
    assert!(crate::data::mods::class_pool("rifle")
        .iter()
        .any(|m| m.family.is_some_and(|f| f != "riven")));
}

/// A weapon does not roll a stat it does not have.
///
/// Verglas Prime is the case that prompted this: a
/// SENTINEL weapon the player never aims, hit-scan, "Ammo Max: ∞ / Ammo
/// Type: None", 100% Cold. Its wiki table has no Zoom row and no Recoil
/// row, so four stats plus all three physical ones are impossible on it —
/// out of a 24-stat rifle pool, seven were being offered as choices that
/// could never appear on a real card.
#[test]
fn a_weapon_does_not_roll_a_stat_it_does_not_have() {
    let v = excluded_for("verglas_prime");
    for id in ["zoom", "weapon_recoil", "ammo_maximum", "projectile_speed"] {
        assert!(v.contains(&id), "verglas_prime must not roll {id}: {v:?}");
    }
    // 100% Cold, and its family's cards carry no physical stat: refused by
    // that evidence, not by the share.
    for id in PHYSICAL {
        assert!(v.contains(&id), "verglas_prime {id}: {v:?}");
    }
    // …and it keeps everything a sentinel weapon really has.
    for id in ["magazine_capacity", "reload_speed", "punch_through", "cold"] {
        assert!(!v.contains(&id), "verglas_prime does have {id}: {v:?}");
    }

    // Cernos Prime is 165.6/9.2/9.2, and its family's cards carry Impact and
    // neither 5% component — refused by that evidence, not by the share.
    let c = excluded_for("cernos_prime");
    assert!(!c.contains(&"impact"), "impact is 90% of the arrow: {c:?}");
    assert!(c.contains(&"puncture") && c.contains(&"slash"), "both are 5%: {c:?}");
    // A projectile weapon keeps its flight speed, and one with a real ammo
    // pool keeps Ammo Maximum.
    assert!(!c.contains(&"projectile_speed") && !c.contains(&"ammo_maximum"), "{c:?}");
}

/// The share is read on EVERY form the weapon fires for free, not on the
/// one the arsenal happens to show.
///
/// Larkspur Prime, reported by a player through the owner:
/// his riven is Fire Rate / Heat with a NEGATIVE Impact, and the editor
/// would not offer Impact at all. Its beam is 10 of 90 Impact — 11%, under
/// the 25% line — but its alt-fire, one held button away and no gauge to
/// fill, is 140 of 420, which is 33%. One riven covers both, so the pool
/// is the union.
#[test]
fn a_free_alt_fire_counts_toward_the_physical_share() {
    // BOTH ENTRIES, because the report came back on the OTHER one. This
    // asserted `larkspur_prime` alone — the card the owner relayed — and a
    // player reported the plain Larkspur refusing negative Impact a
    // fortnight later. It was already right, and a
    // family where one member is pinned and its twin is not is precisely
    // how a fixed bug gets re-reported: there is nothing to point at.
    for id in ["larkspur", "larkspur_prime"] {
        let l = derived_for(id);
        assert!(!l.contains(&"impact"), "{id}: the alt-fire is 33% Impact: {l:?}");
        // Nothing else is invented: neither form deals Puncture or Slash.
        assert!(l.contains(&"puncture") && l.contains(&"slash"), "{id}: {l:?}");
    }

    // The OTHER rule is the flight one, and here the derivation is only
    // the fallback. Phantasma Prime's plasma bomb genuinely flies at
    // 25 m/s, which by the derivation alone would keep Projectile Speed
    // — and 500 real Phantasma rivens carry it zero times, so the survey
    // overrules the reasoning. Gotva Prime is the same claim with
    // no survey behind it: its family is the one warframe.market refused,
    // so the derivation still answers for it.
    let p = excluded_for("phantasma_prime");
    assert!(p.contains(&"projectile_speed"), "0 of 500 Phantasma cards: {p:?}");
    for id in ["burston_prime", "gotva_prime", "karak_wraith", "prisma_grinlok"] {
        let e = excluded_for(id);
        assert!(e.contains(&"projectile_speed"), "{id} is hit-scan in every form: {e:?}");
    }
}



/// A FAMILY IS CALLED WHAT DE CALLS IT.
///
/// `riven_family` decides which weapons share a pool, which an
/// `exceptions.yaml` entry covers, and whether the family can be surveyed
/// at all, since the market is queried by that name. So a name nobody else
/// uses silently makes the family a singleton and the survey empty.
///
/// "Strip the variant prefix" is right for a Prime, a Vandal, a Wraith, a
/// Prisma, a Rakta or a Telos — DE's list holds `Boltor` and no `Boltor
/// Prime` — and WRONG for a weapon with no ordinary counterpart, where the
/// prefix IS the name: `Kuva Ayanga`, `Gotva Prime`, `Vadarya Prime`, `Coda
/// Bassocyst`, `Dual Coda Torxica`, `EFV-5 Jupiter`. It caught six, one of
/// which is why `pools.yaml` records "Gotva: NOT SURVEYED (the API
/// refused)" — it was asked about a weapon that does not exist.
///
/// THE SNAPSHOT CONFIRMS AND NEVER REFUTES. `de_families.yaml` is one week
/// of trades, so a family absent from it is a family nobody traded, not a
/// family that does not exist. This asserts the names DE DOES know and
/// reports the rest.
#[test]
fn every_riven_family_is_spelled_the_way_de_spells_it() {
    let de: std::collections::HashSet<&str> =
        de_families().iter().map(String::as_str).collect();
    assert!(de.len() > 300, "the snapshot loaded: {}", de.len());
    let mut families: std::collections::BTreeMap<&str, Vec<&str>> = Default::default();
    for w in crate::data::weapons::all() {
        if let Some(f) = w.riven_family.as_deref() {
            families.entry(f).or_default().push(w.id.as_str());
        }
    }
    // A CASE-FOLDED MATCH IS STILL A MISMATCH, and it is the interesting
    // one: the schema says "in all capital case" and the dump is title
    // case, so a spelling that differs only in capitalisation is a sign the
    // name came from somewhere other than DE.
    let folded: std::collections::HashMap<String, &str> =
        de.iter().map(|x| (x.to_lowercase(), *x)).collect();
    let mut wrong = Vec::new();
    let mut untraded = 0;
    for (f, members) in &families {
        if de.contains(f) {
            continue;
        }
        match folded.get(&f.to_lowercase()) {
            Some(right) => wrong.push(format!("{f:?} should be {right:?} ({members:?})")),
            None => untraded += 1,
        }
    }
    assert!(
        wrong.is_empty(),
        "{} family names disagree with DE's own `compatibility`:\n  {}",
        wrong.len(),
        wrong.join("\n  ")
    );
    // …AND THE COVERAGE IS ASSERTED, not merely printed. Every family in
    // this roster was traded in the surveyed week, so an untraded one is
    // new information — a weapon so obscure nobody trades its rivens, or a
    // name that is wrong in a way case-folding cannot see. Either is worth
    // being told about rather than discovering later.
    assert_eq!(
        untraded, 0,
        "{untraded} of {} families are not in DE's snapshot at all",
        families.len()
    );
}

/// A RIVEN FAMILY AGREES WITH ITSELF, because a riven belongs to the family
/// and not to a member of it.
///
/// One card equips on every member — a Ballistica riven IS a Ballistica
/// Prime riven and a Rakta Ballistica riven, which is what `riven_family`
/// means and why the exception table and the survey are both keyed on it.
/// So a pool derived from one member describes a card that cannot exist,
/// and the app tells a player holding the real thing that it is "not a
/// legal riven".
///
/// FIFTEEN FAMILIES DISAGREED WITH THEMSELVES. The sharpest is the
/// Ballistica: the Prime's charged shot is over the 25% Slash line and the
/// other two members are under it, so the same card was legal on one entry
/// and refused on the other two.
///
/// ASSERTED OVER EVERY FAMILY rather than over the fifteen. A list of names
/// cannot report the sixteenth, and the sixteenth arrives the day a weapon
/// joins a family with a damage split of its own — which is a thing DE does
/// (the Tenet and Kuva variants in this list are all exactly that).
#[test]
fn a_riven_family_agrees_with_itself() {
    use std::collections::BTreeMap;
    let mut by_family: BTreeMap<&str, Vec<(&str, Vec<&'static str>)>> = BTreeMap::new();
    for w in crate::data::weapons::all() {
        let Some(f) = w.riven_family.as_deref() else { continue };
        let mut e = excluded_for(&w.id);
        e.sort_unstable();
        by_family.entry(f).or_default().push((w.id.as_str(), e));
    }
    assert!(by_family.len() > 200, "every family is asked: {}", by_family.len());
    let mut split = Vec::new();
    for (f, members) in &by_family {
        if members.iter().any(|(_, e)| *e != members[0].1) {
            split.push(format!(
                "{f}: {}",
                members
                    .iter()
                    .map(|(id, e)| format!("{id}{e:?}"))
                    .collect::<Vec<_>>()
                    .join(" vs ")
            ));
        }
    }
    assert!(
        split.is_empty(),
        "a riven equips on every member of its family, so the pool cannot \
             differ between members — {} families disagree:\n  {}",
        split.len(),
        split.join("\n  ")
    );

    // …AND THE FAMILY IS THE UNION, not the intersection. A test that only
    // asserted agreement would pass just as well on a derivation that
    // refused everything to everybody, so this names what the union WON:
    // the Ballistica Prime's charged shot is 18% Slash on a 44% Puncture
    // body, over the line, and all three members roll it now.
    for id in ["ballistica", "ballistica_prime", "rakta_ballistica"] {
        let e = derived_for(id);
        assert!(!e.contains(&"slash"), "{id}: the Prime's charge earns Slash: {e:?}");
        // Nothing is invented: no member is over the line on Impact.
        assert!(e.contains(&"impact"), "{id}: no member deals 25% Impact: {e:?}");
    }
    // The Ogris is the other direction of the same rule — the KUVA member
    // is the one over the line, and the ordinary one inherits it.
    assert!(!derived_for("ogris").contains(&"impact"));
    assert!(!derived_for("ogris").contains(&"puncture"));
}

/// THE FAMILY POOL IS THE UNION, and these are the cards that say so.
///
/// A riven equips on every member of its family, so a family whose members
/// disagree under the 25% line has two readings the rule cannot choose
/// between: UNION (one member over the line earns it for all) or
/// INTERSECTION. Only cards can answer it, and among the SURVEYED families
/// seven (family, stat) pairs disagree — the listings say UNION on five
/// (Boar Slash, Braton Impact, Braton Puncture, Karak Slash, Sybaris
/// Puncture) and INTERSECTION on two, which are ONE family already carrying
/// its own exception. So the union is the rule and the Sicarus is the
/// exception — which is the shape this whole file is built on, arrived at
/// from the other end.
///
/// EACH HALF IS ASSERTED ON ITS OWN CALL, because four of the five UNION
/// families are also in `exceptions.yaml` — so `excluded_for` returns the
/// exception's answer whatever the rule does, and passes on a derivation
/// sabotaged to take the INTERSECTION. The union half asks `derived_for`,
/// the rule alone; the Sicarus half asks `excluded_for`, the rule plus the
/// exception that overrules it.
#[test]
fn a_family_pool_is_the_union_and_the_sicarus_is_the_exception() {
    // The five the cards earn through ONE member being over the line —
    // against the RULE, so an intersection here cannot hide behind an
    // exception written from the same survey.
    for (id, stat) in [
        ("boar", "slash"),
        ("boar_prime", "slash"),
        ("braton", "impact"),
        ("braton_prime", "impact"),
        ("mk1_braton", "impact"),
        ("braton_vandal", "puncture"),
        ("karak", "slash"),
        ("karak_wraith", "slash"),
        ("kuva_karak", "slash"),
        ("dex_sybaris", "puncture"),
    ] {
        let e = derived_for(id);
        assert!(
            !e.contains(&stat),
            "{id} rolls {stat} — a family member is over the line and real \
                 cards carry it, so the RULE must be the union: {e:?}"
        );
    }
    // …and the two the cards refuse, on BOTH members, through the
    // exception rather than through the derivation.
    for id in ["sicarus", "sicarus_prime"] {
        let e = excluded_for(id);
        assert!(
            e.contains(&"puncture") && e.contains(&"slash"),
            "{id}: 0 of 500 live listings carry either, which is what the \
                 exception records — {e:?}"
        );
    }
}

/// AN INCARNON FORM DOES NOT WIDEN THE POOL, and this is what counting it
/// would cost — in real cards, per family, rather than in argument.
///
/// The rule above reads the union of the forms a weapon fires FOR FREE, and
/// what settles where "for free" stops is not the reasoning (a gauge form
/// is paid for with evolutions) but the cards. Removing the
/// `is_adapter_form` filter moves 25 weapons, seven of which would gain a
/// PHYSICAL stat the survey records **zero** times:
///
/// About 3,200 listings, not one carrying a stat the wider rule would
/// offer — the same evidence the flight rule rests on, so it is one finding
/// on both halves of the derivation rather than two arguments.
///
/// THE SURVEY IS EVIDENCE AND NOT A LAW, as `data/rivens/pools.yaml` says
/// itself: *"absence in 500 listings is strong evidence and not a
/// guarantee. An in-game card that contradicts a `never` here beats the
/// file."* One real card settles it the other way, recorded as an entry in
/// `exceptions.yaml`. This test exists so flipping the rule is a decision
/// with a price tag rather than a one-line edit that reddens nothing.
#[test]
fn an_incarnon_form_does_not_widen_the_physical_pool() {
    // THE SEVEN, AND WHAT COUNTING THEM FOUND:
    //
    // | family | stat the Incarnon form would unlock | cards carrying it |
    // | --- | --- | --- |
    // | Boltor | Slash | 0 of 500 |
    // | Latron | Impact | 0 of 500 |
    // | Atomos | Impact | 0 of 500 |
    // | Lex | Impact | 0 of 500 |
    // | Dual Toxocyst | Slash | 0 of 500 |
    // | Kunai | Slash | 0 of 430 |
    // | Bronco | Slash | 0 of 309 |
    //
    // Each pair is (weapon, the stat its Incarnon form would unlock). The
    // assertion is that the stat is still EXCLUDED — that is, that the
    // gauge-switched form was not counted.
    for (id, stat, cards) in [
        ("boltor", "slash", 500),
        ("boltor_prime", "slash", 500),
        ("telos_boltor", "slash", 500),
        ("latron", "impact", 500),
        ("latron_prime", "impact", 500),
        ("latron_wraith", "impact", 500),
        ("atomos", "impact", 500),
        ("lex", "impact", 500),
        ("lex_prime", "impact", 500),
        ("dual_toxocyst", "slash", 500),
        ("kunai", "slash", 430),
        ("mk1_kunai", "slash", 430),
        ("bronco", "slash", 309),
    ] {
        let e = derived_for(id);
        assert!(
            e.contains(&stat),
            "{id}: counting the Incarnon form would offer {stat}, which \
                 0 of {cards} real cards in this family carry — {e:?}"
        );
    }
    // …AND THE NEGATIVE CONTROL, which is what keeps this from being a test
    // that would pass on a derivation that excluded everything: the same
    // weapons still roll the physical stats their BASE form earns.
    let lex = derived_for("lex");
    assert!(!lex.contains(&"puncture"), "the Lex is 88% Puncture: {lex:?}");
    let kunai = derived_for("kunai");
    assert!(!kunai.contains(&"puncture"), "the Kunai is 90% Puncture: {kunai:?}");
    // …and the FREE alt-fire is still counted, which is the rule this one
    // bounds rather than replaces.
    assert!(!derived_for("larkspur").contains(&"impact"));
}

/// AN EXCEPTION OVERRIDES THE RULES, and only an exception does.
///
/// The survey is not in this path. Every answer it gave is an entry in
/// `exceptions.yaml` carrying the count it came from, so this asserts the
/// ANSWERS, which is what a player sees, rather than
/// which file produced them.
#[test]
fn an_exception_overrides_the_derivation_and_nothing_else_does() {
    // 1. A REAL CARD. The Furis is hit-scan in both forms, so the rules
    //    refuse Projectile Speed; a player has the card.
    let f = excluded_for("furis");
    assert!(!f.contains(&"projectile_speed"), "a real Furis card carries it: {f:?}");
    // The MK1 is the same riven, because it is the same family.
    assert!(!excluded_for("mk1_furis").contains(&"projectile_speed"));
    // …and the Dual Toxocyst is the same case with the same reason: both
    // its forms are hit_scan here, the Incarnon one is a Projectile in
    // DE's code, and a player's card carries the stat as a curse.
    let t = excluded_for("dual_toxocyst");
    assert!(!t.contains(&"projectile_speed"), "a real card carries it: {t:?}");

    // 2. ADDING what the rules refused. The Ocucor is 9% Puncture and 91%
    //    Radiation — the 25% rule strikes all three physical stats, and all
    //    three roll on real cards.
    let o = excluded_for("ocucor");
    for id in ["impact", "puncture", "slash", "projectile_speed"] {
        assert!(!o.contains(&id), "the Ocucor rolls {id}: {o:?}");
    }
    // …and TAKING AWAY what they allowed. The Phenmor is 30% Puncture, over
    // the line, and no live listing carries it.
    assert!(excluded_for("phenmor").contains(&"puncture"));
    // Zoom is not derived at all — nothing in the weapon data says a Boar
    // has no scope, so only an exception can say it.
    assert!(excluded_for("boar").contains(&"zoom"));
    // A share landing EXACTLY on 25% is decided by neither of us: Karak
    // Wraith's Slash is 7.75 of 31 and the rule reads "more than 25%".
    assert!(!excluded_for("karak_wraith").contains(&"slash"));

    // 3. THE DERIVATION answers for every unexcepted NON-physical stat, and
    //    a physical one the cards cannot settle is offered as unconfirmed —
    //    the Acrid's market is too thin to say either way.
    let v = excluded_for("verglas_prime");
    assert!(v.contains(&"zoom"));
    let acrid = unconfirmed_for("acrid");
    assert!(!acrid.is_empty(), "the share rule refuses something on a Toxin pistol");
    assert!(acrid.iter().all(|id| !excluded_for("acrid").contains(id)), "{acrid:?}");
}

/// THE SURVEY IS A CHECK, NOT A SOURCE — this is the check.
///
/// It exists because the opposite arrangement fails silently. With
/// `pools.yaml` outranking the derivation, a re-run of the scrape that came
/// back "nothing rolls anything" for all 26 families would empty every
/// pool in the app, and the only thing that noticed was two tests about
/// something else.
///
/// Now a disagreement is a FAILURE that names the family and the stat, and
/// the fix is a human one: promote it into `exceptions.yaml` with its count,
/// or fix the rule. A broken scrape fails this immediately and loudly,
/// because a broken scrape disagrees with everything at once.
#[test]
fn the_survey_still_agrees_with_the_rules() {
    let mut checked = 0;
    let mut bad: Vec<String> = Vec::new();
    for w in crate::data::weapons::all() {
        let Some(fam) = w.riven_family.as_deref() else { continue };
        let Some(sv) = survey(fam) else { continue };
        let ours = excluded_for(&w.id);
        checked += 1;
        // A physical stat's answer IS a survey (`physical.yaml`, the newer and
        // per-stat one), so this older count is checked against the rules only.
        for r in sv.rollable.iter().filter(|s| !PHYSICAL.contains(s)) {
            if ours.contains(r) {
                bad.push(format!("{fam}/{r}: {} listings carry it, we refuse it", sv.n));
            }
        }
        for n in sv.never.iter().filter(|s| !PHYSICAL.contains(s)) {
            if !ours.contains(n) {
                bad.push(format!("{fam}/{n}: no listing carries it, we offer it"));
            }
        }
    }
    assert!(checked > 0, "no surveyed family was reached");
    assert!(
        bad.is_empty(),
        "the survey and the rules disagree — promote each into \
             data/rivens/exceptions.yaml with its count, or fix the rule:\n  {}",
        bad.join("\n  ")
    );
}

/// A SURVEY THAT SAYS NOTHING ROLLS ANYTHING IS A BROKEN SCRAPE.
///
/// The literal failure of 2026-08-08: the per-stat queries started coming
/// back empty, so every family was written with an empty `rollable` and a
/// `never` listing the entire stat table. That is not a discovery about
/// Warframe, it is a discovery about the endpoint — and it has to be named
/// as one before anybody reads the numbers.
#[test]
fn a_survey_that_refuses_everything_is_a_broken_scrape() {
    for s in surveys() {
        assert!(
            !s.rollable.is_empty(),
            "{}: the survey says nothing rolls — that is the scrape failing, \
                 not the game (n={})",
            s.family,
            s.n
        );
    }
}

/// Faction damage prints as a MULTIPLIER, because that is what the card
/// says: a malus the sim stores as -0.41 reads "x0.59 Damage to Corpus", so its range runs 0.xx-1.xx and never shows a
/// minus sign. Everything else keeps its sign and its percent.
#[test]
fn a_faction_stat_prints_the_multiplier_the_card_shows() {
    let by = |id: &str| pool("rifle").iter().find(|x| x.id == id).unwrap();
    let corpus = by("damage_to_corpus");
    assert_eq!(corpus.shown_as(), Shown::Multiplier);
    assert!((corpus.shown(-0.41) - 0.59).abs() < 1e-9, "the card's own number");
    assert!(corpus.print(-0.41).starts_with("x0.59"), "{}", corpus.print(-0.41));
    assert!(corpus.print(0.45).starts_with("x1.45"), "{}", corpus.print(0.45));
    // And a typed card number comes back to the stored fraction.
    assert!((corpus.from_shown(0.59) + 0.41).abs() < 1e-9);
    assert!((corpus.from_shown(corpus.shown(0.123)) - 0.123).abs() < 1e-12);

    // A real malus on a real weapon lands in 0.xx, never below zero: the
    // malus multiplier is -0.495 at two bonuses, so 0.45 x 1.3 x 0.495.
    let s = spec(&["damage", "multishot"], Some("damage_to_corpus"), 8);
    let v = s.value_of(corpus, 1.0, false, 1.3);
    assert!(v < 0.0, "stored as a loss: {v}");
    let (lo, hi) = s.bounds_of(corpus, false, 1.3);
    assert!(corpus.shown(lo) > 0.0 && corpus.shown(hi) < 1.0, "0.xx band");

    // Percent and plain-number stats are untouched.
    assert_eq!(by("damage").shown_as(), Shown::Percent);
    assert!(by("damage").print(1.65).starts_with("+165.0%"));
    assert_eq!(by("punch_through").shown_as(), Shown::Number);
    assert!(by("punch_through").print(2.7).starts_with("+2.7 "));
}

/// The card shows ONE decimal on a percentage, so we show one: nobody can
/// read a second one off a riven they own. What is NOT
/// rounded is the arithmetic — the roll behind a displayed 144.8 is
/// whatever 144.8 implies, exactly.
#[test]
fn the_reading_is_the_cards_precision_and_the_maths_is_not() {
    let by = |id: &str| pool("rifle").iter().find(|x| x.id == id).unwrap();
    let multishot = by("multishot");
    assert_eq!(multishot.decimals(), 1);
    assert_eq!(multishot.print(1.447_87), "+144.8% Multishot");
    assert_eq!(by("damage_to_corpus").decimals(), 2, "the card's own two");

    // The value behind it is untouched: `shown` is exact and the roll a
    // typed reading implies is exact too, one decimal in or not. Read off
    // the middle of the band so the case is an ordinary roll and not a
    // clamp — the band moves with shape and disposition, so it is asked
    // for rather than written down.
    let s = spec(&["multishot", "damage"], None, 8);
    let (lo, hi) = s.bounds_of(multishot, true, 1.3);
    let reading = (multishot.shown((lo + hi) / 2.0) * 10.0).round() / 10.0;
    let r = s.roll_for_value(multishot, true, 1.3, multishot.from_shown(reading));
    assert!((multishot.shown(s.value_of(multishot, r, true, 1.3)) - reading).abs() < 1e-9);
    assert!(r > ROLL_MIN && r < ROLL_MAX, "an ordinary roll, not an end: {r}");
}

/// The percentile is the roll's place in its own band, so it compares two
/// stats on one card and two cards on different weapons — disposition,
/// shape and base all divide out.
#[test]
fn the_percentile_is_where_the_roll_landed_in_its_band() {
    assert!((percentile(ROLL_MIN) - 0.0).abs() < 1e-9);
    assert!((percentile(ROLL_MAX) - 100.0).abs() < 1e-9);
    assert!((percentile(1.0) - 50.0).abs() < 1e-9);
    assert!((percentile(1.08) - 90.0).abs() < 1e-9);
    // Out-of-band rolls cannot report out-of-band positions.
    assert!((percentile(2.0) - 100.0).abs() < 1e-9);
    assert!((percentile(0.0) - 0.0).abs() < 1e-9);

    // It is the SIZE of the roll, not a judgement: a malus at the top of
    // its band is the 100th too, and it is the worst one there is.
    let by = |id: &str| pool("rifle").iter().find(|x| x.id == id).unwrap();
    let s = spec(&["damage", "multishot"], Some("weapon_recoil"), 8);
    let worst = s.value_of(by("weapon_recoil"), ROLL_MAX, false, 1.3);
    let mild = s.value_of(by("weapon_recoil"), ROLL_MIN, false, 1.3);
    assert!(worst.abs() > mild.abs());
    assert!(percentile(ROLL_MAX) > percentile(ROLL_MIN));
}

/// A tie is the NORM in a constructor, because ranking is by ROLL and
/// every stat can sit at 1.1 at once. The name must then still be one name, and must not depend
/// on the order the stats were entered.
#[test]
fn a_tied_name_does_not_depend_on_the_order_the_stats_were_entered() {
    let all_max = |ids: &[&str]| {
        let mut s = spec(ids, None, 8);
        for st in &mut s.bonuses {
            st.roll = ROLL_MAX;
        }
        s
    };
    let a = all_max(&["heat", "cold", "multishot"]);
    let b = all_max(&["multishot", "heat", "cold"]);
    let c = all_max(&["cold", "multishot", "heat"]);
    assert_eq!(a.name(1.3), b.name(1.3), "declaration order must not show");
    assert_eq!(a.name(1.3), c.name(1.3));

    // And it holds for stats with DIFFERENT bases too, because the value
    // never enters the ranking — three at 1.1 tie however big they are.
    let x = all_max(&["damage", "critical_chance", "multishot"]);
    let y = all_max(&["multishot", "damage", "critical_chance"]);
    assert_eq!(x.name(1.3), y.name(1.3));
}

/// Every stat rolls its band INDEPENDENTLY, so the corner where all three
/// bonuses are maximal and the malus minimal is a legal riven — merely
/// astronomically unlikely. It has to be constructible, because it is the CEILING, which
/// is exactly the riven an optimizer wants to know about.
#[test]
fn the_god_roll_corner_is_legal_and_is_the_ceiling() {
    let by = |id: &str| pool("rifle").iter().find(|x| x.id == id).unwrap();
    let mut god = spec(&["damage", "multishot", "critical_chance"], Some("weapon_recoil"), 8);
    for s in &mut god.bonuses {
        s.roll = ROLL_MAX;
    }
    god.malus.as_mut().unwrap().roll = ROLL_MIN;
    assert!(god.illegal().is_empty(), "{:?}", god.illegal());

    // Nothing legal beats it on a bonus...
    let mid = spec(&["damage", "multishot", "critical_chance"], Some("weapon_recoil"), 8);
    assert!(
        god.value_of(by("damage"), ROLL_MAX, true, 1.3)
            > mid.value_of(by("damage"), 1.0, true, 1.3)
    );
    // ...and nothing legal has a gentler malus: the malus is negative, so
    // the smallest roll is the least harmful.
    let worst = mid.value_of(by("weapon_recoil"), ROLL_MAX, false, 1.3);
    let best = god.value_of(by("weapon_recoil"), ROLL_MIN, false, 1.3);
    assert!(best.abs() < worst.abs(), "min roll is the kindest malus");

    // Independence is the point: two stats at different rolls is legal,
    // which it would not be if one quality applied to the whole riven.
    let mut mixed = spec(&["damage", "multishot"], None, 8);
    mixed.bonuses[0].roll = ROLL_MIN;
    mixed.bonuses[1].roll = ROLL_MAX;
    assert!(mixed.illegal().is_empty());
}

/// The name is GENERATED, from the bonuses, ranked by ROLL.
///
/// The wiki's own worked example is the test: a Vectis riven named
/// "Sati-critaata" has "Multishot as the highest stat, Critical Chance as
/// the second highest, and Base Damage as the lowest". Multishot's base
/// (90%) is the SMALLEST of those three — Damage is 165% — so "highest"
/// cannot mean the value. It means the roll.
#[test]
fn the_name_comes_from_the_stats_ranked_by_roll() {
    let rolled = |pairs: &[(&str, f64)], malus: Option<&str>| RivenSpec {
        class: "rifle".into(),
        bonuses: pairs
            .iter()
            .map(|(id, r)| RolledStat { id: (*id).into(), roll: *r })
            .collect(),
        malus: malus.map(|id| RolledStat { id: id.into(), roll: 1.0 }),
        rank: 8,
        polarity: Polarity::Madurai,
    };

    // The wiki's Vectis, verbatim.
    let vectis = rolled(
        &[("multishot", 1.10), ("critical_chance", 1.05), ("damage", 0.95)],
        None,
    );
    assert_eq!(vectis.name(1.0), "Sati-critaata");

    // Value ordering would have said Visi- (damage 165% is the biggest),
    // which is the whole point of the example.
    assert!(!vectis.name(1.0).starts_with("Visi"));

    // Declaration order does not matter; the roll does.
    let shuffled = rolled(
        &[("damage", 0.95), ("multishot", 1.10), ("critical_chance", 1.05)],
        None,
    );
    assert_eq!(shuffled.name(1.0), "Sati-critaata");

    // Two bonuses: no core, so "CoreSuffix" — the higher roll's PREFIX
    // fragment and the lower one's suffix.
    let two = rolled(&[("damage", 1.10), ("multishot", 0.95)], None);
    assert_eq!(two.name(1.0), "Visican");

    // Disposition cannot rename a riven: it scales every stat alike, and
    // the ranking never looks at the value anyway.
    assert_eq!(vectis.name(1.0), vectis.name(1.55));

    // The malus contributes no fragment.
    //
    // UNVERIFIED: the wiki says names are drawn from "the randomized
    // attributes the mod has" without saying whether the malus is one of
    // them, and its example has three bonuses so it cannot settle this.
    // An in-game 2-bonus-plus-malus riven does.
    let with_malus = rolled(&[("damage", 1.10), ("multishot", 0.95)], Some("weapon_recoil"));
    assert_eq!(with_malus.name(1.0), two.name(1.0));
}

/// ONE (family, stat) IN ONE FILE. `physical.yaml` is regenerated by the survey
/// and `exceptions.yaml` is hand-written; the same pair in both is two answers
/// with nothing saying which wins, and a regeneration could silently flip it.
#[test]
fn no_riven_fact_is_stated_in_both_evidence_files() {
    use std::collections::BTreeMap;
    let mut seen: BTreeMap<(String, String), &str> = BTreeMap::new();
    let mut both = Vec::new();
    let files = crate::build::rivens::pools::exception_files();
    assert_eq!(files.len(), 2, "both evidence files load");
    for (path, fams) in files {
        for f in fams {
            for s in f.rolls.iter().chain(f.never.iter()) {
                if let Some(other) = seen.insert((f.family.clone(), s.stat.clone()), path) {
                    both.push(format!("{} / {}: {other} and {path}", f.family, s.stat));
                }
            }
        }
    }
    assert!(both.is_empty(), "{}", both.join("\n"));
}
