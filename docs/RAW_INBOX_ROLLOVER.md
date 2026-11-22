# Raw submission storage rollover

The raw inbox owns input until Native intake, requested intake or family intake
acknowledges it. The published builds, accepted scores, results and witnesses
have independent owners. Leaving the old raw queue in the same table after
ordinary deletions does not return its allocated heap and index space to the
filesystem.

## Ownership and service behavior

During copying, workers keep using the original PostgreSQL table and its
existing roles, triggers and queue positions. New raw input is mirrored to a
logged spool in the existing attached tablespace. The spool follows deletions.
The archive cursor advances only when a bounded page from private R2 has been
decoded into an independent PostgreSQL cluster and the source transaction
rechecks the exact rows, their positions and the full page digest.

A cut allowed by the storage owner preserves the raw table's OID, index
constraints and roles. It reuses the same table with a new heap and indexes.
New rows and protected old rows stay in the hot spool until they are reinserted
in the cut transaction. They include positioned builds, reviews, workflow
continuations and inputs with independent live consumers. The spool's recorded
input is kept byte for byte. The shared position table stays in place, including
equipment entries. After the cut, cold ordinary entries without a position use
their immutable object and durable PostgreSQL claim as their owner. The
operator can read or restore the archived page independently while Native
intake continues processing the hot queue. The optional library count is
unknown while part of the inbox is cold.

The recovery process loads bounded pages in archive order. Recovered legacy
entries remain unpositioned. Existing Native keyset capture still advances
only after its exact acknowledgement and wraps at its ordinary boundary. An
acknowledgement of a pre-cut capture checks its original six raw fields. A
matching acknowledgement can retire the cold claim without reloading it. A
different submission with a reserved ID fails. A committed page leaves a
receipt that prevents a retry from restoring its consumed inputs.

## Operator sequence

Use the canonical storage owner to install the module in the attached PostgreSQL
tablespace. Verify the mounted device, the active intake binary and the exact
rollover-aware acknowledgement source from the release candidate first. Keep
normal schema installation, raw maintenance and index rewrites out of the
active archive window. Set up separate libpq services for the source and the
disposable restore cluster. The restore cluster must have a different
PostgreSQL system identifier. Run the data commands with the authorized
storage operator connection. Limit R2 access to the private backup prefix.
The HTTP library role needs execution on `raw_inbox_library_count()` if it is
separate from the storage owner.

Run `install`, then `begin <generation>` using a fresh UUID. Use `status`
to read the active generation, cursor and terminal receipts. Repeat small
`copy` windows against the approved root and attached reserves. Ordinary
concurrent deletes can invalidate a fetched page. Start the next window from
the durable cursor. An unreferenced R2 object left by a failed copy is safe to
leave in place.

Before `cut`, confirm the last range is exhausted and measure the spool,
actual available disk and whole-host load. Fund new hot files and WAL until
commit, when the old root allocation goes away. Use measured peak and duration
budgets from rehearsal and verify the source's current trigger profile. Give
the cut a short lock and statement budget. Record root and attached `df`
before and after. PostgreSQL relation bytes are a separate measurement.

After cut, run bounded `drain` windows under the approved hot backlog and
filesystem budgets. Count live rows since earlier allocations can be reused.
Let actual Native intake work between windows and watch its receipt, hot queue
size, available disk and service latency. Stop if a digest, queue owner or
position differs. Use `finish <generation>` when all verified parts have
terminal receipts and the claim table is empty.

```sh
python3 scripts/raw_inbox_rollover.py install --tablespace wfsim_data
python3 scripts/raw_inbox_rollover.py begin "$generation"
python3 scripts/raw_inbox_rollover.py copy --work "$archive_work" \
  --restore-service "$restore_service" --root-reserve "$root_reserve" \
  --work-reserve "$attached_reserve" --parts "$parts" --seconds "$seconds"
python3 scripts/raw_inbox_rollover.py cut "$generation" --work "$archive_work" \
  --root-reserve "$root_reserve" --work-reserve "$attached_reserve" \
  --hot-rows "$hot_rows" --hot-bytes "$hot_bytes" --peak-root-bytes "$cut_peak" \
  --seconds "$cut_seconds"
python3 scripts/raw_inbox_rollover.py drain --work "$archive_work" \
  --root-reserve "$root_reserve" --work-reserve "$attached_reserve" \
  --max-hot-rows "$hot_ceiling" --part-peak-bytes "$part_peak" \
  --parts "$parts" --seconds "$seconds"
python3 scripts/raw_inbox_rollover.py finish "$generation"
```

## Recovery across an interrupted command

Before a committed cut, `abort <generation>` drops its temporary ownership
state and opens a clean window. The original raw table has remained live.
An interrupted SQL transaction rolls back the heap replacement together with
the spool and ownership changes. If the client loses the cut response, read
`status` for the same generation's `cut_at` before issuing another command.
A completed cut can only recover forward from its verified objects. Check
the archived part receipt and replay the same bytes after an uncertain load.
Operations never overwrite an existing object or delete R2 objects.

Retain the private generation data for every database backup that may restore
the generation's claims. Recovery of such a backup must restore access to
the original checksummed objects before starting its archive intake.

## Verification

`check_raw_inbox_rollover.mjs` uses two real local PostgreSQL clusters, the
shipping Native intake binary, a private-object fixture with immutable writes,
and the actual queue and Worker APIs. It runs the copy, aborted cut,
committed cut, stale capture acknowledgement, corrupt remote read, recovery,
and replay through their command or storage entry points. Measure production
capacity and archive throughput separately from this fixture.
