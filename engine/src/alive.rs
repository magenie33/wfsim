// SPDX-License-Identifier: AGPL-3.0-or-later
//! THE ENGINE SAYS IT IS ALIVE from inside its work: once a fight run
//! (`fight::monte_carlo::shard_onto`) and once a data file read
//! (`data::files_under`), to whatever the host registered. The browser's worker
//! posts it to the page, so a long call is told from a worker the browser
//! reclaimed; the listener throttles by the clock. It reads nothing a fight
//! counts and writes nothing, so no answer and no work moves with it.

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
