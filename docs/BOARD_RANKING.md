# Incremental ranking projection

`scripts/board_result_flow.mjs` selects this reader with
`WFSIM_BOARD_READ_INDEXED=1`. Its installer is `boardRankSchema` in
`scripts/board_rank_schema.mjs`. Install as the canonical storage owner with the
trusted schema, existing reader role and HTTP role named explicitly. New heaps
and indexes use `wfsim_data`. The installer rejects existing projection storage;
it cannot reset a source UUID or imply that old mailbox acknowledgements rebuilt
missing destination state.

## Authority and handoffs

Canonical builds, effective `board_facts` and reviews are the inputs. Native
`describe`, `render` and `floor` provide legality, publication fields, shape,
floor groups and entry decisions. Prepared rows and generated page bodies are
derived outputs. They never become score inputs. Unknown source aliases block
that partition until explicitly qualified.

The router records affected identity readiness and source-key routing versions,
then acknowledges the captured mailbox key in the same PostgreSQL transaction.
The normalizer reads one current build, its valid-mode facts and their mailbox
headers from a coherent snapshot. Native work happens outside transactions.
Applying it updates that identity, affected shape winners, dependencies and
publication readiness with the exact dirty acknowledgement in one transaction.
Another key's larger sequence never means that a smaller committed key was
applied. Consumers serialize dependency registration with routing through their
own advisory locks; canonical producers take none of these locks.

A baseline captures coherent measured sources and their build, selected-fact
and selected-review headers once into an immutable file. It streams at most
256 sources per database fetch and prepares at most 64 per Native batch. File
digest, terminal count, byte cursor and committed coverage guard restart.
The reader can prepare pages for up to three distinct partitions in one turn
with `WFSIM_BOARD_READ_BASELINE_CONCURRENCY` (default one). They share its
database pool and serialize calls to the same Native process. Each database
checkpoint is independent. New live normalization wins over late baseline pages. Every Native publication
mode registers a measurement dependency, including modes without a fact.
At the baseline boundary, paged reconciliation records obligations for changed
or previously unobserved headers. The partition becomes complete only after
these obligations are applied. Registered dependencies continue to route new
changes independently.

The cache retains Native shape winners under score, default-corner, canonical
source and mode order. Fact changes render only their identity. An input update that preserves every affected shape winner and its exact
publication fields acknowledges the new dependency versions without scheduling
an identical snapshot. A changed score, ordering field or visible measurement
time on a winner schedules the partition. Snapshot preparation scans this derived ordered index and indexed group maxima; it does
not read the complete canonical library again or run fights.

### Shared descriptions and observed input versions

One Native executable, canonical identity and source record hash identify a
description. Partitions that read the same record share it. Every partition
retains its own source hash, live precedence, requested measurement modes,
selected review IDs and exact observed mailbox versions. A registered key can
have an unseen version. Build routing uses identity, measurement routing uses
identity and ruler, and removed reviews use the indexed review IDs. Baseline
reconciliation advances through bounded pages of source identities even when
every observed version matches.

On an existing indexed database the storage owner installs
`board_rank_compact_schema.mjs upgrade` while the reader is stopped. Existing
partition rows, dependency versions and pinned pages stay readable. Updating
one source transfers its subscriptions atomically. Unreferenced descriptions
are released when their last input set changes. Include descriptions, input
sets, legacy dependencies, the source epoch and the rest of the ranking
projection together in its consistent database backup.

Recovering the legacy writer requires its expanded rows. After budgeting
their additional storage, stop the compact writer and call
`board_rank_expand_inputs(64)` in independent transactions until it returns
zero. The storage owner's `board_rank_compact_schema.mjs rollback` restores
the writable dependency table after expansion. Earlier snapshots remain
accessible through the same page reader throughout recovery.

## Complete versions

Each generation captures an immutable winner file under the source UUID,
Native digest, engine, baseline cut and index revision. Native floors and output
uploads proceed in pages of at most 256. Output-prefix digest and byte cursor
commit only after file fsync. Restart verifies the prefix and discards an
uncommitted tail. A stale lease cannot append or acknowledge another claim.

Staged database rows are private until the snapshot header, storage reference,
partition revision and covered publication acknowledgement commit together.
A newer index change may wait behind an older complete generation; it remains
ready and cannot be acknowledged by that generation. This avoids starvation
when inputs keep arriving. Identical visible output keeps its revision and
public clock. Existing full-input snapshot rows remain readable through the
same `board_read_page` function.

One cold baseline is active at a time. A cut is limited to 512 MiB, a winner or
output file to 256 MiB, a Native request to 4 MiB and a public row to 64 KiB.
Work lives beneath the explicitly guarded Data root. Deterministic qualification
failures and missing or corrupt recovery files block their own readiness;
ordinary polling cannot reset them into repeated complete scans.
When production storage is required, preparation also preserves 32 GiB of Data
space and 1 GiB on the system filesystem for WAL and the host. A depleted
reserve stops preparation while the complete published versions remain readable.

Published files and completed baseline cuts are reclaimed after their database
state is sufficient for recovery. Public snapshot references retain at least
three versions and all versions younger than ten minutes. Orphan database rows
retire in batches of at most 1,000. Only known files under this reader's unique
cut/generation directories may be removed. Unregistered directories require a
ten-minute age guard before collection. Canonical rows and other consumers'
working directories are outside this collector.

### Restartable input for a large partition

An operator can stage one weapon/ruler with `scripts/stage_rank_baseline.py`
on the board Data mount. The scan stores its last canonical library ID with
each page. The input sequence uses a cache of one. The scanner reads its
position, then waits for transactions already holding mailbox write locks to
finish before reading builds. This establishes which committed inputs the
scan must replay without blocking producers. The source UUID, cluster identity,
Native digest and enabled capture triggers guard the stage across restarts.

The `replay` action opens a single repeatable-read snapshot, enumerates
coalesced mailbox keys in bounded pages, and reloads every changed build,
ruler measurement and affected review from that snapshot. A later commit
cannot displace an earlier transaction at the start boundary. The `manifest`
action seals the resulting inputs in the existing bounded ranking format.
The operator validates Native selection and complete page coverage, then hands
the cut to the reserved reader. The reader retains its normal per-key change
routing, reconciliation and atomic publication.

### A shared cut for the complete catalog

The immutable archive identifies the members to reconcile. A consistent copy
of `board.meta.json` and its `board/` files includes every listed scene, its
leader index and its weapon shards. `board_catalog_archive.mjs inventory ROOT`
produces the inventory. Pin the archive digest returned by the importer and
the Native executable digest for this cut. The `import ROOT INVENTORY ARCHIVE_DB NATIVE` action checks
each file against the inventory, decodes build IDs with Native and checkpoints
each batch. An index partition without members prevents sealing.

On the guarded Data volume, `stage_rank_catalog.py STAGE scan --archive SHA
--native SHA` walks the selection primary key in bounded pages. Each SQLite
transaction saves its cursor, shared build records, effective facts and
dependency versions. `catchup` drains a new producer fence, records changed
identities in paged transactions and reloads the affected records. It resumes
using the saved mailbox cursor and outstanding identities. `replay` closes
the remaining changes under one time-limited repeatable-read snapshot. A
closing timeout leaves the last committed cut available. Another catchup can
reduce the work in that window.

`qualify_rank_catalog.mjs CAPTURE_DB ARCHIVE_DB SCOPE_DB NATIVE WEAPON RULER`
resumes Native ranking from the shared cut. The operator can set
`WFSIM_BOARD_SCOPE_DB_BYTES` to cap its temporary SQLite database and budget
a separate copy for its rollback journal. A descriptor is reusable under
the same Native and source-record hashes. The scope database records the
destination of each archived row and SHA-256 hashes of the ranked pages.
A retained fact under a retired ruler is counted separately. Only a selected,
succeeded review with matching identity, mode, engine and completion time can
account for a replaced score. An unaccounted historical identity holds its scope.

`rank_catalog_coverage.mjs CAPTURE_DB ARCHIVE_DB LEDGER_DB SCOPE_DB` records
one scope receipt, its archived member dispositions and its ordered page hashes.
Repeat an incomplete receipt transfer to resume. Once these records are durable
and the matching publication has been verified, the temporary per-scope ranking
database can be released. Omitting the last argument reconciles the union of archive
partitions and measured partitions against those receipts. Missing scopes,
orphan facts and unresolved members hold the complete catalog. An eligible
scope can emit the existing input format with `stage_rank_catalog.py STAGE
manifest --scope WEAPON RULER --archive SHA --native SHA`. Preserve the original
snapshots and compare each staged publication with its qualified page hashes
before switching the catalog. Capacity planning includes the captured inputs,
archive and descriptor indexes, durable rank tables, transient WAL and existing
storage reserves throughout the staged write.

### Preserving the published board

The storage owner installs the current public directory with the SQL emitted by
`node scripts/board_public_head_schema.mjs` on an existing indexed installation.
With the same database in `PG_URL`, run
`node scripts/import_board_publication.mjs SITE_ROOT INVENTORY_JSON INVENTORY_SHA256`
against the sealed copy of the served files. The digest identifies the inventory
of actual published bytes, including the scene indexes and publication stamp.
The importer checks each file and preserves its original ranks and leaders.
Each weapon and ruler commits independently. The first import pins the inventory
for recovery. Repeating the command fills only missing selections. A verified
native revision or an explicit withdrawal takes precedence over the import.

### Publishing queued scopes

The publication queue uses the same `00`–`99` zones as the compute queue.
A verified measurement inherits its computation's zone. Explicit partition
requests enter `00`, ordinary measurements without a computation enter `03`,
and unrequested catalog replay enters `99`. The earliest executable zone
runs first, then its FIFO position. A pending partition coalesces changes,
keeps its oldest outstanding timestamp and moves to the new zone's tail
on promotion. A scope waiting on a retry or a reader does not block other
eligible work. All zones share the same capacity.

The controller uses the scope's qualified inputs when present. It captures and
qualifies current rows when needed. The reader applies new inputs to a
registered ranking by identity. Both paths reconcile later changes before
selecting a complete revision. The existing public reference stays readable
while publication is pending. Retries keep their position and resume at the
saved checkpoint. An accepted input that leaves the public rows identical
keeps the existing published revision. The controller scans captured inputs
for records missing from a selected rank and outdated dependencies. Recovered
measurements use the zone of the undelivered result. The controller resumes
its checkpoint after the work is committed and revisits the oldest keys after
a complete pass.

The storage owner installs the SQL emitted by `board_catalog_priority_schema.mjs`.
An existing ranked deployment also applies
`node scripts/board_rank_recent_schema.mjs --zones` before switching the
measurement writers, controller and reader. The upgrade adds queue metadata
and leaves pending work in place. The controller upgrades its SQLite queue
in the same file on startup.

Run `scripts/run_rank_catalog_batch.py priority-run CONFIG` alongside the
indexed reader with the existing service accounts. Run one controller at a
time. `priority-step` runs one scheduling step. `priority-status` reports
outstanding scopes by two-digit zone and phase, the oldest timestamp for
each group and the reason and retry time for held work.

### Registering a qualified catalog scope

Apply the SQL emitted by `node scripts/board_rank_catalog_schema.mjs` as
the storage owner. Restart the indexed reader with the matching payload.
Pin the original archive, coherent capture, qualification ledger and public
revisions. With the owner's `PG_URL`, run
`node scripts/board_rank_catalog_scan.mjs CAPTURE_DB`. The command installs
change capture at a writer boundary and scans retained mailbox headers once
from the cut's drained floor. Each page persists its cursor together with the
affected build and measurement keys. A build binding covers all its rulers.

Save the JSON returned by the catalog `manifest` command. With the same work
volume as the reader, run `node scripts/adopt_rank_catalog.mjs LEDGER
MANIFEST_JSON NATIVE WORK [PINNED_REVISION] [--requested INVENTORY_JSON]`.
A published head stays selected. An advanced head returns its actual revision
for comparison. An existing player reservation needs its pinned request
position and mailbox version. A paused partial cut also needs its original
file digest, index position, coverage and quiescent state. Accepted changes
may advance the private index while its cut is paused. The transfer locks the
partition, requires every outstanding identity to be applied, and preserves
the original witness together with the actual index and its current revision. Its typed indices
and referenced Native descriptions are copied into a separate recovery schema
in the index tablespace. The original cut file stays pinned. The request and
its dirty work keep their identities. Budget these scoped copies and their WAL
before transferring an occupied cut.

The reader stages a new scope privately and compares each PostgreSQL page
with the independent qualification digest. It checks the captured dependency
versions and the coalesced changes for that weapon and ruler, including first
measurements absent from the cut. A checkpoint commits its outstanding work
and cursor together. Once those changes are applied, the reader verifies
the current staged bytes and atomically selects the generation and activates
any retained player request. A changed public selection holds the transfer.

After the full catalog has been selected, run
`node scripts/board_rank_catalog_scan.mjs CAPTURE_DB --finish ARCHIVE_DB LEDGER_DB`.
It verifies global historical coverage and each current public selection before
retiring the temporary change capture. The canonical mailbox continues to feed
the ordinary reader.

### Adopting the complete catalog

`scripts/adopt_rank_catalog_batch.mjs` consumes a qualified catalog and keeps
one resumable adoption journal. Its JSON configuration names the following
operator inputs.

| Field | Input |
| --- | --- |
| `stage` | Guarded directory containing `catalog.sqlite`, `archive.sqlite` and `qualification.sqlite` |
| `state` | Private controller directory for the journal and one reusable manifest scratch file |
| `work`, `native` | The active ranking reader's work directory and pinned Native binary |
| `coverage`, `budget` | Complete historical qualification and measured capacity forecast for the same source cut |
| `pins` | One `{members,proof}` pair of SQLite files per protected inventory in the qualification result |
| `reservations` | Current player reservation inventory, including request positions, mailbox versions and occupied cut witnesses |
| `pg_socket`, `database_role` | Local PostgreSQL socket path and owner role when using peer authentication |

Use `python3 scripts/run_rank_catalog_batch.py plan CONFIG.json` on a host
whose capture and ranking work have separate service owners. The supervisor
runs the controller under the existing PostgreSQL OS account. Its database
connection uses `PG_URL`, with owner operations under `database_role` and a
separate connection for storage measurements. The reader's OS account receives
one opened input file, checks the byte count and digest, and writes and syncs
its own immutable cut. The controller can also ask it to verify a named
predecessor. Both roles check the same mounted Data volume. With a common file
owner, use `node scripts/adopt_rank_catalog_batch.mjs plan CONFIG.json`.

`plan` reconciles the archive and every protected identity with its outcome.
It records the selected revisions in a repeatable read. A head that advanced
since qualification also needs its full stored Native output and page digests.
`run` advances one registration at a time. `step` advances a bounded unit and
`status` reports outstanding scopes and sampled storage high-water. Restart
with the same configuration and journal. Refresh a changed reservation witness
against its original request before retrying. During adoption the controller
checks Data and WAL headroom at stage boundaries and samples it every second.
Each public page is compared with both official sites at its immutable
revision. A changed selection is checked again. Closing holds the selected
rows briefly while comparing the verified revisions and retiring the temporary
capture in one transaction.

## Activation and recovery

The public board keeps its weapon and ruler keys through engine releases and
rank index replacement. Valid measurements retain their original engine
provenance. The selected publication stays available while the replacement
index builds against the same verified measurements.

Installing this additive schema does not start a consumer or change task
admission, official computation, generation controls or static publication.
Stop the existing reader before switching its immutable payload and enabling
the indexed flag. The source UUID and Native digest must match the installed
recovery boundary. Do not run two reader implementations against one mailbox.

If activation fails, stop the indexed reader and keep complete published
references and the compatible page function. Restoring a previous payload is
an explicit recovery operation: acknowledgements and their derived cache must
stay together. `boardRankRollbackSchema` removes the additive schema only while
no indexed input transfer or staged snapshot reference is retained, restores the original page function
and preserves the canonical source identity. It refuses a rollback that would
strand a published or pinned version. Neither installer performs historical
task regeneration or score recomputation.

### Standard riven eligibility

The current rank policy evaluates each riven before its candidate reaches
the shape winners. A reader on a retained Native descriptor epoch uses the
current `wfsim-board` as its policy binary. Pin its exact file and SHA-256 in
`WFSIM_BOARD_RIVEN_POLICY_BIN` and `WFSIM_BOARD_RIVEN_POLICY_SHA256`. The
catalog controller binds the same file and digest in `riven_policy_binary`
and `riven_policy_sha256`. The existing descriptor epoch retains its Native
and source digests.

For an installed ranking database, generate the additive upgrade with
`node scripts/board_rank_schema.mjs --riven-policy-upgrade`. The storage owner
installs it and sets the approved `digest` at row 1 of
`state_board_rank_riven_policy` at the reader switch. A generation under an
older policy must be regenerated from the same index before selection.

`node scripts/board_rank_riven_repair.mjs MANIFEST.json` schedules affected
identities in the existing index queue. The manifest binds `source`, `native`,
`policy`, `scopes` with `weapon`, `ruler`, `revision`, `index_revision` and
`identities`, and `historical` with individual old revision keys. Use an
audited current revision for each scope. The selected revision remains
available while the new index and pages are prepared. The replacement and
withdrawal of the prior public revision commit atomically against the
prepared index and current head. Refresh an advanced scope before retrying.
Withdrawal retains the stored snapshot and its independent measurements.
