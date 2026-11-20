// SPDX-License-Identifier: AGPL-3.0-or-later
//! THE ENGINE SAYS IT IS ALIVE from inside its work: after each group of fight
//! events, each Monte Carlo run, and each data file read. The browser's worker
//! throttles these pulses by the clock before posting to the page.

use std::cell::Cell;

thread_local! {
    static BEAT: Cell<Option<fn()>> = const { Cell::new(None) };
}

/// The host's listener, or `None` to stop.
pub fn set(listener: Option<fn()>) {
    BEAT.with(|b| b.set(listener));
}

/// One word that the work goes on.
#[inline]
pub fn beat() {
    if let Some(f) = BEAT.with(Cell::get) {
        f();
    }
}
