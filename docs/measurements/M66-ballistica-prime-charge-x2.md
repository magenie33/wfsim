# M66 — the charge multiplier is (1 + progress), and a full charge is the ×2 end of it ✅ (owner, 2026-08-30)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Eight readings on a **Ballistica Prime**, all with **Headcracker installed**
(+3 base damage, and it cannot be uninstalled). They settle three things the
published stats do not say, and they confirm two the engine already did.

### The readings

Uncharged panel **43** (2.2 Impact / 23.6 Puncture / 17.2 Slash), charged panel
**158**. "Near full" is the highest the owner could reach by hand without the
bar filling.

| build | no charge | near full | full |
| --- | --- | --- | --- |
| no damage mods | **44** | **85** | **160** |
| +220% base damage | **142** | **273** | **512** |

GunCO, at +220% base damage, written `mods × status types`:

| | 3×2 | 2×2 | 1×3 | 3×1 | 2×1 |
| --- | --- | --- | --- | --- | --- |
| Normal Shot | **241** | **208** | **191** | | |
| Charged Shot | **709** | **643** | | **610** | **577** |
| Incarnon Form | | | | **5864** | |

### 1. ON THIS WEAPON, THE CHARGE MULTIPLIER IS (1 + PROGRESS) — THE BAR, NOT THE CLOCK

**ONE WEAPON, NOT A CLASS.** Everything in this entry is the Ballistica
Prime's, and it is the only weapon this has been found on (owner). No other
charge weapon has been measured for it, and none may be given it without its
own reading — a bow's drawn shot is calibrated against golden values that this
would break.

Releasing early multiplies the Normal Shot's OWN base by (1 + progress), and
the number tracks the PROGRESS rather than the time held (owner). **The
evolution's flat +3 rides BESIDE that multiplier**, which is M79's law —
`base × attack multiplier + flat`, measured four ways on a Magistar — and one
release point reproduces both readings exactly:

```
(40 × 1.99 + 3)       × 33/32 =  85.2  ->  85   (measured 85)
(40 × 1.99 + 3) × 3.2 × 33/32 = 272.6  -> 273   (measured 273)
```

**The bar was nearly full when both were taken** (owner), which is what 1.99
says. Folding the +3 inside instead needs a 92% bar and still misses the second
row by 1. The two shapes are otherwise near-indistinguishable HERE, because the
flat add is 7.5% of this weapon's base: on the base Ballistica, whose own
Headcracker is +30 against a base of 25, they differ by 22% at half charge.

At the top of the ramp the shot becomes the CHARGED attack, which is where the
discontinuity is: 40 × 2 = 80 against the 160 a full charge actually deals.

### 2. A FULL CHARGE DEALS TWICE THE PUBLISHED CHARGED DAMAGE

The wiki's infobox and DE's export both give 76 per projectile. The game deals
**152**: with Headcracker's +3 that is a base of 155, and

```
155   × 33/32 = 159.8  -> 160
155   × 3.2 × 33/32 = 511.5  -> 512
```

The ×2 is on the PUBLISHED base and not on the evolution's flat add — 152 + 3
reproduces both rows where (76 + 3) × 2 = 158 misses both. Neither of this
weapon's other two attacks is doubled: the Normal Shot's 44 is 43 × 33/32 and
the Incarnon form's 5864 is 833 × 3.2 × 2.2 exactly.

### 3. THE CO TERM'S BASE IS DOUBLED WITH IT — 80, NOT 76

Two unknowns fall out of the four charged GunCO rows on their own, because each
row is `(B + 0.4·a·b·C) × 33/32`:

```
(3×2) - (2×2):  0.8·C = 687.5 - 623.5 = 64   ->  C = 80.0
back-substituted:                             ->  B = 496 = 155 × 3.2
```

B confirms §2 independently of the two full-charge pops. C = 80 is the
UNCHARGED 40 with the same ×2 on it, which is the catalog's own sentence
("uses uncharged damage value") surviving the multiplier. As a fraction of this
attack's 152 that is 40/76 = **0.5263**, and the catalog's `50%` is it rounded —
so `co_base_fraction` moved off the published number and onto the measured one.

### What was already right

**Quantization** (M57), on two independent readings: this weapon's 5/55/40
split lands on 2 + 18 + 13 = 33 units of the ModdedBase/32 scale, a flat +3.1%,
and 43 → 44.34 → **44** and 137.6 → 141.9 → **142** are both exact. The
Incarnon form is mono-Slash, so it sits at 32/32 and its 5864 confirms the
other end: a quantization that gained anything there would have missed it.

**The rest of these readings are M80.** Three more builds on this weapon —
an element, two per-type physical mods, and a Galvanized Shot ladder — settle
the composition of quantization's DENOMINATOR rather than anything about the
charge, so they are their own entry. The one thing they say about THIS entry
is that the ladder re-derives **C = 80** from a direction §3 did not use.

### Not settled by this

**Nothing about how far this reaches, except where it does NOT.** The base
Ballistica and the Rakta Ballistica were measured for the same thing and
**neither has it** (owner) — the ×2 is one weapon's, not the family's. Every
other charge weapon in the roster is untested and stays as published: a bow's
drawn shot is calibrated against golden values this would break.
`data/notes.yaml` `charge_x2_is_the_primes_alone` is what the three entries
carry.

**The other two read their published damage, and quantization is the whole of
the difference** (owner). Four readings, and every one falls out of M57 with
nothing else added:

| weapon | attack | published | IPS split | units of base/32 | deals |
| --- | --- | --- | --- | --- | --- |
| Ballistica | Burst Shot | 25 ×4 bolts | 25 / 50 / 25 | 8 + 16 + 8 = **32** | 25 × 32/32 = **25** |
| Ballistica | Charged Shot | 100 | 10 / 80 / 10 | 3 + 26 + 3 = **32** | 100 × 32/32 = **100** |
| Rakta Ballistica | Burst Shot | 75 ×4 bolts | 25 / 50 / 25 | 8 + 16 + 8 = **32** | 75 × 32/32 = **75** |
| Rakta Ballistica | Charged Shot | 300 | 5 / 90 / 5 | 2 + 29 + 2 = **33** | 300 × 33/32 = 309.4 → **309** |

**The Rakta is the same weapon showing quantization pay nothing and pay
3.125%**, and the only thing that changed is the split — which is what M57 says
decides it. The base Ballistica pays nothing on either attack, so the pair is
a control on the Prime's own +3.125%: the gain is not a property of this
family, of a charged shot, or of a bolt.

**Charging CONCENTRATES on those two, and the Prime is the one it does not.**
The base and the Rakta drop from 4 bolts to 1 when charged, and the burst's
whole total goes into that bolt: 4 × 25 = 100 and 4 × 75 = 300, with the single
bolt's own crit, status and Puncture share bought instead of damage. **The
Prime keeps all four** (`multishot: 4.0` on both of its non-Incarnon entries,
and the wiki's infobox prints 160 and 304 for the two modes against 40 and 76
a bolt), so its charge is a straight damage gain and not a trade. Nothing about
the family's shape argues for the ×2 or against it — the four full-charge pops
are what settle it, and they are §2.

**The ramp itself is not modelled.** A partial charge is a shot this engine
cannot fire — the entry says so in `unmodeled:`. It is dominated at both ends
on this weapon (a 50% charge is 66 damage per 0.7 s against 44 per 0.3 s and
160 per 1.1 s), so the two ends are the two builds worth ranking, which is what
the roster carries.

**The arsenal's charged panel reads 158**, i.e. (76 + 3) × 2, while the damage
is computed from 152 + 3. The panel doubles the evolution's flat add and the
hit does not; ours shows what the hit uses.
