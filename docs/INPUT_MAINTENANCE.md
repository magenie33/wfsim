# Prepared input maintenance

The normalization owner imports committed consumer acknowledgements before
checking the normalization stage's production pause. Intake, inventory,
projection and publication pauses do not disable this maintenance turn.
`WFSIM_BOARD_MAINTENANCE_PAUSED=1` pauses maintenance itself. A stop request
prevents the next source commit or capture unlink.

The production path is `live_board.sh` with `INCREMENTAL_INVENTORY=1`, then
`split_board_pipeline.mjs serve`, its normalization child, and
`runBoardInputMaintenance`. The supervisor schedules children without consulting
stage pauses. Each child owns its stage lock before maintenance runs. The
`inventory_pipeline.mjs --serve` entry delegates to the same supervisor.
Mount checks, a missing required database environment and writer lock conflicts
hold the service before any maintenance; these are independent prerequisites.

## Explicit existing-state binding

`WFSIM_BOARD_MAINTENANCE_BINDING` names a qualified JSON file. An absent binding
reports a hold and creates no maintenance state. A release does not create a
binding or enable maintenance automatically. The file contains:

- `v: 1`, the absolute `work` path, and `root: {path, identity}`. An identity is
  the existing device, inode and owner UID, represented as three strings.
- The selected `task`, `generation`, `engine` and complete source `contract`.
- `methods`, returned by `maintenanceMethods()`. This includes the adapter
  source and the reused source/collection methods; a change needs a new review.
- Exactly two `consumers`, inventory and projection. Each supplies its existing
  `path`, source contract and historical audited consumer contract. The current
  executable's consumer digest does not establish compatibility with an outbox.
- `files`, keyed by the exact controller, controller-lock, source and consumer
  database paths. Each supplies its existing `identity`, accepted `schema` from
  `maintenanceSchema()`, and a qualified absolute `walCeilingBytes` for writes.
- `limits`: `reserveBytes`, per-database `walBytes`, `maxAckBytes`,
  `maxSpanCuts` and `maxVersions`.

Qualification belongs to an explicit operation. Automatically taking the
current files as the binding would adopt unreviewed data and schemas. Opening
uses SQLite URI `mode=ro` or `mode=rw`, never create mode. Read-only schema
validation precedes writable opening. No runtime schema migration, generation,
consumer registration, capture or native process is part of maintenance.

## Bounded turns and commits

The same normalization owner acquires the existing controller lock. Each
consumer outbox is read in its own read-only transaction. Its source, historical
contract, metadata size and complete audit must match the retained sealed cut.
Inventory acknowledgements additionally validate their complete input span.
Each source acknowledgement commits independently. A restart rereads the
outbox; an identical acknowledgement is idempotent, and an earlier or changed
receipt is refused. Consumer databases are never written by maintenance.

A turn admits at most two acknowledgements, 256 span cuts, 256 obsolete
versions and 256 change blocks. The selected retention bodies and keys must
fit 64 KiB. The per-database WAL allowance is at most 4 MiB, separately backed
by each file's absolute WAL ceiling; the filesystem must retain its configured
reserve plus three allowances. Allocation and identity checks run before
mutation and at transaction boundaries. These guards do not certify arbitrary
SQLite write amplification or a host's peak disk usage.

The deadline is at most one second, checked between bounded operations. Capture
collection retains its existing sealed, same-generation, non-baseline,
acknowledged and unreferenced eligibility rules. A turn considers one capture
and retires at most one file of at most 256 KiB. Manifest bytes and directory
entries are bounded. A larger file, an inventory span above the cut limit, or
unqualified allocation holds for a separately qualified operation; maintenance
does not hide an unbounded pass behind these limits.

Source and controller commits are separate. No cross-database crash atomicity
is assumed. Capture deletion retains its durable per-file intent and cursor;
interruption after an unlink resumes only that recorded operation. Unsealed
captures, old generations, baselines and additional abandonment eligibility
are outside this maintenance path.

Version collection makes database space reusable. `retiredFileBytes` counts
manifest file sizes; it does not measure released filesystem allocation.
`filesystemBytesReclaimed` remains null without an independent physical-space
measurement. Shared record bodies and whole database files are not collected.

`node scripts/check_board_input_maintenance.mjs` exercises the actual production
supervisor command with stage pauses, an absent native binary and an unusable
database endpoint. SQLite fixtures also cover missing-file races, ownership,
historical contracts, schema/method changes, WAL and metadata admission,
acknowledgement rollback and restart. `check_source_capture_gc.mjs` covers
retained captures and stop fences around the durable unlink intent.
