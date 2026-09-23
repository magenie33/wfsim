// SPDX-License-Identifier: AGPL-3.0-or-later
//! ONE PELLET, from the barrel to what it left behind — the damage pipeline of
//! `docs/MECHANICS.md` for a single projectile: where it lands, what it rolls,
//! what it settles into the target's pools, what it spreads to the bodies
//! around it, and what it writes down.
//!
//! The shot resolved its numbers once and every pellet reads the same ones
//! (`Strike`); what a pellet CHANGES is the run's live state (`Live`). The two
//! are destructured at the top, so the pipeline below names each piece the way
//! the rest of the fight does.

use super::*;
use super::bumps::{bump_buffs, bump_status_buffs};

/// WHAT THIS PELLET IS, as the shot resolved it.
/// EVERYTHING ONE ATTACK NEEDS, composed once and resolved through
/// [`settle_pellet`].
///
/// NOT `Shot`: a melee swing, a lingering field's tick and an ability are none
/// of them a shot, and this is the one path all of them resolve through. A
/// strike is what a combatant does.
pub(super) struct Strike<'a> {
    pub(super) active: &'a FightParams,
    pub(super) qvec: &'a DamageVector,
    pub(super) direct_pre_snap: DamageVector,
    pub(super) variants: &'a [(DamageVector, f64, f64)],
    pub(super) variant_rad: &'a [DamageVector],
    pub(super) modded_base: f64,
    pub(super) co_base: crate::model::CoBase,
    pub(super) combo_multiplier: f64,
    pub(super) combo_spec: Option<crate::model::SniperCombo>,
    pub(super) swing: &'a Option<crate::model::ComboHit>,
    pub(super) swing_mult: f64,
    pub(super) swing_forced_types: &'a [DamageType],
    pub(super) swing_forced_independent: &'a [&'static str],
    pub(super) tennokai: bool,
    pub(super) tennokai_heavy: bool,
    pub(super) melee_struck: &'a [usize],
    pub(super) struck: &'a [usize],
    pub(super) body_at: &'a [crate::rules::space::Vec2],
    pub(super) bounce_bodies: &'a [crate::rules::space::Vec2],
    pub(super) chain_layout: &'a Option<crate::rules::chain::Layout>,
    pub(super) ricochet_layout: &'a Option<crate::rules::chain::Layout>,
    pub(super) aim_off_axis: f64,
    pub(super) effective_cc: f64,
    pub(super) crit_chance_relative: f64,
    pub(super) flat_crit: f64,
    pub(super) cc_mult: f64,
    pub(super) sc_mult: f64,
    pub(super) sc_arc_shot: f64,
    pub(super) weakpoint_cd: f64,
    pub(super) weakpoint_sc: f64,
    pub(super) bd_reload_add: f64,
    pub(super) bd_eximus_add: f64,
    pub(super) dt_mult: f64,
    pub(super) ms_damage: f64,
    pub(super) status_damage: f64,
    pub(super) live_rate: f64,
    pub(super) beam_ramp: f64,
    pub(super) undamaged: bool,
    pub(super) t: f64,
    pub(super) bar: &'a BuffBar,
    pub(super) rec_roster: &'a [BuffSeries],
    pub(super) rec_buff_index: &'a [Option<usize>],
    pub(super) params: &'a FightParams,
}

/// WHAT IT CHANGES — the run, as it stands at this pellet.
pub(super) struct Live<'a> {
    pub(super) r: &'a mut RunResult,
    pub(super) rec: &'a mut crate::record::Record,
    pub(super) d: &'a mut crate::rules::rng::Draws,
    pub(super) target: &'a mut TargetState,
    pub(super) others: &'a mut Vec<SpreadFoe>,
    pub(super) debuffs: &'a mut DebuffState,
    pub(super) gal: &'a mut GalStacks,
    pub(super) arc: &'a mut ArcRuntime,
    pub(super) buff_stacks: &'a mut Vec<LiveStacks>,
    pub(super) windows: &'a mut CardWindows,
    pub(super) ammo: &'a mut Ammo,
    pub(super) incarnon: &'a mut IncarnonState,
    pub(super) meter: &'a mut Meter,
    pub(super) tendril: &'a mut Tendrils,
    pub(super) crit_per_hit: &'a mut CritPerHit,
    pub(super) sniper_combo: &'a mut SniperComboCount,
    pub(super) weakpoint_pile: &'a mut LiveStacks,
    pub(super) strike_spread: &'a mut Option<SpreadStrike>,
    pub(super) fields: &'a mut Vec<FieldState>,
    pub(super) orbs: &'a mut Vec<OrbState>,
    pub(super) any_big: &'a mut bool,
    pub(super) any_head: &'a mut bool,
    pub(super) beam_merge: &'a mut f64,
    pub(super) encumber_done: &'a mut bool,
    pub(super) field_duration_boost: &'a mut bool,
    pub(super) influence_until: &'a mut f64,
    pub(super) landed_this_shot: &'a mut bool,
    pub(super) super_crit_armed: &'a mut bool,
}

/// WHAT A WEAK-POINT KILL GRANTS — one site, because "headshot kill" has to
/// mean ONE thing across the engine: the DIRECT hit entered a weak point and
/// the DIRECT hit finished that body. A bleed that kills afterwards, an
/// explosion that kills beside it and a bounce's assumed head are all kills,
/// and none of them is this.
///
/// Called once per body the round killed that way — the aimed one, and each
/// one behind it the same round punched through.
fn weakpoint_kill(
    params: &FightParams,
    arc: &mut ArcRuntime,
    windows: &mut CardWindows,
    t: f64,
) {
    // Deadhead's precision boundary.
    arc.bump_trigger(&params.arcane.buffs, ArcTrigger::HeadshotKill, t);
    // Galvanized Scope / Crosshairs: EACH STACK KEEPS ITS OWN 12 s CLOCK,
    // which is what `push_capped`'s list of expiries is — the mod's second
    // half is the one Galvanized family member that does not decay together.
    if let Some(s) = &params.crit_chance_stack {
        crate::fight::debuffs::DebuffState::push_capped(
            &mut windows.crit_on_headshot_stacks,
            t + s.duration,
            s.max_stacks as usize,
            t,
        );
    }
}

/// Fire one pellet of this shot.
///
/// ALWAYS INLINED, and measured: the loop fires this per pellet, and left to
/// the compiler it costs 13% a shot on `one_fight` (gotva_prime, braton_prime)
/// — with plain `#[inline]` too. Inlined always, what is left is 1-2%, which
/// is what moving the pipeline into its own file costs.
#[inline(always)]
pub(super) fn settle_pellet(pellet_idx: u32, shot: &Strike, live: &mut Live) {
    let Strike {
        active,
        qvec,
        direct_pre_snap,
        variants,
        variant_rad,
        modded_base,
        co_base,
        combo_multiplier,
        combo_spec,
        swing,
        swing_mult,
        swing_forced_types,
        swing_forced_independent,
        tennokai,
        tennokai_heavy,
        melee_struck,
        struck,
        body_at,
        bounce_bodies,
        chain_layout,
        ricochet_layout,
        aim_off_axis,
        effective_cc,
        crit_chance_relative,
        flat_crit,
        cc_mult,
        sc_mult,
        sc_arc_shot,
        weakpoint_cd,
        weakpoint_sc,
        bd_reload_add,
        bd_eximus_add,
        dt_mult,
        ms_damage,
        status_damage,
        live_rate,
        beam_ramp,
        undamaged,
        t,
        bar,
        rec_roster,
        rec_buff_index,
        params,
    } = *shot;
    let r = &mut *live.r;
    let rec = &mut *live.rec;
    let d = &mut *live.d;
    let target = &mut *live.target;
    let others = &mut *live.others;
    let debuffs = &mut *live.debuffs;
    let gal = &mut *live.gal;
    let arc = &mut *live.arc;
    let buff_stacks = &mut *live.buff_stacks;
    let windows = &mut *live.windows;
    let ammo = &mut *live.ammo;
    let incarnon = &mut *live.incarnon;
    let meter = &mut *live.meter;
    let tendril = &mut *live.tendril;
    let crit_per_hit = &mut *live.crit_per_hit;
    let sniper_combo = &mut *live.sniper_combo;
    let weakpoint_pile = &mut *live.weakpoint_pile;
    let strike_spread = &mut *live.strike_spread;
    let fields = &mut *live.fields;
    let _orbs = &mut *live.orbs;
    let any_big = &mut *live.any_big;
    let any_head = &mut *live.any_head;
    let beam_merge = &mut *live.beam_merge;
    let encumber_done = &mut *live.encumber_done;
    let field_duration_boost = &mut *live.field_duration_boost;
    let influence_until = &mut *live.influence_until;
    let landed_this_shot = &mut *live.landed_this_shot;
    let super_crit_armed = &mut *live.super_crit_armed;
    // PLENTIFUL MAYHEM, discrete branch: "Damage bonus from multishot
    // consuming ammo only applies to projectiles GENERATED BY
    // multishot" — pellet 0 is the weapon's own projectile and never
    // takes it. An INDEPENDENT multiplier ("multiplicative to base
    // damage bonuses like Serration"), so it multiplies the finished
    // instance rather than joining a bucket. With no multishot source
    // there is no pellet 1 and the perk is worth exactly nothing.
    let pm_mult = if pellet_idx > 0 {
        1.0 + active.multishot_ammo_bonus
    } else {
        1.0
    };
    // EXPIRE WHAT HAS RUN OUT, before anything reads a stack count.
    //
    // ITS OWN CALL, not a side effect of the mitigation snapshot:
    // that snapshot lives in the stage loop, and the per-pellet reads
    // below — Cold's flat crit damage received and Condition Overload's
    // type count — would go with it and count a stack that had already
    // expired. `mitigation` still
    // prunes and pruning is idempotent, so this is the one call that
    // decides WHEN, rather than a second copy of the rule.
    debuffs.prune(t, status_damage);
    // Live target-side state for THIS pellet (earlier pellets' procs
    // already count): Cold's flat crit damage received, and Condition
    // Overload's type count. The MITIGATION amps are read one level
    // further in — see the stage loop.
    // Crit damage: resolved multiplier + Cold's flat bonus received
    // + Sharpened Bullets' live on-kill buff + the arcane's
    // assumed-max conditional (Outburst).
    // Primary Blight / Frostbite: a stacking crit-damage grant,
    // already resolved to an ABSOLUTE per-stack value against the
    // weapon's base crit damage (ArcaneDef::fx), so it adds straight
    // into the same total as Cold's flat bonus.
    // Same split as crit chance above: `crit_damage_relative` is a bucket bonus every
    // stage scales by its OWN base crit damage; `cd_abs` is a flat add
    // every stage takes as-is.
    let crit_damage_relative = arc.total(&params.arcane.buffs, ArcGrant::CritDamage, t)
        + arc.cd_bonus(active, t)
        + buff_total(active, crate::model::BuffGrant::CritDamage, &mut *buff_stacks, t)
        // DREAMER'S WRATH: `+32% critical damage for Tennokai attacks`
        // — on the one swing the window bought and no other.
        + if tennokai { active.tennokai.crit_damage } else { 0.0 }
        + params.arcane.crit_damage_relative;
    // SPITEFUL DEFILEMENT rides the same after-mods FLAT bucket Cold's
    // received bonus does — "Bonus is added after mods as a flat value"
    // (wiki, supplied measured 2026-08-10), so it is added to the
    // finished multiplier rather than scaling the weapon's base.
    //
    // The counter is DISTINCT TYPES, which is what the card's own
    // example insists on ("having 5 corrosive and 5 radiation status
    // effects on a target will not disable this buff") — and it is
    // Condition Overload's counter, read here rather than recomputed,
    // so the anti-CO perk and CO can never disagree about the number
    // they are both reading.
    let spiteful = match params.crit_damage_below_status_count {
        Some((threshold, bonus))
            if (debuffs.distinct_statuses() as u32) < threshold =>
        {
            bonus
        }
        _ => 0.0,
    };
    let cd_abs = debuffs.cold_cd_bonus(t) + spiteful + weakpoint_cd;
    // PRELUDE OF MIGHT is the one perk whose condition is read at the
    // MOMENT OF THE HIT rather than off the arsenal: "With Critical
    // Chance below 40%", plus the wiki's note on the same row —
    // "Condition is affected by the critical chance increase effect of
    // Puncture status". So the value tested is `effective_cc`, the very
    // number the crit roll is about to use, and every live source is in
    // it: Weakened, a flat grant (Arcane Avenger), a relative one
    // (Crosshairs, an arcane's stacks). Puncture is only the one the
    // wiki names, being the sole source that sits on the TARGET and so
    // can raise your crit chance without your panel ever moving.
    //
    // Read PER SHOT, not per pellet: `effective_cc` is the weapon's
    // crit chance, while a pellet's may go higher on a weak point
    // (Pistol Acuity) or be REPLACED outright (Gotva Prime's set
    // chance). Those are properties of where a projectile landed, not
    // of the weapon the condition asks about.
    //
    // Nothing to take back when the perk is absent, and nothing to take
    // back when the panel already failed the condition — `resolve` then
    // never granted it and leaves this `None`.
    let prelude_lost = match active.crit_multiplier_below_crit_chance {
        Some((granted, below)) if effective_cc >= below => granted,
        _ => 0.0,
    };
    let cd_total = active.crit_multiplier - prelude_lost
        + active.unmodded_crit_damage * crit_damage_relative
        + cd_abs
        // Mauler's Magazine, earned inside the fight — a BASE grant,
        // already multiplied by the crit-damage mods at `resolve`, the
        // same conversion `FlatBaseDamage` takes one bracket over.
        + buff_total(active, crate::model::BuffGrant::BaseCritDamage, &mut *buff_stacks, t)
        // …and the other half of the same condition, decided by the
        // same shot: an absolute add, already multiplied by the
        // crit-damage mods at `resolve`.
        + if undamaged { active.crit_damage_on_undamaged } else { 0.0 };
    // Live BASE-DAMAGE bucket additions, evaluated per instance:
    //  - arcane stacks (Merciless/Deadhead/Dexterity/Cascadia Flare)
    //  - Overwhelming Attrition's earned stacks — VERBATIM (wiki
    //    Laetum): "Damage bonus is ADDITIVE to base damage bonuses
    //    such as Hornet Strike", the opposite of its tier sibling
    //    Devouring Attrition, which the same page calls
    //    multiplicative. Both therefore also scale ModifiedBase, so
    //    status payloads follow, exactly like Hornet Strike.
    // The stacks read here are the ones EARNED so far; the hit that
    // grants a stack does not benefit from it (the bump happens
    // after the status roll below).
    let arcane_base_damage = arc.total(&params.arcane.buffs, ArcGrant::BaseDamage, t)
        + bd_reload_add
        + bd_eximus_add
        // Primary Compression's `adds` row: the same bracket a live
        // base-damage buff joins, so Serration dilutes it exactly as
        // the wiki's "additive with damage bonuses" says it should.
        + active.compression_base_damage
        + buff_total(active, crate::model::BuffGrant::BaseDamage, &mut *buff_stacks, t)
        // Striking Succession, already converted by `resolve` into the
        // share of this bucket its flat number is worth — so it lands
        // here and NOT diluted, which is the whole point of the
        // conversion.
        + buff_total(active, crate::model::BuffGrant::FlatBaseDamage, &mut *buff_stacks, t)
        // …AND KILLING BLOW, which is a term in this bucket and not a
        // multiplier — see `heavy_attack_base_damage`.
        + heavy_attack_base_damage(active)
        // …AND RAGE: "additive with mods like Pressure Point".
        + arc.rage_bonus(t);
    // FEIGNED RETREAT / SWIFT CONCLUSION: a condition on the TARGET,
    // evaluated per instance because the target's health is falling
    // while the shot is being resolved.
    //
    // HEALTH, not the pools in front of it: a target still on its
    // shields or overguard is not below half HEALTH, and reading the
    // total would have turned this on at the start of every fight
    // against an Eximus.
    //
    // WHERE IT LANDS IS THE WEAPON'S CO BRACKET, and the Kunai's page
    // is what says so: "additive with Hornet Strike in basic Kunai
    // form, and multiplicative in Incarnon form. It is also additive
    // with Galvanized Strike in BOTH forms." Galvanized Strike IS the CO
    // bonus — so the rule is not two rules, it is one: this bonus goes
    // wherever CO goes. `gunco_bucket` routes it.
    let half_hp = if params.foe.max_health() > 0.0
        && target.health < 0.5 * params.foe.max_health()
    {
        active.base_damage_below_half_health
    } else {
        0.0
    };
    let base_damage = active.base_damage_bonus;
    let arc_ratio = (1.0 + base_damage + arcane_base_damage) / (1.0 + base_damage);
    let mb_live = modded_base * arc_ratio;
    // GunCO family — ONE machinery (wiki CO catalog; user
    // 2026-07-27): every source contributes rate × TARGET-COUNTER
    // into the same bracket, is scaled by the original-base
    // fraction (evolution flat damage excluded), and combines per
    // the weapon's CoBehavior, direct hits only. Sources differ
    // ONLY in their counter:
    //   Condition Overload (Galvanized Strike + innate, one merged
    //     rate since they share it) → distinct status TYPES;
    //   Secondary Shiver → live Cold STACKS (Frozen counts as 10).
    let co_mult = gunco_bucket(
        params, active, &mut *debuffs, &mut *gal, t, base_damage, arcane_base_damage, arc_ratio,
        half_hp,
        co_base,
        crate::model::CoStage::Direct,
    );
    // The explosion's own, and only when it differs — an evolution
    // that raises the radial's damage without raising its CO base
    // (the Burston's +42) makes these two numbers diverge. Computed
    // beside the direct hit's so both read the SAME counters at the
    // same instant.
    // ...off the ACTIVE form. `active`, not `params`: in a cycle the two
    // differ for the whole base phase, and this line reading the outer
    // params gave a base-form shot the Incarnon's explosion (M32).
    let co_mult_radial = match &active.radial {
        // AN EXPLOSION carries no half-health term either — a
        // DIRECT-hit bonus, like the CO it rides beside.
        Some(r) if r.takes_condition_overload => gunco_bucket(
            params, active, &mut *debuffs, &mut *gal, t, base_damage, arcane_base_damage, arc_ratio, 0.0,
            r.co_base, crate::model::CoStage::Radial,
        ),
        _ => Gunco { bucket: arc_ratio, ..Default::default() },
    };

    // Part FIRST, crit roll second: weak-point crit chance (Pistol
    // Acuity; Cascadia Accuracy under assumed-max) exists only on
    // the pellet that actually lands on a weak point.
    //
    // The landing spot is rolled PER PELLET, not per trigger pull: aiming at the head does not put every
    // pellet of a spread on it, so `headshot_pct` is a per-pellet
    // aim weight. Consequences that follow from this and are
    // deliberate: the Incarnon gauge charges per headshot PELLET
    // (multishot fills it faster), on-headshot buffs trigger from
    // any one pellet, and the reported headshot rate is
    // pellets/pellets. Do NOT "fix" this into a per-pull roll.
    // WHERE THIS PELLET LANDED. An aimed shot draws against the
    // scenario's aim weights; an UNAIMED attack draws against its own
    // flat chance, because `headshot_pct` describes the player and the
    // player is not pointing this one. Same helper the field's ticks
    // use, so the six strikes of one orb cannot answer differently.
    let part = match active.unaimed_headshot_chance {
        Some(c) => unaimed_part(&params.body_parts, c, &mut d.spine),
        None => pick_part(&params.body_parts, &mut d.spine),
    };
    let cc_pellet = effective_cc
        + if part.is_head {
            // Weak-point-only crit chance is relative too, and it is
            // DIRECT-only, so the direct part's base is the right one.
            active.unmodded_crit_chance
                * (active.weakpoint_crit_chance_relative + params.arcane.weakpoint_crit_chance_relative)
        } else {
            0.0
        };
    // KING'S GAMBIT, the other half of the same bullet: "x0 Critical
    // Chance on Bodyshots". MULTIPLICATIVE, and the card's own note is
    // why it is applied here rather than folded into a bucket —
    // "Bodyshot modifier is multiplicative with all sources of Critical
    // Chance, effectively making non-headshot critical hits impossible".
    // A bucket term could be cancelled by enough crit chance; a x0 here
    // cannot, which is the whole perk.
    let cc_pellet =
        if part.is_head { cc_pellet } else { cc_pellet * active.bodyshot_crit_chance_multiplier };
    // GOTVA PRIME: an armed pellet's crit chance is SET, replacing the
    // modded value and the weak-point bonus alike — "Set Critical
    // Chance ignores all other modifiers, whether from mods or Warframe
    // abilities". The tier UPGRADE still runs on the result, which is
    // how Vigilante reaches a Tier-4 hit off it, so the lock binds the
    // chance and not the ceiling.
    let cc_pellet = match params.super_crit_on_status {
        Some(sc) if *super_crit_armed => {
            *super_crit_armed = false;
            sc.crit_chance
        }
        _ => cc_pellet,
    };
    let tier =
        upgrade_crit_tier(roll_crit_tier(cc_pellet, &mut d.spine), active.crit_tier_upgrade_chance, &mut d.spine);
    // Headshot bonuses form an additive bracket that MULTIPLIES
    // the base multiplier (Enemy_Body_Parts, verbatim template:
    // 3 × (1 + Deadhead 30% + Target Acquired 75%) = 6.15x). A 1x
    // head still benefits (1 × 1.3). Acuity's Weak Point Damage is
    // ADDED to the part multiplier first (at 1.5× the listed value
    // on true weak points — wiki Pistol_Acuity: 3 + 3.5×1.5 =
    // 8.25x) and the bracket multiplies the sum. Rides the part
    // context into DoT snapshots.
    // The additive bracket, and the ONE weapon whose innate share is
    // not in it: "Cernos Prime's headshot bonus is unique and stacks
    // MULTIPLICATIVELY with Primary Deadhead's headshot bonus" — a
    // per-weapon anomaly carried on the weapon. On a 3x head with
    // Deadhead that is 3 x 1.3 x 1.5 = 5.85x against 5.4x additive.
    // LINGERING JUDGEMENT and SEQUENTIAL SKULLBUSTER are ADDITIVE:
    // "Headshot damage bonus stacks additively with Primary Deadhead's
    // headshot damage bonus", so they sum with the arcane's before the
    // bracket is spent — which is why a Deadhead build gets much less
    // than +50% out of one. They sit with the ARCANE's term because
    // that is the group the card names; no weapon carries both a
    // multiplicative innate and one of these.
    let streak_bonus = match params.headshot_streak {
        Some(s) if t < windows.streak => s.value,
        _ => 0.0,
    } + buff_total(active, crate::model::BuffGrant::HeadshotDamage, &mut *buff_stacks, t);
    // WHAT A HEAD WOULD BE WORTH, computed whether or not THIS pellet
    // found one: a RICOCHET rolls its own head, on another body, later
    // in the same shot, and it is worth exactly what a head is worth
    // here. Split out rather than duplicated so the two can never say
    // different things.
    let (hb_head, hi_head) = if active.headshot_bonus_multiplicative {
        (params.arcane.headshot_multiplier_bonus + streak_bonus, active.headshot_damage_bonus)
    } else {
        (
            params.arcane.headshot_multiplier_bonus + streak_bonus + active.headshot_damage_bonus,
            0.0,
        )
    };
    let (head_bonus, head_innate) =
        if part.is_head { (hb_head, hi_head) } else { (0.0, 0.0) };
    // …and the same arithmetic `part_factor` does below, for a head on
    // SOME OTHER body — whose own parts decide the multiplier, because a
    // formation may hold more than one kind of enemy.
    // PER PELLET, because each pellet is its own projectile with its own
    // flight. Filled on the first attack part that needs it and read by
    // the second, so the collision and the explosion are one flight.
    let mut ric_path: Option<Vec<(usize, bool)>> = None;
    let head_factor = |fs: &crate::formation::FoeSpec| -> f64 {
        let m = fs
            .body_parts
            .iter()
            .find(|p| p.is_head)
            .map_or(1.0, |p| p.multiplier);
        // The WEAPON may overrule what a head is worth — Tenet Arca
        // Plasmor, "1x headshot multiplier". Its own value REPLACES the
        // part's, and the additive brackets still pay on top of it.
        let m = active.headshot_multiplier.unwrap_or(m);
        (m + 1.5 * active.weakpoint_damage) * (1.0 + hb_head) * (1.0 + hi_head)
    };
    let head_mult = active.headshot_multiplier.unwrap_or(part.multiplier);
    let wp_mult = if part.is_head {
        head_mult + 1.5 * active.weakpoint_damage
    } else {
        part.multiplier
    };
    let part_factor = wp_mult * (1.0 + head_bonus) * (1.0 + head_innate);
    // …AND WHAT AN ELECTRICITY OR GAS TICK IS WORTH WHERE IT LANDS: the
    // same brackets over a 1x base, acuity left out (`lands_on_a_part`).
    let head_landing = (1.0 + hb_head) * (1.0 + hi_head);
    let landing = (1.0 + head_bonus) * (1.0 + head_innate);
    // Wiki Critical_Hit §Critical Headshots: a crit on an eligible
    // >1x location doubles cd inside the tier formula (a cd_total
    // that INCLUDES Cold's flat bonus — freeze.yaml notes).
    let cd = if part.crit_bonus && head_mult > 1.0 && part.multiplier > 1.0 {
        2.0 * cd_total
    } else {
        cd_total
    };
    let crit_multiplier = 1.0 + tier as f64 * (cd - 1.0);

    // Faction bonus (System A) is a total-damage multiplier applied
    // once per instance; DoT/status ticks apply it a SECOND time
    // (fm² below) — the wiki "double dip".
    // Secondary Surge (assumed-max): a FINAL multiplier on the shot,
    // multiplicative with Hornet Strike (wiki notes). Secondary
    // Fortifier: ×overguard_multiplier while the target's Overguard holds.
    // Primary Compression's `multiplies` row rides the same slot, and
    // it is `active`'s rather than `params`' — the form being fired owns
    // it, because one arcane is worth +240% in the Torid's base form
    // and nothing in its Incarnon.
    // HOISTED so `apply` is handed the SAME number that went in, and
    // can take it back off the share that carries past a depleted
    // Overguard rather than re-deriving it from a pool it has already
    // spent.
    let og_mult = if target.overguard > 0.0 {
        params.arcane.overguard_multiplier
    } else {
        1.0
    };
    let arc_final =
        params.arcane.final_multiplier * active.compression_multiplier * og_mult;

    // ---- ATTACK PARTS (MECHANICS §7) -------------------------
    // A projectile carries TWO instances where the weapon declares a
    // radial, resolved SEPARATELY because they ARE separate damage —
    // wiki (Laetum): "Initial hit and explosion apply status
    // separately". The per-stage bindings SHADOW the direct-hit names,
    // so the proc block below serves whichever is in flight. The radial
    // fires ONCE PER PULL where the weapon says so, from the ACTIVE
    // form (M32).
    // ---- WHERE THIS PELLET WENT (MECHANICS §11) --------------
    // Spread is an ANGLE, so what it costs is a function of range.
    // Drawn uniform over [0, 2 x spread], whose mean is the stat's own
    // definition; the direction is not drawn, because the target is a
    // circle. NOT DRAWN AT ALL unless a miss is possible, which is what
    // leaves `d.aim` untouched in every fight with no range in it.
    let range = params.range_to_centre();
    let gap_m = params.gap();
    // HOW FAR THE TARGET ALREADY IS FROM THE AIM LINE, before a degree
    // of spread is added. Zero whenever the weapon points at it, which
    // is every fight this engine ran before aim became a place you
    // choose — and zero is what makes the whole clause below collapse
    // to what it was.
    let off_axis = aim_off_axis;
    // THE DEVIATION AND ITS DIRECTION ARE KEPT, not collapsed into
    // one scalar. How far the pellet passed the aimed body is the only
    // thing ONE body can be asked; a crowd asks WHERE, so both survive
    // to build the epicentre below.
    let (dev, phi) = match active.spread {
        Some(s) if !s.is_pinpoint() && range > 0.0 => {
            let dev = s.draw(d.aim.next_f64());
            // WHICH WAY IT WENT, drawn whenever the weapon points away
            // from the body OR there is a crowd.
            // Against one body only the magnitude decides anything;
            // with a crowd the side decides who is in the blast. Off
            // `blast_dir`, a stream of its own, so adding the draw
            // shifts no other roll — see `rules::rng::Draws`. No board ruler
            // sets an aim point, so nothing that was drawn from `aim`
            // before is drawn from it now either.
            let phi = if off_axis > 0.0 || !params.others.is_empty() {
                d.blast_dir.next_f64()
            } else {
                0.0
            };
            (dev, phi)
        }
        // A PINPOINT WEAPON POINTED ELSEWHERE still misses: no spread
        // does not mean no aim.
        _ => (0.0, 0.0),
    };
    let aim_offset = match active.spread {
        Some(s) if !s.is_pinpoint() && range > 0.0 => {
            crate::rules::space::miss_distance_off_axis(range, off_axis, dev, phi)
        }
        // A PINPOINT WEAPON POINTED ELSEWHERE still misses: no spread
        // does not mean no aim.
        _ => crate::rules::space::miss_distance(range, off_axis),
    };
    // …AND A BODY IS A CIRCLE OF ONE RADIUS: hitting the circle is a
    // hit, so this is ray-versus-circle and nothing more
    // (`rules::space::miss_distance`). It asks only whether the pellet reached
    // the target; where on it a landed pellet went is `headshot_pct`'s
    // question, already pinned per pellet, and folding a silhouette's
    // height in here would ask that twice. `rules::space::BODY_RADIUS_M` is
    // the model's one free parameter.
    //
    // AT CONTACT THIS IS ALWAYS TRUE at any cone width, because the
    // muzzle is then one radius from the target's centre — a property
    // of the geometry rather than a special case in it.
    // …AND IT HAS TO REACH. A weapon's RANGE is a wall, not a ramp:
    // past it the shot does not exist and a target beyond takes
    // literally zero. The Phantasma's page lists *"Limited range of 20
    // meters"* and *"No Damage Falloff"* in one breath, which are two
    // separate facts.
    //
    // MEASURED TO THE SURFACE, like every distance a reader is shown:
    // `gap` is what the shot flies and what the arena prints, so "20 m"
    // is the number on the scene. `INFINITY` where none is declared.
    let in_range = gap_m <= active.range_m;
    let pellet_lands = aim_offset <= crate::rules::space::BODY_RADIUS_M && in_range;
    // WHERE THE ROUND WENT OFF — ONE EPICENTRE FOR THE WHOLE
    // EXPLOSION.
    //
    // NOT TWO. With the aimed body reading its falloff from the miss
    // distance and every OTHER body reading its distance from the aimed
    // body's SURFACE, a shot nine metres wide drops the aimed body's
    // damage by 61% and leaves the body two metres behind it taking a
    // direct hit's blast. Measured on the
    // wire before this landed: 7115 with the shot on target, 7120 with
    // it nine metres off.
    //
    // A ROUND THAT HIT DETONATES ON THE SURFACE it touched; one that
    // missed detonates where it actually passed. The two agree at the
    // boundary, and `detonation_of_miss` is built so that its distance
    // back to the aimed body IS `aim_offset` — so this cannot move the
    // aimed body's number, which is asserted in `space`.
    let det = if pellet_lands {
        crate::rules::space::Detonation {
            at: crate::rules::space::detonation_point(params.target_at, params.player_at),
            height_m: 0.0,
        }
    } else {
        crate::rules::space::detonation_of_miss(
            crate::rules::space::muzzle(params.player_at, params.aim_point()),
            params.aim_point(),
            params.target_at,
            range,
            off_axis,
            dev,
            phi,
        )
    };
    if pellet_lands {
        *landed_this_shot = true;
    }
    // A PELLET THAT WENT NOWHERE IS STILL SOMETHING THAT HAPPENED.
    //
    // Three exits below produce no damage at all — outside the cone,
    // out of the weapon's range, an explosion that reached nobody — and
    // until this landed they produced no ROW either, so "why did a
    // three-pellet shot pop two numbers" had no answer anywhere in the
    // ledger. It is not a per-pellet row: on every official ruler the
    // target is at contact and this never fires at all.
    //
    // THE ARRIVAL ITSELF IS NOT A ROW. It was, for an afternoon, with
    // the flight on it — how far the round went, what was left of the
    // reach and of the punch-through budget. Against a target at
    // contact that is one row per pellet saying "it arrived, 0.00 m",
    // which is half the stream to say nothing, and the owner took it
    // back out the same day. What it was going to answer —
    // which numbers are one arrival — is still answerable from the shot
    // they share; what is genuinely lost is the flight, and no scenario
    // this app ships makes that a number worth a row.
    if rec.is_on() {
        if !in_range {
            // OUT OF RANGE IS NOT A MISS in the aiming sense — the
            // round never arrived, so its explosion does not go off
            // either. Both facts are the same sentence to a reader.
            rec.push(t, Some(0), crate::record::Kind::Miss {
                reason: "out of the weapon's range",
            });
        } else if !pellet_lands {
            rec.push(t, Some(0), crate::record::Kind::Miss {
                reason: "outside the cone",
            });
        }
    }
    // A TERMINAL BLAST GOES OFF WHERE THE ROUND DISSIPATES, not on the
    // first body it touched — see `data::weapons::BlastKind` and
    // MEASUREMENTS M53. The budget buys MATERIAL,
    // so the round crosses bodies until it cannot get out of one and
    // detonates there; what is left over when it clears them all is
    // spent as flight, since this arena has no wall to stop it.
    // With no punch through nothing moves.
    //
    // Only on a pellet that LANDS. One that missed never touched
    // anything to punch through, so where it ends up is the ordinary
    // miss geometry's question and is left to it.
    // A SLAM GOES OFF AT THE WIELDER'S OWN FEET, whatever the swing
    // touched — which is what makes it the one melee attack with no
    // reach problem: nothing has to be hit for it to detonate, and the
    // 2.5 m a hammer swings has nothing to do with the 10 m the floor
    // does. It is checked FIRST because it does not care whether the
    // pellet landed.
    let det = if active.radial.as_ref().map(|r| r.blast_kind)
        == Some(crate::model::BlastKind::Slam)
    {
        crate::rules::space::Detonation { at: params.player_at, height_m: det.height_m }
    } else if pellet_lands
        && active.radial.as_ref().map(|r| r.blast_kind)
            == Some(crate::model::BlastKind::Terminal)
    {
        let aim = params.aim_point();
        let mut bodies = Vec::with_capacity(params.others.len() + 1);
        bodies.push(params.target_at);
        bodies.extend(params.others.iter().map(|f| f.at));
        crate::rules::space::Detonation {
            at: crate::rules::space::dissipation_point(
                det.at,
                crate::rules::space::muzzle(params.player_at, aim),
                aim,
                &bodies,
                params.punch_through_m,
                params.projectile_width_m,
            ),
            height_m: det.height_m,
        }
    } else {
        det
    };
    // A COMBO SWING THAT ENDS ON A SLAM fires the weapon's own, and
    // it REPLACES the attack's radial rather than joining it: a light
    // melee attack has no explosion of its own, so there is never a
    // second one to conflict with, and one radial stage per shot is
    // what the loop is built around.
    let attack_radial = match (swing.as_ref().and_then(|h| h.slam_multiplier), active.slam) {
        (Some(k), Some(slam)) => Some(crate::build::loadout::ResolvedRadial {
            damage: slam.damage.scale(k),
            modified_base: slam.modified_base * k,
            ..slam
        }),
        _ => active.radial,
    };
    let radial_stage = match attack_radial {
        Some(r) if !r.takes_multishot && pellet_idx > 0 => None,
        other => other,
    };
    // …AND THE SWING SCALES THE EXPLOSION, which on a SLAM is the whole
    // attack. A heavy slam is `3x base` delivered as a radial and it
    // takes the combo multiplier like any other heavy attack, so a
    // swing multiplier that reached only the direct stage would leave
    // the one melee mode that is entirely radial reading its unswung
    // base — 630 at every combo tier.
    //
    // A STANCE SLAM CARRIES ITS OWN MULTIPLIER, already in `attack_radial`,
    // so the swing's — zero on a slam-only row — is not it. What it takes
    // is what every slam takes: the counter a heavy spends, and Seismic
    // Wave, which "also increase[s] the damage dealt by slam attacks
    // performed via Stance Combos" (W`Seismic_Wave`).
    let stance_slam = !tennokai_heavy
        && active.slam.is_some()
        && swing.as_ref().is_some_and(|h| h.slam_multiplier.is_some());
    let radial_mult = if stance_slam {
        (if active.spends_combo { combo_multiplier } else { 1.0 }) * (1.0 + active.slam_damage)
    } else {
        swing_mult
    };
    let radial_stage = match radial_stage {
        Some(r) if (radial_mult - 1.0).abs() > 1e-12 => Some(crate::build::loadout::ResolvedRadial {
            damage: r.damage.scale(radial_mult),
            modified_base: r.modified_base * radial_mult,
            ..r
        }),
        other => other,
    };
    // THE STAGES OF ONE PELLET, in the order they go off: the bullet,
    // the explosion, and then each bomblet the explosion threw — a
    // contact hit and an explosion of its own, `count` times over.
    //
    // NO BOMBLETS WITHOUT THE EXPLOSION THAT THREW THEM: they are what
    // a detonation releases, so a pellet that never detonated releases
    // none. `takes_multishot` is false on both halves, so the same
    // clause that keeps a once-per-shot explosion off pellets 1.. keeps
    // the bomblets off them (`ClusterSpec`: the count is the bomb's).
    //
    // COUNTED, NOT COLLECTED. This is the engine's hottest loop and a
    // `Vec` of stages is an allocation every pellet fires — the list is
    // `[direct, radial, (contact, blast) x count]`, which an index
    // answers for free.
    let cluster = active
        .cluster
        .filter(|_| pellet_idx == 0 && radial_stage.is_some());
    let bomblets = cluster.map_or(0, |c| c.count.round().max(0.0) as usize);
    let n_stages = 1 + usize::from(radial_stage.is_some()) + 2 * bomblets;
    for stage in 0..n_stages {
        let rad = match stage {
            0 => None,
            1 => radial_stage,
            s => cluster.map(|c| if s % 2 == 0 { c.contact } else { c.blast }),
        };
        let direct = rad.is_none();
        // EVERY INSTANCE RE-READS THE TARGET — not every shot, and not
        // even every pellet.
        //
        // A pellet that carries an explosion is TWO instances, and the
        // explosion settles against the state its own collision left
        // one instant earlier: a weapon forcing a Viral proc on both
        // halves pops `200 / 1,200 / 450 / 1,500`, which is the ladder
        // read at 0 / 1 / 2 / 3 stacks. Reading it once per pellet gave
        // `200 / 600 / 450 / 1,350` — each explosion sharing its
        // collision's snapshot, a step behind all fight. It is a few
        // per cent on a status build, always in the direction of "this
        // build is good", and invisible in every mean this engine
        // reports; the combat record is what made it visible, because
        // the row states the stacks it read.
        // `a_volley_settles_pellet_by_pellet_and_each_instance_re_reads_the_target`
        // is the golden test.
        rec.begin_instance();
        let mit = debuffs.amps(
            t,
            status_damage,
            active.armor_strip_per_puncture,
            params.squad.enemy_armor_multiplier,
        );
        // …AND WHAT THE SHOOTER HAD UP, for THIS instance. The same
        // rule as the line above, on the other side of the fight: a
        // volley changes the SHOOTER's state too — Secondary Enervate
        // stacks on a headshot, so pellet 1's hit is what pellet 2
        // reads — and it was sampled once at the shot and stamped on
        // every row of it.
        //
        // That made the panel say something its own numbers denied: two
        // rows one pellet apart showed the same buffs and the same
        // stacks on the target, and one carried a Condition Overload
        // bracket the other did not, because the factor was current and
        // the column was not. A state column that does not match the
        // number beside it is worse than no column.
        if rec.wants(t) {
            let stacks = sample_stacks(
                params, rec_roster, t, &mut *arc, &mut *gal, &mut *buff_stacks,
                &windows.crit_on_headshot_stacks, windows.crit_on_headshot, windows.weakpoint_buff,
                windows.fire_rate_after_reload,
                windows.base_damage_after_reload, windows.base_damage_eximus,
                windows.streak, tendril.count, crit_per_hit.stacks, bar,
                combo_at(combo_spec, params.combo_held, sniper_combo.count, sniper_combo.last_hit, t),
                incarnon.incarnon_until,
        *influence_until,
            );
            rec.set_stacks(stacks);
        }
        // A MISSED PELLET DEALS NOTHING AND DOES NOTHING. Skipping the
        // stage rather than zeroing its damage is the whole point: the
        // status draw, the gauge charge, the on-hit buffs and the combo
        // count all live inside it, and a hit that dealt zero is not
        // what a miss is.
        //
        // A BEAM THAT STRUCK NOBODY THEREFORE DEALS NOTHING, which is
        // the owner's own rule read straight: *"if it
        // hits, it hits; if it does not, it is zero"*. What it does NOT
        // do yet is leave its damage sphere on the floor where it
        // landed — see docs/UNMODELLED.md. Trying it by keeping the
        // instance alive and zeroing the damage made a MISS proc
        // status, charge the gauge and feed the on-kill buffs, which is
        // what the paragraph above exists to prevent.
        if direct && !pellet_lands {
            continue;
        }
        // OUT OF RANGE IS NOT A MISS — the round never got here at all,
        // so the explosion does not go off either. A missed grenade
        // lands beside the target and still detonates (the clause
        // below); one fired past the weapon's reach does not arrive.
        //
        // A projectile that FALLS SHORT would really detonate at the
        // end of its flight, which is a different model and is not
        // invented here: no weapon in the roster declares both a
        // `range_m` and a `radial:`, so this is the honest answer
        // rather than the convenient one.
        if !in_range {
            continue;
        }
        // …AND THE EXPLOSION STILL GOES OFF. A missed grenade lands
        // beside the target rather than vanishing, so the radial is
        // resolved from where the pellet actually crossed — which is
        // the first thing in this engine that ever gave the explosion's
        // own falloff a distance to read. Past the blast
        // radius there is no explosion to resolve at all.
        if let Some(r) = rad {
            // DOES IT REACH ANYBODY? Asked of the whole formation
            // and not of the AIMED body: a wide shot that leaves that
            // one body out of range still detonates on the bodies
            // standing where it landed. Asked with
            // `blast_reach(aim_offset)`, the same reach the damage
            // below uses, or it fires one body radius early.
            let reaches = |b: crate::rules::space::Vec2| {
                r.falloff_at(crate::rules::space::blast_reach(det.distance_to(b))) > 0.0
            };
            if !reaches(params.target_at)
                && !params.others.iter().any(|f| reaches(f.at))
            {
                continue;
            }
        }
        // Instance values — the shadowing happens here. The explosion
        // rolls its own crit tier off its own crit chance, and the live
        // crit buffs reach it: the relative ones scale ITS base, the
        // absolute ones add flat. Under AssumedMax those same bonuses
        // arrive through the mod bucket in `r.crit_chance`, so this is
        // what makes the two policies agree about one mod.
        // THIS PROJECTILE'S OWN VECTOR, when the weapon has one per
        // projectile. `pellet_idx` wraps, so a multishot source that
        // did add projectiles would cycle the elements again rather
        // than run off the end of the list.
        let own = if variants.is_empty() {
            None
        } else {
            Some(pellet_idx as usize % variants.len())
        };
        let (qvec, tier) = match &rad {
            None => (own.map_or(*qvec, |i| variants[i].0), tier),
            Some(r) => {
                // NO `weakened_cc` here. Puncture's Weakened is a flat
                // crit-chance buff on the VICTIM, and the wiki states
                // its one exclusion outright: "This is a flat critical
                // chance buff (like Arcane Avenger), but does not apply
                // to Area of Effect damage or Warframe abilities"
                // (Damage/Puncture_Damage). An explosion is Area of
                // Effect damage, so it does not get it — the lingering
                // field never did, and the radial's copy of this line
                // was the odd one out.
                let rcc = r.crit_chance + flat_crit + r.base_crit_chance * crit_chance_relative;
                // The set promotes a critical hit "from Primary
                // Weapons" with no qualifier about which attack part
                // made it, and the direct hit and the lingering field
                // both get it — an explosion left out would be an
                // artifact of where the code was edited, not a rule.
                let t2 = upgrade_crit_tier(
                    roll_crit_tier(rcc, &mut d.spine),
                    active.crit_tier_upgrade_chance,
                    &mut d.spine,
                );
                (
                    own.map_or_else(
                        || r.damage.quantized_against(r.modified_base),
                        |i| variant_rad[i],
                    ),
                    t2,
                )
            }
        };
        // WARFRAME ABILITY ELEMENTS, added to the FINISHED vector.
        //
        // Not through the elemental hierarchy — "does not combine with
        // other elements" is stated on every one of the four augment
        // pages, and it is the whole reason they are worth having
        // separately from a mod. Sized off THIS
        // stage's own ModifiedBase, because "additive with elemental
        // mods" makes them a percentage of the part's base the same
        // way an elemental mod is (MECHANICS §7).
        //
        // Read at `t`: they expire, and after they do the weapon is
        // simply the weapon again.
        let stage_mb = match &rad {
            None => modded_base,
            Some(r) => r.modified_base,
        };
        // THE VECTOR BEFORE THE ABILITY'S ELEMENTS, kept so the row
        // can show how its base was built rather than asserting one
        // number. See `Damage::base_steps`.
        // THE VECTOR BEFORE THE SNAP, for the ledger's quantization
        // layer. Read from the ACTIVE stage rather than reconstructed:
        // the radial has its own, and a row that showed the direct
        // hit's grid for an explosion would be a different weapon's
        // arithmetic.
        let pre_quantization = match &rad {
            None => direct_pre_snap,
            Some(r) => r.damage,
        };
        let pre_ability_total = qvec.total();
        let qvec = params.with_live_elements(qvec, stage_mb, t, windows);
        // A merged beam tick carries the SUM of its beams. `qtotal`
        // is what the instance deals; the crit CHANCE that produced
        // `tier` above was deliberately left at one beam's.
        let qtotal = qvec.total() * *beam_merge;
        // The SHAPE comes from the vector, never from `qtotal`: a
        // merged beam tick scales the total by `(*beam_merge)` while the
        // composition is unchanged, and dividing by the scaled total
        // understated Toxin's shield bypass by exactly that factor.
        let shares = TypeShares::of(&qvec);
        let crit_multiplier = match &rad {
            None => crit_multiplier,
            Some(r) => {
                // No `part.crit_bonus` doubling: that is the crit-
                // HEADSHOT rule and an explosion has no hit location.
                let rcd = r.crit_damage + r.base_crit_damage * crit_damage_relative + cd_abs;
                1.0 + tier as f64 * (rcd - 1.0)
            }
        };
        let part_factor = if direct { part_factor } else { 1.0 };
        let landing = if direct { landing } else { 1.0 };
        // ModifiedBase carries the merge too, which is what makes
        // damaging status effects "affected TWICE by multishot": more
        // procs from the summed status chance, and a bigger payload
        // each because the instance itself is bigger.
        let mb_live = *beam_merge
            * match &rad {
                None => mb_live,
                Some(r) => r.modified_base * arc_ratio,
            }
            // THE CHAMBERS REACH STATUS DAMAGE, which is stated and not
            // inferred: *"The damage bonus applies to all Multishot
            // hits and to Status Damage"* (wiki, Primed Chamber). It is
            // the ONE line that separates this card from Synth Charge,
            // whose page never says either way and which therefore
            // multiplies the instance and leaves `mb_live` alone.
            * cc_mult;
        // WHAT A SPREAD'S STATUSES BURN OFF, and it is THIS number
        // rather than the bare `modded_base` every spread was handed.
        // `spread_hit` multiplies the arcane ratio back in, so the one
        // term is divided out here and the rest — the merge above, the
        // Chamber — travels. A merged beam's DoT was `(*beam_merge)` times
        // bigger on the body it was aimed at than on the body BEHIND
        // it, which is the same hit still flying.
        let spread_mb = mb_live / arc_ratio;
        // The direct hit always carries CO. An explosion does NOT by
        // default — the mods say direct hits only — but the engine
        // supports the case the mods forbid, because some entries do it
        // anyway and the CO catalog lists them one at a time: the
        // Zylok's Incarnon radial has a row reading "Radial hit only
        // receives CO bonus on target DIRECTLY HIT by bullet", which
        // the single-target arena always is. Per-entry weapon data, so
        // no roster weapon is affected until one declares it.
        // THE BRACKET, AND WHAT IT IS MADE OF. The engine multiplies
        // by `gunco.bucket`; the ledger shows the terms, which is the
        // difference between an algebraic rearrangement and a fiction.
        let gunco = match &rad {
            None => co_mult,
            Some(r) if r.takes_condition_overload => co_mult_radial,
            Some(_) => Gunco { bucket: arc_ratio, ..Default::default() },
        };
        let bucket = gunco.bucket;
        // Primary Crux's stacks join the status-chance BUCKET (wiki:
        // "additive to mods like Rifle Aptitude"), so the relative
        // bonus multiplies THIS part's own unmodded base — the
        // explosion's differs from the direct hit's.
        // ...and SENTIENT SURGE's status half rides the same bucket
        // for the same reason ("Additive to other ... status chance
        // mods"). Summed with the arcane's before either is spent, so
        // the two cannot end up multiplying each other.
        // …AND IT IS THE SHOT'S OWN SUM, not a second one. Two sums
        // over one bracket is two answers: this one carried the arcane
        // and Sentient Surge and left out everything LIVE that lands in
        // the same place — Weeping Wounds' `(combo - 1)`, an on-kill
        // status buff, Enduring Affliction's Lifted gate — so those
        // cards rolled nothing at all while the panel showed them.
        let sc_arc = sc_arc_shot;
        // WISEMAN'S REGARD, LIVE: "30% of CURRENT Critical Chance",
        // and current means at this shot. The row names Secondary
        // Outburst, Cascadia Overcharge, Secondary Enervate and
        // Galvanized Crosshairs among the sources that feed it — all
        // live, none of them on the panel — and the owner's ruling is
        // that anything landing on the WEAPON's own crit chance counts. So the panel's static answer is taken back and
        // `effective_cc` pays instead: the same number the crit roll is
        // about to use, which is per SHOT and therefore excludes the
        // weak-point bonus the card's own row also excludes.
        //
        // The DIRECT part only. An explosion has its own status chance
        // and its own base, and the card is about the weapon's.
        let derived_sc = match active.derived_status_from_crit {
            Some((rate, cap, folded)) if rad.is_none() => {
                (rate * effective_cc).min(cap) - folded
            }
            _ => 0.0,
        };
        let status_chance = *beam_merge
            * (match &rad {
                None => active.status_chance + active.base_status_chance * sc_arc,
                Some(r) => r.status_chance + r.base_status_chance * sc_arc,
            } + derived_sc)
            // Death Knell's, on the FINISHED number.
            + weakpoint_sc;
        // EACH PART'S OWN. The direct hit reads the attack's list; an
        // EXPLOSION reads its own, because "Guaranteed Impact proc" is
        // said of one and not the other on the same weapon (the Scourge
        // pair says it of the spear explosion; the Astilla says it of
        // the direct hit and not of its radial).
        // …PLUS WHAT A WARFRAME ABILITY FORCES. Valence Formation imbues
        // its element *"with guaranteed Status"* (wiki), so that element
        // procs on every hit whatever the weapon's status chance.
        //
        // IT RIDES THE SAME LIST as a weapon's own "guaranteed Impact
        // proc", which is what keeps the rest of the status path — the
        // immunity renormalisation, the DoT bookkeeping — from having to
        // know an ability is involved. It is also what makes the
        // interaction fall out rather than be arranged: Overwhelming
        // Attrition asks for a hit that "is neither Critical nor
        // applies a Status Effect", and a hit that always procs can
        // never be one.
        let ability_forced =
            crate::data::abilities::forced_status_elements_at(&params.abilities, t);
        // ONE SLOT PER DAMAGE TYPE IS ENOUGH: both sources are sets of
        // types and the merge below refuses a duplicate, so the union
        // can never be longer than the type list itself.
        let mut forced_buf = [DamageType::Impact; DamageType::ALL.len()];
        let forced_len = forced_procs(
            active, &rad, direct, swing, swing_forced_types, &ability_forced, pellet_idx,
            &mut forced_buf,
        );
        let forced: &[DamageType] = &forced_buf[..forced_len];

        // Devouring Attrition: an INDEPENDENT multiplier rolled per
        // INSTANCE that did not crit (wiki: "multiplicative to base
        // damage bonuses such as Hornet Strike"; "affects both
        // forms", the explosions included).
        let attrition = noncrit_mult(active.noncrit_bonus, tier, &mut d.spine);
        // THE SHOT COMBO COUNTER, as the counter stood when this shot
        // was fired. Read BEFORE the hit is counted, because that is
        // the multiplier the player saw under the reticle when they
        // pulled — the hit that reaches Minimum Combo is the one that
        // ARMS it, not the one that spends it.
        //
        // It multiplies the whole shot, direct and radial alike: the
        // wiki calls it "a bonus to their total damage". Only the
        // DIRECT hit builds it — *"Area-of-effect and damage over time
        // do not affect the Strike Combo Counter"* — which is counted
        // below.
        let combo_now =
            combo_at(combo_spec, params.combo_held, sniper_combo.count, sniper_combo.last_hit, t);
        let combo_multiplier = active.sniper_combo.map_or(1.0, |c| c.multiplier(combo_now));
        // DAMAGE FALLOFF over the distance this instance travelled.
        //
        // THE DIRECT PART ONLY, and the range is asked of the POINT it
        // went off at rather than of the fight — see
        // `FightParams::range_to`. The explosion's own falloff is
        // measured from its epicentre, which sits on the target it just
        // hit, so a radial takes full damage here whatever the
        // engagement range is; that is the same thing its `unmodeled:`
        // line has always said and it stays true.
        //
        // 1.0 at point blank and for every weapon that lists no
        // falloff, which is the whole roster minus nineteen entries —
        // so this factor moves no number the engine reported before the
        // arena had a distance in it.
        let falloff = falloff_factor(active, params, rad.as_ref(), det, gap_m);
        // A SPREAD INSTANCE LANDS ON A BODY, so the pellet's own
        // head factor comes back off before it is handed on.
        //
        // `raw` below is multiplied by `part_factor`, and every spread
        // mechanism was fed `raw / bucket` — so a chain hop, a splash
        // and an echo all inherited the aimed pellet's HEADSHOT on
        // their direct damage, while `spread_hit`'s own doc comment
        // said "NEVER A HEADSHOT ... `part_factor` is 1.0 here". It was
        // 1.0 where that comment looks (the PROC scale) and not in the
        // damage, so the claim was true of half the instance (found
        // 2026-08-17, building punch-through). Single-target fights are
        // untouched: with no formation, no spread mechanism runs.
        //
        // PUNCH THROUGH IS THE EXCEPTION and takes `raw` undivided: it
        // is the same pellet on the same line, so if it took a head it
        // keeps taking one.
        let body_only = |x: f64| x / part_factor.max(1e-9);
        let dt_here = if direct && active.consecutive_hit_radial_only { 1.0 } else { dt_mult };
        let raw = qtotal
            * part_factor
            * crit_multiplier
            * bucket
            * params.faction_at_time(t)
            * arc_final
            * attrition
            // DOUBLE TAP stands on its own: "multiplicatively stacks
            // with damage bonuses like Serration and Faction Damage
            // Bonus", so it is a factor here and never a bucket term.
            // ON THE LATRON INCARNON ONLY THE EXPLOSION TAKES IT: the
            // collision reads 148 with the pile empty and full (M102).
            * dt_here
            // SYNTH CHARGE, on the magazine's LAST round only: "Damage
            // stacks multiplicatively with Hornet Strike, and any area
            // damage the weapon may have is also affected" — so it is a
            // factor here, beside Double Tap's, and it reaches the
            // explosion because every part of the shot comes through
            // this line.
            * sc_mult
            // THE CHAMBERS, on the magazine's FIRST round only: Charged
            // Chamber is "multiplicative with other damage mods" and
            // Primed Chamber "is applied multiplicatively after all
            // other modifiers from mods and abilities", so it is a
            // factor here beside Synth Charge's. It reaches the whole
            // shot — "applies to all Multishot hits" — and the status
            // payload takes it separately below, which is the one thing
            // that tells it apart from the last-round card.
            * cc_mult
            // ECLIPSE: "an unique multiplier", so it stands beside the
            // others rather than joining any of them — and CONDITION
            // OVERLOAD IS THE ONE THING IT DOES NOT REACH, measured
            // (MEASUREMENTS M79). `eclipse_at` spends it on everything
            // but the share the CO term put in the bracket, which is
            // the same arithmetic as Eclipse joining that bracket and
            // is the only one of the two this reading can tell apart.
            * eclipse_at(params.ability_final_at(t), co_mult.co_share)
            * beam_ramp
            * combo_multiplier
            // Multishot paid in DAMAGE, for the weapon that works that
            // way — "multiplicative to other sources of damage", so it
            // stands here beside Double Tap rather than in a bucket.
            * ms_damage
            * pm_mult
            * falloff;
        let head_direct = direct && part.is_head;
        let col = target.incoming_column(&params.foe);
        // THE TARGET AS IT STOOD, before this instance touched it. Read
        // HERE and not afterwards: `apply` spends the pools and, on a
        // kill, respawns the body outright, so a snapshot taken on the
        // next line would be a snapshot of a different fight.
        let mut breakdown = Breakdown::default();
        let settled = target.apply(
            raw,
            shares,
            head_direct,
            t,
            &params.foe,
            false,
            &mit,
            og_mult,
            watching(rec, &mut breakdown),
        );
        let (effective, killed, broke) =
            (settled.effective, settled.killed, settled.broken);
        // THE AIMED SEED'S CHAINS, HERE, so they take multishot the
        // only way that is honest: by being inside the pellet loop.
        // *"only targets directly hit by the beam benefit"*, and a
        // merged beam's multishot IS the pellet count — so a chain
        // launched from the body the beam struck fires once per landing
        // pellet, and one launched from a body the RADIUS caught fires
        // once for the shot. The other half is after the loop.
        // AN EXPLOSION REACHES THE WHOLE FORMATION, which is the
        // other half of what a radius mod buys and the half a grenade
        // lives on. Its own stage, because a blast has no path: every
        // body the sphere touches takes one instance at its own
        // falloff.
        if let (Some(rr), false) = (rad, others.is_empty()) {
            spread_from_blast(
                windows,
                det,
                &mut *others,
                params,
                active,
                &rr,
                if falloff > 0.0 {
                    body_only(raw / bucket / falloff)
                } else {
                    0.0
                },
                shares,
                crit_multiplier,
                tier,
                attrition,
                spread_mb,
                status_chance,
                forced,
                &qvec,
                &mut *gal,
                &mut *arc,
                &mut *r,
                rec,
                d,
                t,
            );
        }
        // …AND ONE EXPLOSION PER BOUNCE, centred on the body the
        // projectile came off rather than on the aimed one. "Dealing
        // damage once for any collision on enemies, and AGAIN FOR THE
        // EXPLOSION" — this is the second half of that sentence, and it
        // is the larger half on this family: 140 to the collision's 50.
        if let (Some(rr), Some(path)) = (rad, ric_path.as_deref()) {
            for &(body, _) in path {
                let Some(idx) = body.checked_sub(1) else { continue };
                let Some(fs) = params.others.get(idx) else { continue };
                blast_at(
                windows,
                    crate::rules::space::Detonation {
                        at: fs.at,
                        height_m: 0.0,
                    },
                    &mut *others,
                    params,
                    active,
                    &rr,
                    if falloff > 0.0 {
                        body_only(raw / bucket / falloff)
                    } else {
                        0.0
                    },
                    shares,
                    crit_multiplier,
                    tier,
                    attrition,
                    spread_mb,
                    status_chance,
                    forced,
                    &qvec,
                    &mut *gal,
                    &mut *arc,
                    &mut *r,
                    rec,
                    d,
                    t,
                    SpreadBy::Ricochet,
                );
            }
        }
        // WHERE THE PROJECTILE BOUNCED, and whether each arrival found
        // a head — decided ONCE for this pellet, because the collision
        // (direct part) and the explosion (radial part) are two passes
        // over the same flight and must not disagree about it.
        if direct && ric_path.is_none() {
            if let Some(rc) = params.ricochet {
                let from = struck.first().copied().unwrap_or(0);
                let n = params.others.len() + 1;
                // TWO MECHANICS, and the field each wiki page publishes
                // is what tells them apart (MECHANICS §Bounce).
                //
                //   · RICOCHET is hitscan and SEEKS: it "redirects to
                //     hit another enemy" inside a stated `range_m`.
                //     The neighbour walk IS that mechanic.
                //   · BOUNCE is a projectile and REFLECTS at the angle
                //     of incidence, with no range at all. It gets the
                //     geometry.
                // `range_m` is INFINITY when the data states none, and
                // stating none is exactly what a BOUNCE weapon does.
                let hops = if rc.range_m.is_finite() {
                    ricochet_layout
                        .as_ref()
                        .map(|l| crate::rules::chain::bounce_path(l, n, from, rc.bounces))
                        .unwrap_or_default()
                } else {
                    crate::rules::space::bounce_path(
                        params.player_at,
                        bounce_bodies,
                        from,
                        rc.bounces,
                        // WHERE ON THE FIRST BODY IT LANDED, uniform
                        // across the width. The one assumption in the
                        // path; see `rules::space::bounce_path`.
                        d.spine.next_f64() * 2.0 - 1.0,
                    )
                };
                ric_path = Some(
                    hops.into_iter()
                        // ONE ROLL PER ARRIVAL, off the same stream the
                        // aimed pellet's body part comes from — a place
                        // the shot landed is a place the shot landed.
                        .map(|b| (b, d.spine.chance(rc.headshot_chance)))
                        .collect::<Vec<_>>(),
                );
            }
        }
        // WHAT THIS PELLET LEFT ON EVERY BODY IT STRUCK — the aimed
        // one and each body the swing followed through to. Melee
        // Influence spreads from all of them, so they are gathered here
        // and fired once the aimed body's own statuses have landed.
        let mut influence_seeds: Vec<(usize, Landed)> = Vec::new();
        // WHAT THE ROUND DID BEHIND THE FIRST BODY — read below, where a
        // weak point's triggers are fired.
        let mut punched = PunchedWeakPoints::default();
        if direct && !others.is_empty() {
            // …and the SHOT's own factors, kept for the half that fires
            // once rather than per pellet — recorded on a MISS too, so
            // the sphere that went off on the floor still has a number
            // behind it.
            if strike_spread.is_none() {
                *strike_spread = Some(SpreadStrike {
                    raw_per_bucket: body_only(raw / bucket),
                    shares,
                    crit_multiplier,
                    crit_tier: tier,
                    attrition,
                    modded_base: spread_mb,
                    status_chance,
                    forced: forced.to_vec(),
                    vector: qvec,
                });
            }
            // …AND EVERY BOUNCE'S COLLISION. The projectile arriving
            // again, in full, at a body it has not hit — and the one
            // spread that may land on a head.
            if let Some(path) = ric_path.as_deref() {
                spread_from_ricochet(
                windows,
                    &mut *others,
                    params,
                    active,
                    path,
                    body_only(raw / bucket),
                    shares,
                    crit_multiplier,
                    tier,
                    attrition,
                    spread_mb,
                    status_chance,
                    forced,
                    &qvec,
                    &head_factor,
                    head_landing,
                    &mut *gal,
                    &mut *arc,
                    &mut *r,
                    rec,
                    d,
                    t,
                );
            }
            // …AND THE ECHO, per landing pellet, because the arcane
            // says each one triggers it.
            spread_from_echo(
                windows,
                &mut *others,
                debuffs.confusion.len(),
                params,
                active,
                body_only(raw / bucket),
                shares,
                crit_multiplier,
                tier,
                attrition,
                spread_mb,
                status_chance,
                forced,
                &qvec,
                &mut *gal,
                &mut *arc,
                &mut *r,
                rec,
                d,
                t,
            );
            // …AND EVERYTHING BEHIND THE TARGET that this pellet
            // punched through to, at full damage. Only where there is
            // no beam: with one, these bodies are the chain's seeds
            // and `spread_from_seeds` below already pays them.
            // …AND EVERY BODY THIS SWING REACHED, at `FT^(n-1)`.
            //
            // MELEE'S OWN ANSWER TO "who else did that hit", and it is
            // exclusive with punch through rather than beside it: a
            // swing has no punch-through budget (nothing in the melee
            // pool grants one and the wiki excludes slams and AoE from
            // Follow Through by name), so the two can never both fire.
            if let Some(ft) = active.follow_through.filter(|_| direct) {
                spread_from_follow_through(
                windows,
                    &mut *others,
                    params,
                    active,
                    melee_struck,
                    &mut influence_seeds,
                    ft,
                    raw / bucket,
                    shares,
                    crit_multiplier,
                    tier,
                    attrition,
                    spread_mb,
                    status_chance,
                    forced,
                    &qvec,
                    &mut *gal,
                    &mut *arc,
                    &mut *r,
                    rec,
                    d,
                    t,
                );
            }
            if params.beam.is_none() {
                punched = spread_from_punch_through(
                windows,
                    &mut *others,
                    params,
                    active,
                    struck,
                    raw / bucket,
                    shares,
                    crit_multiplier,
                    tier,
                    attrition,
                    spread_mb,
                    status_chance,
                    forced,
                    &qvec,
                    head_direct,
                    part_factor,
                    landing,
                    &mut *gal,
                    &mut *arc,
                    &mut *r,
                    rec,
                    d,
                    t,
                );
            }
            // THE AIMED SEED'S CHAINS take multishot by firing per
            // landing pellet — so a pellet that landed on nobody starts
            // none of them. The radius-caught seeds' half fires once
            // after the loop, miss or not.
            if let Some(beam) = params.beam {
                spread_from_seeds(
                    windows,
                    &mut *others,
                    params,
                    active,
                    beam,
                    body_only(raw / bucket),
                    shares,
                    crit_multiplier,
                    tier,
                    attrition,
                    spread_mb,
                    status_chance,
                    forced,
                    &qvec,
                    &mut *gal,
                    &mut *arc,
                    &mut *r,
                    rec,
                    d,
                    t,
                    chain_layout.as_ref(),
                    true,
                    struck,
                );
            }
        }
        // THE ACCOUNT OF THIS HIT — written HERE because this is the
        // one place every factor exists at the same time. Anywhere else
        // and the list would be reconstructed, which is how a breakdown
        // comes to disagree with the number it explains.
        //
        // ONE WRITER, TWO READERS. The panel's worked example and the
        // combat record's row for this hit are the same list of
        // factors, so it is built once and handed to both. Two lists
        // would be two things to keep in step.
        let hit_steps: Vec<crate::record::Step> = if rec.is_on() {
            vec![
                (crate::record::Factor::BodyPart, part_factor),
                (crate::record::Factor::Critical, crit_multiplier),
                (crate::record::Factor::ConditionOverload, bucket),
                faction_layers(params, t, DEPTH_HIT)[0],
                faction_layers(params, t, DEPTH_HIT)[1],
                (crate::record::Factor::ArcaneFinal, arc_final),
                (crate::record::Factor::Attrition, attrition),
                (crate::record::Factor::WarframeAbility, eclipse_at(params.ability_final_at(t), co_mult.co_share)),
                (crate::record::Factor::BeamRamp, beam_ramp),
                // DOUBLE TAP and SYNTH CHARGE were applied and
                // never listed, which is the exact failure a ledger
                // exists to catch — it only slipped because both are
                // 1.0 in every build the check had run.
                // `check_combat_record` asks it of EVERY row now.
                (crate::record::Factor::DoubleTap, dt_here),
                (crate::record::Factor::SynthCharge, sc_mult),
                (crate::record::Factor::ChamberFirstRound, cc_mult),
                (crate::record::Factor::SniperCombo, combo_multiplier),
                (crate::record::Factor::MultishotAsDamage, ms_damage),
                (crate::record::Factor::MultishotGenerated, pm_mult),
                // DISTANCE. Listed even when it is 1.0 — this
                // ledger keeps a factor of exactly 1.0 rather
                // than dropping it, because "falloff ×1.00" is
                // the answer to "why does range not hurt me"
                // and a missing line is not.
                (crate::record::Factor::DamageFalloff, falloff),
            ]
        } else {
            Vec::new()
        };
        if direct {
            // ONE PER LANDING PELLET. *"Weapons with Multishot will
            // count each successful hit from the same shot as multiple
            // shot instances"* — and per enemy under Punch Through,
            // which this arena has only one of. Counted here so the
            // NEXT instance sees it, which is the order the previous
            // paragraph reads it in.
            if active.sniper_combo.is_some() {
                sniper_combo.count = combo_now + 1;
                sniper_combo.last_hit = t;
            }
            r.sources.direct += effective;
            add_by_type(&mut r.sources.direct_by_type, &qvec, effective, &col);
        } else {
            r.sources.radial += effective;
            add_by_type(&mut r.sources.radial_by_type, &qvec, effective, &col);
        }
        // THE ONE SITE THAT KNOWS ALL FOUR SHAPES: a hit is plain, a
        // crit, a weak point, or both — and "both" is its own number in
        // game, not a crit with a multiplier on it.
        r.note_kills(killed as u32, t, params.drop_is_in_reach(target.at));
        // …AND WHAT THE KILL LEAVES STANDING. `direct` because a ghost
        // is left by the shot rather than by anything it set off, and
        // the range is the card's own ("within 50 meters of the user").
        if killed && direct && leaves_one(active, params.player_at, params.target_at) {
            r.ghost_kills += 1;
        }
        // EXECUTIONER'S FORTUNE. Rolled HERE and nowhere else, because
        // this is the only place that knows both halves of its
        // condition: `head_direct` says the pellet landed in a head,
        // `killed` says it finished the target. An explosion never
        // headshots, so a radial pellet cannot pay.
        //
        // PER PELLET, like every on-hit roll here ("additional
        // shots from Multishot count as separate weakpoint hits"). AND
        // IT DOES NOT ROLL IN AN INCARNON FORM: "Does not affect
        // Incarnon Form", because what it refills is a MAGAZINE and an
        // Incarnon form has max CHARGES. Tested at the TRIGGER so the
        // roll is not even taken — one that can never be spent still
        // draws from `extra`.
        // LINGERING JUDGEMENT's streak, for the same reason:
        // `head_direct` is the only place a headshot is known to have
        // LANDED, per pellet, so a multishot pull can arm it alone. THE
        // ARMING HIT DOES NOT BENEFIT — its damage was settled above
        // and the window opens here.
        if let Some(s) = params.headshot_streak {
            if head_direct && s.hits > 0 {
                windows.headshot_times.retain(|&x| t - x < s.within);
                for _ in 0..=punched.hits {
                    windows.headshot_times.push(t);
                }
                if windows.headshot_times.len() >= s.hits as usize {
                    windows.streak = t + s.duration;
                    // SPENT. A streak is the last `hits` inside the
                    // window, so the ones that armed it cannot arm it
                    // again — otherwise every later headshot would
                    // re-arm on the same two and the "within 2 seconds"
                    // clause would never bind.
                    windows.headshot_times.clear();
                }
            }
        }
        roll_instant_reload(active, params, d, head_direct, killed, incarnon, ammo);
        // …AND ONCE PER BODY BEHIND IT. A punched weak point is a weak-point
        // hit, so it rolls; a card that asks for a KILL reads that body's.
        for i in 0..punched.hits {
            roll_instant_reload(active, params, d, head_direct, i < punched.kills, incarnon, ammo);
        }
        // A LANDED grenade leaves its field, whatever it rolled:
        // "Grenades stick to allies, enemies and surfaces", and a stuck
        // grenade means the target "cannot move out of the cloud".
        // Per PELLET — each multishot projectile is its own grenade and
        // its own cloud, and stacking is MEASURED (M13).
        //
        // The first tick lands WITH the impact — ✅ measured (M13): a
        // hit shows the direct number and the field's first number
        // together, then 9 more over the remaining 9 s. The wiki's
        // "Clouds do not instantly do damage, so enemies that are quick
        // may run through the cloud" describes the grenade arming, not
        // the tick clock; reading it as a delayed first tick cost a
        // tenth of the field's damage.
        if direct {
            if let Some(fp) = &active.lingering {
                // Renewed Horror doubles THIS field's lifetime, so it
                // ticks 20 times instead of 10 — ✅ measured (M13): one
                // direct number plus twenty field numbers.
                let boost = if *field_duration_boost {
                    active.field_duration_on_empty_reload
                } else {
                    1.0
                };
                let mut part = *fp;
                part.duration_seconds *= boost;
                let fresh = FieldState {
                    // …AND THE GRIMOIRE'S ORB IS THE OTHER SHAPE. Its
                    // contact is the direct hit this pellet already
                    // settled, and its pulses run on a one second clock
                    // from there — so its field opens one interval late
                    // rather than doubling the collision.
                    next_tick: t + part.first_tick_delay_seconds,
                    ticks_left: (part.duration_seconds * part.tick_rate).round() as u32,
                    part,
                    // Plentiful Mayhem follows a GENERATED grenade into
                    // the cloud it leaves — which is
                    // the whole value of the perk here, the cloud being
                    // most of this weapon's damage.
                    damage_multiplier: pm_mult,
                };
                match fp.stacking {
                    crate::model::FieldStacking::Stack => fields.push(fresh),
                    crate::model::FieldStacking::Refresh => {
                        fields.clear();
                        fields.push(fresh);
                    }
                }
            }
        }
        if direct {
            // A HIT TAKES A SECOND OFF THE RECHARGE. *"Hitting enemies
            // with the primary fire reduces recharge time by 1 second
            // per hit"*, and the page settles both of the questions
            // that raises without a field: *"Multishot will count as an
            // additional hit"* — so it is per landing PELLET, which is
            // what this branch counts — and *"Radial damage does not
            // count an additional hit"*, which is why it is inside
            // `direct` rather than beside it.
            if let Some(m) = active.meter {
                meter.seconds += m.seconds_per_hit;
            }
            r.pellets += 1;
            // A PELLET THAT WENT THROUGH: `struck` is who is on the
            // line, so a second body means this bolt left the first.
            if struck.len() > 1 {
                bump_buffs!(params, buff_stacks, rec_buff_index, rec, crate::model::BuffTrigger::PunchThrough, t, d.extra);
            }
            r.crits += (tier >= 1) as u32;
            r.big_crits += (tier >= 2) as u32;
            r.crit_tier_sum += tier;
            r.headshots += part.is_head as u32;
            r.headshots_on_others += punched.hits;
            *any_head |= part.is_head;
            *any_big |= tier >= 2;

            // Crosshairs' on-HEADSHOT buff refreshes on every head
            // hit (kills only matter for its stacks).
            if part.is_head {
                if let Some(b) = params.crit_chance_on_headshot {
                    windows.crit_on_headshot = t + b.duration;
                }
                // LEADED GAS, refreshed the same way and on the same event.
                if let Some(b) = params.on_weakpoint {
                    windows.weakpoint_buff = t + b.duration;
                }
                // EXIMUS ADVANTAGE — the WEAK POINT is the trigger
                // ("Despite the description specifying headshots, the
                // effect can be trigger on weak-point hits"), and the
                // second half of it is the TARGET. Against anything
                // that is not an Eximus this line never runs, which is
                // the whole reason the mod is not a plain on-headshot
                // buff. It REFRESHES rather than stacking.
                if params.foe.eximus {
                    if let Some(b) = params.base_damage_on_eximus_weakpoint {
                        windows.base_damage_eximus = t + b.duration;
                    }
                }
                // …AND THE PER-HIT ONES BELOW ARE PAID ONCE PER WEAK
                // POINT THE ROUND LANDED, the bodies it punched through
                // included (`PunchedWeakPoints`): the round enters the
                // same part of each, so a shot through two heads is two
                // weak-point hits and not one. The two windows above
                // take no count — a refresh is a refresh.
                // …AND A BODY THE ROUND KILLED BEHIND THE FIRST IS A
                // WEAK-POINT KILL. Paid HERE rather than in the kill path
                // below, which never runs when the aimed body also died: it
                // `continue`s out of this pellet.
                for _ in 0..punched.kills {
                    weakpoint_kill(params, arc, windows, t);
                }
                for _ in 0..=punched.hits {
                    // Lethal Rearmament: every headshot grants a stack —
                    // a LOCKED buff earns it too, it just never loses it.
                    // EVERY buff that triggers on a headshot, including
                    // its own chance roll. One line for the family.
                    bump_buffs!(params, buff_stacks, rec_buff_index, rec, crate::model::BuffTrigger::Headshot, t, d.extra);
                    // DEATH KNELL, per PELLET: "individual Multishot bullets
                    // can proc Death Knell" (wiki).
                    if let Some(w) = params.weakpoint_stacks {
                        weakpoint_pile.bump(t, w.duration_seconds, w.max_stacks);
                    }
                    // Primary Crux: a weak-point HIT (not a kill), per
                    // PELLET. Bumped here, AFTER this pellet's status
                    // chance was read above — the hit that grants a stack
                    // does not benefit from it, the same rule the
                    // base-damage stacks follow. A killing headshot still
                    // counts: this runs before the kill path's `continue`.
                    arc.bump_trigger(&params.arcane.buffs, ArcTrigger::WeakpointHit, t);
                    bump_buffs!(params, buff_stacks, rec_buff_index, rec, crate::model::BuffTrigger::ConsecutiveHeadshot, t, d.extra);
                }
            } else {
                // …AND A BODY HIT TAKES THE PILE. The only trigger in
                // this sim that the next shot can undo, and the reason
                // it is not `Headshot` with a clock: what ends it is
                // what you hit, not how long you waited.
                for (i, b) in params.stacking_buffs.iter().enumerate() {
                    if b.trigger == crate::model::BuffTrigger::ConsecutiveHeadshot {
                        buff_stacks[i] = LiveStacks::seed(0, b.max_stacks, b.duration);
                    }
                }
            }
        }

        if let Some(pool) = broke {
            push_break_proc(&mut *debuffs, params, t, pool);
        }
        // THE ROW FOR THIS PELLET — a MACRO with two call sites,
        // because a pellet has two ways of ending and the row has to
        // survive both.
        //
        // A single write before `settle_procs` names what the instance
        // APPLIED, which is right for a pellet that leaves the target
        // standing and wrong for one that KILLS: the kill branch below
        // `continue`s — correctly, since the killing instance's procs
        // die with the old individual — and would take the row with it.
        // Against a level 1 Crewman that is most of the shots, and the
        // record would show a fight with no kills in it while the meter
        // beside it counted six.
        //
        // The killing call passes NO procs, which is not a shortcut: it
        // is the same sentence that branch already makes.
        macro_rules! log_this_pellet {
            ($procs:expr) => {
            // THE ROW FOR THIS PELLET, written HERE rather than beside
            // `apply` — because a row says what the instance APPLIED, and
            // the proc list is not final until the line above. Everything
            // else it needs was snapshotted at the moment it landed
            // (`before`, `breakdown`, `settled`), so waiting costs nothing and
            // buys the one column a reader checks a status build with.
            ledger::settle(
                &mut *r, rec,
                t,
                Combatant::WIELDER,
                0,
                qvec.dominant(),
                match (part.is_head, tier > 0) {
                    (true, true) => PopKind::HeadCrit,
                    (true, false) => PopKind::Head,
                    (false, true) => PopKind::Crit,
                    (false, false) => PopKind::Direct,
                },
                &breakdown, settled,
                Some(&debuffs),
                ledger::Clock::Hit,
                || Instance {
                    // WHICH PELLET OF THE PULL THIS WAS. The first is the one
                    // the trigger would have fired with no multishot at all;
                    // every one after it is multishot's, and telling them apart
                    // is the difference between "my multishot works" and
                    // "something is double-counting".
                    //
                    // THE ORIGIN IS THE PELLET'S, NOT THE STAGE'S. A
                    // pellet with an explosion produces two rows and
                    // they are the SAME pellet — `radial` says which
                    // half and `pellet` says whose — so a Laetum
                    // Incarnon's three pellets read as six numbers in
                    // three pairs rather than as three plus three. The engine already settles
                    // them in that order: the stage loop is inside the
                    // pellet loop, so only the LABEL was missing.
                    // A MELEE SWING'S SECOND LANDING IS NOT MULTISHOT.
                    // It rides the same loop, and the column that says
                    // WHY this instance exists must not answer with a
                    // mechanic melee has no card for.
                    origin: if pellet_idx == 0 {
                        crate::record::Origin::Own
                    } else if swing.as_ref().is_some_and(|h| h.hits > 1) {
                        crate::record::Origin::Own
                    } else {
                        crate::record::Origin::Multishot
                    },
                    pellet: Some(pellet_idx + 1),
                    radial: !direct,
                    // THE WEAPON'S OWN DAMAGE, before any bracket —
                    // the layers start here and the first one is the
                    // base-damage bracket. Never `qtotal`, which is the
                    // chain's MIDDLE.
                    base: if (1.0 + base_damage).abs() > 1e-12 {
                        stage_mb / (1.0 + base_damage)
                    } else {
                        stage_mb
                    },
                    layers: pellet_layers(
                        stage_mb,
                        gunco,
                        base_damage,
                        arcane_base_damage,
                        &pre_quantization,
                        if pre_ability_total > 0.0 { qvec.total() / pre_ability_total } else { 1.0 },
                        *beam_merge,
                        &hit_steps,
                        part.multiplier,
                        params.headshot_damage_bonus,
                        part.is_head,
                        cd,
                        active.unmodded_crit_damage,
                        (
                            params.ability_final_at(t) - 1.0,
                            1.0 - co_mult.co_share.clamp(0.0, 1.0),
                        ),
                    ),
                    crit_damage: cd,
                    part: Some(part.name.clone()),
                    head: part.is_head,
                    crit_tier: tier,
                    procs: $procs,
                },
            );
            };
        }
        if killed {
            gal.bump_on_kill(params, t);
            arc.on_kill(params, t);
            if head_direct {
                weakpoint_kill(params, arc, windows, t);
            }
            // The killing instance's procs die with the old
            // individual; the CLOUDS do not — see the note in
            // `field_tick`. What follows hits the fresh spawn, standing
            // in whatever is still burning where it spawned.
            debuffs.on_death(params.acid_shells, &params.foe);
            log_this_pellet!(Vec::new());
            continue;
        }
        // THE EXTRA HIT, off a WEAPON damage instance — the direct
        // pellet and the explosion alike, since both are hits the gun
        // dealt ("Most non-standard weapon hits will trigger an Extra
        // Hit, including Acid Shells and Concealed Explosives").
        //
        // AFTER the kill check, which is the wiki's rule and costs one
        // line here rather than a condition: "If a hit that would
        // trigger an Extra Hit kills the enemy, the Extra Hit will not
        // be triggered."
        //
        // `stage_bracket` is the correction: the extra hit is scaled by
        // the BASE ATTACK's elemental/IPS bracket, and `raw` already
        // carries THIS stage's. They are the same number on the direct
        // hit — the ratio is exactly 1 and nothing moves — and differ on
        // an explosion whose damage type is not the gun's.
        let stage_bracket = if stage_mb > 0.0 { qvec.total() / stage_mb } else { 1.0 };
        let xh_bracket = active.extra_hit_bracket(t, windows) / stage_bracket.max(1e-12);
        // …and the BODY PART, a second time, on a direct hit only. DE's
        // CN card, in the same breath as the faction double-dip: "同理，
        // 弱点倍率也会被计算两次". A radial struck no body part, so
        // `part_factor` is already 1.0 there and this reads as it should.
        if fire_extra_hits(
            raw,
            xh_bracket,
            part_factor,
            head_direct,
            status_chance,
            t,
            &mut *debuffs,
            &mut *gal,
            &mut *arc,
            &mut *target,
            params,
            active,
            &mit,
            &mut *r,
            rec,
            &mut d.status,
        ) {
            // …and the clouds stay where they were. See `field_tick`.
            continue;
        }
        // Per-INSTANCE proc roll (wiki Multishot/Status_Effect):
        // forced ++ SC draws weighted by the QUANTIZED vector, unit
        // immunities renormalized.
        let mut procs = status::procs_for_hit(
            forced,
            status_chance,
            &qvec,
            &params.foe.status_immunities,
            &mut d.status,
        );
        // HUNTER MUNITIONS: a critical hit rolls its OWN Slash status,
        // per pellet, "not affected by the weapon's Status Chance, or
        // damage type distribution, besides being indirectly affected
        // by its Critical Chance" (wiki). So it is a separate draw
        // pushed onto this pellet's proc list rather than anything
        // that touches the status roll above — and a weapon with no
        // Slash in its vector still gets one, which is the whole point
        // of the mod.
        //
        // Pushed HERE rather than applied as a bleed, which makes
        // its damage right for free: a Slash proc is `0.35 x ModdedBase
        // x the PROCCING HIT's crit/part`, so the tier this pellet
        // rolled and the part it struck already scale it.
        //
        // It STACKS with an innate Slash ("but can stack with Slash
        // statuses applied using a weapon's innate status chance") and
        // does not check `procs`. It cannot double a FORCED one, and a
        // weapon-forced Slash is checked here at its source, since
        // `procs` cannot say which Slash came from where; Internal
        // Bleeding's own guard runs after and sees this push.
        if active.slash_on_crit > 0.0
            && tier >= 1
            && !params.forced_procs.contains(&DamageType::Slash)
            && !params
                .foe
                .status_immunities
                .contains(&DamageType::Slash)
            && d.extra.chance(active.slash_on_crit)
        {
            procs.push(DamageType::Slash);
        }
    // Secondary Encumber: on a status this pellet applied, roll
    // ONE extra status of a uniformly random type (13-type pool,
    // independent of the weapon's vector — wiki), at most once per
    // instant (= per trigger pull, and the radial STAGE shares the
    // limit — the wiki names Explosions among the simultaneous
    // attacks that "only proc up to once on a single target").
    //
    // This reproduces the wiki's per-shot rate
    //   1 − (1 − chance × min(statusChance, 1)) ^ pellets
    // exactly, without implementing the min() as a cap: a pellet
    // either applied a status (`!procs.is_empty()`) or it did not, so
    // status chance above 100% guarantees the first proc but cannot
    // give a pellet two shots at Encumber. Trigger scope is
    // health-bar statuses only (U33 patch note) — the only kind a
    // proc list ever holds.
    if params.arcane.encumber_chance > 0.0
        && !*encumber_done
        && !procs.is_empty()
        && d.extra.chance(params.arcane.encumber_chance)
    {
        const POOL: [DamageType; 13] = [
            DamageType::Impact,
            DamageType::Puncture,
            DamageType::Slash,
            DamageType::Heat,
            DamageType::Cold,
            DamageType::Electricity,
            DamageType::Toxin,
            DamageType::Blast,
            DamageType::Corrosive,
            DamageType::Magnetic,
            DamageType::Viral,
            DamageType::Gas,
            DamageType::Radiation,
        ];
        let idx = (d.extra.next_f64() * POOL.len() as f64) as usize % POOL.len();
        procs.push(POOL[idx]);
        *encumber_done = true;
    }
    // Internal Bleeding / Hemorrhage: one roll per damage INSTANCE
    // when a `from` status landed and no `to` status did; chance ×2
    // while the LIVE fire rate is strictly below 2.5.
    //
    // The `!procs.contains(to)` guard is the whole stacking rule, and
    // it is STRICTER than Hunter Munitions': this one "cannot produce
    // multiple procs in a single instance of damage alongside ANY
    // other Slash sources, such as a weapon's innate Slash, Hunter
    // Munitions, or the debuff from Seeking Talons" (wiki) — innate
    // Slash included, which is why it reads `procs` rather than
    // `forced_procs`. Hunter Munitions pushes above, so it is already
    // in `procs` here and this roll is skipped, which is exactly
    // "if both proc at the same time, only 1 slash proc is applied".
    roll_proc_conversion(active, d, live_rate, &mut procs);
    // Overwhelming Attrition's TRIGGER, evaluated once the proc
    // list is final: "On Hit that is neither Critical nor applies
    // a Status Effect" (wiki). PER DAMAGE INSTANCE — measured
    // (MEASUREMENTS M11: one shot into a crowd fills all 3 stacks,
    // and one shot at a LONE target grants exactly 2 — the direct
    // hit and the explosion each arm it). So a shot whose direct hit
    // and whose explosion are both plain arms the buff twice,
    // bounded by the stack cap.
    // STRIKING SUCCESSION: "On Hit", with no qualifier at all — so it
    // is armed by the same damage instance the line below inspects, and
    // simply does not read what the instance did. Per instance for the
    // same measured reason (M11): a direct hit and its explosion are
    // two.
    bump_buffs!(params, buff_stacks, rec_buff_index, rec, crate::model::BuffTrigger::Hit, t, d.extra);
    if tier == 0 && procs.is_empty() {
        bump_buffs!(params, buff_stacks, rec_buff_index, rec, crate::model::BuffTrigger::PlainHit, t, d.extra);
    }
    // STORMBURST: the condition is on the TARGET, read here where the
    // debuffs are in hand. Bumped AFTER this pull's multishot was
    // rolled, so the hit that earns a stack does not fire it — the same
    // rule every other stacking buff in this loop follows.
    bump_status_buffs!(params, buff_stacks, &*debuffs, t, d.extra);
    // ...and ARM it for the next pellet. ONE roll per pellet that
    // landed at least one status — "Applying multiple status effects in
    // a single hit does not increase the chance for the effect" — and
    // per PELLET rather than per trigger pull, since the card says it
    // triggers "separately for each bullet when using Multishot".
    //
    // Rolled BEFORE `settle_procs` consumes `procs`, and after the spend
    // above: a pellet that spends the buff can re-arm it with its own
    // status, which is what makes a high-status weapon hold it up.
    if let Some(sc) = params.super_crit_on_status {
        if !procs.is_empty() && d.extra.chance(sc.chance) {
            *super_crit_armed = true;
        }
    }
    // PARAGON ESSENCE: "On Status Effect", one stack per status that
    // LANDS. Read literally — the card names the effect, not the hit —
    // so a pellet that procs twice earns two. Bumped before
    // `settle_procs` consumes the list, and only here: this is where a
    // PELLET's own statuses land, and a field tick's or an extra hit's
    // are a different sentence that no card in the roster has written
    // yet.
    for _ in 0..procs.len() {
        bump_buffs!(params, buff_stacks, rec_buff_index, rec, crate::model::BuffTrigger::StatusApplied, t, d.extra);
    }
    // INDEPENDENT PROCS land on the HIT, not on the roll — that is what
    // "independent from damage" means, and it is why this is here
    // rather than inside `settle_procs`: a weapon that lifts lifts with
    // no status chance at all, and a field tick or an Extra Hit that
    // borrows this weapon's vector does NOT lift, because it is not
    // this attack landing.
    //
    // Against one target at the centre the direct and the radial of the
    // Mausolon's laser always arrive together, so no distinction is
    // drawn between which of the two lifts.
    if active.independent_procs.contains(&"lifted") {
        // A STATE, so it REFRESHES: the later expiry wins rather than
        // the count going up.
        let until = t + LIFTED_SECONDS * params.status_duration_multiplier;
        debuffs.lifted = Some(debuffs.lifted.map_or(until, |e| e.max(until)));
    }
    // …AND THE SWING'S OWN, which a stance marks per attack. Same rule
    // as the weapon-level pair one line up: applied on the HIT rather
    // than through the roll, and refreshed rather than stacked.
    if swing_forced_independent.contains(&"lifted") {
        let until = t + LIFTED_SECONDS * params.status_duration_multiplier;
        debuffs.lifted = Some(debuffs.lifted.map_or(until, |e| e.max(until)));
    }
    if swing_forced_independent.contains(&"knockdown") {
        let until = t + KNOCKDOWN_SECONDS * params.status_duration_multiplier;
        debuffs.knockdown = Some(debuffs.knockdown.map_or(until, |e| e.max(until)));
    }
    if active.independent_procs.contains(&"knockdown") {
        let until = t + KNOCKDOWN_SECONDS * params.status_duration_multiplier;
        debuffs.knockdown = Some(debuffs.knockdown.map_or(until, |e| e.max(until)));
    }
        // …AND THE ORDINARY END OF A PELLET, which does get to name
        // what it applied.
        log_this_pellet!(procs.clone());
        // …AND WHAT MELEE INFLUENCE WILL COPY, taken before
        // `settle_procs` consumes the list. Cloned only when the arcane
        // is equipped: this is the hot loop and every other build pays
        // an empty `Vec`.
        let aimed_procs = if params.arcane.influence_chance > 0.0 {
            procs.clone()
        } else {
            Vec::new()
        };
        settle_procs(
            procs,
            t,
            // THE HIT'S ATTRITION ROLL TRAVELS WITH ITS STATUSES. A proc's
            // magnitude is the applying instance's — which is why
            // `crit_multiplier` is already here — and Devouring/Devastating
            // Attrition is a per-instance multiplier of exactly that shape,
            // so a DoT applied by a 21x hit ticks for 21x.
            //
            // Measured through the Debilitate chain: the
            // final DoT eats, i.e. 21x21. Two layers for three faction
            // layers — the split instance rolls one, and the other can
            // only be the applying hit's, carried here. A DoT is not a
            // hit, so it never rolls one of its own. MEASUREMENTS M37.
            InstanceScale {
                mb_live,
                crit_multiplier,
                part_factor,
                landing,
                attrition,
                // The BASE ATTACK's, so a Blast stack this instance applies
                // remembers the bracket its detonation's extra hit takes —
                // not this stage's, which the detonation itself never gets.
                xh_bracket: active.extra_hit_bracket(t, windows),
            },
            &mut *debuffs,
            &mut *gal,
            &mut *arc,
            &mut *target,
            params,
            active,
            &mit,
            &mut *r,
            rec,
            &mut d.status,
            &params.foe,
            DEPTH_PROC,
        );
        // MELEE INFLUENCE — every eligible status this swing applied,
        // arriving on everything standing around the body that took it.
        //
        // THE WHOLE SWING SEEDS IT, aimed body and Follow Through
        // alike: *"Melee Influence only triggers from direct melee
        // strikes"*, and a swing reaching past the first body is one of
        // those. A body the swing KILLED seeds nothing — *"hits that
        // one-hit-kill enemies cannot trigger nor benefit"*, which the
        // wiki files under Bugs and which is the behaviour all the same.
        if params.arcane.influence_chance > 0.0 {
            influence_seeds.push((0, Landed { procs: aimed_procs, raw, killed: false }));
            // THE SPREAD READS THE WINDOW THIS SWING FOUND OPEN, and the
            // grant below is rolled after: the buff is what an
            // Electricity status GRANTS, so the swing that grants it has
            // already resolved. Nothing published says the granting hit
            // also spreads, and assuming it would invent a copy of the
            // swing.
            let open = t < *influence_until;
            let scale = InstanceScale {
                // THE STATUS IS THE SWING'S OWN, one derivation further
                // out — so it burns off the base the swing's statuses
                // burn off, and only the faction rung differs. The wiki
                // checks it from that side: an ordinary Electricity proc
                // of 228 spreads as 353, and 353 / 228 is exactly one
                // more faction multiplier.
                mb_live,
                crit_multiplier,
                part_factor,
                landing,
                attrition,
                xh_bracket: active.extra_hit_bracket(t, windows),
            };
            if open {
                for (from, landed) in &influence_seeds {
                    if landed.killed {
                        continue;
                    }
                    let carried: Vec<(DamageType, f64)> = landed
                        .procs
                        .iter()
                        .copied()
                        .filter(|ty| influence_can_spread(*ty))
                        .map(|ty| (ty, landed.raw * shares.share(ty)))
                        .collect();
                    spread_from_influence(
                        body_at,
                        &mut *others,
                        &mut *target,
                        &mut *debuffs,
                        params,
                        active,
                        *from,
                        &carried,
                        scale,
                        params.arcane.influence_radius_m,
                        &mut *gal,
                        &mut *arc,
                        &mut *r,
                        rec,
                        d,
                        t,
                    );
                }
            }
            // …AND THE ROLL THAT OPENS IT, off any Electricity status
            // this swing landed. One roll for the swing rather than one
            // per body: the card is *"On Melee Electricity Status"* and
            // a window that is already open cannot be refreshed, so a
            // second success in the same instant buys nothing anyway.
            if !open
                && influence_seeds.iter().any(|(_, l)| {
                    !l.killed && l.procs.contains(&DamageType::Electricity)
                })
                && d.extra.chance(params.arcane.influence_chance)
            {
                *influence_until = t + params.arcane.influence_seconds;
            }
        }
    }
}
