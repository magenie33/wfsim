# Weapon intake — what it costs, and what order to do it in

Plan of record, 2026-08-04. The roster is 7 weapons; the wiki knows 353 ranged
ones. This is how the next ones get in.

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
