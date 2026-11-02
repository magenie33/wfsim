// SPDX-License-Identifier: AGPL-3.0-or-later
// Compatibility API for the task modules. Business types register in factory.js.
export * from "./task_contract.js";
import { createLedger } from "./task_ledger.js";
import { createUsage } from "./task_usage.js";
import { createDispatch } from "./task_dispatch.js";
import { createVerification } from "./task_verification.js";
import { createDelivery } from "./task_delivery.js";
import { createCredit } from "./task_credit.js";
import { createObservation } from "./task_observation.js";

export function factory({ verbs, producers, threshold = 2, spot = 0.05, recheck = 0, random = () => Math.random(), on_disproved = null, refusing = true,
  dispatcher = createDispatch }) {
  const producerOf = (name) => {
    const p = producers[name];
    if (!p) throw new Error(`no producer ${name}`);
    return p;
  };
  const usage = createUsage();
  const ledger = createLedger({ verbs, producerOf });
  const delivery = createDelivery({ producerOf, liveDemands: ledger.liveDemands, commit: ledger.commit });
  const demandAll = async (env, asks, engine, now = Date.now()) => {
    const registered = await ledger.demandAll(env, asks, engine, now);
    try { await delivery.deliverMany(env, registered.ready, now); }
    catch (error) { console.log(JSON.stringify({ event: "task.delivery_pending", error: String(error) })); }
    return registered.ids;
  };
  const demand = async (env, ask, engine, now = Date.now()) => (await demandAll(env, [ask], engine, now))[0];
  const credit = createCredit();
  const onLease = async (env, demand, now) => {
    try { await producerOf(demand.producer).on_lease?.(env, demand, now); }
    catch (error) { console.log(JSON.stringify({ event: "task.lease_notice_failed", producer: demand.producer, error: String(error) })); }
  };
  const dispatchVerbs = Object.freeze(Object.fromEntries(Object.entries(verbs).map(([name, v]) => [name,
    Object.freeze({ lease_ms: v.lease_ms, pack: v.pack,
      official_producers: v.official_producers && Object.freeze([...v.official_producers]) })])));
  const dispatch = dispatcher({ verbs: dispatchVerbs, onLease, liveDemands: ledger.liveDemands, sift: ledger.sift,
    restate: ledger.restate, restateAll: ledger.restateAll, charge: usage.charge });
  const verification = createVerification({ verbs, producers, producerOf, threshold, spot, recheck, random, on_disproved, refusing, ...usage, ...credit, ...delivery, ...ledger,
    deliverPlan: delivery.schedule, release: dispatch.release });
  const observation = createObservation();
  return {
    demand, demandAll, withdraw: ledger.withdraw,
    next: dispatch.next, nextPack: dispatch.nextPack, headOf: dispatch.headOf, packable: dispatch.packable,
    renew: dispatch.renew, renewRow: dispatch.renewRow, renewMany: dispatch.renewMany, release: dispatch.release,
    releaseAll: dispatch.releaseAll, releaseAllOf: dispatch.releaseAllOf, releaseUnnamed: dispatch.releaseUnnamed, refuseRows: dispatch.refuseRows,
    claim: dispatch.claim, answer: verification.answer, answerPack: verification.answerPack, answerOfficial: verification.answerOfficial,
    settle: verification.settle, settleQueued: verification.settleQueued, refuse: verification.refuse, syncRules: verification.syncRules,
    redeliver: delivery.redeliver, repay: credit.repay, liftIdle: usage.liftIdle, audit: observation.audit,
  };
}
