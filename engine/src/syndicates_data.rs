//! THE SIX SYNDICATE RADIAL EFFECTS — `data/syndicates/`.
//!
//! Justice, Truth, Entropy, Sequence, Blight, Purity. Each is a 1000-damage
//! explosion of its own element in a 25 m radius, armed by AFFINITY the weapon
//! earns and fired on a 30-second cooldown.
//!
//! # Why they are a table and not fields on a card
//!
//! Dozens of augment mods and every Syndicate weapon grant one of exactly six
//! effects. Written onto each card, the same six facts would exist in many
//! copies, each free to drift; here a mod names its syndicate and the numbers
//! live once (data/README.md, "define once / reference anywhere").
//!
//! # What varies, and what does not
//!
//! The wiki states the shared half ONCE for all six — the damage, the radius,
//! the guaranteed proc, the 25% restore, the 30 s buff, the 30 s cooldown — and
//! tabulates only three columns: the ELEMENT, which attribute is restored, and
//! which buff is given. So those are the only fields that differ, and a seventh
//! effect would differ in the same three.
//!
//! JUSTICE IS THE ONE EXCEPTION and it is about the status: "With the exception
//! of Justice, radial explosions have a 100% chance to apply their respective
//! Status Effect... Instead of causing a Blast status effect, Justice stuns
//! nearby enemies". Its damage is Blast and its proc is not, which is why
//! `guaranteed_status` is a field rather than an assumption.
//!
//! # What this sim uses
//!
//! The explosion, whole: a 25 m radius is one the arena's only enemy is always
//! inside. The restore and the buff act on the WARFRAME and are carried
//! unread — they are what tells the six apart for a player, and a
//! weapon-damage sim has nowhere to spend them.

use std::sync::OnceLock;

use serde::Deserialize;

use crate::damage::DamageType;

/// One syndicate radial effect, resolved.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SyndicateDef {
    pub id: &'static str,
    pub name: &'static str,
    /// Flat damage the explosion deals — 1000 for all six.
    pub damage: f64,
    /// Its element. The only thing that varies about the damage.
    pub element: DamageType,
    pub radius_m: f64,
    /// Does it force its element's status? True for five; Justice stuns
    /// instead.
    pub guaranteed_status: bool,
    /// Affinity the WEAPON must earn to fill the gauge. 1000 at a maxed
    /// augment (a lower rank needs more) and 1000 flat on a Syndicate weapon.
    pub affinity_to_fill: f64,
    /// After firing: points reset to zero AND no affinity converts at all
    /// until this is out. Two rules, and the second is why a fast-killing
    /// build cannot bank points through the downtime.
    pub cooldown_seconds: f64,
}

#[derive(Debug, Deserialize)]
struct RestoreFile {
    #[allow(dead_code)]
    stat: String,
    #[allow(dead_code)]
    fraction: f64,
}

#[derive(Debug, Deserialize)]
struct BuffFile {
    #[allow(dead_code)]
    stat: String,
    #[allow(dead_code)]
    amount: f64,
    #[allow(dead_code)]
    duration_seconds: f64,
}

#[derive(Debug, Deserialize)]
struct SyndicateFile {
    id: String,
    name: String,
    damage: f64,
    element: String,
    radius_m: f64,
    guaranteed_status: bool,
    affinity_to_fill: f64,
    cooldown_seconds: f64,
    // Carried in the file and not in the resolved type: the Warframe half has
    // nowhere to go in a weapon-damage sim, and a field nothing reads would be
    // prose. It is parsed so the file is validated rather than ignored.
    #[allow(dead_code)]
    restore: RestoreFile,
    #[allow(dead_code)]
    buff: BuffFile,
}

fn all() -> &'static [SyndicateDef] {
    static S: OnceLock<Vec<SyndicateDef>> = OnceLock::new();
    S.get_or_init(|| {
        let mut out: Vec<SyndicateDef> = crate::data::files_under("syndicates/")
            .map(|(path, text)| {
                let f: SyndicateFile = serde_norway::from_str(text)
                    .unwrap_or_else(|e| panic!("parse {path}: {e}"));
                SyndicateDef {
                    damage: f.damage,
                    element: crate::weapons_data::damage_type(&f.element),
                    radius_m: f.radius_m,
                    guaranteed_status: f.guaranteed_status,
                    affinity_to_fill: f.affinity_to_fill,
                    cooldown_seconds: f.cooldown_seconds,
                    id: Box::leak(f.id.into_boxed_str()),
                    name: Box::leak(f.name.into_boxed_str()),
                }
            })
            .collect();
        out.sort_by_key(|s| s.id);
        out
    })
}

/// Every syndicate effect, by id.
pub fn effects() -> &'static [SyndicateDef] {
    all()
}

/// One effect by id, or `None` for a name nothing defines.
pub fn get(id: &str) -> Option<&'static SyndicateDef> {
    all().iter().find(|s| s.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ALL SIX, and the shared half really is shared.
    ///
    /// The wiki states the damage, radius and cooldown once for the whole
    /// family and tabulates only the element, the restore and the buff — so a
    /// file that disagreed on the shared half would be a transcription slip,
    /// not a variant, and nothing else would catch it.
    #[test]
    fn the_family_is_six_and_agrees_on_what_it_shares() {
        let all = effects();
        assert_eq!(all.len(), 6, "{:?}", all.iter().map(|s| s.id).collect::<Vec<_>>());
        for s in all {
            assert_eq!(s.damage, 1000.0, "{}", s.id);
            assert_eq!(s.radius_m, 25.0, "{}", s.id);
            assert_eq!(s.cooldown_seconds, 30.0, "{}", s.id);
            assert_eq!(s.affinity_to_fill, 1000.0, "{}", s.id);
        }
        // ...and the SIX elements are six different ones, which is the only
        // thing that makes them worth telling apart in a damage sim.
        let mut els: Vec<DamageType> = all.iter().map(|s| s.element).collect();
        els.dedup();
        assert_eq!(els.len(), 6, "each syndicate has its own element");
    }

    /// JUSTICE STUNS INSTEAD OF PROCCING, and is the only one that does.
    #[test]
    fn only_justice_forgoes_the_guaranteed_status() {
        for s in effects() {
            assert_eq!(
                s.guaranteed_status,
                s.id != "justice",
                "{} — Justice stuns rather than applying Blast; the other five proc",
                s.id
            );
        }
        assert_eq!(get("justice").expect("justice").element, DamageType::Blast);
        assert_eq!(get("truth").expect("truth").element, DamageType::Gas);
    }
}
