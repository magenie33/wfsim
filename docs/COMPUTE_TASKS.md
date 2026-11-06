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
cursor fallback. Each pass scans one time window, advancing through empty
windows and retaining failed facts for retry. Readiness delivery does not wait
for this historical cursor.

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
before deploying code. The same upgrade installs the restricted inventory reader
and grants its execution to the existing pipeline role. Existing binaries
ignore the additive schema. A lock timeout or failed index build must be
resolved before deployment; an invalid concurrent index must be removed before
retrying. Deploy the Worker and daemon from the same validated commit, then
run normal queue reconciliation to publish the first complete inventory.
Until that scan succeeds, the page shows an unknown total. Rolling back code
retains the additive schema and inventory; no task or score backfill is needed.

## Complete demand

Full reconciliation captures builds, scores, queue rows and task lifecycle in one
read-only repeatable-read transaction. A lost connection aborts the capture;
partial files never become a published inventory. The engine walks that
snapshot with the existing admission gate before scene pacing. Retired rulers
and refused orphan rows are excluded.

GitHub-hosted Linux queue and scoring preparation add an ephemeral 24 GiB swap
file while preserving existing swap. Capture and classifier scratch retain at
least 60 GiB free after allocation; insufficient capacity releases only unused
Android, .NET and GHC SDKs, then stops preparation if the reserve is still short.
Each ruler's temporary output is removed after its ordered merge succeeds.

The scan also emits current canonical build records through the engine's
existing normalization, preserving mod ranks, assembly, wielder and explicit
riven rolls. A stored record wins for an identity already in the snapshot.
An absent current identity is bound to its existing source input before its
rows are asked. These insert-only bindings preserve source timestamps and
use no arrival or family-search path. Orphan rows and retired rulers require
no binding. A binding with no canonical source rejects the inventory.

The board adapter derives each candidate's canonical `simulate` question id
from its stored record, ruler and mode. Equal questions count once across
batches, build aliases and consumers. Current-engine verified facts awaiting
delivery count as zero computation. A spot check counts once even when its
board result is still owed. Independent registered consumers and current
verification work join the same question-id union. A missing build record or
incomplete lifecycle export rejects publication.

`factory_inventory_rows` exposes ids, lifecycle, verbs and live consumer kinds;
it exposes no requests or witness identity. Its owner-controlled search path
is captured when the schema is installed. Only the pipeline role receives
execution permission; it receives no direct task-table grants. Business
registration, cancellation and eligibility remain owned by their producers.

The authenticated inventory endpoint stores the classified snapshot without
adding a later database reading. It records input time, source commit, engine
and database snapshot. An older scan cannot overwrite newer inputs, and an
engine change invalidates its cached goal. The snapshot is derived observation
and never task input.

### Incremental inventory

`INCREMENTAL_INVENTORY=1` enables the inventory coordinator in `live_board.sh`.
It uses the existing ranker command pipes, with one filesystem lock around a
cycle. The ranker starts empty and derives the coherent bootstrap once. This
path replaces the ranker's hourly library reload and rolling score
read with authoritative source updates. The normal queue and scoring workflows
retain full reconciliation as a fallback.

Live board uploads run in one background process with a per-request timeout.
Confirmed file hashes are flushed through an atomic checkpoint replacement;
the captured board stamp is sent after its files. Uploads do not hold the
inventory loop, and a failed uploader resumes from its confirmed checkpoint.

`worker/pg/inventory.sql` records coalesced build and question invalidations in
the same transaction as source changes. Revisions are equality tokens, never a
commit-order cursor. Lease, heartbeat, delivery and credit updates do not
materialize task records for inventory. Restricted functions expose source
inputs; the pipeline receives no direct access to the inventory tables.

Each update opens one repeatable-read receipt transaction. It captures every
pending key, changed records and lifecycle states, and complete affected weapon
cohorts. Canonical aliases join their source identities when reading facts and
queue membership. The source files are flushed and hashed before the database
seals the cut. Source writes are forbidden in this receipt transaction.

The ranker normalizes changed sources through its existing library cache. Its
cohort command uses the same admission and entry-line helpers as a full walk,
including each target ruler's admitted identities, default corners and retired
facts. It emits complete owner replacements in bounded chunks. New source
changes remain pending for the next cut while the captured cut finishes.

The disposable SQLite catalog stores canonical 128-bit question ids and owner
references. Stored records take precedence over normalized fallbacks. Aliases,
batches and independent consumers join the same question; verification retains
the full classifier's precedence. Replacement commits before acknowledgment,
and acknowledgments compare the captured revision. A persisted token ledger
makes replay after local acknowledgment idempotent.

A generation is fenced by engine, adapter and classifier contract. Bootstrap
compares every effective question id, category and registration flag with the
full classifier before activation. A ranker restart starts a shadow rebuild
from source tables. The previous catalog remains available while a replacement
is built; the published goal retains its normal engine and freshness checks.
Engine changes reuse legitimate score facts and never request a rescore.

Publication uses the sealed cut's database time and snapshot, even if later
changes are already pending. It never advances freshness to the publication
time. Interrupted or expired cuts cannot publish. Completed input spools are
removed after publication; current and previous catalog generations and the
last publication manifest remain. SQLite uses a 16MiB page cache, bounded
imports and no memory mapping; the coordinator reserves 8GiB of filesystem
space and caps initial capture at 45 minutes and delta capture at 15 minutes.

Apply the additive task schema before enabling this path. Rollback sets
`INCREMENTAL_INVENTORY=0` and restarts the live service on the current validated
release, preserving engine consistency with the other channels. Source tables
and the last published inventory are preserved.

The community goal displays the last complete inventory and its update time.
It changes when a complete scan is published; the minute's work signal remains
a separate live reading. A missing or older-than-three-hours inventory shows an
unknown total and no clearance estimate. Queue totals remain available for
dispatch health and never substitute for complete demand.

The queue and scoring pipelines bound their intake step independently of
reconciliation. A failed or timed-out intake retains unacknowledged arrivals
and still permits one complete scan of committed data. The workflow reports
the intake failure after reconciliation; a published inventory does not turn
a failed intake into a successful run.

`check_task_modules` holds dependency boundaries, a new verb and producer,
substituted dispatch, failed consumers, failed credit and idempotent recovery.
`check_task_inventory` holds the full union, admission invariance, completion,
orphan/retired exclusion, extensibility, freshness and older-scan rejection.
`check_incremental_inventory` covers the catalog, frozen cuts, replay and exact
full-classifier audit. `check_incremental_inventory_pg` covers transactional
invalidation, restricted source reads, receipt-only writes and cut publication.
`check_inventory_pipeline` uses the actual ranker and an independent full engine
walk across source changes, retirement, alias deletion and durable processing.
