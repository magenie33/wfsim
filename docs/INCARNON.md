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

## Done — 10 weapons, 5 adapters, 2 naturals

| adapter / weapon | weapons in the repo |
| --- | --- |
| Boar Genesis | `boar`, `boar_prime` (+ `_incarnon`) |
| Burston Genesis | `burston`, `burston_prime` (+ `_incarnon`) |
| Furis Genesis | `furis`, `mk1_furis` (+ `_incarnon`) |
| Torid Genesis | `torid` (+ `_incarnon`) |
| Dual Toxocyst Genesis | `dual_toxocyst` (+ `_incarnon`) |
| Laetum (natural) | `laetum` (+ `_incarnon`) |
| **Phenmor (natural)** | `phenmor` (+ `_incarnon`) — 2026-08-08 |
| **Braton Genesis** | `braton`, `mk1_braton`, `braton_vandal`, `braton_prime` (+ `_incarnon`) — 2026-08-08 |
| **Latron Genesis** | `latron`, `latron_wraith`, `latron_prime` (+ `_incarnon`) — 2026-08-08 |
| **Boltor Genesis** | `boltor`, `telos_boltor`, `boltor_prime` (+ `_incarnon`) — 2026-08-08 |
| **Sybaris Genesis** | `sybaris`, `dex_sybaris`, `sybaris_prime` — BULK, 2026-08-08 |
| **Dera Genesis** | `dera`, `dera_vandal` — BULK, 2026-08-08 |
| **Lato Genesis** | `lato`, `lato_vandal`, `lato_prime` — BULK, 2026-08-08 |
| **Lex Genesis** | `lex`, `lex_prime` — BULK, 2026-08-08 |

Remaining: **19 adapters (36 weapons) + 2 naturals**.

## BULK vs HAND — what "rough" means, precisely

Owner, 2026-08-08: "我想先一口气把那个列表里的武器都加进去，灵化部分可以粗略，然后
我一把枪一把枪地核实." So from the Sybaris onward the intake runs a pipeline, and
the two halves of a weapon are held to different standards ON PURPOSE:

| | source | standard |
| --- | --- | --- |
| **stats** (both forms, damage, crit, status, rate, magazine, reload, gauge) | WFCD `attacks` = DE's own export | EXACT. On all eleven hand-checked guns it agreed with the wiki infobox field for field. |
| **evolutions** | the wiki's evolution table, transcribed | ROUGH. A clause the intake's rule engine recognises becomes a real effect; one it does not becomes a kind NAMED `unmodelled_<its own words>`. |

**"Rough" never means silent.** An `unmodelled_*` kind loads as
`EvoEffect::Inert`, and since 2026-08-08 BOTH the builder tile and the optimizer
row print it as "not modelled yet" / "partly modelled" with the clause in the
tooltip. A perk that does nothing says so where you pick it.

**What the rule engine reads today**: base damage, base crit chance, base crit
multiplier, base status chance, base magazine, ammo capacity, fire rate, reload
speed, projectile speed, accuracy, recoil, headshot damage, zoom, punch-through,
Incarnon charge rate, and the non-crit damage chance. Everything else — every
CONDITIONAL clause especially — is inert by construction: a conditional is never
mined for its numbers, because reading "On Kill: +30 damage" as an unconditional
+30 is the one failure mode worse than not parsing at all.

**Still pending on a bulk weapon, and worth knowing before trusting one:**

- **zh perk names and card text.** The weapon names come from DE's export; the
  EVOLUTION strings need the CN wiki one adapter at a time, and the bulk pass
  does not do it. A Chinese session shows English perk names on a bulk weapon
  until its family is transcribed.
- **CO catalog rows.** The intake assumes no row (the ordinary class). Two
  families in the roster have one — Burston and Braton, both for their Incarnon
  RADIAL — so a bulk weapon with an explosion should be checked against the
  catalog during the per-gun pass.
- **anything the wiki says in prose rather than in the table**: innate multishot
  on a form, guaranteed procs, ricochet counts, per-form status splits.

**Eleven weapons in one night**, which is the number to plan the rest against:
one natural and three adapters, 22 weapon entries and 112 evolution files, and
NO ENGINE WORK — every perk either mapped onto a kind the engine already had or
loaded as a named inert one. The cost was never the weapon file; it was reading
two wikis carefully enough to catch the three places they disagree.

### What the Phenmor cost, against what this file predicted

Predicted "4 new perks, no new mechanic". Actual: **no engine work at all**, and
four perks that load as INERT rather than as new kinds — two of them already
inert on the Furis (an instant reload the sim cannot end; Ready Retaliation's
reload-speed kind, which this loader has no arm for) and two genuinely new
shapes, both of which would be worth real damage here:

- **Spiteful Defilement** — a crit multiplier while the TARGET carries fewer
  than three statuses. The counter exists (Condition Overload's bucket IS the
  status-type count); the conditional bracket does not. It is the
  anti-CO perk, so no build wants both.
- **Lingering Judgement** — a buff armed by a headshot STREAK (2 in 2 s, held
  8 s). On the official ruler, which puts every shot into a head, it would arm
  on the second shot and never lapse: a flat +50% headshot damage for the whole
  engagement, and the largest unmodelled thing in the set so far.

Everything else mapped onto kinds already in the engine. Two guard tests caught
the weapon on the way in and are the reason nothing shipped silently: the
Cannonade roster (a semi-auto base form whose Incarnon form is Auto — the Dual
Toxocyst's shape, on the rifle side for the first time) and the inert-effects
pin.

### What the Braton family cost — 8 weapons in one pass

Predicted "4 new perks". Actual: **three** new inert kinds and no engine work,
which is what a four-variant adapter is supposed to look like — one wiki table,
36 evolution files, and the numbers are the only thing that varies.

- **Daring Reverie** — the larger half needs a CHANNELED ABILITY, a Warframe
  state this arena has no concept of. Worth naming because on three of the four
  variants the conditional half is the BIGGER number.
- **Munitions Grit** — the +20% multishot has no flat-multishot arm for an
  evolution. Its surcharge IS modelled, and the pair is circular: the surcharge
  only pays on projectiles multishot generated.
- **Gunsmoke Pick Up** — out of reach twice over: no ammo-restore kind, and a
  PUNCH THROUGH trigger needs a second body behind the first.

The Incarnon form is the roster's **second explosion to take Condition
Overload**, after the Burston — its own catalog row, with "Radial hit only
receives CO bonus on target directly hit by bullet" and "AoE does not scale off
multishot", both declared on the radial.

**TWO SOURCE DISAGREEMENTS**, out of 36 values, both recorded in the yaml
rather than reconciled: the EN and CN wikis SWAP Survivor's Edge's crit chance
between the Braton and the MK1-Braton (10/12 against 12/10), and they read
Mercenary Chamber's Vandal capacity as 750 against 755. The EN numbers ship,
because the EN page is where the effect text was transcribed from. A swap is
the one kind of disagreement that looks like agreement if you only check the
multiset, which is why it is written down.

### What the Latron family cost

Four inert kinds, no engine work, and the two that matter are NEAR-MISSES
rather than absences — which is the useful kind of gap to find, because each
one is a trigger arm away from working:

- **Riddled Target** wants the live stacking-multishot buff the engine already
  has. That one's trigger is an ELECTRICITY status (Stormburst's); this one is
  PUNCTURE. Large here: the base form is 60-80% Puncture, so four stacks of
  +25% would ride on the weapon's own main damage type and never lapse.
- **Flensing Spikes** strips armour per PUNCTURE status. Armour stripping exists
  for Corrosive and Heat — the two the game strips with — and a third rule has
  no arm. Against the official ruler's Thrax at level 9999 it would be worth a
  great deal.

**A THIRD THING THE ARENA CANNOT HOLD**, and it is the weapon rather than a
perk: the Incarnon form is a RICOCHET projectile that explodes "up to 6 times".
The first collision and its explosion land on the target in front of you and the
other five bounces have nowhere to go — the same treatment punch-through and
beam chaining already get here. So a Latron's number is its SINGLE-TARGET
number, and the weapon is worth more against a crowd, which is true of it in
game too.

**Zero source disagreements** across all 27 values, against the Braton's two.
That is the argument for reading both wikis every time rather than only when
something looks odd.

### What the Boltor family cost

Three inert kinds, no engine work, and one of them is the one that matters most
to everything still on this list:

- **Rapid Reinforcement** is the MOST COMMON PERK IN THE SET — 14 guns by the
  count above, more than any other name — and this is its first appearance in
  the repo. It is a reload-speed bonus, which `evolutions_data` has no arm for
  even though the MODS loader does. Implementing that one arm removes a slot
  from half the remaining program, which is exactly what the ordering section
  predicted.
- **Crimson Overture** would be the first on-kill stacking buff to move the
  BASE damage; the engine's existing ones (Galvanized Chamber, Bladed Rounds)
  all multiply it.
- **Hunter's Mantra**'s second half needs a channeled ability, like the
  Braton's Daring Reverie — and both its payloads are spatial anyway.

**THE INCARNON IS A PSEUDO-SHOTGUN**: 3 base multishot, a per-projectile damage
a fraction of the base form's, and a 10/30/60 Impact/Puncture/Slash split on
every variant. It is the first RIFLE here that fires like one, and the shape was
already pinned by the Boar. The per-projectile status chance reads lower than
the base form's for the same reason it does on a shotgun — three rolls instead
of one.

**One source disagreement**, on Crimson Overture's stack cap: EN reads 4x for
the Boltor and 3x for the other two, CN reads 3x for all three. The EN number
ships and the disagreement is recorded at the site. The EN table also renders
Hunter's Mantra's base-damage number onto the Incarnon Form row above it; the CN
page and its own summary table both give it to Hunter's Mantra alone, and that
is the reading followed.

**Icons are not free.** Five of the thirteen had no verified wiki `File:` name
and were dropped rather than guessed — the site build FAILS on an icon that
does not resolve, and a guessed name caches as a 31 KB HTML error page that
passes a naive existence check. The Laetum's thirteen ship without icons for
the same reason.

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
| Boltor\* | Boltor (MR2)\*, Telos (MR12)\*, Prime (MR13)\* | Auto | auto projectile | 8 → 160 | 3 | **done 2026-08-08** |
| Sybaris | Sybaris (MR5), Dex (MR7), Prime (MR12) | Burst | hit-scan | 8 → 200 | 3 | — |
| Braton\* | Braton (MR0)\*, Mk1 (MR0)\*, Prime (MR8)\*, Vandal (MR4)\* | Auto | auto hit-scan + AoE | 10 → 200 | 4 | **done 2026-08-08** |
| Dera | Dera (MR4), Vandal (MR7) | Auto | hit-scan | 2 → 50 | 4 | — |
| Soma | Soma (MR6), Prime (MR7) | Auto-Spool | hit-scan | 10 → 200 | 4 | **spool** |
| Dread | Dread (MR5) | Charge (bow) | charged projectile, 0.6 s | 5 → 20 | 5 | — |
| Latron\* | Latron (MR0)\*, Prime (MR10)\*, Wraith (MR7)\* | Semi-Auto | semi projectile + AoE | 5 → 40 | 5 | **done 2026-08-08** |
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
| Phenmor\* | primary | semi projectile → auto projectile | 4 | **done 2026-08-08** — the prediction held: no engine work, and the four "new" perks load inert rather than needing kinds |
| Onos | secondary | auto projectile → held projectile **and** charged hit-scan + radial | 6 | TWO Incarnon attacks in one form — the only gun here that does that |
| Felarx | primary | auto projectile → semi projectile | 11 | almost nothing shared; also `ReloadStyle = ByRound` |

## Order, and why

1. ~~**Phenmor**~~ — **done 2026-08-08**. 4 new perks, no new mechanic, and 9 of
   its 13 perk names already carried; the highest ratio in the set, and it cost
   no engine work at all.
2. **The 19 shared perk kinds**, driven by whichever adapter needs them first.
   Rapid Reinforcement alone appears on 14 guns; doing it once removes a slot
   from half the remaining program.
3. **The zero-mechanic adapters, widest family first**: ~~Braton (4)~~,
   ~~Latron (3)~~, ~~Boltor (3)~~ **all done 2026-08-08**, then Sybaris (3), Lato (3),
   Ballistica (3), then the pairs (Dera, Vasto, Lex, Bronco, Kunai, Sicarus,
   Gammacor, Angstrum) and the singles (Miter, Despair, Cestra, Atomos).
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
