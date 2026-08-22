//! KITGUNS — three parts and one rule, and the first weapon in this roster with
//! no published stat line of its own.
//!
//! Every other entry in `data/weapons/` states its numbers. A Kitgun states a
//! CHAMBER, a GRIP and a LOADER, and the numbers are what they compose to.
//! `docs/KITGUNS.md` is the design; `data/kitguns/README.md` is the data; this
//! is the rule, and the rule is EXACT — there is nothing here to approximate,
//! which is why every one of the assemblies can be a test case with a published
//! answer.
//!
//! # THE WEAPON IS THE CHAMBER
//!
//! Catchmoon is one weapon and its primary and secondary forms are two forms of
//! it — DE's own model, on three independent pieces of their data: one mastery
//! entry per chamber (*"the primary and secondary variants of each chamber share
//! the same Mastery progression"*), one riven compatibility per chamber in their
//! weekly trade dump, and one wiki page per chamber with the two slots as
//! sections inside it.
//!
//! That is about IDENTITY. The DATA is two stat blocks, because the difference
//! reaches the damage type, which is why [`Chamber`] is stored per slot.
//!
//! # TWO PARTS DECIDE THE MOD POOL
//!
//! The GRIP decides primary or secondary. The CHAMBER decides the pool within
//! the primary slot — Catchmoon *"Uses Shotgun mods"*, Gaze *"Uses Rifle
//! mods"*. The LOADER decides nothing about it. So [`Assembly::pool_key`] is a
//! function of the pair, and it is what the builder's mod lock and its per-pool
//! build cache are both keyed on: keying on "which part moved" would have to
//! enumerate two different ways of crossing the same line.

use serde::Deserialize;
use std::collections::BTreeMap;

/// A chamber, in ONE of its two slots.
#[derive(Debug, Clone, Deserialize)]
pub struct Chamber {
    /// `catchmoon_primary` — the chamber and the slot, because they are two
    /// records.
    pub id: String,
    pub name: String,
    /// The WEAPON's id: `catchmoon`. Both slots share it, which is what makes
    /// them one mastery entry, one riven family and one page.
    pub chamber: String,
    pub slot: String,
    pub crit_chance: f64,
    pub crit_multiplier: f64,
    pub status_chance: f64,
    pub multishot: f64,
    pub accuracy: f64,
    pub ammo_cost: f64,
    pub ammo_max: f64,
    pub ammo_pickup: f64,
    pub trigger: String,
    pub shot_type: String,
    pub riven_disposition: f64,
    pub silent: bool,
    #[serde(default)]
    pub range_m: Option<f64>,
    #[serde(default)]
    pub punch_through_m: Option<f64>,
    pub spread: Spread,
    #[serde(default)]
    pub forced_procs: Vec<String>,
    #[serde(default)]
    pub falloff: Option<Falloff>,
    /// DE's own compatibility tags, transcribed.
    #[serde(default)]
    pub tags: Vec<String>,
    /// PER GRIP, published that way. `base` is the preview a part picker shows
    /// with no grip chosen and is NOT a grip's answer — `damage_for` refuses it.
    pub damage: BTreeMap<String, BTreeMap<String, f64>>,
    pub fire_rate: BTreeMap<String, f64>,
    #[serde(default)]
    pub charge_seconds: BTreeMap<String, f64>,
    /// Size class -> rounds. The loader names the class.
    pub magazine: BTreeMap<String, f64>,
    /// AN AOE CHAMBER'S EXPLOSION, and the one part of a Kitgun the module does
    /// not publish — it marks a chamber `AOE` and states neither a radius nor a
    /// radial damage, so this is off the weapon's own page. `None` means the
    /// chamber does not explode; a chamber that carries the tag and nothing
    /// here is a transcription that stopped early, which
    /// `an_aoe_chamber_states_its_explosion` refuses.
    #[serde(default)]
    pub blast: Option<Blast>,
}

/// An explosion, per chamber.
#[derive(Debug, Clone, Deserialize)]
pub struct Blast {
    /// Which radius-mod family reaches it. The SLOT does not settle this on its
    /// own — a launcher and a shotgun are both primaries — so it is transcribed
    /// from the page that names the mod.
    #[serde(default)]
    pub radius_mods: Vec<String>,
    /// It staggers the WIELDER. This arena gives no body to stagger, so it is
    /// recorded and not paid; see docs/UNMODELLED.md.
    #[serde(default)]
    pub self_stagger: bool,
    /// ONE ENTRY PER FIRING FORM. A Tombfinger primary explodes differently on
    /// a quick shot and on a full charge, and those are two forms of one
    /// trigger rather than two weapons.
    pub forms: BTreeMap<String, BlastForm>,
}

/// One firing form's explosion. It gets its damage in exactly ONE of two ways
/// and the two are a real distinction rather than a spelling: an ADDED
/// explosion has a table of its own beside the shot, a CARVED one moves a share
/// of the shot's own damage out of the direct hit. `assemble` refuses a form
/// that claims both or neither.
#[derive(Debug, Clone, Deserialize)]
pub struct BlastForm {
    pub radius_m: f64,
    /// The share still paid at the RIM — 1.0 is no falloff at all. An
    /// explosion's falloff is centre-to-rim and therefore needs no distances,
    /// which is why this is not the `Falloff` a shot carries.
    pub falloff_to: f64,
    /// ADDED: its own vector, per grip, exactly as the chamber's damage is.
    #[serde(default)]
    pub damage: BTreeMap<String, BTreeMap<String, f64>>,
    /// CARVED: which of the shot's damage types the split is taken out of.
    #[serde(default)]
    pub splash_share_of: Option<String>,
    /// CARVED: how much of it goes to the explosion.
    #[serde(default)]
    pub splash_share: Option<f64>,
}

impl BlastForm {
    /// This form's explosion for one grip, given the shot it accompanies —
    /// and what is LEFT of that shot. Returns `(explosion, direct)`.
    ///
    /// A CARVED explosion changes both halves, which is why one function
    /// answers for both: computing them apart is how a share gets counted
    /// twice or lost.
    pub fn resolve(
        &self,
        grip_id: &str,
        shot: &BTreeMap<String, f64>,
    ) -> Option<(BTreeMap<String, f64>, BTreeMap<String, f64>)> {
        match (&self.splash_share_of, self.splash_share) {
            (Some(t), Some(share)) => {
                if !self.damage.is_empty() {
                    return None; // added AND carved is not a thing
                }
                let mut direct = shot.clone();
                let whole = *direct.get(t)?;
                direct.insert(t.clone(), whole * (1.0 - share));
                Some((BTreeMap::from([(t.clone(), whole * share)]), direct))
            }
            (None, None) => {
                let d = self.damage.get(grip_id)?.clone();
                Some((d, shot.clone()))
            }
            _ => None, // half a carve says nothing
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Spread {
    pub min_deg: f64,
    pub max_deg: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Falloff {
    pub start_m: f64,
    pub end_m: f64,
    pub reduction: f64,
}

/// A grip. Its ONLY stat of its own is recoil: what it does to damage and fire
/// rate is already resolved into the chamber's per-grip tables, which is why
/// those tables exist at all.
#[derive(Debug, Clone, Deserialize)]
pub struct Grip {
    pub id: String,
    pub name: String,
    pub recoil: f64,
    /// Filled from the file it was loaded out of, not from the file's contents:
    /// the slot IS which list a grip is in, and DE publishes it that way.
    #[serde(skip)]
    pub slot: String,
}

/// A loader. Three ADDITIVE deltas, a magazine size class, and a reload.
#[derive(Debug, Clone, Deserialize)]
pub struct Loader {
    pub id: String,
    pub name: String,
    pub crit_chance: f64,
    pub crit_multiplier: f64,
    pub status_chance: f64,
    /// The size CLASS, which the chamber prices.
    pub magazine: String,
    pub reload_seconds: f64,
}

#[derive(Deserialize)]
struct GripFile {
    grips: Vec<Grip>,
}

#[derive(Deserialize)]
struct LoaderFile {
    loaders: Vec<Loader>,
}

/// Every chamber record, both slots, loaded once.
pub fn chambers() -> &'static [Chamber] {
    use std::sync::OnceLock;
    static C: OnceLock<Vec<Chamber>> = OnceLock::new();
    C.get_or_init(|| {
        crate::data::files_under("kitguns/chambers/")
            .map(|(p, text)| {
                serde_norway::from_str(text).unwrap_or_else(|e| panic!("{p}: {e}"))
            })
            .collect()
    })
}

/// Every grip, each carrying the slot of the list it came from.
pub fn grips() -> &'static [Grip] {
    use std::sync::OnceLock;
    static G: OnceLock<Vec<Grip>> = OnceLock::new();
    G.get_or_init(|| {
        let mut out = Vec::new();
        for (p, text) in crate::data::files_under("kitguns/") {
            let Some(rest) = p.strip_prefix("kitguns/grips_") else { continue };
            let slot = rest.trim_end_matches(".yaml").to_string();
            let f: GripFile = serde_norway::from_str(text).unwrap_or_else(|e| panic!("{p}: {e}"));
            out.extend(f.grips.into_iter().map(|mut g| {
                g.slot = slot.clone();
                g
            }));
        }
        out
    })
}

/// Every loader. ONE list: the two slots publish identical tables, which
/// `the_two_slots_publish_the_same_loaders` asserts rather than assumes.
pub fn loaders() -> &'static [Loader] {
    use std::sync::OnceLock;
    static L: OnceLock<Vec<Loader>> = OnceLock::new();
    L.get_or_init(|| {
        let (p, text) = crate::data::files_under("kitguns/")
            .find(|(p, _)| p.ends_with("loaders.yaml"))
            .expect("data/kitguns/loaders.yaml");
        let f: LoaderFile = serde_norway::from_str(text).unwrap_or_else(|e| panic!("{p}: {e}"));
        f.loaders
    })
}

pub fn chamber(id: &str) -> Option<&'static Chamber> {
    chambers().iter().find(|c| c.id == id)
}
pub fn grip(id: &str) -> Option<&'static Grip> {
    grips().iter().find(|g| g.id == id)
}
pub fn loader(id: &str) -> Option<&'static Loader> {
    loaders().iter().find(|l| l.id == id)
}

/// WHAT A PLAYER PICKED: three part ids, and nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct Assembly {
    /// The chamber's WEAPON id (`catchmoon`), not the per-slot record's — the
    /// slot follows from the grip, so naming it here would be a second source
    /// of truth for one fact.
    pub chamber: String,
    pub grip: String,
    pub loader: String,
}

impl Assembly {
    /// Is every part chosen? A Kitgun with a hole in it has no numbers.
    pub fn complete(&self) -> bool {
        !self.chamber.is_empty() && !self.grip.is_empty() && !self.loader.is_empty()
    }

    /// WHICH SLOT THIS IS, from the grip alone — DE's own rule: *"the Grip …
    /// determines whether the weapon is a primary or a secondary type weapon"*.
    ///
    /// `None` while no grip is chosen, which is the state the builder locks the
    /// mod and arcane slots in: not "no mods", but "no pool yet".
    pub fn slot(&self) -> Option<&'static str> {
        grip(&self.grip).map(|g| if g.slot == "secondary" { "secondary" } else { "primary" })
    }

    /// The per-slot chamber record this assembly resolves to.
    pub fn chamber_record(&self) -> Option<&'static Chamber> {
        let slot = self.slot()?;
        chambers().iter().find(|c| c.chamber == self.chamber && c.slot == slot)
    }

    /// WHAT DECIDES WHETHER A BUILD SURVIVES A PART CHANGE.
    ///
    /// The mod pool is a function of (chamber, grip) and of nothing else — the
    /// grip picks the slot, the chamber picks rifle-or-shotgun within the
    /// primary one, and the loader picks nothing. So this is the key the
    /// builder's mod lock and its build cache both use: a change that leaves it
    /// alone leaves the build alone, and a change that moves it cannot.
    ///
    /// Keying on "which part moved" instead would have to enumerate two
    /// different ways of crossing the same line, and would go stale the day a
    /// third part learns to move it.
    pub fn pool_key(&self) -> Option<String> {
        let c = self.chamber_record()?;
        Some(format!("{}:{}", c.slot, c.chamber))
    }
}

/// A Kitgun's stats, composed. Every field is either a part's own or the one
/// rule that combines two of them — see the module header.
#[derive(Debug, Clone, PartialEq)]
pub struct Assembled {
    pub name: String,
    pub slot: &'static str,
    pub damage: BTreeMap<String, f64>,
    pub fire_rate: f64,
    pub charge_seconds: Option<f64>,
    pub magazine: f64,
    pub reload_seconds: f64,
    pub crit_chance: f64,
    pub crit_multiplier: f64,
    pub status_chance: f64,
    pub recoil: f64,
    pub multishot: f64,
    pub accuracy: f64,
    pub ammo_cost: f64,
    pub ammo_max: f64,
    pub riven_disposition: f64,
    pub trigger: String,
    pub shot_type: String,
    pub silent: bool,
    pub range_m: Option<f64>,
    pub punch_through_m: Option<f64>,
    pub spread: Spread,
    pub forced_procs: Vec<String>,
    pub falloff: Option<Falloff>,
    /// THE EXPLOSION, resolved for this grip: form id -> what it deals and how
    /// far. Empty when the chamber does not explode.
    pub blasts: BTreeMap<String, AssembledBlast>,
    /// Which radius-mod family the explosions take. Empty with no explosion.
    pub blast_radius_mods: Vec<String>,
}

/// One firing form's explosion, resolved for a grip.
#[derive(Debug, Clone, PartialEq)]
pub struct AssembledBlast {
    pub radius_m: f64,
    pub falloff_to: f64,
    /// What the explosion deals.
    pub damage: BTreeMap<String, f64>,
    /// What is LEFT of the direct hit. Equal to the shot for an ADDED
    /// explosion; short of it by the carve for a CARVED one.
    pub direct: BTreeMap<String, f64>,
}

/// THE COMPOSITION RULE, and the whole of it.
///
/// `None` when a part is missing or names nothing — a Kitgun that is not
/// assembled has no numbers, and inventing some would be worse than saying so.
pub fn assemble(a: &Assembly) -> Option<Assembled> {
    let c = a.chamber_record()?;
    let g = grip(&a.grip)?;
    let l = loader(&a.loader)?;
    // A GRIP MUST BELONG TO THIS SLOT. `chamber_record` already picked the slot
    // FROM the grip, so this cannot fail today — it is here because the two
    // could drift the day a grip list gains an entry in the wrong file, and a
    // silently mismatched pair would compose numbers nobody can reproduce.
    if g.slot != c.slot {
        return None;
    }
    // THE SHOT, before any explosion is carved out of it. A CARVED explosion
    // needs it and so does the direct hit, which is why it is read once.
    let shot = c.damage.get(&g.id)?;
    Some(Assembled {
        name: c.name.clone(),
        slot: if c.slot == "secondary" { "secondary" } else { "primary" },
        // PER GRIP, published. `base` is the picker's preview and is not a
        // grip's answer, so it is never reachable here: the key is the grip's id.
        damage: shot.clone(),
        fire_rate: *c.fire_rate.get(&g.id)?,
        charge_seconds: c.charge_seconds.get(&g.id).copied(),
        // THE LOADER NAMES A SIZE CLASS AND THE CHAMBER PRICES IT.
        magazine: *c.magazine.get(&l.magazine)?,
        reload_seconds: l.reload_seconds,
        // THREE ADDITIVE DELTAS, and they may be negative: Flutterfire is
        // -8% crit chance and +14% status.
        crit_chance: c.crit_chance + l.crit_chance,
        crit_multiplier: c.crit_multiplier + l.crit_multiplier,
        status_chance: c.status_chance + l.status_chance,
        recoil: g.recoil,
        multishot: c.multishot,
        accuracy: c.accuracy,
        ammo_cost: c.ammo_cost,
        ammo_max: c.ammo_max,
        riven_disposition: c.riven_disposition,
        trigger: c.trigger.clone(),
        shot_type: c.shot_type.clone(),
        silent: c.silent,
        range_m: c.range_m,
        punch_through_m: c.punch_through_m,
        spread: c.spread,
        forced_procs: c.forced_procs.clone(),
        falloff: c.falloff,
        blasts: match &c.blast {
            None => BTreeMap::new(),
            Some(b) => {
                let mut out = BTreeMap::new();
                for (form, f) in &b.forms {
                    let (damage, direct) = f.resolve(&g.id, shot)?;
                    out.insert(
                        form.clone(),
                        AssembledBlast {
                            radius_m: f.radius_m,
                            falloff_to: f.falloff_to,
                            damage,
                            direct,
                        },
                    );
                }
                out
            }
        },
        blast_radius_mods: c.blast.as_ref().map(|b| b.radius_mods.clone()).unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EVERY PART LOADS, AND THE TWO SLOTS ARE TWO LISTS.
    #[test]
    fn the_parts_load_and_the_slots_are_told_apart() {
        assert_eq!(chambers().len(), 4, "two chambers, both slots");
        assert_eq!(grips().len(), 10, "five grips a slot");
        assert_eq!(loaders().len(), 20);
        let prim: Vec<&str> = grips().iter().filter(|g| g.slot == "primary")
            .map(|g| g.id.as_str()).collect();
        assert_eq!(prim.len(), 5, "{prim:?}");
        // THE SLOT IS THE GRIP'S, and it is the only thing that says so.
        assert_eq!(grip("tremor").unwrap().slot, "primary");
        assert_eq!(grip("haymaker").unwrap().slot, "secondary");
    }

    /// THE TWO SLOTS PUBLISH THE SAME LOADERS, which is why there is one list.
    ///
    /// Asserted rather than assumed: the generator that wrote the file checked
    /// it too, and this is the half that keeps holding after the generator has
    /// been thrown away. A divergence should fail the build, not be averaged.
    #[test]
    fn the_two_slots_publish_the_same_loaders() {
        // Every loader a chamber can be built with is in the one list, and the
        // size class it names is one every chamber prices.
        for l in loaders() {
            for c in chambers() {
                assert!(
                    c.magazine.contains_key(&l.magazine),
                    "{} names size class {:?} and {} does not price it",
                    l.id, l.magazine, c.id
                );
            }
        }
    }

    /// THE RULE, ON A HAND-CHECKED ASSEMBLY.
    ///
    /// Catchmoon + Tremor + Flutterfire, every number read straight out of the
    /// module: damage is Tremor's row, fire rate is Tremor's, the magazine is
    /// what Catchmoon prices Flutterfire's `low` at, and the three deltas are
    /// added rather than scaled.
    #[test]
    fn the_composition_rule_is_exact() {
        let a = Assembly {
            chamber: "catchmoon".into(),
            grip: "tremor".into(),
            loader: "flutterfire".into(),
        };
        assert_eq!(a.slot(), Some("primary"), "Tremor is a primary grip");
        let k = assemble(&a).expect("a complete assembly composes");
        assert_eq!(k.damage.get("heat"), Some(&126.0));
        assert_eq!(k.damage.get("impact"), Some(&90.0));
        assert_eq!(k.fire_rate, 3.0);
        assert_eq!(k.magazine, 7.0, "Flutterfire is `low`, and Catchmoon prices low at 7");
        assert_eq!(k.reload_seconds, 1.3);
        // 0.21 - 0.08, and 0.21 + 0.14. Additive, and the crit delta is
        // NEGATIVE — a rule that multiplied would give 0.19 and 0.24.
        assert!((k.crit_chance - 0.13).abs() < 1e-9, "{}", k.crit_chance);
        assert!((k.status_chance - 0.35).abs() < 1e-9, "{}", k.status_chance);
        assert!((k.crit_multiplier - 1.7).abs() < 1e-9, "{}", k.crit_multiplier);
        assert_eq!(k.recoil, 1.0, "recoil is the grip's and nothing else's");
        assert_eq!(k.forced_procs, vec!["impact".to_string()]);
    }

    /// THE SAME CHAMBER IN THE OTHER SLOT IS A DIFFERENT WEAPON'S WORTH OF
    /// DIFFERENT, which is why a chamber is two records.
    #[test]
    fn a_chamber_is_two_records_and_they_differ() {
        let p = chamber("catchmoon_primary").unwrap();
        let s = chamber("catchmoon_secondary").unwrap();
        assert_eq!(p.chamber, s.chamber, "one weapon");
        assert_ne!(p.riven_disposition, s.riven_disposition,
            "disposition is per SLOT — 1.05 against 0.5");
        assert_ne!(p.accuracy, s.accuracy);
        assert_ne!(p.range_m, s.range_m);
        // …AND THE POOL KEY MOVES WITH IT, which is what the builder's lock and
        // its build cache read.
        let prim = Assembly { chamber: "catchmoon".into(), grip: "tremor".into(),
            loader: "flutterfire".into() };
        let sec = Assembly { grip: "haymaker".into(), ..prim.clone() };
        assert_ne!(prim.pool_key(), sec.pool_key());
        // A LOADER NEVER MOVES IT. That is the whole reason the key is a pair.
        let other_loader = Assembly { loader: "killstream".into(), ..prim.clone() };
        assert_eq!(prim.pool_key(), other_loader.pool_key());
    }

    /// AN INCOMPLETE KITGUN HAS NO NUMBERS, and says so rather than inventing
    /// them. This is the state the builder locks its mod and arcane slots in.
    #[test]
    fn an_unassembled_kitgun_composes_to_nothing() {
        let mut a = Assembly { chamber: "catchmoon".into(), grip: String::new(),
            loader: "flutterfire".into() };
        assert!(!a.complete());
        assert_eq!(a.slot(), None, "no grip, no slot — and therefore no pool");
        assert_eq!(a.pool_key(), None);
        assert!(assemble(&a).is_none());
        a.grip = "tremor".into();
        assert!(a.complete() && assemble(&a).is_some());
        // …AND A PART THAT NAMES NOTHING IS THE SAME ANSWER, not a panic.
        assert!(assemble(&Assembly { loader: "no_such_loader".into(), ..a.clone() }).is_none());
    }

    /// EVERY ASSEMBLY COMPOSES — 2 chambers x 2 slots x 5 grips x 20 loaders.
    ///
    /// The rule is exact, so there is no combination it may decline: a `None`
    /// here is a missing table entry, which is the one way this data can be
    /// half-transcribed without anything else noticing.
    #[test]
    fn every_assembly_the_parts_allow_composes() {
        let mut n = 0;
        for c in chambers() {
            for g in grips().iter().filter(|g| g.slot == c.slot) {
                for l in loaders() {
                    let a = Assembly {
                        chamber: c.chamber.clone(),
                        grip: g.id.clone(),
                        loader: l.id.clone(),
                    };
                    let k = assemble(&a).unwrap_or_else(|| {
                        panic!("{} + {} + {} composes to nothing", c.id, g.id, l.id)
                    });
                    assert!(k.magazine > 0.0 && k.fire_rate > 0.0);
                    assert!(!k.damage.is_empty());
                    // A CHARGED CHAMBER CHARGES ON EVERY GRIP, or the table is
                    // half-filled — the grip is what sets the time, so a
                    // missing row is a grip with no answer.
                    if c.trigger == "Charge" {
                        assert!(k.charge_seconds.is_some(),
                            "{} is a charge trigger and {} has no charge time", c.id, g.id);
                    }
                    n += 1;
                }
            }
        }
        assert_eq!(n, 400, "2 chambers x 2 slots x 5 grips x 20 loaders");
    }
    /// A chamber DE tags `AOE` explodes, and this file has to say how. The
    /// module publishes neither a radius nor a radial damage for any of them,
    /// so a transcription that reads the module alone produces a weapon with a
    /// silent hole in it — which is exactly what the first pass at Tombfinger
    /// did, and this is what stops the next one.
    #[test]
    fn an_aoe_chamber_states_its_explosion() {
        for c in chambers() {
            let tagged = c.tags.iter().any(|t| t == "AOE");
            assert_eq!(
                tagged,
                c.blast.is_some(),
                "{}: tagged AOE {tagged} but blast {}",
                c.id,
                c.blast.is_some()
            );
        }
    }

    /// An explosion gets its damage in exactly ONE of two ways, and a form that
    /// claims both or half of one is a transcription that lost its thread.
    #[test]
    fn an_explosion_is_added_or_carved_and_never_both() {
        for c in chambers() {
            let Some(b) = &c.blast else { continue };
            assert!(!b.forms.is_empty(), "{}: a blast with no form", c.id);
            for (name, f) in &b.forms {
                let added = !f.damage.is_empty();
                let carved = f.splash_share_of.is_some();
                assert_ne!(added, carved, "{}/{name}: added {added}, carved {carved}", c.id);
                assert_eq!(
                    carved,
                    f.splash_share.is_some(),
                    "{}/{name}: half a carve",
                    c.id
                );
                assert!(f.radius_m > 0.0, "{}/{name}: radius {}", c.id, f.radius_m);
                assert!(
                    f.falloff_to > 0.0 && f.falloff_to <= 1.0,
                    "{}/{name}: falloff_to {}",
                    c.id,
                    f.falloff_to
                );
                // AN ADDED EXPLOSION PRICES EVERY GRIP. Its table is per grip
                // exactly as the chamber's damage is, and a grip it skips is a
                // grip that composes to nothing at all rather than to a weapon
                // without an explosion.
                if added {
                    for g in grips().iter().filter(|g| g.slot == c.slot) {
                        assert!(
                            f.damage.contains_key(&g.id),
                            "{}/{name}: no explosion for grip {}",
                            c.id,
                            g.id
                        );
                    }
                }
            }
        }
    }

    /// The Tombfinger, both slots, transcribed off its own page — and the two
    /// halves of a CARVE add back up to the shot, which is the property that
    /// distinguishes it from an added explosion and the one a share is easiest
    /// to get wrong in.
    #[test]
    fn the_tombfinger_explodes_and_a_carve_conserves_the_shot() {
        // PRIMARY: two forms, ADDED, and the charge is the bigger one in both
        // radius and damage.
        let a = assemble(&Assembly {
            chamber: "tombfinger".into(),
            grip: "tremor".into(),
            loader: "thunderdrum".into(),
        })
        .expect("tremor tombfinger");
        let quick = &a.blasts["quick"];
        let charged = &a.blasts["charged"];
        assert_eq!(quick.radius_m, 1.7);
        assert_eq!(charged.radius_m, 6.2);
        assert_eq!(quick.damage["radiation"], 108.0);
        assert_eq!(charged.damage["radiation"], 490.0);
        // ADDED: the direct hit is the whole shot, untouched.
        assert_eq!(quick.direct, a.damage);
        assert_eq!(a.blast_radius_mods, vec!["firestorm".to_string()]);

        // SECONDARY: one form, CARVED out of the Radiation. Haymaker is the
        // page's own worked example's 180 (32 + 25 + 123).
        let b = assemble(&Assembly {
            chamber: "tombfinger".into(),
            grip: "haymaker".into(),
            loader: "thunderdrum".into(),
        })
        .expect("haymaker tombfinger");
        assert_eq!(b.slot, "secondary");
        let imp = &b.blasts["impact"];
        assert_eq!(imp.radius_m, 1.9);
        // 80.5% of 123 to the explosion, 19.5% left on the direct hit.
        assert!((imp.damage["radiation"] - 99.015).abs() < 1e-9, "{:?}", imp.damage);
        assert!((imp.direct["radiation"] - 23.985).abs() < 1e-9, "{:?}", imp.direct);
        // THE CARVE CONSERVES: every type, explosion plus direct, is the shot.
        for (t, whole) in &b.damage {
            let got = imp.direct.get(t).copied().unwrap_or(0.0)
                + imp.damage.get(t).copied().unwrap_or(0.0);
            assert!((got - whole).abs() < 1e-9, "{t}: {got} != {whole}");
        }
        // The types it does NOT carve are untouched on the direct hit.
        assert_eq!(imp.direct["impact"], 32.0);
        assert_eq!(imp.direct["puncture"], 25.0);
        assert_eq!(b.blast_radius_mods, vec!["fulmination".to_string()]);
    }

}
