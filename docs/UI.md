# wfsim — UI Vision

What sets wfsim apart from predecessor calculators (Overframe-style form
pages): besides a build/config UI, there is a **live 2D top-down view of
the fight**.

## Kill score is reported as a RATE (KPM)

The kill score — whole kills plus the fraction of the current target's pool
already drained — grows with the engagement, so two runs of different length
could not be compared at a glance. The headline is now **KPM**, score per
minute, and the score itself sits beside it as the engagement total. That is
the same shape the damage numbers already had: a rate to compare with, a total
to read.

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
| the player | `tenno_data::Tenno` — stats, and a `state` every conditional mod is asked about |
| target that never wastes DPS | `dummy::TargetMode::InstantRespawn` |
| aim quality / headshot feel | `dummy::BodyPart::aim_weight` |
| plane, positions, ranges | **nothing yet** — see below |

**The Arena VIEW has no engine behind it.** There was an `engine::world`
(`Vec2`, `Circle`, an `Engagement` of shooter-vs-target-circle with a hard
range cutoff) written alongside these decisions in 2026-07-24. It was deleted
on 2026-08-02 with **zero callers**, having never been wired to anything: the
sim fights one target and assumes it is in range, so a plane had nothing to
decide. Two modules named after the same thing, one of them dead, is worse than
one honest gap — and the decisions above are the part worth keeping, which is
why they live here and not in code.

When positions become real they belong ON `arena::Arena`, beside the actors
that would have them, not in a parallel module.

## Replay

The Simulator's result carries the MEDIAN engagement, frame by frame: the
target's pools, every counter the panel reports, the damage meter's own
composition, and **live stacks per buff**.

**It sits at the TOP and drives the whole panel**.
The panel renders once at its finished state — hero, KPIs, damage meter, DPS
curve, detail — and the replay re-reads all of it at whatever instant the
cursor stops on: the headline recounts, the KPIs recount, the meter
re-composes against the damage dealt SO FAR (a composition of a fight in
progress is read against that fight, not against its end), both curves grey
out everything past `t`, the pools refill.

Its own heading is the word "Replay" and nothing else — a transport control
does not need explaining, and the sentence that was there took a line from the
thing it was describing.

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

It is the same fight the headline number came from, not a fresh run and not an
average. `Rng` is SplitMix64 with a single `u64` of state, so a run records
what it started from (`RunResult::rng_state`) and `dummy::replay` re-runs that
one bit-for-bit. Cost: ONE extra engagement, and only when asked — the
marginal-gain scan calls the same endpoint once per candidate and shows no
replay, so `replay: true` is opt-in and only the Simulator's Run sends it.

Why it earns its space: it turns arguments into pictures. "Is Primary Frostbite
pinned at 40 stacks or decaying?" was a paragraph of reasoning; it is now a
curve that climbs 0 → 40 over sixty seconds and answers itself.

## Presets and customs — two kinds of collection

**PRESETS vs CUSTOMS** — two kinds of collection, and the difference is who
CONSUMES them. "Preset" is the CATEGORY and never the name of a collection or
of an item in one: a build is a build, a scenario a scenario, a search a
search, a riven a riven. Each bar declares its `noun`, which names new items
("build 2") and every tooltip that refers to one.
A **preset** is a saved state of something that always exists, read only by
its own module: `builder-builds` (a build), `simulator-scenarios` (a fight,
buff settings included), `optimizer` (a search: the SCOPE and `finalists`, and
nothing else — never buffs, never a run count, never a thread count). There is
always ≥1, "active" means the state you are in.

THE OPTIMIZER TAB IS TWO HALVES AND TWO BOXES: one box is the SEARCH and is
exactly what a search preset saves; the next is the SIMULATOR's fight,
read-only, edited there. What sits OUTSIDE both boxes is in neither preset —
the final round's run count is that thing. It is a PREFERENCE with a key of
its own, TYPED rather than defaulted from elsewhere, saved by no preset and
pinned by no ruler. The cost is stated rather than hidden: the two counts can
differ, so a winner may be crowned at a precision the replay will not use —
and the ranking already reports it, marking a row `≠` when the two disagree by
more than 4σ.

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
chip bar, and the key is `wfsim-customs-<weapon>-<domain>` /
`wfsim-custom-open-…`. Everything below the key is shared: storage, undo,
per-weapon scoping, ⇤ import.

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
rulers handle it: the SERVER forces 0 on a weapon that cannot headshot. A
shared bar offers no "⇤ import" — there is no other weapon to import from.

**NOTHING OUTSIDE A COLLECTION WRITES ITS STATE.** A build carries no `sim`
snapshot: a build is a build, and the live scenario is seeded from the active
`simulator-scenarios` entry and from nowhere else. "What this build was last
measured under" is `lastResult.key`, which lives outside `state` and is what
makes a stale result show as stale. Every collection writes through
`storePresetList`, which is what makes one Ctrl+Z stack cover all four.
Customs are OPTIONAL by nature: nothing is auto-created, the last one can be
deleted, and the editor stands down instead of showing a document that is not
there. Presets are not — the modules behind them always have a state, and "no
build" is not something the builder can show.

## A share link reproduces the whole thing

**A SHARE LINK reproduces the whole thing**: `/weapons/<Wiki_Name>?b=<code>`
carries the build, the RIVENS it equips (a custom exists only on the machine
that made it, so it must travel inline), the scenario it was measured in, and
the measurement itself as the sharer's claim. Opening one creates a NEW copy
of each — never a merge, never an overwrite — repoints the build's riven ids
at the copies, strips the query so a refresh cannot import twice, and says
what it dropped. The payload is POSITIONAL and omits everything derivable
(defaults, max ranks, a buff left at its own default, the shape drafts a riven
regenerates).
A v3 link names an id by its place in `data/share_order.yaml`, which is
APPEND-ONLY and held there by a ratchet — `engine::share_order` recomputes the
generator's digest over the whole list and fails on anything that is not an
append, so a reorder is a red test rather than a link that quietly opens
somebody else's build. It is worth 3.4x: the same Laetum is 279 characters as
slugs and 79 as indices. The v2 array is still the one internal
representation, so `importShare` and everything below it are untouched; v1 and
v2 links still open.
AND v3 IS PLAIN TEXT IN THE URL. At 79 characters deflate makes the payload
BIGGER, so the text goes in raw; the separators are RFC 3986 unreserved
characters and sub-delims a query accepts unescaped. A payload it cannot
express — a CLAIM, or a name in a script the URL would escape — falls back to
the deflate+base64 form, so the encoder measures all three and takes the
shortest.

A NAME THE SHAPE IMPLIES DOES NOT TRAVEL: a board riven's local name is
`boardRivenName(shape)`, derived on arrival, which is shorter AND names it in
the reader's own language.
It rides the QUERY, not the fragment — a fragment never reaches a crawler and
these links are meant to be posted. The card (`drawShareCard`, a canvas PNG to
paste into chat) always carries the wordmark and the site's host, and a QR of
the same link. `qrMatrix` is a from-scratch encoder (byte mode, ECC L, mask
0), VERIFIED against a reference encoder's matrices and decoded back out of
the rendered PNG by an independent decoder. It is drawn at a FIXED 8 device
pixels per module — measured: at 4 the card only scans at full size, at 6 it
survives a 0.66x shrink, at 8 it still reads at 1080px wide after JPEG 60,
which is what a chat app hands back. The code's size is therefore an input to
the layout, not an output.

## The page that asks for something

**THE PAGE THAT ASKS FOR SOMETHING ARGUES THE WAY THE REST OF THE SITE DOES.**
Every figure on `/support` is COUNTED, never typed: `PROJECT_FACTS` is written
into `app.js` by `build_site_app.py` (engine tests, browser checks, commits,
the first commit's day) and everything the page can count for itself comes
from `META` and `BOARD`.

A FIGURE IS CLAIMED ONLY IF A READER CAN CHECK IT against the public
repository. A count of in-game measurements cannot be, so it is not on the
page.

THE ORDER IS THE ARGUMENT: what this is and what it holds, why it can be
checked, what the reader has already got out of it, then the door. The
evidence behind that shape and behind the $3 floor is NextAfter's
donation-page experiments (a stated value proposition; video loses),
Wikimedia's banner testing (a facts appeal over a personal one, and a low
suggested amount), Cialdini & Schroeder 1976 (legitimising a small gift raises
participation without lowering the mean gift) and Adena/Huck/Rasul (a higher
suggestion buys a higher mean at the cost of participation).

TWO CHANNELS, TWO JOBS: Ko-fi is the one-off, Patreon the month. THE ACCOUNT
IS THE PROJECT (`ko-fi.com/wfsim`) and the first-person half lives in the
platform's own bio.

A SUBSCRIPTION BUYS ORDER AND COMPANY, NEVER PRODUCT — a channel where the
work is discussed, and reports read first. DE's Content Policy forbids
charging for access to what is made with Warframe assets and permits passive
advertising instead; this project's own promise is that nothing bought moves a
result. NO RESPONSE TIME IS PROMISED IN ANY LANGUAGE: "read first" is an
order, "within N hours" is an obligation that grows with every subscriber.

THE SUPPORTER COUNT IS THE ONLY MONEY FIGURE PUBLISHED, and the store cannot
hold more: `SUPPORT` is one empty KV key per Ko-fi message id with the DAY as
metadata — no amount, no name, no email. A namespace of its OWN, because
`/api/board/pending` counts every key in `SUBMISSIONS`.

WHAT THE READER HAS RUN NEVER LEAVES THE BROWSER. `wfsim-use` is two integers
written by `runSim` and read by `/support` alone; the page says so where it
prints them.

---

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

## Every enemy has a name, and the page can ask about one

**EVERY ENEMY HAS A NAME, AND THE PAGE CAN ASK ABOUT ONE.** `SpreadFoe` is
`{state, debuffs}` per body; `formation::FoeSpec::id` names it, stable across
edits because it travels in the scenario and is filled in BY POSITION when
blank. The aimed body is `e1` and lives on the `Arena` — it is not in the
formation list, it is the fight's own target — so the crowd reads as one list.
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

SCREEN ("+N more took damage and are not followed"), never applied silently.
One body draws no chips at all.

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

## A measurement costs its summary, not its replay

**A MEASUREMENT COSTS ITS SUMMARY, NOT ITS REPLAY.** A REPLAY is 600 frames of
debuff series per followed body plus a hit account per attack part — **65 KB
against the 1.6 KB summary** of every number a card, a share or the board ever
reads, 42x — and one would be stored per WEAPON. About seventy-five weapons
fill a 5 MB origin. Past that `setItem` THROWS in the save path of the run
that just finished, and the reader is told "sim failed: QuotaExceededError"
for a simulation that worked.
So a replay NEVER reaches the disk: `stripReplays` takes it out on the way to
`localStorage` and `resultMem` (keyed by weapon AND preset) keeps it for the
session. A SHED SWEEPS THE ORIGIN, not the list — a quota belongs to the
origin, so `shedOtherResults` walks every `wfsim-presets-*`, oldest result
first, and the list being written is the LAST thing it may touch. AND WHAT IS

ALREADY THERE COMES BACK: `reclaimStoredReplays` strips every replay written
under the old rule on the way in.

## Progress belongs where the work is being read

**PROGRESS BELONGS WHERE THE WORK IS BEING READ.** A pool of ninety mods at a
real run count is tens of seconds of a list that does not move, and a list
that does not move is read as broken rather than as busy. The per-row "…" chip
is a different claim: it says THIS row has no answer yet. `scanStrip` is one
component — a bar, a count, sticky at the top of the list — fed from whichever
scan state that list reads, mounted in all five places a scan ranks something.

## A page that is not a module is a shell page

**A PAGE THAT IS NOT A MODULE IS A SHELL PAGE** — /support, /benchmark and
/download. It belongs to no weapon, so it sits beside the home grid rather
than under `/weapons/<name>`, and it is not a fourth MODULE: it produces
nothing the three consume. /download is the offer for the Windows client, and
it is a PAGE rather than a button because what a downloader asks is a page —
what SmartScreen does on first run, why the program is unsigned, what updating
costs, what uninstalling means, where the source is. Its SmartScreen section
is the OWNER'S OWN WORDING, transcribed from the notes file that ships beside
the binary: the notes answer the warning after the download, the page answers
it before. The home hero carries one LINE pointing at it — that page is read
by someone who has not yet seen the tool work, which is the worst moment to
ask them to run an unsigned executable, and the people who want the client are
the ones already using the site. See `docs/DESKTOP.md`.

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
label) derives from the domain. A preset belongs to ONE WEAPON, so the storage
key also carries it (`wfsim-presets-<weapon>-<domain>`) — DOM ids and labels
stay weapon-free, and copying a preset across weapons is the explicit "⇤
import" action, which drops per axis what the target cannot hold. URLs mirror
English wiki page names (spaces → `_`); an internal id appears in a URL only
where the wiki name is not one weapon's alone — two Kitgun slots are one wiki
page and two roster entries, so the lowest id keeps the wiki name and the other
lives at its id rather than at nothing (`urlSlug`, and `url_slug` in
`build_site_app.py`, which must stay the same rule).

---

## Planned

- **Surface each attack part's CO anomalies in the builder panel**. Condition Overload is full of per-entry quirks that no rule
  predicts — the CO catalog lists them one attack at a time, and weapon families
  split down the middle (Lato Vandal has a row, Lato Prime does not; Zylok Prime
  is docked to 94%, the plain Zylok is not). MECHANICS §6 has the evidence.
  The panel already renders per-part rows, so each part should state its own CO
  standing: the behaviour class, whether that part receives CO at all (an AoE
  part normally does not — the Torid's cloud is an exception), and the base
  fraction with what dilutes it. Today a build can silently differ from another
  by a factor the panel never mentions.

## Not decided yet

- Rendering cadence vs simulation tick (fixed 240 fps sim clock exists in
  `sim::SimConfig`).
- How movement paths (target walking, player strafing) are authored in the
  arena view.
