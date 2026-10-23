# Incarnon guns — the whole roster, and what covers what

The Incarnon roster: what covers what, and how each part is read. Read the
Notes, on every weapon, before calling it done — an Incarnon form's cadence can
live there and nowhere else (docs/WEAPON_INTAKE.md §"READ THE PAGE").

Everything below is read from the wiki: `Incarnon` for the adapter→weapon
mapping, each `<X> Incarnon Genesis` page for the gauge and the evolutions, and
`Module:Weapons/data/<slot>` for the attack table (`private/scripts/wiki_weapons.py`).
Perk "already carried" means a `data/evolutions/*.yaml` in this repo already
carries that name.

## Every tier is installed

A finished Genesis cannot be emptied, so no build has an empty evolution tier.
A tier a build does not name holds its FIRST option, and first is the game's
order: `position_in_tier` in each `data/evolutions/*.yaml`, transcribed from
the row order of the wiki's Evolutions table (1 = the default). The engine
sorts the pool by it and refuses a tier whose positions do not run 1..n.
`evolutions::complete` is the one fill rule; the server applies it to every
request (`chosen_evolutions`, the optimizer's variant table) and the page
mirrors it (`defaultEvolutions`). Only the Shapley analysis sends a list as
given. Old share links and presets carrying an empty tier read it as that
tier's default; the wire keeps its `--`.

A tier with ONE option (tier 1, the form) is therefore no choice, and nothing
offers it as one (`evoFixed`): the builder draws it with nothing to pick, the
optimizer's limits leave it out, and the Shapley analysis installs it in every
subset rather than making it a part. The server needs no rule: a one-option
tier is a factor of 1 in the search's product and has no other option to scan.

A mod that needs the same trigger on every firing mode (the Cannonades) is
therefore never equippable on an Incarnon weapon: tier 1 is always there.

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
- **Melee is docs/MELEE.md §7's**: every melee Genesis and natural Incarnon
  (Innodem, Praedos, Ruvox, Thalys) is entered to the same ROUGH standard, its
  form a buff on the swings rather than a second entry.

## BULK vs HAND — what "rough" means, precisely

The intake runs a pipeline, and the two halves of a weapon are held to
different standards ON PURPOSE:

| | source | standard |
| --- | --- | --- |
| **stats** (both forms, damage, crit, status, rate, magazine, reload, gauge) | the wiki module's `Attacks[]` | EXACT — one row per attack, which is what an Incarnon weapon's two forms need. |
| **evolutions** | the wiki's evolution table, transcribed | ROUGH. A clause the intake's rule engine recognises becomes a real effect; one it does not becomes a kind NAMED `unmodelled_<its own words>`. |

**"Rough" never means silent.** An `unmodelled_*` kind loads as
`EvoEffect::Inert`, and BOTH the builder tile and the optimizer
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
  EVOLUTION strings are in no export, so a Chinese session shows English perk
  names on a bulk weapon until its family is read off the Chinese client
  (DATA_SOURCES §"Chinese text").
- **CO catalog rows** — read and applied for every weapon in the program:

  | weapon | row | what it means here |
  | --- | --- | --- |
  | **Latron / Latron Prime** | Incarnon Mode, **Multiplying** | A free-standing final multiplier is a different number on any build carrying Serration. |
  | Latron **Wraith** | *no row* | **Multiplying, carried from the family.** The row names the Latron and its Prime; the form is the ADAPTER's and the projectile is the same, so the class reaches this variant too. CATALOGS.md §1. |
  | **Kunai / MK1-Kunai** | Incarnon Mode, **Multiplying** | applied at intake. |
  | Braton family | Incarnon Form Radial, Adding, 95% | already applied. |
  | Burston family | Incarnon Form Radial, Adding, **24%** | both halves modelled: the class by `takes_condition_overload`, the 24% by the radial's OWN derived `co_base_fraction` (13 + 42 = 55, 13/55). |
  | Zylok family | Incarnon Form Radial, Adding, 90% | derived per variant. The row mixes them — 700 is the Prime's radial, the +76 is the base Zylok's perk — so its 90% is the one catalog figure not reproduced, deliberately. CATALOGS.md. |
  | Torid, Felarx, Angstrum, Ballistica Prime, Dread, Paris, Miter | Multiplying on the named attack | applied as each lands. **Felarx is Multiplying on BOTH modes** — asked directly, so for that weapon the row is not about one attack. The Angstrum's other form is left ordinary with the question written at the site: one weapon is not a rule. |
  | Cestra, Despair, Atomos, Bronco Prime, Vasto Prime, Lato Vandal, Lex Prime, Dual Toxocyst, Furis | Adding + "CO-bonus does not use base damage increase Evolution" | `co_base_excludes_this_evolution: true` on the perk each row NAMES. Including an evolution's flat damage is the engine's default, so a row here that is not flagged computes CO on a base the game does not use. Every row and its perk: CATALOGS.md §"CO-bonus does not use base damage increase Evolution". |
  | **Kunai** | "CO-bonus **DOES** use base damage increase Evolution" | the one weapon where the engine's blanket exclusion is WRONG. No way to express it today; noted at the site. |
  | Stug | Blob Impact, **0%, "Does not apply"** | `co_behavior: inert` when the Stug lands. |
  | Ballistica / Ballistica Prime / Dread / Paris / Miter | Charged Attack, Adding, **25–50%** | these DO fit `co_base_fraction`, which is per weapon ENTRY and a charged shot is its own entry (the Cernos Prime already does exactly this at 0.5). |
- **anything the wiki says in prose rather than in the table**: innate multishot
  on a form, guaranteed procs, ricochet counts, per-form status splits.

## The perks

Across all 34 guns the wiki lists **318 evolution slots / 119 distinct perks**.
A name already carried means the engine effect exists and the new weapon only
needs its own YAML with its own numbers.

Every Genesis has the same 9-slot shape: EVO1 Incarnon Form, EVO2 ×2, EVO3 ×3,
EVO4 ×3. The naturals have 13.

A Prime shares its family's perk list and differs only in numbers. Perks that
appear on two or more guns:

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

## The gauge is two numbers, and one of them is datamined

`incarnon.gauge` needs `charges_to_fill` and `rounds_per_charge`. The module's
**`IncarnonChargeGain`** is the rounds granted per charging hit, and the Genesis
page states how many hits fill the gauge; `max_rounds` is their product on every
weapon where the wiki states both (the one exception is Vectis: 5 × 10 = 50, page
says 45 — measure it before trusting either).

**Almost every adapter charges on WEAKPOINT hits. Three charge on DIRECT hits:
Torid, Angstrum, Stug** (wiki, Incarnon — verbatim in `torid_incarnon.yaml`).

### A CHARGE POOL IS NOT A MAGAZINE, and nothing in the game treats it as one

Roster-wide invariant, stated measured: **no mechanism anywhere
restores charges in an Incarnon form, and none spends extra ones.** The pool is
filled by the gauge — weakpoint or direct hits, converted at
`rounds_per_charge` — and emptied by firing at the form's own `ammo_cost`. That
is the complete list of things that touch it.

It is the reason behind a whole family of "does not affect Incarnon Form"
sentences that would otherwise each look like their own special case:

| the effect | why the gauge is exempt |
| --- | --- |
| Extended Volley, Retribution's Vessel (`flat_base_magazine`) | it resizes a MAGAZINE |
| Final Fusillade (`multishot_on_last_round`) | a charge pool has no "last round" |
| Ammo Efficiency, ammo mods | the gauge is outside the ammo economy |
| Executioner's Fortune | it refills a MAGAZINE, not max charges |

Each of those is a separate gate in the code, which is four chances to forget the
fifth — so the invariant is asserted as a PROPERTY instead:
`no_evolution_resizes_an_incarnon_charge_pool` installs every evolution a
charge-backed form can carry, all at once, and requires the pool to come out the
size the data declares. It was verified to fail with any one gate removed.

## Primary adapters

`*` = in the repo. "new perks" counts names this repo does not carry yet, out of 9.

| adapter | variants | base trigger | Incarnon form | rounds/hit → max | new perks | needs |
| --- | --- | --- | --- | --- | --- | --- |
| Boar | Boar (MR2)\*, Boar Prime (MR11)\* | Auto | held hit-scan | 3 → 150 | 0 | **done** |
| Burston | Burston (MR0)\*, Burston Prime (MR12)\* | Burst/Auto | auto hit-scan + radial | 30 → 600 | 0 | **done** |
| Torid | Torid (MR4)\* | Semi-Auto | held beam | 34 → 170 | 0 | **done** |
| Miter | Miter (MR6) | Charge | auto projectile + radial | 5 → 20 | 2 | — |
| Boltor\* | Boltor (MR2)\*, Telos (MR12)\*, Prime (MR13)\* | Auto | auto projectile | 8 → 160 | 3 | **done** |
| Sybaris | Sybaris (MR5), Dex (MR7), Prime (MR12) | Burst | hit-scan | 8 → 200 | 3 | — |
| Braton\* | Braton (MR0)\*, Mk1 (MR0)\*, Prime (MR8)\*, Vandal (MR4)\* | Auto | auto hit-scan + AoE | 10 → 200 | 4 | **done** |
| Dera | Dera (MR4), Vandal (MR7) | Auto | hit-scan | 2 → 50 | 4 | — |
| Soma | Soma (MR6), Prime (MR7) | Auto-Spool | hit-scan | 10 → 200 | 4 | **spool** |
| Dread | Dread (MR5) | Charge (bow) | charged projectile, 0.6 s | 5 → 20 | 5 | — |
| Latron\* | Latron (MR0)\*, Prime (MR10)\*, Wraith (MR7)\* | Semi-Auto | semi projectile + AoE | 5 → 40 | 5 | **done** |
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
| Phenmor\* | primary | semi projectile → auto projectile | 4 | **done** — its Incarnon fire rate spools down, a mechanic stated only in the Notes (MECHANICS §9) |
| Onos | secondary | auto projectile → held projectile **and** charged hit-scan + radial | 6 | TWO Incarnon attacks in one form — the only gun here that does that |
| Felarx | primary | auto projectile → semi projectile | 11 | almost nothing shared; also `ReloadStyle = ByRound` |

## Reading a perk

A clause nothing in a one-target fight can pay is `out_of_scope`, with one of
UNMODELLED.md's classes as the reason, rather than `unmodelled_*`.

**THE BRACKET IS THE WORK, not the number.** One perk name can mean different
arithmetic on different families:
Blazing Barrel is "+0.05 BASE Multishot" on the Strun and "+5% Multishot" on the
Sybaris, both written `0.05`, and they differ by 2.2x the moment a multishot mod
is equipped. Striking Succession's "+15 Base Damage" is worth x1.58 bare and
x1.58 with Serration, where the same number in the bucket is x1.22. Feigned
Retreat's own flat damage is excluded from its own percentage. None of this is
visible on a bare weapon, which is why every one of them is pinned by a test
that fails when the bracket moves.

**Read the RAW wikitext, not the page.** Three separate readings of a rendered
table said a perk was variant-exclusive; the markup said `colspan="3" | -`,
which is that table's way of writing "no per-variant difference", and the perk
was on all three every time. Two perks carry an activation chance that appears
only in a notes cell — Headcracker's 50% is the difference between the card and
a perk twice as strong. Hoplite Virtue's "On Shield Break" is on six guns and
only ONE of the six pages says whose shield ("This is on personal shield break,
not breaking enemy shields") — the difference between a trigger that fires every
fight and one that can never fire here.

## What has to be measured

Only the mechanics, not the weapons. A weapon whose every part is already
pinned needs no new measurement (the rule is in WEAPON_INTAKE.md, and it is why
Boar Prime cost nothing to verify). New sessions are needed for:

- **spool** — rounds-to-full-rate against sustained DPS, one Gorgon;
- **by-round reload** — Strun, interrupted and uninterrupted;
- **duplex cadence** — Zylok, shots per second at the listed fire rate;
- **Vectis's gauge** — 5 hits × 10 rounds should be 50, the page says 45;
- **Stug** — everything about it.

## Perks this loader does not model, and what each needs

`data::evolutions`'s `every_inert_perk_is_accounted_for` pins the list; this is
what each entry is waiting on. An unknown effect kind is the only spelling that
means "nothing models this yet" and stays true — the kinds that would fit all
pay out UNCONDITIONALLY, so `flat_base_damage` would load Haven Foray's
overshield clause as a silent +30 on every build, `flat_base_crit_multiplier`
would grant Prelude of Might's +3x to everyone.

`unmodeled_effects` is derived from the same variants, so a perk's tile and this
list cannot disagree.

### Ammo efficiency, and it is CONDITIONAL

Efficiency is real DPS the moment a reserve runs dry, so it is not an indirect
stat. One member is gated on a movement state and one on a headshot window, and
applying either unconditionally overstates the build. Both also land on the
Laetum's Incarnon magazine, which is charge-backed and takes no efficiency at
all.

Every perk's efficiency joins the 75% cap (`docs/MECHANICS.md` §"Ammo
Efficiency"): Enough for Everyone is +75%, Crimson Overture +25% a stack up to
3x. The Vasto's Reaper's Plenty grants infinite ammo, which the cap does not
reach. All three are `out_of_scope` here, the arena running on infinite ammo.

### The Furis Genesis

Five of its eight perks, each written under a kind this loader does not know.
What the remaining one needs: **Haven Foray** wants a Tenno with overshields,
which `TennoCondition` has no room for.

### The Phenmor

Four perks that were inert. Two are the family's — an instant reload the sim
cannot end, and Ready Retaliation's reload-speed kind, both implemented since.
The other two would be real damage here rather than handling stats:

- **Spiteful Defilement** is the ANTI-Condition-Overload perk: a crit multiplier
  that pays while the target carries fewer than three statuses and stops the
  moment CO starts paying. The counter it needs exists (CO's bucket IS the
  status-type count); a crit bracket that reads it does not.
- **Lingering Judgement** is armed by a headshot STREAK — two inside two
  seconds, held for eight. The engine has per-headshot triggers for fire rate
  and reload and a flat headshot-damage bonus, but nothing that counts N hits
  inside a window. On the official ruler, which puts every shot into a head, it
  would arm on the second shot and never lapse: a flat +50% headshot damage for
  the whole engagement, and the largest thing on this list.

### The Braton family — one adapter, four weapons

Every gap here is four rows of the same fact.

- **Daring Reverie**'s larger half needs a CHANNELED ABILITY, a Warframe state
  this arena has no concept of. Worth naming because on three of the four
  variants the conditional half is the BIGGER number, so a Braton's figure is
  not its ceiling.
- **Munitions Grit**'s +20% multishot has no flat-multishot arm. Its surcharge
  (`multishot_consumes_ammo`) IS modelled, and the pair is circular: the
  surcharge only pays on projectiles multishot generated.
- **Gunsmoke Pick Up** is out of reach twice — no ammo-restore kind, and a
  PUNCH THROUGH trigger needs a second body behind the first.

### The Latron family — three weapons, four kinds

Two are near-misses rather than absences.

- **Riddled Target** wants the live stacking-multishot buff the engine already
  has; that one's trigger is an ELECTRICITY status and this one is PUNCTURE.
  The machinery exists and the trigger arm does not — a large gap here, since
  the base form is 60–80% Puncture, so four stacks of +25% would be held up
  indefinitely off the weapon's own main damage type.
- **Flensing Spikes** strips armour per PUNCTURE status. Armour stripping exists
  for Corrosive and Heat, the two the game strips with; a third rule has no arm.
  Against the official ruler's Thrax at level 9999 it would be worth a great
  deal.
- **Marksman's Focus** is zoom, which is not merely cosmetic in general — a zoom
  level carries its own damage or crit bonus on many weapons — but carries none
  on this one. Marksman's Hand is recoil and IS loaded, into the indirect
  bucket.

### The Boltor family — three weapons, three kinds

- **Crimson Overture** is an on-kill stacking buff on the BASE damage, and would
  be the first: the engine's on-kill stacks (Galvanized Chamber's multishot,
  Bladed Rounds' crit damage) all multiply the base rather than move it.
- **Hunter's Mantra**'s second half needs a CHANNELED ABILITY, and both of its
  payloads are spatial anyway — punch-through needs a second body and accuracy
  needs a miss to prevent.

Rapid Reinforcement is NOT on the list: it is implemented
(`EvoEffect::ReloadSpeedBonus`, into the additive bucket the mods feed), and so
is the conditional member of that family, **Ready Retaliation** — armed when a
reload from empty completes (UNMODELLED.md §"The MANUAL reload").
