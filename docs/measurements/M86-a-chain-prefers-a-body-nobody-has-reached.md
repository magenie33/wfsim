# M86 — a chain prefers a body nobody has reached yet ✅ (owner, 2026-09-10)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

### What it settles

**A HOP PREFERS A BODY NO PATH HAS REACHED**, and takes one that has been hit
only when there is nothing fresh in range. It is a PREFERENCE, not a ban — the
wiki's *"The chain from the target hit after the Punch Through can deal damage
to the first target, and vice versa"* is what a crowded corner still produces,
and the fallback is what produces it.

**IT IS THE CHAIN'S RULE, not the Boar's.** `chain::resolve_in` and
`resolve_with` are the two paths every chaining weapon in the roster resolves
through, so the Amprex, the Atomos, the Torid Incarnon, the Larkspur, the Kuva
Nukor and the Ignis all take it.

### What it disagreed with, and it was the model

Nine instances were landing on SEVEN bodies in the group-clear formation: the
Boar Incarnon's three beams stand in one column, and the second beam's first hop
went back to the body the first beam had used as its seed. THE READING IS NINE
— nine bodies lit, counted in a fight — against the model's seven.

    col+0/rank0   seed of beam 1 (1.000)  …and beam 2's first hop (0.800)
    col-1/rank0   beam 1's first hop (0.800)  …and beam 2's second hop (0.640)

### What M52 did NOT settle

[M52](M52-chain-path-is-fixed.md) measured two seeds fired at once and found
*"hitting (1,1) and (2,1) at the same time perturbs neither — two seeds, two
independent paths, each the same one it walks alone"*. That is still true and is
still asserted: a path does not revisit its OWN bodies, and one path running out
does not stop another.

But the two paths it read were **disjoint** — `1,1-1,2-1,3-1,4-2,4-3,4` against
`2,1-3,1-4,1-4,2-3,2-2,2` — so neither ever wanted a body the other had taken.
The reading is silent on the question this one answers, and it took a formation
dense enough to force the collision to notice.

### The ordering bug it uncovered

The first ship of this rule marked a seed as TAKEN when its own turn came round,
so the first beam could hop onto a body the second was about to stand on. Read
on the live site, group-clear, the Incarnon cycle: eight bodies where nine were
expected, and the signature was in the totals — one body carrying **1.8x** a
plain seed (its own 1.0 plus another beam's 0.8) while a ninth body took
nothing.

The seeds are all known before a single hop is chosen, so this is an ordering
bug with a fixed answer rather than a tie-break: every seed is marked before any
path walks. `a_beam_never_hops_onto_another_beams_seed` holds it, on a line of
bodies where the nearest thing to the first beam IS the second beam's seed —
coverage cannot see it there (six bodies on a line are all reached either way),
so the assertion is about WHICH bodies each path took.

### What it costs, and the direction is the surprise

Measured with `one_fight`, 361 bodies at 3 m, Thrax Centurion level 60, 180 s,
1000 runs x 3:

| | before | after |
| --- | --- | --- |
| `boar_prime_incarnon` | 3.351 | **1.812** (−46%) |
| `torid_incarnon`, `amprex`, `kuva_nukor`, `larkspur_prime`, `atomos`, `ignis_wraith` | — | byte-identical |

**SPREADING PAYS LESS, NOT MORE.** Thin damage across many bodies is more of it
lost to armour and overguard than concentrated damage that breaks one. The
number a player would call intuitive — nine bodies lit up instead of seven — is
the number that scores WORSE, which is the whole reason it had to be a reading
rather than a preference.

**THE OTHERS ARE UNCHANGED IN THIS FIXTURE AND THAT IS NOT A PREDICTION.** The
global set only decides anything when one shot has SEVERAL seeds — self-aiming
beams, punch through reaching more than one body, or a damage radius catching
more than the body it went off on. In `one_fight`'s default build the other
chaining weapons fire ONE seed, and a single path never collides with itself.
The board's rows carry real builds — a Torid with Primed Firestorm catches
several bodies in its sphere and seeds a chain from each — so those rows will
move, and the rescore is what says by how much.
