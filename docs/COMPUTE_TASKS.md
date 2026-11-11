# Compute task modules

`worker/tasks.js` preserves the factory API and composes independent modules.
Business types register verbs and producers in `worker/factory.js`.

Official scoring claims overdue questions before reading inputs.
`fetch_score_inputs.mjs` resolves the claimed board references through indexed
queue, build and fact keys in one repeatable-read snapshot. Its disk spool
deduplicates aliases and preserves batch time/key order for every shard.
It retains at most 40,000 claimed questions, reads 256 calculation keys per
database call and exports only their builds and measurements. Cancelled batches
and missing builds do not enter the scoring input. The release job returns
unanswered leases even when preparation fails.

Scoring never enumerates the complete library or reconciles the queue.
Missing demand registration belongs to queue maintenance's backfill; it cannot
bypass leasing by being handed directly to a scorer. Complete backup remains
owned by `backup.yml` and the database backup service; the scoring artifact is
only the current run's input.

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

## Computation identity and admission

A computation ID is the first 32 hexadecimal characters of SHA-256 over the
operation, a newline and canonical entity inputs. The operation declares the
required output: a board measurement, a best-build search or a complete report.
Inputs include the build, fight, seed, repetitions and search scope or budget.
Engine and release identifiers, review tokens, retention windows, upload
consent, source labels and executor thread counts do not participate.

`/api/computation/input` materializes calculation inputs through the actual
fight and search parsers. Simulation uses the existing build canonicalizer,
retaining element pairing and the exilus seat. A riven is identified by its
submitted stats, rolls, rank and polarity; local card names remain source
context. Optimization retains start order and slot positions because the
search algorithm consumes them. Missing mode retains its constructor semantics.
The page, bot and family intake use this same endpoint or Rust function.
`input_schema=1` identifies that admission contract without entering the hash.
An older request producer must reload before admitting persistent shared work;
volunteer execution capabilities remain unchanged.

`compute_admit_tasks` admits inputs, the current execution round, every trigger
snapshot and current consumer links in one transaction. A nonempty registered
producer, reference and trigger identity are required. Repeating a trigger is
idempotent; changing its payload is an identity conflict. A batch retains all
trigger snapshots while its last occurrence sets the current consumer link.
Deadlines and waiting priority may change without replacing a held lease.

`state_compute_scheduling.id` is the unique computation key. Taking a lease
keeps that entry in the queue; another source joins it. Independent witnesses
are attempts within that same round, rather than new queue entries. An accepted
result is reused across deployments. Retention expiry or an explicit review
can open a later round under the same computation ID. Confirmed invalidation
fences the previous round before opening its replacement; a repeated correction
cannot open another round. Result engine and submission engine remain separate
provenance. Old candidates cannot certify a replacement round implicitly.

`relation_compute_task_sources` preserves trigger snapshots separately from
`relation_task_consumers`, which owns current delivery. A continuation carries
at most the latest snapshot per consumer, while prior rounds retain their own
triggers. Task archives include these source relations. Every producer must
register its durable source lookup in `compute_source_exists` as well as its
business callbacks.

## Identity cutover and source cleanup

Apply the additive task schema and daemon grants before publishing matching
Worker, native, WASM and bot code. During an identity cutover, fence application
writers with the recorded database-role fence. Use the validated native engine
on loopback to prepare bounded plans with `scripts/migrations/compute_identity.mjs`.
The operator-only apply path checks the actual fence, matches the original
round and consumers, and commits each replacement atomically. It preserves raw
inputs, prior results and a `record_compute_identity_changes` receipt; request
and consumer references move to the standardized computation. Replaying a plan
is idempotent. Retained source records cannot be replaced with inferred history.

`scripts/compute_queue_sources.mjs` audits at most 1,000 active entries per page.
Its default is read-only; `--apply` removes only entries without any resolvable
live business request or plan. A missing initiator is not evidence of a missing
source. Removal rechecks sources under the queue row lock, clears current and
previous leases, withdraws consumers and records `record_compute_queue_removals`.
Builds, frozen inputs and accepted results remain. The returned cursor and
observation time belong to that page; they do not claim a complete queue count.
A subsequent valid trigger can admit the same computation again.

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
node scripts/computation_schema.mjs accounts > computation-accounts.sql
psql -d wfsim_accounts -X -v ON_ERROR_STOP=1 -f computation-accounts.sql
```

The library upgrade also installs computation records, witness snapshots,
signature counters, score replacement cleanup and optimization retention. Apply
the account upgrade as its database owner and then the daemon grants. Existing
details are shortened only for optimization and appraisal. Public names must
not be enabled before both database upgrades have succeeded.

The output adds nullable delivery and payment readiness, creates their partial
indexes concurrently and installs delivery and retention guards. Run outside a transaction
and confirm `demands_delivery` and `answers_credit_pending` are valid and ready
before deploying code. The same upgrade installs the restricted inventory reader
and grants its execution to the existing pipeline role. Existing binaries
ignore the additive schema. A lock timeout or failed index build must be
resolved before deployment; an invalid concurrent index must be removed before
retrying. Deploy the Worker and daemon from the same validated commit, then
run normal queue reconciliation to publish the first complete inventory.
The compute page reads queued orders independently of that scan. Rolling back code
retains the additive schema and inventory; no task or score backfill is needed.

## Complete demand

The stable `calculation_id` is `build_id|ruler|mode` (`calculationId` in code).
Dates and batches are not part of it. Each pending calculation has one
`(build_id, ruler, mode)` queue row; the wire continues carrying this ID as `ref`.
Repeated batches and refresh rounds merge into its original batch and may fill
an absent initiator. Only newly inserted calculations increase a pipeline batch's total.
A unique index enforces the calculation key; a completed row may enter a later
round. Periodic refreshes wait fourteen days after the last measurement.
Explicit board corrections retain their immediate independent-review path.

The demand projection gives a full background-read statement budget when queue
counts or compute-cost estimates need refreshing. The normal budget applies
only while both caches remain valid; their refresh intervals are independent.
`counts_at` retains the queue count's sampling time across file publications.
Pending queue keys and consumer keys are unique, so their counting join needs
no intermediate grouping of every build, ruler and mode.

Each order's `sources` partitions its total into `requested`, `system` and
`unattributed`, using its recorded initiator. Anonymous, account and bot request
tags count as requested; the explicit system tag counts as system; absent or
unrecognized tags remain unattributed. These counts describe recorded causes,
not distinct people, execution devices or complete historical derivation chains.
Report orders use their current scheduling producer, the earliest live demand,
so shared computations count once and family searches retain their own category.

Full reconciliation captures builds, scores, queue rows and task lifecycle in one
read-only repeatable-read transaction. A lost connection aborts the capture;
partial files never become a published inventory. The engine walks that
snapshot with the existing admission gate before scene pacing. Retired rulers
and refused orphan rows are excluded.

GitHub-hosted Linux full queue maintenance adds an ephemeral 24 GiB swap
file while preserving existing swap. Capture and classifier scratch retain at
least 60 GiB free after allocation; insufficient capacity releases only unused
Android, .NET and GHC SDKs, then stops preparation if the reserve is still short.
Each ruler's temporary output is removed after its ordered merge succeeds.
The swap and scratch reserve are checked again after cache restoration, builds
and intake, immediately before the full capture.
Full reconciliation defaults to two concurrent ruler walks;
`WFSIM_INVENTORY_WORKERS` overrides that budget. The standalone classifier
uses a quarter of available memory for its SQLite pager, capped at 4 GiB.
In-process callers retain the 16 MiB default unless they supply a validated
`cacheKiB` budget, at most 16 GiB. These settings change resources only.

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

The queue workflow's `inventory_only` input skips intake and publishes the same
complete snapshot, including insert-only canonical build bindings. It stops
before queue admission or cleanup and rejects rescore, weapons, why, day and
cancel inputs. These scans serialize in an independent concurrency group so a
queue mutation run cannot block a requested count. Normal scheduled
reconciliation keeps its existing path.

Each full-inventory phase reports progress every thirty seconds. Its process
RSS, peak RSS, swap, available host memory, scratch space, Linux memory/I/O
pressure and cgroup memory events are recorded when available. The final exit
report retains CPU time and peak RSS; periodic readings survive a lost runner.
Native enumeration retains source records as JSON text for refusal diagnostics;
parsed records are temporary, and validated builds are shared across rulers.
`--inventory-workers 1..8` bounds concurrent ruler walks without changing their
admission rules or ordered outputs. Library normalization retains its shared pool.

### Incremental inventory

`live_board.sh` defaults to the guarded inventory coordinator
(`INCREMENTAL_INVENTORY=1`). Explicit `INCREMENTAL_INVENTORY=0` selects legacy
full-library reads.
It uses the existing ranker command pipes, with one kernel-held SQLite writer
lock around a cycle. The ranker starts empty; a full bootstrap requires an
explicit durable rebuild task. This
path replaces the ranker's hourly library reload and rolling score
read with authoritative source updates. Normal queue maintenance retains full
reconciliation as a fallback. Official scoring reads only its leased inputs.

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
The coordinator sends at most 128 existing 256-key ACK chunks per SQL request,
targeting 4MiB of UTF-8 input; an existing larger chunk travels alone. Every
chunk's local receipt is durable before sending. One SQL statement invokes
the unchanged validator and exact-revision ACK for each chunk, so a failure
rolls back the whole page. Local acknowledgments follow database success;
a lost response replays the durable receipts without counting them twice.

A generation is fenced by engine, adapter and classifier contract. Bootstrap
compares every effective question id, category and registration flag with the
full classifier before activation. Deployment, process restart, a missing
catalog and a changed contract never authorize a full rebuild. The classic
ranker's memory is not restored from its catalog: a different native instance
reports `needs_restore` and preserves the existing catalog and published files.
The published goal retains its normal engine and freshness checks.
Engine changes reuse legitimate score facts and never request a rescore.

`inventory/controller.sqlite` separates immutable rebuild requests, mutable
task state and execution events. A request pins the mode, engine, adapter and
classifier contract, expected PostgreSQL generation, reason and disk budget.
Only a matching unconsumed request may call `inventory_begin`. An uncertain
generation start or interrupted full capture requires explicit attention;
background retries cannot start another generation or repeat the full capture.
A sealed pending cut can resume after its receipt, manifest and input hashes
are verified. Previously verified unchanged files need only metadata checks.

The operator records a request with the deployed scripts and binaries:

```sh
node scripts/inventory_rebuild.mjs status --work "$WORK"
node scripts/inventory_rebuild.mjs request --work "$WORK" --bin "$BIN" \
  --mode classic --reason "approved full inventory rebuild" \
  --max-new-bytes "$INVENTORY_REBUILD_MAX_BYTES"
node scripts/inventory_rebuild.mjs cancel --work "$WORK" \
  --task "$REBUILD_TASK_ID" --reason "cancel this attempt"
```

`WORK` and `BIN` are absolute paths; request also requires `PG_URL`.
`INVENTORY_REBUILD_MAX_BYTES` is an explicitly assessed growth budget, with no
default. Admission checks that budget plus an 8GiB reserve against current
free space and pins the filesystem. These checks do not qualify a full-scale
bootstrap's storage or time requirements. Request records intent without
capturing inputs; the next matching controller cycle consumes it. Status is
readable during a running cycle. Request and cancellation refuse immediately
while a controller cycle holds its lock. Cancellation
retains partial inputs, catalogs and the task history. A new attempt needs a
new request. Use `--mode relational` for the relational coordinator.

Blocked inventory reports `needs_bootstrap`, `needs_restore`, `needs_resume`,
`request_changed` or `needs_space` in `inventory/status.json`. The live loop
keeps its ranker process and published files, stops its active uploader, skips
publication and waits its normal
poll interval. It does not restart the ranker to resolve a blocked inventory.

Publication uses the sealed cut's database time and snapshot, even if later
changes are already pending. It never advances freshness to the publication
time. Interrupted or expired cuts cannot publish. After durable publication,
cleanup may remove only that completed task's capture directory. Protected
directories, unrelated captures and historical catalog generations remain;
startup does not collect them. The last publication manifest remains.
SQLite uses a 16MiB page cache, bounded
imports and no memory mapping; the coordinator reserves 8GiB of filesystem
space and caps initial capture at 45 minutes and delta capture at 15 minutes.

Dirty-token export retains one cursor page at a time, starting at 5,000 rows
and growing at most fourfold to 80,000. The largest observed UTF-8 row sizes
the next page toward 8MiB; this is a sizing target, not a hard byte limit for
unseen rows. Cohort pages retain their 5,000-row ceiling. Each FETCH has its
own statement timeout within the original capture budget and snapshot. Phase
diagnostics record page rows, bytes, fetch time and the next requested size.
The restricted `inventory_epoch_open` function validates the owning receipt
and opens a transaction-local cursor over the primary-key index. It returns
the portal name, avoiding a whole-result PL/pgSQL `RETURN QUERY` spool. The
pipeline receives function execution rights without access to private tables;
the legacy set-returning API remains available to older callers.

Generation replacement and sealed-cut publication lock the generation state,
not the dirty table. Source writers can commit during both operations: a cut
owns its repeatable-read snapshot and later revisions remain pending. The
legacy empty-dirty publisher retains its table barrier because it promises an
empty current backlog rather than a sealed snapshot.

### Relational inventory runner

Both inventory runners write atomic phase status and bounded rotating events
under the work directory's `diagnostics/`. Source, engine, generation, epoch,
run id, row counts and process memory identify each attempt. Heartbeats update
`updated_at` without moving `last_progress_at`; a failed or interrupted run
retains its last phase and safe error code. Official scoring uses the same
diagnostics for claiming and input capture, uploaded even on failure with a
fourteen-day artifact lifetime. Diagnostic output goes to stderr so ranker
command pipes remain intact.

`inventory_relational_pipeline.mjs` is an alternative, explicitly selected
inventory runner. It requires the existing inventory schema and a binary
directory containing `wfsim-board`, `ENGINE` and `VERSION`:

```sh
mkdir -p /path/to/work
flock -n /path/to/work/inventory.lock node --max-old-space-size=192 \
  scripts/inventory_relational_pipeline.mjs /path/to/work /path/to/bin
```

Invoke another cycle while the returned `pending` is true; otherwise wait at
least a minute. The runner does not install a service or change `live_board.sh`.
Select exactly one inventory coordinator for a database generation. The
ranker coordinator and this runner must not run concurrently, even in separate
work directories. Board publication and queue admission remain separate
consumers; a published inventory count never creates or cancels work.

The native `--inventory-operator` supplies canonical records, admitted ruler
bits, modes, default dependencies, ruler families and the authoritative entry
decision. It holds no library. Requests are limited to 256 items and 8MiB.
The board adapter stores these relationships in SQLite and maintains group
maxima, reverse dependencies and score intervals through indexes. Semantic
comparison keys come from the engine; they do not define execution batches.
Unchanged maxima propagate nothing. Changed maxima visit the conservative
threshold-crossing interval, then ask the engine for the exact final decision.
Parents also invalidate their dependent rank and roll variants.

The first generation streams a complete source snapshot into disk indexes.
Subsequent cuts capture changed identities and lifecycle records without
expanding them into weapon cohorts. Adaptive input pages target 100ms within
the operator's item limit. Byte offsets, pending ranges, candidate jobs and
revision receipts survive process restart. A completed projection precedes
acknowledgment, and source updates after the cut retain their own dirty token.
Publication keeps the captured database clock. An expired cut retains its
baseline and takes a new delta cut; it does not renormalize history.

Engine, binary or algorithm changes require an explicit new rebuild task.
Compatible sealed input and relational checkpoints resume in the same
generation after a stateless native operator restart.
Bootstrap audits the question union with the disk classifier; CI independently
compares membership with the complete native walk. This is a board adapter
with explicit dependencies, not a general incremental SQL planner. A change
affecting every membership still requires proportional output work. The
indexed maximum query may examine multiple high scores when target admission
filters them; the time budget bounds cycles softly, not individual SQL calls.

Apply the additive task schema before enabling this path. When recovery is
required, preserve source tables, checkpoints and the last published inventory
and resolve the reported action before resuming. The legacy
`INCREMENTAL_INVENTORY=0` mode performs full library reads; switching modes is
an explicit operational decision and does not restore an inventory checkpoint.

The community goal sums `demand.json`'s `orders` and displays its update time.
These counts describe actual pending queue entries; admission owns which work
enters them. Leased work remains pending until completion. The page does not
expand the library into prospective work or require an inventory scan. Missing,
expired and different-engine inventory fields cannot hide queued orders.
An empty order list is zero; an unavailable reading is unknown. Failed refreshes
retain the previous reading and its original update time.

The page displays the demand producer's optional queue `clear_hours` estimate
at the last 24 hours' average compute. Missing cost samples or capacity leave
that estimate unknown without hiding the order count. The inventory's separate
clearance observation remains bound to its engine, snapshot and input time.

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
