use super::*;

/// THE HAND LIST MAY BE LONG; IT MAY NOT BE SHORT.
///
/// `build::rivens::SIGN_IS_NOT_THE_ANSWER` is read by weapon on purpose — a list
/// a person can audit beats a rule assembled per build — and a hand list's one
/// failure is going stale, silently, the day a Genesis lands. So membership is
/// DERIVED here from the effects themselves and the list is held to it.
///
/// A ROW MORE THAN THIS FINDS IS FINE: it costs two fights that answer "the god
/// roll". A row FEWER publishes a card the fight would have argued with.
#[test]
fn every_weapon_that_pays_for_not_having_something_is_on_the_riven_list() {
    let mut want: std::collections::BTreeMap<&str, std::collections::BTreeSet<&str>> =
        Default::default();
    for e in pool() {
        // WHAT EARNS A WEAPON A ROW: a perk that pays for NOT having something,
        // which makes whatever supplies it a cost rather than a gain.
        let stat = match e.effects.iter().find(|x| {
            matches!(
                x,
                EvoEffect::ChanceDamageOnNoncrit { .. }
                    | EvoEffect::CritMultiplierBelowCritChance { .. }
                    | EvoEffect::CritDamageBelowStatusCount { .. }
                    | EvoEffect::FlatBaseDamageOnEmptyReload(_)
                    | EvoEffect::FieldDurationOnEmptyReload(_)
                    | EvoEffect::MagGrowthOnEmptyReload { .. }
            )
        }) {
            Some(EvoEffect::ChanceDamageOnNoncrit { .. })
            | Some(EvoEffect::CritMultiplierBelowCritChance { .. }) => "critical_chance",
            Some(EvoEffect::CritDamageBelowStatusCount { .. }) => "status_chance",
            Some(_) => "magazine_capacity",
            None => continue,
        };
        want.entry(e.weapon.as_str()).or_default().insert(stat);
    }
    assert!(want.len() >= 20, "the walk found {} weapons", want.len());

    let listed: std::collections::BTreeMap<&str, std::collections::BTreeSet<&str>> =
        crate::build::rivens::SIGN_IS_NOT_THE_ANSWER
            .iter()
            .map(|(w, s)| (*w, s.iter().copied().collect()))
            .collect();
    let missing: Vec<String> = want
        .iter()
        .filter(|(w, stats)| !listed.get(*w).is_some_and(|got| stats.is_subset(got)))
        .map(|(w, stats)| format!("{w} {stats:?}"))
        .collect();
    assert!(
        missing.is_empty(),
        "these weapons pay for not having something and no riven row says so: {missing:?}"
    );
}

/// NOTHING IN THE ROSTER RESIZES AN INCARNON CHARGE POOL — no evolution,
/// under any combination.
///
/// A roster-wide invariant, stated measured: there is no
/// mechanism anywhere that restores charges in an Incarnon form or spends
/// extra ones. The pool is filled by the GAUGE and emptied by firing, and
/// that is the whole of it — which is why the magazine family of effects is
/// gated on `incarnon.is_none()` in three separate places
/// (`FlatBaseMagazine`, `MultishotOnLastRound`, and the ammo rules), and why
/// Executioner's Fortune does not roll there at all.
///
/// Three gates is three chances to forget the fourth, so this asserts the
/// PROPERTY instead of the gates: every evolution a charge-backed form can
/// carry, all at once, must leave its pool exactly the size the data
/// declares. A future effect that reaches the gauge fails here rather than
/// shipping as a quietly bigger magazine on seven weapons.
#[test]
fn no_evolution_resizes_an_incarnon_charge_pool() {
    let mut checked = 0;
    for spec in crate::data::weapons::all() {
        if spec.gauge_form.is_none() {
            continue;
        }
        // The whole ladder at once, one option per tier — the widest set a
        // build can hold, so anything that could reach the pool is in it.
        // The GROUP owns the evolutions, not the form — an Incarnon
        // entry's perks are filed under its base weapon's id.
        let group = spec.transform_group.as_deref().unwrap_or(&spec.id);
        let mut ids: Vec<&str> = Vec::new();
        for tier in 1..=tier_count(group) {
            for opt in options(group, tier) {
                ids.push(opt.id.as_str());
            }
        }
        if ids.is_empty() {
            continue;
        }
        checked += 1;
        let bare = crate::model::WeaponBase::from_data(&spec.id, true, &[]);
        let loaded = crate::model::WeaponBase::from_data(&spec.id, true, &ids);
        assert_eq!(
            loaded.magazine_size, bare.magazine_size,
            "{}: an evolution resized the charge pool ({} -> {})",
            spec.id, bare.magazine_size, loaded.magazine_size
        );
    }
    assert!(checked >= 5, "only {checked} charge-backed forms checked");
}

#[test]
fn loads_the_dt_evolution_pool() {
    let frame_seconds: Vec<_> = pool().iter().filter(|e| e.weapon == "dual_toxocyst").collect();
    assert!(frame_seconds.len() >= 9, "expected the 9 DT evolutions, got {}", frame_seconds.len());
    assert_eq!(options("dual_toxocyst", 2).len(), 2); // the EVO II choice
    // Broken evolutions carry the wiki flag.
    assert!(get("dual_toxocyst_ready_retaliation").unwrap().currently_broken);
    assert!(get("dual_toxocyst_neurotoxin").unwrap().currently_broken);
}

#[test]
fn fevered_and_carnage_parse_their_wiki_values() {
    let fe = get("dual_toxocyst_fevered_frenzy").unwrap();
    assert!(fe.effects.contains(&EvoEffect::FlatBaseDamage(50.0)));
    assert!(fe
        .effects
        .contains(&EvoEffect::AssumedMaxMultishot { total: 1.0, max_stacks: 20 }));
    let ca = get("dual_toxocyst_carnage_reign").unwrap();
    assert!(ca.effects.contains(&EvoEffect::FlatBaseDamage(60.0)));
    // …AND ITS SECOND CLAUSE IS GATED, NOT DEAD. "+33% Direct Damage per
    // Status Type" carries an UNLISTED "With Energy Max >= 200", which is what MEASUREMENTS M49 was actually measuring:
    // both runs found it paying nothing, and both were made on the neutral
    // Tenno this repo ships, whose max energy is 150. The gate explains
    // those numbers rather than contradicting them.
    //
    // It was filed as a `live_bug` until then, which told a reader "do not
    // pick this perk for that half" about a perk that pays on the right
    // frame. That is the difference this assertion exists to hold.
    assert!(
        ca.live_bugs().is_empty(),
        "the clause is conditional, not broken: {:?}",
        ca.live_bugs()
    );
    let gate = ca
        .effects
        .iter()
        .find_map(|e| match e {
            EvoEffect::GatedByTenno { gate, grant, value } => Some((*gate, *grant, *value)),
            _ => None,
        })
        .expect("the +33% loads as a gated CO source");
    assert_eq!(gate.1, crate::model::GatedGrant::ConditionOverload);
    assert!((gate.2 - 0.33).abs() < 1e-9, "{:?}", gate.2);
    // THE THRESHOLD IS >=, and asserted at the boundary rather than in the
    // middle: 200 is the number the card names, so a frame AT 200 pays. A
    // gate transcribed as `> 200` passes every test that only tries 150 and
    // 300, and is wrong for exactly the frames a player would build for.
    let at = |energy: f64| {
        let mut t = crate::data::tenno::default_tenno().clone();
        t.energy = energy;
        gate.0.holds(&t)
    };
    assert!(!at(150.0), "the neutral Tenno is under the gate — this is what M49 measured");
    assert!(!at(199.0), "under the threshold");
    assert!(at(200.0), "the card says 200 or higher, so 200 pays");
    assert!(at(300.0), "over the threshold");
    // The perk is NOT fully unmodelled — its +60 works, and the tile must
    // not tell a player the whole option is dead.
    assert!(!ca.fully_unmodeled());
    let cf = get("dual_toxocyst_commodores_fortune").unwrap();
    assert!(cf.effects.contains(&EvoEffect::FlatBaseCritChance(0.20)));
}

#[test]
fn broken_evolutions_apply_nothing() {
    use crate::model::WeaponBase;
    let with = WeaponBase::from_data("dual_toxocyst", false, &["dual_toxocyst_commodores_fortune", "dual_toxocyst_evolved_autoloader", "dual_toxocyst_fevered_frenzy"]);
    let mut probe = with.clone();
    apply(&mut probe, &[get("dual_toxocyst_ready_retaliation").unwrap()]);
    assert!((probe.base_vector.total() - with.base_vector.total()).abs() < 1e-9);
    assert_eq!(probe.base_crit_chance, with.base_crit_chance);
}

/// A broken evolution changes NOTHING — whatever it grants.
///
/// The test above can only be as strong as the data it picks, and no
/// SHIPPED broken evolution carries an effect `apply` would act on: both
/// of them resolve to something `apply` ignores anyway, so a regression in
/// the `currently_broken` filter would not have shown up there. This
/// builds a synthetic one carrying ONE OF EVERY effect `apply` writes
/// through, so the guard is on the filter itself rather than on today's
/// data — including the two write paths added on 2026-08-03
/// (`Indirect` and `AmmoMaxSet`), which reach fields the old test never
/// looked at.
#[test]
fn a_broken_evolution_changes_nothing_whatever_it_grants() {
    use crate::model::WeaponBase;
    use crate::model::IndirectStat;
    let everything = |broken: bool| EvolutionDef {
        misprints: Vec::new(),
        id: "synthetic".into(),
        name: "Synthetic".into(),
        weapon: "torid".into(),
        tier: 9,
        icon: None,
        description: String::new(),
        currently_broken: broken,
        co_base_excludes_this_evolution: None,
        co_base_excludes_only_form: None,
        base_form_only: false,
        effects: vec![
            EvoEffect::FlatBaseDamage(100.0),
            EvoEffect::FlatBaseDamageOnEmptyReload(50.0),
            EvoEffect::FlatBaseCritChance(0.5),
            EvoEffect::FlatBaseCritMultiplier(1.5),
            EvoEffect::FlatBaseStatusChance(0.5),
            EvoEffect::FlatBaseStatusChanceByForm { base: 0.4, incarnon: 0.9 },
            EvoEffect::FlatBaseMagazine(30.0),
            EvoEffect::Indirect(IndirectStat::Accuracy, 0.5),
            EvoEffect::AmmoMaxSet(999.0),
        ],
    };
    let base = WeaponBase::from_data("torid", false, &[]);

    let mut broken = base.clone();
    apply(&mut broken, &[&everything(true)]);
    assert!(
        (broken.base_vector.total() - base.base_vector.total()).abs() < 1e-9,
        "a broken evolution moved base damage"
    );
    assert_eq!(broken.base_crit_chance, base.base_crit_chance);
    assert_eq!(broken.base_crit_damage, base.base_crit_damage);
    assert_eq!(broken.base_status_chance, base.base_status_chance);
    assert_eq!(broken.magazine_size, base.magazine_size);
    assert_eq!(broken.ammo_reserve, base.ammo_reserve, "broken set the reserve");
    assert!(broken.indirect.is_empty(), "broken wrote an indirect stat: {:?}", broken.indirect);

    // ...and the SAME evolution unbroken must move every one of them, or
    // this test would pass on an `apply` that does nothing at all.
    let mut live = base.clone();
    apply(&mut live, &[&everything(false)]);
    assert!(live.base_vector.total() > base.base_vector.total());
    assert!(live.base_crit_chance > base.base_crit_chance);
    assert!(live.base_crit_damage > base.base_crit_damage);
    assert!(live.base_status_chance > base.base_status_chance);
    assert!(live.magazine_size > base.magazine_size);
    assert_eq!(live.ammo_reserve, 999.0);
    assert_eq!(live.indirect, vec![(IndirectStat::Accuracy, 0.5)]);
}

/// Final Fusillade is BASE FORM ONLY. Both forms load
/// the SAME evolution id — the gate has to be the form, not the id, so this
/// pins that the charge-backed form comes out with nothing.
#[test]
fn final_fusillades_last_round_multishot_skips_the_incarnon_form() {
use crate::model::WeaponBase;
    let evos = ["torid_final_fusillade"];
    let base = WeaponBase::from_data("torid", false, &evos);
    let inc = WeaponBase::from_data("torid_incarnon", false, &evos);
    assert!(
        (base.multishot_on_last_round - 3.0).abs() < 1e-9,
        "base form got {}",
        base.multishot_on_last_round
    );
    assert_eq!(
        inc.multishot_on_last_round, 0.0,
        "a charge-backed magazine has no last round to gate on"
    );
    // The flat base damage on the same evolution DOES reach both forms —
    // otherwise this test would pass on a build that dropped the whole
    // evolution rather than just its conditional half.
    let bare = WeaponBase::from_data("torid_incarnon", false, &[]);
    assert!(inc.base_vector.total() > bare.base_vector.total());
}

/// Extended Volley: "Does not apply to Incarnon Form's Magazine", and that
/// form uses max charges rather than a magazine. The
/// gate is load-bearing because an Incarnon form's `magazine_size` IS its
/// charge pool — an ungated `+=` quietly made it 179 rounds.
#[test]
fn extended_volley_leaves_the_charge_pool_alone() {
use crate::model::WeaponBase;
    let evos = ["torid_extended_volley"];
    let base = WeaponBase::from_data("torid", false, &evos);
    let inc = WeaponBase::from_data("torid_incarnon", false, &evos);
    assert!((base.magazine_size - 14.0).abs() < 1e-9, "5 + 9 = {}", base.magazine_size);
    assert!(
        (inc.magazine_size - 170.0).abs() < 1e-9,
        "the charge pool must stay 170, got {}",
        inc.magazine_size
    );
}
/// EVERY evolution effect that loads INERT, pinned.
///
/// An inert effect is a legitimate answer — "+50% Accuracy" decides
/// nothing in an arena with no geometry — but it is indistinguishable at a
/// glance from a MISSPELLED `kind:`, which also lands in `Inert(other)`
/// and silently contributes nothing. That is the failure this exists for:
/// the Boar's evolutions were written against a loader that had no
/// crit-multiplier arm, and only reading the loader by hand caught it.
///
/// So the set is written down. Adding an evolution whose effect does not
/// load fails here until someone states which it is — a mechanic the arena
/// cannot express, or a typo.
#[test]
fn the_inert_evolution_effects_are_the_ones_we_meant() {
    let mut found: Vec<String> = Vec::new();
    for def in pool() {
        for e in &def.effects {
            if let EvoEffect::Inert(what) = e {
                found.push(format!("{} :: {what}", def.id));
            }
        }
    }
    found.sort();
    // Each line is a DECISION, and the reason is beside the effect in its
    // own yaml. Kept as a flat list so a diff here is readable.
    let expected: Vec<&str> = vec![
        // (The four `unlocks_weapon` tier-1 entries are NOT here: they
        // apply nothing, the form being a separate weapon with its own
        // stats. Not INERT either — `UnlocksForm` carries the form's id,
        // which lets a form request imply the evolution that IS it.)
        // (RELOAD CADENCE keeps no Ready Retaliation. Only the
        // Phenmor's page states a window ("for 6 seconds") and the rest
        // stop at "+100% Reload Speed", because the buff is scoped to the
        // RELOAD ACTION — the Phenmor's 6 s is the buff icon's life rather
        // than the bonus's.)
        // ---- AMMO EFFICIENCY, and it is CONDITIONAL -----------------
        // Real DPS the moment a reserve runs dry, but one is gated on a
        // movement state and one on a headshot window, so applying either
        // unconditionally would overstate the build. They also land on the
        // Laetum's Incarnon magazine, which takes no efficiency at all.
        // ---- ONE-STACK STACKING BUFFS -------------------------------
        // A "timed buff" is a stacking buff with ONE stack, landing here
        // when its PAYLOAD is one the engine does not model. Ripper Rounds:
        // punch through, multi-target only. Neurotoxin: "+70% Toxin for 3 s
        // on headshot" — the one genuine gap here, though it is also
        // `currently_broken` and `apply` skips those, so they cancel out.
        "dual_toxocyst_neurotoxin :: stacking_buff toxin_damage",
        // WHAT EACH ENTRY IS WAITING ON: docs/INCARNON.md §"Perks this
        // loader does not model, and what each needs".
        //
        // An unknown effect kind is the only spelling that means "nothing
        // models this yet" and stays true — every kind that would fit pays
        // out UNCONDITIONALLY, so loading one grants a conditional bonus to
        // builds that do not meet the condition. `unmodeled_effects` is
        // derived from these same variants, so a perk's tile and this list
        // cannot disagree.
    ];
    // TWO POPULATIONS, AND THE PREFIX IS WHICH. The list above is the ARGUED
    // one: a hand-written perk whose effect the engine cannot express, where
    // an inert entry is a decision somebody made and wrote a reason for, and
    // where a NEW one appearing is a mistake until argued.
    //
    // The bulk Incarnon intake produces the other population.
    // Its rule engine turns a clause it does not recognise into a kind NAMED
    // `unmodelled_<the clause's own words>` — self-declaring by construction,
    // and there are hundreds of them, one per unrecognised clause per weapon.
    // Listing those individually would be a list nobody reads that grows by
    // 30 lines per adapter; the NAME is the declaration, and both the builder
    // and the optimizer print them as "not modelled yet".
    //
    // The invariant that still bites: the two populations may not mix. A
    // hand-written perk may not hide behind the prefix (its kind would have
    // to be renamed to do so, which is not something you do by accident), and
    // the argued list may not contain a prefixed kind.
    const BULK: &str = "unmodelled_";
    assert!(
        !expected.iter().any(|e| e.contains(&format!(":: {BULK}"))),
        "the argued list must not contain a bulk-intake kind — those declare              themselves by name"
    );
    let expected: Vec<String> = expected.into_iter().map(str::to_string).collect();
    let missing: Vec<&String> = expected.iter().filter(|e| !found.contains(e)).collect();
    let extra: Vec<&String> = found
        .iter()
        .filter(|f| !expected.contains(f))
        .filter(|f| !f.contains(&format!(":: {BULK}")))
        .collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "the inert set moved.
  NEW (implement it, or add it here with a reason, or let the intake name it   `unmodelled_*`): {extra:#?}
  GONE (drop it from the list): {missing:#?}"
    );
}
/// An evolution's HANDLING stats reach the resolved panel.
///
/// They have no single-target damage payload, which is what makes them
/// easy to drop — and dropping them means the evolution equips and its
/// number vanishes. This asserts the whole path: yaml -> loader ->
/// `WeaponBase.indirect` -> `resolve`'s bucket, in the same place a mod's
/// would land.
#[test]
fn an_evolutions_handling_stats_reach_the_panel() {
    use crate::build::loadout::resolve;
    use crate::model::{IndirectStat, StackPolicy};
    let of = |weapon: &str, evo: &str| -> Vec<(IndirectStat, f64)> {
        let base = crate::model::WeaponBase::from_data(weapon, true, &[evo]);
        resolve(&base, &[], StackPolicy::Emergent).indirect
    };
    let find = |v: &[(IndirectStat, f64)], want: IndirectStat| {
        v.iter().find(|(s, _)| *s == want).map(|(_, x)| *x)
    };

    // Practiced Grip: "+50% Accuracy".
    let grip = of("boar_prime", "boar_prime_practiced_grip");
    assert_eq!(find(&grip, IndirectStat::Accuracy), Some(0.50), "{grip:?}");

    // FORTRESS SALVO IS NOT HERE, deliberately. Its card reads "With
    // Armor Over 450: +4 Punch Through", so demanding the 4 metres in the
    // unconditional bucket would codify a perk requiring 450 armour paying
    // out with none. It is a `GatedByTenno` grant and reaches the panel
    // only through the gate;
    // `a_gated_perk_asks_the_frame_holding_the_gun` walks both sides.
    let salvo = of("boar_prime", "boar_prime_fortress_salvo");
    assert_eq!(find(&salvo, IndirectStat::PunchThrough), None, "{salvo:?}");

    // Marksman's Hand: "-50% Recoil". NEGATIVE, like the mods'.
    let hand = of("dual_toxocyst", "dual_toxocyst_marksmans_hand");
    assert_eq!(find(&hand, IndirectStat::Recoil), Some(-0.50), "{hand:?}");

    // Swift Deliverance: "+50% Projectile Speed", which was `unmodeled`.
    let swift = of("torid", "torid_swift_deliverance");
    assert_eq!(find(&swift, IndirectStat::ProjectileSpeed), Some(0.50), "{swift:?}");

    // Mercenary Chamber SETS the reserve rather than adding to a bucket.
    let base = crate::model::WeaponBase::from_data(
        "boar_prime", true, &["boar_prime_mercenary_chamber"],
    );
    assert_eq!(base.ammo_reserve, 195.0);
}
