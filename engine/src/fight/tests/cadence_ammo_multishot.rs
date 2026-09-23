use super::*;

/// A FLAT BASE-DAMAGE BUFF IS NOT DILUTED BY SERRATION, and that is the
/// only reason it is a bracket of its own.
///
/// Striking Succession grants "+15 Base Damage" a stack. A base add raises
/// the number the base-damage bucket multiplies, so its RELATIVE worth is
/// `(base + flat) / base` — the same figure whether or not a damage mod is
/// equipped. A bucket grant of the same nominal size would be worth less
/// with every mod added to that bucket.
///
/// So the test is not a golden number: it is that the perk's gain is the
/// SAME bare and modded. `resolve` converts the flat number into a bucket
/// share once the mods are known, and getting that conversion wrong shows
/// up here as a gain that shrinks.
#[test]
fn a_flat_base_damage_buff_keeps_its_worth_when_a_damage_mod_goes_in() {
    let arena = crate::arena::Arena::training(120.0);
    let dmg = |evo: &[&str], mods: &[&str]| {
        let base = crate::model::WeaponBase::from_data("paris_prime", true, evo);
        let pool = crate::data::mods::pool_for_weapon("paris_prime");
        let multishot: Vec<&crate::model::ModDef> = mods
            .iter()
            .map(|m| pool.iter().find(|d| d.id == *m).unwrap_or_else(|| panic!("no mod {m}")))
            .collect();
        let panel = crate::build::loadout::resolve(&base, &multishot, crate::model::StackPolicy::Emergent);
        let p = FightParams::from_panel(&panel, &arena, &ArcaneFx::none());
        let s = monte_carlo(&p, 8, 0x5017);
        s.mean_damage / s.mean_pellets.max(1e-9)
    };
    let perk = ["paris_prime_striking_succession"];

    let bare = dmg(&perk, &[]) / dmg(&[], &[]);
    let modded = dmg(&perk, &["serration"]) / dmg(&[], &["serration"]);
    assert!(bare > 1.02, "the perk is worth something at all: x{bare:.4}");
    assert!(
        (bare - modded).abs() / bare < 0.02,
        "a FLAT base add is worth the same with a damage mod in: x{bare:.4} bare,              x{modded:.4} with Serration — a bucket grant would have shrunk"
    );
}

/// BLAZING BARREL, and the two brackets one perk name lands in.
///
/// The Strun family's card reads "+0.05 BASE Multishot" and the Sybaris
/// family's "+5% Multishot". On a bare weapon those are the same 0.05 a
/// stack and the difference is invisible — which is exactly why `base:` is
/// required in the yaml rather than defaulted. With a multishot MOD in, the
/// base add is multiplied by the mod bucket and the percentage is not, and
/// this test is the one place that separation is pinned.
///
/// Hell's Chamber is +120%, so at five stacks the Strun's 0.25 becomes 0.55
/// while a percentage grant of the same size would stay flat. Asserted as a
/// RATIO between the two brackets rather than against a golden number, so
/// it survives any change to the weapon's own pellet count.
#[test]
fn blazing_barrel_lands_in_the_bracket_its_card_names() {
    let arena = crate::arena::Arena::training(60.0);
    // A magazine large enough to reach the cap and stay there: the stacks
    // are cleared by the reload, so a 2-round shotgun would spend the run
    // climbing.
    let pellets = |evo: &[&str], mods: &[&str]| {
        let base = crate::model::WeaponBase::from_data("strun_prime", true, evo);
        let pool = crate::data::mods::pool_for_weapon("strun_prime");
        let multishot: Vec<&crate::model::ModDef> = mods
            .iter()
            .map(|m| pool.iter().find(|d| d.id == *m).unwrap_or_else(|| panic!("no mod {m}")))
            .collect();
        let panel = crate::build::loadout::resolve(&base, &multishot, crate::model::StackPolicy::Emergent);
        let p = FightParams::from_panel(&panel, &arena, &ArcaneFx::none());
        let s = monte_carlo(&p, 12, 0xB1A2);
        s.mean_pellets / s.mean_shots
    };
    let perk = ["strun_prime_blazing_barrel"];

    // BARE: the grant is worth its face value, so the perk raises the
    // pellet count at all. Without this the ratio below is 1.0 for the
    // uninteresting reason that nothing happened.
    let (bare_off, bare_on) = (pellets(&[], &[]), pellets(&perk, &[]));
    assert!(
        bare_on > bare_off,
        "on firing, +0.05 base multishot a stack: {bare_off:.3} -> {bare_on:.3} pellets a shot"
    );

    // MODDED: the same perk is worth MORE, because a base add is multiplied
    // by the multishot bucket. A percentage grant would have gained exactly
    // the bare amount here, so the two brackets are told apart by whether
    // this ratio exceeds 1.
    let (mod_off, mod_on) = (pellets(&[], &["hells_chamber"]), pellets(&perk, &["hells_chamber"]));
    let bare_gain = bare_on - bare_off;
    let mod_gain = mod_on - mod_off;
    assert!(
        mod_gain > bare_gain * 1.5,
        "a BASE add is multiplied by the bucket: +{bare_gain:.3} pellets bare,              +{mod_gain:.3} with Hell's Chamber — a percentage grant would have given +{bare_gain:.3} in both"
    );
}

/// …AND THE RELOAD TAKES THEM, but swapping OUT of the Incarnon form does
/// not. VERBATIM (wiki, Strun Incarnon Genesis): "resets entirely upon
/// reloading. Entering Incarnon Form counts as reloading but exiting does
/// not."
///
/// That one exception is the whole reason `ClearedBy::Reload` exists beside
/// `MagazineRefilled`, which fires on both transforms. Asserted through the
/// SHAPE of the buff rather than by replaying a swap, because what the two
/// clearers disagree about is a single event and the disagreement is
/// declared, not emergent.
#[test]
fn blazing_barrel_is_cleared_by_a_reload_and_not_by_a_refill() {
    let base = crate::model::WeaponBase::from_data("strun_prime", true, &["strun_prime_blazing_barrel"]);
    let b = base
        .stacking_buffs
        .iter()
        .find(|b| b.id == "on_firing_multishot")
        .expect("the perk pushes its buff");
    assert_eq!(b.cleared_by, crate::model::ClearedBy::Reload);
    assert_eq!(b.trigger, crate::model::BuffTrigger::Firing);
    assert_eq!(b.grant, crate::model::BuffGrant::BaseMultishot);
    // No clock: the card states none and the reset is what ends it.
    assert!(b.duration.is_infinite(), "no timer — the reload is the end");
    assert_eq!(b.max_stacks, 5);

    // …and the Sybaris's same-named perk is the OTHER bracket.
    let syb = crate::model::WeaponBase::from_data("sybaris_prime", true, &["sybaris_prime_blazing_barrel"]);
    let s = syb
        .stacking_buffs
        .iter()
        .find(|b| b.id == "on_firing_multishot")
        .expect("the Sybaris carries it too");
    assert_eq!(s.grant, crate::model::BuffGrant::Multishot);
    assert_eq!(s.max_stacks, 10);
}

/// THE LATRON'S TWO PUNCTURE PERKS, both measured against the status they
/// read rather than against a build that happens to be good.
///
/// Riddled Target is "+25% Multishot for 8s per Puncture Status, 4x" and
/// Flensing Spikes is "Remove 20% of enemy Armor per Puncture Status". Both
/// were inert for the same reason and neither for a good one: the stacking
/// buff existed but its loader arm named Electricity, and armour stripping
/// existed but only for the two statuses that strip on their own.
///
/// A TARGET WITH ARMOUR is the fixture, because that is the only place the
/// second perk can be seen at all — and the two are asserted separately, so
/// one carrying the other cannot pass for both.
#[test]
fn the_latrons_puncture_perks_read_the_status_they_name() {
    let arena = crate::arena::Arena::training(30.0);
    let run = |evo: &[&str]| {
        let base = crate::model::WeaponBase::from_data("latron_prime", true, evo);
        let panel = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
        let mut p = FightParams::from_panel(&panel, &arena, &ArcaneFx::none());
        // ARMOUR, and a target that survives long enough to carry five
        // Puncture stacks. The training dummy has none of either.
        p.target.base_armor = 500.0;
        p.target.base_health = 2_000_000.0;
        monte_carlo(&p, 20, 0x1A7)
    };
    let off = run(&[]);
    let riddled = run(&["latron_prime_riddled_target"]);
    let flensing = run(&["latron_prime_flensing_spikes"]);

    // RIDDLED TARGET pays in PELLETS: +25% multishot a stack, four stacks,
    // on a weapon whose own damage is mostly Puncture — so the pellet count
    // per shot has to rise, and that is a thing no damage bonus can fake.
    let per_shot = |s: &Summary| s.mean_pellets / s.mean_shots;
    assert!(
        per_shot(&riddled) > per_shot(&off) * 1.2,
        "four stacks of +25% multishot: {:.3} -> {:.3} pellets a shot",
        per_shot(&off), per_shot(&riddled)
    );

    // FLENSING SPIKES pays in MITIGATION: the same pellets, landing harder,
    // because the armour in front of the health is gone. Asserted on damage
    // per pellet so the multishot perk above cannot be mistaken for it.
    let per_pellet = |s: &Summary| s.mean_effective_damage / s.mean_pellets;
    assert!(
        per_pellet(&flensing) > per_pellet(&off) * 1.1,
        "20% of the armour a Puncture stack: {:.1} -> {:.1} a pellet",
        per_pellet(&off), per_pellet(&flensing)
    );
    // …and it is the ARMOUR it removed, not damage it added: against a
    // target with none, the perk is worth nothing at all.
    let bare = |evo: &[&str]| {
        let base = crate::model::WeaponBase::from_data("latron_prime", true, evo);
        let panel = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
        let mut p = FightParams::from_panel(&panel, &arena, &ArcaneFx::none());
        p.target.base_armor = 0.0;
        p.target.base_health = 2_000_000.0;
        monte_carlo(&p, 20, 0x1A7).mean_effective_damage
    };
    let (a, b) = (bare(&[]), bare(&["latron_prime_flensing_spikes"]));
    assert!(
        (a - b).abs() / a < 0.01,
        "unarmoured, an armour strip is worth nothing: {a:.0} vs {b:.0}"
    );
}

/// A BOUNCE REACHES BODIES THE SHOT NEVER AIMED AT, and it takes the
/// attack's EXPLOSION with it.
///
/// The claim is the WHOLE CHAIN — that a crowd takes more than one body
/// would, and that a formation of ONE bounces nowhere, this arena having no
/// terrain to come off. The fixture states the bounce itself
/// (`a_bouncing_projectile_in_a_crowd`).
#[test]
fn a_bounce_needs_a_crowd_and_one_body_bounces_nowhere() {
    let crowd = |n: usize, bounces: bool| {
        let mut p = a_bouncing_projectile_in_a_crowd(n, 0.5);
        if !bounces {
            p.ricochet = None;
        }
        let out = run_once(&p, &mut Rng::new(0x5EED));
        (out.taken.touched(), out.effective_damage())
    };
    let (one, dmg_one) = crowd(0, true);
    assert_eq!(one, 1, "a formation of one bounces nowhere — no terrain here");
    // …AND A CROWD IS WHERE IT LIVES. A BODY COUNT CANNOT TELL THE TWO
    // APART: the 4 m explosion reaches bodies the bounce never touched, so
    // the one thing that isolates the bounce is turning it off in the same
    // formation. The bounce COUNT is a ceiling, asserted where it is decided
    // (`rules::space::tests`).
    let (many, dmg_many) = crowd(24, true);
    let flat = crowd(24, false).1;
    assert!(many > 1, "a crowd is reached: {many} bodies");
    assert!(dmg_many > flat * 1.05,
        "the bounces are most of a crowd fight: {flat:.0} -> {dmg_many:.0}");
    assert!(dmg_many > dmg_one * 2.0,
        "a crowd takes far more than one body: {dmg_one:.0} -> {dmg_many:.0}");
}

/// …AND IT MAY LAND ON A HEAD, which no other spread in this engine can.
///
/// The assertion is on the DAMAGE rather than on a counter, because a chance
/// that is stored and not applied looks exactly like one that works — a
/// humanoid head is 3x, so a coin-flip head is worth a large, visible
/// fraction of every bounce.
///
/// TWO CONTROLS, because one alone proves nothing: at 0.0 the bounces are
/// body hits and at 1.0 they are all heads, and the measured run must sit
/// between them. The AIMED body is held at a pure body shot in all three
/// (`headshot_pct` 0), so the only thing moving is the bounce.
#[test]
fn half_of_a_weapons_bounces_find_a_head() {
    // MANY RUNS: the roll is a coin flip, so one engagement says nothing
    // about the rate.
    let at = |chance: f64| {
        monte_carlo(&a_bouncing_projectile_in_a_crowd(5, chance), 60, 0x8EAD)
            .mean_effective_damage
    };
    let (body, half, head) = (at(0.0), at(0.5), at(1.0));
    // A SMALL SHARE, and deliberately so. A bounce needs a CROWD to have
    // anywhere to go and the explosion reaches 4 m, so in any fixture where
    // bounces happen the blast — which never headshots — is most of the
    // damage. What the chance moves is the collision beside it, and the
    // assertion is scaled to that rather than to the whole number: the
    // fault it exists to catch is a chance stored and never applied, which
    // would make all three equal.
    assert!(head > body * 1.01, "a head is worth 3x: {body:.0} -> {head:.0}");
    assert!(half > body && half < head,
        "half the bounces: {body:.0} < {half:.0} < {head:.0}");
    // …and it really is about HALF — within a quarter of the gap, which is
    // the only scale this means anything on.
    let want = 0.5 * (body + head);
    assert!((half - want).abs() < 0.25 * (head - body),
        "half should sit near the midpoint {want:.0}, got {half:.0}");
}

/// SWIFT PUNISHMENT ASKS ABOUT THE PLAYER, and the neutral one cannot
/// answer yes.
///
/// "With Sprint Speed 1.2 or Higher: +30% Direct Damage per Status Type" —
/// a question about who is carrying the gun rather than about the gun, so
/// it is answered where the Tenno exists. The default wielder sprints at
/// 0.9, the slowest a frame has, so the perk's second
/// half pays NOTHING until a frame is named.
///
/// THE CARD SAYS 1.2 AND THE EFFECT WANTS 1.1. VERBATIM, from the Notes
/// cell of this row: *"Despite the description, the effect only requires
/// 1.1 or higher sprint speed."* 1.15 is the case that tells the two
/// readings apart, and it is the whole of the fix.
///
/// The flat +6 it also grants is untouched at every speed, which is what
/// says the gate is on the right half.
#[test]
fn swift_punishment_wants_1_1_however_the_card_reads() {
    let base = crate::model::WeaponBase::from_data(
        "latron_prime", true, &["latron_prime_swift_punishment"],
    );
    let bare = crate::model::WeaponBase::from_data("latron_prime", true, &[]);
    let at = |sprint: f64| {
        let mut t = crate::data::tenno::default_tenno().clone();
        t.sprint = sprint;
        crate::build::loadout::resolve_for(&base, &[], crate::model::StackPolicy::Emergent, &t)
    };
    assert_eq!(at(0.9).co_per_type, 0.0, "0.9 reaches neither reading");
    assert_eq!(at(1.05).co_per_type, 0.0, "just under the real threshold");
    // THE SHARP ONE: between the printed 1.2 and the measured 1.1.
    assert!((at(1.15).co_per_type - 0.30).abs() < 1e-9, "{}", at(1.15).co_per_type);
    assert!((at(1.1).co_per_type - 0.30).abs() < 1e-9, "the threshold itself");
    assert!((at(1.2).co_per_type - 0.30).abs() < 1e-9, "and above it");
    // …and the flat half is the perk's either way.
    let plain = crate::build::loadout::resolve(&bare, &[], crate::model::StackPolicy::Emergent);
    assert!(at(0.9).modified_base > plain.modified_base, "the +6 pays regardless");
    assert!((at(0.9).modified_base - at(1.2).modified_base).abs() < 1e-9);
}

/// …AND AMALGAM SERRATION IS ENOUGH ON ITS OWN.
///
/// VERBATIM, from the same Notes cell and from Deadly Pace's: *"Equipping
/// Amalgam Serration will allow any Warframe to reach the threshold without
/// needing to mod for sprint speed on the Warframe itself."*
///
/// Its "+25% Sprint Speed" is a WARFRAME stat the weapon carries — the
/// reason the mod is barred from companion weapons — so the gate has to
/// read the BUILD as well as the frame. It did not, and the two wiki notes
/// pin each other: the slowest frame reaches 0.9 x 1.25 = 1.125, which
/// clears 1.1 and does not clear the 1.2 the card prints. "Any Warframe" is
/// only true at 1.1, so each note is evidence for the other.
///
/// PLAIN SERRATION IS THE CONTROL: same family, same damage bucket, no
/// sprint clause — so a test that only equipped the Amalgam could not tell
/// "the sprint bonus opened the gate" from "any mod did".
#[test]
fn amalgam_serration_opens_the_sprint_gate_for_the_slowest_frame() {
    let base = crate::model::WeaponBase::from_data(
        "latron_prime", true, &["latron_prime_swift_punishment"],
    );
    let pool = crate::data::mods::class_pool("rifle");
    let pick = |id: &str| pool.iter().find(|m| m.id == id)
        .unwrap_or_else(|| panic!("{id} in the rifle pool"));
    let amalgam = pick("amalgam_serration");
    let plain = pick("serration");
    // THE SPRINT CLAUSE IS ON THE CARD, so the test rests on the data
    // rather than on the mod's name.
    assert!(
        amalgam.effects.iter().any(|e| matches!(
            e, crate::model::ModEffect::Indirect(crate::model::IndirectStat::SprintSpeed, _))),
        "{:?}", amalgam.effects);
    let with = |m: &crate::model::ModDef| {
        let t = crate::data::tenno::default_tenno().clone();   // sprint 0.9
        assert_eq!(t.sprint, 0.9, "the neutral wielder is the slowest frame");
        crate::build::loadout::resolve_for(&base, &[m], crate::model::StackPolicy::Emergent, &t)
    };
    assert!((with(amalgam).co_per_type - 0.30).abs() < 1e-9,
        "0.9 x 1.25 = 1.125 clears 1.1: {}", with(amalgam).co_per_type);
    assert_eq!(with(plain).co_per_type, 0.0,
        "plain Serration carries no sprint clause and opens nothing");
}

/// THE EMPTY MAGAZINE ARMS IT, and the TRANSFORM is what proves that.
///
/// Owner
///
/// A reload alone cannot tell the two readings apart: armed at the empty
/// magazine and armed when the reload starts both make that reload faster.
/// THE TRANSMUTE CAN — it happens between the two, so it is faster under
/// the first reading and untouched under the second.
///
/// A synthetic cycle, because the real one that has this perk cannot show
/// it: the Phenmor's base form transmutes on a full gauge and never empties,
/// so nothing ever arms it there (which is its own measured fact — the perk
/// is worth exactly zero in that cycle). Here the base form has a 2-round
/// magazine and charges on direct hits, so it empties, transforms, and
/// comes back, over and over.
#[test]
fn an_empty_magazine_arms_ready_retaliation_before_any_reload() {
    let mk = |rs: f64| {
        // THE PERK IS THE BASE FORM'S, and only the base form's: the
        // evolution loader drops it on a charge-backed form. The outer
        // params below therefore carry ZERO on purpose — setting it in both
        // places hid a real bug for one run, where the animations were
        // reading the Incarnon half's copy and finding nothing there.
        let base_form = FightParams {
            damage: DamageVector::new().with(DamageType::Impact, 50.0),
            crit_multiplier: 1.0,
            magazine_size: 2.0,
            reload_seconds: 2.0,
            rs_on_reload: rs,
            body_parts: mono_body(1.0),
            ..no_status()
        };
        FightParams {
            damage: DamageVector::new().with(DamageType::Impact, 100.0),
            crit_multiplier: 1.0,
            magazine_size: 2.0,
            reload_seconds: 2.0,
            ammo_efficiency_applies: false,
            body_parts: mono_body(1.0),
            duration_seconds: 60.0,
            cycle: Some(IncarnonCycle {
                starts_primed: false,
                base_form: Box::new(base_form),
                arms: Arms::Gauge { charge_on: crate::model::ChargeOn::DirectHits, charges_to_fill: 2 },
                ends: Ends::ChargeMagazine,
                // LONG animations, so the difference between running them at
                // one speed and at double shows up in whole transforms
                // rather than in rounding.
                transmute_out_seconds: 2.0,
                transmute_seconds: 2.0,
                reload_bucket: 0.0,
            }),
            ..no_status()
        }
    };
    let off = monte_carlo(&mk(0.0), 20, 0x5EED);
    let on = monte_carlo(&mk(1.0), 20, 0x5EED);
    assert!(
        on.mean_transforms > off.mean_transforms,
        "the transform is faster with the buff already up: {} -> {} transforms",
        off.mean_transforms, on.mean_transforms
    );
    // AND THIS FIXTURE NEVER RELOADS AT ALL, which is the scenario stated
    // rather than a flaw in it: the gauge fills on the shot that empties the
    // magazine, so the weapon transforms instead of reloading, every time.
    // The reload half of the perk has its own test above.
    assert_eq!(off.mean_reloads, 0.0, "the fixture transforms rather than reloading");
}

/// A RELOAD IS PAID FOR WHILE IT RUNS, and the bonus composes with the
/// static bucket rather than replacing it.
///
/// There is no lapsing-window case: Ready Retaliation lasts exactly as long
/// as the reload it starts, so nothing runs out halfway through. What is
/// left is the arithmetic that is the point — mods worth +100%
/// already halve a 4 s reload to the 2 s that arrives here, and the perk's
/// +100% on top makes it 4/(1+1+1) of the unmodded four rather than 1 s.
#[test]
fn a_reload_bonus_composes_with_the_bucket_it_joins() {
    let plain = reload_span(4.0, 0.0, 0.0);
    assert!((plain - 4.0).abs() < 1e-9, "no bonus at all: {plain}");

    let doubled = reload_span(4.0, 0.0, 1.0);
    assert!((doubled - 2.0).abs() < 1e-9, "+100% on an unmodded reload: {doubled}");

    let stacked = reload_span(2.0, 1.0, 1.0);
    assert!((stacked - 4.0 / 3.0).abs() < 1e-9, "on top of +100% of mods: {stacked}");
}

/// A BATTERY REFILLS BETWEEN SHOTS, and slowing the weapon enough removes
/// its reload entirely.
///
/// The Shedu's numbers (wiki, verbatim in `data::weapons::Battery`): a
/// 7-round battery, 28 rounds a second, a 0.4 s delay with rounds left.
/// Only the part of the gap BEYOND the delay pays, so the weapon breaks
/// even at `0.4 + 1/28 = 0.4357 s` a shot — **2.295 rounds a second**,
/// 8.2% under its listed 2.50.
///
/// That margin is the whole point and it is why this is not a
/// differently-spelled reload: the listed rate is 0.036 s above break-even,
/// so a single fire-rate penalty crosses it and the reload disappears.
/// A weapon getting strictly better from a NEGATIVE mod is a claim that has
/// to be asserted rather than described.
#[test]
fn a_battery_refills_between_shots_and_a_slow_enough_one_never_reloads() {
    let shedu = |fire_rate: f64| FightParams {
        fire_rate,
        magazine_size: 7.0,
        reload_seconds: 1.25, // = 1.0 s delay + 7/28 s refill
        duration_seconds: 60.0,
        body_parts: mono_body(1.0),
        battery: Some(crate::model::Battery {
            regen_per_second: 28.0,
            delay_empty_seconds: 1.0,
            delay_partial_seconds: 0.4,
        }),
        ..no_status()
    };
    // AT the listed rate the gap IS the delay: nothing regenerates, and the
    // battery runs dry every seven rounds exactly as a magazine would.
    let listed = run_once(&shedu(2.5), &mut Rng::new(4));
    assert!(listed.reloads > 0, "at the listed rate it must still reload");

    // ABOVE break-even it still drains, just slowly — at 2.35/s the gap
    // returns 0.715 rounds against the 1.0 it spends, so the battery lasts
    // 24.6 shots instead of 7. The mechanic is a SLOPE, not a switch, and a
    // test that only tried the two extremes would not have said so.
    //
    // (2.30/s drains too, at 0.026 rounds a shot — 268 of them, which is
    // 116 s and does not fit this fixture's minute. Worth recording: the
    // approach to break-even is asymptotic, so "does it reload" stops being
    // a question about the weapon and becomes one about the clock.)
    assert!(run_once(&shedu(2.35), &mut Rng::new(4)).reloads > 0);

    // JUST BELOW IT (2.25/s) the reload is gone for good.
    let slow = run_once(&shedu(2.25), &mut Rng::new(4));
    assert_eq!(slow.reloads, 0, "a battery under break-even must never empty");

    // …and it is worth REAL rounds: 10% slower and no downtime at all beats
    // the listed rate over a minute.
    assert!(
        slow.shots > listed.shots,
        "slowed {} shots against {} at the listed rate",
        slow.shots, listed.shots
    );

    // THE MECHANIC IS THE DIFFERENCE, not the numbers: the same weapon with
    // no battery reloads at either rate.
    let plain = FightParams { battery: None, ..shedu(2.25) };
    assert!(run_once(&plain, &mut Rng::new(4)).reloads > 0);
}

/// A SPOOL THAT CLIMBS is the same arithmetic pointed the other way, and it
/// costs a magazine's worth of time rather than a magazine's worth of rounds.
///
/// The Gorgon's numbers — 12.5 rounds/s from 20%, full on the 9th shot
/// (wiki). Its 90-round magazine takes 7.99 s instead of 7.20 s, i.e. the
/// spool costs **11% of the time** to fire one magazine, and it is paid once
/// per magazine rather than once per fight because a reload is a pause.
///
/// This runs beside the faller deliberately: one `spool_factor` serves both,
/// and the day it stops serving both, one of these two fails.
#[test]
fn a_spool_that_climbs_costs_the_first_shots_of_every_magazine() {
    let gorgon = FightParams {
        fire_rate: 12.5,
        magazine_size: 100_000.0, // one long burst: the climb, uninterrupted
        duration_seconds: 10.0,
        body_parts: mono_body(1.0),
        ..no_status()
    };
    assert_eq!(run_once(&gorgon, &mut Rng::new(1)).shots, 125, "the listed rate, flat");

    let spooled = FightParams {
        sustained_fire_rate: Some(crate::model::SustainedFireRate {
            start: 0.20,
            end: 1.00,
            over_shots: 7.5,
        }),
        ..gorgon.clone()
    };
    assert_eq!(run_once(&spooled, &mut Rng::new(1)).shots, 116);

    // ONCE PER MAGAZINE, NOT ONCE PER FIGHT. Nine shots of climb out of 90
    // is 11% of the time; out of a magazine of 9 it would be most of it. The
    // same derived reset the faller uses does both, with no rule of its own.
    let small = FightParams { magazine_size: 9.0, reload_seconds: 0.0001, ..spooled.clone() };
    let small_flat = FightParams { sustained_fire_rate: None, ..small.clone() };
    let a = f64::from(run_once(&small, &mut Rng::new(1)).shots);
    let b = f64::from(run_once(&small_flat, &mut Rng::new(1)).shots);
    assert!(a < b * 0.75, "a 9-round magazine is all climb: {a} vs {b}");
}

/// The floor is a fraction of the LIVE rate, so a fire-rate mod raises both
/// ends and never buys its way out of the spool. Rapid Wrath's +20% is
/// worth +20% at the floor as well as at the ceiling — which is also why
/// the spool cannot be folded into the listed stat.
#[test]
fn a_fire_rate_bonus_scales_the_spooled_rate_too() {
    let spooled = FightParams {
        fire_rate: 13.33,
        sustained_fire_rate: Some(crate::model::SustainedFireRate {
            start: 1.00,
            end: 0.60,
            over_shots: 51.0,
        }),
        magazine_size: 100_000.0,
        duration_seconds: 60.0, // long past the 51 shots, so the floor dominates
        body_parts: mono_body(1.0),
        ..no_status()
    };
    let faster = FightParams { fire_rate: 13.33 * 1.2, ..spooled.clone() };
    let slow = f64::from(run_once(&spooled, &mut Rng::new(1)).shots);
    let fast = f64::from(run_once(&faster, &mut Rng::new(1)).shots);
    assert!((fast / slow - 1.2).abs() < 0.01, "{fast} / {slow}");
}

/// A BOW paces on its draw alone, not on `1 / fire_rate`. Cernos Prime's
/// numbers: 0.5 s draw + 0.65 s reload of its single nocked arrow = 1.15 s
/// a shot, against the 1.65 s the fire-rate stat alone would give. The
/// stat itself is untouched — it is what fire-rate GATES read.
///
/// `DrawOnly` is stated because it is the exception: every OTHER charge
/// weapon adds the listed rate's interval to the draw (see
/// `a_general_charge_weapon_pays_the_draw_AND_the_rate`).
#[test]
fn a_charge_weapon_paces_on_the_draw_not_the_fire_rate() {
    let bow = FightParams {
        fire_rate: 1.0,
        charge_seconds: Some(0.5),
        charge_cadence: crate::model::ChargeCadence::DrawOnly,
        magazine_size: 1.0,
        reload_seconds: 0.65,
        duration_seconds: 10.0,
        body_parts: mono_body(1.0),
        ..no_status()
    };
    // Shots at 0, 1.15, 2.30 … 9.20 — nine of them inside 10 s.
    let r = run_once(&bow, &mut Rng::new(1));
    assert_eq!(r.shots, 9, "10 s / 1.15 s + 1");

    // The same weapon read as an ordinary 1.0 fire-rate gun: 1.65 s a
    // shot, seven shots. This is what the roster did before charge times.
    let as_rate = FightParams { charge_seconds: None, ..bow.clone() };
    assert_eq!(run_once(&as_rate, &mut Rng::new(1)).shots, 7);

    // A TAPPED bow: no draw to pay, so the 0.65 s nock is the whole cycle
    // (wiki Fire Rate's bow formula with a zero charge term). Shots at 0,
    // 0.65, 1.30 … 9.75 — sixteen inside 10 s, against the charged form's
    // nine, for half the damage each.
    let tapped = FightParams { charge_seconds: Some(0.0), ..bow.clone() };
    assert_eq!(run_once(&tapped, &mut Rng::new(1)).shots, 16, "10 s / 0.65 s + 1");

    // `fire_rate` here is the RESOLVED stat and `charge_seconds` the
    // RESOLVED draw — the panel already spent the mod bucket on both, so
    // raising the stat alone must NOT shorten the draw a second time.
    // Only a live in-sim buff does, and it divides by its own factor.
    let stat_only = FightParams { fire_rate: 2.0, ..bow.clone() };
    assert_eq!(run_once(&stat_only, &mut Rng::new(1)).shots, 9, "mods are not re-applied");
}

#[test]
fn faction_mult_scales_direct_damage_linearly() {
    // Status off: only the direct-hit multiply applies (no DoT double-dip),
    // and faction_multiplier is applied AFTER the RNG rolls, so the same seed
    // yields damage scaled exactly by faction_multiplier.
    let plain = single_part(BodyPart {
        name: "body".into(),
        aim_weight: 1.0,
        multiplier: 1.0,
        is_head: false,
        crit_bonus: false,
    });
    let boosted = FightParams {
        faction_multiplier: 1.30,
        ..plain.clone()
    };
    let a = monte_carlo(&plain, 3000, 7);
    let b = monte_carlo(&boosted, 3000, 7);
    assert!(
        (b.mean_damage / a.mean_damage - 1.30).abs() < 1e-9,
        "ratio was {}",
        b.mean_damage / a.mean_damage
    );
}

#[test]
fn faction_bonus_applies_only_vs_matching_target_faction() {
    use crate::build::loadout::resolve;
    use crate::model::WeaponBase;
    use crate::model::{Faction, ModDef, ModEffect, Rarity, StackPolicy};
    use crate::rules::capacity::Polarity;
    let expel = ModDef {
        stance: None,
        exclusive_to: &[],
        unmodeled: false,
        out_of_scope: false,
        id: "expel_grineer",
        name: "expel_grineer",
        base_drain: 9,
        max_rank: 5,
        polarity: Polarity::Madurai,
        rarity: Rarity::Uncommon,
        exilus: false,
        family: None,
        requires_weapon: None,
        excludes_weapon: Vec::new(),
        set: None,
        requires: None,
        disables: Vec::new(),
        effects: vec![ModEffect::FactionDamage(Faction::Grineer, 0.30)],
    };
    let base = WeaponBase::from_data(
        "dual_toxocyst",
        true,
        &[
            "dual_toxocyst_commodores_fortune",
            "dual_toxocyst_evolved_autoloader",
            "dual_toxocyst_fevered_frenzy",
        ],
    );
    let panel = resolve(&base, &[&expel], StackPolicy::AssumedMax);
    let parts = mono_body(1.0);
    let grineer_target = {
        let mut t = TargetParams::training_dummy();
        t.faction = Faction::Grineer;
        t
    };
    let arena = |target, body_parts| crate::arena::Arena {
        target,
        body_parts,
        ..crate::arena::Arena::training(10.0)
    };
    let vs_grineer = FightParams::from_panel(&panel, &arena(grineer_target, parts.clone()), &ArcaneFx::none());
    let vs_other = FightParams::from_panel(&panel, &arena(TargetParams::training_dummy(), parts), &ArcaneFx::none());
    assert!(
        (vs_grineer.faction_multiplier - 1.30).abs() < 1e-9,
        "grineer {}",
        vs_grineer.faction_multiplier
    );
    assert!(
        (vs_other.faction_multiplier - 1.0).abs() < 1e-9,
        "unknown {}",
        vs_other.faction_multiplier
    );
}

#[test]
fn ten_shots_in_ten_seconds_at_one_per_second() {
    let s = monte_carlo(&FightParams::default(), 100, 1);
    assert!((s.mean_shots - 10.0).abs() < 1e-9);
}

/// The set promotes a hit that ALREADY crit, and only that one — the wiki
/// is explicit that it "triggers exclusively on critical hits", so a
/// normal hit can never be turned into one.
#[test]
fn the_set_bonus_promotes_only_a_hit_that_already_crit() {
    let mut rng = Rng::new(7);
    assert_eq!(upgrade_crit_tier(0, 1.0, &mut rng), 0, "a normal hit stays normal");
    assert_eq!(upgrade_crit_tier(1, 1.0, &mut rng), 2, "yellow -> orange");
    assert_eq!(upgrade_crit_tier(2, 1.0, &mut rng), 3, "orange -> red");
    assert_eq!(upgrade_crit_tier(1, 0.0, &mut rng), 1, "no set, no promotion");
    // At 20% (all four primary members) roughly a fifth of crits move up.
    let n = 20_000;
    let up = (0..n).filter(|_| upgrade_crit_tier(1, 0.20, &mut rng) == 2).count();
    let f = up as f64 / n as f64;
    assert!((f - 0.20).abs() < 0.02, "promoted {f:.3} of crits, expected ~0.20");
}

#[test]
fn monte_carlo_is_deterministic() {
    let a = monte_carlo(&FightParams::default(), 500, 12345);
    let b = monte_carlo(&FightParams::default(), 500, 12345);
    assert_eq!(a.mean_damage, b.mean_damage);
    assert_eq!(a.std_damage, b.std_damage);
}

#[test]
fn headshot_rate_is_about_half() {
    let s = monte_carlo(&FightParams::default(), 1000, 999);
    assert!((s.mean_headshot_rate - 0.5).abs() < 0.02);
}

#[test]
fn produces_positive_damage() {
    let s = monte_carlo(&FightParams::default(), 1000, 7);
    assert!(s.mean_damage > 0.0);
    assert!(s.dps > 0.0);
}

#[test]
fn mean_damage_matches_hand_computed_expectation_without_status() {
    // Status off, 10 shots: Enervate ramps cc = 5%,15%,...,95% (sum 5.0).
    // Per shot: E = 0.5*75*(1+cc) + 0.5*(75*3)*(1+3cc) = 150 + 375*cc,
    // so E[total] = 10*150 + 375*5.0 = 3375. (The Dual Toxocyst vector
    // quantizes to a total of exactly 75.)
    let s = monte_carlo(&no_status(), 2000, 42);
    assert!(
        (s.mean_damage - 3375.0).abs() / 3375.0 < 0.02,
        "mean damage was {}",
        s.mean_damage
    );
    assert_eq!(s.mean_dot_damage, 0.0);
    assert_eq!(s.mean_procs, 0.0);
}

#[test]
fn multishot_doubles_pellets_and_damage_deterministically() {
    // Multishot 2.0: every pull fires exactly 2 pellets, each its own
    // instance. crit 1.0, mono body, no status: 10 pulls x 2 x 75 = 1500,
    // 20 pellets, ammo still 10 (one per pull -> no reload at mag 12).
    let p = FightParams {
        crit_multiplier: 1.0,
        multishot: 2.0,
        body_parts: mono_body(1.0),
        ..no_status()
    };
    let s = monte_carlo(&p, 50, 6);
    assert!(
        (s.mean_damage - 1500.0).abs() < 1e-9,
        "dmg {}",
        s.mean_damage
    );
    assert!((s.mean_pellets - 20.0).abs() < 1e-9);
    assert!((s.mean_shots - 10.0).abs() < 1e-9);
    assert_eq!(s.mean_reloads, 0.0);
}

/// Final Fusillade: +3 multishot on the LAST round of the magazine, and on
/// no other. A 5-round magazine fired dry gives four 1-pellet pulls and one
/// 4-pellet pull — 8 pellets, not 5 (gate never fires) and not 20 (gate
/// always fires).
#[test]
fn final_fusillade_adds_multishot_only_on_the_magazines_last_round() {
    let p = |bonus: f64| FightParams {
        multishot: 1.0,
        multishot_on_last_round: bonus,
        fire_rate: 1.0,
        magazine_size: 5.0,
        // Exactly one magazine: dry reserves stop the run rather than
        // reloading into a second one, so the counts are exact.
        infinite_reserve: false,
        reserve_ammo: 0.0,
        duration_seconds: 10.0,
        body_parts: mono_body(1.0),
        ..no_status()
    };
    let off = monte_carlo(&p(0.0), 4, 3);
    let on = monte_carlo(&p(3.0), 4, 3);
    assert!((off.mean_shots - 5.0).abs() < 1e-9, "shots {}", off.mean_shots);
    assert!((on.mean_shots - off.mean_shots).abs() < 1e-9, "cadence must not change");
    assert!((off.mean_pellets - 5.0).abs() < 1e-9, "baseline {}", off.mean_pellets);
    assert!((on.mean_pellets - 8.0).abs() < 1e-9, "boosted {}", on.mean_pellets);
    assert_eq!(off.mean_reloads, 0.0);
}

/// THE CHAMBER FAMILY pays the magazine's FIRST round and nothing else.
///
/// Five rounds, one magazine, +100% on the first: the damage of the whole
/// magazine is 6/5 of the unbuffed one, which is what "one shot at 2x out
/// of five" means and is not what "the magazine was full" would give on
/// its own.
#[test]
fn a_chamber_pays_the_first_round_of_the_magazine_and_no_other() {
    let p = |bonus: f64| FightParams {
        multishot: 1.0,
        first_round_damage: bonus,
        fire_rate: 1.0,
        magazine_size: 5.0,
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        arcane: ArcaneFx::none(),
        // Exactly one magazine, so the ratio is arithmetic and not a
        // question about how many reloads fitted in the window.
        infinite_reserve: false,
        reserve_ammo: 0.0,
        duration_seconds: 10.0,
        body_parts: mono_body(1.0),
        ..no_status()
    };
    let off = monte_carlo(&p(0.0), 4, 3);
    let on = monte_carlo(&p(1.0), 4, 3);
    assert!((off.mean_shots - 5.0).abs() < 1e-9, "shots {}", off.mean_shots);
    assert!((on.mean_shots - off.mean_shots).abs() < 1e-9, "cadence must not change");
    assert!(
        (on.mean_damage / off.mean_damage - 1.2).abs() < 1e-6,
        "one of five rounds doubled: {} vs {}",
        on.mean_damage, off.mean_damage
    );
}

/// …AND THE GATE IS A POST-SHOT READING, which is the half of the rule that
/// only shows itself under AMMO EFFICIENCY.
///
/// VERBATIM (wiki, both chamber pages): the bonus lands *"as long as the
/// magazine counter is at Max Magazine - 1 after a shot is fired"*, and
/// *"when used alongside 100% ammo efficiency, make sure one shot is
/// missing from the magazine, since the buff doesn't apply on a completely
/// full one"*.
///
/// So at 100% efficiency the magazine never leaves full and the mod is
/// worth EXACTLY NOTHING — where a "the magazine was full" gate would have
/// paid it on every shot of the run, which is the same bug DE fixed on the
/// Vectis Incarnon in ver 43.5.
#[test]
fn full_ammo_efficiency_never_leaves_the_magazine_full_enough_to_pay() {
    let p = |bonus: f64| FightParams {
        multishot: 1.0,
        first_round_damage: bonus,
        fire_rate: 1.0,
        magazine_size: 5.0,
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        arcane: ArcaneFx { ammo_efficiency: 1.0, ..ArcaneFx::none() },
        ammo_efficiency_applies: true,
        duration_seconds: 10.0,
        body_parts: mono_body(1.0),
        ..no_status()
    };
    let off = monte_carlo(&p(0.0), 4, 3);
    let on = monte_carlo(&p(1.0), 4, 3);
    assert!((on.mean_shots - off.mean_shots).abs() < 1e-9, "cadence must not change");
    assert!(
        (on.mean_damage - off.mean_damage).abs() < 1e-6,
        "a magazine that never empties never reads max-1: {} vs {}",
        on.mean_damage, off.mean_damage
    );
}

/// Plentiful Mayhem, discrete branch: "only applies to projectiles
/// GENERATED BY multishot". Two pellets a pull means ONE plain and ONE at
/// x1.6, so a pull deals 2.6 pellet-units against a plain 2.0 — not 3.2,
/// which is what treating it as a weapon-wide bonus would give.
#[test]
fn plentiful_mayhem_pays_only_the_multishot_generated_projectiles() {
    let p = |bonus: f64| FightParams {
        multishot: 2.0,
        multishot_ammo_bonus: bonus,
        // No crit variance: every pellet must deal the SAME number, or the
        // ratio below would depend on which pellet happened to crit.
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        arcane: ArcaneFx::none(),
        body_parts: mono_body(1.0),
        ..no_status()
    };
    let off = monte_carlo(&p(0.0), 4, 3);
    let on = monte_carlo(&p(0.6), 4, 3);
    assert!(
        (on.mean_pellets - off.mean_pellets).abs() < 1e-9,
        "the perk must not change the pellet count"
    );
    let ratio = on.mean_damage / off.mean_damage;
    assert!((ratio - 2.6 / 2.0).abs() < 1e-9, "ratio {ratio}");
    // With NO multishot source there is no generated projectile at all, so
    // the perk is worth exactly nothing — the wiki's rule, stated as a test.
    let solo = |bonus: f64| FightParams { multishot: 1.0, ..p(bonus) };
    let (a, b) = (monte_carlo(&solo(0.0), 4, 3), monte_carlo(&solo(0.6), 4, 3));
    assert!((a.mean_damage - b.mean_damage).abs() < 1e-9, "inert at 1x multishot");
}

/// …and it follows the generated grenade into the CLOUD it leaves. One pull, two grenades, ten ticks each: 400 plain + 640
/// boosted against a 800 baseline. That is where the perk's value is on
/// this weapon — the cloud is most of its damage.
#[test]
fn plentiful_mayhem_follows_the_generated_grenade_into_its_cloud() {
    let p = |bonus: f64| FightParams {
        damage: DamageVector::default(), // inert impact: the fields alone
        lingering: Some(cloud(crate::model::FieldStacking::Stack)),
        multishot: 2.0,
        multishot_ammo_bonus: bonus,
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        arcane: ArcaneFx::none(),
        fire_rate: 1.0,
        // Exactly one pull — a one-round magazine behind a reload longer
        // than the run — then a 60 s window so both clouds finish. Reserves
        // stay INFINITE so the generated grenade can always pay its round;
        // starving it is the next test, and it must not leak into this one.
        magazine_size: 1.0,
        reload_seconds: 999.0,
        infinite_reserve: true,
        duration_seconds: 60.0,
        ..no_status()
    };
    let off = monte_carlo(&p(0.0), 4, 3);
    let on = monte_carlo(&p(0.6), 4, 3);
    assert!((off.mean_shots - 1.0).abs() < 1e-9, "shots {}", off.mean_shots);
    assert!(
        (off.mean_field_ticks - 20.0).abs() < 1e-9,
        "two grenades, ten ticks each: {}",
        off.mean_field_ticks
    );
    assert!(
        (on.mean_field_ticks - off.mean_field_ticks).abs() < 1e-9,
        "a damage bonus must not change the tick COUNT"
    );
    assert!((off.mean_damage - 800.0).abs() < 1e-9, "baseline {}", off.mean_damage);
    assert!(
        (on.mean_damage - (400.0 + 640.0)).abs() < 1e-9,
        "boosted {} (expected 400 plain + 640 from the generated grenade)",
        on.mean_damage
    );
}

/// Ammo efficiency is a DIVIDED COST and the magazine keeps the fraction —
/// ✅ measured (MEASUREMENTS M14). A 5-round magazine at 75% efficiency
/// takes 20 shots, because each costs 0.25.
#[test]
fn ammo_efficiency_divides_the_cost_and_the_magazine_keeps_the_fraction() {
    let p = |eff: f64| FightParams {
        arcane: ArcaneFx { ammo_efficiency: eff, ..ArcaneFx::none() },
        ammo_efficiency_applies: true,
        fire_rate: 1.0,
        magazine_size: 5.0,
        // One magazine only: dry reserves stop the run instead of
        // reloading, so the shot count is exactly what the magazine bought.
        infinite_reserve: false,
        reserve_ammo: 0.0,
        duration_seconds: 100.0,
        body_parts: mono_body(1.0),
        ..no_status()
    };
    let plain = monte_carlo(&p(0.0), 4, 3);
    assert!((plain.mean_shots - 5.0).abs() < 1e-9, "shots {}", plain.mean_shots);
    let eff = monte_carlo(&p(0.75), 4, 3);
    assert!(
        (eff.mean_shots - 20.0).abs() < 1e-9,
        "5 rounds at 0.25 each = 20 shots, got {}",
        eff.mean_shots
    );
}

/// …and an overdraw's DEBT survives the reload — ✅ measured (M14, user's
/// in-game run on a 5-round magazine): 3 buffed shots leave 4.25, five
/// full-cost shots take that to −0.75 and trigger the reload, and the fresh
/// magazine comes back at 4.25, not 5. The tell in game is the UI, which
/// shows the CEILING: one more 0.25 shot moves 4.25 to exactly 4.00 and the
/// readout drops 5 -> 4, which a clean 5.00 magazine could never do.
#[test]
fn an_efficiency_overdraw_carries_its_debt_through_the_reload() {
    // 60% efficiency = 0.4 a shot, chosen because it does NOT divide a
    // 5-round magazine evenly — which is the only way the two models can
    // be told apart:
    //   magazine 1: 13 shots take 5.0 to -0.2, then reload.
    //   carry    -> 5.0 + (-0.2) = 4.8, which buys exactly 12 more = 25.
    //   reset    -> a clean 5.0, which buys 13 more = 26.
    let p = FightParams {
        arcane: ArcaneFx { ammo_efficiency: 0.6, ..ArcaneFx::none() },
        ammo_efficiency_applies: true,
        fire_rate: 1.0,
        magazine_size: 5.0,
        reserve_ammo: 5.0, // exactly one reload's worth, then dry
        infinite_reserve: false,
        reload_seconds: 0.001,
        duration_seconds: 100.0,
        body_parts: mono_body(1.0),
        ..no_status()
    };
    let s = monte_carlo(&p, 4, 3);
    assert!((s.mean_reloads - 1.0).abs() < 1e-9, "reloads {}", s.mean_reloads);
    assert!(
        (s.mean_shots - 25.0).abs() < 1e-9,
        "expected 25 shots (13 + 12, the debt carried); 26 would mean the \
             reload wiped it. got {}",
        s.mean_shots
    );
}

/// A LOCK IS ABSOLUTE, AND THE SIM OWNS HALF OF IT.
///
/// "Equipping this mod will set weapon's Fire Rate to its default ignoring
/// other bonuses, EVEN NEGATIVE EFFECTS" (wiki, Semi-Rifle/Shotgun/Pistol
/// Cannonade); Primary and Pistol Acuity say the same of Multishot.
/// `resolve` empties the mod bucket, but the weapon's own Frenzy passive is
/// a x2.5 in the BUFF BAR and an arcane's multishot is added per shot — both
/// past the resolver, both surviving a lock that stopped at the bucket.
///
/// The measurable consequence, and why it is worth a test rather than a
/// comment: on Dual Toxocyst a Cannonade build kept Frenzy's x2.5 cadence,
/// so the sim reported roughly two and a half times the shots the game can
/// fire.
#[test]
fn a_locked_stat_ignores_the_live_sources_too() {
    let head = vec![BodyPart {
        name: "head".into(),
        aim_weight: 1.0,
        multiplier: 3.0,
        is_head: true,
        crit_bonus: true,
    }];
    // ---- FIRE RATE: the buff bar's Frenzy multiplier.
    let fr = |frenzy: bool, locked: bool| {
        let p = FightParams {
            frenzy,
            locked_stats: if locked { vec!["fire_rate"] } else { Vec::new() },
            fire_rate: 4.0,
            magazine_size: 1e9, // no reload to blur the cadence
            body_parts: head.clone(),
            duration_seconds: 60.0,
            ..no_status()
        };
        monte_carlo(&p, 6, 5).mean_shots
    };
    let bare = fr(false, false);
    assert!(fr(true, false) > bare * 1.5, "Frenzy pays when nothing locks it");
    assert!(
        (fr(true, true) - bare).abs() < 1e-9,
        "under the lock the weapon fires at its DEFAULT cadence: {} vs {bare}",
        fr(true, true)
    );
    // ...and locking it changes nothing on a build that had no buff to lose,
    // so the assertion above is about the lock and not about the flag.
    assert!((fr(false, true) - bare).abs() < 1e-9);

    // ---- MULTISHOT: an arcane's live stacks (Primary Overcharge, a
    // `Passive` trigger — simply ON, so it needs no event to arm).
    let mut tenno = crate::data::tenno::default_tenno().clone();
    tenno.energy = 1000.0;
    tenno.state.energy_pct = 1.0;
    let over = crate::data::arcanes::for_slot("primary", "primary_overcharge")
        .expect("primary_overcharge");
    let fx = over.fx(5, crate::model::StackPolicy::Emergent, &[], &tenno);
    assert!(!fx.buffs.is_empty(), "a 1,000-energy frame arms it");
    let multishot = |locked: bool| {
        let p = FightParams {
            arcane: fx.clone(),
            locked_stats: if locked { vec!["multishot"] } else { Vec::new() },
            multishot: 1.0,
            base_multishot: 1.0,
            magazine_size: 1e9,
            body_parts: head.clone(),
            duration_seconds: 30.0,
            ..no_status()
        };
        // Damage stands in for the pellet count: the cadence is untouched,
        // so a run's damage is proportional to the pellets each pull rolls.
        monte_carlo(&p, 6, 9).mean_damage
    };
    let (open, locked) = (multishot(false), multishot(true));
    assert!(open > locked * 2.0, "+350% multishot is 4.5 pellets a pull: {open} vs {locked}");
    // The locked run is the weapon's DEFAULT pellet count — the same number a
    // build with no arcane at all fires.
    let none = {
        let p = FightParams {
            // NO arcane at all — the fixture's own would otherwise be the
            // difference being measured.
            arcane: crate::data::arcanes::ArcaneFx::none(),
            multishot: 1.0,
            base_multishot: 1.0,
            magazine_size: 1e9,
            body_parts: head.clone(),
            duration_seconds: 30.0,
            ..no_status()
        };
        monte_carlo(&p, 6, 9).mean_damage
    };
    assert!((locked - none).abs() < 1e-6, "{locked} vs {none}");
}

/// A FREE shot needs no round in the magazine. At 100%
/// ammo efficiency the cost is zero, so an empty magazine is not a reason to
/// reload — the Dual Toxocyst case, where the last round headshots, the
/// magazine lands on 0 and that same kill arms Frenzy.
#[test]
fn a_zero_cost_shot_fires_off_an_empty_magazine_instead_of_reloading() {
    // A ONE-round magazine is what puts the boundary in reach: shot 1 is
    // taken before Frenzy exists, so it pays full price and lands the
    // magazine on exactly 0 — and that same headshot arms the +100%
    // efficiency. Every later shot is free, so the magazine must never be
    // refilled. A static 100% efficiency could not test this: the magazine
    // would never reach 0 in the first place.
    let head = vec![BodyPart {
        name: "head".into(),
        aim_weight: 1.0,
        multiplier: 3.0,
        is_head: true,
        crit_bonus: true,
    }];
    let p = |frenzy: bool| FightParams {
        frenzy,
        magazine_size: 1.0,
        // One spare round, so a reload is possible AND countable if the
        // gate wrongly fires.
        infinite_reserve: false,
        reserve_ammo: 1.0,
        body_parts: head.clone(),
        ..no_status()
    };
    let with = monte_carlo(&p(true), 20, 4);
    assert_eq!(
        with.mean_reloads, 0.0,
        "a free shot must fire off the empty magazine, not reload"
    );
    // Without Frenzy every shot costs a round, so the same fixture spends
    // its one reserve round on a reload and then runs dry at 2 shots —
    // which is exactly what the old gate did even WITH Frenzy.
    let without = monte_carlo(&p(false), 20, 4);
    assert!((without.mean_reloads - 1.0).abs() < 1e-9, "reloads {}", without.mean_reloads);
    assert!((without.mean_shots - 2.0).abs() < 1e-9, "shots {}", without.mean_shots);
    assert!(
        with.mean_shots > without.mean_shots,
        "free shots keep firing: {} vs {}",
        with.mean_shots,
        without.mean_shots
    );
}

/// Ammo efficiency CAPS at 100%: a shot can cost
/// nothing, never less. Stacking past the cap buys nothing and must never
/// start refunding ammo — a magazine cannot grow while the weapon fires.
#[test]
fn ammo_efficiency_caps_at_free_and_never_refunds() {
    // The pure function first, since that is where the ceiling lives.
    assert_eq!(ammo_efficiency(true, 1.0, 1.0, 1.0, 0.0), 1.0, "3x over the cap");
    assert_eq!(ammo_efficiency(true, 0.0, 0.0, 0.0, 0.0), 0.0);
    assert_eq!(ammo_efficiency(false, 1.0, 1.0, 1.0, 1.0), 0.0, "charge-backed is exempt");
    // AN ABILITY'S SHARE MULTIPLIES rather than adding — the wiki's own
    // rule, and the arithmetic that separates the two: 75% on top of 50%
    // is 87.5% (cost 0.5 x 0.25), where adding would have read 125% and
    // clamped to free.
    assert!((ammo_efficiency(true, 0.5, 0.0, 0.0, 0.75) - 0.875).abs() < 1e-12);
    // …and either one alone is itself.
    assert!((ammo_efficiency(true, 0.0, 0.0, 0.0, 0.75) - 0.75).abs() < 1e-12);
    assert_eq!(ammo_efficiency(true, 0.0, 0.0, 0.0, 1.0), 1.0);

    // End to end: an absurd stack behaves exactly like a plain 100%. Note
    // this half cannot catch a silent refund on its own — a magazine that
    // grows is not reported anywhere, so nothing downstream would move.
    // The assertions above are the real guard; this one pins that going
    // over the cap does not disturb the cadence.
    let p = |eff: f64| FightParams {
        arcane: ArcaneFx { ammo_efficiency: eff, ..ArcaneFx::none() },
        ammo_efficiency_applies: true,
        fire_rate: 1.0,
        magazine_size: 3.0,
        infinite_reserve: false,
        reserve_ammo: 0.0, // no reserve at all: a refund would be visible
        duration_seconds: 20.0,
        body_parts: mono_body(1.0),
        ..no_status()
    };
    let exact = monte_carlo(&p(1.0), 4, 3);
    let over = monte_carlo(&p(5.0), 4, 3);
    assert!((exact.mean_shots - 20.0).abs() < 1e-9, "shots {}", exact.mean_shots);
    assert!(
        (over.mean_shots - exact.mean_shots).abs() < 1e-9,
        "over-cap must behave as exactly free: {} vs {}",
        over.mean_shots,
        exact.mean_shots
    );
    assert_eq!(over.mean_reloads, 0.0);
}

/// Plentiful Mayhem on the real Incarnon numbers — ✅ measured: the 170-charge pool at 8
/// ticks per second lasts **170 / 8 / multishot** seconds, against 170/8 =
/// 21.25 s without the perk. That is the whole cost of the +60%.
#[test]
fn plentiful_mayhem_shortens_the_incarnon_window_by_the_multishot_factor() {
    // torid_incarnon.yaml: pseudo_reload.magazine 170, attack.fire_rate 8
    // (ticks per second, trigger "held").
    const CHARGES: f64 = 170.0;
    const TICK_RATE: f64 = 8.0;
    let p = |multishot: f64, bonus: f64| FightParams {
        continuous: true,
        fire_rate: TICK_RATE,
        multishot,
        base_multishot: 1.0,
        multishot_ammo_bonus: bonus,
        magazine_size: CHARGES,
        // The charge pool is outside the ammo economy: no reserve behind
        // it, and no efficiency reaches it.
        ammo_efficiency_applies: false,
        infinite_reserve: false,
        reserve_ammo: 0.0,
        duration_seconds: 120.0,
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        arcane: ArcaneFx::none(),
        body_parts: mono_body(1.0),
        ..no_status()
    };
    // Without the perk the window is multishot-independent: a merged beam
    // still bills ONE charge a tick however many beams it merges.
    for multishot in [1.0, 2.0, 5.0] {
        let s = monte_carlo(&p(multishot, 0.0), 4, 3);
        assert!(
            (s.mean_shots - CHARGES).abs() < 1e-9,
            "no perk at {multishot}x: expected {CHARGES} ticks, got {}",
            s.mean_shots
        );
    }
    // With it, every projectile bills a charge, so the window divides.
    for multishot in [1.0, 2.0, 5.0] {
        let s = monte_carlo(&p(multishot, 0.6), 4, 3);
        let want = CHARGES / multishot;
        assert!(
            (s.mean_shots - want).abs() < 1e-9,
            "{multishot}x multishot: expected {want} ticks, got {}",
            s.mean_shots
        );
        // …and that is the user's 170/8/multishot, stated as seconds.
        let seconds = s.mean_shots / TICK_RATE;
        assert!(
            (seconds - CHARGES / TICK_RATE / multishot).abs() < 1e-9,
            "{multishot}x multishot: expected {} s, got {seconds}",
            CHARGES / TICK_RATE / multishot
        );
    }
}

/// Ammo efficiency serves the MAGAZINE round only — it never reaches
/// Plentiful Mayhem's multishot surcharge (✅ measured,).
/// So 100% efficiency makes the shot itself free while every generated
/// projectile still pays full price out of reserve, and the two pools
/// empty independently.
#[test]
fn ammo_efficiency_does_not_pay_for_plentiful_mayhems_extra_projectiles() {
    let p = FightParams {
        arcane: ArcaneFx { ammo_efficiency: 1.0, ..ArcaneFx::none() },
        ammo_efficiency_applies: true,
        multishot: 3.0, // 1 magazine round + 2 surcharged extras
        multishot_ammo_bonus: 0.6,
        fire_rate: 1.0,
        magazine_size: 5.0,
        infinite_reserve: false,
        // Exactly two shots' worth of extras, so the starvation boundary
        // lands inside the window and is visible.
        reserve_ammo: 4.0,
        duration_seconds: 5.0,
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        body_parts: mono_body(1.0),
        ..no_status()
    };
    let s = monte_carlo(&p, 4, 3);
    // The magazine round is free, so the weapon never reloads and fires
    // the whole window: 5 shots at 1/s.
    assert!((s.mean_shots - 5.0).abs() < 1e-9, "shots {}", s.mean_shots);
    assert_eq!(s.mean_reloads, 0.0, "a free magazine round never reloads");
    // Reserve pays for the extras at FULL price: 2 + 2, then it is dry and
    // the remaining shots fire alone. 3 + 3 + 1 + 1 + 1 = 9 pellets.
    // Were efficiency to reach the surcharge, all five shots would carry
    // three pellets for 15.
    assert!(
        (s.mean_pellets - 9.0).abs() < 1e-9,
        "expected 9 pellets (3+3+1+1+1); 15 would mean efficiency paid for \
             the extras. got {}",
        s.mean_pellets
    );
}

/// CO on an AoE part is the EXCEPTION. The mods say Condition Overload
/// boosts DIRECT hits, so a field gets nothing unless its weapon declares
/// otherwise — the Torid's cloud does, and the CO catalog gives it a row
/// precisely because an AoE part taking CO is not supposed to happen. This
/// pins the DEFAULT, which no roster weapon exercises yet.
#[test]
fn a_field_takes_condition_overload_only_where_the_weapon_declares_it() {
    let p = |takes: bool| FightParams {
        // Zero-damage impact that still forces an IMPACT proc: the target
        // carries one status TYPE for the CO bracket to count, and a
        // stagger adds no damage of its own to confound the field's total.
        damage: DamageVector::default(),
        forced_procs: vec![DamageType::Impact],
        lingering: Some(crate::build::loadout::ResolvedLingering {
            takes_condition_overload: takes,
            ..cloud(crate::model::FieldStacking::Stack)
        }),
        co_per_type: 1.0,
        co_behavior: crate::model::CoBehavior::Independent,
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        arcane: ArcaneFx::none(),
        fire_rate: 1.0,
        magazine_size: 1.0,
        reload_seconds: 999.0,
        duration_seconds: 60.0,
        ..no_status()
    };
    let off = monte_carlo(&p(false), 4, 3);
    let on = monte_carlo(&p(true), 4, 3);
    assert!(
        (off.mean_field_ticks - on.mean_field_ticks).abs() < 1e-9,
        "the flag must move damage, not tick counts"
    );
    assert!(
        on.mean_damage > off.mean_damage + 1e-9,
        "declaring it must be worth something: {} vs {}",
        on.mean_damage,
        off.mean_damage
    );
    // The un-declared field is the plain 10 x 40 with no CO bracket at all.
    assert!(
        (off.mean_damage - 400.0).abs() < 1e-9,
        "expected a bare 400, got {}",
        off.mean_damage
    );
}

/// …and the same for an EXPLOSION. The mods forbid it — CO boosts direct
/// hits — but the engine supports the case anyway, because the CO catalog
/// lists entries that do it: the Zylok's Incarnon radial receives CO "on
/// target directly hit by bullet", which the arena always is. No roster
/// weapon declares it, so this test is the only thing holding the branch
/// open.
#[test]
fn a_radial_takes_condition_overload_only_where_the_weapon_declares_it() {
    let radial = |takes: bool| crate::build::loadout::ResolvedRadial {
        blast_kind: crate::model::BlastKind::Contact,
        damage: {
            let mut d = DamageVector::default();
            d.set(DamageType::Radiation, 100.0);
            d
        },
        modified_base: 100.0,
        crit_chance: 0.0,
        crit_damage: 1.0,
        base_crit_chance: 0.0,
        base_crit_damage: 1.0,
        status_chance: 0.0,
        base_status_chance: 0.0,
        radius_m: 2.0,
        falloff_start_m: 0.0,
        falloff_reduction: 0.0,
        forced_procs: Default::default(),
        takes_condition_overload: takes,
        takes_multishot: true,
        co_base: crate::model::CoBase::whole_for(crate::model::CoStage::Radial),
    };
    // Zero-damage direct hit that still forces an Impact proc, so the only
    // damage reported is the explosion's and the target carries one status
    // type for CO to count.
    let p = |takes: bool| FightParams {
        damage: DamageVector::default(),
        forced_procs: vec![DamageType::Impact],
        radial: Some(radial(takes)),
        co_per_type: 1.0,
        co_behavior: crate::model::CoBehavior::Independent,
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        arcane: ArcaneFx::none(),
        fire_rate: 1.0,
        body_parts: mono_body(1.0),
        ..no_status()
    };
    let off = monte_carlo(&p(false), 4, 3);
    let on = monte_carlo(&p(true), 4, 3);
    assert!(
        (on.mean_shots - off.mean_shots).abs() < 1e-9,
        "the flag must not change the cadence"
    );
    assert!(
        on.mean_damage > off.mean_damage + 1e-9,
        "declaring it must be worth something: {} vs {}",
        on.mean_damage,
        off.mean_damage
    );
    // Strike 1 lands before any status exists, so every shot after it doubles
    // under CO — the un-declared explosion stays flat at 100 a shot.
    assert!(
        (off.mean_damage - 100.0 * off.mean_shots).abs() < 1e-9,
        "expected a flat 100/shot with no CO, got {}",
        off.mean_damage
    );
}

/// A reload draws WHOLE rounds — ✅ measured on a
/// 5-round magazine. The table is the measurement, verbatim.
#[test]
fn a_reload_draws_whole_rounds_and_leaves_the_fraction_behind() {
    let cap = 5.0;
    // 1.5 -> floor(3.5) = 3 -> 4.5, NOT a full 5.
    assert_eq!(reload_draw(cap, 1.5), 3.0);
    // 3.25 -> floor(1.75) = 1 -> 4.25, which is what the user saw.
    assert_eq!(reload_draw(cap, 3.25), 1.0);
    // 4.25 -> floor(0.75) = 0: the reload is refused, and the HUD's
    // ceiling makes it read as an already-full magazine.
    assert_eq!(reload_draw(cap, 4.25), 0.0);
    // Empty and overdrawn are the same rule, not a special case: a shot
    // cannot overdraw by a whole round, so the draw is always `cap`.
    assert_eq!(reload_draw(cap, 0.0), 5.0);
    assert_eq!(reload_draw(cap, -0.75), 5.0, "M14: comes back at 4.25");
    // Never negative, however overfull the magazine.
    assert_eq!(reload_draw(cap, 9.0), 0.0);
}

/// The two arcane stack-decay families, told apart IN THE SIM. Primary
/// Crux is the `all_drop` one — VERBATIM (wiki): *"All stacks are lost when
/// the buff's duration expires"*, confirmed in game:
/// the timer runs out and the whole pile goes at once. The other family
/// (Merciless/Deadhead/Dexterity) loses ONE stack and resets the timer.
///
/// `data::arcanes` already pins Crux's flag; this pins that the flag still
/// means something by the time the shot loop reads it.
#[test]
fn arcane_stacks_all_drop_on_timeout_or_bleed_off_one_at_a_time() {
    // 3 stacks x +1 multishot each, on a trigger that can never fire (no
    // status in this fixture), so the run only ever DECAYS from full.
    // Stacks are seeded full with expiry = duration (ArcRuntime::init).
    let p = |all_drop: bool| FightParams {
        arcane: ArcaneFx {
            buffs: vec![ArcBuffSpec {
            owner: "test".into(),
                grant: ArcGrant::Multishot,
                trigger: ArcTrigger::ToxinStatus,
                per_stack: 1.0,
                max_stacks: 3,
                duration: 2.0,
                all_drop,
                one_per_instance: false,
                initial_stacks: 3,
            }],
            ..ArcaneFx::none()
        },
        multishot: 1.0,
        base_multishot: 1.0,
        fire_rate: 1.0,
        magazine_size: 100.0, // no reload inside the window
        duration_seconds: 8.0,   // shots at t = 0..7
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        body_parts: mono_body(1.0),
        ..no_status()
    };
    // all_drop: 3 stacks until t=2, then nothing at all.
    //   4 4 1 1 1 1 1 1 = 14
    let cliff = monte_carlo(&p(true), 4, 3);
    assert!((cliff.mean_shots - 8.0).abs() < 1e-9, "shots {}", cliff.mean_shots);
    assert!(
        (cliff.mean_pellets - 14.0).abs() < 1e-9,
        "expected 14 pellets (4 4 1 1 1 1 1 1), got {}",
        cliff.mean_pellets
    );
    // lose-one-and-reset: one stack every 2 s instead of a cliff.
    //   4 4 3 3 2 2 1 1 = 20
    let graceful = monte_carlo(&p(false), 4, 3);
    assert!(
        (graceful.mean_pellets - 20.0).abs() < 1e-9,
        "expected 20 pellets (4 4 3 3 2 2 1 1), got {}",
        graceful.mean_pellets
    );
}

/// ONE STACK PER DAMAGE INSTANCE, and the instance is the TRIGGER PULL.
/// Wiki (Cascadia Flare), verbatim: *"Only one stack can be added per
/// damage instance; applying multiple Heat status effects, such as via
/// Multishot or Archon Vitality in a single hit will not generate multiple
/// stacks."* The sim bumped once per PROC — inside `settle_procs`, which
/// runs per pellet — so five pellets each proccing Heat granted five.
///
/// Measured through the REPLAY rather than through damage: the claim is
/// about the stack count, and reading anything else would let a wrong
/// count pass by cancelling against a right multiplier.
#[test]
fn a_per_instance_arcane_gains_one_stack_a_pull_not_one_a_pellet() {
    let p = |one_per_instance: bool| FightParams {
        arcane: ArcaneFx {
            id: "test".into(),
            buffs: vec![ArcBuffSpec {
                owner: "test".into(),
                grant: ArcGrant::CritDamage,
                trigger: ArcTrigger::HeatStatus,
                per_stack: 0.0, // observed, never applied: no feedback
                max_stacks: 40,
                duration: 1000.0, // no decay inside the window
                all_drop: true,
                one_per_instance,
                initial_stacks: 0,
            }],
            ..ArcaneFx::none()
        },
        // FIVE pellets, every one of them forcing a Heat proc: the exact
        // case the wiki names.
        multishot: 5.0,
        base_multishot: 5.0,
        forced_procs: vec![DamageType::Heat],
        fire_rate: 1.0,
        magazine_size: 100.0,
        duration_seconds: 10.0, // pulls at t = 0..9
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        body_parts: mono_body(1.0),
        ..no_status()
    };
    let stacks_at_end = |one: bool| {
        let params = p(one);
        let s = monte_carlo(&params, 1, 3);
        // 20 frames over 10 s, so the LAST frame sits at t = 9.5 — after
        // the t = 9 pull rather than on top of it.
        let rep = replay(&params, s.median_run.rng_state, 20);
        let i = rep.buffs.iter().position(|x| x.id == "arcane:test").expect("buff in roster");
        *rep.frames.last().expect("frames").stacks.get(i).expect("stack series")
    };
    // 10 pulls, 5 pellets each. Capped: one a pull -> 10. Uncapped: one a
    // pellet -> 40, the ceiling, reached in the first two pulls.
    assert_eq!(stacks_at_end(true), 10, "one stack per trigger pull");
    assert_eq!(stacks_at_end(false), 40, "and per pellet without the cap");
}

/// Plentiful Mayhem STARVES: the projectiles are produced in order, each
/// paying a round as it goes, and one that cannot pay is simply not fired. The round itself always comes from the magazine, so
/// the shot still happens — it just fires fewer pellets.
#[test]
fn plentiful_mayhem_drops_the_pellets_the_reserve_cannot_pay_for() {
    // 4x multishot, one magazine round, and only TWO reserve rounds for the
    // three extras: two are afforded, the third is dropped -> 3 pellets.
    let p = |reserve: f64| FightParams {
        multishot: 4.0,
        multishot_ammo_bonus: 0.6,
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        arcane: ArcaneFx::none(),
        fire_rate: 1.0,
        magazine_size: 1.0,
        infinite_reserve: false,
        reserve_ammo: reserve,
        duration_seconds: 30.0,
        body_parts: mono_body(1.0),
        ..no_status()
    };
    let starved = monte_carlo(&p(2.0), 4, 3);
    assert!((starved.mean_shots - 1.0).abs() < 1e-9, "the shot still fires");
    assert!(
        (starved.mean_pellets - 3.0).abs() < 1e-9,
        "1 magazine round + 2 affordable extras = 3 pellets, got {}",
        starved.mean_pellets
    );
    // Dry reserve starves every extra, leaving the weapon's own projectile.
    let dry = monte_carlo(&p(0.0), 4, 3);
    assert!(
        (dry.mean_pellets - 1.0).abs() < 1e-9,
        "only the magazine round's own pellet survives, got {}",
        dry.mean_pellets
    );
    // And the dropped pellets are not billed: damage tracks what FIRED.
    assert!(
        (starved.mean_damage / dry.mean_damage - (1.0 + 2.0 * 1.6)).abs() < 1e-9,
        "ratio {}",
        starved.mean_damage / dry.mean_damage
    );
}

/// Plentiful Mayhem, continuous branch. A merged beam has no separable
/// generated projectile, so the perk scales the multishot BONUS instead —
/// and lands on the same 1 + 1.6(M-1) the discrete branch reaches. The
/// ammo draw still bills the RAW rolled count, which is what shortens the
/// Incarnon window.
#[test]
fn plentiful_mayhem_on_a_beam_scales_the_bonus_and_drains_the_charge() {
    let dmg = |bonus: f64| FightParams {
        continuous: true,
        fire_rate: 8.0,
        multishot: 2.0,
        base_multishot: 1.0,
        multishot_ammo_bonus: bonus,
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        arcane: ArcaneFx::none(),
        // Ammo must not bind here: both runs then tick at the SAME instants,
        // so the beam ramp cancels out of the ratio.
        magazine_size: 10_000.0,
        infinite_reserve: true,
        duration_seconds: 2.0,
        body_parts: mono_body(1.0),
        ..no_status()
    };
    let (off, on) = (monte_carlo(&dmg(0.0), 4, 3), monte_carlo(&dmg(0.6), 4, 3));
    assert!(
        (on.mean_shots - off.mean_shots).abs() < 1e-9,
        "same tick count: {} vs {}",
        on.mean_shots,
        off.mean_shots
    );
    let ratio = on.mean_damage / off.mean_damage;
    assert!((ratio - 2.6 / 2.0).abs() < 1e-9, "ratio {ratio}");

    // The charge magazine: 2x multishot bills 2 rounds a tick, so ten
    // rounds last five ticks instead of ten.
    let ammo = |bonus: f64| FightParams {
        magazine_size: 10.0,
        infinite_reserve: false,
        reserve_ammo: 0.0,
        // The charge-backed marker: no Capacity behind this magazine, so
        // the multishot surcharge comes out of the pool itself.
        ammo_efficiency_applies: false,
        duration_seconds: 60.0,
        ..dmg(bonus)
    };
    let (a, b) = (monte_carlo(&ammo(0.0), 4, 3), monte_carlo(&ammo(0.6), 4, 3));
    assert!((a.mean_shots - 10.0).abs() < 1e-9, "baseline ticks {}", a.mean_shots);
    assert!((b.mean_shots - 5.0).abs() < 1e-9, "boosted ticks {}", b.mean_shots);
}

#[test]
fn fractional_multishot_is_a_chance_for_one_more() {
    // Multishot 1.5 -> mean pellets/pull about 1.5.
    let p = FightParams {
        multishot: 1.5,
        body_parts: mono_body(1.0),
        ..no_status()
    };
    let s = monte_carlo(&p, 2000, 6);
    let per_pull = s.mean_pellets / s.mean_shots;
    assert!((per_pull - 1.5).abs() < 0.02, "pellets/pull {per_pull}");
}

#[test]
fn magazine_and_reload_cadence_is_exact() {
    // No Frenzy: 12-round magazine at 1 shot/s, 2.35 s reloads, 30 s:
    // shots 0..11 (12), reload -> resume 14.35..25.35 (12), reload ->
    // resume 28.70, 29.70 (2) = 26 shots, 2 reloads.
    let p = FightParams {
        duration_seconds: 30.0,
        ..no_status()
    };
    let s = monte_carlo(&p, 20, 4);
    assert!((s.mean_shots - 26.0).abs() < 1e-9, "shots {}", s.mean_shots);
    assert!((s.mean_reloads - 2.0).abs() < 1e-9);
}

/// Primary Crux: a stacking buff on weak-point HITS (not kills), whose
/// status-chance grant joins the status BUCKET — wiki: "Status Chance
/// bonus is additive to mods like Rifle Aptitude" — so it is RELATIVE to
/// the attack part's own base. Pinned by arithmetic rather than a rate
/// estimate: 25% base + 10 stacks x +30% = 25% + 75% = exactly 100%, i.e.
/// one proc per instance.
#[test]
fn primary_crux_stacks_status_chance_on_weakpoint_hits() {
    let part = |is_head| {
        vec![BodyPart {
            name: "part".into(),
            aim_weight: 1.0,
            multiplier: 1.0,
            is_head,
            crit_bonus: false,
        }]
    };
    // Stacks are EARNED (docs/BUFFS.md), so this reads the trigger with
    // nothing else in it: weak-point hits build the buff to its cap, body
    // hits build nothing at all. The body run is therefore not "the buff
    // lapsed" but "the buff never existed" — identical to no arcane,
    // instance for instance, which is a stricter statement than the
    // seeded version of this test could make.
    let mk = |is_head, a: ArcaneFx| FightParams {
        status_chance: 0.25,
        base_status_chance: 0.25,
        base_crit_chance: 0.0,
        duration_seconds: 20.0,
        arcane: a,
        body_parts: part(is_head),
        ..FightParams::default()
    };
    let bare = run_once(&mk(true, ArcaneFx::none()), &mut Rng::new(11));
    assert!(
        bare.procs < bare.pellets,
        "25% status chance must not proc every instance ({} of {})",
        bare.procs,
        bare.pellets
    );
    // Weak-point hits: every hit is a trigger, so the buff reaches its
    // 10-stack cap within ten instances and 25% + 10 x 30% of 25% = 100%
    // status chance from there on — most instances proc, and far more
    // than the unbuffed run does.
    let head = run_once(&mk(true, arc("primary_crux")), &mut Rng::new(11));
    assert!(
        head.procs > bare.procs,
        "weak-point hits build the buff ({} vs {})",
        head.procs,
        bare.procs
    );
    assert!(
        head.procs >= head.pellets - 10,
        "at the cap every instance procs; only the climb falls short ({} of {})",
        head.procs,
        head.pellets
    );
    // Body only: nothing ever triggers it, so the arcane contributes
    // NOTHING — not "less", nothing. Same seed, same count as no arcane.
    let body = run_once(&mk(false, arc("primary_crux")), &mut Rng::new(11));
    assert_eq!(
        body.procs, bare.procs,
        "no weak-point hit = no stack = the arcane may not change a thing"
    );
}

/// Crux's second grant feeds the SAME ammo-efficiency bucket as Frenzy
/// (wiki: "additive with other sources of Ammo Efficiency"). 10 stacks x
/// +6% = 60%, so a shot costs 0.4 rounds and the 12-round magazine covers
/// 30 shots — more than this 25 s window fires at 1 shot/s, so the reloads
/// disappear entirely.
#[test]
fn primary_crux_ammo_efficiency_stretches_the_magazine() {
    let p = |a: ArcaneFx| FightParams {
        duration_seconds: 25.0,
        arcane: a,
        body_parts: vec![BodyPart {
            name: "head".into(),
            aim_weight: 1.0,
            multiplier: 3.0,
            is_head: true,
            crit_bonus: true,
        }],
        ..no_status()
    };
    let bare = monte_carlo(&p(ArcaneFx::none()), 20, 4);
    assert!(bare.mean_reloads > 0.0, "the fixture must reload without it");
    let crux = monte_carlo(&p(arc_stacked("primary_crux")), 20, 4);
    assert_eq!(crux.mean_reloads, 0.0, "60% efficiency covers the window");
    assert!(
        crux.mean_shots >= bare.mean_shots,
        "shots {} vs {}",
        crux.mean_shots,
        bare.mean_shots
    );
}
