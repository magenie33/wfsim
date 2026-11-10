// SPDX-License-Identifier: AGPL-3.0-or-later
// Delivery names one calculator; accepted factory witnesses own verification and credit.
import { sessionAccount, sha256 } from "./accounts.js";
import { contributorAccounts, witnessNames } from "./contributor_identity.js";
import { COMPUTATION_KEEP_MS } from "./computation_records.js";
import { iso } from "./instant.js";
import { questionId } from "./task_contract.js";
import { ENGINE_ID } from "./verify.js";

export const deliveryKey = (question, device, at) => questionId("delivery", { question, device, at });
async function keepSource(env, { id, engine, weapon, owner, device, official = 0, at }) {
  await env.LIBRARY.batch([
    env.LIBRARY.prepare(`INSERT INTO state_public_computation_summaries (question,kind,engine,weapon,completed_at,expires_at)
      VALUES (?,'delivery',?,?,?,?) ON CONFLICT DO NOTHING`).bind(id, engine, weapon || null, at, iso(Date.parse(at) + COMPUTATION_KEEP_MS)),
    env.LIBRARY.prepare(`INSERT INTO relation_computation_witnesses (question,owner,device,official,completed_at,credited_at,position)
      SELECT ?,?,?,?,?,?,0 FROM state_public_computation_summaries WHERE question=? AND kind='delivery' ON CONFLICT DO NOTHING`)
      .bind(id, owner || device, device, official ? 1 : 0, at, at, id),
  ]);
  return id;
}
export async function answerDeliverySource(env, question, answer, weapon) {
  const id = await deliveryKey(question, answer.device, answer.at);
  return keepSource(env, { id, engine: answer.engine, weapon, owner: answer.owner,
    device: answer.device, official: answer.official, at: answer.at });
}
export async function localDeliverySource(env, request, body) {
  if (!/^[a-f0-9-]{36}$/.test(body.task || "") || !/^[a-f0-9]{64}$/.test(body.owner || "")
    || !/^[a-f0-9]{32}$/.test(body.profile || "") || !ENGINE_ID.test(body.engine || "")
    || !/^[a-z0-9_]{1,100}$/.test(body.weapon || "")) return null;
  const owner = await sessionAccount(env, request);
  if (!owner) return null;
  const account = (await contributorAccounts(env, [owner])).get(owner);
  if (account?.profile_id !== body.profile) return null;
  const id = await questionId("local_delivery", { owner: await sha256(body.owner), task: body.task, account: owner });
  return keepSource(env, { id, engine: body.engine, weapon: body.weapon, owner, device: "account:" + owner, at: iso(Date.now()) });
}
export async function deliveryName(env, source, fallback = null) {
  if (source) {
    const { results } = await env.LIBRARY.prepare(`SELECT w.* FROM relation_computation_witnesses w JOIN state_public_computation_summaries r
      ON r.question=w.question WHERE r.question=? AND r.kind='delivery' AND r.expires_at>? ORDER BY w.position LIMIT 1`)
      .bind(source, iso(Date.now())).all();
    if (results.length) return (await witnessNames(env, results))[0];
  }
  if (fallback?.verifier) return (await witnessNames(env, [{ device: fallback.verifier, official: 0, completed_at: fallback.at }]))[0];
  return { name: null, profile: null };
}
