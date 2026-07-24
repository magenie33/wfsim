# wfsim data

Versioned game data — the "database". Data is **normalized** so anything reusable
is defined once and referenced by `id`, never inlined/duplicated. Terminology
follows [`../docs/GLOSSARY.md`](../docs/GLOSSARY.md); how to fetch data in bulk
from the wiki's structured modules is in
[`../docs/DATA_SOURCES.md`](../docs/DATA_SOURCES.md).

## Reference graph

```
item  ──references──▶  perk  ──grants_buff──▶  buff
(weapon / arcane / mod)   (trigger + rank scaling)   (the granted effect)
```

- An **item** never inlines a perk or buff — it lists perk `id`s.
- A **perk** never inlines a buff — it names the buff via `grants_buff`.
- Because references are by `id`, a buff can be granted by many perks, and a perk
  can be carried by many items, with a single source of truth for each.

## Directories

| dir | holds | key fields |
|---|---|---|
| `buffs/` | granted-effect definitions | `default_scope`, `duration_seconds` (null = untimed), `stacking`, `per_stack_modifiers` / `modifiers`, `rate_limit_hz`, `reset` |
| `perks/` | grantors (arcane / weapon or Warframe passive / Incarnon evolution) | `trigger`, `grants_buff`, `buff_scope`, optional `ranks` (rank-scaled params) |
| `arcanes/` | arcane **items** | `rarity`, `max_rank`, `arcanes_to_max`, `drop_chance`, `perk` |
| `weapons/` | weapons | `forms` (multi-form), `perks` (perk id list), `incarnon_evolutions` |
| `mods/` | mods | (tbd) |
| `enemies/` | enemies | `stats` (base values at `base_level`), `body_parts` (multiplier / `is_head` / `crit_bonus`), `faction_damage_override`, raw `mechanics` |
| `factions/` | faction damage modifiers (post-U36: x1.5 vulnerable / x0.5 resistant, faction-wide) | `factions.<id>.vulnerable/resistant`, `special` (Object, Overguard) |

## Where a parameter lives

A parameter goes on whichever entity *owns* it, so it is not duplicated:

- **Buff-intrinsic** (shape of the effect): per-stack value, rate cap, duration,
  scope default. Example: Secondary Enervate's `+0.10` flat crit per stack and
  its `30 Hz` cap live on the **buff**.
- **Perk-intrinsic** (grant logic + rank scaling): trigger, and per-rank params
  the buff refers to. Example: Secondary Enervate's rank→`reset_after_big_crits`
  table lives on the **perk**; the buff's `reset.threshold_param` points at it.
- **Item-intrinsic**: rarity, drop chance, max rank, slot. Live on the **item**.

## Conventions

- Every entry has `schema_version`, an `id` (matches the filename), a `source`
  (`url` + `retrieved` date), and a `verification` block (`status`:
  `unverified` / `verified` / `disputed`). Nothing is trusted until a golden test
  moves it to `verified` (see [`../docs/CORE.md`](../docs/CORE.md) §5).
- Names can collide across kinds by design (a *Frenzy* perk grants a *Frenzy*
  buff); the directory + `kind` disambiguate.
- Schemas are drafts (`schema_version: 0`) and will be pinned as the loader lands.
