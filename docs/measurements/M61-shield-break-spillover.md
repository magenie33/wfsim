# M61 — a shot that BREAKS a shield keeps killing through it (owner, 2026-08-27) ⚠ OPEN

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

An unmodded **Laetum in its BASE form** (the same gun as [M60](#m60--headshot-bonuses-add-and-the-crit-tier-ladder-holds--owner-2026-08-25): 160 = 64 Impact + 96 Slash, crit multiplier 2.20x) against a **level 1 Corpus Crewman, no Steel Path — 120 shield, 90 health**. Two numbers where the pop-up showed two.

The owner's report, verbatim:

> 刚刚我发现了一个问题，那就是敌人的超短暂的破盾保护好像很多时候是不触发的，或者选择性触发（例如震地的出场在会触发）。但是打枪的时候，例如我造成1w伤害，这个人只有100盾100血，那么这一枪会直接秒了。

```
奏凯普通
96slash+64impact=160
critical damage 2.2x

1级crew man无钢铁加成
shield 120
health 90

无mod
爆头
暴击 120+1776=1896
无暴击 120+620=740

无盾爆头
暴击 2015
无暴击 860

身体
爆击 341+12
无暴击 158+2

无盾身体
无暴击 160
暴击 353

有+220+165基伤
爆头
暴击  120+ 9920
无暴击 120+4316

无盾爆头
暴击 10160
无暴击 4556

身体
爆击 1630+80
无暴击 743+33

无盾身体
无暴击 776
暴击 1710
```

### What it settles, and it does not need a model to settle it

**A HIT THAT BREAKS A SHIELD PAYS THE REST INTO HEALTH, IN THE SAME INSTANT.**
Every one of the four headshot lines shows the shield's `120` beside a health
number of 620 to 9,920 — one trigger pull, two pop-ups, and the target has 90
health. It dies to that shot.

`dummy::TargetState::apply` does neither half:

```rust
if self.shield > 0.0 {
    shield_part = rest * mit.disrupt_amp;   // the WHOLE non-Toxin hit
} else { … }
…
self.shield = 0.0; // no spill
self.gate_until = now + 0.1;
```

so a 10,000-damage hit on a 120-point shield is charged **entirely** to the
shield, 9,880 of it is discarded, health takes **nothing**, and every instance
for the next 0.1 s is multiplied by 0.05 unless it is a direct weakpoint hit.
The same shot that kills in game leaves the target at full health here, and the
follow-up shot is quartered twenty times over. Two separate faults:

1. **NO SPILL.** The excess past the shield's remaining points is thrown away.
2. **THE GATE IS NOT THE ONE THE GAME APPLIES.** M1 asked whether Toxin's
   shield-bypass damage is reduced by the gate and never resolved; this says
   the gate does not stop the instance that broke the shield at all.

**IT HAS NEVER SHOWN UP ON THE BOARD**, and that is why it survived: all three
entries in `data/enemies/` — `thrax_centurion`, `corrupted_heavy_gunner`,
`demolisher_devourer` — carry `shield: 0`. Every ruler, every golden test and
every board row is fought against a target with no shields, so the entire
Corpus half of the mitigation model is unexercised.

**AND THE GAME SHOWS IT AS TWO NUMBERS**, which is the shape
`crate::record` was built for on the same day: one row per number the game pops,
each with its own pool and its own mitigation ledger. The Toxin split
(`Pool::Shield` / `Pool::Health`) is already that mechanism; this is a second
member of it, reached by overflow rather than by bypass.


### The second capture: the same fight with the shield ALREADY DOWN 41

The owner's follow-up, on the same Laetum and the same level 1 Crewman with
`+220% +165%` base damage, after knocking 41 points off the shield — so 79
remained:

```
220+165基伤
打掉41的盾
爆头
暴击  79+10002
无暴击 79+4398

身体
爆击 1628+82
无暴击 741+35
```

**It settles the body rule outright.** `health = 0.05 × (damage − shield
remaining)` and `shield shown = damage − health`, at TWO different shield
values and both crit tiers:

| | damage | shield 120 | `0.05 ×` | shield 79 | `0.05 ×` |
|---|---|---|---|---|---|
| white | 776 | 743 + **33** | 32.80 | 741 + **35** | 34.85 |
| crit | 1710 | 1630 + **80** | 79.50 | 1628 + **82** | 81.55 |

**And it settles the head rule's shape**, which the first capture could only
state as a constant: the shield shows its REMAINING POINTS and the hit is
charged **twice** them.

| | damage | shield 120 | `− 240` | shield 79 | `− 158` |
|---|---|---|---|---|---|
| white | 4556 | 79 + **4316** | 4316 | **4398** | 4398 |
| crit | 10160 | **9920** | 9920 | **10002** | 10002 |

Twelve points across two mod levels, two crit tiers and two shield values. The
cost tracks the shield exactly — 240 at 120 points, 158 at 79 — so the `2` is a
property of the rule and not a coincidence of the first capture.

### …and it moves the open question OFF the shield entirely

Fitting the four "no shield" headshot readings against their body counterparts
gives, for both crit tiers:

```
white:  head = 6.0000 × body − 100.0
crit:   head = 6.0022 × body − 103.8
```

A slope that is the same at both tiers, and a flat offset that does not scale
with the damage mods. **That offset is `2 × 50`** — which under the head rule
above is a shield of 50 points, so those readings were taken with the shield
NOT at zero. Adding it back:

| | corrected reading | wiki `Head: 3.0x` + [M60](#m60--headshot-bonuses-add-and-the-crit-tier-ladder-holds--owner-2026-08-25)'s ladder |
|---|---|---|
| head crit | 2,115 / 10,260 | **2,112 / 10,243** — 0.14% |
| head white | 960 / 4,656 | 480 / 2,328 — **exactly ×2** |

So the CRITICAL headshot reproduces the wiki's multiplier and M60's ladder to
inside M60's own unexplained +0.19%, and the WHITE headshot is exactly twice
what the same two say it should be. This engine computes 480 for that shot
(`160 × 3.0`, verified through `/api/simulate`'s hit account), so if the ×2 is
real it is wrong on every white headshot in the app — which on a low-crit build
is most of its damage.

**M60 ASKED FOR THIS EXACT NUMBER AND NEVER GOT IT.** Its own closing paragraph
names the three pop-ups that would close its 0.19%: *"a body white (should be
160), a body yellow (352), and a head white (240)"*. Its capture turned out to
be yellow/orange, so no white headshot was ever measured — and this is the
first one.

### What would close it

**ANSWERED: neither.** The readings were taken through the unit's HELMET — see
the section above. The engine's `Head: 3.0x` is right and nothing in the head
path needed to move. The BODY rule is
implemented (`ENEMY_SHIELD_GATE_LEAK`, `dummy::TargetState::apply`) and
reproduces all eight of its numbers; the head path still charges the shield once
rather than twice, and is therefore known to be off by one shield pool on a
weakpoint hit against a shielded target.

### RESOLVED: the headshot readings were on the HELMET

A Crewman wears one, it is its own destructible hitbox, and while it is on it
takes MORE than the head beneath it — destroy it and headshots read the
ordinary `Head: 3.0x` (owner, 2026-08-27). Every head reading above was taken
through a helmet, and this engine has one head per body and no way for a part
to be destroyed and reveal another, so it aims at the bare head from the first
shot.

**The engine's head multiplier was never wrong.** An unmodded body shot is 160,
this sim computes `160 × 3.0 = 480` for the head, and the capture read 860.

WHAT THE HELMET ACTUALLY IS, IS NOT MEASURED (owner, 2026-08-27). Its
multiplier, whether it has a health pool of its own, how much, and what
destroying it costs are all unknown; the readings are consistent with something
near 6x and that is an inference from four numbers taken through it, not a
figure anybody has read off a page. Nothing should be built on it — the entry
here exists so the next person does not re-derive the anomaly from scratch.

**AND THE CRIT READING'S APPARENT AGREEMENT WAS A COINCIDENCE**, which is worth
recording because it nearly bought a wrong conclusion. `3.0 × 4.4` (the wiki's
head multiplier under M60's critical-headshot ladder) and `6.0 × 2.2` (a 6x
helmet under a plain crit multiplier) are **both 13.2**, so the critical
headshot number cannot tell the two apart and it matched the ladder to 0.14%.
Only the WHITE headshot separates them — 480 against 960 — and it says helmet.

The gap is admitted on the unit (`data/enemies/crewman.yaml`, `unmodeled:`).
Modelling it would need two things this engine does not have — a part with its
own health that can be destroyed, and a part REVEALED by another one breaking —
and a measurement of both, which is the part that does not exist yet.

### Still open: a flat ~100

`head = 6.00 × body − 100` fits all four helmet readings, across two mod levels
and both crit tiers, and the `−100` does not scale with a +385% damage bucket.
It is not a multiplier, not the shield (those readings are single numbers, and
a shielded hit pops two), and not rounding — it is 11.6% of the smallest
reading. It appears only on the head; the four body numbers are exact.

The likeliest explanation is that it is not real: the unmodded capture and the
modded one were separate sessions, and a line fitted through two different
loadouts has an intercept that belongs to neither. **One capture would settle
it** — three damage levels in ONE session, same evolutions and arcanes, only
the mods changed, white headshots. Three points on a line through the origin
means the `−100` was an artifact of the fit.

It changes no model either way while the helmet is unmodelled, so it is a loose
end rather than a bug.

### What is NOT settled — three arithmetic facts that need the owner

The body lines and the head lines do not fit one model, and the mismatch is not
noise:

**(a) The head is exactly 6.0x the body, at BOTH crit tiers.** Fitting
`head = S × B + c` across the unmodded and the +385% base-damage captures
(`B` = 1 and 4.85) gives, for both:

| | S | S ÷ matching body number | c |
|---|---|---|---|
| no crit | 960 | 960/160 = **6.00** | **−100** |
| crit | 2,116 | 2116/353 = **5.99** | **−101** |

M60 measured this weapon's headshot multiplier at **1.5x** on a Techrot Babau
and confirmed the wiki's critical-headshot ladder, under which a head CRIT
should be `1.5 × 4.4 = 6.6x` the base while a body crit is `2.2x` — a head/body
ratio of **3.0 for crits and 1.5 for whites**, not 6.00 for both.

**(b) There is a constant −100 on the head lines and none on the body lines.**
The body numbers scale by exactly 4.85 between the two captures
(160→776, 353→1710); the head numbers scale by 5.30 and 5.04, and only a flat
−100 reconciles them. Nothing in the target's sheet is 100.

**(c) The body lines lose nothing to the shield and the head lines lose 120.**
Comparing TOTALS: `158+2 = 160` and `341+12 = 353` are exactly the no-shield
body numbers, while `120+620 = 740` is exactly 120 below the no-shield 860 (and
the same 120 for the other three head lines). Under any single spill model the
shield should cost the same on both.

**ANSWERED by the second capture above** (owner, 2026-08-27): the pair is
`shield + health` in that order, the body lines were fired at a full 120, and
the head lines' apparent inconsistency was the shield being charged TWICE its
points rather than anything about an evolution. What is left open moved off the
shield entirely — see "What would close it".
