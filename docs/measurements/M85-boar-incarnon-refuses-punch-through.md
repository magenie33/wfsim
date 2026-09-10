# M85 — the Boar Incarnon refuses punch through, and its three beams reach nine ✅ (owner, 2026-09-10)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Verbatim:

> 我刚刚测试了，野猪的平常穿透是有效的，但是灵化下是无效的，不会穿透，我穿透叠满了，总共还是打9个。初始的那个就是没有穿透

### What it settles

**PUNCH THROUGH PAYS NOTHING IN INCARNON FORM.** The base form takes it
normally; the form does not take it at all, with the mods stacked to maximum.
`punch_through_mods: false` on both Incarnon entries — the same field the Latron
and Torid Incarnons already carry, and for the same kind of reason.

**THE WIKI SAYS OTHERWISE AND THE MEASUREMENT WINS.** The Genesis page:
*"each beam chaining up to 2 nearby enemies within 10 meters of the initial
target **and subsequent targets struck by punch through**"*. That clause
describes something this form cannot do, because the beam it describes does not
punch through. What the sentence is really about is the chain rule for a weapon
that CAN — the Larkspur's shape — and it was transcribed onto an entry where
the premise is false.

**NINE IS THE WHOLE OF IT, and nine is the model.** Three beams, each taking one
body it acquired for itself, each chaining two more:
`3 x (1 + 2) = 9`. The count was implemented from the page the same day and this
reading confirms it independently — the number did not come from the page's
arithmetic, it came from counting bodies in a fight.

**ONLY THE AIMED BODY CAN HEADSHOT.** Verbatim:

> 只有准信的那个敌人可以爆头，其余的都是自动锁定到身体的，灵化下

The two beams the weapon acquires for itself lock BODIES. So a headshot is a
property of where the player is pointing and not of the weapon, which is the
rule `chain::Instance` already carried for every other spread — a splash, a
chain hop, a tendril — and it now has a reading behind it here rather than an
inheritance.

### What it overturns

The hours before, I measured punch-through as worth **1.78x** on this form in a
361-body formation (`one_fight`, Seeking Force against Point Blank alone) and
reported it as a real payoff. It was the engine simulating a mechanic the game
does not give this weapon: `struck_bodies` walked the ray, each struck body
seeded its own chain, and every one of those chains is damage that never
happens. The figure is void and the entries now refuse the mods, which is what
the base form's own behaviour — punch through works there — makes checkable.

**AN ENGINE THAT ALREADY HAD THE FIELD IS THE WORST PLACE TO FIND THIS.**
`punch_through_mods` has existed since the Latron Incarnons were entered; the
Boar's entries simply never declared it, so they took the class default. A
default is not a decision, and nothing distinguishes "we decided true" from "we
never asked" once the value is the same.

### Still open

The two beams the weapon acquires for itself do not cast rays of their own in
this engine — only the aimed beam does — so a form that DID punch through would
still be modelled short. It costs nothing to leave alone here, because this form
punches through nothing at all.
