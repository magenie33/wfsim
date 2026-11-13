use super::*;

struct Probe {
    p: FightParams,
    body: Body,
    gal: GalStacks,
    arc: ArcRuntime,
    r: RunResult,
    rec: crate::record::Record,
    rng: Rng,
    w: CardWindows,
}

impl Probe {
    fn new() -> Self {
        let mut foe = Foe::training_dummy();
        foe.base_overguard = 100_000.0;
        foe.base_shield = 100_000.0;
        let p = FightParams { arcane: ArcaneFx::none(), foe, ..FightParams::default() };
        let mut state = TargetState::spawn(&p.foe, p.target_at);
        state.overguard = 0.0;
        state.shield = 0.0;
        Self { body: Body { state, debuffs: DebuffState::default(), fallen: false },
            gal: GalStacks::default(), arc: ArcRuntime::init(&p), r: RunResult::default(),
            rec: crate::record::Record::window(0.0, 30.0, 10000, 0),
            rng: Rng::new(41), w: windows_for(&p), p }
    }

    fn break_pool(&mut self, pool: BrokenPool, at: f64) {
        push_break_proc(&mut self.body.debuffs, Seat::WIELDER, &self.p, at, pool,
            &self.p, &self.p.foe, &mut self.body.state, &mut self.gal, &mut self.arc,
            &mut self.r, &mut self.rec, &mut self.rng, 0.0, 0);
    }

    fn electric(&mut self, head: bool, at: f64) {
        let part = if head { all_head().remove(0) } else { mono_body(1.0).remove(0) };
        let mit = self.body.debuffs.mitigation(at, 1.0, 0.0, 1.0);
        settle_procs(vec![DamageType::Electricity], Seat::WIELDER, at, 0, InstanceScale {
            mb_live: 160.0, crit_multiplier: 1.0, part_factor: if head { 5.4 } else { 1.0 },
            landing: ProcLanding::at(if head { 1.8 } else { 1.0 }, &part),
            attrition: 1.0, xh_bracket: 1.0, status_damage_live: 0.0,
        }, &mut self.body.debuffs, &mut self.gal, &mut self.arc, &mut self.body.state,
            &self.p, &self.p, &mit, &mut self.r, &mut self.rec, &mut self.rng, &self.p.foe, DEPTH_PROC);
    }

    fn tick(&mut self, until: f64) {
        process_ticks(&mut self.w, &mut self.body, &mut self.gal, &mut self.arc, until,
            &self.p, &self.p, &mut self.r, &mut self.rec, &mut self.rng, &self.p.foe, 0);
    }
}

#[test]
fn m8_break_uses_the_breaking_weapons_normal_electric_modifiers_and_duration() {
    for pool in [BrokenPool::Overguard, BrokenPool::Shield] {
        let mut q = Probe::new();
        q.body.debuffs.disrupt = vec![6.0; 10];
        q.p.elem_dot_bonus = vec![(DamageType::Electricity, 2.65)];
        q.p.status_duration_multiplier = 2.0;
        q.p.status_damage_multiplier = 1.6;
        q.p.faction_multiplier = 1.55;
        q.break_pool(pool, 0.0);
        let dot = &q.body.debuffs.dots[0];
        assert_eq!(dot.ticks_left, 12);
        assert_eq!(dot.bracket, 2.65);
        assert_eq!(dot.depth, DEPTH_PROC);
        assert_eq!(dot.landing, ProcLanding::default());
        let max = match pool { BrokenPool::Overguard => q.p.foe.overguard(), BrokenPool::Shield => q.p.foe.max_shield() };
        let source = 0.005 * 10.0 * max;
        let parent = source * 1.6 * 1.55;
        let expected = parent + (0.5 * source * 1.6_f64.powi(2) * 1.55_f64.powi(2)
            + 0.5 * 1.6 * 1.55) * 2.65;
        assert!((q.r.tally.effective() - parent).abs() < 1e-8);
        q.tick(0.01);
        assert!((q.r.tally.effective() - expected).abs() < 1e-7,
            "{} != {expected}", q.r.tally.effective());
    }
}

#[test]
fn m8_break_snapshots_the_magnetic_count_and_keeps_the_common_electric_clock() {
    let mut q = Probe::new();
    q.body.debuffs.disrupt = vec![6.0; 10];
    q.break_pool(BrokenPool::Overguard, 0.0);
    let parent = q.r.tally.effective();
    let frozen = q.body.debuffs.dots[0].frozen;
    q.body.debuffs.disrupt = vec![6.0; 1];
    q.tick(0.01);
    let first = q.r.tally.effective();
    q.body.debuffs.disrupt = vec![6.0; 10];
    q.tick(1.01);
    assert_eq!(q.body.debuffs.dots[0].frozen, frozen);
    assert!((q.r.tally.effective() - parent - 2.0 * (first - parent)).abs() < 1e-8);
    q.break_pool(BrokenPool::Overguard, 1.1);
    assert_eq!(q.body.debuffs.dots[1].next_tick, q.body.debuffs.dots[0].next_tick);
}

#[test]
fn m8_body_break_resets_the_pile_and_a_new_head_electric_proc_restores_it() {
    for restore_head in [false, true] {
        let mut q = Probe::new();
        q.p.weakness_bonus = Some(0.5);
        q.p.foe.magnetic_break_proxy = Some(crate::rules::status::HitProxy {
            weakness: 1.0, damage_multiplier: Some(2.0),
        });
        q.p.arcane = arc("secondary_deadhead");
        q.arc = ArcRuntime::init(&q.p);
        q.body.debuffs.disrupt = vec![6.0; 10];
        q.electric(true, 0.0);
        q.break_pool(BrokenPool::Overguard, 0.0);
        assert_eq!(q.body.debuffs.proc_landing(&q.body.debuffs.dots[0]), ProcLanding::default());
        if restore_head { q.electric(true, 0.0); }
        q.p.foe.mode = TargetMode::InstantRespawn;
        q.body.state.health = 1.0;
        q.tick(0.01);
        assert_eq!(q.r.kills, 1);
        assert_eq!(q.arc.states[0].stacks, u32::from(restore_head));
        let events: Vec<_> = q.rec.events().iter().filter_map(|e| match &e.kind {
            crate::record::Kind::Damage(d) => Some(d), _ => None,
        }).collect();
        assert_eq!(events.len(), 2);
        assert!(!events[0].head);
        assert_eq!(events[1].head, restore_head);
    }
}

#[test]
fn m8_fixed_target_measured_ticks_follow_the_declared_weapon_and_proxy() {
    let spec = crate::data::enemies::all().into_iter()
        .find(|enemy| enemy.id == "corrupted_heavy_gunner").unwrap();
    let weapon = crate::model::WeaponBase::from_data("laetum", true, &["laetum_caput_mortuum"]);
    let panel = crate::build::loadout::resolve(&weapon, &[], crate::model::StackPolicy::AssumedMax);
    for aim in ["body", "head"] {
        for (electricity, measured) in [(1.0, 3455.0), (2.65, 9154.0)] {
            let mut arena = crate::arena::Arena::training(10.0);
            arena.target = spec.target_params(50, true, true, TargetMode::InfiniteHealth).unwrap();
            arena.body_parts = spec.aim_parts(&[(aim, 1.0)]).unwrap();
            let mut q = Probe::new();
            q.p = FightParams::from_panel(&panel, &arena, &arc("secondary_deadhead"));
            q.arc = ArcRuntime::init(&q.p);
            assert_eq!(q.p.weakness_bonus, Some(0.5));
            q.p.elem_dot_bonus = vec![(DamageType::Magnetic, 3.0), (DamageType::Electricity, electricity)];
            q.body.state = TargetState::spawn(&q.p.foe, q.p.target_at);
            q.body.state.overguard = 0.0;
            q.body.state.shield = 0.0;
            q.body.debuffs.canticle_armor_strip = 1.0;
            q.body.debuffs.disrupt = vec![6.0; 10];
            q.break_pool(BrokenPool::Overguard, 0.0);
            assert_eq!(q.body.debuffs.dots[0].frozen, 3454.0);
            assert_eq!(q.body.debuffs.dots[0].landing, ProcLanding::default());
            let parent = q.r.tally.effective();
            assert_eq!(parent, 6908.0);
            q.tick(0.01);
            assert_eq!((q.r.tally.effective() - parent).round(), measured, "{aim}, Electricity{electricity}");
        }
    }
}

#[test]
fn m8_break_electricity_uses_the_normal_area_proc_path() {
    let mut q = Probe::new();
    q.body.debuffs.disrupt = vec![6.0; 10];
    q.break_pool(BrokenPool::Overguard, 0.0);
    assert_eq!(q.r.procs, 1);
    assert_eq!(q.body.debuffs.area_out.len(), 1);
}

#[test]
fn m8_forced_break_electricity_uses_the_normal_immunity_filter() {
    let mut q = Probe::new();
    q.body.debuffs.disrupt = vec![6.0; 10];
    q.p.foe.status_immunities = vec![DamageType::Electricity];
    q.break_pool(BrokenPool::Overguard, 0.0);
    assert!(q.body.debuffs.dots.is_empty());
    assert!(q.body.debuffs.area_out.is_empty());
    assert_eq!(q.r.procs, 0);
    assert_eq!(q.r.tally.effective(), 5000.0);
}

#[test]
fn m8_the_breaking_context_does_not_come_from_older_magnetic_stacks() {
    let mut q = Probe::new();
    q.p.status_duration_multiplier = 3.0;
    q.body.debuffs.disrupt = vec![18.0; 10];
    q.p.status_duration_multiplier = 0.5;
    q.p.elem_dot_bonus = vec![(DamageType::Electricity, 2.65)];
    q.break_pool(BrokenPool::Overguard, 0.0);
    assert_eq!(q.body.debuffs.dots[0].ticks_left, 3);
    assert_eq!(q.body.debuffs.dots[0].bracket, 2.65);
    assert_eq!(q.body.debuffs.disrupt, vec![18.0; 10]);
}

#[test]
fn m8_break_uses_the_receiving_bodys_pool_and_ignores_weapon_base_damage() {
    let mut q = Probe::new();
    q.body.debuffs.disrupt = vec![6.0; 10];
    q.p.dot_modified_base = Some(1e12);
    let mut receiver = q.p.foe.clone();
    receiver.base_overguard = 1000.0;
    push_break_proc(&mut q.body.debuffs, Seat::WIELDER, &q.p, 0.0,
        BrokenPool::Overguard, &q.p, &receiver, &mut q.body.state, &mut q.gal,
        &mut q.arc, &mut q.r, &mut q.rec, &mut q.rng, 0.0, 0);
    assert_eq!(q.body.debuffs.dots[0].frozen, 0.0025 * 10.0 * receiver.overguard());
}

#[test]
fn m8_mixed_weapon_and_break_seeds_pay_one_accumulator_at_the_latest_anchor() {
    for restore_head in [false, true] {
        let mut q = Probe::new();
        q.body.debuffs.disrupt = vec![6.0; 10];
        q.electric(true, 0.0);
        q.break_pool(BrokenPool::Overguard, 0.0);
        if restore_head { q.electric(true, 0.0); }
        let seeds = 432.0 * if restore_head { 2.0 } else { 1.0 }
            + 0.0025 * 10.0 * q.p.foe.overguard();
        let expected = 0.005 * 10.0 * q.p.foe.overguard()
            + (seeds + 0.5) * if restore_head { 1.8 } else { 1.0 };
        q.tick(0.01);
        assert!((q.r.tally.effective() - expected).abs() < 1e-8);
        assert_eq!(q.r.dot_ticks, 1);
    }
}

#[test]
fn m8_break_packet_is_immediate_while_its_proc_waits_for_the_shared_clock() {
    let mut q = Probe::new();
    q.electric(true, 0.0);
    q.tick(0.01);
    let old_seed = q.body.debuffs.dots[0].frozen;
    let before = q.r.tally.effective();
    q.body.debuffs.disrupt = vec![6.0; 10];
    q.p.status_damage_multiplier = 1.9;
    q.break_pool(BrokenPool::Overguard, 0.25);
    assert_eq!(q.r.tally.effective() - before, 9500.0);
    assert_eq!(q.body.debuffs.dots[0].frozen, old_seed);
    assert!((q.body.debuffs.dots[1].frozen - 9025.0).abs() < 1e-9);
    assert_eq!(q.body.debuffs.dots[1].next_tick, 1.0);
    let after = q.r.tally.effective();
    q.tick(0.26);
    assert_eq!(q.r.tally.effective(), after);
}

#[test]
fn m8_break_packet_kill_earns_one_kill_and_leaves_no_proc_on_the_fresh_spawn() {
    for (id, expected) in [("primary_deadhead", 0), ("primary_merciless", 1)] {
        let mut q = Probe::new();
        q.p.foe.mode = TargetMode::InstantRespawn;
        q.body.state.health = 1.0;
        q.p.arcane = arc(id);
        q.arc = ArcRuntime::init(&q.p);
        q.body.debuffs.disrupt = vec![6.0; 10];
        q.break_pool(BrokenPool::Overguard, 0.0);
        assert_eq!(q.r.kills, 1);
        assert_eq!(q.arc.states[0].stacks, expected, "{id}");
        assert!(q.body.debuffs.dots.is_empty());
        q.tick(1.01);
        assert_eq!(q.r.kills, 1);
    }
}

#[test]
fn m8_break_packet_credits_the_receiving_body_and_matches_the_record() {
    let mut q = Probe::new();
    q.body.debuffs.disrupt = vec![6.0; 10];
    push_break_proc(&mut q.body.debuffs, Seat::WIELDER, &q.p, 0.0,
        BrokenPool::Overguard, &q.p, &q.p.foe, &mut q.body.state, &mut q.gal,
        &mut q.arc, &mut q.r, &mut q.rec, &mut q.rng, 0.0, 3);
    assert_eq!(q.r.taken.by_body().0[3], 5000.0);
    assert_eq!(q.r.taken.by_body().0[0], 0.0);
    let events: Vec<_> = q.rec.events().iter().filter_map(|e| match &e.kind {
        crate::record::Kind::Damage(d) => Some((e, d)), _ => None,
    }).collect();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0.subject, Some(3));
    assert_eq!(events[0].1.effective, q.r.tally.effective());
    assert_eq!(events[0].1.part.as_deref(), Some("body"));
}

#[test]
fn m8_forced_proc_side_effect_cannot_pay_the_packet_into_a_fresh_spawn() {
    let mut q = Probe::new();
    q.p.foe.mode = TargetMode::InstantRespawn;
    q.body.state.health = 1.0;
    q.p.arcane.flat_damage_on_status = 750.0;
    q.body.debuffs.disrupt = vec![6.0; 10];
    q.break_pool(BrokenPool::Overguard, 0.0);
    assert_eq!(q.r.kills, 1);
    assert_eq!(q.body.state.generation, 1);
    assert_eq!(q.body.state.health, q.p.foe.max_health());
    assert_eq!(q.r.sources.arcane_on_status, 750.0);
    assert_eq!(q.r.tally.effective(), 750.0);
    assert!(q.body.debuffs.dots.is_empty());
}
