use super::*;

/// WHICH HALF OF ITS DEATH A BODY IS IN. Only a Thrax has a second half, and
/// only when the fight asked for it (`TargetParams::spectral`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Phase {
    Physical,
    /// The physical form is down and the spectre has not stood up yet: nothing
    /// can be damaged, and nothing ticks.
    Reforming { until: f64 },
    /// A bar only the OPERATOR can empty. This engine fires weapons, so
    /// everything it does to a spectre is refused — which is the whole point of
    /// the switch: it says out loud that no gun finishes a Thrax.
    Spectral,
}

/// Live pools of the target during a run.
#[derive(Clone)]
pub(super) struct TargetState {
    /// See [`Phase`]. `Physical` on every body in every fight but one.
    pub(super) phase: Phase,
    /// WHERE THIS BODY STANDS, so what it drops falls somewhere rather than
    /// into the Tenno's hand: `engine::ammo` pays a pickup only to a player
    /// within `pickup_range_m` of it. A respawn puts the new body in the same
    /// place, which is what `TargetMode::InstantRespawn` means.
    pub(super) at: crate::rules::space::Vec2,
    pub(super) overguard: f64,
    pub(super) shield: f64,
    pub(super) health: f64,
    /// Shield-gate window end (0.1 s after a shield break: all damage
    /// ×5% except direct weakpoint hits — user model, M1).
    pub(super) gate_until: f64,
    /// Attenuation bookkeeping: 1 s buckets anchored at spawn.
    pub(super) atten_window_start: f64,
    pub(super) atten_window_damage: f64,
}

/// THE UNDAMAGED TEST, and every perk that asks the question asks it here.
/// VERBATIM (wiki, Paris Incarnon Genesis): *"Enemies are undamaged as long as
/// their health and shield have not been damaged. Damaging Overguard is not
/// taken into account."*
///
/// IT READS LIVE STATE, NOT HISTORY: a pool restored to full is undamaged
/// again, and a target whose overguard absorbs everything never stops being.
///
/// A free function because the OVERGUARD EXCLUSION has to be assertable
/// DIRECTLY: no sim can catch it, since every fixture that keeps health intact
/// long enough freezes all three pools at once (`TargetMode::InfiniteHealth`),
/// and there a wrong implementation reads the same "undamaged" a right one does.
pub(super) fn target_undamaged(t: &TargetState, p: &TargetParams) -> bool {
    t.health >= p.max_health() - 1e-9 && t.shield >= p.max_shield() - 1e-9
}

impl TargetState {
    pub(super) fn spawn(p: &TargetParams, at: crate::rules::space::Vec2) -> Self {
        Self::spawn_at(p, 0.0, at)
    }

    pub(super) fn spawn_at(p: &TargetParams, now: f64, at: crate::rules::space::Vec2) -> Self {
        if let Err(e) = p.validate() {
            panic!("invalid target: {e}");
        }
        Self {
            at,
            phase: Phase::Physical,
            overguard: p.overguard(),
            shield: p.max_shield(),
            health: p.max_health(),
            gate_until: 0.0,
            atten_window_start: now,
            atten_window_damage: 0.0,
        }
    }

    /// THE POOLS AND THE LIVE ARMOUR, as a row of the combat record needs them.
    ///
    /// Taken BEFORE the instance lands, which is what makes the record a chain
    /// a reader can walk: row `n`'s pools are row `n−1`'s minus what row `n−1`
    /// took out of them, so damage counted twice or never recorded breaks the
    /// chain where it happened (see [`crate::record::TargetAt`]).
    pub(super) fn snapshot(&self, p: &TargetParams, mit: &Mitigation, now: f64) -> crate::record::TargetAt {
        crate::record::TargetAt {
            overguard: self.overguard,
            shield: self.shield,
            health: self.health,
            // `TargetParams::armor` re-runs the level-scaling curve on every
            // call, so this is not the free field read it looks like — which is
            // why every caller takes the snapshot behind `watching`.
            armor: p.armor() * mit.armor_multiplier,
            shield_gate_until: (self.gate_until > now).then_some(self.gate_until),
        }
    }

    /// Which vulnerability column an instance landing RIGHT NOW would read.
    /// Same rule [`apply`](Self::apply) uses — the Overguard layer has its
    /// own table — exposed so a caller can split the reported damage by type
    /// the way the target actually took it. Call it BEFORE `apply`: the pool
    /// it answers for is the one that is still standing.
    pub(super) fn incoming_column(&self, p: &TargetParams) -> crate::data::factions::Column {
        if self.overguard > 0.0 {
            p.type_mods.overguard
        } else {
            p.type_mods.faction
        }
    }

    /// Apply one damage instance under a live [`Mitigation`] snapshot.
    /// Returns `(effective_damage, killed, broken_pool)`.
    ///
    /// Mitigation model (docs/MECHANICS.md §8, unverified). Order is
    /// Overguard → Shields → Health, and every component is first
    /// scaled by the vulnerability COLUMN the pool reads (System B,
    /// `factions_data`) — which is what `shares` is for.
    /// - Overguard: raw × its column × Disrupt amp, no armour, and Toxin does
    ///   NOT bypass it. What the pool cannot absorb CARRIES ON into what is
    ///   under it — the depletion protection is the PLAYER's alone (M82).
    /// - Shields: the non-Toxin portion × Disrupt amp; the Toxin portion goes
    ///   straight to health.
    /// - Health: × Virus amp × (1 − 0.9·√(armor_eff/2700)), `armor_eff` floored
    ///   at 1, unless the instance ignores armour (Cinematic ticks).
    /// - Shield gate (M1): 0.1 s after a break, ALL damage ×5% except direct
    ///   weakpoint hits.
    /// - Attenuation (boss types): clamped per instance and per 1 s bucket,
    ///   after every other layer.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply(
        &mut self,
        raw: f64,
        shares: TypeShares,
        head_direct: bool,
        now: f64,
        p: &TargetParams,
        ignores_armor: bool,
        mit: &Mitigation,
        // WHAT THE CALLER ALREADY MULTIPLIED INTO `raw` FOR OVERGUARD'S SAKE
        // — Secondary Fortifier's `overguard_multiplier`, or 1.0. It is handed
        // over rather than applied here because the caller applies it on the
        // way in; this only needs it to take it back OFF the share that
        // carries past a depleted pool, which never met the Overguard the card
        // names.
        overguard_multiplier: f64,
        // WHERE THE BREAKDOWN GOES, when anyone is watching. `None` is the
        // 999 runs of a thousand that are never replayed.
        led: Option<&mut Breakdown>,
    ) -> Settled {
        // A BODY PAST ITS FIRST DEATH TAKES NOTHING FROM A WEAPON. The
        // reforming window is invulnerable, and the spectre after it is
        // Operator-only — *"Void damage deals 10x damage to the spectral
        // form"*, and this engine has no Operator. Refusing here rather than at
        // the call sites is what makes it hold for a DoT tick, an explosion and
        // a chain alike.
        if self.phase != Phase::Physical {
            if let Phase::Reforming { until } = self.phase {
                if now >= until {
                    self.phase = Phase::Spectral;
                }
            }
            return Settled::default();
        }
        // THE 0.1 s WINDOW IS NOBODY'S, YET. It is set on a shield break below
        // and read by nothing, and that is the measurement rather than an
        // omission: only a melee GROUND SLAM takes it — gunfire does not, and
        // neither does a gun's AoE.
        //
        // This engine fires guns and nothing else, so applying it to every
        // instance was quartering shots the game does not quarter. The state
        // stays because it is the TARGET's and it is correct; what is missing
        // is an attack that reads it, and that arrives with melee.
        //
        // It is DRAWN, in the combat record's `shield_gate_until` — a window
        // nobody can see is a window nobody can check when the time comes.
        let gate = 1.0;
        let gated = raw;
        // THE TARGET AS IT STOOD, before any of this. Read while `led` is still
        // `Some` — i.e. only when somebody is reading — and before the pools
        // below are spent, since a killing instance respawns the body outright
        // and a snapshot taken afterwards would describe a different fight.
        let before = if led.is_some() {
            self.snapshot(p, mit, now)
        } else {
            crate::record::TargetAt::default()
        };

        // Route into pools. The vulnerability COLUMN is chosen by
        // the pool, not only by the enemy: Overguard has its own table (wiki
        // Overguard — neutral but ×1.5 Void), and it is a layer over the unit
        // rather than part of it, so the unit's own column never reaches it.
        //
        // THE LEDGER IS DERIVED FROM THESE VERY NUMBERS, never from a second
        // pass over `shares`. `TypeShares::whole` and its siblings each walk
        // the seventeen damage types, and asking them again for the display
        // half cost **+10.3%** on `one_fight` with every answer unchanged
        // — so `share × column` is recovered by dividing the
        // portion this function already computed, which is O(1) and cannot
        // disagree with it by construction.
        let mut shield_part = 0.0f64;
        let mut health_part = 0.0f64;
        let mut og_part = 0.0f64;
        let mut led_armor_effective = 0.0f64;
        let mut led_hp_armor = 1.0f64;
        let mut led_hp_floored = false;
        // What the overflow past a broken shield was multiplied by on its way
        // into health: the gate's 5%, or 1.0 for a weakpoint hit that ignored
        // it. Left at 0.0 while no shield broke, which is not a factor and is
        // why the ledger only lists it when it happened.
        let mut led_gate_leak = 0.0f64;
        let mut led_shield_cap = 1.0f64;
        let mut led_past_shield = 1.0f64;
        // The half of `health_part` that came over a broken shield rather than
        // straight through it — see the Toxin split above.
        let mut leak_part = 0.0f64;
        let mut led_health_scale = 1.0f64;
        let mut led_og_column = 1.0f64;
        let mut led_sh = (0.0f64, 1.0f64);
        let mut led_hp = (0.0f64, 1.0f64);
        let mut led_sh_dtype = DamageType::Impact;
        let mut led_hp_dtype = DamageType::Impact;
        // WHAT IS LEFT OF THE INSTANCE ONCE OVERGUARD HAS TAKEN ITS SHARE —
        // the whole of it when the target carries none.
        let mut passed = gated;
        let mut led_og_remaining = 1.0f64;
        if self.overguard > 0.0 {
            let whole = shares.whole(&p.type_mods.overguard);
            let full = gated * whole * mit.disrupt_amp;
            if full > self.overguard {
                // ENEMY OVERGUARD CARRIES OVER, and it is the PLAYER's that
                // does not. wiki `Overguard`: a player's has *"a 0.5 second
                // invulnerability gate when fully depleted, preventing any
                // excess damage from leaking into their shield or health
                // pool"*, added in Update 33.6 — whose own note says
                // *"This Overguard Depletion Protection applies only to Player
                // Overguard, not Overguard seen on enemies"*, and the enemy
                // paragraph says *"Enemies do not have any invulnerability
                // gating when their Overguard is depleted."* Measured in game:
                // an Overguard holder can be killed by one shot.
                og_part = self.overguard;
                led_og_remaining = self.overguard / full;
                // …AND IT ARRIVES UNFORTIFIED. Secondary Fortifier multiplies
                // the instance before it gets here and its card says "to
                // Overguard", so the share that never met the pool must not
                // keep the bonus (MEASUREMENTS M38).
                passed = gated / overguard_multiplier * (1.0 - led_og_remaining);
            } else {
                og_part = full;
                passed = 0.0;
            }
            if led.is_some() {
                led_og_column = whole;
            }
        }
        if passed > 0.0 {
            let col = &p.type_mods.faction;
            let toxin = passed * shares.toxin_portion(col);
            let rest = passed * shares.non_toxin_portion(col);
            if self.shield > 0.0 {
                // THE SHIELD GATE, AND IT BELONGS TO THE HIT THAT BREAKS THE
                // SHIELD.
                //
                // wiki `Shield`: *"Enemies have a shield gate that lasts 0.1
                // seconds, during which only 5% of the damage dealt will damage
                // their health"* and *"5% of the damage dealt when hitting the
                // shield gate will target enemy Health"*. This engine had the
                // 0.1 s WINDOW and nothing else: the breaking hit's excess was
                // charged to the shield in full and thrown away (`// no spill`),
                // so a 10,000-damage shot on a 120-point shield left the target
                // at full health. In game it kills.
                //
                // Measured exactly, on four body shots spanning 160 to 1,710
                // damage against a 120-shield level 1 Crewman: health took
                // 2 / 12 / 33 / 80, which is `0.05 × (damage − 120)` to the last
                // digit every time. The five per cent is of the OVERFLOW, not of
                // the whole hit — `0.05 × 160` would be 8 and the pop-up read 2.
                let to_shield = rest * mit.disrupt_amp;
                if to_shield > self.shield {
                    // …AND A WEAKPOINT HIT IGNORES IT COMPLETELY: *"Any
                    // Headshots or shots to Weakspots completely bypass Corpus
                    // enemy Shield Gating"* — the same bool the 0.1 s window
                    // already reads, so the two halves of one rule are one
                    // question asked once.
                    let overflow = to_shield - self.shield;
                    shield_part = self.shield;
                    if to_shield > 0.0 {
                        // TWO SIDES OF ONE DIVISION: what the pool had left to
                        // lose (`Portion::pool_remaining`, on the shield's row)
                        // and what it did not absorb (`Portion::past_shield`, on
                        // the health row the overflow becomes).
                        led_shield_cap = self.shield / to_shield;
                        led_past_shield = overflow / to_shield;
                    }
                    led_gate_leak = if head_direct { 1.0 } else { ENEMY_SHIELD_GATE_LEAK };
                    // KEPT APART FROM THE TOXIN THAT BYPASSED. Both end up in
                    // health and they got there by different routes: Toxin
                    // never met the shield, this crossed a broken one — so it
                    // carries the Disrupt amp the shield damage was computed
                    // with, and Toxin does not. One portion cannot describe a
                    // sum of two chains, so they are two (found by
                    // `check_combat_record`).
                    leak_part = overflow * led_gate_leak;
                    health_part += leak_part;
                } else {
                    shield_part = to_shield;
                }
            } else {
                health_part += rest;
            }
            health_part += toxin;
            // Health mitigation: virus amp + (live) armor.
            let before = health_part;
            if health_part > 0.0 {
                let dr = if ignores_armor {
                    0.0
                } else {
                    scaling::armor_damage_reduction(p.armor() * mit.armor_multiplier)
                };
                let boosted = health_part * mit.virus_amp;
                health_part = if dr > 0.0 {
                    (boosted * (1.0 - dr)).max(1.0)
                } else {
                    boosted
                };
                if led.is_some() {
                    // WHAT VIRUS AND ARMOUR DID, as one factor — it is the same
                    // for both halves of `health_part`, so applying it to the
                    // leak's share is exact rather than proportional.
                    led_health_scale = if before > 0.0 { health_part / before } else { 1.0 };
                    led_armor_effective = p.armor() * mit.armor_multiplier;
                    // THE TERM ACTUALLY APPLIED, floor included. `1 − DR` is
                    // the formula and is not always the number: the health path
                    // floors a mitigated instance at 1 damage, so on a target
                    // thick enough to reach it a ledger printing `1 − DR` would
                    // stop multiplying out.
                    led_hp_armor = if boosted > 0.0 {
                        health_part / boosted
                    } else {
                        1.0
                    };
                    led_hp_floored = dr > 0.0 && (boosted * (1.0 - dr)) < 1.0;
                }
            }
            if led.is_some() && gated != 0.0 {
                // THE SPLIT, named. A shapeless instance is one neutral lump —
                // all of it non-Toxin at ×1.00 — which is what `TypeShares`
                // answers from the other side.
                //
                // A SHARE IS OF THE WHOLE INSTANCE, so what Overguard ate is
                // already gone from it: the ledger's first factor is what
                // reached this pool, and the column falls out of the division
                // unchanged because the same fraction is in both halves.
                let carried = passed / gated;
                let toxin_share = shares.toxin() * carried;
                let split = |portion: f64, share: f64| {
                    if share > 1e-12 {
                        (share, portion / gated / share)
                    } else {
                        (share, 1.0)
                    }
                };
                if self.shield > 0.0 {
                    led_sh = split(rest, carried - toxin_share);
                    led_hp = split(toxin, toxin_share);
                    led_sh_dtype = shares.dominant_non_toxin();
                    led_hp_dtype = DamageType::Toxin;
                } else {
                    led_hp = split(before, carried);
                    led_hp_dtype = shares.dominant();
                }
            }
        }

        // Attenuation: clamp the instance total, then the 1 s bucket.
        let mut effective = og_part + shield_part + health_part;
        let mut atten = 1.0f64;
        if let Some(a) = p.attenuation {
            while now >= self.atten_window_start + 1.0 {
                self.atten_window_start += 1.0;
                self.atten_window_damage = 0.0;
            }
            let hp = p.max_health();
            let allowed = (a.instance_fraction * hp)
                .min(a.dps_fraction * hp - self.atten_window_damage)
                .max(0.0);
            if effective > allowed {
                let k = if effective > 0.0 {
                    allowed / effective
                } else {
                    0.0
                };
                og_part *= k;
                shield_part *= k;
                health_part *= k;
                effective = allowed;
                atten = k;
            }
            self.atten_window_damage += effective;
        }

        // THE PORTIONS, once every factor is final. Pushed in the order the
        // game shows them and skipped where nothing landed, so `portions()` is
        // exactly the numbers that popped.
        //
        // ONLY THE RUN BEING RECORDED PAYS FOR THEM. The breakdown is read by
        // exactly one consumer — the combat record — and a Monte Carlo would
        // pay for it 999 times out of 1000. Filling it unconditionally measured
        // +13.3% on `one_fight`'s four shapes with every answer unchanged —
        // a pure tax.
        let mut out = Settled {
            raw,
            effective,
            killed: false,
            overkill: 0.0,
            spilled: 0.0,
            health: health_part,
            virus_stack_health: f64::from(mit.virus_stacks) * health_part,
            armor_left_health: mit.armor_multiplier * health_part,
            broken: None,
        };
        let Some(b) = led else {
            return Self::finish(self, p, now, og_part, shield_part, health_part, out);
        };
        // EMPTIED FIRST — the slot is reused for every instance in the fight,
        // so a breakdown that only ever appended would grow into a record of
        // the whole run and one instance's row would carry another's numbers.
        b.reset(gate, led_armor_effective, before);
        b.push(Portion {
            pool: Pool::Overguard,
            dtype: shares.dominant(),
            share: 1.0,
            column: led_og_column,
            disrupt_amp: mit.disrupt_amp,
            past_shield: 1.0,
            shield_gate: 1.0,
            virus_amp: 1.0,
            armor: 1.0,
            floored: false,
            attenuation: atten,
            pool_remaining: led_og_remaining,
            effective: og_part,
        });
        b.push(Portion {
            pool: Pool::Shield,
            dtype: led_sh_dtype,
            share: led_sh.0,
            column: led_sh.1,
            disrupt_amp: mit.disrupt_amp,
            past_shield: 1.0,
            shield_gate: 1.0,
            virus_amp: 1.0,
            armor: 1.0,
            floored: false,
            attenuation: atten,
            pool_remaining: led_shield_cap,
            effective: shield_part,
        });
        // TWO WAYS INTO HEALTH, and each gets its own row when both happened.
        // Toxin never met the shield; the leak crossed a broken one, so it
        // carries the Disrupt amp and the gate and Toxin carries neither.
        let leak_effective = leak_part * led_health_scale;
        b.push(Portion {
            pool: Pool::Health,
            dtype: led_hp_dtype,
            share: led_hp.0,
            column: led_hp.1,
            disrupt_amp: 1.0,
            past_shield: 1.0,
            shield_gate: 1.0,
            virus_amp: mit.virus_amp,
            armor: led_hp_armor,
            floored: led_hp_floored,
            attenuation: atten,
            pool_remaining: 1.0,
            effective: health_part - leak_effective,
        });
        b.push(Portion {
            pool: Pool::Health,
            dtype: led_sh_dtype,
            share: led_sh.0,
            column: led_sh.1,
            // THE SHIELD'S OWN AMP IS IN THIS NUMBER, because the overflow was
            // measured against shield damage — `to_shield` is `rest x
            // disrupt_amp`, so what got past it carries that factor whether or
            // not Disrupt does anything to health.
            disrupt_amp: mit.disrupt_amp,
            past_shield: led_past_shield,
            // THE GATE, on the portion it actually multiplied. A field of its
            // own rather than folded into a neighbour, because a ledger whose
            // point is that every line is nameable cannot afford a line whose
            // name is somebody else's.
            shield_gate: led_gate_leak,
            virus_amp: mit.virus_amp,
            armor: led_hp_armor,
            floored: led_hp_floored,
            attenuation: atten,
            pool_remaining: 1.0,
            effective: leak_effective,
        });

        out = Self::finish(self, p, now, og_part, shield_part, health_part, out);
        out
    }

    /// SPEND THE POOLS. Split out of [`Self::apply`] because the ledger's
    /// early exit and the full path must not be two spellings of the pool
    /// arithmetic — a fight whose numbers depended on whether anyone was
    /// watching would be the one bug this whole feature exists to prevent.
    pub(super) fn finish(
        &mut self,
        p: &TargetParams,
        now: f64,
        og_part: f64,
        shield_part: f64,
        health_part: f64,
        mut out: Settled,
    ) -> Settled {
        if p.mode == TargetMode::InfiniteHealth {
            return out;
        }

        if og_part > 0.0 {
            self.overguard -= og_part;
            if self.overguard <= 0.0 {
                out.spilled += -self.overguard;
                self.overguard = 0.0;
                out.broken = Some(BrokenPool::Overguard);
            }
        }
        if shield_part > 0.0 {
            self.shield -= shield_part;
            if self.shield <= 0.0 {
                out.spilled += -self.shield;
                self.shield = 0.0; // no spill
                self.gate_until = now + 0.1;
                out.broken = Some(BrokenPool::Shield);
            }
        }
        self.health -= health_part;
        if self.health <= 0.0 {
            // READ BEFORE THE RESPAWN WIPES IT. Whichever instance takes the
            // bar past zero owns the whole excess — a DoT tick as much as a
            // bullet — and that is the same number as (everything that landed)
            // minus (the bar), because every instance before it was absorbed
            // whole.
            out.overkill = -self.health;
            // A THRAX DIES TWICE, and the FIGHT decides whether the second half
            // happens. The physical form falling is not a kill: no on-kill
            // buff, no drop, no respawn — every one of those waits for the
            // spectre, which no weapon can reach. Its bar is what is left of
            // the individual, and the statuses on the body go with the body.
            if let Some(sp) = p.spectral {
                self.phase = Phase::Reforming { until: now + sp.delay_seconds };
                self.overguard = 0.0;
                self.shield = 0.0;
                self.health = p.max_health() * sp.health_share;
                self.gate_until = 0.0;
                self.atten_window_start = now;
                self.atten_window_damage = 0.0;
                return out;
            }
            *self = TargetState::spawn_at(p, now, self.at); // instant respawn, same spot
            out.killed = true;
        }
        out
    }
}
