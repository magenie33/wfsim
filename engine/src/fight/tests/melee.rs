use super::*;



fn magistar_evo(form: &str, evos: &[&str]) -> Summary {
    magistar_evo_with(form, evos, &[])
}

/// …AND WITH THE WINDOW HELD OPEN, which is what the card's "no timeout"
/// knob means (docs/BUFFS.md): the run opens with the Incarnon up and it
/// never closes. It is how a test asks what the Genesis is WORTH without
/// also asking whether this mode can arm it — two questions, two tests.
fn magistar_evo_armed(form: &str, evos: &[&str], mods: &[&str]) -> Summary {
    magistar_params(form, evos, mods, &[("melee_incarnon", (1, true))])
}

/// …AND WITH MODS, which a melee Incarnon needs: arming it is a heavy
/// attack at 6x combo, and a heavy mode earns no points of its own.
fn magistar_evo_with(form: &str, evos: &[&str], mods: &[&str]) -> Summary {
    magistar_params(form, evos, mods, &[])
}

/// `cfg` is the buff CARDS, by id — the configured policy, which is how a
/// test says "this player holds four stacks" without asking the arena to
/// produce the kills that earn them.
fn magistar_params(
    form: &str,
    evos: &[&str],
    mods: &[&str],
    cfg: &[(&str, (u32, bool))],
) -> Summary {
    let base = crate::model::WeaponBase::from_data(form, true, evos);
    let pool = crate::data::mods::pool_for_weapon(form);
    let refs: Vec<&crate::model::ModDef> =
        mods.iter().filter_map(|id| pool.iter().find(|m| m.id == *id)).collect();
    let panel = crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
    let arena = crate::arena::Arena::training(60.0);
    // The same decision the page makes — see `melee_fight`.
    let p = FightParams::for_panel(&panel, &arena, &crate::data::arcanes::ArcaneFx::none(), || {
        let unarmed: Vec<&str> = evos
            .iter()
            .copied()
            .filter(|id| !crate::data::evolutions::states_incarnon_window(id))
            .collect();
        let b = crate::model::WeaponBase::from_data(form, true, &unarmed);
        crate::build::loadout::resolve(&b, &refs, crate::model::StackPolicy::Emergent)
    });
    let mut p = p;
    if !cfg.is_empty() {
        let mut c = BuffConfig::new();
        for (id, v) in cfg {
            c.insert((*id).into(), *v);
        }
        p.apply_buff_config(&c);
    }
    monte_carlo(&p, 30, 3)
}

/// **A STANCE DECIDES WHAT THE WEAPON FIRES**, which no mod in this
/// roster had ever done before.
///
/// Every other card changes what a weapon fires WITH. A stance publishes a
/// combo per form and installing one replaces the entry's own script, so
/// the same Magistar in the same mode is a different sequence of swings
/// under Crushing Ruin (Raging Whirlwind: 1400% over 3.00 s in three
/// inputs) and under Shattering Storm (Falling Rock: 2100% over 4.90 s in
/// four, each of them ending on a slam).
///
/// IT NEEDS NO SLOT OF ITS OWN, and that is what made it cheap: a stance is
/// legal in the stance slot and nowhere else, so a flat mod list can say
/// which entry is the stance by looking at it — exactly what the exilus
/// slot could not do.
#[test]
fn a_stance_decides_what_the_weapon_fires() {
    let script = |mods: &[&str], form: &str| {
        let base = crate::model::WeaponBase::from_data(form, false, &[]);
        let pool = crate::data::mods::pool_for_weapon(form);
        let refs: Vec<&crate::model::ModDef> =
            mods.iter().filter_map(|id| pool.iter().find(|m| m.id == *id)).collect();
        let p = crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        p.combo_script.iter().map(|h| h.multiplier).collect::<Vec<_>>()
    };
    let bare = script(&[], "magistar");
    let ruin = script(&["crushing_ruin"], "magistar");
    let storm = script(&["shattering_storm"], "magistar");
    assert_eq!(bare, ruin, "the entry's own script IS Crushing Ruin's");
    assert_ne!(
        ruin, storm,
        "two stances gave the same neutral combo: {ruin:?}",
    );
    // …AND ONLY THE FORM IT SUPPLIES. A stance's block combo replaces the
    // block form's script and leaves the neutral one alone, which is what
    // "keyed by form" has to mean.
    assert_ne!(
        script(&["shattering_storm"], "magistar_block"),
        script(&["shattering_storm"], "magistar"),
        "one stance gave the same script to two different forms",
    );
    // THE HEAVY IS THE SAME ON BOTH, which is a fact about hammers rather
    // than about this code — a hammer's heavy is its class multiplier, and
    // the stance decides only the animation. Asserted so a future stance
    // that DOES differ cannot land unnoticed.
    assert_eq!(
        script(&["crushing_ruin"], "magistar_heavy"),
        script(&["shattering_storm"], "magistar_heavy"),
        "the two hammer stances' heavy attacks disagree",
    );
    // …and it reaches the FIGHT, not just the panel.
    let dps = |mods: &[&str]| {
        magistar("magistar", mods, 30.0, None).mean_damage
    };
    assert_ne!(
        format!("{:.0}", dps(&["crushing_ruin"])),
        format!("{:.0}", dps(&["shattering_storm"])),
        "two stances produced the same fight",
    );
}

/// **A MELEE INCARNON IS A BUFF, NOT A FORM**: it changes numbers and not animations, so there is no
/// second weapon entry and the whole Genesis is stat changes on the seven
/// ways the weapon is already swung.
///
/// EVO1 IS EXACTLY 2x on a build with nothing else in the base-damage
/// bucket, which is what `+100% Melee Damage` means when it is a BRACKET
/// term rather than a flat addition — and telling those two apart is the
/// whole reason `EvoEffect::BaseDamageBonus` exists beside
/// `FlatBaseDamage`.
#[test]
fn the_incarnon_form_is_a_hundred_per_cent_in_the_pressure_point_bucket() {
    let bare = magistar_evo_armed("magistar", &[], &[]).mean_damage;
    let form =
        magistar_evo_armed("magistar", &["magistar_evo1_incarnon_form"], &[]).mean_damage;
    assert!(
        (form / bare - 2.0).abs() < 0.02,
        "+100% in an empty bucket should be exactly 2x: {bare:.0} -> {form:.0}",
    );
}

/// **A MELEE INCARNON IS EARNED, AND WHAT EARNS IT IS A HEAVY ATTACK.**
///
/// *"Reach 6x Combo and then Heavy Attack to activate Incarnon Form for 180
/// seconds"* — and the heavy attack is the whole of the condition:
/// a stationary heavy, a heavy slam, or a TENNOKAI heavy, which is what
/// gives a light combo mode any way in at all, since its loop performs no
/// heavy of its own.
///
/// The three cases are the three answers, and they are asserted together
/// because each one alone reads as a bug:
///
/// 1. A BARE HEAVY BUILD CANNOT ARM IT. A heavy attack earns no combo
///    points, so its counter is the initial-combo FLOOR and nothing else —
///    zero without a card, which is 1x, which is not 6x. Installing the
///    Genesis on that build changes not one number.
/// 2. …AND A FLOOR THAT REACHES 6x DOES. Corrupt Charge's +30 and
///    Galvanized Reflex's earned +80 are 110 points, and 6x is 100.
/// 3. A LIGHT COMBO MODE ARMS IT THROUGH TENNOKAI, whose free heavy is a
///    heavy attack like any other.
#[test]
fn a_melee_incarnon_is_earned_and_a_heavy_attack_is_what_earns_it() {
    let evo = ["magistar_evo1_incarnon_form"];
    // 1. Nothing to arm it with.
    let bare = magistar_evo_with("magistar_heavy", &[], &[]).mean_damage;
    let bare_evo = magistar_evo_with("magistar_heavy", &evo, &[]).mean_damage;
    assert_eq!(
        bare, bare_evo,
        "a heavy build with no combo floor cannot reach 6x, so the Genesis pays nothing"
    );
    // 2. …and a floor that gets there. Corrupt Charge's +30 alone is 2x, so
    //    the four earned stacks of Galvanized Reflex are what closes it —
    //    HELD rather than earned, because this fixture's target does not
    //    die and a card's stack count is exactly the knob for that.
    let floor = ["corrupt_charge", "galvanized_reflex"];
    let held = [("galvanized_reflex", (4, true))];
    let off = magistar_params("magistar_heavy", &[], &floor, &held).mean_damage;
    let on = magistar_params("magistar_heavy", &evo, &floor, &held).mean_damage;
    assert!(
        on > off * 1.5,
        "a floor of 110 points is 6x and must arm it: {off:.0} -> {on:.0}"
    );
    // …AND CORRUPT CHARGE ALONE IS NOT ENOUGH: +30 is 2x, and 6x is 100
    // points, so the same Genesis on the same mode pays nothing.
    let thin = magistar_params("magistar_heavy", &[], &["corrupt_charge"], &[]).mean_damage;
    let thin_evo =
        magistar_params("magistar_heavy", &evo, &["corrupt_charge"], &[]).mean_damage;
    assert_eq!(thin, thin_evo, "+30 initial combo is 2x, which is not 6x");
    // 3. …and a light combo mode, through Tennokai's free heavy.
    let tk = ["disciplines_merit"];
    let light_off = magistar_evo_with("magistar", &[], &tk).mean_damage;
    let light_on = magistar_evo_with("magistar", &evo, &tk).mean_damage;
    // BELOW THE HELD 2x ON PURPOSE: the fight opens un-armed and spends the
    // first seconds climbing to 6x and waiting on a Tennokai flash, so the
    // Genesis is worth less than the ceiling `magistar_evo_armed` measures.
    // That gap IS the window being earned.
    assert!(
        (1.3..1.9).contains(&(light_on / light_off)),
        "a Tennokai heavy is a heavy attack and must arm it, part-way in: {light_off:.0} -> {light_on:.0}"
    );
    // …AND WITHOUT ONE IT STAYS SHUT, which is what makes the line above a
    // statement about Tennokai rather than about the light mode.
    let plain_off = magistar_evo_with("magistar", &[], &[]).mean_damage;
    let plain_on = magistar_evo_with("magistar", &evo, &[]).mean_damage;
    assert_eq!(
        plain_off, plain_on,
        "a light combo loop performs no heavy attack, so it never arms it"
    );
}

/// **THE PURE-HEAVY BUILD IS THREE MULTIPLIERS, AND THE THIRD IS A CLOCK.**
///
/// The Magistar's Incarnon Form is worth 6x to a heavy loop against 2x to a
/// light one, and the difference is not a bigger number — it is the same
/// `+100%` damage, TIMES a combo multiplier the +30 initial combo buys
/// (1x to 2x), TIMES more swings from `+50% Heavy Attack Wind Up Speed`,
/// which shortens the CHARGE and leaves the swing after it alone.
///
/// THE WIND-UP IS THE ONE THAT COULD SILENTLY NOT WORK, so it is asserted
/// on the SHOT COUNT: a 0.4 s charge becomes 0.27 s ahead of the 0.8 s
/// swing, so the cycle goes from 1.2 s to 1.07 s — about 1.1x the swings.
#[test]
fn the_incarnon_form_pays_a_heavy_build_three_ways() {
    let bare = magistar_evo_armed("magistar_heavy", &[], &[]);
    let form = magistar_evo_armed("magistar_heavy", &["magistar_evo1_incarnon_form"], &[]);
    assert!(
        (1.05..1.15).contains(&(form.mean_shots / bare.mean_shots)),
        "a 0.4 s charge at +50% speed is 0.27 s before a 0.8 s swing, so about 1.1x the swings: {:.0} -> {:.0}",
        bare.mean_shots, form.mean_shots,
    );
    let gain = form.mean_damage / bare.mean_damage;
    assert!(
        (3.8..5.0).contains(&gain),
        "damage 2x, combo 1x->2x and swings 1.1x should be about 4.4x, got x{gain:.2}",
    );
    // …AND SWIFT BREAK IS ADDITIVE WITH IT, in the same bucket: 0.4 / 1.8
    // is 0.22 s, a cycle of 1.02 s, so strictly more swings.
    let swift = magistar_evo_armed(
        "magistar_heavy",
        &["magistar_evo1_incarnon_form", "magistar_swift_break"],
        &[],
    );
    assert!(
        swift.mean_shots > form.mean_shots,
        "+30% more wind-up speed bought nothing: {:.0} -> {:.0}",
        form.mean_shots, swift.mean_shots,
    );
}

/// **FLASHING BLEED IS WORTH A THIRD OF THIS WEAPON**, which is why it is
/// modelled rather than declared.
///
/// The Magistar is 80% Impact and every one of its four combos forces an
/// Impact proc somewhere in the sequence, so `+50% Chance of a Slash Status
/// Effect on an Impact Status Effect` is a bleed off most of them. It
/// reaches the same runtime Hemorrhage does.
#[test]
fn flashing_bleed_turns_this_weapons_impact_into_bleeds() {
    let form = magistar_evo("magistar", &["magistar_evo1_incarnon_form"]);
    let bleed = magistar_evo(
        "magistar",
        &["magistar_evo1_incarnon_form", "magistar_flashing_bleed"],
    );
    assert!(
        bleed.mean_damage > form.mean_damage * 1.15,
        "an Impact-to-Slash roll on an 80% Impact weapon bought only {:.0} -> {:.0}",
        form.mean_damage, bleed.mean_damage,
    );
    assert!(
        bleed.mean_dot_damage > form.mean_dot_damage * 1.5,
        "the extra damage did not arrive as a DoT: {:.0} -> {:.0}",
        form.mean_dot_damage, bleed.mean_dot_damage,
    );
}

/// REACH IS METRES AND METRES ARE BODIES — so an evolution that buys reach
/// buys NOTHING against one target, and that is the assertion.
///
/// Orokin Reach is `+1.4 Range`, which on a hammer's 2.5 m is more than half
/// again. Against a lone body at contact it is worth exactly zero, and a
/// perk that quietly paid damage for reach would be caught here.
#[test]
fn orokin_reach_buys_metres_and_not_damage() {
    let form = magistar_evo("magistar", &["magistar_evo1_incarnon_form"]).mean_damage;
    let reach = magistar_evo(
        "magistar",
        &["magistar_evo1_incarnon_form", "magistar_orokin_reach"],
    )
    .mean_damage;
    assert!(
        (reach / form - 1.0).abs() < 0.01,
        "reach paid damage against one body: {form:.0} -> {reach:.0}",
    );
    let base = crate::model::WeaponBase::from_data(
        "magistar", true, &["magistar_evo1_incarnon_form", "magistar_orokin_reach"],
    );
    let refs: Vec<&crate::model::ModDef> = Vec::new();
    let panel = crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
    assert!(
        (panel.range_m - 3.9).abs() < 1e-9,
        "2.5 m + 1.4 m should be 3.9 m, got {:.2}",
        panel.range_m,
    );
}

/// **A MOD THAT NAMES AN ATTACK PAYS ON THAT ATTACK AND ON NO OTHER.**
///
/// Three cards say so in their own words — Killing Blow is "+120% Melee
/// Damage on Heavy Attack", Seismic Wave is "+200% Slam Attack Damage" —
/// and the seven melee modes are what makes that checkable at all: the same
/// build, the same target, one card, and it moves one mode and not another.
///
/// THE NEGATIVE HALF IS THE POINT. A bucket wired into the base-damage
/// bracket would pass the first assertion of each pair perfectly and pay
/// every mode, which is the exact bug these two cards invite.
#[test]
fn a_card_that_names_an_attack_pays_on_that_attack_alone() {
    let gain = |form: &str, m: &str| {
        magistar(form, &[m], 30.0, None).mean_damage / magistar(form, &[], 30.0, None).mean_damage
    };
    let kb_heavy = gain("magistar_heavy", "killing_blow");
    let kb_light = gain("magistar", "killing_blow");
    assert!(kb_heavy > 1.5, "Killing Blow bought a heavy build nothing: x{kb_heavy:.3}");
    assert!(
        (kb_light - 1.0).abs() < 0.02,
        "Killing Blow paid an ordinary swing x{kb_light:.3} — it says `on Heavy Attack`",
    );

    let sw_slam = gain("magistar_heavy_slam", "seismic_wave");
    // Tidal Force: the one Crushing Ruin combo that ends on no slam.
    let sw_light = gain("magistar_forward", "seismic_wave");
    assert!(sw_slam > 1.5, "Seismic Wave bought a slam build nothing: x{sw_slam:.3}");
    assert!(
        (sw_light - 1.0).abs() < 0.02,
        "Seismic Wave paid an ordinary swing x{sw_light:.3} — it says `Slam Attack Damage`",
    );
}

/// REACH IS METRES, AND METRES ARE BODIES.
///
/// DE's card reads `+3 Range`, not a percentage, which on a hammer's 2.5 m
/// is more than a doubling — so the assertion is not that the number went
/// up but that the SWING FOUND MORE PEOPLE. A ring at 4.0 m centre to
/// centre is a gap of 3.5 m: outside a bare hammer's reach and inside a
/// Primed Reach one.
#[test]
fn reach_is_metres_and_metres_are_bodies() {
    let bare = crate::model::WeaponBase::from_data("magistar", false, &[]);
    let pool = crate::data::mods::pool_for_weapon("magistar");
    let pr: Vec<&crate::model::ModDef> =
        pool.iter().filter(|m| m.id == "primed_reach").collect();
    let panel = crate::build::loadout::resolve(&bare, &pr, crate::model::StackPolicy::Emergent);
    assert!(
        (panel.range_m - 5.5).abs() < 1e-9,
        "2.5 m + 3 m should be 5.5 m, got {:.2}",
        panel.range_m,
    );
    let reached = |mods: &[&str]| {
        let r = magistar("magistar_forward", mods, 20.0, Some(4.0));
        r.mean_damage_by_body.0.iter().filter(|d| **d > 0.0).count()
    };
    assert_eq!(reached(&[]), 1, "a 2.5 m swing should find only the body at contact");
    // REACH IS THE WEDGE'S RADIUS, NOT ITS ANGLE, and the two combos show
    // both halves on the same ring. Tidal Force spins once
    // (`Types = { "360" }`), so once the radius covers the ring it takes all
    // nine; Winding Temper's swings are all sweeps, so it takes the aimed body
    // and the three inside `MELEE_ARC_DEG` — the ring stands at 45-degree
    // steps, and a 90-degree wedge holds the one in front and one either side.
    // Its closing SLAM is taken off, because a slam is a sphere.
    let sweep = |mods: &[&str]| {
        let r = swings_only("magistar_block", mods, Some(4.0));
        r.mean_damage_by_body.0.iter().filter(|d| **d > 0.0).count()
    };
    assert_eq!(reached(&["primed_reach"]), 9, "a 5.5 m spin should take the whole ring");
    assert_eq!(sweep(&[]), 1, "a 2.5 m sweep should find only the body at contact");
    assert_eq!(sweep(&["primed_reach"]), 4, "a 5.5 m sweep takes its wedge and no more");
}

/// **CORRUPT CHARGE IS A TRADE, AND BOTH HALVES REACH THE FIGHT** — but
/// only one of them can be MEASURED on this weapon, and that is a finding
/// rather than a gap.
///
/// `+30 Initial Combo, -50% Combo Duration`. The floor pays a heavy build,
/// which is the first half. The clock is halved from 5.0 s to 2.5 s and
/// costs the Magistar NOTHING, because it swings every 0.90 s and refreshes
/// the counter four times over inside even the shortened window. That is
/// why the card is a staple rather than a trade for most of the roster: the
/// penalty bites a slow weapon and a build that stops hitting, and a hammer
/// on a target that never dies is neither.
///
/// So the clock is asserted where it is exact — on the PANEL, including the
/// order it composes with Body Count in, which is the one thing that could
/// silently go wrong.
#[test]
fn corrupt_charge_buys_the_floor_and_sells_the_clock() {
    let heavy = magistar("magistar_heavy", &["corrupt_charge"], 30.0, None).mean_damage
        / magistar("magistar_heavy", &[], 30.0, None).mean_damage;
    assert!(
        heavy > 1.3,
        "+30 initial combo should put every heavy at 2x: x{heavy:.3}",
    );
    let clock = |mods: &[&str]| {
        let base = crate::model::WeaponBase::from_data("magistar", false, &[]);
        let pool = crate::data::mods::pool_for_weapon("magistar");
        let refs: Vec<&crate::model::ModDef> =
            mods.iter().filter_map(|id| pool.iter().find(|m| m.id == *id)).collect();
        crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent)
            .combo_duration_seconds
    };
    assert!((clock(&[]) - 5.0).abs() < 1e-9, "the weapon's own clock is 5 s");
    assert!((clock(&["corrupt_charge"]) - 2.5).abs() < 1e-9, "-50% of 5 s is 2.5 s");
    // SECONDS FIRST, THEN THE RELATIVE CARD. `(5 + 12) x 0.5 = 8.5`, and the
    // other order would give `5 x 0.5 + 12 = 14.5` — a 70% difference from
    // one line of arithmetic, which is why it is pinned.
    assert!(
        (clock(&["body_count", "corrupt_charge"]) - 8.5).abs() < 1e-9,
        "(5 + 12) x 0.5 should be 8.5, got {:.2}",
        clock(&["body_count", "corrupt_charge"]),
    );
}

/// **A COMBO DURATION MALUS STOPS THE COUNTER, it does not shorten it** —
/// *"A zero or negative combo duration prevents increasing the combo
/// counter"*, which is a harder stop than the 0.1 s floor beside it.
///
/// Nothing in the mod pool reaches it and a melee RIVEN does: Combo
/// Duration's malus is -8.2 s at the Magistar's 1.35 disposition, against a
/// weapon whose own clock is five seconds. So the rule is asserted where it
/// is visible — Blood Rush reads the counter and nothing else, so a build
/// carrying it pays exactly the same as one that does not.
#[test]
fn a_zero_combo_clock_stops_the_counter_rather_than_shortening_it() {
    let riven = |malus: &str| crate::build::rivens::RivenSpec {
        class: "melee".to_string(),
        bonuses: vec![crate::build::rivens::RolledStat { id: "melee_damage".into(), roll: 1.0 }],
        malus: Some(crate::build::rivens::RolledStat { id: malus.to_string(), roll: 1.0 }),
        rank: 8,
        polarity: crate::rules::capacity::Polarity::Madurai,
    };
    let fight = |malus: &str, mods: &[&str]| {
        let base = crate::model::WeaponBase::from_data("magistar", false, &[]);
        let pool = crate::data::mods::pool_for_weapon("magistar");
        let rv = riven(malus).to_mod_def("riven:test", 1.35);
        let mut refs: Vec<&crate::model::ModDef> =
            mods.iter().filter_map(|id| pool.iter().find(|m| m.id == *id)).collect();
        refs.push(&rv);
        let panel =
            crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        let arena = crate::arena::Arena::training(30.0);
        let p = FightParams::from_panel(&panel, &arena, &crate::data::arcanes::ArcaneFx::none());
        (panel.combo_frozen, panel.combo_duration_seconds, monte_carlo(&p, 24, 909).mean_damage)
    };

    // -8.2 s on a 5 s weapon: the seconds floor at 0.1 and the COUNTER is
    // the thing that stops.
    let (frozen, clock, with_rush) = fight("combo_duration", &["blood_rush"]);
    assert!(frozen, "a -8.2 s malus on a 5 s clock must freeze the counter");
    assert!((clock - 0.1).abs() < 1e-9, "the seconds still floor at 0.1, got {clock}");
    let (_, _, without) = fight("combo_duration", &[]);
    assert!(
        (with_rush - without).abs() < 1e-9,
        "Blood Rush reads a counter that never rises: {with_rush} vs {without}"
    );

    // …AND THE CHECK BITES ONLY ON THIS MALUS. The same card with any other
    // negative leaves the clock alone, and then Blood Rush is worth real
    // damage — so the assertion above is about the freeze and not about the
    // fixture being unable to tell two builds apart.
    let (still, clock, rushed) = fight("attack_speed", &["blood_rush"]);
    assert!(!still && (clock - 5.0).abs() < 1e-9, "an unrelated malus leaves the clock at 5 s");
    let (_, _, plain) = fight("attack_speed", &[]);
    assert!(rushed > plain * 1.05, "Blood Rush pays on a live counter: {rushed} vs {plain}");
}

/// AN EXTRA COMBO POINT PER HIT IS WORTH WHAT READS THE COUNTER.
///
/// Quickening's `+20% Combo Count Chance` is one point on a fifth of the
/// hits — against a neutral combo whose swings are already worth 3.5 points
/// each, so it is a small push on a big number, and the fixture pairs it
/// with Blood Rush because the counter is worth nothing to a build that
/// reads it with nothing.
#[test]
fn an_extra_combo_point_pays_whatever_reads_the_counter() {
    let with_reader = |mods: &[&str]| magistar("magistar", mods, 30.0, None).mean_damage;
    let plain = with_reader(&["blood_rush"]);
    let quicker = with_reader(&["blood_rush", "true_punishment"]);
    assert!(
        quicker > plain,
        "+100% combo count chance bought a Blood Rush build nothing: {plain:.0} -> {quicker:.0}",
    );
}



/// **A CRIT CARD THAT READS x2 ON A HEAVY IS DOUBLED THERE AND NOWHERE
/// ELSE**, and the card carries the rule rather than the bucket.
///
/// `+120% Critical Chance (x2 for Heavy Attacks)` is True Steel's own text,
/// and Sacrificial Steel and Galvanized Steel say the same. BLOOD RUSH IS
/// IN THE SAME BRACKET AND SAYS NOTHING OF THE KIND — which is why doubling
/// the bracket would have been wrong, and why this is asserted as a PAIR.
#[test]
fn a_crit_card_that_says_x2_on_a_heavy_is_doubled_only_there() {
    let cc = |form: &str, mods: &[&str]| {
        let base = crate::model::WeaponBase::from_data(form, false, &[]);
        let pool = crate::data::mods::pool_for_weapon(form);
        let refs: Vec<&crate::model::ModDef> =
            mods.iter().filter_map(|id| pool.iter().find(|m| m.id == *id)).collect();
        crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent).crit_chance
    };
    // 20% base. `20 x (1 + 1.20)` is 44%, and doubled it is `20 x (1 + 2.40)`
    // = 68% — the card's own arithmetic, on the two forms that spend combo.
    assert!((cc("magistar", &["true_steel"]) - 0.44).abs() < 1e-9);
    assert!((cc("magistar_slide", &["true_steel"]) - 0.44).abs() < 1e-9);
    assert!((cc("magistar_heavy", &["true_steel"]) - 0.68).abs() < 1e-9);
    assert!((cc("magistar_heavy_slam", &["true_steel"]) - 0.68).abs() < 1e-9);
    // …AND BLOOD RUSH IS NOT DOUBLED. It reads the combo counter live, so
    // the panel shows the weapon's own 20% either way — which is exactly
    // the assertion: whatever the form, this card changed nothing here.
    assert!((cc("magistar", &["blood_rush"]) - 0.20).abs() < 1e-9);
    assert!((cc("magistar_heavy", &["blood_rush"]) - 0.20).abs() < 1e-9);
}

/// MAIMING STRIKE NAMES THE SLIDE, and nothing else is a slide.
#[test]
fn maiming_strike_pays_the_slide_and_no_other_swing() {
    let cc = |form: &str| {
        let base = crate::model::WeaponBase::from_data(form, false, &[]);
        let pool = crate::data::mods::pool_for_weapon(form);
        let refs: Vec<&crate::model::ModDef> =
            pool.iter().filter(|m| m.id == "maiming_strike").collect();
        crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent).crit_chance
    };
    // `20 x (1 + 1.50)` = 50% on the slide, and the weapon's own 20%
    // everywhere else.
    assert!((cc("magistar_slide") - 0.50).abs() < 1e-9, "{}", cc("magistar_slide"));
    for f in ["magistar", "magistar_forward", "magistar_block", "magistar_heavy",
              "magistar_heavy_slam"] {
        assert!((cc(f) - 0.20).abs() < 1e-9, "{f} took a slide-only card: {}", cc(f));
    }
}

/// **CONDITION OVERLOAD IS UNCONDITIONAL, AND IT SKIPS A SLAM.**
///
/// Two facts, one card, and this engine had the first one wrong for a day:
/// melee's Condition Overload was routed through the Galvanized family's
/// path, which EARNS the same payload on a kill and therefore opens at zero
/// stacks — so the most important mod in the melee pool paid exactly
/// nothing, in all seven modes. `starts_full` is the fix and this is what
/// pins it.
///
/// THE SECOND FACT WAS ALREADY RIGHT and is asserted so it stays that way:
/// *"This damage does not apply to Slams, Heavy Slams, or Radial Attack
/// explosions"* — which falls out of `takes_condition_overload` defaulting
/// to false on an explosion, rather than being arranged.
///
/// THE THREE MODES ARE THREE DIFFERENT ANSWERS and that is the point: a
/// light combo lands four swings a cycle into a target carrying statuses,
/// a heavy lands fewer, and a slam is a radial that takes none of it.
#[test]
fn condition_overload_pays_a_swing_and_never_a_slam() {
    let gain = |form: &str| {
        let with = magistar(
            form, &["primed_fever_strike", "volcanic_edge", "condition_overload"], 60.0, None,
        ).mean_damage;
        let without =
            magistar(form, &["primed_fever_strike", "volcanic_edge"], 60.0, None).mean_damage;
        with / without
    };
    let light = gain("magistar");
    let heavy = gain("magistar_heavy");
    let slam = gain("magistar_heavy_slam");
    assert!(light > 1.5, "Condition Overload bought a combo build x{light:.4}");
    assert!(heavy > 1.1, "Condition Overload bought a heavy build x{heavy:.4}");
    assert!(
        (slam - 1.0).abs() < 1e-9,
        "Condition Overload paid a SLAM x{slam:.6} — the card says it does not",
    );
}


/// **TENNOKAI IS THE ONE MELEE MECHANIC THAT CHANGES WHAT THE LOOP DOES**,
/// rather than what a number is: when its window is open, the next swing of
/// a light combo becomes a HEAVY ATTACK — the class's multiplier in place
/// of the stance's, times a combo multiplier it reads AND DOES NOT SPEND.
///
/// That last clause is the whole mechanic. A combo build climbs the counter
/// to 12x with its swings and then fires FREE 12x heavy attacks between
/// them, which is why a 15% chance is worth nearly three times the build.
///
/// IT NEEDS A CARD. Three of the seven "enable Tennokai" and the mechanic
/// does not exist without one — the game's own answer, not a modelling
/// shortcut — so the negative control is a build carrying none of them.
#[test]
fn tennokai_turns_a_swing_into_a_free_heavy_attack() {
    let dps = |mods: &[&str]| magistar("magistar", mods, 60.0, None).mean_damage;
    let off = dps(&[]);
    let on = dps(&["mentors_legacy"]);
    assert!(
        on > off * 1.8,
        "a 15% chance of a free heavy attack bought {off:.0} -> {on:.0}",
    );
    // …AND THE BIGGEST SINGLE NUMBER GOES UP BY MORE THAN THE TOTAL, which
    // is what says the swing CHANGED rather than the build getting a
    // uniform bonus: the class multiplier is 6x where the biggest stance
    // swing is 5x, and the combo multiplier rides on top of it.
    let hit_off = magistar("magistar", &[], 60.0, None).mean_max_hit;
    let hit_on = magistar("magistar", &["mentors_legacy"], 60.0, None).mean_max_hit;
    assert!(
        hit_on > hit_off * 3.0,
        "the biggest hit barely moved: {hit_off:.0} -> {hit_on:.0}",
    );
}

/// A STANCE SLAM LANDS ITS OWN MULTIPLE AND SEISMIC WAVE PAYS IT — the
/// slam-only row a light combo ends on, and Hysteria's heavy opener.
#[test]
fn a_stance_slam_lands_and_seismic_wave_pays_it() {
    for form in ["magistar_block_forward", "valkyr_talons_heavy"] {
        let bare = melee_fight(form, &[], &[], None, 60.0, None).mean_damage;
        let waved = melee_fight(form, &[], &["seismic_wave"], None, 60.0, None).mean_damage;
        assert!(waved > bare * 1.01, "{form}: Seismic Wave bought {bare:.0} -> {waved:.0}");
    }
}

/// **HYSTERIA'S COMBO POINTS ARE THE MEASURED ONES** (MEASUREMENTS M95), per
/// hit on one target, and a row with none still reads the multiplier.
#[test]
fn hysteria_earns_its_measured_combo_points() {
    let round = |id: &str| -> f64 {
        crate::data::weapons::spec(id)
            .unwrap()
            .attack
            .combo_script
            .iter()
            .map(|h| h.combo_points * f64::from(h.hits))
            .sum()
    };
    // The wiki's notation: `Nx v` is N hits of v.
    assert_eq!(round("valkyr_talons"), 18.0, "1 / 1 / 2x 2 / 2x 2 / 2 / 2x 3");
    assert_eq!(round("valkyr_talons_forward"), 6.0, "1 / 1 / 2 / 2");
    assert_eq!(round("valkyr_talons_block"), 22.0, "2 / 2x 3 / 3x 3 / 1 + 3 + 1");
    assert_eq!(round("valkyr_talons_block_forward"), 30.0, "2 / 2x 2 / 3x 2 / 2x 2 / 3x 3 / 1 + 3 + 1");
    assert_eq!(round("valkyr_talons_slide"), 6.0, "6x 1, not the 18 its 300% would give");
}

/// **A ROW FILLED FROM THE RULE FOLLOWS IT, AND EVERY ROW HAS A SOURCE.**
///
/// Combo points are data the game sets per attack, so every melee file
/// states them — measured (MEASUREMENTS M95) or filled from the wiki's rule
/// (`see notes: combo_points_from_multiplier`). A filled row is checked
/// against that rule so a typo cannot pass as a measurement; a row of a form
/// that SPENDS the counter states 0, since a heavy attack earns nothing.
#[test]
fn every_combo_row_states_its_points_and_a_filled_one_follows_the_rule() {
    const MARK: &str = "see notes: combo_points_from_multiplier";
    let rows = |v: &serde_norway::Value| v.as_sequence().cloned().unwrap_or_default();
    let mut checked = 0;
    let mut wrong: Vec<String> = Vec::new();
    // FROM DISK: the source is a COMMENT, and the embedded data is stripped of
    // its comments on the way in (`engine/build.rs`).
    let data = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../data");
    let dirs = std::iter::once(data.join("weapons/melee"))
        .chain(std::fs::read_dir(data.join("mods")).unwrap().filter_map(|e| e.ok()).map(|e| e.path()));
    let mut paths: Vec<std::path::PathBuf> = Vec::new();
    for dir in dirs {
        if let Ok(rd) = std::fs::read_dir(&dir) {
            paths.extend(
                rd.filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "yaml")),
            );
        }
    }
    for p in paths {
        let owned = std::fs::read_to_string(&p).unwrap();
        let text = owned.as_str();
        let path = p.display().to_string();
        let doc: serde_norway::Value = serde_norway::from_str(text).unwrap();
        // (spends the counter, rows) for every block of rows in the file.
        let blocks: Vec<(bool, Vec<serde_norway::Value>)> = match doc.get("combos") {
            Some(c) => c
                .as_mapping()
                .into_iter()
                .flatten()
                .map(|(k, v)| (k.as_str() == Some("heavy"), rows(v)))
                .collect(),
            None => match doc.get("attack").and_then(|a| a.get("combo_script")) {
                Some(s) => {
                    let spends = doc["attack"].get("spends_combo").and_then(|b| b.as_bool()) == Some(true);
                    vec![(spends, rows(s))]
                }
                None => continue,
            },
        };
        if blocks.iter().all(|(_, r)| r.is_empty()) {
            continue;
        }
        assert!(text.contains(MARK) || text.contains("M95"), "{path}: combo points with no source");
        let filled = text.contains(MARK);
        for (spends, block) in blocks {
            for row in block {
                let mult = row["multiplier"].as_f64().unwrap();
                let stated = row["combo_points"].as_f64().unwrap();
                // BASE POINTS: none on a spending row, all of them on an
                // ordinary stance's, one a hit on an Exalted one (M97).
                let base = row["combo_points_base"].as_f64().unwrap();
                let want_base = if spends { 0.0 } else if filled { stated } else { 1.0 };
                if base != want_base {
                    wrong.push(format!("{path}: {mult} states base {base}, want {want_base}"));
                }
                if !filled {
                    continue;
                }
                let rule = if spends { 0.0 } else { combo_points_for(mult, 1.0) };
                checked += 1;
                if stated != rule {
                    wrong.push(format!("{path}: {mult} states {stated}, the rule fills {rule}"));
                }
            }
        }
    }
    assert!(checked > 100, "the sweep found only {checked} filled rows");
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Every total `hits` can come to — `(points, base)` a hit — with the
/// probability of each, by walking every sequence of roll outcomes.
fn combo_outcomes(hits: &[(f64, f64)], chance: f64, gain: f64) -> Vec<(f64, f64)> {
    let mut out: std::collections::BTreeMap<u64, (f64, f64)> = Default::default();
    let rolls_needed = hits.iter().map(|(_, b)| 2 * (*b as u32)).sum::<u32>();
    for pattern in 0u64..(1u64 << rolls_needed) {
        let mut i = 0;
        let mut prob = 1.0;
        let mut total = 0.0;
        for (p, b) in hits {
            total += swing_combo_gain(*p, *b, chance, gain, &mut |q| {
                let win = pattern >> i & 1 == 1;
                i += 1;
                prob *= if win { q } else { 1.0 - q };
                win
            });
        }
        // A PATTERN IS COUNTED ONCE: the bits past the rolls it used are
        // other patterns' business.
        if pattern >> i == 0 {
            let e = out.entry(total.to_bits()).or_insert((total, 0.0));
            e.1 += prob;
        }
    }
    out.into_values().collect()
}

/// **WHAT A HIT EARNS, AGAINST EVERY MEASUREMENT OF IT** (MEASUREMENTS M96,
/// M97): the totals each reading can reach, and the ones it cannot.
#[test]
fn a_hit_earns_its_base_points_through_the_gain_gate_and_the_extra_chance() {
    let values = |hits: &[(f64, f64)], chance: f64, gain: f64| -> Vec<f64> {
        combo_outcomes(hits, chance, gain).into_iter().filter(|(_, pr)| *pr > 0.0).map(|(v, _)| v).collect()
    };
    let mean = |hits: &[(f64, f64)], chance: f64, gain: f64| -> f64 {
        combo_outcomes(hits, chance, gain).into_iter().map(|(v, pr)| v * pr).sum()
    };
    // DAKRA PRIME, Vengeful Revenant's neutral opener: 3 points, all base.
    let dakra = [(3.0, 3.0)];
    assert_eq!(values(&dakra, 0.0, 0.0), [3.0]);
    assert_eq!(values(&dakra, 0.2, 0.0), [3.0, 4.0, 5.0, 6.0]);
    assert_eq!(values(&dakra, 1.0, 0.0), [6.0], "exactly 100% doubles and rolls nothing");
    assert_eq!(values(&dakra, 1.794, 0.0), [6.0, 7.0, 8.0, 9.0], "measured 9 9 9 8 7 8");
    assert_eq!(values(&dakra, 0.0, -0.573), [0.0, 1.0, 2.0, 3.0]);
    // …THE GATE BESIDE +120%: each base point is lost whole or kept with its
    // doubled share, so the total is never 1 — measured 2.82 over 17, no 1.
    let gated = values(&dakra, 1.2, -0.573);
    assert!(!gated.contains(&1.0), "{gated:?}");
    assert!((mean(&dakra, 1.2, -0.573) - 2.818).abs() < 0.01);
    // DAKRA PRIME'S AERIAL OPENER, 200%: 2 points, both base.
    assert_eq!(values(&[(2.0, 2.0)], 0.594, 0.0), [2.0, 3.0, 4.0], "measured 2 to 4");
    assert_eq!(values(&[(2.0, 2.0)], 0.0, -0.573), [0.0, 1.0, 2.0], "measured 0 to 2");
    // HYSTERIA: one base point a hit, the rest riding along. Its 3-point
    // aerial finisher at 120% is 6 or 7 and never 8 or 9 (measured 30 times).
    assert_eq!(values(&[(3.0, 1.0)], 1.2, 0.0), [6.0, 7.0]);
    // …and the whole aerial round, 2 / 2x 2 / 3: 18 to 22, exactly 18 at 100%.
    let aerial = [(2.0, 1.0), (2.0, 1.0), (2.0, 1.0), (3.0, 1.0)];
    assert_eq!(values(&aerial, 1.2, 0.0), [18.0, 19.0, 20.0, 21.0, 22.0]);
    assert_eq!(values(&aerial, 1.0, 0.0), [18.0]);
}

/// A HIT THAT CAME TO 0 POINTS DOES NOT HOLD THE COUNTER (MEASUREMENTS M97);
/// a heavy attack, which earns nothing by kind, keeps the refresh it had.
#[test]
fn a_hit_that_earned_nothing_does_not_refresh_the_combo_timer() {
    assert!(!refreshes_combo_timer(1.0, true, 0.0), "every base point lost");
    assert!(refreshes_combo_timer(1.0, true, 1.0));
    assert!(refreshes_combo_timer(1.0, false, 0.0), "a heavy attack");
    assert!(!refreshes_combo_timer(0.0, true, 3.0), "nothing landed");
}

/// **SPRING-LOADED BLADE'S STACKS WIDEN THE REACH MID-FIGHT.** Each status
/// buys +1 m for 24 s, two stacks on independent timers, read at the swing.
/// A ring at 3.5 m around the aimed body is mostly out of a Praedos's 2.5 m
/// and inside 4.5 m, so the stacks bring it in; a ring at 20 m stays out
/// wherever the wielder stands, and the card must then move nothing at all.
#[test]
fn spring_loaded_blade_stacks_reach_and_reaches_what_it_brings_into_range() {
    let dps = |mods: &[&str], gap: f64| magistar("praedos_slide", mods, 30.0, Some(gap)).mean_damage;
    let near_off = dps(&[], 3.5);
    let near_on = dps(&["spring_loaded_blade"], 3.5);
    assert!(near_on > near_off * 2.0, "two stacks reach the 3.5 m ring: {near_off:.0} -> {near_on:.0}");
    assert_eq!(dps(&[], 20.0), dps(&["spring_loaded_blade"], 20.0), "4.5 m reaches nothing at 20 m");

    let card = crate::data::mods::pool_for_weapon("praedos")
        .into_iter()
        .find(|m| m.id == "spring_loaded_blade")
        .expect("in the melee pool");
    let Some(crate::model::ModEffect::GrantsStackingBuff(b)) = card.effects.first().cloned() else {
        panic!("a stacking buff");
    };
    assert_eq!(b.grant, crate::model::BuffGrant::MeleeRange);
    assert_eq!(b.decay, crate::model::BuffDecay::PerStackExpiry, "independent timers");
    assert_eq!((b.per_stack, b.max_stacks, b.duration), (1.0, 2, 24.0));
}

/// **A SLIDE ATTACK OPENS THE WINDOW AND TAKES IT.** A slide lands direct
/// melee hits like any light swing, so it rolls for the flash, and a slide
/// loop that gets one fires the class's heavy attack in place of its next
/// slide — the play is to use the window the moment it opens.
#[test]
fn a_slide_attack_opens_tennokai_and_takes_it() {
    for form in ["praedos_slide", "valkyr_talons_slide"] {
        let dps = |mods: &[&str]| magistar(form, mods, 60.0, None).mean_damage;
        let off = dps(&[]);
        let on = dps(&["disciplines_merit"]);
        assert!(on > off * 1.3, "{form}: a slide loop with Tennokai must fire its heavies: {off:.0} -> {on:.0}");
    }
}

/// **A TENNOKAI HEAVY BREAKS THE STANCE CHAIN**, so the next light swing
/// starts the combo over.
///
/// THE WIKI SAYS NOTHING about a stance chain's position — asked directly,
/// and the page is silent on what advances it and what resets it — so this
/// is the owner's answer and is recorded as one rather than derived.
///
/// IT DECIDES WHICH SWINGS EVER HAPPEN, which is why it could not be left
/// to whichever behaviour fell out. Raging Whirlwind is
/// `400 / 200 / 300 / 500`, and a chain that restarts on every window fires
/// the opener again and again. THE SHARP CASE IS DISCIPLINE'S MERIT: it
/// opens the window every FOUR hits, which is exactly the length of that
/// combo, so the 500% finisher is never reached at all.
///
/// Asserted on the DAMAGE of a Discipline's Merit build against a plain
/// Tennokai one, because the chain position is not a number this engine
/// reports — what it does is decide which multipliers get fired, and a
/// cadence tuned to the combo's own length is where the two readings
/// diverge most.
#[test]
fn a_tennokai_heavy_restarts_the_stance_combo() {
    // A SCRIPT WHOSE SWINGS DIFFER SHARPLY, so restarting is visible: the
    // Magistar's neutral combo opens at 400% and finishes at 500%.
    let script = |mods: &[&str]| {
        let base = crate::model::WeaponBase::from_data("magistar", false, &[]);
        let pool = crate::data::mods::pool_for_weapon("magistar");
        let refs: Vec<&crate::model::ModDef> =
            mods.iter().filter_map(|id| pool.iter().find(|m| m.id == *id)).collect();
        crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent)
            .combo_script.iter().map(|h| h.multiplier).collect::<Vec<_>>()
    };
    assert_eq!(script(&[]).len(), 5, "the fixture's combo is four swings and a slam");

    // EVERY FOUR HITS is the combo's own length, so a chain that restarts
    // never reaches the finisher — and one that merely skips a swing does.
    // The two readings therefore differ, and by more than noise.
    let merit = magistar("magistar", &["mentors_legacy", "disciplines_merit"], 60.0, None)
        .mean_damage;
    let roll = magistar("magistar", &["mentors_legacy"], 60.0, None).mean_damage;
    assert!(
        merit > roll,
        "a guaranteed window every four hits is worth less than a 15% roll:              {roll:.0} -> {merit:.0}",
    );
    // …AND THE OPENER IS WHAT GETS FIRED. A restarting chain spends its
    // swings on the first entries of the script, so a build whose window
    // fires often lands FEWER swings of the whole combo — measurable as the
    // shot count, which the window's own wind-up also moves, so this is a
    // floor rather than an identity.
    let shots_on = magistar("magistar", &["mentors_legacy", "disciplines_merit"], 60.0, None)
        .mean_shots;
    let shots_off = magistar("magistar", &[], 60.0, None).mean_shots;
    assert!(
        shots_on > 0.0 && shots_off > 0.0,
        "{shots_off} -> {shots_on}",
    );
}

/// **EVERY TENNOKAI CARD ENABLES IT**, and that is why they are a family
/// rather than a base card with six accessories.
///
/// All seven open with the same three words — *"Enables Tennokai."* — and
/// only then say what else they do. This was asserted the other way round
/// first, on the assumption that Mentor's Legacy was a prerequisite; the
/// export's own card text says otherwise, on all six of the others.
///
/// SO THE NEGATIVE CONTROL IS A BUILD WITH NONE OF THEM, which is where the
/// mechanic genuinely does not exist — the game's own answer, not a
/// modelling shortcut.
#[test]
fn every_tennokai_card_enables_it_and_a_build_with_none_has_no_window() {
    let dps = |mods: &[&str]| magistar("magistar", mods, 60.0, None).mean_damage;
    let none = dps(&[]);
    for card in [
        "mentors_legacy", "disciplines_merit", "dreamers_wrath", "masters_edge",
        "opportunitys_reach", "conditions_perfection",
    ] {
        assert!(
            dps(&[card]) > none * 1.5,
            "{card} says `Enables Tennokai` and opened no window: {none:.0} -> {:.0}",
            dps(&[card]),
        );
    }
    // TRUTH'S FLAME IS THE SEVENTH AND IT IS NOT A BONUS. It opens the
    // window like the rest and then charges for it: the damage half pays
    // only in a window a KILL chained, and a Tennokai attack that fails to
    // kill EMPTIES THE COMBO COUNTER. On this fixture — one target it does
    // not always finish — that is worth less than the six above and more
    // than nothing, which is the shape of a gamble rather than a card.
    let flame = dps(&["truths_flame"]);
    assert!(
        flame > none * 1.1,
        "truths_flame says `Enables Tennokai` and opened no window: {none:.0} -> {flame:.0}",
    );
    assert!(
        flame < none * 1.5,
        "truths_flame is priced as an unconditional bonus again: {none:.0} -> {flame:.0}",
    );
    // …AND A CADENCE BEATS THE ROLL. `every 4 melee hits` is 25% against
    // the base 15%, so the two together are worth more than either alone.
    // THE MARGIN IS THINNER THAN THE TWO CHANCES SUGGEST, because a heavy
    // attack earns no combo points: a build that converts one swing in four
    // climbs the counter Blood Rush reads more slowly.
    let roll = dps(&["mentors_legacy"]);
    let both = dps(&["mentors_legacy", "disciplines_merit"]);
    assert!(
        both > roll * 1.1,
        "every-4-hits bought nothing over the 15% roll: {roll:.0} -> {both:.0}",
    );
}

/// **A TENNOKAI SWING LANDS ONCE**, however many the swing it replaced
/// landed — the window does not buff a light swing, it substitutes a heavy
/// attack for it.
///
/// ASSERTED ON THE RULE AND NOT ON A FIGHT, which is the honest place for
/// it: the window also costs a wind-up the light swing did not pay, and
/// that alone moves every total the other way — so a fight-level assertion
/// passes with the rule deleted, on the strength of the slower cadence.
/// Rogue Edict is what makes the rule bite at all: its rows land 5, 4 and 2
/// times, where every row of Raging Whirlwind lands once.
#[test]
fn a_tennokai_swing_lands_once_however_many_the_swing_it_replaced_landed() {
    assert_eq!(swing_instances(5, true), 1, "a converted spin is one heavy attack");
    assert_eq!(swing_instances(5, false), 5, "…and an ordinary one is five swings");
    assert_eq!(swing_instances(1, true), 1);
    // A ROW THAT NEVER SAYS SO STILL LANDS: `hits` defaults to 1 and a 0
    // would be a row that does nothing at all.
    assert_eq!(swing_instances(0, false), 1);
}

/// **A TENNOKAI ATTACK CHARGES AT ITS OWN SPEED**, and no card touches it.
///
/// *"The Wind-Up Speed of Tennokai attacks is not affected by Wind-Up Speed
/// bonuses from other sources"* — so the two clocks are resolved apart, and
/// a build carrying both wind-up cards moves ONE of them. On this weapon it
/// moves the ordinary heavy the right way past the Tennokai one, which is
/// the card's own clause and not an artefact: the window is a speed-up of
/// the CLASS's charge, and a heavy build has already bought a bigger one.
#[test]
fn a_tennokai_attacks_wind_up_ignores_every_card_that_buys_wind_up() {
    let panel = |mods: &[&str]| {
        let base = crate::model::WeaponBase::from_data("praedos_heavy", false, &[]);
        let pool = crate::data::mods::pool_for_weapon("praedos_heavy");
        let refs: Vec<&crate::model::ModDef> =
            mods.iter().filter_map(|id| pool.iter().find(|m| m.id == *id)).collect();
        assert_eq!(refs.len(), mods.len(), "every named mod is in this weapon's pool");
        crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent)
    };
    let cards = ["killing_blow", "amalgam_organ_shatter", "mentors_legacy"];
    let bare = panel(&["mentors_legacy"]);
    let rushed = panel(&cards);
    // THE WINDOW'S SWING HAS NO CHARGE AT ALL — measured, see
    // `notes: tonfa_heavy_timing`. Zero whatever the build bought, which is
    // both halves of *"not affected by Wind-Up Speed bonuses from other
    // sources"* and the thing the mechanic is for.
    for p in [&bare, &rushed] {
        assert_eq!(
            p.tennokai.windup_seconds, 0.0,
            "the window's swing charges: {}",
            p.tennokai.windup_seconds,
        );
    }
    // …WHILE THE ORDINARY ONE DID MOVE, which is what makes the first
    // assertion a claim rather than two constants agreeing.
    let (b, r) = (
        bare.heavy.expect("a tonfa has a heavy").windup_seconds,
        rushed.heavy.expect("a tonfa has a heavy").windup_seconds,
    );
    assert!(r < b * 0.8, "two wind-up cards bought nothing at all: {b} -> {r}");
}

/// MASTER'S EDGE PAYS THE WINDOW'S SWING AND NO OTHER.
///
/// `+60% Tennokai damage`, on a build where the window is most of the
/// output — so the whole fight moves by less than 60% and by a lot more
/// than nothing, and both bounds are the assertion.
#[test]
fn masters_edge_pays_only_what_the_window_bought() {
    let dps = |mods: &[&str]| magistar("magistar", mods, 60.0, None).mean_damage;
    let base = dps(&["mentors_legacy"]);
    let edged = dps(&["mentors_legacy", "masters_edge"]);
    let gain = edged / base;
    assert!(
        (1.05..1.60).contains(&gain),
        "+60% on the Tennokai swing alone came to x{gain:.3} of the whole fight",
    );
}

/// THE COMBO LADDER, as the wiki publishes it: 2x at 20 hits, one more
/// every 20, 12x at 220 and no further.
///
/// A STEP FUNCTION, and 1x is a real state rather than "no combo" — a heavy
/// attack at 1x deals its class multiplier and nothing more.
#[test]
fn the_combo_ladder_is_the_wikis_own_table() {
    for (points, want) in [
        (0.0, 1.0), (19.0, 1.0), (20.0, 2.0), (39.0, 2.0), (40.0, 3.0),
        (100.0, 6.0), (219.0, 11.0), (220.0, 12.0), (1000.0, 12.0),
    ] {
        assert_eq!(
            melee_combo_multiplier(points), want,
            "{points} points should be {want}x",
        );
    }
}

/// INITIAL COMBO IS A FLOOR THAT REFILLS, not a second pool.
///
/// *"Heavy attacks spend initial combo, which regenerates at a rate of 40
/// combo points per second"*. It is what makes a pure-heavy build work: the
/// Magistar's Incarnon Form carries +30, which is back inside 0.75 s
/// against a 1.07 s cycle, so every heavy lands at 2x rather than 1x.
#[test]
fn initial_combo_is_a_floor_that_refills_at_forty_a_second() {
    // Nothing earned, nothing granted: the floor is zero however long you
    // wait.
    assert_eq!(melee_combo_points(0.0, 0.0, 10.0), 0.0);
    // …and with a grant it fills at 40 a second and stops at the grant.
    assert_eq!(melee_combo_points(0.0, 30.0, 0.0), 0.0);
    assert_eq!(melee_combo_points(0.0, 30.0, 0.5), 20.0);
    assert_eq!(melee_combo_points(0.0, 30.0, 0.75), 30.0);
    assert_eq!(melee_combo_points(0.0, 30.0, 5.0), 30.0);
    // THE HIGHER OF THE TWO, which is what "floor" means: a light build
    // that has earned 200 does not fall back to 30.
    assert_eq!(melee_combo_points(200.0, 30.0, 5.0), 200.0);
    // …and 0.75 s of refill is exactly the 2x tier, which is the whole
    // arithmetic of the pure-heavy build stated as an assertion.
    assert_eq!(melee_combo_multiplier(melee_combo_points(0.0, 30.0, 0.75)), 2.0);
}

fn magistar(form: &str, mods: &[&str], duration: f64, spacing: Option<f64>) -> Summary {
    melee_fight(form, &[], mods, None, duration, spacing)
}

/// The same fixture with EVOLUTIONS, which the Praedos needs and no hammer
/// does: its five tiers ship with the weapon rather than with an adapter —
/// and with an ARCANE, which is where a crowd card lives.
fn melee_fight(
    form: &str,
    evos: &[&str],
    mods: &[&str],
    arcane: Option<&str>,
    duration: f64,
    spacing: Option<f64>,
) -> Summary {
    let base = crate::model::WeaponBase::from_data(form, false, evos);
    let pool = crate::data::mods::pool_for_weapon(form);
    let refs: Vec<&crate::model::ModDef> =
        mods.iter().filter_map(|id| pool.iter().find(|m| m.id == *id)).collect();
    let panel = crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
    let mut arena = crate::arena::Arena::training(duration);
    // A RING OF BODIES AT `gap` METRES, built here rather than in `Arena`:
    // the fixture is the one place that wants a crowd, and `training` is
    // deliberately one body so no golden value depends on a formation.
    if let Some(gap) = spacing {
        arena.others = (1..9)
            .map(|i| {
                let a = std::f64::consts::TAU * f64::from(i) / 8.0;
                crate::formation::FoeSpec {
                    id: format!("e{}", i + 1),
                    params: crate::target::TargetParams::training_dummy(),
                    body_parts: crate::target::BodyPart::humanoid(),
                    at: crate::rules::space::Vec2::new(gap * a.cos(), gap * a.sin()),
                }
            })
            .collect();
    }
    // THE SAME DECISION THE PAGE MAKES (`FightParams::for_panel`): a melee
    // Incarnon is the weapon resolved twice, so the fixture resolves it
    // twice too. Calling `from_panel` here would measure the Genesis on for
    // the whole engagement, which is a build nobody can produce.
    //
    // AT ITS MAX RANK, which is the only rank a board row is ever built
    // at and the one the card's own numbers are printed for.
    let fx = arcane.map_or_else(crate::data::arcanes::ArcaneFx::none, |id| {
        let card = crate::data::arcanes::pool_for_weapon(form, "melee")
            .into_iter()
            .find(|a| a.id == id)
            .unwrap_or_else(|| panic!("the melee pool seats {id}"));
        card.fx(
            card.max_rank,
            crate::model::StackPolicy::Emergent,
            &[],
            crate::data::tenno::default_tenno(),
        )
    });
    let p = FightParams::for_panel(&panel, &arena, &fx, || {
        let unarmed: Vec<&str> = evos
            .iter()
            .copied()
            .filter(|id| !crate::data::evolutions::states_incarnon_window(id))
            .collect();
        let b = crate::model::WeaponBase::from_data(form, false, &unarmed);
        crate::build::loadout::resolve(&b, &refs, crate::model::StackPolicy::Emergent)
    });
    monte_carlo(&p, 24, 909)
}

/// A COMBO WITH ITS SLAMS TAKEN OFF, over 20 s — for a test about who a SWING
/// reaches, since a stance slam is a sphere and reaches the room.
fn swings_only(form: &str, mods: &[&str], spacing: Option<f64>) -> Summary {
    let base = crate::model::WeaponBase::from_data(form, false, &[]);
    let pool = crate::data::mods::pool_for_weapon(form);
    let refs: Vec<&crate::model::ModDef> =
        mods.iter().filter_map(|id| pool.iter().find(|m| m.id == *id)).collect();
    let mut panel = crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
    // A SLAM TAKEN OFF LEAVES ITS TIME BEHIND: Winding Temper keeps its
    // whole 2.25 s on the slam row, and the three swings alone take none.
    let mut kept: Vec<crate::model::ComboHit> = Vec::new();
    for h in std::mem::take(&mut panel.combo_script) {
        match (h.slam_multiplier, kept.last_mut()) {
            (Some(_), Some(prev)) => prev.delay_seconds += h.delay_seconds,
            (Some(_), None) => {}
            (None, _) => kept.push(h),
        }
    }
    panel.combo_script = kept;
    let mut arena = crate::arena::Arena::training(20.0);
    if let Some(gap) = spacing {
        arena.others = (1..9)
            .map(|i| {
                let a = std::f64::consts::TAU * f64::from(i) / 8.0;
                crate::formation::FoeSpec {
                    id: format!("e{}", i + 1),
                    params: crate::target::TargetParams::training_dummy(),
                    body_parts: crate::target::BodyPart::humanoid(),
                    at: crate::rules::space::Vec2::new(gap * a.cos(), gap * a.sin()),
                }
            })
            .collect();
    }
    monte_carlo(&FightParams::from_panel(&panel, &arena, &crate::data::arcanes::ArcaneFx::none()), 24, 909)
}

/// **SHOCKWAVE SYNERGY IS PAID BY THE CROWD**, and it is the only card in
/// the game that earns combo points on a heavy mode.
///
/// *"For each enemy hit by Slam radius, gain 4 Combo Count"* (wiki,
/// Praedos). Every other heavy build in this roster earns nothing — a heavy
/// attack adds no points — so its counter can only hold what the
/// initial-combo floor regenerates between swings. This one climbs with the
/// bodies in the sphere.
///
/// ASSERTED ON THE GAIN'S DEPENDENCE ON THE CROWD rather than on a value,
/// because that is the whole of what the perk is: the same card on one body
/// pays four points a slam, and on nine pays thirty-six.
///
/// …AND IT IS THE ORDINARY SLAM THAT PAYS. A HEAVY slam earns nothing
/// which is the general rule rather than a carve-out:
/// a swing that spends the counter adds nothing to it. So the mode under
/// test is the combo that ENDS in a slam — `block_forward`, whose trailing
/// hit is `slam_multiplier: 1.0` — and the heavy slam is asserted flat
/// beside it, which is what makes the gate checkable at all.
#[test]
fn shockwave_synergy_is_paid_by_the_crowd() {
    // BLOOD RUSH IS WHAT MAKES THE POINTS VISIBLE on a light combo: the
    // counter does not multiply a normal swing, so a build with nothing
    // reading it pays exactly the same at 1x and at 12x.
    let at = |form: &str, evos: &[&str], gap: Option<f64>| {
        melee_fight(form, evos, &["blood_rush"], None, 60.0, gap).mean_damage
    };
    let perk = ["praedos_shockwave_synergy"];
    let solo_off = at("praedos_block_forward", &[], None);
    let solo_on = at("praedos_block_forward", &perk, None);
    let crowd_off = at("praedos_block_forward", &[], Some(3.0));
    let crowd_on = at("praedos_block_forward", &perk, Some(3.0));
    assert!(
        crowd_on > crowd_off,
        "the perk paid nothing in a crowd: {crowd_off:.0} -> {crowd_on:.0}"
    );
    assert!(
        crowd_on / crowd_off > solo_on / solo_off,
        "the gain did not scale with the crowd: solo {:.3}x, crowd {:.3}x",
        solo_on / solo_off,
        crowd_on / crowd_off
    );
    // THE HEAVY SLAM IS THE SAME PERK ON THE SAME CROWD AND PAYS NOTHING.
    assert_eq!(
        at("praedos_heavy_slam", &perk, Some(3.0)),
        at("praedos_heavy_slam", &[], Some(3.0)),
        "a heavy slam spends the counter, so it cannot earn into it"
    );
}

/// A HEAVY WAITS FOR THE RUNG IT CAN REACH, AND NOT A MOMENT PAST IT.
///
/// The counter pays in STEPS, so the most favourable cycle is a rung
/// boundary or the animation's own floor — never the full refill, which is
/// what a build with a deep initial combo would wait for if the wait were
/// priced linearly.
#[test]
fn a_heavy_waits_for_a_rung_and_never_for_the_full_refill() {
    // Nothing to wait for: the cycle is the recovery.
    assert_eq!(heavy_cycle_seconds(0.7, 0.0, 0.0), 0.7);
    // The recovery already carries the counter past 2x, so it waits none.
    assert_eq!(heavy_cycle_seconds(0.7, 0.0, 30.0), 0.7);
    // A tenth of a second short of 2x, and 2x is worth the tenth.
    assert_eq!(heavy_cycle_seconds(0.4, 0.0, 30.0), 0.5);
    // 110 initial combo is 6x after 2.75 s of refill, and taking it would
    // be 2.18 multiplier-seconds against the 4.00 this stops at.
    assert_eq!(heavy_cycle_seconds(0.4, 0.0, 110.0), 0.5);
    // …and a longer recovery moves which rung that is, not the rule.
    assert_eq!(heavy_cycle_seconds(1.2, 0.0, 110.0), 1.5);
}

/// THE FIGHT OPENS WITH THE INITIAL-COMBO FLOOR FULL.
///
/// *"Initial Combo grants a minimum value of combo points when IDLE"* — so
/// the FIRST heavy attack of the engagement already pays it, and the 40
/// points a second is what a heavy attack owes back rather than what the
/// player owes on the way in.
///
/// A HALF-SECOND FIGHT IS ONE SLAM, which is the only way to ask this
/// question: every later swing pays the floor whichever rule is in force.
#[test]
fn the_first_slam_of_a_fight_already_holds_its_initial_combo() {
    let one = |mods: &[&str]| magistar("magistar_heavy_slam", mods, 0.5, None).mean_damage;
    let gain = one(&["corrupt_charge"]) / one(&[]);
    assert!(
        (1.9..2.1).contains(&gain),
        "+30 initial combo should open the fight at 2x, not 1x: x{gain:.3}",
    );
}

/// A STANDING HEAVY WAITS TOO, and that is what makes the rule the SWING's
/// rather than the slam's.
///
/// THE SWING IS TAKEN OFF, leaving the 0.4 s charge as the whole cycle: the
/// real 0.96 s swing is already past the first rung, so nothing would wait.
/// 0.4 s is SHORT OF THE RUNG: 16 points is still 1x, so Corrupt Charge's
/// +30 initial combo buys nothing if the swing goes the moment it can.
/// Waiting 0.1 s more to 20 points buys 2x for a quarter of the cadence,
/// and the mode takes it.
#[test]
fn a_standing_heavy_waits_the_seventh_of_a_second_that_buys_a_tier() {
    for m in ["weeping_wounds", "melee_prowess", "galvanized_elementalist"] {
        let a = magistar("magistar", &[], 60.0, None).mean_procs;
        let b = magistar("magistar", &[m], 60.0, None).mean_procs;
        println!("SC {m}: {a:.2} -> {b:.2}");
    }
    let dps = |mods: &[&str]| {
        let form = "magistar_heavy";
        let base = crate::model::WeaponBase::from_data(form, false, &[]);
        let pool = crate::data::mods::pool_for_weapon(form);
        let refs: Vec<&crate::model::ModDef> =
            mods.iter().filter_map(|id| pool.iter().find(|m| m.id == *id)).collect();
        let mut panel = crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        for h in &mut panel.combo_script {
            h.delay_seconds = 0.0;
        }
        let arena = crate::arena::Arena::training(60.0);
        monte_carlo(&FightParams::from_panel(&panel, &arena, &ArcaneFx::none()), 24, 909).mean_damage
    };
    let gain = dps(&["corrupt_charge"]) / dps(&[]);
    assert!(
        (1.4..2.0).contains(&gain),
        "the wait should buy 2x at a quarter of the cadence: x{gain:.3}",
    );
}

/// WEEPING WOUNDS ROLLS, and the counter it reads is live.
///
/// `Status Chance = Weapon Status Chance x [1 + Mod Status Bonus + Weeping
/// Wounds Bonus x (Combo Multi - 1)]` (wiki, verbatim) — so the term is in
/// the bracket the ROLL reads, not merely in the one a derived-crit card
/// reads. ONE BRACKET, ONE SUM: a second sum over the same bracket is a
/// second answer, and the cards that land only in the unread one pay
/// nothing while the panel shows them paying.
///
/// A NEUTRAL COMBO IS THE FIXTURE because it is where the counter climbs:
/// a heavy mode empties it with the swing that read it.
#[test]
fn weeping_wounds_reaches_the_roll_and_not_only_the_panel() {
    let procs = |mods: &[&str]| magistar("magistar", mods, 60.0, None).mean_procs;
    let bare = procs(&[]);
    let weeping = procs(&["weeping_wounds"]) / bare;
    // …AND AN ORDINARY STATUS MOD IS THE CONTROL, so a fixture that cannot
    // see status at all cannot pass this by accident.
    let flat = procs(&["melee_prowess"]) / bare;
    assert!(weeping > 1.3, "a live counter should roll far more status: x{weeping:.3}");
    assert!(flat > 1.1, "the control has to move too: x{flat:.3}");
}

/// A FORM INHERITS THE COMBO COUNTER'S CLOCK, and six of seven modes had
/// none.
///
/// `combo_duration_seconds` is the WEAPON's — five seconds on almost every
/// melee — and a form that does not inherit it reads zero, floored at the
/// 0.1 s the wiki names. That is a counter that dies between every pair of
/// swings: Blood Rush and Weeping Wounds climb nothing, and Heavy Attack
/// Efficiency keeps points that are gone before the next one.
#[test]
fn every_melee_form_carries_the_weapons_combo_clock() {
    for form in [
        "magistar", "magistar_forward", "magistar_block", "magistar_block_forward",
        "magistar_heavy", "magistar_slide", "magistar_heavy_slam",
    ] {
        let base = crate::model::WeaponBase::from_data(form, false, &[]);
        let refs: Vec<&crate::model::ModDef> = Vec::new();
        let p = crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        assert_eq!(p.combo_duration_seconds, 5.0, "{form} lost the counter's clock");
    }
}

/// HEAVY ATTACK EFFICIENCY SPENDS LESS OF THE COUNTER, and the counter
/// regenerates from what is left.
///
/// *"40% heavy attack efficiency will change the amount spent to 60% combo
/// points … capped at 90%"*, and *"Heavy attacks spend initial combo, which
/// REGENERATES at a rate of 40 combo points per second"*. Two rules, one
/// number: 90% of a 140-point counter is 126, and 40 a second climbs the
/// remaining 14 back inside a fifth of a second.
///
/// A HEAVY MODE EARNS NO POINTS, so spending only the earned half spent
/// nothing and left the counter at zero — efficiency bought exactly nothing
/// in the one family of modes whose cards sell it.
#[test]
fn heavy_attack_efficiency_keeps_the_counter_and_it_regenerates_from_there() {
    // Nothing held: the floor fills from zero, which is every build without
    // an efficiency card.
    assert_eq!(melee_combo_points(0.0, 140.0, 0.5), 20.0);
    assert_eq!(melee_combo_points(0.0, 140.0, 3.5), 140.0);
    // …and 90% efficiency leaves 126 of 140, which is back at the floor in
    // 0.35 s rather than the 3.5 s a restart would take.
    assert_eq!(melee_combo_points(126.0, 140.0, 0.35), 140.0);
    assert_eq!(melee_combo_points(126.0, 140.0, 0.0), 126.0);
    // ABOVE THE FLOOR IT ONLY WAITS: points earned past the initial-combo
    // value are not regeneration's business.
    assert_eq!(melee_combo_points(200.0, 140.0, 5.0), 200.0);
}

/// A SLAM'S TOXIN BYPASSES A SHIELD AND ITS BLAST DOES NOT — on a mode
/// whose WHOLE attack is the explosion.
///
/// The heavy slam is the one melee mode with no direct hit at all, so it is
/// the only one that can show whether the shield split is derived per
/// STAGE rather than off a direct vector this mode does not have. Its own
/// vector is pure Blast, and a Toxin mod buys the share that goes STRAIGHT
/// TO HEALTH — which is why a build that clears a Corpus line needs one.
///
/// ASSERTED ON THE LEDGER, because effective damage cannot see it: a shield
/// applies no reduction, so bypassing it changes WHERE a hit lands and not
/// how big it is. What it buys is kill speed, and what proves it is a row
/// that says `health` while the shield is still standing.
///
/// TOXIN DOES NOT BYPASS OVERGUARD, which is why the ruler's own body
/// cannot ask this: a Thrax Centurion's pool is overguard, and an Eximus
/// Crewman's is too — hence the plain one here.
#[test]
fn a_slams_toxin_reaches_health_through_a_shield() {
    let pools = |mods: &[&str]| -> Vec<(crate::target::Pool, DamageType)> {
        let base = crate::model::WeaponBase::from_data("magistar_heavy_slam", false, &[]);
        let pool = crate::data::mods::pool_for_weapon("magistar_heavy_slam");
        let refs: Vec<&crate::model::ModDef> =
            mods.iter().filter_map(|id| pool.iter().find(|m| m.id == *id)).collect();
        let panel = crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        let spec = crate::data::enemies::EnemySpec::load(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../data/enemies/crewman.yaml"
        )))
        .expect("a body with shields");
        let mut arena = crate::arena::Arena::training(6.0);
        arena.target = spec
            .target_params(500, false, false, TargetMode::InstantRespawn)
            .expect("level 500, and NOT an Eximus: overguard is neutral");
        let p = FightParams::from_panel(&panel, &arena, &ArcaneFx::none());
        record(&p, 7, 0.0, 6.0, 40, 0)
            .events()
            .iter()
            .filter_map(|e| match &e.kind {
                crate::record::Kind::Damage(d) => Some((d.pool, d.dtype)),
                _ => None,
            })
            .collect()
    };
    let blast = pools(&[]);
    assert!(!blast.is_empty(), "the fixture has to land something");
    assert!(
        blast.iter().all(|(p, _)| *p == crate::target::Pool::Shield),
        "pure Blast has nothing that bypasses: {blast:?}",
    );
    let toxic = pools(&["primed_fever_strike"]);
    assert!(
        toxic.iter().any(|(p, t)| *p == crate::target::Pool::Health && *t == DamageType::Toxin),
        "the Toxin share should reach health past the shield: {toxic:?}",
    );
    assert!(
        toxic.iter().any(|(p, _)| *p == crate::target::Pool::Shield),
        "…and the rest of the same hit should still be spent on the shield",
    );
}

/// A LIFTED GATE IS SIMULATED, AND WHAT DECIDES IT IS THE CADENCE.
///
/// Enduring Affliction is *"+100% Status Chance on Lifted enemies"* and
/// `Lifted` is a status this engine tracks, not a Tenno state it assumes —
/// so the card pays exactly when a swing lands while a PREVIOUS swing's
/// Lift is still standing. The swing that lifts never amplifies itself.
///
/// FOUR CARDS MAKE IT PAY. Three wind-up cards take the 0.4 s charge to
/// 0.14 s and Fury the 0.8 s swing (at the Magistar's 0.833) to 0.74 s: 0.88 s
/// a cycle, inside `LIFTED_SECONDS`, so every swing after the first sees it.
/// The neutral combo forces Impact and Knockdown and never a Lift, so the
/// same card is worth nothing there — and a gate that only says yes is
/// indistinguishable from no gate at all, which is why both halves are
/// asserted.
#[test]
fn enduring_affliction_pays_where_a_lift_is_still_standing() {
    let procs = |form: &str, mods: &[&str]| magistar(form, mods, 60.0, None).mean_procs;
    let fast = ["killing_blow", "amalgam_organ_shatter", "melee_elementalist", "fury"];
    let with_card = [fast.as_slice(), &["enduring_affliction"]].concat();
    let heavy = procs("magistar_heavy", &with_card) / procs("magistar_heavy", &fast);
    let light = procs("magistar", &["enduring_affliction"]) / procs("magistar", &[]);
    assert!(
        heavy > 1.3,
        "a swing inside the Lift should roll far more status: x{heavy:.3}",
    );
    assert!(
        (light - 1.0).abs() < 1e-9,
        "a neutral combo lifts nothing, so the same card pays nothing: x{light:.3}",
    );
}

/// KILLING BLOW IS ADDITIVE WITH PRESSURE POINT, AND SEISMIC WAVE IS NOT.
///
/// Two cards that read the same on their faces — `+X% Melee Damage on
/// <kind of attack>` — and their own pages put them in different places:
/// *"Damage bonus is additive to mods such as Pressure Point"* (Killing
/// Blow) against *"Slam damage bonus is multiplicative to base damage
/// (e.g. Pressure Point)"* (Seismic Wave).
///
/// SO THE ARITHMETIC IS THE ASSERTION. Against Primed Pressure Point's
/// +165%, Killing Blow's +120% is `(1 + 1.65 + 1.20) / (1 + 1.65)` = x1.453
/// and Seismic Wave's +200% is a flat x3. A heavy SLAM is the fixture
/// because its cadence cannot move: it has no wind-up for Killing Blow's
/// other half to shorten.
#[test]
fn killing_blow_joins_the_bucket_and_seismic_wave_multiplies_it() {
    let dps = |mods: &[&str]| magistar("magistar_heavy_slam", mods, 60.0, None).mean_damage;
    let base = dps(&["primed_pressure_point"]);
    let killing = dps(&["primed_pressure_point", "killing_blow"]) / base;
    let seismic = dps(&["primed_pressure_point", "seismic_wave"]) / base;
    assert!(
        (1.43..1.48).contains(&killing),
        "additive with the bucket is x1.453, multiplicative would be x2.2: x{killing:.3}",
    );
    assert!(
        (2.9..3.1).contains(&seismic),
        "a slam multiplier is its own x3: x{seismic:.3}",
    );
}

/// GALVANIZED REFLEX EARNS THE FLOOR ITSELF, which on a heavy mode is the
/// half of the card that carries the build.
///
/// *"On Melee Kill: +20 Initial Combo for 20s. Stacks up to 4x"* — combo
/// POINTS, so four stacks are +80 and the counter a heavy mode returns to
/// after every swing is four tiers higher. It needs a target that DIES,
/// which the training dummy never does, so this one fights a Thrax at level
/// one: the same body the ruler uses, at a level a slam can clear.
#[test]
fn galvanized_reflex_earns_initial_combo_and_a_heavy_mode_keeps_it() {
    let killing = |mods: &[&str]| {
        let base = crate::model::WeaponBase::from_data("magistar_heavy_slam", false, &[]);
        let pool = crate::data::mods::pool_for_weapon("magistar_heavy_slam");
        let refs: Vec<&crate::model::ModDef> =
            mods.iter().filter_map(|id| pool.iter().find(|m| m.id == *id)).collect();
        let panel = crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        let mut arena = crate::arena::Arena::training(60.0);
        let spec = crate::data::enemies::EnemySpec::load(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../data/enemies/thrax_centurion.yaml"
        )))
        .expect("the ruler's own body");
        arena.target = spec
            .target_params(1, false, false, TargetMode::InstantRespawn)
            .expect("level 1");
        let p = FightParams::from_panel(&panel, &arena, &ArcaneFx::none());
        monte_carlo(&p, 30, 3)
    };
    let bare = killing(&[]);
    let earned = killing(&["galvanized_reflex"]);
    assert!(bare.mean_kills > 2.0, "the fixture has to kill: {}", bare.mean_kills);
    let gain = earned.mean_damage / bare.mean_damage;
    assert!(
        gain > 1.5,
        "kills should buy tiers the counter keeps: x{gain:.3}",
    );
}

/// …AND THE WHOLE OF THE MODE'S BUILD IS THAT COUNTER.
///
/// Nothing else the loop does moves: the recovery is the same either way,
/// and Corrupt Charge's +30 initial combo is back well inside it. So the
/// mode slams as often and hits twice as hard, which is what a floor that
/// refills faster than the cycle is worth.
/// MELEE EXPOSURE IS HELD FOR THE WHOLE ENGAGEMENT, and it is the melee
/// pool's one card that pays a slam.
///
/// *"On Ability Cast: Gain 60% Corrosive Damage on Melee strikes for 25s.
/// Stacks up to 240%"* — a Warframe cast is a thing this arena cannot do,
/// so the choice is the cap or nothing, and the cap is what a melee player
/// holds. It lands in the elemental-mod bracket, which is why it pays the
/// EXPLOSION: Condition Overload does not, and a slam build's whole
/// question is which cards reach the radial at all.
#[test]
fn melee_exposure_holds_its_stacks_and_pays_the_explosion() {
    let base = crate::model::WeaponBase::from_data("magistar_heavy_slam", false, &[]);
    let refs: Vec<&crate::model::ModDef> = Vec::new();
    let panel = crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
    let arena = crate::arena::Arena::training(60.0);
    let card = crate::data::arcanes::pool_for_weapon("magistar_heavy_slam", "melee")
        .into_iter()
        .find(|a| a.id == "melee_exposure")
        .expect("the melee pool seats it");
    let fx = card.fx(
        card.max_rank,
        crate::model::StackPolicy::Emergent,
        &[],
        crate::data::tenno::default_tenno(),
    );
    // FOUR STACKS OF 60%, and EMERGENT is the policy the board runs: a card
    // whose stacks are assumed must not be assumed only under assumed-max.
    assert_eq!(fx.added_elements, vec![(DamageType::Corrosive, 2.4)]);
    let bare = monte_carlo(&FightParams::from_panel(&panel, &arena, &ArcaneFx::none()), 30, 3);
    let with = monte_carlo(&FightParams::from_panel(&panel, &arena, &fx), 30, 3);
    let gain = with.dps / bare.dps;
    assert!(
        gain > 2.0,
        "+240% in the elemental bracket should more than double a bare slam: x{gain:.2}",
    );
}

#[test]
fn initial_combo_doubles_a_heavy_slam_loop_and_costs_it_no_time() {
    let bare = magistar("magistar_heavy_slam", &[], 60.0, None).mean_damage;
    let charged = magistar("magistar_heavy_slam", &["corrupt_charge"], 60.0, None).mean_damage;
    let gain = charged / bare;
    assert!(
        (1.8..2.2).contains(&gain),
        "+30 initial combo should be 1x -> 2x on every slam: x{gain:.3}",
    );
}

/// **THE SEVEN MODES ARE SEVEN BUILDS**, which is the claim the whole
/// melee model rests on — and the cheapest way for it
/// to be false is for them to share a number.
///
/// Every one of them fires, and no two agree: they differ in the swing
/// multipliers, in the cadence those swings imply, in what they force on
/// the target and in whether they spend the combo counter.
#[test]
fn every_way_to_swing_it_is_a_different_fight() {
    const FORMS: [&str; 7] = [
        "magistar", "magistar_forward", "magistar_block", "magistar_block_forward",
        "magistar_heavy", "magistar_slide", "magistar_heavy_slam",
    ];
    let mut seen: Vec<(&str, f64)> = Vec::new();
    for f in FORMS {
        let r = magistar(f, &[], 20.0, None);
        assert!(r.mean_damage > 0.0, "{f}: swings and deals nothing");
        for (other, d) in &seen {
            assert!(
                (r.mean_damage - d).abs() > 1e-6,
                "{f} and {other} are the same fight: {:.3}", r.mean_damage,
            );
        }
        seen.push((f, r.mean_damage));
    }
}

/// BLOOD RUSH READS THE COUNTER; A HEAVY SWING EMPTIES IT.
///
/// The two facts are one mechanic seen from both ends, and together they
/// are why `heavy` is a different BUILD from `neutral` rather than a
/// different animation. In a combo mode the counter climbs and the card
/// pays; in a heavy mode the swing that reads it is the swing that spends
/// it, so it is standing at the floor every time.
#[test]
fn blood_rush_pays_a_combo_build_and_not_a_heavy_one() {
    let gain = |form: &str| {
        let without = magistar(form, &[], 30.0, None).mean_damage;
        let with = magistar(form, &["blood_rush"], 30.0, None).mean_damage;
        with / without
    };
    let light = gain("magistar");
    let heavy = gain("magistar_heavy");
    assert!(light > 1.05, "Blood Rush bought a combo build nothing: x{light:.4}");
    assert!(
        heavy < light,
        "Blood Rush is worth as much to a heavy build (x{heavy:.4}) as to a combo one              (x{light:.4}) — the counter is not being spent",
    );
}

/// A SWING REACHES WHAT IS INSIDE ITS RANGE AND NOTHING OUTSIDE IT.
///
/// The positive control for Follow Through, and the fixture is chosen so
/// the boundary is unambiguous: a ring at 3.0 m centre to centre is a GAP
/// of exactly 2.5 m, which is exactly a hammer's reach, and a test standing
/// on a knife edge tells you nothing. 2.6 m is comfortably inside and 4.0 m
/// comfortably outside.
#[test]
fn a_swing_reaches_a_crowd_inside_its_range_and_no_further() {
    // Tidal Force, because the other three combos END on a slam.
    let alone = magistar("magistar_forward", &[], 20.0, None).mean_damage;
    let near = magistar("magistar_forward", &[], 20.0, Some(2.6)).mean_damage;
    let far = magistar("magistar_forward", &[], 20.0, Some(4.0)).mean_damage;
    assert!(
        near > alone * 1.2,
        "a crowd inside a 2.5 m reach took nothing: {alone:.0} -> {near:.0}",
    );
    assert!(
        (far - alone).abs() < alone * 0.02,
        "a crowd 3.5 m from a 2.5 m swing took something: {alone:.0} -> {far:.0}",
    );
}

/// FOLLOW THROUGH DECAYS GEOMETRICALLY, and a hammer's 0.4 is steep enough
/// to see: the second body takes 40%, the third 16%, the fourth 6.4%.
///
/// Asserted as a CEILING on what the crowd can add rather than as a ratio,
/// because every body settles its own armour, its own status count and its
/// own death — `spread_hit`'s whole point. Eight bodies at full damage
/// would be 9x the lone fight; at `0.4^n` the series sums to 1.67x, and the
/// gap between those two is what this is about.
#[test]
fn follow_through_decays_by_the_order_the_swing_reached_them() {
    let alone = magistar("magistar", &[], 20.0, None).mean_damage;
    let crowd = magistar("magistar", &[], 20.0, Some(2.6)).mean_damage;
    let ratio = crowd / alone;
    assert!(
        ratio < 3.0,
        "eight bodies added {ratio:.2}x — a hammer's 0.4 follow through cannot pay that",
    );
    assert!(ratio > 1.2, "eight bodies in reach added only {ratio:.2}x");
}

/// **A SLIDE ATTACK TAKES NO FOLLOW THROUGH**, so a ring of bodies takes
/// one number rather than a decaying series.
///
/// *"Follow Through … (excludes Slam Attacks and Slide Attacks)"* (wiki,
/// Melee), which is `FT = 1.0` — the wiki's own bottom row, every target at
/// 100% — and NOT an absent stat, which would reach the aimed body alone.
/// Asserted as EQUALITY across the ring: at 0.4 the ninth body took 0.4^8,
/// which is 0.07% of the first, so nothing but equality can tell the two
/// readings apart.
///
/// A COMBO SWING IS THE CONTROL beside it. Hell's Wave and Raging Whirlwind
/// are the same weapon at the same reach, and only one of them decays — so
/// this cannot pass by the crowd being unreachable or by the decay being
/// switched off everywhere.
#[test]
fn a_slide_attack_pays_every_body_in_reach_in_full_and_a_swing_does_not() {
    let ring = |form: &str| {
        let r = magistar(form, &[], 20.0, Some(2.6));
        let hit: Vec<f64> =
            r.mean_damage_by_body.0.iter().copied().filter(|d| *d > 0.0).collect();
        (hit.first().copied().unwrap_or(0.0), hit.last().copied().unwrap_or(0.0), hit.len())
    };
    let (first, last, n) = ring("magistar_slide");
    assert_eq!(n, 9, "a spin reaches the aimed body and all eight around it");
    assert!(
        (last - first).abs() < first * 0.02,
        "the ring decayed on a slide attack: {first:.0} -> {last:.0}",
    );
    let (s_first, s_last, _) = ring("magistar");
    assert!(
        s_last < s_first * 0.2,
        "a hammer's 0.4 follow through did not decay a combo swing: {s_first:.0} -> {s_last:.0}",
    );
}

/// A SLAM IGNORES THE REACH THAT DECIDES EVERY OTHER MELEE MODE.
///
/// A hammer swings 2.5 m; its heavy slam is a 10 m sphere centred on the
/// wielder's own feet. So on a crowd standing further apart than the reach,
/// the combo modes hit the one body in front and the slam hits the room —
/// which is why the Magistar is played the way it is, and the reason
/// `BlastKind::Slam` exists.
#[test]
fn only_the_slam_reaches_a_crowd_a_swing_cannot() {
    let alone = |f: &str| magistar(f, &[], 20.0, None).mean_damage;
    let crowd = |f: &str| magistar(f, &[], 20.0, Some(4.0)).mean_damage;
    // Tidal Force, because the other three combos END on a slam.
    let swing = crowd("magistar_forward") / alone("magistar_forward");
    let slam = crowd("magistar_heavy_slam") / alone("magistar_heavy_slam");
    assert!(
        swing < 1.02,
        "a 2.5 m swing found a crowd 3.5 m away: x{swing:.4}",
    );
    assert!(
        slam > 1.5,
        "a 10 m slam sphere did not reach the crowd around it: x{slam:.4}",
    );
}

/// A 360deg SWING IS THE ONLY ONE THAT TAKES A BODY BEHIND YOU.
///
/// The stance tables mark them, and it is the one spatial fact that
/// separates two combos of the same weapon. The fixture puts the crowd in a
/// full ring, so half of it is behind the wielder: an ordinary sweep gets
/// the front, a spin gets all of it.
///
/// IT COUNTS BODIES, NOT DAMAGE, and that is the model teaching rather than
/// the test being lazy. A hammer's Follow Through is 0.4, so the bodies a
/// spin adds are the FOURTH onward and they are worth `0.4^3 = 6.4%` and
/// less: the front three come to 0.624 of a body and all eight to 0.666, a
/// 3% difference in damage for a 2.7x difference in bodies reached. The
/// claim is about who was hit; asserting it on a total would be asserting
/// it on the one number that cannot see it.
#[test]
fn a_spin_reaches_behind_the_wielder_and_a_sweep_does_not() {
    let reached = |f: &str| {
        let r = magistar(f, &[], 20.0, Some(2.6));
        r.mean_damage_by_body.0.iter().filter(|d| **d > 0.0).count()
    };
    // Hell's Wave is one 200% spin; Winding Temper's three swings are all
    // ordinary sweeps, and its closing slam is taken off.
    let spin = reached("magistar_slide");
    let sweep = swings_only("magistar_block", &[], Some(2.6)).mean_damage_by_body.0.iter().filter(|d| **d > 0.0).count();
    assert_eq!(spin, 9, "a spin should reach the aimed body and all eight around it");
    assert!(
        sweep < spin,
        "a sweep reached {sweep} bodies and a spin {spin} — the forward test did nothing",
    );
    assert!(sweep >= 2, "a sweep reached only {sweep} — it should still get the front");
}

/// **MELEE INFLUENCE REACHES WHAT THE SWING CANNOT**, and that is the whole
/// card: a 2.5 m Tonfa against a crowd standing 3.5 m away hits one body,
/// and the arcane puts that body's statuses — and that element's damage —
/// on all eight.
///
/// THE FIXTURE IS THE SAME ONE `only_the_slam_reaches_a_crowd_a_swing_cannot`
/// uses, for the same reason: at 4.0 m the ring is outside every melee
/// reach in the roster and comfortably inside a 20 m spread, so a crowd
/// taking anything at all can only have taken it from here.
///
/// SHOCKING TOUCH IS THE PRICE OF ENTRY. The card is *"On Melee Electricity
/// Status"* and a Praedos is Impact/Puncture/Slash, so a build with no
/// Electricity in its vector can never open the window — which is asserted
/// beside it, because an arcane that fires unconditionally would pass the
/// first claim and fail the game.
#[test]
fn melee_influence_carries_a_swings_statuses_to_a_crowd_out_of_reach() {
    let crowd = |mods: &[&str], arcane: Option<&str>| {
        let r = melee_fight("praedos", &[], mods, arcane, 60.0, Some(4.0));
        r.mean_damage_by_body.0.iter().skip(1).sum::<f64>()
    };
    let bare = crowd(&["shocking_touch"], None);
    let with = crowd(&["shocking_touch"], Some("melee_influence"));
    assert!(
        bare < 1.0,
        "a 2.5 m swing found a crowd 3.5 m away without the arcane: {bare:.1}",
    );
    assert!(
        with > 1000.0,
        "the arcane put nothing on a crowd it reaches by 16 metres: {with:.1}",
    );
    // …AND NOT WITHOUT AN ELECTRICITY STATUS TO OPEN THE WINDOW. Molten
    // Impact is the sharp control rather than a bare weapon: Heat SPREADS,
    // so a build carrying it has everything the second half of the card
    // needs and nothing the first half does. A bare Praedos would pass this
    // even with the trigger deleted, because Impact and Slash cannot
    // spread either.
    let heat = crowd(&["molten_impact"], Some("melee_influence"));
    assert!(
        heat < 1.0,
        "a weapon with no Electricity in its vector opened the window: {heat:.1}",
    );
}

/// …AND WHICH STATUSES IT CARRIES IS THE PAGE'S OWN LIST.
///
/// Ten spread and everything else does not, which partitions `DamageType`
/// exactly — so this is written out rather than asserted as "not physical",
/// the reading that would let the next type in silently.
#[test]
fn melee_influence_carries_the_ten_elements_and_nothing_else() {
    for ty in [
        DamageType::Cold, DamageType::Electricity, DamageType::Heat, DamageType::Toxin,
        DamageType::Blast, DamageType::Corrosive, DamageType::Gas, DamageType::Magnetic,
        DamageType::Radiation, DamageType::Viral,
    ] {
        assert!(influence_can_spread(ty), "{ty:?} is on the wiki's spreadable list");
    }
    for ty in [
        DamageType::Impact, DamageType::Puncture, DamageType::Slash,
        DamageType::Void, DamageType::Tau, DamageType::True, DamageType::Cinematic,
    ] {
        assert!(!influence_can_spread(ty), "{ty:?} is not");
    }
}

/// …AND IT STOPS AT THE ARCANE'S OWN RADIUS, which is the card's and not
/// the weapon's: 20 m at rank 5 against a 2.5 m reach.
///
/// The ring is placed either side of it with the aimed body's own metre of
/// standoff already inside the margin — 15 m is in from every angle, 23 m
/// is out from every angle — because a fixture that lands on the boundary
/// tests the floating point rather than the rule.
#[test]
fn melee_influence_stops_at_the_arcanes_own_radius() {
    let crowd = |gap: f64| {
        let r = melee_fight(
            "praedos", &[], &["shocking_touch"], Some("melee_influence"), 60.0, Some(gap),
        );
        r.mean_damage_by_body.0.iter().skip(1).sum::<f64>()
    };
    assert!(crowd(15.0) > 1000.0, "a 20 m spread missed a ring at 15 m");
    let far = crowd(23.0);
    assert!(far < 1.0, "a 20 m spread reached a ring at 23 m: {far:.1}");
}

/// TRUTH'S FLAME IS A GAMBLE, AND BOTH SIDES OF IT ARE PAID.
///
/// *"Kills grant an additional 4s Tennokai opportunity"*, *"the damage
/// bonus is only active following the first kill"*, and *"Curse activates
/// when failing to kill a target with a Tennokai attack, and reset your
/// Combo Counter"*. So on a fight it can finish it is worth something, and
/// on one it cannot the chain never happens and the curse fires every time.
///
/// THE CONTRAST IS THE ASSERTION. Master's Edge is an unconditional +60% on
/// the same swing, so it pays on both fights; if Truth's Flame ever pays
/// like that on the fight with no kills, its terms have stopped applying.
#[test]
fn truths_flame_pays_where_it_can_kill_and_charges_where_it_cannot() {
    let go = |card: Option<&str>, level: f64| {
        let mut mods = vec!["primed_pressure_point", "sacrificial_steel", "organ_shatter"];
        if let Some(c) = card { mods.push(c); }
        let _ = level;
        melee_fight("praedos", &[], &mods, None, 60.0, Some(3.0))
    };
    // THIS FIXTURE KILLS NOTHING — training dummies — which is exactly the
    // half worth asserting here: the chain never happens, the damage bonus
    // never pays, and the curse fires on every swing. The paying half is
    // measured where kills do happen (42.0 against 35.6 on the group ruler
    // at Lv 60) and asserted by the Tennokai card sweep above, which now
    // requires this card to be worth LESS than the six unconditional ones.
    let (bare, flame) = (go(None, 60.0), go(Some("truths_flame"), 60.0));
    assert!(
        flame.mean_self_damage.of(DamageType::Heat) > 0.0,
        "the curse never fired, so nothing was ever risked",
    );
    // …AND NOTHING CHARGES A BUILD THAT DOES NOT CARRY IT.
    assert_eq!(bare.mean_self_damage.total(), 0.0, "a bare build paid a curse");
}

/// A SWING UNDER FULL STANCE DAMAGE STILL EARNS ITS POINT.
///
/// Measured on Rogue Edict's first input — `200%` then `5x 50%`, one press
/// — which shows SEVEN. Reading the wiki's proportionality alone gives 4.5
/// and a flat point an instance gives 6, so the two halves are both needed
/// and this is the case that says so.
///
/// TIED TO THE DATA, not to a remembered shape: if the stance is ever
/// re-transcribed the arithmetic below is re-read from it.
#[test]
fn a_swing_under_full_stance_damage_still_earns_its_point() {
    let base = crate::model::WeaponBase::from_data("praedos", false, &[]);
    let pool = crate::data::mods::pool_for_weapon("praedos");
    let stance = pool
        .iter()
        .find(|m| m.id == "sovereign_outcast")
        .expect("the stance is in the pool");
    let refs = vec![stance];
    let panel = crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
    // ONE PRESS is the entries up to and including the first that waits.
    let press: Vec<_> = panel
        .combo_script
        .iter()
        .take_while(|h| h.delay_seconds == 0.0)
        .chain(panel.combo_script.iter().find(|h| h.delay_seconds > 0.0))
        .collect();
    assert_eq!(press.len(), 2, "Rogue Edict opens on two entries");
    let earned: f64 = press
        .iter()
        .map(|h| combo_points_for(h.multiplier, f64::from(h.hits)))
        .sum();
    assert!(
        (earned - 7.0).abs() < 1e-9,
        "the first input earns 7 in game, this says {earned}: {:?}",
        press.iter().map(|h| (h.multiplier, h.hits)).collect::<Vec<_>>(),
    );
}

/// …AND IT PAYS ON ONE BODY TOO, because the body that was struck is
/// inside the radius it is the centre of.
///
/// MEASURED BY THE OWNER, and it replaces the opposite claim. The arcane is
/// one instance dealt to everything within reach, and zero metres is within
/// reach: the host takes the element's damage again and it force-procs
/// there like anywhere else. The wiki's example counts "every other enemy"
/// because that is the half of it the sentence is about — it is not a
/// statement that the host is skipped.
///
/// It matters most where it is least visible: a single-target ruler reads
/// this arcane as worth nothing at all if the host is excluded, and that is
/// the shape of the board it would publish.
#[test]
fn melee_influence_pays_on_the_body_it_came_from() {
    let lone = |arcane: Option<&str>| {
        melee_fight("praedos", &[], &["shocking_touch"], arcane, 60.0, None).mean_damage
    };
    let bare = lone(None);
    let with = lone(Some("melee_influence"));
    assert!(
        with > bare,
        "the host took no copy of its own status: {bare:.4} -> {with:.4}",
    );
    // …AND IT IS THE ELEMENT'S OWN NUMBER, not a rounding: an Electricity
    // build spreading Electricity moves the aimed body by a real share.
    assert!(
        with > bare * 1.05,
        "the copy landed but paid almost nothing: {bare:.4} -> {with:.4}",
    );
}

/// A SWING SWEEPS AN ARC, and `MELEE_ARC_DEG` is what decides who is in it.
///
/// The bodies stand at KNOWN angles off the aim line and at the SAME
/// distance, so the reach decides nothing and neither sits on the boundary:
/// 30 degrees is inside a 90-degree arc, 60 is outside, and a half-plane
/// would take both.
///
/// It is the one invented number in this geometry — the game publishes an
/// arc for no stance — so it is the one that needs a test saying which
/// reading the model holds.
#[test]
fn a_swing_sweeps_an_arc_rather_than_everything_in_front() {
    // …the aim is +y (`Arena::training`), so an angle off it is measured
    // from there and both signs are the same claim.
    let body_at = |off_deg: f64| {
        let a = (90.0 - off_deg).to_radians();
        crate::formation::FoeSpec {
            id: format!("e{off_deg}"),
            params: TargetParams::training_dummy(),
            body_parts: BodyPart::humanoid(),
            // 2.8 m centre to centre is a 2.3 m gap against a 2.5 m reach:
            // comfortably inside, and not the knife edge 3.0 would be.
            at: crate::rules::space::Vec2::new(2.8 * a.cos(), 2.8 * a.sin()),
        }
    };
    let base = crate::model::WeaponBase::from_data("magistar", false, &[]);
    let panel = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
    let mut arena = crate::arena::Arena::training(1.0);
    arena.others = vec![body_at(30.0), body_at(60.0)];
    let p = FightParams::for_panel(&panel, &arena, &crate::data::arcanes::ArcaneFx::none(), || {
        panel.clone()
    });
    assert_eq!(
        p.melee_struck(false, 0.0),
        vec![0, 1],
        "a 90-degree sweep takes the aimed body and the one 30 degrees off it, and nothing at 60",
    );
    assert_eq!(
        p.melee_struck(true, 0.0),
        vec![0, 1, 2],
        "a spin takes everything in range whatever angle it stands at",
    );
}
