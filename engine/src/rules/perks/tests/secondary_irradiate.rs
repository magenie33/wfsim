use super::*;

const SOURCE: Option<Source> = Some(Source {
    weapon_type: 7, source_proc_type: 30, arcane_intermediate: false,
});

fn start<C>(state: &mut SecondaryIrradiate, listener: &mut Listener, victim: usize, context: C) -> Job<C> {
    state.activate(listener, victim, 10, HitClass::Ordinary, SOURCE, context).unwrap()
}

fn add(state: &mut SecondaryIrradiate, listener: &Listener, victim: usize, n: u32) {
    assert!(state.collect(listener, victim, 10, SOURCE, |stored| stored == 7, false, n));
}

#[test]
fn merged_and_separate_notifications_pay_the_same_count_once() {
    for notifications in [vec![2], vec![1, 1], vec![20], vec![1; 20]] {
        let mut state = SecondaryIrradiate::default();
        let mut listener = Listener::default();
        let first = start(&mut state, &mut listener, 0, "original projectile");
        let second = start(&mut state, &mut listener, 0, "later projectile");
        for &n in &notifications { add(&mut state, &listener, 0, n); }
        let total = notifications.iter().sum::<u32>();
        let echo = state.resume(first, true, 1.8, |context, _| {
            assert_eq!(*context, "original projectile"); 100.0
        }).unwrap();
        assert_eq!(echo.num_hits, f64::from(total));
        assert_eq!(echo.queried * echo.coefficient, 100.0 * (1.8 * f64::from(total)));
        assert_eq!(state.count(0), 0.0);
        let mut queried = false;
        assert!(state.resume(second, true, 1.8, |context, _| {
            queried = true; assert_eq!(*context, "later projectile");
        }).is_none());
        assert!(queried);
    }
}

#[test]
fn activation_preserves_collected_hits_and_resets_shared_charge() {
    let mut state = SecondaryIrradiate::default();
    let mut a = Listener::default();
    let mut b = Listener::default();
    let job = start(&mut state, &mut a, 0, 1);
    assert!(state.collect(&a, 0, 10, SOURCE, |_| true, true, 2));
    let other = start(&mut state, &mut b, 1, 2);
    assert_eq!(state.count(0), 2.0);
    assert_eq!(state.RadialDamageOnMaxRadStackHit_IsFullyCharged, None);
    assert!(state.collect(&b, 1, 10, SOURCE, |_| true, true, 3));
    assert_eq!(state.resume(job, true, 1.8, |_, charge| charge).unwrap().queried, Some(true));
    assert_eq!(state.count(1), 3.0);
    assert_eq!(state.resume(other, true, 1.8, |_, _| ()).unwrap().num_hits, 3.0);
}

#[test]
fn activation_hit_filter_does_not_restrict_installed_collector() {
    let mut state = SecondaryIrradiate::default();
    let mut listener = Listener::default();
    for class in [HitClass::Radial, HitClass::Script, HitClass::Dot] {
        assert!(state.activate(&mut listener, 0, 10, class, SOURCE, ()).is_none());
        assert!(!listener.installed());
    }
    assert!(!state.collect(&listener, 0, 10, SOURCE, |_| true, false, 1));
    let job = start(&mut state, &mut listener, 0, ());
    for _ in [HitClass::Ordinary, HitClass::Radial, HitClass::Script, HitClass::Dot] {
        add(&mut state, &listener, 0, 1);
    }
    assert_eq!(state.resume(job, true, 1.8, |_, _| ()).unwrap().num_hits, 4.0);
    assert!(listener.installed());
    add(&mut state, &listener, 0, 2);
    let next = start(&mut state, &mut listener, 0, ());
    assert_eq!(state.resume(next, true, 1.8, |_, _| ()).unwrap().num_hits, 2.0);
}

#[test]
fn low_radiation_clears_only_its_victim_and_preserves_charge() {
    let mut state = SecondaryIrradiate::default();
    let mut listener = Listener::default();
    let _job = start(&mut state, &mut listener, 0, ());
    add(&mut state, &listener, 0, 3);
    add(&mut state, &listener, 1, 5);
    assert!(state.collect(&listener, 0, 9, SOURCE, |_| true, true, 20));
    assert_eq!(state.count(0), 0.0);
    assert_eq!(state.count(1), 5.0);
    assert_eq!(state.RadialDamageOnMaxRadStackHit_IsFullyCharged, Some(false));
}

#[test]
fn source_filters_and_supplied_type_relation_are_independent() {
    let mut state = SecondaryIrradiate::default();
    let mut listener = Listener::default();
    let job = start(&mut state, &mut listener, 0, ());
    for source in [None,
        Some(Source { weapon_type: 7, source_proc_type: 5, arcane_intermediate: false }),
        Some(Source { weapon_type: 7, source_proc_type: 30, arcane_intermediate: true })]
    {
        assert!(state.activate(&mut listener, 0, 10, HitClass::Ordinary, source, ()).is_none());
        assert!(!state.collect(&listener, 0, 10, source, |_| true, false, 20));
    }
    assert!(!state.collect(&listener, 0, 10, SOURCE, |_| false, false, 20));
    let subtype = Some(Source { weapon_type: 99, ..SOURCE.unwrap() });
    assert!(state.collect(&listener, 0, 10, subtype, |stored| stored == 7, false, 2));
    assert_eq!(state.resume(job, true, 1.8, |_, _| ()).unwrap().num_hits, 2.0);
}

#[test]
fn invalid_context_returns_before_query_or_consume() {
    let mut state = SecondaryIrradiate::default();
    let mut listener = Listener::default();
    let job = start(&mut state, &mut listener, 0, ());
    add(&mut state, &listener, 0, 3);
    assert!(state.resume(job, false, 1.8, |_, _| panic!("invalid query")).is_none());
    assert_eq!(state.count(0), 3.0);
    for coefficient in [0.0, -1.8] {
        let next = start(&mut state, &mut listener, 0, ());
        let mut queried = false;
        assert!(state.resume(next, true, coefficient, |_, _| { queried = true; }).is_none());
        assert!(queried);
        assert_eq!(state.count(0), 0.0);
    }
}

#[test]
fn keyed_victim_listener_services_separate_owners() {
    let mut a = SecondaryIrradiate::default();
    let mut b = SecondaryIrradiate::default();
    let mut listener = Listener::default();
    let first = start(&mut a, &mut listener, 0, ());
    let second = start(&mut b, &mut listener, 0, ());
    add(&mut a, &listener, 0, 2);
    add(&mut b, &listener, 0, 3);
    assert_eq!(a.resume(first, true, 1.8, |_, _| ()).unwrap().num_hits, 2.0);
    assert_eq!(b.resume(second, true, 1.8, |_, _| ()).unwrap().num_hits, 3.0);
    listener.invalidate();
    assert!(!a.collect(&listener, 0, 10, SOURCE, |_| true, false, 10));
}
