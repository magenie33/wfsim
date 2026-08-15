//! The ARENA — a fight's two actors and how long they are at it.
//!
//! The sim modelled one actor for a long time: the target. Everything about
//! the player was either absent or smuggled in as a loose parameter beside the
//! weapon's numbers — an `aiming: bool` here, a Tenno the resolver reached for
//! itself there. This is the other half, stated once: somebody is holding the
//! weapon, somebody is being shot at, for this long.
//!
//! It is the SETUP, not the resolution. [`crate::dummy::DummyParams`] is what
//! you get when a build is resolved against an arena — flat, because the hot
//! loop reads it every tick. The arena is what a caller ASSEMBLES: the web api
//! builds one from the scenario JSON, the optimizer's `Scenario` embeds one so
//! its winner is scored under the fight the replay will run, and both hand the
//! same value to the same constructor. Two modules reading one scenario into
//! two lookalike shapes is how they drift a field at a time (user, 2026-08-02).

use crate::dummy::{BodyPart, TargetParams};
use crate::space::Vec2;
use crate::tenno_data::Tenno;

/// One engagement: who shoots, who is shot, from where, for how long.
#[derive(Debug, Clone)]
pub struct Arena {
    /// The player. What they ARE (a Warframe's stat block, read by the arcanes
    /// that scale off armor or energy) and what they are DOING (the state every
    /// `condition:` on a mod card asks about).
    pub tenno: Tenno,
    /// Where the player stands, and where the target stands — the fight's 2D
    /// layer (`crate::space`).
    ///
    /// Two POINTS rather than one distance, because the distance is what a
    /// second body cannot be described by. They belong to the arena rather than
    /// to the Tenno or the target for the same reason `duration_secs` does: a
    /// position is a fact about this engagement, not about who is in it, and
    /// the same Tenno stands somewhere else in the next fight.
    ///
    /// When there is more than one target this becomes a position per target
    /// and `target_at` is the one being aimed at; nothing else here changes.
    pub player_at: Vec2,
    pub target_at: Vec2,
    /// The enemy: its level, its pools, its faction, its scaling curves.
    pub target: TargetParams,
    /// The target's hitboxes — where a pellet can land and what each spot
    /// multiplies. They belong to the target, and travel with it.
    pub body_parts: Vec<BodyPart>,
    /// Engagement length. A property of the FIGHT, not of the measurement,
    /// which is why the optimizer needs it while needing neither run count nor
    /// metric.
    pub duration_secs: f64,
    /// WARFRAME ABILITY BUFFS the player brought — Roar, Eclipse, Nourish and
    /// the four elemental augments (`data/abilities/`).
    ///
    /// HERE and not on the build, because that is what they are: a thing done
    /// TO this weapon for a while, by a frame the sim does not otherwise model
    /// yet. Two builds compared under the same Roar is a comparison; one of
    /// them getting it is not — so it rides with the fight, the optimizer gets
    /// it by construction, and the BOARD sends none.
    ///
    /// Already RESOLVED (`abilities_data::resolve`): Ability Strength applied
    /// and the same-family conflicts settled, so nothing downstream can forget
    /// that two Roars do not stack.
    pub abilities: Vec<crate::abilities_data::ActiveAbility>,
}

impl Arena {
    /// How far the shot has to travel — metres from the player to the target.
    pub fn engagement_range(&self) -> f64 {
        self.player_at.distance(self.target_at)
    }

    /// The engine's own fixture: the neutral Tenno against a training dummy
    /// for 10 s. Production code never uses it — a real fight names its
    /// target — but every test that only cares about weapon numbers wants
    /// exactly this and should not have to spell it out.
    pub fn training(duration_secs: f64) -> Self {
        Self {
            tenno: crate::tenno_data::default_tenno().clone(),
            target: TargetParams::training_dummy(),
            body_parts: crate::dummy::DummyParams::humanoid_parts(),
            // CONTACT — as close as two bodies can stand (`space`), which is
            // what point blank means once they have a size at all. The fixture
            // measures weapon numbers, and any more range than this would put a
            // falloff weapon's golden value at the mercy of a distance nobody
            // asked for. Moving it here from a literal zero changed no golden
            // value: only a cone wider than 26.6 degrees can miss at 0.5 m, and
            // two entries in the roster have one.
            player_at: Vec2::ORIGIN,
            target_at: Vec2::new(0.0, crate::space::CONTACT_RANGE_M),
            duration_secs,
            // The fixture is the NEUTRAL player, and no frame is running
            // anything for them.
            abilities: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fixture is the NEUTRAL player: aiming, no frame, nothing running.
    /// A test that measures a weapon must not be measuring an invisible Tenno
    /// with 1,200 armor by accident.
    #[test]
    fn the_training_arena_is_a_neutral_player_against_a_bare_dummy() {
        let a = Arena::training(10.0);
        assert!(a.tenno.state.aiming);
        assert!(!a.tenno.state.invisible);
        assert_eq!(a.tenno.armor, 0.0);
        assert_eq!(a.target.base_armor, 0.0);
        assert_eq!(a.duration_secs, 10.0);
    }

    /// …AND IT STANDS ON THE TARGET. Every golden value in this engine was
    /// measured at point blank, so the fixture's range is 0 and a falloff
    /// weapon's fixture number cannot move when a distance is set elsewhere.
    #[test]
    fn the_training_arena_is_fought_at_contact() {
        assert_eq!(
            Arena::training(10.0).engagement_range(),
            crate::space::CONTACT_RANGE_M
        );
    }

    /// A RANGE IS TWO POINTS, and either of them may move. The web api puts the
    /// player on the origin and the target up the y axis, but nothing in the
    /// engine may depend on that — the moment there are two targets, one of
    /// them is not on an axis.
    #[test]
    fn engagement_range_is_the_distance_between_the_two_of_them() {
        let mut a = Arena::training(10.0);
        a.target_at = Vec2::new(0.0, 30.0);
        assert_eq!(a.engagement_range(), 30.0);
        a.player_at = Vec2::new(6.0, 22.0);
        assert_eq!(a.engagement_range(), 10.0);
    }
}
