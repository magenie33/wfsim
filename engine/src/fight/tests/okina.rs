use super::*;

/// THE OKINA, resolved the way the page resolves a melee Incarnon: twice, the
/// armed panel with EVO1 and the unarmed one without (`FightParams::for_panel`).
///
/// `frail` puts a level-1 unit on the target and on four bodies beside it, all
/// inside the daggers' 15 m, because a dagger is made by a KILL and the
/// training dummy cannot die. `armed` holds the window open from the first
/// swing, the card knob a test uses to ask what the form is worth.
fn okina(form: &str, evos: &[&str], mods: &[&str], frail: bool, armed: bool) -> FightParams {
    okina_with(form, evos, mods, None, frail, armed)
}

/// …WITH A MELEE ARCANE, at its max rank.
fn okina_with(
    form: &str,
    evos: &[&str],
    mods: &[&str],
    arcane: Option<&str>,
    frail: bool,
    armed: bool,
) -> FightParams {
    let weapon = if form.starts_with("okina_prime") { "okina_prime" } else { "okina" };
    let base = crate::model::WeaponBase::from_data(form, false, evos);
    let pool = crate::data::mods::pool_for_weapon(weapon);
    let refs: Vec<&crate::model::ModDef> =
        mods.iter().filter_map(|id| pool.iter().find(|m| m.id == *id)).collect();
    assert_eq!(refs.len(), mods.len(), "every mod is in the Okina's pool: {mods:?}");
    let panel = crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
    let mut arena = crate::arena::Arena::training(30.0);
    if frail {
        let unit = crate::data::enemies::all()
            .into_iter()
            .find(|e| e.id == "corrupted_heavy_gunner")
            .expect("the roster has one");
        let body = unit
            .target_params(1, false, false, TargetMode::InstantRespawn)
            .expect("a level 1 ordinary unit is legal");
        arena.target = body.clone();
        arena.others = (1..=4)
            .map(|i| crate::formation::FoeSpec {
                id: String::new(),
                params: body.clone(),
                body_parts: BodyPart::humanoid(),
                at: crate::rules::space::Vec2::new(1.5 + i as f64, 1.0),
            })
            .collect();
    }
    let fx = arcane.map_or_else(crate::data::arcanes::ArcaneFx::none, |id| {
        let card = crate::data::arcanes::pool_for_weapon(form, "melee")
            .into_iter()
            .find(|a| a.id == id)
            .unwrap_or_else(|| panic!("the melee pool seats {id}"));
        card.fx(card.max_rank, crate::model::StackPolicy::Emergent, &[], crate::data::tenno::default_tenno())
    });
    let mut p = FightParams::for_panel(&panel, &arena, &fx, || {
        let unarmed: Vec<&str> = evos
            .iter()
            .copied()
            .filter(|id| !crate::data::evolutions::states_incarnon_window(id))
            .collect();
        let b = crate::model::WeaponBase::from_data(form, false, &unarmed);
        crate::build::loadout::resolve(&b, &refs, crate::model::StackPolicy::Emergent)
    });
    if armed {
        let mut c = BuffConfig::new();
        c.insert("melee_incarnon".into(), (1, true));
        p.apply_buff_config(&c);
    }
    p
}

/// The dagger rows of one fight's record, as `(t, effective)`.
fn dagger_rows(p: &FightParams) -> Vec<(f64, f64)> {
    rows_of(p, crate::record::Origin::SpectralDagger)
}

/// One origin's rows of one fight's record, as `(t, effective)`.
fn rows_of(p: &FightParams, origin: crate::record::Origin) -> Vec<(f64, f64)> {
    record(p, 0x0C1A, 0.0, 30.0, 1_000_000, 0)
        .events()
        .iter()
        .filter_map(|e| match &e.kind {
            crate::record::Kind::Damage(d) if d.origin == origin => {
                Some((e.t, d.effective))
            }
            _ => None,
        })
        .collect()
}

/// **A KILL MAKES A DAGGER, AND THE DAGGER HOLDS UP NO SWING.**
///
/// The same fight twice with the same dice, the daggers switched off in the
/// second: the swing count is identical — a dagger flies on its own clock —
/// and only the first has dagger rows in its record.
#[test]
fn a_kill_makes_a_dagger_and_the_dagger_holds_up_no_swing() {
    let evo = ["okina_evo1_incarnon_form"];
    let with = okina("okina", &evo, &[], true, true);
    assert!(with.spectral_dagger.is_some(), "the armed panel makes daggers");
    let mut without = with.clone();
    without.spectral_dagger = None;

    let a = run_once(&with, &mut Rng::new(0x0C1A));
    let b = run_once(&without, &mut Rng::new(0x0C1A));
    assert!(b.kills > 3, "the fixture has to kill: {}", b.kills);
    assert_eq!(a.shots, b.shots, "a dagger must not cost the swing loop a single swing");

    let rows = dagger_rows(&with);
    assert!(!rows.is_empty(), "kills in the form must make daggers that land");
    assert!(dagger_rows(&without).is_empty());
    // A STRIKE IS NEVER BEFORE ITS ORBIT: the first dagger is made by the first
    // kill, and it circles a full second before it seeks.
    // …AND IT LANDS AT ITS KILL'S OWN TIME plus the orbit and the flight — not
    // at the next swing's: the first kill makes the first dagger.
    let first_kill = b.first_kill_at.expect("a kill");
    let g = with.spectral_dagger.expect("daggers");
    let flight = with
        .body_positions()
        .iter()
        .map(|&at| crate::rules::space::gap(with.player_at, at))
        .fold(f64::INFINITY, f64::min)
        / g.speed_mps;
    let due = first_kill + g.rules.orbit_seconds + flight;
    assert!(
        (rows[0].0 - due).abs() < 1e-9,
        "the first dagger lands at {:.4} s, due at {due:.4} s (kill {first_kill:.4} s)",
        rows[0].0
    );
}

/// **NO FORM, NO DAGGERS.** A light combo with no Tennokai card performs no
/// heavy attack, so the Incarnon Form is never armed — and the kills it makes
/// in the meantime make nothing.
#[test]
fn a_kill_outside_the_form_makes_no_dagger() {
    let p = okina("okina", &["okina_evo1_incarnon_form"], &[], true, false);
    assert!(run_once(&p, &mut Rng::new(0x0C1A)).kills > 3, "the fixture has to kill");
    assert!(dagger_rows(&p).is_empty(), "the form was never up, so no kill made a dagger");
}

/// **WHAT REACHES A DAGGER, AND WHAT DOES NOT** — read off the resolved panel,
/// where each bucket is one term and nothing else moves it.
///
/// *"Dagger direct hits and explosions are not affected by melee damage
/// bonuses, such as Incarnon form's innate +100% melee damage, Pressure
/// Point"*; *"Dagger damage type can be changed with elemental mods"*; and the
/// EVO2 pair's base damage reaches the daggers and not the swing (the page's
/// Bugs).
#[test]
fn melee_damage_misses_the_dagger_and_elements_and_the_evo2_bug_reach_it() {
    let dagger = |evos: &[&str], mods: &[&str]| {
        let base = crate::model::WeaponBase::from_data("okina", false, evos);
        let pool = crate::data::mods::pool_for_weapon("okina");
        let refs: Vec<&crate::model::ModDef> =
            mods.iter().filter_map(|id| pool.iter().find(|m| m.id == *id)).collect();
        let panel = crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        let g = panel.spectral_dagger.expect("EVO1 makes daggers");
        (g.strike.damage.total(), g.blast.damage.total(), panel.damage.total())
    };
    let evo = ["okina_evo1_incarnon_form"];
    let (strike, blast, swing) = dagger(&evo, &[]);
    assert!((strike - 140.0).abs() < 1e-9, "the infobox's 21 + 49 + 70, untouched by +100%: {strike}");
    assert!((blast - 140.0).abs() < 1e-9, "the explosion's 140 Cold: {blast}");
    assert!((swing - 280.0).abs() < 1e-9, "…while the swing takes the Form's +100%: {swing}");

    let (pp, _, pp_swing) = dagger(&evo, &["pressure_point"]);
    assert!((pp - strike).abs() < 1e-9, "Pressure Point must not reach the dagger: {pp}");
    assert!(pp_swing > swing, "…and does reach the swing");

    let (cold, _, _) = dagger(&evo, &["north_wind"]);
    assert!(cold > strike * 1.5, "an elemental mod reaches it: {strike} -> {cold}");

    let (red, red_blast, red_swing) = dagger(&["okina_evo1_incarnon_form", "okina_seeing_red"], &[]);
    assert!((red - 200.0).abs() < 1e-9, "+60 base damage on the dagger: {red}");
    assert!((red_blast - 200.0).abs() < 1e-9, "…on both of its halves: {red_blast}");
    assert!((red_swing - swing).abs() < 1e-9, "…and none on the swing: {red_swing}");
}

/// **SEEING RED PAYS THROUGH THE COUNTER AND NOWHERE ELSE.** True Kiss forces a
/// Slash on its second input, so from then on every hit lands on a bleeding
/// target and earns five more points. With Blood Rush reading the counter that
/// is damage; with nothing reading it the fight is the same fight, to the digit
/// — which is also the proof its `+60` never reached the swing.
#[test]
fn seeing_red_pays_through_the_counter_and_nowhere_else() {
    let run = |evos: &[&str], mods: &[&str]| monte_carlo(&okina("okina", evos, mods, false, false), 12, 7).mean_damage;
    let red = ["okina_seeing_red"];
    assert_eq!(run(&[], &[]), run(&red, &[]), "nothing reads the counter, so the perk pays nothing");
    let off = run(&[], &["blood_rush"]);
    let on = run(&red, &["blood_rush"]);
    assert!(on > off * 1.05, "Blood Rush reads a counter Seeing Red fills faster: {off:.0} -> {on:.0}");
}

/// **SYNERGIST SURETY IS STATUS DAMAGE, ON A CLOCK THE CRITS KEEP.** It grows
/// what every bleed ticks for and leaves every direct hit exactly where it was.
#[test]
fn synergist_surety_grows_the_bleeds_and_not_the_hits() {
    let run = |evos: &[&str]| monte_carlo(&okina("okina_prime", evos, &[], false, false), 12, 7);
    let off = run(&[]);
    let on = run(&["okina_prime_synergist_surety"]);
    assert!(
        on.mean_dot_damage > off.mean_dot_damage * 1.05,
        "five stacks of +6% status damage on a 30% crit weapon: {:.0} -> {:.0}",
        off.mean_dot_damage,
        on.mean_dot_damage
    );
    let direct = |s: &Summary| s.mean_damage - s.mean_dot_damage;
    assert!(
        (direct(&on) - direct(&off)).abs() < 1e-6 * direct(&off),
        "a direct hit is not a status: {:.3} vs {:.3}",
        direct(&off),
        direct(&on)
    );
}

/// **MELEE CAREEN REACHES THE DAGGER**, and the dagger is what feeds it: ten
/// forced Cold stacks is a frozen target, so the explosion after a strike and
/// every dagger after the first lands on one — *"Daggers can benefit from Melee
/// Careen's damage multiplier against frozen enemies"*.
#[test]
fn melee_careen_multiplies_a_dagger_on_a_frozen_body() {
    let evo = ["okina_evo1_incarnon_form"];
    let off = dagger_rows(&okina_with("okina", &evo, &[], None, true, true));
    let on = dagger_rows(&okina_with("okina", &evo, &[], Some("melee_careen"), true, true));
    let mean = |v: &[(f64, f64)]| v.iter().map(|x| x.1).sum::<f64>() / v.len().max(1) as f64;
    assert!(!off.is_empty() && !on.is_empty());
    assert!(
        mean(&on) > mean(&off) * 1.3,
        "x2.5 on a frozen body: a dagger row averages {:.0} -> {:.0}",
        mean(&off),
        mean(&on)
    );
}

/// **INFLUENCE CARRIES A DAGGER'S EXPLOSION, AND NOT ITS FORCED COLD.** With the
/// window held open, the Okina's swings spread nothing (they deal no element),
/// so every Influence row in the fight is a dagger's. With the explosion's own
/// status roll taken away, what is left is forced Cold alone, and that spreads
/// nothing at all.
#[test]
fn influence_carries_a_daggers_explosion_and_not_its_forced_cold() {
    let evo = ["okina_evo1_incarnon_form"];
    let mut p = okina_with("okina", &evo, &[], Some("melee_influence"), true, true);
    p.influence_open = Some(f64::INFINITY);
    let mut no_daggers = p.clone();
    no_daggers.spectral_dagger = None;
    assert!(
        rows_of(&no_daggers, crate::record::Origin::Influence).is_empty(),
        "the swings carry no element, so they spread nothing"
    );
    assert!(
        !rows_of(&p, crate::record::Origin::Influence).is_empty(),
        "a dagger's explosion rolls Cold, and Influence carries it"
    );
    let mut forced_only = p.clone();
    if let Some(g) = forced_only.spectral_dagger.as_mut() {
        g.blast.status_chance = 0.0;
        g.strike.status_chance = 0.0;
    }
    assert!(
        rows_of(&forced_only, crate::record::Origin::Influence).is_empty(),
        "forced Cold is never spread"
    );
}

/// **A DAGGER IS BORN AT ITS KILL'S OWN TIME**, not at the swing that next looks
/// — a status tick that kills between two swings has its dagger orbiting from
/// that moment. And six is the most there can be: two more kills with no room
/// make nothing.
#[test]
fn a_dagger_is_born_at_its_kills_own_time_and_six_is_the_most() {
    let p = okina("okina", &["okina_evo1_incarnon_form"], &[], true, true);
    let g = p.spectral_dagger.expect("daggers");
    let flight = p
        .body_positions()
        .iter()
        .map(|&at| crate::rules::space::gap(p.player_at, at))
        .fold(f64::INFINITY, f64::min)
        / g.speed_mps;
    let mut r = RunResult { kills: 8, kill_clock_on: true, ..Default::default() };
    for (k, at) in [3.0, 3.25, 3.5, 3.75, 4.0, 4.1, 4.15, 4.19].into_iter().enumerate() {
        r.kill_clock[k] = at;
    }
    r.kill_clock_len = 8;
    let (mut mark, mut live) = (0u32, Vec::new());
    make_daggers(Seat::WIELDER, &p, &p, 4.2, &mut r, &mut mark, &mut live);
    assert_eq!(live.len(), 6, "up to six at once");
    for (k, born) in [3.0, 3.25, 3.5, 3.75, 4.0, 4.1].into_iter().enumerate() {
        let due = born + g.rules.orbit_seconds + flight;
        let at = live[k].due().expect("a dagger strikes");
        assert!((at - due).abs() < 1e-9, "dagger {k}: {at} against {due}");
    }
    assert_eq!((mark, r.kill_clock_len), (8, 0), "every kill is spent and the clock is emptied");
}

/// **A STACKING CRIT-DAMAGE GRANT REACHES THE DAGGER.** Galvanized Steel held at
/// four stacks against the same card with its kill trigger denied: the dagger
/// is a timed part, and its crit damage was read without the stacking grants.
#[test]
fn galvanized_steel_stacks_reach_the_dagger() {
    let evo = ["okina_evo1_incarnon_form"];
    let mut held = okina("okina", &evo, &["galvanized_steel"], true, true);
    let mut c = BuffConfig::new();
    c.insert("galvanized_steel".into(), (4, true));
    held.apply_buff_config(&c);
    let mut none = okina("okina", &evo, &["galvanized_steel"], true, true);
    none.deny_buff_triggers(&["kill".to_string()]);
    let mean = |v: &[(f64, f64)]| v.iter().map(|x| x.1).sum::<f64>() / v.len().max(1) as f64;
    let (held, none) = (dagger_rows(&held), dagger_rows(&none));
    assert!(!held.is_empty() && !none.is_empty());
    assert!(
        mean(&held) > mean(&none) * 1.1,
        "+120% crit damage on the dagger: a dagger row averages {:.0} -> {:.0}",
        mean(&none),
        mean(&held)
    );
}
