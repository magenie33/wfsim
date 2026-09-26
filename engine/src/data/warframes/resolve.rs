use super::*;

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct SlotPick {
    pub id: String,
    /// `None` = max rank.
    #[serde(default)]
    pub rank: Option<u32>,
    /// AN ARCANE'S STACKS WHEN THE FIGHT OPENS, where it keeps them "until
    /// death" (Molt Augmented). `None` = 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stacks: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct HelminthPick {
    /// 1-4.
    pub slot: u8,
    pub ability: String,
}

/// **THE FLOOR AN OPERATOR CANNOT BE BELOW.**
///
/// Focus is ONE-WAY: a player far enough in to have an Operator has picked a
/// school and cannot un-pick it, so "no Focus at all" is an account nobody has
/// and a poor thing to measure a weapon against. A build that links no Operator
/// is read as this one.
///
/// VAZARIN, because it is the school that pays a weapon NOTHING — not one of
/// its nodes carries an `effects:`, so the floor is a real choice that grants
/// no number. (Naramon is the other such school; picking between the two moves
/// nothing.) The artifact stays EMPTY, which is what an unearned one is.
pub const FLOOR_SCHOOL: &str = "vazarin";

/// THE OPERATOR a Warframe build links: the active Focus school, which of its
/// conditional nodes to count as running, and the school's artifact. A node's
/// condition is the Operator's own action, so it is ASSUMED when ticked and
/// never simulated.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct OperatorPick {
    pub school: String,
    #[serde(default)]
    pub assumed: Vec<String>,
    #[serde(default)]
    pub artifact: ArtifactPick,
}

/// The active school's artifact, every card at max rank.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct ArtifactPick {
    #[serde(default)]
    pub mods: Vec<String>,
    #[serde(default)]
    pub arcane: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct Build {
    #[serde(default)]
    pub operator: Option<OperatorPick>,
    pub frame: String,
    #[serde(default)]
    pub mods: Vec<SlotPick>,
    #[serde(default)]
    pub exilus: Option<SlotPick>,
    #[serde(default)]
    pub aura: Option<SlotPick>,
    #[serde(default)]
    pub arcanes: Vec<SlotPick>,
    #[serde(default)]
    pub shards: Vec<ShardPick>,
    #[serde(default)]
    pub helminth: Option<HelminthPick>,
}

/// One source's share of a stat.
#[derive(Debug, Clone, PartialEq)]
pub struct Contribution {
    /// The mod, arcane or shard effect id (`<shard>/<effect>`).
    pub from: String,
    pub value: f64,
    /// A flat amount added after the multiplier, rather than a percentage.
    pub flat: bool,
    /// A multiplier on the finished stat (Catalyzing Shields' x0.20).
    pub times: bool,
}

/// THE SHIELD GATE: how long "invulnerable when shields break" lasts on this
/// build, and whether casting re-opens it. docs/WARFRAMES.md §Shield gate.
#[derive(Debug, Clone, PartialEq)]
pub struct ShieldGate {
    pub max_shields: f64,
    /// The gate after a FULL break, in seconds.
    pub full_seconds: f64,
    /// The card that fixes it, when one does.
    pub fixed_by: Option<String>,
    /// Shields per energy spent casting, and each source's share.
    pub energy_to_shield: f64,
    pub sources: Vec<(String, f64)>,
    pub casts: Vec<GateCast>,
}

/// One ability's cast, as the shield gate sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct GateCast {
    pub slot: u8,
    pub ability: String,
    pub energy: f64,
    pub shields: f64,
    /// The refill reaches max shields.
    pub full: bool,
    pub seconds: f64,
}

/// The gate after shields break holding `s` shields (W`Shield`):
/// "Shield/180 + 1/3" under 53, "(Shield/350)^0.65 + 1/3" to 1,150, 2.5 above.
pub fn shield_gate_seconds(s: f64) -> f64 {
    if s <= 0.0 {
        0.0
    } else if s < 53.0 {
        s / 180.0 + 1.0 / 3.0
    } else if s <= 1150.0 {
        (s / 350.0).powf(0.65) + 1.0 / 3.0
    } else {
        2.5
    }
}

/// A set's energy-to-shield share for this many cards seated, from
/// `data/mod_sets/<set>.yaml`'s `energy_to_shield_by_count`.
pub(super) fn set_energy_to_shield(set: &str, count: u32) -> f64 {
    #[derive(Deserialize)]
    struct SetShield {
        id: String,
        #[serde(default)]
        energy_to_shield_by_count: BTreeMap<u32, f64>,
    }
    crate::data::files_under("mod_sets/")
        .filter_map(|(_, t)| serde_norway::from_str::<SetShield>(t).ok())
        .find(|s| s.id == set)
        .and_then(|s| s.energy_to_shield_by_count.get(&count).copied())
        .unwrap_or(0.0)
}

#[derive(Debug, Clone, PartialEq)]
pub struct StatLine {
    pub stat: FrameStat,
    pub base: f64,
    /// Σ percentage bonuses.
    pub bonus: f64,
    /// Σ flat bonuses.
    pub flat: f64,
    pub value: f64,
    pub sources: Vec<Contribution>,
}

#[derive(Debug, Clone)]
pub struct AbilityLine {
    pub stat: &'static AbilityStat,
    /// Before the build's stats, after the Helminth's own number.
    pub base: f64,
    pub value: f64,
}

/// A number an ability produces from the frame's own stats.
#[derive(Debug, Clone, PartialEq)]
pub struct DerivedLine {
    pub label: String,
    pub stat: FrameStat,
    pub value: f64,
}

#[derive(Debug, Clone)]
pub struct ResolvedAbility {
    pub slot: u8,
    pub ability: &'static Ability,
    pub helminth: bool,
    pub energy_cost: f64,
    pub drain_per_second: Option<f64>,
    pub lines: Vec<AbilityLine>,
    pub derived: Vec<DerivedLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionKind {
    /// Not computed yet.
    Unmodelled,
    /// Cannot pay out in this panel.
    OutOfScope,
    /// Seated, and pays nothing in this build.
    Inert,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Admission {
    pub from: String,
    pub text: String,
    pub kind: AdmissionKind,
}

/// One tag the build carries, and what carries it.
#[derive(Debug, Clone, PartialEq)]
pub struct TagSource {
    pub tag: Capability,
    /// The mod, arcane or ability id, or the frame's id for its passive.
    pub from: String,
    pub when: String,
}

#[derive(Debug, Clone)]
pub struct Resolved {
    pub frame: &'static WarframeDef,
    pub stats: Vec<StatLine>,
    pub abilities: Vec<ResolvedAbility>,
    /// Every tag and every source of it, in tag order. An augment without its
    /// ability grants nothing, as it pays nothing.
    pub tags: Vec<TagSource>,
    pub shield_gate: ShieldGate,
    pub admissions: Vec<Admission>,
    /// What the build asked for and could not seat, each with the reason.
    pub refused: Vec<String>,
    /// THE AUGMENTS SEATED, by mod id, and only those whose ability this loadout
    /// actually carries — an augment without it pays nothing, which is the rule
    /// the admissions already state. A fight reads this to know whether an
    /// ability's augment is on the frame casting it.
    pub augments: Vec<&'static str>,
    /// The arcanes that move a CAST's strength, which only the fight can spend.
    pub cast_arcanes: crate::data::casting::CastArcanes,
    /// The arcanes that arm a buff on a WEAPON (Arcane Fury), which only that
    /// weapon's fight can run.
    pub weapon_buffs: Vec<WielderBuff>,
}

pub(super) fn by_rank(ladder: &[f64], rank: u32) -> f64 {
    ladder[(rank as usize).min(ladder.len() - 1)]
}

impl Resolved {
    pub fn stat(&self, s: FrameStat) -> &StatLine {
        self.stats.iter().find(|l| l.stat == s).expect("every stat is resolved")
    }
}

/// What a socket is worth to a Warframe's own panel: (stat, value, is it flat).
///
/// Keyed by the shard EFFECT id, because that is what names the quantity. Every
/// value here is transcribed in `data/shards/`, and W`Archon_Shard` says how it
/// lands: the Crimson and Amber percentages are "additive with similar buffs",
/// and each Azure number is a "Flat value increase after all bonuses are applied".
pub(super) fn shard_stat(effect: &str) -> Option<(FrameStat, bool)> {
    Some(match effect {
        "ability_strength" => (FrameStat::AbilityStrength, false),
        "ability_duration" => (FrameStat::AbilityDuration, false),
        "casting_speed" => (FrameStat::CastingSpeed, false),
        "max_health" => (FrameStat::Health, true),
        "shield_capacity" => (FrameStat::Shield, true),
        "armor" => (FrameStat::Armor, true),
        "energy_max" => (FrameStat::Energy, true),
        _ => return None,
    })
}

/// Resolve a build. An `Err` is a build that names no frame this data has;
/// everything else the build cannot seat is `refused`, and the rest resolves.
pub fn resolve(b: &Build) -> Result<Resolved, String> {
    let frame = warframe(&b.frame).ok_or_else(|| format!("unknown Warframe: {}", b.frame))?;
    let mut refused: Vec<String> = Vec::new();
    let mut admissions: Vec<Admission> = Vec::new();

    // THE LOADOUT FIRST, because an augment pays only while its ability is in it.
    let mut loadout: Vec<(u8, &'static Ability, bool)> = frame
        .abilities
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let a = ability(id).unwrap_or_else(|| panic!("{}: unknown ability {id}", frame.id));
            (i as u8 + 1, a, false)
        })
        .collect();
    if let Some(h) = &b.helminth {
        match ability(&h.ability) {
            Some(a) if !a.subsumable => refused.push(format!("{} cannot be infused", a.name)),
            Some(a) if frame.abilities.contains(&a.id) => {
                refused.push(format!("{} already has {}", frame.name, a.name))
            }
            Some(a) if (1..=loadout.len() as u8).contains(&h.slot) => loadout[h.slot as usize - 1] = (h.slot, a, true),
            Some(_) => refused.push(format!("there is no ability slot {}", h.slot)),
            None => refused.push(format!("unknown ability: {}", h.ability)),
        }
    }
    let carries = |id: &str| loadout.iter().any(|(_, a, _)| a.id == id);

    // THE CARDS, each checked against the slot it sits in.
    let mut seated: Vec<(&'static WarframeMod, u32)> = Vec::new();
    let mut seat = |pick: &SlotPick, slot: &str, refused: &mut Vec<String>| {
        let Some(m) = mod_by_id(&pick.id) else {
            refused.push(format!("unknown mod: {}", pick.id));
            return;
        };
        let why = match slot {
            "aura" if !m.aura => Some("is not an aura"),
            "exilus" if !m.exilus => Some("is not an exilus mod"),
            "main" if m.aura => Some("goes in the aura slot"),
            _ => None,
        };
        if let Some(w) = why {
            refused.push(format!("{} {w}", m.name));
            return;
        }
        if seated.iter().any(|(x, _)| x.id == m.id) {
            refused.push(format!("{} is already seated", m.name));
            return;
        }
        if let Some(f) = &m.family {
            if let Some((x, _)) = seated.iter().find(|(x, _)| x.family.as_ref() == Some(f)) {
                refused.push(format!("{} cannot be seated beside {}", m.name, x.name));
                return;
            }
        }
        seated.push((m, pick.rank.unwrap_or(m.max_rank).min(m.max_rank)));
    };
    for p in b.mods.iter().take(8) {
        seat(p, "main", &mut refused);
    }
    if b.mods.len() > 8 {
        refused.push(format!("{} mods for eight slots", b.mods.len()));
    }
    if let Some(p) = &b.exilus {
        seat(p, "exilus", &mut refused);
    }
    if let Some(p) = &b.aura {
        seat(p, "aura", &mut refused);
    }

    let mut bonus: BTreeMap<FrameStat, (f64, f64, Vec<Contribution>)> = BTreeMap::new();
    let mut add = |stat: FrameStat, from: &str, value: f64, flat: bool| {
        let e = bonus.entry(stat).or_default();
        if flat {
            e.1 += value;
        } else {
            e.0 += value;
        }
        e.2.push(Contribution { from: from.to_string(), value, flat, times: false });
    };
    // The two shield-gate cards: a multiplier on the finished shields, and a
    // fixed gate length.
    let mut shield_times: Vec<(String, f64)> = Vec::new();
    let mut gate_fixed: Option<(String, f64)> = None;

    for (m, rank) in &seated {
        // A SET BONUS counts the set's cards seated, and adds a share of THIS
        // card's own value.
        let n = m
            .set
            .as_ref()
            .map_or(0, |s| seated.iter().filter(|(x, _)| x.set.as_ref() == Some(s)).count() as u32);
        let set_share = m.set_bonus.iter().filter(|(k, _)| *k <= n).map(|(_, v)| *v).next_back().unwrap_or(0.0);
        let dead = m.augments.as_deref().filter(|a| !carries(a));
        if let Some(a) = dead {
            admissions.push(Admission {
                from: m.id.clone(),
                text: format!("augments {}, which this loadout does not carry", ability(a).map_or(a, |x| x.name.as_str())),
                kind: AdmissionKind::Inert,
            });
        }
        for e in &m.effects {
            match e {
                FrameEffect::Bonus(s, v) if dead.is_none() => {
                    add(*s, &m.id, at_rank(*v, *rank, m.max_rank) * (1.0 + set_share), false)
                }
                FrameEffect::Bonus(..) => {}
                FrameEffect::Flat(s, v) if dead.is_none() => add(*s, &m.id, *v, true),
                FrameEffect::Flat(..) => {}
                FrameEffect::ShieldMultiplier(r) => shield_times.push((m.id.clone(), by_rank(r, *rank))),
                FrameEffect::ShieldGateSeconds(r) => gate_fixed = Some((m.id.clone(), by_rank(r, *rank))),
                FrameEffect::Unmodelled(t) => admissions.push(Admission {
                    from: m.id.clone(),
                    text: t.clone(),
                    kind: AdmissionKind::Unmodelled,
                }),
                FrameEffect::OutOfScope(t) => admissions.push(Admission {
                    from: m.id.clone(),
                    text: t.clone(),
                    kind: AdmissionKind::OutOfScope,
                }),
                FrameEffect::Arcane(_) => unreachable!("refused on a mod at load"),
            }
        }
    }

    let mut arcane_ids: Vec<&str> = Vec::new();
    // `(from, per, increase, cap)`, spent once Max Health is known.
    let mut per_health: Vec<(String, f64, f64, f64)> = Vec::new();
    let mut cast_arcanes = crate::data::casting::CastArcanes::default();
    let mut weapon_buffs: Vec<WielderBuff> = Vec::new();
    for p in b.arcanes.iter().take(2) {
        let Some(a) = arcane_by_id(&p.id) else {
            refused.push(format!("unknown arcane: {}", p.id));
            continue;
        };
        if arcane_ids.contains(&a.id.as_str()) {
            refused.push(format!("{} is already seated", a.name));
            continue;
        }
        arcane_ids.push(&a.id);
        let rank = p.rank.unwrap_or(a.max_rank).min(a.max_rank);
        for e in &a.effects {
            match e {
                FrameEffect::Bonus(s, v) => add(*s, &a.id, at_rank(*v, rank, a.max_rank), false),
                FrameEffect::Flat(s, v) => add(*s, &a.id, *v, true),
                FrameEffect::ShieldMultiplier(r) => shield_times.push((a.id.clone(), by_rank(r, rank))),
                FrameEffect::ShieldGateSeconds(r) => gate_fixed = Some((a.id.clone(), by_rank(r, rank))),
                FrameEffect::Unmodelled(t) => admissions.push(Admission {
                    from: a.id.clone(),
                    text: t.clone(),
                    kind: AdmissionKind::Unmodelled,
                }),
                FrameEffect::OutOfScope(t) => admissions.push(Admission {
                    from: a.id.clone(),
                    text: t.clone(),
                    kind: AdmissionKind::OutOfScope,
                }),
                FrameEffect::Arcane(ArcaneRule::StrengthPerMaxHealth { per, increase, cap }) => {
                    per_health.push((a.id.clone(), *per, by_rank(increase, rank), by_rank(cap, rank)));
                }
                FrameEffect::Arcane(ArcaneRule::StrengthPerKill { per_stack, max_stacks }) => {
                    let stacks = p.stacks.unwrap_or(0).min(*max_stacks);
                    if stacks > 0 {
                        add(FrameStat::AbilityStrength, &a.id, by_rank(per_stack, rank) * f64::from(stacks), false);
                    }
                    cast_arcanes.per_kill = Some((by_rank(per_stack, rank), *max_stacks, stacks));
                    admissions.push(Admission {
                        from: a.id.clone(),
                        text: "the stacks it opens with; each kill in a fight adds one to what the action list casts after it".into(),
                        kind: AdmissionKind::OutOfScope,
                    });
                }
                FrameEffect::Arcane(ArcaneRule::StrengthAfterOperatorAbility(r)) => {
                    cast_arcanes.after_operator_ability += by_rank(r, rank);
                    admissions.push(Admission {
                        from: a.id.clone(),
                        text: "paid to a cast in the simulator's action list, after an Operator ability".into(),
                        kind: AdmissionKind::OutOfScope,
                    });
                }
                FrameEffect::Arcane(ArcaneRule::WeaponBuff { slot, trigger, grant, chance, duration, per_stack }) => {
                    weapon_buffs.push(WielderBuff {
                        slot: slot.clone(),
                        buff: crate::model::StackingBuff {
                            id: a.id.as_str(),
                            trigger: *trigger,
                            grant: *grant,
                            per_stack: by_rank(per_stack, rank),
                            // ONE STACK THAT A NEW TRIGGER REFRESHES: the card
                            // states a bonus and a duration, never a count.
                            max_stacks: 1,
                            duration: *duration,
                            chance: *chance,
                            decay: crate::model::BuffDecay::LoseOneAndReset,
                            initial_stacks: 0,
                            stacks_per_trigger: 1,
                            per_shell: false,
                            cleared_by: crate::model::ClearedBy::Nothing,
                            card_opens_full: false,
                        },
                    });
                    admissions.push(Admission {
                        from: a.id.clone(),
                        text: format!("paid to a {slot} weapon in its fight"),
                        kind: AdmissionKind::OutOfScope,
                    });
                }
                FrameEffect::Arcane(ArcaneRule::StrengthPerCastStack { per_stack, max_stacks }) => {
                    cast_arcanes.per_cast_stack = Some((by_rank(per_stack, rank), *max_stacks));
                    admissions.push(Admission {
                        from: a.id.clone(),
                        text: "paid to the casts in the simulator's action list".into(),
                        kind: AdmissionKind::OutOfScope,
                    });
                }
            }
        }
    }

    for pick in b.shards.iter().take(5) {
        let Some(d) = crate::data::shards::all().iter().find(|s| s.id == pick.shard) else {
            refused.push(format!("unknown shard: {}", pick.shard));
            continue;
        };
        let Some(o) = d.options.iter().find(|o| o.id == pick.effect) else {
            refused.push(format!("{} has no effect {}", d.name, pick.effect));
            continue;
        };
        let from = format!("{}/{}", d.id, o.id);
        let v = if pick.tauforged { o.tauforged } else { o.value };
        match shard_stat(&o.id) {
            Some((s, flat)) => add(s, &from, v, flat),
            None => admissions.push(Admission {
                from,
                text: o.text.clone(),
                kind: AdmissionKind::OutOfScope,
            }),
        }
    }

    // THE OPERATOR'S FOCUS: an always-on node counts, a conditional one only
    // when the Operator build assumes it.
    // NO LINK IS THE FLOOR, NOT NOTHING: see `FLOOR_SCHOOL`. It is resolved here
    // rather than at each surface that builds a request, so the page, the board
    // and the search cannot disagree about what a weapon's least is.
    let floor = OperatorPick { school: FLOOR_SCHOOL.into(), ..Default::default() };
    let linked = b.operator.as_ref().unwrap_or(&floor);
    let school = match focus_school(&linked.school) {
        Some(s) => Some((s, linked)),
        None => {
            refused.push(format!("unknown Focus school: {}", linked.school));
            None
        }
    };
    if let Some((_, o)) = school {
        refused.extend(artifact_refusals(&o.artifact));
    }
    if let Some((s, o)) = school {
        for n in s.nodes.iter().filter(|n| n.always || o.assumed.contains(&n.id)) {
            let from = format!("focus:{}:{}", s.id, n.id);
            for e in &n.effects {
                match e {
                    FrameEffect::Bonus(st, v) => add(*st, &from, *v, false),
                    FrameEffect::Flat(st, v) => add(*st, &from, *v, true),
                    FrameEffect::ShieldMultiplier(r) => shield_times.push((from.clone(), by_rank(r, 99))),
                    FrameEffect::ShieldGateSeconds(r) => gate_fixed = Some((from.clone(), by_rank(r, 99))),
                    FrameEffect::Unmodelled(_) | FrameEffect::OutOfScope(_) => {}
                    FrameEffect::Arcane(_) => unreachable!("refused on a node at load"),
                }
            }
        }
    }

    // THE STATS. Health, shields, armour and energy are
    //   base × (1 + Σ mods) + Σ flat
    // (W`Health`, W`Shield`, W`Armor`, W`Energy_Capacity`); the ability stats
    // start at 100% and every modifier "combine[s] additively".
    let mut stats: Vec<StatLine> = FrameStat::ALL
        .into_iter()
        .map(|s| {
            let base = match s {
                FrameStat::Health => frame.health,
                FrameStat::Shield => frame.shield,
                FrameStat::Armor => frame.armor,
                FrameStat::Energy => frame.energy,
                FrameStat::SprintSpeed => frame.sprint,
                _ => 1.0,
            };
            let (pct, flat, mut sources) = bonus.get(&s).cloned().unwrap_or_default();
            let mut value = base * (1.0 + pct) + flat;
            // "x0.20 Max Shield Capacity" — applied to the finished shields.
            if s == FrameStat::Shield {
                for (from, t) in &shield_times {
                    value *= t;
                    sources.push(Contribution { from: from.clone(), value: *t, flat: false, times: true });
                }
            }
            StatLine { stat: s, base, bonus: pct, flat, value, sources }
        })
        .collect();
    // A SHARE OF MAX HEALTH, read off the finished health: "round(Max Health ÷
    // 250 × Strength Increase)" (W`Arcane_Bellicose`).
    let health = stats.iter().find(|l| l.stat == FrameStat::Health).map_or(0.0, |l| l.value);
    for (from, per, increase, cap) in per_health {
        let v = ((health / per * increase * 100.0).round() / 100.0).min(cap);
        if let Some(l) = stats.iter_mut().find(|l| l.stat == FrameStat::AbilityStrength) {
            l.bonus += v;
            l.value += l.base * v;
            l.sources.push(Contribution { from, value: v, flat: false, times: false });
        }
    }
    let val = |s: FrameStat| stats.iter().find(|l| l.stat == s).map_or(1.0, |l| l.value);
    let strength = val(FrameStat::AbilityStrength);
    let duration = val(FrameStat::AbilityDuration);
    let range = val(FrameStat::AbilityRange);
    let efficiency = val(FrameStat::AbilityEfficiency);
    let casting = val(FrameStat::CastingSpeed);

    // W`Ability_Efficiency`:
    //   "Final Ability Cost = Ability Cost · max(200% − Ability Efficiency, 25%)"
    //   "Final Energy Drain = Energy Drain · max((200% − Ability Efficiency) / Ability Duration, 25%)"
    let cost_multiplier = (2.0 - efficiency).max(0.25);
    let drain_multiplier = if duration > 0.0 { ((2.0 - efficiency) / duration).max(0.25) } else { f64::INFINITY };

    let abilities: Vec<ResolvedAbility> = loadout
        .iter()
        .map(|&(slot, a, helminth)| {
            let lines: Vec<AbilityLine> = a
                .stats
                .iter()
                .map(|st| {
                    let base = if helminth { st.helminth_value.unwrap_or(st.value) } else { st.value };
                    let mut value = match st.scales_with {
                        Scaling::Strength => base * strength,
                        Scaling::Duration => base * duration,
                        Scaling::Range => base * range,
                        Scaling::CastingSpeed if casting > 0.0 => base / casting,
                        Scaling::CastingSpeed => f64::INFINITY,
                        Scaling::None => base,
                    };
                    if let Some(m) = st.min {
                        value = value.max(m);
                    }
                    if let Some(c) = st.cap {
                        value = value.min(c);
                    }
                    AbilityLine { stat: st, base, value }
                })
                .collect();
            let mut derived = Vec::new();
            for l in &lines {
                let Some(target) = l.stat.adds_to_base else { continue };
                let t = stats.iter().find(|x| x.stat == target).expect("every stat is resolved");
                derived.push(DerivedLine {
                    label: format!("{} while {} is active", target.label(), a.name),
                    stat: target,
                    value: t.base * (1.0 + t.bonus + l.value) + t.flat,
                });
                if let Some((with, mult)) = &l.stat.channel_multiplier {
                    if let Some((_, w, _)) = loadout.iter().find(|(_, x, _)| &x.id == with) {
                        derived.push(DerivedLine {
                            label: format!("{} while {} and {} are active", target.label(), a.name, w.name),
                            stat: target,
                            value: t.base * (1.0 + t.bonus + mult * l.value) + t.flat,
                        });
                    }
                }
            }
            for t in &a.unmodelled {
                admissions.push(Admission { from: a.id.clone(), text: t.clone(), kind: AdmissionKind::Unmodelled });
            }
            ResolvedAbility {
                slot,
                ability: a,
                helminth,
                energy_cost: if helminth { a.infused_energy_cost.unwrap_or(a.energy_cost) } else { a.energy_cost }
                    * cost_multiplier,
                drain_per_second: a.drain_per_second.map(|d| d * drain_multiplier),
                lines,
                derived,
            }
        })
        .collect();

    let mut tags: Vec<TagSource> = Vec::new();
    let mut claim = |from: &str, grants: &[TagGrant], infused: bool| {
        for g in grants {
            let when = match (&g.infused, infused) {
                (Some(None), true) => continue,
                (Some(Some(w)), true) => w.clone(),
                _ => g.when.clone(),
            };
            tags.push(TagSource { tag: g.tag, from: from.to_string(), when });
        }
    };
    claim(&frame.id, &frame.passive_tags, false);
    for (m, _) in &seated {
        if m.augments.as_deref().is_none_or(|a| loadout.iter().any(|(_, x, _)| x.id == a)) {
            claim(&m.id, &m.tags, false);
        }
    }
    for id in &arcane_ids {
        if let Some(a) = arcane_by_id(id) {
            claim(&a.id, &a.tags, false);
        }
    }
    for (_, a, helminth) in &loadout {
        claim(&a.id, &a.tags, *helminth);
    }
    // AN UNLOCKED NODE CAN BE USED whether or not its stat is assumed, so every
    // tag of the active school counts.
    if let Some((s, _)) = school {
        for n in &s.nodes {
            claim(&format!("focus:{}:{}", s.id, n.id), &n.tags, false);
        }
    }
    // THE SHIELD GATE. Energy spent casting is converted to shields by the Augur
    // set and Brief Respite, and any refill re-opens the gate. Its length is the
    // shields refilled — or Catalyzing Shields' fixed value, whatever the refill
    // (MEASUREMENTS M92).
    let max_shields = stats.iter().find(|l| l.stat == FrameStat::Shield).map_or(0.0, |l| l.value);
    let mut sources: Vec<(String, f64)> = Vec::new();
    let augur = seated.iter().filter(|(m, _)| m.set.as_deref() == Some("augur")).count() as u32;
    if augur > 0 {
        sources.push(("augur".into(), set_energy_to_shield("augur", augur)));
    }
    for (m, rank) in seated.iter().filter(|(m, _)| m.aura) {
        if let Some(v) = crate::data::auras::by_id(&m.id).and_then(|a| a.energy_to_shield) {
            sources.push((m.id.clone(), at_rank(v, *rank, m.max_rank)));
        }
    }
    let energy_to_shield: f64 = sources.iter().map(|(_, v)| v).sum();
    let casts: Vec<GateCast> = if max_shields > 0.0 && energy_to_shield > 0.0 {
        abilities
            .iter()
            .filter(|x| x.ability.cost_type.is_none() && x.energy_cost > 0.0)
            .map(|x| {
                let shields = x.energy_cost * energy_to_shield;
                let full = shields >= max_shields;
                let seconds = match &gate_fixed {
                    Some((_, fixed)) => *fixed,
                    None => shield_gate_seconds(shields.min(max_shields)),
                };
                GateCast { slot: x.slot, ability: x.ability.id.clone(), energy: x.energy_cost, shields, full, seconds }
            })
            .collect()
    } else {
        Vec::new()
    };
    if !casts.is_empty() {
        tags.push(TagSource {
            tag: Capability::Invulnerable,
            from: "shield_gate".into(),
            when: format!(
                "the shield gate, re-opened by casting: {}",
                casts
                    .iter()
                    .map(|c| format!("{} {:.2} s", ability(&c.ability).map_or(c.ability.as_str(), |a| a.name.as_str()), c.seconds))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }
    let shield_gate = ShieldGate {
        max_shields,
        full_seconds: gate_fixed.as_ref().map_or(shield_gate_seconds(max_shields), |(_, s)| *s),
        fixed_by: gate_fixed.map(|(id, _)| id),
        energy_to_shield,
        sources,
        casts,
    };
    tags.sort_by_key(|t| t.tag);

    let augments = seated
        .iter()
        .filter_map(|(m, _)| m.augments.as_deref().filter(|a| carries(a)).map(|_| m.id.as_str()))
        .collect();
    Ok(Resolved { frame, stats, abilities, tags, shield_gate, admissions, refused, augments, cast_arcanes, weapon_buffs })
}

/// The capacity an aura adds, from W`Aura`: "matching polarity … double of the
/// Aura's "drain" … In a slot without a polarity, the capacity is the same as the
/// listed drain, and in a slot of a different polarity, the additional capacity is
/// 80% of listed drain, rounded down". W`Mod` says "reduces it by 25%, rounded
/// mathematically" instead; the two part only at 2, 6, 10 and 14. The stance
/// slot (`rules::capacity::stance_capacity`) already follows the Aura page.
pub fn aura_capacity(grant: u32, aura_polarity: &str, slot_polarity: Option<&str>) -> u32 {
    match slot_polarity {
        None => grant,
        Some(p) if p == aura_polarity || p == "omni" => grant * 2,
        Some(_) => (f64::from(grant) * 0.8).floor() as u32,
    }
}
