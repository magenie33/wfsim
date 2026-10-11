//! A FIGHT'S WORK IS WHAT ITS SOURCE RAN: every `#[metered]` function charges
//! its source weight once per call and every loop its body's each turn
//! (`tools/gas-macro`), and the engine reads the count around its runs
//! (`shard_onto`). The weights are counted from the source, so native and
//! wasm builds count the same, at any optimisation level. docs/WASM.md §"Gas".

use std::cell::Cell;

/// WHAT A MILLION OF GAS COSTS, and what ONE RUN does, in billionths of a
/// point. Fit by `.github/workflows/work.yml` (`scripts/work_calibrate.mjs`),
/// then rounded to prices judged by the same report (`prices=`): the fit was
/// 44_250 and 374_385, and these time a row as well and pay 1.143 of the old total.
pub const GAS_PRICE_PER_MILLION: u64 = 50_000;
pub const RUN_PRICE: u64 = 400_000;

thread_local! {
    // Per thread: a native scorer fights on many at once.
    static GAS: Cell<u64> = const { Cell::new(0) };
}

/// A metered region's charge (`#[metered]` writes the calls).
#[inline(always)]
pub fn charge(n: u64) {
    if option_env!("WFSIM_GAS_OFF").is_none() {
        GAS.with(|g| g.set(g.get().wrapping_add(n)));
    }
}

/// A FUNCTION BODY'S TALLY: its weight and its loops' turns, paid into the
/// count once, when the body returns or unwinds.
pub struct Tally(Cell<u64>);

impl Tally {
    #[inline(always)]
    pub fn new(n: u64) -> Self {
        Self(Cell::new(n))
    }
    #[inline(always)]
    pub fn add(&self, n: u64) {
        self.0.set(self.0.get().wrapping_add(n));
    }
}

impl Drop for Tally {
    #[inline(always)]
    fn drop(&mut self) {
        charge(self.0.get());
    }
}

/// The count so far, on this thread.
pub fn gas_now() -> u64 {
    GAS.with(Cell::get)
}

/// THE WORK OF `gas` over `runs` runs: integer arithmetic, the same everywhere.
pub fn work_of(gas: u64, runs: u32) -> u64 {
    let w = u128::from(gas) * u128::from(GAS_PRICE_PER_MILLION) / 1_000_000 + u128::from(runs) * u128::from(RUN_PRICE);
    u64::try_from(w).unwrap_or(u64::MAX)
}
