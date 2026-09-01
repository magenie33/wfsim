# The browser and node checks

`scripts/check_*.mjs`. Each asserts a property of the shipping build; run the
ones a change touches. AGENTS.md lists them by name and one line of what they
assert — this file is the shape of each and why it is shaped that way.

## `check_page_bodies`

`node --check` over every check script. No browser; runs
first in CI.

## `check_parity`

The builder and the optimizer offer the same options, the
same visibility, the same ORDER, under the same numbers and names, on every
axis. It asserts the property rather than a list (`orderOptScope` reads the
order off the builder's own blocks) and SCRAMBLES the sections first, since
markup authored in the right order would pass on a page where nothing orders
anything. Run it after adding a weapon or anything a weapon can carry.

## `check_board_submit`

Plain node against a KV stub, no browser. Every key
`boardPayload()` emits, read out of `app.js`, is a key the worker's `AXES`
table knows how to keep; every key survives into storage; two builds differing
in any one axis are two records.

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
directions. An equip rule is asked of EVERY firing mode and installing a form
ADDS one. The engine decides (`pool_for_build`), `/api/meta` states the
consequence per evolution (`evo_forbids`), and the page acts on it: the picker
stops offering it, installing the form unequips it and says so, the Form
control greys the options with the reason on screen without moving the
scenario's own selection, and the sim refuses the pair. It covers the LOCK the
same families carry ("set to its default ignoring other bonuses, even negative
effects"): the panel pins the stat and NAMES what pinned it, and a buff whose
only grant is that stat is not offered — a lock reaches evolutions, arcanes
and passives, not just mods (MEASUREMENTS M30).

## `check_board_link`

A board row opens THAT row: the build it names AND the
ruler it is on. It walks every ruler and asserts against `BOARD` itself. It
holds a case the live board has never had — ONE WEAPON, TWO MODES — by
injecting a synthetic second-mode row. IT ALSO WATCHES THE ORDER one level
down: the builder's picker groups a weapon's deeper ranks by mode and numbers
each inside its group, asserted over EVERY weapon the board holds in more than
one mode, picking the WORST-INTERLEAVED one for the DOM half. The rank
assertion beside it says #1 is that mode's LEADER, because a position counter
and its rank agree however the list is ordered.

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

## `check_wf_buffs`

A Warframe ability buff is the FIGHT's and reaches the
number: the section draws in both languages under DE's OWN names (战吼,
黯然失色), the card's value follows Ability Strength, ticking one moves a real
`/api/simulate`, two of a FAMILY do not stack AND the page says which one
lost, the optimizer shows the same buffs read-only, and — the negative control
— no RULER carries one.

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

## `check_opt_modes`

Mode is the BUILDER's control and the OPTIMIZER's
dimension. Pinning a mode makes every ranked row come back in it, pooling both
DOUBLES the candidate count, and each row carries the mode it was scored in
into the build it becomes. Server-side a VARIANT is a (mode, evolution set)
pair, which is why nothing downstream had to learn about modes.

## `check_run_counts`

How hard you measure is a number someone can set, in all
three modules, and the answer differs in each. The simulator defaults to the
rulers' 1000 so a first number is comparable with the board without touching a
box (1.3 s a run in the shipping build, against 0.14 s at 100). The quick calc
takes its own with a FLOOR of 10, where a status mod stops being a coin flip
(M24: one run swings it ±39 points), and a number under it is raised rather
than obeyed. The optimizer's final round takes its own too, and it is a
PREFERENCE: typed, never blank, in NEITHER half of the tab and in NEITHER
preset. It asserts the number on screen is the number sent, that the box is
drawn outside both halves, that `snapshotOpt` does not carry it, that
restoring a scope leaves it where the reader put it — and, its negative
control, that the CPU threads box is gone and no `threads` reaches the request.

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
`engine::builds::BUILD_AXES` is the one declaration, served at
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

## `check_slot_ranges`

Every axis says how many of its slots a candidate fills,
in one shape: the mods axis is 8 slots and a number 0–8, every other axis is
ONE slot and 0–0 / 0–1 / 1–1. It walks all three states on all four axes ON
THE WIRE. The range is DERIVED first and adjusted second, so no existing scope
grows; a PIN is not a range (a pinned candidate settles at 1–1 with the inputs
disabled); and 0–0 KEEPS THE CANDIDATES, which is why the evolution ladder
keys on the RANGE rather than the marks. Mode and valence carry the row
read-only at 1–1. The empty choice is a mark like any other — `none`, or
`none:<pool>` on an arcane seat — so the range is a VIEW over the option set
and needs no field in the preset, the request or the round trip. An arcane
costs no capacity and no Forma, so an empty seat can only tie the same build
with the arcane in it: `an_arcane_seat_marked_none_is_not_a_default`.

## `check_stance_capacity`

A stance is an AURA: it HANDS capacity back instead of spending it — five
points, ten on a matching slot — so the number beside the mod slots is the
weapon's own capacity plus the grant. The Magistar's slot is Vazarin, which
makes Shattering Storm free at 70 and Crushing Ruin 65 unless a Forma is spent
on the slot, and all four readings are asserted ON SCREEN.

It is a page check rather than an engine one because the page owns this
arithmetic: `capacityUsed()` and its Forma bill MIRROR `engine::mods`, so an
engine that is right and a mirror that is not still reads wrong to everyone.

It also asserts the AUTO PLAN, because that mirror drifted the same way twice:
planning against the weapon's own capacity and not the stance's grant buys
polarizations the build does not need, and reaches for an UMBRA FORMA to do it —
the one item `engine::mods::fit` is written to spare. The engine answers five
regular Forma and no Umbra for that build; the page has to say the same.

## `check_build_retriever`

**A RETRIEVER'S SHAPE DOES NOT MOVE.** The benchmark bar is four controls —
ruler, mode, riven, rank — and it draws all four on every weapon: with a hundred
board rows or with none, and whether or not any of the four has a second answer
to offer. It grew and shrank before, dropping the riven control entirely wherever
a mode had one ranking, which taught the reader to read its SHAPE as information
— and then "no riven control" and "no riven" were the same picture. A dead
control says "asked, and there was one answer" where an absent one said nothing.

**…AND WHAT IS OPEN IS SAID SOMEWHERE ELSE.** The bar's controls fall back to the
board's leader whenever nothing official is loaded, so a reader on their own
build was shown a ruler, a mode and a rank belonging to a build that was NOT on
the page — including on a freshly opened weapon, which opens no row at all. That
is a sentence the bar was never in a position to say, so `#build-current` says
it, in each of its three states: a board row (read-only), one of your own, and
the unsaved build the page starts on.

**IT MUST RUN AGAINST `site/`.** `board.json` is FETCHED at runtime and the
native dev server does not serve it, so a run pointed at 8787/8799 sees an empty
board and every assertion passes on placeholder text. The first check asserts the
weapon under test has rows, which is what makes that loud instead of green.

## `check_build_size`

How full a searched build must be is a RANGE
(`build_min`–`build_size`): both ends push each other, both ride the search
preset, both reach the request. The floor starts at 0 and is drawn AFTER the
mod list, because how full a build must be is a CONCLUSION and means nothing
until the required and the pooled are chosen. "Nothing marked" is the empty
option, as on every other axis; once anything is marked the DERIVED floor is
at least 1 and wins. A 0 ceiling OUTRANKS the derived floor in three places
that must agree — `min_slots`, the guard refusing pooled mods with no slot to
reserve, and the page's `poolStarved` — or `SubsetSpace::new(1, 0)` reports a
legal request as "no legal builds in this scope". The row says what the marks
raised the floor to, stated only when the two DIFFER. The count is the product
of all six axes.

## `check_riven_pool`

The riven editor offers the stats that weapon's rivens
actually roll, in BOTH slots. THE RULES DECIDE AND THE SURVEY CHECKS:
`rivens_data::derived_for` is the model, `data/rivens/exceptions.yaml`
overrides it per riven FAMILY with the evidence in each entry, and
`data/rivens/pools.yaml` (from `scripts/survey_riven_pools.py`) is read by a
TEST and by nothing else. See DATA_SOURCES §"Riven pools" (MEASUREMENTS M35).

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

## `check_search`

A real optimize in the shipping build: a scope it finished
reports `exhaustive` and says so on screen, a budgeted one reports its
COVERAGE and does not pretend, and the WORKER FLEET covers more ground than
one worker would.

## `check_gain_band`

A quick-calc chip says HOW WELL IT KNOWS its own number
and never prints a zero. The scan reads `score_mean`/`score_se` and
`dps_mean`/`dps_se`, which the server already computes, rather than the MEDIAN
run. THE WIDTH IS THE COMPARISON'S OWN and it is DERIVED: `/api/simulate`
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

The quick-calc gain scan obeys the evolution TIER LADDER,
so it never ranks a perk the builder will not let you click.

## `check_replay`

The median engagement plays back on screen: the buff curves
draw, scrubbing drains the pools, and play advances the clock at the chosen
multiplier.

## `check_preset_independence`

No collection's state is written from outside
it: switching a build must not move the fight, and editing the fight must not
touch a build.

## `check_share`

It opens a share link in a browser that has never seen the
build and asserts what is on SCREEN, not what is in the variables.

## `check_tenno`

The fight's PLAYER reaches the panel, the sim and a share
link, so an arcane that scales off a Warframe is worth nothing with no frame
and +500% with one.

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
and the page asks the ENGINE which (`/api/build/keys` → `builds::board_key`).
A build is not its spelling: `canonical_mods` sorts the non-elementals by
drain and leaves the elementals in the order that PAIRS them, evolutions are a
set, a riven is a shape and not its rolls, and the mod POOL is what tells an
elemental mod from any other — which only the engine has. A MATCH IS PROOF AND
AN ABSENCE IS NOT: the board LISTS only builds scoring at least half their
weapon's leading row, so the page only ever suppresses an upload it can prove
is redundant. Its NEGATIVE CONTROL is the half that matters — a build the
board does not hold must still be offered.

## `check_support`

The page that ASKS for something makes its case in numbers
it COUNTED. A drawn figure and a counted one look identical, so each is
compared against the source it claims: the weapons tile against
`META.weapons`, the mods tile against the union of `META.mod_pools`, the built
line against the injected `PROJECT_FACTS`. The other half is the one line
about the READER — how much they have run here — asserted absent on a browser
that has run nothing, correct after a real run at a run count the check chose,
and absent from the request that run sent. Its negative controls are the
channels (an entry with no url draws nothing) and the supporter line (silent
while its store is unconfigured). It FORCES English rather than inheriting it,
since the app boots into the browser's language.

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
