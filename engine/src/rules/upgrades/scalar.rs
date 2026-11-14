use super::{Accumulator, OperationType, UpgradeEntry, UpgradeType};

/// The mutating manager and returned-value query have distinct materialization boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarMutation {
    RetainOperations,
    Materialize,
}

/// Semantic protected-scalar projection; pending operations are copied with their base.
#[derive(Debug, Clone, Copy)]
pub struct NativeScalar {
    base: f32,
    operations: Accumulator,
}

#[wfsim_gas_macro::metered]
impl NativeScalar {
    pub fn new(base: f32) -> Self { Self { base, operations: Accumulator::default() } }
    pub fn base(&self) -> f32 { self.base }
    pub fn has_operations(&self) -> bool { self.operations.has_operations() }
    pub fn push(&mut self, operation: OperationType, value: f32) { self.operations.push(operation, value); }
    pub fn value(&self, rounded: bool) -> f32 { self.operations.apply(self.base, rounded) }

    /// Matching and native traversal order are supplied by the bound caller.
    pub fn install_selected(&mut self, kind: UpgradeType,
        entries: impl IntoIterator<Item = UpgradeEntry>, mutation: ScalarMutation)
    {
        for entry in entries {
            if entry.UpgradeType == kind { self.push(entry.OperationType, entry.Value); }
        }
        if mutation == ScalarMutation::Materialize { self.materialize(); }
    }

    /// The returned-value manager consumes the stored base supplied by its caller.
    /// Parent pending operations remain part of the scalar, not this float input.
    pub fn query_selected(self, kind: UpgradeType,
        entries: impl IntoIterator<Item = UpgradeEntry>, rounded: bool) -> f32
    {
        let mut scalar = Self::new(self.base);
        scalar.install_selected(kind, entries, ScalarMutation::RetainOperations);
        scalar.value(rounded)
    }

    pub fn materialize(&mut self) {
        self.base = self.value(false);
        self.operations = Accumulator::default();
    }
}
