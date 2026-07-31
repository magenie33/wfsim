# wfsim data

Versioned game data — the "database". Data is **normalized** so anything reusable
is defined once and referenced by `id`, never inlined/duplicated. Terminology
follows [`../docs/GLOSSARY.md`](../docs/GLOSSARY.md); how to fetch data in bulk
from the wiki's structured modules is in
[`../docs/DATA_SOURCES.md`](../docs/DATA_SOURCES.md).

## Reference graph

```
item  ──references──▶  perk (trigger + grants)
(weapon / arcane / mod)   (data/perks/, carries its granted effects inline)
```

- A perk carries its `grants` block inline; a separate buff table returns
  only if two perks ever grant the same effect.

## i18n: English is the source, translations are overlays

- ids are NEVER translated; every entity's own `name` field is the English
  source of truth. A locale is a DIRECTORY — `data/i18n/<locale>/` — whose
  yaml files are MERGED into one overlay (`engine::i18n_data`), and there is
  no English overlay at all:

  | file | written by | holds |
  |---|---|---|
  | `names.yaml` | a translator | `id → display name`, per entity table |
  | `ui.yaml` | a translator | `ui:` (keyed by the English source string) + `effect_phrases:` |
  | `descriptions.yaml` | **generated** | mod/arcane card text in DE's own words, one entry per rank |

  Two files may not fill the same table — a duplicate key is a hard error,
  not a last-one-wins.
- Overlays may be arbitrarily incomplete: a missing entry falls back to
  English in the UI. Partial translation is always a valid state.
- Referential integrity is machine-enforced (`engine::i18n_data` tests):
  every overlay key must be a real id — a translator's typo fails CI, so a
  translation PR can never break the app.
- **Dual verification**: every localized name should be witnessed by BOTH
  sources — (1) DE's official client strings via WFCD warframe-items
  (`python scripts/wfcd_i18n.py check` automates this arm, joining our
  `internal_name` to their `uniqueName`), and (2) the community wiki's
  对照 table (https://warframe.huijiwiki.com/wiki/Project:中英名称对照,
  human cross-check in PR review). `wfcd_i18n.py fill` bulk-seeds a
  section from source (1).
- **A card is not a bag of terms.** Mod and arcane descriptions are NOT
  assembled by substituting terms into our English line — they are DE's own
  localized sentence, per rank, taken whole
  (`python scripts/wfcd_i18n.py descriptions` → `descriptions.yaml`). Phrase
  substitution reaches "Fire Rate" → "射速" and stops there: the same card's
  "(x2 for Bows)" stayed English, where DE writes "（弓类武器效果加倍）". Two
  tests hold the generated file to our data — one entry per rank, and every
  number it states must be one we state too (allowing for DE's display
  rounding; ranks between the two we store are our interpolation, so only the
  endpoints are compared).
- `effect_phrases` remains, DEMOTED to the fallback: it translates what DE
  never wrote — our engine-generated effect lines, panel labels, and the
  entities their export cannot be joined to (Incarnon evolutions carry no
  `internal_name`, so all 31 are still on this path). `ui:` is keyed by the
  English source string. English needs no entries: the source is the fallback.

## Perks: define once, reference anywhere

A `perks:` entry is **either a bare id (a reference) or a full inline
definition** — both forms are valid, and **both register the perk in the
GLOBAL perk namespace**:

```yaml
# weapon_a.yaml — defines a one-off perk INLINE (no ceremony needed)
perks:
  - id: venom_burst
    trigger: on_kill
    grants: { ... }

# weapon_b.yaml — a later carrier just references the bare id,
# regardless of where the definition lives
perks:
  - venom_burst
```

The three rules:

1. **Define once, anywhere.** A perk's single definition may live in
   `perks/<id>.yaml` (the table) or inline in one item's `perks:` list.
   Resolution (`engine::weapons_data::perk`) searches the table first, then
   every item's inline definitions.
2. **Every other carrier writes the bare id.** Never a second copy — both
   Dual Toxocyst forms reference `frenzy` (a table perk).
3. **Ids are globally unique, machine-enforced** —
   `weapons_data::tests::perk_ids_are_globally_unique_across_table_and_inlines`
   fails the build on a duplicate (inline shadowing a table id, or the same
   id inlined twice), so a bare id is never ambiguous.

Moving a much-shared inline perk into `perks/<id>.yaml` is tidiness, not a
sharing requirement: it is a pure move — no referencing entry changes.

## Directories

| dir | holds | key fields |
|---|---|---|
| `perks/` | grantors (weapon passives; loaded by `engine::weapons_data::perks`) | `trigger`, `scope`, `duration_seconds`, `max_stacks`, `grants` (inline effect block) |
| `arcanes/` | arcane **items** | `rarity`, `max_rank`, `arcanes_to_max`, `drop_chance`, `perk` |
| `weapons/` | weapons | **`form`** (REQUIRED — which form this entry is, from the closed vocabulary `base` / `charged` / `incarnon`; see [`../docs/GLOSSARY.md`](../docs/GLOSSARY.md) "FORMS"), `transform_group` (the entries that are forms of ONE weapon), `perks` (perk id list), `incarnon_evolutions` |
| `mods/` | mods | `polarity`, `base_drain`/`max_rank` (drain = base+rank), bucketed `effects` with `per_rank`, **`family` + `incompatible_with`** (variants of one mod are mutually exclusive - the wiki module's `Incompatible` field, machine-readable) |
| `debuffs/` | **debuffs** applied by procs — same shape as `buffs/`, scoped to the target (a proc is only the trigger; see BUFFS.md "Debuffs") | `applied_by.damage_type`, `duration_seconds`, `max_stacks`, `stack_overflow`, `per_stack_modifiers`, `modifier_caps/conditions`, `cc_effects`, `aliases`, `internal_name` |
| `enemies/` | enemies (loaded by `engine::enemy_data`; `custom/` holds synthetic test targets) | `stats` (base values at `base_level`), `body_parts` (multiplier / `is_head` / `crit_bonus`; aim weights are scenario-side), `scaling_faction`, `can_be_eximus`, `faction_damage_override`, `synthetic`, raw `mechanics` |
| `i18n/` | one DIRECTORY per locale (`zh/`, files merged; loaded by `engine::i18n_data`, served at `/api/i18n`) | `id → name` maps (`weapons`, `enemies`, `damage_types`, `mods`, `arcanes`, `evolutions`), `ui` + `effect_phrases`, and the generated `mod_descriptions` / `arcane_descriptions` (DE's card text, one entry per rank) |
| `factions/` | faction damage modifiers (post-U36, faction-wide) as **numeric multipliers** per damage type (unlisted = 1.0; today's values happen to be 1.5/0.5 — never assume it) | `factions.<id>.<damage_type>: <mult>`, `special` (Object, Overguard pools), `faction_mods` (Bane system) |
| `tenno/` | the **player** (loaded by `engine::tenno_data`). **INERT on purpose** — it participates in no calculation yet; it exists so the mechanics that need a player have somewhere to attach instead of each inventing one. `health`/`shield` are PLACEHOLDERS at 1, not Warframe stats | `health`, `shield`, `overguard`, `armor`, `energy` |

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
- **An effect the loader does not parse must SAY SO, in a comment.** The three
  loaders disagree about what happens to a `kind` they do not recognise, and
  only one of them leaves a trace:
  - `evolutions_data` falls through to `EvoEffect::Inert(kind)`, which the UI
    renders as *"<kind> (no single-target DPS effect)"* — visible, honest.
  - `arcanes_data` has an explicit `kind: unmodeled` carrying a **`note`**,
    which `describe()` renders. There, `note` IS a consumed field.
  - `mods_data` hits `_ => return None`: the effect is dropped and the mod
    loads **as if the entry were not there**, with nothing on screen to say so.
  That silence is how `blast_radius_bonus` sat inert on Fulmination for months.
  So any entry whose `kind` its own loader does not match carries a comment
  naming the consequence. Grep the loader before assuming a kind is live —
  `note`/`desc` are consumed for arcanes and dead everywhere else.
- **`data/debuffs/` is loaded by nothing.** It is the written spec for status
  effects; the behaviour is hand-implemented in `engine::dummy` /
  `engine::status`, which cite the files by name. Treat a change there as a
  documentation change that also needs code.
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
