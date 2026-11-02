# Compute task modules

`worker/tasks.js` preserves the factory API and composes independent modules.
Business types register verbs and producers in `worker/factory.js`.

| Module | Owns |
| --- | --- |
| `task_contract.js` | Canonical identity, witness independence and lifecycle constants |
| `task_ledger.js` | Demand registration, deduplication, withdrawal and durable state access |
| `task_dispatch.js` | Selection, claims, leases, renewal and release |
| `task_usage.js` | Compute-cost accounting used by fair scheduling |
| `task_verification.js` | Answers, agreement and verified facts |
| `task_delivery.js` | Consumer delivery and retries |
| `task_credit.js` | Idempotent contribution settlement and repayment |
| `task_observation.js` | Read-only health observations |
| `task_inventory.js` | Validation and publication of complete workload snapshots |

The modules import common contracts, not business implementations or each
other. The composition root supplies explicit interfaces. Dispatch receives
lease metadata, demand eligibility, lease notification and usage charging;
it receives no registration, verification, credit or result-delivery interface.
An alternative dispatcher implements the same selection and lease API and is
supplied through the factory's `dispatcher` option.

Verb metadata declares official-producer restrictions. A new registered verb
or producer requires no type-name branch in dispatch. Business callbacks remain
owned by their producers. Lease-notification failure is observed without
discarding the lease returned to the witness.

## Verification and delivery

Verified task state commits before consumer writes or contribution settlement.
Successful verification remains durable when either fails. Delivery batches
healthy consumers together; a failed batch is retried per consumer so one
failure does not hold successful consumers. Failed demands stay live for
redelivery. Failed contribution credits are repaid from durable answers.
The verifier commits `credit_work` and `credit_at` with its fact, preserving
its exact witness selection, corrected work and payment hour. Recovery consumes
these decisions without repeating verification or reconstructing its policy.
Unpaid decisions protect task detail from retention until payment completes.

Fact commitment marks live demands with `delivery_at` in the same transaction.
A request for an existing fact receives the same durable readiness marker,
regardless of that fact's age. Failed consumers retry after a minute; successful
delivery clears readiness and serves the demand. SQL consumers lock and check
the fact's engine and canonical result plus the demand's identity, registration
time and payload before applying writes. A superseded result or demand cannot
acknowledge replacement work. Pre-upgrade demands retain the bounded fact-time
cursor fallback.

Consumers must make delivery idempotent by their stable demand identity.
Neither a delivery retry nor a repayment creates a new computation. Normal
pack delivery has a bounded number of database calls independent of its rows.
External callbacks must also reject superseded business generations when they
apply side effects; their preflight check cannot span an external service.

## Schema and rollout

Before deploying the Worker or factory daemon, apply the additive delivery
upgrade to the library database as its owner. Print it from the schema source:

```sh
node scripts/task_schema.mjs > task-delivery.sql
psql -d wfsim -X -v ON_ERROR_STOP=1 -f task-delivery.sql
```

The output adds nullable delivery and payment readiness, creates their partial
indexes concurrently and installs delivery and retention guards. Run outside a transaction
and confirm `demands_delivery` and `answers_credit_pending` are valid and ready
before deploying code. Existing binaries
ignore the additive schema. A lock timeout or failed index build must be
resolved before deployment; an invalid concurrent index must be removed before
retrying. Deploy the Worker and daemon from the same validated commit, then
run normal queue reconciliation to publish the first complete inventory.
Until that scan succeeds, the page shows an unknown total. Rolling back code
retains the additive schema and inventory; no task or score backfill is needed.

## Complete demand

The engine's complete admitted missing-row scan and the registered board queue
form one deduplicated inventory by `(build_id, ruler, mode)`. Refused orphans
and retired rulers are excluded. Counting precedes scene-admission pacing.
Moving a row into the dispatch queue does not change its inventory membership.

The scan publishes a compact snapshot in `factory_findings.task_inventory`
through `POST /api/board/inventory`, authenticated with the factory token.
The pipeline role needs no access to task or account tables. The board adapter
owns its declared coverage; a submitted scan cannot replace other task kinds.
The publication statement adds registered active questions outside the scan's
declared producer coverage, including new registered task kinds and official
spot checks. It records scan start, completion and source commit. An older
scan cannot overwrite newer inputs, and a partial or invalid scan publishes
nothing. Consumers read this as a derived snapshot, never as task input.

The community goal displays the last complete inventory and its update time.
It changes when a complete scan is published; the minute's work signal remains
a separate live reading. A missing or older-than-three-hours inventory shows an
unknown total and no clearance estimate. Queue totals remain available for
dispatch health and never substitute for complete demand.

`check_task_modules` holds dependency boundaries, a new verb and producer,
substituted dispatch, failed consumers, failed credit and idempotent recovery.
`check_task_inventory` holds the full union, admission invariance, completion,
orphan/retired exclusion, extensibility, freshness and older-scan rejection.
