# The combat record — one ordered stream of what a fight did

The record is the only output of this app that can be laid beside a
recording and checked number for number. AGENTS.md carries the constraints;
this file carries the design and the measurements behind them.

## A fight pops numbers, they are events, and there is one list of them

**A FIGHT POPS NUMBERS, THEY ARE EVENTS, AND THERE IS ONE LIST OF THEM.**
Everything else a replay carries is a CURVE, and an aggregate cannot tell one
hit for 400,000 from twenty for 20,000. IT IS THE COMBAT RECORD, REPLAYED —
one stream, not two: two lists filled from one place still hold different
SETS, so a number could float over a body with no row to explain it. For a
panel whose claim is "this is what happened", ONE-TO-ONE IS THE CLAIM.

THE CAP IS A DISPLAY DECISION, made in `popsDraw`: twelve a frame, biggest
kept, the rest counted. DE caps its own display the same way ("a maximum of 10
tick numbers are shown at once"), and it is unavoidable besides — a dense
fight deals ~320,000 instances against 600 frames. The dropped count is on
screen.

THE TWO VIEWS MEET ON AN ID: a drawn number carries `data-rpevent` and a table
line `data-recevent`, both the event's own place in the stream.

ON THE PAGE it is a DOM overlay appended AFTER `mountArena`, never before —
the mount takes the host over and rewrites it. The analysis mount publishes
`host.__arena` and the overlay puts that viewBox through the svg's own fit:
ONE geometry.

IT IS FETCHED ON A GESTURE. Pressing play or scrubbing asks for the record; it
does NOT ride along with the result.

## The stream is the four things a fight does, plus a miss

**THE STREAM IS THE FOUR THINGS A FIGHT DOES, PLUS A MISS.** A shot, a
reload's two ends, a transmute's two ends, and the numbers the enemy takes.
There is no ARRIVAL row. Against a target at CONTACT, which is every official
ruler, it is one row per pellet reading "it arrived, 0.00 m" — 6,192 arrivals
against 12,404 damage rows on the board's leading Laetum, half the stream
saying nothing. Which numbers are one arrival is still answerable from the
shot they share.

THE MISS IS REAL: three exits in the pellet loop produce no damage at all —
outside the cone, out of the weapon's range, an explosion that reached nobody
— so "why did a three-pellet shot pop two numbers" has an answer. It is NOT a
per-pellet row: on every official ruler the target is at contact and it never
fires.

## Damage comes through one door, and the compiler holds it

**DAMAGE COMES THROUGH ONE DOOR, AND THE COMPILER HOLDS IT.**
`engine::dummy::ledger` owns the run's totals and the DPS curve in types whose
fields are PRIVATE TO IT, so the only thing in this crate that can move them
is `ledger::settle` — which books the number and writes the row that explains
it in the same call. A damage site that moves every curve on the page and
appears in no ledger does not compile.

THE RAW TRAVELS ON `Settled`: `apply` carries it back, so a site cannot book
one figure and settle another.

THE ARGUMENTS ARE A CLOSURE, which is what lets the per-site gate go: an
`Instance` is a dozen locals, three Vecs and a String, and a `TargetAt` re-runs
the armour scaling curve — +4.0% on `one_fight` if the 999 runs nobody reads
pay for it. `settle` takes `impl FnOnce() -> Instance` and calls it only when
the record is on.

THE GENERIC HALF IS TINY, measured: the whole body inside the closure-generic
function is stamped out per damage site and costs **+2.7%**; splitting the
cold half into a non-generic `write_row` puts it back. `Curve` is its own type
rather than a third field on `Meter` for the same reason: it is a 600-slot
array, `RunResult` is `Copy`, and grouping 4.8 KB with the two hot scalars
costs **2.4%**.
Verified to bite means the sabotage must fail to COMPILE: a tenth site writing
`r.meter.raw += 1.0` is rejected with "field `raw` of struct `Meter` is
private".

## A factor is a type, and the wire sends its index

**A FACTOR IS A TYPE, AND THE WIRE SENDS ITS INDEX.** `record::Factor` replaces
a `&'static str` written at each call site: nothing tied the word "critical" to
the 4.4 beside it, a typo was a new factor nobody would notice, and TWO
different things were both called "shield gate" (the 0.1 s window and the 5%
leak past a broken shield). The table is sent once and a row names its factors
by index; the weapon state and the two stack lists are omitted when unchanged
from the row before and filled forward on arrival. Measured: **859 → 481 bytes
an event, 17.2 → 9.6 MB, 1,811 → 546 ms** for 20,000 rows.
`Record::wants(t)` is the other half: a row's arguments cost a `TargetAt`
snapshot, three Vecs and a String, which on a dense build are built for the
whole fight and thrown away by `push`.
One page-side trap: `const F` declared beside the other helpers at the bottom
of `recordRow` while the factor lookups sit above it is a temporal dead zone,
and it throws from inside an async paint — which surfaces as the panel sitting
on "reading…" for ever with nothing in the console.

## A row’s state columns are the instance’s, and its labels are not its identity

**A ROW'S STATE COLUMNS ARE THE INSTANCE'S, AND ITS LABELS ARE NOT ITS

IDENTITY.** Every state column on a row belongs to that row: `set_stacks` is
called in the stage loop beside `DebuffState::amps`, the roster is built once
per run because `buff_roster` allocates, and it costs nothing measurable
because it only runs while a record is being taken. A state column that does
not match the number beside it is the panel telling a reader their own
arithmetic is wrong.

A LABEL IS TRANSLATED; A KEY IS NOT. Every factor chip carries `data-factor`,
every pool `data-pool`, every origin chip `data-origin`, all in the ENGINE's
own spelling, and the checks ask those: the DOM carries the identity, the text
carries the language.

THE RECORD IS A WINDOW THE PLAYHEAD SETS. The 20,000-event cap bites on
exactly the builds people argue about — the board's leading Laetum deals
~230,000 damage instances over 180 s — so scrubbing or playing past the end of
the window fetches the next one, the panel states the slice it is showing, and
how many did not fit is counted in the chip beside them.

## A measurement’s record is a query, not a payload

**A MEASUREMENT'S RECORD IS A QUERY, NOT A PAYLOAD.** `/api/log` is
deliberately not a field on `/api/simulate`: an ordinary fight deals
**2,000–5,000** damage instances over 180 s and the densest build measured
deals **408,817**, so a log that rode along would be free on most builds and
megabytes on exactly the ones a player is most likely to be arguing about.
Asking separately costs ONE re-run — about a millisecond single-target — and
keeps "a measurement costs its summary" intact. `/api/simulate` answers with
the median run's RNG state as two u32 halves, which is the handle that makes
the log the report's own fight.

## A long record is paged, and it can leave this window

**A LONG RECORD IS PAGED, AND IT CAN LEAVE THIS WINDOW.** The densest build
measured is 24,652 events; a table of that many rows is ~250,000 cells, laid
out again on every repaint of the result panel. THE FETCH AND THE VIEW ARE

PAGED SEPARATELY: the stream in memory is the entire fight — `Copy as text`
writes all of it and a floating number can name its row across a page boundary
— and only what is on SCREEN is bounded (`REC_PAGE`, 500). The pager is drawn
above the table AND below it.

IT OPENS IN A WINDOW OF ITS OWN: the parent keeps the state and calls the same
`recordBody`/`wireRecord` against the child's host, so there is ONE
implementation and the window is only where it is drawn. Every control is
found inside the HOST rather than in `document`, because the two are different
documents. The child is WRITTEN rather than navigated to — a real navigation
would boot a second copy of the whole SPA to display a table the parent
already holds. A BLOCKED POPUP IS NOT A SILENCE, and closing the window hands
it back. `recordMarkup` emits an EMPTY host and `paintRecord` fills it.
The popup half of the check needs `evaluate(…, { userGesture: true })`.
