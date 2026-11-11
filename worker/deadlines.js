// SPDX-License-Identifier: AGPL-3.0-or-later
// HOW LONG EACH ASK MAY WAIT FOR OFFICIAL FALLBACK (worker/factory.js
// re-exports it). A module that imports nothing, so the order center in
// wfsim-cloud states the same policy without bundling the factory.

/// Zones and FIFO order volunteer execution. A question past its deadline is
/// also eligible for its official executor. Deadlines do not reorder a zone.
export const REFRESH_WITHIN_MS = 14 * 86_400_000;
export const DEADLINE_MS = {
  board_review: 15 * 60_000,
  chat: 2 * 60_000,
  user_calculation: 6 * 3_600_000,
  new_build: 6 * 3_600_000,
  survey_build: 3 * 86_400_000,
  survey: 86_400_000,
  family_build: 3 * 86_400_000,
  refresh: REFRESH_WITHIN_MS,
  sweep: 7 * 86_400_000,
};
