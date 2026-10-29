# User calculation tasks

The task center is global chrome in the top-right header. Calculation pages
own configuration and submission; the center owns the queue, progress,+cancellation, retry and result links. It has no separate create/edit form.

Simulation uses the durable local queue. Quick calculation contributes one
transient item read directly from `gainScan`; replacing its inputs does not
add history. Its details retain worker health and the calculator reset action.

## Boundaries

`USER_TASK_TYPES` supplies input validation, request schema, labels, result
storage and report links. `USER_TASK_EXECUTORS` declares supported types and
executes their frozen requests. The local executor calls the existing engine
operations. A type can exist without supporting community execution.

`UserTaskQueue` interprets neither simulation inputs nor result numbers. Its
states are `queued`, `running`, `interrupted`, `succeeded`, `failed` and
`cancelled`. Changing input creates a new request through its calculation
page. Retry preserves the accepted input and starts another attempt.

## Persistence and execution

IndexedDB stores accepted requests, captured origin context, ordering and task
state in `wfsim-user-tasks`. Acceptance is acknowledged after the transaction
commits. Progress is an in-memory observation of an attempt and is broadcast
to other tabs; it does not rewrite the frozen request on every tick.

A Web Lock selects one runner across tabs. Another per-task lock serializes
cancellation with artifact storage and terminal state. An attempt and
generation identify the only execution allowed to settle a task. A stale
progress message or result cannot change cancelled or retried work.

One explicit local task uses the existing worker pool at a time. Simulation
retains foreground priority over automatic quick calculations. Cancellation
stops claiming simulation pieces and discards any late completion without
terminating unrelated work. Native fetches receive the attempt's abort signal.

An abandoned running attempt becomes interrupted. A result committed before a
crash is adopted by its stable attempt id. Queued work is eligible only when
its engine and request schema match. Incompatible work remains inspectable
and requires a fresh calculation from its page.

Without IndexedDB or Web Locks, tasks are kept for the current session and the
center says so. No cross-tab or reload recovery is promised for that queue.

## Results

Simulation captures the engine request, build, weapon, preset, scenario and
comparison key before waiting. Completion writes into the existing results
store using that captured context. The task stores only a reference; it does
not keep a second editable copy of the summary. Replay data retains its
existing in-memory policy.

The board upload projection is captured from the same submitted form. It is
used only for the existing consent-controlled upload, never as execution input.
Completion submits that captured build; an asynchronous board verdict repaints
only the context it describes.

The result is stored before the task is marked complete. A report link opens
the existing simulator in another tab at `?measurement=<record id>` so the
current editor remains unchanged. Removing a task from the list preserves its
result record. Normal result retention still applies; a trimmed or deleted
record is reported as unavailable.

## Checks

`node scripts/check_user_task_queue.mjs` checks an unrelated calculation type,
input ownership, ordering, cancellation, recovery, compatibility and artifact
references. Its negative control removes the attempt guard and must fail.

`WFSIM_BASE=http://127.0.0.1:<port> node scripts/check_user_task_center.mjs`
drives real simulation requests through the original Run button, queues work
for different weapons, changes pages and checks record ownership, late results,
report navigation, reload and phone layout. Existing storage, run-count,
calculator recovery and agent-door checks cover the reused paths.
