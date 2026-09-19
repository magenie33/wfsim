/// A STANCE COMBO RUNS FOR ITS PUBLISHED DURATION AT ITS PUBLISHED RATE,
/// and the two columns pin each other.
///
/// `Module:Stances/data` gives `Duration`; the rendered stance table prints
/// the combo's damage-per-second beside it. A DROPPED damage row leaves the
/// rate wrong; a RE-TIMED input leaves the duration wrong. Neither half can
/// pass alone, which is what a transcription of this table has instead of a
/// measurement.
///
/// It is the check that was missing while Falling Rock ran for 3.03 s
/// against its published 4.90: the duration had been DERIVED as
/// `total / rate`, and that divides back out to the printed rate however
/// many rows were lost on the way.
#[test]
fn a_stance_combo_runs_for_its_published_duration_at_its_published_rate() {
    // stance, form, `Duration`, %/s — the wiki's own two columns.
    const TABLE: &[(&str, &str, f64, f64)] = &[
        ("crushing_ruin", "neutral", 3.00, 466.7),
        ("crushing_ruin", "forward", 2.60, 307.7),
        ("crushing_ruin", "block", 2.25, 400.0),
        ("crushing_ruin", "block_forward", 4.25, 400.0),
        ("shattering_storm", "neutral", 4.90, 428.6),
        ("shattering_storm", "forward", 2.60, 346.2),
        ("shattering_storm", "block", 3.30, 333.3),
        ("shattering_storm", "block_forward", 3.55, 507.0),
        ("sovereign_outcast", "neutral", 2.85, 771.9),
        ("sovereign_outcast", "forward", 1.75, 514.3),
        ("sovereign_outcast", "block", 1.25, 720.0),
        ("sovereign_outcast", "block_forward", 3.00, 466.7),
        ("gemini_cross", "neutral", 4.60, 467.4),
        ("gemini_cross", "forward", 1.20, 333.3),
        ("gemini_cross", "block", 2.85, 561.4),
        ("gemini_cross", "block_forward", 2.30, 565.2),
    ];
    // Both classes in the roster, so a stance is found wherever it lives.
    let pool: Vec<crate::model::ModDef> = crate::data::mods::pool_for_weapon("magistar")
        .into_iter()
        .chain(crate::data::mods::pool_for_weapon("praedos"))
        .collect();
    for &(stance, form, duration, rate) in TABLE {
        let m = pool.iter().find(|m| m.id == stance).expect("stance in a class pool");
        let rows = m
            .stance
            .expect("a stance carries combos")
            .iter()
            .find_map(|(f, hits)| (*f == form).then_some(*hits))
            .unwrap_or_else(|| panic!("{stance} has no {form} combo"));
        let clock: f64 = rows.iter().map(|h| h.delay_seconds).sum();
        // A SLAM ROW IS NOT IN THE RATE. `calcTotalDmgMulti` skips
        // `Types = { "Slam" }` — the radial does not land on the body the
        // swing struck — and here such a row carries `multiplier: 0.0`, so
        // it drops out of this sum for the same reason without a flag.
        let damage: f64 = rows.iter().map(|h| h.multiplier * f64::from(h.hits)).sum();
        assert!(
            (clock - duration).abs() < 1e-3,
            "{stance}/{form}: {clock:.4} s against a published {duration}",
        );
        assert!(
            (damage * 100.0 / clock - rate).abs() < 0.1,
            "{stance}/{form}: {:.1}%/s against a published {rate}",
            damage * 100.0 / clock,
        );
    }
}
