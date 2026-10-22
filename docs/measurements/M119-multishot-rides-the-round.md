# M119 — multishot rides the weapon's own rounds ✅ (owner, 2026-10-22)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

## The readings

- **Laetum, both forms**, with multishot, on level-1 enemies that die to any
  hit: only the first number ever pops. Fired at the floor beside a group,
  one group of numbers — the first round's explosion — and nothing from the
  rest.
- **A packed group of level-1 Runners**: the first dies to the direct hit and
  the rest to the explosions.
- **Burston**, a burst on a level-1 Runner with another behind it: three
  rounds of ammunition spent, one body killed, nothing reaches the one behind.
- **A shotgun** at an angle where the spread covers two bodies hits both.
- **Tigris Prime**: two independent shots a pull, which can kill two bodies.
- **Punch through with multishot**: two bodies punched read five numbers,
  each its own crit.

## What it settles

- **A shot's physical pattern is the weapon's own**; multishot adds instances
  to a round — each its own number, crit and status — that land where the
  round lands and never reach another body by themselves.
- **A body a shot killed takes nothing more of that shot**; the round's
  blast still reaches the bodies standing, and the body stands back up once
  the shot is settled. A burst is one shot for this.
- **Only the round punches through**, rolling its multishot again at every
  body, each instance its own crit; a dead body in front stops nothing.
- **Order and visibility are M62's**: instances settle in order and each reads
  the target as the instances before it left it.

Tests: `fight::tests::m119_multishot_rides_the_round`,
`fight::tests::elements_arcanes_weakpoints::emergent_multishot_stacks_are_earned_by_kills_from_zero`.
