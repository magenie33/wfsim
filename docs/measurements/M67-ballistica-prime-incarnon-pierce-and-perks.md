# M67 — the Ballistica's Incarnon form pierces bodies, and its two tier-2 perks keep different clocks ✅ (owner, 2026-08-30)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

Three readings on the **Ballistica Prime**, all of them about things the stat
block does not carry.

### THE INCARNON FORM HAS INFINITE BODY PUNCH THROUGH, and its stat reads 0

**Five enemies in a line, all struck, no falloff and no stop.** The arsenal
shows `Punch Through 0.0 m` and the wiki's module agrees, so the number is not
where this mechanic lives — the EVO1 card is: *"Fire cross-shaped projectiles
that punch through enemies"*, printed once for the whole family.

**The Punch Through page never mentions the Ballistica**, in either of its two
lists, which is the only reason this was ever modelled as 0. Its definition of
the class is what the weapon is: *"Some weapons that shoot wide projectiles or
a stream of particles possess infinite body Punch Through … pierce an unlimited
amount of enemies, but not level geometry, objects, or barriers."* An X-shaped
projectile is a wide one, the Dread's and the whole Paris family's Incarnon
forms are on that list, and a community-curated gallery missing an entry is a
gap in the gallery rather than a fact about the weapon.

**Written as `infinite` now, not as a big number.** The word is the statement
and `space::INFINITE_BODY_PUNCH_THROUGH_M` is what the engine holds — finite
on purpose, because a budget that survives every body is spent as flight by
`dissipation_point` and an infinity there is a NaN epicentre. Thirty entries
that carried the number now carry the word.

### TWO PERKS, TWO STACK CLOCKS, and the tier makes them a choice

Both are tier-2 options on the same weapon and they decay differently:

| perk | stacks | clock |
| --- | --- | --- |
| Headcracker | +7.5% fire rate, 10x | INDEPENDENT — each stack carries its own, and they expire one by one |
| Prolific Perforation | +10% crit chance, 8x | CLASSIC — the whole pile goes at once when the window lapses |

Headcracker was already `per_stack_expiry`. Prolific Perforation was not
modelled at all: its clause sat as `out_of_scope` with the reason "one target",
which stopped being true when the arena grew a formation and punch through
started crossing bodies (`space::struck_along`). It is a real buff now —
`BuffTrigger::PunchThrough`, one stack per BOLT that left the body it hit, so a
four-bolt shot into a line earns four — with `BuffDecay::AllAtOnce` and the
crit chance in the bracket its own card names ("additive to other sources of
Critical Chance such as Pistol Gambit").

**AND THE ADMISSION WAS THE WRONG SHAPE ANYWAY.** "Cannot be triggered in this
fight" is a sentence the model should not need: a weapon with no punch through
and a fight with one body earn no stacks by the mechanic itself, and saying so
separately is a second implementation of the rule that can drift from the
first. What the entry now carries is the effect; the number it is worth against
a lone target is zero, and that falls out.

### AND THE PROJECTILE HAS WIDTH — an assumption, flagged as one

The X-shaped projectile is a horizontal line rather than a point, and it
**headshots everything it sweeps**: two rows of enemies 2 m apart, shot down
the middle, take a head hit each (owner). The engine had no width at all — a
shot was a ray that struck bodies within `BODY_RADIUS_M` of its centre line —
so `projectile_width_m` is new, defaults to 0, and 0 is a ray.

**4 m is a working figure and NOT a source.** Neither the wiki, the module nor
DE's export publishes a width for any weapon in the class, on this weapon or on
the Arca Plasmor and Catchmoon the same pages call "wide projectiles". A
measurement replaces it in this weapon's yaml and nowhere else.

The HEADSHOT half needed nothing: punch through already carries the aimed
pellet's hit location down the line ("the same round still flying in a straight
line: this plane holds it at one height"), and a swept body arrives by the same
path. What the width changed is only WHO is on the shot. Down the middle of
those two rows the cycle measures 91,754 DPS against the charged shot's 5,693,
which is the sweep and nothing else — no body is on the centre line at all.
