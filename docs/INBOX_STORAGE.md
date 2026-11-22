# Temporary submission storage

Ordinary submissions have three owners: a small FIFO position, one temporary
original payload, then the canonical build and captured intake obligation.
`state_build_inbox_positions` retains the original queue order.
`record_build_inbox_payloads` holds the original six fields in bounded time
generations. Neither the Worker nor a client decides canonical build identity.

## Retry and retirement boundary

The Worker generates an opaque random version-eight source and a separate
generation, frozen together in one SQL call. The source carries no timestamp;
the original record retains only its submission day. A generation is a
15-minute UTC interval, stored only in the temporary queue. Neither value is
accepted from a submission body. The database accepts the previous, current and next
interval only while they remain above its monotonic `closed_before` boundary.
Changing the clock cannot reopen closed retry eligibility. A fresh HTTP submission has
a fresh source; replaying an expired database statement cannot recreate it.

Pending input retains its exact payload. An exact retry reuses its FIFO entry;
the same source with different original fields or another active generation
fails. Native normalization and
the acknowledgement commit together. Accepted inputs must have a canonical
build and durable source/intent binding. A Native refusal must be explicit.
Both outcomes immediately clear the original text and FIFO entry. Only a
temporary hash and outcome remain in that generation for legal retries.
No hash or extra row is added to a permanent result or source table.

After closing retry eligibility, maintenance moves only pending originals to
that generation's retained partition, preserving the original FIFO position.
An original moves at most once. The original partition can then be dropped,
including accepted/refused retry metadata and its indexes. The retained
partition drops when its remaining inputs finish. A live older tail does not
prevent later completed generations from retiring. The storage control and
its monotonic boundary remain; retired per-generation metadata is deleted.

Legacy official family winners use an authenticated entry tied to their
existing family parent. Queue insertion and parent completion commit together.
Pending retries compare the original payload and retain its first arrival
clock. Formal intake custody is reused
by the family delivery worker. A closed parent cannot recreate a refused input
after temporary metadata has retired.

## Installation and activation

The storage operator installs the exact combined candidate. Keep public writes,
live intake, hourly intake and family delivery paused while changing their
schema and scripts. Complete the independent old-table physical cut first;
its protected hot child is outside this module's generation registry.

1. Generate `node scripts/build_inbox_schema.mjs TABLESPACE SCHEMA HTTP_ROLE
   PIPELINE_ROLE` and apply it under the existing storage owner. The selected
   attached tablespace must already be mounted and capacity-qualified. The
   installer preserves the old acknowledgement's OID, ACL and configuration,
   changes its body, installs the queue view and storage functions, and grants
   the submission and acknowledgement entries. It also grants the installed
   factory daemon role its ordinary/family entries. Storage and automatic
   maintenance initially remain disabled.
   The first `build_inbox_positions_generation` index scans existing FIFO
   positions even when its predicate selects no rows. That single statement
   has ten minutes; all other installation statements retain fifteen seconds,
   with the same 500 ms lock budget throughout.
2. Install the candidate Worker, including `inbox_source.js`, and its factory
   delivery modules. Install `build_batches.mjs` and `intake_batches.mjs` in
   both live and hourly consumers. Keep the same pinned Native binary when
   its engine/input contract is unchanged. The final acknowledgement assertion
   makes a restored old acknowledgement fail the whole handoff transaction.
3. Install the maintenance package and private `PG_URL` environment under
   `/opt/wfsim-inbox-storage`. Its PostgreSQL role needs ownership of these
   new relations and functions. The systemd Unix user must be able to inspect
   the real database mount. Do not give HTTP writers partition-maintenance
   privileges.
4. Write the approved capacity file and run `prepare --capacity FILE` on the
   database host. It prepares previous/current/next partitions in the recorded
   tablespace. Write a readiness JSON with `v:1`, the exact 40-hex `source`,
   and 64-hex readback hashes for `schema`, `worker`, `live`, `hourly`, `typed`
   and `native`. Run `enable --readiness FILE --automatic` only after those
   deployed paths are verified. The database stores that receipt's digest.
5. Install `deploy/wfsim-inbox-storage.service` and `.timer`, the environment
   file `/etc/wfsim-inbox-storage.env`, and capacity file
   `/etc/wfsim-inbox-storage.capacity.json`. Reload systemd and explicitly
   enable/start the timer. Confirm its next trigger and a completed service
   turn, the prepared current window and the database automatic flag before
   reopening submissions. New Worker writes fail while the entry is absent,
   disabled or has no prepared eligible partition.

The capacity file contains `v:1`, `work`, positive `root_reserve_bytes` and
`data_reserve_bytes`, `rows` from 1 through 64 and `seconds` from 1 through 10.
The operator supplies the actual approved reserves. Every mutation rechecks
the real data device and free bytes, with another 64 MiB root and 16 MiB data
allowance. Linux whole-host full I/O pressure must stay at or below avg10 4
and avg60 6. Each SQL transaction has a 200 ms lock budget and at most eight
seconds, clipped to the remaining turn. Busy or pressured turns can stop;
they do not reduce the reserve or pressure gates. A timer must remain active
to prepare future intervals; missing maintenance fails new submissions closed.

## Migration bridge and recovery

`bridge --capacity FILE` moves at most the approved number of unfinished
ordinary rows from `ONLY record_raw_archive_arrivals`. It preserves all six
fields and the existing FIFO position atomically. It does not scan the old
parent, archive hot inputs, equipment or typed mirrors. Those inputs continue
through their existing owners. `finish-bridge --capacity FILE` truncates only
an independently empty arrivals relation after acquiring short locks. Its OID
and attachment remain intact. Any remaining protected row prevents release.

Use `status` after an uncertain response. Preparation and retirement recognize
completed work; carry selects only originals that have not moved. A failed
transaction retains both payload and location. A lost successful handoff does
not reload input. Relation bytes and removed files are evidence of relation
retirement; measure filesystem availability independently.

During an incident, stop new submissions and intake before reverting scripts.
Setting `enabled=false` pauses the new entry but does not reopen legacy raw
writes after adoption. Keep the control row, closed boundary, payloads, queue
positions and new acknowledgement installed. Resume forward using the same
source and positions. Do not drop the storage module or revert to an old
reader while new positions remain. Reinstalling a compatible body preserves
the control state. A rollback of a prepared database transaction is safe; a
committed closed generation must recover forward.

## Adjacent owners

Explicit requested calculations and corrections still use
`entity_board_requests` and `state_board_requests`. `requestedBoardPass` stages
their Native input and `board_request_stage` removes an exact old raw mirror.
The request's frozen input remains part of that existing business record;
this module does not claim its retirement.

Workflow family continuation input is owned by `state_build_family_inbox`.
`familyBoardPass` calls `build_family_stage`, which commits canonical custody,
removes its exact raw mirror and deletes the typed input. Equipment keeps
`record_equipment_submissions`; ordinary intake commits its canonical record
and `build_ack_submissions` deletes the exact equipment input and FIFO entry.
These existing tables reuse PostgreSQL space; this release does not claim
partition-file retirement for requested records, workflow family tables or
equipment tables.
