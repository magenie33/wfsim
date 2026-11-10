// SPDX-License-Identifier: AGPL-3.0-or-later
// Protocol fixtures and CI selection consume the same frontend input declaration.
import { readFileSync } from 'node:fs';

export const CLIENT_INPUTS = {
  'scripts/check_board_sync.mjs': ['web/src/static/app/69-board-work.js'],
  'scripts/check_board_reviews.mjs': ['web/src/static/app/83-headless-card.js', 'web/src/static/app/87-headless.js'],
  'scripts/check_nona_results.mjs': ['data/i18n/zh/ui.yaml'],
};
export function clientSource(check) {
  if (!CLIENT_INPUTS[check]) throw Error('undeclared protocol client fixture: ' + check);
  return CLIENT_INPUTS[check].map(path => readFileSync(new URL('../' + path, import.meta.url), 'utf8')).join('\n');
}
