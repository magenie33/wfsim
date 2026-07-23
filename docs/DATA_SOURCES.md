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
- Always keep the `verification` block: even authoritative base stats stay
  `unverified` for the full pipeline until a golden test confirms the computed
  result against Simulacrum.
