//! WARFRAME ABILITY BUFFS — the fight's PLAYER, loaded from `data/abilities/`.
//!
//! A mod, an arcane and an evolution belong to the BUILD; Roar belongs to the
//! fight. That split is the whole reason this is a separate family rather than
//! a row in the mod pool: two builds compared under the same Roar is a
//! comparison, one of them getting it is not — and the BOARD carries none of
//! these, which is what keeps a board row a statement about the weapon.
//!
//! EARLY ACCESS (owner, 2026-08-08: "注意这个部分未来会迁移，目前相当于抢先开
//! 放"). When frames land, the Ability Strength comes from the frame and the
//! duration from its Ability Duration; the buff DEFINITIONS here do not change,
//! only where their two inputs come from. That is why [`resolve`] takes
//! `strength` and a duration override as arguments rather than reading them
//! from anywhere: the caller that supplies them is the part that will move.
//!
//! Four effect kinds, and the differences between them are all measured or
//! quoted rather than assumed:
//!
//! - [`AbilityEffect::FactionDamage`] (Roar) joins the bracket a Bane mod is
//!   in, so it DOUBLE-DIPS on status for free — this engine already applies
//!   `faction_at(f, depth)`, and the wiki's "the bonus is used twice in the
//!   calculation of status damage" is that.
//! - [`AbilityEffect::FinalDamage`] (Eclipse) is "an unique multiplier" and,
//!   explicitly, is NOT double-dipped: "Unlike faction damage, which double
//!   dips for status effects, the one from Eclipse is applied once."
//! - [`AbilityEffect::AddElement`] (Nourish, Shock Trooper, Fireball Frenzy,
//!   Freeze Force, Venom Dose) adds a percentage of ModifiedBase as its
//!   element, and DOES NOT COMBINE (owner: "注意不合成") — it lands on the
//!   finished vector, after the elemental hierarchy has run.
//! - [`AbilityEffect::ExtraHit`] (Xata's Whisper) is the odd one out: it does
//!   not scale the weapon's number at all, it fires a SECOND damage instance
//!   worth a percentage of the first (wiki `Extra_Hit`). The engine's rules
//!   for it are in `dummy::fire_extra_hit`; what lives here is only which
//!   element and how much.
//!
//! THE FIRST THREE ARE MULTIPLIERS AND THE FOURTH IS AN INSTANCE, which is the
//! split worth keeping in mind when a fifth arrives: a multiplier can be read
//! at any point in the pipeline by whoever needs it, and an instance has to be
//! FIRED by something that knows what triggered it.

use std::sync::OnceLock;

use serde::Deserialize;

use crate::damage::DamageType;

/// What an ability buff does to a weapon's damage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AbilityEffect {
    /// Additive inside the FACTION bracket (Bane's), and matching every
    /// faction rather than one. Double-dips on status because that bracket
    /// does.
    FactionDamage(f64),
    /// Its own multiplicative bucket, applied ONCE — to the hit and to the
    /// status payload alike, never squared.
    FinalDamage(f64),
    /// `+x of ModifiedBase` as this element, NOT entering the elemental
    /// hierarchy. Additive with elemental mods in SIZE, separate from them in
    /// PLACEMENT.
    AddElement(DamageType, f64),
    /// An EXTRA HIT: a whole second damage instance, entirely of this element,
    /// worth this fraction of the instance that triggered it. Not a multiplier
    /// on anything — `dummy::fire_extra_hits`, MECHANICS §7 §"Extra Hit", and
    /// docs/EXTRA_HIT.md for the law its members share.
    ///
    /// THREE THINGS DIFFER BETWEEN MEMBERS and nothing else does, which is what
    /// makes this a category rather than four special cases:
    ///
    /// - the ELEMENT, fixed for most and CHOSEN for Resupply, whose gear wheel
    ///   offers ten. A chosen one arrives on the pick, so one definition serves
    ///   every choice;
    /// - whether its status is GUARANTEED or rolls the weapon's own chance.
    ///   Xata's rolls ("附加的虚空伤害具有基于武器本身触发几率的独立触发几率");
    ///   Toxic Lash is 100% Toxin, and Resupply grants "the selected Elemental
    ///   Damage and Status Effect";
    /// - a WEAPON CLASS that doubles it — Resupply is 20/30/40/50% on Sniper
    ///   Rifles against 10/15/20/25%. Applied in [`resolve`], the one function
    ///   handed both the ability and the weapon it is cast on, so nothing
    ///   downstream has to know what a sniper is.
    ExtraHit {
        element: DamageType,
        frac: f64,
        forced_status: bool,
    },
}

/// One selectable buff, as the data declares it.
#[derive(Debug, Clone)]
pub struct AbilityDef {
    pub id: &'static str,
    pub name: &'static str,
    /// The Warframe it comes from, or `Helminth` for a subsumed version.
    pub frame: &'static str,
    /// Buffs that REPLACE each other rather than adding: same family, only the
    /// strongest runs (wiki, Freeze Force: "Multiple Freeze Forces do not
    /// stack; the buff with the highest Ability Strength will take effect").
    pub family: &'static str,
    pub helminth: bool,
    /// At max rank and 100% Ability Strength.
    pub value: f64,
    /// At max rank and 100% Ability Duration, in seconds.
    pub duration_s: f64,
    pub effect: AbilityEffect,
    /// The elements this ability lets you CHOOSE between, empty when it fixes
    /// one. The page draws a picker from this; `resolve` reads the choice off
    /// the pick.
    pub elements: Vec<&'static str>,
    /// (class, multiplier) — a weapon class this is worth more on. See
    /// [`AbilityEffect::ExtraHit`].
    pub class_bonus: Option<(&'static str, f64)>,
    /// What this ability does that the sim does NOT compute, in the player's
    /// own words on the card. Same field name and same meaning as a mod's and
    /// an arcane's, so `/api/meta` publishes it under the same key and the page
    /// renders all three with one function — a gap that lives only in a yaml
    /// comment is a gap nobody can act on (owner, 2026-08-08).
    pub unmodelled: Vec<&'static str>,
    /// …and the admission that is not a shortfall: this IS modelled, it matches
    /// the live game, and DE did not mean it to work this way. Xata's Whisper
    /// firing off a Blast detonation is the wiki's own Bugs entry.
    pub live_bugs: Vec<&'static str>,
    pub url: Option<&'static str>,
}

/// One buff as it RUNS in a fight: strength applied, duration decided.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActiveAbility {
    pub id: &'static str,
    /// When it stops, in seconds from the start of the engagement.
    /// `f64::INFINITY` = the whole fight (the page's "whole fight" button).
    pub ends_at: f64,
    pub effect: AbilityEffect,
}

impl ActiveAbility {
    /// Is it running at `t`? Half-open, so a 30s Roar is gone at exactly 30.
    pub fn live_at(&self, t: f64) -> bool {
        t < self.ends_at
    }
}

#[derive(Deserialize)]
struct EffectFile {
    kind: String,
    #[serde(default)]
    element: Option<String>,
    /// `element: selectable` — the choices, in the order the game offers them.
    #[serde(default)]
    elements: Vec<String>,
    /// Does its status always land, or does it roll the weapon's chance?
    #[serde(default)]
    forced_status: bool,
    /// A weapon CLASS this is worth more on, and by how much (Resupply doubles
    /// on `sniper`). Two fields rather than a map: there is one such rule.
    #[serde(default)]
    class_bonus_for: Option<String>,
    #[serde(default)]
    class_bonus: Option<f64>,
}

#[derive(Deserialize)]
struct SourceFile {
    #[serde(default)]
    url: Option<String>,
}

#[derive(Deserialize)]
struct AbilityFile {
    id: String,
    name: String,
    frame: String,
    family: String,
    #[serde(default)]
    helminth: bool,
    value: f64,
    duration_s: f64,
    effect: EffectFile,
    #[serde(default)]
    unmodelled: Vec<String>,
    #[serde(default)]
    live_bugs: Vec<String>,
    #[serde(default)]
    source: Option<SourceFile>,
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// Every ability buff in the data, in id order.
pub fn all() -> &'static [AbilityDef] {
    static A: OnceLock<Vec<AbilityDef>> = OnceLock::new();
    A.get_or_init(|| {
        let mut out: Vec<AbilityDef> = Vec::new();
        for (path, text) in crate::data::files_under("abilities/") {
            let f: AbilityFile = serde_norway::from_str(text)
                .unwrap_or_else(|e| panic!("{path}: {e}"));
            // The two element-carrying kinds read the same field the same way,
            // so it is parsed once — a second copy of these four lines is how
            // one of them ends up accepting a typo the other rejects.
            let element = |kind: &str| {
                let e = f
                    .effect
                    .element
                    .as_deref()
                    .unwrap_or_else(|| panic!("{path}: {kind} needs an element"));
                DamageType::from_name(e)
                    .unwrap_or_else(|| panic!("{path}: unknown element {e}"))
            };
            let effect = match f.effect.kind.as_str() {
                "faction_damage" => AbilityEffect::FactionDamage(f.value),
                "final_damage" => AbilityEffect::FinalDamage(f.value),
                "add_element" => AbilityEffect::AddElement(element("add_element"), f.value),
                "extra_hit" => AbilityEffect::ExtraHit {
                    // A SELECTABLE element defaults to the first choice and is
                    // replaced by the pick; a fixed one is itself.
                    element: if f.effect.element.as_deref() == Some("selectable") {
                        let first = f.effect.elements.first().unwrap_or_else(|| {
                            panic!("{path}: `element: selectable` needs `elements:`")
                        });
                        DamageType::from_name(first)
                            .unwrap_or_else(|| panic!("{path}: unknown element {first}"))
                    } else {
                        element("extra_hit")
                    },
                    frac: f.value,
                    forced_status: f.effect.forced_status,
                },
                other => panic!("{path}: unknown ability effect kind {other}"),
            };
            out.push(AbilityDef {
                id: leak(f.id),
                name: leak(f.name),
                frame: leak(f.frame),
                family: leak(f.family),
                helminth: f.helminth,
                value: f.value,
                duration_s: f.duration_s,
                effect,
                elements: f.effect.elements.iter().cloned().map(leak).collect(),
                class_bonus: match (f.effect.class_bonus_for, f.effect.class_bonus) {
                    (Some(c), Some(x)) => Some((leak(c), x)),
                    _ => None,
                },
                unmodelled: f.unmodelled.into_iter().map(leak).collect(),
                live_bugs: f.live_bugs.into_iter().map(leak).collect(),
                url: f.source.and_then(|s| s.url).map(leak),
            });
        }
        out.sort_by_key(|a| a.id);
        out
    })
}

pub fn get(id: &str) -> Option<&'static AbilityDef> {
    all().iter().find(|a| a.id == id)
}

/// One entry of a request: which buff, and how long the player says it lasts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AbilityPick<'a> {
    pub id: &'a str,
    /// Seconds, or `None` for "the whole fight".
    pub duration_s: Option<f64>,
    /// Which element, where the ability offers a choice (Resupply's gear
    /// wheel). `None` everywhere else, and on a stored pick that predates the
    /// picker — the definition's own first choice stands in.
    pub element: Option<&'a str>,
}

/// Scale an ability's value by Ability Strength.
///
/// LINEAR, for all three kinds. Every page here states the bonus as a single
/// max-rank number "affected by Ability Strength" with a per-rank table that is
/// itself linear in rank, and none of them documents a cap — so 300% strength
/// is 3x the printed number and the calculator says so rather than guessing at
/// a ceiling DE has not written down.
pub fn at_strength(v: f64, strength: f64) -> f64 {
    v * strength
}

/// The buffs that are actually RUNNING, given what was picked and how strong
/// the frame is.
///
/// THE FAMILY RULE IS APPLIED HERE, once, so no consumer can forget it: within
/// a family only the strongest survives (owner, 2026-08-08: "同时选了 roar 和
/// roar（helminth），那就选择生效当前最强的"). Comparing by resolved VALUE
/// rather than by whether one is subsumed is what makes the rule survive a
/// buffed Helminth Roar beating an unbuffed Rhino's — which is the case the
/// wiki's "highest Ability Strength will take effect" is about.
///
/// Unknown ids are dropped rather than erroring: a stored scenario outlives the
/// data, and a fight that refuses to run because a buff was renamed is worse
/// than one that runs without it. The page reports what it dropped.
pub fn resolve(
    picks: &[AbilityPick<'_>],
    strength: f64,
    weapon_class: &str,
) -> Vec<ActiveAbility> {
    let mut best: Vec<(&'static str, f64, ActiveAbility)> = Vec::new();
    for p in picks {
        let Some(def) = get(p.id) else { continue };
        let value = at_strength(def.value, strength);
        let effect = match def.effect {
            AbilityEffect::FactionDamage(_) => AbilityEffect::FactionDamage(value),
            AbilityEffect::FinalDamage(_) => AbilityEffect::FinalDamage(value),
            AbilityEffect::AddElement(t, _) => AbilityEffect::AddElement(t, value),
            AbilityEffect::ExtraHit { element, forced_status, .. } => AbilityEffect::ExtraHit {
                // THE PICK'S element wins where the ability offers a choice.
                element: p
                    .element
                    .and_then(DamageType::from_name)
                    .filter(|_| !def.elements.is_empty())
                    .unwrap_or(element),
                frac: value
                    * def
                        .class_bonus
                        .map_or(1.0, |(c, x)| if c == weapon_class { x } else { 1.0 }),
                forced_status,
            },
        };
        let live = ActiveAbility {
            id: def.id,
            ends_at: p.duration_s.unwrap_or(f64::INFINITY),
            effect,
        };
        match best.iter_mut().find(|(f, _, _)| *f == def.family) {
            Some(slot) if slot.1 >= value => {}
            Some(slot) => *slot = (def.family, value, live),
            None => best.push((def.family, value, live)),
        }
    }
    best.into_iter().map(|(_, _, a)| a).collect()
}

/// The FACTION-bracket total running at `t` (Roar's contribution to the bucket
/// a Bane mod is in).
pub fn faction_bonus_at(list: &[ActiveAbility], t: f64) -> f64 {
    list.iter()
        .filter(|a| a.live_at(t))
        .filter_map(|a| match a.effect {
            AbilityEffect::FactionDamage(v) => Some(v),
            _ => None,
        })
        .sum()
}

/// The FINAL multiplier running at `t` (Eclipse). Multiplicative between
/// distinct sources, which is what "an unique multiplier" says — and there is
/// only ever one of these today, since Eclipse's two variants share a family.
pub fn final_mult_at(list: &[ActiveAbility], t: f64) -> f64 {
    list.iter()
        .filter(|a| a.live_at(t))
        .filter_map(|a| match a.effect {
            AbilityEffect::FinalDamage(v) => Some(1.0 + v),
            _ => None,
        })
        .product()
}

/// Which of the ADD-ELEMENT buffs are running at `t`, as (element, fraction of
/// ModifiedBase). Two buffs granting the SAME element add, because they are
/// additive with elemental mods and therefore with each other.
pub fn added_elements_at(list: &[ActiveAbility], t: f64) -> Vec<(DamageType, f64)> {
    let mut out: Vec<(DamageType, f64)> = Vec::new();
    for a in list.iter().filter(|a| a.live_at(t)) {
        if let AbilityEffect::AddElement(ty, v) = a.effect {
            match out.iter_mut().find(|(t2, _)| *t2 == ty) {
                Some(e) => e.1 += v,
                None => out.push((ty, v)),
            }
        }
    }
    out
}

/// The EXTRA HITS running at `t`, as (element, fraction of the triggering
/// instance).
///
/// A LIST, and they do not merge the way [`added_elements_at`] merges two
/// grants of one element: each source is its own second damage instance with
/// its own status roll, so Toxic Lash and Xata's Whisper on one weapon are two
/// extra hits and not one bigger one (wiki `Extra_Hit`, which counts them
/// per source). Only one per FAMILY can be in this list — [`resolve`] settled
/// that before anything got here.
pub fn extra_hits_at(list: &[ActiveAbility], t: f64) -> Vec<ExtraHitLive> {
    list.iter()
        .filter(|a| a.live_at(t))
        .filter_map(|a| match a.effect {
            AbilityEffect::ExtraHit { element, frac, forced_status } => {
                Some(ExtraHitLive { element, frac, forced_status })
            }
            _ => None,
        })
        .collect()
}

/// One extra hit as the sim needs it: what element, what share of the instance
/// that triggered it, and whether its status is a roll or a certainty.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExtraHitLive {
    pub element: DamageType,
    pub frac: f64,
    /// Toxic Lash is "100% (Toxin status chance)" and Resupply grants "the
    /// selected Elemental Damage and Status Effect"; Xata's rolls the weapon's
    /// own. The difference is per member, so it travels with the member.
    pub forced_status: bool,
}

/// Every distinct moment the ACTIVE SET changes, `0.0` first and each expiry
/// after it, deduplicated and sorted.
///
/// The sim precomputes a damage vector per interval rather than per shot: the
/// set is piecewise constant and changes at most once per buff, so this turns
/// a per-shot rebuild into at most seven of them.
pub fn checkpoints(list: &[ActiveAbility]) -> Vec<f64> {
    let mut out = vec![0.0];
    for a in list {
        if a.ends_at.is_finite() && a.ends_at > 0.0 && !out.contains(&a.ends_at) {
            out.push(a.ends_at);
        }
    }
    out.sort_by(|a, b| a.partial_cmp(b).expect("no NaN in checkpoints"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_ability_loads_and_says_where_it_came_from() {
        let a = all();
        assert!(a.len() >= 12, "{} abilities", a.len());
        for d in a {
            assert!(!d.name.is_empty());
            assert!(d.value > 0.0, "{}", d.id);
            assert!(d.duration_s > 0.0, "{}", d.id);
            // A NUMBER WITHOUT A SOURCE IS A GUESS. Every one of these is a
            // wiki figure and the file has to name the page it came off.
            assert!(
                d.url.is_some_and(|u| u.starts_with("https://wiki.warframe.com/")),
                "{} has no wiki source",
                d.id
            );
        }
    }

    /// THE ONE RULE THE PAGE CANNOT BE TRUSTED WITH. Picking Roar and Roar
    /// (Helminth) is an ordinary thing to do — one is your frame, one is a
    /// squadmate's — and adding them would be worth +80% instead of +50%.
    #[test]
    fn two_buffs_of_a_family_do_not_stack_and_the_stronger_wins() {
        let picks = [
            AbilityPick { id: "roar_helminth", duration_s: None, element: None },
            AbilityPick { id: "roar", duration_s: None, element: None },
        ];
        let live = resolve(&picks, 1.0, "");
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].id, "roar");
        assert_eq!(faction_bonus_at(&live, 0.0), 0.5);

        // …and STRENGTH decides it, not which one is subsumed: the wiki's rule
        // is "the buff with the highest Ability Strength will take effect", so
        // a 200%-strength Helminth Roar (0.60) beats a 100% Rhino's (0.50).
        // Same call, different strengths, is the closest this can get to two
        // players — and it is what the field means.
        let solo = resolve(&[AbilityPick { id: "roar_helminth", duration_s: None, element: None }], 2.0, "");
        assert!(faction_bonus_at(&solo, 0.0) > 0.5);
    }

    /// Different families DO add — Roar and Eclipse are different buckets, and
    /// two different elements are two different elements.
    #[test]
    fn different_families_all_run_at_once() {
        let picks = [
            AbilityPick { id: "roar", duration_s: None, element: None },
            AbilityPick { id: "eclipse", duration_s: None, element: None },
            AbilityPick { id: "shock_trooper", duration_s: None, element: None },
            AbilityPick { id: "freeze_force", duration_s: None, element: None },
            AbilityPick { id: "xatas_whisper", duration_s: None, element: None },
        ];
        let live = resolve(&picks, 1.0, "");
        assert_eq!(live.len(), 5);
        assert_eq!(faction_bonus_at(&live, 0.0), 0.5);
        assert!((final_mult_at(&live, 0.0) - 3.0).abs() < 1e-9);
        let els = added_elements_at(&live, 0.0);
        assert_eq!(els.len(), 2);
        assert!(els.iter().all(|(_, v)| (*v - 1.0).abs() < 1e-9));
        // …and the extra hit is in NEITHER of those two lists, which is the
        // whole point of it being a fourth kind: it is not an element added to
        // the vector and not a multiplier on it.
        let xh = extra_hits_at(&live, 0.0);
        assert_eq!(xh.len(), 1);
        assert_eq!(xh[0].element, DamageType::Void);
        assert!((xh[0].frac - 0.26).abs() < 1e-9);
    }

    /// THE SUBSUMED COPY IS THE WHOLE ABILITY, and that is why there is only
    /// one card for it.
    ///
    /// Every other Helminth variant here exists because the subsumed version is
    /// WEAKER — Roar loses 20 points, Eclipse 170. The wiki lists no reduced
    /// ladder for this one, so the two would have been the same 26% under two
    /// names, and a family whose members are identical is one buff listed twice
    /// (owner, 2026-08-09: "一致就保留一个"). Asserted from the other side: the
    /// pairs that DO differ still differ, so deleting the duplicate cannot be
    /// mistaken for a licence to collapse the rest.
    #[test]
    fn the_subsumed_whisper_needed_no_card_of_its_own() {
        assert!(get("xatas_whisper").is_some());
        assert!(
            get("xatas_whisper_helminth").is_none(),
            "the subsumed copy carries the same numbers — it is one buff"
        );
        for (full, cut) in [
            ("roar", "roar_helminth"),
            ("eclipse", "eclipse_helminth"),
            ("nourish", "nourish_helminth"),
        ] {
            assert!(get(cut).unwrap().value < get(full).unwrap().value, "{cut}");
        }
    }

    /// AN EXTRA HIT ADMITS WHAT IT DOES NOT DO — the Bullet Attractor it
    /// applies and the Blast interaction that is a bug, both on its own card,
    /// because a card renders its own text and nothing else speaks for it.
    #[test]
    fn the_whisper_states_its_gaps_and_its_bug() {
        for id in ["xatas_whisper"] {
            let d = get(id).unwrap_or_else(|| panic!("{id} missing"));
            assert!(
                d.unmodelled.iter().any(|u| u.contains("Bullet Attractor")),
                "{id} says nothing about the Void proc"
            );
            assert!(
                d.live_bugs.iter().any(|b| b.contains("Blast")),
                "{id} does not admit the Blast interaction is a bug"
            );
        }
        // NEGATIVE CONTROL: an ability with nothing to admit admits nothing. A
        // check that only asserts presence passes just as well on a data set
        // that shouts "not modelled" at everything.
        let roar = get("roar").expect("roar");
        assert!(roar.unmodelled.is_empty() && roar.live_bugs.is_empty());
    }

    /// A DURATION IS A DURATION. The buff bar is not consulted; these start at
    /// the first shot and stop, which is what the page's control means.
    #[test]
    fn a_duration_ends_the_buff_and_whole_fight_never_does() {
        let live = resolve(
            &[
                AbilityPick { id: "roar", duration_s: Some(30.0), element: None },
                AbilityPick { id: "eclipse", duration_s: None, element: None },
            ],
            1.0,
            "",
        );
        assert_eq!(faction_bonus_at(&live, 29.9), 0.5);
        assert_eq!(faction_bonus_at(&live, 30.0), 0.0);
        assert_eq!(faction_bonus_at(&live, 1e9), 0.0);
        assert!((final_mult_at(&live, 1e9) - 3.0).abs() < 1e-9);
        assert_eq!(checkpoints(&live), vec![0.0, 30.0]);
    }

    /// The four augments pay a MEASURED element each, and Venom Dose's is the
    /// one a rule engine would get wrong: it is named for a toxin and pays
    /// Corrosive.
    #[test]
    fn the_augments_pay_the_elements_the_wiki_says() {
        for (id, want) in [
            ("shock_trooper", DamageType::Electricity),
            ("fireball_frenzy", DamageType::Heat),
            ("freeze_force", DamageType::Cold),
            ("venom_dose", DamageType::Corrosive),
            ("nourish", DamageType::Viral),
        ] {
            let d = get(id).unwrap_or_else(|| panic!("{id} missing"));
            assert!(
                matches!(d.effect, AbilityEffect::AddElement(t, _) if t == want),
                "{id}: {:?}",
                d.effect
            );
        }
    }
}
    /// A FAMILY WITH TWO IDENTICAL MEMBERS IS ONE BUFF LISTED TWICE.
    ///
    /// The Helminth variants exist because the subsumed version is WEAKER —
    /// Roar 50% → 30%, Eclipse 200% → 30%, Nourish 75% → 45%. Where the wiki
    /// lists no reduced ladder the ability is unchanged, and a second card
    /// carrying the same number is a choice nobody can make wrongly and nobody
    /// can make rightly: the family rule already runs whichever is stronger, so
    /// ticking both is ticking one (owner, 2026-08-09: "一致就保留一个").
    #[test]
    fn no_two_abilities_of_a_family_are_the_same_buff() {
        for a in all() {
            for b in all() {
                if a.id >= b.id || a.family != b.family {
                    continue;
                }
                assert!(
                    (a.value - b.value).abs() > 1e-9 || (a.duration_s - b.duration_s).abs() > 1e-9,
                    "{} and {} are the same buff — keep one",
                    a.id,
                    b.id
                );
            }
        }
    }

    /// THE CATEGORY'S THREE PER-MEMBER FACTS, asserted on the members that
    /// have them — because each one is a field, and a field nobody reads is a
    /// field that quietly stops working.
    #[test]
    fn the_extra_hit_members_differ_only_in_what_the_data_says() {
        // 1. A CHOSEN element. Resupply's gear wheel offers ten; the pick
        //    decides, and the definition's first choice stands in for a pick
        //    that predates the picker.
        let def = get("resupply").expect("resupply");
        assert_eq!(def.elements.len(), 10, "the gear wheel");
        let dflt = resolve(&[AbilityPick { id: "resupply", duration_s: None, element: None }], 1.0, "rifle");
        assert_eq!(extra_hits_at(&dflt, 0.0)[0].element, DamageType::Heat, "the first choice");
        let cold = resolve(
            &[AbilityPick { id: "resupply", duration_s: None, element: Some("cold") }],
            1.0,
            "rifle",
        );
        assert_eq!(extra_hits_at(&cold, 0.0)[0].element, DamageType::Cold);
        // …and a chosen element is ignored where the ability fixes one.
        let fixed = resolve(
            &[AbilityPick { id: "xatas_whisper", duration_s: None, element: Some("cold") }],
            1.0,
            "rifle",
        );
        assert_eq!(extra_hits_at(&fixed, 0.0)[0].element, DamageType::Void);

        // 2. A WEAPON CLASS that doubles it: 25% on a rifle, 50% on a sniper.
        let rifle = extra_hits_at(&dflt, 0.0)[0].frac;
        let sniper = extra_hits_at(
            &resolve(&[AbilityPick { id: "resupply", duration_s: None, element: None }], 1.0, "sniper"),
            0.0,
        )[0]
        .frac;
        assert!((rifle - 0.25).abs() < 1e-9, "{rifle}");
        assert!((sniper - 0.50).abs() < 1e-9, "{sniper}");

        // 3. A FORCED status, or the weapon's own roll. Toxic Lash is "100%
        //    (Toxin status chance)" and Resupply grants "the selected Elemental
        //    Damage and Status Effect"; Xata's rolls.
        for (id, forced) in [("toxic_lash", true), ("resupply", true), ("xatas_whisper", false)] {
            let live = resolve(&[AbilityPick { id, duration_s: None, element: None }], 1.0, "rifle");
            assert_eq!(extra_hits_at(&live, 0.0)[0].forced_status, forced, "{id}");
        }

        // …and Toxic Lash's own number, for guns: 30% Toxin, 45 s.
        let tl = get("toxic_lash").expect("toxic_lash");
        assert!((tl.value - 0.30).abs() < 1e-9);
        assert!((tl.duration_s - 45.0).abs() < 1e-9);
        assert!(matches!(tl.effect, AbilityEffect::ExtraHit { element: DamageType::Toxin, .. }));
    }

