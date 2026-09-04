# M48 — the Burston Prime's CO reads 13 of its 55 on the DIRECT hit too ✅ (owner, 2026-08-16)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Setup.** Burston Prime, Incarnon form, ONLY Forceful Finality (+42 base
damage) and Galvanized Aptitude equipped, every shot on the TORSO, unarmoured
target.

| Aptitude stacks | status types on target | direct (crit) | radial |
| --- | --- | --- | --- |
| 1 | 1 | **181** | — |
| 2 | 1 | **196** | **65** |
| 2 | 2 | **227** | **76** |

…and four on the **BASE form**, which is the independent confirmation — another
attack, another crit multiplier (1.8), another fraction (46/88 = 0.523 against
the Incarnon's 0.236), and one reading that is the REFERENCE the others are
divided by:

| stacks | status types | direct (crit) | / bare |
| --- | --- | --- | --- |
| — | 0 (bare crit) | **188** | 1.0000 |
| 1 | 3 | **306** | 1.6277 |
| 2 | 3 | **423** | 2.2500 |

    1 + 0.4 x 1 x 3 x f = 1.6277  ->  f = 0.5231
    1 + 0.4 x 2 x 3 x f = 2.2500  ->  f = 0.5208
                                      46/88 = 0.5227

Both within 0.4%, on a form whose fraction is a different number entirely.

The target was a Corpus **Crewman**, which is where the x1.19 comes from: the
damage-type column is the faction's, and Corpus is `puncture: 1.5`. Our column
puts the bare crit at `(26.4 + 26.4x1.5 + 35.2) x 1.8 = 182.2` against a
measured **188** — a 3.1% gap that is a SEPARATE and much smaller question (the
shield pool has its own column, and which pool a Crewman's first hits land on
depends on its shields) and that the ratios above are immune to.

**TAKE THE RATIO TO A BARE HIT, NEVER THE ABSOLUTE.** The target's damage-type
column multiplies everything and cancels in the ratio — here it is x1.19 on the
base form's IPS mix (`188 / (88 x 1.8) = 1.187`) and about x1.0 on the Incarnon
form's pure Heat, which is the only reason the Incarnon absolutes fit on the
nose. Working from absolutes without the bare reading made the base-form
readings look like FOUR status types when three were reported and three were
right; the bare crit dissolved that immediately. The same lesson as M46's
`(crit_at_n − crit_at_0) / non_crit`.

### What was wrong

The catalog's 24% was read as belonging to the **radial alone** — the row names
"Incarnon Form Radial Attack" — so the direct hit computed its CO term on the
full evolved 55. The error is multiplicative in the CO term, so it grows with
the build:

| reading | game | engine before | overstated by |
| --- | --- | --- | --- |
| 1 stack, 1 type | 181 | 231 | +28% |
| 2 stacks, 1 type | 196 | 297 | +52% |
| 2 stacks, 2 types | 227 | 429 | **+89%** |

**THE EXCLUSION IS THE PERK'S, NOT THE ATTACK PART'S.** Once stated that way it
needs no new machinery: `co_base_excludes_this_evolution` already sets the
weapon's `co_base_fraction`, and the roster already carries the flag on eleven
other perks (docs/CATALOGS.md). It is now on both tier-2 +42 options of both
Burston variants — Forceful Finality and Fortress Salvo, flagged together the
way the catalog already flags the Atomos's two tier-2 options.

### Consequence for the board

The Burston Prime's published score was computed on the overstated direct hit
and will FALL when the board rescores. That is the correction working: the
weapon was ranked on damage the game does not deal, and the size of the drop is
a function of how much CO the build was carrying.
