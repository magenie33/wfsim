# Weapon intake — what it costs, and what order to do it in

The wiki knows 353 ranged weapons. This is what one costs and how the next
ones get in.

**THE ORDER IS docs/INCARNON.md's** (owner, 2026-08-07): every Incarnon primary
and secondary comes first. What one weapon costs, and the READ THE PAGE rule,
apply to all of it; batches A and B below are the backlog behind that program,
and batch C (sniper) is part of it (Vectis).

## The bottleneck is MEASUREMENT, not typing

Reading a weapon out of the wiki is now mechanical —
`private/scripts/wiki_weapons.py` gives the whole entry as JSON, cross-checked
against WFCD, and every field maps to our schema by name. Call that an hour a
weapon, most of it reading comments.

What is not mechanical is the rule that makes any of it worth having:

> **Golden values only change with an in-game measurement justifying it. A
> faithful-looking implementation without a measurement is not correct.**

So the plan is shaped around play sessions, not typing sessions. **Batch
weapons that can be measured in one sitting**, and prefer batches where the
weapons are already correct by construction — a weapon that reuses only
mechanics already pinned by an existing measurement needs no new one.

(Note: `tests/golden/` is an empty directory. The measured values live as
`#[test]`s inside the engine modules, each citing an `M<n>` from
docs/MEASUREMENTS.md — 30 of them so far.)

## READ THE PAGE, NOT ONLY THE MODULE

The filter that picks "simple" weapons reads the data module's STRUCTURED
fields — `Attacks`, `Trigger`, `Class`, `Zoom`, `SniperComboMin`. A weapon's
PASSIVE is not one of them. It lives in the page's prose under
`==Characteristics==`, and the module says nothing about it at all.

Gotva Prime went in on 2026-08-05 as a "no new mechanics" rifle. It has one:
"Status Effects have a 15% chance to set the next hit's Critical Chance to
300%" — a probabilistic, status-triggered crit-chance LOCK, which the engine
has no machinery for. The commit claimed the batch needed nothing new; that was
true of the other two and false of this one, and nothing caught it because
nothing was reading the page.

So the intake check is: `?action=raw` on the WEAPON PAGE, and read
`==Characteristics==` before calling anything simple. Karak Wraith has no such
line; Prisma Grinlok's only one is "Innate Madurai polarity", which is data we
already carry.

**And `==Notes==` is the same trap one section further down.** The Phenmor was
audited perk by perk, matched the infobox on every field, and shipped as done
(2026-08-08). Its Notes carry *"Fire rate decreases from 100% to 60% over 51
shots as the trigger is held"* — a mechanic no evolution mentions, that the
infobox contradicts, and that is worth more than any perk on the weapon: 51
shots is 3.8 s of a 408-round magazine, so the Incarnon form ran **51% too
fast** until 2026-08-10.

The pattern in both cases is the same and it is worth stating as a rule: **the
structured fields describe the weapon, the prose says which of them are lies.**
A stat that the page later qualifies is more dangerous than a stat that is
missing, because the missing one leaves a hole and the qualified one leaves a
number that looks right.

## ADVERSARY WEAPONS: Kuva, Tenet, Coda — and the axis they bring

A weapon carried by a Kuva Lich (Grineer, 赤毒), a Sister of Parvos (Corpus,
Tenet) or a Coda (Infested, 终幕) is not just another entry: it comes with a
**VALENCE BONUS**, and that is a build axis nothing else in the roster has.

**Kuva Nukor is the template** (owner, 2026-08-13).

VERBATIM (wiki, Kuva Weapons §Elemental Bonus):

> The Kuva weapons additionally have bonus damage of one damage type which can
> either be Impact, Heat, Cold, Electricity, Toxin, Magnetic, or Radiation,
> ranging from **25-60% of the weapon's base damage** determined randomly. …
> This additional bonus damage **applies as weapon base damage**, meaning
> elemental mods and status that scale from base / modified base damage will be
> affected.

Three consequences, and each is why the axis exists rather than a field:

- **It is a property of the COPY, not of the model.** Two Kuva Nukors are two
  different weapons and neither is "the" Kuva Nukor. So the choice lives in the
  BUILD (`valence: {element, bonus}`, saved in a build preset, reset when the
  weapon changes) while the weapon declares only what it CAN have
  (`valence: {elements, min, max}`). Same split a riven has.
- **Nothing downstream had to learn it.** It arrives as the weapon's own base
  vector — merging into that element if the weapon already deals it — and an
  innate element already composes with the mod elements the way MECHANICS §3
  rule 2 says. `weapons_data::apply_valence` is the whole implementation, called
  where `apply_deployment` is called, which is every path that builds a weapon
  for a request.
- **The BOARD ranks one, and the row states its element.** 25% and 60% are a
  35-point swing in base damage, so a row that does not say which element is not
  reproducible — which is the one thing every row on that board is. A board row
  therefore carries `valence`, the ruler scores every row at the roll's MAXIMUM
  (which every player can reach by Valence Fusion, so it is a term of the
  standard rather than a property of somebody's copy), and the element is part
  of `builds::identity` — two elements are two entrants, exactly as two modes
  are. A build with NO element is refused by `builds::validate`, which is
  legality rather than a ruler's opinion.

  **A weapon with no row is not blocked, it is unsubmitted.** The board is fed
  by submissions and the builder's official builds ARE its rows, so a weapon
  nobody has run under the official ruler has neither — 101 of 159 entries are
  in that state today, the Kuva Nukor among them. Verified end to end
  (2026-08-14): under the official ruler a full eight-mod, arcane-seated Kuva
  Nukor passes every submission gate and its payload carries the element.

**Rank 40, not 30.** "Polarizing the weapon increases its max rank by 2, capping
at rank 40 after 5 polarizations, granting the weapon additional mod capacity" —
so a maxed adversary weapon carries **80** capacity where an ordinary one
carries 60, which is most of why these builds look the way they do
(docs/INVESTMENT.md).

**What the Nukor cost, and what the next one will not.** The axis is written
once; the second Kuva weapon is an ordinary intake plus four lines of yaml. What
each still costs is its own prose — the Nukor's Characteristics carry a beam
ramp that starts at 30% instead of the usual 20%, a 2-target chain worth nothing
against one enemy, and MICROWAVE, a status effect of its own that Condition
Overload counts and this engine has no type for.

## THE ROUTINE — what a first pass has to touch to be right

Written after the Kuva Nukor, whose element reached the builder and not the
submission, and after the Boar Prime, whose CO row was read as a family rule.
Every line below is a mistake somebody already made. Work it in order; each step
names where the answer lives, so "I did not know where to look" is not one of
the ways this goes wrong.

**A PRIME AND ITS ORDINARY ARE ONE INTAKE** (owner, 2026-08-14). They share a
riven family, a CO row, a Primary Compression row, a mod pool, an art path and
every sentence of prose — so doing them apart pays for the reading twice and
gets one of them wrong, which is exactly how the Boar's row became the Boar
Prime's. Add both, or neither.

### 1. The sources, and which one wins

| what | source | rule |
| --- | --- | --- |
| every stat | the RENDERED weapon page's infobox | `?action=raw` gives you `{{WeaponInfoboxAutomatic}}` and nothing else; `Module:Weapons/data/*` truncates alphabetically and a reader invents past the cut |
| the same stats again | WFCD `warframe-items` `attacks` array | joined by `internal_name` == `uniqueName`, NEVER by name |
| what the numbers MEAN | the page's Characteristics, Notes, Tips | the structured fields describe the weapon; the prose says which of them are lies |
| `base_drain` / `max_rank` | WFCD only | the wiki is wrong for ~20 mods |

Disagreements are not rounding. Record which source you took and why, in the
file — every existing weapon does.

### 2. The per-weapon CATALOGS, before writing a line of yaml

Both are a formula plus a table with ONE ROW PER WEAPON, both are
authoritative, and in both **absence means ORDINARY, not unknown**
(docs/CATALOGS.md).

**They are cached locally**: `node scripts/fetch_catalogs.mjs` puts both pages'
wikitext under `vendor/wiki/`, so a row is `grep`-able and re-reading a catalog
costs nothing. It goes through the repo's headless Chrome because the wiki
answers `curl` with a 403 and its API with a bot challenge. Run it with
`--force` before an intake: it says when a catalog MOVED, which is the one
event that invalidates every row this repo has transcribed.

- **Condition Overload** — find the row by the entry's own name AND its Attack
  Name cell. A row for `Throw` says nothing about Primary Fire, and a row for
  the ordinary variant says nothing about the Prime. Transcribe the columns
  verbatim into the file, including the ones you are not acting on.
- **Primary Compression** — the same, for every AoE the weapon has. Its
  `Compression Effectiveness` column is about WHICH RADIUS the arcane reads,
  not how much of the bonus is paid.

An AoE part takes no CO unless its own row says so. Do not generalise a row to
a class, a form, or a family.

### 3. Its ATTACKS, and how many entries they need

One weapon ENTRY carries one attack. A weapon with a second attack you can
choose at the trigger — a charged shot, an alt-fire — is TWO entries in one
`transform_group`, and the alternate one gets its own `form:` kind. Reach for a
kind that reads true: borrowing `charged` for a thrown spear puts a wrong word
in a saved preset and in every share link. Adding a `FormKind` is three lines.

A part that is not a separate trigger is not an entry: an explosion on impact
is a `radial:` on the attack it belongs to, with its own crit, status, element,
radius and falloff.

### 4. What the engine CANNOT do with it

Write the list before writing the numbers, in the weapon's own `unmodeled:` —
the page shows it, and a gap that lives only in your head ships as a promise.
`docs/UNMODELLED.md` names the six standing reasons (one target, no distance,
no movement, no holster, infinite ammo, nobody shoots back). A weapon usually
adds nothing new; when it does, that is the interesting half of the intake.

### 5. The rest of the checklist

| item | where | fails how, if skipped |
| --- | --- | --- |
| art | `data/assets.yaml`, then `scripts/fetch_images.py` | `build_site_app.py` FAILS on a missing file |
| Chinese name | `data/i18n/zh/names.yaml` via `scripts/wfcd_i18n.py` | **transcribed from DE, never translated**; unreachable ⇒ leave empty and say so |
| riven family | `riven_family:` | the editor offers the wrong stat pool |
| innate polarities | `polarities:`, `exilus_polarity:` | every Forma plan is wrong |
| disposition | the infobox | every riven is wrong |
| `internal_name` | WFCD `uniqueName` | the join key for every future cross-check |
| tests | `cargo test --workspace` | a data file that does not parse fails the build, not the test |
| checks | `check_parity`, `check_equip_rules`, `check_disclosure` | per-weapon by construction; run the ones the weapon can reach |

### 6. Last, the question that is not about this weapon

Does it exercise a mechanic nothing else does? Then it needs a MEASUREMENT
(docs/MEASUREMENTS.md) before its number means anything — and a golden test, or
the number is a faithful-looking implementation with nothing behind it.

## What one weapon costs

| item | where | notes |
| --- | --- | --- |
| the weapon file | `data/weapons/<slot>/<id>.yaml` | 70–100 lines, every number from the wiki module |
| art | `data/assets.yaml` + `scripts/fetch_images.py` | the build FAILS on a missing file, so this is not optional |
| Chinese name | `data/i18n/zh/names.yaml` | **transcribed from DE, never translated.** If it cannot be reached, leave it empty and say so |
| checks | `check_parity`, `check_equip_rules`, `check_enemies` … | re-run; they are per-weapon by construction |
| a measurement | docs/MEASUREMENTS.md | **only if the weapon exercises something not already pinned** |

## The batches

### Batch A — RIFLES. The only batch that buys mod coverage.

A `Class = "Rifle"` weapon carries `CompatibilityTags = { "ASSAULT_AMMO" }`,
and that tag gates **15 mods nothing in the roster can reach today** — Tactical
Reload, Spring-Loaded Chamber, Maximum Capacity, Guided Ordnance, Vanquished
Prey, Rifle Ammo Mutation (+ Primed), Deft Tempo, Hydraulic Gauge, Loose Hatch,
Overview, Recover, Gun Glide, Tainted Mag. That is the single largest mod gate
in the game we do not have — larger than Sniper's 14.

Our two rifle-pool weapons are a launcher (Torid) and a bow (Cernos Prime).
Neither is an assault rifle, so the tag has never applied.

Zero new engine mechanics. Candidates, all single-attack, Auto or Semi-Auto,
hit-scan or plain projectile:

| weapon | MR | trigger | crit | status | why |
| --- | --- | --- | --- | --- | --- |
| **Gotva Prime** | 14 | Auto | 23% | 27% | modern Prime auto rifle, the shape most players hold |
| **Kuva Karak** | 13 | Auto | 23% | 31% | Kuva variant — exercises the Kuva bonus-element axis |
| **Prisma Grinlok** | 11 | Semi-Auto | 21% | 37% | a SEMI-AUTO rifle: different cadence, and it is what proves the trigger axis is not shotgun-only |
| AX-52 / Reconifex | 12 / 14 | Auto | 26 / 28% | 18 / 16% | spares |
| Veldt / Grinlok | 8 / 7 | Semi-Auto | 22 / 15% | 22 / 35% | low-MR spares |

Three is the recommendation. The 15 mods are the deliverable; the weapons are
how they become reachable.

### Batch B — PISTOLS, DUAL PISTOLS, SHOTGUNS. Breadth, no new mods.

Every one of these pools is already covered (Laetum, Dual Toxocyst, Boar
Prime), so this batch unlocks **zero** mods. What it buys is roster breadth and
engine validation across shapes we have one example of each.

| weapon | class | why |
| --- | --- | --- |
| **Arca Plasmor** | Shotgun | a PROJECTILE shotgun — every shotgun we model is hit-scan |
| **Vaykor Hek** | Shotgun | syndicate variant, 25% crit, the classic burst-damage shotgun |
| **Aklex Prime** | Dual Pistols | 150 base, semi-auto, high per-shot |
| **Knell Prime** | Pistol | 40% crit / 10% status — a crit-only profile the roster lacks |
| **Magnus Prime** | Pistol | balanced 28/28, the "no gimmick" control |

### Batch C — SNIPER (Vectis Prime). 14 mods + two new mechanics.

The 14 sniper mods, plus:
- **Shot Combo Counter** — `1.5 + 0.5⌊log₃(hits / min)⌋`, min 5 → 5/15/45/135
  for 1.5x/2x/2.5x/3x. 2 s window, scoped-in only, multishot and punch-through
  each count, AoE and DoT do not.
- **Zoom buffs** — intrinsic, not moddable, additive with mod bonuses of the
  same kind. Vectis Prime: 3.5x → +40% headshot damage, 6x → +60%.

Both gate on `aiming`, which the scenario already carries, and the benchmark
sets `aiming: true` — so a sniper on the board gets its combo.

Then its Incarnon form separately: embedded projectiles dealing Cold three
times over 2.5 s, plus a 6.7 m headshot explosion. `Critical Parallel` and
`Survivor's Edge` are the same perks Boar Prime has, so they reuse the
define-once perk path.

## Order, and why

**A → B → C.** A is the biggest mod win per unit of work and is pure data. B is
pure data with no mod win, so it is the batch to do while waiting on
measurements. C is the one that needs engine work and a measurement session of
its own, so it goes last and gets undivided attention.

## Per batch, what is actually needed from the owner

- **A**: one session measuring one rifle's sustained DPS on a known target, to
  confirm the auto-rifle cadence. The other two are the same mechanics.
- **B**: one measurement for Arca Plasmor (projectile shotgun is a shape we
  have not measured). The rest reuse pinned mechanics.
- **C**: a combo-counter session — hit counts against multiplier steps — and a
  zoom-tier headshot check. This is the one that cannot be skipped.
