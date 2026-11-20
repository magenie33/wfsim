# Captured board inputs

The target read path is defined in
[COMPUTE_QUEUE_CONTRACT.md](COMPUTE_QUEUE_CONTRACT.md#publication-and-qualification).
This component implements source capture and delivery to a same-database
projection. The production installer and running board do not select it.
[BOARD.md](BOARD.md#the-live-board) describes the installed path.

## Authority and boundaries

`entity_weapon_builds`, `state_board_measurement_selection` and
`entity_measurement_reviews` remain authoritative. `board_capture.sql` captures
their inserted or updated rows and explicit deletion values into
`state_board_input_mailbox` in the source transaction. Measurement keys encode
the JSON array of build identity, ruler and mode. Pending calculations, attempts
and potential-work inventory are separate inputs to separate consumers.

The mailbox carries current state per key. Updating a pending key coalesces its
previous captured value. Contribution and audit occurrence consumers preserve
their immutable accepted-result identities independently. A mailbox version
cannot stand for an accounting event or proof of every accepted result.

Capture performs no native normalization, ranking, queue traversal or network
request. Source failure rolls back its handoff. Explicit removals persist until
applied, and their version watermarks remain afterwards. Truncating a captured
source is refused because it would bypass row removal capture.

## Delivery contract

`worker/pg/board_mailbox.sql` provides these owner-installed functions:

| Function | Contract |
| --- | --- |
| `board_input_put(scope,key,payload)` | Captures an object or SQL NULL removal; an unchanged fingerprint keeps its version |
| `board_input_seed(scope,key,payload)` | Inserts version zero only if no live value or removal watermark exists |
| `board_input_claim(items,token,lease_seconds,source)` | Claims at most 256 ready headers after checking the expected source identity |
| `board_input_read(scope,key,token,version,max_bytes)` | Reads the still-owned current payload within an explicit byte bound, at most 8 MiB |
| `board_input_lock(scope,key,token,version)` | Locks and checks the key inside its projection transaction |
| `board_input_ack(scope,key,token,version)` | Marks the exact claim applied and frees its completed payload; a newer value remains ready |
| `board_input_retry(scope,key,token,version,error,delay_seconds)` | Records the failure and requeues the claim; a NULL delay retains it for explicit repair |

The partial ready index excludes completed and blocked history. Live versions
precede version-zero baseline seeds, so a seed backlog cannot hold new results
behind the import. Seed pacing and resource budgets remain the importer's
responsibility. Each lane seeks its timestamp range in the same ordered
index, including when a generic plan serves a large ready backlog. Claims
inspect a fixed maximum of 256 candidate keys and skip locks before taking the caller's
bounded subset. Headers contain byte sizes, not payloads. Sequence values order
versions of one key; neither their maximum nor their allocation order is a
cross-transaction completion cursor. Earlier allocated values can commit later.

`scripts/board_input_mailbox.mjs` requires the expected source identity. Read
packets retain their canonical payload and SHA-256 fingerprint. Native work
runs after reading, outside a transaction. `apply` then checks the source,
lease, current version and fingerprint under the key lock. Its callback performs
only bounded destination writes and next-stage readiness in that PostgreSQL
transaction. ACK is part of the same commit. A failed write or expired lease
rolls back the projection and handoff together. Superseded input does not invoke
the projection writer. Repeating a committed apply returns `already_applied`.

Completed payloads are freed while identity, version, fingerprint, removal and
failure metadata remain. The mailbox and destination receipts belong to one
recovery boundary. Losing destination state requires an explicit rebuild or
restore; normal startup cannot recreate it and trust old acknowledgements.

## Installation and baseline

Apply `board_mailbox.sql` and then `board_capture.sql` within one bounded
installation transaction as the canonical tables' owner. The source UUID must
be retained with the mailbox. Reapplying preserves it; missing metadata beside
existing keys requires restore. Functions pin their schema search path and have
no PUBLIC execution grants. A projection role receives only claim, read, lock,
ACK and retry execution, plus its own destination writes. Producer and seed
permissions stay separate.

Install live capture before seeding history. Seed with bounded pages and a
durable per-source progress receipt. Version zero cannot overwrite concurrently
captured values or deletions. Seed completion proves baseline coverage; draining
a page or reaching the largest allocated version does not. Partition cutover
also requires its native projection and concurrent-change application proofs.
Until then the complete served partition remains selected. The page presents
the current board without migration banners.

This installation does not change existing inventory capture or seal functions,
start a rebuild, reopen cancelled work, grant publication rights or select a
new online read path. Ranking projection, revision-aware pagination, page caching
and served-partition activation require their own qualified implementation.
