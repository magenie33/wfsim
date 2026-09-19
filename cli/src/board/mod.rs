// SPDX-License-Identifier: AGPL-3.0-or-later
//! THE SCORER — turns submitted builds into a board.
//!
//! Reads the library as a JSON array on stdin, reads the open generation's
//! FACTS as a file, and writes one file per weapon plus a cross-weapon index.
//! Fetching either and committing the result are the workflow's job
//! (`.github/workflows/scores.yml`), and neither needs the engine. What needs
//! the engine is the only thing here — running each build under the benchmark
//! and reading the number off.
//!
//! WHY A SUBMISSION CARRIES NO SCORE: nobody's number is trusted because
//! nobody's number is asked for. A row's score is produced HERE under the
//! benchmark's own pinned seed, so anyone with the repo reproduces any row
//! exactly.
//!
//! IT NEVER SPEAKS TO THE DATABASE. Files in, files out; the network lives in
//! `scripts/ship_facts.sh` and `scripts/fetch_facts.sh`, which a stub `curl`
//! can drive — which is what makes every hop of the chain testable.
//!
//!   cat library.json | wfsim-board single_target site/board //!     --facts-in facts-known.ndjson --facts facts.ndjson

mod entry;
mod facts;
mod measure;
mod publish;
mod queue;
mod run;
mod state;

pub use run::run;
