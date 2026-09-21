use super::*;

fn build(mods: &[&str]) -> Build {
    Build {
        frame: "valkyr".into(),
        mods: mods.iter().map(|m| SlotPick { id: (*m).into(), rank: None }).collect(),
        ..Build::default()
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn every_file_loads_and_names_what_exists() {
    assert!(mods().len() > 100, "{}", mods().len());
    assert!(arcanes().len() > 50);
    let mut ids: Vec<&str> = mods().iter().map(|m| m.id.as_str()).collect();
    ids.dedup();
    assert_eq!(ids.len(), mods().len(), "a mod id is filed twice");
    for m in mods() {
        if let Some(a) = &m.augments {
            assert!(ability(a).is_some(), "{} augments `{a}`, which is not in data/warframe_abilities/", m.id);
        }
        if let Some(f) = &m.family {
            assert!(mods().iter().filter(|x| x.family.as_ref() == Some(f)).count() > 1, "{}: a family of one", m.id);
        }
    }
    for a in abilities() {
        assert!(!a.icon.is_empty(), "{} has no icon", a.id);
        for g in &a.augments {
            assert!(mod_by_id(g).is_some() || a.frame != "valkyr", "{} names augment {g}", a.id);
        }
    }
    for f in warframes() {
        assert_eq!(f.abilities.len(), if f.id == PROTOTYPE { 0 } else { 4 }, "{}", f.id);
        for id in &f.abilities {
            assert!(ability(id).is_some(), "{}: {id}", f.id);
        }
    }
    assert!(helminth_pool().count() > 70);
}

/// A MODELLED LINE RESTATED AT MAX RANK IS THE CARD ITSELF. The effect list
/// was read off each card, so a restatement that disagrees is a line the
/// model reads differently from the card.
#[test]
fn every_card_restates_itself_at_max_rank() {
    for m in mods() {
        assert_eq!(m.card_at(m.max_rank).join("\n"), m.description, "{}", m.id);
    }
    for a in arcanes() {
        assert_eq!(a.card_at(a.max_rank).join("\n"), a.description, "{}", a.id);
    }
    assert_eq!(mod_by_id("intensify").unwrap().card_at(0), vec!["+5% Ability Strength"]);
}

#[test]
fn a_bare_valkyr_is_her_rank_30_arsenal() {
    let r = resolve(&build(&[])).unwrap();
    assert_eq!(r.stat(FrameStat::Health).value, 750.0);
    assert_eq!(r.stat(FrameStat::Shield).value, 185.0);
    assert_eq!(r.stat(FrameStat::Armor).value, 855.0);
    assert_eq!(r.stat(FrameStat::Energy).value, 150.0);
    assert_eq!(r.stat(FrameStat::AbilityStrength).value, 1.0);
    assert_eq!(r.abilities.len(), 4);
    assert_eq!(r.abilities[1].ability.id, "warcry");
    assert_eq!(r.abilities[1].energy_cost, 75.0);
}

/// Umbral Vitality's table (W`Umbral_Vitality`): 9% at rank 0, 55% at rank 5.
#[test]
fn a_mod_scales_by_rank_plus_one() {
    assert!(close(at_rank(1.0, 0, 10), 1.0 / 11.0));
    assert_eq!((at_rank(1.0, 5, 10) * 100.0).round(), 55.0);
    let mut b = build(&["intensify"]);
    b.mods[0].rank = Some(0);
    assert!(close(resolve(&b).unwrap().stat(FrameStat::AbilityStrength).value, 1.05));
}

/// W`Umbral_Set`: Vitality 100% → 180% and Intensify 44% → 77% with all three.
#[test]
fn the_umbral_set_raises_each_card_by_its_own_share() {
    let r = resolve(&build(&["umbral_vitality", "umbral_fiber", "umbral_intensify"])).unwrap();
    assert!(close(r.stat(FrameStat::Health).bonus, 1.8));
    assert!(close(r.stat(FrameStat::Armor).bonus, 1.8));
    assert!(close(r.stat(FrameStat::AbilityStrength).bonus, 0.77));
    let two = resolve(&build(&["umbral_vitality", "umbral_intensify"])).unwrap();
    assert!(close(two.stat(FrameStat::Health).bonus, 1.3));
    assert!(close(two.stat(FrameStat::AbilityStrength).bonus, 0.55));
}

/// W`Warcry`: "855 × (1 + 1 + 0.5 × (1 + 0.3))" for Steel Fiber and Intensify.
#[test]
fn warcry_adds_to_base_armor_and_triples_in_hysteria() {
    let r = resolve(&build(&["steel_fiber", "intensify"])).unwrap();
    let w = &r.abilities[1];
    assert_eq!(w.derived.len(), 2);
    assert!(close(w.derived[0].value, 855.0 * (1.0 + 1.0 + 0.5 * 1.3)));
    assert!(close(w.derived[1].value, 855.0 * (1.0 + 1.0 + 3.0 * 0.5 * 1.3)));
}

/// W`Ability_Efficiency` and W`Energy_Capacity`'s worked example.
#[test]
fn cost_and_drain_follow_the_efficiency_page() {
    // Fleeting Expertise: +60% efficiency, -60% duration.
    let r = resolve(&build(&["fleeting_expertise", "streamline"])).unwrap();
    let eff = r.stat(FrameStat::AbilityEfficiency).value;
    assert!(close(eff, 1.9));
    assert!(close(r.abilities[1].energy_cost, 75.0 * 0.25), "the 25% floor");
    let h = &r.abilities[3];
    let dur = r.stat(FrameStat::AbilityDuration).value;
    assert!(close(h.drain_per_second.unwrap(), 5.0 * ((2.0 - eff) / dur).max(0.25)));
    // "300 × (1+1.85+0.25) + 75 = 1005" — here on Valkyr's 150.
    let mut b = build(&["primed_flow"]);
    b.shards.push(ShardPick { shard: "azure_archon_shard".into(), effect: "energy_max".into(), tauforged: true });
    assert!(close(resolve(&b).unwrap().stat(FrameStat::Energy).value, 150.0 * 2.85 + 75.0));
}

#[test]
fn the_helminth_replaces_one_slot_with_its_own_numbers() {
    let mut b = build(&[]);
    b.helminth = Some(HelminthPick { slot: 1, ability: "roar".into() });
    let r = resolve(&b).unwrap();
    assert_eq!(r.abilities[0].ability.id, "roar");
    assert!(r.abilities[0].helminth);
    assert!(r.refused.is_empty());
    // Her own ability is not a second copy.
    b.helminth = Some(HelminthPick { slot: 1, ability: "warcry".into() });
    assert_eq!(resolve(&b).unwrap().refused.len(), 1);
    let as_infused = ability("warcry").unwrap().stats[0].helminth_value;
    assert_eq!(as_infused, Some(0.3));
    // "Subsumed Pillage uses 50 energy instead of shields."
    b.helminth = Some(HelminthPick { slot: 1, ability: "pillage".into() });
    let r = resolve(&b).unwrap();
    assert_eq!(r.abilities[0].energy_cost, 50.0);
    assert!(!r.abilities[0].ability.infused_notes.is_empty());
}

#[test]
fn an_augment_without_its_ability_pays_nothing_and_says_so() {
    let mut b = build(&["eternal_war"]);
    assert!(resolve(&b).unwrap().admissions.iter().all(|a| a.kind != AdmissionKind::Inert));
    b.helminth = Some(HelminthPick { slot: 2, ability: "roar".into() });
    assert!(resolve(&b).unwrap().admissions.iter().any(|a| a.kind == AdmissionKind::Inert));
}

#[test]
fn a_slot_refuses_what_does_not_belong_in_it() {
    let mut b = build(&["intensify", "umbral_intensify", "steel_charge"]);
    b.exilus = Some(SlotPick { id: "intensify".into(), rank: None });
    let r = resolve(&b).unwrap();
    // a family twice, an aura in a main slot, a non-exilus in the exilus slot
    assert_eq!(r.refused.len(), 3, "{:?}", r.refused);
}

fn tags_of_build(b: &Build) -> Vec<(Capability, String)> {
    resolve(b).unwrap().tags.into_iter().map(|t| (t.tag, t.from)).collect()
}

/// Rolling Guard: "grants a brief period of invulnerability and removes all
/// Status Effects when rolling"; Valkyr's passive grants invulnerability.
#[test]
fn a_tag_names_every_source_that_grants_it() {
    let b = build(&["rolling_guard"]);
    let t = tags_of_build(&b);
    assert!(t.contains(&(Capability::Invulnerable, "valkyr".into())));
    assert!(t.contains(&(Capability::Invulnerable, "rolling_guard".into())));
    assert!(t.contains(&(Capability::StatusCleanse, "rolling_guard".into())));
    assert!(!t.iter().any(|(c, _)| *c == Capability::DamageCap));
    // Hysteria: "becoming immune to Status Effects" — an immunity, not a cleanse.
    assert!(t.contains(&(Capability::StatusImmunity, "hysteria".into())));
    assert!(!t.contains(&(Capability::StatusCleanse, "hysteria".into())));
}

/// "Subsumed Omamori ... cannot gain invulnerability", and Well of Life's
/// infused cooldown is 120 s.
#[test]
fn an_infused_ability_carries_the_infused_versions_tags() {
    let mut b = build(&[]);
    b.helminth = Some(HelminthPick { slot: 1, ability: "omamori".into() });
    assert!(!tags_of_build(&b).iter().any(|(_, f)| f == "omamori"));
    b.helminth = Some(HelminthPick { slot: 1, ability: "well_of_life".into() });
    let r = resolve(&b).unwrap();
    let w = r.tags.iter().find(|t| t.from == "well_of_life").expect("the infused Well of Life");
    assert!(w.when.contains("120 s"), "{}", w.when);
}

/// An always-on node counts, a conditional one only when assumed, and the
/// active school's tags count either way.
#[test]
fn the_operator_counts_what_is_always_on_and_what_is_assumed() {
    let mut b = build(&[]);
    b.operator = Some(OperatorPick { school: "unairu".into(), assumed: vec![], ..Default::default() });
    let r = resolve(&b).unwrap();
    assert_eq!(r.stat(FrameStat::Armor).value, 855.0 + 200.0, "Stone Skin");
    assert!(r.tags.iter().any(|t| t.from == "focus:unairu:reinforced_return"));
    b.operator = Some(OperatorPick { school: "madurai".into(), assumed: vec![], ..Default::default() });
    assert_eq!(resolve(&b).unwrap().stat(FrameStat::AbilityStrength).value, 1.0);
    b.operator = Some(OperatorPick {
        school: "madurai".into(),
        assumed: vec!["sling_strength".into()],
        ..Default::default()
    });
    assert!(close(resolve(&b).unwrap().stat(FrameStat::AbilityStrength).value, 1.4));
}

/// Every school has its artifact, every card's school and `bonus_per` name a
/// school, and a seating the artifact cannot hold is refused.
#[test]
fn the_artifact_seats_five_known_mods_once_and_one_arcane() {
    assert_eq!(artifact_mods().len(), 20);
    assert_eq!(artifact_arcanes().len(), 5);
    for s in focus_schools() {
        assert!(s.artifact.is_some(), "{} has no artifact", s.id);
    }
    for m in artifact_mods() {
        assert!(focus_school(&m.school).is_some(), "{}: school {}", m.id, m.school);
        if let Some(b) = &m.bonus {
            assert!(b.per == "unique_school" || focus_school(&b.per).is_some(), "{}: bonus per {}", m.id, b.per);
            assert_eq!(m.description.lines().count(), 2, "{}: the bonus is the second line", m.id);
        }
    }

    let mut b = build(&[]);
    let pick = |mods: &[&str], arcane: Option<&str>| OperatorPick {
        school: "madurai".into(),
        assumed: vec![],
        artifact: ArtifactPick {
            mods: mods.iter().map(|s| s.to_string()).collect(),
            arcane: arcane.map(str::to_string),
        },
    };
    b.operator = Some(pick(&["ubri_kaneph", "da_ren", "omn_evi", "yar_dal", "sey_taph"], Some("zid_an_asheir")));
    assert!(resolve(&b).unwrap().refused.is_empty());
    b.operator = Some(pick(&["ubri_kaneph", "ubri_kaneph"], None));
    assert!(resolve(&b).unwrap().refused.iter().any(|r| r.contains("seated twice")));
    b.operator = Some(pick(&["ubri_kaneph", "da_ren", "omn_evi", "yar_dal", "sey_taph", "evir_ti"], None));
    assert!(resolve(&b).unwrap().refused.iter().any(|r| r.contains("5 mod slots")));
    b.operator = Some(pick(&[], Some("arcane_grace")));
    assert!(resolve(&b).unwrap().refused.iter().any(|r| r.contains("unknown artifact arcane")));

    // M93: two Madurai, two Vazarin, one Naramon.
    let seated: Vec<&ArtifactMod> = ["ubri_kaneph", "sil_tabol", "da_ren", "metem_hakh", "omn_evi"]
        .iter()
        .map(|id| artifact_mod_by_id(id).unwrap())
        .collect();
    let card = |id: &str| artifact_mod_by_id(id).unwrap().card_with(&seated);
    assert_eq!(card("ubri_kaneph"), ["+60% Damage to Amps", "+20% Amp Damage"], "Vazarin and Naramon, not Madurai");
    assert_eq!(card("metem_hakh")[1], "+30% Operator Health & Shields", "Madurai and Naramon, not Vazarin");
    assert_eq!(card("sil_tabol")[1], "+30% Status Damage", "each Vazarin card");
    assert_eq!(card("da_ren")[1], "+0 Operator Shields", "no Unairu card");
}

/// W`Shield`'s formula at Valkyr's 185, Catalyzing Shields' x0.20 and 1.33 s,
/// and a cast that refills past max giving the full gate under every reading.
#[test]
fn the_shield_gate_follows_the_shield_page_and_catalyzing_shields() {
    let bare = resolve(&build(&[])).unwrap();
    assert!(close(bare.shield_gate.full_seconds, (185.0f64 / 350.0).powf(0.65) + 1.0 / 3.0));
    assert!(bare.shield_gate.casts.is_empty(), "no refill source, no re-opened gate");

    let mut b = build(&["catalyzing_shields", "augur_secrets", "augur_message"]);
    b.aura = Some(SlotPick { id: "brief_respite".into(), rank: None });
    let r = resolve(&b).unwrap();
    assert!(close(r.stat(FrameStat::Shield).value, 185.0 * 0.2));
    assert!(close(r.shield_gate.full_seconds, 1.33));
    // two Augur cards (80%) + Brief Respite (150%)
    assert!(close(r.shield_gate.energy_to_shield, 2.3));
    let warcry = r.shield_gate.casts.iter().find(|c| c.ability == "warcry").unwrap();
    assert!(warcry.full, "75 energy x2.3 refills 37 shields");
    let t = r.tags.iter().find(|t| t.from == "shield_gate").expect("the derived tag");
    assert_eq!(t.tag, Capability::Invulnerable);

    // ONE Augur card (40%): Rip Line's 25 energy restores 10 of 37 shields,
    // and Catalyzing Shields still gives its 1.33 s (MEASUREMENTS M92).
    let partial = resolve(&build(&["catalyzing_shields", "augur_secrets"])).unwrap();
    let rip = partial.shield_gate.casts.iter().find(|c| c.ability == "rip_line").unwrap();
    assert!(!rip.full);
    assert!(close(rip.seconds, 1.33));
    // …and without it, the gate is the refill's own: W`Shield`'s formula at 10.
    let bare = resolve(&build(&["augur_secrets"])).unwrap();
    let rip = bare.shield_gate.casts.iter().find(|c| c.ability == "rip_line").unwrap();
    assert!(close(rip.seconds, 10.0 / 180.0 + 1.0 / 3.0));
}

#[test]
fn aura_capacity_doubles_matched_and_floors_mismatched() {
    assert_eq!(aura_capacity(7, "madurai", Some("madurai")), 14);
    assert_eq!(aura_capacity(7, "madurai", None), 7);
    assert_eq!(aura_capacity(9, "madurai", Some("naramon")), 7);
    assert_eq!(aura_capacity(5, "madurai", Some("naramon")), 4);
}

/// THE PROTOTYPE BUILD IS THE FIGHT'S FLOOR, and a Helminth has no slot on it.
#[test]
fn a_bare_prototype_is_the_floor_wielder() {
    let t = crate::data::tenno::default_tenno();
    let mut b = Build { frame: PROTOTYPE.into(), ..Build::default() };
    let r = resolve(&b).unwrap();
    assert_eq!(r.stat(FrameStat::Health).value, t.health);
    assert_eq!(r.stat(FrameStat::Shield).value, t.shield);
    assert_eq!(r.stat(FrameStat::Armor).value, t.armor);
    assert_eq!(r.stat(FrameStat::Energy).value, t.energy);
    assert_eq!(r.stat(FrameStat::SprintSpeed).value, t.sprint);
    assert!(r.abilities.is_empty());
    b.helminth = Some(HelminthPick { slot: 1, ability: "roar".into() });
    assert!(!resolve(&b).unwrap().refused.is_empty(), "nothing to infuse");
}
