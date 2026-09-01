//! The TENNO — the player side of a fight, loaded from `data/tenno/`.
//!
//! A fight has TWO actors; this is the player. It carries what the player IS
//! (a Warframe's stat block) and what the player is DOING ([`TennoState`]), and
//! both reach the calculation. Field names are the wiki's own
//! (`Module:Warframes/data`), so a transcribed frame fills them in rather than
//! needing a second vocabulary.
//!
//! `data/tenno/default.yaml` is the NEUTRAL Tenno — no frame chosen, no
//! abilities running, aiming down sights — which a scenario starts from and
//! overrides, so every field has a defined meaning at its default.
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
    /// The neutral player's is 0.9, the SLOWEST a frame has. Same rule as every other field here: with no frame chosen
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
    /// THE SQUAD'S AURAS. On the Tenno rather than the build, because an aura
    /// is the WARFRAME's: two players with the same gun and different squads
    /// are two fights, which is the rule `data/abilities/` already follows —
    /// and it is why an aura can no more reach the BOARD than Roar can.
    #[serde(default)]
    pub auras: Vec<crate::auras_data::AuraPick>,
    /// …AND THE FRAME'S ARCHON SHARDS, up to five sockets. Same placement and
    /// the same reason: this block is what becomes a real Warframe when frames
    /// are built, so everything put here transfers rather than moves.
    #[serde(default)]
    pub shards: Vec<crate::shards_data::ShardPick>,
}

/// WHAT THE WARFRAME BRINGS, resolved — auras and shards folded into the shapes
/// the engine already reads, plus the two things that are genuinely new.
///
/// It is computed rather than stored so a pick can never disagree with its
/// effect: the picks are the state, this is a view of them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SquadEffects {
    /// Corrosive Projection's term. A MULTIPLIER on the target's armour, and
    /// the engine's armour formula has named it since it was written —
    /// `x (1 - 0.18 x corrosive_projections)` in data/debuffs/ignite.yaml,
    /// documented and never fed a value until now.
    pub enemy_armor_multiplier: f64,
    /// Shield Disruption / EMP Aura, the same shape on shields.
    pub enemy_shield_multiplier: f64,
    /// THE CEILING, per status id. Emerald: *"Increase max stacks of Corrosion
    /// Status by +2 (+3)"*, and the page is explicit that a WEAPON's corrosion
    /// may exceed ten because of it. Five Tauforged sockets are +15, and the
    /// armour formula is `1 - (0.20 + 0.06 x stacks)` — so this is the only
    /// entry here that changes what a build CAN DO rather than how much it does.
    pub stack_cap_bonus: Vec<(String, f64)>,
    /// Flat additions to the two Warframe stats a weapon arcane reads.
    pub armor: f64,
    pub energy: f64,
}

impl SquadEffects {
    /// How many extra stacks of `status` the sockets are worth.
    pub fn cap_bonus(&self, status: &str) -> usize {
        self.stack_cap_bonus
            .iter()
            .filter(|(k, _)| k == status)
            .map(|(_, v)| *v)
            .sum::<f64>()
            .max(0.0) as usize
    }
}

impl Tenno {
    /// Fold the picks into effects. `class` is the weapon's own class, because
    /// the Amp family pays ONE class and nothing to any other.
    pub fn squad(&self, class: &str) -> SquadEffects {
        use crate::auras_data::AuraEffect;
        use crate::shards_data::ShardEffect;
        let mut out = SquadEffects::default();
        // COACTION DRIFT FIRST: it multiplies the other auras and does nothing
        // itself, so it has to be known before any of them is added.
        let strength = 1.0
            + self
                .auras
                .iter()
                .filter_map(|p| crate::auras_data::by_id(&p.id))
                .filter_map(|d| match d.effect {
                    AuraEffect::AuraStrength(v) => Some(v),
                    _ => None,
                })
                .sum::<f64>();
        for pick in &self.auras {
            let Some(d) = crate::auras_data::by_id(&pick.id) else { continue };
            // A SQUAD-STACKING AURA IS ADDITIVE WITH ITSELF, 1 to 4; one that is
            // not ignores the count rather than multiplying by it.
            let n = if d.squad_stacking { pick.count.clamp(1, 4) as f64 } else { 1.0 };
            match d.effect {
                AuraEffect::EnemyArmor(v) => out.enemy_armor_multiplier += v * n * strength,
                AuraEffect::EnemyShield(v) => out.enemy_shield_multiplier += v * n * strength,
                AuraEffect::WeaponDamage(_) => {}
                AuraEffect::AuraStrength(_) => {}
            }
        }
        for pick in &self.shards {
            match crate::shards_data::resolve(pick) {
                Some(ShardEffect::Armor(v)) => out.armor += v,
                Some(ShardEffect::EnergyMax(v)) => out.energy += v,
                Some(ShardEffect::StatusStackCap(v, which)) => {
                    out.stack_cap_bonus.push((which, v));
                }
                _ => {}
            }
        }
        let _ = class;
        out
    }

    /// THE SHARD BONUSES THAT LAND IN A MOD BUCKET, for a weapon in this SLOT.
    ///
    /// Two of the twenty-seven do, exactly: Crimson's *"+25% Primary Status
    /// Chance"* and *"+25% Secondary Critical Chance"* are the same quantity a
    /// mod of that stat feeds, so they go in the same bucket and every lock,
    /// every panel line and the optimizer's scoring treat them as one more
    /// card. The rest either pay a different layer or are narrower than any
    /// bucket here, and [`ShardEffect::unmodelled_reason`] is what says which.
    ///
    /// [`ShardEffect::unmodelled_reason`]: crate::shards_data::ShardEffect::unmodelled_reason
    pub fn shard_bonuses(&self, slot: &str) -> StatBonuses {
        use crate::shards_data::ShardEffect;
        let mut out = StatBonuses::default();
        for pick in &self.shards {
            // THE SLOT GATE IS THE OPTION's, not the effect's: "Primary
            // Electricity Damage" names an element AND a slot, so the two are
            // separate fields and this reads the one it needs.
            let Some(d) = crate::shards_data::all().iter().find(|x| x.id == pick.shard) else {
                continue;
            };
            let Some(o) = d.options.iter().find(|o| o.id == pick.effect) else { continue };
            if o.slot.as_deref().is_some_and(|s| s != slot) {
                continue;
            }
            match o.at(pick.tauforged) {
                ShardEffect::StatusChance(v, _) => out.status_chance += v,
                ShardEffect::CritChance(v, _) => out.crit_chance += v,
                _ => {}
            }
        }
        out
    }

    /// The base-damage bonus the AMP family grants a weapon of `class`, which
    /// is the one aura effect that lands in a mod bucket.
    pub fn aura_damage_bonus(&self, class: &str, pools: &[&str]) -> f64 {
        use crate::auras_data::AuraEffect;
        let strength = 1.0
            + self
                .auras
                .iter()
                .filter_map(|p| crate::auras_data::by_id(&p.id))
                .filter_map(|d| match d.effect {
                    AuraEffect::AuraStrength(v) => Some(v),
                    _ => None,
                })
                .sum::<f64>();
        self.auras
            .iter()
            .filter_map(|p| crate::auras_data::by_id(&p.id))
            .filter_map(|d| match d.effect {
                // `pays` IS THE GATE, and it asks two different questions: a
                // POOL for the three amps that follow mod compatibility, a
                // CLASS for Dead Eye, which is narrower than any pool.
                AuraEffect::WeaponDamage(v) if d.pays(class, pools) => Some(v),
                _ => None,
            })
            .sum::<f64>()
            * strength
    }
}

/// 0.9 — the slowest sprint any Warframe has. See [`Tenno::sprint`].
fn slowest_frame() -> f64 {
    0.9
}


fn yes() -> bool {
    true
}

/// The player STATES a card can be conditional on. One home for all of them:
/// `aiming` as a bool threaded through `loadout::resolve`, beside a separate
/// Tenno holding the rest, is two places for one kind of fact.
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
    /// FULLY SWITCHED TO THE MELEE WEAPON, rather than quick-melee.
    ///
    /// *"With Melee Weapon Equipped"* is a card's own wording and it means the
    /// weapon is DRAWN — a quick-melee swing from a gun does not satisfy it.
    /// Defaults TRUE, which is what every ruler runs and what a melee build is
    /// played as; a scenario may turn it off to ask what the same build is
    /// worth swung out of a rifle.
    pub melee_equipped: bool,
    /// THIS WEAPON IS THE ONLY ONE EQUIPPED — the Vasto's Lone Gun, *"With No
    /// Primary Equipped"*.
    ///
    /// It is the LOADOUT, which is a different fact from what the fight does
    /// with it. The arena has always fired one weapon for the whole engagement,
    /// and that never answered "what else are you carrying": the standing
    /// ruling is that the Tenno walks in with a full loadout, so every clause about the other slots reads FALSE. This
    /// is the knob that says otherwise.
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

/// THE FIGHT'S OWN STAT BONUSES — "the effect equals stuffing in another mod".
///
/// Everything a weapon is handed by something that is not its build: a squad
/// buff, a Warframe ability, an arcane on another weapon, a Helminth this app
/// has no entry for. Rather than one definition per source — each needing a
/// bracket nobody published — the player says what it is WORTH and the SHAPE
/// declares the arithmetic: these join the additive buckets the MODS feed, so a
/// scenario's +60% multishot and Split Chamber's +90% sum.
///
/// PERMANENT: no trigger, no clock, no stack count, since a `data/abilities/`
/// buff is the thing with a duration. There is no uptime to model, and the
/// number the player types is what every shot gets.
///
/// It lives on the TENNO because it is the fight's side of the fight, which
/// carries it everywhere for free — every `resolve_for` is already handed the
/// fight's player.
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
    /// AMMO EFFICIENCY, and it was the one bucket the panel had no box for. The engine has always had the quantity — several
    /// arcanes grant it — so this is the reader finally being able to say it.
    pub ammo_efficiency: f64,
    /// Fast Hands' — `time = base / (1 + bucket)`.
    pub reload_speed: f64,
    /// Magazine Warp's, as a fraction of the base magazine.
    pub magazine: f64,
}

impl StatBonuses {
    /// Is every bucket zero? The default, and what a fight that says nothing
    /// means — it keeps the panel from listing a source worth nothing.
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
            // TRUE, because a melee build is played with the melee weapon out —
            // and every ruler runs it that way.
            melee_equipped: true,
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

/// THE WIELDER OF A COMPANION WEAPON — a Sentinel, not a Warframe.
///
/// TWO ACTORS, ONE OF WHICH HOLDS THE GUN. A robotic weapon
/// is carried by a companion while the WARFRAME is still in the fight behind it,
/// bringing the aura and the archon shards — so only the wielder's own STAT
/// BLOCK comes from here and everything else stays the Warframe's. The auras a
/// companion weapon can take are the proof that it does: `rifle_amp` reaches an
/// Artax, and an aura is something a Warframe wears.
///
/// Same floor rule as [`default_tenno`], different roster: the lowest any
/// released Sentinel has, stat by stat. See `data/tenno/sentinel.yaml`.
pub fn sentinel_wielder() -> &'static Tenno {
    static T: OnceLock<Tenno> = OnceLock::new();
    T.get_or_init(|| {
        let text = crate::data::file("tenno/sentinel.yaml").expect("embedded data/tenno/sentinel.yaml");
        serde_norway::from_str(text).expect("parse data/tenno/sentinel.yaml")
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
    /// **A COMPANION WEAPON IS HELD BY A SENTINEL**, and the two wielders have
    /// different floors.
    ///
    /// Read off the wiki's own infoboxes across all 17 Sentinels, which agree
    /// with DE's export digit for digit — sentinel stats do not scale with rank,
    /// so there is no "at Rank 30" figure to prefer.
    ///
    /// ASSERTED AGAINST THE WARFRAME FLOOR, not as literals alone: the point of
    /// the entry is that the two DIFFER, and a test that only pinned five
    /// numbers would pass just as well on a file that had been copied.
    #[test]
    fn a_companion_weapon_is_held_by_a_sentinel_and_it_is_a_different_floor() {
        let s = sentinel_wielder();
        let w = default_tenno();
        assert_eq!(s.id, "sentinel");
        assert_eq!(s.health, 450.0, "Wyrm Prime");
        assert_eq!(s.shield, 130.0, "Shade");
        assert_eq!(s.armor, 80.0, "Carrier and eleven others");
        // The wiki's Sentinel infobox has no Energy row and no sprint row; the
        // export lists `power` and the house rule is the wiki's.
        assert_eq!(s.energy, 0.0);
        assert_eq!(s.sprint, 0.0, "a sentinel flies — no legs, no sprint gate");
        // THEY ARE NOT THE SAME WIELDER. A sentinel is tougher and less
        // armoured, which is exactly why showing one for the other was wrong.
        assert!(s.health > w.health && s.armor < w.armor,
            "sentinel {}/{} vs warframe {}/{}", s.health, s.armor, w.health, w.armor);
        // A SENTINEL IS ALWAYS AIMING, restated here because `webapi` forces it
        // too and the two must not drift.
        assert!(s.state.aiming);
        assert!(!s.state.invisible && !s.state.airborne);
    }

    /// impossible to ship by accident.
    #[test]
    fn the_default_tenno_is_the_floor_of_every_released_frame() {
        let t = default_tenno();
        assert_eq!(t.id, "tenno");
        // THE LOWEST ANY RELEASED FRAME HAS AT RANK 30, stat by stat — so this is not a frame, it is the floor of all of them,
        // and a gate that opens here opens for everybody.
        assert_eq!(t.health, 250.0, "Nokko — the wiki's \"150 (250 at Rank 30)\"");
        // ZERO, AND ZERO IS A VALUE. Six frames have no shields and four have no
        // energy pool, and the first pass at these floors read those zeros as
        // MISSING DATA — leaving Grendel's 95 and a guessed 150 standing as the
        // floors of stats whose real floor is nothing. It
        // errs in the direction that costs a player: a floor that is too high
        // makes the neutral Tenno pay a bonus some frames cannot, which is the
        // one thing this file exists to prevent.
        assert_eq!(t.shield, 0.0, "Inaros, Nidus, Kullervo — \"Shields 0 (0 at Rank 30)\"");
        assert_eq!(t.armor, 105.0, "Ash, Banshee, Gyre");
        assert_eq!(t.energy, 0.0, "Hildryn, Lavos — \"Energy 0 (0 at Rank 30)\"");
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
        // A FULL POOL OF NOTHING IS STILL NOTHING. `energy_pct` is 1.0 and the
        // pool is zero, so what is left in it is zero — the two fields say
        // different things and this is where that shows.
        assert_eq!(t.energy_now(), 0.0, "a full pool of nothing");
    }

    /// THE AMP FAMILY DOES NOT SHARE ONE GATE, and the four weapons below are
    /// the four answers.
    ///
    /// Rifle Amp asks a MOD POOL — *"also affects bows, sniper rifles and
    /// launchers"* — so a bow is paid and a shotgun is not, and no CLASS draws
    /// that line. Dead Eye asks a CLASS and is narrower than any pool:
    /// *"only affects actual sniper rifles … even though bows and launchers
    /// draw from the sniper ammo pool, they are not affected"*.
    ///
    /// It is asserted through `WeaponBase`'s own `class`/`mod_pools` rather
    /// than through typed strings, because the fault this catches is a weapon
    /// whose pools never reached the panel — which reads exactly like an aura
    /// that pays nothing.
    #[test]
    fn an_amp_pays_the_weapons_its_own_page_names_and_no_others() {
        use crate::auras_data::AuraPick;
        let mut t = crate::tenno_data::default_tenno().clone();
        let bonus = |t: &crate::tenno_data::Tenno, id: &str| {
            let b = crate::loadout::WeaponBase::from_data(id, false, &[]);
            t.aura_damage_bonus(b.class, b.mod_pools)
        };
        // A NEUTRAL PLAYER BRINGS NOTHING. The negative control first, because
        // every assertion below is a difference from it.
        for w in ["braton_prime", "boar_prime", "cernos_prime", "rubico_prime", "lex_prime"] {
            assert_eq!(bonus(&t, w), 0.0, "{w}: no aura, no bonus");
        }

        t.auras = vec![AuraPick { id: "rifle_amp".into(), count: 1 }];
        assert_eq!(bonus(&t, "braton_prime"), 0.27, "a rifle is the obvious one");
        assert_eq!(bonus(&t, "cernos_prime"), 0.27, "a BOW draws the rifle pool and is paid");
        assert_eq!(bonus(&t, "rubico_prime"), 0.27, "so does a sniper");
        assert_eq!(bonus(&t, "boar_prime"), 0.0, "a shotgun is NOT paid by Rifle Amp");
        assert_eq!(bonus(&t, "lex_prime"), 0.0, "nor a pistol");

        t.auras = vec![AuraPick { id: "dead_eye".into(), count: 1 }];
        assert_eq!(bonus(&t, "rubico_prime"), 0.525, "Dead Eye pays an actual sniper");
        assert_eq!(bonus(&t, "cernos_prime"), 0.0,
            "…and NOT a bow, which draws sniper ammo and is named as excluded");
        assert_eq!(bonus(&t, "braton_prime"), 0.0, "nor an assault rifle");

        t.auras = vec![AuraPick { id: "shotgun_amp".into(), count: 1 }];
        assert_eq!(bonus(&t, "boar_prime"), 0.18, "the shotgun amp is the SMALLER number");
        assert_eq!(bonus(&t, "braton_prime"), 0.0);

        t.auras = vec![AuraPick { id: "pistol_amp".into(), count: 1 }];
        assert_eq!(bonus(&t, "lex_prime"), 0.27);
        assert_eq!(bonus(&t, "braton_prime"), 0.0);

        // COACTION DRIFT MULTIPLIES THE OTHERS AND DOES NOTHING ITSELF — so it
        // is worth zero alone and +15% of whatever it is carried with.
        t.auras = vec![AuraPick { id: "coaction_drift".into(), count: 1 }];
        assert_eq!(bonus(&t, "braton_prime"), 0.0, "it grants no damage of its own");
        t.auras.push(AuraPick { id: "rifle_amp".into(), count: 1 });
        assert!((bonus(&t, "braton_prime") - 0.27 * 1.15).abs() < 1e-9,
            "…and lifts the amp it is carried with");
    }

    /// AN ARCHON SHARD LANDS IN THE BUCKET A MOD OF THAT STAT FEEDS, and only
    /// for the SLOT its card names.
    ///
    /// Two of the twenty-seven do — Crimson's *"+25% Primary Status Chance"*
    /// and *"+25% Secondary Critical Chance"* — and the slot gate is the thing
    /// worth asserting: the two live on the same shard, so a build that read
    /// the colour instead of the option would pay both to everything.
    ///
    /// The other twenty-five are covered from the other side: every one of them
    /// must be able to SAY why it pays nothing.
    #[test]
    fn a_shard_pays_its_own_slot_and_the_rest_say_why_they_do_not() {
        use crate::shards_data::ShardPick;
        let mut t = crate::tenno_data::default_tenno().clone();
        assert_eq!(t.shard_bonuses("primary"), crate::tenno_data::StatBonuses::default());

        t.shards = vec![ShardPick {
            shard: "crimson_archon_shard".into(),
            effect: "primary_status_chance".into(),
            tauforged: false,
        }];
        assert_eq!(t.shard_bonuses("primary").status_chance, 0.25);
        assert_eq!(t.shard_bonuses("secondary").status_chance, 0.0,
            "a PRIMARY shard pays nothing to a secondary");
        // TAUFORGED IS HALF AGAIN, and it is the same socket.
        t.shards[0].tauforged = true;
        assert_eq!(t.shard_bonuses("primary").status_chance, 0.375);

        t.shards = vec![ShardPick {
            shard: "crimson_archon_shard".into(),
            effect: "secondary_crit_chance".into(),
            tauforged: false,
        }];
        assert_eq!(t.shard_bonuses("secondary").crit_chance, 0.25);
        assert_eq!(t.shard_bonuses("primary").crit_chance, 0.0);

        // FIVE SOCKETS ARE ADDITIVE with themselves — nothing here caps a
        // repeat, which is what the game does.
        t.shards = (0..5)
            .map(|_| ShardPick {
                shard: "crimson_archon_shard".into(),
                effect: "secondary_crit_chance".into(),
                tauforged: false,
            })
            .collect();
        assert!((t.shard_bonuses("secondary").crit_chance - 1.25).abs() < 1e-9);

        // …AND EVERY OTHER EFFECT SAYS WHY IT PAYS NOTHING. Derived from the
        // roster rather than from a list of ids: an effect added tomorrow that
        // is neither applied nor explained fails here, which a hand list could
        // never report.
        for d in crate::shards_data::all() {
            for o in &d.options {
                let paid = !matches!(
                    o.at(false),
                    crate::shards_data::ShardEffect::StatusChance(..)
                        | crate::shards_data::ShardEffect::CritChance(..)
                        | crate::shards_data::ShardEffect::Armor(_)
                        | crate::shards_data::ShardEffect::EnergyMax(_)
                        | crate::shards_data::ShardEffect::StatusStackCap(..)
                );
                assert_eq!(
                    paid,
                    o.at(false).unmodelled_reason().is_some(),
                    "{}/{}: an effect is APPLIED or it SAYS WHY NOT, never neither and never both",
                    d.id, o.id
                );
            }
        }
    }
}
