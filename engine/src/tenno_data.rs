//! The TENNO — the player side of a fight, loaded from `data/tenno/`.
//!
//! **Nothing here participates in a damage calculation yet, by design.** The
//! sim models one actor, the target; this module exists so the mechanics that
//! need a *player* have a place to attach when they are implemented, instead of
//! each one inventing its own (user, 2026-07-30). Today it loads, it is pinned
//! by a test, and no number in the sim changes because of it.
//!
//! What is waiting on it, all currently recorded as unmodelled:
//! - Secondary Fortifier — Overguard gained per damage dealt to Overguard;
//! - Secondary Surge — damage scaled by the ENERGY left after an ability cast;
//! - the GunCO "Adding" omission list, which is mostly Warframe abilities
//!   (Vex Armor, Eclipse, Furious Javelin, Parasitic Link) — MECHANICS §6;
//! - self-stagger from one's own radial attacks (Cautious Shot).
//!
//! `health` / `shield` are PLACEHOLDERS at 1 (see the yaml). They are not
//! Warframe stats and nothing should treat the value as meaningful — a frame's
//! real numbers replace them when frames land.

use std::sync::OnceLock;

use serde::Deserialize;

/// The player's stat block, shaped like a WARFRAME's — the field names are the
/// wiki's own (`Module:Warframes/data`: Armor, Health, Shield, Energy, Sprint),
/// so the day a frame is transcribed it fills these in rather than needing a
/// second vocabulary invented for it (user, 2026-08-02).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Tenno {
    pub id: String,
    pub name: String,
    /// PLACEHOLDER (1). Not a Warframe value.
    pub health: f64,
    /// PLACEHOLDER (1). Not a Warframe value.
    pub shield: f64,
    /// Above shields, and blocks status. 0 is its true unbuffed value.
    pub overguard: f64,
    /// Mitigates health damage only. 0 is its true unbuffed value — and the
    /// number Primary Bulwark reads ("+1% damage per point above 1,000").
    pub armor: f64,
    pub energy: f64,
    /// Sprint multiplier (wiki `Sprint`; Loki Prime 1.25). No damage reader
    /// yet — recorded because the frame has it.
    #[serde(default = "one")]
    pub sprint: f64,
    /// What the player is DOING. A mod's card gates on these ("while
    /// Invisible", "while Airborne"), and the neutral Tenno is doing none of
    /// them — which is why those mods contribute nothing today and will
    /// contribute the moment a frame that does turns one on.
    #[serde(default)]
    pub state: TennoState,
}

fn one() -> f64 {
    1.0
}

/// The player STATES a weapon mod can be conditional on. Every field defaults
/// to false: the neutral Tenno is standing still, visible, on the ground.
#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
pub struct TennoState {
    /// Spectral Serration: "+330% Damage while Invisible".
    #[serde(default)]
    pub invisible: bool,
    /// Aerial Ace and the Aero set: "while Airborne".
    #[serde(default)]
    pub airborne: bool,
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
    }
}
