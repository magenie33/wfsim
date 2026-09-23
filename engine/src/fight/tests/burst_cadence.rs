//! A BURST IS PLAYED ROUND BY ROUND. The Akarius Prime: two rockets 0.12 s
//! apart, then 1/3.667 s to the next pull — and a magazine of nine fires its
//! last rocket alone and reloads after that one pull's wait.
use super::*;
use crate::record::Kind;

fn trace(mag: f64) -> (Vec<f64>, Vec<f64>) {
    let base = crate::model::WeaponBase::from_data("akarius_prime", false, &[]);
    let panel = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
    let mut p = FightParams::from_panel(&panel, &crate::arena::Arena::training(4.0), &ArcaneFx::none());
    p.magazine_size = mag;
    p.foe.base_health = 1e15;
    let rec = record(&p, 1, 0.0, 4.0, 10_000, 0);
    let (mut shots, mut reloads) = (Vec::new(), Vec::new());
    for e in rec.events() {
        match &e.kind {
            Kind::Strike { .. } => shots.push(e.t),
            Kind::ReloadStart { .. } => reloads.push(e.t),
            _ => {}
        }
    }
    (shots, reloads)
}

#[test]
fn the_akarius_fires_its_pairs_on_the_burst_delay_and_a_lone_last_rocket_alone() {
    let wait = 1.0 / 3.667;
    let (shots, reloads) = trace(8.0);
    for pull in 0..4 {
        let at = pull as f64 * (0.12 + wait);
        assert!((shots[2 * pull] - at).abs() < 1e-9, "pull {pull} starts at {at}: {shots:?}");
        assert!((shots[2 * pull + 1] - at - 0.12).abs() < 1e-9, "its second rocket: {shots:?}");
    }
    assert!((reloads[0] - (shots[7] + wait)).abs() < 1e-9, "reload one pull's wait after the 8th");

    let (shots, reloads) = trace(9.0);
    let lone = 4.0 * (0.12 + wait);
    assert!((shots[8] - lone).abs() < 1e-9, "the ninth is a pull of its own: {shots:?}");
    assert!((reloads[0] - (lone + wait)).abs() < 1e-9, "and the reload follows it: {reloads:?}");
}
