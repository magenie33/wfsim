# M37 — a Debilitate DoT eats Attrition TWICE, and it is a BUG ✅ (owner, 2026-08-08)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Reported, measured, then explained.** A player asked the owner whether the
elemental hit that triggers Primary Debilitate can also trigger the Felarx's
20x, and the calculator said no:

**THE REPORT.** A DoT that Debilitate triggered can itself roll Devouring
Attrition's outer x21 — and the site's calculator could not show it.

He tried it:

**THE READING.** It does trigger. Three Roar layers in the chain — direct hit,
extra hit, DoT — and Attrition applies TWICE, so the DoT that lands carries Roar
cubed and 441x.

Roughly read: three Roar layers, two Attrition layers, Elementalist and the
elemental mods are all taken. With ONLY Roar equipped the DoT ticks once and
Debilitate is gone.

So the chain is **direct hit → extra hit → DoT** with a Roar layer at each step
(the
`f³` this engine already applied as `DEPTH_DERIVED_PROC`, see M33), and
**Devouring/Devastating Attrition applies twice**: 21 x 21 = **441**.

### It is not a design — it is a leak, and the leak names the rule

The reading, and the reason this is filed as DE's bug rather than as an
interaction: the +21 also lands on the DoT Debilitate produced, and that is not
intended (2026-08-08).

**HOW THE ZERO CARRIES.** A successful Debilitate fires one instance of ZERO
damage, multiplied by that instance's own faction bracket and its own 50%
Attrition roll (it cannot crit). When the DoT is computed from it the zero is
replaced by the previous tier's damage — and those two multipliers come along.

The split fires a damage instance whose damage is **zero** — which is why the
wiki can call it a separate instance and say it "has no damage" in the same
breath. Zero still gets multiplied, by that instance's own faction bracket and
its own Attrition roll. Then the DoT is computed off it, the zero is **replaced
by the parent hit's value**, and the two multipliers already applied to the zero
are left in. One instance's multipliers on another instance's magnitude.

Being able to name the mechanism is what makes the third ruling below
predictable rather than a second measurement.

### What it pins

1. **A hit's Attrition roll travels with the statuses it applied.** A proc's
   magnitude is the applying instance's — that is why `crit_mult` was already
   carried into `settle_procs` — and Attrition is a per-instance multiplier of
   the same shape. This engine was passing 1.0.
2. **The split rolls a second one of its own**, on the zero.
3. **The split's roll lands even when the parent hit CRIT.** The zero instance
   has no crit of its own — there is nothing to crit — so "on a hit that is
   neither Critical nor…" is satisfied whatever the parent did — it fires on a
   50% roll, and it does not crit either. A critting build therefore gets 1 x 21
   here, never 1. This is
   the ruling that matters in practice, since the parent hit on a real Felarx
   build usually crits and its own roll is then worth nothing.

Two rolls and not three: the DoT itself never rolls, because a DoT is not a hit.
Had it rolled the number would have been 21³ = 9261, and had only the split
rolled it would have been 21.

`the_debilitate_dot_carries_two_attrition_layers` asserts all three, and each
fails on its own when removed: an ordinary Slash DoT must come out at **x21.0**,
a split's Toxin DoT at **x441** (±25 over 200 runs), and a fight that crits every
shot must still show **x21** on the split's DoT. The perk's own 50% is forced to
1.0 for the same reason as M36 — the question is which layers apply, not the
odds. Two details the test has to work around: turning the perk on consumes an
extra RNG draw per instance, so a single pair of runs compares two different
fights and only an average means anything; and Puncture is made immune, because
Weakened is a flat crit-chance buff on the victim that would otherwise set the
crit rate the test is trying to control.

### Why the explanation is believed, and what it does not settle

It is not a fit to 441 — it PREDICTS things that were not measured, and they
hold:

- **It predicts the 3-vs-2 asymmetry.** Faction lands at every step of the chain
  because a status always carries it one layer more than what caused it;
  Attrition lands only where an instance ROLLS, and the DoT does not roll
  because it is not a hit. Three and two fall out of one story. Any account
  where the split "just gets a bonus layer of everything" has to explain why
  Attrition is not also three.
- **It predicts the crit ruling**, which is the counter-intuitive one, and it
  was implemented from the prediction rather than from a measurement.
- **It is consistent about Cold** (2026-08-08): the roll picks one of the two
  components — roll Cold and it is a 441x Cold with no payload, roll Toxin, Heat
  or Electricity and it is a 441x DoT. A 441x multiplier on a status that has no
  damage payload is worth nothing — which is what this engine does anyway, since
  only a DoT type reaches `push_dot`. A theory that has to special-case Cold
  would be a worse theory.
- **It explains the wiki's two sentences at once** — "separate damage instance"
  and "has no damage" are contradictory until the instance's damage is a literal
  zero.

**THE CARRIER IS NOT GENERALISED, and that is a decision rather than an
omission** (2026-08-08): this x21 is the one that is unintended, and everything
else stays modelled as it was. The obvious next question was whether every
free-standing final
multiplier double-dips — **Condition Overload** being the candidate M36 already
established is its own bracket on this weapon. It is not asked, and CO stays
CO¹: the owner plays this weapon and the 21 is the only term he has seen behave
this way. Should that change, the measurement is one run — hold the status count
fixed, compare the DoT with and without CO — and the term to add sits next to
`attrition` in the same struct.

The same shape showed up in M33's Cyte-09 chain — its resupply looked like the
same case, as though something were being handed down layer by layer. That is
what makes "a carrier
passed down the chain" worth treating as the model rather than as a story about
one arcane.

### Reproduced on the shipping site

A Felarx Incarnon cycle, eight shotgun mods making Corrosive
(`primed_charged_shell` + `shell_shock` + `toxic_barrage` + `contagious_spread`,
with `shotgun_elementalist` alongside), Primary Debilitate at rank 5, against a
level-9999 Steel Path eximus Corrupted Heavy Gunner, 60 runs of 30s through
`/api/simulate` in the shipping wasm build:

| build | direct | split DoT (Toxin+Electricity) |
|---|---|---|
| Debilitate, no Attrition | 1.28 M | 0.18 M |
| Debilitate + Devastating Attrition | 7.48 M | **7.86 M** |

The split's DoT grows **44x** while the direct damage grows 6.2x — the gap is
the second layer — and it ends up **larger than every direct hit in the fight
combined**, which is the shape of the reading: the DoT ticks once and
Debilitate is gone.

THE TARGET HAS TO SURVIVE TO 10 STACKS. On a level-150 gunner the same build
shows NO split at all with Attrition equipped and a healthy one without it: the
21x kills it before the tenth Corrosive stack lands, and the arcane's condition
is never met. That is the model working, not failing, but it means the
interaction is invisible in any scenario where the weapon simply wins.

### THE SPLIT'S ROLL IS ITS OWN COIN ✅ (owner, 2026-08-10)

**AND THE SPLIT ROLLS ITS OWN COIN.** Only the direct hit's x21 is the direct
hit's; Debilitate's zero-damage extra hit rolls Attrition again for itself.

Which is what the engine does, and now what a test says. The forced-chance runs
above cannot see it — with the perk pinned at 1.0 every roll succeeds, so "rolls
its own" and "copies the hit's" produce the same 441 — so the claim is made at
the perk's REAL 50%, where the two readings are far apart:

| | expectation of the DoT's multiplier |
| --- | --- |
| two independent coins | `E[hit] × E[split]` = 11 × 11 = **121** |
| the split copying the hit | `E[hit²]` = ½·441 + ½·1 = **221** |

`the_debilitate_dot_carries_two_attrition_layers` reads **x121**. The joint
distribution of the two rolls, instrumented while writing it, comes out
25/25/25/25 across (1,1) (1,21) (21,1) (21,21) — four equal cells, which is the
whole of what "independent" means.

**So a 21× hit does NOT guarantee a 441× DoT.** The four outcomes are ×1 a
quarter of the time, ×21 half, ×441 a quarter — and the ×441 the owner measured
is the top of that spread rather than the rule.

The same independence is why a COLD split is worth nothing however it rolls: it
takes its own coin like any other, and then has no damage payload to spend it on
(2026-08-08: roll Cold and it is a 441x Cold, which has no payload).

**A note on how this was nearly mis-read.** The first version of the test
compared 400 runs against a 200-run baseline and reported x243 — close enough to
221 to look like the engine was copying the roll. It was a ratio between two
different numbers of fights. The instrumented joint distribution is what settled
it, and it is worth remembering that a suspicious factor of ~2 is usually a
bookkeeping error rather than a mechanic.

### A CRIT COSTS THE SPLIT A COIN ✅ (owner, 2026-08-10)

**A CRITICAL DIRECT HIT STILL LEAVES A x21 DoT**, carrying the crit's own
multipliers (crit damage, weak-point crit) — because Debilitate itself never
crits.

Both halves are true and they pull opposite ways:

- **a critical hit is not eligible for Devouring Attrition**, so the HIT's coin
  is gone — one coin instead of two;
- **the split instance never crits**, so ITS coin is always live, and the DoT
  still inherits the hit's crit multiplier and its body part.

So the answer to "could it be more than 21x" is yes — it is `crit_mult × 21`
when it rolls. But the comparison that matters is between builds, and it is
arithmetic:

|  | expectation of the DoT's multiplier |
| --- | --- |
| not critting | `E[hit] × E[split]` = 11 × 11 = **121** |
| critting | `crit_mult × 11` |

**They cross at a crit multiplier of 11.** Measured, 200 runs a cell:

| build | Attrition is worth | split DoT total |
| --- | --- | --- |
| no crit | **×120.6** | 1.97e10 |
| always 3× | ×11.0 | 9.44e9 |
| always 11× | ×11.0 | 3.46e10 |
| always 21× | ×11.0 | 6.61e10 |

The ×11 is the SAME at 3×, 11× and 21×, which is what shows it is the hit's coin
that went missing rather than a scaled version of it. And a 3× crit build's
split DoT comes out at **half** a non-critting one's, despite the crit.

**This is the DoT bucket alone.** The direct damage still wants crits by a wide
margin and no real Felarx build gives them up — but it is the one place in this
model where two of the weapon's own perks pull against each other, and
`a_crit_costs_the_split_a_coin_and_pays_it_back_in_multiplier` pins both ends of
it.

### ✅ CONFIRMED IN GAME on a second weapon (owner, 2026-08-10)

**THE SPLIT IS PERMANENTLY NON-CRITICAL**, not a zero-damage hit that may roll
one. Read with crit chance raised to guaranteed: the DoT Debilitate triggers
still comes out x21 some of the time.

Everything above rests on the split instance being **permanently non-critical**
rather than **a zero-damage hit that happens to roll crit against a zero**. The
two readings deal identical damage — zero either way — and are opposite about
eligibility: under the second, a build that crits every shot disqualifies the
split too and the whole extra layer disappears.

The Phenmor at guaranteed crit separates them, and it comes out the first way.
It is also a second weapon and a second perk: the original deduction came off
the Felarx's **Devastating** Attrition, this is the Phenmor's **Devouring**
Attrition, so the behaviour belongs to the SPLIT and not to one card. Nothing
changed — the engine passes a literal tier `0` at
`dummy.rs` `attrition: attrition * noncrit_mult(ap.noncrit_bonus, 0, rng)`, and
claim 3 of `the_debilitate_dot_carries_two_attrition_layers` already asserted a
fight that crits every shot still takes ×21. This upgrades that claim from a
reading of "on a hit that is not critical" to a run.

### What is still open

- **This is a bug, so it can be patched.** Nothing here is a designed
  interaction, and a DE hotfix that stops the zero from carrying its multipliers
  removes both extra layers at once. That is a reason to keep it in one place
  (the split's `InstanceScale`) rather than to generalise it — and the reason
  the arcane's card SAYS SO: `live_bugs:` on `primary_debilitate.yaml` is a
  fourth kind of admission, the only one that is not a shortfall: build it, mark
  it as possibly unintended, stay faithful to the game as it is, and change it if
  DE fixes it. The other
  three tell a player the number is lower than the card promises; this one tells
  them it is right today and rests on something DE can take away.
- **The lingering FIELD's ticks** (Torid's cloud) roll their own crit tier, so by
  the ordinary argument they are instances and should roll Attrition. Left at
  1.0: no weapon in the roster carries both, and this measurement does not reach
  it.
- ~~**元素师**~~ — CLOSED. It is `shotgun_elementalist` (霰弹枪元素师), an
  ordinary elemental-damage mod, and it already reaches the split's DoT the way
  every elemental mod does: the split runs the normal proc path and picks up its
  own element's bracket. Nothing to change.
- **Whether an ordinary status DoT double-dips faction** remains M33's question.
  This entry changes the Attrition term only.
