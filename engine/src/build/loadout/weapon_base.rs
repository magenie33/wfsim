use super::*;

/// The Evolution II choice — a SEARCH DIMENSION. A
impl WeaponBase {
    /// Apply an arbitrary equipped-evolution set (data ids) from
    /// data/evolutions/*.yaml onto the raw base. An EMPTY list = nothing
    /// installed at any choosable tier.
    pub(super) fn apply_evolution_ids(mut self, evo_ids: &[&str]) -> Self {
        let evos: Vec<_> = evo_ids
            .iter()
            .map(|id| {
                crate::data::evolutions::get(id)
                    .unwrap_or_else(|| panic!("missing evolution yaml: {id}"))
            })
            .collect();
        crate::data::evolutions::apply(&mut self, &evos);
        self
    }

    /// Build a weapon FORM from its data entry: the raw yaml panel
    /// (`data::weapons::base_panel`) with an arbitrary equipped-evolution
    /// selection applied (data ids; empty = bare weapon). The engine knows
    /// no specific weapon — `id` is purely a data key.
    /// A FLAT BASE-DAMAGE ADD, folded the way an evolution's is.
    ///
    /// The base damage TOTAL rises by `flat` and the vector scales pro-rata, so
    /// the composition is untouched and every downstream reading of the base
    /// follows. The EXPLOSION takes it too and keeps multiplying its UNEVOLVED
    /// base for Condition Overload, which the CO catalog's Burston radial row
    /// settles ("Attack Damage 55 | CO Damage Bonus at +100% 13 | 24%").
    ///
    /// ONE IMPLEMENTATION, two callers: `data::evolutions::apply` for a plain
    /// flat perk and `resolve_for` for one the player's state gates. A gated
    /// "+40 with overshields" and an ungated "+40" are the same statement, so
    /// they must not come out as different panels.
    /// WHAT THE CO TERM READS, PAIRED WITH THE BASE IT IS A SHARE OF —
    /// derived from [`Self::co_base`], never stored. The FACT is the absolute;
    /// pairing it with its own denominator here means the two cannot disagree,
    /// and a caller cannot hand one stage's share to another stage's base.
    pub fn co_base_pair(&self) -> CoBase {
        CoBase::new(self.co_base, self.base_vector.total(), CoStage::Direct)
    }

    /// The share alone, for a reader that has no stage to pair it with — the
    /// panel, a test, the combat record.
    pub fn co_base_fraction(&self) -> f64 {
        self.co_base_pair().fraction()
    }

    /// WHAT SHARE OF THE VECTOR AN ATTACK'S MULTIPLIER MUST LEAVE ALONE —
    /// derived from [`Self::unswung_base`], never stored, for the reason
    /// [`Self::co_base_fraction`] is derived.
    ///
    /// It survives every later multiplication: a base-damage mod and an
    /// elemental mod both scale the flat add and the weapon's own base by the
    /// same factor, so the share the vector carries stays this one.
    pub fn unswung_fraction(&self) -> f64 {
        let total = self.base_vector.total();
        if total <= 0.0 {
            return 0.0;
        }
        (self.unswung_base / total).clamp(0.0, 1.0)
    }

    /// Add flat base damage, and say how much of it the GunCO term's base
    /// grows by.
    ///
    /// `into_co` is USUALLY 0 or `flat` and is passed as an amount rather than
    /// a bool on purpose: a build carrying two flat-damage perks that disagree
    /// contributes part of its total, and a bool cannot say that.
    pub fn add_flat_base_damage(&mut self, flat: f64, into_co: f64) {
        if flat <= 0.0 {
            return;
        }
        let original_total = self.base_vector.total();
        // A DIRECT VECTOR OF ZERO IS A FORM WHOSE WHOLE ATTACK IS ITS EXPLOSION
        // — the heavy slam states exactly that — so this cannot return early on
        // it: the add still has an explosion to reach.
        if original_total > 0.0 {
            let evolved = original_total + flat;
            self.base_vector = self.base_vector.scale(evolved / original_total);
            self.co_base += into_co;
            // …AND THE ATTACK'S OWN MULTIPLIER LEAVES IT ALONE, measured
            // (M79). It rides beside the swung base rather than inside it,
            // which is what an EXPLOSION already does here — the radial takes
            // the same add as an ABSOLUTE, on a base its slam multiplier has
            // already been spent on, and that half was measured first (M69).
            self.unswung_base += flat;
        }
        // AN EXPLOSION TAKES THE SAME ABSOLUTE ADD, and a SLAM is measured to.
        // The Magistar's heavy slam is 1050 and its `+100 Base Damage` reads
        // 1095 at a body's width from the epicentre — 95% of 1150, which is the
        // add landing ONCE. Multiplied by the slam's own 5x it would be 1550,
        // and 1095 is 71% of that: the falloff floor, which is the edge of the
        // radius and not the muzzle of it (MEASUREMENTS M69).
        let flat_added = |r: &mut RadialBase| {
            let rad_original = r.base_vector.total();
            if rad_original <= 0.0 {
                return;
            }
            r.base_vector = r.base_vector.scale((rad_original + flat) / rad_original);
            // …AND ITS CO BASE DOES NOT GROW, EVER. That is the behaviour this
            // refactor preserved rather than chose: the old code set the
            // radial's fraction to `original / evolved` unconditionally while
            // the direct hit's followed the perk's flag, so the two parts of one
            // weapon could disagree about the same +42. Written as `+= 0.0` so
            // the day a measurement arrives there is one line to change.
            r.co_base += 0.0;
        };
        if let Some(r) = self.radial.as_mut() {
            flat_added(r);
        }
        // …AND THE WEAPON'S OWN SLAM, which a combo ends on. It is the same
        // attack seen from a different swing and was taking no flat add at all.
        if let Some(sl) = self.slam.as_mut() {
            flat_added(sl);
        }
    }

    pub fn from_data(id: &str, frenzy_active: bool, evo_ids: &[&str]) -> Self {
        Self::from_data_assembled(id, frenzy_active, evo_ids, None)
    }

    /// [`WeaponBase::from_data`], for a MODULAR weapon.
    ///
    /// The assembly is composed into the SPEC before the panel is derived from
    /// it, so an evolution applied afterwards sees the assembled weapon and not
    /// the chamber's preview — which is the order the two mechanics have to be
    /// in, since an evolution states a DELTA against whatever it is fired on.
    pub fn from_data_assembled(
        id: &str,
        frenzy_active: bool,
        evo_ids: &[&str],
        assembly: Option<&crate::data::weapons::kitguns::Assembly>,
    ) -> Self {
        crate::data::weapons::base_panel_assembled(id, frenzy_active, assembly)
            .apply_evolution_ids(evo_ids)
    }

}
