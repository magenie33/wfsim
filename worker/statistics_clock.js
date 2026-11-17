// SPDX-License-Identifier: AGPL-3.0-or-later
// Public statistics expire from their sampled counts, never from publication.
export const STATISTICS_FRESH_MS=120_000;
const stamp=value=>typeof value==='string'&&Number.isFinite(Date.parse(value))&&new Date(value).toISOString()===value;
export const statisticsExpiresAt=at=>stamp(at)?new Date(Date.parse(at)+STATISTICS_FRESH_MS).toISOString():null;
export function currentStatistics(value,now=Date.now()){
 if(!value?.ok||value.v!==1||![value.at,value.counts_at,value.expires_at].every(stamp)
  ||!(value.order_epoch===null||stamp(value.order_epoch)))return false;
 const sampled=Date.parse(value.counts_at),published=Date.parse(value.at),expires=Date.parse(value.expires_at);
 return sampled<=published&&published<=now&&now<expires&&value.expires_at===statisticsExpiresAt(value.counts_at);
}
