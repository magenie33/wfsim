# Servers

What runs where, and the rule that decides it. Addresses and keys are in
`private/` (`private/qq/bot.json` for the bot and the board server,
`private/hetzner/` for the database, `private/hk/` for the China door); this
file names roles.

## Why a machine exists

A machine is kept for one of three reasons and no other:

1. **State** — the library, the one irreplaceable thing (docs/BOARD.md).
2. **A fixed address** — QQ answers only calls from a whitelisted IP.
3. **Long work** — a fight, a projection, a browser drawing a card.

Everything else runs where it costs no machine (the edge) or no standing one
(GitHub Actions).

## Where a service goes

Asked in this order; the first answer that settles it wins.

1. **Is the problem measured?** A service moves for a failure counted in its
   logs, never to match a picture. A move with nothing measured behind it buys
   nothing and costs a machine.
2. **Who waits on it?** A reader waiting — a chat, a page — puts the service
   near the reader and the platform it answers through. Background work that
   retries on its next cycle can sit across a slow link.
3. **What crosses the link?** A service that reads the database whole each
   cycle belongs beside it; one that pulls deltas does not.
4. **Does it harm the irreplaceable?** The database's machine runs only what
   talks to the database, and only what its disk can spare.

## The roles

| role | where | runs | why there |
| --- | --- | --- | --- |
| edge | Cloudflare | the website fallback, the worker (the door, the factory's API, accounts), the board in R2 behind `cdn.wfsim.app`, the MCP worker, the edge worker; `wfsim-cloud` for money | placed API, shared data and fallback delivery |
| website | configured website host | nginx, the checked static release, same-origin API forwarding and certificate renewal | a direct website entrance; the data core and placed API remain together |
| data core | netcup, Manassas | Postgres, `wfsim-factoryd` over its unix socket, the database tunnel, verified task archives and database backups | the library, and the settler that writes it every second |
| board server | Tencent, mainland | `wfsim-live` (docs/BOARD.md §"The live board"), `wfsim-pg-tunnel` | background: it pulls the facts that moved and pushes the files that did, so a lost push is sent on the next cycle; the data core's disk has none to spare |
| China door | Tencent, Hong Kong | the QQ bot (`wfsim-bot`, its Chrome kept current by `update-chrome.timer`), `cn.wfsim.app`, the relay that carries the board's work for readers in mainland China (docs/DISTRIBUTION.md §"The relay") | QQ's whitelist, Discord and the site are all near it; a card is a page load of the site, which from the mainland crosses a link that drops at peak hours |
| official batch | GitHub Actions | CI, `ship.yml`, and the clock: `queue`, `scores`, `settle`, `survey`, `publish`, `backup`, `mirrors` | long work with no standing machine |
| distribution | Tencent COS, mirrors | the desktop channel (docs/DISTRIBUTION.md) | reachable from the mainland |

`ship.yml` deploys the bot to the China door and the live board to the board
server (`scripts/ship_bot.py`, `bot.json`'s `live`). The data core's daemon is
deployed with `python scripts/ship_factoryd.py` from the same source commit
after changes to its worker or scripts; `--verify` checks its commit and heartbeat.

An unchanged live board keeps its process and actual `VERSION`. Shipping compares
both checked CI binaries and the complete runtime inventory, including witness
packs and release fields other than `commit` and `release`. A stable active process
must have started after every runtime, unit and environment-file change; missing,
changed or unknown state uses the normal deployment path.

The board/intake build runs in the pinned Linux container without deployment
credentials. Shipping promotes the exact source/input/target-verified artifact;
the legacy path builds only when no prepared artifact exists. An invalid prepared
artifact stops the release. Compilation never establishes that a remote service
already loaded its files; the remote byte and process-start proof still applies.

The prepared board binary carries `bin/inventory-compat.json`. Its
`inventory-catalog-v1` key hashes complete semantic inputs: the CLI's Cargo path
dependency closure, non-test Rust tokens, manifests/locks, build environment and
the engine's existing code/data classifiers and prose normalization. Digests
remain full SHA-256 values. Translations, compiled board bookkeeping and unrelated
documentation do not change this catalog key; the actual binary SHA still binds
the sidecar to its executable. Source/run and every raw input/output remain in
the separate strict prepared receipt. Catalog compatibility does not authorize
skipping deployment or weaken the complete loaded-runtime proof.

Unrecognized compile-time includes, module paths, build scripts, procedural
inputs or build configuration use full fallback. Missing legacy sidecars also
use the binary hash; a present corrupt or wrongly bound sidecar is refused.
A normal legacy deployment removes stale remote sidecar metadata after the
unchanged-runtime decision, before replacing the binary. The probe itself
performs no remote writes.

An unchanged bot keeps its loaded process. Its single deploy inventory includes
all bot modules, transitive relative imports, generated engine, WASM packs,
stylesheet and images. The active Node process must use that root and have
started after every runtime, unit and environment-file change. Style/package
directory entries must match and remain stable throughout the probe. A file
replaced without restarting, a missing import or an unknown probe cannot skip
deployment. The live proof runs before any co-hosted bot upload.

## Website host

`WEBSITE_TARGET` enables `scripts/ship_website.py` in the release job. It is JSON
with `host` (`user@address`), `port`, `key` and `known_hosts`. The release environment's
`WEBSITE_SSH_KEY` and `WEBSITE_KNOWN_HOSTS` supply the two files. The host receives
the existing CI artifact after the backend and edge workers deploy; it builds
nothing. Every file is verified before an atomic directory switch. Failed health
checks restore the previous directory. Live board files are excluded and remain
served from their authoritative bucket through the backend.

`deploy/website.nginx` serves the static release, retains the previous hashed
assets and forwards dynamic routes through `/etc/nginx/wfsim-backend.conf`.
That private include names the backend's HTTPS host, verifies its certificate,
and replaces `X-WFSim-Website` and `X-WFSim-Website-IP` with the shared
`WEBSITE_PROXY_SECRET` and the socket's client address. It never trusts a
visitor's forwarded address. The worker restores the canonical URL and address
only with the valid secret, preserving same-origin checks, OAuth callbacks,
cookies and network independence. Proxy geography is unknown rather than the
website server's country.

The private include's body allowance covers the backend's upload limits;
`worker/live_board.js` owns the board upload limit. It clears upstream `Alt-Svc`
and NEL policies because the website host negotiates its own transport.

The certificate is renewed with certbot's webroot at `/srv/wfsim/acme`.
`wrangler.acme.jsonc` routes only HTTP validation paths while DNS still points at
the edge. Its `WEBSITE_ACME_ORIGIN` is the website host's reachable HTTP hostname.
After DNS changes, nginx serves the same validation files directly.

The `website dns` workflow checks DNS read/write access by creating and removing
a temporary TXT record. With `apply=true`, it verifies the host's TLS and release
before changing the two website names. It saves the existing mappings and restores
the Worker domains if a change fails. It checks `CF_DNS_TOKEN`, `CF_API_TOKEN`
and `CF_WORKERS_TOKEN` in order; publishing Workers alone does not grant DNS edit access.

## Factory capacity and deployment

The heartbeat, fair table and settlement use a separate database pool from
statistics and maintenance. Background jobs run one at a time and use
per-statement and idle-transaction timeouts. The home tally and demand picture
share one rolling score read for a minute; scores remain
the source, and a failed read is retried rather than cached as zero.

`scripts/factory_capacity.mjs` reads CPU availability, including cgroup CPU
quotas, host load and Linux memory/I/O pressure. Expensive demand samples and
the tally back off according to elapsed time and pressure, then recover
gradually. This changes execution budgets, never fairness, witness counts or
credit. Demand's live reading still refreshes each minute and its queue totals
refresh every ten minutes. Its compute-cost estimate is kept between ten minutes
and one hour. The tally refreshes
up to ten minutes, preserving a longer configured interval. The daemon records the
resources and measured durations in `factory_findings.capacity`.

Initiator cleanup locks at most 1,000 completed rows per table and uses indexed
state ranges to skip active rows. Task retention archives at most 100 questions
per call in private R2 before pruning; the unarchived daemon call returns zero.
Both skip locked rows. Delivery, recent activity, credit and current calibration
proof keep their retention guards.

`WFSIM_BACKGROUND_DUTY_PCT` sets each adaptive job's wall-time budget
(1–25, default 5), not a CPU utilization limit.
`WFSIM_BACKGROUND_MAX_MS` bounds sample backoff (60,000–3,600,000, default
3,600,000). `WFSIM_BACKGROUND_AUTO=0` keeps the configured clocks. Overrides
belong in `/etc/wfsim-factoryd.env`, not edits to source on each machine.

On an existing database host with PostgreSQL, the versioned schemas and Node
at `/usr/local/bin/node`, `python scripts/ship_factoryd.py --bootstrap` creates
the OS/database role, installs its grants and an initial local-socket environment,
then deploys. The grant sources are `deploy/factoryd_grants.sql` and
`deploy/factoryd_accounts_grants.sql`. Existing environment files are preserved;
`FACTORY_ENV_FILE` explicitly supplies a replacement, with mode 640.

`FACTORY_TARGET`, `FACTORY_SSH_KEY` and `FACTORY_KNOWN_HOSTS` select another
host; otherwise the target comes from `private/hetzner/ssh.env`. R2 credentials
come from the environment or `private/hetzner/r2-public.env`. Neither is in the
bundle. Deployment builds the required indexes concurrently from their schema
definitions, runs every tick once before switching, and restores the previous
daemon if post-deployment verification fails.

The daemon is a singleton per database: a reserved Postgres session owns an
advisory lock and losing that session stops it. Copying deployment configuration
to another host does not create another primary database or a second settler.
Move the active daemon by stopping the old service before starting the new one.

## Database storage

`python scripts/ship_health.py` installs `wfsim-health.timer`, using the same
explicit target options as the daemon deploy. It runs `scripts/pg_health.py`
each minute on the data core.
It uses bounded read-only SQL and one bounded board metadata read. It records
daemon and inventory age, engine mismatch, actual board score age, settlement
backlog, archive failure, lock holders, idle transactions and resource pressure.
An unsuccessful probe is unknown and degraded, never an empty healthy result.
The monitor neither repairs services nor changes work or credit.

`/var/lib/wfsim-health/status.json` is the latest observation. Its
`events.ndjson` and two rotated files retain at most 6 MiB; the system journal
also receives each observation. An incident id ties repeated samples and the
recovery together. Exit 1 means a completed degraded observation, accepted by
the unit; `status` and `alerts` in the JSON are the health result. An active
timer or service alone does not prove healthy work. A missing or older-than-
three-minutes status file means monitoring itself is stale.

For an incident, save the status and events, `journalctl -u wfsim-health
-u wfsim-task-archive -u wfsim-factoryd --since '30 minutes ago'`, and the board
host's `journalctl -u wfsim-live` plus its work directory's `diagnostics/`.
Match source, engine, generation, epoch and run id before comparing samples.
SQL statements, task bodies, credentials and connection URLs are excluded.
`sudo -u postgres python3 /opt/wfsim-health/scripts/pg_health.py --once` prints
a fresh observation without replacing the retained incident state.

`python scripts/ship_work_calibration.py` installs the named calibration read
before the work-price job uses it. The pipeline role can execute this bounded
function without selecting factory tables. Its query is also the official
calibration API's source. Reads use short read-only transactions, and native
sample bodies remain inside the fitting job.

`python scripts/ship_task_archive.py` installs the versioned archive tool and
`wfsim-task-archive.timer` beside the database. It requires the database host's
existing private R2 rclone configuration. `TASK_DETAIL_KEEP_DAYS` in
`worker/task_contract.js` owns the seven-day hot window. The timer runs serial
bounded batches, with eight-second statements and one-second lock waits. Each
pass spends at most thirty seconds starting batches; individual transfers and
the service retain their own timeout. Failed uploads or readbacks leave rows
in PostgreSQL. The archive contains complete questions, answers, closed demands,
engine refusals and dispatch hints; builds, scores and contribution aggregates
remain independent live records.

Each object captures 500 tasks by default, bounded at 5,000. After both
objects pass full readback, pruning still locks and rechecks at most 100 tasks
per transaction. `WFSIM_TASK_ARCHIVE_BATCH` in
`/etc/wfsim-task-archive.env` sets the capture target; a statement timeout
reduces that attempt's target. The journal reports capture, publication and
pruning time separately. Capture size changes object overhead, not retention
eligibility or the deletion lock budget.

Archives use `r2:wfsim-pg-backup/task-details-v1/<UTC day>/<batch>/`, outside
the nightly backup's thirty-day expiry. Each gzip object has a final manifest
with question ids, cutoff, byte counts and SHA-256 hashes. This captures history;
only PostgreSQL supplies current state. Archive age does not assert that its
rows were deleted: pruning rechecks both eligibility and exact content under
the question lock. `python scripts/pg_task_archive.py --verify <manifest>` reads
and checks a batch independently. Local status is
`/var/lib/wfsim-task-archive/latest.json`; failures are in the service journal.

`worker/pg/library.sql` sets vacuum thresholds on the factory and board tables:
2% of dead versions plus a fixed floor, with an earlier insert threshold to keep
new pages visible to index-only reads. `scores_pace` covers both time-ordered
score reads and rolling provenance counts; they share one timestamp index.

Routine vacuum makes dead tuple and index space reusable by later writes;
reusable space inside a relation is distinct from free filesystem space.
Maintenance runs serially with a disk budget for WAL, temporary files and any
replacement index, and stops before the reserve is exhausted. `VACUUM FULL`
requires a replacement table and is unsuitable for an almost-full filesystem.

## Database volume lifecycle

Large indexes and temporary query files may use a separate PostgreSQL tablespace
on a mounted data volume. PostgreSQL must require that mount before starting;
a mounted volume is part of the database, even when it holds only indexes.
Move indexes individually with bounded lock waits, or rebuild them concurrently
in the target tablespace to preserve foreground writes. Keep filesystem reserve
for WAL and cancel maintenance before it exhausts that reserve.

Logical database backups include the objects in every tablespace. A restore
must create the tablespace directories and ownership before restoring globals,
or omit tablespace definitions and use `pg_restore --no-tablespaces` on the
custom archives. Provider server snapshots do not include attached volumes.

Before removing a volume, move every table and index back, reset every role or
database setting that uses its tablespace, and drop the empty tablespace. Remove
the mount dependency only after PostgreSQL no longer references it; then unmount,
detach and delete the volume. The destination needs space for the moved objects
and normal database operations.

## One instance

**A service runs on one host.** Two bots claim the same messages, and a chat
sees every reply twice; two live boards take the same submissions and push the
same files. So:

- A move stops the old instance before the new one starts.
- A unit a ship still restarts on a host that no longer runs the service
  idles there (a drop-in `ExecStart=/bin/sleep infinity`) until the ship's
  target names the new host, then is disabled.
- A host runs nothing a role above does not name.

## Where it is heading

Design rules for the next move, each taken only when the first test above
says so:

- **The bot's answer belongs at the edge**, the way the MCP worker answers
  from the engine; the China door keeps only the fixed address QQ requires.
  Discord's interactions are an HTTPS endpoint the edge already can serve.
- **A card is a published product**: drawn when a weapon's heads move
  (docs/BOARD.md §"The heads") and served from R2, so a chat never waits on a
  browser.
- **Official compute is a trusted client of the factory**, asking for work by
  the volunteers' own protocol, not a pipeline of its own.
- **The projector belongs beside the data** once what it publishes makes the
  cross-border hop count, on a disk of its own.
