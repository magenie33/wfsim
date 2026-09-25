// SPDX-License-Identifier: AGPL-3.0-or-later
//! `/api/panel`: the FINAL stats panel. Every bucket merged across the build,
//! each stated with its per-source breakdown ("who contributed what") — the
//! panel is where the model explains itself. Static view: max-rank mods;
//! conditionals at max stacks (AssumedMax), except sentinels where
//! conditionals never fire.

use serde_json::{json, Value};
// NO `resolve` HERE, and that is the point: it is the neutral-Tenno wrapper, and
// this panel was its last caller in this crate. Every endpoint resolves for the
// FIGHT's player, which is what "one player, both answers" means when it is true
// rather than intended.
use wfsim_engine::build::loadout::{resolve_for, ResolvedPanel};
use wfsim_engine::model::WeaponBase;
use wfsim_engine::model::{ModDef, ModEffect, StackPolicy};
use wfsim_engine::model::pct as fpct;
use crate::buffs::{arcane_choices, arcane_fx_for, buffs_json, enumerate_buffs, evo_buffs};
use crate::fight::chosen_evolutions;
use crate::registry::{WeaponInfo, attack_desc, base_for, default_weapon_id, form_unlock_evo, mod_not_here, weapon, wspec};
use crate::request::{err_json, get_str, prettify};
use crate::rivens::{mod_pool_with_rivens, riven_stat_ids_ok};
use crate::tenno::{floor_json, tenno_from, wielder_from};

/// A NUMBER AS A PANEL SHOWS IT — trimmed, not rounded to a fixed width.
///
/// `format!("{x}")` on an f64 gives the shortest string that reads back as the
/// same number, which is exactly right for a RECORD and exactly wrong for a
/// card: `1.0 - 0.7` is `0.30000000000000004`, and the falloff row printed
/// **"30.000000000000004% at 6.2 m"** on 27 of the roster's 141 falloff
/// entries — every weapon whose reduction is 0.7, 0.8 or 0.9.
///
/// Three decimals is past anything this data carries — the finest real figure
/// on these rows is a tenth of a metre — so the trim can never eat a digit
/// somebody wrote down. Trailing zeros and a trailing point go, so `6.2` stays
/// `6.2` and `30.0` reads `30`.
fn display_number(x: f64) -> String {
    let s = format!("{x:.3}");
    // ONLY WHERE THERE IS A POINT TO TRIM BACK TO. `{:.3}` always writes one,
    // so the guard is dead today — and it is here because the trim is
    // otherwise safe by an invariant of the line above it: on a string with no
    // decimal point, `trim_end_matches('0')` turns 30 into 3. Sabotaging the
    // format specifier to prove this function's own test bites printed exactly
    // that.
    let s = match s.split_once('.') {
        Some(_) => s.trim_end_matches('0').trim_end_matches('.'),
        None => s.as_str(),
    };
    // `-0` is a real f64 and never a thing a reader wants to see.
    if s == "-0" { "0".into() } else { s.to_string() }
}

pub fn panel_json(v: &Value) -> Value {
    let info = weapon(get_str(v, "weapon", default_weapon_id()));
    // THE PLAYER, RESOLVED ONCE AND FIRST, because the mod loop needs it: a
    // mod that scales off the Tenno has to say what THIS one is worth to it.
    // One call rather than two, because two `tenno_from` in one function is two
    // answers that can differ.
    let panel_tenno = tenno_from(v, info);
    let policy = if info.sentinel {
        StackPolicy::BaseOnly
    } else {
        StackPolicy::AssumedMax
    };
    // (`form` in the request is ignored: every available form renders.)
    let evos = match chosen_evolutions(v, info) {
        Ok(e) => e,
        Err(e) => return err_json(e),
    };
    let evo_refs: Vec<&str> = evos.iter().map(String::as_str).collect();

    let mod_ids: Vec<String> = v
        .get("mods")
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|m| m.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    // EIGHT MAIN, ONE EXILUS — AND A STANCE BESIDE THEM, which is a slot of its
    // own and not one of the nine. A melee build sends ten ids and every one of
    // them is legal; counting the flat list refused the full build outright.
    // `board::builds::validate_with` has subtracted the stances before comparing since
    // the slot landed, and this is the same subtraction.
    let stance_pool = wfsim_engine::data::mods::pool_naming(&info.id, &mod_ids);
    let stances = mod_ids
        .iter()
        .filter(|id| {
            stance_pool.iter().any(|m| m.id == id.as_str() && m.stance.is_some())
        })
        .count();
    if mod_ids.len() - stances > 9 {
        return err_json("at most 8 slots + 1 exilus");
    }
    // …AND ONE STANCE, because there is one slot for it. Two in a list is a
    // build nobody can hold, and admitting it would resolve two combo scripts
    // with only the first ever read — the same refusal `board::builds::validate_with`
    // has made since the slot landed.
    if stances > 1 {
        return err_json(format!("{stances} stances, and there is one stance slot"));
    }
    if let Err(e) = riven_stat_ids_ok(v, info) {
        return err_json(e);
    }
    let p = mod_pool_with_rivens(v, info, &evo_refs);
    let mut refs: Vec<&ModDef> = Vec::with_capacity(mod_ids.len());
    for id in &mod_ids {
        match p.iter().find(|m| m.id == id) {
            Some(m) => refs.push(m),
            None => return err_json(mod_not_here(id, info, &evo_refs)),
        }
    }

    // ---- forms: EVERY available form renders side by side (no switching;
    // user decision). The Incarnon Form section exists only while its
    // tier-1 unlock is selected. `meta` states the trigger/shot mechanics
    // from the weapon data (data/weapons yamls).
    // Section titles come from the REGISTERED form (`data/weapons` `form:`),
    // so a bow's first section says "Charged Strike" rather than "Base Form".
    let mut forms_list: Vec<(&'static str, String, WeaponBase)> = Vec::new();
    for f in wfsim_engine::data::weapons::forms_of(&info.id) {
        // A gauge-switched form exists only while its tier-1 unlock is chosen.
        // THE ADAPTER, not the gauge: this hides a form until its unlock is in
        // the build, and a form that needs no adapter is never hidden.
        if f.kind.is_adapter_form()
            && !form_unlock_evo(info).is_some_and(|u| evo_refs.contains(&u))
        {
            continue;
        }
        forms_list.push((
            wspec(f.weapon_id).form_label(),
            attack_desc(wspec(f.weapon_id)),
            base_for(v, f.weapon_id, &evo_refs),
        ));
    }

    let (src, mut conditionals) = mod_sources(v, info, &refs, &forms_list, policy, &panel_tenno);
    let (evo_src, evo_flat) = evolution_sources(info, &evo_refs, &panel_tenno);
    // READ BEFORE THE SECTIONS, because one of their rows depends on it — the
    // same `arcane_fx` the buff cards below use, hoisted rather than built
    // twice.
    let arcane_fx = arcane_fx_for(v, info, &forms_list[0].2, policy);
    let inputs = SectionInputs {
        v, info, refs: &refs, policy, src, evo_src, evo_flat,
        compression_per_m: arcane_fx.compression_damage_per_m,
    };

    // ONE PLAYER, BOTH ANSWERS. These rows resolved against
    // the NEUTRAL Tenno while the sim resolved against the fight's, so every
    // player-gated grant in the roster was paid by the sim and absent from the
    // panel: Haven Foray's "+50 with Overshields", Guardian's Might's +74, the
    // channeled-ability family, every "With Sprint Speed 1.2 or Higher". The
    // buff-card half of this endpoint was fixed then and the STATS half was
    // not, which is why it stayed invisible — the numbers it hid were on
    // exactly the cards nobody had a control for yet.
    //
    // Found by the loadout knob: Lone Gun's "+14 Base Magazine
    // Capacity" moved the sim's magazine to 20 and the Magazine row still read
    // 6.
    let forms: Vec<Value> = forms_list
        .iter()
        .map(|(label, meta, b)| {
            form_section(label, meta, b, &resolve_for(b, &refs, policy, &panel_tenno), &inputs)
        })
        .collect();

    // Configurable buffs of this build (weapon-scoped) for the Sim panel —
    // mods + arcane + the weapon passive, plus evolution-granted buffs
    // (Fevered Frenzy's permanent stacks).
    //
    // A real build: every mod on it is on it, so every lock is real.
    let mut buffs = enumerate_buffs(&refs, &refs, &arcane_fx, info, &tenno_from(v, info));
    for b in evo_buffs(&evos) {
        if !buffs.iter().any(|x| x.id == b.id) {
            buffs.push(b);
        }
    }

    // A SET WHOSE MEMBERS ARE NOT ALL WEAPON MODS HAS A LOWER CEILING HERE than
    // it does in game, and the player has to be told the number rather than
    // reading 20% and assuming it is the cap.
    //
    // "Set Mods … offer increasing bonuses when one or more mods in a set are
    // equipped on the player's Warframe AND weapons … Only the number of
    // equipped mods within the set dictates the Set Bonus strength" (wiki,
    // Set_Mods). The Vigilante set is six, and two of them — Vigor and Pursuit
    // — go on the FRAME, which this engine has no loadout for. So a weapon
    // build tops out at 4 x 5% = 20% against the game's 30%.
    //
    // DERIVED, not written down: `members` is the set's real size and the
    // weapon-side count is however many of our mods name it, so a set we later
    // complete stops printing this on its own.
    {
        use std::collections::BTreeSet;
        let mut said: BTreeSet<&str> = BTreeSet::new();
        for m in &refs {
            let Some(set_id) = m.set else { continue };
            if !said.insert(set_id) {
                continue;
            }
            let Some(def) = wfsim_engine::data::mod_sets::set_def(set_id) else { continue };
            let ours = wfsim_engine::data::mod_sets::members_carried(set_id);
            if ours >= def.members {
                continue;
            }
            conditionals.push(json!({
                "mod": def.name,
                "desc": format!("set bonus: {:.0}% per equipped member", def.per_mod * 100.0),
                "active": true,
                "why": format!(
                    "{} of the set's {} members are weapon mods — the rest go on the WARFRAME,                      which this sim has no loadout for. So a build here tops out at {:.0}%,                      against {:.0}% in game.",
                    ours, def.members, f64::from(ours) * def.per_mod * 100.0,
                    f64::from(def.members) * def.per_mod * 100.0),
            }));
        }
    }

    // A FORM THAT CANNOT ZOOM CANNOT BE AIMING, and a mod that pays nothing has
    // to say so on the page rather than just resolving to zero. The engine
    // already answers the aim question FALSE for such a form; this is the half
    // the player can see.
    //
    // Named per FORM, because that is the granularity of the fact: the Vasto
    // aims and its Incarnon form does not, so "your Galvanized Crosshairs does
    // nothing" would be wrong and "it does nothing in Incarnon Form" is right.
    for (form_name, _label, fb) in &forms_list {
        if !fb.cannot_zoom {
            continue;
        }
        for m in &refs {
            let aim_gated = m.effects.iter().any(|e| {
                matches!(e, wfsim_engine::model::ModEffect::WhileTenno(
                    wfsim_engine::model::TennoCondition::Aiming, _))
            });
            if aim_gated {
                conditionals.push(json!({
                    "mod": m.name,
                    "desc": "pays only while aiming",
                    "active": false,
                    "why": format!(
                        "{form_name} cannot aim down sights (the card's own words are \"cannot Zoom\"), so an on-aim bonus never applies in it"),
                }));
            }
        }
    }

    json!({
        "ok": true,
        "weapon": info.name,
        "policy": if info.sentinel { "base only (sentinel)" } else { "conditionals at max stacks" },
        "forms": forms,
        "conditionals": conditionals,
        // WHO IS HOLDING THE GUN, as numbers rather than as an assumption. Several perks and mods read the player and the
        // panel showed none of it: "0 = no frame" is what the FIELDS say, and a
        // reader had no way to see what the fight actually resolved to once a
        // frame, an aura or an archon shard had moved it.
        //
        // Reported by the SERVER because it is the server that built it —
        // `tenno_from` applies the frame, then the typed overrides, then what
        // the squad brings, and a page re-deriving that would be a second
        // implementation of the one thing every gated perk is asked about.
        "tenno": {
            "health": panel_tenno.health,
            "shield": panel_tenno.shield,
            "armor": panel_tenno.armor,
            "energy": panel_tenno.energy,
            "sprint": panel_tenno.sprint,
        },
        // …AND THE WIELDER BEFORE THE FIGHT'S OVERRIDES, which is what a ticked
        // override starts from and what an unticked one falls back to.
        "wielder": floor_json(&wielder_from(v, info)),
        "buffs": buffs_json(&buffs),
    })
}

/// One mod's share of a bucket: (key, mod name, contribution fraction, note).
type ModSource = (&'static str, String, f64, Option<String>);
/// A share that is not a mod's: (key, source name, PRE-FORMATTED value, note).
type OtherSource = (&'static str, String, String, Option<String>);

/// WHAT EVERY FORM'S SECTION READS beside its own base and panel: the request,
/// the weapon, its mods and policy, and who contributed what.
struct SectionInputs<'a> {
    v: &'a Value,
    info: &'a WeaponInfo,
    refs: &'a [&'a ModDef],
    policy: StackPolicy,
    src: Vec<ModSource>,
    evo_src: Vec<OtherSource>,
    /// The flat base additions the evolutions make — base damage, crit chance,
    /// status chance, magazine — which a row subtracts to show the raw base.
    evo_flat: (f64, f64, f64, f64),
    /// Primary Compression's per-metre damage ramp, or 0 with no such arcane on
    /// the build. The BLAST RADIUS row needs it: the arcane keeps a fifth of the
    /// sphere and the panel is resolved without knowing what is equipped.
    compression_per_m: f64,
}

/// Every equipped mod's share of each bucket, and the lines that never merge
/// into one.
fn mod_sources(
    v: &Value,
    info: &WeaponInfo,
    refs: &[&ModDef],
    forms_list: &[(&'static str, String, WeaponBase)],
    policy: StackPolicy,
    panel_tenno: &wfsim_engine::data::tenno::Tenno,
) -> (Vec<ModSource>, Vec<Value>) {
    // ---- per-bucket source attribution (mirrors resolve()'s buckets) ----
    // key -> [(mod name, contribution fraction, note)]
    let mut src: Vec<(&'static str, String, f64, Option<String>)> = Vec::new();
    let mut conditionals: Vec<Value> = Vec::new(); // lines that never merge into a bucket
    for m in refs {
        let name = m.name.to_string();
        // A SET THAT ENHANCES ITS OWN MEMBERS SAYS SO ON THE CARD. What the mod
        // is worth depends on what is beside it, which is exactly what this
        // list is for — and the row is drawn dim when the set is short, so a
        // reader can see the 25% they are one card away from.
        let self_scale = wfsim_engine::data::mod_sets::self_scale_for(m, refs);
        if let Some(set) = m.set.and_then(wfsim_engine::data::mod_sets::set_def) {
            if set.kind == wfsim_engine::data::mod_sets::SetBonusKind::SelfScaling {
                let have = refs.iter().filter(|x| x.set == Some(set.id)).count();
                conditionals.push(json!({
                    "mod": name,
                    "desc": format!(
                        "{} set — every member is worth {} more once all {} are equipped",
                        set.name,
                        wfsim_engine::model::pct(set.per_mod),
                        set.members,
                    ),
                    "active": self_scale > 1.0,
                    "why": if self_scale > 1.0 {
                        format!("all {} equipped, so this card's own numbers are {} higher",
                            set.members, wfsim_engine::model::pct(set.per_mod))
                    } else {
                        format!("{have} of {} equipped, so this card is worth its face", set.members)
                    },
                }));
            }
        }
        for e in &m.effects {
            use ModEffect::*;
            let before = src.len();
            let mut push = |key: &'static str, v: f64, note: Option<String>| {
                src.push((key, name.clone(), v, note));
            };
            // A player-gated effect still LISTS, tagged with the state it
            // waits on: the reader needs to see that the mod contributes, and
            // under what condition. When the fight's Tenno is not doing it the
            // row says what the mod WOULD give and gives nothing — which is
            // the whole reason the condition is modelled rather than folded
            // in. Unwrap here, let the ordinary arms push, tag them below.
            let (e, tenno_gate): (&ModEffect, Option<String>) = match e {
                WhileTenno(c, inner) => (&**inner, Some(c.describe())),
                other => (other, None),
            };
            match *e {
                WhileTenno(..) => unreachable!("unwrapped above"),
                // JAHU CANTICLE. Not a term in any bucket — it takes armour off
                // OTHER bodies, so what it is worth is a property of the fight
                // rather than of the build, and it says so on its own line the
                // way Acid Shells does.
                // THE INVOCATIONS. Two of them raise a knob that belongs to the
                // FIGHT rather than to any bucket on this weapon, and two pay
                // nothing at all — so all four say what they do on a line of
                // their own, and the two that pay nothing say why.
                AbilityStat(stat, ..) => {
                    conditionals.push(json!({
                        "mod": name,
                        "desc": e.describe(),
                        "active": stat.unmodelled_reason().is_none(),
                        "why": stat.unmodelled_reason().unwrap_or(
                            "it raises the FIGHT's own Warframe-buff knob, so it is worth exactly what the buff you have ticked is worth — and nothing with none ticked",
                        ),
                    }));
                }
                StripOnKillInRange(..) => {
                    conditionals.push(json!({
                        "mod": name,
                        "desc": e.describe(),
                        "active": true,
                        "why": "it needs KILLS and it needs a crowd — against one target there is nobody left to strip, and against none that dies it never fires at all",
                    }));
                }
                // ACID SHELLS. Three columns of one card, each its own effect,
                // and none of them a term in anybody's damage sum — what it
                // produces is an explosion at a CORPSE, so the panel says what
                // that explosion is and leaves the buckets alone.
                AcidShells(_) => {
                    conditionals.push(json!({
                        "mod": name,
                        "desc": e.describe(),
                        "active": true,
                        "why": "on every kill, the corpse explodes and can kill again — the chain is what this mod is for, and it needs a crowd to be worth anything",
                    }));
                }
                // NIGHTWATCH NAPALM. A whole attack PART rather than a bucket
                // line — it leaves a field the weapon does not have — so it
                // states what the fire is and where it lands rather than
                // pretending to be a term in someone's damage sum.
                GrantsLingering(field) => {
                    conditionals.push(json!({
                        "mod": name,
                        "desc": e.describe(),
                        "active": true,
                        "why": format!(
                            "a fire that ticks {} Heat for {}s — it cannot crit, its {}% status is not moved by mods, and only base-damage and faction mods scale it",
                            field.base_vector.total(),
                            field.duration_seconds,
                            (field.base_status_chance * 100.0).round()),
                    }));
                }
                // …and the share of the blast it covers, stated beside it.
                LingeringAreaFraction(v) => {
                    conditionals.push(json!({
                        "mod": name,
                        "desc": e.describe(),
                        "active": true,
                        "why": format!(
                            "the fire covers {}% of this rocket's explosion AREA, so a blast-radius mod grows it too",
                            (v * 100.0).round()),
                    }));
                }
                // HARKONAR SCOPE. A CONDITIONAL and not a stat row: the seconds
                // it adds are worth nothing unless the fight has a combo
                // counter at all, which is a question about the WEAPON (a
                // sniper) and about the fight (scoped in). The panel states
                // both rather than printing "+12s" beside a weapon that has no
                // window — see `build::loadout::ModEffect::ComboDuration`.
                ComboDuration(v) => {
                    // The WEAPON's own window, read the same way the buff
                    // card's roster reads it one screen up.
                    let window = wfsim_engine::data::weapons::spec(&info.id)
                        .and_then(|w| w.sniper_combo)
                        .map(|c| c.seconds);
                    conditionals.push(json!({
                        "mod": name,
                        "desc": e.describe(),
                        "active": window.is_some(),
                        "why": match window {
                            Some(w) => format!(
                                "this weapon's combo drops one stack after {w}s without a landing hit, and this makes it {}s — worth nothing from the hip, where there is no counter",
                                w + v),
                            None => "only a SNIPER carries a combo counter, and this weapon has none for the seconds to extend".to_string(),
                        },
                    }));
                }
                // DOUBLE TAP is a CONDITIONAL, not a bucket line: it is worth
                // nothing until the hits are consecutive and it stands on its
                // own multiplier, so folding it into base damage would both
                // overstate a fresh magazine and put it in the wrong bracket.
                // SYNTH CHARGE is a CONDITIONAL, not a bucket line: it is worth
                // nothing on every round but the magazine's last, and it stands
                // on its own multiplier — folding it into base damage would
                // both overstate every shot and put it in the wrong bracket.
                LastRoundDamage(x) => {
                    conditionals.push(json!({
                        "mod": name,
                        "desc": e.describe(),
                        "active": x > 0.0,
                        "why": if x > 0.0 {
                            "the magazine's LAST round only, and multiplicative with Hornet Strike rather than additive with it".to_string()
                        } else {
                            "nothing here: the mod has no effect on a continuous weapon or on an Incarnon fire mode, whatever its magazine".to_string()
                        },
                    }));
                }
                // THE CHAMBER FAMILY, the same shape from the other end of the
                // magazine — and it is a conditional for the same reason: it
                // pays one round in `magazine` and stands on its own
                // multiplier, so a bucket line would overstate every shot and
                // put it in the wrong bracket.
                FirstRoundDamage(x) => {
                    conditionals.push(json!({
                        "mod": name,
                        "desc": e.describe(),
                        "active": x > 0.0,
                        "why": "the magazine's FIRST round only — the counter has to read one under full AFTER the shot, so an ammo-efficiency source that leaves the magazine full pays nothing at all",
                    }));
                }
                // DEGREES OF CONE, not a damage stat and not the accuracy
                // bucket — the panel's accuracy row would claw it back with
                // every accuracy mod on the build, which is the one thing the
                // source says cannot happen.
                AddedSpread(x) => {
                    conditionals.push(json!({
                        "mod": name,
                        "desc": e.describe(),
                        "active": x > 0.0,
                        "why": "a WIDER cone, so it costs nothing at contact and grows with the distance on the arena floor — and no accuracy mod on this build takes any of it back",
                    }));
                }
                // A MOD-GRANTED STACKING BUFF gets a CARD (see
                // `enumerate_buffs`), which is where its stack count is set —
                // so here it only has to say what it is and what earns it.
                GrantsStackingBuff(b) => {
                    conditionals.push(json!({
                        "mod": name,
                        "desc": e.describe(),
                        "active": true,
                        "why": format!(
                            "earned in the fight and lost {} — the card beside the build sets how many stacks it opens with",
                            match b.decay {
                                wfsim_engine::model::BuffDecay::AllAtOnce =>
                                    format!("WHOLE, {}s after the last one", b.duration),
                                wfsim_engine::model::BuffDecay::PerStackExpiry =>
                                    format!("one at a time, each {}s after it was earned", b.duration),
                                wfsim_engine::model::BuffDecay::LoseOneAndReset =>
                                    format!("one at a time, {}s after the last one", b.duration),
                            }),
                    }));
                }
                ConsecutiveHitDamage { per_stack, max_stacks, duration } => {
                    conditionals.push(json!({
                        "mod": name,
                        "desc": e.describe(),
                        "active": true,
                        "why": format!(
                            "counted per TRIGGER PULL, not per pellet: every pellet of a pull gets                              {:.0}% x (hits so far including this pull - 1), capped at {max_stacks}                              stacks, and the pile lapses {duration}s after the last hit",
                            per_stack * 100.0),
                    }));
                }
                BaseDamage(x) => push("base_damage", x, None),
                Multishot(x) => push("multishot", x, None),
                CritChance(x) => push("crit_chance", x, None),
                CritDamage(x) => push("crit_damage", x, None),
                StatusChance(x) => push("status_chance", x, None),
                StatusDamage(x) => push("status_damage", x, None),
                // Its own row: the chance is not a status chance and does not
                // pool with one - it is a separate roll off a critical hit.
                SlashOnCrit(x) => push("slash_on_crit", x, None),
                AmmoEfficiency(x) => push("ammo_efficiency", x, None),
                FireRate(x) => push("fire_rate", x, None),
                // Its own row, not fire rate's: a charge-rate mod shortens the
                // DRAW and leaves an uncharged form's cadence alone.
                ChargeRate(x) => push("charge_rate", x, None),
                ReloadSpeed(x) => push("reload", x, None),
                Element(t, x) | CombinedElement(t, x) => {
                    src.push(("elements", name.clone(), x, Some(format!("{t:?}"))));
                }
                // Physical (IPS) mod: scales the base of that physical type.
                Physical(t, x) => {
                    src.push(("physical", name.clone(), x, Some(format!("{t:?}"))));
                }
                // SENTIENT SURGE: the bonuses scale with ACTIVE TENDRILS, and
                // a tendril costs a kill — so the panel cannot state a number
                // without assuming a fight. It states the CAP under
                // assumed-max (4 tendrils, the Ocucor's own limit, which is
                // where the wiki's "up to 240%" comes from) and lists it as a
                // conditional otherwise, the same shape every other on-kill
                // mod here takes.
                //
                // `tendril_max` is read off the WEAPON rather than written
                // into the mod, so the cap cannot disagree with the passive
                // that produces it.
                // The refill buys uptime, not damage, so it has no bucket to
                // join — it is the reason the bonuses above survive, and it is
                // listed on the card rather than attributed to a stat.
                MagazineRefillOnKill(..) => {}
                // A SYNDICATE RADIAL is not a stat bucket — it is a flat
                // explosion on its own clock, so there is no percentage to
                // attribute to a damage source. The card states it in full
                // (`describe`) and the sim reports what it dealt under its own
                // heading.
                SyndicateRadial { .. } => {}
                PerTendril { crit_chance, status_chance } => {
                    let cap = f64::from(
                        wfsim_engine::data::weapons::spec(&info.id)
                            .and_then(|w| w.tendrils)
                            .map_or(0, |t| t.max),
                    );
                    match policy {
                        StackPolicy::BaseOnly => conditionals.push(json!({
                            "mod": name, "desc": e.describe(), "active": false,
                            "why": "the bonus scales with active tendrils, and a tendril costs a kill — none are up at the start of a fight, and a reload clears every one"})),
                        _ => {
                            let note = Some(format!("{cap} tendrils assumed (the cap)"));
                            push("crit_chance", crit_chance * cap, note.clone());
                            push("status_chance", status_chance * cap, note);
                        }
                    }
                }
                OnKillMultishot {
                    per_stack,
                    max_stacks,
                    ..
                } => match policy {
                    StackPolicy::BaseOnly => conditionals.push(json!({
                        "mod": name, "desc": e.describe(), "active": false,
                        "why": "a companion weapon cannot TRIGGER this - the on-kill roll comes from the Tenno's own weapons - and this arena simulates one weapon alone, so the stacks the Tenno would share never arrive"})),
                    _ => push(
                        "multishot",
                        per_stack * max_stacks as f64,
                        Some(format!("on kill, {max_stacks} stacks assumed")),
                    ),
                },
                ConditionOverload {
                    per_stack,
                    max_stacks,
                    ..
                } => match policy {
                    StackPolicy::BaseOnly => conditionals.push(json!({
                        "mod": name, "desc": e.describe(), "active": false,
                        "why": "a companion weapon cannot TRIGGER this - the on-kill roll comes from the Tenno's own weapons - and this arena simulates one weapon alone, so the stacks the Tenno would share never arrive"})),
                    _ => push(
                        "co",
                        per_stack * max_stacks as f64,
                        Some(format!(
                            "on kill, {max_stacks} stacks assumed, per status type on target"
                        )),
                    ),
                },
                // LEADED GAS. The STATUS half is a row; the ELEMENT half is a
                // share of the base added to the damage vector, so it is a
                // damage row nowhere and belongs to the card's own line.
                OnWeakpointElementAndStatus { bonus, .. } => match policy {
                    StackPolicy::BaseOnly => conditionals.push(json!({
                        "mod": name, "desc": e.describe(), "active": false,
                        "why": "sentinel weapons cannot headshot"})),
                    _ => push(
                        "status_chance",
                        bonus,
                        Some("on a weak-point hit, buff assumed up".into()),
                    ),
                },
                OnHeadshotCritChance { bonus, .. } => match policy {
                    StackPolicy::BaseOnly => conditionals.push(json!({
                        "mod": name, "desc": e.describe(), "active": false,
                        "why": "sentinel weapons cannot headshot"})),
                    _ => push(
                        "crit_chance",
                        bonus,
                        Some("on headshot, buff assumed up".into()),
                    ),
                },
                OnHeadshotKillCritChance {
                    per_stack,
                    max_stacks,
                    ..
                } => match policy {
                    StackPolicy::BaseOnly => conditionals.push(json!({
                        "mod": name, "desc": e.describe(), "active": false,
                        "why": "sentinel weapons cannot headshot"})),
                    _ => push(
                        "crit_chance",
                        per_stack * max_stacks as f64,
                        Some(format!("on headshot kill, {max_stacks} stacks assumed")),
                    ),
                },
                Indirect(stat, x) => {
                    src.push(("indirect", name.clone(), x, Some(stat.label().to_string())));
                }
                OnEquipHandling { .. } => conditionals.push(json!({
                    "mod": name, "desc": e.describe(), "active": true,
                    "why": "temporary on weapon swap-in; never a static stat"})),
                // Faction bonus is conditional on the target's faction, so the
                // static panel lists it rather than folding it into a bucket.
                FactionDamage(fac, x) => conditionals.push(json!({
                    "mod": name, "desc": e.describe(), "active": false,
                    "why": format!("+{}% total damage only vs {fac:?} (applied ×2 on DoT ticks)",
                        (x * 100.0).round())})),
                MagazineCapacity(x) => push("magazine", x, None),
                // Attributed on the radius rows of whichever parts have one.
                BlastRadius(x) => push("radius", x, None),
                StatusDuration(x) => push("status_duration", x, None),
                // Conditional buff, assumed active at max in this static panel.
                CondBuff(b, x) => {
                    use wfsim_engine::model::CondBucket as B;
                    let key = match b {
                        B::BaseDamage => "base_damage",
                        B::Multishot => "multishot",
                        B::CritChance => "crit_chance",
                        B::CritDamage => "crit_damage",
                        B::StatusChance => "status_chance",
                        B::StatusDamage => "status_damage",
                        B::FireRate => "fire_rate",
                        B::ReloadSpeed => "reload",
                    };
                    // "assumed active" is exactly what this panel is: it
                    // resolves under AssumedMax, so the number belongs here.
                    // What it does NOT mean is that the SIM has it — see
                    // MECHANICS "Conditional buffs with no live model".
                    push(key, x, Some("conditional buff, assumed active".into()));
                }
                // Weak-point effects: conditional on the part hit — listed,
                // never folded into a static bucket.
                WeakpointDamage(x) => conditionals.push(json!({
                    "mod": name, "desc": e.describe(), "active": true,
                    "why": format!("+{}% added to the weak-point multiplier ON weak-point hits \
                        (1.5× listed on true weak points)", (x * 100.0).round())})),
                WeakpointCritChance(x) => conditionals.push(json!({
                    "mod": name, "desc": e.describe(), "active": true,
                    "why": format!("+{}% relative crit chance ON weak-point hits only",
                        (x * 100.0).round())})),
                OnKillCritDamage { bonus, .. } => match policy {
                    StackPolicy::BaseOnly => conditionals.push(json!({
                        "mod": name, "desc": e.describe(), "active": false,
                        "why": "a companion weapon cannot TRIGGER this - the on-kill roll comes from the Tenno's own weapons - and this arena simulates one weapon alone, so the stacks the Tenno would share never arrive"})),
                    _ => push(
                        "crit_damage",
                        bonus,
                        Some("on kill, buff assumed up".into()),
                    ),
                },
                OnReloadDamage { bonus, .. } => match policy {
                    StackPolicy::BaseOnly => conditionals.push(json!({
                        "mod": name, "desc": e.describe(), "active": false,
                        "why": "a companion weapon cannot TRIGGER this - the reload is the Tenno's - and this arena simulates one weapon alone, so the buff the Tenno would share never arrives"})),
                    _ => push(
                        "base_damage",
                        bonus,
                        Some("on reload from empty, buff assumed up".into()),
                    ),
                },
                // EXIMUS ADVANTAGE. The bucket line is the same as Deadly
                // Efficiency's, but the NOTE has to carry the target, because
                // that is the half a panel cannot check: the same build against
                // a non-Eximus gets none of this and the number would otherwise
                // read as unconditional.
                OnEximusWeakpointDamage { bonus, .. } => match policy {
                    StackPolicy::BaseOnly => conditionals.push(json!({
                        "mod": name, "desc": e.describe(), "active": false,
                        "why": "it takes a weak-point hit on an EXIMUS to open the window, and a companion weapon's hits are not the Tenno's"})),
                    _ => push(
                        "base_damage",
                        bonus,
                        Some("on an Eximus weak-point hit, buff assumed up".into()),
                    ),
                },
                // HATA-SATYA. Its ceiling is a number DE publishes (500%), so
                // assumed-max has something honest to state — unlike the
                // tendrils, whose cap comes from the weapon. What the panel
                // cannot say is how long it takes to get there: 416 hits at max
                // rank, and a reload puts it back to zero.
                CritChancePerHit(c) => match policy {
                    StackPolicy::BaseOnly => conditionals.push(json!({
                        "mod": name, "desc": e.describe(), "active": false,
                        "why": "the pile is built by this weapon's own hits and cleared by its reload, and a companion weapon's hits are not the Tenno's"})),
                    // THE CEILING ITSELF, not a stack count times a rate: what
                    // DE publishes is the number, and the hit count under it is
                    // ours to derive.
                    _ => push(
                        "crit_chance",
                        c.max_bonus,
                        Some(format!("{} hits assumed (the {} cap)", c.max_stacks(), wfsim_engine::model::pct(c.max_bonus))),
                    ),
                },
                OnReloadFireRate { bonus, .. } => match policy {
                    StackPolicy::BaseOnly => conditionals.push(json!({
                        "mod": name, "desc": e.describe(), "active": false,
                        "why": "a companion weapon cannot TRIGGER this - the reload is the Tenno's - and this arena simulates one weapon alone, so the buff the Tenno would share never arrives"})),
                    _ => push(
                        "fire_rate",
                        bonus,
                        Some("on reload, buff assumed up".into()),
                    ),
                },
                // ---- MELEE COMBO. All five are LIVE numbers, so none of them
                // is a panel stat: what a card is worth here depends on where
                // the combo counter is at the instant of the swing, and the
                // panel describes a weapon at rest. They are stated as
                // conditionals for the same reason Double Tap's stacks are —
                // the alternative is a crit-chance figure on the panel that no
                // swing in the fight ever has.
                // THE FIVE THAT PAY ONLY IN SOME MODES, or not as a stat at
                // all. Each says what it is worth and WHERE, because a panel
                // number would be a lie in five of the seven melee modes.
                MeleeComboDurationMultiplier(_) | MeleeRange(_) => conditionals.push(json!({
                    "mod": name, "desc": e.describe(), "active": true,
                    "why": "it changes the fight rather than a damage stat — reach decides how many                             bodies a swing reaches, and the clock decides how long the counter lives"})),
                SlamDamage(_) => conditionals.push(json!({
                    "mod": name, "desc": e.describe(), "active": true,
                    "why": "it pays on a SLAM and on nothing else, so it is worth zero in every                             mode that swings"})),
                // A CRIT CARD WHOSE VALUE DEPENDS ON THE MODE. The panel shows
                // the weapon at rest, and at rest there is no swing to be a
                // heavy one — so it is a line rather than a number, the same
                // answer the two cards below get.
                // TENNOKAI IS A BEHAVIOUR, not a stat: it turns a swing into a
                // heavy attack when a window is open, so there is no number the
                // panel could put on a weapon at rest.
                Tennokai { .. } => conditionals.push(json!({
                    "mod": name, "desc": e.describe(), "active": true,
                    "why": "a window this build opens during the fight, in which a heavy attack                             costs no combo — so what it is worth depends on how much counter the                             build has climbed to when it fires"})),
                CritChanceHeavyDoubled(_) => conditionals.push(json!({
                    "mod": name, "desc": e.describe(), "active": true,
                    "why": "the card reads x2 on a heavy attack, so what it is worth depends on                             which of the seven melee modes this build is"})),
                CritChanceOnSlide(_) => conditionals.push(json!({
                    "mod": name, "desc": e.describe(), "active": true,
                    "why": "it names an attack — a SLIDE — so it is worth its whole number in                             that mode and exactly nothing in the other six"})),
                HeavyWindUpSpeed(_) => conditionals.push(json!({
                    "mod": name, "desc": e.describe(), "active": true,
                    "why": "it shortens the CHARGE before a heavy swing, which attack speed does                             not touch — so it pays in the two heavy modes and nowhere else"})),
                HeavyAttackDamage(_) => conditionals.push(json!({
                    "mod": name, "desc": e.describe(), "active": true,
                    "why": "it pays on a HEAVY attack and on nothing else"})),
                ComboCountChance(_) => conditionals.push(json!({
                    "mod": name, "desc": e.describe(), "active": true,
                    "why": "extra combo points per hit — what they are worth depends on what reads                             the counter, which is Blood Rush and Weeping Wounds in a combo mode                             and the heavy multiplier in a heavy one"})),
                ComboGainChance(_) => conditionals.push(json!({
                    "mod": name, "desc": e.describe(), "active": true,
                    "why": "a GATE on every base combo point a hit earns, not a share of Additional Combo                             Count Chance — a lost point takes whatever that chance gave it, and a                             hit left with none does not hold the combo timer (MEASUREMENTS M97)"})),
                ComboCountChanceOnLifted(_) => conditionals.push(json!({
                    "mod": name, "desc": e.describe(), "active": true,
                    "why": "its gate is a status this fight tracks: every heavy slam forces Lifted, and                             a light combo forces none — so what it pays is decided by the mode                             rather than assumed"})),
                StatusChanceOnLifted(_) => conditionals.push(json!({
                    "mod": name, "desc": e.describe(), "active": true,
                    "why": "its gate is a status this fight tracks: every heavy slam forces Lifted, so from                             the second slam on the target is carrying it"})),
                CritChancePerCombo(v) => conditionals.push(json!({
                    "mod": name, "desc": e.describe(), "active": true,
                    "why": format!(
                        "the combo counter is live, so this is worth nothing at 1x and {} at the 12x cap —                          and it is additive with Point Strike inside the same bracket",
                        wfsim_engine::model::pct(v * 11.0))})),
                StatusChancePerCombo(v) => conditionals.push(json!({
                    "mod": name, "desc": e.describe(), "active": true,
                    "why": format!(
                        "the combo counter is live, so this is worth nothing at 1x and {} at the 12x cap",
                        wfsim_engine::model::pct(v * 11.0))})),
                MeleeComboDuration(_) => conditionals.push(json!({
                    "mod": name, "desc": e.describe(), "active": true,
                    "why": "it buys TIME on the counter rather than a stat — what it is worth depends on                             how often this build lands a hit"})),
                InitialCombo(_) => conditionals.push(json!({
                    "mod": name, "desc": e.describe(), "active": true,
                    "why": "the floor the counter returns to after a heavy attack, which is what a pure                             heavy build spends and what a light build never notices"})),
                HeavyAttackEfficiency(_) => conditionals.push(json!({
                    "mod": name, "desc": e.describe(), "active": true,
                    "why": "it pays only on a form that SPENDS the counter — the two heavy modes"})),
                // Event mechanic — no static stat; the sim rolls it per hit.
                ProcConversion { .. } => conditionals.push(json!({
                    "mod": name, "desc": e.describe(), "active": true,
                    "why": "rolled per damage instance in the sim"})),
                // A BONUS THE PLAYER DECIDES. It IS in the panel's numbers —
                // `resolve_for` folded it into the same buckets a plain mod
                // writes — so this line exists to say WHERE it came from, and
                // to state what the player is worth to it. Without it the panel
                // would show a base-damage figure with no card explaining it.
                TennoScaled { stat, above, unit, per_unit, cap, .. } => {
                    let have = (stat.of(panel_tenno) - above).max(0.0);
                    let steps = (have / unit.max(1e-9)).floor();
                    let paid = (steps * per_unit).min(cap);
                    conditionals.push(json!({
                        "mod": name, "desc": e.describe(), "active": paid > 0.0,
                        "why": format!(
                            "this Warframe has {:.0} {} — {:.0} whole steps of {:.0}, so it pays {:.0}%{}",
                            stat.of(panel_tenno), stat.name(), steps, unit, paid * 100.0,
                            if paid >= cap { " (capped)" } else { "" })}));
                }
            }
            // Tag whatever the arms just pushed, so the panel never shows a
            // contribution without the condition that earns it.
            if let Some(cond) = &tenno_gate {
                for row in src.iter_mut().skip(before) {
                    row.3 = Some(match row.3.take() {
                        Some(t) => format!("{t}; {cond}"),
                        None => cond.to_string(),
                    });
                }
            }
        }
    }
    // A `tenno_scaled` ARCANE contributes without being a mod, a bucket or a
    // buff card: its value is a WARFRAME STAT read off the fight's Tenno, so
    // there is no stack to configure and nothing in the resolved panel to
    // attribute it to. It gets a conditional line — the one channel for "this
    // pays, and here is what decides it" — because a contribution the sim
    // applies and the panel never mentions is exactly the disagreement the
    // rest of this function exists to prevent.
    {
        let arc = arcane_fx_for(v, info, &forms_list[0].2, policy);
        let t = tenno_from(v, info);
        for b in arc.buffs.iter() {
            if b.trigger != wfsim_engine::model::ArcTrigger::Passive {
                continue;
            }
            let what = match b.grant {
                wfsim_engine::model::ArcGrant::Multishot => "Multishot",
                _ => "Base Damage",
            };
            conditionals.push(json!({
                "mod": prettify(&b.owner),
                "desc": format!("{} {what}", fpct(b.per_stack)),
                "active": true,
                "why": format!("from your Warframe — armor {:.0}, max energy {:.0}. Set them in the Tenno block; with no frame this pays nothing",
                    t.armor, t.energy),
            }));
        }
    }
    (src, conditionals)
}

/// The shares that are not a mod's: the chosen evolutions, then the fight's
/// own bonuses.
fn evolution_sources(
    info: &WeaponInfo,
    evo_refs: &[&str],
    panel_tenno: &wfsim_engine::data::tenno::Tenno,
) -> (Vec<OtherSource>, (f64, f64, f64, f64)) {
    // Non-mod sources: the CHOSEN evolutions (data-driven). Flat base
    // damage and flat base crit chance alter the WEAPON BASE before mods —
    // the stat rows show the raw base and attribute the delta here; the
    // multishot stacks and CO rate join their buckets. Broken evolutions
    // report zero via the accessors, so nothing is listed for them.
    // (key, source name, PRE-FORMATTED value, note)
    let mut evo_src: Vec<(&'static str, String, String, Option<String>)> = Vec::new();
    // THE FIGHT's PLAYER, hoisted: both the source list below and the resolved
    // panels at the end of this function ask it, and asking twice is how they
    // came to disagree.
    let (mut evo_flat_bd, mut evo_flat_cc) = (0.0f64, 0.0f64);
    let (mut evo_flat_sc, mut evo_flat_mag) = (0.0f64, 0.0f64);
    if form_unlock_evo(info).is_some() {
        // Tiers are per weapon (adapters I-IV, Zariman weapons I-V), so the
        // numeral is BUILT, not indexed - a fixed table silently rendered the
        // Laetum's fifth tier as "EVO IV".
        let tiername = |t: u32| {
            let mut n = t;
            let mut out = String::from("EVO ");
            for (v, sym) in [(10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I")] {
                while n >= v {
                    out.push_str(sym);
                    n -= v;
                }
            }
            out
        };
        for def in evo_refs
            .iter()
            .filter_map(|id| wfsim_engine::data::evolutions::get(id))
        {
            let name = format!("{} ({})", def.name, tiername(def.tier));
            let v = def.flat_base_damage();
            if v > 0.0 {
                evo_flat_bd += v;
                evo_src.push((
                    "base_damage",
                    name.clone(),
                    format!("+{v:.0} flat"),
                    Some("added to the weapon base pro-rata, before mods".into()),
                ));
            }
            let v = def.flat_base_crit_chance();
            if v > 0.0 {
                evo_flat_cc += v;
                evo_src.push((
                    "crit_chance",
                    name.clone(),
                    format!("+{:.0}% base", v * 100.0),
                    Some("into the BASE crit chance — crit mods multiply it".into()),
                ));
            }
            let v = def.flat_base_status_chance();
            if v > 0.0 {
                evo_flat_sc += v;
                evo_src.push((
                    "status_chance",
                    name.clone(),
                    format!("+{:.0}% base", v * 100.0),
                    Some("into the BASE status chance — status mods multiply it".into()),
                ));
            }
            let v = def.flat_base_magazine();
            if v > 0.0 {
                evo_flat_mag += v;
                evo_src.push((
                    "magazine",
                    name.clone(),
                    format!("+{v:.0} rounds"),
                    Some("into the BASE magazine — magazine mods multiply it".into()),
                ));
            }
            // …AND THE GATED SPELLING, listed exactly when it PAYS. It is NOT
            // added to `evo_flat_mag`, which the base column subtracts: this
            // add is not in `base.magazine_size` at all (the resolver folds it
            // onto a clone), so subtracting it would print a base of −8.
            let v = def.gated_flat_magazine(panel_tenno);
            if v > 0.0 {
                evo_src.push((
                    "magazine",
                    name.clone(),
                    format!("+{v:.0} rounds"),
                    Some("into the BASE magazine, and only while the fight says this weapon is the only one carried".into()),
                ));
            }
            let v = def.assumed_multishot();
            if v > 0.0 {
                evo_src.push((
                    "multishot",
                    name.clone(),
                    fpct(v),
                    Some("on-ability-cast stacks, assumed full".into()),
                ));
            }
            let v = def.co_per_type();
            if v > 0.0 {
                evo_src.push((
                    "co",
                    name.clone(),
                    fpct(v),
                    Some("innate, per status type on target".into()),
                ));
            }
        }
    }

    // THE FIGHT'S OWN BONUSES, attributed. They seed the buckets inside
    // `resolve_for`, so the numbers are already right everywhere — this is the
    // half a number cannot say: WHERE it came from. A magazine that grew with
    // no source listed is the panel telling half a story, and a bucket that
    // grew because the scenario said so is exactly the case a reader will
    // otherwise blame on a mod.
    //
    // Listed under the same `evo_src` channel the evolutions use rather than
    // `src`, because `src` is per-MOD and this is not one — it only looks like
    // one from the arithmetic's side.
    {
        let b = &panel_tenno.bonuses;
        let scen = "Scenario bonus".to_string();
        for (key, v) in [
            ("base_damage", b.base_damage),
            ("multishot", b.multishot),
            ("crit_chance", b.crit_chance),
            ("crit_damage", b.crit_damage),
            ("status_chance", b.status_chance),
            ("status_damage", b.status_damage),
            ("fire_rate", b.fire_rate),
            ("reload", b.reload_speed),
            ("magazine", b.magazine),
        ] {
            if v != 0.0 {
                evo_src.push((
                    key,
                    scen.clone(),
                    fpct(v),
                    Some("from the fight, not the build — the same bucket a mod feeds".into()),
                ));
            }
        }
    }
    (evo_src, (evo_flat_bd, evo_flat_cc, evo_flat_sc, evo_flat_mag))
}

/// One stats section per form; its params are `base` / `panel` so every row
/// reads the ACTIVE form's numbers.
fn form_section(
    label: &'static str,
    meta: &str,
    base: &WeaponBase,
    panel: &ResolvedPanel,
    inputs: &SectionInputs,
) -> Value {
    let (v, info, refs, policy) = (inputs.v, inputs.info, inputs.refs, inputs.policy);
    let (src, evo_src) = (&inputs.src, &inputs.evo_src);
    let (evo_flat_bd, evo_flat_cc, evo_flat_sc, evo_flat_mag) = inputs.evo_flat;
    let sources = |key: &str, tag: Option<&str>| -> Vec<Value> {
        let evo = evo_src
            .iter()
            .filter(move |(k, _, _, _)| *k == key && tag.is_none())
            .map(|(_, name, v, note)| json!({ "mod": name, "value": v, "note": note }));
        evo.chain(
            src.iter()
                .filter(|(k, _, _, note)| {
                    *k == key && tag.is_none_or(|t| note.as_deref() == Some(t))
                })
                .map(|(_, name, v, note)| {
                    // `fraction` is the RAW fraction beside the formatted text,
                    // so the panel can show the arithmetic — `40 × (1 +
                    // 1.65 + 0.60)` — instead of only its answer. Everything
                    // in one bracket is one multiplicative bucket, and that
                    // shape teaches the bucket better than any sentence.
                    //
                    // Evolution sources carry no `fraction` on purpose: several
                    // are FLAT additions rather than percentages, so an
                    // expression built from them would assert arithmetic
                    // that is not what the engine did. The page draws the
                    // line only when every term in the row is a fraction.
                    json!({ "mod": name, "value": fpct(*v), "fraction": v,
                        "note": if tag.is_some() { Value::Null } else { json!(note) } })
                }),
        )
        .collect()
    };
    // ---- stat rows: base -> final, with the merged bonus and its sources ----
    let num = |x: f64| -> String {
        if x >= 100.0 {
            format!("{x:.0}")
        } else {
            format!("{x:.1}")
        }
    };
    let pc = |x: f64| format!("{:.1}%", x * 100.0);
    let mut stats = Vec::new();
    // Every base stat is ALWAYS listed (the panel must state the whole
    // base panel, not just what changed) — the UI drops the arrow when
    // base == final.
    // A LOCKED row says so. Base == final and an empty source list is what
    // a stat nothing touched looks like too, and the difference matters: one
    // is a build that bought nothing, the other is a build whose mods are
    // being ignored on purpose ("Fire Rate cannot be modified"). Named after
    // the mod that did it, because that is the thing to take off.
    let lock_by = |key: &str| -> Option<String> {
        panel.locked.contains(&key).then(|| {
            refs.iter()
                .find(|m| m.disables.contains(&key))
                .map_or_else(String::new, |m| m.name.to_string())
        })
    };
    let mut row = |key: &'static str, label: &str, base_s: String, final_s: String| {
        let mut srcs = sources(key, None);
        let mut j = json!({ "key": key, "label": label, "base": base_s, "final": final_s });
        if let Some(by) = lock_by(key) {
            // ...AND WHAT THE LOCK IS THROWING AWAY. The row kept listing
            // every bonus feeding a stat it had already zeroed, so a build
            // with Critical Deceleration under a Cannonade read "3.3/s ·
            // locked · −20% Critical Deceleration" — a pinned number and a
            // contribution to it, on the same row, with nothing saying
            // which one won. The number was always
            // right; the row argued with itself about it. Marking them is
            // better than dropping them: "this mod does nothing here" is
            // exactly what the reader came for, and a missing line says it
            // to nobody.
            for s in srcs.iter_mut() {
                s["ignored"] = json!(true);
            }
            j["locked_by"] = json!(by);
        }
        j["sources"] = json!(srcs);
        stats.push(j);
    };
    // Base columns show the RAW weapon base (pre-evolution): the evolution
    // flat deltas are attributed as named source rows, not hidden in "base".
    let raw_bd = base.base_vector.total() - evo_flat_bd;
    let raw_cc = base.base_crit_chance - evo_flat_cc;
    let raw_sc = base.base_status_chance - evo_flat_sc;
    let raw_mag = base.magazine_size - evo_flat_mag;
    row(
        "base_damage",
        "Base Damage",
        num(raw_bd),
        num(panel.modified_base),
    );
    row(
        "multishot",
        "Multishot",
        format!("×{}", num(base.base_multishot)),
        format!("×{}", num(panel.multishot)),
    );
    row(
        "crit_chance",
        "Crit Chance",
        pc(raw_cc),
        pc(panel.crit_chance),
    );
    row(
        "crit_damage",
        "Crit Damage",
        format!("×{}", num(base.base_crit_damage)),
        format!("×{}", num(panel.crit_damage)),
    );
    row(
        "status_chance",
        "Status Chance",
        pc(raw_sc),
        pc(panel.status_chance),
    );
    // Identical formatting on both sides — the UI drops the arrow only
    // when the strings match ("×1" vs "×1.0" must not differ).
    row(
        "status_damage",
        "Status Damage",
        format!("×{}", num(1.0)),
        format!("×{}", num(panel.status_damage_multiplier)),
    );
    row(
        "status_duration",
        "Status Duration",
        format!("×{}", num(1.0)),
        format!("×{}", num(panel.status_duration_multiplier)),
    );
    row(
        "fire_rate",
        "Fire Rate",
        format!("{}/s", num(base.base_fire_rate)),
        format!("{}/s", num(panel.fire_rate)),
    );
    // Incarnon form: the magazine is a charge-backed resource (Max Charges,
    // inert to magazine mods) and there is no reload — instead two transition
    // times, each scaled by the reload formula base/(1 + reload bonus).
    if let Some(inc) = base.gauge_form {
        let rl = panel.reload_bonus;
        stats.push(json!({ "key": "magazine", "label": "Max Charges",
        "base": num(inc.max_charges), "final": num(inc.max_charges),
        "sources": json!([]) }));
        // HOW THE GAUGE FILLS, which the panel never said. The engine has
        // always read it — `charge_on` is weapon data and the shot loop
        // counts headshots or pellets accordingly — but a player could not
        // SEE it, and the two rules do not merely differ in speed: at a 0%
        // headshot rate a weakpoint-charged weapon never transforms at all
        // (measured: Burston Prime 0 transforms, Torid 4, same fight).
        // That is the largest thing an Incarnon weapon can do, decided by a
        // field with no row.
        let (what, why) = match inc.charge_on {
            wfsim_engine::model::ChargeOn::WeakpointHits => (
                "weakpoint hits",
                "weakpoint hits only — at a 0% headshot rate this weapon never reaches its Incarnon form. A radial or field instance can never contribute: it has no hit location",
            ),
            wfsim_engine::model::ChargeOn::DirectHits => (
                "direct hits",
                "ANY direct hit, so the form does not depend on the headshot rate (wiki Incarnon: \"Angstrum Incarnon Genesis and Torid Incarnon Genesis are instead charged through direct hits\"). A lingering field is not a direct hit and does not charge it",
            ),
            wfsim_engine::model::ChargeOn::Kills => (
                "kills",
                "KILLS, not hits — so this form is worth what the fight lets you earn, and against a single target that does not die it never arrives at all. A radial, a field tick or a status proc all count: the kill is what is asked for, not the instance that landed it. Kills made with the earned form itself do not pay for the next one",
            ),
        };
        stats.push(json!({ "key": "gauge", "label": "Gauge Fills On",
        "base": "—",
        // A COUNT, so no decimal: "5 direct hits", not "5.0".
        "final": format!("{:.0} {what}", inc.charges_to_fill),
        "note": why,
        "sources": sources("incarnon_charge_rate", None) }));
        stats.push(json!({ "key": "transmute_in", "label": "Transmute In",
        "base": format!("{}s", num(inc.transmute_in)),
        "final": format!("{}s", num(inc.transmute_in / (1.0 + rl))),
        "sources": sources("reload", None) }));
        stats.push(json!({ "key": "transmute_out", "label": "Transmute Out",
        "base": format!("{}s", num(inc.transmute_out)),
        "final": format!("{}s", num(inc.transmute_out / (1.0 + rl))),
        "sources": sources("reload", None) }));
    } else {
        row(
            "magazine",
            "Magazine",
            num(raw_mag),
            num(panel.magazine_size),
        );
        row(
            "reload",
            "Reload",
            format!("{}s", num(base.base_reload)),
            format!("{}s", num(panel.reload_seconds)),
        );
    }
    // THE CONE, and what an accuracy mod does to it.
    //
    // It was on no card at all, which made Heavy Caliber's downside
    // invisible on every weapon in the roster: a reader saw `+165% base
    // damage` and nothing about the cost. The cost is real on a Braton and
    // exactly ZERO on a launcher, and the difference decides whether that
    // mod belongs in the build — so a panel that shows neither is hiding
    // the more interesting half.
    //
    // ACCURACY IS NOT THE STAT. The weapon-level `accuracy` field is
    // derived and fuzzy — the wiki prints it as a CATEGORY — and this
    // engine does not read it; the aim model reads the CONE in degrees. An
    // accuracy mod DIVIDES that cone, which is why a weapon whose cone is
    // already zero pays nothing: `0 / 1.55` is still 0.
    if let (Some(sb), Some(sr)) = (base.spread, panel.spread) {
        let deg = |x: f64| format!("{}°", display_number(x));
        let pinpoint = sr.min_deg <= 0.0 && sr.max_deg <= 0.0;
        let cone = |a: f64, b: f64| {
            if (a - b).abs() < 1e-9 { deg(a) } else { format!("{}–{}", deg(a), deg(b)) }
        };
        stats.push(json!({
            "key": "spread",
            "label": "Cone",
            "base": cone(sb.min_deg, sb.max_deg),
            "final": cone(sr.min_deg, sr.max_deg),
            // THE NOTE IS THE POINT on a pinpoint weapon: it says the
            // accuracy penalty in the build above cost nothing, which is
            // not something a `0° -> 0°` row says by itself.
            "note": if pinpoint {
                "this weapon fires exactly where the reticle is, so an accuracy                      penalty has no cone to widen and costs nothing here"
            } else {
                "the first shot's cone and where sustained fire takes it — an                      accuracy mod divides both"
            }.to_string(),
            "sources": sources("accuracy", None),
        }));
    }
    // BOWS state their cadence, because the Fire Rate row above is NOT it:
    // wiki Fire Rate gives bows a formula of their own — "Effective Fire
    // Rate = 1 / (Modded Charge Time + Modded Reload Time)" — which has no
    // fire-rate term at all. So the panel prints the draw (the half a
    // fire-rate mod actually shortens, at double value on a bow) and the
    // rate that formula yields, or a build reads 1.6/s where it fires 1.0.
    //
    // A tapped shot has NO draw: its row would be a constant 0.00s, so it
    // is left out and only the effective rate is stated.
    //
    // INSERTED beside the Fire Rate row it belongs next to rather than
    // pushed here: `row` holds the `stats` borrow until its last call.
    if let (Some(b), Some(f)) = (base.charge_seconds, panel.charge_seconds) {
        let at = stats
            .iter()
            .position(|s| s["key"] == "fire_rate")
            .map_or(stats.len(), |i| i + 1);
        let mut rows = Vec::new();
        if b > 0.0 {
            // Two decimals: a doubled bow bonus lands on values like
            // 0.31 s that `num`'s one decimal would round away.
            rows.push(json!({ "key": "charge_time", "label": "Charge Time",
                "base": format!("{b:.2}s"), "final": format!("{f:.2}s"),
                "sources": sources("fire_rate", None) }));
        }
        let eff = |charge: f64, reload: f64| format!("{:.2}/s", 1.0 / (charge + reload));
        rows.push(json!({ "key": "effective_fire_rate", "label": "Effective Fire Rate",
            "base": eff(b, base.base_reload), "final": eff(f, panel.reload_seconds),
            "note": "a bow's real cadence: 1 / (charge + reload), with no fire-rate term \
                     (wiki Fire Rate)".to_string(),
            "sources": json!([]) }));
        for (i, r) in rows.into_iter().enumerate() {
            stats.insert(at + i, r);
        }
    }
    // A continuous beam's impact SPHERE. Firestorm enlarges it, and without
    // this row the mod reads as equipped-but-doing-nothing on this form.
    // The note carries the honest part: the sphere adds no damage to a
    // target the beam already struck, so it is worth nothing single-target
    // and a great deal in a crowd, where every enemy it catches starts its
    // own chain.
    if let (Some(bb), Some(bp)) = (base.beam, panel.beam) {
        stats.push(json!({ "key": "radius", "label": "Beam Radius",
            "base": format!("{} m", num(bb.damage_radius_m)),
            "final": format!("{} m", num(bp.damage_radius_m)),
            "note": format!(
                "no single-target damage (a struck target is hit once); in a crowd every enemy it catches starts its own chain ({} hops, {} m, x{} per hop)",
                bp.chain_hops, num(bp.chain_range_m), num(bp.chain_damage_per_hop)),
            "sources": sources("radius", None) }));
    }
    // PER-WEAPON behavior: GunCO sources (Galvanized Strike, Carnage Reign,
    // Secondary Shiver) combine differently per weapon class, and their base
    // EXCLUDES evolution flat damage — this note states what the model
    // actually computes on THIS weapon, and is shared by every GunCO row.
    // WHICH PARTS take it. "Direct hits only" is the rule the mod cards
    // state and it is what every unlisted weapon does — but an AoE part
    // carries its own eligibility and a few entries have it (MECHANICS
    // §6), so the note has to be built from the weapon rather than
    // asserted. It was a hardcoded "direct hits only", which the Burston
    // Incarnon makes false: its explosion takes CO.
    let radial_co = base.radial.as_ref().is_some_and(|r| r.takes_condition_overload);
    let field_co = base.lingering.as_ref().is_some_and(|f| f.takes_condition_overload);
    // A FIXED FORMAT, NOT A SENTENCE. The rule is
    // three slots — BEHAVIOUR, the BASE the term reads, and which PARTS
    // take it — so a reader can compare two weapons by looking at the same
    // position twice instead of parsing two paragraphs. Anything genuinely
    // odd goes in the note BESIDE it rather than swelling the line.
    let parts = {
        let mut v = vec!["direct"];
        if radial_co {
            v.push("radial");
        }
        if field_co {
            v.push("field");
        }
        v.join(" + ")
    };
    let behavior = match panel.co_behavior {
        wfsim_engine::model::CoBehavior::AdditiveWithBaseDamage => "additive",
        wfsim_engine::model::CoBehavior::Independent => "multiplying",
        wfsim_engine::model::CoBehavior::Inert => "inert",
    };
    let excluded = (panel.co_base_fraction() - 1.0).abs() > 1e-9;
    // THE PERCENTAGE IS ALWAYS PRINTED, including the ordinary 100%. A slot that is blank when nothing is odd cannot
    // be told apart from a slot nobody filled in, and "100%" is the claim
    // being made — that this weapon reads its WHOLE base — which is worth
    // as much scrutiny as a 52%. ORIGINAL of EVOLVED: `raw_bd` is the
    // pre-evolution base and the fraction is original/evolved, so the
    // denominator is what the panel prints as this attack's base damage.
    let co_rule = if behavior == "inert" {
        "inert · base = n/a · parts = none".to_string()
    } else {
        format!(
            "{behavior} · base = {:.0}% ({:.0} of {:.0}) · parts = {parts}",
            panel.co_base_fraction() * 100.0,
            raw_bd,
            base.base_vector.total()
        )
    };

    // THE NOTE IS FOR WHAT THE THREE SLOTS CANNOT SAY, and is absent on an
    // ordinary weapon — which is most of them. Three things earn one.
    let mut notes: Vec<String> = Vec::new();
    if behavior == "inert" {
        notes.push("this weapon takes no Condition Overload at all — the catalog lists it as \"Does not apply\"".into());
    }
    if excluded {
        notes.push("an evolution raised this attack's base without raising what CO reads, so the term is computed on the ORIGINAL base (docs/CATALOGS.md)".into());
    }
    if radial_co || field_co {
        notes.push("an AoE part taking CO is a per-entry exception the catalog lists; every unlisted weapon is direct hits only".into());
    }
    if behavior == "multiplying" {
        notes.push("multiplying stands OUTSIDE the base-damage bracket, so Serration does not dilute it; additive joins that bracket and is diluted".into());
    }

    // ALWAYS SHOWN, even with no source equipped.
    //
    // A row that appears only once a GunCO card is on the build hides the
    // one thing a reader can check — WHICH RULE this weapon is computed
    // under — until they have already committed to the mod. The rules are
    // per weapon and transcribed by hand from a catalog, so a wrong one can
    // stand for months. Putting the adopted rule on every weapon's panel is
    // what lets that be caught by
    // someone who owns the gun rather than by someone re-reading the yaml.
    //
    // It is a STATEMENT OF METHOD, not an admission — `unmodeled:` and the
    // disclosure banner are for what the sim cannot do; this is what it
    // does, said out loud so it can be argued with.
    let has_source = panel.co_per_type > 0.0;
    stats.push(json!({ "key": "co", "label": "Condition Overload",
        "base": "—",
        "final": if has_source {
            format!("{} per status type on target", fpct(panel.co_per_type))
        } else {
            "no source equipped".to_string()
        },
        "rule": co_rule,
        "note": if notes.is_empty() { Value::Null } else { json!(notes.join(" · ")) },
        "sources": sources("co", None) }));

    // The equipped arcane on the panel: Secondary Shiver is a GunCO-family
    // source, so its row carries the SAME per-weapon caveat as the CO row.
    let tenno = tenno_from(v, info);
    // Cascadia Accuracy's weak-point crit joins Acuity's in the sim
    // (`active.weakpoint_crit_chance_relative + params.arcane.weakpoint_crit_chance_relative`), so the row
    // below has to add it or it would state less than the sim applies.
    let mut arcane_wp_cc = 0.0;
    for (pool, aid, want_rank) in arcane_choices(v, info) {
        if let Some(def) = wfsim_engine::data::arcanes::for_slot(&pool, &aid) {
            let rank = want_rank.unwrap_or(def.max_rank).min(def.max_rank);
            let fx = def.fx(rank, policy, base.traits, &tenno);
            arcane_wp_cc += fx.weakpoint_crit_chance_relative;
            if fx.per_cold_base_damage > 0.0 {
                stats.push(json!({ "key": "shiver", "label": "Per Cold Status (Shiver)",
                "base": "—",
                "final": format!("{} damage per Cold status on target (cap {})",
                    fpct(fx.per_cold_base_damage), fx.cold_cap),
                // The SAME per-weapon rule, because Shiver is a GunCO
                // source: it reads the same base and joins the same
                // bracket, so it states the same three slots.
                "rule": co_rule.clone(),
                "note": "GunCO family — the weapon's Condition Overload rule applies to this too",
                "sources": [json!({ "mod": format!("{} (arcane, rank {rank})", def.name),
                    "value": fpct(fx.per_cold_base_damage), "note": "per Cold stack; Frozen counts as the full 10" })] }));
            }
        }
    }

    // WEAK-POINT bonuses (Acuity, Cascadia Accuracy). They had NO rows at
    // all: the damage half was invisible and the crit half was folded into
    // the flat Crit Chance row, so a mod worth +350% on heads read as
    // either nothing or as an unconditional 126%. Both halves are
    // conditional on where the bullet lands, and the number a reader can
    // act on is the one that holds THERE — stated next to the plain one,
    // never in place of it.
    let wp_cc_total = panel.weakpoint_crit_chance_relative + arcane_wp_cc;
    if wp_cc_total > 0.0 {
        stats.push(json!({ "key": "weakpoint_cc", "label": "Weak Point Crit Chance",
        "base": pc(panel.crit_chance),
        "final": format!("{} on a weak point", pc(panel.crit_chance + panel.base_crit_chance * wp_cc_total)),
        "note": format!(
            "{} relative to the {} base, additive with Point Strike — on WEAK-POINT hits only. \
             Everywhere else the crit chance above stands, and the radial explosion never gets \
             it at all (an explosion has no hit location)",
            fpct(wp_cc_total), pc(panel.base_crit_chance)),
        "sources": sources("crit_chance", None) }));
    }
    if panel.weakpoint_damage > 0.0 {
        stats.push(json!({ "key": "weakpoint_damage", "label": "Weak Point Damage",
        "base": "—",
        // Two decimals, not the panel's usual one: the wiki's worked
        // example is "3 + 3.5 x 1.5 = 8.25x" and a row printing 8.2 no
        // longer matches the source it cites.
        "final": format!("+{:.2} to the weak-point multiplier", 1.5 * panel.weakpoint_damage),
        "note": format!(
            "the listed {} is ADDED to the enemy's own weak-point multiplier at 1.5x on a true \
             weak point (wiki: a 3x head becomes 3 + {:.2} = {:.2}x), and headshot-multiplier \
             bonuses multiply the sum. Weak-point hits only",
            fpct(panel.weakpoint_damage), 1.5 * panel.weakpoint_damage,
            3.0 + 1.5 * panel.weakpoint_damage),
        "sources": Vec::<Value>::new() }));
    }

    // Elements: one row per contributed element (position/order matters for
    // combining — the damage section shows the combined result).
    let mut elem_rows = Vec::new();
    let mut seen_elems: Vec<String> = Vec::new();
    for (k, _, _, note) in src {
        if *k == "elements" {
            if let Some(t) = note {
                if !seen_elems.contains(t) {
                    seen_elems.push(t.clone());
                }
            }
        }
    }
    for t in &seen_elems {
        let total: f64 = src
            .iter()
            .filter(|(k, _, _, n)| *k == "elements" && n.as_deref() == Some(t))
            .map(|(_, _, v, _)| v)
            .sum();
        elem_rows.push(json!({ "key": "elements", "label": t, "base": "—",
        "final": format!("{} of modified base", fpct(total)),
        "sources": sources("elements", Some(t)) }));
    }

    // Indirect stats (recoil, accuracy, ammo…): not in theoretical DPS,
    // real in practice; base is unmodified (0%), final = Σ.
    let mut indirect_rows = Vec::new();
    // Not every indirect stat is a fraction — punch through and beam range
    // are METRES, a double-jump refresh is a COUNT, an explosion-on-kill is
    // flat DAMAGE. `IndirectStat::format` owns that, so this table and the
    // effect line on the card cannot drift apart.
    for (stat, total) in &panel.indirect {
        // PUNCH THROUGH IS REPORTED RESOLVED, below — this bucket is the
        // raw sum of what the mods GRANT, and since 2026-08-17 that is not
        // what the weapon HAS: an AoE attack takes none of it (*"Projectile
        // AoE weapons cannot have their Punch Through stat modified"*), so
        // a Primed Shred on a Torid would have posted +2.2 m against an
        // engine that spends 0.
        if *stat == wfsim_engine::model::IndirectStat::PunchThrough {
            continue;
        }
        indirect_rows.push(
            json!({ "key": "indirect", "label": stat.label(), "base": "—",
        "final": stat.format(*total), "sources": sources("indirect", Some(stat.label())) }),
        );
    }
    // …AND HERE IT IS, as the number the simulation actually spends: the
    // weapon's own depth plus every grant the attack is allowed to take.
    //
    // The row is drawn whenever there is anything to say — a weapon that
    // brings its own (which the mod bucket never knew about, so no row was
    // drawn at all) or a mod that tried. The SOURCES stay the mods', which
    // is what makes a zeroed total legible rather than mysterious: the
    // grants are listed and the final says 0 m.
    {
        let stat = wfsim_engine::model::IndirectStat::PunchThrough;
        let granted: f64 = panel
            .indirect
            .iter()
            .filter(|(s, _)| *s == stat)
            .map(|(_, v)| *v)
            .sum();
        if panel.punch_through_m > 0.0 || granted > 0.0 {
            indirect_rows.push(json!({
                "key": "indirect", "label": stat.label(), "base": "—",
                "final": stat.format(panel.punch_through_m),
                "sources": sources("indirect", Some(stat.label())),
            }));
        }
    }

    // A weapon is the GUN plus the PROJECTILE(s) it launches: the gun carries cadence and capacity, each projectile
    // carries its own damage, crit, status — and, when it is a radial,
    // its blast geometry. Split the flat row list along that line
    // instead of stating a single "base attack" that belongs to neither.
    const ON_PROJECTILE: &[&str] = &[
        "base_damage",
        "crit_chance",
        "crit_damage",
        "status_chance",
        "status_damage",
        "status_duration",
        "co",
        "shiver",
        // Weak-point bonuses belong to the PROJECTILE that lands on the
        // weak point, and to that one only: the explosion has no hit
        // location, so leaving them among the weapon-wide rows would read
        // as a claim over both parts.
        "weakpoint_cc",
        "weakpoint_damage",
    ];
    let key_of = |r: &Value| r["key"].as_str().unwrap_or("").to_string();
    let (direct_rows, weapon_rows): (Vec<Value>, Vec<Value>) = stats
        .into_iter()
        .partition(|r| ON_PROJECTILE.contains(&key_of(r).as_str()));

    // A damage vector as displayed rows: type, amount, share of the total.
    let vector_rows = |v: &wfsim_engine::rules::damage::DamageVector| {
        let total = v.total();
        v.iter_nonzero()
            .map(|(t, amt)| {
                json!({ "type": format!("{t:?}"), "amount": num(amt),
                "share": format!("{:.0}%", amt / total * 100.0) })
            })
            .collect::<Vec<Value>>()
    };

    let mut parts = vec![json!({
        "id": "direct",
        "label": "Direct hit",
        "meta": "on contact",
        "stats": direct_rows,
        "damage": vector_rows(&panel.damage),
        "damage_total": num(panel.damage.total()),
    })];

    // The radial explosion is a SECOND projectile-borne damage instance
    // with its own crit and status (MECHANICS §7) — the panel states it
    // in full rather than leaving the reader to assume it copies the
    // direct hit. Status damage/duration are weapon-wide multipliers, so
    // they repeat: they describe the procs THIS instance applies.
    if let (Some(rb), Some(rr)) = (base.radial.as_ref(), panel.radial.as_ref()) {
        let rsrc = |key: &'static str| sources(key, None);
        // Geometry reads as a distance, not a stat: 2 m, not 2.0.
        let dist = display_number;
        // PRIMARY COMPRESSION SHRINKS WHAT IT PAID FOR, so the row says the
        // sphere the fight actually fires (`FightParams::from_panel`) rather
        // than the one the mods built. A reader comparing the two would
        // otherwise find the panel's radius nowhere in the fight.
        let shown_radius = rr.radius_m
            * match panel.compression {
                Some(c) if c.radius_lost_m > 0.0 && inputs.compression_per_m > 0.0 => {
                    wfsim_engine::build::loadout::COMPRESSION_RADIUS_KEPT
                }
                _ => 1.0,
            };
        let mut rows = vec![
            json!({ "key": "base_damage", "label": "Base Damage",
                "base": num(rb.base_vector.total()), "final": num(rr.modified_base),
                "sources": rsrc("base_damage") }),
            json!({ "key": "crit_chance", "label": "Crit Chance",
                "base": pc(rb.base_crit_chance - evo_flat_cc),
                "final": pc(rr.crit_chance),
                "sources": rsrc("crit_chance") }),
            json!({ "key": "crit_damage", "label": "Crit Damage",
                "base": format!("×{}", num(rb.base_crit_damage)),
                "final": format!("×{}", num(rr.crit_damage)),
                "sources": rsrc("crit_damage") }),
            json!({ "key": "status_chance", "label": "Status Chance",
                "base": pc(rb.base_status_chance - evo_flat_sc),
                "final": pc(rr.status_chance),
                "sources": rsrc("status_chance") }),
            json!({ "key": "status_damage", "label": "Status Damage",
                "base": format!("×{}", num(1.0)),
                "final": format!("×{}", num(panel.status_damage_multiplier)),
                "sources": rsrc("status_damage") }),
            json!({ "key": "status_duration", "label": "Status Duration",
                "base": format!("×{}", num(1.0)),
                "final": format!("×{}", num(panel.status_duration_multiplier)),
                "sources": rsrc("status_duration") }),
            json!({ "key": "radius", "label": "Blast Radius",
                "base": format!("{} m", dist(rb.radius_m)),
                "final": format!("{} m", dist(shown_radius)),
                "note": (shown_radius < rr.radius_m).then(|| format!(
                    "Primary Compression keeps a fifth of it while aiming — {} m traded for the damage bonus",
                    dist(rr.radius_m - shown_radius))),
                "sources": rsrc("radius") }),
        ];
        // Falloff: full damage inside `start`, then linear down to
        // (1 − reduction) at the rim. Stated as what the rim actually
        // takes, which is the number a reader can act on.
        rows.push(json!({ "key": "falloff", "label": "Damage Falloff", "base": "—",
            "final": format!("{}% at {} m", dist((1.0 - rr.falloff_reduction) * 100.0),
                dist(shown_radius)),
            "note": if rr.falloff_start_m > 0.0 {
                format!("full damage within {} m, then linear", dist(rr.falloff_start_m))
            } else {
                "linear from the epicentre; a directly-hit enemy takes 100%".to_string()
            },
            "sources": json!([]) }));
        // CONDITION OVERLOAD, stated on the explosion ITSELF — because the
        // answer is normally "no" and this reader is looking at one of the
        // entries where it is "yes". The direct hit's row cannot carry it:
        // it names one bonus, and the two parts do not get the same one.
        // Shown only when a CO source is equipped, like the direct row.
        // UNCONDITIONAL, like the direct hit's: this part's rule is a fact
        // about the weapon, not about what is currently equipped.
            // THE SAME THREE SLOTS as the direct hit's, so a reader
            // comparing the two parts of one weapon compares positions
            // rather than paragraphs. This part has
            // its own base and its own eligibility, so both are printed
            // here rather than inherited from the row above.
            let (value, rule, note) = if rr.takes_condition_overload {
                let orig = rb.base_vector.total() * rr.co_base_fraction();
                let cut = (rr.co_base_fraction() - 1.0).abs() > 1e-9;
                (
                    format!("{} per status type on target", fpct(panel.co_per_type)),
                    format!(
                        "{behavior} · base = {:.0}% ({} of {}) · this part = takes CO",
                        rr.co_base_fraction() * 100.0,
                        num(orig),
                        num(rb.base_vector.total())
                    ),
                    if cut {
                        "THE EXCEPTION: CO normally reaches direct hits only, and this                              explosion is declared to take it — on the enemy the bullet directly                              hit, which a single target always is. An evolution raises the                              explosion's damage without raising the base CO reads, which is where                              the reduced percentage comes from"
                            .to_string()
                    } else {
                        "THE EXCEPTION: CO normally reaches direct hits only, and this                              explosion is declared to take it — on the enemy the bullet directly                              hit, which a single target always is"
                            .to_string()
                    },
                )
            } else {
                (
                    "excluded".to_string(),
                    format!("{behavior} · base = n/a · this part = excluded"),
                    "the rule: Condition Overload reaches DIRECT hits only, so this                          explosion takes none of it. Weapon-wide damage buckets still reach it —                          CO is the one thing an AoE part loses"
                        .to_string(),
                )
            };
            rows.push(json!({ "key": "co", "label": "Condition Overload",
                "base": "—", "final": value, "rule": rule, "note": note,
                "sources": if rr.takes_condition_overload { sources("co", None) } else { vec![] } }));
        parts.push(json!({
            "id": "radial",
            "label": "Radial explosion",
            "meta": format!("{} m radius", dist(rr.radius_m)),
            "stats": rows,
            "damage": vector_rows(&rr.damage),
            "damage_total": num(rr.damage.total()),
        }));
    }

    // THE BOMBLETS THE EXPLOSION THREW — two more parts, and they are on
    // the card for the reason every part is: the damage number above the
    // card counts them, so a reader who cannot see them is reading a total
    // that does not add up. Each states its COUNT in the meta line, because
    // "18 Radiation" and "5 × 18 Radiation" are different weapons.
    if let (Some(cb), Some(cr)) = (base.cluster.as_ref(), panel.cluster.as_ref()) {
        let n = display_number(cr.count);
        let part_rows = |b: &wfsim_engine::model::RadialBase,
                         r: &wfsim_engine::build::loadout::ResolvedRadial| {
            vec![
                json!({ "key": "base_damage", "label": "Base Damage",
                    "base": num(b.base_vector.total()), "final": num(r.modified_base),
                    "sources": sources("base_damage", None) }),
                json!({ "key": "crit_chance", "label": "Crit Chance",
                    "base": pc(b.base_crit_chance), "final": pc(r.crit_chance),
                    "sources": sources("crit_chance", None) }),
                json!({ "key": "crit_damage", "label": "Crit Damage",
                    "base": format!("×{}", num(b.base_crit_damage)),
                    "final": format!("×{}", num(r.crit_damage)),
                    "sources": sources("crit_damage", None) }),
                json!({ "key": "status_chance", "label": "Status Chance",
                    "base": pc(b.base_status_chance), "final": pc(r.status_chance),
                    "sources": sources("status_chance", None) }),
            ]
        };
        parts.push(json!({
            "id": "cluster_contact",
            "label": "Bomblet contact",
            "meta": format!("×{n}, on contact"),
            "stats": part_rows(&cb.contact, &cr.contact),
            "damage": vector_rows(&cr.contact.damage),
            "damage_total": num(cr.contact.damage.total()),
        }));
        parts.push(json!({
            "id": "cluster_blast",
            "label": "Bomblet explosion",
            "meta": format!("×{n}, {} m radius", display_number(cr.blast.radius_m)),
            "stats": part_rows(&cb.blast, &cr.blast),
            "damage": vector_rows(&cr.blast.damage),
            "damage_total": num(cr.blast.damage.total()),
        }));
    }

    // The lingering FIELD is a THIRD kind of part (MECHANICS §7): it does
    // not land once, it ticks. So it states its own clock — rate, lifetime
    // and the resulting total — on top of the same per-instance stats,
    // because "40 damage" means nothing here without "×10 ticks".
    // FROM THE PANEL, not from the weapon. A field a MOD granted — Nightwatch
    // Napalm's fire — has no `base.lingering` at all, so reading the weapon
    // here drew nothing: the part was resolved, simulated, and appeared on
    // no card, which on an Ogris is most of the build's damage. `ResolvedPanel::lingering_base` is whichever one the
    // field actually resolved from.
    if let (Some(fb), Some(fr)) = (panel.lingering_base.as_ref(), panel.lingering.as_ref()) {
        let fsrc = |key: &'static str| sources(key, None);
        let dist = display_number;
        // ✅ measured (MEASUREMENTS M13): the first tick lands WITH the
        // impact, so the count is the plain product — ten for a 10 s cloud.
        let ticks = (fr.duration_seconds * fr.tick_rate).round();
        // Renewed Horror: the shot after an empty reload gets a longer
        // cloud. 1.0 = the evolution is not equipped, and the rows stay
        // silent about it rather than stating a boost of ×1.
        let boost = panel.field_duration_on_empty_reload;
        let boosted = (boost > 1.0).then_some((fr.duration_seconds * boost, ticks * boost));
        let mut rows = vec![
            json!({ "key": "base_damage", "label": "Damage per Tick",
                "base": num(fb.base_vector.total()), "final": num(fr.modified_base),
                "sources": fsrc("base_damage") }),
            json!({ "key": "crit_chance", "label": "Crit Chance",
                "base": pc(fb.base_crit_chance - evo_flat_cc),
                "final": pc(fr.crit_chance),
                "sources": fsrc("crit_chance") }),
            json!({ "key": "crit_damage", "label": "Crit Damage",
                "base": format!("×{}", num(fb.base_crit_damage)),
                "final": format!("×{}", num(fr.crit_damage)),
                "sources": fsrc("crit_damage") }),
            json!({ "key": "status_chance", "label": "Status Chance",
                "base": pc(fb.base_status_chance - evo_flat_sc),
                "final": pc(fr.status_chance),
                "sources": fsrc("status_chance") }),
            json!({ "key": "status_damage", "label": "Status Damage",
                "base": format!("×{}", num(1.0)),
                "final": format!("×{}", num(panel.status_damage_multiplier)),
                "sources": fsrc("status_damage") }),
            json!({ "key": "status_duration", "label": "Status Duration",
                "base": format!("×{}", num(1.0)),
                "final": format!("×{}", num(panel.status_duration_multiplier)),
                "sources": fsrc("status_duration") }),
            // The clock. Neither is mod-scaled: fire-rate mods change shots
            // per second, not the cloud's own tick rate, and the cloud is
            // not a status effect so status duration does not reach it.
            json!({ "key": "tick_rate", "label": "Tick Rate", "base": "—",
                "final": format!("{}/s", dist(fr.tick_rate)), "sources": json!([]) }),
            json!({ "key": "field_duration", "label": "Field Duration",
                "base": "—", "final": format!("{} s", dist(fr.duration_seconds)),
                "note": match boosted {
                    // The doubled cloud is one shot in `magazine`, so state
                    // both numbers rather than an average nobody can check
                    // against a damage number in game.
                    Some((d, n)) => format!(
                        "{} ticks per field, the first landing with the impact; \
                         the shot after an empty reload gets {} s = {} ticks",
                        dist(ticks), dist(d), dist(n)),
                    None => format!("{} ticks per field, the first landing with the impact",
                        dist(ticks)),
                },
                "sources": json!([]) }),
            json!({ "key": "field_total", "label": "Total per Field",
                "base": num(fb.base_vector.total() * ticks),
                "final": num(fr.modified_base * ticks),
                "note": "one grenade, before crit and Condition Overload".to_string(),
                "sources": json!([]) }),
            json!({ "key": "radius", "label": "Field Radius",
                "base": format!("{} m", dist(fb.radius_m)),
                "final": format!("{} m", dist(fr.radius_m)),
                "sources": fsrc("radius") }),
            json!({ "key": "falloff", "label": "Damage Falloff", "base": "—",
                "final": format!("{}% at {} m",
                    dist((1.0 - fr.falloff_reduction) * 100.0), dist(fr.radius_m)),
                "note": "the grenade sticks, so the target stands at the epicentre"
                    .to_string(),
                "sources": json!([]) }),
            // Worth up to ~5x here, so it is stated on the panel rather
            // than buried in the yaml.
            json!({ "key": "field_stacking", "label": "Overlapping Fields",
                "base": "—",
                "final": match fr.stacking {
                    wfsim_engine::model::FieldStacking::Stack => "stack",
                    wfsim_engine::model::FieldStacking::Refresh => "refresh",
                },
                "note": "measured (MEASUREMENTS M13)".to_string(),
                "sources": json!([]) }),
        ];
                    // THE FIELD'S OWN CO ROW, on the same three slots as the direct
        // hit's and the explosion's — added 2026-08-16, because the part
        // had none at all and "no row" reads as "nobody thought about it"
        // rather than as an answer. A field keeps the DIRECT hit's base
        // fraction: the catalog puts the Torid's cloud on the same base as
        // its main fire (`field_tick` passes `active.co_base_fraction()`).
        rows.push(json!({ "key": "co", "label": "Condition Overload",
            "base": "—",
            "final": if fb.takes_condition_overload {
                format!("{} per status type on target", fpct(panel.co_per_type))
            } else {
                "excluded".to_string()
            },
            "rule": if fb.takes_condition_overload {
                format!(
                    "{behavior} · base = {:.0}% ({:.0} of {:.0}) · this part = takes CO",
                    panel.co_base_fraction() * 100.0,
                    raw_bd,
                    base.base_vector.total()
                )
            } else {
                format!("{behavior} · base = n/a · this part = excluded")
            },
            "note": if fb.takes_condition_overload {
                "THE EXCEPTION: CO normally reaches direct hits only, and this field is                      declared to take it. It keeps the DIRECT hit's base — the catalog puts the                      cloud on the same base and the same behaviour as the main fire"
            } else {
                "the rule: Condition Overload reaches DIRECT hits only, so this field takes                      none of it. Weapon-wide damage buckets still reach it"
            },
            "sources": if fb.takes_condition_overload { sources("co", None) } else { vec![] } }));
parts.push(json!({
            "id": "field",
            "label": "Lingering field",
            "meta": format!("{} m, {} s", dist(fr.radius_m), dist(fr.duration_seconds)),
            "stats": rows,
            "damage": vector_rows(&fr.damage),
            "damage_total": num(fr.damage.total()),
        }));
    }

    json!({
        "label": label,
        "meta": meta,
        "stats": weapon_rows,
        "elements": elem_rows,
        "indirect": indirect_rows,
        "parts": parts,
    })
}

/// A weak-point bonus is conditional on WHERE THE BULLET LANDS, and no policy
/// can turn a body shot into a head shot. Folding Acuity's crit half into the
/// flat Crit Chance row (AssumedMax) while leaving its damage half conditional
/// is one mod under two treatments: the Burston Incarnon then reads 126% crit
/// chance on every shot AND hands the same 126% to its explosion,
/// which has no hit location at all.
#[cfg(test)]
mod weakpoint_panel_tests {
    use super::*;

    fn incarnon_panel(mods: Value) -> Value {
        panel_json(&json!({
            "weapon": "burston_prime",
            "mods": mods,
            "evolutions": ["burston_prime_evo1_incarnon_form",
                           "burston_prime_forceful_finality"],
        }))
    }

    fn form<'a>(p: &'a Value, label: &str) -> &'a Value {
        p["forms"]
            .as_array()
            .expect("forms")
            .iter()
            .find(|f| f["label"] == json!(label))
            .unwrap_or_else(|| panic!("no {label}"))
    }

    fn part<'a>(f: &'a Value, id: &str) -> &'a Value {
        f["parts"]
            .as_array()
            .expect("parts")
            .iter()
            .find(|p| p["id"] == json!(id))
            .unwrap_or_else(|| panic!("no {id} part"))
    }

    fn row<'a>(p: &'a Value, key: &str) -> Option<&'a Value> {
        p["stats"].as_array()?.iter().find(|r| r["key"] == json!(key))
    }

    #[test]
    fn acuity_does_not_inflate_the_unconditional_crit_chance() {
        let inc = incarnon_panel(json!(["primary_acuity"]));
        let f = form(&inc, "Incarnon Form");
        let direct = part(f, "direct");
        assert_eq!(
            row(direct, "crit_chance").expect("crit row")["final"],
            json!("28.0%"),
            "the plain crit chance is the weapon's, not the weak-point one"
        );
        let wp = row(direct, "weakpoint_cc").expect("a weak-point crit row exists");
        assert_eq!(wp["final"], json!("126.0% on a weak point"));
        assert!(row(direct, "weakpoint_damage").is_some(), "and the damage half is stated");
    }

    #[test]
    fn the_explosion_gets_neither_half() {
        let inc = incarnon_panel(json!(["primary_acuity"]));
        let radial = part(form(&inc, "Incarnon Form"), "radial");
        assert_eq!(
            row(radial, "crit_chance").expect("crit row")["final"],
            json!("28.0%"),
            "an explosion has no hit location, so a weak-point bonus cannot reach it"
        );
        assert!(row(radial, "weakpoint_cc").is_none());
        assert!(row(radial, "weakpoint_damage").is_none());
    }

    /// Equipping it must still CHANGE something, or the fix would have been
    /// "delete the mod's effect" and the test above would pass anyway.
    /// PISTOL ACUITY IS THE SAME EFFECT, so it must not need its own fix —
    /// the bucket is chosen by `ModEffect`, never per mod. Asserted on a
    /// secondary because a rifle-only test would pass just as happily if the
    /// arm were duplicated per mod class.
    #[test]
    fn the_pistol_twin_behaves_identically() {
        let p = panel_json(&json!({
            "weapon": "laetum",
            "mods": ["pistol_acuity"],
            "evolutions": ["laetum_evo1_incarnon_form"],
        }));
        let f = form(&p, "Incarnon Form");
        let direct = part(f, "direct");
        assert_eq!(
            row(direct, "crit_chance").expect("crit row")["final"],
            json!("22.0%"),
            "the plain crit chance stays the weapon's"
        );
        assert_eq!(
            row(direct, "weakpoint_cc").expect("weak-point crit row")["final"],
            json!("99.0% on a weak point")
        );
        // 1.5 x the listed +350%, printed to two decimals so it matches the
        // wiki's own worked example (3 + 5.25 = 8.25x).
        assert_eq!(
            row(direct, "weakpoint_damage").expect("weak-point damage row")["final"],
            json!("+5.25 to the weak-point multiplier")
        );
        // And the Laetum's explosion is the ORDINARY case: no weak-point
        // bonus, and no CO either (it declares neither).
        let radial = part(f, "radial");
        assert_eq!(row(radial, "crit_chance").expect("crit row")["final"], json!("22.0%"));
        assert!(row(radial, "weakpoint_cc").is_none());
    }

    #[test]
    fn without_the_mod_there_are_no_weak_point_rows() {
        let bare = incarnon_panel(json!([]));
        let direct = part(form(&bare, "Incarnon Form"), "direct");
        assert!(row(direct, "weakpoint_cc").is_none());
        assert!(row(direct, "weakpoint_damage").is_none());
    }
}

/// WHAT A PANEL PRINTS IS NOT WHAT A RECORD STORES.
#[cfg(test)]
mod display_number_tests {
    use super::display_number;

    /// THE ONE THAT SHIPPED. `1.0 - 0.7` is not 0.3 in binary, so the falloff
    /// row read "30.000000000000004% at 6.2 m" — on 27 of the roster's 141
    /// falloff entries, which is every weapon whose reduction is .7, .8 or .9.
    #[test]
    fn a_subtraction_that_does_not_land_on_a_round_number_still_prints_one() {
        for (reduction, want) in [(0.7, "30"), (0.8, "20"), (0.9, "10"), (0.25, "75")] {
            assert_eq!(display_number((1.0 - reduction) * 100.0), want);
        }
    }

    /// …AND A REAL DIGIT SURVIVES. The trim is the whole risk here: a formatter
    /// that tidied 30.000000000000004 by rounding to whole numbers would also
    /// turn a 6.2 m radius into 6.
    #[test]
    fn a_digit_somebody_wrote_down_is_kept() {
        assert_eq!(display_number(6.2), "6.2");
        assert_eq!(display_number(1.7), "1.7");
        assert_eq!(display_number(0.5), "0.5");
        assert_eq!(display_number(18.67), "18.67");
        assert_eq!(display_number(0.125), "0.125");
    }

    /// Trailing zeros and a signed zero are noise on a card.
    #[test]
    fn a_whole_number_reads_whole_and_zero_has_no_sign() {
        assert_eq!(display_number(30.0), "30");
        assert_eq!(display_number(0.0), "0");
        assert_eq!(display_number(-0.0), "0");
    }

    /// EVERY FALLOFF ROW IN THE ROSTER, walked — so a weapon transcribed
    /// tomorrow with a reduction nobody thought of cannot bring the artefact
    /// back. A percentage a reader sees may not carry more than one decimal:
    /// nothing in this data does, and a longer one is the bug.
    #[test]
    fn no_weapon_prints_a_float_artefact_in_its_falloff() {
        let mut checked = 0;
        for w in wfsim_engine::data::weapons::all() {
            for r in w.attack.radial.iter() {
                let Some(red) = r.falloff_reduction else { continue };
                checked += 1;
                let shown = display_number((1.0 - red) * 100.0);
                let decimals = shown.split_once('.').map_or(0, |(_, d)| d.len());
                assert!(
                    decimals <= 1,
                    "{}: falloff reduction {red} prints {shown}%",
                    w.id
                );
            }
        }
        assert!(checked > 100, "only {checked} falloff rows — the walk found nothing");
    }
}

#[cfg(test)]
mod a_passive_names_itself {
    
    use serde_json::json;

    /// **A WEAPON THAT OFFERS A BUFF CARD STATES THE PASSIVE BEHIND IT.**
    ///
    /// With no mods, no arcanes and no evolutions on the build, a card can
    /// only come from the WEAPON or from the WIELDER, and the wielder's is
    /// already named by the frame's own `passive:` prose. So every remaining
    /// card belongs to a weapon mechanic, and one with no `passive_lines`
    /// entry is a mechanic the page never names — a reader has to know the
    /// weapon has it before the knob means anything.
    ///
    /// A RATCHET RATHER THAN ONE MORE ENTRY IN A LIST NOBODY REREADS: the next
    /// weapon passive wired into the engine fails here until it says what it
    /// is.
    #[test]
    fn a_weapon_with_a_buff_card_has_a_passive_line() {
        for s in wfsim_engine::data::weapons::roster() {
            let panel = super::panel_json(&json!({ "weapon": s.id, "mods": [] }));
            let cards: Vec<&str> = panel["buffs"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|b| b["id"].as_str())
                        .filter(|id| *id != wfsim_engine::data::rage::BUFF_ID)
                        .collect()
                })
                .unwrap_or_default();
            if cards.is_empty() {
                continue;
            }
            assert!(
                !wfsim_engine::data::weapons::passive_lines(&s.id).is_empty(),
                "{} offers {cards:?} and states no passive",
                s.id
            );
        }
    }
}
