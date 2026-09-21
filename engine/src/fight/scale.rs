use super::*;

/// One instance's damage scaling, as the status payloads need it.
#[derive(Debug, Clone, Copy)]
pub(super) struct InstanceScale {
    /// Live ModifiedBase (base-damage bucket applied).
    pub(super) mb_live: f64,
    /// The instance's crit multiplier (1.0 when it did not crit).
    pub(super) crit_multiplier: f64,
    /// Body-part multiplier — always 1.0 for a radial or a field.
    pub(super) part_factor: f64,
    /// What an Electricity or Gas tick is worth where THIS instance struck —
    /// [`Dot::landing`]. 1.0 off a head.
    pub(super) landing: f64,
    /// LEADED GAS' element bonus AS THIS INSTANCE CARRIED IT — `(element,
    /// share of the modified base)` while the weak-point window was open, and
    /// `None` the rest of the time.
    ///
    /// IT TRAVELS WITH THE INSTANCE for the reason the bracket below does: a
    /// status is settled where the weapon's live windows are no longer in
    /// scope, and an element bonus belongs in that element's DoT bracket.
    ///
    /// SNAPSHOT AT THE PROC, which is this engine's rule for every mod-granted
    /// element bonus (`elem_dot_bonus` is resolved once). The game re-reads it
    /// per tick — the wiki says the bonus reaches clouds made before the buff
    /// and stops reaching them when it lapses — so a cloud that outlives the
    /// window is worth more here than in game, and one made just before the
    /// window opens is worth less.
    pub(super) weakpoint_element: Option<(crate::rules::damage::DamageType, f64)>,
    /// DEVOURING/DEVASTATING ATTRITION on THIS instance, or 1.0.
    ///
    /// 1.0 everywhere but the Primary Debilitate split, and that is the whole
    /// claim: an ordinary status DoT is a tick of an effect, while the arcane's
    /// split is a damage INSTANCE the wiki names as one — so it rolls the
    /// per-instance multipliers a hit rolls, this one included.
    /// MEASUREMENTS M37.
    pub(super) attrition: f64,
    /// The EXTRA HIT bracket of the weapon that fired this instance —
    /// `1 + Σ elemental bonuses + Σ (base-attack IPS share × that IPS bonus)`,
    /// read off the ACTIVE FORM's base attack (`FightParams::extra_hit_bracket`).
    ///
    /// It travels with the instance for ONE reason: a Blast stack detonates
    /// 1.5 s later, in `process_ticks`, where nothing about the weapon is in
    /// scope any more — and the extra hit that fires off that detonation is
    /// multiplied by this bracket even though the detonation itself takes no
    /// elemental bonus at all. Snapshotting it at application time is also what
    /// makes an ability-granted element (Nourish) that has since EXPIRED still
    /// count for a stack applied while it was up, the same rule `value`
    /// already follows.
    pub(super) xh_bracket: f64,
}

/// The GunCO-family bracket for one damage instance — MECHANICS §6. Every
/// source contributes rate × its TARGET counter, is scaled by the
/// original-base fraction, and combines per the weapon's [`CoBehavior`].
///
/// Shared by the direct hit and the lingering FIELD, because the CO catalog
/// puts the cloud on the SAME rate and behavior as the main fire — the Torid's
/// two rows are both 100%, Multiplying.
///
/// The counter is read HERE rather than snapshotted when the field spawned —
/// WHAT THE CONDITION OVERLOAD BRACKET IS, and what it is MADE OF.
///
/// THE QUOTIENT ALONE IS A FICTION AS A FACTOR: on an `Adding` weapon the
/// bracket is `(1 + base + arcane + co + half-health) / (1 + base)`, a ratio of
/// the base bracket to itself — exactly right as arithmetic and the reason a
/// panel showing it printed `x5.706 Condition Overload`, a number the game does
/// not have. The terms travel with it, so the ledger shows the sum.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Gunco {
    /// What the engine multiplies by — unchanged, and still a quotient.
    pub(super) bucket: f64,
    /// Condition Overload's own contribution to the base bracket.
    pub(super) co: f64,
    /// …and the two numbers behind it, for a reader checking a card.
    pub(super) rate: f64,
    pub(super) types: f64,
    /// The below-half-health bonus, which shares this bracket.
    pub(super) half_hp: f64,
    /// WHAT SHARE OF [`Self::bucket`] CONDITION OVERLOAD PUT THERE — 0 with no
    /// source equipped, and the whole of what ECLIPSE must not multiply
    /// (MEASUREMENTS M79). Computed here because this is the one place that
    /// holds both the term and the bracket it went into.
    pub(super) co_share: f64,
}

/// ECLIPSE, AS THE FACTOR IT ACTUALLY IS — its bonus on everything the hit is
/// made of EXCEPT the share Condition Overload put in the base bracket.
///
/// MEASURED (M79): a Magistar reading 1644 under a 30% Eclipse with one CO
/// stack only comes out at `weapon x swing x (1 + 0.3 + 0.8) + flat x 1.3`.
/// Eclipse multiplying the whole bracket reads 1808.
///
/// The reading cannot tell "a multiplier the CO term does not see" apart from
/// "a term in the same bracket as CO" — they are the same arithmetic — so this
/// keeps the page's own word for it ("an unique multiplier") and takes the CO
/// share out of it, which is the smaller of the two claims.
pub(super) fn eclipse_at(mult: f64, co_share: f64) -> f64 {
    1.0 + (mult - 1.0) * (1.0 - co_share.clamp(0.0, 1.0))
}

/// Pox's row in the same catalog: "Damage recalculates on every tick".
#[allow(clippy::too_many_arguments)]
pub(super) fn gunco_bucket(
    params: &FightParams,
    ap: &FightParams,
    debuffs: &mut DebuffState,
    gal: &mut GalStacks,
    at: f64,
    base_damage: f64,
    arcane_base_damage: f64,
    arc_ratio: f64,
    // The below-half-health bonus, routed HERE rather than into `arcane_base_damage`
    // because its bracket is the weapon's CO bracket — see the call site.
    half_hp: f64,
    // What CO reads, PAIRED with the base it is a share of. The direct hit's
    // lives on `ap`; an explosion carries its own, because an evolution can
    // raise what the explosion deals without raising what CO reads — and the
    // pair is what keeps one stage's absolute off another stage's denominator.
    co_base: crate::model::CoBase,
    // WHICH PART IS BEING RESOLVED. Checked against the tag the base carries,
    // so a new stage cannot inherit another's denominator by being written next
    // to it: the assertion fires until the author either builds that stage its
    // own pair or says `borrowed_for` and means it.
    stage: crate::model::CoStage,
) -> Gunco {
    debug_assert_eq!(
        co_base.stage(),
        stage,
        "a {stage:?} stage was handed a {:?} CO base — build its own pair, or say borrowed_for",
        co_base.stage(),
    );
    let co_rate = ap.co_per_type
        + params
            .co_stack
            .as_ref()
            .map_or(0.0, |s| s.per_stack * gal.co.current(at, s.duration) as f64);
    let cold = debuffs
        .cold_status_count(at)
        .min(params.arcane.cold_cap);
    let gunco_total = [
        (co_rate, debuffs.distinct_statuses() as u32),
        (params.arcane.per_cold_base_damage, cold),
    ]
    .iter()
    .map(|(rate, count)| rate * *count as f64)
    .sum::<f64>()
        * co_base.fraction();
    // THE HALF-HEALTH BONUS SHARES THIS BRACKET, so it shares its base fraction
    // too. The Dread's page spells out all three halves of that: its conditional
    // bonus "ignores the base damage increase from the same perk, the 2x damage
    // from the charged shot (Primary Fire only) and Galvanized Aptitude's damage
    // bonus" — the second is `co_base_fraction` (0.5 on a bow's charged entry),
    // and the third falls out of being CO's SIBLING here rather than nested
    // inside it.
    let half_hp = half_hp * co_base.fraction();
    let terms = |bucket: f64, numerator: f64| Gunco {
        bucket,
        co: gunco_total,
        rate: co_rate,
        types: debuffs.distinct_statuses() as f64,
        half_hp,
        co_share: if numerator > 0.0 { (gunco_total / numerator).clamp(0.0, 1.0) } else { 0.0 },
    };
    match ap.co_behavior {
        // Joins the base-damage bucket: diluted by Hornet Strike, sharing the
        // bracket with the arcane's bonus.
        crate::model::CoBehavior::AdditiveWithBaseDamage => {
            let numerator = 1.0 + base_damage + arcane_base_damage + gunco_total + half_hp;
            terms(numerator / (1.0 + base_damage), numerator)
        }
        crate::model::CoBehavior::Independent => {
            let numerator = 1.0 + gunco_total + half_hp;
            terms(arc_ratio * numerator, numerator)
        }
        // No CO bracket to join, so the ordinary one: the base-damage bucket.
        crate::model::CoBehavior::Inert => terms(
            arc_ratio * (1.0 + base_damage + half_hp) / (1.0 + base_damage),
            0.0,
        ),
    }
}
