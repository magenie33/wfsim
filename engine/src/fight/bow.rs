use super::*;

/// Drawing completes on its own turn; every release starts another nock.
#[derive(Default)]
pub(super) enum BowState {
    #[default]
    Nocked,
    Drawing(crate::model::FormKind),
    Nocking,
}

#[wfsim_gas_macro::metered]
impl BowState {
    pub(super) fn before_release(&mut self, form: crate::model::FormKind, draw: f64) -> Option<f64> {
        if matches!(self, Self::Drawing(f) if *f == form) || draw <= 0.0 {
            *self = Self::Nocking;
            None
        } else {
            *self = Self::Drawing(form);
            Some(draw)
        }
    }
}

#[wfsim_gas_macro::metered]
impl FightParams {
    pub(super) fn uses_reserve_arrows(&self) -> bool {
        self.bow_cycle.is_some_and(|b| b.uses_reserve)
    }
}

#[wfsim_gas_macro::metered]
impl Ammo {
    /// The resource the active form spends, shared by the shot and its list.
    pub(super) fn available(&self, params: &FightParams, incarnon: &IncarnonState) -> f64 {
        let active = match &params.cycle {
            Some(cy) if incarnon.in_base_form => &cy.base_form,
            _ => params,
        };
        if active.uses_reserve_arrows() {
            if params.infinite_reserve { f64::INFINITY } else { self.reserve }
        } else if incarnon.in_base_form && params.cycle.as_ref()
            .is_some_and(|cy| matches!(cy.ends, Ends::ChargeMagazine | Ends::SingleShot)) {
            incarnon.base_magazine
        } else {
            self.loaded
        }
    }
}

/// Wiki Fire Rate / Nataruk: reload speed changes nocking, not drawing.
#[wfsim_gas_macro::metered]
pub(super) fn nock_span(
    params: &FightParams,
    active: &FightParams,
    arc: &mut ArcRuntime,
    stacks: &mut [LiveStacks],
    rs_armed: bool,
    t: f64,
) -> f64 {
    let bow = active.bow_cycle.expect("a bow's nock");
    let bonus = params.arcane.reload_bonus
        + arc.total(&params.arcane.buffs, ArcGrant::ReloadSpeed, t)
        + live_reload_speed(params, active, rs_armed, stacks, t);
    reload_span(bow.nock_seconds, active.reload_bonus, bonus)
}
