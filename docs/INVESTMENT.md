# Investment: what has been installed on this weapon

**Status: CAPACITY IS REAL; THE PLAYER'S FORMA RULES ARE ON SCREEN; THE
ADAPTERS ARE STILL ASSUMED.** Capacity depends on rank, rank depends on Forma,
and the planner is the engine's (§The planner). Carrying anything in a share
link is not done, and does not need to be: the rules are the player's.

So the app still assumes an Exilus adapter and an arcane adapter, silently. What it no longer assumes is the number those produce: an
adversary weapon ranks to 40 and finishes at 80, and every surface that prints
a capacity says so.

## A POLARITY BELONGS TO THE WEAPON, NOT TO THE SLOT

Two slots' polarities can be SWAPPED without changing what either slot IS — the
exilus slot stays exilus, it just carries a different polarity afterwards. It is
the least-known thing in this file (the owner did not know it either until he
tried it), and the whole Forma model rests on it.

**WHAT IT MEANS FOR PLANNING.** The weapon's polarities are a POOL, not a set of
fixed positions, so `plan_forma_spending` flattens `innate_slots` and matches
the biggest-drain mods against the multiset. That is not an approximation — it
is the rule.

**AND THE EXILUS SLOT'S POLARITY IS IN THAT POOL**, even for a build that puts
no mod in the exilus slot at all. Swap it onto a main slot; the exilus slot
carries whatever came back and sits empty. The board withheld it until
2026-08-16 on the reasoning that "the slot is out of scope, so its polarity is
not a discount this build gets to spend" — and the *so* was the error, because
the polarity is not attached to the slot. The adapter is assumed installed
anyway (below), so the slot exists.

It was over-charging **699 of the 928 stored board rows by one Forma each** —
three quarters of the board, the Torid alone 95 rows.

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

`mods::plan_forma` and `forma::plan` both implement it, and
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

30 + Catalyst = 60, which is the constant hardcoded today. 40 + Catalyst = 80.
(The +10 stance bonus that takes a Paracesis to 90 is melee-only.)

Settled measured:

- **An Arch-gun has no Exilus slot at all**, so the question of it consuming
  capacity does not arise. Nor does a robotic (Sentinel/MOA) weapon have one.
- **Arcane adapters consume no capacity.** An arcane is its own slot with no
  drain.
- **The unranked base of 15 does not apply to weapons.** A weapon starts at 30.

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

The instinct to model this as a row of toggles was wrong.
The right shape:

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

`engine::forma::plan` answers for ONE item and ANY NUMBER of its configs,
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
multiset over it is billed and the cheapest bucket is placed. A colour nobody
carries only ever mismatches, so it is not tried. The cost is thousands of
layouts in the usual case and tens of milliseconds in the worst one measured
(eight colours, four configs). `mods::fit` stays the optimizer's greedy
planner; `forma::tests::one_loadout_bills_what_fit_bills` holds the two to the
same bill on one config.

**THE BILL** is `Σ max(0, target − start)` per polarity, a bare slot counted as
a polarity of its own (blanking takes a Forma), each bought slot billed as the
item that makes it — Omni, Umbra, or a regular Forma. A grant slot outside the
pool (the stance slot) costs one when it changes. No Forma makes the Aura
colour. Mastery Forma are added on top up to the rank floor.

**THE ORDER a layout is judged in**: Umbra Forma (under "when needed"), the
grant slot unmatched (once anything is spent), Forma, Omni, then the WORST
config's spare capacity, the total spare, and the item's own colours moved.

**THE RULES** are the player's and are GLOBAL — one set for every weapon and
every frame. What is per item is which configs are planned together, and the
FIRST of them is always the one being edited: it keeps its mod positions, and
the others are rearranged onto the layout.

| rule | default |
| --- | --- |
| Catalyst / Reactor | on |
| reach max rank | on |
| stance / aura slot first | on |
| Omni Forma | never · allowed (only where it saves a Forma) · preferred |
| Umbra Forma | never · **when needed** · allowed (an ordinary Forma) |
| Forma limit | none |

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

The capacity line follows the rules (`builderCap`, `wfCapacity`): no Catalyst
halves it, and without the mastery Forma a rank-40 weapon's rank is what the
layout spent. The board judges every build at `BENCHMARK_INVESTMENT` whatever
the player's rules say.

## Where the truth has to live

`engine::mods` owns the whole model, and the client consumes its conclusions.

This is not tidiness, it is the lesson of 2026-08-04 applied before the fact.
The JS today reimplements this arithmetic in FOUR functions — `slotDrain`,
`modDrain`, `capacityUsed`, `autoForma` — and they have already diverged:
`Omni` exists in the JS polarity set and **not in `mods::Polarity` at all**.
That divergence is harmless today only because user-chosen polarities are never
sent to the engine. The moment Omni becomes a planning input it stops being
harmless.

Adding rank-40, Omni, Umbra and four adapters to both copies would be the same
mistake four times over. **The four JS functions are deleted in phase 2**, and
what replaces them is a number the server computed.

## Phases

Each one is independently shippable and independently verifiable.

1. ~~**Engine only, no UI risk.**~~ **DONE 2026-08-04** (`engine::mods`):
   `Polarity::Omni`, `rank_after`, `forma_to_max_rank`, `Investment`,
   `FormaCost`, `plan_forma_with`, and `fit` — which owns the whole question
   and is what the UI will call. Thirteen tests, the wiki's own numbers pinned.

   **FIVE IS A CAP ON RANK, NOT ON FORMA** — the one thing this phase got wrong
   before the tests said so. You may polarize as many slots as you have; only
   the first five raise the max rank. So eight heavy mods on a rank-40 weapon
   settle at SIX polarizations: five buy rank 40 (capacity 80) and the sixth
   buys nothing but a halved slot, which is exactly what the game does. A
   budget is self-consistent when the rank it claims comes from polarizations
   actually spent, and spending more than five is never a contradiction.
2. **The wire.** PART DONE 2026-08-04 — the SERVER no longer hardcodes 60.
   `WeaponSpec.max_rank` is read (the data carried it and nothing looked),
   `builds::validate` judges a submission at the weapon's own capacity, and
   `/api/simulate`'s `forma` block reports `rank`, `cap` and the bill split by
   item. `engine::mods::cost_of` answers the OTHER question — what the layout
   you actually set costs, as against what the cheapest would be — which until
   now existed only as `formaCount()` in the client.

   **The literal 60 is gone.** `/api/meta` states `max_rank`,
   `capacity` and `forma_min` per weapon — the ANSWER, not the ladder — so the
   client holds no capacity arithmetic of its own: `capOf(id)` and
   `formaMin(id)` read what the server computed with `mods::capacity` /
   `rank_after` / `forma_to_max_rank`. That is what makes an adversary weapon
   count against 80 in the builder, on the share card, and in the auto plan.
   Nothing moved for an existing build: every rank-30 weapon answers 60 and 0.

   **Auto now spends the mastery Forma.** `autoForma` planned for
   minimum-Forma-to-fit and stopped, so a Kuva Nukor fitting in two
   polarizations was measured against the 80 capacity that only five buy. It
   now takes the same floor `plan_forma_spending` does (`at_least`), and
   `formaCount` bills the remainder the same way `fit` does — five Forma on a
   rank-40 weapon whatever the slots need, because reaching rank 40 is what
   they pay for.

   **Still to do:** the three remaining JS functions (`slotDrain`, `modDrain`,
   `capacityUsed`/`formaCount`). They cannot go until the panel carries the
   slot polarities (it sends mod ids only), because "what does MY layout cost"
   needs the layout.
3. **The UI.** The icon strip, the three choices, capacity read from the server,
   and the "this build cannot be made" message.
4. **Travel.** Share codes and presets carry the three choices. **An old link
   has no such field and must mean exactly what it means today** — Catalyst on,
   adapters on, polarise to max — or every link already posted changes meaning.

## Still open

- ~~**Which weapons are rank 40.**~~ **CLOSED 2026-08-14.** The Kuva Nukor is
  the roster's first `max_rank: 40`, so the ladder is verified end-to-end
  against a real weapon rather than by tests alone: `scripts/check_valence.mjs`
  asserts 40 / 5 / 80 off `/api/meta`, that the builder counts against 80, that
  a full eight-mod build is not shown as impossible, that Auto spends the five,
  and — the control — that an ordinary weapon is still 60 with no mastery
  Forma.
- Whether anything else grants capacity on a gun. The stance bonus is melee, so
  nothing known does — but this is an absence, and absences are worth re-reading
  the wiki for when a new weapon class lands.
