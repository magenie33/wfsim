# wfsim — UI Vision

Besides the build and configuration forms, the page draws a **live 2D top-down
view of the fight**.

## Every scenario has ONE core metric, and nothing on the page names it

What a run is judged by is a term of the SCENARIO. The metrics are declared
once, in `engine::rules::metrics::ALL` — an id, the response field it reads, whether
that field is a total to turn into a per-minute rate, and its unit — and served
at `/api/meta.metrics`. The Measure control, the headline number and its unit,
the picker's and the optimizer's gain scans, the board strip and the scorer all
resolve an id against that table.

**NOTHING ASKS "IS IT DPS".** Written as `metric === "dps" ? … : KPM`, a third
metric is drawn as kills per minute — silently, in the units of a different
question — and that fork was in eight places. Adding a metric is now an entry in
`ALL`: `check_metrics.mjs` drives itself off the table, so it covers the new one
without being edited, and a temporary third entry reached the control, the
headline and its unit with no page change at all. An id the engine does not
declare is refused by `parse_fight`, which is where a share link arrives.

One literal survives on the page (`METRIC_FALLBACK`), because a scenario
constant is evaluated before `/api/meta` has been fetched.

### The kill score is reported as a RATE

The kill score — whole kills plus the fraction of the current target's pool
already drained — grows with the engagement, so two runs of different length
could not be compared at a glance. Under the default metric the headline is
**KPM**, score per minute, and the score itself sits beside it as the
engagement total. That is the same shape the damage numbers already had: a rate
to compare with, a total to read.

Simulator: `1.20 KPM · 2.40 kill score in 120s · …`
Optimizer row: `#1 · 1.20 KPM · 552,523 DPS · 2.40 kill score / 120s`

Presentation only — nothing was rescaled underneath. The optimizer still ranks
on the score, and at a fixed duration KPM is a monotone transform of it, so no
ordering moves. KPM is only as duration-invariant as DPS is: measured on one
Torid build, 30 s vs 120 s gave 0.044 vs 0.049 KPM while the totals went 0.022
vs 0.098 — the residual is ramp-up, reloads and the DoT tail, exactly the
drift DPS shows over the same pair (18,653 vs 20,551).

## Core decisions

- **Two surfaces**:
  1. **Config UI** — build/weapon/enemy/scenario setup. Deliberately simple;
     can be much sparser than predecessor tools.
  2. **Arena view** — a 2D top-down rendering of the simulated fight, so
     real environments and **AoE / multi-target damage** can be tested
     spatially instead of as scalar "assume N enemies in radius" checkboxes.
- **Geometry**: every actor (Warframe, enemy) is a **circle, radius 0.25 m**
  (assumption; refine later). The world is a plane — **the Z axis is dropped**
  for now.
- **"Feel" is probability**: aim wobble, headshot ratio, reaction time are
  modeled as probabilities (e.g. body-part aim weights), not simulated motor
  control.
- **No wasted DPS while measuring**: the standard measuring scenario is one
  Warframe vs one target circle with `TargetMode::InstantRespawn` — the
  target respawns in place the instant it dies (no on-death transformations).

## Engine mapping

| UI concept | engine |
|---|---|
| the fight, both actors | `arena::Arena` (a `Tenno`, a target with its hitboxes, a duration) |
| the player | `data::tenno::Tenno` — stats, and a `state` every conditional mod is asked about |
| target that never wastes DPS | `fight::TargetMode::InstantRespawn` |
| aim quality / headshot feel | `fight::BodyPart::aim_weight` |
| plane, positions, ranges | **nothing yet** — see below |

**The Arena VIEW has no engine behind it.** The sim fights one target and
assumes it is in range, so a plane has nothing to decide; the decisions above
live here and not in code.

When positions become real they belong ON `arena::Arena`, beside the actors
that would have them, not in a parallel module.

## Replay

**THE RESULT IS TWO BLOCKS.** The top one is the MEAN over every run — the
headline and its KPI row — and nothing moves it: it is what ranks, the same
figure the board, the optimizer and the quick calc read. The block under it is
the BENCHMARK FIGHT: one run, the middle one when the runs are ranked by the
scenario's metric (the upper middle of an even count), headed by its own number
beside the average and a line saying it is one sample and differs from it.

The Simulator's result carries that fight frame by frame: the target's pools,
its counters, the damage meter's own composition, and **live stacks per buff**.

**The replay sits at the TOP of that block and drives all of it**.
The block renders once at its finished state — its number, KPIs, damage meter,
DPS curve — and the replay re-reads all of it at whatever instant the
cursor stops on: the headline recounts, the KPIs recount, the meter
re-composes against the damage dealt SO FAR (a composition of a fight in
progress is read against that fight, not against its end), both curves grey
out everything past `t`, the pools refill.

Its own heading is the word "Replay" and nothing else — a transport control
does not need explaining.

The target's pools are a FIXED GRID, not a flowing row: every figure changes on
every frame, and a flex row re-measures itself each time, so the labels slid
about for the whole playback and the page read as if it were shaking. Fixed
columns and tabular figures hold still — and leave room for a second and third
enemy without a re-layout.
Rewind to 0 and the panel reads as a fight that has not happened; return to the
end and it is byte-identical to how it first rendered. That is what "replay"
means — a cursor that only slid along a line would be a decoration.

Re-read IN PLACE, never re-rendered: rebuilding the markup sixty times a second
would drop every open sub-row, every scroll position and the caret you just
clicked. Cells carry `data-kpi` / `data-mk` keys naming the series that feeds
them, and the wire format is the panel's own shapes with arrays where it has
numbers (`kpi` mirrors the KPI row, `sources` mirrors `damage_sources`), so the
client draws an instant of the fight with the same code that draws the end of
it. ~88 KB for a 60 s fight.

One row per
buff, each a short curve, all open by default — the question they answer is
"was this thing actually up", and a row you have to click to answer it will not
be clicked. `avg` and `uptime` sit in the header so the group reads at a
glance; play/pause + 1x/2x/5x/20x + a scrubber move one cursor across every
curve at once.

It is the same fight the block's own number came from, not a fresh run and not
an average. `Rng` is SplitMix64 with a single `u64` of state, so a run records
what it started from (`RunResult::rng_state`) and `fight::replay` re-runs that
one bit-for-bit. Cost: ONE extra engagement, and only when asked — the
marginal-gain scan calls the same endpoint once per candidate and shows no
replay, so `replay: true` is opt-in and only the Simulator's Run sends it.

## Presets and customs — two kinds of collection

**PRESETS vs CUSTOMS** — two kinds of collection, and the difference is who
CONSUMES them. **EVERY PRESET IS BORN "preset N"**, whatever it is a preset of —
a build, a scenario, a search, a Warframe or Operator build are one concept and
carry one name (`newPresetName`); a stored one may still carry its collection's
old noun and is read as generated (`isGeneratedName`). Each bar declares a
`noun`, which words its tooltips. A riven or an enemy is a custom, not a preset,
and is named for what it is ("riven 2").
A **preset** is a saved state of something that always exists, read only by
its own module: `builder-builds` (a build), `simulator-scenarios` (a fight,
buff settings included), `optimizer` (a search: the SCOPE and `finalists`, and
nothing else — never buffs, never a run count, never a thread count).

**EVERY PRESET BAR IS ONE COMPONENT, THE BUILD BAR'S** (`renderPresetBarIn`):
your own entries, then — where the collection PUBLISHES entries — a read-only
group of the ones opened into it, then "+ new". A published entry is one the
reader did not make: a board build, an official ruler. It is read-only
(⧉ copies it into one of your own, × takes it out of the bar without touching
it), it is kept in the bar by what it IS (`cfg.pins`, never by a rank), and the
open one is always among them. It comes in from the collection's way in: the
build finder for a board build, the bar's own list for a ruler. A bar's
`load()` is the reader's own entries alone; `published()` is the collection's
published ones, and a collection publishes exactly when its bar config has
them. No separate bar exists for published entries.

**EVERY SAVED COLLECTION IS ONE ROW OF `COLLECTIONS`** (`34-presets.js`): its
`kind` — preset or custom — and its `scope`, what one entry is about (a weapon,
a riven family, a frame, a companion, or `global`). Anything new a reader can
save is one more row, and whether it is weapon-scoped, which bar kind it gets
and what the sync shows are read from it rather than from a list of their own.

**AN ENTRY IS ITS `id`; ITS NAME IS A LABEL.** The id is opaque, never changed
by a rename, and everything that points at an entry points at it: the open one
(`presetId`, and every stored pointer), a link's `?build=`, a build's riven slot
(`riven:<id>`), a fight's custom target (`custom:<id>`), a roster seat, a
Warframe build's Operator link, a Forma group. So TWO ENTRIES MAY SHARE A NAME,
and a rename moves nothing. A name is still generated unique ("preset N",
"… copy"), because a reader tells chips apart by it; nothing depends on it.
`presetFind` resolves a pointer by id first and by name only for what an older
page stored or a caller typed, and one-time migrations at boot rewrite the
pointers, custom-target references and Forma groups an older page wrote by name.
Ids are minted in one place, `mintPresetIds`, at boot and on the way into
`storePresetList`; a new entry is born with one (`presetEntry`). A riven's is
minted by `foldRivensIntoOneList`, together with repointing the slots that name
it.

**THE DEFAULT IS THE BLANK, AND NOTHING IS OWNED UNTIL IT IS MADE.** Every
collection has a DEFAULT: the blank (`cfg.blank()`), always there, read-only,
never stored and never listed in its own bar. Owning none, the bar is empty and
the editor stands on the default; the first EFFECTIVE edit writes "preset 1", and
a preset edited back to the blank is DELETED — by CONTENT it is the default
again, so what is left is the default. Only an edit deletes it: a blank one made
by "+ new" stays until it has been worked on. Whether a state is the blank is read
from its content (`cfg.isBlank`), never from a stored flag. A build compares
against the blank of its weapon once one has been seen. A scenario's default
is the first official ruler, open read-only in its bar, so a first number is
one the board can compare. "Active" means the
state you are in.

**A LINK NAMES A PRESET BY ID, OR NONE.** Named, it means that preset; the
default, or one that was deleted, means the default. UNSET — never chosen — it
means the first preset, or the default while there is none, and follows: a preset
written later is what it means from then on. Picking a type (a frame) chooses no
preset.

THE OPTIMIZER TAB IS TWO HALVES AND TWO BOXES: one box is the SEARCH and is
exactly what a search preset saves; the next is the SIMULATOR's fight,
read-only, edited there. The final round has no count of its own: it is a
simulation at the simulator's Runs, so a row's number is what simulating its
build answers (docs/OPTIMIZER.md §"The answer").

CPU THREADS IS GONE: how much of this machine the page may use is ONE setting,
in the TOPBAR (`compute-select`). `woptWorkerCount()` is `poolSize()`; an
older preset's `threads` and `runs` are read by nothing and dropped on the
next save.
A **custom** is a thing you MADE that the OTHER modules consume — `rivens`
becomes a mod in the pool, `enemies` becomes an entry in the scenario's target
list. A custom enemy is the SAME TYPE as a published unit (`EnemySpec`), which
is what keeps the rest of the app ignorant of it. Three things are its own: an
inline `damage_modifiers` column, because a target nobody published may want a
vulnerability no faction has; a `status_immunities` list, which is a DIFFERENT
MECHANIC and not that column reading 0; and the fact that it is NOT
weapon-scoped. Owning none is ordinary, each carries its own identity rather
than a label you invented, and deleting one breaks references elsewhere (a
riven delete clears the slot that equipped it — a preset delete can never do
that). The mental model is a FILE: a list you pick from, one open at a time,
none open being a real state — so the UI is a list + editor, NOT the preset
chip bar, and the key is `wfsim-customs-<domain>` /
`wfsim-custom-open-…`. Everything below the key is shared: storage, undo,
scoping.

**DAMAGE IMMUNITY AND STATUS IMMUNITY ARE TWO MECHANICS**, and the wiki puts
both halves in one paragraph (`Status_Effect` §Status Immunity Interactions):
*"Proc type chances are not altered by enemy resistances or weaknesses to the
damage components used in their computation; however, they are modified by
enemy status immunities. When an attack procs a status effect on an enemy
which is immune to a particular proc type, the respective damage type is
excluded from proc type chance calculations for that enemy"* — independently,
"regardless of whether that enemy is also immune to Corrosive damage". So a x0
column changes what a hit DEALS and leaves the proc draw alone; a status
immunity changes what it PROCS, by leaving the denominator so the other types
RENORMALIZE onto the roll (the wiki's own example moves the other four from
18/5/9/23% to 33/8/17/42%).
The optimizer owns no scenario — it RUNS the simulator's, drawn by the same
renderer over the same state, READ-ONLY there, with a link to the simulator: a
preset is edited in exactly one place. That includes the BUFFS. The chain is
builder → simulator → optimizer, each reading upstream and writing nothing.

**NOTHING CROSSES BETWEEN WEAPONS — EXCEPT THE FIGHT, AND A RIVEN WITHIN ITS
FAMILY.** A BUILD and a SEARCH are statements about ONE weapon and are never
born from each other: a weapon opened for the first time gets a blank build,
the search's `finalists` resets, and the previous weapon's optimizer RANKING
is cleared rather than left on screen under the new weapon's name.

A SCENARIO is not a statement about a weapon, so it is SHARED across the
roster — one list, key `wfsim-presets-simulator-scenarios` with no weapon in
it (`SHARED_DOMAINS`), and switching weapons keeps the fight you are measuring
under. The one weapon-scoped knob it holds is headshot %, handled the way the
rulers handle it: the SERVER forces 0 on a weapon that cannot headshot.

**"HEADSHOTS WHILE CHARGING" IS THE AIM OF ONE PHASE.** On a weapon whose mode
cycles into an Incarnon form, the scenario may say every hit lands on the head
while the base form charges the gauge (`gauge_charging_headshots`); once the
weapon transforms, headshot % applies. It is the scenario's, not the mode's:
how well a player aims belongs to the fight a build is measured in, so a build
cannot carry a promise of its own aim. Off is what every ruler plays, and the
server forces it off where it forces headshot % to 0.

**NOTHING OUTSIDE A COLLECTION WRITES ITS STATE.** A build carries no `sim`
snapshot: a build is a build, and the live scenario is seeded from the active
`simulator-scenarios` entry and from nowhere else. "What this build was last
measured under" is its newest result record's `key` (§Results), which is what
makes a stale result show as stale. Every collection writes through
`storePresetList`, which is what makes one Ctrl+Z stack cover all four.
Customs are OPTIONAL by nature: nothing is auto-created, the last one can be
deleted, and the editor stands down instead of showing a document that is not
there. Presets are not — the modules behind them always have a state, and "no
build" is not something the builder can show.

## Build sync

**A SIGNED-IN READER'S SAVED ENTRIES ARE THE SAME ON EVERY BROWSER SIGNED IN
TO THE ACCOUNT**, free to every account — every entry under `wfsim-presets-*`
and `wfsim-customs-*`, and every reminder (`wfsim-reminders`), the reader leaves
synced, entry by entry, matched by `id`.
Saving on a browser is never limited; what an account holds may be, and the
server states it. `web/src/static/app/37-sync.js` is the page's half;
the server's half is the private worker's `/api/cloud/sync` (docs/ACCOUNTS.md
§"The private worker"), which keeps the newest write of each entry. localStorage
stays the working copy: signed out, or without the feature, nothing runs.

| rule | where it is held |
| --- | --- |
| a change is an entry whose signature moved since the last round; a deletion is an entry gone | `syncRound`, `wfsim-sync` |
| a change carries the version it was made from (`base`); one made from a version no longer current is a conflict, and both are kept — the account's version as the entry, this browser's as a copy beside it, the editor staying on its own | `syncBase`, `syncConflict` |
| each browser names itself and reports how its round went, so one that cannot sync is visible from the others | `syncDevice`, `syncReport` |
| a round runs after an edit settles, on sign-in, on coming back to the tab, and once on leaving it | `syncSoon`, `pagehide` |
| the first round on a browser is a union: everything here is pushed, everything there pulled | `syncRound` |
| a result never travels: it is a record of its own (§Results) | `syncBody` |
| an entry edited while the round ran keeps the edit; the next round pushes it | `syncApply` |
| two entries may share a name, so nothing is renamed on the way in | `syncApply` |
| the page re-applies an entry on screen that changed, for every collection `COLLECTIONS` names | `syncShow` |
| a browser that synced with another account merges nothing until the reader asks on `/account`, and counts none of it as this account's | `other`, `syncAdopt`, `syncedCounts` |
| undo steps over a list that changed underneath them are dropped | `syncShow` |
| an entry with `cloud_sync: false` stays on this browser; the cloud on its chip — and on a riven's row in its list — switches it | `isCloudSynced`, `cloudMark` |
| signed out where accounts exist, the cloud is hollow and links to sign in, coming back to the page; a click on it picks no entry | `cloudMark` |
| taking one off the account sends `{ id, cloud_sync: false }` and nothing else; another browser keeps its copy and stops syncing it | `syncRound`, `syncApply` |
| a new entry is synced unless "upload new items" (this browser's `wfsim-sync-auto`) is off | `syncRound` |
| the server states what the account may hold per pool (`allowance`); past it a new entry stays here and turning one on is refused | `syncAllowance`, `setCloudSync` |
| `/account/sync` lists every item this browser holds, newest first or by weapon, and switches sync one at a time or several at once; it deletes nothing | `cloudPage` (19-cloud-page.js) |
| its device and trash lists are the signed-in account's: another account signing in on the page forgets them, and an answer for the last one is dropped | `cloudForget` |
| a custom travels with what names it: an entry a round pushes brings every custom it names, counted against the allowance; one past it stays here and is named | `syncRound`, `customRefs` |
| customs are pushed before presets, so no browser pulls a build before its riven | `syncRound` |
| taking a custom off the account says how many synced items use it | `setCloudSync`, `syncNamers` |
| a reference this browser cannot resolve is HELD, never dropped: the page stands something in (an empty slot, the roster's first unit), says so, and the save puts the reference back where the stand-in still is; it is seated when the custom arrives | `holdAbsent`, `keepAbsent`, `syncShow` |

A custom declares how entries name it (`ref` in `COLLECTIONS`), and that is
all sync and the hold need. A page that resolves a custom reference must call
`holdAbsent` where it would otherwise substitute or clear one: a substitute
that is saved is a loss sync carries to every browser. Builds and scenarios
hold; a page that does not yet hold must not save what it substituted.

## Saved items as a file

**WHAT A READER SAVED IS THEIRS TO TAKE ANYWHERE, signed in or not.** "Saved
items" in the topbar's overflow, and on `/account/sync`, exports every
collection `COLLECTIONS` names as one file and imports one back
(`33-saves.js`). Sync is the convenience of not having to.

| rule | where it is held |
| --- | --- |
| the file is `{ format: "wfsim-saves", version, exported_at, lists: { <domain>: [entry] } }`; a board row, a sync choice and a stored result do not travel | `savesExport`, `savesBody` |
| import never overwrites: an item missing here is added as it was, one held unchanged is skipped, one held changed is added beside it as `(imported)` | `savesImport` |
| a custom added as a copy has every reference to it in the file repointed at the copy, so customs are read first | `savesImport` |
| a file from a newer version is refused, not half-read | `SAVES_VERSION` |
| imported items are ordinary new items: signed in, they sync under the account's rules | `syncSoon` |

## How the page speaks

No 最 / 第一 / 顶级 / 终极 in anything that reads as promotion — the
Advertising Law forbids superlatives. ONE WORD FOR ONE THING: a setup is 场景
and a run is 战斗, and a game term is DE's own (裂罅, 异况超量, 超宏防护);
`our_chinese_uses_one_word_for_each_thing` holds the list.

## Every box takes a Chinese input method

An input method COMPOSES: pinyin is typed, then characters are picked, and
every keystroke between fires `input` with `isComposing` set.

- **A BOX THAT REDRAWS ITSELF LISTENS WITH `onTyped`**, which skips the
  composing and runs once the characters are committed. Rebuilding the box
  under a composition kills it, and nothing Chinese can be typed at all.
- **AN ENTER OR AN ESC ASKS `imeComposing(e)` FIRST.** While composing, Enter
  picks a candidate and Esc drops the pinyin; neither is the page's to submit,
  close or cancel on. Safari sends the committing Enter with `isComposing`
  already false, which is why the helper also reads keyCode 229.

`check_ime_keys` holds the second rule over every handler; `check_ime_search`
types pinyin into each box that filters as it is typed in.

## The build finder

**EVERY BUILD IS IN THE BUILD BAR; THE FINDER ONLY FINDS.** The builder's top
box, the build finder (`renderBuildFinder`), answers "what do I equip" first
and "why" on request. It is the board's builds as a LIST, best first, scoped by
ruler, mode and riven and filtered by what a build CONTAINS — a mod, an arcane,
an evolution, a part, an element, a riven stat, each "must have" or "I don't
have". Mode has an "All" when the ruler holds more than one: every mode in one
list by score, each row naming its mode beside a rank that is still its own
mode's. It lists the top five of a scope until asked for more (twenty at a
time, folded back in one click). It holds a query and never a selection. "Open"
puts the build in the build bar as a read-only chip and makes it current; THE
BAR IS THE ONE PLACE THAT SAYS WHICH BUILD IS OPEN — its selected chip names the
build and a lock marks a board row, so nothing else on the page repeats it.
BUILDER ONLY: the simulator keeps the build bar and has no finder. It folds like
every box on the page, to its title and count, and it SHIPS SHUT above a
build of the reader's own on that weapon — with none there, it ships open
(§"Every box folds").

**NOTHING IN IT IS A THRESHOLD SOMEBODY CHOSE.** Every number it shows is a
count or a score off the board: no "just as good" band, no "core" cut, no
colour for how bad a gap is — a gap is green or red by its sign alone. A
judgement written into the page is one more constant to keep true for every
weapon and ruler, and the reader can draw it from the counts.

**THREE PARTS, IN ORDER OF HOW OFTEN THEY ARE WANTED.** The OVERVIEW, folded by
default (fold `finder-overview`): per module, every piece the scope's builds
carry, how many carry it and the rank of the best of them; a click cycles it
through "must have", "I don't have" and clear. The LIST. And a row OPENED in
place, the same for every row: the whole build and, piece by piece, how many
builds here carry that piece and the best build here without it, against this
one, with "I don't have it" beside each. That gap is to the best board build
lacking the piece, not a measured one-for-one swap, and the page says so.

**THE COMPARISON IS THE READER'S.** Any opened build can be made the reference:
every other row then shows its gap to it, mutes what it shares with it, and an
opened row lists what it has that the reference does not. There is no default
reference — the board's #1 is only the reference if the reader makes it one.

**A ROW IS THE SIMULATOR'S BUILD CARD.** `buildCardHtml` draws a build from a
descriptor — mods, parts, arcane, evolutions, valence as chips with icons and
full names — for the simulator's "what is being tested" block and for every
finder row alike, so the page has one picture of a build. THE MODS RUN IN THE
ORDER THE BUILD WAS SAVED, the exilus last: elements combine in slot order, so
a row sorted any other way can read as another element build. The riven rides
in the mods as its stats.

**A BOARD BUILD IN THE BAR IS KEPT BY WHAT IT IS.** A builtin id ends in its
rank, and a rescore renumbers the board, so the bar stores the build's identity
(`boardRowIdentity`) plus its cell, per weapon (`wfsim-opened-board-<weapon>`),
and the OPEN board build is recorded the same way beside the active pointer
(`wfsim-opened-board-active-<weapon>`) — a page reopens on the build, never on
whatever holds its old rank. A build that has left the board stops resolving and
drops out of the bar; nothing is pruned while the weapon's board is not in hand.
× on its chip takes it out of the bar (the board keeps it) and, if it was open,
lands on your first build or the blank one.

## A share link is a build, and never a fight

**A SHARE LINK IS A STATEMENT ABOUT A WEAPON, NEVER ABOUT A FIGHT.**
`/weapons/<Wiki_Name>?b=<code>` carries the build and the RIVENS it equips (a
custom exists only on the machine that made it, so it must travel inline), and
carries nothing else. Opening one creates a NEW copy of each — never a merge,
never an overwrite — repoints the build's riven ids at the copies, strips the
query so a refresh cannot import twice, and says what it dropped. The payload
is POSITIONAL and omits everything derivable (defaults, max ranks, the shape
drafts a riven regenerates).

THE FIGHT AND THE MEASUREMENT ARE FIELDS 7 AND 8, FROZEN AT 0. A scenario is
`SHARED_DOMAINS` — one list for the whole roster — so a link that planted one
would follow the reader onto every other weapon they own, and a number measured
in a fight the reader does not have is not a claim they could check. Links
posted while both travelled still carry them; `decodeShare` does not read the
fields, which is why `importShare` has no scenario step to guard rather than a
guarded one. `check_share` builds such a link by hand and asserts the reader's
own fight, their scenario list and the build's results are all untouched.
A v3 link names an id by its place in `data/share_order.yaml`, which is
APPEND-ONLY and held there by a ratchet — `engine::data::share_order` recomputes the
generator's digest over the whole list and fails on anything that is not an
append, so a reorder is a red test rather than a link that quietly opens
somebody else's build. It is worth 3.4x: the same Laetum is 279 characters as
slugs and 79 as indices. The v2 array is still the one internal
representation, so `importShare` and everything below it are untouched; v1 and
v2 links still open.
AND v3 IS PLAIN TEXT IN THE URL. At 79 characters deflate makes the payload
BIGGER, so the text goes in raw; the separators are RFC 3986 unreserved
characters and sub-delims a query accepts unescaped. A payload it cannot
express — a name in a script the URL would escape — falls back to the
deflate+base64 form, so the encoder measures all three and takes the shortest.

A NAME THE SHAPE IMPLIES DOES NOT TRAVEL: a board riven's local name is
`boardRivenName(shape)`, derived on arrival, which is shorter AND names it in
the reader's own language.

**NO NAME TRAVELS AT ALL, AND THE LINK IS SHORT.** A name is the one field a
person types and it was most of a link — a riven named in Chinese is dozens of
`%XX` escapes, and a long random-looking string is what phishing heuristics
flag. `shareCode` drops the build name and replaces every riven name with its
generated one; the reader's copy is named `build (shared)` and `Crita-…`.
The code is then STORED: `POST /api/s` on the worker returns an id, and the
link is `/weapons/<Wiki_Name>/s/<id>`. The id is the worker's hash of
(weapon, code) — the same build is one id and one row, no client picks an id,
and the row under an id never changes (worker/index.js §"SHORT SHARE LINKS",
table `shares`). The short path serves the weapon's own prerendered page, so a
pasted link previews as that weapon; boot takes the id off the address before
anything reads it and `route` lands the build through `importShare`, exactly
as a `?b=` link does. Only wfsim.app and the desktop shell make short links; a
dev server or a check (127.0.0.1) makes the long form so no test writes the
live store, and ANY failure to store falls back to the long form, which opens
as it always has. `check_share_short.mjs` holds the worker's half.
It rides the QUERY, not the fragment — a fragment never reaches a crawler and
these links are meant to be posted.

**A PASTED SHORT LINK PREVIEWS AS ITS BUILD.** A chat reads the head without
running the page, and the weapon's own head describes the board's best build.
So the worker restates the head for `/weapons/<name>/s/<id>`: title
`<Weapon> build | WFSim`, a description listing mods, riven stats, arcanes and
evolutions in ENGLISH (the one language both markets read), `og:url` naming
the link, and `noindex` (the canonical still names the weapon). It decodes the
stored code with THE PAGE'S CODEC — `29-share-codec.js`, copied verbatim into
`worker/share_codec.js` by `scripts/gen_worker_parts.mjs`, whose `--check` runs
in CI — against `site/share-names.json`, which the site build writes from
`data/share_order.yaml` and the data's English names. Nothing a sharer typed
reaches a preview. Any failure serves the weapon page unchanged.

**…AND UNFURLS AS A CARD.** The rewritten head names `/og/s/<id>.png?v=<n>` as
a 1200×630 `summary_large_image`. The worker draws it on first ask —
`share_card.js` writes the SVG from the same `shareBuildNames` the text uses,
`share_png.js` rasterises it with resvg and Inter (both vendored under
`worker/vendor/` with their licences) — and the edge cache keeps it for ever
under that address, so `SHARE_CARD_V` is bumped whenever the drawing changes.
TEXT ONLY, like the weapon cards the build draws: the resvg build does not read
the site's WebP art, and a card that states things needs no DE art to do it.

**A SHORT LINK MAY NAME WHO SHARED IT, WHEN AN EXTENSION CAN SAY.** It may
carry a second segment, `/weapons/<Wiki_Name>/s/<id>/<sig>`; boot takes both
off the address, and once the build lands the page asks the extension
(`shareWho`) and draws its answer over the build bar while that build is open,
and on the card. An extension that can sign offers it in the share panel
(`shareSigner`); any failure is the plain short link. The page draws nothing of
its own here, so where nothing is mounted a link names nobody
(docs/UI.md §Extensions).

**THE SHARER'S RESULT MAY TRAVEL BESIDE THE LINK, NEVER IN IT.** A scenario is
always selected, so the panel offers "include my result in this scenario" (off
until chosen; the choice is browser storage). The result is a CLAIM stored in
the `shares` row and hashed into the id, never a field of the code: opening the
link still lands the build alone. An OFFICIAL scenario is named by its id — the
whole fight, which any reader can run; any other fight is stated by its terms
(enemy, level, Steel Path, duration) and marked as the sharer's own, with a
target the sharer built left unnamed. `shareClaim` in the worker rebuilds it
field by field against `share-names.json`: no typed text reaches a preview, so
a forger can misstate the number and nothing else. The value is `fmtScore`'s
spelling, sent as text, so the preview prints what the page printed.

**THE PANEL OFFERS THREE WAYS OUT**: copy the link; copy it AS TEXT — the same
build decoded from the link's own code, named in the sharer's language, for a
chat that shows a link as a bare string; and the system share sheet where the
browser has one (`navigator.share`).

**THE PANEL IS ONE, AND IT OPENS WHERE A BUILD IS FOUND.** Besides the build
bar's own button, a share sits beside a build wherever one is arrived at — an
opened finder row, a search's finalist, the simulator's result — and opens the
same panel (`openBuildShare`); a second panel would be a second answer to what
a link carries. A board row or the result is made the open build first, so the
bar names what leaves. A FINALIST IS NOT OPENED — opening any build resets the
search — so the panel opens under its row and encodes that row's build
(`sharePayload(build)`), without the reader's result and the card, which are
both drawn from the open build and would describe another.

## The share card

**THE CARD IS THE BUILDER'S STEPS, IN THE BUILDER'S ORDER.** "…as a card" in
the share panel draws a long picture of the build (`31-share-card.js`): one
block per builder step, keyed by the step's id in `CARD_BLOCKS` and ordered by
`builderSteps()`, so a step moved on the page moves on the card and a step
with no block fails `check_share_card`. A block with nothing to say for this
build draws nothing; small blocks (arcane, valence, parts, wielder) pair on a
row. The link beside it is the same build link.

**THE HEAD IS A POSTER.** The backdrop is one word of the weapon's ENGLISH name
in outlined capitals, in every locale: the word the fewest roster names share
(`cardKeyword`, so "Prime", "Kuva" or "Tenet" never win and no list is kept),
the other words small on its shoulders. The art stands on it with no frame,
lifted by its shadow; the name, kind and mode sit bottom left in the reader's
language. The MODE IS ITS NAME: a mode is an action list, and the link carries
it whole.

**THE MODS ARE THE ARSENAL'S**: stance above, the eight in two rows of four,
exilus below, each with its art, name, rank and the drain the slot's colour
makes it (green matched, red mismatched); an empty slot still draws its
polarity, since the Forma is part of whether the build is legal. Capacity and
Forma head the block, and a riven's own rolls stand beside the grid.

**WHAT ELSE IT SAYS IS WHAT THE PANEL SENDS**: the sharer's result only when
"include my result" is on, and the signer only when the link is signed, as
`/api/cloud/share/<sig>` answers for that link. The foot carries the QR,
because a phone cannot click a picture.

**THE SHOWCASE IS THE SHARER'S, UNDER THE HEAD** (`cardShowcase`): their name,
the WFSim Volunteer honour, a signed link's mark, and the figures they tick —
points and their place on the weekly, monthly and all-time rankings — read from
`/api/account/devices` for the account signed in. It is offered only to an
account named on the ranking with points, the consent the ranking already
asks; before the sharer picks, it shows points and the best place, a tie going to
the longer ranking. With it on, the foot does not name the sharer again. The
honour is filled and a paid mark only outlined: what was computed reads first.

**A THEME IS TOKENS, NEVER LAYOUT** (`CARD_THEMES`): colours and fonts. The
blocks and their order are the build's.

## The support page

Every figure on `/support` is COUNTED, never typed: `PROJECT_FACTS` is written
into `app.js` by `build_site_app.py` (commits and the first commit's day) and
everything else comes from `META` and `BOARD_META`. A figure is claimed only if
a reader can check it against the public repository.

THREE FIGURES: what is modelled (`rosterSize()`, a SUM over META's categories
so a new one counts itself and the function is never edited again), what
players built with it (`submissions`), and what came back out (`listed`).
Submissions are ONE pool every ruler reads, so they are MAXed; listed scores
are each ruler's own and are ADDED.

NO SUM APPEARS ON THE PAGE. `check_support` asserts the whole page against a
currency pattern, because a digit comes back in a sentence unnoticed.

THE APP NEVER ASKS. No result, limit, banner or mail points the reader
anywhere they did not go looking. A reader at the sync allowance is told the
limit and offered Export. A feature may be SHOWN where it would be used — signed out, a
saved entry's cloud is hollow and links to sign in — the control itself, never
a line, a count or a reminder beside it.

WHAT THE READER HAS RUN NEVER LEAVES THE BROWSER. `wfsim-use` is two integers
written by `runSim` and read by `/support` alone; the page says so where it
prints them.

## Extensions

**WHAT A DEPLOYMENT ADDS BEYOND THE CALCULATOR IS MOUNTED, NEVER BUILT IN.** At
boot the page fetches `/api/cloud/page.js` and runs it (`01-ext.js`); the script
registers what it adds in `EXT` and draws with the page's own helpers. With
nothing behind that door — a dev server, a fork, the desktop shell — nothing
mounts and the calculator is whole, so no line of the page may assume an
extension is there.

| what an extension registers | the page uses it for |
| --- | --- |
| `EXT.pages[kind]` — `path`, `view`, `title`, `nav`, `open`, `settings`, `available()`, `load()`, `render(account)`, `shown(main)` | a page in `#auth-page`, routed by `authKindOf`: `open` for anyone, `settings` among the account's pages and in their navigation while `available()`; `view` is its `app.view` kind |
| `EXT.strings[lang]` | `tr`, after the page's own table |
| `EXT.hooks.act(el, kind)` | a `data-auth` action `authAct` does not know; true when taken |
| `EXT.hooks.deleteNote()` | what else deleting the account ends, under the danger zone |
| `EXT.hooks.support(slot)` | the support page's `#ext-support` |
| `EXT.hooks.shareSigner()`, `shareWho(id, sig)` | signing a short link, and naming who signed one |

A hook that throws is a hook that is not there (`extHook`). `route` and
`loadAccount` wait for the mount only until it has settled, so an extension's
address opens its page and every later route starts synchronously as before.

**A STATIC DOCUMENT HAS SLOTS, FILLED BY THE WORKER.** `/privacy`, `/terms` and
`/refunds` read with no script, so their `data-ext` elements are filled by the
site's worker before the page leaves (`worker/ext_documents.js`); a slot the
private worker does not answer is removed.

---

## Every box folds, and a floating menu indexes them

**A WEAPON PAGE IS ONE LONG SCROLL, AND THE READER WANTS THREE LINES OF IT.**
So every box on it shuts from the header it already has: a `.block` from its
`.bh`, a `.fold.sect` from its `h3.sim-h`, the build finder from its `.fd-head`
(rewired on each of its renders, which redraw the head). Collapsed, the
simulator is two header rows and the optimizer is one.

**ONE MECHANISM, THREE SHAPES, ONE STATE.** The result panel's `foldBlock`s,
the sections authored in `index.html` and the blocks themselves all resolve to
`setFold`, and all of them are keyed in `wfsim-folds` — by fold id ALONE, so
what you shut stays shut across a run, a tab switch, a reload and the next
WEAPON: what a reader wants to look at is a habit, not a property of a gun. A
block's fold id is its own `id`, stamped by `wireStaticFolds`; nothing lists the
blocks anywhere.

**A BOX MAY SHIP SHUT, AND ONLY UNTIL THE READER ANSWERS.** `shut` in the
markup is the default `folded()` falls back to while `wfsim-folds` holds no
entry for that box; the stored answer outranks it forever after, "open"
included. The build finder is the one that ships shut — it is a page of table
above the build you came to edit. With no build of your own on the weapon
there is nothing to edit and the finder is the answer, so it ships open;
which one is decided on arriving at the weapon, so a first edit does not shut
it under the reader (`ships_open`).

**THE BODY IS EVERY CHILD BUT THE HEADING** (`.fold.sect.shut > :not(.fold-h)`)
rather than a `.fold-b` the markup has to remember to wrap. A section that
gains an element tomorrow folds with it; one that gains a wrapper does not
quietly stop folding.

**A CONTROL IN A HEADING IS THAT CONTROL'S CLICK.** The Buffs section's "all",
the Forma buttons in the mods block's `.bh`, the mods axis's filter box, the
finder's whole search box (`role=search`, its tokens and suggestions with it):
the heading is allowed to carry them, and folding the thing they belong to
instead is the one way this feature can make the page worse (`foldsOnClick`).

**THE JUMP MENU IS READ OFF THE PAGE, NEVER LISTED.** `pageFolds()` walks the
visible module's blocks, the sections inside each and any fold standing beside
them (`.config-page > section.fold`, the finder), so a section added
tomorrow is in the menu, in "collapse all" and reachable with no edit — and
each row is named by its own heading, already translated, rather than by our
id. Two things it must do and a plain anchor list would not: a row OPENS its
target and every fold above it (scrolling to a shut section lands on a heading
with nothing under it, and a section inside a shut block does not move the page
at all), and it says which rows are currently shut.

**IT DRAGS, AND THE CLAMP IS APPLIED TO THE DRAWING, NOT TO THE WISH.** No
corner is free on every window, so the position is the reader's and is
remembered; but a spot chosen on a wide monitor is off-screen on a laptop, so
`placeJump` clamps what it writes and leaves `jump.x/y` alone — a window
narrowed and widened again puts the menu back where it was. The grip is BOTH
the handle and the switch: a pointer that never moved 4px is a click. The drag
is tracked on the WINDOW, because a pointer that leaves a 30px button stops
sending its events and `setPointerCapture` throws on a pointer the browser does
not consider active — an uncaught throw there puts up the boot-failure notice
on a page that booted perfectly well.

`check_folds` holds all of it.

## A finger scrolls; it does not drag the fight

**A FINGER SCROLLS; IT DOES NOT DRAG THE FIGHT.** A browser decides who owns a
gesture at `pointerdown` and never gives it back, so a body that drags on
touch means the finger that started on it can no longer SCROLL — and a 19x19
formation covers the canvas in bodies. A LONG PRESS CANNOT FIX IT: once the
gesture is the browser's it is gone. The answer is a MODE the reader turns on
— a ✥ chip in the scene's own control row, off by default, drawn only where
`navigator.maxTouchPoints > 0` (a touchscreen laptop reports a FINE pointer,
so a `pointer: coarse` query is the wrong test). `touch-action` follows it:
`pan-y` off, `none` on. A mouse is unaffected.

## A section wider than the screen

**A SECTION'S BODY SCROLLS SIDEWAYS; IT IS NEVER CLIPPED.** `.block` rounds its
corners with `overflow:clip`, and its `.bb` scrolls on x — so a row that does
not fit a phone is reachable rather than cut off at the edge with nothing to
say it continues. A phone's own scrollbar is an overlay that is invisible until
touched, so `05-hscroll.js` gives every overflowing body a bar of its own:
pinned to the bottom of the screen while the section is on it, a thumb to
drag, a tap on the track to jump, and ‹ › to step. It appears only while the
body is wider than its box. THE BAR IS THE BACKSTOP, NOT THE LAYOUT: a section
that a phone reader uses gets a narrow layout of its own (the riven editor's
stat rows go to three lines under 720px), and `check_mobile` measures that.

## Every enemy has a name, and the page can ask about one

**EVERY ENEMY HAS A NAME, AND THE PAGE CAN ASK ABOUT ONE.** `fight::Body` is
`{state, debuffs}` per body; `formation::FoeSpec::id` names it, stable across
edits because it travels in the scenario and is filled in BY POSITION when
blank. The aimed body is `e1`, and on the wire it lives on the `Arena` rather
than in the formation list; INSIDE the fight it is `bodies[0]` like any other,
which is the numbering `FightParams::body` and `damage_by_body` share.
`/api/simulate` returns a ROLL CALL (`bodies: [{id, aimed, at, damage}]`) of
the ones that took something, because a per-BODY figure is the only thing that
can say a crowd was REACHED rather than a big number produced.

SETTING UP A FIGHT AND READING ONE ARE TWO THINGS, so the RESULT panel draws
its OWN copy of the scene: the scenario's canvas is where a body is PLACED —
draggable, with distance shortcuts and +1/+8; the result's is read-only,
shaded by what each body TOOK, marks the one being examined and PICKS rather
than drags. `mountArena` takes `heat`/`selected`/`onPick` for the second kind.

A BODY IS NAMED WHERE IT IS CREATED (`nextFoeId`), one past the highest ever
used rather than one past the count, so deleting a body never hands its name
to the next one.

THE DEBUFF TABLE HAS A SUBJECT: `Replay::tracked` names the bodies it
followed, the panel draws a chip per body, and picking one redraws the table
from the stored result at no simulation cost. It follows the aimed body plus
the hardest-hit few (`REPLAY_TRACKED = 8`), because a series is 600 frames ×
15 debuffs = 18 KB a body and a 19x19 would be 6.5 MB. The cap is SAID ON
SCREEN, never applied silently. One body draws no chips at all.

**AND THE CAP COSTS A READER A WAIT, NOT AN ANSWER.** Every other row of the
roll call is clickable too: the click re-runs the SAME engagement following
that body — `replay_follow: [id]`, `runs: 1`, and the `run` key handed back so
the endpoint replays the fight on screen rather than a new one that merely
agrees with it. What comes back is merged into the result in hand, never
swapped for it. It is EXACT rather than an estimate, because a run is
reproducible bit-for-bit from its `rng_state` — the same property the scout run
that ranks bodies already leans on, read the other way round.

## There is one fight, and every module sends it

**THERE IS ONE FIGHT, AND EVERY MODULE SENDS IT.** `theFight()` is the only
spelling, and THE LIVE `sim` IS THE FIGHT — not the preset behind it, which is
a saved COPY that `applyScenario` seeds `sim` from and the auto-save writes
back.

THE ONLY THING A CALLER OWNS is `replay`, a `seed`, `run_series` and the quick
calc's RUN COUNT — the reader's precision, not an edit to the fight. It lands
LAST in the spread, or the box silently does nothing while every chip's
tooltip quotes it.

THE BUFF MAP TRAVELS WHOLE, because buff settings are the FIGHT's and it is
the BUILD that decides which have a source (the server's `BuffCfg` is a
lookup, so an entry nothing grants is never read). Pruning it to the current
build makes the quick calc a different fight the moment a candidate grants a
buff the current build lacks — which is every candidate worth ranking.

A SWITCH IS THE OTHER WAY A FIGHT MOVES, and it has to RE-ASK rather than
repaint: an EDIT re-runs the scan through `markScenarioDirty`'s debounce, a
switch goes nowhere near it. `scenariosChanged` calls `refreshGains()` — that
hook rather than the call sites, because it is already the only thing every
scenario mutation goes through.

## A fight is n against m, and zone 2 draws it

**THE ZONE IS TWO SIDES AND THE FLOOR BETWEEN THEM.** Left is what the reader
DECLARED about their own play, right is what the engine FOUND about the enemy,
and the arena in the middle is where both of them stood — so the layout IS the
claim this zone exists to make rather than two headings over two lists of
chips. A condition about the TARGET is simulated; one about the TENNO is
assumed, and now a reader can see which is which without being told.

**IT SCALES BECAUSE THE PICTURE DOES.** Nine bodies were a chip reading "9
bodies" and three seats were not on the page at all; a dot per body and a row
per seat is the same block at 1 against 1 and at 3 against 361.

**THE FLOOR IS THE FIGHT AS IT OPENED**, and it stays that way when the arena
learns to MOVE. Where everyone started is a CONDITION — the only question this
zone answers — and where they got to is what happened, which belongs to the
replay and follows its playhead. The two scenes are two questions rather than
one drawn twice, so the day positions become time-varying only the replay's
mount takes a clock.

**ONE ACTOR'S SHEET IS A FOLD OF THE RECORD**, not a second set of counters:
`actorSheet` groups the rows the ledger already wrote, an ALLY by where its
damage came from and an ENEMY by who dealt it. Every figure on it is a sum of
rows the reader can open, so the summary cannot disagree with the thing it
summarises — and it costs the engine nothing, where a per-actor accumulator in
`RunResult` would be 25 KB an engagement (`MAX_BODIES` is 400). It is ONE
engagement, like everything else below the replay bar; the means live in zone
1 and in the per-seat table.

## A fight is n against m, and the roster is the other half

**A FIGHT HOLDS MORE THAN ONE GUN.** The enemy half has been a list since the
formation existed; the roster in block 2 (`28-fight-roster.js`) is the other
one — a squadmate, a companion or a summon, which are one thing to the engine:
something with a build that acts on its own clock.

**A SEAT IS A LINK, NEVER A COPY** — `{weapon, preset}`, the same shape the
wielder's is. The preset IS the build, so editing it moves this fight too; a
copy stored in the scenario would be a second answer to "what is that build"
with no rule for which wins. It resolves at send time through `seatPayload`,
the same function the open build goes through, so an axis added to
`BUILD_AXES` reaches every seat or none.

**SEAT 1 IS THE OPEN BUILD**, is not in the list, and cannot be removed or
pointed anywhere: it is the one the page is about, the one the panel resolves
and the only one a board submission carries. The roster is a declared scenario
field, so an official ruler — which never mentions one — is one gun.

**EVERY READER OF A FIGHT WIRES IT**, `/api/simulate` and `/api/log` both,
through `seat_the_rest`. A record drawn from the wielder alone is a TRUE
record of a fight nobody ran, every row in it correct and the fight wrong.
`combatants` comes back one row per seat, each naming the WEAPON it brought —
read from the run rather than looked up in a roster that has moved on.

## Aim is dragged, and a pick reads nothing

**AIM IS DRAGGED, AND A PICK READS NOTHING.** A bare click clears the
selection and does not aim; the rail carries an explicit AIM TOOL, after which
the marker drags in Select like anything else with a position. A fight that
moves on a mis-click makes the result on screen a result for a fight nobody
was in.

PICKING AN ENEMY IN THE RESULT READS NO STORAGE: `renderResults` records
`shownResult` and a pick redraws THAT. Storage is what survives a reload or a
preset switch, and nothing else asks it a question.

HARDEST HIT FIRST, in both views. `tracked` comes back in the engine's slot
order, which answers "who got followed" rather than "who took the most", so
the display order is sorted; `data-rpfoe` stays the index into `tracked`,
because `dstacks[k]` is that body's series. Each chip carries the number it is
sorted by.

A UNIT IS A COLOUR, derived and never declared: `unitHue` hashes the id
(FNV-1a) into a hue, so a unit that arrives tomorrow already has one and it is
the same colour on every machine. The same hue is a swatch wherever a unit is
NAMED.

## A body is the unit it was placed with

**A BODY IS THE UNIT IT WAS PLACED WITH.** The card on the left of the arena
says what you are ABOUT to place; it is not a control over the floor. The unit
is STAMPED at placement (`placeAt`, `arenaAddFoe`) and nothing afterwards
moves it — the server reads a blank unit as "the aimed body's", so an
unstamped body changes species when the card does. `FoeSpec` carries a
per-body `enemy`, `level` and `eximus`; the LEVEL deliberately stays the
FIGHT's, one dial for every body, so a body leaves it blank and follows.
The AIMED body still follows the card, because that card IS the fight's target
and the aimed body is what a placement copies. That split is invisible until
somebody switches and watches nothing change, so the panel states it and
counts what is holding a different unit.

EVERY FORMATION SAVED BEFORE THE RULE PINS ITSELF ON LOAD: `applyScenario`
fills the blank in from the scenario's own enemy. Growth stopping is not the
same as what is already there being fixed.

## The page says which build it is

**THE PAGE SAYS WHICH BUILD IT IS.** A fix that is deployed and a fix that is
on the reader's screen are two different things, and without a version on the
page neither side of a bug report can tell "still broken" from "still holding
the old file". `build_site_app.py` stamps the footer with the commit, a `+`
for a dirty tree, and a DIGEST of the two sources the guard is about — the
commit alone is not enough, because `site/` is built from a WORKING TREE. The
dev server ships the `dev` placeholder. The same stamp goes into `app.js` as
`BUILD_ID`, so a browser holding an old page with a new script can say so
(`checkBuildMatches`).

**IT IS A CONTENT HASH AND NOTHING ELSE.** The question the guard asks is "is
this the same script", so the token has to move when `app.js` or `index.html`
moves and stay still otherwise. Anything volatile in it rewrites all 386
prerendered pages on every build and buries the diffs that matter: a clock did
that once per run, a commit sha once per commit. So the pages carry the DIGEST
alone, `stamp_once` computes it once (two calls to a clock could disagree, and
a page and a script that disagree tell every visitor they are stale), and a
rebuild of unchanged sources is a byte-for-byte no-op.

**THE COMMIT GOES IN `app.js`, NOT IN THE PAGE.** A human reading the footer
wants it, and the guard does not; `BUILD_SHA` is substituted into the script
alone and the footer is drawn from both at boot — after `checkBuildMatches`,
which reads the token the page was SERVED with and would otherwise be reading
what this line just wrote over it.

## Results

**A RUN IS A RECORD, WRITTEN ONCE** (`35-results.js`): the build and the fight
as they were (`build`, `fight`, and `key`, which joins them), the engine that
measured them (`engine`: a digest of the code and data a number is computed
from, translations excluded — `engine_id` in the site build), which preset and scenario it came
from, and the summary `r`. A preset holds no result: "this build's number" is
its newest record (`resultLatest`), so a second run does not erase the first.
Every run is recorded; each build keeps its newest `RESULT_KEEP` unpinned
records per scenario, and a pinned one (`kept`) until it is deleted. The blank
records nothing, having no build to name.

**A RECORD IS NEVER MIGRATED.** A field it lacks was not measured when it was
written, and a reader says "not recorded" rather than reading 0; a new field
raises `RESULT_SCHEMA` and applies from then on. A `lastResult` an older page
left in a saved entry becomes a record with what it held — no engine, the
fight as its `key` states it — and leaves the entry only once the record is
stored. Two records compare RUN BY RUN only on one engine and one fight.

**A REPLAY NEVER REACHES THE DISK.** It is 600 frames of debuff series per
followed body plus a hit account per attack part, about forty times the
summary, and the one part of a result a button regenerates. `resultMem` (keyed
by weapon AND preset) keeps it for the session. Records live in IndexedDB
(`wfsim` / `results`), so a preset list in `localStorage` is small again; with
no IndexedDB the in-memory mirror is all there is.

## A slot keeps the card and its rank apart; the wire joins them

**A SLOT HOLDS `{ mod, rank }`, AND EVERY LIST THAT LEAVES THE PAGE HOLDS
`<card>@<rank>`** below max rank (docs/OPTIMIZER.md §A card is searched at max
rank). `slotModId` and `splitRank` are the only crossings; a share link keeps
its own positional rank field. The quick calc's `every rank` control edits the
list of cards offered once per rank, by card, stored in `wfsim-gain`; a named
card is a picker row per rank, each with its own gain chip, and the optimizer
pools each rank of a pooled one.

## Progress belongs where the work is being read

**PROGRESS BELONGS WHERE THE WORK IS BEING READ.** A pool of ninety mods at a
real run count is tens of seconds of a list that does not move, and a list
that does not move is read as broken rather than as busy. The per-row "…" chip
is a different claim: it says THIS row has no answer yet. `scanStrip` is one
component — a bar, a count, sticky at the top of the list — fed from whichever
scan state that list reads, mounted in all five places a scan ranks something.

## A page that is not a module is a shell page

**A PAGE THAT IS NOT A MODULE IS A SHELL PAGE** — /support, /benchmark,
/utility and /download. It belongs to no weapon, so it sits beside the home grid rather
than under `/weapons/<name>`, and it is not a fourth MODULE: it produces
nothing the three consume. /download is the offer for the Windows client, a
PAGE that answers what a downloader asks — what SmartScreen does on first run,
why the program is unsigned, what updating costs, what uninstalling means,
where the source is. THE ONLY ENTRY IS THE TOPBAR'S OVERFLOW MENU, and no
surface of the site offers a download anywhere else. `check_downloads` asserts
the home page offers nothing on every user agent it drives, and that /download
is the one surface telling a Mac, Linux or phone reader it will not run there.
See `docs/DESKTOP.md`.

## Utility

**/UTILITY IS THE GAME'S LIVE STATE AND THE READER'S REMINDERS ON IT**, and
none of it feeds the builder, the simulator or the optimizer (`docs/CORE.md`
§4). One tab per kind the worker lists, then Reminders.

| rule | where it is held |
| --- | --- |
| everything listed is an item `{kind, id, attributes, names, started_at_ms, ends_at_ms}`; a new kind is a parser in the worker and a tab registered in `UTILITY_KINDS` | `worker/world.js` `KINDS`, `27-utility.js` |
| DE's file is relayed by the bot server each minute (DE refuses the worker) and served from R2; the feed is the live site's, from every origin, asked each minute while a Utility page shows and every two minutes elsewhere while a reminder waits | `worldLoad`, `utilityClock` |
| Void Fissures shows all of the game's three lists, or one — star chart, Steel Path, Void Storms, in that order on every chip row — by relic era, then list, then time left | `41-fissures.js` |
| Arbitrations are in no DE file: the worker reads a schedule computed ahead, puts the hour open now in the feed, and serves the next fourteen days apart (`/api/world/arbitrations`), which the page lists by day, filtered by mission type and faction | `arbitrationsOf`, `47-arbitrations.js` |
| a reminder is a kind and some attributes; it matches an item of that kind holding every one | `reminderMatches` |
| the bell on a row makes one, holding the attributes the kind names (a fissure's list, era and mission; an arbitration's mission type and faction) until the reader changes the chips | `reminderDraft`, `*_REMINDER_DEFAULT` |
| Reminders also makes one from nothing, for what is not open: one kind at a time, each attribute Any until picked, offering only what can occur beside the other picks — a fissure's combination of list, era and mission as the game has opened one (the worker logs every fissure it sees), an arbitration's as a node of the schedule holds them | `fissureBuild`, `fissureLog`, `arbitrationBuild` |
| a reminder for a kind known ahead says when it fires next instead of how many are open | `reminderStatus`, `arbitrationNext` |
| a saved reminder nothing can ever match says so in red — no fissure the game has opened holds its picks, or no node of the arbitration schedule does | `possible`, `fissurePossible`, `arbitrationPossible` |
| what a new reminder already matches is seen, so it fires for what opens later | `reminderAdd` |
| reminders sync with a signed-in account as a pool of their own; they fire on each browser while the site is open there, on any page: once per item, never for one open when the reminder was made, said on the page, counted on the Utility link until Reminders is opened | `reminderCheck`, `reminderRead`, `syncPool` |
| a system notification as well only when the reader turns it on, which is when the browser asks, and only for a tab in the background | `reminderSystemOn` |

Where the names come from: `docs/DATA_SOURCES.md` §"The world state".

## The page is three modules, plus editors

**THE PAGE IS THREE MODULES — Builder | Simulator | Optimizer** — with one
tab/view each, plus EDITORS that feed them. An editor is not a fourth module:
it produces something the three consume, and it earns a tab only because it is
too big to live inside one of them. Rivens is the first
(`/weapons/<Name>/rivens`) — what it produces is a MOD, which is why a riven
equips, searches and gets optimized through the ordinary pool with no
riven-specific code in any of the three. A new tab has to pass that test: name
what the three do with its output, or it belongs inside one of them.
Preset collections are domain-named `<owner>-<collection>`, where the owner is
a module — or an editor, and an editor whose ENTIRE content is one collection
is its own domain (`rivens`). Every durable name (localStorage key, DOM id,
label) derives from the domain. ONE STORE PER COLLECTION
(`wfsim-presets-<domain>`), and each entry carries its `scope` — the weapon,
frame, companion or riven family it is about (`entryScope`); a read filters by
it and a write replaces that owner's slice, so an entry still belongs to ONE
weapon. An owner in the key made a store per weapon, which is a list per weapon
to count, sync, undo and migrate. An entry's own `scope` beats any key it is
found under: `foldOwnerLists` files a per-owner list an older page left back
into the one list, and the sync reads a per-owner `list` the same way
(`syncHome`). Undo remembers ONE OWNER'S SLICE, so undoing a step on one weapon
puts back nothing of another's. THERE IS NO CROSS-WEAPON COPY: what survived a rescope was
the mods every gun shares, since the axes that make a build its own (evolutions,
valence, the assembly, a riven) are exactly the ones the target cannot hold.
A build worth having comes from the board or from a share link. URLs mirror
English wiki page names (spaces → `_`); an internal id appears in a URL only
where the wiki name is not one weapon's alone — two Kitgun slots are one wiki
page and two roster entries, so the lowest id keeps the wiki name and the other
lives at its id rather than at nothing (`urlSlug`, and `url_slug` in
`build_site_app.py`, which must stay the same rule).

**A REPORT OF A MODULE IS A BLOCK OF THAT MODULE.** The Shapley analysis
(部件价值分析) reads the build and the fight and produces a report, which is
the opposite of an editor. So it is a block of the Simulator tab, shut until
opened, and never a tab of its own. `docs/SHAPLEY.md`.

**…EXCEPT A REPORT WHOSE SUBJECT IS NOT THE BUILD ON SCREEN.** The Riven
Analyst (`/weapons/<Name>/riven-analyst`, `21-riven-analyst.js`) reports on
every riven the weapon's board has measured, so it belongs to no one module's
open build and has a tab.
Its numbers are the headless query's (docs/BOARD.md §"The Riven Analyst").

---

## The builder's steps run in the order a build is made

**MODE, PARTS, MODS, ARCANE, VALENCE, EVOLUTION, WIELDER** — how the weapon is
used first; then the parts that make a modular weapon a weapon at all; then
what every weapon has (mods, arcane); then what only some have (an adversary's
valence before evolutions, so an adversary Incarnon reads in order); and who
holds it last. The order is the markup's, and `builderSteps()` reads it, so the
numbering — and anything else that lists a build's parts, a share card among
them — follows the page rather than a second list. A block a weapon does not
have is hidden and skips no number.

## The wielder is the build's; external bonuses are the fight's

**A WEAPON BUILD NAMES WHO HOLDS IT** — the Wielder block, first in the builder:
one of the Warframe page's saved builds, linked by its preset `id`
(docs/WARFRAMES.md §A weapon's wielder). It is NEVER EMPTY: the floor is the
Prototype frame, which has its own builder page and saved builds like any other,
and with nothing written for it is stored and sent as no wielder. The top
control chooses the TYPE (a searchable list of Warframes with their faces); which
preset of it is the framed page's own bar's.

**THE ONE WHO LINKS CHOOSES; THE LINKED ONLY SAYS SO.** A weapon build names
which preset it holds — the Wielder's second control — and choosing a preset on
the frame's own page moves no link. That page marks each preset with who links
it (`usedBy`), read off the links themselves. Deleting a linked preset says who
is affected and takes a second click; their links land on the DEFAULT, the
frame's read-only blank, which is offered in the picker only once the frame owns
a preset or a link means it.

**A ROBOTIC WEAPON'S HOLDER IS A COMPANION HOST**, never a Warframe: its Wielder
offers the hosts and a preset of one, and frames the host's own page
(docs/WARFRAMES.md §Companions). The same rules govern the link, and the host's
own bar lists its presets and marks who links each.

**THE BUILDER IS THE COMPLETE EDITOR AND THE SIMULATOR IS THE SUMMARY.** The
Builder's Wielder block frames the Warframe page, and the Operator page inside
it. The Simulator's build card shows the same configuration read-only — the
Warframe with the Operator nested under it — each with a link to the page that
edits it. Warframe abilities cast by others, the squad and the controller's
buffs are the FIGHT's, outside every build, and stay below the summary.
A weapon a frame summons offers only its frames, and its title follows the frame
("Valkyr Prime Talons").

**UNDER THE SUMMARY, WHAT THE FRAME DOES** (`65-sim-frame.js`). Every
conditional node of the linked Operator's school is one row: OFF, ASSUMED UP, or
SIMULATED where the fight can earn it (docs/BUFFS.md §The Operator's actions).
The row stores nothing: OFF and ASSUMED are the Operator build's tick, written to
that build, and SIMULATED is the action in the scenario's list (`sim.apl`),
which the same block edits — order, `once` or kept up, and the ability that
summons the weapon. The blank Warframe build links the first Operator build, as
an unset link does anywhere. The wielder's arcanes a fight runs are listed with
what earns each; Molt Augmented's opening stacks are written to the Warframe
build's arcane pick, so the blank build cannot set them.

**ABILITY STRENGTH IS THE WIELDER'S** unless the fight types one over it:
`sim.ability_strength` is null by default, the box shows the wielder's own as its
placeholder (`panelWielder.ability_strength`), and a stored 1 — the neutral
value an older page wrote for everybody — reads as null.

**THE FIGHT'S "EXTERNAL BONUSES" SECTION IS WHAT OTHERS HAND THE WIELDER**: its
state, the squad's auras, the fight's own stat bonuses — and a tick-and-number
override per stat (health, shields, armor, energy, sprint). Unticked, the stat is
the wielder's, as the server resolved it on the last panel (`panelWielder`);
ticked, the typed number replaces it. There is no frame picker and no shard
socket in a scenario any more: both are the linked Warframe build's.
