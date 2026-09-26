use super::*;

/// One emergent stacking buff, resolved at a rank.
#[derive(Debug, Clone, PartialEq)]
pub struct ArcBuffSpec {
    /// The arcane that granted this buff. A weapon may seat MORE THAN ONE
    /// (an Arch-Gun takes a Primary and a Secondary), and the per-buff config
    /// key is `arcane:<owner>[:<index within that arcane>]` — so the buff has
    /// to carry its origin or merging two arcanes would lose which is which.
    pub owner: String,
    pub grant: ArcGrant,
    pub trigger: ArcTrigger,
    pub per_stack: f64,
    pub max_stacks: u32,
    pub duration: f64,
    /// true = ALL stacks drop on timeout (the on-status family: Cascadia
    /// Flare, Conjunction Voltage); false = lose ONE stack and reset the
    /// timer (the kill family: Merciless/Deadhead/Dexterity).
    pub all_drop: bool,
    /// true = at most ONE stack per damage instance, where the instance is the
    /// TRIGGER PULL and not the pellet. Cascadia Flare states it and names
    /// multishot as the case: *"Only one stack can be added per damage
    /// instance; applying multiple Heat status effects, such as via Multishot
    /// or Archon Vitality in a single hit will not generate multiple stacks."*
    /// Default false, and per ENTRY: the pages for Primary Blight, Primary
    /// Frostbite and Conjunction Voltage — the rest of the same 40-stack
    /// on-status family — do not state the rule, and absence is not evidence
    /// of it (the CO catalog taught this).
    pub one_per_instance: bool,
    /// Stacks at t = 0 (arcane stacking buffs start FULL — user setting).
    pub initial_stacks: u32,
}

/// The flat arcane parameter block the simulator consumes — one arcane,
/// resolved at a rank under a stack policy. `Default` = no arcane (the
/// multiplier fields default to their 1.0 identity, NOT zero).
#[derive(Debug, Clone, PartialEq)]
pub struct ArcaneFx {
    /// AN ECHO — a fraction of THIS hit dealt to every other body within
    /// [`Self::echo_radius_m`] of it. Secondary Irradiate is the only member.
    ///
    /// Zero on everything else, and zero against one target however good the
    /// arcane is: an echo needs somebody to echo to.
    pub echo_share: f64,
    pub echo_radius_m: f64,
    /// …AND WHAT THE HIT BODY HAS TO BE WEARING FOR IT TO FIRE. Secondary
    /// Irradiate's card is *"On hitting enemies afflicted by 10 stacks of
    /// Radiation"*, and 10 is the cap — so this is not a rider on the effect,
    /// it is the effect's whole price.
    ///
    /// Zero means no gate. It was zero for every rank of every build until
    /// 2026-08-18, because the loader read `trigger`/`grants` and the per-rank
    /// values and never `condition:` — so the echo fired on a target with no
    /// Radiation on it at all, which is what a player noticed and reported.
    ///
    /// A CONDITION ABOUT THE TARGET IS SIMULATED; ONE ABOUT THE PLAYER IS
    /// ASSUMED. The other three `condition:` strings in `data/arcanes/` are
    /// Tenno states this sim does not model (sliding, overshields, buffing an
    /// ally), where assumed-max is the house reading and is disclosed as such.
    /// This one is a debuff the engine has tracked since it had statuses at
    /// all, so assuming it was never "we cannot know" — it was not looking.
    pub echo_needs_radiation_stacks: u32,
    pub id: String,
    /// Emergent stacking buffs (kill/status families).
    pub buffs: Vec<ArcBuffSpec>,
    /// Secondary Enervate: run the ramp/reset perk at this rank.
    pub enervate_rank: Option<u8>,
    /// Deadhead rank 5: joins the additive headshot bracket that multiplies
    /// the part multiplier.
    pub headshot_multiplier_bonus: f64,
    /// Static reload-speed bucket addition (Merciless rank 5).
    pub reload_bonus: f64,
    /// PAX CHARGE: does the magazine RECHARGE instead of reloading?
    ///
    /// A flag rather than a rate, because the rate is the WEAPON's and the
    /// arcane does not know it — so an arcane that grants this on a weapon with
    /// no rate grants nothing, which is the honest answer for a chamber whose
    /// row nobody has transcribed.
    pub rechargeable_magazine: bool,
    /// Σ RELATIVE crit-chance bonuses from assumed-max conditionals
    /// (Overcharge, Outburst) — they join the crit-chance BUCKET, so each
    /// attack part multiplies its OWN unmodded base by this. Kept relative all
    /// the way into the sim on purpose: resolving it against the direct part's
    /// base here is what silently excluded the explosion.
    pub crit_chance_relative: f64,
    /// Σ RELATIVE crit-damage bonuses (Outburst) — same rule as `crit_chance_relative`.
    pub crit_damage_relative: f64,
    /// Σ RELATIVE crit chance on weak-point hits only (Cascadia Accuracy,
    /// assumed-max). Direct hits only — a radial never hits a weak point — so
    /// the sim multiplies it by the DIRECT part's base.
    pub weakpoint_crit_chance_relative: f64,
    /// Final damage multiplier on direct hits (Secondary Surge assumed-max:
    /// the cap, multiplicative with Hornet Strike). 1.0 = none.
    pub final_multiplier: f64,
    /// Longbow Sharpshot's bonus on the shot after a weak point hit; 0.0 = not
    /// equipped. The sim decides which shots carry it.
    pub weakpoint_next_shot_damage: f64,
    /// Primary Debilitate's per-instance chance to split a saturated combined
    /// status into one of its components. 0.0 = the arcane is not equipped.
    pub debilitate_chance: f64,
    /// Secondary Shiver: +per per ACTIVE Cold status on the target (cap
    /// `cold_cap`), a GunCO-family source — applied per the weapon's
    /// CoBehavior bracket alongside Condition Overload.
    pub per_cold_base_damage: f64,
    pub cold_cap: u32,
    /// Cascadia Empowered: each status proc adds a flat damage instance of
    /// the proc's type (unaffected by mods/crit/parts; faction once; enemy
    /// mitigation applies).
    pub flat_damage_on_status: f64,
    /// Secondary Encumber: chance that a proc-carrying trigger pull adds ONE
    /// extra status of a uniformly random type (13-type pool, wiki), at most
    /// once per instant (= per trigger pull here).
    pub encumber_chance: f64,
    /// Secondary Cryogenic: Cold statuses applied to the target on each
    /// Puncture status (single-target: the radius burst collapses onto the
    /// main target, which the wiki confirms is also hit).
    pub cold_bursts_on_puncture: u32,
    /// Secondary Fortifier: total damage multiplier while the target still
    /// has Overguard (x3..x8 in-game → stored as the multiplier). 1.0 = none.
    pub overguard_multiplier: f64,
    /// Akimbo Slip Strike under assumed-max (sliding/aim-gliding not simmed):
    /// added to BuffBar ammo efficiency. Gated on the `dual_pistols` trait.
    pub ammo_efficiency: f64,
    /// Primary Compression's two ramps, PER METRE of blast radius given up.
    /// They are not bonuses yet: what they are worth is the weapon's modded
    /// radius times its own row in the arcane's per-weapon table, so
    /// `build::loadout::resolve_for` spends them and the panel carries the answer.
    pub compression_damage_per_m: f64,
    pub compression_effectiveness_per_m: f64,
    /// AN ELEMENT THIS ARCANE ADDS, as a fraction of ModifiedBase — the shape
    /// `data::abilities::AddElement` already has, and it lands in the same
    /// bracket for the same reason: *"additive with elemental mods"*, so it
    /// raises that element's DoT tick as well as the hit.
    ///
    /// IT DOES NOT COMBINE. A weapon whose mods make Viral and an arcane
    /// adding Corrosive deals Viral AND Corrosive, which is what the ability
    /// path already does with Volt's Electricity.
    pub added_elements: Vec<(crate::rules::damage::DamageType, f64)>,
    /// MELEE INFLUENCE — the chance a melee Electricity status opens the window
    /// in which this weapon's elemental statuses spread. Zero = not equipped.
    ///
    /// THREE NUMBERS AND NOT ONE, because the card is two mechanics: a ROLL
    /// that opens a clock, and a RADIUS that the clock makes matter. See
    /// `fight::spread_from_influence`.
    pub influence_chance: f64,
    pub influence_radius_m: f64,
    pub influence_seconds: f64,
}

impl Default for ArcaneFx {
    fn default() -> Self {
        Self {
            id: String::new(),
            buffs: Vec::new(),
            enervate_rank: None,
            echo_share: 0.0,
            echo_radius_m: 0.0,
            echo_needs_radiation_stacks: 0,
            headshot_multiplier_bonus: 0.0,
            reload_bonus: 0.0,
            rechargeable_magazine: false,
            crit_chance_relative: 0.0,
            crit_damage_relative: 0.0,
            weakpoint_crit_chance_relative: 0.0,
            final_multiplier: 1.0,
            weakpoint_next_shot_damage: 0.0,
            debilitate_chance: 0.0,
            per_cold_base_damage: 0.0,
            cold_cap: 0,
            flat_damage_on_status: 0.0,
            encumber_chance: 0.0,
            cold_bursts_on_puncture: 0,
            overguard_multiplier: 1.0,
            ammo_efficiency: 0.0,
            compression_damage_per_m: 0.0,
            compression_effectiveness_per_m: 0.0,
            added_elements: Vec::new(),
            influence_chance: 0.0,
            influence_radius_m: 0.0,
            influence_seconds: 0.0,
        }
    }
}

impl ArcaneFx {
    /// Drop every buff whose grant a LOCKED stat silences.
    ///
    /// Five arcanes grant Multishot (Primary Blight, Frostbite, Overcharge,
    /// Shotgun Vendetta, Conjunction Voltage) and Primary Acuity sets multishot
    /// to the weapon's default "ignoring other bonuses" — so under one they are
    /// worth nothing, exactly as an evolution's live buff is. It is a filter
    /// rather than a zero for the same reason: a card that opens and grants
    /// nothing is a measurement nobody can make.
    pub fn without_locked(mut self, locked: &[&str]) -> Self {
        if !locked.is_empty() {
            self.buffs.retain(|b| !locked.contains(&b.grant.locked_stat()));
            if locked.contains(&"ammo_efficiency") {
                self.ammo_efficiency = 0.0;
            }
        }
        self
    }

    pub fn none() -> Self {
        Self::default()
    }

    /// Fold several arcanes into ONE effect set.
    ///
    /// An Arch-Gun seats two — "Archguns possess two Arcane Enhancement slots
    /// to equip one Primary Arcane and one Secondary Arcane" (wiki Arch-Gun)
    /// — and two arcanes stack the way two mods do: their buckets add and
    /// their buffs coexist. So the SIM never has to learn that a weapon can
    /// have more than one; it reads one `ArcaneFx`, as it always has.
    ///
    /// Every field folds the way its own mechanic does:
    /// - additive buckets SUM (`crit_chance_relative`, `reload_bonus`, …)
    /// - multipliers MULTIPLY (`final_multiplier`, `overguard_multiplier` — 1.0 = none)
    /// - the buff lists CONCATENATE, each spec carrying its `owner` so a
    ///   per-buff config key still names the arcane it came from
    /// - a per-arcane PERK (`enervate_rank`) is whichever states one; no two
    ///   arcanes carry the same perk, so there is nothing to combine
    ///
    /// `id` becomes a joined name for display only — the config keys read
    /// `owner`, not this.
    pub fn merged(parts: &[ArcaneFx]) -> ArcaneFx {
        let live: Vec<&ArcaneFx> = parts.iter().filter(|a| !a.id.is_empty()).collect();
        match live.len() {
            0 => ArcaneFx::none(),
            1 => live[0].clone(),
            _ => {
                let mut out = ArcaneFx {
                    id: live.iter().map(|a| a.id.as_str()).collect::<Vec<_>>().join("+"),
                    ..ArcaneFx::none()
                };
                for a in live {
                    out.buffs.extend(a.buffs.iter().cloned());
                    out.enervate_rank = out.enervate_rank.or(a.enervate_rank);
                    out.headshot_multiplier_bonus += a.headshot_multiplier_bonus;
                    out.reload_bonus += a.reload_bonus;
                    out.crit_chance_relative += a.crit_chance_relative;
                    out.crit_damage_relative += a.crit_damage_relative;
                    out.weakpoint_crit_chance_relative += a.weakpoint_crit_chance_relative;
                    out.per_cold_base_damage += a.per_cold_base_damage;
                    out.cold_cap = out.cold_cap.max(a.cold_cap);
                    out.flat_damage_on_status += a.flat_damage_on_status;
                    out.encumber_chance += a.encumber_chance;
                    out.cold_bursts_on_puncture += a.cold_bursts_on_puncture;
                    out.ammo_efficiency += a.ammo_efficiency;
                    out.compression_damage_per_m += a.compression_damage_per_m;
                    out.compression_effectiveness_per_m += a.compression_effectiveness_per_m;
                    // TWO ARCANES ADDING ONE ELEMENT ADD, because each is
                    // additive with elemental mods and therefore with the
                    // other — `added_elements_at` says the same for abilities.
                    for &(t, v) in &a.added_elements {
                        match out.added_elements.iter_mut().find(|(t2, _)| *t2 == t) {
                            Some(slot) => slot.1 += v,
                            None => out.added_elements.push((t, v)),
                        }
                    }
                    out.final_multiplier *= a.final_multiplier;
                    out.weakpoint_next_shot_damage += a.weakpoint_next_shot_damage;
                    // One arcane grants it and a weapon seats at most one of
                    // any arcane, so this is a max rather than a sum — summing
                    // would invent a stacking rule nothing states.
                    out.debilitate_chance = out.debilitate_chance.max(a.debilitate_chance);
                    out.overguard_multiplier *= a.overguard_multiplier;
                }
                out
            }
        }
    }
}
