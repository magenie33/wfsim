# The browser and node checks

`scripts/check_*.mjs`. Each asserts a property of the shipping build; run the
ones a change touches. AGENTS.md lists them by name and one line of what they
assert — this file is the shape of each and why it is shaped that way.

A check that runs the worker's SQL runs it on an in-memory PGlite laid with
`worker/pg/*.sql` (`scripts/pg/test_db.mjs`), behind the statement API the
worker gets from Hyperdrive; it needs `npm install --prefix scripts/pg`.

## `check_page_bodies`

`node --check` over every check script, and over `app.js` joined from its
parts. No browser; runs first in CI.

## `check_app_parts`

`app.js` is `web/src/static/app/NN-name.js`, joined in filename order with
nothing between the parts: `web/build.rs` for the dev server, `app_source()` in
`scripts/build_site_app.py` for `site/`, `scripts/app_source.mjs` for every
check that reads the page's source. The filename prefix is the only statement
of order, and a part is a fragment of one classic script — the page's
top-level names stay globals. It asserts that nothing but parts sits in the
directory, that no two share a prefix, that no part runs past the line ceiling
(a ratchet: split a part, never raise it), and that no whole
`web/src/static/app.js` competes with the parts. No browser.

## `check_time_storage`

Every stored instant is UTC ISO 8601 to the millisecond (docs/NAMING.md §9).
It reads the schemas and the code that writes them: an instant column that is
not TEXT, an `_at_ms` column, a day cut from `now()`, a day written under an
instant's name outside the anonymous store, a write to the second, and a
clock bound to a statement as a number, which an ISO column compares as
smaller than every string. The
private worker's schemas and sources are read where it is checked out. No
browser.

## `check_release_identity`

The built `site/` names its own release and cannot be
mixed with another's. No browser — every assertion is a fact about the files,
so it runs on every change rather than the ones somebody remembered. It holds
five things: `site/release.json` and the `RELEASE_ID` stamped into `app.js`
agree; `pkg/` holds exactly one module, named by its own digest, with the glue
carrying the same one; `worker.js` asks for those names and for no unhashed
one; `board.meta.json` stamps the board sitting beside it; and the page reads
its age and its submission count from that stamp rather than from the copy of
`board_state.yaml` compiled into the wasm, which is as old as the build. The
second and third are the ones that matter most: an unhashed module path is a browser cache
that can hold the PREVIOUS engine under this build's `app.js`, which is
different answers from the same seed with no error anywhere.

## `check_card_page`

The long image the bots send (`45-card-page.js`, docs/AGENT.md §"The QQ bot"):
each kind of card draws what its address asks for and its code, keeps the
address's question through the weapon opening, is the only thing on the page,
and says `data-card-ready` only once every riven's values have arrived — a card
drawn before it went onto the page lost them, and the screenshot showed bare
stat names.

## `check_qq`

The QQ door (`worker/qq.js`) with no network: the address check signs QQ's
published example, a signed event is kept and a changed one refused, and the
bot server's pull hands each message out once until it is answered.

## `check_fx_lines`

The bot's `fx` reader with a fixture pool, no network: a line reads with or
without spaces, `+` or 正, and its side follows 正/负 or the number's sign against
the stat's direction (变焦-50 a malus, 后坐力-50 a bonus); the two disagreeing, a
missing number or an unknown word is an error rather than a guess.

## `check_appraise`

Riven appraisal's door (`worker/appraise.js`) with no network: only a bot opens
one, the page reads it without the chat it came from, an asker opens at most
five an hour, every build handed back is kept with its thanks stripped of
links, the first build the bot accepts wins once, a told appraisal is not told
again, and each channel's bot sees only its own.

## `check_appraisal_page`

Riven appraisal's page (`81-appraisal.js`) against a stand-in for its door, in
minutes because it runs a real search: the link lands on the weapon's
optimizer under the appraisal's ruler with the preset's four starts — the
asker's riven, pinned, beside each element's 60/60 card — answering one build
at ten fights a candidate; the search starts with no click; the reader's own
presets, rivens and checkpoint are left exactly as they were; the winner goes
back as a build carrying that riven's shape and the typed name and no score;
and the finalists reach the board even with uploading switched off. The same
link with `?freeze` writes the request the search sent and its engine, starting
nothing; and that frozen request and context, run as volunteer work through the
background's own search, answers the same build to the key, under its lease,
with the search's work.

## `check_agent_door`

`window.wfsim` is one door, and an action is something a
reader can do — `docs/AGENT.md`. Two of its assertions are static, read off
`app.js` rather than off the page, because duplication is invisible from
inside a running page: the mod-exchange decision and the clear-to-innate rule
each exist ONCE, and the weapon control calls the door instead of its own
copy. Every tool is in exactly one of the door's skills, and every skill says
what it is for. The rest drive the page through the door itself — seat a mod, set a
polarity, plan the Forma, seat an arcane, install an evolution, clear the
build, change the fight, open a module, run it — and assert the SLOT ON SCREEN
redrew, which is the difference between an agent working the page and one
working behind it. Every action that is not a query declares what it writes, and a reader's-hand
action is never a tool and is refused without the hand. An edit made just
before opening another build, or duplicating, is kept (verified to bite: with
the flush removed both fail). A search must start, refuse a second start, report its phase and
stop when asked. Every query must leave the observation byte-identical, and
the stats read must name the mod behind a change. Then the refusals, each of which must come back as
`ok:false` with a reason rather than as an exception.

## `check_mcp_tools`

The MCP server's queries, run with no page: `mcp/headless.js` against the
engine `mcp/engine.js` bundles, with a host like the worker's — the board off
`site/board/`, English, no screen. A weapon found by its Chinese name, a board
read for a weapon nobody opened, a row's `build` reading the stats panel, and
the refusals for a missing build and a mistyped weapon. And the one
translation between a saved build and the wire, both ways: every riven-free
board row's build of every weapon, stored as a saved build and read back, is
the build it was. No browser.

## `check_nona`

Nona against a local stand-in provider speaking both protocols, streamed in
pieces the way providers send them, read only through the page and the requests
— never her module's internals. She is sent her seven fixed tools, whose
catalogue names every door action; the tools fit cap T and every skill's
document its cap, in characters; every reader message carries the page and the
skill of the module they are on; a scripted `act` lands on the page and in the
trail, and her first change branches the reader's build — which is asserted
untouched afterwards, and taken back through the change card. Each reply shows
the tokens the provider billed and no guessed price, Anthropic requests carry
cache breakpoints, conversations survive a reload and reopen from the picker
with their trail, memory rides a request except an incognito one, an incognito
chat is not kept, and the key is kept for the tab unless the reader asks to
remember it. Verified to bite: with the branch policy disabled and the
incognito guard removed from the save, seven assertions fail; with the preload
off, or a skill dropped from the door, the catalogue and preload assertions
fail. A model that answers with nothing is sent the page's check once and then
answers; with the check removed, that assertion fails. A profile past its cap
is tidied before she is asked — she is sent the tidied one, the reader is told,
and one tap brings the old one back; with the tidy switched off, those
assertions fail.

## `test_nona_core`

Not a page check: Node imports `web/src/static/nona/core/` directly, which is
possible because nothing there touches the page, the network or the clock. A
stored shape from before versions migrates and one newer than the code is left
alone; no shipped field name leaves `FROZEN`; the view leaves a deep-frozen
record untouched and a later request repeats the earlier one's prefix byte for
byte in both protocols; the budget sets aside the oldest results and keeps the
newest six (two when forced); the summary cuts at the fourth-newest turn; only
unmeasured numbers are marked; both protocols encode to golden bodies and
decode a stream cut mid-line; memory writes the reader's own words and only
proposes the rest, and a model's tidy of it is refused for an unknown source,
a slot dropped or invented, a value too long or a result over the cap. Verified to bite on four planted changes: keep five, pass
three-digit whole numbers, stop merging same-role turns, never activate.

## `check_nona_launcher`

Nona's launcher, driven by real mouse and touch input over CDP: a drag settles
it on the nearer side edge at the height it was left, without opening her; a
press that barely moves still opens her; the next click after a drag is not
eaten; where it was left survives a reload; a smaller window keeps it on
screen below the topbar; a finger drags it on a phone. Verified to bite: a
quick first drag that left the button before its first move was lost while
the press was followed on the button rather than the window.

## `check_nona_boundary`

Not a page check: the files under `web/src/static/nona/`, read as text with
comments and string contents blanked. No file names one of `app.js`'s
top-level names unless it binds that name itself; imports point only down
(core ← runtime ← ui ← index); `core/` names no browser object, clock or
randomness, `runtime/` no DOM, `ui/` no network or storage, and the one
`fetch` is `runtime/transport.js`; every file is listed in the dev server's
`NONA_FILES` at its own path, the site build copies the directory whole, and
the page loads the module after `app.js`. Verified to bite on a planted page
function, a page variable, a `fetch` in ui, a clock in core, an upward import
and an unlisted file — and to stay green on the same names in a comment and a
string.

## `check_agent_coverage`

Every control a reader can use is on the door or says why not. A control is
what the page made one — an element handed a click, change or input listener
(recorded by wrapping `addEventListener` before the boot), an `on*` property,
or a `[data-dd]` dropdown — on three weapons and every module, all folds open.
Each is inside an action's `anchor` or an `AGENT_EXEMPT` entry, the innermost
match deciding; an exemption no page matches is stale; the `todo` count is a
ratchet. Verified to bite: dropping one exemption named its control.

## `check_parity`

Every axis the builder shows belongs to the weapon it is on: an axis is shown
exactly when it has options (and `weaponAxes` answers for the weapon it was
ASKED about), the served polarities arrive intact, an unmodelled evolution is
marked on the tile where it is chosen, and every axis a build has travels in a
board submission. Run it after adding a weapon or anything a weapon can carry.
It opens every weapon's page, so CI shards it: `WFSIM_PARITY_SHARD=i/n` takes
every n-th weapon from the i-th, and four jobs sweep the roster in parallel.

## `check_board_submit`

Plain node against a KV stub, no browser. Every key
`boardPayload()` emits, read out of `app.js`, is a key the worker's `AXES`
table knows how to keep; every key survives into storage; two builds differing
in any one axis are two records.

## `check_engine_grace.py`

Python over the repository's own data: which older engine `release.json`
accepts and on which weapons (`build_site_app.py` `accepted_engines`). A
weapon file moves that weapon and every form, transform group and riven family
linked to it; a kitgun chamber moves the weapon built on it, any other kitgun
part every kitgun; anything else — mechanics, shared data — moves everything,
and that engine is accepted to join the served one (`join`). Tests are never
compared. An engine replaced more than `ENGINE_GRACE_DAYS` ago is dropped with
its file list.

## `check_accounts_pause`

While `ACCOUNTS_PAUSED` is set the worker answers every sign-in step (an OAuth
return included), account change and paid-half write 503 `maintenance` with a
`retry-after`, and none of them reaches the accounts; a read is answered by its
own route. Bites: an OAuth return let through — it fails two.

## `check_pg_scripts`

The board's pipeline scripts themselves, not their stubs, on an on-disk PGlite
through `PG_URL` (`scripts/db.sh`, `scripts/db.mjs`): the inbox read, a build
shipped twice as one row, a batch asked for twice counting both, the queue read
in order, a fact shipped and overwritten with its row spent, the facts and the
library read, an orphan and a dead ruler purged. Bites: an upsert's unqualified
column, which Postgres calls ambiguous — it fails three of them.

## `check_dials`

Plain node on PGlite: a dial never set, or a table that cannot be read, is its
default; a value outside a dial's bounds, or not a dial, is refused and
nothing is written, and a row put out of bounds by hand is ignored; an isolate
keeps what it read for `DIALS_FRESH_MS` and reads the table after, the one that
set it at once; every change is logged with the value before and after and
who set it; a `/api/board/work` reply carries exactly the client dials, as in
force. `CLIENTS_PER_FACT` is not a dial.

## `check_board_sync`

Plain node on PGlite: protocol 8's one door (`worker/sync.js`). An id the site
does not make, a malformed body or another protocol is refused whole; no yes, no
work. A sync is handed a pack with every dial a protocol 8 client reads and the
protocols served. A sync before its `next_ms` is handed nothing, yet its results
are taken and its rows renewed; a result sent twice, or by protocol 7 and then 8,
is one answer; one whose lease ran out counts while nobody took the row, and is
dropped `not_held` once somebody did; a row it names is kept and every other one
it held goes back. A row its engine refused is not handed to it again, and
refused by `refusals_to_withdraw` owners on the served engine is withdrawn,
written down, said by the watchdog and kept withdrawn when asked again. While work
is short a pack is at most open over asking, leased `pack_lease_ms`; a question
someone waits on goes alone, never to a computer holding a pack. `sync_off`
answers `fallback: 7`. `WFSIM_SYNC_PROC=1` runs it with each sync one call
(dial `sync_proc`), alone and beside `WFSIM_SETTLE_ASYNC=1`.

## `check_sync_proc`

Plain node on PGlite: protocol 8's sync as one call (`board_sync`,
docs/BOARD.md §"The sync in the database").

- Six random runs, three with answers settled in the request and three with
  them queued, hand the same syncs to two databases, one through the procedure
  and one through the worker's statements. Each reply, and at the end every
  table the sync touches, must match, leases named by the order they were
  first handed out and an answer by its place, not its number. The books hold
  rows two producers ask, rows the scorer stops owing, a row someone waits on,
  a row a riven chat asks too, clients on older engines, an account over its
  fair share, held and claimed devices, and clients that name a row done and
  held at once.
- The runs went through the call, and some came back to the worker (WS001).
- Through the procedure a sync with results, renewals, states and points is at
  most two waits on a database; through the statements it is more.

## `check_initiator`

Plain node on PGlite: who asked (docs/BOARD.md §"Who asked"). A demand and its
question carry the initiator, a question the earliest-due live demand's, again
after a withdrawal; with `initiator_share_pct` set, an initiator past its share
of the live leases is passed over and the system is not, nobody is at or under
`initiator_floor_rows`, and a lease run out does not count; the SQL's unset
floor is worker/dials.js's default; `factory_forget_codes` erases the user and
bot codes of an ask that is over and keeps the rest; a board batch's initiator
and a code's shape; a submission signed in or out, and an arrival asked as its
queue row names, never as the runner says.

## `check_merit`

Plain node on PGlite: merit (docs/BOARD.md §"Merit"). The caps by rank; an
account first seen granted all its points up to its balance cap, one whose
row the toggle made first granted all the same; a mint with nothing new adds
nothing, a day's mint stops at the day's cap and what it held back never mints
the next day, a full balance mints nothing; an account sees its own, the public
sums only; a change from another site or from nobody signed in is refused.
Priority: the switch keeps and deletes the code's mapping; a switched-on
player's arrivals are held at their price up to the hour's cap and due 30%
sooner, an anonymous one is not; past the share nothing is held; a row that
went ahead of someone is charged, one that went ahead of nobody or was
withdrawn refunded, once, and a settled hold keeps no code; a row someone waits
on is never held.

## `check_board_verify`

Plain node on PGlite (`worker/pg/*.sql`): a board row as a
question of the fact factory. Only the factory's secret asks; an owed row is
asked by `new_build` (an arrival's own build) or `sweep`, an arrival's scored
row not again, a rescore's on purpose, and the backfill asks what nobody did.
A `refresh-` row is due fifteen days after its fact finished, asked at five
days or fourteen, by the batch or the backfill (`oldest_rows.py --self-test`
holds which rows an hour asks: the oldest, within the room, and the forced).
A row is handed out as its build and never a number; a page of an older
protocol, of another engine, or without the reader's yes is handed nothing; a
further result never comes from the same client, another device of its owner
or its network; equal bits make a `verified:` fact, settle the queue row and
credit both clients once, and their tasks say confirmed; other work is a
dispute, answered as an agreement is, its tasks "checking". The official
machines: the live loop claims a dispute and its fact credits the one it agrees
with and refuses the other for a day (told `banned`); a spot check that
disproves an agreement refuses both, takes back their pay and voids their
answers; the scorer claims only what is past its deadline, no client is handed
it, `fetch_queue.sh` reads exactly that and the rows nobody asked for, and its
answer pays a client whose bits it reproduced; the release hands the rest back.
Rows come in PACKS: sized to a minute of the computer's pace, never past its
fair share of what is open, `PACK_FIRST` before its pace is known, leased twice
their length; the first rows in order under one lease; a pack asked ahead, and
no more than `PACKS_HELD` held; answers sent before the pack ends taken, its
other rows still its own, `done` giving back the rest and a row answered twice
counted once; each row judged alone (a fact, a dispute) and credited once; a
row no longer owed left out and withdrawn. A lease given back, an answer of
another engine dropped; what an answer cost (its counts, lanes and time) is
kept; and an order open before the factory is adopted once, as of when it
opened, its result carried.

## `check_gas`

Two parts. Plain node, in the light job: every top-level fn, impl and trait in
the metered sources (`METERED`) carries `#[wfsim_gas_macro::metered]`, whatever its visibility — it
failed with one attribute removed, on a `pub(in crate::fight)` fn too; every
`macro_rules!` there whose body loops is a `metered_expr!` — it failed on
`bump_buffs` unwrapped; and no production line of engine/src,
webapi/src or optimizer/src calls a transcendental on an f64 (`.powf(`,
`.exp(`, `.sin(`, `.hypot(`…) except `rules/math.rs`, the one libm — it failed
with one call put back in data/rage.rs and one in optimizer/src. With `build` (CI's `gas` job: rust, the
wasm32 target, wasm-bindgen at Cargo.lock's version) or `native=` and `pkg=`:
the rows of `tests/gas/rows.json`, fought by the native `wfsim-gas` and by the
wasm engine as the page fights them, give the same gas, work and score, the
same again on a fresh instance and cut five ways — one run a shard over lanes,
seven, asked last first, two halves carried — and the work is gas.rs's price
of the count; and the first row, fought again after all the others in the same
native process and the same wasm instance, counts what it did first. It failed
with one weight changed in the wasm build only, and with a charge made once a
thread in `run_once`.

## `check_lane_memory`

CI's `gas` job, on the wasm it built: one fresh instance fights the rows of
`tests/gas/rows.json` as a board lane does, and its linear memory — which never
shrinks, so it is that lane's for life — stays under `CEILING_MB`. Without
`pkg=` it reads `site/pkg`. It failed at 50.0 MB on the build whose weapon
catalog held every file's yaml tree at once (docs/WASM.md §"Memory").

## `check_clients_doc`

Plain node. Every key of docs/CLIENTS.md's `Values` block is the constant it
names, read from the worker and engine/src/fight/gas.rs (every file defining
it), a key nobody reads fails, and a key it reads that the block leaves out
fails. It failed with `run_price` changed in the doc, and with a key added.

## `check_compute`

The `/compute` page, the account answered by interception. This browser's tasks
are drawn from what it kept and by each task's KIND: a board order as its
weapon and ruler, linked to that board and never naming its mods, and a kind
the page does not know still drawn; a task in progress shows how far it is;
signed out, it says how to count the work under a name. Signed in, every device
of the account is listed by its name with this browser marked, with what each
is doing or when it last answered; one claimed before it had a name is given
its guess; a device is renamed inline and removed after an inline question.
Built site only: nothing computes until a card asks and is answered — an old
default's yes and a yes to an older statement are no answer — a no is kept, a
card page never carries it, a yes turns it on with the statement and when; the
top bar's ring says a task runs, keeps its size and the search's when the task
ends, and leaves only when computing is turned off; a battery holds it unless
the reader allows it; together takes its share of the device whether or not
the reader is at it, as lanes running that share of the time, and past 100%
with solo, while the reader computes, the two share in proportion; solo and
together are two sliders in the menu, a slider let go sets its share, and the
sum is said. A row cut short resumes: a page that goes mid-row keeps that row back
from what it gives back, its fold saved with its engine; the next ask renews
it before asking for anything and folds on to the uncut row's bits; a row
taken since is dropped unanswered, another engine's fold never renewed, and
turning computing off drops it. These board-order cases run on protocol 7, the
one a page falls back to.

## `check_compute_queue`

Protocol 8 on the page (docs/CLIENTS.md §"The cycle, protocol 8"), the server and
the work signal stubbed before the app boots. Built site only. Protocol 8 is
spoken where IndexedDB opens; a measured row's result is kept there until a
reply takes it, goes out by beacon when the page goes (holding nothing), and
after a reload goes in on the next load. Pending points are the work of every
result kept or taken and not yet confirmed, over 1e9. Idle with nothing open,
the page reads the signal and calls no worker; with work open it syncs within
the signal's jitter; `next_ms` holds. A signal is aged by the server's clock (a
same-origin `Date`), never the reader's; stale or unreadable, the page syncs
directly at most every two minutes. Told `fallback: 7`, or with no IndexedDB,
it speaks protocol 7. `WFSIM_BREAK=<name>` sabotages one rule at runtime, to see
its assertion fail.

## `check_stats_files`

The public stats on the page (docs/BOARD.md §"The public stats"), the CDN and
the worker stubbed before the app boots. Built site only. The home tally reads
`stats/tally.json` and asks no worker; a file stale by the server's clock,
unreachable or of another version is passed over for `/api/board/tally`.
Signed out, the ranking is the file's period sliced to the page; a file of
another day, or none, is passed over for `/api/contributors`; signed in, the
worker is asked and the file never read. `WFSIM_BREAK=<name>` sabotages one
rule at runtime, to see its assertion fail.

## `check_quick_fleet`

A frozen survey question, cut short, searched in the page by `quickFleet` on
one worker, on three, on a count that changes every round and on workers resting
between calls: the same build, the same number and the same work each time,
since two volunteers' answers are confirmed only to the bit. A search run to keep its workers hands them back,
and the next one starts on them.

## `check_idle_devices`

Plain node on PGlite: ids that asked in the last half hour are sorted into an
old engine, an asking loop and not computing; one that answered, only just
asked once, or went away is none of them. The watchdog says each kind in
Chinese once, not again for the same devices, and again, listing all, when a
device it has not said joins a kind.

## `check_tasks`

Plain node on PGlite (`worker/pg/*.sql`): the fact factory (`worker/tasks.js`)
on random books. LIVENESS — a witness is handed exactly the first question it
may answer in order (a waited-on one, then the earliest deadline), compared
with a slow scan of the rule, and nothing only when nothing is eligible; a
pack is exactly the first n of them a pack may carry, none someone waits on.
TERMINATION — honest, lying and non-deterministic witnesses answering at
random leave no question waiting once the official machines have had their
turn, and no honest witness is refused. Then the production cases one each:
nine answered-out questions do not starve a tenth, three answers differing
only in work are an engine fault and refuse nobody, two producers asking one
thing share one computation credited once, a fact is told at once to a later
ask and opens again on another engine, a withdrawn ask is handed to nobody.
A pack's answers sent together, in two parts, make every state, fact, credit
and hour that each alone makes, and two settlings of one question at once pay
each witness once. The watchdog says a stalled kind, a dispute nobody settled,
once a cause; a question four hours past due but not one an hour past; clients
disagreeing with the hour's facts on the served engine, the worst owner first,
but not a few among many, another engine's or an older hour's; and a scene's
cell rulers still filling, measured and owed, then nothing once full. A recheck leaves two agreeing open, needing a third, nobody
credited and nothing delivered, and settled again it stays open; no device,
owner or network of either is the third; a third agreeing from a pack makes the
fact and credits all three once, a third differing makes a dispute, and a
rechecked row past its deadline goes to the official machines, who credit the
two who agreed.

## `check_settler`

Plain node on PGlite: the settler in shadow (`worker/settler.js`,
`scripts/factoryd.mjs`, docs/BOARD.md §"The settler"). Its fair table is the
worker's `tableOf` and reads back from `fair_shares` to the last digit; a target
moved past the tolerance, or an account missing, is a difference. Its snapshot
counts computing as `/api/board/computing` serves it, open as what may be taken
now, urgent as what someone waits on, and its heartbeat names its commit. Three
questions settled through the real request path are judged as it judged them,
none twice, none before the lag; a fact whose bits no group gave and a witness
left unpaid are each a difference, kept in the finding with the question. An
official answer and an older engine's peer are counted apart; two answers of
one owner are one witness. One pass of the daemon's every tick runs clean.

## `check_settler_async`

Plain node on PGlite, for the settler's queue (dial `settle_async`, docs/BOARD.md
§"The settler").

- Twelve random books are answered on two databases, one settling in the
  request and the other queueing each answer for a pass. Both must leave the
  same questions, credits, verifiers, hours, scores, queue and demands.
- A pass is killed at each of its calls in turn. Once the next pass and the
  watchdog's `repay` have run, the result must be what one clean pass leaves.
- Every question is settled a second time, and nothing changes.
- A queued question goes to nobody within the grace and to anyone after it.
- An answer clears the claim of a pass holding its row.
- An official answer and a producer told every answer still settle in the
  request.

`WFSIM_SETTLE_ASYNC=1` runs `check_tasks`, `check_board_verify`,
`check_board_sync` and `check_fair` with every answer queued and a pass run at
once (`scripts/pg/test_db.mjs`).

## `check_fair`

Plain node on PGlite (`worker/pg/*.sql`): fair share
while work is short (`worker/fair.js`). The targets are 0.7 by capacity asking (work delivered) and
0.3 equal among established accounts, so with 80% of the capacity against four
accounts of 5% the targets are 62% and 9.5%. A new account shares by its cores
alone. Every unclaimed browser is one account. Then through `/api/board/work`,
two rows open: an account that took the whole window is handed an ordinary
nothing (not stale, not banned) while the walk-ins get a pack. With 400 rows
open, the same account gets a full pack. Capacity is work delivered: the best
credited hour, or the credited rate now if that is more. Uncredited work never
counts, a machine with nothing credited counts at the median, and a best hour
from before the served engine's first answer counts for nothing.

## `check_renew`

Plain node on PGlite (`worker/pg/*.sql`): a pack's lease is a heartbeat. A row
whose lease ran out while its page computed it is kept on renewal when nobody
took it, and its answer forty minutes in is taken. A row another computer took
since is not kept, nor is a row outside the lease. A row under the ceiling
is handed to nobody else; past it the renewal still holds, another computer
may take it too with the first kept as the holder before, a third take keeps
the first and lets the second go, and the holder before's answer counts — one
owner's two holders answering being one witness, credited to nobody.

## `check_riven_gain`

Plain node on PGlite (`worker/pg/*.sql`), the served
engine stubbed: a riven gain as a producer of the fact factory. Only the bot
freezes its question, once, and freezing it is the ask; a computer asking for
work is handed it only on the served engine, and nobody else holds it
meanwhile; a computer offering few cores is not handed a fresh one someone
waits on until it has waited two minutes; a lease runs on while its holder
says so; the chat is told once that a computer took it; an answer counts only
under its own lease and is kept as a build the bot may judge; one answer
credits nobody, the second run never goes to the same owner nor the same
network, unequal answers credit nobody, and two owners' equal answers credit
both, the work and the hour, agree the appraisal and send its build to the
board's door. Beside the board's orders the order is a chat's, then a new
build's row before a survey shape due in a day, an overdue shape before that
row, and a shape before a rescore. A question frozen on an older engine is
still handed out; a chat asking what a survey agreed is answered at once with
a build to tell; and an appraisal frozen before the factory is adopted once,
due as of when it was opened, its answers carried.

## `check_ledger`

Plain node on PGlite (`worker/pg/*.sql`). A device's
ledger gives each credited hour to its span's holder — the device itself, the
reader's own account, or another account never named — says an ended span as
ended, adds its all-time split up by holder, lists its window's hours alone
and says earlier ones exist, and counts the window's unpaid results by why
(disagreed, waiting, refused), a volunteer's alone; it is asked by POST with a
well-formed id, and by an owner by six characters, nobody else; an account's
ledger holds its own spans' hours device by device, a device taken since
counted as removed; no answer carries a full device id or an account's id or
name.

## `check_contribution`

Plain node on PGlite (`worker/pg/*.sql`), the paid half
stubbed. A signed-in browser claims its device and the last claim owns it, its
work going with it; an account's points are its devices' credited work, a
refused device's counting for nothing; every account with a claimed device is
ranked, most first, by all its points or this UTC calendar month's alone, and
ANONYMOUS — no name, handle or mark, and the paid half never asked about it —
until it agrees, when it shows the display name or else the username and the
mark the paid half proves; a no is kept, a yes can be taken back, and the
reader's own row is marked to them alone; a ranking answers the rows asked
for and how many there are, each with a key that is the same in every answer
and period, carries no account id and is another the next day; a yes and work credited make the
account a volunteer, since that yes, shown beside a name and never on an
anonymous row; a browser asked by its own id is told
what it earned and whether it is claimed, a refused one nothing; nothing is
claimed signed out, from another site or with a malformed id; an owner sees
each device by its name, when it last answered and the task it holds by kind
and public facts alone, renames and removes it, and nobody else can; deleting
the account releases its devices and its answer.

## `check_folds`

Every block and every section on a weapon page folds, and the jump menu is READ
OFF them. Both halves derive from the DOM, so the check asserts the property
rather than a list of sections: each box carries a fold id and a caret (both
applied by the wiring pass — a box it never reached looks identical and does
nothing when clicked), shutting one hides its body by COMPUTED STYLE, a control
in a heading keeps its own click, what you shut survives a reload, and the menu
lists exactly the folds the page has under their own translated names. Then it
jumps into the deepest section of a fully collapsed page and asserts the block
above it opened while the target kept its own fold, which the row's caret then
opens without moving the page. The menu is dragged with a real pointer
(`Input.dispatchMouseEvent` — a page-side `PointerEvent` carries no active
pointer id): shut by its grip, open by its header, and off the corner to see it
stop at the window's edge and under the topbar. A click on the grip must still
open it, growing away from the side it is parked on, and the section scrolled
to must be the marked row and the name on the shut grip. Verified to
bite: removing the heading-control guard, the section half of `pageFolds` and
the clamp reddened twelve assertions between them, each naming its own half.

## `check_metrics`

Every metric `engine::rules::metrics::ALL` declares reaches the page, and the check
does not know what they are either: it reads `/api/meta.metrics`, asserts each
is offered by the Measure control, and picks each one to see ITS unit and ITS
number on the headline. So a metric added to the engine is covered without this
being edited — proven by adding a third entry, which reached the control and
the headline with no page change. It also asserts that an id the engine does
not declare is REFUSED rather than drawn in the default's units.

## `check_mobile`

GEOMETRY, not DOM: the page fits the screen at 360–1280px,
nothing past the viewport, no sideways scroll, and a mod NAME keeps room to be
one. It measures the page WITH A POPOVER OPEN, which is the one thing that can
leave the viewport with no container noticing — `place` caps the width BEFORE
the clamp, because a popover wider than the screen cannot be clamped into it.
It sets `maxTouchPoints` itself, since `mobile: true` on
`setDeviceMetricsOverride` leaves it at 0 and every touch-only behaviour would
go untested.

## `check_equip_rules`

What a mod's CARD says the weapon may do, in both
directions. An equip rule is asked of EVERY firing mode, and an Incarnon
weapon always has its form installed. The engine decides (`pool_for_build`),
`/api/meta` states the consequence per evolution (`evo_forbids`), and the page
acts on it: the picker never offers a Cannonade on Dual Toxocyst and does on
Magnus, and a saved build still carrying the pair greys the Incarnon modes
with the reason on screen and is refused by the sim. It covers the LOCK the
same families carry ("set to its default ignoring other bonuses, even negative
effects"), on Magnus: the panel pins the stat and NAMES what pinned it. That a
lock also removes a live buff is the engine's
(`a_multishot_lock_removes_a_live_multishot_buff_too`, MEASUREMENTS M30).

## `check_board_link`

A board row opens THAT row: the build it names AND the
ruler it is on. It walks every ruler and asserts against `BOARD` itself. THE
EXILUS SLOT IS ITS OWN ASSERTION, because a row carries that card in a field of
its own: comparing everything the page opened against the row's flat mod list
read as agreement while the ninth card was dropped, and on a melee that card is
the Tennokai one. It holds a case the live board has never had — ONE WEAPON,
TWO MODES — by injecting a synthetic second-mode row. IT ALSO WATCHES THE ORDER one level
down: the builder's picker groups a weapon's deeper ranks by mode and numbers
each inside its group, asserted over EVERY weapon the board holds in more than
one mode, picking the WORST-INTERLEAVED one for the DOM half. The rank
assertion beside it says #1 is that mode's LEADER, because a position counter
and its rank agree however the list is ordered.

## `check_board_depth`

HOW DEEP THE BOARD IS READ is the reader's choice, and the publisher no longer
holds anything back — so the rule left a place with unit tests for one without.
This is that rule on the real page: half is the default and the boundary is
INCLUSIVE, "All" shows every row, the line is drawn per GROUP (one ruler, one
mode, one riven-ness) so a strong group cannot empty a weak one, a group whose
leader scored zero is never emptied, `#1` still means `#1` because a depth
keeps a PREFIX, and the choice outlives a reload. ROWS ARE INJECTED: the
published board is whatever the bot last wrote, and a fixture is the only way
to ask about a row at exactly half. Verified to bite — turning the boundary
into `>` fails five of its nine assertions.

## `check_disclosure`

What the app does NOT model is ON THE PAGE, in every
family that has one: a weapon banner, an evolution chip, a mod line, an arcane
line, an enemy caveat. It covers the fourth kind of admission, which is not a
shortfall: a LIVE BUG (`live_bugs:` on an arcane, or beside the effect it
kills on an evolution) says the number is RIGHT, the game is wrong, and a
hotfix changes it. The live bug is INJECTED, with the flag's removal as the
negative control, so the claim is that the machinery can SAY it rather than
that some perk happens to be broken. It carries a NEGATIVE CONTROL — a weapon
with nothing to admit shows no banner — runs in BOTH languages, and walks the
BOARD, where weapons are compared and a weapon with unmodelled parts must not
look like one without them.

## `check_fixed_tier`

A tier with one option (every Genesis's tier 1) is installed and never
offered: the builder draws it with no ⋯ and a click opens nothing, the
optimizer's limits do not list it, and the Shapley analysis has no part for
it while installing it in every subset, so the full subset is the build on
screen.

## `check_wf_buffs`

A Warframe ability buff is the FIGHT's and reaches the
number: the section draws in both languages under DE's OWN names (战吼,
黯然失色), the card's value follows Ability Strength, ticking one moves a real
`/api/simulate`, two of a FAMILY do not stack AND the page says which one
lost, the optimizer's fight card names the ticked one and holds no control,
and — the negative control — no RULER carries one.

## `check_pace_and_hits`

What a room-clear is paced by, and where an impossible
number hides. `dps` is the whole engagement with its reloads in it; burst DPS
is the same damage over the time the trigger was down, RECOMPUTED rather than
trusted. Beside it: time to the first kill with its spread, the opening
magazine, the biggest single instance, damage per shot and per pellet. Every
block folds and REMEMBERS across a re-render and a reload, so the state lives
outside the markup.

## `check_combat_record`

A ledger has to multiply out, asked of EVERY row.
`engine::record` is one ordered stream of everything that happened where **a
row is one number the game POPS**, not one hit — the only output of this app
that can be laid beside a recording and checked number for number. A pellet
landing on a shielded body pops TWO numbers, because Toxin bypasses a shield
and the rest does not. IT IS THE WRITE PATH, NOT A REPORT: the stream is
filled by the same call that moves the target's pools, from the same numbers.
What it is authoritative about is bounded and the bounds are in
`engine/src/record.rs`. THE CHECK DOES THE ARITHMETIC OFF THE SCREEN — it
reads the factors as DRAWN, multiplies them, and compares with the two totals
the same row prints. It pins the KIND list so a fifth thing cannot arrive
unnoticed, and MAKES a miss happen (the target pushed to 40 m) because every
claim about misses passes perfectly on a fight that has none.

## `check_damage_pops`

Every drawn number NAMES the record row it is (the id
resolves, that row's damage is the text on screen, the row belongs to the
frame being shown), and every row in that frame is drawn, up to the cap. The
second half is not decoration: "every number on screen is a row" is satisfied
perfectly by drawing ONE of them.

## `check_debuff_coverage`

The DEBUFF table is the BUFF table read from the
other side, one component fed from both: `DEBUFF_ROSTER` mirrors
`buff_roster`, `Frame.debuffs` mirrors `Frame.stacks`, one renderer. It
asserts the SYMMETRY rather than the numbers, plus the one thing that is not
symmetric: A RESPAWN IS THE SAME TARGET, so its stacks drop to zero and climb
again INSIDE one series and that gap counts against uptime. Rows the run never
touched are dropped.

## `check_custom_enemies`

A target you MADE is a target like any other, which
is the test of the claim: a custom enemy is an `EnemySpec` in the scenario's
list, so the simulator, the optimizer and the target card need no code for it.
The IMMUNITY is MEASURED rather than read off the card (a Toxin-immune target
takes literally nothing from a Torid; the same target at x1 takes something),
and DELETING a custom must repoint the fight.

## `check_run_counts`

How hard you measure is a number someone can set, in all
three modules, and the answer differs in each. The simulator defaults to the
rulers' 1000 so a first number is comparable with the board without touching a
box (1.3 s a run in the shipping build, against 0.14 s at 100). The quick calc
takes its own with a FLOOR of 10, where a status mod stops being a coin flip
(M24: one run swings it ±39 points), and a number under it is raised rather
than obeyed. The optimizer has no count of its own — its final round is a
simulation at the simulator's — so it asserts there is no final-round box, no
`final_runs` in the request, that the request carries the simulator's runs and
follows them when they change — and, its negative control, that the CPU
threads box is gone and no `threads` reaches the request.

## `check_arena`

The arena is a place you can DRAG, and what you drag is what
gets simulated. Bodies are drawn at their REAL radius (`space::BODY_RADIUS_M`,
0.25 m), so "as close as they go" is visible: they touch at CONTACT (0.5 m)
and will not pass through each other, which the engine clamps to as well. The
scene uses a viewBox, because a host has zero pixel width while its panel is
on another tab and one drag would write `[null, null]` into the fight;
`paint()` replaces the markup, so listeners are delegated rather than bound to
circles. A BENCHMARK'S FIGHT IS NOT DRAGGABLE and the scene refuses the
gesture ITSELF, because `lockOfficialScenario` sweeps
`input,select,button,textarea` and these bodies are SVG circles. The check
opens a scenario of its own first and asserts that it did, since the app lands
a first-time visitor ON the official ruler. The OPTIMIZER draws the same scene
read-only.

## `check_roster`

A fight is n against m, and the page sets BOTH halves. The enemy half has been
a list since the formation existed; the roster in block 2 is the other one. A
seat is a LINK (`{weapon, preset}`), never a copy, so editing that preset moves
this fight too — and it resolves through the same `seatPayload` the open build
goes through, which the check proves the only way it can be proved: two seats
on ONE weapon in two builds deal two numbers. Seat 1 is the open build, has no
remove and is the only one a board can take. The RECORD is a second reader of
the same engagement, so `/api/log` wires the roster through the same
`seat_the_rest` — a record drawn from the wielder alone is a TRUE record of a
fight nobody ran. Two negative controls: one seat sends no roster at all, and
an official ruler is one gun whatever the last fight held.

## `check_formation`

A formation is something you build on the floor, and what
you build is what gets simulated: bodies draw without standing on each other,
any one drags, the payload matches the scene body for body, and a real
`/api/simulate` answers HIGHER for a crowd than for one body. AIM IS A PLACE
rather than a target: the marker rides the target until dragged, and once
dragged the beam is on whichever body the LINE crosses — asserted with two
bodies on one line where the nearest to the cursor is the FAR one. Two
negative controls: a formation of one sends zero and a null aim, and an
official ruler refuses a crowd both by disabling the control and by not moving
when it is clicked anyway. It asserts the per-body unit stamp ON THE WIRE.

## `check_gunco_stated`

Every weapon says which Condition Overload rule it is
computed under, with nothing equipped. The rules are per weapon and
hand-transcribed: Adding or Multiplying, which attack parts take it, what
fraction of the base the term reads. It is unconditional and says "no source
equipped" plus how one WOULD be computed. The check walks all three behaviours
from three weapons the catalog classifies differently and asserts they are
three different sentences.

## `check_opt_limits_scroll`

A pick in the optimizer's Limits redraws the whole box, and each list in it
scrolls on its own. It scrolls the mod list down, excludes a row it can see,
takes it back, and asserts the list and the page are where they were — a
fresh list opens at its top, which is what a forgetful redraw looks like.

## `check_opt_replay`

The only check about a build that CANNOT go stale: it
runs a real search, applies the winner through the button's own path, runs the
simulator, and asserts the two numbers agree inside 4σ of their two standard
errors. It does not know what an axis is, so a fifth one is covered on the day
it is added. Its rotation of NEGATIVE CONTROLS is discovered from the row's
own `replay` keys: each is deleted in turn, the ones the engine notices are
named in the assertion's own title, and a degenerate axis is REPORTED rather
than failed. The sharp one is last: a build assembled from a replay with a
LIVE axis removed must fail the assertion that otherwise passes. Two weapons,
because no single one has every axis live.

## `check_build_axes`

The cheap half of that pair, and the file says so.
`engine::board::builds::BUILD_AXES` is the one declaration, served at
`/api/meta.build_axes`; the three JS surfaces that carry their own spellings —
the page's build state, the share tuple, the worker's board record — each
declare which axis their fields cover. It asserts coverage BOTH ways and that
the worker's record and identity key are still DERIVED from its table. Plain
node against the served meta and two source files.

## `check_melee_slots`

A melee weapon has TWO slots a gun does not, and one
decides what it swings. Every assertion is on the WIRE or on a real
`/api/simulate`. Its sharpest pair is the ROUND TRIP — `buildPayload` into
`stateFromBuild` must put the stance back in the STANCE slot rather than in
slot 9 — and the FALLBACK: an empty slot fires the entry's own script, which
happens to be Crushing Ruin's, so a stance that failed to apply would read as
a pass.

## `check_stance_capacity`

A stance is an AURA: it HANDS capacity back instead of spending it — five
points, ten on a matching slot — so the number beside the mod slots is the
weapon's own capacity plus the grant. The Magistar's slot is Vazarin, which
makes Shattering Storm free at 70 and Crushing Ruin 65 unless a Forma is spent
on the slot, and all four readings are asserted ON SCREEN.

It is a page check rather than an engine one because the page owns this
arithmetic: `capacityUsed()` and its Forma bill MIRROR `engine::rules::capacity`, so an
engine that is right and a mirror that is not still reads wrong to everyone.

It also asserts the AUTO PLAN, because that mirror drifted the same way twice:
planning against the weapon's own capacity and not the stance's grant buys
polarizations the build does not need, and reaches for an UMBRA FORMA to do it —
the one item `engine::rules::capacity::fit` is written to spare. The engine answers five
regular Forma and no Umbra for that build; the page has to say the same.

## `check_opt_upload`

**EVERY FINALIST GOES TO THE BOARD.** A path to the store that runs off a
simulator run alone takes one build at a time, so a search ranking twenty
uploads none of them. Two properties are asserted, and the second is the one that is easy to get
wrong — the payload comes off the ROW (its mods, arcanes, mode, valence, exilus)
and not off the page, which holds a different build; and **nothing is
pre-filtered here**, because how full a build must be is the searcher's own
setting and `/api/board/check` is `validate_for_board` rather than a copy of it.
The check hands it a two-mod build and asserts the REFUSAL came from the door.

Consent still gates it, and with uploading off the line says so rather than
staying blank.

## `check_opt_row_axes`

**A RESULT ROW STATES EVERY AXIS THE SEARCH VARIED.** `engine::board::builds::BUILD_AXES`
declares what a build consists of, and a ranking row that omits one cannot be
reproduced. The case that earned the check is the VALENCE: an adversary weapon's
progenitor element is part of what the build IS — two Kuva Nukors differing only
in it are two builds with two scores — and the rows drew mods, arcanes,
evolutions and mode but not that, so a search ranging over three elements
printed three rows that read identically. Assembly and the riven were missing
the same way.

**IT ASSERTS THE DESCRIBER, not a search.** `buildContentsHtml` is the one
component the optimizer's rows and the simulator's "open now" line both use, so
a missing axis is missing in both at once — and driving a real search would
spend minutes to exercise a pure function. It is handed one build varying on
every axis at once, which is not a realistic build and is the point: a realistic
one leaves axes empty, and an empty axis and an unrendered axis look the same.

**AND ENGLISH IS PINNED.** Every name on that line is translated, so a check
whose needles are English reads a Chinese page as eight missing axes — the same
picture a genuinely missing axis makes. `LANG` is read at boot, so the switch is
a reload.

## `check_calc_recovers`

**THE QUICK CALC SURVIVES LOSING ITS WORKERS**: it never stops producing
numbers and stays stopped. Three faults each make that permanent on its own,
and the check holds all three:

- `laneAt` replaces a DEAD lane rather than returning it, and nothing falls back
  to a fixed lane — the lane count is a stored preference and the trigger is
  deterministic, so a reload would rebuild the same dead pool;
- `laneAsk` treats `worker_dead` as a failure, like `cancelled`, never as an
  empty measurement;
- the fight's key is stamped only by a scan that measured everything, so a scan
  that died half way is asked again.

**IT KILLS THE POOL**, which is the only honest way to test a recovery path, and
asserts the calculator reaches a COMPLETE answer anyway — completion being the
key, which is now stamped only by a scan that measured everything. On the old
behaviour it reports `86 of 87 could not be measured`, which is the bug as the
reader met it.

**A MODULE THAT IS ONLY SLOW KEEPS ITS LANE.** The worker says so every 5 s
while the page downloads its module; with the loading watchdog cut to 7.5 s,
the page's module dropped and the wasm held back 16 s by the test server
(`openApp`'s `delay`), the lane still answers `/api/meta`. Verified to bite: with the beat removed, the lane dies.

**ONE DOWNLOAD, STARTED WITH THE PAGE.** A cold load makes exactly one wasm
request, and it begins before app.js has finished arriving: the page's preload
and `engineCompile`'s fetch only share a response while the two match, and a
mismatch is a second full download every boot survives. Verified to bite: with
the preload removed, the request starts after app.js.

**ONE ENGINE A PAGE.** With the wasm blocked in every worker's own CDP session,
three lanes made after boot still answer: they are handed the page's compiled
module and download nothing (docs/WASM.md §"One engine a page").

**A MODULE THAT WILL NOT DOWNLOAD FAILS ITS LANE AT ONCE.** The wasm is blocked
at the page, which downloads it for every worker, the module it held is
dropped, and the lane must report `worker_dead` well inside the 90 s
loading watchdog, which is how long a reader whose download failed would
otherwise wait.

**A FROZEN PAGE IS NOT A SILENT WORKER.** A lane holding a request nobody
answers is frozen through `Page.setWebLifecycleState` for longer than its stall
window; it must be alive on waking and dead once silence runs on while awake,
because the watchdog counts silence only in the looks it actually takes.

**A FAILED LANE SAYS WHY.** With `track` wrapped to be read, the frozen lane's
death is `worker_silent`, the blocked download's is `worker_load_network` while
the site names this page's release, and `worker_load_stale` once `/release.json` names
a newer one (docs/ANALYTICS.md).

**A BOOT WITH NO ENGINE SAYS SO.** Reloaded with the wasm still blocked, the
page's banner must name the engine, never a `TypeError` from reading the
engine's first answer when there was none.

## `check_build_finder`

**THE FINDER FINDS, THE BAR HOLDS.** The build finder is a query over the
board's builds and never a selection, so every row it lists must satisfy the
query: a card required from the overview is carried by every listed build,
excluded it is carried by none, and a third click clears it; "I don't have it"
on a piece of an opened row excludes it the same way. A row's mods are in the
order the build was saved, exilus last. The overview ships shut; any row opens
in place into the same detail, one at a time; any build can be made the
reference, every other row then reads against it, an opened row lists what it
has that the reference does not, and the reference is taken back as it was
given. "Open" makes the build current AND puts it in the build bar as a
read-only chip, and the finder then says it is already there. It folds like
every box, ships SHUT above a build of the reader's own on the weapon and
OPEN with none — a first build made there does not shut it — and the reader's
answer is what gets stored, the stored answer
carries to the next weapon in both directions, a redraw keeps it, a click in
its search box does not fold it, and the jump menu lists it by name.

**KEPT BY WHAT THE BUILD IS.** A board build's id ends in its rank, so the check
drops the opened build's score to renumber it, leaves the weapon and comes back:
the bar must hold that same build once and the page must reopen on it — not on
whatever now holds the old rank. × takes it out of the bar and leaves an unsaved
build open rather than another board row.

**…AND THE BAR IS WHERE IT IS SAID.** A board row is a locked chip with every
builder block inert, a copy of it is an ordinary chip you can edit, and the
unsaved build the page lands on selects no chip at all. The scenario bar keeps
its single control, and a weapon with no rows draws the finder's empty state
rather than a stale table.

**IT MUST RUN AGAINST `site/`.** `board/<weapon>.json` is FETCHED at runtime and
the native dev server does not serve it, so a run pointed at 8787/8799 sees an
empty board and every assertion passes on an empty table. The first check
asserts the weapon under test has rows, which is what makes that loud.

## `check_riven_pool`

The riven editor offers the stats that weapon's rivens
actually roll, in BOTH slots. A PHYSICAL stat is refused only by evidence
(`data/rivens/physical.yaml`, or a card in `exceptions.yaml`) and one with none is
offered; every other stat comes from `build::rivens::derived_for`, overridden per
riven FAMILY by `exceptions.yaml`. See DATA_SOURCES §"Riven pools" (MEASUREMENTS
M35).

THE TWO SLOTS ARE DIFFERENT LISTS, which is why a case may state one answer per
slot: five stats are bonus-only and one melee stat is malus-only.

## `check_riven_family`

A riven is a card for a weapon FAMILY, not an entry:
*"Riven mods can be used on variants of a particular weapon, including MK1,
Prime, Vandal, Wraith, Dex, Prisma, Mara, and Syndicate variants"*. The scope
is (FAMILY, RIVEN CLASS) rather than the family, because a KITGUN chamber
built as a primary takes a RIFLE riven and as a secondary a PISTOL one. A
saved riven holds ROLLS and the shown value is that roll against THIS weapon's
disposition, recomputed by `/api/riven` on every render, so one card reads
1.45's worth on a Burston and 1.35's on its Prime. It holds the shared list,
the disposition RATIO (2.243 → 2.088 = 1.35/1.45), three negative controls,
the migration, and the rename/delete sweep with a same-named card in another
family as its control. A RENAME AND A DELETE REACH EVERY BUILD THAT NAMES THE
CARD (`repointRivenInBuilds`, whose SCOPE IS PASSED IN because rename and
delete pass the family's members while the migration passes one weapon). AN
EDIT IS NOT A DELETE: editing a riven is the game's own reroll, so a build
KEEPS it and picks the new values up — dropping the rank from 8 to 0 on the
Burston takes the Burston Prime's build from 18 drain / +208.8% to 2 /
+23.2%, slot intact.

## `check_enemies`

Every TARGET shows a picture that loads, a wiki link built
from its ENGLISH name (the whole pass runs in both languages, because a
localized name in a wiki URL lands on garbage), its VULNERABILITY COLUMN, and
a statement of what the sim does not model about it. Enemy art is declared in
the enemy's own YAML (`image:`, wiki-hosted), NOT in `data/assets.yaml`.

## `check_start_edit`

An optimizer start is edited in the builder, and the player's own build
survives it: a start is added and shown as the simulator's card, opened in the
builder (banner up, build bar away, a pin on every position), a card pinned and
another changed; Done writes both into the start, Discard leaves it as it was,
and each time the player's build and preset come back with identical contents.
An optimize from the start is a quick descent whose every answer keeps the
pinned card.

## `check_result_to_start`

"Send this build to the optimizer" adds the build the shown result MEASURED as
a start — the copy taken when the run was sent — and opens the optimizer: a
short fight is run, a card changed afterwards, and the new start holds the
measured cards and not the edited ones. Sending it again through the door
(`simulator.result.send`) adds no second copy.

## `check_search`

A real optimize in the shipping build: even a tiny scope is DESCENDED and
the page says so; a big one descends from the player's start and keeps its
locked card in every row; an asked-for walk (`strategy: 'exhaust'`) that
finishes reports `exhaustive`, one the budget cuts reports its COVERAGE and
does not pretend, and the WORKER FLEET covers more ground than one worker
would.

## `check_search_presets`

The built-in searches (`data/search/presets.yaml`): a reader who owns no
search has the first one — four starts, each the open weapon's 60/60 card of
one element, at its stated run terms — with its controls inert and its note
shown but the run button live; an edit that reaches it anyway is not stored;
⧉ makes an editable copy, deleting that copy brings the built-in back, and a
shotgun's starts are the shotgun's cards.

## `check_ime_keys`

Every keydown handler that reads Enter or Escape asks whether an input method
is composing (docs/UI.md §"Every box takes a Chinese input method"). No
browser; runs beside `check_app_parts` in CI.

## `check_ime_search`

Chrome's own IME emulation types pinyin into each box that filters as it is
typed in — the home search, the board's, the optimizer's mod filter, the sync
page's list and a long preset bar's — and asserts the box survives the
composing, holds the committed characters and filters by them. A box added
that redraws itself belongs in its list.

## `check_gain_band`

A quick-calc chip says HOW WELL IT KNOWS its own number
and never prints a zero. The scan reads `score`/`score_se` and
`dps`/`dps_se`, the means the server already computes. THE WIDTH IS THE COMPARISON'S OWN and it is DERIVED: `/api/simulate`
returns the per-run series when the caller says it will pair with it
(`run_series`), and the chip's band is the spread of `c_i - ratio*b_i` over
those runs. A chip therefore has three shapes and the check asserts all three
OCCUR: exact (`+165%`), banded (`≈+3.1% ±7.2%`), and a measured zero that says
"no effect here" in words and points at the row's own disclosure line. An
option not SEPARATED from the leader is marked `tied`, on the leader too. Its
NEGATIVE CONTROL is Serration against Amalgam Serration: they differ only in
base damage, band to exactly zero, and order as the cards state — measured
0.9623 = 2.55/2.65 at every build strength and run count, which survives only
because the two are paired against the same luck.

## `check_market_marks`

**THE MARK IS A PROPERTY OF A CARD, NOT OF ONE LIST.** warframe.market is the
authority on what a card costs, so a tradeable one carries a mark that opens
its page — in the mod picker AND on the equipped mod, in the arcane picker AND
on the equipped arcane, and in the weapon's own title. The seated mod's mark
must be the picker row's mark: the same card cannot have two prices. The
reader's switch (topbar settings) then has to take EVERY anchor off the page, not just
the list that happens to be open, and put them all back.

## `check_mode_def`

A mode is EXPLAINED, not just named, and its name is
DERIVED. Each sentence is a TEMPLATE with `{named}` holes filled from
`/api/meta`'s forms, so a weapon that arrives tomorrow explains itself and
costs no translation. It explains THE MODE YOU ARE IN and not the other six —
exactly one entry, and it is the one you are in, the second clause carrying
the meaning; comparison across modes is the BOARD's job, which ranks every
mode of every weapon as its own row. The names appear in the DROPDOWN, read
from `modeOpts`, and must TELL THE MODES APART (a mode id can be a form id).
Every line either carries a NUMBER or names something this mode does and its
neighbours do not: how many of its swings reach the whole room, whether it
spends the combo counter, whether its damage is a slam the weapon's reach does
not bound, what it forces on the target. THE THREE NUMBERS ARE IN ONE UNIT:
`swing_share` and `radial_share` ride the FORM, because a combo script's
multipliers are relative to the ENTRY they are written in and the explosion is
not in the script at all. A NAME THAT ALREADY SAYS THE TRIGGER DOES NOT SAY IT
TWICE, compared on the SOURCE strings with both sides checked non-empty.
Carries a MATCHED PAIR (the Mausolon and Cortege must not be told they have an
Incarnon anything; the Torid and Lex must still say so) and runs in both
languages.

## `check_gain_freshness`

A scenario edit reaches the quick calc immediately,
including a field nobody has invented yet: the scan's cache key is DERIVED
from the fight it will run. It asserts the EVOLUTION axis (which ranks with no
picker open, so it tests the re-ask and not a repaint) and probes the scan's
own BASELINE rather than a candidate's gain.

## `check_buff_cards`

Buff cards are named in the display language, open at the
stack count the rule says, and report a coverage never rounded up to a flat
100%. It walks the one buff that is a WEAPON PASSIVE — the Ocucor's tendrils —
because a stack count nobody can set is a mod nobody can measure. See BUFFS.md.

## `check_gain_axes`

A weapon opens holding each evolution tier's first option, and the quick-calc
gain scan's evolution candidates swap ONE tier's perk, never empty one. It also
holds the MODE axis to its one field: a mode moves no other part of the build,
so a candidate that overrode a second field would be measuring a build nobody
asked for — and a mode a mod has taken off the weapon is listed without being
measured.

## `check_replay`

The benchmark fight plays back on screen while the average above it holds
still: the buff curves
draw, scrubbing drains the pools, and play advances the clock at the chosen
multiplier.

## `check_preset_independence`

No collection's state is written from outside
it: switching a build must not move the fight, and editing the fight must not
touch a build.

## `check_zero_presets`

Nothing is owned until it is made: browsing weapons and opening the optimizer
store nothing, and the first real edit stores exactly one. A bar with none is
empty and says the editor stands on the default; a build edited back to the blank
is deleted, and the next effective edit writes `preset 1` again.

## `check_preset_ids`

Every stored entry has an `id` of its own and keeps it: a list stored without
ids is minted at boot and keeps them on the next load, a duplicated id is split,
"+ new" and ⧉ mint fresh ones, and an edit and its undo keep the entry's. A
riven is minted by the fold, which repoints the build that names it. And
everything points by id: a pointer, a Forma group and a fight's custom target
an older page stored by name are read as the ids they named, two builds may
share a name and each opens as itself, and renaming a custom target moves
nothing a fight names.

## `check_webmcp`

The browser's agent gets the door's table: with a stand-in `document.modelContext`
installed before the page's scripts, the tools are registered while the page
boots without breaking the boot, the live set once booted is every tool the
door lists with the choices only `META` knows, the first set is taken back by
its signal, and a tool answers as the door does.

## `check_agent_discovery`

What an agent reads to find WFSim, in the built `site/`: the AI catalog names
only documents the site serves, each entry by exactly one of `url` or `data`
under a `urn:air:` identifier and a media type; robots.txt and the page's head
point at it; and the A2A card the build wrote is the one `mcp/a2a.js` serves.
Run after a site build.

## `check_mcp_auth`

The MCP server's door, `mcp/index.js` with its engine stubbed: it describes
itself as a protected resource, a tool call without a key spends the address's
allowance and with one the key's, the handshake and the list spend nothing, a
key that is not one is a 401, and a key is looked up once. The two build
tools save a build as its weapon's preset in the page's own shape and read it
back as the build it was, replace one by id, and say what is missing — a key or
a claim. The A2A card carries every field A2A requires and lists
exactly the queries the endpoint runs; a data part runs its skill, text finds a
weapon, a 0.3 caller is answered in 0.3's shape, and tasks, streaming and an
unknown version are refused with A2A's codes.

## `check_account_names`

The account page's names and agents, and the name an account goes by, against an account API faked in the
page: a born `user_` name reads as not chosen, the Profile form saves both
names and the top bar shows the display name at once, a refusal is said in the
card, and inside the day after a change the username field is shut while the
display name still saves; the agents acting for the account are listed, and one
click disconnects one. Signing up asks for one yes — the privacy policy and the
account kept outside mainland China — and neither the email form nor a
provider's button goes anywhere until it is ticked.

## `check_sync_client`

Two browsers of one account end on the same entries, against a server faked in
the page with the real one's rules: the first sync is a union, a name clash is
settled the same way on both, an edit and a deletion reach the other browser,
the measured result never travels, another account's entries wait to be asked
for, an account without the feature pushes nothing, and a remote edit to the
build on screen reaches the screen. The cloud on a chip keeps an entry on one
browser and tells the others to keep theirs; with "upload new items" off a new
entry stays here; the allowance the server states is kept to, and an item the
server refuses stays here. A riven's row in its list carries the cloud too, and
a click switches it without opening the riven. Signed out, the cloud is a link to
sign in that comes back to the page and picks no entry; with no way to sign
in, no chip carries one.

## `check_desktop_idle_restart`

The page made to look like the client, its updater saying an update is ready:
not applied while the window was touched in the last ten minutes, nor while a
row is mid-measure; applied by itself once idle with nothing running.

## `check_desktop_shell_notice`

The page made to look like the client, with the build it states varied: a
shell older than `SHELL_MINIMUM`, and one too old to state its build, are each
offered the download in the corner and on /download; the current shell is
offered nothing and /download says there is nothing to install; a notice
closed stays closed.

## `check_share_entries`

A share beside a build is the one panel, for that build: the build bar's
button, an opened finder row, the simulator's result and a search's finalist
each open it in view and are told apart as `share.entry`. A finder row is made
the open build first; a finalist is not — opening a build resets the search —
so its panel sits under its row, its link decodes to that finalist's mods,
nothing is saved, the search's results stay, and the reader's result and the
card, both drawn from the open build, are not offered.

## `check_share_card`

Every builder step has a card block, the card's blocks come in the order the
page shows the steps, and moving a step on the page moves it on the card. The
backdrop word is the one fewest weapons share (PLASMOR, NIKANA, LATO, BRAMMA);
the card is drawn 1080 wide at twice the pixels, and the share panel draws it.

## `check_sync_customs`

A custom travels with what names it, against the sync server faked as
`check_sync_client` fakes it: a synced build brings the riven it names, even one
kept on this browser, and the riven reaches the account first; past the customs
allowance the build goes and the riven stays, named. A browser without the
riven shows its slot as absent, saves the build WITH it, plans no Forma
without it, and seats it when it arrives; a fight whose custom target is absent
stands a unit in and saves the target. Taking a riven off says who uses it.

## `check_saves`

Saved items as one file: the export carries every collection and no board
row, sync choice or measured result; an empty browser takes it back as it was;
a second read changes nothing; an item changed here is kept and the file's is
added beside it, with a copied riven's builds repointed at the copy; a file
that is not an export is refused out loud; both controls are in the topbar's
overflow.

## `check_weapon_search`

The topbar search is one box: no filter row or sort above the list, a weapon's
kind found by its word in the page's language and in English (a Kitgun chamber
by its class too), a name as before; on a phone the results take the screen's
width, the floating button steps aside while they are open, every row's kind
is shown translated wherever a translation exists, and a close button over them
shuts the list, empties the box and puts the keyboard away. An empty box opens
nothing, and a burst of typing draws the list once, when it pauses.

## `check_cloud_page`

`/account/sync` against an account and a sync server faked in the page: every
item from every collection and weapon in one list, newest first; a build opens
on its weapon by id; an item switches off and on; the status filter, the search
and the by-weapon grouping; several switch at once, and switching several on
stops at the allowance the server states, out loud; nothing on the page deletes.

## `check_ability_casting`

The fight offers a **Cast them** box, off by default — which is what every board
row was measured under — and ticking it sets the fight's own field, reaches the
request through `theFight()`, and unticking clears the key rather than sending
`false`. The box is re-found after each toggle: a re-render replaces the node,
and clicking the old one clicks something no longer on the page.

## `check_exalted_strength`

Valkyr Talons' damage is Hysteria's at 100% strength, so the claws read 250 in a
bare Valkyr's hands and 325 with Intensify seated on the linked build — the
+30% the build resolved, through the link the weapon holds rather than a number
typed into the fight — while an ordinary melee weapon in the same hands is
untouched.

## `check_companion_mods`

The companion builder draws ten general slots with four innate Penjaga
polarities, offers the universal pool and no model's own card, says plainly that
none of it pays, and seating one through the picker spends capacity and is born
as a build; a rank down costs a point less.

## `check_companion_host`

A robotic weapon is held by the companion host and sends no wielder; its picker
offers the hosts and never a Warframe, and its block frames the host's own page,
which frames nothing further. A build of the host is what the weapon holds, and
an ordinary weapon still holds a Warframe. The floor is the Sentinels' and the
MOA's lowest (367 / 130 / 80).

## `check_embed_chrome`

A page framed by a weapon's Wielder block — the Warframe page, and the Operator
page framed inside it — shows none of the outer page's furniture: no top bar,
footer, jump menu or Nona launcher, so the screen holds one launcher, not one
per frame. Verified to bite: against the build before the fix it counts three.

## `check_wielder_pane`

A weapon's Wielder block frames the Warframe page (`?embed`, the shell's chrome
off) with the Operator page framed inside it. An edit made through the frame's
own code is born as `preset 1`, becomes the wielder and reaches the request as
the resolved shards, without reloading the frame; edited back to the blank it is
deleted and the link is the default, and the next effective edit writes one
again. A fresh reader starts on the unbuilt Prototype with nothing stored, and a
weapon that never chose a preset follows the first one once it is written.

## `check_link_direction`

Two weapons link two presets of one Warframe: that frame's page marks each with
who links it, choosing one there moves neither link, and deleting a linked
preset says who is affected, takes a second click, and lands its links on the
default (sent as no wielder for the Prototype) while the other weapon's link is
untouched.

## `check_operator_link`

A Warframe page links its Operator like a weapon links a wielder: a type control
and a preset control, the linked preset pays and is marked on the Operator's own
page with who links it, and deleting it lands the link on the read-only default
(no Operator) and not on the first preset left.

## `check_sim_wielder`

The Simulator's build card carries the Warframe with its Operator nested under
it, read-only, each with a link to the page that edits it (`?build=` names the
preset); a build written for the frame shows up there from the same stored
preset. It edits nothing.

## `check_every_rank`

A card on the every-rank list is offered once per rank in the builder's picker
and in the quick calc's candidates; picking a lower rank seats the card at that
rank, the build leaves as `<card>@<rank>` and comes back as the same slot; a card
taken off the list is offered at max only. Verified to bite: a `lowerRanks` that
returns nothing fails five of its eight.

## `check_share`

It opens a share link in a browser that has never seen the
build and asserts what is on SCREEN, not what is in the variables.

## `check_tenno`

The fight's PLAYER reaches the panel, the sim and a share
link, so an arcane that scales off a Warframe is worth nothing with no frame
and +500% with one. The frame is picked in the build bar's Wielder control and
read off the floor the server resolves; the overrides and the extra stats are
the scenario's. Runs in CI beside `check_parity`.

## `check_squad`

A squad AURA and an ARCHON SHARD ride on the fight's `Tenno`.
Every assertion is on the wire or on a real `/api/simulate`. Its damage
assertion needs a fight where ARMOUR is the binding constraint: at the default
level an unmodded rifle never gets a target off its shields, the armour term
is never read, and the two runs come back byte-identical. It measures kill
PROGRESS, because dps is what the weapon puts out and armour decides what
arrives.

## `check_storage`

How much room the app takes on the reader's machine. It
measures the RATIO rather than asserting a constant, fills the disk from OTHER
weapons' keys to prove the shed sweeps the origin, and plants a replay written
under the old rule to prove the boot takes it back. Its second assertion keeps
the fix honest: the panel must STILL DRAW a replay.

## `check_one_fight`

Holds no list of fields: it asserts every module's
outgoing request against `theFight()` ITSELF, so a field invented tomorrow is
covered by nobody.

## `check_scan_progress`

A scan says how far along it is where the work is
being READ, mounted in all five places a scan ranks something. AN AXIS ONLY
SHOWS ITS OWN, since two lists can be open at once. It draws NOTHING when
nothing runs and the check asserts the ABSENCE as well as the presence. Its
evolution half needs a CROWD: a one-body Torid ranks its dozen evolutions
faster than the 250 ms repaint throttle, so nothing is ever drawn.

## `check_board_dedup`

A build the board already holds is not sent to it again,
and the page asks the ENGINE which (`/api/build/keys` → `board::builds::board_key`).
A build is not its spelling: `canonical_mods` sorts the non-elementals by
drain and leaves the elementals in the order that PAIRS them, evolutions are a
set, a riven is a shape and not its rolls, and the mod POOL is what tells an
elemental mod from any other — which only the engine has. A MATCH IS PROOF AND
AN ABSENCE IS NOT: the board LISTS only builds scoring at least half their
weapon's leading row, so the page only ever suppresses an upload it can prove
is redundant. Its NEGATIVE CONTROL is the half that matters — a build the
board does not hold must still be offered.

## `check_missing_asset`

**A HASHED FILE THAT IS GONE IS A 404, AND NOT THE APP.** `not_found_handling:
single-page-application` answers every unmatched path with index.html and a
200, which is right for a route and catastrophic for a content-addressed file.
The check drives the worker with an ASSETS stub that behaves as the platform
does — the shell, 200, `text/html`, for anything with no file behind it — and
asserts a miss under `/asset/` or `/pkg/` comes back a 404 whose body is not the
shell. Status alone is not the assertion: what must never happen is the app's
own html arriving under a script's name.

**THE NEGATIVE CONTROL IS EVERY ROUTE.** `/weapons/Torid`, `/benchmark`,
`/account` and `/` are all paths with no file behind them, so a worker that 404s
those has taken down every deep link on the site — a bigger outage shipped as
the fix. It also reads `build_site_app.py` for the other half, since keeping one
previous generation only shows up across two builds: a `rmtree` on either
directory is the bug, and the check names it. Verified to bite by restoring the
fall-through and by restoring the wholesale clear, one per half.

## `check_support`

`/support` states what WFSim holds in numbers it COUNTED. A drawn figure and
a counted one look identical, so each is
compared against the source it claims: the weapons tile against
`META.weapons`, the mods tile against the union of `META.mod_pools`, the built
line against the injected `PROJECT_FACTS`. The other half is the one line
about the READER — how much they have run here — asserted absent on a browser
that has run nothing, correct after a real run at a run count the check chose,
and absent from the request that run sent. It FORCES English rather than
inheriting it, since the app boots into the browser's language.

## `check_forma_plan`

**A COLOUR CANNOT BE PUT IN A DRAWER.** The weapon has nine slots and every
innate polarity sits on one of them, so a colour no mod wants lands on a
mod-less slot (free) or on a modded one (+25%) — unless a Forma spent elsewhere
overwrites it, which each one bought does for nothing, because the bill is
`max(added, removed)`.

Ballistica Prime is the shape that exposes it: four colours over nine slots, so
a nine-mod build has no mod-less slot to park anything on. The check walks every
all-Madurai window of its pool and asserts the two ways the plan had it wrong —
a red slot that could have been shed for FREE (25% of that mod paid for
nothing), and a plan that measured the drain it would have had if the colour
vanished, declared a fit and left the panel printing 64 / 60.

It also pins the two things the reader sees: the bill is a STATE and not a
history, so a polarity off and back on lands on the number it started from; and
the page's bill is the engine's, because the plan is MIRRORED in JS rather than
asked for.

## `check_forma_group`

**BUILDS OF ONE ITEM SHARE ITS POLARITIES.** Two saved Torid builds planned
together come out wearing one main layout, each with exactly its own mods, and
the open build keeps its positions. The rules are global: a Catalyst switched
off halves the capacity line, a Forma limit refuses in Chinese, and a Warframe
plans through the same box with a capacity line that agrees with the plan.
With mods kept in place neither build's mods move. Planning ahead over the
Torid's whole board draws a curve where each point costs more and reaches
further, marks the first point on the line, saves a pick already wearing the
point's layout with every card of its row, and places the ticked builds onto
the point's exact positions. HOW MANY points that curve has is the board's and
not the code's — it stops at the ceiling its groups reach, which the scoring bot
moves — so that a curve has depth at all is `coverage_climbs_with_forma`, on a
fixture. `WFSIM_BASE` points it at a dev server.

## `check_rescore_paths`

Only the clock and a person may start a board run. Plain node, no browser: it
reads `.github/workflows/scores.yml` and refuses a `push` trigger, requires
either the `schedule` or a declared hold, refuses any path list, requires the
`--shard i/N` denominator to name the same output the matrix is built from,
refuses an `if:` that gates a step on an output set BELOW it, keeps the pass
that PUBLISHES from also being one that scores, and requires every scoring call
to bound its backlog and the clock behind it — `--new-limit` for builds with no
score at all (a count, since they have no cost), and `--deadline` to make that
count a promise.

Each of those spins the wheel without turning it while every run stays green. A
pushed run duplicates the one a person is about to start and is cancelled by the
next push; a path list is a second answer to a question the database already
answers; a bound naming a count without a clock is a bound that means nothing,
because rows differ 79x in cost.

## `check_submission_hop`

A submission survives the whole hop, and every hop in it is the real one: the
page's own `boardPayload()` in headless Chrome, then `worker/index.js` against a
stub database, then the inbox row it stores, then the `wfsim-intake` binary, then the
build that would land in the library. Only the network and the database are stubbed —
nothing about what a build IS is.

It asserts that the door stores the record VERBATIM and under a random id, that
a riven travels as a shape and never as rolls, that intake derives the id from
the build, that no `mode` survives into the library, and that the PAIRING the
player built is the pairing that is stored while the ORDER is the canonical one.

Each end has its own check; this one looks at what comes out the far end, which
no per-end check sees.

## `check_comment_style`

No attribution and no dated decision survives in the
repo's prose, and the narrative phrases that mark a history being retold are
ratcheted: the count may fall and never rise. `docs/MEASUREMENTS.md` is exempt.

Two more numbers hold the SHAPE of what is left, and they are read differently.
**Blocks over twenty lines** are the RATCHET — past twenty a block has stopped
stating a rule and started explaining a subject, which is what `docs/` is for,
and a subject explained twice is two explanations that drift. The count may fall
and never rise.

**Comments per line** is the backstop the essay count needs, since splitting one
essay into two compliant blocks would otherwise pass; splitting produces no new
comment line. It is a RATIO because comments growing with the code is healthy
and comments growing faster than the code is not — an absolute count cannot tell
those apart, and taxes every new module for the size of the old ones. It is a
LIMIT rather than a ratchet: 0.3, against the 0.269 the repo has sat at since
the prose pass, so ordinary work never reaches it and only a real turn
commentward does. A whole-repo average cannot honestly do more than that.

`.md`, `.html` and `.css` are outside both, which is what makes "move the
subject into `docs/`" an answer rather than a shuffle.

## `check_commit_trailers`

No commit in the pushed range credits an AI tool: no `Co-authored-by:` naming
one, no "generated with" line, no robot marker. The rule belongs to the repo,
not to any one agent's settings — each tool has its own switch and most default
to adding the line, so only a check every tool's commits meet holds for all of
them. CI reads the range a push or pull request adds; locally it reads
`origin/main..HEAD`. `--strip <file>` is the same test as a commit-msg hook
(`.githooks/commit-msg`, `git config core.hooksPath .githooks`), which deletes
the lines before the commit exists. A human `Co-authored-by:` passes.

## `check_usage_device`

**A DEVICE IS ITS TRAITS.** The page reads as `mouse_wide_browser` on the
desktop profile, and as `touch_narrow_wechat` once CDP emulates touch, a phone's
width and WeChat's user agent; its cores go out as one of the steps. **COMPUTING
OUT OF SIGHT IS JUDGED BY WHAT HAPPENED**: with `document.hidden` stubbed and the
absence aged by hand, `judgeBackground` says `kept`, `stopped` and `idle` for a
finished task, no beat and beats without a task, `frozen` after a `freeze`
event, and nothing for three minutes.

## `check_page_titles`

**A TITLE SAYS WHAT THE PAGE IS, IN BOTH LANGUAGES.** Every page a search finds
or a reader pastes is served a title with Chinese in it and the brand, an
`og:title` equal to it, and exactly one `<h1>` (none on /compute and
/contributors, which draw theirs). A weapon's carries its Chinese name. In the
browser, /weapons/Torid keeps the title it was served, and a page reached inside
the app — the simulator tab, /compute, and Torid again — takes
`<subject> — <what> | WFSim` instead. docs/UI.md §Page titles.

---

## A check cleans up after itself

**A CHECK CLEANS UP AFTER ITSELF.** Each `openApp` runs Chrome in its own
throwaway profile under `%TEMP%`. `finish` kills the whole process tree
(`taskkill /T` on win32), waits for it, and retries the removal; on Windows
`kill()` reaches only the node that was spawned and Chrome's children hold the
directory. `sweepStaleProfiles` deletes any `wfsim-*` older than an hour ON

THE WAY IN, which is the only cleanup a run that throws, is interrupted, or
never calls `finish()` can get.

## UI verification over CDP

UI verification: drive headless Chrome over CDP (Node ≥22 has a global
WebSocket; Chrome is at the default install path). Assert real DOM state;
screenshots for layout review. `scripts/cdp.mjs` is the shared harness — a
static server for `site/`, the Chrome launch, `evaluate`, `check`, `finish`.
A check's page-side body is a TEMPLATE LITERAL: an unescaped backtick in it,
including in a comment, ends the literal early.
