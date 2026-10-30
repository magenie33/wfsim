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
build finder for a board build, the bar's own list for a ruler.
**EDITING A READ-ONLY ENTRY FORKS IT** (`lockReadOnly`): every region of the
editor that the entry owns shows a not-allowed cursor, and a click anywhere in
it makes your own copy and opens it — the same copy as ⧉. A control that is only
disabled swallows its click silently, which reads as a broken page. A bar's
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
The scenario is global and edited in its original Simulator section. Builder
keeps Quick calc and links to that scenario editor; Optimizer reads the same
fight. Every edit reaches the same global state, including buff settings.
An official ruler stays read-only; its copy action creates an editable scenario.

**NOTHING CROSSES BETWEEN WEAPONS — EXCEPT THE FIGHT, AND A RIVEN WITHIN ITS
FAMILY.** A BUILD and a SEARCH are statements about ONE weapon and are never
born from each other: a weapon opened for the first time gets a blank build,
the search's `finalists` resets, and the previous weapon's optimizer RANKING
is cleared rather than left on screen under the new weapon's name.

A SCENARIO is not a statement about a weapon, so it is SHARED across the
roster — one list, key `wfsim-presets-simulator-scenarios` with no weapon in
it (`SHARED_DOMAINS`), and switching weapons keeps the fight you are measuring
under. Headshot % remains the global document value; the server forces 0 where
the weapon cannot headshot, and the whole-fight view explains both values.

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

A published build's riven is read-only too: opening it resolves its immutable
specification without saving a custom. Its reference names the riven family
and specification, independently of its display name. Deleting an owned riven
does not affect it. Copying the build, including the first edit that forks it,
makes an owned riven with a fresh id and points the copied build at that id.

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
items" in the topbar's settings menu, and on `/account/sync`, exports every
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

## Board selection

Every board entry point selects a fight, enemy strength, operating conditions,
then other conditions. The default is Standard Single Target, Lv 9999 Steel
Path, 100% headshots while aiming, and standard conditions. Names and options
come from the same benchmark metadata. A full path names the selected cell in
pickers, scenario presets, task details and reports.

Changing a dimension preserves compatible choices in hierarchy order.
Unmeasured cells remain selectable and show an empty result. Board ids and
published measurements retain their identity. The official-scenario popover
uses the same picker; the Forma planner groups its multiple selections in the
same order.

The benchmark overview ranks each weapon once, using its highest measured score
across the allowed modes and the selected build filter. It shows no mode badge.
The link retains the winning build's mode and riven group. Detailed builds,
admission thresholds and retention remain grouped by mode.
Primary and secondary Kitguns are separate weapon entries in search, the fight
roster and the overview. Same-named weapons in different slots include the slot
in their names. Each Kitgun entry takes its own highest score across modes and
assemblies; its configuration page offers only its own parts.

## Build Library

The weapon root is its Build Library; `/builder` edits the active build.
`/build-library` and the weapon's `/benchmark` are aliases of the library.
Existing root links carrying a shared build or board build open Builder;
report parameters retain their Simulator or Optimizer destination. A library
query carries `view=library`, separating its board conditions from a legacy
link that opens a board build. The global `/benchmark` compares weapons.

Board conditions, filters, sorting and results occupy four sibling cards in
that order. Each has its own title, border and spacing. Equipment search,
active conditions and Overview belong to filters; ordinary and advanced sort
controls belong to sorting.
Ordinary sort, priority fields and directions use the site's shared dropdown.
The shared board selector sets the comparison context independently of filter
editor mode. The results toolbar pins its count, board summary and navigation
back to the control cards.

The library starts on the benchmark marked `primary`: Standard Single Target,
Steel9999, aimed headshots and Standard conditions. It does not inherit the
active build's ruler. Mode starts at All. A query selects a ruler, modes,
riven presence, required/excluded equipment, refused triggers, a minimum
published score and a maximum unpolarized MOD drain. The drain uses the
build's actual slots and ranks, with Exilus included and stance excluded;
it is not a Forma estimate.

Static attribute bounds cover critical chance, critical multiplier, status
chance, multishot, fire rate and magazine size. Values include only permanent
bonuses, resolved by the engine for the mode's main attack form (the transformed
form for a cycle). A mixed card keeps its permanent half; aiming, stack, hit,
kill, player-state and target-state bonuses do not enter these bounds. Such
parts remain searchable by equipment. The batch endpoint `/api/build/static`
resolves builds without a fight; the page requests it only for active bounds,
keeps results by engine and full build identity, and reports unavailable values
separately from zero. Bounds and equipment conditions compose in one query.

Filters and sorting have independent Simple/Advanced switches. Both edit
one query: a Boolean filter tree and an ordered list of sort keys. Simple
selections project onto top-level AND conditions. Conditions that cannot be
projected remain active as a retained-advanced summary and an explicit notice;
additional simple filters join the original expression with AND. Custom sort
priorities remain visible in a summary beside the ordinary sort control.
Switching editors changes no rule; choosing a new ordinary sort replaces the
sort list. Each area's text editor can change only that area's query part.
Text is parsed as data, never executed; invalid text leaves the applied query
intact and reports its error inline.

The expression language uses field ids, numeric comparisons, quoted values,
AND/OR and parentheses; AND binds more tightly than OR. ORDER BY supplies
sort keys in priority order; without it, score descending applies. Attribute chance fields use percentage points;
critical multiplier uses the multiplier itself. For example:

```text
crit_chance <= 50 AND (status_chance >= 100 OR equipment = "mod:galvanized_diffusion") ORDER BY score DESC, crit_chance ASC
```

The language is the URL's `filter` value. Board context and retained depth stay
outside the expression: a score is comparable only within its ruler. Static
attributes are calculated only when a comparison or sort needs them; unknown
sort values stay last regardless of direction.

Sorting offers score in either direction, calculation date and base MOD drain.
Same-score alternatives retain their groups, with the selected sort's first
member as the representative. The first twenty places render immediately;
scrolling appends twenty more automatically. The results toolbar keeps the
board context, result count and navigation in view; detailed filters expand
on request in their own card. Back restores loaded places and position.
Query conditions and sorting travel in the URL; opening a result enters
Builder with that build.
Restoring the same weapon's editor state leaves its Library address intact,
including while published board rows arrive. Only the Library writes its query.
Changing conditions does not scroll to the results; only explicit navigation
and restoring a saved view move the viewport.

Refreshing conditions parses equipment and triggers only for the selected
ruler. Cached equipment and trigger data expires with its builtin collection or
trigger metadata;
published row arrays are replaced when data arrives. Search vocabulary still
covers the weapon's full board and is built when searched. Tie alternatives are
derived when displayed, in the preferred member order even under another sort.

Summary is a compact, expandable overview of the selected ruler and mode,
independent of equipment filters. It quotes published best builds, the
riven/free comparison, coverage and modelling admissions. Each quoted build
opens its record in Builder. The same headless renderer writes the page's
static HTML, so the search-readable summary uses the same published facts.

**NOTHING IN IT IS A THRESHOLD SOMEBODY CHOSE.** Every number it shows is a
count or a score off the board: no "just as good" band, no "core" cut, no
colour for how bad a gap is — a gap is green or red by its sign alone. A
judgement written into the page is one more constant to keep true for every
weapon and ruler, and the reader can draw it from the counts.

**THREE PARTS, IN ORDER OF HOW OFTEN THEY ARE WANTED.** The OVERVIEW, folded by
default (fold `finder-overview`): per module, equipment tiles use existing
images and show matching counts, board-wide counts and the best matching rank.
The selected board fixes their vocabulary and order; filtering keeps zero-count
tiles and selected conditions visible. A click cycles through "must have",
"I don't have" and clear while keeping the tile at the same viewport position.
The LIST. And a row OPENED in
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
ORDER THE BUILD WAS SAVED: elements combine in slot order, so a row sorted any
other way can read as another element build. Aura or stance, main mods and
Exilus each occupy a separate row, in that order. Slot position decides the
row, so an Exilus-eligible mod in a main slot remains among the main mods.
The riven rides in the mods with its own section below: official stat text comes from
`/api/riven`, with each actual roll. The section heading is simply "Riven".
Shared riven displays put spliced bonuses first and the malus last, keeping
each stat's stored slot and roll intact.

**A TIE IS ONE ROW.** Builds on one exact score share a place (BOARD.md §"A
place is a score"), and the finder lists a place once (`tieGroups`): its
preferred build (`preferredBuild`), a badge saying how many builds tie, and
the POSITIONS they differ in with the alternatives at each — the mod set, the exilus, each arcane
slot and each evolution tier is a position, so two tier-2 evolutions are
alternatives and a tier-2 and a tier-4 never are (`boardRowParts`). When
several positions vary, the line says whether the measured builds are every
combination of those alternatives or only some of them, because a tie is known
only between builds that were measured. The tie opens into every exact build,
each an ordinary row with what it changes against the first marked. "Show more"
counts places.

**A TRIGGER CAN BE REFUSED.** Under the search box, the triggers the scope's
builds ask for, in the fight's own words and groups (`/api/meta.buff_triggers`,
the switches a scenario turns off): refusing one drops every build with a part
that asks for it, and a group header refuses all of its own. Which parts ask is
`/api/meta.part_triggers` — a part's buff cards by the panel's own reading, a
`trigger:` or `condition:` its data states, or an effect that fires on an event
and draws no card (`EVENT_KINDS`, `webapi/src/buffs.rs`, ratcheted by
`every_event_kind_is_read`). The words are precise: refusing weak-point kills
keeps every card fired by any kill. Each toggle counts the builds here that
carry it and names their parts on hover. The refusal is the reader's and is kept
across weapons (`wfsim-finder-triggers-off`): it says how somebody plays.
A card that is only stronger on a weak point (a headshot multiplier) asks for no
trigger and is never refused this way.

**A BOARD BUILD IN THE BAR IS KEPT BY WHAT IT IS.** A builtin id ends in its
position, and a rescore renumbers the board, so the bar stores the build's identity
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
a signed link's mark, and the figures they tick — points and their place on the
weekly, monthly and all-time rankings — read from `/api/account/devices` for the
account signed in. It is offered only to an account named on the ranking with
points, the consent the ranking already asks; before the sharer picks, it shows
points and the best place, a tie going to the longer ranking. With it on, the
foot does not name the sharer again. The contributor rank is filled and a paid
mark only outlined: what was computed reads first.

**A THEME IS TOKENS, NEVER LAYOUT** (`CARD_THEMES`): colours and fonts. The
blocks and their order are the build's.

## The support page

Every figure on `/support` is COUNTED, never typed: `PROJECT_FACTS` is written
into `app.js` by `build_site_app.py` (commits and the first commit's day) and
everything else comes from `META` and `BOARD_META`. A figure is claimed only if
a reader can check it against the public repository.

The first figures have two scopes: this browser's simulations and fights,
then player-uploaded builds and scores verified by players' devices. No local
history means no personal figures. Submissions are one shared pool and use the
maximum across rulers; verified player scores use the same tally feed as home,
without official scores or extrapolation. One visible-page read per minute
refreshes the tally; a failed read keeps the last confirmed numbers.

Numbers animate on entering view and when their measured targets change,
stopping at those targets. Reduced motion displays final values immediately.
Personal history never joins an account or leaves the browser. Repository
counts and the method remain under the expandable method section; home links
to `/support#method`.

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
| `EXT.pages[kind]` — `path`, `view`, `title`, `nav`, `open`, `settings`, `available()`, `load()`, `render(account)`, `shown(main)`, `entry(account)` | a page in `#auth-page`, routed by `authKindOf`: `open` for anyone, `settings` among the account's pages and in their navigation while `available()`; `view` is its `app.view` kind; optional `entry` returns `{path, label}` for the account menu, or null |
| `EXT.strings[lang]` | `tr`, after the page's own table |
| `EXT.hooks.act(el, kind)` | a `data-auth` action `authAct` does not know; true when taken |
| `EXT.hooks.deleteNote()` | what else deleting the account ends, under the danger zone |
| `EXT.hooks.support(slot)` | the support page's `#ext-support` |
| `EXT.hooks.accountLoaded(account)`, `footer()` | refresh extension state after the account read, then fill the footer's `#ext-footer` |
| `EXT.hooks.shareSigner()`, `shareWho(id, sig)` | signing a short link, and naming who signed one |
| `EXT.hooks.contributorMark(mark)`, `contributorRow(mark)` | a named ranking row's mark, and the class its row takes (lowercase words only) |

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
So each editor box shuts from the header it already has: a `.block` from its
`.bh`, a `.fold.sect` from its `h3.sim-h`. Collapsed, the
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
included. Build Library keeps its results open; its Summary and equipment
overview ship shut, and detailed filters expand from the toolbar.

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

Mouse wheels scroll the page over an inline arena by default. Enable scroll
zoom opts into canvas zoom; Escape or leaving the inline arena turns it off.
Expand editor moves the live target container into a large dialog and enables
zoom there. Panel controls independently hide tools, enemy information,
selection details and measurements. Closing restores the container and page
scrolling. Field edits keep their ordinary scenario bindings, and official
scenarios remain locked. Navigation closes the editor before changing pages.

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

A scenario is one global document. Current context and All settings are
projections of the same controls; search covers the complete vocabulary.
Battlefield, weapon use, external bonuses, starting buffs, simulation rules
and measurement are its sections. Global inputs remain editable when a weapon
forces a different effective value; that consequence is shown beside the input.
Arch-Gun deployment and per-class rules are available from every weapon under
All settings. Technical details read the full document without a second editor.
Quick calc preferences stay in Builder; run counts stay in the bottom run block.

The global buff catalog is the union of the existing legal scope enumeration
for every weapon. Equal ids retain their metadata variants and applicable
weapons. The current build's metadata wins, then that weapon's potential
variant; other weapons' cards label differing caps and defaults by source.
One buff id has one saved configuration. Rendering neither seeds defaults nor
clamps stored stacks; the engine applies the current source's cap. A disabled
trigger changes the effective result, leaving global inputs editable.
All settings can introduce a buff that the current weapon cannot grant.

The compact sticky shell and one-row page tabs stay available while scrolling.
Long tab strips scroll horizontally; selecting a route reveals its active tab.
Local sticky bars use the measured shell and tab heights and compact in that
space. Weapon artwork and large titles remain in normal document flow.

Simulation and optimization keep their bottom run blocks and original result
views in their modules. Each run block has its own heading in the jump menu.
Scenario and run cards use the same default width as the build card above.
Side shortcuts click those same run controls and mirror their disabled state;
they introduce no separate task state or result renderer.
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

## Task center

The top-right task center observes simulation, optimization and saved-riven
appraisals submitted from their existing pages. It owns progress, waiting
order, cancellation, retry and report links (`docs/TASKS.md`). Quick calculation
occupies one replaceable automatic item. Work for other players is identified
separately. The center contains no new-task or configuration form.

The saved-riven list selects one or several editable cards and a ruler for
appraisal. Text import feeds the same saved cards as the editor and screenshot
reader. Each batch item retains its own failure, retry and saved report.

## Results

**A RUN IS A RECORD, WRITTEN ONCE** (`35-results.js`): the build and the fight
as they were (`build`, `fight`, and `key`, which joins them), the engine that
measured them (`engine`: a digest of the code and data a number is computed
from, translations, prose and comments excluded — `engine_id` in the site build), which preset and scenario it came
from, and the summary `r`. A preset holds no result: "this build's number" is
its newest record (`resultLatest`), so a second run does not erase the first.
Every run is recorded; each build keeps its newest `RESULT_KEEP` unpinned
records per scenario, and a pinned one (`kept`) until it is deleted. A run
without a saved build belongs to its task, and is not attached to another
blank build. Its result is shown on the page only while the inputs match.

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

## The compute page changes in place

**/compute IS WATCHED, NOT READ ONCE.** A volunteer leaves it open for hours,
so the block that moves — today's tiles, the task being computed, the list of
recent tasks (`#rt-live`, 73-compute.js) — is drawn once and changed in place
from then on. A redraw of the page replaces every other block around it and
never takes it out of the document, because a node moved restarts every
animation in it. A progress tick changes that block alone.

- A new task's card slides in and the cards under it glide down; a card whose
  state changes keeps its place and changes colour.
- A confirmation flies its points to today's gold tile, which counts them in as
  they land; several land one after another.
- The task being computed holds its finish, full and green, before it drops
  into the list, even when the next task has begun.
- A search (a riven gain) shows its starts as lanes in the optimizer's own
  words, and opened, the build it concluded on as the builder's card.
- With reduced motion asked for, only colours and numbers change.

## Compute: solo and together

The page names its two shares SOLO and TOGETHER (自己算 and 一起算), never
"help" or "community computing", and says DEVICE, never computer, phone or
tablet, so a new kind of device changes no word. They are two sliders, 10–100%,
together with its own switch and the battery choice beside it: one control
(`computeSharesHtml`) in the settings menu and on `/compute`, painted in place so
a slider being dragged is never redrawn. It shows percentages of processing
power, never core counts. The rules behind it are docs/BOARD.md §"Contribution"
rule 0.

A device is first called what its system, browser and — on a phone — maker and
model say (`deviceGuessReady`: ua-parser-js, pinned in `web/lib/pins.json` and
served from `/lib/` like the OCR runtime, plus the browser's client hints for a
model the user agent hides); the owner's own name always replaces it.

## A page that is not a module is a shell page

**A PAGE THAT IS NOT A MODULE IS A SHELL PAGE** — /support, /benchmark,
/utility and /download. It belongs to no weapon, so it sits beside the home grid rather
than under `/weapons/<name>`, and it is not a fourth MODULE: it produces
nothing the three consume. /download is the offer for the Windows client, a
PAGE that answers what a downloader asks — what SmartScreen does on first run,
why the program is unsigned, what updating costs, what uninstalling means,
where the source is. THE ONLY ENTRY IS THE TOPBAR'S SETTINGS MENU, and no
surface of the site offers a download anywhere else. `check_downloads` asserts
the home page offers nothing on every user agent it drives, and that /download
is the one surface telling a Mac, Linux or phone reader it will not run there.
See `docs/DESKTOP.md`.

## Page titles

**EVERY TITLE HAS ONE SHAPE**: `<subject> — <what it is> | WFSim`. The brand
ends every one, so a reader learns one mark from every tab and result; a page
whose subject already names it (/support, /download) does not repeat it.

**THE SERVED TITLE IS BILINGUAL.** A page ships in English, while most of its
readers search in Chinese, and Baidu reads only what is served. So the site
build (`build_site_app.py`, `titled_page`) writes the English, then the
Chinese a search types: a weapon's Chinese name from `names.yaml`, and 配卡
(the players' word for a build) and 星际战甲 from `ui.yaml` — a missing
overlay fails the build. The home page carries the brand in both scripts,
`WFSim (WF模拟)`.

**THE APP KEEPS THE SERVED TITLE ON THE PAGE IT WAS SERVED FOR**
(`servedTitleKept`): Google indexes the title it rendered, so replacing it on
boot would hand the crawler a one-language title. Every page reached inside
the app takes the same shape in the reader's language. A URL meant to be found
or pasted gets its own served file (`shell_pages`): a path left to the SPA
fallback previews and indexes as the home page.

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
| Reminders also makes one from nothing, for what is not open: one kind at a time, each attribute Any until picked, offering only what can occur beside the other picks — a fissure's combination of list, era, planet, mission and node as the game has opened one (the worker logs every fissure it sees; a node's planet is the node's own), an arbitration's as a node of the schedule holds them | `fissureBuild`, `fissureLog`, `arbitrationBuild` |
| a reminder for a kind known ahead says when it fires next instead of how many are open | `reminderStatus`, `arbitrationNext` |
| a fissure reminder says how often one like it opened in the last 7 days, and with none open, how long ago the last did — counted from the same log | `fissureStats`, `SEEN_WINDOW_MS` |
| a saved reminder nothing can ever match says so in red — no fissure the game has opened holds its picks, or no node of the arbitration schedule does | `possible`, `fissurePossible`, `arbitrationPossible` |
| what a new reminder already matches is seen, so it fires for what opens later | `reminderAdd` |
| reminders sync with a signed-in account as a pool of their own; they fire on each browser while the site is open there, on any page: once per item, never for one open when the reminder was made | `reminderCheck`, `syncPool` |
| what fired is said the way every notification is (§"Notifications"); none in a card while the Reminders tab is open | `reminderCheck` |
| a reminder set quiet (its Pop up chip off) fires into the bell only: no card, no system notification | `reminderQuiet` |

## Notifications

**EVERYTHING THE SITE TELLS THE READER IS IN ONE PLACE, AND NOTHING COVERS THE
PAGE** (`49-notices.js`). Three sources: reminders that fired in this browser,
what the site said (docs/ACCOUNTS.md §"Notices"), and the account's state while
it lasts.

| rule | where it is held |
| --- | --- |
| a bell on the bar for every reader, beside the account and outside the phone menu, its count what is unread | `renderBell`, `#rem-bell` |
| its panel and `/notifications` list all three, the account's state pinned on top and marked, the rest newest first; filter chips All / Reminders / Messages | `noticeRows`, `noticeFilterHtml` |
| opening either reads everything, and the rows still mark what was new; a reader with no account keeps the mark in this browser | `noticesRead`, `NOTICES_READ_KEY` |
| as one arrives, a card in the bottom corner, above Nona's button, says it and leaves after 8 s, held while the pointer is on it; at most three, and more at once is one card that counts them | `noticeCards`, `TOAST_MS`, `TOASTS_MAX` |
| a message arriving is said in a card once, on the first browser that sees it; a browser's first answer only sets the line, so a backlog is counted, never a stack of cards | `noticesAnnounce`, `NOTICES_SAID_KEY` |
| per category, the reader turns the card and the system notification on or off; the system one is off in a browser until asked for (the browser asks), on in the desktop client, and comes only while the page is out of sight | `notifyOn`, `noticeSystem` |
| the desktop client shows a Windows notification, and clicked it brings the window back on the page it names | `notify` in `desktop/src/notify.rs` |
| the site is asked once the account is known, then every 15 minutes while the tab is on screen and never while it is not | `noticesLoad`, `NOTICES_EVERY_MS` |

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
page and two roster entries, so both use their full ids. A shared wiki-name
route remains an alias for the lowest id (`urlSlug`, and `url_slug` in
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

## A category the builder does not hold yet

**EVERY PLACE IN THE GAME THAT TAKES A CARD HAS A CATEGORY**, built or not. A
slot in `data/equipment_slots.yaml` with `holds: placeholder` gets one card on
the home grid and a page at `/<its name, lowercased>` (`/arch-melee`), and the
page lists the cards the category takes from `data/catalogue/<slot>.yaml`
(`/api/catalogue`), in DE's own words, under one line saying nothing there is
simulated. That file is generated by `scripts/catalogue.py` from DE's Public
Export and carries no effect: building a category means giving its slot a real
`holds`, moving each card it models into a modelled directory, and deleting its
catalogue file. The address is the game's name, not the id, so it stays.

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
own bar lists its presets and marks who links each. A seated card says whether
the fight runs it — **In the fight** on the one kind that does (Fired Up), the
panel's ⊘ on the rest — and the weapon sends the linked build's cards with their
ranks, so a rank step on the host moves the weapon's numbers.

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

Independent content cards share browser layout preferences: dimensions and
order within their module. Edit layout reveals resize and reorder handles;
keyboard arrows support both operations. Restoring the default layout returns
cards to their authored slots and natural dimensions without changing builds
or scenarios. A module's layout is shared across weapons. The jump index reads
the resulting DOM order. Fixed navigation remains outside the movable cards.

Warframe roots open Build Library; `/builder` opens the existing editor.
`?build=` and embedded links keep their editor intent. The library lists this
browser's saved frame builds through the shared build card renderer and links
each record to its existing editor. It does not claim published board results.
