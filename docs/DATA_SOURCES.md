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

## Which source wins (revised 2026-07-30)

The rule used to be "the wiki module is authoritative for every mechanical
field". That was wrong for two of them, and nothing could show it while only
one source was consulted.

| field | authority | why |
| --- | --- | --- |
| `base_drain`, `max_rank` | **WFCD** (`vendor/warframe-items`) | `Module:Mods/data` is wrong for ~20 mods. Checked against the wiki PAGE rank tables as a third, independent data point (hand-maintained but per-rank, so hard to get wrong) — Point Strike, Split Chamber, Metal Auger, Barrel Diffusion, Convulsion, Deep Freeze, Gunslinger, Suppress — and the page agreed with WFCD **8/8** |
| `polarity`, `rarity`, `exilus`, `internal_name`, verbatim `description` | **wiki module** | unchanged; WFCD has no exilus flag and its display text is rounded |
| per-rank effect VALUES | **WFCD `levelStats`** | a full ramp, both ends checkable; the module gives max rank only |
| everything mechanical | **cross-check both** | a disagreement is itself the finding — `crosscheck.py` reports SOURCE-SPLIT |

**Join by `internal_name` == WFCD's `uniqueName`. Never by name.** WFCD carries
stale duplicates sharing a display name: its first entry called "Serration" is
*Flawed* Serration. That join is exact — every mod and arcane file in `data/`
matches exactly one entry, none unmatched. A name-keyed lookup is what made an
earlier pass "confirm" MaxRank 5 for Hawk Eye and Steady Hands from a
collision duplicate, and record the wrong conclusion in both files.

### Open SOURCE-SPLIT: the Torid Incarnon's beam geometry

The official wiki's raw wikitext (`?action=raw`, 2026-07-30) reads, with markup:

```
'''37''' meter range, and a '''2.3''' meter damage radius ...
chaining to up to '''5''' nearby enemies within '''7''' meters
```

Secondary sources in circulation — the abandoned Fandom wiki, Overframe build
pages, Steam threads — quote **40 m / 3 m / 6 m** instead, and at least one
search index asserts 37/2.3 is the *older* pair. WFCD cannot arbitrate: its
export carries **no radius or range at all** for the Incarnon attack, only the
Poison Cloud's falloff.

We follow the official wiki (37 / 2.3 / 7), which is what `torid_incarnon.yaml`
holds. **Nothing computes with these yet** — they are geometry for the
multi-target model, so a wrong value cannot currently move a result. Worth a
measurement before the 2D model consumes them; MEASUREMENTS M15 already needs
the same setup and can take the tape measure at the same time.

### What the second source caught

Cost of having had only one: **20 mods** wrong on `base_drain`/`max_rank`, and
**7** wrong on effect values. The value errors mattered more than the drains:

- `fire_rate_bonus` sat at the placeholder pair `rank0 0.1667 / rankMax 1.0`
  (= 1/6 and 1.0, never filled in) on **five** rifle mods. Two of them are
  DRAWBACK mods, so the sim read Critical Delay's −20% and Vile Precision's
  −36% fire-rate penalties as **+100% bonuses**. Primed Shred read +100%
  instead of +55%.
- `internal_bleeding` had a `proc_conversion` mod modeled as an
  `elemental_damage_bonus` — the generator read the element out of the
  description and invented a damage bucket. Its pistol twin Hemorrhage was
  already correct.
- `metal_auger`'s punch-through ramp is NON-linear (0.4 / 0.7 / 1.0 / 1.4 /
  1.8 / 2.1), so `rankMax/6` was the wrong rank-0.

Note the single-source audit could not have found the Primed Shred one even in
principle: it checks that a modeled value appears in the mod's own description,
and `1.0` matched via the "+1 multiplier" reading against the `2` in
"(x2 for Bows)". A wrong value hiding behind a coincidence in the same string
is exactly what a second, independent ramp rules out.

### Second pass: rendered text vs `levelStats`, rank by rank (2026-07-31)

The checks above compare stored VALUES. This one compares what the card
SAYS — `desc_ranks` from `/api/meta` against WFCD's `levelStats`, every mod
at every rank, 1050 mod-ranks over 153 mods. It is the only check that can
see a value landing in the wrong SLOT, because both sides are the same
sentence. What it caught:

- **`shred` was a sixth survivor of the `(1/6, 1.0)` placeholder pair** — the
  audit above found five and this one was still reading **+100% fire rate at
  max instead of +30%**. That one is not cosmetic: the sim built with it.
- Values matched to placeholders by POSITION rather than by kind. Galvanized
  Crosshairs writes its duration and stack cap as literals, so its two X's are
  both crit — by position the 12-second duration took the second and printed
  "+1200% Critical Chance". Same shape in Galvanized Scope, Twitch, Reflex
  Draw, Aerial Ace.
- Rank-varying numbers written as LITERALS: `hawk_eye` "+80% Zoom" and
  `steady_hands` "-60% Weapon Recoil" (both ramp from a quarter of that), and
  "for 9s" in five pistol buff mods whose duration ramps 2s → 9s.
- A ramping duration stored as one constant (eight mods): the card read the
  max-rank duration at every rank. `duration_rank0` states the other end;
  `duration` stays the max-rank value the engine builds with.

`fixed_and_rank_varying_values_land_in_the_right_slots` pins the cases;
`desc_info_fills_every_x_across_the_pool` (now over EVERY class, not just the
pistol pool it was written for) fails on any placeholder left unfilled.

### Mod compatibility is a UNION of pools (2026-07-31)

"Primary Mod" is not one pool. DE tags every mod, and WFCD carries the tag as
`compatName`:

| tag | count | who draws it |
|---|---|---|
| `PRIMARY` | 10 | every primary weapon |
| `Rifle` | 118 | the rifle class — assault rifles, bows, launchers, snipers, spearguns, crossbows, arm-cannons |
| `Assault Rifle` | 15 | assault rifles only |
| `Sniper` | 14 | snipers only |
| `Bow` | 10 | bows only |
| `Shotgun` | 119 | shotguns — a separate pool, not a subset of Rifle |

So a weapon's pool is a union: a launcher draws `PRIMARY` + `Rifle` and no
narrower tag; a bow draws `PRIMARY` + `Rifle` + `Bow`; a shotgun draws
`PRIMARY` + `Shotgun`. `data/mods/<pool>/` is one pool each and a weapon names
the ones it draws (`mod_pools: [primary, rifle]`).

One flat pool per weapon was right only while every rifle-class weapon in the
roster was a launcher. It fails in both directions the moment a second type
arrives: an assault-rifle mod would be offered to a launcher, and a shotgun
would draw nothing.

**Still missing: 6 of the 10 `PRIMARY` mods.** Two matter a great deal —
**Hunter Munitions** (+30% chance to apply Slash on a critical hit) and
**Sinister Reach** (+12m beam range, which the Torid Incarnon's beam wants).
The others are Combustion Beam, Hunter Track, Shivering Contagion, Aero
Periphery.

They are NOT added yet on purpose. Their mechanics — slash-on-crit, kill
explosions, status spread — are not in the engine, and a mod the engine cannot
model does not merely go missing from the optimizer: it gets RANKED, at zero.
A build page that lists Hunter Munitions and scores it worthless is worse than
one that does not list it, because the first looks like an answer. Each needs
its mechanic before its card.

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
| `wfcd.py` | loads the vendored WFCD export, indexed by `uniqueName` |
| `crosscheck.py --type <T>` / `--arcanes <S>` | DUAL VERIFICATION: ours vs wiki vs WFCD. Reports MISMATCH (ours disagrees with WFCD) and SOURCE-SPLIT (the two sources disagree with each other). Compares each number at the SOURCE's own precision, since WFCD's display rounds |
| `reconcile_wfcd.py --type <T>` | rewrites `base_drain` / `max_rank` from WFCD. Only those two — matching an effect kind to a phrase in a stat string is guesswork, and a wrong guess there changes damage |
| `reconcile_families.py --type <T>` | `family` / `incompatible_with` by union-find over the wiki's `Incompatible` lists — the mutual-exclusivity groups the optimizer enforces |
| `gen_assets.py` | writes `data/assets.yaml`: our ids → WFCD `imageName`, which the UI serves from `cdn.warframestat.us`. **Re-run whenever a weapon, mod or arcane is added** — a missing entry is a silently image-less card, which is how the Torid shipped with no picture and the whole rifle mod set with none either. Misses are written as commented lines for a human to fill; `<weapon>_incarnon` is always a miss worth fixing by hand, because the resolver finds the Genesis ADAPTER item rather than the gun |

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

- ~~The module's `Description` can lag its own `MaxRank`~~ — **withdrawn
  2026-07-30.** That read Hawk Eye and Steady Hands backwards: the module's
  `MaxRank` 5 is the wrong field, not its description. WFCD says fusionLimit 3
  and the wiki page's rank table agrees, so the rank-3 text was right all
  along. `audit_mod_effects.py` still has a DESC-STALE category because the
  shape is possible in principle, but neither known case is one.
- Amalgam Barrel Diffusion really grants **109.50%** multishot and the tooltip
  rounds to 110% — an explicit allowlist entry with the wiki quote, not a
  tolerance that would hide the next real error.
