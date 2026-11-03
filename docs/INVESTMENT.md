# Investment: what has been installed on this weapon

Capacity depends on rank, rank depends on Forma, and the planner is the
engine's (§The planner). The Exilus adapter and the arcane adapter are assumed
installed. An adversary weapon ranks to 40 and finishes at 80, and every
surface that prints a capacity says so. A share link carries no Forma rules:
the rules are the player's.

## A POLARITY BELONGS TO THE WEAPON, NOT TO THE SLOT

Two slots' polarities can be SWAPPED without changing what either slot IS — the
exilus slot stays exilus, it just carries a different polarity afterwards. The
whole Forma model rests on it.

**WHAT IT MEANS FOR PLANNING.** The weapon's polarities are a POOL, not a set of
fixed positions, so `plan_forma_spending` flattens `innate_slots` and matches
the biggest-drain mods against the multiset. That is not an approximation — it
is the rule.

**AND THE EXILUS SLOT'S POLARITY IS IN THAT POOL**, even for a build that puts
no mod in the exilus slot at all. Swap it onto a main slot; the exilus slot
carries whatever came back and sits empty. The polarity is not attached to the
slot, so a build that leaves the exilus slot out of scope still spends its
polarity. The adapter is assumed installed anyway (below), so the slot exists.

**WHAT DOES NOT MOVE.** The exilus SLOT still only accepts an exilus-eligible
mod, and that constraint is about how many mods fit, never about which polarity
goes where. So it does not reach the planner: nine polarities, eight or nine
mods, and the eligibility rule lives in the slot check.

## A COLOUR CANNOT BE PUT IN A DRAWER

The pool is nine colours over nine slots, not a bag with a lid: a colour no mod
wants still sits somewhere. It goes on a MOD-LESS slot, where it costs nothing
and changes no number, or on a modded one at +25% — unless a Forma spent
elsewhere overwrites it, and each one BOUGHT erases one unwanted colour for
free, because the bill is `max(added, removed)`.

So the count that has to sit on a mod is `pool − matched − mod-less slots`, and
it goes on the SMALLEST drains there are. Two rules follow, and they are the
whole of the policy:

- **Never pay a Forma to blank a slot.** The same Forma spent on the mod's own
  colour halves it instead of shedding a quarter, so blanking is never the best
  use of one.
- **Keep a leftover colour only where it is free or where dropping it would
  cost a Forma.** A mismatched slot is worse than a blank one (125% against
  100%), so "more polarities" is not worth anything on its own.

`rules::capacity::plan_forma` and `build::forma::plan` both implement it, and
`check_forma_plan` holds the page to it.

## The mechanics, verified (wiki)

| fact | the wiki's own words |
| --- | --- |
| capacity follows rank | "Items have a limited Mod Capacity, that correlates to their Rank. The maximum rank is normally **30**, but for some items it is **40**" |
| the Catalyst | "**doubles the available Mod capacity**" |
| rank 40 | "max rank caps at 40 after **5 polarizations** (max rank increases by **2 per Forma** added)" |
| the Exilus slot | "any eligible mods used on the slot will **consume mod capacity, like normal mods**" — and the adapter fits "a Primary, Secondary or Melee weapon" only |
| Gravimag | "Allows archwing guns to be deployed in terrestrial zones" — no capacity or rank effect, but it requires a Catalyst already installed |

So, for a gun:

```
max_rank_now = base_max_rank == 40 ? min(30 + 2 * forma_used, 40) : 30
capacity     = max_rank_now * (catalyst ? 2 : 1)
```

30 + Catalyst = 60. 40 + Catalyst = 80.
(The +10 stance bonus that takes a Paracesis to 90 is melee-only.)

Settled measured:

- **An Arch-gun has no Exilus slot at all**, so the question of it consuming
  capacity does not arise. Nor does a robotic (Sentinel/MOA) weapon have one.
- **Arcane adapters consume no capacity.** An arcane is its own slot with no
  drain.
- **The unranked base of 15 does not apply to weapons.** A weapon starts at 30.

**FIVE IS A CAP ON RANK, NOT ON FORMA.** You may polarize as many slots as you
have; only the first five raise the max rank. So eight heavy mods on a rank-40
weapon settle at SIX polarizations: five buy rank 40 (capacity 80) and the
sixth buys nothing but a halved slot, which is exactly what the game does. A
budget is self-consistent when the rank it claims comes from polarizations
actually spent, and spending more than five is never a contradiction.

`scripts/check_valence.mjs` holds the ladder against a real rank-40 weapon (the
Kuva Nukor): 40 / 5 / 80 off `/api/meta`, the builder counting against 80, a
full eight-mod build not shown as impossible, Auto spending the five — and, the
control, an ordinary weapon still at 60 with no mastery Forma.

## The feedback loop, and why the default removes it

On a rank-40 weapon every Forma does two things: it polarises a slot AND adds
2 to the max rank. So "how many Forma does this build need" has a moving
target — more Forma means more capacity means possibly fewer polarised slots.

**The default is to polarise the full 5 times**, because
that is what full mastery affinity requires whether or not the build needs it.
That fixes capacity at 80 before planning starts, so the default path needs no
solver at all. Only the opt-out — "I do not want to spend 5" — reintroduces the
fixed point, which iterates and converges in at most 5 steps.

## The interaction: investment is DERIVED, not configured

The shape:

1. **Every slot is open.** Place mods, arcanes, evolutions freely — the builder
   never refuses on grounds of an adapter you have not installed.
2. **The investment is worked out afterwards**, automatically, with no button
   to press: what Catalyst / adapters / Forma this build would require.
3. **An icon strip states what it comes to** — what is installed on this
   weapon, in one glance.
4. **Switching build re-runs it**, because a different build wants a different
   investment.
5. **If the build cannot be made at all, say so** — the one case where the
   builder has to push back.

So the investment is an OUTPUT of the build, not a second thing to keep in
sync with it. What remains are the player's RULES, which are not the build's —
§The planner lists them.

## The planner

`engine::build::forma::plan` answers for ONE item and ANY NUMBER of its configs,
under the player's RULES. The item is a `Board` (a weapon or a Warframe:
main slots, an exilus slot, and a slot that GRANTS capacity — the stance or
the aura); each config is a `Loadout`.

**THE CONFIGS SHARE ONE LAYOUT.** Polarity is the item's, and every config on
it sees the same slots, while each config places its mods where it likes. So
the answer is one multiset of polarities, plus the exilus slot's and the grant
slot's — the only two that are positional, because only one kind of card goes
there. Planning each config alone and merging does not work: the merged
colours outnumber the slots.

**THE ANSWER IS EXHAUSTIVE.** The alphabet is a bare slot, the colours the item
carries, the colours some card matches, and Omni when the rules allow it; every
multiset over it is billed, and the buckets are walked cheapest first. A colour
nobody carries only ever mismatches, so it is not tried. Positions are free in
that walk, which never under-rates a layout, so it both orders the exact work
and says when to stop. `free_drain` is the positions-free drain, a greedy held
to the Hungarian answer by `free_drain_is_the_optimal_assignment`. `rules::capacity::fit`
stays the optimizer's greedy planner; `one_loadout_bills_what_fit_bills` holds
the two to the same bill on one config.

**THEN THE COLOURS ARE PLACED.** Every config shares the slots' positions, so
what moves is MODS. A card is ORDERED when it bears an element — its place
among the other ordered cards decides what pairs — and a move keeps ordered
cards in order, so no pairing changes; every other card goes anywhere. The
first config's own best placement anchors the colours, and swaps are taken
while they help the worst config, then the total, then move fewest mods. With
**mods stay in place** nothing moves at all: each slot's colour has to serve
what every config keeps there, which is a search over arrangements pruned by
capacity, and it may take more Forma or Omni. Past a work budget either search
returns what it found and says it is not exhaustive.

**THE BILL** is `Σ max(0, target − start)` per polarity, a bare slot counted as
a polarity of its own (blanking takes a Forma), each bought slot billed as the
item that makes it — Omni, Umbra, or a regular Forma. A grant slot outside the
pool (the stance slot) costs one when it changes. No Forma makes the Aura
colour. Mastery Forma are added on top up to the rank floor.

**THE ORDER a layout is judged in**: Umbra Forma (under "when needed"), the
grant slot unmatched (once anything is spent), Forma, Omni, then the WORST
config's spare capacity, the total spare, mods moved, and the item's own
colours moved.

**THE RULES** are the player's and are GLOBAL — one set for every weapon and
every frame. What is per item is which configs are planned together, and the
first of them is always the one being edited.

| rule | default |
| --- | --- |
| Catalyst / Reactor | on |
| reach max rank | on |
| stance / aura slot first | on |
| Omni Forma | never · allowed (only where it saves a Forma) · preferred |
| Umbra Forma | never · **when needed** · allowed (an ordinary Forma) |
| Forma limit | none |
| mods stay in place | off |

A player may also state what the item ALREADY carries (`Start`): that layout
is free, and the Forma it took count toward the rank. The page does not ask for
it yet.

**THE PAGE** asks `/api/forma/plan` and puts the answer in the slots — the
"auto" button and the Forma plan block run the same call, on the weapon page
and the Warframe page alike. The block stands on its own under the build bar,
not inside the mods block: it belongs to the item and to the builds planned on
it, and its rules to every item. The rules live in
`wfsim-forma-rules`; the builds planned with the open one in
`wfsim-forma-group-<item>`, by preset id, where the item is the weapon id or
`warframe-<frame>`. Planning writes the layout into every ticked build. A board
build is read-only, so it is planned alone and offers no partners.

## The optimizer of the plan

`engine::build::forma::optimize` answers the question a plan cannot: what ONE layout
reaches across whole groups of builds, and what each Forma buys. A group is
covered by the best of its builds that fits, as a share of its leader; hard
loadouts must fit every answer. The result is a CURVE — the cheapest layout
for each worst-group share, each point strictly better than the one before —
so the reader sees what one more Forma is worth rather than one number.
Under the same rules as the plan, fixed order included. It reads; it never
writes a build.

On the weapon page it is **plan ahead**, inside the Forma block. A group is a
board RULER, every mode of it; riven rows count only when asked, and the leader
is the best row in scope. The line (80% by default) marks the first point that
reaches it. A pick can be saved as a build already placed on the point's
layout, and the ticked builds can be placed onto the point for no further
Forma (`start` = the point, limit 0). The scope is per weapon, in
`wfsim-forma-reach-<weapon>`. Applying a point keeps main, Exilus and grant
polarities in their chosen positions while the mods move to fit. A Warframe
has no board, so no optimizer.

Plan ahead requests every scene before constructing its groups. An incomplete
download reports failure rather than planning over a partial board. Every row
in scope is read; a row with a card this page cannot read is left out rather
than read light.

The capacity line follows the rules (`builderCap`, `wfCapacity`): no Catalyst
halves it, and without the mastery Forma a rank-40 weapon's rank is what the
layout spent. The board judges every build at `BENCHMARK_INVESTMENT` whatever
the player's rules say.

## Where the truth has to live

`engine::rules::capacity` owns the whole model, and the client consumes its
conclusions. `/api/meta` states `max_rank`, `capacity` and `forma_min` per
weapon — the ANSWER, not the ladder — so `capOf(id)` and `formaMin(id)` hold no
arithmetic of their own; `board::builds::validate` judges a submission at the
weapon's own capacity; `/api/simulate`'s `forma` block reports `rank`, `cap`
and the bill split by item; `rules::capacity::cost_of` answers what the layout
you actually set costs, as against what the cheapest would be.

**Auto spends the mastery Forma**: five Forma on a rank-40 weapon whatever the
slots need, because reaching rank 40 is what they pay for — the same floor
`plan_forma_spending` takes (`at_least`), billed the way `fit` bills it.

**THE CLIENT STILL CARRIES A SECOND COPY**: `slotDrain`, `modDrain`,
`capacityUsed` and `formaCount` in `46-forma.js`, because the panel sends mod
ids only and "what does MY layout cost" needs the layout. They must agree with
`rules::capacity`; a change to the arithmetic is made in both.

## Still open

- Whether anything else grants capacity on a gun. The stance bonus is melee, so
  nothing known does — but this is an absence, and absences are worth re-reading
  the wiki for when a new weapon class lands.
