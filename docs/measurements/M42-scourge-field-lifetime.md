# M42 — the Scourge's field dies when the NEXT THROW STARTS, not when it lands (owner, 2026-08-14)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Question.** The wiki states the exclusivity and not its timing: *"Only one
field can be deployed at a time. Throwing the spear will remove existing
fields."* Removal at the new spear's IMPACT and removal at the throw's
initiation look identical on a single throw and are opposite mechanics on a
build that throws continuously — the first hands a spam build ~100% uptime, the
second hands it the least uptime of any way to play the weapon.

**Reported, verbatim:**

> 然后我测试发现，之前投掷过留下的东西，会在我投掷发起的那一刻消失

(*"then I tested it and found that what a previous throw left behind disappears
the instant I initiate a throw"* — the removal is keyed to the throw ACTION.)

**What it settles.** The field's own clocks are ceilings a throw build never
reaches. *"The field lasts for 20 seconds"* and *"pulses immediately on impact
then once every 5 seconds"*, so a build that throws every second gets **the
impact pulse and nothing else** — the every-5-s pulses require not throwing for
5 seconds, and the 20 s duration requires not throwing for 20. And because the
old field goes at the throw's START rather than at the new one's impact, there
is a DEAD BAND on every throw — the whole travel time, plus whatever the throw
animation costs before release — where no field exists at all.

So the FIELD ENTITY is worth least to the build that throws most, and the way
to hold one for its full 20 s is to throw ONCE and then fire the primary.

**Then the second half, which reverses that for the part that matters** (owner,
same day, answering the question this measurement had left open):

> 消失的只是立场，消失前被附加的立场是不影响的，这个立场效果就是虚空的特效

(*only the FIELD disappears; what it had already applied is unaffected — and the
field effect IS the Void effect.*)

Two things at once. The debuff on an enemy is **not** taken back with the field
that applied it, so a build throwing every 1.6 s re-applies a 4.7 s attractor on
every impact and the TARGET carries one continuously — the opposite conclusion
from the field entity's, and the one that decides whether a spam build has
attractor at all. And it is the Void Bullet Attractor, i.e. `DebuffState::
attractor` — the debuff the engine already had, reachable until now only from
Xata's Whisper.

**What it does NOT settle.** The headshot rate the field is worth: unchanged and
still the blocker (docs/UNMODELLED.md §Bullet Attractor). This pair of reports
settles the field's UPTIME and its identity; neither says what easier aiming is
worth, and the wiki refuses to ("does not guarantee a headshot").

**Status: WIRED, for exactly what it is worth here** (`attractor_seconds: 4.7`
on both thrown entries). One line in the Condition Overload counter and nothing
else, which is all `DebuffState::attractor` has ever been worth in this arena.
Three consequences fall out of the two clocks rather than being modelled:

- 4.7 s against a ≤1.6 s cycle means it is simply UP from the second throw on;
- the field is planted AFTER the throw's own pellets land, so a throw never
  counts the field its own impact planted — the ordering that claims least;
- the field's every-5-s pulses can never fire in a throw-only fight (the cycle
  is shorter than the interval), so omitting them is exact here, not an
  approximation.

Pinned by `a_thrown_speargun_plants_a_bullet_attractor_that_counts`, which
measures it through the CO count — a build with a CO bracket must beat the same
build that cannot see the field — and carries the negative control that the two
thrown entries are the only planters in the roster. Verified to bite: dropping
the plant makes the two builds identical to the last point.

### Sources

- [`Scourge Prime`](https://wiki.warframe.com/w/Scourge_Prime) — Characteristics:
  the 2 m field on heads within 14 m, 4.7 s on an enemy, a pulse every 5 s, 20 s
  of field, one field at a time
- the owner's test above
