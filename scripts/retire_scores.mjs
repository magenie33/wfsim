// SPDX-License-Identifier: AGPL-3.0-or-later
import { readFileSync } from 'node:fs';

const origin = process.env.FACTORY_ORIGIN || 'https://wfsim.app';
if (!process.env.FACTORY_TOKEN) throw new Error('FACTORY_TOKEN is required');
const repair = process.argv[2] === '--repair';
const reason = process.env.WHY || 'corrected simulation model';
const post = async body => {
  const response = await fetch(origin + '/api/board/retire', {method:'POST',
    headers:{'content-type':'application/json',authorization:'Bearer ' + process.env.FACTORY_TOKEN},
    body:JSON.stringify(body),signal:AbortSignal.timeout(180000)});
  const result = await response.json();
  if (!response.ok || !result.ok) throw new Error(`score retirement: HTTP ${response.status}: ${result.error || 'invalid reply'}`);
  return result;
};
let count = 0;
if (repair) {
  let after;
  do {
    const result = await post({repair:true,...(after ? {after} : {})});
    count += result.reviews.length;
    after = result.after;
  } while (after);
} else {
  const release = await (await fetch(origin + '/release.json', {signal:AbortSignal.timeout(30000)})).json();
  const rows = readFileSync(process.argv[2], 'utf8').split('\n').filter(Boolean).map(line => {
    const row = JSON.parse(line);
    return {identity:row.identity || row.build_id,ruler:row.ruler,mode:row.mode,
      ...(row.measured_at ? {measured_at:row.measured_at} : {})};
  });
  for (let from = 0; from < rows.length; from += 50) {
    const result = await post({engine:release.engine,reason,rows:rows.slice(from,from+50)});
    count += result.reviews.length;
    console.log(`score retirement: ${count}/${rows.length} rows submitted for independent verification`);
  }
}
console.log(`score retirement: ${count} review(s) retained or requested`);
