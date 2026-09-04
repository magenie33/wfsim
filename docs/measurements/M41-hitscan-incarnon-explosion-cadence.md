# M41 — a hitscan Incarnon's explosion fires ONCE PER TRIGGER PULL ✅ (owner, 2026-08-11)

*Protocol and setup: [MEASUREMENTS.md](../MEASUREMENTS.md). Cross-references `M<n>` are files in this folder.*

**Question.** The wiki's CO catalog attaches one sentence to the Braton, Burston
and Zylok Incarnon radial rows — "AoE does not scale off multishot" — and the
engine has carried it as `takes_multishot: false` since those weapons went in,
on that sentence alone. A Notes column is not a measurement, and the sentence
admits more than one reading: it could be a statement about the TABLE's own
arithmetic (don't multiply the AoE when computing a theoretical total) rather
than about what the game does, and the Opticor's copy of it lives under **Bugs**
rather than under Notes, so it could also have been hotfixed away since.

**Measured** (Braton Prime, Incarnon form, +150% multishot ⇒ multishot 2.5).
The form fires at 5.67/s and the trigger cannot be released fast enough for a
single round, so the shortest burst obtainable is TWO rounds. Two rounds at 2.5
produce ~5 pellets (2 guaranteed per pull, 50% for a third). Observed:
**exactly 2 explosions.**

That is one explosion per TRIGGER PULL, not one per pellet, and the gap it has
to clear is categorical rather than statistical — 2 against ~5. `takes_multishot:
false` is what the game does.

### What it rules out

Two readings were live before this and are now dead:

- **"The form has innate multishot 2."** It does not. The wiki's stat block
  gives the Braton Prime's Incarnon form AND its radial `Multishot: 1 (70.00
  damage per projectile)` each, and the Burston Prime's both `Multishot: 1
  (13.00 damage per projectile)`. Had the 2 explosions come from an innate pair
  of pellets, the conclusion would have been the OPPOSITE one — the radial
  scaling normally off a base of 2.
- **"The note is about the table's arithmetic, not the game."** It is about the
  game. The forum reports' summary — "multiplied in arsenal, but not in reality"
  — is the right way round, and the arsenal is the half that lies.

### What it does NOT settle

The measurement is the BRATON's. The Burston and Zylok families carry the same
sentence in the same catalog, keyed the same way (one row per family), and this
confirms that reading the sentence literally is correct — but their own rows are
still wiki-sourced. Worth one shot each if the weapons are to hand; the Zylok is
the cheapest to read, being charge-fired with a 500 IPS direct hit against a 700
Heat explosion.

It also says nothing about WHY. The correlation across the catalog is perfect —
every row carrying the sentence is a hitscan attack (six say "Hitscan" in the
attack name outright; Mausolon's says "Based on hitscan damage"), and no
Projectile-typed AoE row carries it — and the plausible mechanism is that a
hitscan shot has no projectile entity to hang an explosion on, so the radial is
spawned by the fire event instead. That remains an inference. Per the catalog
rule it is NOT what the engine acts on: `takes_multishot` is declared per entry
from the row that names it, never derived from a weapon being hitscan.

### Sources

- [`Condition Overload (Mechanic)`](https://wiki.warframe.com/w/Condition_Overload_(Mechanic))
  — the three Incarnon radial rows and every other row carrying the sentence
- [`Braton Prime`](https://wiki.warframe.com/w/Braton_Prime) — the Incarnon
  form's and its radial's `Multishot: 1` stat blocks
- [`Trumna`](https://wiki.warframe.com/w/Trumna) — "Explosion is unaffected by
  multishot" (Notes)
- [`Opticor`](https://wiki.warframe.com/w/Opticor) — "Explosion isn't affected by
  multishot" (**Bugs**)
- the owner's run above
