# User calculation tasks

The top-right task center observes calculations started on their existing
pages. Those pages own configuration and submission. The center owns waiting
order, progress, cancellation, retry, executor transfers and saved report links;
it has no separate creation or configuration form.
The entry reserves two tabular digits for its active count and shows `00`
while idle.

## Types and executors

`USER_TASK_TYPES` supplies a request schema, validation, presentation, execution,
artifact storage and result links. `USER_TASK_EXECUTORS` supplies admission,
execution and cancellation. The queue interprets neither game inputs nor
result numbers. Adding a type does not require changing its lifecycle.

Simulation, optimization and saved-riven appraisal use the durable queue.
Appraisal uses `headlessRivenGainRequest`, followed by
`headlessAppraisalReplay`, the search and grade used by the chat workflow.
Saved cards remain editable in the original riven editor. Text import uses
`readRivenLines`, shared with chat and screenshot stat naming. A batch is an
atomic group of independent requests; each retains its own result and retry.

`UserTransientTask` represents automatic work. A new quick-calculation input
replaces its single slot and invalidates the previous generation. A live scan
retries a lost worker call once; cancellation or a new generation ends retries.
Its details retain worker health and the existing calculator reset. Work this device does
for other players appears separately and reads the existing contribution log.

`board-review` observes an independent review triggered by a matching official
simulation. Its persisted outbox and remote identity survive reloads without
holding a local calculation lock. Completion delivery creates the outbox before
it is acknowledged. Hiding an observation leaves the shared review running.
The server owns its verified number; browser storage keeps only linkage and
submission state, with at most fifty observations.

## Local ownership

Acceptance freezes the request and its origin before awaiting storage. IndexedDB
`wfsim-user-tasks` stores lifecycle, ordering, artifact references, checkpoints
and pending commands/events. Acceptance is acknowledged after its transaction
commits. Progress is an in-memory observation broadcast between tabs.

A Web Lock selects one runner per executor across tabs. The local resource
lock permits one explicit heavy calculation at a time, including an appraisal
replay following a community search. Each operation retains its existing
internal parallelism and power settings. Automatic calculations keep their
existing foreground priority rules.

States are `queued`, `running`, `interrupted`, `succeeded`, `failed` and
`cancelled`. An attempt and generation identify the only execution allowed to
settle a task. Per-task locks serialize cancellation, transfers and artifact
storage. Late results cannot settle cancelled or superseded attempts.

A lost local runner leaves its attempt interrupted unless an already committed
artifact can be adopted. Retry preserves inputs and identity. A compatible
native optimizer job can be reattached; compatible engine checkpoints can
resume their own task. Otherwise retry restarts. An incompatible engine or
request schema requires a fresh request from the original page.

Without IndexedDB or Web Locks, the queue explicitly promises only this
session. It does not promise cross-tab ownership or reload recovery.

## Artifacts and delivery

Simulation summaries remain in the existing result store. Optimizer artifacts
retain every ranked result and its engine-produced replay request. Appraisal
artifacts also retain the replay verdict and the captured reference board.
The task stores references, never a second editable summary. The legacy optimizer
session cache is presentation only; a task report always reads its artifact,
and execution never reads that cache.

A result is stored before terminal state is committed. Reports open the
simulator, optimizer or personal Riven Analyst report in another tab, without
changing the original editor. The analysis page and optimizer history read the
same artifact. A prefix scan of the artifact store keeps those histories
available after the queue item is removed. Frozen riven references retain the card's submitted rolls, rank and
polarity even if the owned card is subsequently edited or deleted. Local
artifacts remain until browser storage is cleared; simulation keeps its existing
bounded retention. An unavailable record is reported as unavailable.

Completion events are persisted with terminal state. The center marks unread
completions. Board submission uses captured ownership and the existing consent;
a chat appraisal reply uses its captured code. Delivery can be retried without
re-running the calculation. Removing history preserves artifacts and pending
delivery context. No browser notification permission is requested.

## Community execution

`worker/user_tasks.js` is a producer of the existing fair fact factory.
`simulate_report` and `optimize_report` compare complete reports, including all
finalists and the fields needed to open or replay them. Wall-clock timings are
excluded from agreement; calculation fields and engine work participate.
Two independent witnesses are required. Disagreeing reports are shown as a
failed request, never an unchecked result. The existing simulation and quick
search entry points perform the work.

Clients advertise `calculation-reports-v1`. The factory's shared eligibility
rule excludes these verbs for older clients, on protocol 8's JavaScript and SQL
paths and protocol 7. Official tooling must explicitly request this producer.
The producer has the same fair-share weight as existing producers, a six-hour
deadline, no fleet reservation and no paid priority. Its browser initiator
uses the existing cap and forgetting rules. Batch requests do not assert that someone
is actively waiting.

A random browser secret owns remote commands and results. The server stores
its hash, no account association. Losing that secret loses access; signing in
does not sync this queue to another browser. Calculation inputs are disclosed
to volunteers when the reader transfers them. A request expires after 30 days.
Shared input/report payloads are removed once their last consumer expires;
aggregate contributions remain. Cancellation withdraws only that consumer.

Acceptance commands are stable and idempotent, including after a lost response.
Cancellation tombstones prevent an earlier cancel from being lost to a later
acceptance. Transfers retain the logical task and frozen input. Remote
acceptance becomes durable before the old local attempt is stopped. If local
completion wins, the duplicate remote request is cancelled. Transfers restart
from identical inputs when no compatible checkpoint is available, as stated
beside the controls.

Remote observation batches pending ids and waits at least the server's stated
interval. Idle browsers do not continuously poll account state. Volunteer
reports are stored before submission and retried until acknowledged.

## Validation and deployment

`check_user_task_queue` exercises lifecycle, batches, transient generations,
checkpoints, delivery and transfer races with a negative cancellation control.
`check_user_task_service` runs the real schema and factory on PGlite.
`check_user_task_center`, `check_task_executors` and
`check_task_service_browser` drive the real page and WASM operations; the last
uses an isolated service and two independent test witnesses.

The release requires `schema.sql` and `sync.sql` to be applied before exposing
community execution, following `docs/BOARD.md`'s migration procedure. A native
dev server has no community backend and hides unsupported transfer controls.
