// SPDX-License-Identifier: AGPL-3.0-or-later
// HOW LONG EACH ASK MAY WAIT — THE WHOLE DISPATCH POLICY (worker/factory.js
// re-exports it). A module that imports nothing, so the order center in
// wfsim-cloud states the same policy without bundling the factory.

/// Hand-outs go earliest deadline first, and a question past its deadline is the
/// official machines' too, whatever its kind. A kind is only when it is asked and
/// this; a refresh is due when its row turns `REFRESH_WITHIN_MS` old, not a span
/// after its ask, so idle volunteers take it early and a busy fleet lets it wait.
export const DEADLINE_MS = {
  chat: 2 * 60_000,
  new_build: 6 * 3_600_000,
  survey_build: 3 * 86_400_000,
  survey: 86_400_000,
  refresh: 12 * 86_400_000,
  sweep: 7 * 86_400_000,
};
