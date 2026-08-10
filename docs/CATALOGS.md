# The per-weapon CATALOGS — tables the wiki publishes and the engine must carry entry by entry

Some mechanics are not a formula plus a weapon stat. They are a formula plus a
**published table with one row per weapon**, where the row can say a thing the
weapon's own numbers never would — that this weapon's bonus multiplies where
everyone else's adds, that this attack part is exempt, that this one does not
work at all.

Those rows are DATA, and the rule for them is the one the roster already
follows for Condition Overload (owner, 2026-07-30): **the catalog is
authoritative, and absence from it means ordinary — not unknown.** A row is
never generalised to a weapon, a form, or a class of behaviour; it is
transcribed for the entry it names.

This file is where the rows live, so they stop being scattered across yaml
comments and can be diffed against the wiki in one pass.

---

## 1. Condition Overload — `co_behavior`, `co_base_fraction`

Wiki: [Condition Overload (Mechanic)](https://wiki.warframe.com/w/Condition_Overload_(Mechanic)).
Columns, verbatim:

> Weapon | Attack Name | Projectile Type | Attack Unmodded Damage | Actual CO Damage Bonus at +100% | CO Damage Bonus Relative To Base Damage | Math/Behavior Type | Notes

`Math/Behavior Type` maps onto `CoBehavior`:

| catalog | ours | what it does |
| --- | --- | --- |
| Multiplying | `independent` | free-standing `× (1 + co × types)` |
| Adding | `additive_with_base_damage` | joins the base-damage bucket, diluted by Serration |
| (no row) | the ordinary case, i.e. `additive_with_base_damage` | |

### Rows carried

| weapon | attack | unmodded | relative | type | our entry |
| --- | --- | --- | --- | --- | --- |
| Torid | Main-fire (Projectile) | 100 | 100% | Multiplying | `torid.yaml` → `independent` |
| Torid | Toxin AoE Cloud (AoE) | 40 | 100% | Multiplying | the cloud's `takes_condition_overload: true` |
| Shedu | Normal Attack (Projectile) | 71 | 100% | Multiplying | `shedu.yaml` → `independent` |

**An AoE part needs its own row to take CO at all.** CO is a direct-hit bonus
everywhere else, which is why the engine's radial path refuses it by default and
the Torid's cloud is the declared exception. The Shedu's explosion has NO row,
so it takes none.

**A weapon's two FORMS can differ.** The Torid's Incarnon form is `Adding` where
its base form is `Multiplying`, which is exactly the shape a refactor flattens —
both forms still "have CO". Pinned by
`the_torid_carries_both_of_its_co_catalog_rows`.

### Where this has already gone wrong

- **Shedu, 2026-08-10.** Filed as `additive_with_base_damage` on the reasoning
  that it had no row. It has one, and it says Multiplying. The mistake was
  asserting an absence without opening the page — and the page's own Bugs
  section ("Galvanized Aptitude is multiplicative to base damage sources on
  direct hits") was describing the same behaviour the catalog classifies, since
  Galvanized Aptitude IS the CO bonus. Worth +41% on a status build.

---

## 2. Primary Compression — radius traded for damage

Wiki: [Primary Compression](https://wiki.warframe.com/w/Primary_Compression).
The arcane reads a weapon's **modded** blast radius, shrinks it to a fifth while
aiming, and pays for every metre lost.

```
radius_lost  = radius_modded × (1 − 0.2)      # continuous, NOT per whole metre
damage_bonus = damage_per_metre(rank) × radius_lost
ammo_eff     = eff_per_metre(rank)    × radius_lost
```

Rank ramp (wiki rank table == WFCD `levelStats`, both linear):

| rank | damage per metre | ammo efficiency per metre |
| --- | --- | --- |
| 0 | +50% | +3.0% |
| 5 | +100% | +5.5% |

*"Despite the description stating 'per meter lost,' the bonuses smoothly scale
between whole number radius values … a loss of 6.5 meters of radius gives +650%
Damage and +35.75% Ammo Efficiency."*

### Columns, verbatim

> Weapon | Effectiveness | Base Radius | Max Damage Bonus @ Base Radius | Stacking Behavior | Notes

### Rows carried

| weapon / attack | effectiveness | base radius | max bonus | stacking | notes |
| --- | --- | --- | --- | --- | --- |
| Shedu | 100% | 6.6 m | **+528%** | Multiplies | "Primary Fire AoE not affected by Firestorm; Cannot use reload pulse radial." |
| Torid — Toxin Cloud | 100% | 3.0 m | +240% | Multiplies | "Cloud radius is not reduced." |
| Torid — Incarnon Form + AoE | 0% | 2.3 m | — | — | "Doesn't Work" |

**General exclusion, verbatim:** *"Does not work on Continuous Weapons or beam
attacks with an AoE component. For example, Ignis or Torid Incarnon Genesis."*

### Two fields this needs, and they are per WEAPON ATTACK

Neither is derivable from the arcane, which is the same shape `co_behavior` has:

1. **Effectiveness / does it work at all** — a long tail of 0% rows, plus the
   continuous-beam exclusion.
2. **Stacking class** — Multiplies is the common case on projectile weapons;
   *Ambassador, Battacor, Ferrox, Opticor, Trumna and the Braton/Burston
   Incarnons ADD.*

### And one thing the Shedu's row makes the engine wrong about today

*"Primary Fire AoE not affected by Firestorm."* The engine multiplies EVERY
radial's radius by the blast-radius bucket
(`radius_m: r.radius_m * (1.0 + br)`), so a Shedu carrying Firestorm gets a
bigger explosion here than in game. It changes no damage in a single-target
arena — until this arcane is modelled, at which point it changes the bonus by
44%. A `radius_takes_blast_mods: false` on the radial is the fix, and it has to
land BEFORE the arcane does.

### Status

`primary_compression` is `kind: unmodeled` and stays that way until those two
fields exist. The BLOCKER RECORDED IN ITS YAML IS STALE, though: it says the
engine carries `ResolvedRadial.radius_m` unmodded, and that stopped being true
when the blast-radius bucket landed. What is missing now is the table above, not
the radius.

---

## Adding a catalog

A new one earns a section here when it has the same shape: a published table,
one row per weapon or per attack, saying something the weapon's stats do not.
Put the columns verbatim, the rows the roster actually carries, and — the part
that pays for itself — **where it has already gone wrong**.
