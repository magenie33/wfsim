use super::*;

/// Credit one weapon-damage instance to its bucket's per-type split.
///
/// By each component's SHARE of the instance's vector: exact wherever a pool
/// takes the whole hit, approximate in one place — while shields are up Toxin
/// bypasses them and its siblings do not, so a proportional split cannot see
/// that they were mitigated differently. The bucket TOTAL is unaffected. The
/// vector is the QUANTIZED one, so the shares are the ones that landed:
/// Corrosive 164.73 / Magnetic 52.02 (76/24) snaps to exactly 75/25.
/// Split one instance's EFFECTIVE damage across the types that made it.
///
/// The share is each component's contribution AFTER the vulnerability column,
/// the same weights that produced `effective` — splitting by the raw vector
/// reports a 50/50 Impact/Slash hit on a Grineer unit as 50/50 when Impact did
/// 60% of it.
/// IS ANYONE READING? The gate `TargetState::apply` fills its breakdown behind.
///
/// TWO CONSUMERS, ONE ANSWER: the replay's floating numbers and the combat
/// record explain the same instance, so they are recorded together or not at
/// all — otherwise they are two accounts of one hit.
pub(super) fn watching<'a>(
    rec: &crate::record::Record,
    breakdown: &'a mut Breakdown,
) -> Option<&'a mut Breakdown> {
    rec.is_on().then_some(breakdown)
}

/// What a layer leaves the running total at.
pub(super) fn layer_out(l: &crate::record::Layer) -> f64 {
    use crate::record::Layer::*;
    match l {
        Bracket { out, .. } | Quantize { out, .. } | Mul { out, .. } | Sum { out, .. } => *out,
    }
}

/// THE FULL LEDGER OF A PELLET, in the GAME's order rather than the engine's.
///
/// The engine evaluates in a cheaper order and is entitled to: a non-elemental
/// base bonus multiplies the quantization numerator AND its scale, so it
/// commutes with the snap — which is exactly why Condition Overload can be
/// applied after quantization and still be right. What that licence does NOT
/// license is printing the rearrangement as if it were the formula, and that is
/// what `x5.706 Condition Overload` was: `(1 + base + co) / (1 + base)`, a
/// quotient with a multiplication sign in front of it.
///
/// So the bracket is a BRACKET here, with its terms, and the engine's quotient
/// never reaches the page. The two agree on the product, which is the property
/// `check_combat_record` already asserts of every row.
#[allow(clippy::too_many_arguments)]
pub(super) fn pellet_layers(
    stage_mb: f64,
    gunco: Gunco,
    base_damage: f64,
    arcane_base_damage: f64,
    // The element hierarchy and the quantization it is rounded through, as one
    // factor for now — the snap's own per-component table is the next step.
    // The vector BEFORE quantization, and the base it was quantized against —
    // the snap is a per-component rounding and the only way to draw it as a
    // multiplier was the ratio of its two totals.
    pre_quantization: &DamageVector,
    ability_elements: f64,
    beam_merge: f64,
    steps: &[crate::record::Step],
    part_multiplier: f64,
    headshot_bonus: f64,
    is_head: bool,
    crit_damage: f64,
    crit_damage_base: f64,
    // ECLIPSE'S TWO NUMBERS — the ability's own bonus, and the share of the
    // hit it reaches. `(0.0, 1.0)` where no ability is up, which draws nothing.
    eclipse: (f64, f64),
) -> Vec<crate::record::Layer> {
    use crate::record::{Factor, Layer, Term};
    // THE WEAPON'S OWN DAMAGE, before any bracket. `stage_mb` is it times the
    // base-damage bucket, so the bucket divides back out — and this is the one
    // division the ledger does, to find where the chain STARTS rather than to
    // invent a factor in the middle of it.
    let base = if (1.0 + base_damage).abs() > 1e-12 { stage_mb / (1.0 + base_damage) } else { stage_mb };

    let mut terms: Vec<Term> = Vec::new();
    let mut push = |factor: Option<Factor>, value: f64, of: Option<(f64, f64)>| {
        if value.abs() > 1e-12 {
            terms.push(Term { factor, value, of });
        }
    };
    // UNNAMED, because the engine holds a SUM here and not the cards in it.
    push(None, base_damage, None);
    // …and NAMED, because this one is exact: Condition Overload is the
    // mechanic, and the two numbers behind it are its rate and the count it
    // read.
    push(Some(Factor::ConditionOverload), gunco.co, Some((gunco.rate, gunco.types)));
    push(None, arcane_base_damage, None);
    push(None, gunco.half_hp, None);

    let mut at = base;
    let mut out: Vec<Layer> = Vec::new();
    // AN EMPTY BRACKET IS NOT A LAYER. That is where the pile of `x1.00` went:
    // a bracket with nothing in it has nothing to say, so it is not drawn.
    if !terms.is_empty() {
        let sum = 1.0 + terms.iter().map(|t| t.value).sum::<f64>();
        at = base * sum;
        out.push(Layer::Bracket { factor: Factor::BaseDamageBracket, terms, sum, out: at });
    }
    // THE SNAP, drawn as a snap. Each component goes to the nearest multiple of
    // `ModdedBase / 32` (`DamageVector::quantized_against`), so the only way to
    // fit it into a chain of multipliers was the ratio of the two totals — the
    // same fiction as the bracket above, in a different place.
    //
    // IT IS RECOMPUTED AGAINST THE BRACKET'S OWN OUT rather than against the
    // engine's ModdedBase, and the two are exactly proportional: quantizing
    // `kX` against `ks` is `k` times quantizing `X` against `s`. So this is the
    // GAME's grid, on the game's base, and it lands on the same number.
    let scale = at / crate::damage::QUANTIZATION_DENOMINATOR;
    if scale > 0.0 && pre_quantization.total() > 0.0 {
        let k = if stage_mb > 0.0 { at / stage_mb } else { 1.0 };
        let mut components = Vec::new();
        let mut total = 0.0;
        for (dtype, amount) in pre_quantization.iter_nonzero() {
            let before = amount * k;
            let units = before / scale;
            let after = units.round() * scale;
            total += after;
            components.push(crate::record::Snap { dtype, before, units, after });
        }
        if !components.is_empty() {
            at = total;
            out.push(Layer::Quantize { scale, components, out: at });
        }
    }
    for (factor, v) in [
        (crate::record::Factor::WarframeAbilityElement, ability_elements),
        (crate::record::Factor::MergedBeams, beam_merge),
    ] {
        if (v - 1.0).abs() > 1e-12 {
            at *= v;
            out.push(Layer::Mul { factor, value: v, of: Vec::new(), head: v, out: at });
        }
    }
    for (factor, v) in steps {
        // CONDITION OVERLOAD IS NOT HERE — it is a term of the bracket above,
        // and a weapon whose catalog row says `Multiplying` gets its own layer
        // rather than sharing this one.
        if *factor == Factor::ConditionOverload || (v - 1.0).abs() <= 1e-12 {
            continue;
        }
        at *= v;
        // A BODY PART IS ITSELF A BRACKET: `3.00 x (1 + 0.50 headshot damage)`,
        // and the pair is what a reader checks against the enemy card
        // (MEASUREMENTS M60 — headshot bonuses ADD).
        // A BODY PART IS ITSELF A BRACKET, and a CRIT is itself a formula —
        // `1 + tier x (crit damage - 1)` where the crit damage is a sum of its
        // own. Both are drawn open rather than as one number a reader has to
        // take on trust.
        let of = if *factor == Factor::BodyPart && is_head && headshot_bonus.abs() > 1e-12 {
            vec![Term { factor: Some(Factor::HeadshotDamage), value: headshot_bonus, of: None }]
        } else if *factor == Factor::WarframeAbility
            && eclipse.0.abs() > 1e-12
            && (eclipse.1 - 1.0).abs() > 1e-12
        {
            // ECLIPSE, OPENED UP — the ability's OWN bonus and the share of the
            // hit it reaches, which is everything but what Condition Overload
            // put in the bracket (MEASUREMENTS M79). A reader checking the
            // ability card wants the 0.30 and the share; `x1.18` is the two of
            // them multiplied and is a number nothing on any card says.
            vec![Term { factor: None, value: eclipse.0 * eclipse.1, of: Some(eclipse) }]
        } else if *factor == Factor::Critical && (crit_damage - crit_damage_base).abs() > 1e-9 {
            // THE CRIT DAMAGE, OPENED UP: the weapon's own, plus what the build
            // adds. Unnamed for the same reason the base bucket's sum is — the
            // resolver folds every crit-damage source into one number.
            vec![Term { factor: None, value: crit_damage - crit_damage_base, of: None }]
        } else {
            Vec::new()
        };
        let head = if of.is_empty() {
            *v
        } else if *factor == Factor::Critical {
            crit_damage_base
        } else if *factor == Factor::WarframeAbility {
            1.0
        } else {
            part_multiplier
        };
        out.push(Layer::Mul { factor: *factor, value: *v, of, head, out: at });
    }
    out
}

/// A FLAT LIST OF MULTIPLIERS, as layers — what the eight simple damage sites
/// have and all they need.
///
/// A factor of exactly 1 is DROPPED HERE rather than at the renderer, which is
/// the whole of the "no pile of x1.00" rule: a layer that does nothing is not a
/// layer, and there is nowhere downstream for it to reappear from.
pub(super) fn mul_layers(base: f64, steps: &[crate::record::Step]) -> Vec<crate::record::Layer> {
    let mut at = base;
    let mut out = Vec::with_capacity(steps.len());
    for (factor, v) in steps {
        if (v - 1.0).abs() <= 1e-12 {
            continue;
        }
        at *= v;
        out.push(crate::record::Layer::Mul {
            factor: *factor,
            value: *v,
            of: Vec::new(),
            head: *v,
            out: at,
        });
    }
    out
}

/// WHAT ONE DAMAGE INSTANCE WAS, from the attacker's side.
///
/// Handed to [`log_damage`] by each of the nine sites where damage lands. A
/// struct rather than a dozen positional arguments because most sites fill most
/// of it with a default: a status tick has no body part, an explosion has no
/// crit tier, and saying so by omission is how those two stopped being told
/// apart in the first place.
#[derive(Default)]
pub(super) struct Instance {
    pub(super) origin: crate::record::Origin,
    /// The offensive ledger: `base × Π steps` is what the target was handed.
    /// The offensive ledger — see [`crate::record::Layer`]. `base` is the
    /// weapon's own damage before any bracket, and each layer says what the
    /// running total is after it.
    pub(super) base: f64,
    pub(super) layers: Vec<crate::record::Layer>,
    pub(super) part: Option<String>,
    pub(super) head: bool,
    pub(super) crit_tier: u32,
    /// The crit damage the crit factor was built from — see
    /// [`crate::record::Damage::crit_damage`].
    pub(super) crit_damage: f64,
    /// Where `base` started, and what took it there.

    /// Which pellet of the trigger pull, counting from 1 — see
    /// [`crate::record::Damage::pellet`].
    pub(super) pellet: Option<u32>,
    /// The EXPLOSION half of an attack that has one.
    pub(super) radial: bool,
    /// What this instance APPLIED — a different question from what it was.
    pub(super) procs: Vec<DamageType>,
}
