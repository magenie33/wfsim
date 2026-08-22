//! wfsim damage-pipeline engine.
//!
//! See `docs/CORE.md` section 3. Damage is not a single multiplication but a
//! **strictly ordered** pipeline; every layer is an independent, testable pure
//! function. This is an early build — most layers are not implemented yet.
//!
//! Pipeline layers (placeholders, each calibrated against in-game measurements
//! as it is implemented):
//!   [1] mod resolution   [2] elemental combination   [3] per-hit damage vector
//!   [4] critical tiers   [5] status / proc           [6] hit resolution
//!   [7] target mitigation                            [8] temporal integration
//!
//! Stateful modifiers do not live inside the pure pipeline. You hold [`perks`]
//! (arcanes, weapon passives, Incarnon evolutions); on a trigger a perk grants a
//! [`buffs`]-bar buff — a runtime overlay on a target (weapon, Warframe, squad)
//! shown in the HUD. The pipeline reads a summed contribution snapshot from the
//! buff bar. See `docs/BUFFS.md`.

pub mod naming;
pub mod abilities_data;
pub mod arcanes_data;
// WHAT THE WARFRAME BRINGS, which is neither the weapon's nor the target's.
// Both ride on the fight's Tenno, so `parse_fight` carries them into the
// simulator and the optimizer alike (owner, 2026-08-21).
pub mod auras_data;
pub mod buffs;
pub mod chain;
pub mod formation;
pub mod damage;
pub mod data;
pub mod arena;
pub mod benchmarks_data;
pub mod boards_data;
pub mod builds;
pub mod dummy;
pub mod elements;
pub mod enemy_data;
pub mod evolutions_data;
pub mod factions_data;
pub mod i18n_data;
pub mod kitguns_data;
pub mod loadout;
pub mod mercy;
pub mod mod_sets_data;
pub mod mods;
pub mod mods_data;
pub mod perks;
pub mod shards_data;
pub mod rivens_data;
pub mod rng;
pub mod scaling;
pub mod sim;
pub mod space;
pub mod status;
pub mod syndicates_data;
pub mod tenno_data;
pub mod weapons_data;

// Damage-pipeline layers will be split into their own modules, e.g.:
// pub mod mod_resolution;
// pub mod elements;
// pub mod crit;
// pub mod status;
// pub mod hit;
// pub mod mitigation;
// pub mod simulate;
