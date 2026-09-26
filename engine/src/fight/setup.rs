use super::*;

impl FightParams {
    /// CAN WHAT THIS BODY DROPPED BE COLLECTED? The pack lands where the body
    /// fell and the Tenno does not walk, so the answer is a distance and
    /// nothing else — the flight time a real pickup takes is idealised away
    /// (it is collected the instant the body dies).
    pub fn drop_is_in_reach(&self, body_at: crate::rules::space::Vec2) -> bool {
        crate::rules::space::gap(self.player_at, body_at) <= self.pickup_range_m
    }

    /// WHO FIRES IN THIS FIGHT, seat for seat with [`RunResult::dealt`] and
    /// with the record's `combatant` — the combatant roster, the exact
    /// counterpart of the body roster a formation names.
    ///
    /// ONE WRITER, like [`Self::buff_roster`]. Every reader — the report, the
    /// replay's frames, the record's rows — joins on these ids, so a seat that
    /// starts firing and is not named here is damage nobody can attribute.
    ///
    /// Stable English slugs, never translated: the same rule every id in
    /// `data/` follows, and the page resolves them to names of its own.
    pub fn combatant_ids(&self) -> Vec<String> {
        // A SEAT IS NAMED BY WHERE IT SITS, because the engine does not know
        // what is in it. The page resolves these to names of its own, exactly
        // as it does for a body — which it can only do while each one is
        // DISTINCT, so they are numbered rather than sharing a word.
        let mut out = vec!["wielder".to_string()];
        out.extend((2..=self.also_acting.len() + 1).map(|n| format!("seat{n}")));
        out.truncate(crate::fight::MAX_COMBATANTS);
        out
    }

    /// THE SAME FIGHT WITH ONE MORE THING ACTING IN IT. Its params are
    /// resolved from the SAME arena, so every fight-level term — the foe, the
    /// level, the duration, the faction clock — agrees by construction rather
    /// than by a caller remembering to copy it.
    pub fn and_also(mut self, other: FightParams) -> Self {
        self.also_acting.push(other);
        self
    }

    /// Is this stat LOCKED at the weapon's default by an equipped mod?
    ///
    /// One reader for every live source, so "even negative effects" cannot end
    /// up meaning "every source the resolver happened to see".
    pub fn locks(&self, stat: &str) -> bool {
        self.locked_stats.contains(&stat)
    }

    /// EVERY configurable buff this build carries, as a [`BuffSeries`].
    ///
    /// Deliberately adjacent to [`Self::apply_buff_config`] and written in the
    /// same order off the same fields: the ids are one vocabulary shared by
    /// the config, the web's cards and the replay, and the way to keep three
    /// readers in step is to give them one writer. A buff that gains a config
    /// knob and not a roster entry would be configurable and invisible.
    pub fn buff_roster(&self) -> Vec<BuffSeries> {
        let mut out: Vec<BuffSeries> = Vec::new();
        // Every entry but one is a count out of a count; the exception says so
        // where it is pushed, beside the field it reads. A macro rather than a
        // closure so the one arm that pushes a whole `BuffSeries` can still
        // reach `out`.
        macro_rules! push {
            ($id:expr, $max:expr) => {
                out.push(BuffSeries::stacked($id.into(), $max))
            };
        }
        if self.frenzy {
            push!("frenzy", 1);
        }
        if let Some(multishot) = self.evo_multishot {
            push!("evo_multishot", multishot.max_stacks);
        }
        // UNCAPPED — 0 means "no ceiling", which the api and the UI both read
        // as such rather than as a maximum of zero.
        if self.arcane.enervate_rank.is_some() {
            push!("arcane:secondary_enervate", 0);
        }
        // MELEE INFLUENCE'S WINDOW, as a control. It is EARNED — a roll on an
        // Electricity status opens a clock — so a reader who wants to see what
        // the arcane is worth while it is open has to be able to say so, the
        // way every other earned buff on this bar can be said.
        if self.arcane.influence_chance > 0.0 {
            push!("arcane:melee_influence", 1);
        }
        if let Some(s) = &self.co_stack {
            push!("condition_overload", s.max_stacks);
        }
        if let Some(s) = &self.multishot_stack {
            push!("on_kill_multishot", s.max_stacks);
        }
        if self.evo_base_damage.is_some() {
            push!("evo_reload_damage", 1);
        }
        // READY RETALIATION HAS NO CARD any more. It was one while it was a
        // 6 s window that could be up or down; now it lasts exactly as long as
        // the reload that triggers it, which is not a state a player can be
        // caught without and not a stack count anyone can configure. A card
        // reading 0/1 for a perk that works every time would be a lie.
        // LINGERING JUDGEMENT, the same shape: a window that is open or not.
        if self.headshot_streak.is_some() {
            push!("evo_headshot_streak", 1);
        }
        if let Some(s) = &self.crit_chance_stack {
            push!("on_headshot_kill_cc", s.max_stacks);
        }
        // THE SHOT COMBO COUNTER — a buff by the same three tests: it is
        // gained on a trigger (a landing hit), it is lost on one (two seconds
        // without), and its count is a number a player can read off their own
        // reticle. UNCAPPED (0), because the tiers keep climbing and the wiki
        // gives no ceiling — the eighth is 11025 hits, which no fight reaches
        // and which is not a reason to invent a maximum.
        //
        // It is rostered only when the fight can BUILD one, and `resolve` has
        // already emptied it for a hip-fired scenario — so a hip-fired fight
        // shows a sniper no card at all, which is the one place in the app
        // where not aiming changes what a weapon HAS rather than what it hits.
        // BOTH FORMS ARE ASKED. In an Incarnon cycle the OUTER params are the
        // Incarnon panel's — `incarnon_cycle_from_panels` builds it that way —
        // so a counter declared on the base form is invisible from here, and
        // the Vectis Prime's card was missing for exactly that reason.
        // A MELEE INCARNON IS A BUFF AND IS DRAWN AS ONE — docs/MELEE.md §7.
        // Told apart by how it ENDS, which is what makes it one: a clock is a
        // buff's way out and a charge magazine is a form's.
        if self.cycle.as_ref().is_some_and(|c| matches!(c.ends, Ends::After(_))) {
            push!("melee_incarnon", 1);
        }
        // RAGE, in whole percent out of its cap — the gauge the game draws.
        if !self.combo_script.is_empty() {
            if let Some(s) = crate::data::warframes::warframe(&self.tenno.id).and_then(|f| f.rage) {
                push!(crate::data::rage::BUFF_ID, (s.cap * 100.0).round() as u32);
            }
        }
        if self.sniper_combo.is_some()
            || self.cycle.as_ref().is_some_and(|c| c.base_form.sniper_combo.is_some())
        {
            push!("sniper_combo", 0);
        }
        // TENDRILS — the Ocucor's passive, and a buff by every test that
        // matters: it is gained on a trigger (a kill), it is lost on one (a
        // magazine event), and it has a cap. It is rostered only when a mod
        // READS it, because the count buys nothing on its own (the tendrils'
        // own damage is cosmetic on the beam's target) — a card for it with
        // no Sentient Surge equipped would move no number.
        if self.tendril_max > 0 && (self.crit_chance_per_tendril > 0.0 || self.sc_per_tendril > 0.0) {
            push!("tendrils", self.tendril_max);
        }
        // PYRANA PRIME'S SECOND GUN and the streak that buys it. The streak
        // never shows its last kill: that one turns into the gun.
        if let Some(s) = self.kill_streak_summon {
            push!(crate::model::KillStreakSummonSpec::STREAK_BUFF_ID, s.kills.saturating_sub(1));
            push!(crate::model::KillStreakSummonSpec::BUFF_ID, 1);
        }
        // EVERY stacking buff, by construction. A new one appears on the
        // replay the moment the data declares it — there is no arm to add.
        for b in &self.stacking_buffs {
            push!(b.id, b.max_stacks);
        }
        if self.crit_chance_on_headshot.is_some() {
            push!("on_headshot_cc", 1);
        }
        if self.on_weakpoint.is_some() {
            push!("on_weakpoint_element", 1);
        }
        if self.crit_damage_on_kill.is_some() {
            push!("on_kill_cd", 1);
        }
        if self.base_damage_on_reload.is_some() {
            push!("on_reload_bd", 1);
        }
        if self.base_damage_on_eximus_weakpoint.is_some() {
            push!("on_eximus_weakpoint_bd", 1);
        }
        // HATA-SATYA's pile — a buff by the same three tests the tendrils pass
        // (gained on a trigger, lost on one, capped), with the trigger being a
        // hit rather than a kill. Its cap is the MOD's, not the weapon's, which
        // is why it rides the rate rather than being looked up.
        // …AND THE ONE ROW THAT IS DRAWN AS A NUMBER. Its ceiling is a value
        // DE published (500%) rather than a stack count, so the chart is of the
        // value — see [`StackValue`]. The scale is read off the same field the
        // ceiling came from, which is what stops the two from describing
        // different mods.
        if let Some(c) = self.crit_chance_per_hit {
            out.push(BuffSeries {
                id: "crit_per_hit".into(),
                max_stacks: c.max_stacks(),
                value: Some(StackValue { per_stack: c.per_stack, max: c.max_bonus, unit: "%" }),
            });
        }
        if self.fire_rate_on_reload.is_some() {
            push!("on_reload_fr", 1);
        }
        // One entry per ARCANE, not per grant — the same rule the cards
        // follow, and for the same reason: Frostbite's crit damage and
        // multishot are one stack count by construction.
        let arcane_id = self.arcane.id.clone();
        for spec in self.arcane.buffs.iter() {
            let owner = if spec.owner.is_empty() { &arcane_id } else { &spec.owner };
            let id = format!("arcane:{owner}");
            if !out.iter().any(|x| x.id == id) {
                push!(id, spec.max_stacks);
            }
        }
        out
    }

    /// Apply a per-buff configured policy onto the live specs: the card's
    /// stack count becomes the seed, and a LOCKED card OVERWRITES the buff's
    /// duration with [`crate::model::NO_TIMEOUT`].
    ///
    /// This function is the whole implementation of locking. Nothing downstream knows the concept: every clock in the
    /// sim is `expiry = now + duration`, so an infinite duration is a buff
    /// that earns normally and never falls off. The flag this replaced had to
    /// be re-read at every site that touched a stack count, and was missed at
    /// enough of them that "no timeout" could mean its own opposite.
    ///
    /// Weapon-scoped: recurses into the incarnon cycle's base form.
    pub fn apply_buff_config(&mut self, cfg: &BuffConfig) {
        /// The buff's own duration, or none at all when the card is locked.
        fn clock(duration: f64, locked: bool) -> f64 {
            if locked {
                crate::model::NO_TIMEOUT
            } else {
                duration
            }
        }
        fn set_stack(s: &mut crate::model::StackSpec, cfg: &BuffConfig, id: &str) {
            if let Some(&(stacks, locked)) = cfg.get(id) {
                s.initial_stacks = stacks.min(s.max_stacks);
                s.duration = clock(s.duration, locked);
            }
        }
        fn set_timed(b: &mut crate::model::TimedBuff, cfg: &BuffConfig, id: &str) {
            if let Some(&(stacks, locked)) = cfg.get(id) {
                b.initial_active = stacks > 0;
                b.duration = clock(b.duration, locked);
            }
        }
        // Fevered Frenzy-style permanent stacks: no in-sim trigger, no
        // decay — the configured count is a STATIC multishot choice for the
        // whole run. `locked` is meaningless here (the stacks cannot move
        // either way) and is deliberately ignored.
        // Secondary Enervate: untimed and UNCAPPED, but CONSUMABLE — a big
        // crit resets the pile — so it starts at 0 by the same rule as every
        // timed buff, and the card can say otherwise.
        if self.arcane.enervate_rank.is_some() {
            if let Some(&(stacks, _)) = cfg.get("arcane:secondary_enervate") {
                self.enervate_stacks = stacks;
            }
        }
        if self.arcane.influence_chance > 0.0 {
            if let Some(&(stacks, locked)) = cfg.get("arcane:melee_influence") {
                // OPEN AT THE START, and LOCKED means it never shuts — which is
                // the arcane's ceiling rather than its average, and the reader
                // asked for it by locking it.
                self.influence_open = if locked {
                    Some(f64::INFINITY)
                } else if stacks > 0 {
                    Some(self.arcane.influence_seconds)
                } else {
                    None
                };
            }
        }
        if let Some(base_damage) = self.evo_base_damage {
            if let Some(&(stacks, _)) = cfg.get("evo_reload_damage") {
                let stacks = stacks.min(base_damage.max_stacks);
                // ONE RATIO on the resolved vector. Flat base damage is added
                // pro-rata BEFORE mods, so the bonus rides the whole chain
                // multiplicatively and removing it needs no re-resolve — the
                // same argument `evo_multishot` makes for its scalar.
                let fraction = f64::from(stacks) / f64::from(base_damage.max_stacks);
                let (now, want) = (base_damage.without + base_damage.full, base_damage.without + base_damage.full * fraction);
                if now > 0.0 && want < now {
                    let k = want / now;
                    self.damage = self.damage.scale(k);
                    if let Some(d) = self.dot_modified_base.as_mut() {
                        *d *= k;
                    }
                }
                if let Some(b) = self.evo_base_damage.as_mut() {
                    b.stacks = stacks;
                }
            }
        }
        if let Some(multishot) = self.evo_multishot {
            if let Some(&(stacks, _)) = cfg.get("evo_multishot") {
                let stacks = stacks.min(multishot.max_stacks);
                let fraction = f64::from(stacks) / f64::from(multishot.max_stacks);
                self.multishot -= multishot.full * (1.0 - fraction);
                if let Some(m) = self.evo_multishot.as_mut() {
                    m.stacks = stacks;
                }
            }
        }
        if let Some(s) = self.co_stack.as_mut() {
            set_stack(s, cfg, "condition_overload");
        }
        if let Some(s) = self.multishot_stack.as_mut() {
            set_stack(s, cfg, "on_kill_multishot");
        }
        if let Some(s) = self.crit_chance_stack.as_mut() {
            set_stack(s, cfg, "on_headshot_kill_cc");
        }
        // The combo the run opens with. `locked` is the same reading the
        // tendrils give it: not a duration, but the statement that the thing
        // which ENDS this buff no longer does — here, the decay.
        if let Some(&(stacks, locked)) = cfg.get("sniper_combo") {
            self.combo_initial = stacks;
            self.combo_held = locked;
        }
        // The tendril count, seeded like any stack count. `locked` cannot be a
        // duration here — a tendril has no clock, it is cleared by a magazine
        // event — so it lands on the thing that ENDS this buff instead, which
        // is the same statement everywhere else makes: nothing takes it away.
        if let Some(&(stacks, locked)) = cfg.get("tendrils") {
            self.tendrils_initial = stacks.min(self.tendril_max);
            self.tendrils_held = locked;
        }
        if let Some(s) = self.kill_streak_summon.as_mut() {
            use crate::model::KillStreakSummonSpec as K;
            if let Some(&(stacks, locked)) = cfg.get(K::BUFF_ID) {
                self.kill_streak_summon_opens_active = stacks > 0;
                s.duration_seconds = clock(s.duration_seconds, locked);
            }
            // A LOCKED STREAK never lapses: its clock is the window.
            if let Some(&(stacks, locked)) = cfg.get(K::STREAK_BUFF_ID) {
                self.kill_streak_opens_at = stacks.min(s.kills.saturating_sub(1));
                s.kill_window_seconds = clock(s.kill_window_seconds, locked);
            }
        }
        // Every stacking buff takes the same two knobs the Galvanized family
        // does, and takes them by ID — so a buff the data adds is configurable
        // without a line here.
        for b in self.stacking_buffs.iter_mut() {
            if let Some(&(stacks, locked)) = cfg.get(b.id) {
                b.initial_stacks = stacks.min(b.max_stacks);
                b.duration = clock(b.duration, locked);
            }
        }
        // A MELEE INCARNON TAKES THE SAME TWO KNOBS, and they land where the
        // rule says they land (docs/BUFFS.md): STACKS is what the run opens
        // with — you walked in with it up, which the game allows because the
        // window survives holstering — and NO TIMEOUT is the DURATION, so a
        // locked window simply never closes.
        if let Some(cy) = self.cycle.as_mut() {
            if let Ends::After(seconds) = cy.ends {
                if let Some(&(stacks, locked)) = cfg.get("melee_incarnon") {
                    cy.starts_primed = stacks >= 1;
                    cy.ends = Ends::After(clock(seconds, locked));
                }
            }
        }
        // RAGE TAKES THEM AS PERCENT: what the fight opens at, and a lock that
        // stops the decay.
        if let Some(&(stacks, locked)) = cfg.get(crate::data::rage::BUFF_ID) {
            self.rage_open = Some((f64::from(stacks) / 100.0, locked));
        }
        if let Some(b) = self.crit_chance_on_headshot.as_mut() {
            set_timed(b, cfg, "on_headshot_cc");
        }
        // LEADED GAS is one clock and both stats, so one card switches both.
        if let Some(&(stacks, locked)) = cfg.get("on_weakpoint_element") {
            self.weakpoint_open = Some((stacks > 0, locked));
        }
        if let Some(b) = self.crit_damage_on_kill.as_mut() {
            set_timed(b, cfg, "on_kill_cd");
        }
        if let Some(b) = self.fire_rate_on_reload.as_mut() {
            set_timed(b, cfg, "on_reload_fr");
        }
        // Deadly Efficiency. It is in `buff_roster` and it gets a card, so
        // this arm is what makes that card mean anything — without it both
        // knobs were read, drawn, and dropped.
        if let Some(b) = self.base_damage_on_reload.as_mut() {
            set_timed(b, cfg, "on_reload_bd");
        }
        if let Some(b) = self.base_damage_on_eximus_weakpoint.as_mut() {
            set_timed(b, cfg, "on_eximus_weakpoint_bd");
        }
        // The pile's opening count, seeded like the tendrils' — `locked` cannot
        // be a duration here either, because this buff has no clock: what ends
        // it is the reload.
        if let Some(c) = self.crit_chance_per_hit {
            if let Some(&(stacks, locked)) = cfg.get("crit_per_hit") {
                self.crit_chance_per_hit_initial_stacks = stacks.min(c.max_stacks());
                self.crit_chance_per_hit_held = locked;
            }
        }
        // Keyed off the buff's OWN arcane — not the merged set's id, because a
        // weapon may seat two (an Arch-Gun) and every buff would be renamed the
        // moment a second joined; and not by index either, because ONE ARCANE
        // IS ONE CARD. Frostbite's crit damage and multishot
        // come off the same Cold proc and are the same count by construction,
        // so one setting drives every spec that arcane owns.
        let arcane_id = self.arcane.id.clone();
        for spec in self.arcane.buffs.iter_mut() {
            let owner = if spec.owner.is_empty() { &arcane_id } else { &spec.owner };
            if let Some(&(stacks, locked)) = cfg.get(&format!("arcane:{owner}")) {
                spec.initial_stacks = stacks.min(spec.max_stacks);
                spec.duration = clock(spec.duration, locked);
            }
        }
        if let Some(cy) = self.cycle.as_mut() {
            cy.base_form.apply_buff_config(cfg);
        }
    }

    /// NONE OF THESE TRIGGERS FIRES A BUFF HERE — `crate::data::buff_events`, and
    /// `docs/BUFFS.md` for why. The events still happen and still score.
    ///
    /// **A DENIED BUFF IS REMOVED, NOT ZEROED**, by the argument `NO_TIMEOUT`
    /// makes: a flag has to be honoured at every read site, an `Option`'s zero
    /// is `None`, and nothing downstream learns the concept exists.
    /// Weapon-scoped: recurses into the cycle's base form.
    pub fn deny_buff_triggers(&mut self, denied: &[String]) {
        use crate::data::buff_events::of_builtin;
        if denied.is_empty() {
            return;
        }
        let hit = |t: Option<&str>| t.is_some_and(|x| denied.iter().any(|d| d == x));
        // A ROSTERED ID WITH NO ENTRY IS A PANIC, not a buff nothing can deny:
        // the two look identical from outside.
        let by_id = |id: &str| {
            hit(of_builtin(id)
                .unwrap_or_else(|| panic!("buff `{id}` names no trigger — add it to buff_events::of_builtin")))
        };
        if self.frenzy && by_id("frenzy") {
            self.frenzy = false;
        }
        if self.headshot_streak.is_some() && by_id("evo_headshot_streak") {
            self.headshot_streak = None;
        }
        if self.crit_chance_on_headshot.is_some() && by_id("on_headshot_cc") {
            self.crit_chance_on_headshot = None;
        }
        if self.on_weakpoint.is_some() && by_id("on_weakpoint_element") {
            self.on_weakpoint = None;
        }
        if self.crit_damage_on_kill.is_some() && by_id("on_kill_cd") {
            self.crit_damage_on_kill = None;
        }
        if self.fire_rate_on_reload.is_some() && by_id("on_reload_fr") {
            self.fire_rate_on_reload = None;
        }
        if self.base_damage_on_reload.is_some() && by_id("on_reload_bd") {
            self.base_damage_on_reload = None;
        }
        if self.base_damage_on_eximus_weakpoint.is_some() && by_id("on_eximus_weakpoint_bd") {
            self.base_damage_on_eximus_weakpoint = None;
        }
        if self.crit_chance_per_hit.is_some() && by_id("crit_per_hit") {
            self.crit_chance_per_hit = None;
            self.crit_chance_per_hit_initial_stacks = 0;
        }
        if self.sniper_combo.is_some() && by_id("sniper_combo") {
            self.sniper_combo = None;
            self.combo_initial = 0;
        }
        if self.tendril_max > 0 && by_id("tendrils") {
            self.tendril_max = 0;
            self.tendrils_initial = 0;
        }
        if self.kill_streak_summon.is_some()
            && by_id(crate::model::KillStreakSummonSpec::BUFF_ID)
        {
            self.kill_streak_summon = None;
            self.kill_streak_summon_opens_active = false;
            self.kill_streak_opens_at = 0;
        }
        if self.arcane.enervate_rank.is_some() && by_id("arcane:secondary_enervate") {
            self.arcane.enervate_rank = None;
            self.enervate_stacks = 0;
        }
        // FLAT BASE DAMAGE IS ALREADY IN THE VECTOR, added pro-rata before the
        // mods, so what it bought has to come back out — one ratio.
        if let Some(bd) = self.evo_base_damage {
            if by_id("evo_reload_damage") {
                let now = bd.without + bd.full;
                if now > 0.0 {
                    let k = bd.without / now;
                    self.damage = self.damage.scale(k);
                    if let Some(d) = self.dot_modified_base.as_mut() {
                        *d *= k;
                    }
                }
                self.evo_base_damage = None;
            }
        }
        // A KILL'S PAYLOAD IS DENIED; A KILL'S ECONOMY IS NOT. These three are
        // payloads a card the BUILD carries hands over on a kill — a magazine
        // back (Sentient Surge), a reload for free (Exact Penance), armour off
        // a radius (Jahu Canticle) — and none has a buff card, which is the
        // only reason they survived the ratchets below. What is NOT here is
        // what the fight does with the corpse: affinity, ammo on the floor, an
        // Incarnon gauge. The run still kills, so those still happen.
        if hit(Some("kill")) {
            self.magazine_refill_on_kill = 0.0;
            self.instant_reload_on_kill = None;
            self.strip_on_kill_in_range = None;
        }
        // …AND THE THREE FAMILIES THAT DECLARE THEIR OWN TRIGGER, where a card
        // the data adds tomorrow is classified by nobody.
        for spec in [&mut self.co_stack, &mut self.multishot_stack, &mut self.crit_chance_stack] {
            if spec.as_ref().is_some_and(|s| hit(s.earned_on)) {
                *spec = None;
            }
        }
        self.stacking_buffs.retain(|b| !hit(Some(b.trigger.id())));
        self.arcane.buffs.retain(|b| !hit(b.trigger.id()));
        if let Some(cy) = self.cycle.as_mut() {
            cy.base_form.deny_buff_triggers(denied);
        }
    }

    /// Dual Toxocyst base form damage vector — TEST FIXTURE (the engine
    /// proper knows no specific weapon; production callers build params via
    /// `from_panel` on a data-resolved panel).
    #[cfg(test)]
    pub fn dual_toxocyst_base_vector() -> DamageVector {
        DamageVector::new()
            .with(DamageType::Impact, 7.5)
            .with(DamageType::Puncture, 60.0)
            .with(DamageType::Slash, 7.5)
    }

    /// Dual Toxocyst **base form** as played: Frenzy passive + the chosen
    /// build (Commodore's Fortune + Evolved Autoloader + Fevered Frenzy):
    /// +50 base damage scales the vector pro-rata (75 -> 125, x5/3) and
    /// Commodore's Fortune sets base crit to 25%. Evolution layers apply
    /// to BOTH guns of the transform group.
    #[cfg(test)]
    pub fn dual_toxocyst_base() -> Self {
        Self {
            damage: Self::dual_toxocyst_base_vector().scale(125.0 / 75.0),
            base_crit_chance: 0.25,
            frenzy: true,
            // Sim settings: Frenzy locked at 100% uptime
            // and Fevered Frenzy pre-stacked to 20 (+100% multishot).
            locked_buffs: vec![BuffLock::permanent(LockedBuff::Frenzy)],
            multishot: 2.0,
            ..Self::default()
        }
    }

    /// Dual Toxocyst **Incarnon Form** (data module: 15 I / 37.5 P / 22.5 S,
    /// 11% crit, 3.0x, 43% status, 4.5 fire rate, full-auto). Frenzy WORKS
    /// while transformed — natural headshot
    /// trigger wired here; its Toxin injection needs the loadout layer, so
    /// this bare profile omits it.
    /// The gauge/ammo economy (9 weakpoint charges, 30 rounds each, max 270)
    /// is not cycled here: this profile measures the form in isolation.
    #[cfg(test)]
    pub fn dual_toxocyst_incarnon() -> Self {
        Self {
            // 15/37.5/22.5 x 5/3 (Fevered Frenzy +50 base, pro-rata).
            damage: DamageVector::new()
                .with(DamageType::Impact, 25.0)
                .with(DamageType::Puncture, 62.5)
                .with(DamageType::Slash, 37.5),
            base_crit_chance: 0.31, // 11% + Commodore's Fortune +20 (build)
            crit_multiplier: 3.0,
            status_chance: 0.43,
            fire_rate: 4.5,
            frenzy: true,
            // Pseudo-reload model (gauge locked full): 270 charge-backed
            // rounds, downtime = revert (1.0 s, M9-measured) + re-transmute
            // (2.35 s = base reload).
            magazine_size: 270.0,
            reload_seconds: 3.35,
            ammo_efficiency_applies: false,
            // Fevered Frenzy pre-stacked to 20 (+100% multishot) - the
            // evolution buff applies to both guns of the group.
            multishot: 2.0,
            ..Self::default()
        }
    }

    /// THE FACTION BRACKET AT `t`, Roar included.
    ///
    /// `faction_multiplier` is `1 + Σ bonuses`, and Roar is a bonus in that same
    /// bucket ("considered Faction Damage Bonus, additive with other sources of
    /// Faction Damage" — wiki), so it ADDS to the sum rather than multiplying
    /// the result. Everything the bracket already does then happens to it for
    /// free, the status double-dip included.
    ///
    /// THE TARGET'S OWN MULTIPLIER MULTIPLIES THE FINISHED SUM, and the
    /// position is the whole of it: inside the sum it would be one more Bane
    /// and Roar would land on the wrong side of it; outside `faction_at`'s
    /// power it would be applied once whatever the payload's depth. Here it is
    /// raised with the bracket, which is what was measured
    /// (`faction_bracket_multiplier`).
    pub fn faction_at_time(&self, t: f64) -> f64 {
        self.faction_bracket_at(t) * self.foe.faction_bracket_multiplier
    }

    /// …AND THE SHOOTER'S HALF OF IT ALONE, which is what a Bane's card says.
    /// The product above is what the damage takes; this is what a reader can
    /// CHECK against a mod, and the two are told apart nowhere else.
    pub fn faction_bracket_at(&self, t: f64) -> f64 {
        self.faction_multiplier + crate::data::abilities::faction_bonus_at(&self.abilities, t)
    }

    /// ECLIPSE'S OWN MULTIPLIER at `t`, or 1.0. Applied ONCE wherever it is
    /// applied — the wiki draws the contrast itself: "Unlike faction damage,
    /// which double dips for status effects, the one from Eclipse is applied
    /// once."
    pub fn ability_final_at(&self, t: f64) -> f64 {
        crate::data::abilities::final_mult_at(&self.abilities, t)
    }

    /// ONE ELEMENT'S LIVE BONUS AT `t` — every source of it, and there is no
    /// second path.
    ///
    /// AN ELEMENT BUFF IS A MOD WITH A CLOCK ON IT. Volt's Shock Trooper and
    /// Lavos's imbue are "+X% Electricity", worded and bracketed exactly as
    /// Stormbringer is — "additive with elemental mods" on every one of those
    /// pages — and a weapon augment that grants one (Leaded Gas) is the same
    /// sentence again. So they are read the same way, at the same instant, in
    /// this one function: what differs between them is only WHEN the term is
    /// non-zero.
    ///
    /// AND IT IS READ AT EVERY TICK, which is what being live means: a cloud
    /// burning when the buff lands starts hitting harder, and stops the moment
    /// it lapses. A mod's own share is in the Dot's frozen `bracket` because a
    /// mod is never not equipped — the same number either way.
    pub(super) fn element_at(&self, ty: DamageType, t: f64, w: &CardWindows) -> f64 {
        // THE WEAPON'S OWN WINDOW, opened by an event rather than by a clock —
        // the one thing here a schedule cannot answer.
        let weakpoint = self
            .on_weakpoint
            .filter(|b| b.element == ty && t < w.weakpoint_buff)
            .map_or(0.0, |b| b.bonus);
        weakpoint + self.scheduled_element_at(ty, t)
    }

    /// The share the ABILITIES and the arcane add, which is a pure function of
    /// time — [`Self::element_at`] is what everything reads.
    fn scheduled_element_at(&self, ty: DamageType, t: f64) -> f64 {
        // …AND AN ARCANE'S, which is the same kind of term in the same
        // bracket: `ArcaneFx::added_elements` is held for the whole
        // engagement, so it has no `t` to be read at.
        let arcane: f64 =
            self.arcane.added_elements.iter().filter(|(e, _)| *e == ty).map(|(_, v)| v).sum();
        arcane
            + crate::data::abilities::added_elements_at(&self.abilities, t)
                .iter()
                .filter(|(e, _)| *e == ty)
                .map(|(_, v)| v)
                .sum::<f64>()
    }

    /// The finished vector with the ability elements ON TOP — never through
    /// [`crate::rules::elements::combine`], because they do not combine. A weapon whose mods make Radiation and whose squad
    /// has Volt deals Radiation AND pure Electricity.
    ///
    /// `stage_mb` is THAT attack part's ModifiedBase: an explosion's elemental
    /// mods are a percentage of the explosion's own base (MECHANICS §7), and
    /// an ability sized "additive with elemental mods" is sized the same way.
    pub(super) fn with_live_elements(
        &self,
        qvec: DamageVector,
        stage_mb: f64,
        t: f64,
        w: &CardWindows,
    ) -> DamageVector {
        let mut added = crate::data::abilities::added_elements_at(&self.abilities, t);
        for &(ty, v) in &self.arcane.added_elements {
            match added.iter_mut().find(|(t2, _)| *t2 == ty) {
                Some(slot) => slot.1 += v,
                None => added.push((ty, v)),
            }
        }
        // …AND THE WEAPON'S OWN WINDOW, through the same list: one path, so a
        // card that turns an element on cannot be worth a different number on
        // the hit than in the cloud it seeds.
        if let Some(b) = self.on_weakpoint.filter(|_| t < w.weakpoint_buff) {
            match added.iter_mut().find(|(t2, _)| *t2 == b.element) {
                Some(slot) => slot.1 += b.bonus,
                None => added.push((b.element, b.bonus)),
            }
        }
        if added.is_empty() {
            return qvec;
        }
        let mut out = qvec;
        for (ty, fraction) in added {
            out.add(ty, stage_mb * fraction);
        }
        out.quantized_against(stage_mb)
    }

    /// Build engagement params from a resolved mod loadout (pipeline
    /// [1]+[2] output). Bare-frame scenario: no arcanes, no Frenzy passive
    /// (Incarnon Form), infinite reserve.
    /// A BUILD MEETS AN ARCANE HERE, and only here.
    ///
    /// The arcane is an ARGUMENT rather than something the caller assigns
    /// afterwards, because two of its answers are not the arcane's alone:
    /// Primary Compression is worth what THIS build's blast radius is worth,
    /// and a stat LOCK silences an arcane's buff as it silences a mod's.
    /// Metres from the muzzle to a point on the floor.
    ///
    /// THE SEAM: every distance a damage instance needs is asked this way —
    /// "how far to where this went off" — rather than read off a scenario
    /// field, because the point stops being the target's the moment an
    /// explosion has an epicentre of its own. FROM THE MUZZLE, a point on the
    /// player's own circumference (`rules::space::muzzle`), so every range here is one
    /// radius shorter than the distance between the two bodies.
    pub fn range_to(&self, p: crate::rules::space::Vec2) -> f64 {
        crate::rules::space::muzzle(self.player_at, self.target_at).distance(p)
    }

    /// The ray-versus-circle test's own leg — muzzle to the target's CENTRE,
    /// and not a flight (`rules::space::range_to_centre`). What a shot flies is
    /// [`Self::gap`].
    pub fn range_to_centre(&self) -> f64 {
        crate::rules::space::range_to_centre(self.player_at, self.target_at)
    }

    /// WHERE THE WEAPON POINTS, resolved — the aim point when one is set, and
    /// the target itself when none is, which is the fight this engine ran until
    /// aim became a place you choose. Spelled out here because three callers
    /// were writing `self.aim_at.unwrap_or(self.target_at)` by hand.
    pub fn aim_point(&self) -> crate::rules::space::Vec2 {
        self.aim_at.unwrap_or(self.target_at)
    }

    /// HOW FAR THE TARGET SITS OFF THE AIM LINE, in degrees — 0 whenever the
    /// weapon points at it (`rules::space::off_axis_deg`).
    pub fn off_axis_deg(&self) -> f64 {
        match self.aim_at {
            None => 0.0,
            Some(a) => crate::rules::space::off_axis_deg(
                crate::rules::space::muzzle(self.player_at, a),
                a,
                self.target_at,
            ),
        }
    }

    /// EVERY BODY THIS SHOT PASSES THROUGH, in the order the ray meets them —
    /// index 0 being the aimed body and 1.. lining up with [`Self::others`].
    ///
    /// With no punch-through it is exactly the one body this engine has always
    /// fired at, which is what keeps every existing fight byte-identical.
    ///
    /// STATIC FOR THE ENGAGEMENT: nothing moves, so this is computed once and
    /// hoisted out of the pellet loop rather than asked per shot.
    /// THE FORMATION, IN THE FIGHT'S NUMBERING — 0 is the aimed body and `i`
    /// is `others[i - 1]`, which is the numbering `RunResult::damage_by_body`,
    /// `Fight::bodies` and every spread mechanism already use.
    ///
    /// THE ONLY PLACE THAT ARITHMETIC IS WRITTEN. It was at a dozen call sites,
    /// each pairing a runtime body with the spec beside it by hand, and a
    /// mechanism that reached body 0 through the wrong arm dealt its damage to
    /// the wrong enemy's armour without failing anything.
    pub(crate) fn body(&self, i: usize) -> Option<BodySpec<'_>> {
        match i.checked_sub(1) {
            None => Some(BodySpec {
                id: &self.target_id,
                params: &self.foe,
                body_parts: &self.body_parts,
                at: self.target_at,
            }),
            Some(j) => self.others.get(j).map(|f| BodySpec {
                id: &f.id,
                params: &f.params,
                body_parts: &f.body_parts,
                at: f.at,
            }),
        }
    }

    /// How many bodies the formation holds — never zero.
    pub(crate) fn body_count(&self) -> usize {
        self.others.len() + 1
    }

    /// WHERE THEY ALL STAND, in that same numbering. Built rather than
    /// borrowed because the aimed body's position is a field and the rest live
    /// in a `Vec`; every caller hoists it out of its hot loop.
    pub(crate) fn body_positions(&self) -> Vec<crate::rules::space::Vec2> {
        let mut v = Vec::with_capacity(self.body_count());
        v.push(self.target_at);
        v.extend(self.others.iter().map(|f| f.at));
        v
    }

    pub fn struck_bodies(&self) -> Vec<usize> {
        if self.punch_through_m <= 0.0 || self.others.is_empty() {
            return vec![0];
        }
        let aim = self.aim_point();
        let muzzle = crate::rules::space::muzzle(self.player_at, aim);
        let mut bodies = Vec::with_capacity(self.others.len() + 1);
        bodies.push(self.target_at);
        bodies.extend(self.others.iter().map(|f| f.at));
        let dir = crate::rules::space::Vec2::new(aim.x - muzzle.x, aim.y - muzzle.y);
        let hit = crate::rules::space::struck_along(
            muzzle, dir, &bodies, self.punch_through_m, self.projectile_width_m,
        );
        // THE AIMED BODY IS STRUCK BY DEFINITION. The ray is cast at the aim
        // point, and `webapi` has already resolved which body that is — so a
        // walk that somehow disagrees (an aim point past everything, a body
        // moved onto the line behind it) must not silently drop the target the
        // rest of the engagement is scored against.
        if hit.first() == Some(&0) { hit } else { vec![0] }
    }

    /// WHO A MELEE SWING REACHES, nearest first, the aimed body always first.
    ///
    /// A swing has a REACH and an ARC: a body inside both takes the hit, with
    /// Follow Through's geometric decay in the order the swing got to them.
    ///
    /// TWO SHAPES, because a stance's own table marks them. A `360deg` swing is
    /// a spin and reaches everything within range; an ordinary one sweeps in
    /// FRONT of the wielder, across [`MELEE_ARC_DEG`].
    ///
    /// STATIC FOR THE ENGAGEMENT like `struck_bodies`, and computed per swing
    /// anyway because a combo alternates the two shapes and the list is short.
    ///
    /// `extra_reach_m` is the reach a live buff adds at this swing
    /// (Spring-Loaded Blade's stacks), on top of the resolved range.
    pub fn melee_struck(&self, all_around: bool, extra_reach_m: f64) -> Vec<usize> {
        let reach = match self.range_m {
            r if r.is_finite() && r > 0.0 => r + extra_reach_m,
            _ => return vec![0],
        };
        if self.others.is_empty() {
            return vec![0];
        }
        // WHICH WAY THE SWING FACES — the same line the shot leaves on.
        let aim = self.aim_point();
        let mut out: Vec<(usize, f64)> = vec![(0, crate::rules::space::gap(self.player_at, self.target_at))];
        for (i, f) in self.others.iter().enumerate() {
            let gap = crate::rules::space::gap(self.player_at, f.at);
            if !crate::rules::space::within(gap, reach) {
                continue;
            }
            if !all_around
                && !crate::rules::space::within(
                    crate::rules::space::off_axis_deg(self.player_at, aim, f.at),
                    MELEE_ARC_DEG / 2.0,
                )
            {
                continue;
            }
            out.push((i + 1, gap));
        }
        // NEAREST FIRST, because Follow Through decays in the order the swing
        // reached them and a swing reaches what is closest to it first.
        out.sort_by(|a, b| a.1.total_cmp(&b.1));
        out.into_iter().map(|(i, _)| i).collect()
    }

    /// THE GAP between the two bodies — surface to surface, zero at contact,
    /// and THE DISTANCE A SHOT FLIES.
    ///
    /// WHAT DAMAGE FALLOFF READS, and there is nothing to reconcile: a bullet
    /// vanishes at the target's SURFACE rather than carrying on to its centre,
    /// so the flight, the number on screen and the key a published window is
    /// quoted in are one quantity. `range_to_centre` is one
    /// radius longer and is not a flight — it is the leg the ray-circle test
    /// measures its perpendicular from.
    pub fn gap(&self) -> f64 {
        crate::rules::space::gap(self.player_at, self.target_at)
    }

    pub fn from_panel(
        panel: &crate::build::loadout::ResolvedPanel,
        arena: &crate::arena::Arena,
        arcane: &ArcaneFx,
    ) -> Self {
        // PRIMARY COMPRESSION: the panel brings the metres, the arcane brings
        // what a metre is worth. `adds` joins the live base-damage bracket
        // (diluted by Serration, and it reaches status payloads through
        // ModifiedBase); `multiplies` is a final multiplier on the instance,
        // the slot Secondary Surge occupies.
        let (compression_multiplier, compression_base_damage) = match panel.compression {
            Some(c) => {
                let bonus = arcane.compression_damage_per_m * c.radius_lost_m;
                if c.adds { (1.0, bonus) } else { (1.0 + bonus, 0.0) }
            }
            None => (1.0, 0.0),
        };
        // …AND THE SPHERE IT BOUGHT THAT WITH IS ACTUALLY GONE: *"x0.2 explosion
        // radius"*. The metres charged for are the metres taken, so what is left
        // of the blast is a fifth of it — and the fight is where this can be
        // asked at all, the panel not knowing whether the arcane is equipped.
        //
        // TWO CONDITIONS, one for each way the trade does not happen: the card
        // has to be ON (a weapon's row is data, the arcane is a choice), and the
        // row has to take metres — `radius_lost_m` is 0 on the rows the table
        // marks `doesnt_work`, which pay nothing and shrink nothing.
        //
        // IT CHANGES NO SINGLE-TARGET NUMBER, which is why it went unread for as
        // long as it did: the blast detonates ON the aimed body, at the centre
        // of whatever sphere is left. In a crowd it is the whole cost of the
        // arcane — a 4 m sphere that reached a formation becomes 0.8 m.
        let compression_keeps = match panel.compression {
            Some(c) if c.radius_lost_m > 0.0 && arcane.compression_damage_per_m > 0.0 => {
                crate::build::loadout::COMPRESSION_RADIUS_KEPT
            }
            _ => 1.0,
        };
        let compressed_radial = panel.radial.map(|mut r| {
            r.radius_m *= compression_keeps;
            r.falloff_start_m *= compression_keeps;
            r
        });
        // THE FIELD ONLY WHERE THERE IS NO RADIAL, because that is how the
        // metres were counted (`resolve_for`): a weapon with both pays for its
        // explosion, so its explosion is what shrinks.
        let compressed_lingering = panel.lingering.map(|mut f| {
            if panel.radial.is_none() {
                f.radius_m *= compression_keeps;
                f.falloff_start_m *= compression_keeps;
            }
            f
        });
        let arcane = {
            let mut fx = arcane.clone().without_locked(&panel.locked);
            fx.ammo_efficiency += panel
                .compression
                .map_or(0.0, |c| arcane.compression_effectiveness_per_m * c.radius_lost_m);
            // …AND THE PLAYER'S OWN. The engine has always had the quantity and
            // the panel had no box for it, so a reader who wanted to try an
            // ammo-efficiency source could not say so. It
            // joins the arcane's in the same additive bucket, which is where a
            // second source of one quantity belongs.
            fx.ammo_efficiency += arena.tenno.bonuses.ammo_efficiency;
            fx.ammo_efficiency += panel.ammo_efficiency;
            fx
        };
        // READ BEFORE `arcane` IS MOVED into the struct below. Both halves of
        // Pax Charge, kept as plain numbers so the battery can be built in the
        // initializer without borrowing a field that has already been given
        // away.
        let recharging = arcane.rechargeable_magazine;
        let arc_reload_bonus = arcane.reload_bonus;
        let crate::arena::Arena {
            cast_interrupts,
            apl: apl_inserted,
            target_id,
            tenno,
            target,
            body_parts,
            duration_seconds,
            squad_size,
            abilities,
            ability_picks,
            ability_strength,
            player_at,
            target_at,
            others,
            aim_at,
        } = arena.clone();
        // THE INVOCATIONS MEET THE ABILITIES HERE, and here is the only place
        // they can: a mod belongs to the BUILD and an ability to the FIGHT, and
        // this function is the one that holds both.
        //
        // RE-RESOLVED rather than rescaled. `data::abilities::resolve` applies
        // the strength AND settles the same-family contest, and the contest is
        // decided BY the resolved value — so a bonus big enough to make a
        // Helminth Roar beat a Rhino's has to be in hand before the winner is
        // picked, not multiplied onto the loser afterwards. Rescaling would
        // also have to know which abilities take strength at all, which is a
        // second copy of a rule `resolve` already owns.
        //
        // NOTHING RE-RESOLVES WITHOUT A CARD ASKING. Every fight but one takes
        // the list the arena already carries, byte for byte.
        let abilities = if panel.ability_strength_bonus > 0.0
            || panel.ability_duration_bonus > 0.0
        {
            let picks: Vec<crate::data::abilities::AbilityPick<'_>> = ability_picks
                .iter()
                .map(|p| crate::data::abilities::AbilityPick {
                    id: p.id.as_str(),
                    // DURATION IS A MULTIPLIER ON WHAT WAS ASKED FOR. "The
                    // whole fight" is already the whole fight and cannot be
                    // extended, which is why the `None` case is left alone
                    // rather than given a number.
                    duration_seconds: p
                        .duration_seconds
                        .map(|d| d * (1.0 + panel.ability_duration_bonus)),
                    element: p.element.as_deref(),
                })
                .collect();
            crate::data::abilities::resolve(
                &picks,
                &crate::data::abilities::Caster {
                    strength: ability_strength + panel.ability_strength_bonus,
                    duration: 1.0 + panel.ability_duration_bonus,
                    ..Default::default()
                },
                panel.class,
                panel.slot,
            )
        } else {
            abilities
        };
        // LONE ENFORCER: "+25% Multishot if no enemies are within 5m".
        //
        // HERE, and not in `resolve`, because this is the first clause in the
        // roster that asks about the FIGHT's geometry rather than the player's
        // state — `resolve` is handed a Tenno and never an arena, so a
        // `GatedGrant` could not answer it. This is the seam Primary
        // Compression already uses: the panel brings what the card says, the
        // arena brings whether it is true.
        //
        // ONE ENEMY, so "no enemies within 5m" is exactly "the target is
        // further away than 5m" — decidable now, and it was filed as an edge
        // (`no_distance`) until the arena had a range.
        //
        // A FRACTION OF BASE MULTISHOT, into the same bucket the mods feed —
        // the reading `GatedGrant::Multishot` already carries for every other
        // conditional multishot grant, so a gated +25% and a plain +25% cannot
        // come out as different panels. False at point blank, which is where
        // both boards are scored, so no row moves.
        let lone_bonus = match panel.multishot_beyond_range {
            Some((v, m)) if player_at.distance(target_at) > m => v * panel.base_multishot,
            _ => 0.0,
        };
        // Resolve the faction bucket against THIS target's faction (additive
        // within the matching faction; 1.0 vs a non-match / Unknown).
        let faction_multiplier = 1.0
            + panel
                .faction_damage
                .iter()
                .filter(|(f, _)| *f == target.faction)
                .map(|(_, v)| v)
                .sum::<f64>();
        Self {
            // NOBODY ELSE ACTS UNLESS A CALLER SAYS SO. Another seat is another
            // BUILD, and this constructor was handed one.
            also_acting: Vec::new(),
            sample_by: crate::rules::metrics::RunStat::KillProgress,
            faction_multiplier,
            // RESOLVED ONCE. The picks are the state and this is a view of them,
            // so a pick can never disagree with its effect. The weapon's CLASS
            // is passed because the Amp family pays one class and nothing to
            // any other.
            squad: tenno.squad(panel.class),
            // Straight off the ARENA — the one place a fight is described.
            abilities: abilities.clone(),
            cast_interrupts,
            apl_inserted,
            form: panel.form,
            damage: panel.damage,
            radial: compressed_radial,
            cluster: panel.cluster,
            reload_grenade: panel.reload_grenade,
            falloff: panel.falloff,
            spread: panel.spread,
            // Straight off the ARENA, like `abilities` and `duration_seconds`.
            player_at,
            target_at,
            // …AND SO IS THE REST OF THE FORMATION. A fight's bodies are the
            // fight's, which is what makes the optimizer search the same
            // formation the replay will run.
            others,
            aim_at,
            // The beam's own geometry, when the resolved panel has one — the
            // damage radius here is already the MODDED value, so Firestorm has
            // been applied and what seeds the chains is what the player built.
            beam: panel.beam,
            ricochet: panel.ricochet,
            unaimed_headshot_chance: panel.unaimed_headshot_chance,
            windup_seconds: panel.windup_seconds,
            no_magazine: panel.no_magazine,
            orb: panel.orb,
            // NO METER ON A SINGLE FORM, and that is the `transformed` mode's
            // own definition rather than an omission: it is the form "all
            // engagement", what this thing is worth while you are in it, which
            // is exactly the question a gate must not answer. An Incarnon's
            // transformed mode fires continuously and ignores its gauge for the
            // same reason.
            //
            // The meter binds in the CYCLE, where you have to earn the throw —
            // `tome_cycle_from_panels`.
            meter: None,
            // THE ARENA'S, because the squad is a property of the FIGHT — and
            // the ammo drop table is a function of the squad rather than of the
            // enemy, so this is the one place that answer comes from.
            squad_size,
            // THE TWO PARTS AN ORB DELIVERS, derived from the attack rather
            // than declared beside it. A strike is the attack's own hit and the
            // detonation is its own explosion; the `orb:` block in the data is
            // geometry and a clock, which is the same division `beam:` makes.
            //
            // Both arrive as the resolved TIMED-PART shape, because the
            // arithmetic of a damage instance on a clock of its own is the same
            // whichever mechanism produced it. What is NOT shared is who it
            // lands on, and that is the whole difference between an orb and a
            // field — decided in `process_orbs`, not here.
            orb_strike: panel.orb.map(|_| crate::build::loadout::ResolvedLingering {
                damage: panel.damage,
                modified_base: panel.modified_base,
                crit_chance: panel.crit_chance,
                crit_damage: panel.crit_damage,
                status_chance: panel.status_chance,
                base_crit_chance: panel.base_crit_chance,
                base_crit_damage: panel.base_crit_damage,
                base_status_chance: panel.base_status_chance,
                // The CLOCK belongs to the orb, not to the part; these two are
                // read only by the field walk, which never sees this value.
                tick_rate: 1.0,
                duration_seconds: 0.0,
                first_tick_delay_seconds: 0.0,
                forced_procs: crate::rules::damage::ForcedProcs::from_types(
                    panel.forced_procs.iter().copied(),
                ),
                // A STRIKE HAS NO FALLOFF. It reaches one body, at full damage,
                // wherever inside the orb's reach that body stands — the reach
                // itself is the orb's and lives on `orb` above.
                radius_m: f64::INFINITY,
                falloff_start_m: f64::INFINITY,
                falloff_reduction: 0.0,
                stacking: crate::model::FieldStacking::Stack,
                takes_condition_overload: false,
            }),
            orb_blast: panel.orb.and(panel.radial).map(|r| crate::build::loadout::ResolvedLingering {
                damage: r.damage,
                modified_base: r.modified_base,
                crit_chance: r.crit_chance,
                crit_damage: r.crit_damage,
                status_chance: r.status_chance,
                base_crit_chance: r.base_crit_chance,
                base_crit_damage: r.base_crit_damage,
                base_status_chance: r.base_status_chance,
                tick_rate: 1.0,
                duration_seconds: 0.0,
                first_tick_delay_seconds: 0.0,
                forced_procs: r.forced_procs,
                radius_m: r.radius_m,
                falloff_start_m: r.falloff_start_m,
                falloff_reduction: r.falloff_reduction,
                stacking: crate::model::FieldStacking::Stack,
                takes_condition_overload: r.takes_condition_overload,
            }),
            lingering: compressed_lingering,
            continuous: panel.continuous,
            field_duration_on_empty_reload: panel.field_duration_on_empty_reload,
            multishot_on_last_round: panel.multishot_on_last_round,
            base_multishot_on_last_round: panel.base_multishot_on_last_round,
            multishot_ammo_bonus: panel.multishot_ammo_bonus,
            compression_multiplier,
            compression_base_damage,
            base_damage_below_half_health: panel.base_damage_below_half_health,
            crit_chance_on_undamaged: panel.crit_chance_on_undamaged,
            crit_damage_on_undamaged: panel.crit_damage_on_undamaged,
            arcane,
            headshot_damage_bonus: panel.headshot_damage_bonus,
            headshot_bonus_multiplicative: panel.headshot_bonus_multiplicative,
            noncrit_bonus: panel.noncrit_bonus,
            stacking_buffs: panel.stacking_buffs.clone(),
            base_crit_chance: panel.crit_chance,
            crit_multiplier: panel.crit_damage,
            crit_multiplier_below_crit_chance: panel.crit_multiplier_below_crit_chance,
            unmodded_crit_chance: panel.base_crit_chance,
            unmodded_crit_damage: panel.base_crit_damage,
            status_chance: panel.status_chance,
            base_status_chance: panel.base_status_chance,
            fire_rate: panel.fire_rate,
            charge_seconds: panel.charge_seconds,
            charge_cadence: panel.charge_cadence,
            sustained_fire_rate: panel.sustained_fire_rate,
            // PAX CHARGE turns the magazine into a battery, and this is the
            // one place both halves of it are in hand: the WEAPON states the
            // rate (per chamber, "**not** affected by mods or abilities") and
            // the ARCANE states the delay, as a reload-speed bonus that
            // "stacks additively" with the reload mods.
            //
            // BOTH DELAYS ARE THE SAME, where the Shedu's differ. Its page
            // states two — 1.0 s empty, 0.4 s with rounds left — and Pax
            // Charge's states one, so a split here would be a number nobody
            // published.
            //
            // AN ARCANE THAT CANNOT PAY LEAVES THE WEAPON ALONE rather than
            // taking its own battery away: a chamber whose Pax Charge row
            // nobody has read gets the ordinary reload, which is honest, where
            // a zero rate would be a weapon that never reloads at all.
            echo_multiplier: panel.echo_multiplier,
            battery: match (panel.battery, recharging, panel.recharge_per_second) {
                (_, true, Some(rate)) if rate > 0.0 => {
                    let delay =
                        reload_span(panel.reload_seconds, panel.reload_bonus, arc_reload_bonus);
                    Some(crate::model::Battery {
                        regen_per_second: rate,
                        delay_empty_seconds: delay,
                        delay_partial_seconds: delay,
                    })
                }
                (b, _, _) => b,
            },
            burst: panel.burst,
            frenzy: false,
            magazine_size: panel.magazine_size,
            // …AND A BATTERY'S "RELOAD" IS THE DELAY PLUS THE REFILL.
            //
            // That is the Shedu's own model — its listed 1.25 s reload IS
            // `delay_empty + magazine/rate` — and its yaml states the sum, so
            // nothing extra is needed there. Pax Charge publishes only the
            // DELAY ("base recharge delay is equal to the reload time displayed
            // on the weapon"), so the refill has to be added here or an empty
            // magazine comes back 29/50 = 0.6 s too early on a Tombfinger.
            //
            // PRE-SCALED, because `live_reload_time` divides this by the
            // reload-speed bucket again at fire time and the RATE is "**not**
            // affected by mods or abilities". Compensating for the arcane's own
            // static bonus makes the common case exact; a LIVE reload buff on
            // top still shortens the refill slightly, which is admitted on the
            // arcane's card.
            reload_seconds: match (recharging, panel.recharge_per_second) {
                (true, Some(rate)) if rate > 0.0 => {
                    let b = panel.reload_bonus;
                    let refill = panel.magazine_size / rate;
                    panel.reload_seconds + refill * (1.0 + b + arc_reload_bonus) / (1.0 + b)
                }
                _ => panel.reload_seconds,
            },
            // A CHARGE-BACKED form is "not affected by Ammo Efficiency" (wiki,
            // Torid Incarnon) and every other weapon is. `incarnon.is_some()`
            // is the same marker the magazine rule reads, so the two cannot
            // disagree about which pool is outside the ammo economy.
            //
            // This was hardcoded `false`, which switched ammo efficiency off
            // for EVERY weapon the API simulates — Primary Crux's +60% did
            // nothing, and neither did Frenzy's or a Deadly-Efficiency build.
            // Nothing caught it because every test here builds `FightParams`
            // by hand, where the field defaults to `true`.
            ammo_efficiency_applies: panel.gauge_form.is_none(),
            multishot: panel.multishot + lone_bonus,
            base_multishot: panel.base_multishot,
            evo_multishot: panel.evo_multishot,
            evo_base_damage: panel.evo_base_damage,
            base_damage_bonus: panel.base_damage_bonus,
            co_per_type: panel.co_per_type,
            co_behavior: panel.co_behavior,
            co_base: panel.co_base,
            unswung_fraction: panel.unswung_fraction,
            co_stack: panel.co_stack,
            multishot_stack: panel.multishot_stack,
            crit_chance_on_headshot: panel.crit_chance_on_headshot,
            on_weakpoint: panel.on_weakpoint,
            weakpoint_open: None,
            crit_chance_stack: panel.crit_chance_stack,
            status_damage_multiplier: panel.status_damage_multiplier,
            status_duration_multiplier: panel.status_duration_multiplier,
            elem_dot_bonus: panel.elem_dot_bonus.clone(),
            dot_modified_base: Some(panel.modified_base),
            reload_bonus: panel.reload_bonus,
            reload_from_empty_speed: panel.reload_from_empty_speed,
            weakpoint_damage: panel.weakpoint_damage,
            headshot_multiplier: panel.headshot_multiplier,
            crit_tier_upgrade_chance: panel.crit_tier_upgrade_chance,
            slash_on_crit: panel.slash_on_crit,
            weakpoint_crit_chance_relative: panel.weakpoint_crit_chance_relative,
            bodyshot_crit_chance_multiplier: panel.bodyshot_crit_chance_multiplier,
            derived_status_from_crit: panel.derived_status_from_crit,
            derived_crit_from_status: panel.derived_crit_from_status,
            consecutive_hit_damage: panel.consecutive_hit_damage,
            consecutive_hit_radial_only: panel.consecutive_hit_radial_only,
            last_round_damage: panel.last_round_damage,
            first_round_damage: panel.first_round_damage,
            round_restore_on_status: panel.round_restore_on_status,
            instant_reload_on_kill: panel.instant_reload_on_kill,
            magazine_growth_on_empty_reload: panel.magazine_growth_on_empty_reload,
            crit_damage_on_kill: panel.crit_damage_on_kill,
            fire_rate_on_reload: panel.fire_rate_on_reload,
            base_damage_on_reload: panel.base_damage_on_reload,
            base_damage_on_eximus_weakpoint: panel.base_damage_on_eximus_weakpoint,
            acid_shells: panel.acid_shells,
            crit_chance_per_hit: panel.crit_chance_per_hit,
            combo_script: panel.combo_script.clone(),
            follow_through: panel.follow_through,
            slam: panel.slam,
            heavy: panel.heavy,
            tennokai: panel.tennokai,
            spends_combo: panel.spends_combo,
            combo_duration_seconds: panel.combo_duration_seconds,
            combo_frozen: panel.combo_frozen,
            initial_combo: panel.initial_combo,
            heavy_attack_efficiency: panel.heavy_attack_efficiency,
            crit_chance_per_combo: panel.crit_chance_per_combo,
            status_chance_per_combo: panel.status_chance_per_combo,
            combo_count_chance: panel.combo_count_chance,
            combo_count_chance_on_lifted: panel.combo_count_chance_on_lifted,
            combo_gain_chance: panel.combo_gain_chance,
            combo_count_on_slam_hit: panel.combo_count_on_slam_hit,
            status_chance_on_lifted: panel.status_chance_on_lifted,
            heavy_attack_damage: panel.heavy_attack_damage,
            slam_damage: panel.slam_damage,
            // A fight in contact has not built a pile; the card moves it.
            crit_chance_per_hit_initial_stacks: 0,
            crit_chance_per_hit_held: false,
            rs_on_reload: panel.rs_on_reload,
            armor_strip_per_puncture: panel.armor_strip_per_puncture,
            strip_on_kill_in_range: panel.strip_on_kill_in_range,
            instant_reload: panel.instant_reload,
            headshot_streak: panel.headshot_streak,
            crit_damage_below_status_count: panel.crit_damage_below_status_count,
            super_crit_on_status: panel.super_crit_on_status,
            weakpoint_stacks: panel.weakpoint_stacks,
            spawn_on_kill: panel.spawn_on_kill,
            kill_streak_summon: panel.kill_streak_summon,
            // EARNED, like every other timed buff; the cards move them.
            kill_streak_summon_opens_active: false,
            kill_streak_opens_at: 0,
            beam_ramp_floor: panel.beam_ramp_floor,
            applies_microwave: panel.applies_microwave,
            independent_procs: panel.independent_procs,
            syndicate_radial: panel.syndicate_radial,
            forced_procs: panel.forced_procs.clone(),
            attractor_seconds: panel.attractor_seconds,
            pellet_damage: panel.pellet_damage.clone(),
            multishot_adds_damage: panel.multishot_adds_damage,
            sniper_combo: panel.sniper_combo,
            // NOTHING IN HAND. A fight starts with the counter at zero for the
            // same reason it starts with no tendrils up — the card moves it.
            combo_initial: 0,
            combo_held: false,
            tendril_max: panel.tendril_max,
            target_id,
            punch_through_m: panel.punch_through_m,
            projectile_width_m: panel.projectile_width_m,
            range_m: panel.range_m,
            tendril_range_m: panel.tendril_range_m,
            tendril_acquire_deg: panel.tendril_acquire_deg,
            crit_chance_per_tendril: panel.crit_chance_per_tendril,
            sc_per_tendril: panel.sc_per_tendril,
            // EARNED, like every other timed buff: a fight that has not been
            // in contact has no tendrils up. The card moves it.
            tendrils_initial: 0,
            tendrils_held: false,
            magazine_refill_on_kill: panel.magazine_refill_on_kill,
            proc_conversion: panel.proc_conversion,
            enervate_stacks: 0,
            influence_open: None,
            rage_open: None,
            body_parts,
            foe: target,
            duration_seconds,
            locked_stats: panel.locked.clone(),
            locked_buffs: Vec::new(),
            cycle: None,
            // A weapon runs dry only where the game gives no way to resupply
            // (a ground Arch-Gun). Everywhere else the reserve is a panel
            // figure and the sim keeps firing — we do not model pickups, so
            // stopping would be an artefact of the model, not the game.
            // THE WEAPON-ONLY ANSWER: no reserve at all, or one the game
            // refills. A scenario's Infinite-ammo setting is applied on top of
            // this by the caller — see `parse_fight` — and cannot give ammo
            // back to a weapon that has no way to get any.
            infinite_reserve: !panel.has_reserve || !panel.no_resupply,
            ammo_cost: panel.ammo_cost,
            reserve_ammo: panel.ammo_reserve,
            // THE BODIES DROP unless a scenario says otherwise, which is the
            // game's own behaviour; `parse_fight` applies the setting on top.
            ammo_drops: true,
            ammo_pickup: panel.ammo_pickup,
            ammo_conversion: panel.ammo_conversion,
            pickup_range_m: f64::INFINITY,
            landscape: false,
            // THE SLOT IS THE CLASS. `ammo_type` states the same thing in 373
            // weapon files and never disagrees with `slot`, so it is derived
            // here rather than read twice.
            ammo_class: match panel.slot {
                "primary" => Some(crate::rules::ammo::Pickup::Primary),
                "secondary" => Some(crate::rules::ammo::Pickup::Secondary),
                _ => None,
            },
            tenno,
        }
    }

    /// THE FIGHT THIS PANEL DESCRIBES, un-armed half included.
    ///
    /// **THE DECISION LIVES HERE AND NOWHERE ELSE.** "Does a stated window mean
    /// a two-panel fight?" has exactly one answer, and a caller that asked
    /// `from_panel` directly would silently run a melee Incarnon as if it were
    /// on for the whole engagement — which is what every surface did before the
    /// window was simulated, and is a build no player can produce.
    ///
    /// `unarmed` is a closure because building it means RESOLVING THE SAME
    /// WEAPON AGAIN without the tiers that state the window, and only the
    /// caller knows how to resolve. It is never called on a gun.
    pub fn for_panel(
        panel: &crate::build::loadout::ResolvedPanel,
        arena: &crate::arena::Arena,
        arcane: &ArcaneFx,
        unarmed: impl FnOnce() -> crate::build::loadout::ResolvedPanel,
    ) -> Self {
        match panel.melee_incarnon {
            Some(window) => {
                Self::melee_incarnon_from_panels(panel, &unarmed(), window, arena, arcane)
            }
            None => Self::from_panel(panel, arena, arcane),
        }
    }

    /// A MELEE INCARNON, from the same weapon resolved twice: once with the
    /// Genesis tier that states the window and once without it.
    ///
    /// It is the same machinery the gun cycle uses and that is the point — one
    /// concept for "the numbers change part-way through the fight", with the
    /// two things that actually differ said as data. No stat needs a live
    /// bucket of its own to be part of it, which is what a buff-shaped answer
    /// would have cost: the next Genesis grants something nobody made live, and
    /// the card pays nothing while the panel shows it paying.
    ///
    /// NO ANIMATION AND NO MAGAZINE. A melee Incarnon changes numbers, not
    /// attacks, so both transitions are instant and there is no charge
    /// magazine to spend — [`Ends::After`] is the clock instead.
    pub fn melee_incarnon_from_panels(
        armed: &crate::build::loadout::ResolvedPanel,
        unarmed: &crate::build::loadout::ResolvedPanel,
        window: crate::model::MeleeIncarnon,
        arena: &crate::arena::Arena,
        arcane: &ArcaneFx,
    ) -> Self {
        Self {
            cycle: Some(IncarnonCycle {
                // EARNED, like every other consumable here: the fight opens
                // un-armed and buys the window with a heavy attack.
                starts_primed: false,
                base_form: Box::new(Self::from_panel(unarmed, arena, arcane)),
                arms: Arms::HeavyAtCombo(window.arm_at_combo),
                ends: Ends::After(window.seconds),
                transmute_out_seconds: 0.0,
                transmute_seconds: 0.0,
                reload_bucket: 0.0,
            }),
            ..Self::from_panel(armed, arena, arcane)
        }
    }

    /// The REAL Incarnon cycle engagement from both forms' resolved panels. It
    /// OPENS IN THE BASE FORM WITH AN EMPTY GAUGE (`starts_primed: false`, and
    /// the rule is on that field), rebuilds the gauge on weakpoint hits (Frenzy
    /// per `frenzy_lock`), transmutes, dumps the charge magazine and reverts.
    /// Both transitions scale by the reload formula (M9).
    ///
    /// `frenzy` is the WEAPON's passive, not a constant: it belongs to
    /// whichever weapon lists the perk (Dual Toxocyst does, the Laetum does
    /// not). Hardcoding it here handed DT's ×2.5-on-headshot fire rate to
    /// every transform weapon and made the caller's on/off knob dead in
    /// cycle mode.
    /// THE CYCLE ARMS BOTH FORMS. One arcane, two answers: the Torid's cloud
    /// pays Primary Compression +240% and its Incarnon beam pays nothing, so
    /// each form spends the arcane against its OWN radius.
    pub fn incarnon_cycle_from_panels(
        incarnon: &crate::build::loadout::ResolvedPanel,
        base: &crate::build::loadout::ResolvedPanel,
        frenzy: bool,
        frenzy_lock: LockMode,
        arena: &crate::arena::Arena,
        arcane: &ArcaneFx,
    ) -> Self {
        let rl = 1.0 + incarnon.reload_bonus;
        let inc_form = incarnon.gauge_form;
        let base_form = FightParams {
            frenzy,
            // No ammo-efficiency override here any more: `from_panel` derives
            // it from the panel, and a hardcoded `true` would outrank the data
            // the day a base form became charge-backed. That override existed
            // only to compensate for the broken default it sat next to.
            ..Self::from_panel(base, arena, arcane)
        };
        Self {
            // Frenzy exists in BOTH forms — when
            // the weapon HAS it.
            frenzy,
            locked_buffs: vec![BuffLock {
                buff: LockedBuff::Frenzy,
                mode: frenzy_lock,
            }],
            cycle: Some(IncarnonCycle {
                // The standard reading: earn the first transmute like every
                // other consumable in this sim.
                starts_primed: false,
                base_form: Box::new(base_form),
                // The gauge economy is DATA (the engine knows no weapon
                // names): Dual Toxocyst 9 charges / 1.0 s revert / 2.35 s
                // transmute, Laetum 12 / 2.0 / 2.0. Both transition times
                // scale by the reload formula. An evolution that speeds up
                // charge building (Incarnon Efficiency: +50%) divides the
                // hits needed — 12 becomes 8.
                arms: Arms::Gauge {
                    charge_on: inc_form.map(|f| f.charge_on).unwrap_or_default(),
                    charges_to_fill: inc_form
                        .map(|f| (f.charges_to_fill / (1.0 + f.charge_rate)).ceil() as u32)
                        .unwrap_or(9),
                },
                ends: Ends::ChargeMagazine,
                transmute_out_seconds: inc_form.map_or(1.0, |f| f.transmute_out) / rl,
                transmute_seconds: inc_form.map_or(2.35, |f| f.transmute_in) / rl,
                reload_bucket: rl - 1.0,
            }),
            ..Self::from_panel(incarnon, arena, arcane)
        }
    }

    /// The EXTRA HIT bracket of this form's BASE ATTACK:
    /// `1 + Σ elemental bonuses + Σ (unmodded IPS share × that IPS bonus)`.
    ///
    /// The wiki's `Weapon Hit Damage` term `Unmodded Impact Distribution ×
    /// Impact Bonuses` is why this reads the BASE ATTACK rather than whichever
    /// instance triggered the extra hit, and DE's CN card states the
    /// consequence: a slam whose own damage is 100% Blast still scales its
    /// extra hit by the gun's Impact and Puncture mods ("即使该武器的其它攻击方
    /// 式的初始伤害不包含该物理伤害……依然会根据基本攻击方式的初始伤害受到物理伤
    /// 害MOD加成").
    ///
    /// A RATIO, so the base-damage bucket needs no special handling: `damage`
    /// is `base × (1 + damage mods)` expanded by the element hierarchy and
    /// `dot_modified_base` is the same number before it, so dividing cancels
    /// everything but the bracket. READ AT `t`, because an ability-granted
    /// element is additive with elemental mods and so inside this bracket.
    pub(super) fn extra_hit_bracket(&self, t: f64, w: &CardWindows) -> f64 {
        let mb = self.dot_modified_base.unwrap_or_else(|| self.damage.total());
        if mb <= 0.0 {
            return 1.0;
        }
        self.with_live_elements(self.damage.quantized_against(mb), mb, t, w).total() / mb
    }

    /// The (1 + element bonuses) bracket a MOD gives this element's DoT ticks.
    /// Everything with a clock on it is read per tick instead
    /// ([`Self::element_at`]), which is the same number for a mod and the only
    /// right one for a buff.
    pub(super) fn elem_bracket(&self, t: DamageType) -> f64 {
        self.elem_dot_bonus
            .iter()
            .find(|(x, _)| *x == t)
            .map_or(1.0, |(_, v)| *v)
    }
}

/// THE FIRST KILL IS A TIME, and it is recorded wherever a kill is counted —
/// seven places, because damage lands from seven kinds of source. One method so
/// the two can never disagree: a site that counts a kill and forgets the clock
/// would leave `first_kill_at` reading like a weapon that never killed.
impl RunResult {
    /// …and how many of them a TENDRIL took DIRECTLY, which is the one kind
    /// that spawns nothing.
    ///
    /// Verbatim from the Ocucor's page, in the rule list its weapon file
    /// transcribes: *"a kill by the primary beam, or by a status effect from
    /// any source (including one a tendril applied), spawns a tendril; a DIRECT
    /// kill by a tendril does NOT spawn another."* So a DoT a tendril left
    /// still pays — it is a status kill — and only the tendril's own hit does
    /// not. The counter is the difference between the two.
    pub(super) fn note_tendril_kills(&mut self, killed: u32, at: f64, in_reach: bool) {
        self.kills_by_tendril += killed;
        self.note_kills(killed, at, in_reach);
    }

    /// A KILL, AND WHETHER WHAT IT DROPPED CAN BE REACHED.
    ///
    /// A pickup lands on the BODY (`FightParams::drop_is_in_reach`) and the
    /// Tenno does not walk, so a body that fell outside the pickup radius
    /// leaves a pack nobody collects. Counting kills without that question
    /// could only ever model an infinite reach.
    pub(super) fn note_kills(&mut self, killed: u32, at: f64, in_reach: bool) {
        if killed > 0 && self.first_kill_at.is_none() {
            self.first_kill_at = Some(at);
        }
        self.kills += killed;
        if in_reach {
            self.kills_in_reach += killed;
        }
    }
}
