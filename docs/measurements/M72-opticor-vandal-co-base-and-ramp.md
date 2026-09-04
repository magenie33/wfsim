# M72 — the Opticor Vandal's explosion takes Condition Overload on a base of 400, and its charge ramps ×1 → ×2 across the firable part of the bar (owner, 2026-09-02) ⚠ RECORDED, NOT IMPLEMENTED

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Opticor Vandal, full charge**, Serration at +165%, and the merged GunCO rate
at 40% per status type from each of two sources. Rows are written
`sources–status types`, so 2–2 is a +160% CO bracket and 2–1 is +80%; the
decode below is what pins that reading of the labels. Each cell is
`direct + radial`.

| build | direct | radial |
| --- | --- | --- |
| unmodded panel | **388** | **200** |
| 0–0 | **1027** | **530** |
| 2–1 | **1337** | **850** |
| 2–2 | **1647** | **1170** |

And one more, which is a different fact rather than a fourth row: a target
caught by the explosion **without being the one directly hit takes 530 whatever
it is carrying**.

### 1. 388 IS 400 QUANTIZED, and the panel is already the full-charge figure

The entry's base is the published 400 (40 Impact / 280 Puncture / 80 Slash).
Quantization is 1/32 of the modded base (`damage::QUANTIZATION_DENOMINATOR`),
and this split lands at 10% / 70% / 20% — 3.2 / 22.4 / 6.4 thirty-seconds,
each to the nearest step is 3 / 22 / 6, **31 of 32**. Every reading is the
plain arithmetic times 31/32:

| build | bracket | 400 × bracket | × 31/32 | measured |
| --- | --- | --- | --- | --- |
| unmodded | 1.0 | 400 | 387.5 | **388** |
| 0–0 | 2.65 | 1060 | 1026.875 | **1027** |
| 2–1 | 3.45 | 1380 | 1336.875 | **1337** |
| 2–2 | 4.25 | 1700 | 1646.875 | **1647** |

**It is the same rule as M66's ×33/32, and the numerator is the IPS split.**
The Ballistica Prime's 2.2 / 23.6 / 17.2 over 43 is 1.64 / 17.56 / 12.80
thirty-seconds, nearest 2 / 18 / 13 = **33**; this weapon's rounds the other
way to 31. One rounding rule, two numerators — neither is a weapon quirk.

**The panel figure IS the full-charge hit**, and that is the half of M66 that
does not reach here: the Ballistica Prime's full charge deals TWICE its
published charged damage (M66 §2), and this weapon's deals exactly what the
infobox states. Its ×2 is somewhere else — §4.

### 2. The direct hit's Condition Overload is ordinary

`1647 − 1027 = 620`, and `620 / 1.6 = 387.5`; `1337 − 1027 = 310`, and
`310 / 0.8 = 387.5`. The term reads the entry's own base and joins the same
additive bucket Serration is in — `additive_with_base_damage` at 100%, which
is what the entry already carries.

### 3. THE EXPLOSION'S TERM READS 400 — 200% OF ITS OWN BASE

The radial is a single element, so 200 is 32 of 32 and there is no
quantization offset to disentangle:

```
radial = 200 × (1 + 1.65) + 400 × co
  0–0   530 + 0   = 530    ✓
  2–1   530 + 320 = 850    ✓
  2–2   530 + 640 = 1170   ✓
```

`1170 − 530 = 640 = 400 × 1.6` and `850 − 530 = 320 = 400 × 0.8`. The term's
base is **400**, which is the direct hit's published base and exactly 200% of
the explosion's own 200.

### 4. THE CHARGE RAMPS ×1 → ×2 ACROSS THE FIRABLE 60% OF THE BAR

Three unmodded releases, and the fraction is against the full charge's 388+200:

| release | direct | radial | of a full charge |
| --- | --- | --- | --- |
| the earliest the weapon will fire | **196** | **101** | 50.5% |
| near full, by hand | **369** | **190** | 95.2% |
| full | **388** | **200** | 100% |

**The bar has a dead zone, and the wiki states it on both weapons' own pages** —
*"Can be fired at **40%** of maximum charge, but with less damage"* (Opticor),
*"Can be fired at 40% maximum charge, but with less damage"* (Opticor Vandal).
So nothing fires below 0.40 of the bar, and at that floor the shot is **half** a
full charge. That half is already in the roster as its own entry:
`opticor_vandal_quick` is 200 + 100 against the charged 400 + 200, and
`opticor_quick` is 500 + 200 against 1000 + 400.

The multiplier is therefore `1 + progress` over the FIRABLE part of the bar,
against the quick form's base:

```
mult(bar) = 1 + (bar − 0.4) / 0.6
  bar 0.40  ->  x1.000  ->  200 + 100   (194 + 100 on the pop)
  bar 1.00  ->  x2.000  ->  400 + 200   (388 + 200 on the pop)
```

and both partial readings place themselves on it. `196 / 387.5 = 0.5058` is bar
**40.7%** — the threshold plus about 4 ms of a 0.6 s charge, which is what
letting go "as early as possible" costs. `369 / 387.5 = 0.952` and
`190 / 200 = 0.95` are bar **94%**, which is what "near full" by hand looks
like. Reading it the other way, for anyone holding the trigger: **every 1% of
the bar past 40% is worth 1.67% of the quick shot, and the whole 60% is worth
exactly double.**

**WHETHER THE RAMP IS STRAIGHT IS NOT MEASURED.** Two readings with two unknown
release points fit any monotone curve through the same two ends; the straight
line is imported from M66's Ballistica reading, not found here. What settles it
is one release at a KNOWN point: half the firable part (bar 0.70) predicts
**291 + 150** unmodded, a quarter (bar 0.55) predicts **242 + 125**.

**Where it differs from the Ballistica Prime.** M66 found the ramp topping out
at ×2 and then a DISCONTINUITY above it — a full charge is a different attack
dealing twice the published charged damage, 160 against the ramp's 80. Here the
ramp's ×2 end IS the full charge. Two weapons, the same ramp, and only one of
them jumps at the top.

### The catalog row, and what its columns are

Read from `?action=raw`, header and row verbatim:

> `!Weapon!!Attack Name!!Projectile Type!!Attack Unmodded Damage!!Actual CO Damage Bonus at +100%!!CO Damage Bonus Relative To Base Damage!!Math/Behavior Type!!Notes`
>
> `|{{Weapon|Opticor Vandal}}||Charged Hitscan Radial Attack||AoE||200||400||200%||Adding||Radial hit only receives CO bonus on target directly hit by laser. CO-bonus scales off hitscan damage. AoE does not scale off multishot.`

**The fifth column is an ABSOLUTE, not a rate**: the points this part gains per
+100% of CO. It reads 400 and the measurement pays 400 × 1.6 = 640. The sixth
column is that over the part's own base, `400 / 200 = 200%`. The Opticor's row
is the same shape at `AoE || 400 || 1000 || 250%`.

The Notes clause is confirmed independently by the 530: an enemy the laser did
not hit gets the explosion with **no** CO term, however many statuses it
carries. The sim's single-target arena is always the directly-hit target, so
this changes no answer here — it is why the 400 may be attached to the radial
without a target-side condition.

### NOT IMPLEMENTED — the change set, deliberately held

The engine gives the explosion the term at 100% of its own base, so a 2–2 build
reads **850 where the game pays 1170**, understated by 27% on exactly the
builds that carry the mod. Four things, and three of them are prose that has
gone stale:

1. **`RadialSpec` has no `co_base_fraction`.** A radial's `co_base` is always
   its own vector total (`weapons_data.rs`, `co_base: v.total()`), and only an
   evolution's flat damage can part the two. The missing piece is the yaml
   field, nothing else: `RadialBase` already carries its own `co_base` and
   `dummy.rs` already computes the explosion's term off `r.co_base_fraction`,
   beside the direct hit's and against the same counters.
2. **"`co_base_fraction` is one number per ENTRY" is no longer true of a
   radial**, and it is written in `opticor.yaml`, `opticor_vandal.yaml` and
   `docs/CATALOGS.md` as the reason the fraction cannot be expressed. It is
   the reason the seven AoE parts CATALOGS.md lists are all understated —
   Ambassador 75%, Ferrox / Tenet Ferrox 350% / 333%, both Opticors 250% /
   200%, Trumna 164%, Mutalist Cernos 4100%.
3. **`opticor_vandal.yaml` transcribes the row wrong**: `Hitscan | 300 | 600 |
   200%` against the wiki's `AoE | 200 | 400 | 200%`. The ratio survived and
   the absolutes did not, and the 300 contradicts the same file's
   `magnetic: 200` four lines above. The Opticor's `400 | 1000 | 250%` is right.
4. **CATALOGS.md's multishot table says Opticor / Opticor Vandal are not in the
   roster.** Both are, and both radials already carry `takes_multishot: false`
   (Trumna likewise).
5. **The ramp is not modelled at all.** The roster carries its two ENDS as
   separate forms and nothing between, so a release the player can actually
   hold is a build the calculator cannot state. It is the two Opticors, the
   Lanka and every other weapon whose partial charge is "a separate entry".
6. **A QUICK FORM DOES NOT PAY FOR THE 40% IT MUST CHARGE.**
   `opticor_vandal_quick` declares no `charge_seconds`, and `inherits:` merges
   TOP-LEVEL keys only — the child has its own `attack:`, so nothing inside it
   is inherited. The sim fires it every `1 / 2` = 0.5 s where the game cannot
   release it before `0.4 × 0.6` = 0.24 s of charge, a 0.74 s cycle; the
   Opticor's is 1.0 s modelled against `0.8 + 1.0` = 1.8 s. The sign matters:
   the missing draw makes the quick form look BETTER than the charged one, so
   this is a search's answer and not only a number.

### Still open

The session is not finished, and nothing above is implemented on purpose: the
remaining readings land first and the change set is applied in one pass, so
what is decided and what is not stay separable.

**The ramp's SHAPE**, which §4 states is assumed rather than measured, and
which one release at a known point settles.

**The Opticor proper is unread.** Its row says the explosion's term reads 1000
— the direct hit's whole base, 250% of the radial's own — and that is the same
claim as the Vandal's, not a second one. It needs the same four readings. Its
own page gives it the same 40% floor and its quick entry is already the same
half, so the ramp is the one thing it does not need read twice.
