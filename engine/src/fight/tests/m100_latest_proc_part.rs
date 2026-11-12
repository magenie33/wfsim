use super::*;

fn applications(dtype: DamageType, hits: &[(bool, bool)], lethal: bool, dense: bool)
    -> (FightParams, Body, GalStacks, ArcRuntime, RunResult, crate::record::Record)
{
    let mut p = m100_fixture(dtype, false, 0);
    p.foe.base_armor = 0.0;
    if lethal {
        p.foe.mode = TargetMode::InstantRespawn;
        p.foe.base_health = 1.0;
    }
    p.arcane = arc("secondary_deadhead");
    p.arcane.added_elements = vec![(dtype, 2.0)];
    p.crit_chance_stack = Some(crate::model::StackSpec {
        per_stack: 0.4, max_stacks: 5, duration: 12.0, initial_stacks: 0, earned_on: Some("headshot_kill"),
    });
    let mut body = Body { state: TargetState::spawn(&p.foe, p.target_at), debuffs: DebuffState::default(), fallen: false };
    if lethal { body.state.health = 1.0; }
    let mut gal = GalStacks::default();
    let mut arc = ArcRuntime::init(&p);
    let mut r = RunResult::default();
    let mut rec = crate::record::Record::window(0.0, 10.0, 10000, 0);
    if dense {
        for _ in 0..40 {
            body.debuffs.push_dot(Dot {
                owner: Seat::WIELDER, next_tick: 5.0, ticks_left: 6, cause: u32::MAX,
                frozen: 0.0, landing: ProcLanding::default(), bracket: 1.0,
                depth: DEPTH_PROC, unit: 0.0, dtype: DamageType::Toxin,
                ignores_armor: false, seed: u32::MAX,
            });
        }
    }
    let mut rng = Rng::new(100);
    let cd = crate::rules::damage::quantize_base_crit_damage(2.2);
    for &(head, crit) in hits {
        let part = if head { all_head().remove(0) } else { mono_body(1.0).remove(0) };
        let mit = body.debuffs.mitigation(0.0, 1.0, 0.0, 0.0);
        settle_procs(vec![dtype], Seat::WIELDER, 0.0, InstanceScale {
            mb_live: 160.0,
            crit_multiplier: if crit { if head { 2.0 * cd } else { cd } } else { 1.0 },
            part_factor: if head { 5.4 } else { 1.0 },
            landing: ProcLanding::at(if head { 1.8 } else { 1.0 }, &part),
            attrition: 1.0, xh_bracket: 1.0, status_damage_live: 0.0,
        }, &mut body.debuffs, &mut gal, &mut arc, &mut body.state, &p, &p,
        &mit, &mut r, &mut rec, &mut rng, &p.foe, DEPTH_PROC);
    }
    (p, body, gal, arc, r, rec)
}

#[test]
fn m100_mixed_body_head_and_critical_stacks_match_all_six_readings() {
    let b = (false, false);
    let h = (true, false);
    let c = (true, true);
    for dtype in [DamageType::Electricity, DamageType::Gas] {
        for dense in [false, true] {
            for (hits, expected) in [
                ([b, b, c], 11148.0), ([b, b, h], 3200.0), ([h, b, b], 1778.0),
                ([c, h, b], 7249.0), ([h, h, b], 2834.0), ([b, h, h], 5100.0),
            ] {
                let (p, mut body, mut gal, mut arc, mut r, mut rec) = applications(dtype, &hits, false, dense);
                if dense {
                    assert!(body.debuffs.dots.len() > TICK_QUEUE_MIN);
                }
                let mut w = windows_for(&p);
                process_ticks(&mut w, &mut body, &mut gal, &mut arc, 0.01, &p, &p,
                    &mut r, &mut rec, &mut Rng::new(1), &p.foe, 0);
                assert_eq!(r.tally.effective().round(), expected, "{dtype:?}, {hits:?}, dense={dense}");
                let damage: Vec<_> = rec.events().iter().filter_map(|e| match &e.kind {
                    crate::record::Kind::Damage(d) => Some(d), _ => None,
                }).collect();
                assert!(damage.iter().all(|d| d.head == hits[2].0));
                let recorded: f64 = damage.iter().map(|d| d.effective).sum();
                assert!((recorded - r.tally.effective()).abs() < 1e-8);
            }
        }
    }
}

#[test]
fn m100_precision_flags_do_not_depend_on_the_damage_multiplier() {
    for dtype in [DamageType::Electricity, DamageType::Gas] {
        for head in [false, true] {
            let (p, mut body, mut gal, mut arc, mut r, mut rec) = applications(dtype, &[], true, false);
            let part = BodyPart { name: "weak point".into(), multiplier: 1.0,
                aim_weight: 1.0, is_head: head, is_weak_point: true, crit_bonus: false };
            body.debuffs.push_dot(Dot {
                owner: Seat::WIELDER, next_tick: 0.0, ticks_left: 1, cause: u32::MAX,
                frozen: 10.0, landing: ProcLanding::at(1.0, &part), bracket: 1.0,
                depth: DEPTH_PROC, unit: 0.0, dtype,
                ignores_armor: false, seed: u32::MAX,
            });
            process_ticks(&mut windows_for(&p), &mut body, &mut gal, &mut arc, 0.01,
                &p, &p, &mut r, &mut rec, &mut Rng::new(1), &p.foe, 0);
            assert_eq!(r.kills, 1);
            assert_eq!(arc.states[0].stacks, 1, "{dtype:?}, head={head}");
        }
    }
}

#[test]
fn m100_other_status_kills_do_not_inherit_precision_flags() {
    for dtype in [DamageType::Slash, DamageType::Toxin, DamageType::Heat] {
        let (p, mut body, mut gal, mut arc, mut r, mut rec) = applications(dtype, &[(true, false)], true, false);
        let mut w = windows_for(&p);
        process_ticks(&mut w, &mut body, &mut gal, &mut arc, 1.01, &p, &p,
            &mut r, &mut rec, &mut Rng::new(1), &p.foe, 0);
        assert_eq!(r.kills, 1, "{dtype:?}");
        assert_eq!(arc.states[0].stacks, 0, "{dtype:?}");
        assert!(w.crit_on_headshot_stacks.is_empty());
    }
}

#[test]
fn m100_a_body_break_proc_makes_the_common_tick_a_body_kill() {
    let (mut p, mut body, mut gal, mut arc, mut r, mut rec) = applications(DamageType::Electricity, &[(true, false)], false, false);
    p.foe.base_overguard = 1_000_000.0;
    p.foe.base_health = 5000.0;
    p.foe.mode = TargetMode::InstantRespawn;
    body.state.health = 5000.0;
    body.debuffs.disrupt.push(6.0);
    push_break_proc(&mut body.debuffs, Seat::WIELDER, &p, 0.0, BrokenPool::Overguard,
        &p, &p.foe, &mut body.state, &mut gal, &mut arc, &mut r, &mut rec, &mut Rng::new(9), 0.0);
    process_ticks(&mut windows_for(&p), &mut body, &mut gal, &mut arc, 0.01,
        &p, &p, &mut r, &mut rec, &mut Rng::new(1), &p.foe, 0);
    assert_eq!(r.kills, 1);
    assert_eq!(arc.states[0].stacks, 0);
    assert!(rec.events().iter().any(|e| matches!(&e.kind,
        crate::record::Kind::Damage(d) if d.killed && !d.head && e.cause.is_none())));
}

#[test]
fn m100_the_latest_application_part_decides_the_lethal_tick() {
    for dtype in [DamageType::Electricity, DamageType::Gas] {
        for last_head in [false, true] {
            let hits = [(true, false), (true, true), (last_head, false)];
            let (p, mut body, mut gal, mut arc, mut r, mut rec) = applications(dtype, &hits, true, false);
            let mut w = windows_for(&p);
            process_ticks(&mut w, &mut body, &mut gal, &mut arc, 0.01, &p, &p,
                &mut r, &mut rec, &mut Rng::new(1), &p.foe, 0);
            assert_eq!(r.kills, 1);
            assert_eq!(arc.states[0].stacks, u32::from(last_head), "{dtype:?}, latest head={last_head}");
            assert_eq!(w.crit_on_headshot_stacks.len(), usize::from(last_head));
        }
    }
}
