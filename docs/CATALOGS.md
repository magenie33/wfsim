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

### SIX AXES, not one

This table is why the arcane is not a formula with a weapon stat in it. Every
one of these varies independently, and the first four cannot be derived from
anything the weapon's own data says:

1. **Stacking** — `Multiplies` is the common case, but a real minority `Adds`
   (Ambassador, Battacor, Ferrox, Opticor, Trumna, and every Braton/Burston
   Incarnon), and the Trumna's alt-fire is `Both`.
2. **Effectiveness is not a flag.** Mostly 100% or 0%, but the Vectis pair are
   **4%** ("uses embed radial instead of headshot explosion") and the Trumna's
   alt-fire is **127%** ("Merged; Alt-Fire gains damage from primary fire's
   radius plus a unique multiplier from alt-fire's radius").
3. **When it is evaluated** — most rows say `Snapshot`; the Battacor says
   `Constant Check`.
4. **Which radius is even reduced.** Several weapons collect the bonus while
   paying nothing: the Torid's *"cloud radius is not reduced"*, the Alternox's
   pulse, Penta's napalm, the Simulor's singularity, Ferrox's pull.
5. **Radius mods may not apply at all** — the Shedu and both Trumnas:
   *"Primary Fire AoE not affected by Firestorm."*
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
