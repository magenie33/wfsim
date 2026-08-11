//! The TENNO — the player side of a fight, loaded from `data/tenno/`.
//!
//! A fight has TWO actors. The target has always been one; this is the other.
//! It carries what the player IS (a Warframe's stat block) and what the player
//! is DOING ([`TennoState`]), and both reach the calculation: a mod's
//! `condition: while_invisible` is asked of the state, and an arcane that
//! scales off Warframe armor or energy reads the stats.
//!
//! Field names are the wiki's own (`Module:Warframes/data`: Armor, Health,
//! Shield, Energy, Sprint), so the day a frame is transcribed it fills these in
//! rather than needing a second vocabulary invented for it (user, 2026-08-02).
//!
//! `data/tenno/default.yaml` is the NEUTRAL Tenno — no frame chosen, no
//! abilities running, aiming down sights. A scenario starts from it and
//! overrides what it knows; that is why every field has a defined meaning at
//! its default rather than being a placeholder waiting for a frame.
//!
//! `health` / `shield` are still PLACEHOLDERS at 1 (see the yaml): nothing
//! reads them, because nothing shoots BACK yet. What is waiting on that, all
//! currently recorded as unmodelled:
//! - Secondary Fortifier — Overguard gained per damage dealt to Overguard;
//! - self-stagger from one's own radial attacks (Cautious Shot);
//! - the GunCO "Adding" omission list, which is mostly Warframe abilities
//!   (Vex Armor, Eclipse, Furious Javelin, Parasitic Link) — MECHANICS §6.

use std::sync::OnceLock;

use serde::Deserialize;

/// The player's stat block plus what the player is doing — the fight's second
/// actor, and the counterpart of [`crate::dummy::TargetParams`].
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Tenno {
    pub id: String,
    pub name: String,
    /// PLACEHOLDER (1). Not a Warframe value — nothing shoots back yet.
    pub health: f64,
    /// PLACEHOLDER (1). Not a Warframe value.
    pub shield: f64,
    /// Above shields, and blocks status. 0 is its true unbuffed value.
    pub overguard: f64,
    /// Mitigates health damage only, and the number **Primary Bulwark** reads:
    /// "+1% damage for each unit of armor past 1,000". 0 = no frame chosen.
    pub armor: f64,
    /// MAX energy — the pool, not what is left in it (that is
    /// [`TennoState::energy_pct`]). **Primary Overcharge** reads both.
    pub energy: f64,
    /// Sprint multiplier (wiki `Sprint`; Loki Prime 1.25), and the first
    /// PLAYER stat a weapon perk reads: the Latron family's Swift Punishment is
    /// "With Sprint Speed 1.2 or Higher: +30% Direct Damage per Status Type".
    ///
    /// The neutral player's is 0.9, the SLOWEST a frame has (owner, 2026-08-12:
    /// "我们应该假设tenno的sprint speed是0.9（最慢的）"). Same rule as every
    /// other field here: with no frame chosen the wielder claims nothing, so a
    /// perk gated on speed is OFF until someone says who is carrying the gun.
    /// A default of 1.0 would have been a frame nobody named, and it would have
    /// paid out on a build that cannot reach the threshold.
    #[serde(default = "slowest_frame")]
    pub sprint: f64,
    /// What the player is DOING.
    #[serde(default)]
    pub state: TennoState,
}

/// 0.9 — the slowest sprint any Warframe has. See [`Tenno::sprint`].
fn slowest_frame() -> f64 {
    0.9
}


fn yes() -> bool {
    true
}

/// The player STATES a card can be conditional on. One home for all of them:
/// `aiming` used to be a bool threaded through `loadout::resolve` beside a
/// separate Tenno holding the rest, which is two places for one kind of fact
/// (user, 2026-08-02).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct TennoState {
    /// Holding aim. Gates every `while_aiming` mod (Galvanized Crosshairs /
    /// Scope, Argon Scope, Sharpened Bullets, …). Defaults TRUE — the panel's
    /// optimistic view, and what the sim silently assumed before any of this
    /// was configurable, so no stored scenario changes meaning.
    #[serde(default = "yes")]
    pub aiming: bool,
    /// Spectral Serration: "+330% Damage while Invisible".
    #[serde(default)]
    pub invisible: bool,
    /// Aerial Ace and the Aero set: "while Airborne".
    #[serde(default)]
    pub airborne: bool,
    /// CURRENT energy as a fraction of [`Tenno::energy`]. 1.0 = full, which is
    /// the honest default for a build calculator: you are asking what the gun
    /// does, not what it does eleven casts in. Primary Overcharge's card gates
    /// on "at or above 90% Energy", so this is the number that decides it.
    #[serde(default = "full")]
    pub energy_pct: f64,
}

fn full() -> f64 {
    1.0
}

impl Default for TennoState {
    fn default() -> Self {
        Self { aiming: true, invisible: false, airborne: false, energy_pct: 1.0 }
    }
}

impl Tenno {
    /// Current energy = max × `energy_pct`. What Primary Overcharge's gate
    /// compares, and what Secondary Surge will spend from.
    pub fn energy_now(&self) -> f64 {
        self.energy * self.state.energy_pct
    }
}

/// The default Tenno (`data/tenno/default.yaml`), parsed once.
pub fn default_tenno() -> &'static Tenno {
    static T: OnceLock<Tenno> = OnceLock::new();
    T.get_or_init(|| {
        let text = crate::data::file("tenno/default.yaml").expect("embedded data/tenno/default.yaml");
        serde_norway::from_str(text).expect("parse data/tenno/default.yaml")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// code == data. This is the whole consumer for now: the entry is loaded
    /// and its values are pinned, which is what `data/README.md` requires of a
    /// field with no runtime reader yet. If a future change starts feeding the
    /// Tenno into the sim, THIS test is what makes the placeholder values
    /// impossible to ship by accident.
    #[test]
    fn the_default_tenno_loads_with_placeholder_survivability() {
        let t = default_tenno();
        assert_eq!(t.id, "tenno");
        // The two placeholders: non-zero so a reader cannot mistake the Tenno
        // for dead, and obviously not a real frame's numbers.
        assert_eq!(t.health, 1.0, "placeholder, not a Warframe stat");
        assert_eq!(t.shield, 1.0, "placeholder, not a Warframe stat");
        // These three ARE true unbuffed values, not placeholders.
        assert_eq!(t.overguard, 0.0);
        assert_eq!(t.armor, 0.0);
        assert_eq!(t.energy, 0.0);
        // The NEUTRAL state, which every scenario starts from: aiming (the
        // panel's optimistic view and the historical assumption), no ability
        // running, feet on the ground, energy full.
        assert!(t.state.aiming);
        assert!(!t.state.invisible);
        assert!(!t.state.airborne);
        assert_eq!(t.state.energy_pct, 1.0);
        assert_eq!(t.energy_now(), 0.0, "no frame chosen: no pool to be full of");
    }
}
