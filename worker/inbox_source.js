// SPDX-License-Identifier: AGPL-3.0-or-later
// A closed source generation cannot acquire new retry eligibility.
export const INBOX_GENERATION_MS=900000;
export function inboxSourceId(uuid) {
  if(!/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(uuid))throw Error('invalid inbox source nonce');
  return uuid.slice(0,14)+'8'+uuid.slice(15);
}
export function inboxGeneration(now=Date.now()) {
  if(!Number.isSafeInteger(now)||now<INBOX_GENERATION_MS)throw Error('invalid inbox source clock');
  return Math.floor(now/INBOX_GENERATION_MS);
}
