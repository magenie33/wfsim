//! Damage types (see `docs/MECHANICS.md` §1 and `docs/GLOSSARY.md`).
//!
//! Every hit is a vector over these types, not a scalar. Names follow the wiki.

/// A single damage type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DamageType {
    // Physical (IPS).
    Impact,
    Puncture,
    Slash,

    // Primary elemental.
    Cold,
    Electricity,
    Heat,
    Toxin,

    // Secondary (combined) elemental.
    Blast,     // Heat + Cold
    Corrosive, // Electricity + Toxin
    Gas,       // Heat + Toxin
    Magnetic,  // Cold + Electricity
    Radiation, // Heat + Electricity
    Viral,     // Cold + Toxin

    // Special.
    True,
    Void,
    Tau,
    /// Hidden type (`DT_CINEMATIC_DAMAGE`, once nicknamed "Finishing
    /// Damage"): no faction modifiers anywhere, damages health/shields/
    /// overguard, bypasses **armor** DR only (not other DR sources), not
    /// boosted by physical/elemental bonuses, Sentients don't adapt to it.
    /// Used by Bleed (Slash proc) ticks.
    Cinematic,
}

/// A SET of damage types held in one word — an attack part's FORCED procs.
///
/// A mask rather than a `Vec` because [`crate::loadout::ResolvedRadial`] is
/// `Copy` and the sim copies one per pellet; walking 17 bits costs nothing
/// beside a heap allocation in that loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ForcedProcs(u32);

impl ForcedProcs {
    pub fn from_types(ts: impl IntoIterator<Item = DamageType>) -> Self {
        Self(ts.into_iter().fold(0, |m, t| m | (1 << t as u32)))
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Write the set into `buf` in declaration order and return how many. The
    /// caller keeps the slice interface every consumer already takes, without
    /// this type owning any memory.
    pub fn fill(self, buf: &mut [DamageType; DamageType::ALL.len()]) -> usize {
        let mut n = 0;
        for t in DamageType::ALL {
            if self.0 & (1 << t as u32) != 0 {
                buf[n] = t;
                n += 1;
            }
        }
        n
    }
}

impl DamageType {
    /// Every damage type, in declaration order (indexes match `as usize`).
    pub const ALL: [DamageType; 17] = [
        DamageType::Impact,
        DamageType::Puncture,
        DamageType::Slash,
        DamageType::Cold,
        DamageType::Electricity,
        DamageType::Heat,
        DamageType::Toxin,
        DamageType::Blast,
        DamageType::Corrosive,
        DamageType::Gas,
        DamageType::Magnetic,
        DamageType::Radiation,
        DamageType::Viral,
        DamageType::True,
        DamageType::Void,
        DamageType::Tau,
        DamageType::Cinematic,
    ];

    /// The data-file spelling of a damage type — the lowercase wiki name, the
    /// same token `data/weapons`, `data/debuffs` and `data/factions` all use.
    /// `None` for anything unknown: a caller that must not accept a typo says
    /// so itself (weapon data panics; the faction table refuses to load).
    pub fn from_name(name: &str) -> Option<DamageType> {
        Some(match name {
            "impact" => DamageType::Impact,
            "puncture" => DamageType::Puncture,
            "slash" => DamageType::Slash,
            "cold" => DamageType::Cold,
            "electricity" => DamageType::Electricity,
            "heat" => DamageType::Heat,
            "toxin" => DamageType::Toxin,
            "blast" => DamageType::Blast,
            "corrosive" => DamageType::Corrosive,
            "gas" => DamageType::Gas,
            "magnetic" => DamageType::Magnetic,
            "radiation" => DamageType::Radiation,
            "viral" => DamageType::Viral,
            "true" => DamageType::True,
            "void" => DamageType::Void,
            "tau" => DamageType::Tau,
            "cinematic" => DamageType::Cinematic,
            _ => return None,
        })
    }

    /// The same token back — what a data file would have to say to name this
    /// type, and what the api hands the web UI to look up its display name.
    pub fn name(self) -> &'static str {
        match self {
            DamageType::Impact => "impact",
            DamageType::Puncture => "puncture",
            DamageType::Slash => "slash",
            DamageType::Cold => "cold",
            DamageType::Electricity => "electricity",
            DamageType::Heat => "heat",
            DamageType::Toxin => "toxin",
            DamageType::Blast => "blast",
            DamageType::Corrosive => "corrosive",
            DamageType::Gas => "gas",
            DamageType::Magnetic => "magnetic",
            DamageType::Radiation => "radiation",
            DamageType::Viral => "viral",
            DamageType::True => "true",
            DamageType::Void => "void",
            DamageType::Tau => "tau",
            DamageType::Cinematic => "cinematic",
        }
    }

    /// A primary element (Cold / Electricity / Heat / Toxin) — the ones that
    /// combine into secondary elements.
    pub fn is_primary_element(self) -> bool {
        matches!(
            self,
            DamageType::Cold | DamageType::Electricity | DamageType::Heat | DamageType::Toxin
        )
    }

    /// A COMBINED element — one two primaries make. Stated as its own
    /// predicate rather than as "elemental and not primary", because IPS is
    /// neither and the negation would have swept it in.
    pub fn is_secondary_element(self) -> bool {
        matches!(
            self,
            DamageType::Blast
                | DamageType::Corrosive
                | DamageType::Gas
                | DamageType::Magnetic
                | DamageType::Radiation
                | DamageType::Viral
        )
    }
}

/// A hit's damage as a vector over damage types (docs/MECHANICS.md §1).
///
/// Panel damage is the plain **sum** of the components; a crit multiplies the
/// **whole vector** at once (one roll per hit/pellet, never per component);
/// the defense side then resolves each component independently
/// (docs/MECHANICS.md §5, §8).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DamageVector {
    amounts: [f64; DamageType::ALL.len()],
}

impl DamageVector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder-style: set one component.
    pub fn with(mut self, t: DamageType, amount: f64) -> Self {
        self.amounts[t as usize] = amount;
        self
    }

    pub fn get(&self, t: DamageType) -> f64 {
        self.amounts[t as usize]
    }

    pub fn set(&mut self, t: DamageType, amount: f64) {
        self.amounts[t as usize] = amount;
    }

    pub fn add(&mut self, t: DamageType, amount: f64) {
        self.amounts[t as usize] += amount;
    }

    /// THE BIGGEST COMPONENT — which type a mixed hit reads AS.
    ///
    /// A direct hit is a whole vector, not one type, so "what colour is this
    /// number" has no exact answer. The dominant component is the honest
    /// approximation and it is only ever asked for DISPLAY: nothing in the
    /// damage path reads it. Ties go to the lower index, which is
    /// `DamageType::ALL`'s own order, so the answer is stable.
    pub fn dominant(&self) -> DamageType {
        let mut best = 0;
        for i in 1..self.amounts.len() {
            if self.amounts[i] > self.amounts[best] {
                best = i;
            }
        }
        DamageType::ALL[best]
    }

    /// Panel damage: the sum of all components.
    pub fn total(&self) -> f64 {
        self.amounts.iter().sum()
    }

    /// Scale every component (e.g. by the crit tier multiplier).
    pub fn scale(&self, k: f64) -> Self {
        let mut out = *self;
        for a in &mut out.amounts {
            *a *= k;
        }
        out
    }

    /// This type's share of the total — the proc-type weight
    /// (`P(type) = damage / total`, docs/MECHANICS.md §6).
    pub fn share(&self, t: DamageType) -> f64 {
        let total = self.total();
        if total <= 0.0 {
            0.0
        } else {
            self.get(t) / total
        }
    }

    /// Non-zero components in declaration order.
    pub fn iter_nonzero(&self) -> impl Iterator<Item = (DamageType, f64)> + '_ {
        DamageType::ALL
            .iter()
            .map(|&t| (t, self.get(t)))
            .filter(|&(_, a)| a > 0.0)
    }

    /// Damage quantization (wiki `Damage/Calculation` §Quantization): each
    /// component snaps to the nearest multiple of `ModdedBase/32` — a network
    /// serialization scheme (one total integer + per-type 1/32 multiples).
    /// Applied to the modded base vector BEFORE crits / type modifiers /
    /// faction multipliers (those multiply the quantized values). Mixed
    /// vectors gain or lose a few percent. Granularity was 1/16 before U40.
    ///
    /// **THE DENOMINATOR IS `ModdedBase`, NOT THIS VECTOR'S TOTAL**, and the
    /// difference is every elemental mod ever equipped. The page states both
    /// halves as formulas — `Scale = ModdedBase / 32` and
    /// `x = TypeValue / ModdedBase` — where ModdedBase is `base × (1 + damage
    /// mods)` with the elemental portions EXCLUDED, the same number
    /// [`crate::dummy::DummyParams::dot_modified_base`] already carries for
    /// status payloads. Elements are in the numerator only, which is why the
    /// note beside it holds: a non-elemental bonus "multiplies both the base
    /// value of rounding numerator and Scale of rounding denominator, and
    /// therefore is a simple multiplier to any quantized total".
    ///
    /// IT WAS THIS VECTOR'S TOTAL FOR MONTHS AND NOTHING COULD SEE IT. The
    /// page's worked example is 30/30/40 with no mods at all, so ModdedBase and
    /// the total are the same 100 and the example passes either way — the one
    /// test on this function could not distinguish the two readings. A Braton
    /// Prime with Infected Clip and Hellfire could: base 35, Gas 63, and the
    /// wrong denominator snaps the four components to 33 units instead of 32,
    /// for **101.06 against a measured 98**. Four
    /// builds were measured and the right denominator reproduces all four to
    /// the digit.
    ///
    /// A MONO-TYPE VECTOR IS NO LONGER AUTOMATICALLY LOSSLESS, which is the
    /// visible consequence: it was exactly 32 units of itself, and it is now
    /// however many units of ModdedBase it happens to be. A pure 63 Gas on a
    /// base of 35 is 57.6 units and snaps to 58.
    ///
    /// There is deliberately no zero-argument form. The denominator being
    /// IMPLICIT is the whole of the bug above.
    pub fn quantized_against(&self, modded_base: f64) -> Self {
        if modded_base <= 0.0 || self.total() <= 0.0 {
            return *self;
        }
        let scale = modded_base / QUANTIZATION_DENOMINATOR;
        let mut out = *self;
        for a in &mut out.amounts {
            // `sign(x) × floor(|x| × 32 + 0.5) / 32`, verbatim: f64::round is
            // half-away-from-zero, which is that rule on both signs.
            *a = (*a / scale).round() * scale;
        }
        out
    }
}

/// Damage quantization denominator (1/32 steps since U40; 1/16 before).
pub const QUANTIZATION_DENOMINATOR: f64 = 32.0;

/// Base critical-damage-multiplier quantization (wiki `Critical_Hit`
/// §Quantization): `round(cd × 4095/32) × 32/4095`, applied to
/// `base_cd + weapon-flat CD` before relative mods (docs/MECHANICS.md §5).
/// Note even a clean 2.0x is off-grid: it quantizes to ≈2.000488x.
pub fn quantize_base_crit_damage(cd: f64) -> f64 {
    (cd * 4095.0 / 32.0).round() * 32.0 / 4095.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_damage_is_the_sum_of_components() {
        // Five components of 10 each -> panel 50 (user example, MECHANICS §1).
        let v = DamageVector::new()
            .with(DamageType::Impact, 10.0)
            .with(DamageType::Slash, 10.0)
            .with(DamageType::Heat, 10.0)
            .with(DamageType::Toxin, 10.0)
            .with(DamageType::Corrosive, 10.0);
        assert_eq!(v.total(), 50.0);
        assert_eq!(v.share(DamageType::Heat), 0.2);
    }

    #[test]
    fn crit_scales_the_whole_vector_together() {
        let v = DamageVector::new()
            .with(DamageType::Impact, 10.0)
            .with(DamageType::Toxin, 40.0);
        let crit = v.scale(2.0);
        assert_eq!(crit.get(DamageType::Impact), 20.0);
        assert_eq!(crit.get(DamageType::Toxin), 80.0);
        assert_eq!(crit.total(), 100.0);
    }

    #[test]
    fn quantization_matches_the_wiki_worked_example() {
        // 30 Impact / 30 Puncture / 40 Slash: scale = 100/32 = 3.125.
        // 30/3.125 = 9.6 -> 10 -> 31.25; 40/3.125 = 12.8 -> 13 -> 40.625.
        let q = DamageVector::new()
            .with(DamageType::Impact, 30.0)
            .with(DamageType::Puncture, 30.0)
            .with(DamageType::Slash, 40.0)
            // ModdedBase == the total here: the example carries no mods at all,
            // which is exactly why it could not tell the two denominators
            // apart. `braton_prime_quantizes_against_its_modded_base` is the
            // case that can.
            .quantized_against(100.0);
        assert_eq!(q.get(DamageType::Impact), 31.25);
        assert_eq!(q.get(DamageType::Puncture), 31.25);
        assert_eq!(q.get(DamageType::Slash), 40.625);
        assert_eq!(q.total(), 103.125); // panel 100 deals 103.125 (+3.1%)
                                        // Multipliers apply AFTER quantization: crit x1.5 and the Charger's
                                        // Infested slash x1.5 reproduce the page's table numbers.
        assert_eq!(q.total() * 1.5, 154.6875);
        let vs_charger = q.get(DamageType::Impact)
            + q.get(DamageType::Puncture)
            + q.get(DamageType::Slash) * 1.5;
        assert_eq!(vs_charger, 123.4375);
    }

    #[test]
    fn quantization_edge_cases() {
        // Mono-type vectors are lossless (component/scale = exactly 32).
        // A mono-type vector is lossless AGAINST ITSELF — 32 units exactly —
        // and that is now a statement about the denominator rather than a
        // property of the vector: the same 57.3 Heat on a base of 35 is 52.4
        // units and snaps to 52.
        let mono = DamageVector::new().with(DamageType::Heat, 57.3).quantized_against(57.3);
        assert!((mono.get(DamageType::Heat) - 57.3).abs() < 1e-12);
        let off_base = DamageVector::new().with(DamageType::Heat, 57.3).quantized_against(35.0);
        assert!((off_base.get(DamageType::Heat) - 52.0 * 35.0 / 32.0).abs() < 1e-12);
        // Dual Toxocyst base 7.5/60/7.5: quantizes to 7.03125/60.9375/7.03125
        // - components shift but the total stays exactly 75.
        let frame_seconds = DamageVector::new()
            .with(DamageType::Impact, 7.5)
            .with(DamageType::Puncture, 60.0)
            .with(DamageType::Slash, 7.5)
            .quantized_against(75.0);
        assert_eq!(frame_seconds.get(DamageType::Impact), 7.03125);
        assert_eq!(frame_seconds.get(DamageType::Puncture), 60.9375);
        assert_eq!(frame_seconds.total(), 75.0);
        // Empty vector: no-op.
        assert_eq!(DamageVector::new().quantized_against(0.0).total(), 0.0);
    }

    /// **THE CASE THE WIKI'S OWN EXAMPLE CANNOT DECIDE**, measured four ways.
    ///
    /// A Braton Prime, base 35 split 1.75/12.25/21, with the element bonus
    /// stated per build. Against the vector's TOTAL the four components snap to
    /// 33, 32, 31 and 31 units; against `ModdedBase = 35` they snap to what the
    /// game actually dealt, in all four:
    ///
    /// | build | raw | wrong denominator | ModdedBase | measured |
    /// | --- | --- | --- | --- | --- |
    /// | 90% Toxin + 90% Heat | 98 | 101 | **98.4375** | 98 |
    /// | +200% Corrosive | 105 | 105 | **105.0** | 105 |
    /// | +200% Gas, +90% Toxin | 136.5 | 132 | **136.7188** | 137 |
    /// | +200% Blast, +90% Cold, +90% Heat | 168 | 163 | **168.4375** | 168 |
    ///
    /// The second row is why four were needed: it agrees under both readings,
    /// and a single measurement that happened to be that one would have
    /// confirmed the bug.
    #[test]
    fn braton_prime_quantizes_against_its_modded_base() {
        let braton = || {
            DamageVector::new()
                .with(DamageType::Impact, 1.75)
                .with(DamageType::Puncture, 12.25)
                .with(DamageType::Slash, 21.0)
        };
        // The pop-up is `floor(x + 0.5)` of the instance — the page's own
        // "further rounded to a whole number".
        let popped = |v: DamageVector| (v.total() + 0.5).floor();
        for (label, element, amount, want) in [
            ("90% Toxin + 90% Heat", DamageType::Gas, 63.0, 98.0),
            ("+200% Corrosive", DamageType::Corrosive, 70.0, 105.0),
            ("+200% Blast + 90% Cold + 90% Heat", DamageType::Blast, 133.0, 168.0),
        ] {
            let v = braton().with(element, amount).quantized_against(35.0);
            assert_eq!(popped(v), want, "{label}: {}", v.total());
        }
        // Two elemental components at once — the parallel imbue does not
        // combine with the mod's own element, so both are quantized.
        let split = braton()
            .with(DamageType::Toxin, 31.5)
            .with(DamageType::Gas, 70.0)
            .quantized_against(35.0);
        assert_eq!(popped(split), 137.0, "+200% Gas + 90% Toxin: {}", split.total());
    }

    /// **THE PAGE'S OWN END-TO-END EXAMPLE, WITH ELEMENTS**, which is the one
    /// the 30/30/40 illustration is not: a Nagantaka Prime (base 173, 1/9/90)
    /// with Cryo Rounds, Malignant Force, Hellfire, Piercing Caliber and a Gas
    /// Valence Formation.
    ///
    /// It states the scale as `173 / 32 = 5.40625` — the weapon's BASE, against
    /// a modded total of 951.5 — and then walks all six components. Under the
    /// denominator this engine used until 2026-08-23 the scale would have been
    /// `951.5/32 = 29.7` and not one line of it would reproduce.
    ///
    /// Three separate rules fall out of the same six lines, and each is a thing
    /// the engine has to get right:
    ///
    /// - a component too small for one step **rounds to zero** — the page says
    ///   so outright, "the Impact value of Nagantaka Prime is so low that it
    ///   will not register during attacks";
    /// - a COMBINED element quantizes its SUM as one component (Cold 90% +
    ///   Toxin 60% is one 150% Viral, not two);
    /// - a PARALLEL imbue is its own component beside it, which is what makes
    ///   Valence Formation visible here at all.
    #[test]
    fn the_pages_nagantaka_prime_example_reproduces_line_by_line() {
        let base = 173.0;
        let v = DamageVector::new()
            .with(DamageType::Impact, 1.73)
            // Piercing Caliber is +120% Puncture, and a physical mod scales the
            // weapon's own share of that type.
            .with(DamageType::Puncture, 15.57 * 2.2)
            .with(DamageType::Slash, 155.7)
            // Cryo Rounds 90% + Malignant Force 60%, summed into one Viral.
            .with(DamageType::Viral, 1.5 * base)
            .with(DamageType::Heat, 0.9 * base)
            // Valence Formation, parallel and therefore its own type.
            .with(DamageType::Gas, 2.0 * base)
            .quantized_against(base);
        for (t, want) in [
            (DamageType::Impact, 0.0),
            (DamageType::Puncture, 32.4375),
            (DamageType::Slash, 156.78125),
            (DamageType::Viral, 259.5),
            (DamageType::Heat, 156.78125),
            (DamageType::Gas, 346.0),
        ] {
            assert_eq!(v.get(t), want, "{t:?}");
        }
        assert_eq!(v.total(), 951.5);
    }

    #[test]
    fn base_crit_damage_quantization() {
        // Even a clean 2.0x sits off-grid: 2.0 * 4095/32 = 255.9375 -> 256
        // -> 256 * 32/4095 ≈ 2.000488.
        let q = quantize_base_crit_damage(2.0);
        assert!((q - 2.0004884004884).abs() < 1e-10, "q = {q}");
        // Grid points round-trip exactly.
        let grid = 256.0 * 32.0 / 4095.0;
        assert_eq!(quantize_base_crit_damage(grid), grid);
    }

    #[test]
    fn empty_vector_has_zero_shares() {
        let v = DamageVector::new();
        assert_eq!(v.total(), 0.0);
        assert_eq!(v.share(DamageType::Impact), 0.0);
        assert_eq!(v.iter_nonzero().count(), 0);
    }
}
