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

### The CN wiki is reachable through its API, not its pages (2026-08-03)

`warframe.huijiwiki.com` — the second source `data/README.md` names for display
names, and the ONLY source for Incarnon evolution strings (DE exports none;
WFCD has no entity for them) — serves every page URL and every `?action=raw`
behind a Cloudflare challenge. 403, "Just a moment...", no body.

Its **MediaWiki API answers normally**:

```
curl -A "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/131.0"   "https://warframe.huijiwiki.com/api.php?action=parse&page=<标题>&prop=wikitext&format=json"
```

Recorded because the wall produced a worse outcome than an empty field: with
the pages unreachable, five Boar Prime evolution names were TRANSLATED from
their English instead, and four of the five were wrong (堡垒齐射 / 佣兵膛室 /
熟练握把 / 暴击并行, against DE's 要塞齐射 / 佣兵枪膛 / 熟练之握 / 临界平行).
DE's Chinese names are routinely non-literal — Commodore's Fortune is 准将沐福
— so a name that cannot be read must be left EMPTY and asked for, never
derived.

#### …and on 2026-08-05 the API was walled too

The `api.php` call above now answers **403 Forbidden** for every action,
`list=search` included — the same Cloudflare challenge that already covered the
pages. So the Burston family's 18 evolution names went in EMPTY, which is the
rule ("if a source cannot be reached, LEAVE IT EMPTY AND SAY SO") and which
`python scripts/wfcd_i18n.py check` reports as 18 unnamed.

WEAPON names survived it, because they have a second source: **WFCD's
`i18n.json` carries `zh.name` per `uniqueName`**, so 伯斯顿 / 伯斯顿 Prime /
野猪 / 野猪 Prime are DE's own strings joined on internal name rather than
anyone's reading of the English. Evolution strings have no such fallback —
"DE exports none; WFCD has no entity for them" — which is exactly why the CN
wiki is the only source for them and why losing it costs those 18 and nothing
else.

#### …and on 2026-08-07 it opened again — for curl, not for the language

Both `api.php` and the plain pages answer **200** now. The interesting part is
that they answer 200 to **curl** and **403 to Python's `urllib`**, from the same
machine, in the same minute, carrying the same browser User-Agent. So the
challenge is reading the **TLS fingerprint**, not the header: a fetch written in
the obvious way ("it's just an HTTP GET, do it in the script") will conclude the
wall is still up and leave names empty that could have been read.

    curl -s -A "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36       (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"       --get --data-urlencode "page=盗贼灵化之源"       "https://warframe.huijiwiki.com/api.php?action=parse&prop=wikitext&format=json"

The evolution pages are `<武器>灵化之源` (伯斯顿灵化之源, 盗贼灵化之源,
野猪灵化之源); a weapon page transcludes them with `{{#lst:…|Incarnon}}`, so
reading the weapon page finds the section EMPTY and the names one hop away.

#### …and on 2026-08-08 the whole roster went through it

`scripts/cn_evolution_names.py` reads every family's page and transcribes both
the perk NAME and its CARD TEXT — 447 of the 449 that were still empty. Two
things about it are worth keeping:

**The join is by NUMBERS, never by position.** Both pages list a tier's perks
in the same order, so position would usually work, and "usually" is how a tier
ends up silently shifted by one. It was tried: our perks are read in FILENAME
order, and a positional pass swapped Evolved Autoloader with Swift Deliverance
on the Dera, Kinetic Baffle with Frictionless Flight on the Felarx, and
Marksman's Hand with Ready Retaliation on the Dex Sybaris. Every one of those
looked right in the output.

**A page can be wrong about its own weapon, and the other pages say so.** The
Dera's has 迅速判决 against a magazine-capacity line and 扩充齐发 against a
projectile-speed one — the opposite of what those two names carry on sixteen
other pages, so number-matching faithfully reproduced its swap. The script now
runs until it stops moving: each round counts what each English perk name was
called, and a name read on two or more pages beats a one-off. It reported three
such disagreements (Deathtrap Trigger 死陷触发 8:3, Extended Volley 扩充齐发
18:2, Survivor's Edge 生还占优 29:2).

Two names are still empty and say why in the file: the Felarx's Kinetic Baffle
and Frictionless Flight are listed together in one table cell away from their
values, both carry the number 50, and nothing on the page ties either name to
either effect.

With that, the Burston and Furis families' 36 evolution strings went in and
`wfcd_i18n.py check` reports **nothing unnamed in any family** for the first
time. Two of them also corroborated engine fixes made the day before from the
English wiki alone — 力量前奏 reads "暴击几率低于 40% 时，**基础**暴击伤害增加
+3x" (Prelude of Might applies before mods, not after), and 风雷骤起 reads
"+40% 多重射击，持续 2s，最高叠加至 3 层" (Stormburst's three 2-second stacks).
A second source agreeing is not a measurement, but it is the cheapest check
there is.

### A card is TWO fields, and we were reading one (2026-08-03)

WFCD's `i18n.json` carries a mod's localized card in two places, and DE decides
which one a given sentence lands in:

| field | holds | example |
| --- | --- | --- |
| `levelStats` | the rank's numbers | `["+40% 伤害", "+0.25 穿透"]` |
| `description` | the RULE the card opens with | `["仅适用于半自动扳机。射速无法修改。"]` |

`scripts/wfcd_i18n.py descriptions` read only `levelStats`, so the opening line
was dropped from **35** mods and arcanes — the Cannonades printed their damage
and punch through and said nothing about the trigger they need or the fire rate
they lock, and Firestorm/Fulmination lost "提高范围攻击武器的爆炸半径。" that
their English cards carry.

There is no rule for guessing which field to look in: Primary Acuity's
"多重射击无法变动。" is inside `levelStats`, the Cannonades' equivalent is in
`description`. Both are read and joined, prefix first. The one guard is against
DOUBLE printing — DE writes an augment's whole sentence in both fields with
`|val|` where the number goes, so a prefix already present in the rank line
(compared with digits, the placeholder and whitespace removed) is dropped.

The generator is the only writer of `descriptions.yaml`; a gap like this is
fixed there and regenerated, never patched into the file.

## The wiki's own METHOD page, and where the mechanics pages are (2026-08-09)

Two links worth having in one place, because the answer to "where does this
number come from" for a MECHANIC is not the same as for a data field, and this
document was only about the second (owner, 2026-08-09).

### [`WARFRAME_Wiki:Research`](https://wiki.warframe.com/w/WARFRAME_Wiki:Research)

The wiki's statement of how it establishes anything — its source hierarchy and
its testing toolbox. Both are useful here for different reasons.

**The hierarchy is the same shape as ours**, which is why a wiki page is worth
what it is worth: community research with a stated method > first-party data
(game files, Codex, Arsenal) > developer communications > third-party content >
anecdote. A page carrying a worked example with numbers sits near the top of
that; a page carrying an adjective sits near the bottom, and both look the same
in a quote. When two wiki pages disagree — the EN and CN cards for Xata's
Whisper disagree about its rank ladder and about the body-part double dip
(M40) — this is the tiebreak to apply: prefer the one that shows its arithmetic.

**The toolbox is the protocol library `docs/MEASUREMENTS.md` keeps reaching
for**, and several entries are things worth knowing exist before designing an
experiment:

| tool | what it establishes |
| --- | --- |
| Simulacrum (Relays, 50,000 standing) | the arena every M-number uses; enemies to level 5×MR+30 |
| Synthesis Scanner + Data-Parse Widget (25,000 standing) | an enemy's health class and damage modifiers, in game — the vulnerability COLUMN read off the target rather than off a table |
| Codex | first-party enemy stats and health classes |
| % -damage abilities (Smite, Reave, Mend & Maim) | total health/shields = damage dealt ÷ percent, so a pool can be measured without a table |
| armor strip at exact strength (Seeking Shuriken @143%, Pillage @400%, Abating Link @168%) | a pure-damage test with the armor curve taken out |
| Shattering Impact | the same, and it works on invulnerable enemies |
| Adaptation | which damage TYPE an enemy deals, by watching what it adapts to |
| weak-point revealers (Cyte-09's Seek, Zenith/Scourge alt-fire, Vesper 77 ADS, Thurible, Laetum/Phenmor) | where the multipliers actually are on a model |
| Public Export / drop tables / World State API / EE.log | first-party data, and the export is the one this repo already consumes second-hand through WFCD |

None of that is adopted as a data SOURCE — the two-source rule above is
unchanged. It is a list of instruments, and its place in this repo is that
`docs/MEASUREMENTS.md` designs protocols and this is what they can be built out
of.

### The MECHANICS pages, which are not the same as the item pages

An item page states what a thing does; a MECHANIC page states the formula every
item of that kind obeys, and it is usually the only place the general rule is
written down. `Extra_Hit` is the case that prompted this note: Xata's Whisper's
own page describes its behaviour in prose, and the one line that makes the
prose add up ("Faction Damage Bonuses appear twice in the equation") is on the
mechanic page and nowhere else. MECHANICS §7 §"Extra Hit" quotes it in full.

**So when a card says something odd, look for a mechanic page before recording
it as an anomaly of that card.** `Extra_Hit`, `Faction_Damage_Bonus`,
`Damage_over_Time`, `Enemy_Body_Parts`, `Critical_Hit`, `Status_Effect`,
`Multishot`, `Condition_Overload_(Mechanic)` and the per-type
`Damage/<Type>_Damage` pages are the ones this engine has needed so far, and
each is cited at its formula in MECHANICS.

**And the CN wiki is a second opinion on mechanics, not only on names.** Its
真理密语 page carries three worked examples, an IPS-distribution rule and a Blast
clause that the EN page has none of — see §"The CN wiki is reachable through its
API, not its pages" for how to read it, and M40 for what it settled.

## The module pages TRUNCATE, and a summariser will fill the gap

`Module:Weapons/data/primary` and `/secondary` are single Lua tables of a few
hundred KB. Fetched through a summarising reader they arrive CUT OFF — measured
2026-08-12, the primary reached "Felarx" and the secondary "Hystrix Prime", both
alphabetical. Asked for a weapon past the cut, the reader answered with
confident numbers that were not in the content: a Sicarus fire rate of 5 (it is
3.5) and a Vasto of 3.33 (2.5). Both would have gone into the data.

Two habits come out of it, and they cost nothing:

- **Make absence answerable.** Ask for the alphabetical RANGE actually received,
  and say "if it falls outside that range, answer 'not present in source'".
  A reader that can report absence stops inventing.
- **The RENDERED weapon page is the reliable route for one weapon's stats.**
  Its infobox carries the same module fields — including `Burst Count` and
  `Burst Delay`, which nothing else publishes — and it is small enough to
  arrive whole. Cross-validated: the Dex Sybaris's page and its module entry
  agree digit for digit (2 and 4 rounds, 0.0900 s).

## Which source wins (revised 2026-07-30)

NO ONE SOURCE IS AUTHORITATIVE FOR EVERY MECHANICAL FIELD, and nothing can
show which of them is wrong about a field while only one is consulted. That is
what the cross-check buys, and the table below is which source wins where.

| field | authority | why |
| --- | --- | --- |
| `base_drain`, `max_rank` | **WFCD** (`vendor/warframe-items`) | `Module:Mods/data` is wrong for ~20 mods. Checked against the wiki PAGE rank tables as a third, independent data point (hand-maintained but per-rank, so hard to get wrong) — Point Strike, Split Chamber, Metal Auger, Barrel Diffusion, Convulsion, Deep Freeze, Gunslinger, Suppress — and the page agreed with WFCD **8/8** |
| `polarity`, `rarity`, `exilus`, verbatim `description` | **wiki module** | unchanged; WFCD has no exilus flag and its display text is rounded |
| `internal_name` | **WFCD**, when they split | it is the JOIN KEY, so it is decidable rather than a matter of taste. `Primed Deadly Efficiency` (2026-08-01): module `…PrimedArchwingDamageAfterReloadMod`, WFCD `…DamageOnReloadMod`. With the module's spelling it was the ONE Arch-Gun mod with no localized card text — the join silently found nothing. WFCD's joins and yields all 11 ranks, which the number-agreement test then validates against our own values. The module is hand-maintained; WFCD is generated from DE's export |
| per-rank effect VALUES | **WFCD `levelStats`** | a full ramp, both ends checkable; the module gives max rank only |
| everything mechanical | **cross-check both** | a disagreement is itself the finding — `crosscheck.py` reports SOURCE-SPLIT |

**Join by `internal_name` == WFCD's `uniqueName`. Never by name.** WFCD carries
stale duplicates sharing a display name: its first entry called "Serration" is
*Flawed* Serration. That join is exact — every mod and arcane file in `data/`
matches exactly one entry, none unmatched. A name-keyed lookup is what made an
earlier pass "confirm" MaxRank 5 for Hawk Eye and Steady Hands from a
collision duplicate, and record the wrong conclusion in both files.

## WEAPONS: the wiki module, and never WFCD's `damage` dict (2026-08-04)

The "cross-check both" rule holds, but for a WEAPON the two sources are not
peers and one WFCD field is simply unusable. Owner's call: **wiki first**.

Measured across every primary the two sources share (158 weapons):

| WFCD's top-level `damage` vs the wiki | count |
| --- | --- |
| identical | 18 |
| **PUNCTURE AND SLASH SWAPPED** | **113** |
| otherwise wrong | 27 |

Vectis Prime is a swap: the wiki gives 140 Impact / 157.5 Puncture / 52.5 Slash
— the puncture-heavy profile a sniper should have — and WFCD's dict reports
52.5 Puncture / 157.5 Slash. The "otherwise wrong" 27 are a different failure:
the dict BLENDS several attacks into one figure. Acceltra is 35 pure Impact on
its direct hit, and the dict returns 26 / 8.8 / 35.2 — the shot and its AoE
averaged together.

**WFCD's own `attacks[]` array is fine** and agrees with the wiki; so does its
`damagePerShot` array, whose order is `[impact, puncture, slash]`. It is only
the flat `damage` summary that is wrong — which is the field a casual reader
reaches for first.

So:

| weapon field | authority |
| --- | --- |
| damage split, crit, status, fire rate, multishot, punch-through, per ATTACK | **wiki `Attacks[]`** — an Incarnon weapon has four of them, and only the module distinguishes them |
| `Zoom`, `SniperComboMin`, `SniperComboReset`, `CompatibilityTags`, `IncarnonCharges`, `Falloff`, `ForcedProcs` | **wiki only** — WFCD carries none of them |
| `InternalName` | wiki, cross-checked against WFCD's `uniqueName` (the join key) |
| magazine, reload, mastery, accuracy, disposition | either; cross-check |
| WFCD top-level `damage` / `damagePerShot` dict | **NEVER.** Wrong for 140 of 158 |

Nothing in `data/` was affected — every weapon file was sourced from the module,
and Boar Prime (26/6/8), Torid (100 Toxin) and Cernos Prime (165.6/9.2/9.2, the
charged shot's doubled 82.8/4.6/4.6) all match the wiki exactly. The rule was
already being followed; this records WHY it has to be, with the number attached.

### `private/scripts/wiki_weapons.py`

Reads the module properly instead of scraping a field at a time. A regex scrape
reads `Damage` out of whichever Attack comes first, and an Incarnon weapon has
four — so this parses the Lua table exactly (numbers, strings, booleans, nested
tables, `math.huge` → `None`). Verified against all five slot modules: 198
primary + 155 secondary + 249 melee + 50 archwing + 41 companion = **693
entries, no parse failures**.

```
python private/scripts/wiki_weapons.py "Vectis Prime"          # the entry, as JSON
python private/scripts/wiki_weapons.py "Vectis Prime" --check  # disagreements vs WFCD
python private/scripts/wiki_weapons.py --slot primary --list   # every name + Class
```

`--check` compares magazine / reload / mastery / accuracy / disposition, the
first attack's crit / status / fire rate, and `attacks[0].damage` — deliberately
NOT the flat dict, which would report a false split on most weapons. Vectis
Prime: 0 disagreements.

The modules are cached under `private/scripts/.cache/` (~330 KB per slot);
`--refresh` re-fetches.

### Sources we do NOT use, and what each would be good for (2026-08-01)

Found in wfhub.top's own credits page (Tenno Hub, a Chinese Warframe
companion site) — its data sources barely overlap ours, and three are worth
knowing about. None of them is adopted: the two-source cross-check is what
caught four classes of generator error in the Arch-Gun pool, so a third
source belongs as a third CHECK, not as a replacement for either.

| source | what it is | what it would fix here |
| --- | --- | --- |
| [calamity-inc/warframe-public-export-plus](https://github.com/calamity-inc/warframe-public-export-plus) | DE's own PUBLIC EXPORT, mirrored and enriched | WFCD is a cleaned second-hand dataset and has gaps: **Primed Deadly Efficiency is absent entirely** — no entry, no `imageName`, and the CDN 404s the card — and its `i18n.json` carries only `name` for riven items, no localized `upgradeEntries`. DE's export would answer both. |
| [oracle.browse.wf/dicts](https://oracle.browse.wf/dicts/zh.json) | DE's own localization dictionaries, per language | our Chinese is assembled from three paths (WFCD i18n whole sentences, a hand-written `effect_phrases` table, hand-written names). One source could unify them. |
| [pa001024/riven-mirror](https://github.com/pa001024/riven-mirror) (MIT) | a riven calculator, source-available | ALREADY USED as a third opinion on the riven config multipliers — see the table in `engine/src/rivens_data.rs`. It is where the "community calculators read 1.0" claim actually comes from, and reading its source is what turned that from a rumour into a citation that can be weighed. |

The rest of Tenno Hub's list is worldstate and market data (`api.warframestat.us`,
`api.warframe.market`, `oracle.browse.wf/worldState.json`, `browse.wf/arbys.txt`),
which is live-service state — nothing this project models. **One exception since
2026-08-08**, and it is not price data: see below.

### RIVEN POOLS: the only source is other people's cards (2026-08-08)

Which stats a weapon's riven can roll is DE's own per-weapon table. It is in no
export, the wiki states a rule and immediately disclaims it (*"usually…
Exceptions exist on a case by case basis"*), and the exceptions are not rare —
six of 26 families in this roster.

So the source is **live riven listings**: `api.warframe.market`'s auction search
returns the stats each card rolled. `scripts/survey_riven_pools.py` counts them
per riven family (the unit DE rolls: one Boar riven fits the Boar and the Boar
Prime) and writes `data/rivens/pools.yaml`. This is the one place market data
enters the repo, and what is taken from it is the ATTRIBUTE LIST, never a price.

Three properties make it usable as a source rather than as a rumour:

- **It is counted, not read.** A riven carries 2-3 of ~24 class stats, so a stat
  that can roll appears in ~55 of 500 listings. Measured: rollable stats landed
  at 30-70, impossible ones at 0-4.
- **It admits a middle.** Listings are typed by players and a few are wrong (one
  Latron card claims Slash). Anything between the two bands is UNCLEAR and the
  engine keeps its own derivation, rather than guessing from a count of nine.
- **A real card is the strongest evidence there is.** The Furis is why: 13 of
  500 is inside the unclear band and a player has the riven.

None of that makes the count an AUTHORITY — see §"Riven pools: the rules decide,
the survey checks" below, which is where the counts go now. Full reasoning and
the corrections: `docs/MEASUREMENTS.md` M35.

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
| `Shotgun` | 119 | shotguns — a separate pool, not a subset of Rifle. **Imported 2026-08-03**: 86 importable, 76 shipped (ten are PvP-exclusive, see below) |

So a weapon's pool is a union: a launcher draws `PRIMARY` + `Rifle` and no
narrower tag; a bow draws `PRIMARY` + `Rifle` + `Bow`; a shotgun draws
`PRIMARY` + `Shotgun`. `data/mods/<pool>/` is one pool each and a weapon names
the ones it draws (`mod_pools: [primary, rifle]`).

One flat pool per weapon was right only while every rifle-class weapon in the
roster was a launcher. It fails in both directions the moment a second type
arrives: an assault-rifle mod would be offered to a launcher, and a shotgun
would draw nothing.

All 10 `PRIMARY` mods are recorded (user, 2026-07-31: record the cards first).
**Hunter Munitions is modeled** — see MECHANICS §"Slash on critical". Four
still carry `kind: unmodeled`, and none of them is costly the way that one
would have been: corpse explosions (Combustion Beam) and status spread
(Shivering Contagion) only pay against a second target, aim-glide zoom (Aero
Periphery) has no damage term, and beam range (Sinister Reach) has no mod kind
yet and nothing in the roster can equip it anyway.

### A compat tag is not the whole restriction

`compatName` says WHICH POOL, not whether the weapon qualifies. Sinister Reach
and Combustion Beam are both tagged `PRIMARY` and neither can go on the Torid
(user, 2026-07-31) — they need a CONTINUOUS weapon, which DE's own internal
names say plainly (`WeaponBeamDistanceMod`, `WeaponBeamExplodeOnDeath`).

The Torid is the case that shows where the line falls: **its Incarnon form IS
a continuous beam and it still cannot take them.** The rule is asked of EVERY
firing mode the build has, and the Torid's other one is a semi-auto grenade
launcher. So `requires_weapon: continuous` is an EQUIP gate — the mod is never
offered — as distinct from `requires`, which lets a mod equip and sit inert.

The same sentence runs the other way and is why the pool depends on the BUILD:
a mod needing `semi_auto` is off the weapon once an evolution unlocks a form
that is not (Dual Toxocyst + Semi-Pistol Cannonade — see MEASUREMENTS M23).
`pool_for_build(weapon, evolutions)` is the one function; a CHARGED form does
not count, because such a weapon lists one trigger.

### Images: a map in the repo, the pictures on a CDN

`data/assets.yaml` maps id -> image filename; the images themselves are served
from `https://cdn.warframestat.us/img/<name>` and no binary ever enters the
repo. The map is small, diffable and auditable, and it carries deliberate
overrides with their reasons (an Incarnon FORM shows its BASE weapon's image —
the generator would otherwise resolve it to the Genesis adapter icon).

Two things about it were weak, and both bit on 2026-07-31 when Verglas Prime
and ten mods shipped with no picture at all:

- **Nothing enforced completeness.** A missing entry fails nothing — it just
  renders as blank. `every_data_entry_has_an_image` now walks every weapon,
  mod and arcane in `data/` and names what is missing.
- **The generator was gitignored and fetched a live API**, so nobody else
  could run it and its output could not be reproduced. `scripts/gen_assets.py`
  is committed now and reads the COMMITTED WFCD export instead, joined by
  `internal_name` == `uniqueName` like everything else here. It only ADDS what
  is missing, so the hand-written overrides survive.

      python scripts/gen_assets.py           # report
      python scripts/gen_assets.py --write   # fill in

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

**Two of its findings are EXPECTED, and both are decisions, not gaps** — read
this before "fixing" either (audited 2026-08-01):

- `--type Rifle` reports 6 MISSING: **Apex Predator, Comet Rounds, Lucky Shot,
  Ripper Rounds, Serrated Rounds, Vanquished Prey**. All six are
  `/Lotus/Upgrades/Mods/PvPMods/Rifle/…` and the module tags them
  `Conclave: true` — but so are the four we DO ship (Agile Aim, Twitch, Eject
  Magazine, Reflex Draw), which Update 17.9 made PvE-legal. The module cannot
  tell the two apart; the authority is the wiki's `Rifle_Mods` / `Pistol_Mods`
  tables, which tag the restricted ones "Exclusive to PvP". The allowlist that
  encodes this lives in the engine test
  `mods_data::tests::only_pve_legal_conclave_mods_are_in_the_pools`, so the
  rule ships with the repo even though the script does not.
- `--type Shotgun` reports 10 MISSING: **Bounty Hunter, Crash Shot, Flak Shot,
  Hydraulic Chamber, Kill Switch, Loaded Capacity, Loose Chamber, Momentary
  Pause, Prize Kill, Shred Shot** — the same class of finding as the Rifle six,
  and settled the same way. All ten are `/Lotus/Upgrades/Mods/PvPMods/Shotgun/…`
  and `Shotgun_Mods` tags each "Exclusive to PvP"; the five that page leaves
  unmarked (Broad Eye, Double-Barrel Drift, Lock and Load, Snap Shot, Soft
  Hands) DO ship and are in the engine allowlist (2026-08-03).
- `--type Archgun` reports `zodiac_shred` exilus false != wiki true. Deliberate
  — `MEASUREMENTS.md` **M17** holds the reasoning and the measurement that
  would settle it.
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

## Auditing the mod set (2026-08-02)

Three mechanical sweeps, run after M22 found Primary Acuity modelled as an
unconditional +350%/+350%. Each compares `data/mods/` against a source that
did not write it:

1. **Values vs DE's card.** `levelStats[last].stats` in the committed export is
   the exact line DE prints at max rank. Compared unit-aware (percent kinds as
   percentages, faction `xN.N` as `N-1`, metres and flat counts on their own
   terms) against each effect's `rankMax`: **161 mods with percentage effects,
   0 disagreements**.
2. **Conditions vs the card's own words.** If the card says "Weak Point" or
   "when Aiming", the model must carry the matching condition. This is the one
   that catches the Acuity class of bug, so it is now a TEST rather than a
   script — `a_condition_on_the_card_is_a_condition_in_the_model`, verified to
   fail on the bug it was written for. The three mods it flags by hand are
   indirect-only payloads (movement speed, accuracy, double jump), which the
   test exempts because the condition cannot change a number this calculator
   produces.
3. **Pool vs `compatName`.** Every mod filed under `data/mods/<class>/` checked
   against the export's own compatibility field: **0 misfiled**.

Equippability beyond the pool (a mod a weapon may not take even though the
pool offers it) is `excludes_weapon` / `requires_weapon`, and the export does
not carry those tags — they come from the wiki page, one mod at a time.

## Equippability: the wiki module is the only structured source (2026-08-02)

A mod's `excludes_weapon` mirrors DE's own incompatibility tags. The WFCD
export does NOT carry them — it has `compatName` (which pool) and nothing
about what a mod refuses. The wiki's **`Module:Mods/data`** does, as
`IncompatibilityTags`, the same list the mod infobox prints:

    curl "https://wiki.warframe.com/index.php?title=Module:Mods/data&action=raw"

157 entries in it carry tags; 13 of those are mods we have, and after this
pass all 13 match our files exactly. Eleven were missing before it
(`power_weapon` on Aerial Ace, Argon Scope, Bladed Rounds, Catalyzer Link,
Galvanized Scope/Crosshairs, Hydraulic Crosshairs, Sharpened Bullets, Eject
Magazine; `modular_gun` on Semi-Rifle Cannonade; `sentinel_mod`/`singleshot`
on Synth Charge). Only `sentinel_weapon` is consulted by the pool filter
today — the rest are recorded because the tag is the fact, and a weapon class
that reads them can arrive later.

**A fetched PROSE summary is not a source.** Primary Acuity briefly carried
`excludes_weapon: [sentinel_weapon, power_weapon]` on the strength of a
page-summary that read "cannot be equipped on sentinel or companion weapons".
The page's own wikitext says nothing of the kind and the module gives the mod
no tags at all. Both structured sources agreed and the sentence was invented.
Use the module, or the raw `action=raw` wikitext — never a summary of either.

## Enemies: art, and the Acolytes' three-way stat conflict (2026-08-03)

Enemy portraits are **not** in `data/assets.yaml`. That file maps ids to WFCD
`imageName`s, and WFCD has no enemy art we can use: `api.warframestat.us`
404s "Thrax Centurion", and its `Enemy.json` entries are keyed to a different
internal object (below). The file name therefore lives in the enemy's own
YAML as `image:`, wiki-hosted, exactly like an evolution's `icon:` —
`scripts/fetch_images.py` pulls both through `Special:FilePath` and
`build_site_app.py` refuses to build without them.

### The Acolytes

Six units — Angst, Malice, Mania, Misery, Torment, Violence — with ONE
defensive statline. Three sources, three answers:

| source | health | shield | armor |
|---|---|---|---|
| wiki `Module:Enemies/data/stalker` (post-U40) | 2500 | 1500 | 50 |
| DE's own U40 note ("Base Health: 5,500 (was 550)") | 5500 | 2500 | — |
| WFCD `Enemy.json` | 350 | 200 | 0 |

We take the **wiki module**, and the other two are explainable:

- WFCD is keyed to `.../Acolytes/StrikerAcolyteAvatar`, the wiki module to
  `.../Acolytes/StrikerAcolyteAgent` — **different objects**, so the join by
  `uniqueName` that works for items finds nothing here and the numbers are
  not comparable. WFCD also gives Misery 4000 where every other source has
  all six identical, which is the same symptom.
- DE's note is the patch-note figure; the wiki editor entered 2500/1500 in
  the same revision that carried the U40 changes (rev 2731762, health
  550→2500, shield 200→1500), i.e. after seeing both. The CN wiki still
  carries the pre-U40 350/200/50.

Unresolved until measured in-game. Recorded so a measurement can settle it.

**Impact stacks: 6, not 3.** The EN Acolytes page says any status caps at 4
"with the exception of Impact which can stack up to 3 times". It is the ONLY
page on the wiki that says 3 — `insource:"which can stack up to 6 times"`
hits Bosses, Adversary System, Kuva Lich, Sisters of Parvos, Void Angel,
Necramech and Technocyte Coda, and DE's own U27.3 note (the origin of the
rule, extended to the Acolytes in U29.5.4) reads 6. The CN wiki reads 6 too.
Since the ordinary Impact cap is 5, the exception is inert either way — what
bites is the 4 on everything else.

**Damage attenuation is NOT set.** The Acolytes keep it after U40.0.2, but DE
publishes no MDPI/MDPS constants and we have measured none, so inventing a
fraction would be a faithful-looking guess. The files instead declare
`unmodeled: [damage attenuation]`, which the target card prints — a gap the
reader can see is a limitation; a gap they cannot is a wrong number.

## The faction vulnerability column, checked cell by cell (2026-08-03)

`data/factions/damage_modifiers.yaml` was transcribed by hand from the wiki's
**`Damage/Overview_Table`**. It has now been machine-compared against that
page's wikitext, every faction × every damage type:

    15 wiki columns, 0 mismatched

The table's 15 columns are Tenno, Grineer, Kuva Grineer, Corpus, Corpus
Amalgam, Infested, Infested Deimos, Orokin, Sentient, Narmer, The Murmur,
Zariman, Scaldra, Techrot, Anarchs — and our file holds exactly those, with
the same values. (Beware the parse: the first data row carries a `rowspan`
grouping cell, and a reader that mistakes the `|-` separator for a cell drops
the whole **Impact** row and reports four false mismatches.)

### `FactionDamageOverride` is the wiki's field, not ours

Worth stating because it looks like a wfsim invention. It is in the enemy
module's published schema (`Module:Enemies/data/doc`):

> `FactionDamageOverride` — optional String — "Override for enemies with
> different **faction resistance value** instead of that usually matches their
> faction."

34 module entries carry one (Zariman ×12, Grineer ×5, The Murmur ×2, Corpus
×1; a further 6 hold a pasted InternalName and are wiki typos). Thrax
Centurion's entry reads `Faction = "Unknown"` with
`FactionDamageOverride = "Zariman"` verbatim, which is where our file's pair
comes from.

The schema's wording is also the RULE: it overrides the *resistance value*,
nothing else. So the two faction systems key differently on purpose — Bane
follows `Faction`, the column follows `FactionDamageOverride ?? Faction` — and
a Thrax matches no faction mod while still taking Void ×1.5. It is equally
available to a custom enemy: the field is data, and a hand-made target can set
it to borrow any column in the table.

### Faction values with no column

The enemy modules use 18 distinct `Faction` values; the damage table publishes
15 columns. `Stalker` (the Acolytes), `Unknown` (Thrax), `Duviri`, `Neutral`,
`Objects`, `Predator`, `Prey` and `?` have none.

**That is not a gap — it is the answer.** The fifteen are the whole system, so
a faction the table leaves out is a unit the game gives no vulnerability or
resistance to, and it takes every damage type as written (user, 2026-08-03:
"就只有15个，其他都理解成中性"). `factions_data::column()` returns the neutral
column for an unlisted key rather than reporting an error, and the file holds
exactly the fifteen — no hand-added neutral rows, with a test locking the set
so "everything else is neutral" cannot quietly come to mean "we lost a
column".

## Riven pools: the rules decide, the survey checks

Three files, and which one DECIDES is the whole design (owner, 2026-08-08:
"紫卡不应该是按照规则自动生成的吗？抓取只是来当验证才对"):

| file | role |
|---|---|
| `engine/src/rivens_data.rs::derived_for` | **the model** — the weapon's physical shares, its ammo pool, whether anything it fires travels |
| `data/rivens/exceptions.yaml` | **the overrides** — hand-written, per riven FAMILY, every entry carrying the evidence it came from |
| `data/rivens/pools.yaml` | **the check** — a count over live warframe.market listings, read by a test and by nothing else |

**THE SURVEY IS A CHECK, NOT A SOURCE**, and it was the other way round for a
day. `pools.yaml` outranked the derivation, so a scrape was a silent authority
over 26 weapon families — and a re-run of it came back *"nothing rolls
anything"* for every one of them, wrote itself to disk, and was caught only
because two unrelated tests happened to fail. Nothing in the pipeline was
looking: the file parsed, the pools emptied, and no assertion was about that.

Now `the_survey_still_agrees_with_the_rules` fails, naming the family and the
stat, and the fix is a human one — promote the disagreement into
`exceptions.yaml` with its count, or fix the rule.
`a_survey_that_refuses_everything_is_a_broken_scrape` catches the specific
failure above before anyone reads a number off it.

The exception list is small on purpose: 11 families of 26 have an entry, 15 run
on the rules alone, and a weapon added tomorrow is approximately right before
anybody counts cards. The wiki's own sentence is the licence for it —
*"exceptions exist on a case by case basis"* — and a case-by-case exception is
data, not a formula.
