# M52 — a chain's path is FIXED, and its rule is not in the formation ✅ (owner, 2026-08-17)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

The first measurement of chaining, and the only part of `engine::chain` that
rests on something fired in game rather than on a wiki line.

**THE SETUP.** A **5 x 4** formation in the Simulacrum, bottom-left `(1,1)` and
top-right `(5,4)`. Torid Incarnon.

**THE READINGS.**

```
shooting (1,1):   1,1 - 1,2 - 1,3 - 1,4 - 2,4 - 3,4
shooting (2,1):   2,1 - 3,1 - 4,1 - 4,2 - 3,2 - 2,2
```

Both are five hops, both repeat exactly, and **hitting (1,1) and (2,1) at the
same time perturbs neither** — two seeds, two independent paths, each the same
one it walks alone.

> 我发现如果生成的敌人，是规整在这几个位置的，那么无论什么敌人，都是这样的规律。
> 但是如果模型不是人形，或者展位稍微错位，那么路径也会不一样。

### What it CONFIRMS: nearest

**All ten hops went to an orthogonal neighbour.** Never a diagonal, never past
a nearer body. On a square lattice that is exactly "the nearest viable target",
and it is now measured rather than assumed.

### What it REFUTES: any tie-break made of relative geometry

Nine of the ten hops were exact ties. Every candidate rule was scored against
all ten:

| rule | fits |
| --- | --- |
| entity index — row-major or column-major, lowest or highest | 4–7 / 10 |
| a fixed compass priority, best of all 24 orderings | **8 / 10** |
| a turn preference (straight / left / right / back), best of all 96 | **8 / 10** |
| nearest to the seed / farthest from the seed | 5–6 / 10 |

Nothing fits, and the reason is visible in one pair of steps: arriving at
`(3,1)` heading `+x` the path went STRAIGHT, and arriving at `(4,1)` heading
`+x` it TURNED. Same heading, same shape of choice, two answers — so no
function of the formation's own geometry can be it.

### The explanation, and it is the owner's own clue

**A non-humanoid model changes the path while every relative position stays
identical.** What changes with the model is the COLLIDER. So the order is the
game's spatial query handing back bodies in world-space broadphase order —
which cell each body falls into — and that is not a function of the formation
at all. It explains all three observations at once: a fixed layout gives fixed
cell assignments and therefore a fixed path; nudging a body across a cell
boundary changes it; and so does a collider of a different size.

The owner's guess (*"猜测和怪的坐标有关系？"*) was right, with one correction:
the ABSOLUTE world coordinates, not the positions within the formation.

### What the model does instead

Not reproduce it (owner, 2026-08-17): *"我们不要求100%还原，但是思路是一致的
… 如果多个敌人是永远不动的，那么这个链接的路径是永远固定的，就做到这点就可以了。"*

So `chain::resolve` breaks ties by the lowest body index — arbitrary, and
STABLE, which is the honest pair when the real rule is unknowable. A formation
that does not move always chains the same way, and a test asserts it a hundred
times over, with a body walked off the map as the negative control.

**AND THE UNKNOWABLE PART DOES NOT REACH THE ANSWER.** The total is invariant
to tie-breaking — `seeds x (1 + f + … + f^hops)` — so what nobody can know
moves damage BETWEEN bodies without changing how much the formation took. It
decides which one dies first and nothing else.
