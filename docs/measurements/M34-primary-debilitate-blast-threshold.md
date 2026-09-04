# M34 — Primary Debilitate was dead on Blast, and only a run could tell ✅ (2026-08-08; threshold generalised 2026-08-10)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

From "还有blast是可触发冰和火的" (owner, 2026-08-08). It is a one-line statement
of fact about a combination the arcane is supposed to cover, and the engine did
not cover it — not by omitting Blast from a table, but by making it unreachable.

`components_of(Blast)` has always answered `(Cold, Heat)`, and
`combined_stacks(Blast)` has always counted `debuffs.blast`. The unit test on
the split function passed for Blast the day it was written. What never happened
is the split.

**The reason is Blast's own cap.** Every other combination sits at ten stacks
and waits for an eleventh application — which is what "if an enemy HAS 10
stacks, inflicting the same Status Effect AGAIN" describes, and why the check
reads the count BEFORE this application. Blast does not sit anywhere: reaching
ten DETONATES and drains every stack (`detonate.yaml`: "reaching 10 detonates
everything early"). So the count a later application reads is 0..=9 forever,
and the condition can never be true.

**The tenth APPLICATION is therefore the moment the condition is met** — the
only instant a Blast target has ten, and so the only reading of that sentence
under which Blast is eligible at all.

That was implemented for Blast and for nothing else, on the reasoning that for
the other five the pre-application count is right and counting the new stack
would fire the arcane one application early. **That reasoning was wrong, and the
exemption was the rule.** See below.

### ✅ RESOLVED, and it was never a Blast rule (2026-08-10)

> 如果当前是9层，下一发是10层的话，就可以立刻触发其中一个（根据等级），而不是要
> 等到10级后，再打才触发。这样爆炸实际上是可以触发火或者冰的，而且并不像wiki说
> 的那么rarely
>
> 全部都是这种情况的，我实际测试了

— owner, 2026-08-10, on all six combinations.

So the threshold is the count the target is AT **including the stack this
instance is applying**, for every combined element. DE's card text ("if an enemy
HAS 10 stacks, inflicting the same Status Effect AGAIN") is one step late, and
the `if proc == Blast` branch that stood here for two days is gone:
`debilitate_split` takes `stacks_with_this` and the caller passes
`stacks_before + 1` unconditionally.

**What it costs the other five: one application.** They cap at ten and stay
there, so under either reading every subsequent application splits — the only
difference is the shot that reaches ten, which now splits too. On Blast the same
one-line rule is the entire mechanic, which is why the bug was visible there and
nowhere else.

**And the wiki's "rarely" is wrong about Blast for the same reason.** Under the
card's own reading Blast could never split at all; under the measured one it is
an ordinary member with no special case anywhere in the engine.

This is the shape the repo keeps running into (see
[derive triggers, don't list them]): a fix written as "…and also Blast" was the
general rule with five exceptions no one had tested. The way it got caught was
the owner testing the other five.

### What would have falsified it

A Blast build with a saturating status chance, a Bane, and the arcane at rank 5,
against a target that survives detonation. If Cold and Heat statuses never
appeared, DE would be evaluating the arcane strictly before the stack lands and
Blast would be genuinely exempt. Kept here because the same experiment now runs
the other way: it is `a_blast_build_actually_reaches_the_debilitate_threshold`,
and `the_tenth_application_is_the_one_that_splits` is its non-Blast twin —
nine applications must pay nothing, ten must pay. Both were re-run against the
old reading and both fail on it.

### Why it needed an end-to-end test

`debilitate_splits_only_a_saturated_combination` asks the split FUNCTION what it
does at ten stacks. It answered correctly for Blast the whole time. The gap was
that nothing ever handed it ten, which is invisible to any test that calls the
function directly — so `a_blast_build_actually_reaches_the_debilitate_threshold`
runs a saturating Blast build and asserts the arcane adds damage at all. It
bites: with the pre-application count it reports "added 0". The lesson
generalises past this arcane — a threshold and the thing that produces the
threshold are two different claims, and a unit test on the first says nothing
about the second.

The table test now walks all six combinations rather than Corrosive alone. Five
of six passing is exactly the shape of failure nobody checks for.
