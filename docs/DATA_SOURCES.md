# wfsim — Data Sources

How to get game data efficiently instead of transcribing pages by hand. The
official wiki is backed by **structured Lua data modules**, and they are the
authoritative datamined values (the same source WFCD tooling uses).

## Primary source: wiki Lua data modules (`?action=raw`)

The wiki stores stats in `Module:*/data*` pages. Append `?action=raw` to fetch
the raw Lua table. Weapons are partitioned by slot:

- Loader: `https://wiki.warframe.com/w/Module:Weapons/data` (accessor only)
- Actual data (one per slot), e.g.:
  - `https://wiki.warframe.com/w/Module:Weapons/data/secondary?action=raw`
  - `.../Module:Weapons/data/primary?action=raw`
  - `.../Module:Weapons/data/melee?action=raw`

Other categories live under the same `Module:.../data` pattern (mods, arcanes,
warframes, enemies, ...) — **locate the exact page per category before relying on
it** (names to be confirmed as we add each).

The **page infobox** (the `<div class="row"><div class="label left">…</div>
<div class="value right">…</div></div>` rows) is the human-rendered view of the
same data; parse label/value pairs if the module is unavailable.

## Weapon entry schema (observed)

Each weapon maps name → a table like (Dual Toxocyst, verbatim keys):

```
Accuracy, AmmoMax, AmmoPickup, AmmoType, Class, Conclave, DefaultUpgrades,
Disposition, ExilusPolarity, Family, GripType, Image, IncarnonChargeGain,
IncarnonImage, InternalName, Introduced, Link, Magazine, Mastery, MaxRank, Name,
Polarities, Reload, SellPrice, Slot, Traits, Trigger,
Attacks = [ { AmmoCost, AttackIndex, AttackName, CritChance, CritMultiplier,
              Damage = {Impact, Puncture, Slash, ...}, FireRate, Multishot,
              PunchThrough, Range, ShotType, StatusChance, MinSpread, MaxSpread,
              IsSilent, [IncarnonCharges, Trigger] } ]
```

## Incarnon evolutions: which page documents them

Two kinds of Incarnon weapon, and they are documented in different places —
guessing wrong costs a 404:

- **Installed Genesis** (the Steel Path / Duviri route): the weapon has a
  separate `<Weapon>_Incarnon_Genesis` page carrying the install cost, the
  gauge economy and the evolution tiers. Dual Toxocyst →
  `Dual_Toxocyst_Incarnon_Genesis` ✔ (200).
- **Natively Incarnon** (the Zariman weapons — Laetum, Phenmor, Felarx,
  Innodem…): there is nothing to install, so there is **no** Genesis page.
  Everything lives on the weapon page itself. `Laetum_Incarnon_Genesis`
  does not exist (404) — cite `/w/Laetum`.

The data mirrors the same split: a natively-Incarnon entry carries no
`incarnon.install_cost` block.

## Mapping to our schema

Our field names follow the wiki concept words (snake_case + unit suffixes):

| wiki key | our field |
|---|---|
| `Attacks[]` | `forms[]` (base = Normal Attack, incarnon = Incarnon Form) |
| `CritChance` / `CritMultiplier` | `crit_chance` / `crit_multiplier` |
| `StatusChance` | `status_chance` |
| `FireRate` / `Multishot` | `fire_rate` / `multishot` |
| `PunchThrough` / `Range` | `punch_through_m` / `range_m` |
| `ShotType` "Hit-Scan" | `shot_type: hitscan` |
| `Damage.{Impact,Puncture,Slash}` | `damage.{impact,puncture,slash}` |
| `Mastery` / `MaxRank` | `mastery_rank` / `max_rank` |
| `DefaultUpgrades` (innate mod) | modeled as a `perks[]` entry |

## Plan (reduce the manual workload)

- For now: transcribe from the **module** (not the summarized page) so numbers
  are authoritative; cite the module URL in each entry's `source`.
- Later: a small **importer** fetches these modules and emits our YAML directly,
  so bulk entry is automated. WFCD's `warframe-items` (JSON, same datamined
  source) is an alternative bulk feed to consider for the importer.
- **No `verification` blocks, no `schema_version`** (decision 2026-07-24):
  whatever is written in the data IS the current belief, corrected in place
  as measurements land. Confidence lives in
  git history, not per-file status fields. Golden tests vs
  Simulacrum remain the arbiter of the ENGINE; the data is simply kept
  current.
- **Field discipline** (decision 2026-07-28, full statement in
  [`../data/README.md`](../data/README.md)): fields are structured data a
  program consumes; human narrative is a `#` comment. No prose in fields.
