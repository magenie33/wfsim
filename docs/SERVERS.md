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
| edge | Cloudflare | the site, the worker (the door, the factory's API, accounts), the board in R2 behind `cdn.wfsim.app`, the MCP worker, the edge worker; `wfsim-cloud` for money | serves every request; no machine to keep |
| data core | Hetzner, Falkenstein | Postgres, `wfsim-factoryd` over its unix socket, the tunnel the others reach it by | the library, and the settler that writes it every second |
| board server | Tencent, mainland | `wfsim-live` (docs/BOARD.md §"The live board"), `wfsim-pg-tunnel` | background: it pulls the facts that moved and pushes the files that did, so a lost push is sent on the next cycle; the data core's disk has none to spare |
| China door | Tencent, Hong Kong | the QQ bot (`wfsim-bot`, its Chrome kept current by `update-chrome.timer`), `cn.wfsim.app`, the relay that carries the board's work for readers in mainland China (docs/DISTRIBUTION.md §"The relay") | QQ's whitelist, Discord and the site are all near it; a card is a page load of the site, which from the mainland crosses a link that drops at peak hours |
| official batch | GitHub Actions | CI, `ship.yml`, and the clock: `queue`, `scores`, `settle`, `survey`, `publish`, `backup`, `mirrors` | long work with no standing machine |
| distribution | Tencent COS, mirrors | the desktop channel (docs/DISTRIBUTION.md) | reachable from the mainland |

`ship.yml` deploys the bot to the China door and the live board to the board
server (`scripts/ship_bot.py`, `bot.json`'s `live`). The data core's daemon is
deployed with `python scripts/ship_factoryd.py` from the same source commit
after changes to its worker or scripts; `--verify` checks its commit and heartbeat.

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

`worker/pg/library.sql` sets vacuum thresholds on the factory and board tables:
2% of dead versions plus a fixed floor, with an earlier insert threshold to keep
new pages visible to index-only reads. `scores_pace` covers both time-ordered
score reads and rolling provenance counts; they share one timestamp index.

Routine vacuum makes dead tuple and index space reusable by later writes;
reusable space inside a relation is distinct from free filesystem space.
Maintenance runs serially with a disk budget for WAL, temporary files and any
replacement index, and stops before the reserve is exhausted. `VACUUM FULL`
requires a replacement table and is unsuitable for an almost-full filesystem.

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
