# The agent door

**Status: phase 0 implemented** — `window.wfsim` in `web/src/static/app/` (`88-door.js`, `90-door-actions.js`),
asserted by `scripts/check_agent_door.mjs`. It covers the loop that answers a
question: open a weapon, change the build, change the fight, run it, read the
number. Every control a reader can use on a weapon page is on the door or
named in `AGENT_EXEMPT` with its reason (`check_agent_coverage`), and the
`todo` kind — the door's backlog — is empty: the build axis by axis, the Forma
planner, rivens, the fight's own editors and its arena, custom targets, the
search's scope and run, every preset bar and its undo. Queries read the stats
panel, the last run and the leaderboard, and find what the page can pick.

Everything that drives the page from outside goes through one door:

```js
window.wfsim.observe()          // where the reader is, what is built, what was measured
window.wfsim.do(id, args)       // one action, by id
window.wfsim.tools()            // the table as tool definitions a model can be handed
window.wfsim.actions            // the table as ids, descriptions and anchors
window.wfsim.skills             // the table by module: what each is for, and its actions
window.wfsim.ui                 // the page's tr, dropdown and escaping, for a panel on the page
```

The door answers from the moment `app.js` has run, but only a booted page has
anything to answer about: `observe().ready` says whether it has booted, and the
page fires `wfsim:ready` on `window` when it does.

## Who is on the other side

**A PLAYER IS WATCHING THE SCREEN.** That is the whole reason this surface is
shaped the way it is, and it is the opposite of the surface an automation
client wants. A client driving a browser headlessly wants the UI out of the
way: bypass the controls, batch everything, never repaint. Every rule below
would be dead weight to it, and a door built for it would be a second UI with
no pixels — the page would sit unchanged while its state moved underneath, and
the reader would have nothing to take over.

So the door is not an automation API, and the test for anything proposed for it
is not "could a program use this" but **"is a person watching this happen"**. A
consumer with no reader in front of it — a bot, a command line, another service
— gets the QUERY half and not the actions, because a query has no reader to
surprise. `docs/CORE.md` names the three modules; this names who is at the
keyboard while they run.

## The rules, and what each one is protecting

**AN AGENT INHERITS THE READER'S AUTHORITY, NOT THE READER'S BANDWIDTH.** The
two halves of the door follow from that one sentence and they are deliberately
asymmetric. A change to state goes the way a reader's own hand goes, exactly.
A READ does not: a person pages through a list because a screen is small and
attention is serial, and neither is true of the caller. Making it scroll is
copying a constraint of the body onto something that does not have one — ten
times the tokens to arrive at the same fact, less reliably.

**A query is a faster ROUTE to a fact, never a second DEFINITION of it.** This
is the rule that makes the paragraph above safe, and it is `AGENTS.md`'s "every
fact has one authoritative source" applied here. Reading the board's published
rows directly instead of a page at a time is the same source reached faster.
Re-deriving which rows lead, or precomputing a table that is easier to query,
is a second answer to a question that already has one — and the day the two
disagree, one of them is on the reader's screen and the other is in a chat
window. A selection the page performs is exposed AS a query rather than
reimplemented beside one.

**A query lives in the same table**, marked `query: true`: one list is what a
model is handed, so a query added to the page reaches every agent with no edit
anywhere else. It changes nothing — the check asserts the observation is
identical on both sides of each — and it reports no `changed`. The stats read
asks the panel endpoint the page asks; the finders use the picker's own
`searchHit`, so a name in any locale the page speaks finds the same row.

**What is not in the observation is not known.** A reader sees things this
surface does not carry — a tooltip, a colour, the arena. An agent asked about
one of those will otherwise infer it, fluently and wrongly, so the answer is
that it cannot see it. Anything a reader can learn from the page is eventually
reachable through `observe()`; until it is, it is missing rather than guessed.

**An action is something a reader can do, and nothing else.** Every entry
names the control it stands for (`anchor`) and the check asserts that control
is on the page. This is what keeps an agent's work VISIBLE — the action moves
the state the click moves, and the page redraws for both — and it is what stops
the table growing an entrance no reader has, which is a second UI with no
pixels.

...and it is ONE MOVE, at the grain a reader would call one: something they
could undo with one obvious gesture, and one line in a trail of what happened.
Seating a mod is a move; seating the eight that make up a build is also ONE
move, not eight. Finer than that and the agent performs a sequence of clicks
nobody can follow or wants to wait through; coarser and it stops being
something a reader could have done, which is the rule above.

**The DOM handler calls the action.** Not the other way round, and never both.
A control that keeps its own copy of the decision and an agent that calls the
door are two implementations of one behaviour, and they agree until the day one
is edited. `check_agent_door` asserts the shared decisions exist once, on the
SOURCE, because a running page cannot see its own duplication.

**Every action declares what it writes** — the document of one bar (build,
scenario, search, riven, target), the linked Operator build, this browser's
preferences, the bar its `bar` argument names, or nothing — and a query writes
nothing. Nothing copies an Operator build, so only a hand action writes one:
an agent may have the fight PERFORM an Operator node
(`simulator.node.simulate`), and whether it is assumed up stays the
reader's tick. The official ruler's
lock and an agent's copy-before-write both read the declaration, so they
cannot disagree about what counts as an edit.

**A hand action is a reader's gesture.** Marked `hand`, it is called by the
page's own control with `{ hand: true }` and refused without it, and it is not
in `tools()`: taking an agent's copy back (`shell.preset.adopt`) is the reader's
click, whatever the model decides. An agent has no route to it — not a
different route, none.

**The id is the wire.** `<module>.<subject>.<verb>`, named after the domain and
never after the widget — the same namespace `docs/ANALYTICS.md` fixes for event
names, for the same reason. An agent that learned an id cannot be migrated when
it changes, so an id that ships is frozen under the rule `naming::FROZEN`
states for every other wire.

**The observation says what is open** — the document in each bar that an
edit would write, whether the fight is an official ruler, and the page's
language — so a caller never reads the page's variables to find out.

**The observation is bounded.** An agent pays for every byte in the window it
has to think in, so `observe()` is state and not a DOM dump: scalars travel as
themselves and everything else — the formation, the buff map — reports its
size. That rule is DERIVED rather than listed, so a field added to a scenario
is observed without an edit here.

**A refusal is an answer.** A wrong id, a slot that does not exist, a fight
field that has its own editor: each comes back as `ok:false` with a machine
readable `reason` and, where the question has near answers, the ones that would
have worked. An exception says only that it failed, which is the one thing the
caller already knows.

**Every call reports what changed.** `do()` diffs the observation across the
action, so a caller never needs a screenshot to find out what it did.

**A number says whether it is still true.** `observe().result.fresh` is stated
rather than left to be inferred from a timestamp: a measurement attached to a
build that never produced it is the one lie this surface cannot tell.

## What is deliberately absent

**A search that blocks.** A search is minutes long, so `optimizer.search.start`
returns at once and the caller polls `optimizer.search.read`, which reports
progress while it runs and the ranking once it is done — each row with the
starts that settled on it or the answer it is near — and
`optimizer.search.stop` keeps what was ranked. What a search may use and how
many builds it answers with are set through `optimizer.plan.*`.

**Anything that leaves the browser.** Nothing here shares a build, submits to
the board or opens a link. Those are outward actions and they want a reader's
hand on them.

## What an in-page agent is bound by

Four constraints, each cheapest to honour at the start and each the kind a
hurried implementation walks around. The in-page agent is Nona (九九), in
`app.js` §NONA: bring-your-own-key, OpenAI-compatible or Anthropic, calling the
provider from the browser and running every tool against the local engine.

**It works on a build of its own.** An agent never edits in place the build the
reader has open — it branches, the way "+ new" already does. A player who
watches an experiment happen to the thing they spent an hour on has been robbed
whatever the result was. The branch is made by the page (`nonaBranch`) before
her first change, not asked of the model: a promise kept by instruction alone is
kept only as often as the model obeys.

**A new control is taught by adding its row.** A control with no door row is a
feature she silently cannot use, so `check_agent_coverage` fails on any control
outside every action's `anchor` and outside `AGENT_EXEMPT` — which names the
kind (outward, view, pref, reader, todo) and the reason for each. An anchor is a
selector list, so one action may stand for several controls.

**Her tools are the table.** She is handed `tools()` at each request and her
prompt carries rules, never game data, so an action, a query or a weapon added
to the page reaches her with no edit to her section.

**The trail is derived, not written.** What happened is the sequence of
`changed` sections the calls already return, so it cannot describe a move that
did not land or miss one that did.

**Stopping is not a rollback.** Every action leaves a legal page state, so
"stop" is only "send no more" — there is no half-applied move to unwind. This
is a property the same-path rule hands over for free, and it is lost the moment
an action reaches inside the page's own state.

**Outward actions keep a reader's hand on them.** Sharing, submitting to the
board, anything that leaves the browser: permanently a person's gesture, not a
row in the table. This is not a phase-0 limit to be relaxed later.

## Measuring her

`check_nona` proves the machinery against a scripted stand-in and runs in CI.
Her BEHAVIOUR is measured against a real model by `scripts/nona_eval.mjs`, run
by hand with a key (`NONA_BASE`, `NONA_KEY`, `NONA_MODEL`): each case asks a
reader's question on a fresh page and grades what she did — the tools called,
whether she worked on a copy, whether the reader's build was left alone, and
how many numbers in her answer no tool returned (the target is none). Run it
after changing her prompt, her tools or the door, and compare the totals with
the last run; the records land in `private/nona-eval/`.

## The consumers

The checks drive the page through raw clicks, which is why each of them carries
its own vocabulary of the page and breaks when a control moves. The door is
what they are migrated onto, one check at a time, as each is next touched.

The same table is what an in-page agent is handed: `tools()` is already the
tool-definition shape, so the page can run a model the reader brought their own
key for, execute its calls locally against the wasm engine, and never send a
key or a build anywhere.

A consumer with no page in front of it — a bot in a chat window, a command
line, another service — takes the query half under one id namespace with these,
and takes no actions at all. Nothing there is watching a screen, so an action
would have no reader to be visible to and no hand to take it over: what such a
caller wants from a build it cannot see is a LINK to the page holding it.

## Headless queries

**A QUERY IS IMPLEMENTED ONCE, WITH NO PAGE UNDER IT.** Every surface that
answers without a page — the MCP server, the chat bots, the skill and `llms.txt`
— and the page's own door reach one table of headless queries, and none of them
carries a second copy of what a query does. WFSim changes every week; a surface
that keeps its own copy is the one that answers with last month's numbers.

| layer | holds | changes when |
| --- | --- | --- |
| `87-headless.js` | the table: each query's id, description, argument schema and body | a capability is added |
| `webapi` | the engine work a query asks for, as a route | the engine changes |
| page door | the actions, and each query's call with the build on screen | a control is added (`check_agent_coverage`) |
| Nona | her persona, conversation and memory; tools from `tools()` | never for a new query |
| `mcp/index.js` | the MCP server at `mcp.wfsim.app/mcp`, a worker of its own | the protocol changes |
| Discord / QQ | how a result reads as a chat message | never for a new query |
| skill, `llms.txt`, api catalog | written from the table by `build_site_app.py` | never by hand |

The rules the table keeps:

- **STATELESS.** A headless query takes the weapon or the build as an argument
  — the build as the wire `buildPayload()` sends, whose axes `BUILD_AXES`
  declares. On the door an omitted one is the one on screen (`screen_weapon`,
  `screen_build`); a door query that reads page state a headless caller cannot
  send is a page query, and stays on the door alone.
- **ONE PART, TWO HOSTS.** `87-headless.js` touches no DOM and no page global:
  what a query reads arrives on `host` — `HEADLESS_PAGE_HOST` on the page, the
  worker's own elsewhere, which takes the part by generated copy the way
  `worker/share_codec.js` takes `29-share-codec.js`. Engine work is a `webapi`
  route reached through `host.api`, the same wasm `api()` in the page and in
  the worker, so it ships with every build. The page's own lists use the
  part's functions too (`rankBoard` orders the build bar), so a headless answer
  and the page's are one computation, not two that agree today.
- **NO ACTIONS.** A headless caller has no reader watching (§"Who is on the
  other side"); what it wants from a build it cannot see is a link to the page.
- **WFSIM DECIDES ITS NAMES; A CALLER ADAPTS.** A query may be renamed or
  retired whenever WFSim moves on. What is owed to a caller is a readable
  answer, not a frozen name: a retired id stays in `HEADLESS_RETIRED` with the
  query that replaces it, or none, and a call to it is refused with that
  pointer rather than with "no such tool". The row is removed when WFSim
  chooses; nothing obliges it to stay.
- **THE TWO HALVES AGREE.** `check_agent_door` reads a board row's panel from
  the row opened on screen and from its `build` sent as an argument, and fails
  on any difference; `check_mcp_tools` runs every query in the worker's copy
  with no page at all.

## WebMCP

`91-webmcp.js` hands the door's table to the browser's own agent through
`document.modelContext.registerTool` (`navigator.modelContext` on older Chrome):
every tool `window.wfsim.tools()` lists, under the MCP spelling, run through
`agentDo`. It registers as the script loads, when a browser looks, and again
once the page has booted, because an argument's choices come from `META`; the
first set is taken back by its signal. A call made before the boot waits for it.

## The A2A agent

`mcp/a2a.js`, in the MCP server's worker at `https://mcp.wfsim.app/a2a`:
JSON-RPC, A2A 1.0 with 0.3's `message/send` read too. **A SKILL IS A HEADLESS
QUERY AND NOTHING ELSE**, so a skill the card names is one a caller can run.
`SendMessage` is answered with a message at once and no task is kept; there is
no model behind it — a data part `{skill, args}` names the query, and plain
text is read as a weapon to find. The same optional key and the same allowance
as an MCP tool call. The card is `agentCard`, served at the endpoint's host and
written to the site's `/.well-known/agent-card.json` by the build, versioned by
the engine it runs.

## The MCP server

`mcp/` is a worker of its own at `https://mcp.wfsim.app/mcp`: Streamable HTTP,
no sessions, the headless table's queries as read-only tools. It
bundles the engine rather than sharing the site's worker, because a site worker
carrying six megabytes of wasm would start every page load colder.

- `mcp/headless.js` is `87-headless.js`, copied by `gen_worker_parts.mjs`.
- `mcp/engine.js` is written by `build_site_app.py`: the page's bindgen glue as
  a module, importing the wasm from `site/pkg/` so no second binary is kept. A
  worker cannot compile wasm from bytes at run time; an imported one is
  compiled at upload. The engine starts on the first call that needs it.
- The board comes from the site's worker over the `SITE` service binding, so it
  is as current as the site; everything else is the bundled engine's.
- A key is optional. A tool call without one spends the address's allowance,
  with one the key's (`ADDRESS_LIMIT`, `KEY_LIMIT` in `mcp/wrangler.jsonc`, stated
  as `MCP_LIMITS` in `worker/agents.js`); the handshake and the list spend
  nothing. The key is asked about over `SITE` once a minute per isolate, and a
  key that is not one is a 401 rather than read as none.
- Two tools are the MCP server's own, not the headless table's, because a page
  holds its builds in its own storage: `account_builds_list` and
  `account_builds_save` read and write a claimed key's person's synced builds
  through `/api/cloud/sync`. They translate with `headlessSeat` and
  `headlessStateAxes`, the one translation between a saved build and the wire,
  which the page's `seatPayload` and `stateFromBuild` also call; a save the
  engine's `/api/panel` cannot read is refused. Each hands back
  `/weapons/<page>?build=<id>`, which opens that build, after the sync when the
  browser does not hold it yet (`buildWanted`).
- `ship.py` deploys it after the site, and `--verify` asks it which engine it
  runs against the digest `site/pkg/` serves. It lags silently otherwise.

## The QQ bot

**QQ TAKES CALLS ONLY FROM A WHITELISTED ADDRESS**, so the bot is two halves.
The door in is the site's worker (`worker/qq.js`): QQ calls back to
`/api/qq`, the address check (op 13) is signed there, every event's Ed25519
signature is checked against the AppSecret-derived key, and the messages the
bot answers are kept in `bot_inbox` (`worker/schema.sql`). Nothing is sent from
the worker. The bot server — the one address on QQ's whitelist — pulls those
rows over `/api/qq/claim` (bearer `BOT_RELAY_TOKEN`), answers each from the
headless table, and replies through QQ's API; a row is handed to one pull at a
time, again if it was never marked done, and deleted a day after it arrived.

A GROUP CAN GIVE THE BOT EVERY MESSAGE (its owner, in mobile QQ: 机器人可获取的群聊
消息范围 → 获取群内全部消息; the console subscribes `GROUP_MESSAGE_CREATE`). Then an
@ arrives as `GROUP_MESSAGE_CREATE` too, and the door keeps only an @ of this
bot or a command typed without one (`qqAddressed`); the room's talk is never stored.

Secrets: the worker holds `QQ_APP_SECRET` and `BOT_RELAY_TOKEN`; the server
holds the AppID, the AppSecret (for its access token) and the same relay token.
`scripts/check_qq.mjs` holds the door to QQ's published signing example.

THE BOT SHIPS WITH THE SITE: `ship.py` copies `bot/`, `mcp/engine.js`, its wasm
and `mcp/headless.js` to the server, restarts it and checks the engine it holds
(`scripts/ship_bot.py`; the server and its key are `private/qq/bot.json`).

IT READS THE OWNER'S LIVE BOARD when the server keeps one (`WFSIM_LIVE_BOARD`,
docs/BOARD.md §"The live board"), and the published board otherwise. `ship_bot.py`
ships the live board's Linux binaries and scripts beside the bot.

IT RELAYS DE'S WORLD STATE for the Utility pages, which DE's host refuses to
the worker: each minute `bot/world.mjs` PUTs the file to `/api/world` with the
same relay token (docs/UI.md §"Utility").

HER FACE IS `web/src/static/nona.svg`, the one the site's Nona wears; the QQ
avatar is that file rendered to a PNG and uploaded on QQ's console by hand.

## Riven appraisal

A reader's OWN riven — what it GAINS a weapon, a riven gain (裂罅收益) on every
page and in every chat; "appraisal" stays in ids and the wire — is answered by
the optimizer, run by the community's computers or in the browser of whoever
opens the link — never by re-scoring known builds, which is the board's
question (docs/BOARD.md §"The Riven Analyst").

1. A chat's bot reads the card (`fx` in Chinese chats), checks it with the
   engine's own riven rules and opens an appraisal at `/api/appraise/new`
   (`worker/appraise.js`): weapon, ruler, the card's rolls, and the channel's
   own note of where to answer, which nothing else reads.
2. `/appraise/<code>` opens that weapon's optimizer under the official ruler
   with the appraisal's own BUILT-IN search and searches at once
   (`81-appraisal.js`): `riven_appraisal` in `data/search/presets.yaml` — four
   starts, the riven pinned beside each primary element's 60/60 card the weapon
   takes, one build answered, ten fights a candidate, read-only like every
   built-in. The search is told nothing else, and the
   tab's storage lives in memory, so the reader's own builds, rivens and
   optimizer are untouched. Every finalist goes to the board unasked; the winner
   goes back to the appraisal as a BUILD with an optional name to thank —
   never a score.
   THE SAME LINK WITH `?freeze` sets that search up and stops, writing
   `{ engine, request, context }` to `body[data-request]`: the optimize request
   the page would send and what turns a result into a build
   (`boardBuildContext`). The bot opens it right after the appraisal, in a
   browser context of its own with no storage (`bot/render.mjs`
   `freezeRequest`), and stores it once (`/api/appraise/<code>/request`).
3. ANY COMPUTER COMPUTING FOR THE BOARD runs it. Freezing it is an ASK of the
   fact factory (docs/BOARD.md §"The fact factory"; worker/appraise.js
   §"Volunteer work"): a chat's, someone waiting on it, due in two minutes and
   handed out before anything else; a SURVEY's — every riven shape of a weapon
   the owner opens in bulk, channel `survey`, kept a week — due in a day.
   `/api/board/work` hands it out on the served engine only, and its page runs
   the request through a search of its own (`quickFleet`, the reader's
   optimizer untouched) and sends the build under its lease with the search's
   work. Then it goes to a computer of another owner on another network; two
   such answers equal in build, score and work credit both, and the agreed build
   goes into the board's door as a reader's submission would (`submitRecord`).
   One frozen request asked twice — a chat asking for a shape a survey already
   agreed — is one question, answered at once. The search is deterministic for
   a frozen request, so honest computers agree to the bit. The chat is told the
   community is on it, and the link stays as the asker's own faster way. A
   computer still searching renews its lease every two minutes
   (`/api/appraise/<code>/renew`), so a slow one is never overtaken by its own
   lease, and stops when told the task went elsewhere. A chat's riven gain
   nobody has answered goes to a computer offering idle cores by tier
   (`RIVEN_LANE_TIERS`; the ask carries `lanes`) — eight at once, four after ten
   seconds, any after two minutes — since a search is many short steps in a
   row, so cores on one computer, not more computers, are what make it quick.
   Once one has taken it the chat is told so.
4. The channel's bot claims what came back (`/api/appraise/claim`), replays the
   build itself with the card's real rolls, and the first build it accepts
   wins once; later ones are kept on the board and not announced. It answers
   as a passive reply while QQ's window is open (`bot/qq.mjs`), past it as an
   active message, and only if that fails on the room's next message.

The page and the door are channel-blind: a Discord bot claims `channel:
"discord"` the way the QQ bot claims `"qq"`.

## Machine-readable

An agent that fetches the site rather than driving the page reads what
`build_site_app.py` `ship_agent_files` writes beside the html, and the worker
serves it:

| path | what it is |
| --- | --- |
| `/index.md`, `/weapons.md`, `/weapons/<Wiki_Name>.md` | a page's markdown twin, from the same values as its html |
| `/llms.txt` | llmstxt.org index: what WFSim is and every weapon's twin |
| `/.well-known/agent-skills/index.json` | one skill: look a weapon up, read the board, quote a score |
| `/.well-known/api-catalog` | RFC 9727: the MCP server and the board JSON, the two public read APIs |
| `/.well-known/mcp/server-card.json` | the MCP server: its endpoint and its tools, from the headless table and `mcp/account.js` |
| `/.well-known/agent-card.json` | the A2A agent card, from `agentCard` in `mcp/a2a.js` — the function the endpoint serves it with |
| `/.well-known/ai-catalog.json` | ARD's catalog (ai-catalog 1.0): the MCP server card, the A2A card and the skill, each by the url it is served at; `Agentmap:` in robots.txt and `<link rel="ai-catalog">` in the page's head point at it |
| `/auth.md` | how an agent registers, claims a key for its person and uses it — drawn by the worker (`worker/agents.js`) |
| `/.well-known/oauth-protected-resource`, `/.well-known/oauth-authorization-server` | RFC 9728 and RFC 8414, the second with auth.md's `agent_auth` block, from the same constants as `/auth.md` |
| `/robots.txt` | each AI crawler named, `Content-Signal` granting search, ai-input, ai-train |
| `/.well-known/http-message-signatures-directory` | the Web Bot Auth key directory (§"Web Bot Auth"), drawn by `worker/bot_auth.js` |

A page with a twin (`markdownTwin` in `worker/index.js`) answers
`Accept: text/markdown` with it when markdown ranks at least as high as html,
and carries a `Link` header naming the twin and the discovery documents either
way — which is why `/`, `/weapons` and `/weapons/*` run the worker first. An
unknown `/.well-known/` path is a 404, never the SPA.

A twin states only what the page does, so a fact reaches it through the same
function that writes the html, never a second one.

## Web Bot Auth

WFSim's own tooling fetches other sites — DE's public export, warframe.market,
the wiki — and signs those requests so a site can tell they are WFSim's (IETF
webbotauth: `draft-meunier-web-bot-auth-architecture`, and the directory draft).

| part | where |
| --- | --- |
| the key | one Ed25519 JWK: `private/web-bot-auth.jwk`, and the same key as the site worker's secret `WEB_BOT_AUTH_JWK` |
| the directory | `/.well-known/http-message-signatures-directory`: the public key only, and the response signed by it under `tag="http-message-signatures-directory"` |
| the requests | `bot_headers` in `scripts/bot_auth.py`: `Signature-Agent: "https://wfsim.app"`, and a signature over `@authority` and `signature-agent` under `tag="web-bot-auth"`, five minutes long |

The `keyid` is the key's RFC 7638 thumbprint. A checkout without the key file
sends its requests unsigned: a signature is an identity, and only the key's
holder has one. **A NEW REQUEST TO ANOTHER SITE GOES THROUGH `bot_headers`.**
Rotating the key is a new file and `wrangler secret put WEB_BOT_AUTH_JWK` from
it; a site that cached the old directory stops trusting the old key within its
five-minute cache. `check_bot_auth.mjs` holds the signer to RFC 8032's vectors
and both signatures to Node's verifier, offline.
