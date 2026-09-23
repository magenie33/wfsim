// SPDX-License-Identifier: AGPL-3.0-or-later
//! WHAT THIS SHOT IS, before any of it has left the barrel: the live bar read
//! once, the crit and status chances it fires at, how many pellets it throws
//! and what the round costs the magazine.
//!
//! ONCE PER TRIGGER PULL. Every number here is the same for every pellet of
//! this shot, so resolving it per pellet would be both slower and wrong — a
//! multishot roll or a last-round bonus is a property of the PULL.

use super::*;

/// The shot's own numbers, as the pellet loop reads them.
pub(super) struct Resolved {
    pub(super) flat_crit: f64,
    pub(super) crit_chance_relative: f64,
    pub(super) effective_cc: f64,
    pub(super) undamaged: bool,
    pub(super) weakpoint_cd: f64,
    pub(super) weakpoint_sc: f64,
    pub(super) sc_arc_shot: f64,
    pub(super) live_rate: f64,
    pub(super) bd_reload_add: f64,
    pub(super) bd_eximus_add: f64,
    pub(super) rolled: u32,
    pub(super) sc_mult: f64,
    pub(super) cc_mult: f64,
    pub(super) dt_mult: f64,
    pub(super) ms_damage: f64,
    pub(super) n_pellets: u32,
    pub(super) beam_merge: f64,
}

/// Read the live bar, roll the multishot, and spend the round.
#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_the_shot(
    params: &FightParams,
    active: &FightParams,
    rec: &mut crate::record::Record,
    d: &mut crate::rules::rng::Draws,
    t: f64,
    combo_multiplier: f64,
    bar: &mut BuffBar,
    arc: &mut ArcRuntime,
    gal: &mut GalStacks,
    buff_stacks: &mut [LiveStacks],
    rec_roster: &[BuffSeries],
    rec_buff_index: &[Option<usize>],
    windows: &mut CardWindows,
    tendril: &Tendrils,
    crit_per_hit: &CritPerHit,
    sniper_combo: &SniperComboCount,
    combo_spec: Option<crate::model::SniperCombo>,
    influence_until: f64,
    incarnon: &mut IncarnonState,
    ammo: &mut Ammo,
    debuffs: &mut DebuffState,
    target: &TargetState,
    weakpoint_pile: &mut LiveStacks,
    double_tap: &mut DoubleTap,
    rs_armed: &mut bool,) -> Resolved {
        // Timed buffs (Frenzy) lapse before this shot reads the bar;
        // Permanent locks re-assert — only in phases where the perk exists
        // (Frenzy belongs to the base form).
        bar.expire(t);
        for lock in &params.locked_buffs {
            if lock.mode == LockMode::Permanent {
                match lock.buff {
                    LockedBuff::Frenzy => {
                        if active.frenzy {
                            bar.upsert(Frenzy::permanent_buff());
                        }
                    }
                }
            }
        }

        // Crit chance: base + Enervate stacks (attacker BuffBar) + Weakened
        // stacks (target DebuffBar: flat crit chance received, weapon direct
        // damage only — which our shots are).
        let contribs = bar.total_contributions();
        // Ammo: consume (1 - efficiency) per shot; Frenzy's +100% efficiency
        // zeroes consumption (unless this magazine is charge-backed).
        // Efficiency is a DIVIDED COST, not a chance to save a round: the cost
        // is `1 x (1 - efficiency)` and the magazine keeps the fraction (wiki
        // Energized Munitions: "dividing the ammo cost … and keeps track of the
        // fractions as well"). A partial round still fires — the Exergis's
        // 1-round magazine takes four 0.25 shots — which is why the gate above
        // is "anything left" rather than "a whole round left".
        //
        // A lapsing buff does NOT strand the remainder — ✅ measured
        // (MEASUREMENTS M14): the shot fires at full cost off whatever is left,
        // the counter goes NEGATIVE, and the reload carries that debt into the
        // fresh magazine (see the `+=` above).
        // BuffBar (Frenzy) + static arcane (Akimbo Slip Strike, assumed-max) +
        // live arcane stacks (Primary Crux). Summed and capped by
        // `ammo_efficiency`, which is also what `next_cost` above reads — one
        // definition, so the two cannot drift apart.
        // …AND DEATH KNELL'S: on a one-round magazine, the reload not happening.
        let efficiency = ammo_efficiency(
            active.ammo_efficiency_applies,
            contribs.ammo_efficiency
                + weakpoint_ammo(params.weakpoint_stacks, weakpoint_pile, t),
            params.arcane.ammo_efficiency,
            arc.total(&params.arcane.buffs, ArcGrant::AmmoEfficiency, t),
            crate::data::abilities::ammo_efficiency_at(&params.abilities, t),
        );
        // Final Fusillade's gate, read BEFORE the round is spent: this pull is
        // the magazine's last round if there is at most one left to fire. On a
        // charge-backed form `multishot_on_last_round` is 0.0 anyway (the
        // evolution loader dropped it), so the flag costs nothing there.
        //
        // On a BURST weapon the window is the last BURST, not the last round —
        // Forceful Finality reads "+5 Base Multishot on final magazine burst",
        // and a Burston's final burst is three rounds. Taking the wiki
        // literally as one round would have understated a full magazine's
        // pellets by a fifth (42 + 3x6 = 60 real, against 44 + 6 = 50), which
        // is far too big to wave through as a rounding difference.
        let last_n = active.burst.map_or(1.0, |b| f64::from(b.count));
        // …AND THE WINDOW IS THE ACTIVE MAGAZINE'S, whichever that is.
        // `in_base_form` is only ever true inside an Incarnon CYCLE, so this
        // branch must not read "the cycle's base phase" against "everything
        // else": everything else includes a plain base-form run, which is how a
        // `base`-mode board row is played and how anyone measures the weapon on
        // its own. A burst window written for the cycle silently becomes one
        // round outside it — 5 pellets a magazine instead of 15 on a Burston.
        let mag_left = if incarnon.in_base_form { incarnon.base_magazine } else { ammo.loaded };
        let last_round = mag_left <= last_n + 1e-9;
        // THE CHAMBER FAMILY'S GATE, and it is READ AFTER THE ROUND IS PAID
        // FOR. Both cards say "+X% Damage on first shot in Magazine" and both
        // pages say what that really means: the bonus lands *"as long as the
        // magazine counter is at Max Magazine - 1 AFTER a shot is fired"*, and
        // *"the buff doesn't apply on a completely full one"*.
        //
        // On an ordinary weapon that is exactly the first shot out of a fresh
        // magazine — full goes to full-1 — so the plain case needs no thought.
        // It is AMMO EFFICIENCY that makes the wording load-bearing: a free
        // shot leaves the counter where it was, so a FULL magazine pays
        // nothing however often you fire it and one sitting at max-1 pays
        // every single shot. That is the wiki's Vulkar example, and it is the
        // reason this cannot be `mag_left == mag_max` at the top of the pull.
        //
        // The cost is `active.ammo_cost * (1 - efficiency)`, spelled the same way
        // the spend below spells it — one expression, so the reading and the
        // payment cannot drift.
        let mag_max = if incarnon.in_base_form {
            params.cycle.as_ref().map_or(0.0, |c| c.base_form.magazine_size)
        } else {
            ammo.cap
        };
        let first_round = active.first_round_damage > 0.0
            && ((mag_left - active.ammo_cost * (1.0 - efficiency)) - (mag_max - 1.0)).abs() < 1e-9;
        // The round itself is spent BELOW, once the multishot roll is known:
        // Plentiful Mayhem makes the extra projectiles cost ammo too, so the
        // draw cannot be settled before the roll.

        // CRIT SOURCES SPLIT BY KIND, because an attack part has its OWN base
        // crit stats (§7 Radial): a RELATIVE bonus joins the crit bucket and
        // therefore scales each part's own base, while an ABSOLUTE add (a
        // target-side debuff, a flat grant) lands the same on every part. Both
        // reach the explosion — the crit things a radial loses are the
        // body-part/headshot layer, which it has no hit location for, and
        // Puncture's Weakened, which the wiki excludes from AoE by name.
        //
        // Absolute, shared by every stage:
        // …AND A WARFRAME'S. Wrathful Advance is *"a flat value applied AFTER
        // mods"*, which is this bucket exactly: it lands on every attack part
        // and is never multiplied by a crit-chance card.
        let flat_crit = contribs.flat_crit_chance
            + crate::data::abilities::flat_crit_at(&params.abilities, t);
        let weakened_cc = WEAKENED_FLAT_CC_PER_STACK * debuffs.weakened_active(t) as f64;
        // Relative, shared by every stage: Crosshairs' on-headshot buff and
        // its per-stack-expiry kill stacks (assumes constant aiming), plus the
        // arcane's assumed-max conditionals (Overcharge/Outburst).
        let crit_chance_relative = params.crit_chance_on_headshot.map_or(0.0, |b| {
            if t < windows.crit_on_headshot {
                b.value
            } else {
                0.0
            }
        }) + params.crit_chance_stack.as_ref().map_or(0.0, |s| {
            windows.crit_on_headshot_stacks.retain(|&e| e > t);
            s.per_stack * windows.crit_on_headshot_stacks.len() as f64
        }) + params.arcane.crit_chance_relative
            // SENTIENT SURGE: "Additive to other crit chance and status chance
            // mods", so it belongs in the RELATIVE bucket beside Pistol
            // Gambit's — multiplying the unmodded base, not the modded one.
            + params.crit_chance_per_tendril * f64::from(tendril.count)
            // HATA-SATYA: "additive with similar mods. For example, a max rank,
            // max bonus Hata-Satya and Point Strike will have a 30% × (1 + 500%
            // + 150%) critical chance" — the wiki does the bracket for us, and
            // it is the same one Point Strike is in.
            + params
                .crit_chance_per_hit
                .map_or(0.0, |c| c.bonus(crit_per_hit.stacks))
            // BLOOD RUSH. `Crit Chance = Weapon Crit Chance x [1 + Mod Crit
            // Bonus + Blood Rush Bonus x (Combo Multi - 1)] + Static Crit
            // Bonus` (wiki, verbatim) — so it belongs in this bracket beside
            // Point Strike's, multiplying the UNMODDED base, and nowhere else.
            //
            // IT READS THE COUNTER AND NEVER SPENDS IT, which is why it is
            // worth everything in the four combo modes and nothing in the two
            // heavy ones: there the counter is emptied by the swing that reads
            // it, so it is standing at the floor when the next one starts.
            + active.crit_chance_per_combo * (combo_multiplier - 1.0)
            // …AND EVERY STACKING GRANT OF IT, the bracket Prolific
            // Perforation's card puts itself in by naming Pistol Gambit.
            + buff_total(active, crate::model::BuffGrant::CritChance, buff_stacks, t);
        // VICIOUS PROMISE, both halves of it. VERBATIM (wiki, Paris Incarnon
        // Genesis): "Enemies are undamaged as long as their health and shield
        // have not been damaged. Damaging Overguard is not taken into account."
        // So OVERGUARD IS EXCLUDED from the test — a target being chewed
        // through its overguard is still undamaged, and reading all three pools
        // would have switched this off on the first shot of every Eximus fight.
        //
        // Read per SHOT beside `effective_cc`, which is where the weapon's crit
        // chance is decided; the grants are already converted by `resolve` into
        // the post-mod numbers the card's "Base" wording earns.
        let undamaged = (active.crit_chance_on_undamaged > 0.0 || active.crit_damage_on_undamaged > 0.0)
            && target_undamaged(target, &params.foe);
        // THE ARCANE'S STATUS BONUS, hoisted to SHOT level so a derived stat can
        // read the live status chance the same way it reads the live crit one.
        // The pellet loop below re-reads it for its own roll; this is the same
        // number, one scope out.
        // DEATH KNELL'S TWO NUMBERS, read once per SHOT — every stacking buff
        // in this loop follows that rule. Both add to the FINISHED value.
        let (weakpoint_cd, weakpoint_sc) = params.weakpoint_stacks.map_or((0.0, 0.0), |w| {
            let n = f64::from(weakpoint_pile.current(t, w.duration_seconds));
            (w.crit_multiplier * n, w.status_chance * n)
        });
        let sc_arc_shot = arc.total(&params.arcane.buffs, ArcGrant::StatusChance, t)
            + params.sc_per_tendril * f64::from(tendril.count)
            // WEEPING WOUNDS, the same sentence on the status side: `Status
            // Chance = Weapon Status Chance x [1 + Mod Status Bonus + Weeping
            // Wounds Bonus x (Combo Multi - 1)]`. It rides `sc_arc_shot`
            // because that is this loop's name for "relative status the panel
            // could not fold in", which is exactly what a live counter is.
            + active.status_chance_per_combo * (combo_multiplier - 1.0)
            // ENDURING AFFLICTION, whose gate is a status the engine tracks:
            // every heavy slam forces `Lifted`, so from the second slam on the
            // target is carrying it and the card pays.
            + if debuffs.lifted.is_some_and(|e| e > t) { active.status_chance_on_lifted } else { 0.0 }
            // …AND AN ON-KILL STATUS BUFF (Galvanized Elementalist), which is
            // relative like every other card in this bracket.
            + buff_total(active, crate::model::BuffGrant::StatusChance, buff_stacks, t)
            // …AND LEADED GAS' half of one window, relative like the rest of
            // this bracket. Its other half is an ELEMENT and is added to the
            // vector, not here.
            + if t < windows.weakpoint_buff {
                active.on_weakpoint.map_or(0.0, |b| b.bonus)
            } else {
                0.0
            };
        // HIGH GROUND, LIVE: "+25% of CURRENT Status Chance". The panel folded
        // in what it could see; this takes that back and pays what the shot
        // actually has, which is the panel's status plus whatever the arcanes
        // are adding right now.
        let derived_cc = match active.derived_crit_from_status {
            Some((rate, cap, folded)) => {
                let live_sc = active.status_chance + active.base_status_chance * sc_arc_shot;
                (rate * live_sc).min(cap) - folded
            }
            None => 0.0,
        };
        let effective_cc = active.base_crit_chance
            + flat_crit
            + weakened_cc
            + active.unmodded_crit_chance * crit_chance_relative
            + derived_cc
            + if undamaged { active.crit_chance_on_undamaged } else { 0.0 };

        // Live fire rate (base + Pressurized Magazine's on-reload buff, ×
        // the BuffBar multiplier) — schedules shots below and gates
        // Hemorrhage's below-2.5 doubled chance.
        let fr_reload_add = match active.fire_rate_on_reload {
            Some(b) if t < windows.fire_rate_after_reload => b.value,
            _ => 0.0,
        };
        // A LOCKED fire rate is the weapon's default and nothing else: not
        // Pressurized Magazine's on-reload add, not Frenzy's x2.5 in the bar.
        let live_rate = if params.locks("fire_rate") {
            active.fire_rate
        } else {
            (active.fire_rate + fr_reload_add) * contribs.fire_rate_multiplier
        };
        // Deadly Efficiency's live share of the BASE-DAMAGE bucket. Zero until
        // a reload has finished, and zero again when the window closes.
        let bd_reload_add = match active.base_damage_on_reload {
            Some(b) if t < windows.base_damage_after_reload => b.value,
            _ => 0.0,
        };
        // …and Eximus Advantage's share of the same bucket. "Stacks additively
        // with base damage bonuses like Hornet Strike", so it joins here rather
        // than forming a factor of its own.
        let bd_eximus_add = match active.base_damage_on_eximus_weakpoint {
            Some(b) if t < windows.base_damage_eximus => b.value,
            _ => 0.0,
        };

        // Multishot: pellets this pull = floor + fractional chance; every
        // pellet is an independent damage instance. Earned Galvanized
        // stacks and arcane multishot stacks (Conjunction Voltage: a
        // RELATIVE bonus × base pellets) add live.
        // ...unless MULTISHOT IS LOCKED, in which case the weapon fires its
        // default pellet count and nothing adds to it — an Acuity's sentence is
        // "set to its default ignoring other bonuses", and an arcane's stacks
        // are other bonuses. `resolve` has already emptied the panel's own
        // buckets; this is the live half it cannot reach.
        let ms_locked = params.locks("multishot");
        let ms_eff = active.multishot
            + params
                .multishot_stack
                .as_ref()
                .map_or(0.0, |s| s.per_stack * gal.multishot.current(t, s.duration) as f64)
            + if ms_locked {
                0.0
            } else {
                active.base_multishot * arc.total(&params.arcane.buffs, ArcGrant::Multishot, t)
            }
            // Final Fusillade: a FLAT add on the magazine's last round. It
            // joins `ms_eff` rather than the multishot BUCKET because the
            // evolution grants multishot outright ("+3 Multishot"), not a
            // percentage of the weapon's base.
            + if last_round { active.multishot_on_last_round } else { 0.0 }
            // FORCEFUL FINALITY IS THE OTHER BRACKET, and the card says which:
            // "+5 BASE Multishot on final magazine burst", with the wiki noting
            // on that same row that it is "added before mods, and is thus
            // multiplied by multishot bonuses". So for that burst the weapon's
            // base pellet count IS higher, and everything relative reads the
            // raised number — the mod bucket AND the live grants below it.
            //
            // The bucket is recovered as `multishot / base_multishot`, the
            // ratio the panel already resolved, rather than carried a second
            // time: two copies of one factor is how they come to disagree.
            // `base_multishot` is a weapon stat and never zero.
            + if last_round && active.base_multishot_on_last_round > 0.0 && !ms_locked {
                active.base_multishot_on_last_round
                    * (active.multishot / active.base_multishot.max(1e-9)
                        + arc.total(&params.arcane.buffs, ArcGrant::Multishot, t))
            } else {
                0.0
            }
            // Stormburst: "+0.4 Multishot", flat — same reason Final Fusillade
            // sits here rather than in the bucket above.
            + buff_total(active, crate::model::BuffGrant::FlatMultishot, buff_stacks, t)
            // BLAZING BARREL, both of its shapes, and they are two brackets.
            //
            // "+0.05 BASE Multishot" is added before mods and is therefore
            // MULTIPLIED by them — the bucket recovered as `multishot /
            // base_multishot`, exactly as Forceful Finality does two arms up
            // and for the same quoted reason. "+5% Multishot" is what a
            // multishot MOD grants, so it is a share of the weapon's base.
            //
            // Both are silenced by an Acuity lock, like every other live
            // multishot grant here.
            + if ms_locked {
                0.0
            } else {
                buff_total(active, crate::model::BuffGrant::BaseMultishot, buff_stacks, t)
                    * (active.multishot / active.base_multishot.max(1e-9))
                    + buff_total(active, crate::model::BuffGrant::Multishot, buff_stacks, t)
                        * active.base_multishot
            };
        let rolled = ms_eff.floor() as u32 + d.spine.chance(ms_eff.fract()) as u32;
        // DOUBLE TAP, computed ONCE for the whole pull and applied to every
        // pellet of it. VERBATIM: "the bonus is applied on hit to all pellets as
        // damage * 20% * (hits - 1)", worked through on the card as "with a
        // modded multishot of 3, the first trigger pull would do +40% bonus
        // damage, the second +100%, the third +160%" — so the count INCLUDES
        // this pull's own hits, and one is taken off, which is why an unmodded
        // weapon gets nothing from its first shot.
        //
        // Every pellet reaching the one target IS a hit here ("additional hits
        // caused by punch through or Multishot will allow each bullet to
        // trigger multiple stacks"), so the pull's hits are its pellets.
        // SYNTH CHARGE's window, read off the SAME gate Final Fusillade uses —
        // so a burst weapon's "last round" is its last BURST, and a cycle's
        // window is whichever magazine is actually being fired.
        let sc_mult = if last_round { 1.0 + active.last_round_damage } else { 1.0 };
        // THE CHAMBERS' multiplier, on the magazine's FIRST round only. The two
        // cards are already summed into one number by `resolve` — "stacks
        // additively … for up to 140% bonus damage" — so this is one factor
        // beside Synth Charge's rather than a second bracket.
        let cc_mult = if first_round { 1.0 + active.first_round_damage } else { 1.0 };
        let dt_mult = match active.consecutive_hit_damage {
            Some((per_stack, max_stacks, duration)) => {
                if t >= double_tap.expiry {
                    double_tap.hits = 0;
                }
                // AN EXPLODING PROJECTILE IS TWO HITS where the weapon says so
                // — its collision and its explosion, +40% a projectile at rank
                // 3 (M102). Only the aimed landing counts; a bounce adds none.
                let per_projectile = if active.consecutive_hit_radial_only && active.radial.is_some() { 2 } else { 1 };
                let hits = double_tap.hits + rolled * per_projectile;
                double_tap.hits = hits;
                double_tap.expiry = t + duration;
                1.0 + per_stack * f64::from(hits.saturating_sub(1).min(max_stacks))
            }
            None => 1.0,
        };
        // CONTINUOUS weapons MERGE. VERBATIM (wiki Multishot §Continuous
        // Weapons): "additional beams that hit the same target instead merge
        // into a singular damage tick. This combined tick has damage AND Status
        // Chance equal to the SUM of the individual beams, but the Critical
        // Chance is still equal to that of a single beam."
        //
        // The multiplier is the ROLLED count, not the fractional average, so
        // damaging statuses are "affected TWICE by multishot" while forced
        // procs are "applied after the damage instances are merged", one per
        // tick.
        //
        // PLENTIFUL MAYHEM, continuous branch: "In the Incarnon form, instead
        // of increasing the damage of additional projectiles created by
        // multishot, all multishot bonuses are increased by 60%." A merged beam
        // has no separable generated projectile, so the perk scales the
        // multishot BONUS — and the two readings agree in expectation:
        //   base form   1 + (1+v)(M-1)     [1 original + (M-1) generated]
        //   Incarnon    1 + (1+v)(M-1)     [merged, so damage ∝ multishot]
        // The identity needs base multishot = 1; both Torid forms are.
        let merge_bonus = if active.multishot_ammo_bonus > 0.0 {
            let base_ms = active.base_multishot.max(1.0);
            base_ms + (rolled.max(1) as f64 - base_ms) * (1.0 + active.multishot_ammo_bonus)
        } else {
            rolled.max(1) as f64
        };
        // MULTISHOT THAT IS NOT MORE PROJECTILES. VERBATIM (wiki `Arbucep`):
        // "Multishot increases weapon damage instead of creating additional
        // projectiles. Damage bonus is multiplicative to other sources of
        // damage." So the COUNT stays the weapon's own — which is what keeps
        // its six elements six — and the bucket becomes a factor instead.
        //
        // The factor is the rolled count over the weapon's own, so an unmodded
        // weapon pays 1.0 and every multishot source scales it from there. It
        // is applied as its own multiplier and never joins a bucket, which is
        // what "multiplicative to other sources of damage" says.
        let own_pellets = active.base_multishot.max(1.0);
        let ms_damage = if active.multishot_adds_damage {
            (ms_eff / own_pellets).max(1.0)
        } else {
            1.0
        };
        let (n_pellets, beam_merge) = if active.continuous {
            (1, merge_bonus)
        } else if active.multishot_adds_damage {
            (own_pellets.round() as u32, 1.0)
        } else {
            (rolled, 1.0)
        };
        // Ammo, settled now that the roll is known.
        //
        // The ROUND itself always comes from the magazine and always takes ammo
        // efficiency — that path is unchanged by any perk.
        //
        // PLENTIFUL MAYHEM bills the EXTRA projectiles on top, one round
        // each, and the draw follows the RAW rolled count rather than the
        // 60%-scaled one: the bonus is paid in damage, not billed twice. Ammo
        // efficiency does NOT reach the surcharge (measured), so the magazine
        // round keeps its discount, every generated projectile pays full price,
        // and a 100% efficiency source does not make multishot free.
        //
        // AMMO STARVATION IS REAL, and is why this is a loop rather than one
        // subtraction: the projectiles are produced in order, each paying as it
        // goes, and one that cannot pay IS NOT FIRED. A 4-multishot pull
        // against 3 charges fires three pellets and lands on empty.
        // `ammo_cost` scales the whole spend: efficiency is a DISCOUNT on the
        // cost, not a separate round. A beam paying 0.5 with 20% efficiency
        // spends 0.4, which is what "0.5 ammo per trace" plus an efficiency
        // mod has to mean.
        let spend = active.ammo_cost * (1.0 - efficiency);
        if incarnon.in_base_form {
            incarnon.base_magazine -= spend;
        } else {
            ammo.loaded -= spend;
        }
        ammo.rounds_this_mag += 1;
        // THE TRIGGER PULL ITSELF — the row every pellet, every explosion and
        // every status this shot goes on to cause points back at
        // (`record::Event::cause`). It is also where the weapon's state is
        // stamped, so a row four seconds later still says which form fired it
        // and what was left in the magazine at the time.
        if rec.is_on() {
            record_weapon(params, rec, ammo, incarnon);
            // …AND WHAT THE SHOOTER HAS UP. Sampled at the SHOT, which is the
            // one place in this loop that holds every local the sampler reads —
            // the same reason `weapon_now!` is a macro. A row between two shots
            // carries the count as of the shot before it, which is exact for
            // everything a shot changes and up to one shot stale for a buff
            // that expires on a clock of its own.
            let stacks = sample_stacks(
                params, rec_roster, t, arc, gal, buff_stacks,
                &windows.crit_on_headshot_stacks, windows.crit_on_headshot, windows.weakpoint_buff,
                windows.fire_rate_after_reload,
                windows.base_damage_after_reload, windows.base_damage_eximus,
                windows.streak, tendril.count, crit_per_hit.stacks, bar,
                combo_at(combo_spec, params.combo_held, sniper_combo.count, sniper_combo.last_hit, t),
                incarnon.incarnon_until,
                influence_until,
            );
            rec.set_stacks(stacks);
            rec.begin_shot(t, n_pellets);
        }
        // BLAZING BARREL: the round is SPENT, so it was fired. Here and not in
        // the pellet loop — one shot is one stack however many pellets it threw
        // — and after `ms_eff` was rolled, so the shot that earns the stack does
        // not carry it.
        bump_buffs!(params, buff_stacks, rec_buff_index, rec, crate::model::BuffTrigger::Firing, t, d.spine);
        // READY RETALIATION IS ARMED THE MOMENT THE MAGAZINE RUNS OUT, which is
        // HERE — the shot that spends the last round — and not at the reload
        // that follows. The two are the same instant for a reload and are not
        // the same instant for a TRANSFORM: the shot that fills the gauge can
        // also be the shot that empties the magazine, and the transform is
        // decided before any reload is. Arming at the reload would have left
        // that transform at the plain speed, which is the case the owner used
        // to state the rule.
        if !can_fire(if incarnon.in_base_form { incarnon.base_magazine } else { ammo.loaded }, active.ammo_cost) {
            *rs_armed = true;
        }
    Resolved {
        flat_crit,
        crit_chance_relative,
        effective_cc,
        undamaged,
        weakpoint_cd,
        weakpoint_sc,
        sc_arc_shot,
        live_rate,
        bd_reload_add,
        bd_eximus_add,
        rolled,
        sc_mult,
        cc_mult,
        dt_mult,
        ms_damage,
        n_pellets,
        beam_merge,
    }
}
