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

### Columns and LEGEND, verbatim

> Weapon | Attack Name | Compression Effectiveness | Stacking Behavior with
> Damage Bonuses | Radius Calculation | Base Radius | Max Damage Bonus @ Base
> Radius | Max Damage Bonus w/ Primed Firestorm | Notes | Class

The page's own legend, and it settles two columns that are easy to read wrong:

> **Compression Effectiveness:** Shows how much bigger/smaller the radius
> Compression considers compared to how much it should be considering. 100%
> means "intended" radius/damage calculation. >100% means better than expected.
> <100% means worse than expected.
>
> **Radius Calculation:** On weapons with multiple firing modes that have AoEs
> attached to them, Compression has to determine which radius to use on aiming:
> **Snapshot** = Uses the ads state when fired, not when AoE occurs.
> **Stolen** = Uses another firing mode's radius for the Compression bonus.
> **Doesn't Work** = Compression doesn't apply to this AoE.

Two things that changes about the earlier reading here:

- **Effectiveness is about WHICH RADIUS is considered**, not how much of the
  bonus is paid. The arithmetic lands in the same place — the bonus scales with
  the radius given up — but the Vectis pair's 4% is not a discount on their
  damage, it is the arcane reading a 0.1 m embed radial instead of the headshot
  explosion, and the Trumna alt-fire's 127% is not a bonus multiplier, it is a
  radius counted twice over ("Merged").
- **`Radius Calculation` is a COLUMN, not a note.** `Snapshot` is its ordinary
  value, and the table also uses `Constant Check` (the Battacor), which the
  legend does not list.

And one column is a place to put an EXPLANATION rather than a number: `Max
Damage Bonus w/ Primed Firestorm` holds *"Primary Fire AoE not affected by
Firestorm"* on the Shedu and *"Shotguns cannot equip mod"* on every shotgun —
i.e. it says why that cell has no figure. The last column is the wiki's own
class taxonomy, which is why the Shedu reads `Arm-Cannon` rather than `Rifle`.

The Shedu's row in full, since it is the one the roster leans on:

| Weapon | Attack | Eff | Stacking | Radius calc | Base radius | Max @ base | w/ Primed Firestorm | Notes | Class |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Shedu | Primary Fire + AoE | 100% | Multiplies | Snapshot | 6.6 m | +528% | *Primary Fire AoE not affected by Firestorm* | *Cannot use reload pulse radial.* | Arm-Cannon |

### SIX AXES, not one

This table is why the arcane is not a formula with a weapon stat in it. Every
one of these varies independently, and the first four cannot be derived from
anything the weapon's own data says:

1. **Stacking** — `Multiplies` is the common case, but a real minority `Adds`
   (Ambassador, Battacor, Ferrox, Opticor, Trumna, and every Braton/Burston
   Incarnon), and the Trumna's alt-fire is `Both`.
2. **Effectiveness**, i.e. which radius gets considered — mostly 100% or 0%,
   but the Vectis pair are **4%** and the Trumna's alt-fire is **127%**.
3. **Radius Calculation** — `Snapshot` / `Stolen` / `Doesn't Work`, plus the
   Battacor's `Constant Check`. It only bites on weapons with more than one
   AoE-bearing firing mode, which is most of the ones that matter.
4. **Which radius is even reduced.** Several weapons collect the bonus while
   paying nothing: the Torid's *"cloud radius is not reduced"*, the Alternox's
   pulse, Penta's napalm, the Simulor's singularity, Ferrox's pull.
5. **Radius mods may not apply at all** — the Shedu and both Trumnas.
6. **Multishot can change it.** The Simulor's effectiveness *decreases* with
   multishot (83%, 71%, 63%, 56%) and recovers with Primed Firestorm.

#### "Shotguns cannot equip" is about a MOD, not this arcane — CHECKED

The rendered table drops a word. The wikitext says **"Shotguns cannot equip
mod"**, and the row's last column is the weapon CLASS:

```
| {{Weapon|Corinth}} … || Alt-Fire + AoE || 100% || Multiplies || Snapshot
| 9.4 m (9.8 m)
| +752% (+784%)
| Shotguns cannot equip mod|| || Shotgun
```

The mod is the blast-radius one. Firestorm and Primed Firestorm are RIFLE mods
and Fulmination and Primed Fulmination are PISTOL mods; **there is no shotgun
blast-radius mod at all**. So a shotgun is stuck at its base radius and can
never reach the table's Primed Firestorm column — which is also quiet evidence
that the arcane reads the MODDED radius, since otherwise the note would say
nothing.

It is not an equip rule and there is nothing to implement: the engine already
answers it by construction, because a shotgun's pools are `[primary, shotgun]`
and the mod is in neither. Verified against `/api/meta` (2026-08-11):

| weapon | class | blast-radius mods offered |
| --- | --- | --- |
| Phantasma Prime | Shotgun | — |
| Strun | Shotgun | — |
| Torid | Launcher | firestorm, primed_firestorm |
| Shedu | Rifle | firestorm, primed_firestorm |

*"Archguns cannot equip"* and *"Exalted weapon cannot equip Arcanes"* are the
same shape and are already true here for the same structural reason.

**The real per-weapon exception is the other one.** The Shedu IS offered
Firestorm — it is a rifle — and the mod does nothing to its explosion anyway
("Primary Fire AoE not affected by Firestorm", shared with both Trumnas). That
one the engine gets WRONG today, and it is recorded on the weapon's radial.

### Rows the ROSTER carries

Only ours, so this stays diffable. The full table lives on the wiki.

| our entry | eff | base radius | max bonus | stacking | note |
| --- | --- | --- | --- | --- | --- |
| Shedu | 100% | 6.6 m | +528% | Multiplies | AoE **not affected by Firestorm**; cannot use the reload pulse radial |
| Torid — Toxin Cloud | 100% | 3.0 m | +240% | Multiplies | cloud radius **not reduced** — pays nothing, collects everything |
| Torid — Incarnon | 0% | 2.3 m | — | Doesn't Work | the continuous-beam exclusion |
| Braton / Prime / Vandal / Mk1 — Incarnon | 100% | 3.0 m | +240% | **Adds** | |
| Burston / Prime — Incarnon | 100% | 2.0 m | +160% | **Adds** | |
| Gorgon / Prisma / Wraith — Incarnon | 100% | 5.0 m | +400% | Multiplies | |
| Latron / Prime — Incarnon | 100% | 4.0 m | +320% | Multiplies | |
| Miter — charged shot | 100% | 0.2 m | +16% | Multiplies | "wide projectile, not traditional AoE" |
| Miter — Incarnon | 100% | 3.0 m | +240% | Multiplies | |
| Strun / Prime / Wraith — Incarnon | 100% | 4.0 m | +320% | Multiplies | "Shotguns cannot equip" |
| Phantasma / Prime — Alt-Fire | 100% | 4.8 m | +384% | Multiplies | "Shotguns cannot equip"; bomblets do not benefit |
| Vectis / Prime — Incarnon | **4%** | 0.1 m | +8% | N/A | uses the embed radial, not the headshot explosion |
| Larkspur Prime | Untested / 0% | 9.6 m | — | — | "Archguns cannot equip" |

**General exclusion, verbatim:** *"Does not work on Continuous Weapons or beam
attacks with an AoE component. For example, Ignis or Torid Incarnon Genesis."*

### Status — MODELLED (2026-08-11)

The arcane brings two ramps PER METRE; the weapon brings the metres and the
bracket. They meet in one place and it is not the one you would guess:

- `loadout::resolve_for` computes `panel.compression` — how many metres THIS
  build gives up (modded radius × the row × 0.8), and whether the row `adds`.
  Aim-gated: the card says "on aim", so a Tenno who is not aiming gets `None`.
- `DummyParams::from_panel(panel, arena, arcane)` spends the arcane against
  those metres. It takes the arcane as an ARGUMENT rather than having it
  assigned afterwards, which is what makes the three sites agree — and it is one
  layer below `resolve_for` on purpose: **the optimizer resolves a panel once
  and pairs it with every arcane in the search**, so a panel that had already
  spent an arcane would have to be re-resolved per job.
- `adds` joins `arc_bd`, the live base-damage bracket — so Serration dilutes it.
  `multiplies` joins `arc_final` beside Secondary Surge — so Serration does not.
  `compression_pays_into_the_bracket_its_row_names` measures exactly that
  difference rather than either number on its own.

`the_roster_reproduces_primary_compressions_published_column` re-derives the
table's **Max Damage Bonus @ Base Radius** for all 22 rows from our own weapon
data. That column is not transcribed anywhere — it falls out of the radius, the
row and the rank ramp — so it is a cross-check: a radius typed wrong, an
effectiveness misread or an override invented breaks it.

What is still NOT modelled, and neither changes a number here:

- the radius REDUCTION itself. The panel keeps showing the full radius while
  the arcane is equipped, because this arena has no distance (docs/UNMODELLED.md)
  — every shot lands at point blank, so a fifth of a radius kills exactly as
  much as all of it.
- axis 6, MULTISHOT moving effectiveness (the Simulor). No roster weapon has
  that row.

---

## Adding a catalog

A new one earns a section here when it has the same shape: a published table,
one row per weapon or per attack, saying something the weapon's stats do not.
Put the columns verbatim, the rows the roster actually carries, and — the part
that pays for itself — **where it has already gone wrong**.
