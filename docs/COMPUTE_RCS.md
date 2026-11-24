# Core seconds

A **reference core-second (RCS)** is one second of CPU time in a fixed
reference environment. Prices use integer reference core-milliseconds
(`rcs_ms`). The reference manifest identifies the CPU, operating system,
execution profile, artifacts and measurement procedure. Changing that
environment requires a separately versioned reference. An executor's elapsed
time is an observation for ETA and renewals.

The offline evaluator in `scripts/compute_rcs.py` takes a JSON file and
writes a JSON receipt to stdout. It has no production database or client
dependencies. Live dispatch still uses its existing rules.

## Measurement and pricing

Run `python3 scripts/compute_rcs.py measure suite.json > measurements.json`
on the declared reference host. A suite has `reference`, `samples` (at least
three), `timeout_seconds` and `tasks`. Only use a reviewed local suite.
Each task declares:

| Field | Value |
| --- | --- |
| `task` | Stable task identifier |
| `input` | Frozen semantic operation and input, hashed for split isolation |
| `estimator` | Versioned provider and cost-model family |
| `base_rcs_ms` | Positive estimate from the provider before calibration |
| `split` | `train` or independent `holdout` |
| `command`, `stdin` | Absolute executable with arguments, and its input text |
| `executions_per_sample` | Optional repeat count inside the command, default 1 |
| `executable_sha256`, `stdout_sha256` | Pinned executable and expected output |

The runner verifies the output of one warm-up and every retained repetition.
It records child user plus system CPU time, divided by executions in the
sample. Use a repeat count large enough to amortize startup. Pin any
interpreter, adapter and data artifacts in the reference manifest too. The
command must wait for all computation and exit without persistent children.
An adapter for a new CPU workload can supply the same input and receipt
without sharing WFSim internals.

Run `python3 scripts/compute_rcs.py calibrate measurements.json > prices.json`.
The input may also contain `incumbent` (the previous receipt's `model`) and
`minimum_tasks_per_split`. The estimator corrects the base price by the
median reference-to-base ratio on training tasks. A candidate may move by
at most 25% up or 20% down in one revision. It must reduce held-out median
relative error and avoid increasing held-out worst error. The resulting
errors must also satisfy `max_median_error_bp` and `max_worst_error_bp`
(default 25% and 100%). Keep separate,
fresh holdout evidence for successive promotions. An estimator with
insufficient evidence keeps its existing factor, or uses the provider's
base estimate. No device timings enter the fit.

## Queue replay

Run `python3 scripts/compute_rcs.py replay queue.json`. Its input contains
`reference`, the pricing receipt's `model`, `target_rcs_ms`,
`max_rcs_ms`, `max_rows` and `eligible`. Eligible rows contain `task`,
`zone`, FIFO `position`, `estimator` and `base_rcs_ms`. Supply the exact
rows eligible for that executor and pass them in dispatch order. The
evaluator rejects mismatched reference versions, duplicate positions and
reordered rows.

The evaluator adds whole tasks until the target or count limit. An indivisible
task above target goes alone, including one above the ordinary work cap.
A fragment can leave below target on the approach to a large task, at a cap,
or at the end of eligible work. At a trial target of 600,000 core-milliseconds,
100 tasks priced at 6,000 fit one pack. A 1,200,000 task forms a pack on its own.
The replay does not simulate leases or decide eligibility.

## Trial evaluation

Run `python3 scripts/compute_rcs.py tune trial.json`. The evaluator returns
a candidate or keeps the incumbent. It never applies the result. Trial
production requires a collector that pairs concurrently observed windows
with persistent account-level cohort assignment. Use one price version,
the same eligibility rules and the same task mix strata across both arms.
An unhealthy dispatch window freezes the decision.

The input provides `incumbent_rcs_ms`, `minimum_windows`,
`minimum_accounts_per_arm`, `minimum_foreground_grants_per_arm`,
`minimum_eligible_slot_ms_per_arm`,
`max_idle_bp` (basis points), and absolute `guardrails` for
`first_grant_p95_ms`, `spare_hold_p95_ms`, `lease_expiry_bp` and
`requests_per_1000_rcs`. Each element of `windows` has a `window` id,
`candidate_rcs_ms`, `dispatch_healthy`, `baseline` and `trial`. An arm
contains those four guardrail metrics plus `accounts`, `foreground_grants`,
`eligible_slot_ms` and `idle_slot_ms`.

Each alternative is within 20% of the incumbent. Every retained window
must meet coverage, idle and guardrails, and foreground grant P95 may not
regress. A larger pack additionally needs to reduce idle time. The smallest
qualifying target is preferred. A production controller needs a versioned
promotion and rollback operation and a fresh observation window after each
change. The offline receipt is evidence for that operation.

Run `python3 scripts/check_compute_rcs.py` for the scoped checks.
