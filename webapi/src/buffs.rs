// SPDX-License-Identifier: AGPL-3.0-or-later
//! The configurable buffs of a build, and the per-buff policy a request sets.

use serde_json::{json, Value};
use wfsim_engine::fight::{BuffLock, LockMode, LockedBuff};
use wfsim_engine::model::WeaponBase;
use wfsim_engine::model::{ModDef, ModEffect, StackPolicy};
use crate::registry::{WeaponInfo, has_frenzy};
use crate::request::prettify;
use crate::tenno::tenno_from;

// ---- buff enumeration --------------------------------------------------
//
// The configurable buffs of a build (weapon-scoped; shared across forms), for
// the Sim panel's per-buff cards and for `simulate_json` to map config → spec.
// Enumerated from the SOURCE vocabulary (mod effects + arcane buffs + the
// weapon passive), NOT from resolved Option fields — `panel_json` runs
// AssumedMax where those are empty. `default_*` encode today's behavior so the
// UI pre-fills sensibly. Each buff-type appears at most once in a legal build.
pub(crate) struct BuffMeta {
    pub(crate) id: String,
    name: String,
    /// What this one stack count buys, when the source grants more than one
    /// thing off the same trigger ("Critical Damage + Multishot"). Empty for
    /// a single-grant buff, where the name already says it.
    grants: String,
    max_stacks: u32,
    kind: &'static str, // "stacking" | "toggle"
    default_stacks: u32,
    default_locked: bool,
    /// PERMANENT stacks (no in-sim trigger, no decay — Fevered Frenzy): the
    /// count is a static choice, so the lock control is meaningless and the
    /// UI greys it out with a hint.
    permanent: bool,
    /// NO CEILING. Secondary Enervate gains a stack per hit until a big crit
    /// wipes the pile, so `max_stacks` has nothing honest to hold — the card
    /// shows `∞` and the input takes no maximum.
    uncapped: bool,
    /// WHAT FIRES THIS CARD (`engine::buff_events`), so the page can grey it
    /// under a fight that switches that trigger off. EMPTY means nothing
    /// grants it and no switch can reach it.
    trigger: Option<&'static str>,
}

/// What one CARD asks of the fight, by the same tables the run reads.
///
/// `None` = the id resolves to no trigger at all — a test failure, never a
/// silent empty list, because those are opposite claims.
fn card_trigger(
    id: &str,
    refs: &[&ModDef],
    arcane: &wfsim_engine::data::arcanes::ArcaneFx,
) -> Option<Option<&'static str>> {
    use wfsim_engine::data::buff_events::of_builtin;
    if let Some(t) = of_builtin(id) {
        return Some(t);
    }
    if let Some(owner) = id.strip_prefix("arcane:") {
        let arcane_id = arcane.id.as_str();
        return arcane
            .buffs
            .iter()
            .find(|b| if b.owner.is_empty() { arcane_id } else { &b.owner } == owner)
            .map(|b| b.trigger.id());
    }
    // A MOD-DECLARED BUFF, named by the trigger its data states. Condition
    // Overload is the id that NEEDS this: melee's card is unconditional and the
    // Galvanized family earns the same payload on a kill, so the mod answers
    // and no table here does.
    let declared = |e: &ModEffect| match e {
        ModEffect::GrantsStackingBuff(b) if b.id == id => Some(Some(b.trigger.id())),
        ModEffect::ConditionOverload { earned_on, .. } if id == "condition_overload" => {
            Some(*earned_on)
        }
        _ => None,
    };
    refs.iter().flat_map(|m| m.effects.iter()).find_map(|e| match e {
        ModEffect::WhileTenno(_, inner) => declared(inner),
        _ => declared(e),
    })
}

fn grant_label(g: wfsim_engine::model::ArcGrant) -> &'static str {
    use wfsim_engine::model::ArcGrant::*;
    match g {
        BaseDamage => "Base Damage",
        Multishot => "Multishot",
        ReloadSpeed => "Reload Speed",
        CritDamage => "Critical Damage",
        CritChance => "Critical Chance",
        StatusChance => "Status Chance",
        AmmoEfficiency => "Ammo Efficiency",
    }
}

pub(crate) fn enumerate_buffs(
    refs: &[&ModDef],
    // `always`: the mods in EVERY build this call describes, and therefore the
    // only ones whose `disables:` may suppress a buff card. For a real build
    // that is all of them; for a SCOPE it is the REQUIRED ones only.
    //
    // `fetchAllBuffs` marks the whole pool "search" to ask what buffs a weapon
    // could ever produce, and one candidate's lock is not a fact about the
    // others. Primary Acuity disables multishot, so under the old rule its mere
    // presence in a rifle's pool deleted Galvanized Chamber's on-kill multishot
    // from the list — a build nobody would ever assemble (eighty mods at once)
    // silently removing a card from a build they would.
    always: &[&ModDef],
    arcane: &wfsim_engine::data::arcanes::ArcaneFx,
    info: &WeaponInfo,
    tenno: &wfsim_engine::data::tenno::Tenno,
) -> Vec<BuffMeta> {
    // Sentinels resolve under BaseOnly — conditional buffs never fire, so
    // there is nothing to configure.
    if info.sentinel {
        return Vec::new();
    }
    // A stat an equipped mod has LOCKED has nothing to configure. `resolve` has
    // already emptied every bucket feeding it — "set to its default ignoring
    // other bonuses" — so a card for one would be a control that moves no
    // number: Frenzy under a Cannonade, Galvanized Diffusion under an Acuity.
    let locked = |s: &'static str| always.iter().any(|m| m.disables.contains(&s));
    let mut out: Vec<BuffMeta> = Vec::new();
    let mut push = |b: BuffMeta| {
        if !out.iter().any(|x| x.id == b.id) {
            out.push(b);
        }
    };
    // Weapon passive: Frenzy (Dual Toxocyst); a single on/off "stack".
    // EARNED like every other timed buff: it lasts 3 s off
    // a headshot, so a fight that has not started has not got it. Cheap to
    // earn — the first headshot turns it on — which is exactly why seeding it
    // bought nothing and cost the truth.
    if has_frenzy(info) && !locked("fire_rate") {
        push(BuffMeta {
            id: "frenzy".into(),
            name: "Frenzy".into(),
            grants: String::new(),
            max_stacks: 1,
            kind: "toggle",
            default_stacks: 0,
            default_locked: false,
            permanent: false,
            uncapped: false,
            trigger: None,
        });
    }
    // PYRANA PRIME'S SECOND GUN and the streak that buys it: both earned, so
    // both open empty. The streak's last kill is the gun, hence `kills - 1`.
    //
    // BOTH SAY WHAT THEY ARE WORTH, because neither pays anything a reader can
    // see on the panel: the streak buys a gun and the gun moves two stats the
    // stat block never shows moving. The gun's NAME is the weapon's, so it is
    // read off the entry rather than written here.
    if let Some(s) = wfsim_engine::data::weapons::spec(&info.id).and_then(|w| w.kill_streak_summon) {
        push(BuffMeta {
            id: wfsim_engine::model::KillStreakSummonSpec::STREAK_BUFF_ID.into(),
            name: "Kill Streak".into(),
            grants: format!(
                "{} kills within {:.0} s of each other summon the second gun",
                s.kills, s.kill_window_seconds
            ),
            max_stacks: s.kills.saturating_sub(1),
            kind: "stacking",
            default_stacks: 0,
            default_locked: false,
            permanent: false,
            uncapped: false,
            trigger: None,
        });
        push(BuffMeta {
            id: wfsim_engine::model::KillStreakSummonSpec::BUFF_ID.into(),
            name: format!("Second {}", info.name),
            grants: format!(
                "Magazine ×{}, Fire Rate ×{}, for {:.0} s",
                s.magazine_multiplier, s.fire_rate_multiplier, s.duration_seconds
            ),
            max_stacks: 1,
            kind: "toggle",
            default_stacks: 0,
            default_locked: false,
            permanent: false,
            uncapped: false,
            trigger: None,
        });
    }
    // THE SHOT COMBO COUNTER, the second weapon passive with a card — and the
    // one that needs it most. A stack costs a LANDING HIT and a sniper fires
    // once or twice a magazine, so a 60 s engagement ends around the third
    // tier while the player walked in at the fourth. Without a knob the app
    // would report every sniper at a multiplier nobody plays at.
    //
    // Gated on AIMING, which is the mechanic's own condition ("building combo
    // and benefiting from its multiplier requires being scoped in") and which
    // `resolve` answers the same way for the fight itself — so a hip-fired
    // scenario shows no card AND scores no combo, rather than showing a
    // control that moves nothing.
    if tenno.state.aiming {
        if let Some(c) = wfsim_engine::data::weapons::spec(&info.id).and_then(|w| w.sniper_combo) {
            push(BuffMeta {
                id: "sniper_combo".into(),
                name: "Shot Combo Counter".into(),
                // The count alone would not say what it buys, and the tiers
                // are not linear — so the card states the first one and the
                // rule that generates the rest.
                grants: format!("Damage ×1.5 at {} hits, +0.5 every ×3", c.min),
                max_stacks: 0,
                kind: "stacking",
                // NOTHING IN HAND, like every other earned buff. A sniper
                // walking into a room has whatever the last room left them,
                // and this app does not get to assume it was a good one.
                default_stacks: 0,
                default_locked: false,
                permanent: false,
                // No ceiling: the wiki's tiers keep climbing (the eighth is
                // 11025 hits) and inventing a maximum would be inventing data.
                uncapped: true,
                trigger: None,
            });
        }
    }
    // Mod-granted buffs.
    for m in refs {
        let nm = m.name.to_string();
        for e in &m.effects {
            use ModEffect::*;
            // UNWRAP the player condition, and DROP the effect when the
            // condition does not hold. Argon Scope's on-headshot crit is
            // `WhileTenno(Aiming, OnHeadshotCritChance)`: matching the outer
            // value meant it never produced a card at all, so the sim ran a
            // buff the panel offered no way to set. Unwrapping it
            // unconditionally then made the opposite mistake — a card for a
            // buff that cannot arm, in a fight where the player is not aiming.
            // The resolver already drops it; this is the same question, asked
            // of the same Tenno.
            let e = match e {
                WhileTenno(c, inner) if c.holds(tenno) => &**inner,
                WhileTenno(..) => continue,
                other => other,
            };
            match *e {
                // A MOD-GRANTED STACKING BUFF, carded by the SAME rule the
                // weapon's own take: the id is the buff's, which is the key the
                // replay, the config and the sampler already share, and the
                // name is the MOD's, which is what a reader is looking for on
                // the panel. A locked stat takes its buffs with it — the same
                // filter `resolve` applies to the sim's copy, so the card and
                // the number cannot disagree.
                GrantsStackingBuff(b) if !locked(b.grant.locked_stat()) => push(BuffMeta {
                    id: b.id.into(),
                    name: nm.clone(),
                    grants: String::new(),
                    max_stacks: b.max_stacks,
                    kind: "stacking",
                    // EARNED, so it opens at zero: nothing in hand when the
                    // fight starts, like every other triggered buff here.
                    //
                    // …UNLESS THE CARD SAYS A MISSION NEVER TAKES IT, which is
                    // a different thing from a buff that merely happens to have
                    // no clock: one you are already
                    // carrying when the fight starts and do not have to keep
                    // up. It still HAS a trigger, so it is not `permanent` —
                    // set the card to zero and the run earns it back.
                    default_stacks: if b.card_opens_full { b.max_stacks } else { 0 },
                    default_locked: false,
                    permanent: false,
                    uncapped: false,
                    trigger: None,
                }),
                OnKillMultishot { max_stacks, .. } if !locked("multishot") => push(BuffMeta {
                    id: "on_kill_multishot".into(),
                    name: nm.clone(),
                    grants: String::new(),
                    max_stacks,
                    kind: "stacking",
                    default_stacks: 0,
                    default_locked: false,
                    permanent: false,
                uncapped: false,
                trigger: None,
                }),
                ConditionOverload { max_stacks, .. } => push(BuffMeta {
                    id: "condition_overload".into(),
                    name: nm.clone(),
                    grants: String::new(),
                    max_stacks,
                    kind: "stacking",
                    default_stacks: 0,
                    default_locked: false,
                    permanent: false,
                uncapped: false,
                trigger: None,
                }),
                OnWeakpointElementAndStatus { .. } => push(BuffMeta {
                    id: "on_weakpoint_element".into(),
                    name: nm.clone(),
                    grants: String::new(),
                    max_stacks: 1,
                    kind: "toggle",
                    default_stacks: 0,
                    default_locked: false,
                    permanent: false,
                    uncapped: false,
                    trigger: None,
                }),
                OnHeadshotCritChance { .. } => push(BuffMeta {
                    id: "on_headshot_cc".into(),
                    name: nm.clone(),
                    grants: String::new(),
                    max_stacks: 1,
                    kind: "toggle",
                    default_stacks: 0,
                    default_locked: false,
                    permanent: false,
                uncapped: false,
                trigger: None,
                }),
                OnHeadshotKillCritChance { max_stacks, .. } => push(BuffMeta {
                    id: "on_headshot_kill_cc".into(),
                    name: nm.clone(),
                    grants: String::new(),
                    max_stacks,
                    kind: "stacking",
                    default_stacks: 0,
                    default_locked: false,
                    permanent: false,
                uncapped: false,
                trigger: None,
                }),
                OnKillCritDamage { .. } => push(BuffMeta {
                    id: "on_kill_cd".into(),
                    name: nm.clone(),
                    grants: String::new(),
                    max_stacks: 1,
                    kind: "toggle",
                    default_stacks: 0,
                    default_locked: false,
                    permanent: false,
                uncapped: false,
                trigger: None,
                }),
                // EXIMUS ADVANTAGE. A toggle like the other windows, and the
                // card is worth more here than for most of them: whether it can
                // arm at all is a property of the TARGET, so a player looking at
                // a fight against a non-Eximus can see the buff they are not
                // getting instead of wondering where the damage went.
                OnEximusWeakpointDamage { .. } => push(BuffMeta {
                    id: "on_eximus_weakpoint_bd".into(),
                    name: nm.clone(),
                    grants: String::new(),
                    max_stacks: 1,
                    kind: "toggle",
                    default_stacks: 0,
                    default_locked: false,
                    permanent: false,
                    uncapped: false,
                    trigger: None,
                }),
                // HATA-SATYA's pile. Its cap is the MOD's (500% / the rate),
                // not the weapon's, so unlike the tendrils it needs no lookup —
                // and the card is the whole measurement for the same reason the
                // tendril card is: a stack costs a hit, so how deep the pile
                // was when the fight started is a thing only the player knows.
                CritChancePerHit(c) if !locked("crit_chance") => push(BuffMeta {
                    id: "crit_per_hit".into(),
                    name: nm.clone(),
                    grants: "Critical Chance".into(),
                    // THE STEPPER IS IN HITS and the ceiling is in per cent, so
                    // the maximum it offers is the first hit that reaches the
                    // ceiling — 417 at max rank. Nothing above it moves a
                    // number, and the replay draws the per cent.
                    max_stacks: c.max_stacks(),
                    kind: "stacking",
                    default_stacks: 0,
                    default_locked: false,
                    permanent: false,
                    uncapped: false,
                    trigger: None,
                }),
                OnReloadDamage { .. } => push(BuffMeta {
                    id: "on_reload_bd".into(),
                    name: nm.clone(),
                    grants: String::new(),
                    max_stacks: 1,
                    kind: "toggle",
                    default_stacks: 0,
                    default_locked: false,
                    permanent: false,
                uncapped: false,
                trigger: None,
                }),
                // SENTIENT SURGE reads the Ocucor's TENDRILS, and the count is
                // a buff like any other: gained on a kill, cleared by a
                // magazine event, capped by the weapon. Its cap comes from the
                // WEAPON (`tendrils.max`) — the mod states only the rate, so a
                // card that carried its own maximum would be free to disagree
                // with the passive that produces it.
                //
                // The card is the whole point of the report: a tendril costs a
                // kill, so at a level where kills are slow — or against a
                // target that never dies — the weapon's own augment measures
                // as nothing and there was no knob to say otherwise (player
                // report). One count buys two stats by
                // construction, so it is ONE card that names both, the same
                // rule Frostbite's follows.
                PerTendril { .. } => {
                    let cap = wfsim_engine::data::weapons::spec(&info.id)
                        .and_then(|w| w.tendrils)
                        .map_or(0, |t| t.max);
                    if cap > 0 {
                        push(BuffMeta {
                            // Named for the mod (the client localizes it off
                            // META) with what the stacks ARE in the tail —
                            // "Sentient Surge" alone would leave the reader
                            // guessing what a stack of it is.
                            id: "tendrils".into(),
                            name: format!("{nm} (Tendrils)"),
                            grants: "Critical Chance + Status Chance".into(),
                            max_stacks: cap,
                            kind: "stacking",
                            default_stacks: 0,
                            default_locked: false,
                            permanent: false,
                            uncapped: false,
                            trigger: None,
                        });
                    }
                }
                OnReloadFireRate { .. } if !locked("fire_rate") => push(BuffMeta {
                    id: "on_reload_fr".into(),
                    name: nm.clone(),
                    grants: String::new(),
                    max_stacks: 1,
                    kind: "toggle",
                    default_stacks: 0,
                    default_locked: false,
                    permanent: false,
                uncapped: false,
                trigger: None,
                }),
                _ => {}
            }
        }
    }
    // Secondary Enervate is a PERK, not an `ArcBuffSpec`, so the arcane loop
    // below never saw it and the one arcane whose whole point is a stack count
    // had no card. Untimed, UNCAPPED, and consumed by a big crit — which is
    // why it starts at 0 like everything else that can be spent.
    if arcane.enervate_rank.is_some() {
        push(BuffMeta {
            id: "arcane:secondary_enervate".into(),
            name: wfsim_engine::data::arcanes::secondary("secondary_enervate")
                .map(|d| d.name.clone())
                .unwrap_or_else(|| prettify("secondary_enervate")),
            grants: String::new(),
            max_stacks: 0,
            kind: "stacking",
            default_stacks: 0,
            default_locked: false,
            permanent: false,
            uncapped: true,
            trigger: None,
        });
    }
    // MELEE INFLUENCE IS A WINDOW, NOT A GRANT, so the arcane loop below — which
    // walks `ArcBuffSpec`s — never saw it either. It is the one thing on a
    // melee build a reader most wants to hold still: an Electricity status
    // opens an 18 s clock and cannot refresh while it runs, so a fight's
    // average is not what the card is worth WHILE IT IS OPEN. A toggle says
    // which of the two is being read.
    if arcane.influence_chance > 0.0 {
        push(BuffMeta {
            id: "arcane:melee_influence".into(),
            name: wfsim_engine::data::arcanes::secondary("melee_influence")
                .map(|d| d.name.clone())
                .unwrap_or_else(|| prettify("melee_influence")),
            grants: String::new(),
            max_stacks: 1,
            kind: "toggle",
            default_stacks: 0,
            default_locked: false,
            permanent: false,
            uncapped: false,
            trigger: None,
        });
    }
    // RAGE IS THE WIELDER'S, earned by a melee weapon: a card in whole percent
    // that opens the meter and, locked, holds it (`wfsim_engine::data::rage`).
    let melee = wfsim_engine::data::weapons::spec(&info.id).is_some_and(|s| s.rules_slot() == "melee");
    if let Some(s) = wfsim_engine::data::warframes::warframe(&tenno.id).and_then(|f| f.rage).filter(|_| melee) {
        push(BuffMeta {
            id: wfsim_engine::data::rage::BUFF_ID.into(),
            name: "Rage".into(),
            grants: String::new(),
            max_stacks: (s.cap * 100.0).round() as u32,
            kind: "stacking",
            default_stacks: 0,
            default_locked: false,
            permanent: false,
            uncapped: false,
            trigger: None,
        });
    }
    // Arcane buffs — ONE CARD PER ARCANE, not per grant.
    //
    // Frostbite grants crit damage AND multishot off the same Cold proc, and
    // they are the same stack count by construction: there is no state of the
    // game where one is at 1 and the other at 10. Two cards invited a setting
    // that cannot exist, and the sim had to pick one of them anyway.
    if !arcane.buffs.is_empty() {
        // The buff's OWN arcane names it, not the merged `arcane.id`. A weapon
        // that seats two folds them into one `ArcaneFx` whose id is
        // "primary_deadhead+secondary_deadhead", and every card read that —
        // two identically-named cards the player could not tell apart, which
        // is the whole reason `ArcBuffSpec::owner` exists.
        let named = |id: &str| {
            wfsim_engine::data::arcanes::secondary(id)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| prettify(id))
        };
        let mut seen: Vec<String> = Vec::new();
        for b in arcane.buffs.iter() {
            // A `tenno_scaled` arcane is NOT a card. Primary Bulwark's value
            // is a WARFRAME STAT — not a stack anybody earns or loses — and a
            // "0/1" knob for it would invite switching off a number the frame
            // simply has. It rides the buff machinery to reach its bucket;
            // that is an implementation detail and it stops here. Its own control is WF Armor, in the Tenno block.
            if b.trigger == wfsim_engine::model::ArcTrigger::Passive {
                continue;
            }
            let owner = if b.owner.is_empty() { arcane.id.clone() } else { b.owner.clone() };
            if seen.contains(&owner) {
                continue;
            }
            seen.push(owner.clone());
            // Every grant this arcane makes, so the card can say what the one
            // stack count is buying.
            let grants: Vec<&'static str> = arcane
                .buffs
                .iter()
                .filter(|x| {
                    let o = if x.owner.is_empty() { &arcane.id } else { &x.owner };
                    *o == owner
                })
                .map(|x| grant_label(x.grant))
                .collect();
            let max_stacks = arcane
                .buffs
                .iter()
                .filter(|x| {
                    let o = if x.owner.is_empty() { &arcane.id } else { &x.owner };
                    *o == owner
                })
                .map(|x| x.max_stacks)
                .max()
                .unwrap_or(1);
            push(BuffMeta {
                id: format!("arcane:{owner}"),
                name: named(&owner),
                grants: grants.join(" + "),
                max_stacks,
                kind: if max_stacks > 1 { "stacking" } else { "toggle" },
                default_stacks: 0,
                default_locked: false,
                permanent: false,
                uncapped: false,
                trigger: None,
            });
        }
    }
    // WHAT EACH CARD ASKS OF THE FIGHT, in ONE pass — at the push sites it
    // would be fifteen places to remember, which is how a buff came to be
    // drawn, set and dropped once already (docs/BUFFS.md, "THREE lists").
    for b in out.iter_mut() {
        b.trigger = card_trigger(&b.id, refs, arcane).unwrap_or_default();
    }
    // THE WIELDER'S ARCANES THAT ARM A BUFF ON THIS WEAPON (Arcane Fury) —
    // carded under the arcane's own name, by the id `resolve` hands the fight.
    let slot = wfsim_engine::data::weapons::spec(&info.id).map_or("", |s| s.rules_slot());
    for w in tenno.weapon_buffs.iter().filter(|w| w.slot == slot) {
        if !locked(w.buff.grant.locked_stat()) && !out.iter().any(|x| x.id == w.buff.id) {
            out.push(BuffMeta {
                id: w.buff.id.into(),
                name: wfsim_engine::data::warframes::arcane_by_id(w.buff.id).map_or(w.buff.id.into(), |a| a.name.clone()),
                grants: String::new(),
                max_stacks: w.buff.max_stacks,
                kind: "toggle",
                default_stacks: 0,
                default_locked: false,
                permanent: false,
                uncapped: false,
                trigger: Some(w.buff.trigger.id()),
            });
        }
    }
    out
}

/// Evolution-granted configurable buffs (Fevered Frenzy's permanent stacked
/// multishot): one card per evolution with an `ms_buff`. PERMANENT — no
/// in-sim trigger and no decay, so the stack count is a static choice (full
/// by default) and the lock is display-only.
pub(crate) fn evo_buffs(evo_ids: &[String]) -> Vec<BuffMeta> {
    // NO per-effect knowledge here: the engine decides what is a
    // configurable buff (`EvolutionDef::buff_cards`, an exhaustive match),
    // so a new evolution mechanic surfaces on the cards the moment it is
    // modeled — nothing to remember to add on this side.
    evo_ids
        .iter()
        .filter_map(|id| wfsim_engine::data::evolutions::get(id))
        .flat_map(|def| {
            def.buff_cards().into_iter().map(move |c| BuffMeta {
                id: c.id.into(),
                name: def.name.clone(),
                grants: String::new(),
                max_stacks: c.max_stacks,
                kind: "stacking",
                // WHERE THE CARD OPENS, and the engine says which rule
                // applies. A permanent buff (no trigger, no decay) survives a
                // lull so it starts full; an earned one starts at zero. No
                // card's default depends on the weapon — the ceiling is the
                // same for every weapon that has the perk.
                default_stacks: match c.opens_at {
                    wfsim_engine::data::evolutions::CardOpens::Full => c.max_stacks,
                    wfsim_engine::data::evolutions::CardOpens::Zero => 0,
                },
                default_locked: false,
                permanent: c.permanent,
                uncapped: false,
                // An evolution's card ids are the engine's own, so the table
                // answers all of them with no pool and no arcane.
                trigger: card_trigger(c.id, &[], &wfsim_engine::data::arcanes::ArcaneFx::none())
                    .unwrap_or_default(),
            })
        })
        .collect()
}

pub(crate) fn buffs_json(list: &[BuffMeta]) -> Vec<Value> {
    list.iter()
        .map(|b| {
            json!({
                "id": b.id, "name": b.name, "grants": b.grants, "max_stacks": b.max_stacks,
                "kind": b.kind,
                "default_stacks": b.default_stacks, "default_locked": b.default_locked,
                "permanent": b.permanent,
                "uncapped": b.uncapped,
                // Absent means nothing grants it — a claim, not a gap.
                "trigger": b.trigger,
            })
        })
        .collect()
}

/// WHAT FIRES EACH PART, per board axis (`mods`, `arcanes`, `evolutions`),
/// keyed by id, in the trigger vocabulary of `engine::buff_events`. A part asks
/// for a trigger when one of its buff cards does (`card_trigger`, the panel's
/// own reading), when its data names it as a `trigger:` or `condition:`, or
/// when its effect is one of [`EVENT_KINDS`].
///
/// A PART, NOT A BUILD: a card held behind a player condition (Argon Scope's
/// aiming) still names its trigger and no stat is locked — the finder asks
/// "could this card ask for a weak-point kill", not "does it in this fight".
/// A part nothing fires is absent, which is a claim, not a gap.
pub(crate) fn part_triggers() -> &'static Value {
    static MEMO: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    MEMO.get_or_init(|| {
        use std::collections::{BTreeMap, BTreeSet, HashSet};
        use crate::registry::{evo_group, weapons};
        type Map = BTreeMap<String, BTreeSet<&'static str>>;
        let tenno = wfsim_engine::data::tenno::default_tenno().clone();
        let none = wfsim_engine::data::arcanes::ArcaneFx::none();
        let mut axes: BTreeMap<&str, Map> = ["mods", "arcanes", "evolutions"].into_iter().map(|a| (a, Map::new())).collect();
        let mut put = |axis: &str, id: &str, t: &'static str| {
            axes.get_mut(axis).unwrap().entry(id.to_string()).or_default().insert(t);
        };
        let mut seen = HashSet::new();
        for info in weapons().iter().filter(|i| !i.sentinel) {
            // THE WEAPON'S OWN CARDS (Frenzy, the shot combo) are no part's.
            let mine: Vec<String> = enumerate_buffs(&[], &[], &none, info, &tenno).into_iter().map(|b| b.id).collect();
            for mut m in wfsim_engine::data::mods::pool_for_weapon(&info.id) {
                if !seen.insert(format!("mods:{}", m.id)) {
                    continue;
                }
                m.effects = m.effects.into_iter().map(|e| match e {
                    ModEffect::WhileTenno(_, inner) => *inner,
                    e => e,
                }).collect();
                for b in enumerate_buffs(&[&m], &[], &none, info, &tenno).into_iter().filter(|b| !mine.contains(&b.id)) {
                    b.trigger.into_iter().for_each(|t| put("mods", m.id, t));
                }
            }
            let base = arcane_base(info);
            for pool in &info.arcane_pools {
                for a in wfsim_engine::data::arcanes::pool_for_weapon(&info.id, pool) {
                    if !seen.insert(format!("arcanes:{}", a.id)) {
                        continue;
                    }
                    let fx = a.fx(a.max_rank, StackPolicy::Emergent, base.traits, &tenno);
                    for b in enumerate_buffs(&[], &[], &fx, info, &tenno).into_iter().filter(|b| b.id.starts_with("arcane:")) {
                        b.trigger.into_iter().for_each(|t| put("arcanes", &a.id, t));
                    }
                }
            }
            let group = evo_group(info);
            for d in (1..=wfsim_engine::data::evolutions::tier_count(group))
                .flat_map(|t| wfsim_engine::data::evolutions::options(group, t))
            {
                if seen.insert(format!("evolutions:{}", d.id)) {
                    for b in evo_buffs(std::slice::from_ref(&d.id)) {
                        b.trigger.into_iter().for_each(|t| put("evolutions", &d.id, t));
                    }
                }
            }
        }
        for (axis, id, t) in declared_triggers() {
            put(axis, &id, t);
        }
        json!(axes)
    })
}

/// EFFECTS THAT WAIT FOR AN EVENT AND DRAW NO CARD, and the event: an instant
/// reload, a magazine that grows, a corpse that explodes. A field on the effect
/// (`condition: headshot_kill` on the Phenmor's reload) overrides the kind's.
const EVENT_KINDS: &[(&str, &str)] = &[
    ("instant_reload_on_kill", "kill"),
    ("instant_reload_on_headshot", "headshot"),
    ("explosion_on_kill", "kill"),
    ("strip_on_kill_in_range", "kill"),
    ("crit_per_kill", "kill"),
    ("round_restore_on_status_hit", "hit_enemy_with_status"),
    ("combo_count_on_status_hit", "hit_enemy_with_status"),
    ("magazine_growth_on_empty_reload", "reload_from_empty"),
    ("field_duration_on_empty_reload", "reload_from_empty"),
    ("flat_damage_on_status", "status_applied"),
    ("consecutive_hit_damage", "hit"),
];

/// Every `(axis, id, trigger)` a part's DATA states: a `trigger:` or
/// `condition:` naming a word of `buff_events::ALL`, or an [`EVENT_KINDS`] kind.
fn declared_triggers() -> Vec<(&'static str, String, &'static str)> {
    use serde_norway::Value as Y;
    let word = |w: &str| wfsim_engine::data::buff_events::ALL.iter().map(|(id, _)| *id).find(|id| *id == w);
    let mut out = Vec::new();
    for (axis, dir) in [("mods", "mods/"), ("arcanes", "arcanes/"), ("evolutions", "evolutions/")] {
        for (_, text) in wfsim_engine::data::files_under(dir) {
            let Ok(doc) = serde_norway::from_str::<Y>(text) else { continue };
            let Some(id) = doc.get("id").and_then(Y::as_str) else { continue };
            let mut stack: Vec<&Y> = doc.get("effects").into_iter().collect();
            while let Some(v) = stack.pop() {
                match v {
                    Y::Mapping(m) => {
                        let field = |k: &str| m.get(k).and_then(Y::as_str);
                        let named = field("trigger").and_then(word).or_else(|| field("condition").and_then(word));
                        let kind = field("kind").and_then(|k| EVENT_KINDS.iter().find(|(x, _)| *x == k).map(|(_, t)| *t));
                        if let Some(t) = named.or(kind) {
                            out.push((axis, id.to_string(), t));
                        }
                        stack.extend(m.values());
                    }
                    Y::Sequence(s) => stack.extend(s.iter()),
                    _ => {}
                }
            }
        }
    }
    out
}

// The build's resolved arcane fx (buff specs are policy-independent in shape);
// used for buff enumeration. `none` when the weapon can't equip arcanes.
/// THE WEAPON AN ARCANE READS: the Incarnon form where there is one, since an
/// arcane meets the weapon through its TRAITS and those are the spec's — no
/// evolution moves them. One construction for `simulate` and the optimizer.
pub(crate) fn arcane_base(info: &WeaponInfo) -> WeaponBase {
    WeaponBase::from_data(crate::registry::incarnon_id(info).unwrap_or(&info.id), true, &[])
}

pub(crate) fn arcane_fx_for(
    v: &Value,
    info: &WeaponInfo,
    base: &WeaponBase,
    policy: StackPolicy,
) -> wfsim_engine::data::arcanes::ArcaneFx {
    if !info.uses_arcane {
        return wfsim_engine::data::arcanes::ArcaneFx::none();
    }
    // The same player the sim and the optimizer fight as: an arcane that
    // scales off Warframe armor or energy reads it from here.
    let tenno = tenno_from(v, info);
    let parts: Vec<wfsim_engine::data::arcanes::ArcaneFx> = arcane_choices(v, info)
        .into_iter()
        .filter_map(|(pool, aid, rank)| {
            // POOL-scoped: an arcane from another pool is not equippable in
            // that slot, so it resolves to nothing rather than being applied.
            let def = wfsim_engine::data::arcanes::for_slot(&pool, &aid)?;
            let rank = rank.unwrap_or(def.max_rank).min(def.max_rank);
            Some(def.fx(rank, policy, base.traits, &tenno))
        })
        .collect();
    // Two arcanes are one effect set — see `ArcaneFx::merged`.
    wfsim_engine::data::arcanes::ArcaneFx::merged(&parts)
}

/// An arcane by id, in ANY pool this weapon seats. The optimizer's scope is a
/// flat list of ids rather than one per slot, so it asks this question
/// instead of "is it in THE pool" — an Arch-Gun's scope legitimately mixes
/// Primary and Secondary arcanes.
pub(crate) fn arcane_in_pools(
    info: &WeaponInfo,
    id: &str,
) -> Option<&'static wfsim_engine::data::arcanes::ArcaneDef> {
    info.arcane_pools
        .iter()
        .find_map(|p| wfsim_engine::data::arcanes::for_slot(p, id))
}

/// The arcane a scope mark names in `pool`, and the rank it names: `<id>` is
/// max rank, `<id>@<rank>` a lower one (the mods' [`RANK_MARK`] spelling).
///
/// [`RANK_MARK`]: wfsim_engine::data::mods::RANK_MARK
pub(crate) fn arcane_at_rank(
    pool: &str,
    mark: &str,
) -> Option<(&'static wfsim_engine::data::arcanes::ArcaneDef, u32)> {
    let (id, rank) = wfsim_engine::data::mods::split_rank(mark);
    let d = wfsim_engine::data::arcanes::for_slot(pool, id)?;
    match rank {
        None => Some((d, d.max_rank)),
        Some(r) if r < d.max_rank => Some((d, r)),
        Some(_) => None,
    }
}

/// The arcane chosen for each of the weapon's pools: `(pool, id, rank)`.
///
/// ONE wire shape — a LIST, one entry per pool, in the weapon's pool order:
///
/// ```text
/// "arcane": ["primary_deadhead", "secondary_merciless"]
/// "arcane_rank": [5, 5]
/// ```
///
/// A build saved before a weapon could seat two held a bare value under a
/// pre-data short name ("deadhead"). Both are rewritten ONCE, in the client's
/// storage (`migrateArcaneShape`), so nothing here has to know there was ever
/// another shape — the alternative is two ways of saying the same thing, kept
/// alive forever by the code that reads both.
///
/// Entries past the weapon's pool count are dropped: what a weapon can seat is
/// the weapon's business, not the caller's.
pub(crate) fn arcane_choices(v: &Value, info: &WeaponInfo) -> Vec<(String, String, Option<u32>)> {
    let ids: Vec<String> = v
        .get("arcane")
        .and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str()).map(String::from).collect())
        .unwrap_or_default();
    let ranks: Vec<Option<u32>> = v
        .get("arcane_rank")
        .and_then(|x| x.as_array())
        .map(|a| a.iter().map(|x| x.as_u64().map(|n| n as u32)).collect())
        .unwrap_or_default();
    info.arcane_pools
        .iter()
        .enumerate()
        .filter_map(|(i, pool)| {
            let id = ids.get(i)?;
            (id != "none").then(|| (pool.clone(), id.clone(), ranks.get(i).copied().flatten()))
        })
        .collect()
}

// ---- per-buff configured policy ----------------------------------------
//
// The Sim panel's section 2: `buffs: { "<id>": { stacks, locked } }`. Present
// ⇒ the sim runs Emergent and each buff carries its own initial stacks + lock;
// absent ⇒ the legacy `assume_max`/`frenzy` knobs apply (byte-for-byte).
pub(crate) type BuffCfg = std::collections::HashMap<String, (u32, bool)>;

pub(crate) fn parse_buff_config(v: &Value) -> Option<BuffCfg> {
    let obj = v.get("buffs")?.as_object()?;
    let mut m = BuffCfg::new();
    for (id, cfg) in obj {
        let stacks = cfg.get("stacks").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
        let locked = cfg.get("locked").and_then(|x| x.as_bool()).unwrap_or(false);
        m.insert(id.clone(), (stacks, locked));
    }
    Some(m)
}

/// Frenzy config → (passive active?, the buff-lock vector for a single form).
/// Locked = Permanent (100% uptime); unlocked+stacks = seed once then natural;
/// unlocked+0 = pure natural (no t=0 seed). The passive is always "present".
pub(crate) fn frenzy_apply(cfg: Option<&(u32, bool)>) -> (bool, Vec<BuffLock>) {
    match cfg {
        Some(&(_, true)) => (true, vec![BuffLock::permanent(LockedBuff::Frenzy)]),
        Some(&(stacks, false)) if stacks > 0 => {
            (true, vec![BuffLock::initial(LockedBuff::Frenzy, stacks)])
        }
        Some(&(_, false)) => (true, Vec::new()),
        None => (true, Vec::new()),
    }
}

/// The Frenzy lock mode for the incarnon cycle (baked at construction).
/// UNCONFIGURED IS EARNED, as on the page's card: a ruler sends no buff map, and
/// a Permanent default scored every Dual Toxocyst row above the page's own run.
pub(crate) fn frenzy_lock_mode(cfg: Option<&(u32, bool)>) -> LockMode {
    match cfg {
        Some(&(_, true)) => LockMode::Permanent,
        Some(&(stacks, false)) => LockMode::Initial(stacks),
        None => LockMode::Initial(0),
    }
}

// The per-buff config application lives in the engine (shared with the
// optimizer); `BuffCfg` is its `BuffConfig`.

#[cfg(test)]
mod frenzy_default_tests {
    use super::*;

    /// A RULER SENDS NO BUFF MAP, THE PAGE SENDS THE CARD'S DEFAULT, and the two
    /// must be one fight — or the board ranks above what its own page reproduces.
    #[test]
    fn unconfigured_frenzy_is_the_cards_default() {
        assert_eq!(frenzy_lock_mode(None), frenzy_lock_mode(Some(&(0, false))));
        assert_eq!(frenzy_apply(None), frenzy_apply(Some(&(0, false))));
    }
}

#[cfg(test)]
mod arcane_slot_tests {
    use super::*;
    use crate::registry::weapon;

    /// An Arch-Gun seats TWO arcanes, one from each pool — "Archguns possess
    /// two Arcane Enhancement slots to equip one Primary Arcane and one
    /// Secondary Arcane" (wiki Arch-Gun) — and it is neither a Primary nor a
    /// Secondary weapon itself.
    #[test]
    fn an_archgun_seats_one_arcane_from_each_pool() {
        let lark = weapon("larkspur_prime");
        assert_eq!(lark.slot, "archgun", "its own equipment slot");
        assert_eq!(lark.arcane_pools, vec!["primary", "secondary"]);

        // Every other weapon still seats exactly one, named after its slot.
        assert_eq!(weapon("torid").arcane_pools, vec!["primary"]);
        assert_eq!(weapon("laetum").arcane_pools, vec!["secondary"]);
        // And a sentinel weapon seats none.
        assert!(weapon("verglas_prime").arcane_pools.is_empty());
    }

    /// Both chosen arcanes reach the sim, folded into one effect set. The
    /// pools are ORDERED, so entry i is the arcane for pool i and an id from
    /// the wrong pool resolves to nothing rather than being applied anyway.
    #[test]
    fn both_arcanes_apply_and_the_pools_stay_ordered() {
        let lark = weapon("larkspur_prime");
        let base = WeaponBase::from_data("larkspur_prime", true, &[]);
        let fx = |v: Value| arcane_fx_for(&v, lark, &base, StackPolicy::AssumedMax);

        let one = fx(json!({ "arcane": ["primary_deadhead"] }));
        let two = fx(json!({ "arcane": ["primary_deadhead", "cascadia_overcharge"] }));
        assert!(!one.id.is_empty(), "the primary alone resolves");
        assert!(two.id.contains('+'), "two folded: {}", two.id);
        assert!(
            two.crit_chance_relative > one.crit_chance_relative,
            "the secondary's crit chance joined: {} vs {}",
            two.crit_chance_relative,
            one.crit_chance_relative
        );

        // SWAPPED: each id is now in the other's slot, so neither is
        // equippable and nothing applies.
        let swapped = fx(json!({ "arcane": ["cascadia_overcharge", "primary_deadhead"] }));
        assert!(swapped.id.is_empty(), "wrong pool, wrong slot: {}", swapped.id);
    }

    /// ONE wire shape: a list, one entry per pool. A bare value is not a
    /// second spelling the server understands — the client rewrites storage
    /// to the list shape once, so nothing here reads two formats.
    #[test]
    fn the_wire_shape_is_a_list_and_only_a_list() {
        let torid = weapon("torid");
        let base = WeaponBase::from_data("torid", true, &[]);
        let fx = |v: Value| arcane_fx_for(&v, torid, &base, StackPolicy::AssumedMax);

        let listed = fx(json!({ "arcane": ["primary_deadhead"], "arcane_rank": [5] }));
        assert_eq!(listed.id, "primary_deadhead");
        assert!(listed.headshot_multiplier_bonus > 0.0);

        // A bare value is not a shape: it resolves to nothing rather than
        // being quietly accepted as a second way to say the same thing.
        assert!(fx(json!({ "arcane": "primary_deadhead" })).id.is_empty());

        // A weapon with one pool ignores a second entry: what it can seat is
        // the weapon's business, not the caller's.
        let extra = fx(json!({ "arcane": ["primary_deadhead", "cascadia_overcharge"] }));
        assert_eq!(extra.id, "primary_deadhead");
    }
}

/// EVERY BUFF CARD MUST HAVE A SIM ARM, AND EVERY SIM BUFF MUST HAVE A CARD.
///
/// `enumerate_buffs` (what is drawn) and `FightParams::buff_roster` (what the
/// fight runs) are two independent enumerations over the same data — one
/// matches `ModEffect` arms, the other reads resolved fields — so nothing but
/// a check makes them the same list. Both failure directions are silent on
/// screen: a card with no arm is a control that moves no number, and an armed
/// buff with no card cannot be configured or seen on the replay.
///
/// Written derived rather than listed (memory: derive triggers, don't list
/// them). It walks the whole roster and the whole mod pool, so a weapon, mod
/// or effect added later is covered without anyone remembering to come back.
#[cfg(test)]
mod card_and_sim_agree {
    use super::*;
    use crate::registry::weapon;
    use wfsim_engine::fight::FightParams;
    use wfsim_engine::build::loadout::resolve;
    use wfsim_engine::model::WeaponBase;
    use wfsim_engine::model::StackPolicy;

    /// Buffs the params do not own. `frenzy` is a weapon passive the api
    /// applies (`frenzy_apply`) rather than a field of the build; `arcane:*`
    /// ids come from the arcane and are checked by their own test below.
    fn theirs(id: &str) -> bool {
        id == "frenzy" || id.starts_with("arcane:")
    }

    fn roster_of(weapon: &str, refs: &[&ModDef]) -> Vec<String> {
        let base = WeaponBase::from_data(weapon, false, &[]);
        // EMERGENT, because that is the policy the fight runs under: a buff
        // rostered only at AssumedMax would be a card for a number the sim
        // never earns.
        let p = resolve(&base, refs, StackPolicy::Emergent);
        let params = FightParams::from_panel(
            &p,
            &wfsim_engine::arena::Arena::training(30.0),
            &wfsim_engine::data::arcanes::ArcaneFx::none(),
        );
        params.buff_roster().into_iter().map(|b| b.id).collect()
    }

    #[test]
    fn every_mod_that_draws_a_card_arms_the_sim() {
        let tenno = wfsim_engine::data::tenno::default_tenno().clone();
        let none = wfsim_engine::data::arcanes::ArcaneFx::none();
        let mut pairs = 0;
        for w in wfsim_engine::data::weapons::roster() {
            let info = weapon(&w.id);
            // A sentinel resolves BaseOnly: no conditional ever fires, so
            // `enumerate_buffs` returns nothing by design.
            if info.sentinel {
                continue;
            }
            for m in wfsim_engine::data::mods::pool_for_build(&w.id, &[]) {
                let refs = vec![&m];
                let cards: Vec<String> = enumerate_buffs(&refs, &refs, &none, info, &tenno)
                    .into_iter()
                    .map(|b| b.id)
                    .collect();
                let roster = roster_of(&w.id, &refs);
                pairs += 1;
                for c in cards.iter().filter(|c| !theirs(c)) {
                    assert!(
                        roster.contains(c),
                        "{}+{} draws a card `{c}` the sim never rosters: a control that moves no number",
                        w.id, m.id
                    );
                }
                for r in roster.iter().filter(|r| !theirs(r)) {
                    assert!(
                        cards.contains(r),
                        "{}+{} arms `{r}` in the sim with no card: unconfigurable, and absent from the replay",
                        w.id, m.id
                    );
                }
            }
        }
        assert!(pairs > 500, "the walk collapsed: only {pairs} weapon-mod pairs");
    }

    /// The same rule for ARCANES, whose cards come from a third enumeration.
    #[test]
    fn every_arcane_that_draws_a_card_arms_the_sim() {
        let tenno = wfsim_engine::data::tenno::default_tenno().clone();
        let mut seen = 0;
        for w in wfsim_engine::data::weapons::roster() {
            let info = weapon(&w.id);
            if info.sentinel {
                continue;
            }
            let base = WeaponBase::from_data(&info.id, true, &[]);
            for a in info
                .arcane_pools
                .iter()
                .flat_map(|p| wfsim_engine::data::arcanes::pool_for_weapon(&info.id, p))
            {
                let fx = a.fx(a.max_rank, StackPolicy::Emergent, base.traits, &tenno);
                let cards: Vec<String> = enumerate_buffs(&[], &[], &fx, info, &tenno)
                    .into_iter()
                    .map(|b| b.id)
                    .filter(|id| id.starts_with("arcane:"))
                    .collect();
                let params = FightParams::from_panel(
                    &resolve(&base, &[], StackPolicy::Emergent),
                    &wfsim_engine::arena::Arena::training(30.0),
                    &fx,
                );
                // A `tenno_scaled` arcane ARMS THE SIM AND DRAWS NO CARD, on
                // purpose: its value is a Warframe STAT
                // rather than a stack anybody earns, so a "0/1" knob for it
                // would invite switching off a number the frame simply has. Its
                // control is the Tenno block. So it is excluded from this
                // direction of the check rather than being a violation of it.
                //
                // IT ONLY SURFACED WHEN THE NEUTRAL FRAME GOT REAL STATS. While the default Tenno had 0 armor and
                // 0 energy, every passive arcane scaled to nothing and armed
                // nothing, so the two sides agreed by both being empty.
                let passive: Vec<String> = a
                    .fx(a.max_rank, StackPolicy::Emergent, base.traits, &tenno)
                    .buffs
                    .iter()
                    .filter(|b| b.trigger == wfsim_engine::model::ArcTrigger::Passive)
                    .map(|b| {
                        format!(
                            "arcane:{}",
                            if b.owner.is_empty() { a.id.clone() } else { b.owner.clone() }
                        )
                    })
                    .collect();
                let roster: Vec<String> = params
                    .buff_roster()
                    .into_iter()
                    .map(|b| b.id)
                    .filter(|id| id.starts_with("arcane:"))
                    .filter(|id| !passive.contains(id))
                    .collect();
                seen += 1;
                for c in &cards {
                    assert!(
                        roster.contains(c),
                        "{} on {} draws `{c}` with no sim arm",
                        a.id, w.id
                    );
                }
                for r in &roster {
                    assert!(
                        cards.contains(r),
                        "{} on {} arms `{r}` with no card",
                        a.id, w.id
                    );
                }
            }
        }
        assert!(seen > 50, "the walk collapsed: only {seen} weapon-arcane pairs");
    }
}

#[cfg(test)]
mod buff_event_cards {
    use super::*;
    use crate::registry::{evo_group, weapons};
    use crate::simulate::simulate_json;

    /// EVERY BUFF CARD SAYS WHAT TRIGGERS IT. The page greys a card
    /// whose events a scenario denies and the RUN drops the buff by the same
    /// tables, so a card resolving to NO trigger would be greyed by nothing and
    /// denied by nothing — indistinguishable from outside. An EMPTY list is a
    /// different claim and is allowed; what is asserted is that the id
    /// RESOLVED. Swept over the whole roster, which is where all three
    /// sources of a card live.
    #[test]
    fn every_buff_card_says_what_triggers_it() {
        let tenno = wfsim_engine::data::tenno::default_tenno().clone();
        let mut checked = 0usize;
        let mut orphans: Vec<String> = Vec::new();
        for info in weapons() {
            let pool = wfsim_engine::data::mods::pool_for_weapon(&info.id);
            let refs: Vec<&ModDef> = pool.iter().collect();
            let base = WeaponBase::from_data(&info.id, true, &[]);
            // ONE ARCANE AT A TIME: a card is keyed by its own arcane.
            let mut fxs = vec![wfsim_engine::data::arcanes::ArcaneFx::none()];
            for pool_id in &info.arcane_pools {
                for def in wfsim_engine::data::arcanes::slot_pool(pool_id) {
                    fxs.push(def.fx(def.max_rank, StackPolicy::Emergent, base.traits, &tenno));
                }
            }
            for fx in &fxs {
                for b in enumerate_buffs(&refs, &[], fx, info, &tenno) {
                    checked += 1;
                    if card_trigger(&b.id, &refs, fx).is_none() {
                        orphans.push(format!("{} on {}", b.id, info.id));
                    }
                }
            }
            // …AND THE EVOLUTIONS, whose cards never touch the mod pool.
            let group = evo_group(info);
            let evo_ids: Vec<String> = (1..=wfsim_engine::data::evolutions::tier_count(group))
                .flat_map(|t| wfsim_engine::data::evolutions::options(group, t))
                .map(|d| d.id.clone())
                .collect();
            for b in evo_buffs(&evo_ids) {
                checked += 1;
                if card_trigger(&b.id, &[], &wfsim_engine::data::arcanes::ArcaneFx::none())
                    .is_none()
                {
                    orphans.push(format!("{} on {}", b.id, info.id));
                }
            }
        }
        assert!(checked > 200, "the sweep found almost no cards: {checked}");
        orphans.sort();
        orphans.dedup();
        assert!(
            orphans.is_empty(),
            "these cards resolve to no trigger — add them to \
             `engine::buff_events::of_builtin`, or give the data a trigger:\n  {}",
            orphans.join("\n  ")
        );
    }

    /// …AND THE ONE ID THAT ANSWERS TWICE. `condition_overload` is a card on a
    /// pistol carrying Galvanized Strike and a card on a melee carrying the
    /// original, and the page has to grey the first under a kill-less fight
    /// and never grey the second — which is exactly what the run does.
    #[test]
    fn condition_overloads_card_says_what_that_weapons_mod_says() {
        let of = |weapon: &str| {
            let pool = wfsim_engine::data::mods::pool_for_weapon(weapon);
            let refs: Vec<&ModDef> = pool.iter().collect();
            card_trigger(
                "condition_overload",
                &refs,
                &wfsim_engine::data::arcanes::ArcaneFx::none(),
            )
        };
        assert_eq!(of("lex_prime"), Some(Some("kill")), "Galvanized Strike earns it on a kill");
        assert_eq!(of("magistar"), Some(None), "melee's Condition Overload earns it by nothing");
    }

    /// ONE CARD, ONE ANSWER: an arcane's specs must all want the same thing.
    ///
    /// A card is one per ARCANE — Primary Frostbite's crit damage and multishot
    /// come off the same Cold proc and are one count by construction — so its
    /// events are read off one spec. An arcane whose parts wanted DIFFERENT
    /// things would break that: the run drops each part on its own trigger,
    /// while the card can only be greyed or not, so half of it would keep
    /// working under a badge saying it does not.
    ///
    /// No arcane is shaped that way today. This is what makes that a fact
    /// rather than a coincidence — and the fix, when it fails, is a second card
    /// rather than a wider event list, because a wider list would grey a card
    /// whose other half still fires.
    #[test]
    fn an_arcanes_buffs_all_want_the_same_thing() {
        let tenno = wfsim_engine::data::tenno::default_tenno().clone();
        let mut split: Vec<String> = Vec::new();
        for info in weapons() {
            let base = WeaponBase::from_data(&info.id, true, &[]);
            for pool_id in &info.arcane_pools {
                for def in wfsim_engine::data::arcanes::slot_pool(pool_id) {
                    let fx = def.fx(def.max_rank, StackPolicy::Emergent, base.traits, &tenno);
                    let mut by_owner: std::collections::BTreeMap<&str, Vec<_>> = Default::default();
                    for b in &fx.buffs {
                        let owner = if b.owner.is_empty() { fx.id.as_str() } else { &b.owner };
                        by_owner.entry(owner).or_default().push(b.trigger);
                    }
                    for (arcane_id, trigs) in by_owner {
                        if trigs.windows(2).any(|w| w[0] != w[1]) {
                            split.push(format!("{arcane_id}: {trigs:?}"));
                        }
                    }
                }
            }
        }
        split.sort();
        split.dedup();
        assert!(
            split.is_empty(),
            "these arcanes grant buffs on different triggers under one card —              give each trigger its own card:
  {}",
            split.join("
  ")
        );
    }

    /// A DENIED TRIGGER REACHES THE NUMBER, through the ONE parse the simulator
    /// and the optimizer share. Asserted end to end, because "the field parsed"
    /// and "the run obeyed it" are the two halves that keep coming apart here.
    /// A NAME THE SERVER DOES NOT KNOW CHANGES NOTHING: this list travels in
    /// share links, so a newer vocabulary stays openable.
    #[test]
    fn denying_a_trigger_moves_the_number() {
        let fight = |off: Value| {
            simulate_json(&json!({
                "weapon": "laetum",
                "mode": "base",
                // AN ON-KILL BUILD, so the class under test is most of it, on a
                // target a shot does not always kill: one that dies to any hit
                // dies once a shot whatever the build earned (M119).
                "mods": ["galvanized_diffusion", "galvanized_shot", "primed_pistol_gambit"],
                "arcane": ["secondary_merciless"],
                "enemy": "crewman", "level": 60, "steel_path": false,
                "duration": 30, "runs": 12, "seed": 7, "metric": "kpm",
                "headshot_pct": 100, "aiming": true, "infinite_ammo": true,
                // The map PRESENT means the emergent sim, the path a player is
                // on; empty means every card at its default.
                "buffs": {},
                "buff_triggers_off": off,
            }))
        };
        let score = |v: &Value| v.get("score").and_then(Value::as_f64).unwrap_or(-1.0);

        let open = fight(json!([]));
        assert!(score(&open) > 0.0, "the control fight did not run: {open}");

        // …AND IT KILLED ENOUGH TO EARN THEM: a target that never dies denies
        // on-kill buffs by itself, and this would pass on an engine that
        // ignored the switch.
        let no_kills = fight(json!(["kill"]));
        assert!(
            score(&no_kills) > 0.0 && score(&no_kills) < score(&open) * 0.95,
            "denying kills moved the number by less than 5%, so either the switch              is ignored or this fight earns no kills to deny: {} vs {}",
            score(&no_kills),
            score(&open)
        );

        let unknown = fight(json!(["a_class_this_build_never_heard_of"]));
        assert_eq!(
            score(&unknown),
            score(&open),
            "an unknown trigger must be dropped, not obeyed and not refused"
        );
    }
}

#[cfg(test)]
mod secondary_outburst {
    use crate::panel::panel_json;
    use crate::simulate::simulate_json;
    use serde_json::{json, Value};

    fn req(extra: Value) -> Value {
        let mut v = json!({
            "weapon": "lex_prime", "mods": [],
            "enemy": "corrupted_heavy_gunner", "level": 100,
            "runs": 20, "seed": 7, "duration": 10,
        });
        for (k, x) in extra.as_object().expect("a mapping") {
            v[k] = x.clone();
        }
        v
    }

    fn crit_rate(v: Value) -> f64 {
        let r = simulate_json(&v);
        assert!(r.get("error").is_none(), "{r}");
        r["crit_rate"].as_f64().expect("a crit rate")
    }

    /// THE COUNT IS THE CARD'S: a card left at its default pays nothing, which
    /// is what every ruler runs, and a count set on it is held for the whole
    /// fight up to the 13x no melee exceeds.
    #[test]
    fn outburst_pays_the_combo_the_card_is_set_to_and_nothing_unset() {
        let worn = json!({"arcane": ["secondary_outburst"]});
        let card = panel_json(&req(worn.clone()))["buffs"]
            .as_array()
            .and_then(|a| a.iter().find(|b| b["id"] == "arcane:secondary_outburst").cloned())
            .expect("the arcane has a card");
        assert_eq!((card["max_stacks"].as_u64(), card["default_stacks"].as_u64()), (Some(13), Some(0)));

        let bare = crit_rate(req(json!({})));
        assert_eq!(crit_rate(req(worn)), bare, "an unset card must pay nothing");

        let set = |n: u32| {
            crit_rate(req(json!({
                "arcane": ["secondary_outburst"],
                "buffs": {"arcane:secondary_outburst": {"stacks": n, "locked": false}},
            })))
        };
        let (six, thirteen) = (set(6), set(13));
        assert!(bare < six && six < thirteen, "{bare} -> {six} -> {thirteen}");
        assert_eq!(set(20), thirteen, "13x is the cap");
    }
}

#[cfg(test)]
mod part_trigger_tests {
    fn of(axis: &str, id: &str) -> Vec<String> {
        super::part_triggers()[axis][id].as_array()
            .map(|a| a.iter().map(|x| x.as_str().unwrap().to_string()).collect())
            .unwrap_or_default()
    }

    /// A WEAK-POINT KILL IS NOT A KILL, on a part as in a fight: refusing one
    /// must leave the other's cards standing, so each is answered on its own.
    #[test]
    fn each_part_names_its_own_trigger() {
        assert_eq!(of("arcanes", "primary_deadhead"), ["headshot_kill"]);
        assert_eq!(of("arcanes", "primary_merciless"), ["kill"]);
        assert_eq!(of("arcanes", "primary_crux"), ["weakpoint_hit"]);
        // Held behind AIMING, and still a weak-point hit.
        assert_eq!(of("mods", "argon_scope"), ["headshot"]);
        assert_eq!(of("mods", "galvanized_scope"), ["headshot", "headshot_kill"]);
        // No card at all: an instant reload. The Phenmor's copy states a
        // KILL, which overrides what its kind fires on for the Furis.
        assert_eq!(of("evolutions", "lato_exact_penance"), ["kill"]);
        assert_eq!(of("evolutions", "furis_executioners_fortune"), ["headshot"]);
        assert_eq!(of("evolutions", "phenmor_executioners_fortune"), ["headshot_kill"]);
        assert!(super::part_triggers()["mods"].get("serration").is_none());
    }

    /// AN EFFECT NAMED FOR AN EVENT HAS ONE. A new kind that fires on a kill,
    /// a weak point, a status or a reload and is neither a card nor in
    /// `EVENT_KINDS` would let a refused trigger through the finder unseen.
    #[test]
    fn every_event_kind_is_read() {
        use serde_norway::Value as Y;
        let marks = ["on_kill", "per_kill", "on_headshot", "headshot_kill", "on_status", "status_hit", "empty_reload", "on_reload"];
        let mut missed = Vec::new();
        for (axis, dir) in [("mods", "mods/"), ("arcanes", "arcanes/"), ("evolutions", "evolutions/")] {
            for (path, text) in wfsim_engine::data::files_under(dir) {
                let Ok(doc) = serde_norway::from_str::<Y>(text) else { continue };
                let Some(id) = doc.get("id").and_then(Y::as_str) else { continue };
                let kinds: Vec<String> = serde_norway::to_string(doc.get("effects").unwrap_or(&Y::Null)).unwrap()
                    .lines().filter_map(|l| l.trim_start_matches(['-', ' ']).strip_prefix("kind: ").map(str::to_string)).collect();
                for k in kinds.iter().filter(|k| !k.starts_with("unmodelled") && marks.iter().any(|m| k.contains(m))) {
                    if of(axis, id).is_empty() {
                        missed.push(format!("{path}: {k}"));
                    }
                }
            }
        }
        assert!(missed.is_empty(), "event kinds that name no trigger:\n{}", missed.join("\n"));
    }
}
