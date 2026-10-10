# Analytics: how many people use WFSim, and how well

**Goal.** Answer two questions with numbers instead of guesses: *how many
people use wfsim.app*, and *how well do they use it*. Not marketing
attribution — where visitors come from is recorded as far as the site that
sent them, and no further. It records which module and which weapons are
exercised, and whether a visit produces a RESULT or nothing.

## Why the edge cannot answer it

The deployed site makes **zero server requests after boot**. `api()`
(`web/src/static/app/08-checkpoint-api.js`) dispatches to worker RPC on the
static build, so `/api/simulate`, `/api/optimize` and the rest never touch the
network. Cloudflare's own counts are also biased in ways that cannot be
filtered out on the free plan:

- **`/asset/*` and `/pkg/*` are `immutable`**, so a returning reader requests
  neither; the wasm count is new devices plus one re-download per release, and
  rises with how often the site ships.
- **Unique visitors are per IP.** Mobile IPv6 in China rotates, so one reader
  is several.
- **Crawlers, `.env` scanners and Cloudflare's own Early Hints fetches**
  (user agent `bastion early hints` / `nginx-ssl early hints`, answered 504)
  share the same counters. Referer and ASN are not queryable on the free plan.

So the edge gives an order of magnitude for "how many", and nothing for "how
well". Both come from the usage points below.

## Rules

- **A referrer is its HOST, never its path**: which site sent a reader, not
  which page. A page the browser prerenders is not counted until it is opened.
- **Same-origin, first-party.** Points post to wfsim.app itself. A third-party
  host that mainland China blocks would under-count exactly the players WFSim
  is for — no Google Analytics, no `static.cloudflareinsights.com`.
- **Event names derive from the DOMAIN, never the UI.** A renamed event is a
  permanently broken time series, because history cannot be backfilled. The
  vocabulary is `USAGE_EVENTS` in `worker/index.js`, and
  `scripts/check_usage_events.mjs` holds the page to it.
- **What the server records is never sent as a point.** An account made, a
  sync turned on, a payment: the accounts database already holds each one, and
  a second count of it is two numbers that disagree. A point counts only what
  the server cannot see — a page opened, a form left half done.
- **A DEVICE IS ITS TRAITS, NEVER ITS MODEL.** A user agent or a model list goes
  stale with every new device, and the questions are about traits: tapped or
  clicked, room for the editor, cores for the community's work, an in-app
  browser that limits downloads. Each trait is a handful of classes, so a point
  cannot single a device out; the user agent is read for the two in-app
  browsers alone and never sent. What a device DOES is measured where it can
  be — `compute.background` — rather than inferred from what it says it is.
- **No PII, no cookies, no accounts.** One random `wfsim-cid` in localStorage,
  clearable like every other `wfsim-*` key. No IP, no user agent is written.
- **Global Privacy Control or Do Not Track set: nothing is sent.** Nor after
  the reader turns counting off on `/support#usage`, which also says what is
  counted and shows the browser's id — the footer links there.
  **That page states what is true now and promises nothing further**: when an
  account can be linked to a visitor, it changes in the same commit.
- **A crawler that runs the page is not a reader.** Applebot and Googlebot boot
  the app; the worker answers a point whose user agent is a crawler's
  (`USAGE_CRAWLER`) and writes nothing.
- **Only the live site and the desktop shell send** (`LIVE_HOSTS`,
  `07-usage.js`). A dev server or a check on 127.0.0.1 never writes the live
  dataset.
- **English ids only**, the same rule as wiki URLs.
- **Fire and forget.** A point is never awaited and a failure never reaches the
  page.

## What is collected

`track(event, subject, n)` in `web/src/static/app/07-usage.js` posts to
`POST /api/e`; the worker validates the shape and writes one data point to the
Analytics Engine dataset `wfsim` (binding `USAGE`, `wrangler.jsonc`).

**Once per (event, subject) per page load.** A point says a reader got this far
with this thing, not how many times: forty edits to one build are one build.

| event | fires when | subject | `n` |
| --- | --- | --- | --- |
| `app.boot` | the engine has answered `/api/meta` | how the page was reached: `reload`, `back_forward`, `from_site`, `direct` or `from_<referrer host>`; a reload a newer release caused is `release_idle` (the page reloaded itself) or `release_asked` (the reader clicked) | ms since navigation |
| `app.view` | a page was drawn | page kind (`home`, `weapon_builder`, `weapon_simulator`, `warframe`, …); each account page is its own kind — `login`, `signup`, `reset`, `account`, `account_sync` (`AUTH_VIEWS`) — and a page an extension mounts names its own (`authView`) | — |
| `app.error` | this page's own code failed uncaught — another origin's script (an extension) is not counted | `boot` (the app did not start), `script`, `promise`, then where in our own file — `<kind>_<app\|nona\|page>_<line>_<column>`, read against that release's committed `site/asset/` bundle: a place, never the message | ms since navigation |
| `app.device` | the engine has answered `/api/meta` | what the device can do, in coarse classes: `<input>_<width>_<browser>` — input `touch`, `mouse`, `touchmouse` (touch first, a mouse too), `mousetouch`, `nopointer`; width `narrow` (≤640 px), `medium` (≤1024), `wide`; browser `wechat`, `qq` (their in-app browsers) or `browser` | the logical cores, rounded down to 1, 2, 4, 6, 8, 12, 16, 24 or 32 |
| `engine.fail` | an engine worker failed to load, or stopped answering | why — `worker_load_stale` (a newer release no longer serves this page's files), `worker_load_offline` (the site did not answer either), `worker_load` (it did, and the download failed anyway), `worker_load_timeout` (never said a word), `worker_silent` (stopped answering after it had) | ms since navigation |
| `builder.weapon` | the weapon panel computed a build with ≥1 mod | weapon id | — |
| `builder.warframe` | the Warframe panel computed a build with anything set — a mod, arcane, shard or Helminth ability | frame id | — |
| `builder.operator` | the Operator page answered with a Focus school chosen or an Artifact card seated | — | — |
| `builder.riven` | a custom riven was edited and saved | — | — |
| `builder.companion` | a card was seated on a companion build | companion id | — |
| `builder.enemy` | a custom target was edited and saved | — | — |
| `simulator.start` | a Run Sim began | weapon id | runs |
| `simulator.run` | a Run Sim finished (not stopped) | weapon id | runs |
| `optimizer.start` | a search began, or resumed | weapon id | — |
| `optimizer.run` | a search finished (not cancelled) | weapon id | seconds |
| `compute.background` | the page holding this browser's community work came back after 10 minutes or more out of sight | `kept` (a task finished meanwhile), `stopped` (the page itself was held: at most one beat of work), `idle` (it ran and was handed nothing) | minutes out of sight |
| `share.create` | a build left the page | weapon id | how, the first way per load: 1 link, 2 text, 3 share sheet |
| `share.entry` | a build left the page | where the panel was opened: `bar` (the build bar's button), `finder` (a board build in the finder), `optimizer` (a search's finalist), `simulator` (the result) | — |
| `share.open` | a shared build landed in a reader's app | weapon id | — |
| `board.open` | a board build was opened into the builder | weapon id | — |
| `board.submit` | a build reached the board's inbox | weapon id | — |
| `desktop.download` | a desktop download link was clicked | — | — |
| `door.seen` | a greyed option naming another page was drawn: the share panel's, for readers it does not sign for | the way in: `share` | — |
| `door.open` | a page was reached by a link that names its way in (`?from=`), which is then taken off the address | the way in: `topbar`, `menu`, `share`, `support`, … | — |
| `presets.saved` | the engine has answered `/api/meta` | `presets` or `customs` | how many this browser holds, board rows opened into a bar not counted |
| `nona.open` | Nona's panel was opened | — | 1 with a key set, 0 without |
| `nona.ask` | a reader message was sent to her — never its text | — | which reader message of this conversation it is |
| `nona.concise` | concise mode was switched in her settings | — | 1 on, 0 off |

An `engine.fail` from a visitor with no `app.boot` is a reader the site lost
before it could do anything — the one failure the edge cannot see.

The data point, schema 1 — **frozen once written**: a change adds a schema
number, it never reinterprets a column.

| column | holds |
| --- | --- |
| `index1` | the visitor id — sampling keeps or drops a visitor whole |
| `blob1` | event |
| `blob2` | visitor id |
| `blob3` | subject |
| `blob4` | first path segment the page was on (`home`, `weapons`, `operator`, …; `other` for anything unexpected) |
| `blob5` | page language |
| `blob6` | `web` or `desktop` |
| `blob7` | release id |
| `blob8` | country, from the edge |
| `double1` | schema |
| `double2` | `n` |

## Reading it

```
python scripts/usage.py            # last 14 days
python scripts/usage.py --days 28
```

Needs `CF_ACCOUNT` and `CF_TOKEN` (an API token with **Account Analytics:
Read**) in the environment or in `private/cloudflare.env`. It prints, per day,
visitors and visitors with a result; each module's visitors; the top subjects
per event; and visitors by landing route, language, shell and country.

It also ranks the **most active visitors** — by days seen, then results — under
the first eight characters of their id. A visitor is a browser and nothing
more; it becomes a person only if they tell us the id `/support` shows them,
and then `python scripts/usage.py --visitor <id>` lists everything it sent.

## Community compute

The machines that compute the board are read from the server's own tables, never
from points — `verifiers` and `verifier_hours` already hold every result a device
sent (docs/BOARD.md §"Contribution"):

```
python scripts/compute.py            # every day verifier_hours holds
```

Per UTC day: devices that sent results, those new that day, results, the hours
their fights took, the work credited, and each device's hours online. Then how
many of each day's new devices came back, how concentrated the work is, and the
devices computing in each hour of the day. Totals only: it reads the device ids
to count them, prints none, and never opens the accounts database. It runs
through `npx wrangler d1 execute --remote`, read-only, so it needs the wrangler
login the deploys use.

## Retention

**Analytics Engine keeps three months; `usage_days` keeps the totals for
good.** The site worker's cron (`worker/usage_days.js`, 00:30 UTC) writes each
finished UTC day the table lacks into the `wfsim` database: per event, subject
and market the visitors and points, and the day's visitors — `all`, `result`
(by the same `RESULTS` as `usage.py`, which `check_usage_events` holds equal),
`returning` (seen in the seven days before) and by country. Totals only; no
visitor id is kept. A trend longer than a quarter is read from there:

```
npx wrangler d1 execute wfsim --remote --command "SELECT day, market, visitors FROM usage_days WHERE event = 'visitors' AND subject = 'all' ORDER BY day"
```

It needs the worker secrets `USAGE_ACCOUNT` and `USAGE_READ_TOKEN` (Account
Analytics: Read); without them it writes nothing. A day is written whole and
never revised. Its `visitors`/`all` row goes last and marks it kept, so a run
that stops part way is redone, and a missed run is made up by the next.

## Shown publicly

**THE POPULARITY RANKING** on the home page (`15-home-hot.js`), from
`/api/popularity` (`worker/popularity.js`): per weapon, its `tested` rows —
the visitors who finished a simulation OR a search on it that day, each once
whichever they ran — summed over the last 30 days `usage_days` has finished,
both markets together. A hundred runs in an afternoon add one; a count of runs
would rank the most persistent reader, not the most tested
weapon. The page shows each weapon's PLACE, not the count. Totals the cron already keeps, cached an hour at each edge; `/support`
says it is shown. Nothing new is collected for it.

## Not collected

Referrer PATHS (a referrer is its host), campaign tags, Google Analytics, and any
third-party script on the critical path. No event is joined to an account, and
none carries a build beyond its weapon id or any text a reader typed.
