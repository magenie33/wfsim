use super::*;

/// One rolled stat on a riven.
#[derive(Debug, Clone)]
pub struct RolledStat {
    pub id: String,
    /// Where in the 0.9-1.1 band this stat landed.
    pub roll: f64,
}

/// A riven, as constructed. `disposition` belongs to the WEAPON, not here —
/// it is passed in, so one riven spec reads differently on two weapons
/// exactly as it does in game.
#[derive(Debug, Clone)]
pub struct RivenSpec {
    pub class: String,
    pub bonuses: Vec<RolledStat>,
    pub malus: Option<RolledStat>,
    pub rank: u32,
    pub polarity: Polarity,
}

impl RivenSpec {
    pub fn shape(&self) -> Shape {
        Shape {
            bonuses: self.bonuses.len() as u32,
            malus: self.malus.is_some(),
        }
    }

    /// Every reason this riven could not exist in game. Empty = legal.
    pub fn illegal(&self) -> Vec<String> {
        let mut out = Vec::new();
        let p = pool(&self.class);
        if !self.shape().is_legal() {
            out.push(format!("a riven has 2 or 3 bonuses, not {}", self.bonuses.len()));
        }
        if self.rank > MAX_RANK {
            out.push(format!("rank {} is above {MAX_RANK}", self.rank));
        }
        let mut seen: Vec<&str> = Vec::new();
        for s in self.bonuses.iter().chain(self.malus.iter()) {
            // An EMPTY slot is a riven not described yet, not an illegal one.
            // The caller decides when a card is finished; this only judges
            // what has actually been said.
            if s.id.is_empty() {
                continue;
            }
            let Some(def) = p.iter().find(|x| x.id == s.id) else {
                out.push(format!("{} is not a {} riven stat", s.id, self.class));
                continue;
            };
            // One stat cannot appear twice, malus included.
            if seen.contains(&def.id.as_str()) {
                out.push(format!("{} appears twice", def.id));
            }
            seen.push(&def.id);
            if !(ROLL_MIN - 1e-9..=ROLL_MAX + 1e-9).contains(&s.roll) {
                out.push(format!("{} rolled {:.3}, outside {ROLL_MIN}-{ROLL_MAX}", def.id, s.roll));
            }
        }
        if let Some(c) = &self.malus {
            if let Some(def) = p.iter().find(|x| x.id == c.id) {
                if !def.malus {
                    out.push(format!("{} is bonus-only and can never be the malus", def.id));
                }
            }
        }
        // ...AND THE OTHER DIRECTION, which melee is the first pool to need.
        for s in &self.bonuses {
            if let Some(def) = p.iter().find(|x| x.id == s.id) {
                if !def.bonus {
                    out.push(format!("{} is malus-only and can never be a bonus", def.id));
                }
            }
        }
        out
    }

    /// Legality that depends on the WEAPON, not just on the riven: a stat
    /// [`excluded_for`] refuses. The same list the page's picker and the
    /// board's validation read, so the card, the picker and the board give one
    /// answer — a second rule here refused real Ocucor cards the picker offered.
    pub fn illegal_for(&self, weapon_id: &str) -> Vec<String> {
        let mut out = self.illegal();
        let excluded = excluded_for(weapon_id);
        for s in self.bonuses.iter().chain(self.malus.iter()) {
            if excluded.contains(&s.id.as_str()) {
                out.push(format!("this weapon's riven does not roll {}", s.id));
            }
        }
        out
    }

    /// The value a stat SHOWS, sign included. `bonus = false` applies the
    /// malus multiplier, which is negative and flips the stat.
    ///
    /// EXCEPT ON A MALUS-ONLY STAT, where DE's base already carries the sign
    /// and only the multiplier's SIZE applies. The two negative bases in the
    /// data say different things: Weapon Recoil is negative because its BONUS
    /// reads "-90% Weapon Recoil", so the malus slot flips it to a positive
    /// +67.5%; Chance to Gain Combo Count is negative because it IS the malus,
    /// and flipping it would print a negative slot that reads as a gift.
    /// MEASUREMENTS M71.
    pub fn value_of(&self, stat: &RivenStat, roll: f64, bonus: bool, disposition: f64) -> f64 {
        let rank_scale = PER_RANK * (self.rank + 1) as f64;
        let shape = self.shape();
        let cfg = match (bonus, stat.bonus) {
            (true, _) => shape.bonus_mult(),
            (false, true) => shape.malus_mult(),
            (false, false) => shape.malus_mult().abs(),
        };
        stat.base * rank_scale * disposition * cfg * roll
    }

    /// `(slot, stat, shown value)` for every FILLED slot.
    ///
    /// The slot travels with the value because a card can be half-described
    /// and still have real numbers: the shape is settled the moment it is
    /// chosen, so a stat's value never depends on the other slots being
    /// filled. Skipping the empty ones would slide the rest up by one.
    pub fn resolved_slots(&self, disposition: f64) -> Vec<(usize, &'static RivenStat, f64)> {
        let p = pool(&self.class);
        let find = |id: &str| p.iter().find(|x| x.id == id);
        let mut out = Vec::new();
        for (i, s) in self.bonuses.iter().enumerate() {
            if let Some(def) = find(&s.id) {
                out.push((i, def, self.value_of(def, s.roll, true, disposition)));
            }
        }
        if let Some(c) = &self.malus {
            if let Some(def) = find(&c.id) {
                out.push((self.bonuses.len(), def, self.value_of(def, c.roll, false, disposition)));
            }
        }
        out
    }

    /// `(stat, shown value)` for every rolled stat, bonuses then the malus.
    pub fn resolved(&self, disposition: f64) -> Vec<(&'static RivenStat, f64)> {
        self.resolved_slots(disposition).into_iter().map(|(_, d, v)| (d, v)).collect()
    }

    /// The value a stat would show at the ENDS of its roll band, lowest
    /// first. This is what lets a number box be typed into and stay legal:
    /// the bounds come from the same formula that produces the value.
    pub fn bounds_of(&self, stat: &RivenStat, bonus: bool, disposition: f64) -> (f64, f64) {
        let a = self.value_of(stat, ROLL_MIN, bonus, disposition);
        let b = self.value_of(stat, ROLL_MAX, bonus, disposition);
        if a <= b { (a, b) } else { (b, a) }
    }

    /// The roll a desired VALUE implies, clamped into the legal band — so a
    /// number typed straight from a riven you own lands on a legal roll
    /// instead of being refused.
    pub fn roll_for_value(&self, stat: &RivenStat, bonus: bool, disposition: f64, value: f64) -> f64 {
        let unit = self.value_of(stat, 1.0, bonus, disposition);
        if unit.abs() < 1e-12 {
            return 1.0;
        }
        (value / unit).clamp(ROLL_MIN, ROLL_MAX)
    }

    /// The riven's NAME, generated from its stats — it is not a free field.
    ///
    /// Wiki: the prefix and the core come from the two highest bonus
    /// magnitudes and the suffix from the lowest, in the pattern
    /// `Prefix-CoreSuffix`; the MALUS never contributes a fragment.
    ///
    /// With two bonuses there is no third fragment and the pattern the wiki
    /// also names, `CoreSuffix`, applies. That reading of the two-stat case is
    /// inference from the two patterns it lists, not something it states.
    pub fn name(&self, _disposition: f64) -> String {
        let p = pool(&self.class);
        let mut pos: Vec<(&'static RivenStat, f64)> = self
            .bonuses
            .iter()
            .filter_map(|s| p.iter().find(|x| x.id == s.id).map(|d| (d, s.roll)))
            .collect();
        // Ranked by the ROLL, not by the value. The wiki is explicit —
        // "determined by the magnitude of the randomized MODIFIER on that
        // stat" — and its example proves it: Vectis "Sati-critaata" has
        // Multishot leading, and Multishot's base (90%) is the SMALLEST of
        // the three it carries. Only its roll can have been the largest.
        //
        // This is why the name is disposition-independent: disposition scales
        // every stat alike and cannot reorder anything, and the value never
        // enters at all.
        //
        // TIES ARE THE NORM here rather than the exception, and more so than
        // when this ranked by value: in a CONSTRUCTOR every stat can sit at
        // 1.1 at once, and then all three rolls are equal.
        // The tiebreak is DE's own `upgradeEntries` index — explicit, because
        // a stable sort would have fallen back to the order the stats were
        // added, and a name must not depend on which one someone clicked
        // first. Whether the game breaks the tie the same way is UNVERIFIED;
        // what is verified is that our answer does not move.
        pos.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.order.cmp(&b.0.order)));
        let cap = |s: &str| {
            let mut c = s.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        };
        match pos.len() {
            0 => String::new(),
            1 => cap(&pos[0].0.prefix),
            2 => format!("{}{}", cap(&pos[0].0.prefix), pos[1].0.suffix),
            _ => format!(
                "{}-{}{}",
                cap(&pos[0].0.prefix),
                pos[1].0.prefix,
                pos[2].0.suffix
            ),
        }
    }

    /// Capacity drain. DE's base is 2 and a riven gains 2 per rank, so an
    /// unpolarised riven costs 18 at rank 8 — the number the game shows.
    pub fn drain(&self) -> u32 {
        2 + 2 * self.rank
    }

    /// The riven as a [`ModDef`], so every part of the engine that already
    /// understands a mod understands a riven — resolve, the panel, Forma
    /// planning, the optimizer. `id` must be unique across the pool.
    pub fn to_mod_def(&self, id: &'static str, disposition: f64) -> ModDef {
        let effects = self
            .resolved(disposition)
            .into_iter()
            .filter_map(|(def, v)| effect_of(def, v))
            .collect();
        ModDef {
            // A RIVEN IS NEVER A STANCE.
            stance: None,
            // A riven fits whatever its family fits; it is never written for
            // one weapon the way an augment is.
            exclusive_to: &[],
            // PER STAT, NOT PER CARD. A riven's stats are DE's and one of them
            // is a finisher this arena has no concept of, so the admission goes
            // where the stat is — `/api/riven` marks each `modeled`, and the
            // editor and the mod list both say which line pays nothing. A flag
            // here would print "not modelled yet" over a card whose other two
            // stats are the build.
            unmodeled: false,
            out_of_scope: false,
            id,
            // A riven's card name is the player's own — it is not a DE item, so
            // there is nothing to look up. The UI shows the riven's own label
            // and never wiki-links it (`m.riven` gates that), so the id serves.
            name: id,
            base_drain: self.drain(),
            max_rank: MAX_RANK,
            polarity: self.polarity,
            rarity: Rarity::Legendary,
            exilus: false,
            // A weapon takes ONE riven. Rivens all share
            // one family, which is the rule the pool already has for mutually
            // exclusive mods — so the picker greys the others out, the panel
            // refuses the pair, and the optimizer never enumerates a build
            // holding two, with nothing riven-specific added anywhere.
            family: Some("riven"),
            requires_weapon: None,
            excludes_weapon: Vec::new(),
            set: None,
            requires: None,
            disables: Vec::new(),
            effects,
        }
    }
}

/// One resolved riven stat as a [`ModEffect`]. `None` = a stat the engine does
/// not model; it stays on the card and contributes nothing.
pub(super) fn effect_of(def: &RivenStat, v: f64) -> Option<ModEffect> {
    let element = |n: &str| match n {
        "heat" => Some(DamageType::Heat),
        "cold" => Some(DamageType::Cold),
        "electricity" => Some(DamageType::Electricity),
        "toxin" => Some(DamageType::Toxin),
        "impact" => Some(DamageType::Impact),
        "puncture" => Some(DamageType::Puncture),
        "slash" => Some(DamageType::Slash),
        _ => None,
    };
    Some(match def.kind.as_str() {
        "base_damage_bonus" => ModEffect::BaseDamage(v),
        "multishot_bonus" => ModEffect::Multishot(v),
        "crit_chance_bonus" => ModEffect::CritChance(v),
        "crit_damage_bonus" => ModEffect::CritDamage(v),
        "status_chance_bonus" => ModEffect::StatusChance(v),
        "status_duration_bonus" => ModEffect::StatusDuration(v),
        "fire_rate_bonus" => ModEffect::FireRate(v),
        "reload_speed_bonus" => ModEffect::ReloadSpeed(v),
        "magazine_capacity_bonus" => ModEffect::MagazineCapacity(v),
        "elemental_damage_bonus" => ModEffect::Element(element(def.arg.as_deref()?)?, v),
        "physical_damage_bonus" => ModEffect::Physical(element(def.arg.as_deref()?)?, v),
        "faction_damage_bonus" => {
            let f = Faction::from_name(def.arg.as_deref()?);
            if f == Faction::Unknown {
                return None;
            }
            ModEffect::FactionDamage(f, v)
        }
        "punch_through_bonus" => ModEffect::Indirect(IndirectStat::PunchThrough, v),
        "ammo_max_bonus" => ModEffect::Indirect(IndirectStat::AmmoMax, v),
        "recoil_reduction" => ModEffect::Indirect(IndirectStat::Recoil, v),
        "projectile_speed_bonus" => ModEffect::Indirect(IndirectStat::ProjectileSpeed, v),
        "zoom_bonus" => ModEffect::Indirect(IndirectStat::Zoom, v),
        // MELEE'S OWN, each landing in the bucket its MOD already lands in —
        // the riven's Critical Chance card carries True Steel's "(x2 for Heavy
        // Attacks)" and so does its effect, its Range is Reach's flat metres,
        // and its Combo Duration is Body Count's flat seconds.
        "crit_chance_bonus_heavy_doubled" => ModEffect::CritChanceHeavyDoubled(v),
        "crit_chance_on_slide" => ModEffect::CritChanceOnSlide(v),
        "melee_combo_duration_bonus" => ModEffect::MeleeComboDuration(v),
        "melee_range_bonus_m" => ModEffect::MeleeRange(v),
        "heavy_attack_efficiency" => ModEffect::HeavyAttackEfficiency(v),
        "initial_combo" => ModEffect::InitialCombo(v),
        "combo_count_chance" => ModEffect::ComboCountChance(v),
        "combo_gain_chance" => ModEffect::ComboGainChance(v),
        _ => return None,
    })
}

/// WHICH RIVEN POOL THIS WEAPON ROLLS FROM — the NARROWEST of its mod pools
/// that has riven stats at all.
///
/// The rule lived in `webapi` and had one caller; a second one (canonicalising
/// a board row's elements) is what moved it here. It is the engine's answer for
/// the reason every other equip rule is: two copies of it would be two answers,
/// and this one decides whether a riven's Heat pairs with the build's Cold.
pub fn class_for_weapon(weapon: &str) -> Option<&'static str> {
    let spec = crate::data::weapons::spec(weapon)?;
    if let Some(c) = spec.riven_class.as_deref() {
        return Some(c);
    }
    spec.mod_pools
        .iter()
        .rev()
        .find(|c| !pool(c).is_empty())
        .map(|c| &*Box::leak(c.clone().into_boxed_str()))
}
