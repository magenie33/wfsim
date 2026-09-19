// SPDX-License-Identifier: AGPL-3.0-or-later
//! WHAT ONE RUN CARRIES FROM SHOT TO SHOT, one struct per mechanic, so the
//! loop in `run` names a piece of state by what it belongs to.


/// DOUBLE TAP'S PILE — consecutive hits, its window, and the other form's pile
/// frozen at the last swap.
pub(super) struct DoubleTap {
    /// DOUBLE TAP: consecutive hits, and when they lapse. Reset by the clock
    /// here and never by a miss — this arena has one target that every pellet
    /// reaches, which is the card's other reset ("if the next shot does not hit
    /// an enemy") and it cannot fire.
    pub(super) hits: u32,
    pub(super) expiry: f64,
    /// …AND THE OTHER FORM'S PILE, FROZEN. Each form of a transmuting weapon
    /// keeps its own Double Tap, snapshotted at the instant a transform
    /// COMPLETES and handed back, clock and all, when that form next completes
    /// its way in (MEASUREMENTS M102): a pile at +300% with 0.5 s left comes
    /// back at +300% with 0.5 s left, and a form never yet fired starts from
    /// nothing. `(hits, seconds left)`.
    pub(super) other: (u32, f64),
}

/// THE RECHARGE METER an orb weapon throws from.
pub(super) struct Meter {
    /// THE RECHARGE METER, in seconds toward `seconds_to_fill`. It opens FULL,
    /// and that is the one place a Tome parts company with the rule next door.
    ///
    /// An INCARNON gauge opens empty because a full one is a consumable the
    /// fight has not earned (docs/BUFFS.md) — and it matters most where the
    /// gauge cannot be refilled, so a free opening magazine was pure gift. A
    /// METER is a CLOCK: it fills at one second a second whether or not anyone
    /// is shooting, so it was filling while you ran to the room, and a player
    /// walks into an engagement with it full. Opening it empty would not be
    /// conservative, it would be wrong — and it would cost the first 45 seconds
    /// of every 180-second benchmark to model a state a player is rarely in.
    pub(super) seconds: f64,
    /// …and how far the clock has already been credited, so the seconds are
    /// counted once however many times the loop looks at it.
    pub(super) clocked: f64,
}

/// THE GHOSTS a kill leaves standing, and the kills already paid in them.
pub(super) struct Ghosts {
    /// WHAT THE KILLS LEFT STANDING, one expiry apiece. It exists to be
    /// COUNTED: nothing reads it back into the fight.
    pub(super) standing: Vec<f64>,
    pub(super) kill_mark: u32,
}

/// A SYNDICATE RADIAL'S GAUGE — affinity earned, its cooldown, the kills paid.
pub(super) struct Syndicate {
    /// A SYNDICATE RADIAL's gauge, in affinity, and the same derived-from-kills
    /// trick the tendrils use: `r.kills` is maintained at six sites already.
    pub(super) kill_mark: u32,
    pub(super) points: f64,
    /// When the weapon may convert affinity again. During the cooldown it
    /// converts NOTHING — "the weapon will not convert any affinity into
    /// points, and all collected points are reset to zero" — so this gates the
    /// accumulation, not just the firing.
    pub(super) ready_at: f64,
}

/// THE TENDRILS a kill grows and a reload takes back.
pub(super) struct Tendrils {
    /// THE OCUCOR'S TENDRILS, tracked as two watermarks rather than as a
    /// counter incremented at every kill.
    ///
    /// DERIVED, and deliberately: `r.kills` is already maintained by SIX
    /// different sites (beam kills, status-proc kills, field-tick kills, the
    /// cycle's own…), and a seventh will exist one day. Hooking each of them is
    /// how one gets missed; reading the total they all feed cannot miss any.
    /// Same for the clear, which keys off `r.reloads` — every reload path in
    /// this loop increments it, including the cycle's.
    ///
    /// It also happens to be exactly right about WHICH kills count. The wiki
    /// excludes one case — "Direct kills with tendrils will not generate an
    /// additional tendril" — and a tendril deals no damage in a single-target
    /// arena (its damage on the beam's own target is cosmetic), so a tendril
    /// kills nothing here and `r.kills` is precisely the qualifying set.
    pub(super) kill_mark: u32,
    pub(super) reload_mark: u32,
    /// The card's opening count, which the fight then treats exactly like an
    /// earned one: it is spent by the magazine event that clears the rest.
    pub(super) seed: u32,
    pub(super) count: u32,
}

/// A CRIT-CHANCE-PER-HIT PILE (Hata-Satya's kind): its stacks, and the hits and
/// refills already counted.
pub(super) struct CritPerHit {
    /// HATA-SATYA: the pellet count at the last clear, plus the card's opening
    /// pile. TWO marks — the hits, and the REFILL COUNTER that ends them.
    pub(super) hit_mark: u32,
    pub(super) refill_mark: u32,
    pub(super) seed: u32,
    pub(super) stacks: u32,
}

/// A SNIPER'S SHOT COMBO: the count and the last hit that fed it.
pub(super) struct SniperComboCount {
    /// THE SHOT COMBO COUNTER: the count as of the last landing hit, and when
    /// that was. `combo_at` turns the pair into the count at any later moment.
    /// The seed is in hand at t = 0, so the clock starts there rather than at
    /// minus infinity — otherwise the card's count would decay away before the
    /// first shot.
    pub(super) count: u32,
    pub(super) last_hit: f64,
}

/// A HELD TRIGGER'S SPOOL — shots since it was released, and when the next was due.
pub(super) struct Spool {
    /// HELD-TRIGGER SPOOL — shots since the trigger was last released, and the
    /// moment the next one was due. See `data::weapons::SustainedFireRate`.
    pub(super) shots: f64,
    pub(super) due: f64,
}
