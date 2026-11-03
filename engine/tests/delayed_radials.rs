// SPDX-License-Identifier: AGPL-3.0-or-later
use wfsim_engine::{arena::Arena, build::loadout, data::arcanes::ArcaneFx, fight::{FightParams, record}, model::{StackPolicy, WeaponBase}, record::{Kind, Origin}};

fn fixture(delay: f64, duration: f64) -> FightParams {
    let direct = loadout::resolve(&WeaponBase::from_data("braton", true, &[]), &[], StackPolicy::Emergent);
    let blast = loadout::resolve(&WeaponBase::from_data("ogris", true, &[]), &[], StackPolicy::Emergent);
    let mut p = FightParams::from_panel(&direct, &Arena::training(duration), &ArcaneFx::none());
    let mut radial = blast.radial.unwrap();
    radial.delay_seconds = delay;
    radial.forced_procs = Default::default();
    radial.status_chance = 0.0;
    p.radial = Some(radial);
    p.spread = None;
    p.fire_rate = 1.0;
    p.status_chance = 0.0;
    p.base_crit_chance = 0.0;
    p.magazine_size = 100.0;
    p
}

fn contacts(p: &FightParams) -> (Vec<f64>, Vec<f64>) {
    let rec = record(p, 3, 0.0, p.duration_seconds, 10_000, 0);
    let (mut direct, mut radial) = (Vec::new(), Vec::new());
    for e in rec.events() {
        if let Kind::Damage(d) = &e.kind {
            if d.origin == Origin::Own && e.subject == Some(0) {
                if d.radial { radial.push(e.t); } else { direct.push(e.t); }
            }
        }
    }
    (direct, radial)
}

#[test]
fn explosion_delay_preserves_the_trigger_clock_and_does_not_repeat_contact() {
    let (direct, radial) = contacts(&fixture(1.0, 2.2));
    assert_eq!(direct, [0.0, 1.0, 2.0]);
    assert_eq!(radial, [1.0, 2.0]);
    let (immediate_direct, immediate_radial) = contacts(&fixture(0.0, 2.2));
    assert_eq!(immediate_direct, direct);
    assert_eq!(immediate_radial, direct);
}

#[test]
fn an_explosion_due_after_the_engagement_does_not_land_early() {
    let (direct, radial) = contacts(&fixture(1.0, 0.5));
    assert_eq!(direct, [0.0]);
    assert!(radial.is_empty());
    assert_eq!(contacts(&fixture(0.0, 0.5)).1, [0.0]);
}
