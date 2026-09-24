use super::*;
use crate::data::abilities::{resolve, AbilityPick, Caster};
use crate::rules::damage::DamageVector;

/// THE FOUR READINGS OF M79, and the arithmetic that lands on all of them.
///
/// A Magistar carrying a `+100 Base Damage` Incarnon perk was measured four
/// ways, and one formula fits every one:
/// **`mods x (weapon base x attack multiplier + flat)`**, with Condition
/// Overload reading the weapon's half only and Eclipse not reaching the
/// term CO put in the bracket.
///
/// Every number below the weapon comes from data — the base, the perk,
/// Primed Pressure Point — so the four readings are what this test adds.
#[test]
fn the_magistars_measured_flat_base_damage_ladder() {
    let evos = ["magistar_evo1_incarnon_form", "magistar_edge_of_justice"];
    let base = crate::model::WeaponBase::from_data("magistar", true, &evos);
    assert!((base.base_vector.total() - 310.0).abs() < 1e-9, "the fixture moved");
    assert!((base.unswung_base - 100.0).abs() < 1e-9, "the perk is the flat add");
    let f = base.unswung_fraction();

    // READING 1 — the stance's forward combo at x2, nothing else on the
    // build, measured **536** on a Deimos Runner (every physical type x1.0
    // there, so no faction column to read past).
    let swung = base.base_vector.scale((1.0 - f) * 2.0 + f);
    let mb = base.base_vector.total() * ((1.0 - f) * 2.0 + f);
    assert!((mb - 520.0).abs() < 1e-9, "the attack's base is {mb}");
    let hit = swung.quantized_against(mb).total();
    assert!((hit - 536.25).abs() < 0.01, "computed {hit:.2} against a measured 536");
    // …and the flat INSIDE the multiplier — what this engine did — reads
    // 639, which is the reading this test exists to refuse.
    let inside = base.base_vector.scale(2.0).quantized_against(620.0).total();
    assert!((inside - 639.375).abs() < 0.01, "{inside}");

    // READING 3 — the HEAVY SLAM at x3 under Primed Pressure Point's +165%,
    // measured **2902** against Deimos' x1.5 Blast column: 2902 / 1.5 is
    // 1934.7 and the formula says 1934.5. The explosion has carried the add
    // as an absolute since M69, so this half asserts the base it lands on.
    let slam = crate::model::WeaponBase::from_data("magistar_heavy_slam", true, &evos);
    let r = slam.radial.as_ref().expect("the heavy slam explodes");
    assert!((r.base_vector.total() - 730.0).abs() < 1e-9,
        "the explosion's base is {}", r.base_vector.total());
    assert!(((r.base_vector.total() * 2.65) - 1934.5).abs() < 0.01);
    // …and READING 2, the ordinary slam at x2 on the same build: 1378.
    assert!((2.65_f64 * (210.0 * 2.0 + 100.0) - 1378.0).abs() < 0.01);

    // READING 4 — the same forward swing under a 60% Heat mod, one
    // Condition Overload stack (+80% on one status type) and a 30%
    // Eclipse, measured **1644**. The bracket is the weapon's half only,
    // so CO reads 420 of the attack's 520; the grid is that 520 and never
    // the elements, which ride on top of it.
    let mut hot = base.base_vector;
    hot.add(DamageType::Heat, base.base_vector.total() * 0.6);
    let swung = hot.scale((1.0 - f) * 2.0 + f);
    let co_fraction = base.co_base_fraction() * 2.0 / ((1.0 - f) * 2.0 + f);
    let bracket = 1.0 + 0.8 * co_fraction;
    let at = mb * bracket;
    let snap = swung.scale(bracket).quantized_against(at).total();
    assert!((snap - 1391.0).abs() < 0.01, "{snap}");
    // ECLIPSE MISSES THE CO SHARE, which is the whole of M79's fourth
    // reading: multiplying the finished bracket reads 1808 instead.
    let co_share = (0.8 * co_fraction) / bracket;
    let out = snap * eclipse_at(1.3, co_share);
    assert!((out - 1644.5).abs() < 0.6, "computed {out:.1} against a measured 1644");
    assert!((snap * 1.3 - 1808.3).abs() < 0.6, "{}", snap * 1.3);
}

/// A fixed weapon and a fixed fight, so the only thing moving is the buff.
fn params(abilities: &[(&'static str, Option<f64>)], strength: f64) -> FightParams {
    let picks: Vec<AbilityPick<'static>> = abilities
        .iter()
        .map(|(id, secs)| AbilityPick { id, duration_seconds: *secs, element: None })
        .collect();
    FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 100.0),
        dot_modified_base: Some(100.0),
        fire_rate: 1.0,
        magazine_size: 1e9,
        duration_seconds: 10.0,
        base_crit_chance: 0.0,
        unmodded_crit_chance: 0.0,
        body_parts: vec![BodyPart {
            name: "body".into(),
            aim_weight: 1.0,
            multiplier: 1.0,
            is_head: false,
            is_weak_point: false,
            crit_bonus: false,
        }],
        abilities: resolve(&picks, &Caster { strength, ..Default::default() }, "", "melee"),
        ..FightParams::default()
    }
}

fn direct(p: &FightParams) -> f64 {
    run_once(p, &mut crate::rules::rng::Rng::new(3)).sources.direct
}

/// MICROWAVE IS A STATUS TYPE AND NOTHING ELSE, which is exactly what makes
/// it worth modelling: it is invisible in game and invisible in a damage
/// vector, and the only place it shows up is the COUNT a Condition-Overload
/// bonus multiplies.
///
/// VERBATIM (wiki `Microwave`): *"not listed in the game UI, however is
/// counted towards the damage calculation bonus with Condition-Overload
/// type equipment"*, naming Condition Overload, Galvanized Aptitude,
/// Galvanized Savvy, Galvanized Strike, Secondary Shiver and the Cedo.
///
/// Asserted as an EXACT step of one type's worth, not as a direction: at
/// +50% a type, one more type is x1.5 on the CO bucket, and a fixture that
/// merely went up would pass with it counted twice.
#[test]
fn microwave_is_one_more_status_type_and_nothing_else() {
    let fixture = |on: bool| {
        let mut p = params(&[], 1.0);
        p.applies_microwave = on;
        // ONE status type from the vector — Toxin — plus Microwave when it
        // is on. A forced proc so the count is not a coin flip.
        p.damage = DamageVector::new().with(DamageType::Toxin, 100.0);
        p.dot_modified_base = Some(100.0);
        p.status_chance = 1.0;
        p.co_per_type = 0.5;
        p.co_base = crate::model::CoBase::whole();
        p.magazine_size = 1e9;
        p.duration_seconds = 20.0;
        run_once(&p, &mut crate::rules::rng::Rng::new(17))
    };
    let off = fixture(false).sources.direct;
    let on = fixture(true).sources.direct;
    assert!(off > 0.0, "the fixture must deal damage");
    // 1 type -> x1.5, 2 types -> x2.0. The RATIO of those is 4/3, and it
    // is that rather than "more" because a double count would read 5/3.
    let ratio = on / off;
    assert!((ratio - 4.0 / 3.0).abs() < 0.02,
        "one more status type at +50% each: expected x1.333, got x{ratio:.4}");

    // …AND A WEAPON THAT DOES NOT APPLY IT CANNOT BE HANDED ONE. Two
    // weapons in the game have it and the data says which.
    assert!(crate::data::weapons::spec("kuva_nukor").is_some_and(|s| s.applies_microwave));
    assert!(crate::data::weapons::spec("torid").is_some_and(|s| !s.applies_microwave));
}

/// ENERGIZED MUNITIONS BUYS RELOADS, not damage — the first ability buff in
/// this file that moves no damage bracket at all.
///
/// VERBATIM (Energized_Munitions): "improve all equipped weapons' Ammo
/// Efficiency by 75%", and "This ability reduces ammo usage to 1 after
/// every 4 shots. The way this works is by dividing the ammo cost so each
/// shot consumes a quarter of the original, and keeps track of the
/// fractions as well."
///
/// So a 4-round magazine fires SIXTEEN shots per reload, and the number
/// this test reads is the reload count. Asserted as an exact ratio rather
/// than a direction: 75% is the one figure the whole ability is, and a
/// wrong bracket (adding where it multiplies, or landing on the reserve
/// instead of the magazine) moves it off 4x immediately.
#[test]
fn energized_munitions_quarters_what_a_shot_costs_the_magazine() {
    let fixture = |abilities: &[(&'static str, Option<f64>)]| {
        let mut p = params(abilities, 1.0);
        // A SMALL magazine and a long fight, so reloads are the thing being
        // counted; `params` gives 1e9 rounds, which never reloads at all.
        p.magazine_size = 4.0;
        p.reload_seconds = 1.0;
        p.duration_seconds = 120.0;
        p.infinite_reserve = true;
        run_once(&p, &mut crate::rules::rng::Rng::new(5))
    };
    let plain = fixture(&[]);
    let buffed = fixture(&[("energized_munitions", None)]);
    assert!(plain.reloads > 10, "the fixture must actually reload: {}", plain.reloads);
    // SHOTS PER RELOAD is the invariant, not the reload COUNT: a buffed run
    // spends less of the 120 s reloading, so it fires more shots and the
    // counts move by less than four. A 4-round magazine at quarter cost is
    // 16 shots before it runs dry, and that is exactly the ability.
    let per = |r: &RunResult| f64::from(r.shots) / f64::from(r.reloads.max(1));
    let ratio = per(&buffed) / per(&plain);
    assert!((ratio - 4.0).abs() < 0.25,
        "75% efficiency should quarter what a shot costs: {:.1} -> {:.1} shots a magazine (x{ratio:.2})",
        per(&plain), per(&buffed));
    // …AND THAT IS WHERE THE DPS COMES FROM: the same 120 seconds, fewer of
    // them spent reloading, so more shots leave the barrel. This is the
    // whole reason an ammo buff belongs in a damage calculator.
    assert!(buffed.shots > plain.shots,
        "fewer reloads must buy shots: {} -> {}", plain.shots, buffed.shots);

    // …AND ABILITY STRENGTH DOES NOT MOVE IT. The page's row carries no
    // Strength icon, so a 300%-strength frame gets the same 75% — a card
    // that scaled it would promise 225% efficiency, i.e. free shooting.
    let strong = {
        let mut p = params(&[("energized_munitions", None)], 3.0);
        p.magazine_size = 4.0;
        p.reload_seconds = 1.0;
        p.duration_seconds = 120.0;
        p.infinite_reserve = true;
        run_once(&p, &mut crate::rules::rng::Rng::new(5))
    };
    assert_eq!(strong.reloads, buffed.reloads,
        "ammo efficiency is not affected by ability strength");
}

/// ROAR IS A BANE MOD, and this asserts exactly that and nothing more: it
/// lands in the bracket `faction_multiplier` already is, so a +50% Roar is x1.5
/// on the hit — and the bracket's own squaring on status follows without a
/// line of code, which the DoT test below is for.
#[test]
fn roar_multiplies_the_hit_by_its_faction_bracket() {
    let none = direct(&params(&[], 1.0));
    let roar = direct(&params(&[("roar", None)], 1.0));
    assert!(none > 0.0);
    assert!((roar / none - 1.5).abs() < 1e-9, "x{:.4}", roar / none);

    // …and STRENGTH is linear, so 200% strength is +100%.
    let strong = direct(&params(&[("roar", None)], 2.0));
    assert!((strong / none - 2.0).abs() < 1e-9, "x{:.4}", strong / none);
}

/// A TARGET'S OWN MULTIPLIER RIDES THE FACTION BRACKET, and this pins the
/// POSITION rather than the size — the size is one yaml field and moves
/// with a measurement, the position is the claim (MEASUREMENTS M89).
///
/// Roar is the instrument because it is the only bracket member that can be
/// on the WRONG SIDE and still look right on a hit: at x0.8 and +50%,
/// "inside" is `(1 + 0.5) * 0.8 = 1.2` and "outside" is `1 * 0.8 + 0.5 =
/// 1.3`. Both are "Roar helps"; only one is the bracket.
#[test]
fn a_targets_multiplier_rides_the_faction_bracket_with_roar_inside_it() {
    let plain = direct(&params(&[], 1.0));
    assert!(plain > 0.0);

    let cut = direct(&{
        let mut p = params(&[], 1.0);
        p.foe.faction_bracket_multiplier = 0.8;
        p
    });
    assert!((cut / plain - 0.8).abs() < 1e-9, "x{:.4}", cut / plain);

    let roared = direct(&{
        let mut p = params(&[("roar", None)], 1.0);
        p.foe.faction_bracket_multiplier = 0.8;
        p
    });
    assert!(
        (roared / plain - 1.2).abs() < 1e-9,
        "Roar left the bracket: x{:.4}, and x1.3 is the multiplier applied outside it",
        roared / plain
    );
}

/// …AND IT IS RAISED WITH THE BRACKET, which is the half a direct hit
/// cannot show. MEASUREMENTS M89's two readings, as the arithmetic that
/// forces them: an Aklex Prime with nothing equipped, base 150, into a
/// Demolisher's head, read off the Slash bleed.
#[test]
fn the_targets_multiplier_is_squared_on_a_status_exactly_as_a_bane_is() {
    let bleed = |f: f64| BLEED_COEFFICIENT * 150.0 * 3.0 * faction_at(f, DEPTH_PROC);
    assert!((bleed(0.8) - 100.8).abs() < 0.5, "{}", bleed(0.8));
    assert!((bleed(1.55 * 0.8) - 242.2).abs() < 0.5, "{}", bleed(1.55 * 0.8));

    // THE TWO READINGS THAT WERE NOT THOSE, so the test fails on either
    // wrong answer rather than only on a missing one: absent, and applied
    // once whatever the depth.
    assert!((bleed(1.0) - 157.5).abs() < 0.5);
    assert!((0.8 * bleed(1.0) - 126.0).abs() < 0.5);
    assert!((bleed(1.55) - 378.4).abs() < 0.5);
    assert!((0.8 * bleed(1.55) - 302.7).abs() < 0.5);
}

/// A DoT TRACKS ITS SOURCE, so a buff that ENDS mid-burn stops paying for
/// the rest of it — and one that is up for the whole burn pays throughout.
///
/// Measured in game: a buff the shooter gains while a DoT is burning
/// strengthens the burn IMMEDIATELY, on an elemental bonus (Lavos) and on a
/// faction bonus. So a tick reads its source LIVE; snapshotting every
/// factor at the moment the proc landed is the reading the measurement
/// rules out.
///
/// THE TEST GOES THE OTHER WAY ROUND because it is the same claim and it is
/// the one this harness can stage: abilities here start at zero, so a SHORT
/// Roar is a source buff that disappears under a burn already running. If
/// the tick were still a snapshot, every tick would carry Roar and the two
/// numbers below would be equal.
#[test]
fn a_dot_follows_its_source_and_a_buff_that_ends_stops_paying() {
    let bleed = |seconds: Option<f64>| {
        let mut p = params(&[("roar", seconds)], 1.0);
        p.damage = DamageVector::new().with(DamageType::Slash, 100.0);
        p.status_chance = 1.0;
        p.base_status_chance = 1.0;
        p.foe.base_health = 1e15;
        // ONE SHOT, so every tick after it belongs to one burn and there is
        // no second proc landing under a different buff state to confuse
        // the reading.
        p.fire_rate = 0.02;
        p.duration_seconds = 30.0;
        run_once(&p, &mut crate::rules::rng::Rng::new(3)).tally.dot()
    };
    let none = bleed(Some(0.0));
    let whole = bleed(None);
    let brief = bleed(Some(2.0));

    assert!(none > 0.0 && whole > none, "roar must pay: {none} -> {whole}");
    // A ROAR THAT ENDS PAYS LESS THAN ONE THAT DOES NOT, which is the
    // whole claim — and it cannot be true of a snapshot.
    assert!(
        brief < whole * 0.999,
        "a Roar that ended mid-burn still paid for the whole burn:              {brief} against {whole} — the tick is still a snapshot"
    );
    // …AND MORE THAN NO ROAR AT ALL, or the early ticks were lost too and
    // the reading says nothing about WHEN it stopped.
    assert!(
        brief > none * 1.001,
        "the ticks under Roar were not paid either: {brief} against {none}"
    );

    // ECLIPSE GOES THE OTHER WAY, and it is the control that makes the
    // reading above mean something. Both are abilities with a duration, so
    // if the split were "abilities are live" they would behave alike — they
    // do not, because the split is about the BRACKET: the faction bonus is
    // part of what a status inherits and the FINAL multiplier is applied
    // once, at the proc (owner measured both; the wiki draws
    // the same line for its own reason).
    let ebleed = |seconds: Option<f64>| {
        let mut p = params(&[("eclipse", seconds)], 1.0);
        p.damage = DamageVector::new().with(DamageType::Slash, 100.0);
        p.status_chance = 1.0;
        p.base_status_chance = 1.0;
        p.foe.base_health = 1e15;
        p.fire_rate = 0.02;
        p.duration_seconds = 30.0;
        run_once(&p, &mut crate::rules::rng::Rng::new(3)).tally.dot()
    };
    let e_whole = ebleed(None);
    let e_brief = ebleed(Some(2.0));
    assert!(e_whole > none, "eclipse must pay something: {none} -> {e_whole}");
    assert!(
        (e_brief - e_whole).abs() < e_whole * 1e-9,
        "an Eclipse that ended mid-burn changed the burn: {e_brief} against              {e_whole} — the final multiplier is being re-read and should be frozen"
    );
}

/// AND IT DOUBLE-DIPS ON STATUS, which is the difference from Eclipse.
/// A Slash DoT applied under +50% Roar ticks for 1.5^2 = 2.25x — the wiki's
/// "the bonus is used twice in the calculation of status damage".
///
/// **THE DOUBLE DIP IS ON THE SEED, AND THE ACCUMULATOR'S `1` IS NOT PART
/// OF IT.** The page's own Toxin example is
/// `(40 × 1.55 + 1) × 0.5 × 3.25 × 1.55` — the faction bonus inside the
/// seed AND in `M`, with the `1` added between them, so it takes exactly
/// one of the two layers (`Dot::accumulator_unit`). The ratio is therefore
/// `f²` only in the limit where the seed dwarfs the 1: at a base of 100 it
/// reads 2.2446, and the test asserts the LIMIT so that what is being
/// pinned stays the mechanic rather than one fixture's base damage.
///
/// Eclipse is unaffected either way, and that is not a coincidence: it is a
/// FINAL multiplier, so it multiplies the accumulator and the seed alike
/// and stays exactly x3 at any base.
#[test]
fn roar_is_used_twice_on_a_status_tick_and_eclipse_once() {
    let bleed = |abilities: &[(&'static str, Option<f64>)], base: f64| {
        let mut p = params(abilities, 1.0);
        p.damage = DamageVector::new().with(DamageType::Slash, base);
        p.dot_modified_base = Some(base);
        p.status_chance = 1.0;
        p.base_status_chance = 1.0;
        p.foe.base_health = 1e15;
        run_once(&p, &mut crate::rules::rng::Rng::new(3)).tally.dot()
    };
    // A base large enough that the accumulator is below the tolerance.
    let big = 1e9;
    let plain = bleed(&[], big);
    assert!(plain > 0.0);
    let roar = bleed(&[("roar", None)], big) / plain;
    assert!((roar - 2.25).abs() < 1e-6, "roar on a DoT: x{roar:.4}");
    // …and at an ordinary base it is strictly BETWEEN one layer and two,
    // which is the accumulator's signature and the only shape that can be.
    let small = bleed(&[], 100.0);
    let roar_small = bleed(&[("roar", None)], 100.0) / small;
    assert!(
        (1.5..2.25).contains(&roar_small),
        "the accumulator takes one faction layer, not two or none: x{roar_small:.4}"
    );
    // Eclipse (+200%) is x3 on the hit and x3 on the tick — "Unlike faction
    // damage, which double dips for status effects, the one from Eclipse is
    // applied once". Nine would be the wrong answer. Exact at BOTH bases,
    // because a final multiplier scales the accumulator too.
    for base in [big, 100.0] {
        let ecl = bleed(&[("eclipse", None)], base) / bleed(&[], base);
        assert!((ecl - 3.0).abs() < 1e-6, "eclipse on a DoT at {base}: x{ecl:.4}");
    }
}

/// THE ADDED ELEMENT DOES NOT COMBINE. A weapon whose
/// vector is pure Heat, under Shock Trooper, deals Heat AND Electricity —
/// never Radiation, which is what an elemental MOD would have made of the
/// same two.
#[test]
fn an_ability_element_lands_beside_the_weapons_own_instead_of_combining() {
    let mut p = params(&[("shock_trooper", None)], 1.0);
    p.damage = DamageVector::new().with(DamageType::Heat, 100.0);
    p.dot_modified_base = Some(100.0);
    let r = run_once(&p, &mut crate::rules::rng::Rng::new(3));
    let by = &r.sources.direct_by_type;
    let at = |t: DamageType| by[t as usize];
    assert!(at(DamageType::Heat) > 0.0, "the weapon keeps its own element");
    assert!(at(DamageType::Electricity) > 0.0, "the ability adds its own");
    assert_eq!(at(DamageType::Radiation), 0.0, "and they DO NOT combine");
    // +100% of ModifiedBase, so the two halves are equal.
    let ratio = at(DamageType::Electricity) / at(DamageType::Heat);
    assert!((ratio - 1.0).abs() < 1e-6, "x{ratio:.4}");
}

/// A DURATION ENDS IT. Half a fight of Roar is worth less than all of it
/// and more than none — asserted as an ORDERING rather than a number,
/// because where the shots fall inside the window is the sim's business.
#[test]
fn a_duration_ends_the_buff_mid_fight() {
    let none = direct(&params(&[], 1.0));
    let half = direct(&params(&[("roar", Some(5.0))], 1.0));
    let all = direct(&params(&[("roar", None)], 1.0));
    assert!(none < half && half < all, "{none:.0} / {half:.0} / {all:.0}");
    // The whole-fight run is exactly the 1.5x of the test above, so the
    // partial one is a real fraction of it rather than a rounding.
    assert!((all / none - 1.5).abs() < 1e-9);
}

/// THE MEASURED FIGHT (MEASUREMENTS M40) — a Magnus at 98 base with two
/// 60/60s making Blast (+120%) and a Primed Bane of Grineer (+55%), which
/// is the capture the owner supplied on 2026-08-09. Every extra-hit test
/// below runs on it, so the numbers in them are the numbers on the video.
fn measured() -> FightParams {
    let mut p = params(&[("xatas_whisper", None)], 1.0);
    // 98 IPS + 98 x 1.2 as Blast, and ModifiedBase is the 98: an elemental
    // mod's damage is not part of the base a status burns off.
    p.damage = DamageVector::new()
        .with(DamageType::Impact, 98.0)
        .with(DamageType::Blast, 98.0 * 1.2);
    p.dot_modified_base = Some(98.0);
    p.faction_multiplier = 1.55;
    // NO STATUS unless a test asks for it. The fixture behind `params` is a
    // real weapon and procs; a stray Blast would fold a detonation's extra
    // hit into the ratio the first two tests are about, which is exactly
    // the confusion the last two exist to tell apart.
    p.status_chance = 0.0;
    p.base_status_chance = 0.0;
    p.forced_procs = Vec::new();
    // Nothing may die: these are per-instance numbers, and a respawn would
    // put a fresh bar under half of them.
    p.foe.base_health = 1e15;
    p
}

/// THE WIKI'S OWN WORKED EXAMPLE, to the digit — the strongest citation
/// this interaction has.
///
/// > A gun deals 100 damage per bullet, and we have Thermite Rounds, Rime
/// > Rounds, Stormbringer, Primed Bane of Grineer, and Xata's whisper at
/// > base strength: The initial hit will deal
/// > `100 × (1 + 0.6 + 0.6 + 0.9) × (1 + 0.55) = 480.5`, and Xata's whisper
/// > will deal `0.26 × 480.5 × (1 + 0.55) = 193.6415` (the Faction Damage
/// > Bonus is applied again). If the hit proc'd Blast, then the detonation
/// > damage will be `0.3 × 100 × (1 + 0.55)^2 = 72.075` (Elemental Damage
/// > doesn't apply to Blast detonations and the Faction Damage Bonus is
/// > applied again). Then, Xata's whisper will trigger off said detonation,
/// > dealing `0.26 × 72.075 × (1 + 0.55) × (1 + 0.6 + 0.6 + 0.9) = 90.0433`.
///
/// The four lines check each other: the elemental bracket is INSIDE the
/// hit and OUTSIDE the detonation's extra hit, and the faction bonus lands
/// once more at every step — `f¹` on the hit, `f²` on its extra hit and on
/// the detonation, `f³` on the extra hit off the detonation. Thermite and
/// Rime Rounds COMBINE, so the vector is 100 Impact + 120 Blast + 90
/// Electricity — which is why there is a Blast proc to detonate.
fn wiki_example() -> FightParams {
    let mut p = params(&[("xatas_whisper", None)], 1.0);
    p.damage = DamageVector::new()
        .with(DamageType::Impact, 100.0)
        .with(DamageType::Blast, 120.0)
        .with(DamageType::Electricity, 90.0);
    // THE BASE A STATUS BURNS OFF is the 100, not the 310: an elemental
    // mod's damage is not part of it. That is the whole reason the
    // detonation is 30 and not 93.
    p.dot_modified_base = Some(100.0);
    p.faction_multiplier = 1.55;
    p.status_chance = 0.0;
    p.base_status_chance = 0.0;
    p.forced_procs = Vec::new();
    p.foe.base_health = 1e15;
    p
}

#[test]
fn the_wiki_worked_example_reproduces_to_the_digit() {
    // ONE SHOT, so every figure below is one instance rather than a mean.
    let one = |p: &FightParams| {
        let mut q = p.clone();
        q.duration_seconds = 0.001;
        run_once(&q, &mut crate::rules::rng::Rng::new(3))
    };

    // QUANTISATION IS THE ONE DIFFERENCE, and it is ours being right rather
    // than the example being wrong: DE rounds each element of the vector
    // down to a step of the base, which an illustration written to show a
    // formula has no reason to carry. So the example's 310 is 300.3125 here
    // and every absolute figure below moves with it — the four RELATIONS,
    // which are what the example is demonstrating, are exact.
    let q = wiki_example().damage.quantized_against(100.0).total();
    assert!(q < 310.0 && q > 295.0, "quantised vector {q}");
    let f = 1.55;
    let mb = 100.0;
    // The bracket the extra hit off a detonation picks up: the vector over
    // the base a status burns off — the example's `1 + 0.6 + 0.6 + 0.9`.
    let bracket = q / mb;

    // 1 + 2. THE HIT AND ITS EXTRA HIT.
    let r = one(&wiki_example());
    assert!((r.sources.direct - q * f).abs() < 1e-6, "hit {}", r.sources.direct);
    assert!(
        (r.sources.extra_hit - 0.26 * (q * f) * f).abs() < 1e-4,
        "extra hit {}",
        r.sources.extra_hit
    );

    // 3 + 4. THE DETONATION AND THE EXTRA HIT OFF IT. Forced, because the
    // example says "if the hit proc'd Blast" — and long enough for the fuse.
    let mut p = wiki_example();
    p.forced_procs = vec![DamageType::Blast];
    p.duration_seconds = 30.0;
    let r = one_shot_with_fuse(&p);
    // The detonation is a status payload: 30% of the 100 ModifiedBase,
    // faction squared, and NO elemental bracket.
    let want_det = 0.3 * mb * f * f;
    assert!(
        (r.blast - want_det).abs() < 1e-4,
        "detonation {} wanted {want_det}",
        r.blast
    );
    // …and the extra hit off it takes faction a THIRD time and the whole
    // elemental bracket the detonation itself was denied.
    let want_xh_det = 0.26 * want_det * f * bracket;
    let off_det = r.extra_hit - 0.26 * (q * f) * f;
    assert!(
        (off_det - want_xh_det).abs() < 1e-3,
        "extra hit off the detonation {off_det}, wanted {want_xh_det}"
    );
    // …AND IT IS NEITHER OF THE TWO NUMBERS IT WOULD BE IF EITHER ODDITY
    // WERE MISSING. Both alternatives are what a careful reader would
    // expect — a detonation takes no elemental bonus, so why would the hit
    // off it; and two faction layers is what every other status gets — so
    // ruling them out is the whole of the claim.
    let without_bracket = 0.26 * want_det * f;
    let without_third_faction = 0.26 * want_det * bracket;
    assert!((off_det - without_bracket).abs() > 1.0, "the bracket is missing");
    assert!(
        (off_det - without_third_faction).abs() > 1.0,
        "the third faction layer is missing"
    );
}

/// One shot, then the clock run out so the Blast fuse expires — the
/// detonation and its extra hit are what this returns.
struct FusedRun {
    blast: f64,
    extra_hit: f64,
}
fn one_shot_with_fuse(p: &FightParams) -> FusedRun {
    let mut q = p.clone();
    // One pull, then nothing but time: `magazine_size` of 1 with a reload
    // longer than the run leaves the fuse alone to expire.
    q.magazine_size = 1.0;
    q.reload_seconds = 1e6;
    let r = run_once(&q, &mut crate::rules::rng::Rng::new(3));
    FusedRun {
        blast: r.sources.status[DamageType::Blast as usize],
        extra_hit: r.sources.extra_hit,
    }
}

/// FACTION, TWICE — the whole of the ordinary case. The extra hit is 26% of
/// a hit that already carried the bonus, and it carries it again:
/// `0.26 x 1.55 = 0.403` of the hit, against the 0.26 a reading of the card
/// would predict.
#[test]
fn the_extra_hit_takes_the_faction_bonus_a_second_time() {
    let r = run_once(&measured(), &mut crate::rules::rng::Rng::new(3));
    let ratio = r.sources.extra_hit / r.sources.direct;
    assert!(
        (ratio - 0.26 * 1.55).abs() < 1e-9,
        "extra/direct = {ratio:.6}, wanted {:.6}",
        0.26 * 1.55
    );
    // …and it is VOID, whatever the weapon deals: a separate instance, not
    // a share of the vector ("does not dilute weapon elements").
    let by = &r.sources.extra_hit_by_type;
    assert!(by[DamageType::Void as usize] > 0.0);
    assert_eq!(by[DamageType::Impact as usize], 0.0);
    assert_eq!(by[DamageType::Blast as usize], 0.0);
}

/// …AND THE BODY PART, TWICE, for the same reason and stated in the same
/// breath (DE's CN card: "同理，弱点倍率也会被计算两次"). On a 3x head the
/// hit is tripled once and the extra hit off it is tripled again, so the
/// RATIO between them moves — which is the only way to see a double dip
/// without trusting an absolute number.
#[test]
fn the_extra_hit_takes_the_body_part_multiplier_a_second_time() {
    let head = |mult: f64| {
        let mut p = measured();
        p.body_parts = vec![BodyPart {
            name: "head".into(),
            aim_weight: 1.0,
            multiplier: mult,
            is_head: true,
            is_weak_point: true,
            crit_bonus: false,
        }];
        let r = run_once(&p, &mut crate::rules::rng::Rng::new(3));
        r.sources.extra_hit / r.sources.direct
    };
    let body = head(1.0);
    let three_x = head(3.0);
    assert!((three_x / body - 3.0).abs() < 1e-9, "x{:.4}", three_x / body);
}

/// THE BLAST CHAIN, decoded number by number against the capture: one
/// shot, one forced Blast stack, and 1.5 s later the fuse fires.
///
/// | payload | formula | measured |
/// | --- | --- | --- |
/// | the hit | `98 x 2.2 x 1.55` | 334.18 (read as 323 through the target) |
/// | its extra hit | `x 0.26 x 1.55` | 135 |
/// | the detonation | `0.3 x 98 x 1.55^2` | 71 |
/// | ITS extra hit | `x 0.26 x 1.55 x 2.2` | 63 |
///
/// The last row is the one worth having a test for: the faction bonus lands
/// a THIRD time, and the elemental bracket lands on a payload that is
/// explicitly denied elemental bonuses.
///
/// THE `2.2` IS THE ILLUSTRATION'S, NOT THE GAME'S: the Blast half is
/// `98 x 1.2 = 117.6`, 38.4 steps of the `98/32` scale, which snaps to 38 —
/// so the vector is `214.375` and the bracket a status burns off is
/// `2.1875`. A capture read through mitigation cannot resolve 0.6%. The
/// four RELATIONS are exact; the absolute figures carry the quantization.
#[test]
fn an_extra_hit_fires_off_a_blast_detonation_at_the_third_faction_layer() {
    let mut p = measured();
    // Exactly one shot, and long enough after it for the 1.5 s fuse.
    p.fire_rate = 0.5;
    p.duration_seconds = 1.9;
    p.status_chance = 0.0;
    p.forced_procs = vec![DamageType::Blast];
    let r = run_once(&p, &mut crate::rules::rng::Rng::new(3));

    // 117.6 Blast is 38.4 steps of 98/32 and snaps to 38.
    let qtotal = 98.0 + 38.0 * (98.0 / crate::rules::damage::QUANTIZATION_DENOMINATOR);
    let bracket = qtotal / 98.0;
    let hit = qtotal * 1.55;
    // The detonation reads ModifiedBase — 98 — which quantization never
    // touches, so this half is unmoved.
    let deto = BLAST_COEFFICIENT * 98.0 * 1.55 * 1.55;
    assert!((r.sources.direct - hit).abs() < 1e-6, "hit {:.3}", r.sources.direct);
    assert!(
        (r.sources.status[DamageType::Blast as usize] - deto).abs() < 1e-6,
        "detonation {:.3} vs {deto:.3}",
        r.sources.status[DamageType::Blast as usize]
    );
    // The hit's own extra hit, plus the detonation's.
    let from_hit = hit * 0.26 * 1.55;
    let from_deto = deto * 0.26 * 1.55 * bracket;
    assert!(
        (r.sources.extra_hit - (from_hit + from_deto)).abs() < 1e-6,
        "extra {:.3} vs {:.3} + {:.3}",
        r.sources.extra_hit,
        from_hit,
        from_deto
    );
    // The extra hit off a Blast proc is worth about 89% of the proc, which
    // is only possible with both of the layers above.
    //
    // THE BAND IS THE CAPTURE'S OWN. It read 63 and 71, each a whole
    // number popped in game, so the ratio it pins is `[62.5/71.5,
    // 63.5/70.5]` — 0.874 to 0.901, and nothing tighter is in the video. A
    // tighter band claims a precision two integers cannot carry and turns a
    // 0.57% quantization correction into a red test.
    let ratio = from_deto / deto;
    assert!((0.874..=0.901).contains(&ratio), "{ratio:.4} outside the capture's 63/71");
}

/// NO OTHER STATUS PAYLOAD TRIGGERS ONE — the negative control, and the
/// reason the Blast case is filed as a bug rather than as a rule. A Slash
/// bleed ticks six times under the same buff and pays no extra hit at all.
#[test]
fn a_dot_tick_triggers_no_extra_hit() {
    let mut p = measured();
    p.damage = DamageVector::new().with(DamageType::Slash, 98.0);
    p.status_chance = 0.0;
    p.forced_procs = vec![DamageType::Slash];
    let r = run_once(&p, &mut crate::rules::rng::Rng::new(3));
    assert!(r.tally.dot() > 0.0, "the bleed has to be ticking for this to mean anything");
    // Only the hits paid one, so the ratio is the plain 0.26 x faction —
    // exactly as if the DoT were not there.
    let ratio = r.sources.extra_hit / r.sources.direct;
    assert!((ratio - 0.26 * 1.55).abs() < 1e-9, "{ratio:.6}");
}

/// THE VOID PROC IS WORTH A CONDITION OVERLOAD STACK AND NOTHING ELSE. It
/// deals no damage — a Bullet Attractor is a field, not a payload — so the
/// only way to see it at all is to put a CO weapon behind it and watch the
/// counter move.
#[test]
fn the_void_proc_pays_condition_overload_and_no_damage() {
    let co = |on: bool| {
        let mut p = measured();
        p.status_chance = if on { 4.0 } else { 0.0 };
        p.base_status_chance = p.status_chance;
        p.co_per_type = 0.8;
        p.co_behavior = crate::model::CoBehavior::Independent;
        // Pure Impact: its own proc is a Stagger, which pays no damage
        // either, so any movement in the hit is the CO counter and not a
        // second damage source.
        p.damage = DamageVector::new().with(DamageType::Impact, 98.0);
        let r = run_once(&p, &mut crate::rules::rng::Rng::new(7));
        (r.sources.direct, r.sources.status[DamageType::Void as usize])
    };
    let (quiet, _) = co(false);
    let (loud, void_damage) = co(true);
    assert!(loud > quiet, "CO never moved: {quiet:.0} -> {loud:.0}");
    assert_eq!(void_damage, 0.0, "a Bullet Attractor deals no damage");
}

/// AND A FIGHT WITH NO ABILITIES IS THE FIGHT WE ALWAYS HAD. The board
/// sends none of these, so this is the assertion that the feature costs a
/// board row nothing.
#[test]
fn no_ability_changes_no_number() {
    let bare = FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 100.0),
        ..params(&[], 1.0)
    };
    let mut with_empty = bare.clone();
    with_empty.abilities = resolve(&[], &Caster { strength: 3.0, ..Default::default() }, "", "melee");
    assert_eq!(direct(&bare), direct(&with_empty));
}

/// **WARCRY BUYS ATTACKS, IN THE SAME SUM A FIRE-RATE MOD IS IN.** The page
/// states the bracket and works the example: *"Attack Speed bonus is additive
/// to mods (e.g., Fury)"*, `Attack Speed Mods + Warcry Modifier x (1 + Strength
/// Mods)` = `0.3 + 0.5 x (1 + 0.3)`.
///
/// ASSERTED AS THE BRACKET, not as a shot count: what is claimed is that the
/// ability's share lands in the mods' own sum, so the test is that a build with
/// Warcry fires exactly as often as one whose MODS alone came to the same sum —
/// and not as often as the multiplicative reading of the same two numbers.
/// Counted as SHOTS because attack speed buys no damage at all.
#[test]
fn warcry_is_attack_speed_additive_with_the_mods_and_scaled_by_strength() {
    let shots = |strength: f64, mods: f64, warcry: bool| {
        let picks: &[(&'static str, Option<f64>)] = if warcry { &[("warcry", None)] } else { &[] };
        let mut p = params(picks, strength);
        p.fire_rate = 1.0 + mods;
        run_once(&p, &mut crate::rules::rng::Rng::new(3)).shots
    };
    let with = |strength: f64, mods: f64| shots(strength, mods, true);
    let mods_only = |mods: f64| shots(1.0, mods, false);

    // IT BUYS ATTACKS AT ALL, and at 100% strength it is the card's own 50%.
    assert!(with(1.0, 0.0) > mods_only(0.0));
    assert_eq!(with(1.0, 0.0), mods_only(0.5));
    // THE PAGE'S OWN EXAMPLE — 0.3 + 0.5 x (1 + 0.3) = 0.95.
    assert_eq!(with(1.3, 0.3), mods_only(0.95));
    // …AND NOT THE MULTIPLICATIVE READING of the same two, (1.3 x 1.65) - 1.
    assert_ne!(with(1.3, 0.3), mods_only(1.3 * 1.65 - 1.0));
    // STRENGTH SCALES WARCRY'S SHARE AND NOTHING ELSE: at 0% it buys nothing.
    assert_eq!(with(0.0, 0.3), mods_only(0.3));
}

/// **ETERNAL WAR GROWS WARCRY'S WINDOW AS MELEE KILLS LAND** — *"extends
/// Warcry's duration for each melee kill"*, +2s a kill, *"up to a maximum of
/// double the ability's duration after mods"* — and it pays only on a frame
/// carrying the card.
///
/// THE ONE THING IN THIS FIGHT THAT MOVES AN ABILITY'S WINDOW, so the assertion
/// is that the window OUTLIVES its own seconds: a 4 s Warcry still buying
/// attacks at 8 s is the augment, and nothing else here can do that.
#[test]
fn eternal_war_extends_warcry_while_melee_kills_land() {
    let shots = |augments: &[&str], secs: f64| {
        let picks = [AbilityPick { id: "warcry", duration_seconds: Some(secs), element: None }];
        let mut p = params(&[], 1.0);
        p.abilities = resolve(&picks, &Caster { strength: 1.0, augments, ..Default::default() }, "", "melee");
        // A TARGET THAT DIES TO EVERY SWING AND COMES BACK, so kills land at the
        // swing rate. The default fixture target is `InfiniteHealth` and a 1 HP
        // version of it still never dies — which is what a kill-gated mechanic
        // reads as broken.
        p.foe = super::frail_target(super::TargetMode::InstantRespawn, 0.0, 0.0);
        run_once(&p, &mut crate::rules::rng::Rng::new(3)).shots
    };
    // A SHORT WARCRY, with and without the card: the augment can only add.
    let plain = shots(&[], 4.0);
    let augmented = shots(&["eternal_war"], 4.0);
    assert!(augmented > plain, "{augmented} against {plain}");
    // …AND IT IS THE CARD DOING IT: a frame carrying some OTHER augment gets
    // exactly the unaugmented fight.
    assert_eq!(shots(&["eternal_war_but_not"], 4.0), plain);
    // THE CEILING IS THE ABILITY'S OWN: twice the window, so a 4 s Warcry can
    // reach 8 s and no further — which is the 8 s one's fight.
    assert_eq!(augmented, shots(&[], 8.0));
}

/// **CASTING IS PAID FOR IN TIME AND IN ENERGY, AND THE FIGHT SHOWS BOTH.**
///
/// The default reading is that a ticked ability is up and nobody paid — which
/// is what every board row was measured under. Casting it instead buys a window
/// out of a pool that does not refill, and a cast that roots the frame takes
/// the trigger finger with it.
#[test]
fn casting_costs_shots_and_the_pool_limits_the_window() {
    let plan = |energy: f64, interrupts: bool| {
        let picks = [AbilityPick { id: "warcry", duration_seconds: Some(20.0), element: None }];
        let mut p = params(&[], 1.0);
        p.abilities = resolve(&picks, &Caster::default(), "", "melee");
        p.abilities[0].interrupts_fire = interrupts;
        p.duration_seconds = 60.0;
        let cast = crate::data::abilities::plan_casts(&mut p.abilities, &["warcry"], energy, 60.0);
        p.cast_interrupts = cast.interrupts;
        (p.abilities[0].ends_at_seconds, run_once(&p, &mut crate::rules::rng::Rng::new(3)).shots)
    };
    // A POOL OF 150 AT 75 A CAST IS TWO CASTS: up for 40 s of a 60 s fight.
    let (ends, shots) = plan(150.0, true);
    assert_eq!(ends, 40.0);
    // …AND A BIGGER POOL KEEPS IT UP FOR THE WHOLE FIGHT, which is more attack
    // speed and more shots. Past the end rather than exactly at it: the last
    // recast opens a window the fight does not live to see the end of.
    let (long_ends, long_shots) = plan(1000.0, true);
    assert!(long_ends >= 60.0, "{long_ends}");
    assert!(long_shots > shots, "{long_shots} against {shots}");
    // THE ROOTING IS WHAT COSTS SHOTS: the same fight where the cast does not
    // interrupt fires more, and it is the only difference between the two.
    let (free_ends, free_shots) = plan(1000.0, false);
    assert_eq!(free_ends, long_ends);
    assert!(free_shots > long_shots, "{free_shots} against {long_shots}");
}
