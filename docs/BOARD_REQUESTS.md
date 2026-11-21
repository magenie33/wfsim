# Explicit board requests

The requested intake accepts a missing board measurement and an exact-environment
player correction. Both use the same canonical build intake, captured workflow,
computation identity, community verification and result delivery. Its control is
independent of general intake and admission. Enabling it does not resume bulk
generation, inventory recovery, old submissions or official computation.
Explicit missing-measurement requests declare zone `00`; matching corrections
declare `01`. Targeted admission resolves the participating workflow and live
consumer priority through the common queue restatement. A shared calculation
keeps its execution round and held lease; promotion joins the destination tail.
The retained ordinary `03` prefix is not reprioritized.

## Source and handoffs

A single `/api/board/submit` with `requested:true`, `benchmark` and `mode` captures
one build request. A single eligible player upload from a custom fight names
its official measurement target in `requested_ruler`. The original submission
keeps the player's benchmark when present. The page prefers the selected
eligible ruler, then the primary eligible ruler, then the first admitted ruler.
It confirms delivery from the request receipt. Existing installations apply
`board_requested_schema.mjs --upgrade-submit` before deploying this client.
Scores are absent. Batch submissions remain ordinary inputs.
The page also sends this coverage request when the published board already has
the build; a published row does not prove database coverage. An absent review
baseline follows the same missing-measurement entry.

The normal `/api/board/review` captures its stored baseline and local observation.
A transactional trigger captures only newly inserted review submissions. It
does not import the old inbox. The native intake validates canonical identity,
game legality and exact observed environment. The requested policy selects the
native plan's single build, ruler and sustainable mode, retaining its official
fight. First creation can also register a separate family workflow in zone
`03`. The explicit request receipt tracks the single requested measurement.

A new explicit click that coalesces with an unadmitted legacy review or a
cancelled execution captures a fresh review against the current baseline.
The stranded review becomes superseded. Its old inbox, captured intent and
cancelled computation remain intact. A live admitted review keeps its existing
work. This replacement never enumerates or resumes the old backlog.

| Stage | Durable input | Commit |
| --- | --- | --- |
| Capture | Immutable source and requested environment | Ready request, in the source transaction |
| Normalize | Leased source, pinned native intake | Canonical build, captured intent and staged request; spend only this input |
| Admit | Staged intent | Targeted existing admission and common computation; acknowledge the request |
| Execute and verify | Common queue and execution round | Existing independent community acceptance and workflow delivery |
| Publish | Selected canonical measurement | Existing board mailbox, native projection and website pages |

Native work occurs outside database transactions. A two-minute lease guards
normalization and admission separately. An expired or replaced lease cannot
acknowledge work. A failed transaction retains the previous stage; retry never
scans the general requested lane or recovers unrelated workflows. Existing
measurements satisfy coverage without another execution. Corrections retain
their original baseline until independent verification succeeds.

Semantic native refusals retain a rejected source receipt. Temporary process,
database and lease failures retry after thirty seconds. Rejection also closes
an unadmitted review, so it cannot remain waiting without an executable task.
The source status reports committed workflow state and the selected measurement;
the page observes website publication separately after verification.

## Verified results on a published partition

Each admitted explicit request retains a receipt in `state_board_requested_results`.
The indexed reader checks one due receipt per turn alongside ordinary input
routing, ranking and publication. An active workflow waits for verification.
A correction must match its successful review and the current selected fact.
Existing coverage can satisfy a missing-measurement request. Only a complete,
selected partition on the same source and Native version can receive the handoff.
The receipt retires in the transaction that queues the canonical identity for
the existing rank index. Native shape selection, entry thresholds and immutable
pages apply to these results.

The canonical storage owner installs an existing database upgrade with
`node scripts/board_requested_result_schema.mjs`. The reader discovers the
installed handoff at startup. The owner can pass up to 64 exact request IDs to
`board_requested_result_recover(text[])` for scoped recovery. The function
selects admitted requests by primary key and preserves pending receipts.

## Installation and operation

Provision a dedicated NOLOGIN runner role and restricted login before installing
with `board_requested_schema.mjs` as the canonical storage owner. The installer
uses the data tablespace and a bounded transaction; existing tables require an
explicit upgrade. The role can execute only request claim, stage, admission and
retry functions. It cannot submit requests, change controls or write canonical
tables directly. The HTTP role receives submission execution and status reads.
Installation requires the canonical table owner and pins its own trusted schema,
`pg_catalog`, then `pg_temp` before creating definer functions. Temporary tables
cannot replace source metadata, controls or request work.

The independent service pins its immutable native binary hash and source UUID.
Its environment supplies `WFSIM_REQUESTED_DATABASE_URL`,
`WFSIM_REQUESTED_INTAKE_BIN`, `WFSIM_REQUESTED_INTAKE_SHA256` and
`WFSIM_REQUESTED_SOURCE`, plus the existing data-disk guard variables.
`state_board_request_control.enabled` starts false. Activation and rollback
change only this control and the new service. General intake/admission controls
remain paused while this scoped consumer owns these requests.

## Creation-only family work

The canonical intake transaction reserves one family seed per newly created
submitted weapon build. The same runner advances the seed and its expansions
using the `family` pipeline control. An accepted optimizer result enters a leased
inbox for its exact parent workflow and captured scope. The runner normalizes
one result and admits at most sixteen nodes per five-second turn. Both steps
use the existing bounded admission and measurement delivery functions.

Install the current `build_admission_schema.mjs` output before updating the
runner and its pinned Native intake artifact. It installs the family control
paused. The storage operator can inspect or change it with
`node scripts/task_pipeline.mjs status family`, `resume family` or `pause family`.
A pause preserves the captured work and the inbox. Admitted calculations
continue through the usual computation and verification stages.

Source installation does not qualify a board partition. End-to-end activation
also requires the affected partition's complete historical coverage, current
source capture, native projection, website API and browser read. A queue ACK
or accepted result alone does not prove publication.

With `WFSIM_BOARD_READ_REQUESTED_PARTITIONS=1`, newly staged requests capture
their weapon/ruler targets for the independent reader. A missing target starts
disabled. The reader takes a complete coherent cut of its measured source data,
bounded by 8 MiB and two minutes, validates the current native identities and
uses the unchanged native publisher. Its snapshot and first selection commit
together under the mailbox claim. Existing partitions retain their operator
selection, including an explicit rollback. Unknown aliases, oversized inputs
and failed preparation retain the complete static view and a retry receipt;
they cannot select an incomplete online partition.
Deterministic qualification failures leave the ready queue until explicit repair.
Temporary database, lease and process failures keep the ordinary retry delay.

A newly changed measurement for a complete public partition wakes its canonical
identity directly. The input key coalesces across modes until the reader hands
it to the index. A verified explicit request uses the same indexed handoff,
including when the fact already exists. The index and publication queues keep
the version that needs prompt service across retries and process restarts.
The reader services both recent identities and the ordinary FIFO. At publication
boundaries it alternates recent and oldest eligible work, with one generation
in flight. A changed identity during a generation retains its later version.
Existing installations add these wakeups with `board_rank_recent_schema.mjs`.

A first measured result for an unregistered weapon and ruler records one
pending partition. The priority controller schedules the affected scope from its verified
arrival and the existing request position. A selected historical publication
requires the scoped qualification and catch-up in [BOARD_RANKING.md](BOARD_RANKING.md).
An unseen scope takes a fresh capture under the same source and Native contract. Existing partition selections take precedence. Removals do not
introduce scopes. Recording a partition does not create computation orders.
