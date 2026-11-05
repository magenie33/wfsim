use super::*;

#[test]
fn passive_exilus_strength_follows_its_rank_and_adds_to_other_mods() {
    for (card, frames) in [
        ("anchored_glide", &["zephyr", "zephyr_prime"][..]),
        ("controlled_slide", &["nezha", "nezha_prime"][..]),
        ("warriors_rest", &["excalibur_umbra"][..]),
    ] {
        for frame in frames {
            for (rank, bonus) in [0.025, 0.05, 0.075, 0.1, 0.125, 0.15].into_iter().enumerate() {
                let b = Build {
                    frame: (*frame).into(),
                    mods: vec![SlotPick { id: "intensify".into(), ..Default::default() }],
                    exilus: Some(SlotPick { id: card.into(), rank: Some(rank as u32), ..Default::default() }),
                    ..Build::default()
                };
                let resolved = resolve(&b).unwrap();
                assert!(resolved.refused.is_empty(), "{frame}: {:?}", resolved.refused);
                assert!((resolved.stat(FrameStat::AbilityStrength).value - (1.3 + bonus)).abs() < 1e-12,
                    "{frame} {card} rank {rank}");
                assert!(resolved.admissions.iter().any(|a| a.from == card && a.kind == AdmissionKind::Unmodelled),
                    "{card}: the passive-disabling clause is still unmodelled");
            }
        }
    }
}

#[test]
fn an_unmodelled_passive_clause_does_not_hide_the_ranked_strength_line() {
    for card in ["anchored_glide", "controlled_slide", "warriors_rest"] {
        let m = mod_by_id(card).unwrap();
        assert!(m.card_at(0).iter().any(|line| line == "+2.5% Ability Strength"), "{card}");
        assert_eq!(m.card_at(m.max_rank).join("\n"), m.description.shown(), "{card}");
    }
}
