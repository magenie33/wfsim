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
New live normalization wins over late baseline pages. Every Native publication
mode registers a measurement dependency, including modes without a fact.
At the baseline boundary, paged reconciliation records obligations for changed
or previously unobserved headers. The partition becomes complete only after
these obligations are applied. Registered dependencies continue to route new
changes independently.

The cache retains Native shape winners under score, default-corner, canonical
source and mode order. Fact changes render only their identity. Snapshot
preparation scans this derived ordered index and indexed group maxima; it does
not read the complete canonical library again or run fights.

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
