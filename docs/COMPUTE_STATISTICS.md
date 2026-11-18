# Compute statistics

`statistics_provider.js` supplies one sealed source to the complete factory
snapshot and its detailed measurements. The public queue count has its own clock.
The fast work signal and owner heartbeat have independent clocks and connections.

`pause statistics` gates the provider before reading source state and before
publishing. `resume statistics` continues the current baseline or dirty cursor.
The existing background connection runs a single flight, advancing at most 128
steps or 250 ms before yielding. Each projection operation retains its input,
byte, query, storage and three-second transaction budgets. The existing capacity
duty controller schedules its next turn and other background jobs retain their
turns. `FACTORY_STATISTICS_CATCHUP_DUTY_PCT` optionally allows a higher duty
for active unsealed projection work. It uses the normal duty until the critical
jobs have reported, under pressure or after failure. The higher duty expires
when the first complete sample seals. The provider has a one-connection control lane, independent of the fast
signal. It verifies remote control before and after each bounded transaction.

## Public queue counts

The queue owns the total shown by the compute page. `worker/pg/queue_counts.sql`
captures changes to effective open membership and explicit cancellation in the
source transaction. Each database connection updates its own small per-zone
balance. Completed transactions become visible immediately, including when
the publishing process restarts. Inactive writer balances fold into one row
per zone in bounded batches. Leases and shared consumer changes leave the
total intact.

An activation selects a new baseline. `queue_counts.js` reads disjoint
indexed pages in one repeatable-read snapshot and subtracts the changes
visible to that snapshot before saving its offset. The sum of offsets and
committed balances gives the current zones in a single short read. A failed
initialization retries and an incomplete sample stays private. Install the
schema in the target tablespace and apply `deploy/queue_counts_grants.sql`;
the existing factory maintenance clock seeds a missing epoch. The public
reader and optional fleet readings use independent clocks. The existing
`demand.json` carries `queue.total`, ascending `queue.zones` pairs and
one compatible `orders` item per zone. Its total and timestamp continue to
refresh while throughput or detailed measurements are unavailable.

## Membership and sampling

Canonical effective work remains authoritative. The derived unit stores narrow
contributions and rolls their differences into counts; it admits no work and
changes no task, result, cancellation or credit.

Physical pending identity is `(batch,build,ruler,mode)`. An eligible live consumer
contributes once globally for that identity, independent of matching consumer
count. The post-cut unregistered arm is a separate `UNION ALL`, retaining the
original complete-ref guard; a suffix can legitimately contribute twice. With
no active cut all physical pending rows count. Orphan batches count in physical
pending but do not enter owed orders. Source classification, live new-build
override, withdrawn facts awaiting delivery, exact question/task cancellation,
FIFO, protection, held leases and expiry retain the canonical rules.

Thirteen source relations capture old and new narrow keys: scheduling,
consumers, pending, batches, tasks, results, answer evidence, verification,
cancellation, protection, cuts, activation and inventory findings. Source-row
primary keys own capture revisions; transactions on different source rows do
not share a ref journal lock. A bounded transfer atomically registers logical
work and its owner/revision checkpoint. Fanout cursors and contribution changes
commit together. Short cleanup uses exact revision CAS.
Evidence changes capture keys only when they can affect membership. Inserts
for already-retained work use the order write guard to skip redundant replay.
Settlement details remain available to the sealed pace read.
The baseline advances while replay has room for another page of keys.
Ordinary questions, references and pending rows use bounded set operations
with the same contribution replacements as their resumable cursors. Wide
fanout and ambiguous references retain their individual cursor. A timed-out
operation retries a smaller page from its last committed position.
With an active cut, the question scan seeks the retained queue prefix, tasks
after the cut, protected questions and the two non-open state ranges. It
converts the former question cursor into a lower bound so a rolling upgrade
can keep the work already committed. Ordinary retained questions commit their
contributions and downstream keys with the range checkpoint. Deep consumer
lists become obligations for the existing fanout cursor. Each range follows
its own indexed order, and contributions apply the canonical eligibility views.

A baseline is never published. A repeatable-read tail seals only after every
baseline page is checkpointed, every visible revision is transferred, all
logical fanout is complete and due lease expiries are applied. Exceeding a
budget rolls back the seal and retains work. Asking, computing, settlement,
inventory, cost estimates and the complete demand wire are read in that same
snapshot. Snapshot `at`, demand `at` and `counts_at` all retain its sampling
time. Publishing the cached object never refreshes them.

Projection identity consists of the cut epoch, generation, explicitly published
inventory `(engine,source,snapshot)`, canonical SQL fingerprint and derived
contract. Inventory `source` names its producer version, not the daemon's
current release. The served engine is sampled in the published snapshot. A new
served engine can seal again from the same projection, including legacy
namespaces with an engine in their stored identity.
The current identical head resumes on restart; reentering a retired identity
creates a new incarnation and baseline. Unknown canonical SQL fails closed.
The production default retains at most two namespaces. The identical current
head can resume when that budget is full; a third incarnation stops before
allocating a revision, changing head or modifying retained evidence. Status
reports `capacity_hold`. Runtime options may lower this budget; they cannot
raise it. Membership tests retain their fixture incarnations through an explicit
source copy; the unmodified module rejects that test budget.
Pause, owner stop, source change, head/clock change, capture halt and dependency
loss are checked around awaits, at the last SQL write and before the R2 PUT.

## Explicit installation

Runtime and ordinary daemon bootstrap perform no statistics DDL or source-index
installation. The installer only renders SQL; applying it requires a separately
approved capacity and deployment window. The operator must identify the actual
filesystem behind the selected tablespace and budget retained state, index
build space, WAL and temporary files. A tablespace name alone proves no placement.

Render `scripts/statistics_schema.mjs schema --tablespace=NAME` for transactional
derived schema and capture, `index --tablespace=NAME` for the independent
`CREATE INDEX CONCURRENTLY`, and `grants` for `deploy/statistics_grants.sql`.
Schema and index rendering require an explicit tablespace. Index creation is
outside a transaction and does not replace any source index. Repeated schema
and grant installation preserves nonempty progress.

The daemon reads the qualified statistics allocation from
`FACTORY_STATISTICS_MAX_BYTES` and the reserved physical growth per operation
from `FACTORY_STATISTICS_RESERVE_BYTES`. Set both in its environment as
positive integer byte counts, with the reserve smaller than the allocation.
Absent values keep statistics writes on capacity hold. The operator budgets
WAL and the filesystem floors independently of the derived relation limit.

`statistics_preflight` validates the source paging index's keys, predicate and
readiness, all capture triggers, schema and required runtime privileges.
Missing dependencies leave counts unknown or at their previous complete clock;
the provider does not attempt an unindexed baseline. Only `wfsim_factoryd`
receives the derived reads and necessary writes. It receives no recovery,
generation update or namespace deletion privilege. Controls retain their
existing owner connection; Web roles receive no derived grants.

Oversized capture keys or exhausted storage halt projection capture. Halt
markers cannot be cleared while reusing the old head: missed changes require
a new generation, incarnation and complete baseline. After pausing statistics,
the explicit `recover` SQL takes the projector lock and a `NOWAIT` source-write
fence over every captured relation before rotating generation and clearing
head/markers. An active writer causes the entire recovery to roll back; retry
only after it commits. Retired namespace state is retained for separately
qualified cleanup.
No namespace is deleted to satisfy the generation budget. Logical row deletion
from shared heaps is not a physical-space recovery guarantee. An exhausted
budget requires a separately qualified lifecycle or capacity decision.

`task_pipeline.mjs status statistics` reads dependency flags, phase/cursor,
last sealed clock, halt reason and existence of work. It does not count the
canonical queues. A source-ready implementation is not installation approval;
production capacity and index installation remain separate release gates.
