// SPDX-License-Identifier: AGPL-3.0-or-later
// Public identity is stable; names and visibility are resolved from current account state.
import { shownName } from "./names.js";
import { cloudMarks } from "./cloud.js";

const marksCache = new Map(), MARKS_KEEP_MS = 10 * 60_000;
export async function contributorMarks(env, people, wait = false) {
  const named = [...people.values()].filter(person => person.named && person.label && person.label !== "none");
  const key = person => `${person.id}:${person.label}:${person.chosen_at || ""}`;
  const marks = {}, missing = [];
  for (const person of named) {
    const cached = marksCache.get(key(person));
    if (cached && Date.now() - cached.at < MARKS_KEEP_MS) { if (cached.mark) marks[person.id] = cached.mark; }
    else missing.push(person);
  }
  if (!missing.length) return marks;
  const asking = (async () => {
    const requests = [];
    for (let at = 0; at < missing.length; at += 100) requests.push(cloudMarks(env, missing.slice(at, at + 100).map(person => person.id)));
    const parts = await Promise.all(requests);
    if (parts.some(part => part === null)) return null;
    const found = Object.assign({}, ...parts);
    if (marksCache.size > 20_000) marksCache.clear();
    for (const person of missing) {
      const mark = found[person.id]?.kind === person.label ? found[person.id] : null;
      marksCache.set(key(person), { at: Date.now(), mark });
      if (mark) marks[person.id] = mark;
    }
    return marks;
  })();
  if (wait) return asking;
  let timer;
  const timeout = new Promise(resolve => { timer = setTimeout(() => resolve(marks), 4000); timer.unref?.(); });
  try { return (await Promise.race([asking, timeout])) || marks; } finally { clearTimeout(timer); }
}

export async function contributorAccounts(env, ids, byProfile = false) {
  if (!env.ACCOUNTS || !ids.length) return new Map();
  const wanted = JSON.stringify([...new Set(ids)]);
  const { results } = await env.ACCOUNTS.prepare(`SELECT a.id,a.profile_id,a.username,a.display_name,c.named,c.label,c.chosen_at,
    h.display_name AS held_name,h.reason AS held_reason FROM entity_accounts a LEFT JOIN state_account_public_attribution c ON c.account=a.id
    LEFT JOIN state_account_name_holds h ON h.account=a.id WHERE a.${byProfile ? "profile_id" : "id"} IN (SELECT jsonb_array_elements_text((?::text)::jsonb))`)
    .bind(wanted).all();
  const missing = results.filter(row => !row.profile_id).map(row => ({ id: row.id, profile: crypto.randomUUID().replaceAll("-", "") }));
  if (missing.length) {
    await env.ACCOUNTS.prepare(`UPDATE entity_accounts a SET profile_id=x.profile FROM
      jsonb_to_recordset((?::text)::jsonb) x(id text,profile text) WHERE a.id=x.id AND a.profile_id IS NULL`).bind(JSON.stringify(missing)).run();
    const { results: assigned } = await env.ACCOUNTS.prepare(`SELECT id,profile_id FROM entity_accounts WHERE id IN
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
    const { results } = await env.ACCOUNTS.prepare(`SELECT verifier,account,from_hour,until_hour FROM record_compute_device_ownership_intervals
      WHERE verifier IN (SELECT jsonb_array_elements_text((?::text)::jsonb))`).bind(JSON.stringify(devices)).all();
    spans = results;
  }
  const ownerOf = w => {
    if (w.device === "account:" + w.owner) return w.owner;
    const hour = (w.credited_at || w.completed_at).slice(0, 13);
    return spans.find(s => s.verifier === w.device && s.from_hour <= hour && (s.until_hour == null || hour < s.until_hour))?.account;
  };
  const accounts = await contributorAccounts(env, witnesses.filter(w => !w.official).map(ownerOf).filter(Boolean));
  const marks = await contributorMarks(env, accounts);
  return witnesses.map(w => {
    if (w.official) return { official: true, name: w.device.replace(/^official:/, ""), completed_at: w.completed_at };
    const account = accounts.get(ownerOf(w));
    return { name: account?.named ? account.name : null, profile: account?.named ? account.profile_id : null,
      ...(account?.named && marks[account.id] ? { mark: marks[account.id] } : {}),
      completed_at: w.completed_at };
  });
}
