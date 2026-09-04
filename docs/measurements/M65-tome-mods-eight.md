# M65 — the eight Tome mods, and two readings that were wrong about all of them ✅ (owner, 2026-08-28)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

All eight were transcribed months ago and all eight were filed as paying
nothing. Four of them pay.

### The two readings that were wrong

**"Allies within Affinity Range" excludes you.** It does not. Lohk Canticle and
Fass Canticle were out of scope on the reasoning that this arena has one Tenno
and every point of those cards is spent on other people; the owner plays the
weapon and says the wielder gets it:

> 因为这个有个是可以增加射速的，我们是可以吃到的，因为我们现在已经存在tenno了

The wiki settles nothing either way — it says "ally" and does not say whether
the caster is one — so the measurement decides. Lohk is +7.5% to +30% fire rate
for 15 s on kill, and it is worth **+11.7%** in a fight with kills in it.

**"A drop is not a damage model."** It is now. `engine::ammo` (M64) turns a kill
into what it leaves on the floor, so Khra Canticle's Universal Orb is no longer
refused for being a drop. What refuses it is what the orb CONTAINS: health and
energy, and this arena gives the player neither.

### What each of the eight is worth

| card | what it does | here |
| --- | --- | --- |
| **Lohk Canticle** | +30% Fire Rate to allies on kill, 15 s | **+11.7%** |
| **Jahu Canticle** | −5% Armor and Shields of enemies in range, on kill | **+15.8%** |
| **Vome Invocation** | +4% Ability Strength per hit, 15 stacks | **+10.5%** with Roar |
| **Ris Invocation** | +4% Ability Duration per hit, 15 stacks | **+7.6%** with a 30 s Roar |
| Netra Invocation | +4% Ability Efficiency per hit | nothing — no ability is CAST here |
| Xata Invocation | +1 Energy Regen/s per hit | nothing — no energy pool |
| Fass Canticle | ally shield recharge on kill | nothing — nobody shoots back |
| Khra Canticle | 12% Universal Orb on death | nothing — no health or energy to fill |

Measured solo against eleven level-25 Corrupted Heavy Gunners, cycle mode, one
card at a time. The four that pay nothing measure exactly zero, which is the
control the table needs to mean anything.

### Jahu, and where Affinity Range is measured from

*"Killing enemies reduces the Armor and Shields of other enemies within Affinity
Range"* — and Affinity Range is a 50 m radius **around the squad** (wiki
`Affinity`), not around the corpse. That is what makes it cheap: which body died
does not matter, only that one did, so it is a count rather than a position.

THE SHARES COMPOSE, each kill taking 5% of what is LEFT — the rule every other
strip in this engine follows and the only one under which repeated kills cannot
take armour past zero. UNCONFIRMED against the game: the card states a
percentage and no stacking rule, and the flat reading would make it worth almost
nothing. It compounds with itself, which is most of the +15.8%: a stripped body
dies sooner, and a kill strips again.

The SHIELD half is admitted rather than modelled — `Mitigation` carries an
armour multiplier and nothing that shrinks a shield POOL mid-fight.

### Vome and Ris, and the seam they cross

A mod belongs to the BUILD and an ability to the FIGHT, and these two are a mod
that raises the fight's own knob. They meet in `DummyParams::from_panel`, which
is the one place that holds both — so the Arena now carries the unresolved
PICKS and the strength they were resolved at, and `from_panel` resolves them
again when a card asks.

RE-RESOLVED RATHER THAN RESCALED, for a reason that is easy to miss:
`abilities_data::resolve` settles the same-family contest BY the resolved value,
so a bonus big enough to make a Helminth Roar beat a Rhino's has to be in hand
before the winner is picked. Nothing re-resolves without a card asking, so every
other fight takes the arena's list byte for byte.

TAKEN AT THE CAP, and on this weapon that is nearly exact rather than a
convention: one orb strikes six times and each strike reaches
`floor(3 × multishot)` bodies, so a bare build lands 18 hits with the FIRST orb
against a 15-stack cap, and the meter throws another about every 18 s.

### Still open

**A buff can depend on the weapon being in your hands.** The Invocations' stacks
pay only while the tome is out — swap away and they go dormant, swap back and
they pay again (owner). This engine has one weapon and it is always out, so the
question cannot arise; it is recorded in docs/UNMODELLED.md because it becomes a
real decision the day weapon swapping lands.
