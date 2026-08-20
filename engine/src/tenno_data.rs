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
    /// The neutral player's is 0.9, the SLOWEST a frame has (owner,
    /// 2026-08-12). Same rule as every other field here: with no frame chosen
    /// the wielder claims nothing, so a perk gated on speed is OFF until
    /// someone says who is carrying the gun. A default of 1.0 would have been
    /// a frame nobody named, and it would have
    /// paid out on a build that cannot reach the threshold.
    #[serde(default = "slowest_frame")]
    pub sprint: f64,
    /// What the player is DOING.
    #[serde(default)]
    pub state: TennoState,
    /// …and what the player BRINGS, as mod-bucket adds. See [`StatBonuses`].
    /// Empty on the neutral player, which is the fight the board is scored
    /// under and the honest default: with nobody having said otherwise, this
    /// weapon is getting nothing it did not earn.
    #[serde(default)]
    pub bonuses: StatBonuses,
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
    /// Haven Foray / Guardian's Might: *"With Overshields: Increase Base Damage
    /// by +Y"*. A STATE and not a stat — every frame can hold overshields and
    /// none has them by default, so it is not derivable from `frames.yaml` the
    /// way armor and energy are.
    ///
    /// Defaults FALSE, the same rule as `invisible` and `airborne`: with nobody
    /// having said so, the wielder claims nothing and a perk gated on it is off.
    /// Nothing here takes them away either — see UNMODELLED.md, nobody shoots
    /// back — so this is a declaration rather than something the fight tracks.
    #[serde(default)]
    pub overshields: bool,
    /// Daring Reverie, Hunter's Mantra: *"With Channeled Ability active"*.
    ///
    /// VERBATIM, and the note is the whole definition (Braton_Incarnon_Genesis):
    /// "Channeled Abilities must be draining energy to be considered active.
    /// Abilities that do not drain energy over time such as Nekros's Desecrate,
    /// Hildryn's Haven, or Sevagoth's Gloom (with no enemies nearby) do not
    /// count."
    ///
    /// A STATE the player declares, the same shape as `overshields`: this arena
    /// fires one weapon and casts nothing, so there is no cast for it to
    /// observe. The energy DRAIN the note requires is not modelled either —
    /// which is why the wording on the control has to carry the note, or a
    /// player ticks it for an ability that would not qualify.
    #[serde(default)]
    pub channeling: bool,
    /// THIS WEAPON IS THE ONLY ONE EQUIPPED — the Vasto's Lone Gun, *"With No
    /// Primary Equipped"*.
    ///
    /// It is the LOADOUT, which is a different fact from what the fight does
    /// with it. The arena has always fired one weapon for the whole engagement,
    /// and that never answered "what else are you carrying": the standing
    /// ruling is that the Tenno walks in with a full loadout (owner,
    /// 2026-08-12), so every clause about the other slots reads FALSE. This
    /// is the knob that says otherwise (owner, 2026-08-13).
    ///
    /// Defaults FALSE, which keeps the full loadout as the fight everyone has
    /// been measuring under — every stored scenario and every board row means
    /// exactly what it meant.
    ///
    /// It answers ONE clause today and closes several others HARDER. Lone Gun's
    /// "+40 Base Damage, +14 Base Magazine Capacity" becomes reachable; the
    /// Despair family's "With Dread and Hate Equipped" and every "while
    /// Holstered" / "On Equip from Primary" perk go from *ruled* false to
    /// *impossible* — there is no second weapon to swap to. Those stay
    /// `no_holster` edges either way (UNMODELLED.md §4).
    #[serde(default)]
    pub solo_weapon: bool,
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

/// THE FIGHT'S OWN STAT BONUSES — "the effect equals stuffing in another mod"
/// (owner, 2026-08-13).
///
/// Everything a weapon is handed by something that is not its build: a squad
/// buff, a Warframe ability, an arcane on another weapon, a Helminth this app
/// has no entry for. Rather than one definition per source — each of which
/// would need a bracket nobody published — the player says what it is WORTH and
/// declares the arithmetic by declaring the shape: these join the same additive
/// buckets the MODS feed, so a scenario's +60% multishot and Split Chamber's
/// +90% sum, exactly as two multishot mods would.
///
/// PERMANENT, by the owner's own framing. No trigger, no clock, no stack count:
/// a `data/abilities/` buff is the thing with a duration, and this is the thing
/// without one. That is what makes it cheap to be right about — there is no
/// uptime to model, and the number the player types is the number every shot
/// gets.
///
/// It lives on the TENNO because it is the fight's side of the fight, which is
/// also what carries it everywhere for free: every `resolve_for` in the
/// workspace is already handed the fight's player, so the panel, the simulator
/// and the optimizer cannot disagree about it and no call site had to be
/// remembered.
///
/// NO ELEMENTS. An elemental mod is position-sensitive and enters a hierarchy
/// (MECHANICS §2), so "+90% Heat here" is not a number, it is a place in an
/// ordering — the one bucket on this page that a scalar cannot express.
#[derive(Debug, Clone, Copy, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct StatBonuses {
    /// Serration's bucket.
    pub base_damage: f64,
    /// Split Chamber's.
    pub multishot: f64,
    /// Point Strike's — RELATIVE to the unmodded base, like every crit mod.
    pub crit_chance: f64,
    /// Vital Sense's.
    pub crit_damage: f64,
    /// Rifle Aptitude's.
    pub status_chance: f64,
    /// The status DAMAGE bucket (Nightwatch Napalm's family).
    pub status_damage: f64,
    /// Speed Trigger's. A weapon whose fire rate is LOCKED ignores it, the same
    /// way it ignores a fire-rate mod.
    pub fire_rate: f64,
    /// Fast Hands' — `time = base / (1 + bucket)`.
    pub reload_speed: f64,
    /// Magazine Warp's, as a fraction of the base magazine.
    pub magazine: f64,
}

impl StatBonuses {
    /// Is every bucket zero? The default, and what a fight that says nothing
    /// means — used to keep the panel from listing a source worth nothing.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

impl Default for TennoState {
    fn default() -> Self {
        Self {
            aiming: true,
            invisible: false,
            airborne: false,
            overshields: false,
            channeling: false,
            solo_weapon: false,
            energy_pct: 1.0,
        }
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

/// ONE WARFRAME, as the three numbers a weapon perk can ask about.
///
/// Health and shield are deliberately absent — see `data/frames.yaml`: the
/// export carries rank 0, and their rank-30 gain is per-frame (+100 on Ash,
/// +200 on Inaros Prime) where armor and sprint do not move at all and energy
/// moves by a flat +50. A number that cannot be derived is worse than a missing
/// one, and nothing reads them yet.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Frame {
    pub id: String,
    pub name: String,
    pub armor: f64,
    /// MAX energy at rank 30 (the export's rank-0 pool + 50).
    pub energy: f64,
    pub sprint: f64,
}

#[derive(Debug, Deserialize)]
struct FrameFile {
    frames: Vec<Frame>,
}

/// Every Warframe (`data/frames.yaml`), parsed once and in the file's order,
/// which is alphabetical.
pub fn frames() -> &'static Vec<Frame> {
    static F: OnceLock<Vec<Frame>> = OnceLock::new();
    F.get_or_init(|| {
        let text = crate::data::file("frames.yaml").expect("embedded data/frames.yaml");
        let f: FrameFile = serde_norway::from_str(text).expect("parse data/frames.yaml");
        f.frames
    })
}

/// The frame with this id, if the roster has one.
pub fn frame(id: &str) -> Option<&'static Frame> {
    frames().iter().find(|f| f.id == id)
}

impl Tenno {
    /// This player WITH a frame chosen: its armor, its max energy, its sprint.
    ///
    /// Everything else — what the player is DOING — is untouched, because that
    /// is the scenario's and not the frame's. A frame does not decide whether
    /// you are aiming.
    pub fn with_frame(&self, f: &Frame) -> Tenno {
        Tenno {
            armor: f.armor,
            energy: f.energy,
            sprint: f.sprint,
            ..self.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// THE ROSTER LOADS, and the three numbers are the three a perk can ask
    /// about — with the one that decides a gate checked against the wiki.
    ///
    /// The export carries RANK 0 and this file carries rank 30, which for these
    /// three means: armor and sprint unchanged (they do not scale with rank)
    /// and energy +50. Ash and Inaros Prime were both read off the wiki
    /// infobox to establish that, so both are pinned here.
    #[test]
    fn the_warframe_roster_carries_the_maxed_numbers_a_perk_asks_about() {
        let all = frames();
        assert!(all.len() >= 120, "the whole roster: {}", all.len());

        let ash = frame("ash").expect("Ash");
        assert!((ash.armor - 105.0).abs() < 1e-9, "rank-invariant: {}", ash.armor);
        assert!((ash.sprint - 1.15).abs() < 1e-9);
        assert!((ash.energy - 150.0).abs() < 1e-9, "100 at rank 0, 150 maxed: {}", ash.energy);

        let inaros = frame("inaros_prime").expect("Inaros Prime");
        assert!((inaros.armor - 240.0).abs() < 1e-9);
        assert!((inaros.sprint - 1.05).abs() < 1e-9);
        assert!((inaros.energy - 190.0).abs() < 1e-9, "140 -> 190: {}", inaros.energy);

        // …AND WHICH GATES THEY CAN OPEN, which is the point of carrying them.
        let over = |f: fn(&Frame) -> bool| all.iter().filter(|x| f(x)).count();
        assert!(over(|f| f.sprint >= 1.2) >= 15, "several frames reach the sprint gates");
        assert!(over(|f| f.armor > 450.0) >= 5, "and a few the armor one");
        assert_eq!(
            over(|f| f.energy > 700.0),
            0,
            "NO frame reaches 700 max energy — the Paladin Virtue gate is unreachable              unmodded, which is why the panel keeps a typed override"
        );

        // A frame FILLS the player and touches nothing else: what the player is
        // DOING belongs to the scenario.
        let neutral = default_tenno();
        let with = neutral.with_frame(frame("valkyr_prime").expect("Valkyr Prime"));
        assert!(with.armor > neutral.armor && with.sprint != neutral.sprint);
        assert_eq!(with.state, neutral.state, "a frame does not decide whether you are aiming");
    }

    /// code == data. This is the whole consumer for now: the entry is loaded
    /// and its values are pinned, which is what `data/README.md` requires of a
    /// field with no runtime reader yet. If a future change starts feeding the
    /// Tenno into the sim, THIS test is what makes the placeholder values
    /// impossible to ship by accident.
    #[test]
    fn the_default_tenno_is_the_floor_of_every_released_frame() {
        let t = default_tenno();
        assert_eq!(t.id, "tenno");
        // THE LOWEST ANY RELEASED FRAME HAS AT RANK 30, stat by stat (owner,
        // 2026-08-20) — so this is not a frame, it is the floor of all of them,
        // and a gate that opens here opens for everybody.
        assert_eq!(t.health, 250.0, "Nokko");
        assert_eq!(t.shield, 95.0, "Grendel and Grendel Prime");
        assert_eq!(t.armor, 105.0, "Ash, Banshee, Gyre");
        assert_eq!(t.energy, 150.0);
        assert_eq!(t.sprint, 0.9, "Atlas and Qorvex");
        // Overguard is the exception and stays zero: it is not a frame STAT,
        // it is something an ability grants, and the neutral Tenno casts none.
        assert_eq!(t.overguard, 0.0);
        // AND THE FLOOR DECIDES THE GATES. Fortress Salvo asks for armor over
        // 450 and gets 105, so its punch through is off — a real answer rather
        // than an admission, which is the whole reason these numbers are here.
        assert!(t.armor < 450.0, "Fortress Salvo stays shut on the neutral frame");
        // The NEUTRAL state, which every scenario starts from: aiming (the
        // panel's optimistic view and the historical assumption), no ability
        // running, feet on the ground, energy full.
        assert!(t.state.aiming);
        assert!(!t.state.invisible);
        assert!(!t.state.airborne);
        assert_eq!(t.state.energy_pct, 1.0);
        assert_eq!(t.energy_now(), 150.0, "a full pool, and the pool is the floor");
    }
}
