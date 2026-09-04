# M55 — the Soma Prime's GunCO reads its own 12, and neither Incarnon perk raises it ✅ (owner, 2026-08-22)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

The `additive_with_base_damage` default — *a perk's flat base add raises what
the attack PRINTS and not what the CO term computes on* — was GENERALISED ahead
of the wiki's catalog rather than read off it. Nine readings across four builds
of one weapon now certify it, and they certify the shape as well as the number:
a perk that adds through the PANEL and a perk that adds through a BUFF both stay
out.

**THE READINGS**, as typed. Soma Prime, base form, one 40% source unless the
build says otherwise:

```
3.0爆伤                      3.0爆伤                      1层/2buff- 158-475
36基伤（12+12）+6*2          24基伤（12+12）
200腐蚀                      200腐蚀（40天赋）
0层0buff: 108-324            1层-86
1层2buff：137-411            2层-101-302
                             3层-115-346！
```

> 这个是选了40的那个直伤天赋，加上卡的一层的co（那么应该是80的加成）
>
> 确实是3个异常是158，这是实测的

### The three-point set is what settles it

`24基伤` with one 40% source, at one, two and three status types. Solved as an
absolute — `co_base = (hit/E − base) / (0.40 × types)`, with `E = 3.00` fixed by
`24 × 3 = 72`:

| types | measured | implied `co_base` |
| --- | --- | --- |
| 1 | 86 | 11.67 |
| 2 | 101 | 12.08 |
| 3 | 115 | 11.94 |

**12** — the weapon's own unevolved base, not the 24 on the panel. Three points
over-determine two unknowns, which is why this set decides and a single reading
cannot: the first attempt at this measurement gave one point with a MISCOUNTED
stack, and `(24 + 0.40×24×1)×3 = 100.8` is the same 101 as
`(24 + 0.40×12×2)×3`. Two errors cancelling produced a clean-looking answer of
24 and nearly moved the data. A stack count is the easiest thing in this
measurement to be wrong about; a slope is not.

### Every reading, against `co_base = 12`

`(base_total + 0.40 × 12 × types) × 3`:

| build | base | types | predicted | measured |
| --- | --- | --- | --- | --- |
| Fortress Salvo + Fresh Havoc | 36 | 0 | 108.0 | 108 |
| Fortress Salvo + Fresh Havoc | 36 | 2 | 136.8 | 137 |
| Fortress Salvo | 24 | 1 | 86.4 | 86 |
| Fortress Salvo | 24 | 2 | 100.8 | 101 |
| Fortress Salvo | 24 | 3 | 115.2 | 115 |
| Fortress Salvo, 40% talent + 40% card | 24 | 3 | 158.4 | 158 |

The last row is the one that was PREDICTED before it was confirmed. It arrived
labelled `2buff` and no combination of two status types reaches it — two gives
129.6, and reading the CO base as 24 gives 187.2. Only `0.80 × 3 × 12` lands on
both halves at once, 158.4 and 475.2 against a measured 158 and 475; asked
about it, the target had three.

### The two perks add through different doors and neither reaches the term

**Fortress Salvo** (tier 2, `+12`) is a panel add: `add_flat_base_damage(12, 0)`,
so `base_vector.total()` becomes 24 while `co_base` stays 12 and
`co_base_fraction` is 0.5. **Fresh Havoc** (tier 4, `+6` ×2) never touches the
panel at all — it is a `stacking_buff` granting `FlatBaseDamage`, which lands in
the live base-damage bucket at fire time. Two mechanisms, one answer, and the
36-base rows are what show the second one: the buff moves the hit from 108 to
what a 36 base deals and moves the CO term not at all.

Nothing changed. The entry is here because a rule that was inferred and a rule
that was measured are not the same rule to the next person deciding whether to
trust it — and because the near-miss above is the record of how close a single
reading came to overturning a correct one.
