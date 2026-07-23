//! wfsim optimizer: search for the best mod combination on top of the engine.
//!
//! Principle (see `docs/CORE.md` section 5): the optimizer **only calls the
//! engine** and never reimplements a simplified damage formula of its own —
//! otherwise the "optimum" is fake.
//!
//! Evaluation is hybrid: analytic expected value (fast, for coarse search
//! pruning) plus Monte Carlo (slow, for final calibration and distributions).
//! The objective is switchable (steady DPS / TTK vs a target / total damage in
//! a crowd scenario / burst damage).

#[cfg(test)]
mod tests {
    #[test]
    fn optimizer_scaffold_compiles() {
        assert_eq!(2 + 2, 4);
    }
}
