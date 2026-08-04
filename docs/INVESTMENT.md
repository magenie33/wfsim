# Investment: what has been installed on this weapon

**Status (2026-08-04): PLANNED, not implemented.** Recorded while another
session held the files this touches. Nothing here is built yet; the mechanics
below are verified and the design is decided.

Today the app assumes an Orokin Catalyst, an Exilus adapter and an arcane
adapter, silently, and hardcodes the result as `60` in four places. This
replaces that with the real thing: capacity that depends on rank, rank that
depends on Forma, and adapters that are stated rather than assumed.

## The mechanics, verified (wiki, 2026-08-04)

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

Settled by the owner, 2026-08-04:

- **An Arch-gun has no Exilus slot at all**, so the question of it consuming
  capacity does not arise. Nor does a robotic (Sentinel/MOA) weapon have one.
- **Arcane adapters consume no capacity.** An arcane is its own slot with no
  drain.
- **The unranked base of 15 does not apply to weapons.** A weapon starts at 30.

## The feedback loop, and why the default removes it

On a rank-40 weapon every Forma does two things: it polarises a slot AND adds
2 to the max rank. So "how many Forma does this build need" has a moving
target — more Forma means more capacity means possibly fewer polarised slots.

**The default is to polarise the full 5 times** (owner, 2026-08-04), because
that is what full mastery affinity requires whether or not the build needs it.
That fixes capacity at 80 before planning starts, so the default path needs no
solver at all. Only the opt-out — "I do not want to spend 5" — reintroduces the
fixed point, which iterates and converges in at most 5 steps.

## The interaction: investment is DERIVED, not configured

The instinct to model this as a row of toggles was wrong (owner, 2026-08-04).
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
sync with it. Only three genuine CHOICES remain, because only these three are
the player's and not the build's:

| choice | default | why it is a choice |
| --- | --- | --- |
| use Omni Forma | off | it matches any mod except Umbra mods, so it removes the colour puzzle — but it is a different, costlier item |
| use Umbra Forma | **off** | precious (owner). With it off the planner may not use Umbra polarity, so an Umbra mod pays full or mismatched drain |
| polarise to max | **on** | 5 polarisations is what full mastery needs, even when the build would fit with 3 |

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

1. **Engine only, no UI risk.** `Polarity::Omni`; `capacity_for(weapon,
   investment)`; `plan_forma` taught the Omni/Umbra switches and the rank-40
   loop. Unit tests throughout.
2. **The wire.** `/api/panel` returns capacity, used, and the Forma breakdown by
   type (regular / Omni / Umbra). The four JS functions go. Verified against a
   frozen baseline: capacity and Forma counts must not move for any existing
   build.
3. **The UI.** The icon strip, the three choices, capacity read from the server,
   and the "this build cannot be made" message.
4. **Travel.** Share codes and presets carry the three choices. **An old link
   has no such field and must mean exactly what it means today** — Catalyst on,
   adapters on, polarise to max — or every link already posted changes meaning.

## Still open

- **Which weapons are rank 40.** Per-weapon data (`max_rank`), and the roster
  has none today — every entry is 30. Phase 1 therefore cannot be verified
  end-to-end against a real weapon until a Kuva/Tenet/Coda weapon is added;
  until then it is pinned by tests.
- Whether anything else grants capacity on a gun. The stance bonus is melee, so
  nothing known does — but this is an absence, and absences are worth re-reading
  the wiki for when a new weapon class lands.
