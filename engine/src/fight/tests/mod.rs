//! The fight's tests, one file per thing they exercise. What more than one
//! file builds on — the baseline `Default` fight and the fixtures — lives here.

use super::*;

mod combatant_attribution;
mod two_seats;
mod attrition_times_co;
mod buff_decay_family;
mod burst_cadence;
mod cadence_ammo_multishot;
mod chill_and_co_base;
mod cycle_buff_conversion;
mod debilitate_attrition;
mod dot_ticks_measured;
mod elements_arcanes_weakpoints;
mod every_form_runs;
mod formation_and_spread;
mod fortifier_tick;
mod headshot_buff_wiring;
mod incarnon_reload_route;
mod m102_latron_prime;
mod m103_latron_punch_through;
mod melee;
mod orbs_fields_beams;
mod overguard_status;
mod pellet_volley;
mod perks_and_cards;
mod pools_columns_record;
mod radials_roster_targets;
mod replay_debilitate_misc;
mod replay_reads_every_buff;
mod sniper_combo_fight;
mod status_crit_parts;
mod stormburst;
mod stream_independence;
mod warframe_ability;

#[cfg(test)]
impl Default for FightParams {
    /// TEST FIXTURE baseline: Dual Toxocyst base form + Secondary Enervate,
    /// humanoid dummy, 10 s. Production code never default-constructs
    /// FightParams — a default weapon would smuggle weapon knowledge into
    /// the engine.
    fn default() -> Self {
        Self {
            also_acting: Vec::new(),
            sample_by: crate::rules::metrics::RunStat::KillProgress,
            // A FIXTURE ASSUMES ITS ABILITIES UP, which is the fight's default.
            cast_interrupts: Vec::new(),
            apl_inserted: crate::data::apl::Apl::default(),
            form: crate::model::FormKind::Base,
            // Ordinary: only one measured entry differs (see the field).
            echo_multiplier: 1.0,
            // A FIXTURE BRINGS NO WARFRAME: no auras, no shards.
            squad: crate::data::tenno::SquadEffects::default(),
            enervate_stacks: 0,
            influence_open: None,
            rage_open: None,
            target_id: "e1".to_string(),
            // NO PUNCH THROUGH by default, so the fixture fires the one-body
            // shot every golden value was calibrated against.
            punch_through_m: 0.0,
            projectile_width_m: 0.0,
            range_m: f64::INFINITY,
            damage: Self::dual_toxocyst_base_vector(),
            radial: None,
            cluster: None,
            // POINT BLANK, and no falloff to notice it with — every golden
            // value in this file was measured with the two of them standing on
            // the same spot, so the fixture keeps them there.
            falloff: None,
            // …and nothing may miss: a fixture that dropped shots would put
            // every golden value in this file at the mercy of an aim roll.
            spread: None,
            player_at: crate::rules::space::Vec2::ORIGIN,
            target_at: crate::rules::space::Vec2::ORIGIN,
            lingering: None,
            continuous: false,
            field_duration_on_empty_reload: 1.0,
            multishot_on_last_round: 0.0,
            base_multishot_on_last_round: 0.0,
            multishot_ammo_bonus: 0.0,
            compression_multiplier: 1.0,
            compression_base_damage: 0.0,
            base_damage_below_half_health: 0.0,
            crit_chance_on_undamaged: 0.0,
            crit_damage_on_undamaged: 0.0,
            headshot_damage_bonus: 0.0,
            headshot_bonus_multiplicative: false,
            noncrit_bonus: None,
            stacking_buffs: Vec::new(),
            base_crit_chance: 0.05,
            crit_multiplier: 2.0,
            crit_multiplier_below_crit_chance: None,
            unmodded_crit_chance: 0.05,
            unmodded_crit_damage: 2.0,
            status_chance: 0.37,
            base_status_chance: 0.37,
            forced_procs: Vec::new(),
            attractor_seconds: None,
            status_duration_multiplier: 1.0,
            fire_rate: 1.0,
            charge_seconds: None,
            charge_cadence: crate::model::ChargeCadence::DrawThenRate,
            sustained_fire_rate: None,
            battery: None,
            rs_on_reload: 0.0,
            armor_strip_per_puncture: 0.0,
            instant_reload: None,
            headshot_streak: None,
            crit_damage_below_status_count: None,
            burst: None,
            frenzy: false,
            locked_buffs: Vec::new(),
            cycle: None,
            magazine_size: 12.0,
            reload_seconds: 2.35,
            infinite_reserve: true,
            ammo_drops: true,
            ammo_pickup: 0.0,
            ammo_conversion: 0.0,
            pickup_range_m: f64::INFINITY,
            landscape: false,
            ammo_class: Some(crate::rules::ammo::Pickup::Primary),
            ammo_cost: 1.0,
            reserve_ammo: 72.0,
            ammo_efficiency_applies: true,
            multishot: 1.0,
            base_multishot: 1.0,
            evo_multishot: None,
            evo_base_damage: None,
            locked_stats: Vec::new(),
            base_damage_bonus: 0.0,
            co_per_type: 0.0,
            co_behavior: crate::model::CoBehavior::AdditiveWithBaseDamage,
            co_base: crate::model::CoBase::whole(),
            unswung_fraction: 0.0,
            co_stack: None,
            multishot_stack: None,
            crit_chance_on_headshot: None,
            on_weakpoint: None,
            weakpoint_open: None,
            crit_chance_stack: None,
            status_damage_multiplier: 1.0,
            elem_dot_bonus: Vec::new(),
            faction_multiplier: 1.0,
            abilities: Vec::new(),
            dot_modified_base: None,
            reload_bonus: 0.0,
            weakpoint_damage: 0.0,
            headshot_multiplier: None,
            crit_tier_upgrade_chance: 0.0,
            slash_on_crit: 0.0,
            weakpoint_crit_chance_relative: 0.0,
            bodyshot_crit_chance_multiplier: 1.0,
            derived_status_from_crit: None,
            derived_crit_from_status: None,
            consecutive_hit_damage: None,
            consecutive_hit_radial_only: false,
            last_round_damage: 0.0,
            first_round_damage: 0.0,
            round_restore_on_status: None,
            instant_reload_on_kill: None,
            magazine_growth_on_empty_reload: None,
            crit_damage_on_kill: None,
            fire_rate_on_reload: None,
            base_damage_on_reload: None,
            base_damage_on_eximus_weakpoint: None,
            acid_shells: None,
            crit_chance_per_hit: None,
            combo_script: Vec::new(),
            follow_through: None,
            slam: None,
            heavy: None,
            tennokai: crate::model::Tennokai::default(),
            spends_combo: false,
            combo_duration_seconds: 0.0,
            combo_frozen: false,
            initial_combo: 0.0,
            heavy_attack_efficiency: 0.0,
            crit_chance_per_combo: 0.0,
            status_chance_per_combo: 0.0,
            combo_count_chance: 0.0,
            combo_count_chance_on_lifted: 0.0,
            combo_gain_chance: 0.0,
            combo_count_on_slam_hit: 0.0,
            status_chance_on_lifted: 0.0,
            heavy_attack_damage: 0.0,
            slam_damage: 0.0,
            crit_chance_per_hit_initial_stacks: 0,
            crit_chance_per_hit_held: false,
            super_crit_on_status: None,
            weakpoint_stacks: None,
            spawn_on_kill: None,
            kill_streak_summon: None,
            kill_streak_summon_opens_active: false,
            kill_streak_opens_at: 0,
            beam_ramp_floor: crate::model::BEAM_RAMP_FLOOR,
            applies_microwave: false,
            independent_procs: &[],
            syndicate_radial: None,
            pellet_damage: Vec::new(),
            multishot_adds_damage: false,
            sniper_combo: None,
            combo_initial: 0,
            combo_held: false,
            tendril_max: 0,
            tendril_range_m: 0.0,
            tendril_acquire_deg: 0.0,
            crit_chance_per_tendril: 0.0,
            sc_per_tendril: 0.0,
            tendrils_initial: 0,
            tendrils_held: false,
            magazine_refill_on_kill: 0.0,
            proc_conversion: None,
            // Secondary Enervate at max rank — the historical calibration
            // profile's arcane (the ramp/reset mechanic is the perk).
            arcane: ArcaneFx {
                id: "secondary_enervate".to_string(),
                enervate_rank: Some(5),
                ..ArcaneFx::none()
            },
            body_parts: BodyPart::humanoid(),
            foe: Foe::training_dummy(),
            tenno: crate::data::tenno::default_tenno().clone(),
            duration_seconds: 10.0,
            // ONE BODY — a fixture, not a formation.
            others: Vec::new(),
            aim_at: None,
            // …and no beam geometry: the fixture is a generic weapon, and a
            // chain is something a weapon DECLARES.
            beam: None,
            ricochet: None,
            // AIMED, like every fixture but the one weapon that is not.
            unaimed_headshot_chance: None,
            // A ROUND LEAVES ON THE TRIGGER, like every gun but one.
            windup_seconds: 0.0,
            // …AND IT HAS A CLIP, like every gun but one.
            no_magazine: false,
            // NO CANTICLE on the calibration fixture.
            strip_on_kill_in_range: None,
            // NOTHING IS DEPLOYED: the calibration fixture throws no orb.
            orb: None,
            orb_strike: None,
            orb_blast: None,
            meter: None,
            squad_size: 1,
        }
    }
}

/// Ten Cold procs, at one-second spacing, on a target described by `caps`
/// and `no_frozen`. Returns the state they left behind.
#[cfg(test)]
fn chill(n: usize, caps: Option<StackCaps>, no_frozen: bool, overguard: bool) -> DebuffState {
    let mut d = DebuffState::default();
    for i in 0..n {
        d.apply_cold_proc(i as f64 * 0.1, 1.0, overguard, caps, no_frozen);
    }
    d
}

/// Default params with status disabled — for hand-computed expectations
/// that predate the status sim.
pub(super) fn no_status() -> FightParams {
    FightParams {
        status_chance: 0.0,
        // Zero the BASE too, or a relative status-chance buff (Primary
        // Crux) would resolve against 0.37 in a "no status" fixture.
        base_status_chance: 0.0,
        ..FightParams::default()
    }
}

/// A secondary arcane at max rank under the Emergent policy (crit-base
/// 0 — none of these tests use the assumed-max relative crit paths).
fn arc(id: &str) -> ArcaneFx {
    crate::data::arcanes::secondary(id).unwrap().fx(5, crate::model::StackPolicy::Emergent, &[], crate::data::tenno::default_tenno())
}

/// The same arcane with its stacks ALREADY EARNED.
///
/// Buffs start at zero now (docs/BUFFS.md §Activation policy), which is
/// the right default for a fight and the wrong fixture for a test about
/// what a full stack is worth or how it decays. Seeding it here says which
/// of the two a test is measuring, instead of leaning on whatever the
/// default happens to be — the reason these tests moved when it changed.
fn arc_stacked(id: &str) -> ArcaneFx {
    let mut fx = arc(id);
    for b in fx.buffs.iter_mut() {
        b.initial_stacks = b.max_stacks;
    }
    fx
}

/// Deterministic base: no crits, no procs, 1x body, no arcane.
fn flat_base() -> FightParams {
    FightParams {
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        arcane: ArcaneFx::none(),
        body_parts: mono_body(1.0),
        ..no_status()
    }
}

/// A BOUNCING PROJECTILE IN A CROWD — a collision plus a 4 m explosion, with
/// the BOUNCE stated HERE rather than read from a weapon: no entry declares
/// both since the Latron family dropped its own (MEASUREMENTS M103), and what
/// these tests pin is the MECHANISM, which still carries the Drakgoon's bounce
/// and the Dual Toxocyst's ricochet.
///
/// A CROWD, not a line: a bounce walks to the nearest body it has not reached
/// (`rules::chain::bounce_path`), so bodies strung out at 5 m intervals leave
/// it nowhere to go, and density is what a bounce weapon wants.
///
/// `headshot_pct` stays 0, so the AIMED pellet is always a body shot and the
/// only head in the fight is one a bounce found. `n` is the OTHER bodies.
fn a_bouncing_projectile_in_a_crowd(n: usize, head_chance: f64) -> FightParams {
    let base = crate::model::WeaponBase::from_data("latron_prime_incarnon", false, &[]);
    let refs: Vec<&crate::model::ModDef> = Vec::new();
    let panel = crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
    let mut arena = crate::arena::Arena::training(10.0);
    // A SQUARE around the aimed body at 0.6 m, which is just over a body's
    // width — the packing a reflected projectile can actually travel in.
    let side = (n as f64).sqrt().ceil() as i32;
    let mut at = Vec::with_capacity(n);
    'fill: for j in -side..=side {
        for i in -side..=side {
            if i == 0 && j == 0 {
                continue;
            }
            at.push(crate::rules::space::Vec2::new(
                i as f64 * 0.6,
                arena.target_at.y + j as f64 * 0.6,
            ));
            if at.len() == n {
                break 'fill;
            }
        }
    }
    arena.others = at
        .into_iter()
        .map(|p| crate::formation::FoeSpec {
            id: String::new(),
            params: Foe::training_dummy(),
            body_parts: BodyPart::humanoid(),
            at: p,
        })
        .collect();
    let mut p = FightParams::from_panel(&panel, &arena, &crate::data::arcanes::ArcaneFx::none());
    // A BODY SHOT EVERY TIME on the aimed body — `body_parts` is where
    // the scenario's headshot rate lives, so one part with no head is a
    // player who never lands one. The OTHER bodies keep their heads, which
    // is what leaves the bounce as the only head in the fight.
    p.body_parts = vec![BodyPart {
        name: "body".into(),
        aim_weight: 1.0,
        multiplier: 1.0,
        is_head: false,
        is_weak_point: false,
        crit_bonus: false,
    }];
    p.ricochet = Some(crate::model::Ricochet {
        bounces: 5,
        headshot_chance: head_chance,
        range_m: f64::INFINITY,
    });
    p
}

fn single_part(part: BodyPart) -> FightParams {
    FightParams {
        body_parts: vec![part],
        ..no_status()
    }
}

/// A target that is nothing but head, so every pellet headshots.
/// THE WINDOWS A FIGHT OPENS WITH — every clock shut, which is what "earned"
/// means. A test that wants one open sets that one field and says why.
pub(super) fn windows_for(_p: &FightParams) -> crate::fight::state::CardWindows {
    crate::fight::state::CardWindows {
        fire_rate_after_reload: 0.0,
        crit_on_headshot: 0.0,
        crit_on_headshot_stacks: Vec::new(),
        weakpoint_buff: f64::NEG_INFINITY,
        headshot_times: Vec::new(),
        streak: f64::NEG_INFINITY,
        base_damage_after_reload: 0.0,
        base_damage_eximus: 0.0,
    }
}

pub(super) fn all_head() -> Vec<BodyPart> {
    vec![BodyPart {
        name: "head".into(),
        aim_weight: 1.0,
        multiplier: 1.0,
        is_head: true,
        is_weak_point: true,
        crit_bonus: false,
    }]
}

pub(super) fn mono_body(multiplier: f64) -> Vec<BodyPart> {
    vec![BodyPart {
        name: "body".into(),
        aim_weight: 1.0,
        multiplier,
        is_head: false,
        is_weak_point: false,
        crit_bonus: false,
    }]
}

/// A lingering FIELD fixture in Torid's shape: 40 Toxin a tick, 1 tick/s
/// for 10 s, its own 15% / 2.0x crit and 25% status, and the Torid's
/// anomalous CO eligibility. `stacking` picks the branch (MEASUREMENTS M13
/// measured `stack`; `refresh` is the other weapon-data option).
fn cloud(stacking: crate::model::FieldStacking) -> crate::build::loadout::ResolvedLingering {
    let mut damage = DamageVector::default();
    damage.set(DamageType::Toxin, 40.0);
    crate::build::loadout::ResolvedLingering {
        damage,
        modified_base: 40.0,
        crit_chance: 0.0,   // crit off: tick COUNTS are the assertion
        crit_damage: 2.0,
        status_chance: 0.0, // status off: no DoT confounding the total
        base_crit_chance: 0.15,
        base_crit_damage: 2.0,
        base_status_chance: 0.25,
        tick_rate: 1.0,
        duration_seconds: 10.0,
        // A CLOUD, in all three senses the Grimoire's orb is not one: it
        // opens with the impact (M13), it forces nothing, and it cannot
        // find a head. Stated rather than defaulted, because these are the
        // values the ten-tick arithmetic below rests on.
        first_tick_delay_seconds: 0.0,
        forced_procs: crate::rules::damage::ForcedProcs::from_types([]),
        radius_m: 3.0,
        falloff_start_m: 0.0,
        falloff_reduction: 1.0,
        stacking,
        takes_condition_overload: true,
    }
}

/// A GRIMOIRE-SHAPED ORB: 280 a strike, one a second, six of them over a
/// six second fuse, then a 200 detonation. Geometry and a clock — what it
/// DEALS is the attack's own damage and radial, which `from_panel` derives.
fn orb_spec() -> crate::build::loadout::ResolvedOrb {
    crate::build::loadout::ResolvedOrb {
        fuse_seconds: 6.0,
        strike_interval_seconds: 1.0,
        strike_radius_m: 6.0,
        launch_speed_mps: 6.0,
        speed_after_contact_mps: 2.0,
        // ONE BODY A STRIKE unless a test says otherwise: the chain is what
        // `chain_bodies` above 1 buys, and a count assertion should not be
        // reading a crowd it did not ask for.
        chain_bodies: 1,
        chain_range_m: 6.0,
        chain_damage_per_hop: 1.0,
        // INSTANT unless a test says otherwise: an animation would move
        // every strike time in the tests below by a fraction of a second
        // and none of them are about it.
        throw_seconds: 0.0,
        recovery_seconds: 0.0,
        // AIMED unless a test says otherwise, so a count assertion is not
        // reading a head multiplier it did not ask for.
        unaimed_headshot_chance: None,
    }
}

/// The part an orb strike settles — the attack's own hit, in the shape a
/// timed instance is resolved from. `from_panel` builds this from the
/// resolved panel; a unit fixture states it.
fn orb_part(damage: f64) -> crate::build::loadout::ResolvedLingering {
    let mut v = DamageVector::default();
    v.set(DamageType::Electricity, damage);
    crate::build::loadout::ResolvedLingering {
        damage: v,
        modified_base: damage,
        crit_chance: 0.0,
        crit_damage: 2.0,
        status_chance: 0.0,
        base_crit_chance: 0.2,
        base_crit_damage: 2.0,
        base_status_chance: 0.26,
        tick_rate: 1.0,
        duration_seconds: 0.0,
        first_tick_delay_seconds: 0.0,
        forced_procs: crate::rules::damage::ForcedProcs::from_types([]),
        radius_m: f64::INFINITY,
        falloff_start_m: f64::INFINITY,
        falloff_reduction: 0.0,
        stacking: crate::model::FieldStacking::Stack,
        takes_condition_overload: false,
    }
}

/// A THROWER: one orb, one target, nothing else in the way.
fn orb_thrower() -> FightParams {
    FightParams {
        // AN ORB ATTACK DEALS NOTHING ON ARRIVAL. Its `damage:` is what a
        // STRIKE deals, delivered six times by the orb, so the pellet loop
        // settles nothing at all.
        damage: DamageVector::default(),
        // HELD: it stops where it touches. Every fixture below but the
        // drift one uses this, because a moving orb changes WHO is in
        // reach, and a test about the clock, the crit or the forced proc
        // should not be reading geometry it did not ask about.
        orb: Some(crate::build::loadout::ResolvedOrb {
            speed_after_contact_mps: 0.0,
            ..orb_spec()
        }),
        orb_strike: Some(orb_part(280.0)),
        orb_blast: None,
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        arcane: ArcaneFx::none(),
        body_parts: vec![BodyPart {
            name: "body".into(),
            aim_weight: 1.0,
            multiplier: 1.0,
            is_head: false,
            is_weak_point: false,
            crit_bonus: false,
        }],
        magazine_size: 1.0,
        reload_seconds: 999.0, // exactly one orb inside the window
        infinite_reserve: false,
        reserve_ammo: 0.0,
        duration_seconds: 20.0,
        ..no_status()
    }
}

/// A METER THROWS ON A CLOCK, and the clock is the whole difference between
/// what a Tome's other form is worth and what it is worth in a fight.
///
/// 45 seconds a throw, opening FULL — so a 180 s engagement with nothing
/// else happening gets the opening orb plus one every 45 s after it. The
/// count is the assertion because it is what the meter IS; the damage
/// follows from it.
fn metered() -> FightParams {
    FightParams {
        meter: Some(crate::build::loadout::ResolvedMeter {
            seconds_to_fill: 45.0,
            seconds_per_hit: 1.0,
            seconds_per_ammo_pickup: 10.0,
        }),
        duration_seconds: 180.0,
        ..orb_thrower()
    }
}

fn frail_target(mode: TargetMode, armor: f64, overguard: f64) -> Foe {
    Foe {
        name: "test target".into(),
        spectral: None,
        guardian_aura: false,
        base_level: 1,
        level: 1,
        base_health: 50.0, // below the weakest possible shot (75)
        base_armor: armor,
        base_overguard: overguard,
        base_affinity: 0.0,
        base_shield: 0.0,
        health_curve: crate::rules::scaling::health::UNAFFILIATED,
        shield_curve: crate::rules::scaling::shield::GRINEER,
        attenuation: None,
        stack_caps: None,
        cannot_be_frozen: false,
        steel_path: false,
        eximus: false,
        can_be_eximus: false,
        status_immunities: Vec::new(),
        faction: crate::model::Faction::Unknown,
        type_mods: crate::data::factions::Columns::NEUTRAL,
        faction_bracket_multiplier: 1.0,
        mode,
    }
}

/// A radial params fixture: the direct hit is inert (no damage roll
/// noise, no status), so everything the run reports comes from the
/// explosion.
fn radial_only(radial: crate::build::loadout::ResolvedRadial) -> FightParams {
    FightParams {
        damage: DamageVector::default(),
        radial: Some(radial),
        // No arcane: `FightParams::default()` carries Enervate, whose FLAT
        // crit chance reaches the explosion (absolute sources land on every
        // stage) - real behaviour, but not what this fixture is isolating.
        arcane: ArcaneFx::none(),
        status_chance: 0.0,
        base_status_chance: 0.0,
        base_crit_chance: 0.0,
        forced_procs: Vec::new(),
        ..FightParams::default()
    }
}

fn radial_of(status_chance: f64, crit_chance: f64) -> crate::build::loadout::ResolvedRadial {
    let mut damage = DamageVector::default();
    damage.set(DamageType::Heat, 300.0);
    crate::build::loadout::ResolvedRadial {
        blast_kind: crate::model::BlastKind::Contact,
        damage,
        modified_base: 300.0,
        crit_chance,
        crit_damage: 2.0,
        base_crit_chance: crit_chance,
        base_crit_damage: 2.0,
        status_chance,
        base_status_chance: status_chance,
        radius_m: 2.0,
        falloff_start_m: 0.0,
        falloff_reduction: 0.2,
        forced_procs: Default::default(),
        takes_condition_overload: false,
        takes_multishot: true,
        co_base: crate::model::CoBase::whole_for(crate::model::CoStage::Radial),
    }
}

/// EVERY buff the roster offers must be READ by `apply_buff_config`.
///
/// A card whose setting reaches nothing is the failure mode this whole
/// area keeps producing: `buff_roster` (what exists), `enumerate_buffs`
/// (what is drawn) and `apply_buff_config` (what is obeyed) are three
/// lists, and Deadly Efficiency was in the first two and missing from the
/// third — so its card was drawn, set, and dropped, for as long as it has
/// existed. Nothing about the UI could reveal that: a knob that does
/// nothing looks exactly like a knob whose buff is not up.
///
/// The check is generic on purpose. It does not name the fields a buff
/// writes into — it sets one id at a time and asserts the params CHANGED,
/// so a buff added later is covered without anyone remembering to come
/// back here.
/// ONE PARAMS CARRYING EVERY CONFIGURABLE BUFF AT ONCE — shared by the two
/// ratchets below: is every card READ, and can every card be DENIED.
fn every_buff_params() -> FightParams {
    use crate::model::TimedBuff;
    use crate::model::StackSpec;
    let stack = |per_stack: f64, earned_on: &'static str| StackSpec {
        per_stack,
        max_stacks: 3,
        duration: 6.0,
        initial_stacks: 0,
        earned_on: Some(earned_on),
    };
    let timed = |value: f64| TimedBuff {
        value,
        duration: 4.0,
        initial_active: false,
    };
    FightParams {
        co_stack: Some(stack(0.2, "kill")),
        multishot_stack: Some(stack(0.3, "kill")),
        crit_chance_stack: Some(stack(0.1, "headshot_kill")),
        stacking_buffs: vec![crate::model::StackingBuff {
            id: "on_plain_hit_damage",
            trigger: crate::model::BuffTrigger::PlainHit,
            grant: crate::model::BuffGrant::BaseDamage,
            chance: 1.0,
            decay: crate::model::BuffDecay::LoseOneAndReset,
            per_stack: 4.0,
            max_stacks: 3,
            duration: 10.0,
            initial_stacks: 0,
            stacks_per_trigger: 1,
            per_shell: false,
            cleared_by: crate::model::ClearedBy::Nothing,
            card_opens_full: false,
        }, crate::model::StackingBuff {
            id: "on_headshot_reload_speed",
            trigger: crate::model::BuffTrigger::Headshot,
            grant: crate::model::BuffGrant::ReloadSpeed,
            chance: 1.0,
            decay: crate::model::BuffDecay::LoseOneAndReset,
            per_stack: 0.1,
            max_stacks: 3,
            duration: 6.0,
            initial_stacks: 0,
            stacks_per_trigger: 1,
            per_shell: false,
            cleared_by: crate::model::ClearedBy::Nothing,
            card_opens_full: false,
        }],
        crit_chance_on_headshot: Some(timed(0.5)),
        crit_damage_on_kill: Some(timed(0.6)),
        fire_rate_on_reload: Some(timed(0.7)),
        base_damage_on_reload: Some(timed(0.8)),
        // Both halves, for the same reason the replay fixture carries
        // them: the tendril card exists only where a mod reads the count.
        tendril_max: 4,
        crit_chance_per_tendril: 0.1,
        kill_streak_summon: Some(crate::model::KillStreakSummonSpec {
            kills: 3,
            kill_window_seconds: 2.0,
            duration_seconds: 6.0,
            magazine_multiplier: 2.0,
            fire_rate_multiplier: 1.4,
        }),
        ..FightParams::default()
    }
}

/// No-arcane, crit-off, mono-1x profile for exact payload expectations.
fn bare(forced: DamageType) -> FightParams {
    FightParams {
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        forced_procs: vec![forced],
        arcane: ArcaneFx::none(),
        body_parts: mono_body(1.0),
        ..no_status()
    }
}

// ---- System B: the faction vulnerability column ----------------------
// Keyed by `FactionDamageOverride ?? Faction`, per COMPONENT, and chosen
// by the POOL the damage lands on (docs/MECHANICS.md §8).

/// The infinite dummy, wearing one faction's column.
fn column_dummy(key: &str, overguard: f64) -> Foe {
    Foe {
        type_mods: crate::data::factions::columns_for(key),
        ..frail_target(TargetMode::InfiniteHealth, 0.0, overguard)
    }
}

/// One run's effective damage from a fixed vector against one column.
fn eff_vs(key: &str, v: DamageVector) -> f64 {
    let p = FightParams {
        damage: v,
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        arcane: ArcaneFx::none(),
        body_parts: mono_body(1.0),
        foe: column_dummy(key, 0.0),
        ..no_status()
    };
    monte_carlo(&p, 20, 3).mean_effective_damage
}

fn shielded_target(shield: f64, health: f64) -> Foe {
    Foe {
        base_shield: shield,
        base_health: health,
        ..frail_target(TargetMode::InstantRespawn, 0.0, 0.0)
    }
}

/// M100 fixture: an unmodded Laetum (64 Impact + 96 Slash) with +200% of
/// one element, +30% and +50% headshot damage, forced `element` procs, on a
/// Steel Path level 210 Corrupted Heavy Gunner — and, with `neighbours`,
/// that many more of them 2 m away. `head` aims every shot at the head.
fn m100_fixture(element: DamageType, head: bool, neighbours: usize) -> FightParams {
    let unit = crate::data::enemies::all()
        .into_iter()
        .find(|e| e.id == "corrupted_heavy_gunner")
        .expect("the roster has one");
    let target = unit
        .target_params(210, true, false, TargetMode::InfiniteHealth)
        .expect("a level 210 Steel Path unit is legal");
    let mut parts = BodyPart::humanoid();
    for p in &mut parts {
        p.aim_weight = if p.is_head == head { 1.0 } else { 0.0 };
    }
    let others = (0..neighbours)
        .map(|i| {
            let a = i as f64 * std::f64::consts::TAU / neighbours as f64;
            crate::formation::FoeSpec {
                id: String::new(),
                params: target.clone(),
                body_parts: BodyPart::humanoid(),
                at: crate::rules::space::Vec2::new(2.0 * a.cos(), 2.0 * a.sin()),
            }
        })
        .collect();
    FightParams {
        foe: target,
        others,
        body_parts: parts,
        damage: DamageVector::new()
            .with(DamageType::Impact, 64.0)
            .with(DamageType::Slash, 96.0),
        dot_modified_base: Some(160.0),
        headshot_damage_bonus: 0.5,
        // THE VALENCE SHAPE: an added element in the element's own bracket.
        arcane: crate::data::arcanes::ArcaneFx {
            headshot_multiplier_bonus: 0.3,
            added_elements: vec![(element, 2.0)],
            ..ArcaneFx::none()
        },
        // Long enough for a Toxin tick (+1 s) and a Blast fuse (1.5 s), and
        // a second shot: a neighbour's ticks settle when the next shot does.
        fire_rate: 1.0,
        duration_seconds: 1.6,
        ..bare(element)
    }
}

/// The first tick each body pops from `element`, rounded as the game draws it.
fn m100_first_ticks(p: &FightParams, seed: u64, element: DamageType) -> Vec<Option<f64>> {
    let mut first = vec![None; p.others.len() + 1];
    for e in record(p, seed, 0.0, f64::INFINITY, 10_000, 0).events() {
        if let (crate::record::Kind::Damage(d), Some(s)) = (&e.kind, e.subject) {
            if d.dtype == element && d.origin == crate::record::Origin::Status {
                first[s as usize].get_or_insert(d.effective.round());
            }
        }
    }
    first
}
