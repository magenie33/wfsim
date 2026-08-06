# Custom weapons & custom mods

Two independent visitor-authored features: a **custom weapon** (a panel typed
by hand, carried with every request) and a **custom mod** (a card you define,
equipped and searched like any other mod). Both follow the customs rules
(AGENTS.md): the thing exists only on the machine that made it, so it travels
inline with the request — nothing is stored server-side, and a shared link
carries the whole definition.

## Why they are separate

A custom weapon is a WEAPON: it replaces the data entry. A custom mod is a
MOD: it joins the pool. They can be used together, alone, or not at all —
nothing in the other's path depends on it. `custom:primary` with no custom
mods is a normal weapon; a roster weapon with custom mods is a normal build.

## Custom weapons

- One per equipment slot: `custom:primary`, `custom:secondary`,
  `custom:shotgun`, `custom:archgun`, `custom:sentinel`. The five entries are
  appended to `weapons()` (webapi), so `weapon()`, `meta_json` and
  `riven_class` answer for them with no branches of their own.
- The panel IS the request (`custom_weapon`): 13 damage types (IPS + single
  elements + all six compound elements), crit/status, fire rate, multishot,
  magazine, reload, ammo reserve/cost. Validation is fail-fast: base damage
  positive, every number finite and `|x| <= 1e9`, fire rate > 0.
- **No hidden passives, no evolutions**: `incarnon_id`/`has_perk`/
  `evo_group`/`evo_forbids` short-circuit for `custom:*` ids; the mod pool is
  the slot's union (`primary`+`rifle`, `pistol`, `primary`+`shotgun`,
  `archgun`, `rifle`).
- **Slots are fixed 8 + 1 exilus.** `innate_slots` answers 9 unpolarized
  slots for an unknown id; `OptimizePlan` has no slot field.
- **Riven disposition is the visitor's own**: `custom_weapon.disposition`
  (0.5..=1.55) feeds `riven_json` and `rivens_from` through
  `disposition_of`, so a riven's printed range and its sim value scale with
  it — same formula as the roster: `value = base × 10 × (rank+1) ×
  disposition × config × roll`.
- The optimizer accepts a custom weapon: `parse_optimize` pins the panel
  once into `OptimizePlan.custom_base`, and enumeration and replay deploy
  the SAME base.
- Name is the visitor's own; the URL/slug stays the stable slot
  (`/weapons/primary`), so renaming never breaks a shared link.

## Custom mods

- A card = name + polarity + base_drain + exilus + effects. Stored per weapon
  (`wfsim-customs-<weapon>-custommods`), copied across weapons by the ⇤ import
  action — the same rule rivens have (NOTHING CROSSES BETWEEN WEAPONS).
- **31 effect kinds** map to `ModEffect`: base damage, multishot, crit/status
  chance & damage, fire/charge rate, reload, status damage/duration,
  magazine, slash-on-crit, blast radius, weak-point, physical/element/
  combined element, faction, conditional buff, 19 handling stats, the seven
  trigger kinds (kill/headshot/reload/equip, condition overload, proc
  conversion, while-tenno), each at most ONE per card.
- **Ratios are × factors** (1.65 = +165%), capped at ±100 (= ±10000%) per
  effect; ask for more by repeating the effect — the buckets are additive.
  Duration ≤ 1e6 s, stacks ≤ 1e6.
- **Element order is the combine order**: `elements::combine` pairs the
  mod-order elements two at a time (`chunks_exact(2)`), so a card listing
  Toxin then Electricity becomes Viral, and listing them the other way stays
  pure. The editor's ↑/↓ reorders effects; the order is what the engine sees.
- Custom cards are **repeatable** (no `family`), unlike a riven's one-per-
  weapon rule; the optimizer treats them fixed-only (their numbers are the
  visitor's own, so searching adds nothing).
- Search: `customMods()` builds the mod shape with `name`/`name_en`/`effects`
  (each effect's human line), so the picker's `searchBlob` matches card
  names and effect values in both languages.

## Safety

`clamp_sim_sensitive` (engine loadout.rs) caps the resolved panel's fire
rate and multishot at 65536 before the sim runs, with a warning per cap hit —
a visitor typing 100000% multishot gets an answer in seconds, not a hang.

## Limits

- No radial/lingering/beam/charge/Incarnon mechanics on a custom weapon
  (a beam is expressible via `trigger: held`).
- Custom weapons carry no arcanes (`uses_arcane: false`).
- No DE art exists for a custom weapon; the UI shows a neutral placeholder
  (the asset-coverage test skips `custom:*` ids).
- UI labels follow the language selector: the custom-weapon panel fields and
  the custom-mod editor (effect-kind names, parameter labels, buttons) are
  translated via `data/i18n/zh/ui.yaml` — terms transcribed from the official
  CN client, never invented; damage types use the `damage_types` table in
  names.yaml. English needs no catalog: the source string is the fallback.
- The custom weapon's panel editor is EMBEDDED on the build page (above the
  mod slots), not a separate tab; it appears only while a `custom:*` weapon
  is selected. Effect lines render inside the mod slot's card and the Custom
  Mods list card, and an effect-heavy card grows taller instead of truncating.
- Static deployment (`site/`): the checked-in `site/app.js` predates these
  features — rebuild it with `python scripts/build_site_app.py` (needs the
  pinned `wasm-bindgen-cli`; not required for the native `wfsim-web` server).
