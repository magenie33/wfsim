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
| `mods/` | mods | `polarity`, `base_drain`/`max_rank` (drain = base+rank), bucketed `effects` with `per_rank`, **`family` + `incompatible_with`** (variants of one mod are mutually exclusive - the wiki module's `Incompatible` field, machine-readable) |
| `debuffs/` | **debuffs** applied by procs — same shape as `buffs/`, scoped to the target (a proc is only the trigger; see BUFFS.md "Debuffs") | `applied_by.damage_type`, `duration_seconds`, `max_stacks`, `stack_overflow`, `per_stack_modifiers`, `modifier_caps/conditions`, `cc_effects`, `aliases`, `internal_name` |
| `enemies/` | enemies (loaded by `engine::enemy_data`; `custom/` holds synthetic test targets) | `stats` (base values at `base_level`), `body_parts` (multiplier / `is_head` / `crit_bonus`; aim weights are scenario-side), `scaling_faction`, `can_be_eximus`, `faction_damage_override`, `synthetic`, raw `mechanics` |
| `factions/` | faction damage modifiers (post-U36, faction-wide) as **numeric multipliers** per damage type (unlisted = 1.0; today's values happen to be 1.5/0.5 — never assume it) | `factions.<id>.<damage_type>: <mult>`, `special` (Object, Overguard pools), `faction_mods` (Bane system) |

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

- **Field discipline (2026-07-28): a field is structured data that a program
  consumes; human narrative is a comment.** Every field must have a consumer —
  the engine, the UI, a script, or a code==data pin test (e.g. Secondary
  Enervate's ramp constants, pinned by
  `perks::secondary_enervate::tests::from_rank_matches_the_arcane_yaml`).
  Notes, rules-as-prose, caveats and reasoning go in `#` comments, never in
  fields. Structured game facts WITHOUT a consumer yet (e.g. an unmodeled
  mechanic's parameters) may stay as fields — they are columns awaiting a
  consumer, and they must be values, not sentences.
- Two metadata fields are kept by convention even without a code consumer:
  `source` (`url` — provenance, see
  [`../docs/DATA_SOURCES.md`](../docs/DATA_SOURCES.md)) and `internal_name`
  (DE's uniqueName — the join key to external datasets/importers).
- Every entry's `id` matches its filename. The directory IS the table: no
  `kind`/type tags duplicating what the path already says.
- No `schema_version`, no `verification` blocks (decision 2026-07-24): the
  data is the current belief, corrected in place; confidence lives in git
  history and golden tests.
- Names can collide across kinds by design (a *Frenzy* perk grants a *Frenzy*
  buff); the directory disambiguates.
- **Custom enemies**: any YAML dropped under `enemies/` (conventionally
  `enemies/custom/`) becomes a saved target type. Mark hand-made targets that
  do not exist in-game with `synthetic: true`. The loader rejects impossible
  data instead of guessing: unsupported fields with consequences (e.g.
  `shield > 0` before shields are implemented) and impossible combinations
  (e.g. Eximus of a unit with `can_be_eximus: false`) are hard errors.
