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

## Verification tooling (lives in `private/scripts/` — LOCAL, gitignored)

The pipeline that fills and checks `data/` is not in the repo (`/private/` is
"Private, never committed"), so it is listed here: the CHECKS are part of how
this data is trusted, and a reader of `data/` should know they exist even
though the scripts do not travel with it.

| script | what it asserts |
| --- | --- |
| `wiki_mods.py` / `wiki_arcanes.py` | shared fetch + parse of the authoritative Lua modules |
| `gen_mods.py --type <T>` | generate skeletons; never overwrites a curated file. Owns the import filters (PvP-only, removed-from-game, Flawed, Riven placeholders) |
| `verify_mods.py --type <T>` | COVERAGE (every importable wiki mod of that Type has a file, and no file is a stranger) + drain / polarity / rarity / max_rank / exilus. Imports `gen_mods`' filters so the two cannot disagree about what the pool should hold |
| `verify_arcanes.py --slot <S>` | same, for arcanes, plus the X-templated description token-matching the wiki's max-rank text |
| `audit_mod_effects.py --type <T>` | the EFFECT NUMBERS: every modeled `rankMax` must appear in the mod's own wiki description. Also flags CONDITIONAL-AS-FLAT (a description with an `On <trigger>:` line must produce a `kind: buff`) and DESC-STALE (the module's text lagging its own MaxRank) |
| `audit_arcane_effects.py --slot <S>` | the effect numbers at BOTH ends of the rank ramp, against warframestat `levelStats` |
| `reconcile_families.py --type <T>` | `family` / `incompatible_with` by union-find over the wiki's `Incompatible` lists — the mutual-exclusivity groups the optimizer enforces |

Two systematic failure modes these caught, worth knowing because they are
INVISIBLE to a numbers-only check and both produce data that looks fine:

- **A missing `family`** lets the optimizer equip mutually exclusive mods
  together (Serration + Amalgam Serration) and report the result as a legal
  build. The whole rifle set had none.
- **A conditional bonus modeled as always-on**: the generator splits
  `"On Kill:\n+120% Critical Damage"` into an `unmodeled` marker for the
  trigger plus a plain bonus for the payload, so the value audits clean while
  the mod hands its entire worth over for free. Seven rifle mods.

And two cases where the wiki is the thing that is wrong, kept flagged rather
than silenced:

- The module's `Description` can **lag its own `MaxRank`** (Hawk Eye, Steady
  Hands: MaxRank 5 with rank-3 text). The modeled value is the linear
  extension along the same ramp; both files say so in a comment.
- Amalgam Barrel Diffusion really grants **109.50%** multishot and the tooltip
  rounds to 110% — an explicit allowlist entry with the wiki quote, not a
  tolerance that would hide the next real error.
