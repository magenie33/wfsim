# The agent door

**Status: phase 0 implemented** — `window.wfsim` in `web/src/static/app.js`,
asserted by `scripts/check_agent_door.mjs`. It covers the loop that answers a
question: open a weapon, change the build, change the fight, run it, read the
number. The build is reachable axis by axis — mods and their ranks, slot
polarities and the Forma plan, arcanes, evolutions, mode, valence — and the
observation carries capacity, Forma and the active presets. Queries read the
stats panel, the last run and the leaderboard, and find weapons, mods, arcanes
and targets.

Everything that drives the page from outside goes through one door:

```js
window.wfsim.observe()          // where the reader is, what is built, what was measured
window.wfsim.do(id, args)       // one action, by id
window.wfsim.tools()            // the table as tool definitions a model can be handed
window.wfsim.actions            // the table as ids, descriptions and anchors
```

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

**The id is the wire.** `<module>.<subject>.<verb>`, named after the domain and
never after the widget — the same namespace `docs/ANALYTICS.md` fixes for event
names, for the same reason. An agent that learned an id cannot be migrated when
it changes, so an id that ships is frozen under the rule `naming::FROZEN`
states for every other wire.

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
progress while it runs and the ranking once it is done; `optimizer.search.stop`
keeps what was ranked. Its SCOPE is not on the door yet — a search runs the
scope the reader set.

**Anything that leaves the browser.** Nothing here shares a build, submits to
the board or opens a link. Those are outward actions and they want a reader's
hand on them.

## What an in-page agent is bound by

Four constraints, stated before the panel exists because each of them is
cheapest to honour at the start and each is the kind a hurried implementation
walks around.

**It works on a build of its own.** An agent never edits in place the build the
reader has open — it branches, the way "+ new" already does. A player who
watches an experiment happen to the thing they spent an hour on has been robbed
whatever the result was.

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
