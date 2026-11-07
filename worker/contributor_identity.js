// SPDX-License-Identifier: AGPL-3.0-or-later
// Public identity is stable; names and visibility are resolved from current account state.
import { shownName } from "./names.js";

export async function contributorAccounts(env, ids) {
  if (!env.ACCOUNTS || !ids.length) return new Map();
  const wanted = JSON.stringify([...new Set(ids)]);
  const { results } = await env.ACCOUNTS.prepare(`SELECT a.id,a.profile_id,a.username,a.display_name,c.named,
    h.display_name AS held_name,h.reason AS held_reason FROM accounts a LEFT JOIN contribution_choice c ON c.account=a.id
    LEFT JOIN display_name_holds h ON h.account=a.id WHERE a.id IN (SELECT jsonb_array_elements_text((?::text)::jsonb))`)
    .bind(wanted).all();
  const missing = results.filter(row => !row.profile_id).map(row => ({ id: row.id, profile: crypto.randomUUID().replaceAll("-", "") }));
  if (missing.length) {
    await env.ACCOUNTS.prepare(`UPDATE accounts a SET profile_id=x.profile FROM
      jsonb_to_recordset((?::text)::jsonb) x(id text,profile text) WHERE a.id=x.id AND a.profile_id IS NULL`).bind(JSON.stringify(missing)).run();
    const { results: assigned } = await env.ACCOUNTS.prepare(`SELECT id,profile_id FROM accounts WHERE id IN
      (SELECT jsonb_array_elements_text((?::text)::jsonb))`).bind(wanted).all();
    for (const row of results) row.profile_id = assigned.find(item => item.id === row.id)?.profile_id;
  }
  return new Map(results.map(row => [row.id, { ...row, name: shownName(row.display_name, row.username,
    row.held_name ? { display_name: row.held_name, reason: row.held_reason } : null) }]));
}
export async function witnessNames(env, witnesses) {
  const devices = [...new Set(witnesses.filter(w => !w.official).map(w => w.device))];
  let spans = [];
  if (env.ACCOUNTS && devices.length) {
    const { results } = await env.ACCOUNTS.prepare(`SELECT verifier,account,from_hour,until_hour FROM device_spans
      WHERE verifier IN (SELECT jsonb_array_elements_text((?::text)::jsonb))`).bind(JSON.stringify(devices)).all();
    spans = results;
  }
  const ownerOf = w => {
    const hour = (w.credited_at || w.completed_at).slice(0, 13);
    return spans.find(s => s.verifier === w.device && s.from_hour <= hour && (s.until_hour == null || hour < s.until_hour))?.account;
  };
  const accounts = await contributorAccounts(env, witnesses.filter(w => !w.official).map(ownerOf).filter(Boolean));
  return witnesses.map(w => {
    if (w.official) return { official: true, name: w.device.replace(/^official:/, ""), completed_at: w.completed_at };
    const account = accounts.get(ownerOf(w));
    return { name: account?.named ? account.name : null, profile: account?.named ? account.profile_id : null,
      completed_at: w.completed_at };
  });
}
