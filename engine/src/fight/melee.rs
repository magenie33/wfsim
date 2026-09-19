use super::*;

/// POINTS ON THE MELEE COMBO COUNTER PER TIER above the first.
///
/// The wiki publishes the ladder rather than the formula — 2x at 20 hits, 3x at
/// 40, one more every 20 up to 12x at 220 — and this is that ladder as
/// arithmetic. Venka Prime's 13x and Dex Nikana's shortened 110 are the two
/// exceptions the page names and neither is in the roster; when one arrives it
/// states its own numbers rather than bending this.
pub(super) const MELEE_COMBO_POINTS_PER_TIER: f64 = 20.0;

/// The cap. *"you will also increment a Melee Combo Multiplier from 2x to 12x"*.
pub(super) const MELEE_COMBO_MAX: f64 = 12.0;

/// TENNOKAI'S BASE CHANCE, per landed direct hit.
///
/// *"Landing direct melee hits ... have a 15% chance of flashing a sword icon
/// resembling the Skana Prime on the reticle for 2 seconds"* (wiki, Melee).
pub(super) const TENNOKAI_BASE_CHANCE: f64 = 0.15;

/// …AND HOW LONG THE FLASH LASTS, from the same sentence. Opportunity's Reach
/// replaces it with 4.0.
pub(super) const TENNOKAI_WINDOW_SECONDS: f64 = 2.0;

/// POINTS A HEAVY ATTACK'S FLOOR REFILLS AT, per second.
///
/// *"Heavy attacks spend initial combo, which regenerates at a rate of 40 combo
/// points per second"* (wiki, Melee Combo). It is what makes a pure-heavy build
/// work at all: a Magistar in Incarnon Form carries +30, which is back inside
/// 0.75 s against a 1.07 s cycle, so every heavy lands at 2x rather than 1x.
pub(super) const INITIAL_COMBO_REGEN_PER_SECOND: f64 = 40.0;

/// THE MULTIPLIER THE COUNTER IS WORTH RIGHT NOW.
///
/// `1 + floor(points / 20)`, capped at 12. One at 0..19 points, which is the
/// wiki's own table read as a step function — and 1x is a real state rather
/// than "no combo": a heavy attack at 1x deals its class multiplier and nothing
/// more.
pub(super) fn melee_combo_multiplier(points: f64) -> f64 {
    (1.0 + (points / MELEE_COMBO_POINTS_PER_TIER).floor()).clamp(1.0, MELEE_COMBO_MAX)
}

/// THE COUNTER AS THIS SWING SEES IT: what was earned, or the floor, whichever
/// is higher.
///
/// The floor is `initial_combo` filling at 40 points a second since the last
/// heavy attack emptied the counter — so it is a FLOOR rather than a second
/// pool, which is what makes "spend it and it comes back" and "build on top of
/// it" the same number.
pub(super) fn melee_combo_points(held: f64, initial: f64, since_spend_seconds: f64) -> f64 {
    // IT REGENERATES FROM WHAT IS THERE, not from zero: *"Heavy attacks spend
    // initial combo, which REGENERATES at a rate of 40 combo points per
    // second"*. Heavy Attack Efficiency is what makes the difference visible —
    // it leaves points behind, and 40 a second climbs from those toward the
    // floor rather than racing them from nothing.
    //
    // ABOVE THE FLOOR IT ONLY WAITS. Points earned past the initial-combo value
    // are not regeneration's business; they stand until the counter's own clock
    // takes them.
    let regen = since_spend_seconds.max(0.0) * INITIAL_COMBO_REGEN_PER_SECOND;
    held.max(initial.min(held + regen))
}

/// KILLING BLOW'S BRACKET — what a `+X% Melee Damage on Heavy Attack` card is
/// worth, as a term in the BASE-DAMAGE bucket.
///
/// *"Damage bonus is additive to mods such as Pressure Point"* (wiki, Killing
/// Blow), so it is diluted by everything else in that bucket rather than
/// multiplying the finished swing. SEISMIC WAVE IS THE CONTRAST and the reason
/// the two cannot share a line: *"Slam damage bonus is multiplicative to base
/// damage (e.g. Pressure Point)"* (wiki, Seismic Wave). Both cards read "+X%
/// Melee Damage on <kind of attack>" and they land in different places.
///
/// Zero on every mode that does not spend the counter, which is what "heavy
/// attack" means here.
pub(super) fn heavy_attack_base_damage(ap: &FightParams) -> f64 {
    if ap.spends_combo { ap.heavy_attack_damage } else { 0.0 }
}

/// HOW OFTEN A HEAVY MODE SWINGS, in seconds — and it is not always as soon as
/// it can.
///
/// A HEAVY ATTACK SPENDS THE COUNTER, so the swing is worth waiting for when
/// the counter is about to be worth more. The counter climbs in STEPS
/// (`1 + floor(points / 20)`, refilling at 40 a second), so the candidates are
/// the animation's own floor and each rung the initial-combo floor can still
/// reach — and waiting PAST a rung buys nothing, which is why "wait for the
/// full refill" is the intuitive rule and the wrong one.
///
/// IT IS DERIVED FROM SPENDING THE COUNTER, not from the mode: a heavy slam
/// pays nothing for the wait because its climb was free time anyway, and a
/// standing heavy pays it as idle seconds. Both are the same decision.
///
/// MULTIPLIER PER SECOND IS THE PROXY, and it UNDER-states a wait: Blood Rush
/// reads the same counter and is not in it.
pub(super) fn heavy_cycle_seconds(floor_seconds: f64, earned: f64, initial: f64) -> f64 {
    let per_second = |t: f64| melee_combo_multiplier(melee_combo_points(earned, initial, t)) / t;
    let mut best = floor_seconds.max(1e-6);
    let mut best_rate = per_second(best);
    let rungs = (initial / MELEE_COMBO_POINTS_PER_TIER).floor().max(0.0) as u32;
    for k in 1..=rungs {
        let t = f64::from(k) * MELEE_COMBO_POINTS_PER_TIER / INITIAL_COMBO_REGEN_PER_SECOND;
        if t > best && per_second(t) > best_rate {
            best_rate = per_second(t);
            best = t;
        }
    }
    best
}

/// HOW MANY TIMES ONE SWING LANDS — the stance row's count, or ONE where the
/// Tennokai window has replaced the swing with a heavy attack.
///
/// *"Performing a Heavy Attack or Heavy Slam during this flash"* is what the
/// window buys, so the light swing does not happen: a heavy attack's multiplier
/// is the CLASS's whole total, and multiplying it by a stance row's hit count
/// would pay the same swing five times over on a Rogue Edict spin.
pub(super) fn swing_instances(hits: u32, tennokai_heavy: bool) -> u32 {
    if tennokai_heavy {
        1
    } else {
        hits.max(1)
    }
}

/// WHAT ONE INSTANCE LEFT ON A BODY, for the one mechanism that has to know.
///
/// Melee Influence spreads from every body a SWING struck rather than from the
/// aimed one alone, so a Follow Through instance has to report what it applied
/// and what it was worth. Every other caller of [`spread_hit`] drops it.
#[derive(Debug, Default, Clone)]
pub(super) struct Landed {
    pub(super) procs: Vec<DamageType>,
    /// The instance's own damage before mitigation — what a share of it is
    /// taken from.
    pub(super) raw: f64,
    pub(super) killed: bool,
}

/// WHAT ONE HIT EARNS THE COUNTER (MEASUREMENTS M96, M97). `points` is what the
/// row shows and `base` how many of them are base points, the unit every chance
/// acts on; `chance` is Additional Combo Count Chance and `gain` is Chance to Gain
/// Combo Count (0, or a malus). Each base point, carrying its share of `points`:
/// - survives the gain roll (`1 + gain`), or is lost with its share;
/// - pays its share once more per whole 100% of `chance`;
/// - and rolls what is left of `chance` for one more point.
///
/// NO ROLL IS SPENT where neither chance is set, so a build without them draws
/// the same random stream it always did.
pub(super) fn swing_combo_gain(points: f64, base: f64, chance: f64, gain: f64, roll: &mut impl FnMut(f64) -> bool) -> f64 {
    if base <= 0.0 {
        return 0.0;
    }
    let share = points / base;
    let chance = chance.max(0.0);
    let whole = chance.floor();
    let rest = chance - whole;
    let keep = (1.0 + gain).clamp(0.0, 1.0);
    let mut out = 0.0;
    for _ in 0..(base.round() as u32) {
        if keep < 1.0 && !roll(keep) {
            continue;
        }
        out += share * (1.0 + whole);
        if rest > 0.0 && roll(rest) {
            out += 1.0;
        }
    }
    out
}

/// A HIT HOLDS THE COUNTER ONLY IF IT EARNED SOMETHING: one that came to 0 points
/// leaves the combo timer running down (MEASUREMENTS M97). Asked of a swing that
/// earns at all; a heavy attack keeps the refresh it had.
pub(super) fn refreshes_combo_timer(landed: f64, earns: bool, gained: f64) -> bool {
    landed > 0.0 && (!earns || gained > 0.0)
}

/// THE FILL RULE for a row's `combo_points`, and only that: the fight reads each
/// row's own number (see notes: combo_points_from_multiplier). The multiplier
/// ROUNDED UP, once per instance — *"100% = 1 point"* (wiki, Melee Combo), and a
/// swing under 100% still earns one: Rogue Edict opens `200%` then `5x 50%` and
/// the counter shows SEVEN, against 4.5 proportional and 6 flat. Rounded rather
/// than floored at one: the two agree wherever this roster can tell them apart.
#[cfg(test)]
pub(super) fn combo_points_for(multiplier: f64, instances: f64) -> f64 {
    multiplier.ceil().max(1.0) * instances
}

/// THE MELEE COMBO COUNTER AND TENNOKAI — what one fight carries from swing to
/// swing. A gun never reads it.
pub(super) struct MeleeState {
    /// POINTS, not tiers. *"Stance attacks add combo points, scaling with the
    /// attack's stance damage multiplier (100% stance damage multiplier = 1
    /// point)"*, and the tier is `1 + floor(points / 20)` capped at 12 — see
    /// `melee_combo_multiplier`.
    /// ONE COUNTER, TWO READERS THAT WANT OPPOSITE THINGS. A heavy swing SPENDS
    /// it as a damage multiplier; Blood Rush and Weeping Wounds read it as a
    /// bracket term and never touch it. That is the whole reason the seven melee
    /// forms are seven builds.
    pub(super) combo_points: f64,
    /// THE KILL COUNT AT THE LAST SWING, so the kills since are what Rage is paid.
    pub(super) rage_kill_mark: u32,
    /// WHEN THE COUNTER DIES with nothing added to it. Refreshed by any landed
    /// swing; five seconds on almost every weapon.
    pub(super) combo_expiry: f64,
    /// WHEN THE COUNTER WAS LAST EMPTIED BY A HEAVY ATTACK, which is what the
    /// initial-combo floor regenerates from.
    /// THE FIGHT OPENS WITH THE FLOOR FULL: *"Initial Combo grants a minimum
    /// value of combo points when IDLE or after a combo reset. Heavy attacks
    /// spend initial combo, which regenerates at a rate of 40 combo points per
    /// second"* (wiki, Melee Combo). The 40 a second is what a heavy attack owes
    /// back, not what a player walks in owing — so a build carrying +30 opens
    /// its first heavy at 2x rather than reaching it 0.75 s in.
    pub(super) combo_spent_t: f64,
    /// WHICH SWING OF THE SCRIPT IS NEXT. A gun leaves the script empty and
    /// never reads this.
    pub(super) swing_idx: usize,
    /// A window a landed hit opens, in which a HEAVY attack costs no combo. The
    /// owner settled what to do with it in one clause — use it the moment it
    /// fires — so the loop takes the very next swing rather than inventing a
    /// policy.
    /// TWO NUMBERS AND NOTHING ELSE: when the window closes, and how many hits
    /// have landed since the last one opened (Discipline's Merit replaces the
    /// roll with "every 4 hits", which is the one card that makes the count
    /// load-bearing).
    pub(super) tennokai_until: f64,
    /// WAS THIS WINDOW OPENED BY A TENNOKAI KILL? Truth's Flame pays its damage
    /// only in one that was: *"the damage bonus is only active following the
    /// first kill"*, so the swing that earns the chain does not carry it.
    pub(super) tennokai_chained: bool,
    pub(super) tennokai_hits: u32,
}
