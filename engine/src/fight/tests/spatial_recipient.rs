// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use crate::{formation::FoeSpec, rules::space::Vec2};

#[test]
fn spatial_recipient_refreshes_body_and_mode() {
    let mut primary = FightParams::default();
    primary.gauge_charging_body_parts = primary.body_parts.clone();
    let mut foe = primary.foe.clone();
    foe.level += 5;
    primary.others.push(FoeSpec {
        id: "other".into(),
        params: foe,
        body_parts: mono_body(2.),
        at: Vec2::new(0., 5.),
    });
    let mut alternate = primary.clone();
    alternate.others[0].params.level += 3;
    alternate.others[0].at = Vec2::new(-1., 5.);
    let mut cached = None;
    for (source, index) in [(&primary, 1), (&alternate, 1), (&alternate, 0), (&primary, 1)] {
        let expected = source.body(index).unwrap();
        let view = spatial::Recipient::params(source, &mut cached, index);
        assert_eq!(view.foe.level, expected.params.level);
        assert_eq!(view.target_at, expected.at);
        assert_eq!(view.body_parts[0].multiplier, expected.body_parts[0].multiplier);
        assert!(view.gauge_charging_body_parts.is_empty());
    }
}
