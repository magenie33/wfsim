# Incarnon guns — the whole roster, what covers what, and what is done

Plan of record, 2026-08-07 (owner): **before any other weapon, do every
Incarnon primary and secondary.** This file is the checklist. docs/WEAPON_INTAKE.md
still holds what one weapon costs and the non-Incarnon backlog behind it.

Everything below is read from the wiki: `Incarnon` for the adapter→weapon
mapping, each `<X> Incarnon Genesis` page for the gauge and the evolutions, and
`Module:Weapons/data/<slot>` for the attack table (`private/scripts/wiki_weapons.py`).
Perk "already carried" means a `data/evolutions/*.yaml` in this repo already
carries that name.

## What the set actually is

- **31 Genesis adapters** — 15 primary, 16 secondary — plus **4 natural
  Incarnon guns**: Felarx, Phenmor (Zariman), Laetum (Zariman), Onos (Sanctum
  Anatomica).
- An adapter is installed on a **family**, not on one weapon: "All weapon
  sub-types such as Prime, Wraith, and Vandal are eligible to install the
  Genesis for their respective weapon" (wiki, Incarnon). So 31 adapters cover
  **65 weapons**, and with the naturals the program is **69 weapons**.
- The same sentence continues: "Akimbo and Dual weapons are **not** considered
  the same weapon under the Incarnon system." Akbronco takes no Bronco Genesis,
  Dual Cestra no Cestra Genesis. Dual Toxocyst has an adapter of its own, which
  is why it is on this list and Dual Cestra is not.
- Every weapon here is **two entries** in `data/weapons/` (base form + Incarnon
  form — the TWO WEAPONS MODEL, see `data/weapons/primary/torid.yaml`), so 69
  weapons is ~138 weapon files.
- **Melee is out of scope**: 14 melee Genesis adapters, plus Innodem, Praedos,
  Ruvox and Thalys. Thalys (Isleweaver) shows up in the wiki's Incarnon gallery
  and is a Heavy Scythe — not a gun, not part of this program.
- **Two adapters were missing from the owner's list**: **Braton** (primary, 4
  variants) and **Lato** (secondary, 3 variants). Conversely **Torid** and
  **Dual Toxocyst** are on neither list and are already done.

## Done today — 9 weapons, 5 adapters, 1 natural

| adapter / weapon | weapons in the repo |
| --- | --- |
| Boar Genesis | `boar`, `boar_prime` (+ `_incarnon`) |
| Burston Genesis | `burston`, `burston_prime` (+ `_incarnon`) |
| Furis Genesis | `furis`, `mk1_furis` (+ `_incarnon`) |
| Torid Genesis | `torid` (+ `_incarnon`) |
| Dual Toxocyst Genesis | `dual_toxocyst` (+ `_incarnon`) |
| Laetum (natural) | `laetum` (+ `_incarnon`) |

Remaining: **26 adapters (56 weapons) + 3 naturals**.

## The cost is PERKS, not weapon files

Across all 34 guns the wiki lists **318 evolution slots / 119 distinct perks**.
This repo carries **37** of those names; **82 are new**, filling **127** of the
318 slots. A name we already carry means the engine effect exists and the new
weapon only needs its own YAML with its own numbers — a name we do not carry
may need a new `kind:` in the engine.

Every Genesis has the same 9-slot shape: EVO1 Incarnon Form, EVO2 ×2, EVO3 ×3,
EVO4 ×3. The naturals have 13.

**Variants are nearly free.** A Prime shares its family's perk list and differs
only in numbers — Boar Prime cost 9 evolution files and no engine work. So the
unit of planning is the ADAPTER, and a 4-variant adapter (Braton, Strun) is the
best return in the set.

**Do the shared perks first.** 19 new perk kinds appear on two or more guns and
cover 64 of the 127 new slots:

| appears on | perk |
| --- | --- |
| 14 | Rapid Reinforcement |
| 7 | Void's Guidance |
| 6 | Deathtrap Trigger |
| 4 | Marksman's Focus |
| 3 each | Hoplite Virtue, Resonant Restore, Blazing Barrel |
| 2 each | Hunter's Mantra, Crimson Overture, Hitman's Hoard, Zeroed In, Deadhead, Brutal Edge, Elemental Dominance, Paladin Virtue, Moonrise Velocity, Infused Shots, Wiseman's Regard, Sage's Resolve |

Two wiki spelling traps, both the same perk under two spellings — do not create
a second id for either: Paris's **"Markman's Focus"** is Marksman's Focus
(Dread, Latron, Despair), and Bronco's **"Practised Grip"** is Practiced Grip,
which we already carry (Boar, Soma, Furis).

## Engine mechanics this program needs

Five shapes the engine has no machinery for today. Each gates the adapters
named and nothing else, so they can be scheduled independently.

| mechanic | gates | what it is |
| --- | --- | --- |
| **Spool-up** | Gorgon ×3, Soma ×2 | `Trigger = Auto-Spool`, `Spool` rounds to full rate: Gorgon 9 / Wraith 6 / Prisma 7, Soma 6 / Prime 4. The first rounds of every magazine fire slower. |
| **Duplex trigger** | Zylok ×2 | Two rounds per press+release. Its own trigger family (the Burston precedent: Burst is not Semi-Auto). The Incarnon form is Charge instead, so Precision's Payoff ("burst headshots") is base-form only — the wiki says so outright. |
| **Per-round reload** | Strun ×4 (and Felarx) | `ReloadStyle = ByRound`: the magazine refills a shell at a time and can be interrupted. |
| **Sniper combo + zoom tiers** | Vectis ×2 | Already scoped in WEAPON_INTAKE §Batch C, with the formula and the zoom-buff rule. |
| **Stug's blob economy** | Stug | Five attacks, all AoE: charged blobs that embed, explode, and a bounce explosion on top. Nothing in the roster resembles it; leave it last. |

Also new but small: Gorgon's Incarnon form uses an **`Auto Charge`** trigger
(hold to charge, repeats), which is not one of the five triggers the engine
parses today (`auto`, `semi_auto`, `burst`, `charge`, `held`).

Everything else reuses shapes already pinned: bows (Cernos Prime), charged
projectiles (Phantasma Prime), hit-scan auto/semi/burst, beams (Torid Incarnon
Form — which is what Atomos and Gammacor fire in their BASE form), shotgun
pellets (Boar), and radial AoE with falloff (Torid, Burston Incarnon).

## The gauge is two numbers, and one of them is datamined

`incarnon.gauge` needs `charges_to_fill` and `rounds_per_charge`. The module's
**`IncarnonChargeGain`** is the rounds granted per charging hit, and the Genesis
page states how many hits fill the gauge; `max_rounds` is their product on every
weapon where the wiki states both (the one exception is Vectis: 5 × 10 = 50, page
says 45 — measure it before trusting either).

**Almost every adapter charges on WEAKPOINT hits. Three charge on DIRECT hits:
Torid, Angstrum, Stug** (wiki, Incarnon — verbatim in `torid_incarnon.yaml`).

## Primary adapters

`*` = in the repo. "new perks" counts names this repo does not carry yet, out of 9.

| adapter | variants | base trigger | Incarnon form | rounds/hit → max | new perks | needs |
| --- | --- | --- | --- | --- | --- | --- |
| Boar | Boar (MR2)\*, Boar Prime (MR11)\* | Auto | held hit-scan | 3 → 150 | 0 | **done** |
| Burston | Burston (MR0)\*, Burston Prime (MR12)\* | Burst/Auto | auto hit-scan + radial | 30 → 600 | 0 | **done** |
| Torid | Torid (MR4)\* | Semi-Auto | held beam | 34 → 170 | 0 | **done** |
| Miter | Miter (MR6) | Charge | auto projectile + radial | 5 → 20 | 2 | — |
| Boltor | Boltor (MR2), Telos (MR12), Prime (MR13) | Auto | auto projectile | 8 → 160 | 3 | — |
| Sybaris | Sybaris (MR5), Dex (MR7), Prime (MR12) | Burst | hit-scan | 8 → 200 | 3 | — |
| Braton | Braton (MR0), Mk1 (MR0), Prime (MR8), Vandal (MR4) | Auto | auto hit-scan + AoE | 10 → 200 | 4 | — |
| Dera | Dera (MR4), Vandal (MR7) | Auto | hit-scan | 2 → 50 | 4 | — |
| Soma | Soma (MR6), Prime (MR7) | Auto-Spool | hit-scan | 10 → 200 | 4 | **spool** |
| Dread | Dread (MR5) | Charge (bow) | charged projectile, 0.6 s | 5 → 20 | 5 | — |
| Latron | Latron (MR0), Prime (MR10), Wraith (MR7) | Semi-Auto | semi projectile + AoE | 5 → 40 | 5 | — |
| Strun | Strun (MR1), Mk1 (MR0), Prime (MR14), Wraith (MR10) | Semi-Auto | projectile + AoE | 1 → 40 | 5 | **by-round reload** |
| Gorgon | Gorgon (MR3), Wraith (MR7), Prisma (MR11) | Auto-Spool | auto-charge projectile + AoE | 0.66 → — | 5 | **spool**, auto-charge |
| Vectis | Vectis (MR2), Prime (MR14) | Semi-Auto | projectile + headshot AoE + embed AoE | 10 → 45 | 5 | **sniper combo, zoom tiers** |
| Paris | Paris (MR0), Mk1 (MR0), Prime (MR8) | Charge (bow) | charged projectile, 0.8 s | 5 → 20 | 6 | — |

## Secondary adapters

| adapter | variants | base trigger | Incarnon form | rounds/hit → max | new perks | needs |
| --- | --- | --- | --- | --- | --- | --- |
| Dual Toxocyst | Dual Toxocyst (MR11)\* | Semi-Auto | auto hit-scan | 30 → 270 | 0 | **done** |
| Furis | Furis (MR2)\*, Mk1 (MR0)\* | Auto | held beam | 14 → 280 | 0 | **done** |
| Vasto | Vasto (MR4), Prime (MR10) | Semi-Auto | burst hit-scan | 3 → 24 | 2 | — |
| Angstrum | Angstrum (MR4), Prisma (MR8) | Charge | auto projectile | 40 → 120 | 3 | charges on DIRECT hits |
| Ballistica | Ballistica (MR2), Prime (MR14), Rakta (MR6) | Burst/Charge | charged projectile, 0.4 s | 1.5 → — | 3 | — |
| Despair | Despair (MR4) | Auto (thrown) | auto projectile + radial | 5 → 20 | 3 | — |
| Gammacor | Gammacor (MR2), Synoid (MR7) | Held (beam) | semi projectile + radial | 1 → 15 | 3 | — |
| Atomos | Atomos (MR5) | Held (chaining beam) | semi projectile + radial | 1 → 21 | 4 | — |
| Bronco | Bronco (MR0), Prime (MR4) | Semi-Auto (shotgun sidearm) | hit-scan | 0.5 → — | 4 | — |
| Lato | Lato (MR0), Prime (MR14), Vandal (MR7) | Semi-Auto | hit-scan | 4 → 24 | 4 | — |
| Lex | Lex (MR3), Prime (MR8) | Semi-Auto | semi projectile | 2 → 20 | 4 | — |
| Zylok | Zylok (MR6), Prime (MR13) | **Duplex** | charged hit-scan + radial | 1 → 12 | 4 | **duplex trigger** |
| Cestra | Cestra (MR4) | Auto | auto projectile | 10 → 150 | 5 | — |
| Kunai | Kunai (MR0), Mk1 (MR0) | Auto (thrown) | projectile | 5 → 20 | 5 | — |
| Sicarus | Sicarus (MR3), Prime (MR14) | Burst | hit-scan | 10 → 120 | 5 | — |
| Stug | Stug (MR2) | Auto/Charge | blob embed + blob explosion + bounce explosion | 4 → 120 | 6 | **blob economy**, DIRECT hits |

## Natural Incarnons

These have no adapter and no base/Incarnon split in the wiki's sense — the form
is intrinsic — but they still model as a transform group here, and they carry
**13** evolutions instead of 9.

| weapon | slot | attacks | new perks | note |
| --- | --- | --- | --- | --- |
| Laetum\* | secondary | semi projectile → auto projectile + radial | 0 | **done** |
| Phenmor | primary | semi projectile → auto projectile | 4 | cheapest gun left in the whole program — 9 of its 13 perk names are already carried, most of them by the Laetum |
| Onos | secondary | auto projectile → held projectile **and** charged hit-scan + radial | 6 | TWO Incarnon attacks in one form — the only gun here that does that |
| Felarx | primary | auto projectile → semi projectile | 11 | almost nothing shared; also `ReloadStyle = ByRound` |

## Order, and why

1. **Phenmor** — 4 new perks, no new mechanic, and 9 of its 13 perk names are
   already carried. Highest ratio in the set.
2. **The 19 shared perk kinds**, driven by whichever adapter needs them first.
   Rapid Reinforcement alone appears on 14 guns; doing it once removes a slot
   from half the remaining program.
3. **The zero-mechanic adapters, widest family first**: Braton (4), Latron (3),
   Boltor (3), Sybaris (3), Lato (3), Ballistica (3), then the pairs (Dera,
   Vasto, Lex, Bronco, Kunai, Sicarus, Gammacor, Angstrum) and the singles
   (Miter, Despair, Cestra, Atomos).
4. **Paris + Dread** together — one bow session covers both, and Cernos Prime
   already pins the charge/uncharge shape.
5. **The mechanic-gated ones, one mechanic per batch**: Soma+Gorgon (spool),
   Strun+Felarx (by-round reload), Zylok (duplex), Vectis (sniper combo).
6. **Onos**, then **Stug** last — the two shapes nothing else shares.

## What has to be measured

Only the mechanics, not the weapons. A weapon whose every part is already
pinned needs no new measurement (the rule is in WEAPON_INTAKE.md, and it is why
Boar Prime cost nothing to verify). New sessions are needed for:

- **spool** — rounds-to-full-rate against sustained DPS, one Gorgon;
- **by-round reload** — Strun, interrupted and uninterrupted;
- **duplex cadence** — Zylok, shots per second at the listed fire rate;
- **sniper combo + zoom** — the Vectis session already specified in
  WEAPON_INTAKE §Batch C;
- **Vectis's gauge** — 5 hits × 10 rounds should be 50, the page says 45;
- **Stug** — everything about it.
