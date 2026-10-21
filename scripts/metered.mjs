// SPDX-License-Identifier: AGPL-3.0-or-later
/// THE METERED SOURCES: what a fight runs. A module a new verb's work runs in
/// is added here; check_gas asks every item in it for the attribute, and
/// check_prose refuses a read of prose in it.
export const METERED = ["engine/src/fight", "engine/src/rules", "engine/src/model", "engine/src/target.rs",
  "engine/src/formation.rs", "engine/src/record.rs"];
/// …less the code around the runs, which must not count: the loop that reads
/// the count, the counter itself, and the maths that stands in for std's,
/// which was never counted either.
export const NOT_METERED = ["engine/src/fight/monte_carlo.rs", "engine/src/fight/gas.rs", "engine/src/rules/math.rs"];
