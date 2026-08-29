# The per-weapon CATALOGS — tables the wiki publishes and the engine must carry entry by entry

Some mechanics are not a formula plus a weapon stat. They are a formula plus a
**published table with one row per weapon**, where the row can say a thing the
weapon's own numbers never would — that this weapon's bonus multiplies where
everyone else's adds, that this attack part is exempt, that this one does not
work at all.

Those rows are DATA, and the rule for them is the one the roster already
follows for Condition Overload: **the catalog is
authoritative, and absence from it means ordinary — not unknown.** A row is
never generalised to a weapon, a form, or a class of behaviour; it is
transcribed for the entry it names.

This file is where the rows live, so they stop being scattered across yaml
comments and can be diffed against the wiki in one pass.

---

## 1. Condition Overload — `co_behavior`, `co_base_fraction`

Wiki: [Condition Overload (Mechanic)](https://wiki.warframe.com/w/Condition_Overload_(Mechanic)).
Columns, verbatim:

> Weapon | Attack Name | Projectile Type | Attack Unmodded Damage | Actual CO Damage Bonus at +100% | CO Damage Bonus Relative To Base Damage | Math/Behavior Type | Notes

`Math/Behavior Type` maps onto `CoBehavior`:

| catalog | ours | what it does |
| --- | --- | --- |
| Multiplying | `independent` | free-standing `× (1 + co × types)` |
| Adding | `additive_with_base_damage` | joins the base-damage bucket, diluted by Serration |
| (no row) | the ordinary case, i.e. `additive_with_base_damage` | |

### THE RULE, stated once

> A shared Incarnon Genesis does not make one weapon: a variant is still its
> own entry. Anything not on the CO table is ordinary — direct hits only,
> 100%, added. If the base variant is on the table and the Prime is not, the
> base gets the anomaly and the Prime gets the ordinary rule. Never
> generalise a row to a family.

Four things, and the last two are the ones that keep being got wrong:

1. **ORDINARY has a definition**, not just a name: direct hits only, 100% of the
   attack's base, added into the base-damage bucket. `co_behavior:
   additive_with_base_damage`, no `co_base_fraction`, and a radial that takes
   none at all.
2. **Absence means ordinary**, never unknown. The catalog lists the anomalies.
3. **THE PAGE'S PROSE DOES NOT OVERRIDE THE TABLE.** Its Math section lists
   "Base Damage increases from Incarnon Genesis Evolutions" among the things
   Adding CO ignores, which read as a law would move 107 (entry, perk) pairs
   here. It is not a law — see the section below for why
   the page argues that side itself. Rule 2 already covered this; it is spelled
   out because the prose is what tempts you to break it.
4. **A SHARED GENESIS DOES NOT MAKE ONE WEAPON.** An Incarnon form is still that
   weapon's form, so a row is transcribed for the entries it NAMES and no
   others. If the base variant is on the table and the Prime is not, the base
   gets the anomaly and the Prime gets ordinary.

Enforced by `the_only_condition_overload_anomalies_are_the_ones_the_catalog_names`,
which carries the anomalies as a LIST: a weapon whose FAMILY has a row fails
until the row is checked for that weapon's own name.

### Rows carried

| weapon | attack | unmodded | relative | type | our entry |
| --- | --- | --- | --- | --- | --- |
| Torid | Main-fire (Projectile) | 100 | 100% | Multiplying | `torid.yaml` → `independent` |
| Torid | Toxin AoE Cloud (AoE) | 40 | 100% | Multiplying | the cloud's `takes_condition_overload: true` |
| Shedu | Normal Attack (Projectile) | 71 | 100% | Multiplying | `shedu.yaml` → `independent` |

### THE TENET AND CODA BATCH — re-read 2026-08-20

Twenty weapons arrived at once and the two tables name **seven** of their
attacks. Five are anomalies; two are the catalog saying "checked, ordinary",
which is a different and useful statement.

| weapon | attack | unmodded | bonus | relative | type | our entry |
| --- | --- | --- | --- | --- | --- | --- |
| Tenet Arca Plasmor | Normal Attack (Projectile) | 760 | 760 | 100% | Multiplying | `tenet_arca_plasmor` → `independent` |
| Coda Bassocyst | Normal Attack (Projectile) | 808 | 808 | 100% | Multiplying | `coda_bassocyst` → `independent` |
| Coda Bassocyst | Alt-fire (Homing Projectile) | 303 | **0** | **0%** | N/A — *"Does not apply"* | no entry: the alt fire is unmodelled |
| Coda Hema | Normal Attack (Projectile) | 52 | 52 | 100% | Multiplying | `coda_hema` → `independent` |
| Tenet Plinx | Alt Fire Impact (Projectile) | **1000** | 1000 | 100% | Multiplying — *"Scales properly with magazine size"* | `tenet_plinx_charged` → `independent` |
| Tenet Spirex | Slug Impact (Projectile) | 120 | 120 | 100% | Multiplying | `tenet_spirex` → `independent` |
| Tenet Ferrox | Hitscan AoE Direct (**AoE**) | 60 | **200** | **333%** | Adding — *"Radial hit receives CO bonus on direct hit only"* | **not expressible** — see below |
| Tenet Detron | Normal Attack (Projectile) | 26 | 26 | 100% | **Adding** — *"CO-bonus ignores Damage Falloff"* | ordinary; transcribed anyway |
| Tenet Detron | Burst Shot (Projectile) | 26 | 26 | 100% | **Adding** — *"CO-bonus ignores Damage Falloff"* | ordinary; transcribed anyway |

Four things this batch taught the file.

1. **THE PLINX ROW IS A MEASUREMENT OF SOMETHING ELSE.** Its unmodded damage
   cell reads 1000 where the infobox lists 100, which is the catalog
   independently confirming the weapon's own Notes: *"the attack deals 100
   Impact on contact and 100 Radiation on explosion, multiplied by the magazine
   capacity"*. A CO table cross-checking a damage mechanic is not what it is
   for, and it is the second time the damage column has paid for itself.
2. **A WEAPON CAN HAVE TWO ROWS WITH OPPOSITE ANSWERS.** The Coda Bassocyst's
   primary fire is Multiplying and its alt fire is `Does not apply` — the two
   ends of the vocabulary, on one weapon, three columns apart.
3. **THE FERROX ROW IS THE FIRST ONE THIS ENGINE CANNOT CARRY.** It says two
   things at once: that a RADIAL takes Condition Overload at all, which no
   ordinary radial does, and that its term reads the DIRECT hit's base (200)
   rather than its own (60). `co_base_fraction` is one number per ENTRY, and
   that entry's direct hit is ordinary — so a 3.333 would be wrong for the half
   of the attack the row does not name. The radial takes none, which understates
   a status-stacking build, and the weapon's own `unmodeled:` says so on the
   card.
4. **A ROW THAT SAYS "ORDINARY" IS STILL WORTH TRANSCRIBING.** Both Tenet Detron
   rows are `Adding` at 100%, which is the default — so they add nothing to the
   engine and everything to the reader: they are the difference between *checked
   and ordinary* and *never looked at*. Their Notes cell is a real anomaly with
   no word in `CoBehavior` (the CO term ignoring damage falloff), and it is
   admitted on the weapon rather than silently dropped.

**Rows for roster weapons that are NOT the ordinary case.** Transcribed
2026-08-12 from `?action=raw`, which is also when this table stopped being three
rows — the file exists so the catalog can be diffed against the wiki in one
pass, and most of the roster's rows had been living in INCARNON.md's prose and
in yaml comments instead. That is how five of them came to be wrong.

| weapon | attack | relative | type | our entry |
| --- | --- | --- | --- | --- |
| Akarius Prime | Rocket Impact | 100% | Multiplying | `akarius_prime` → `independent` — the base Akarius has NO row |
| Angstrum / Prisma Angstrum | **Incarnon Mode** | 100% | Multiplying | the `_incarnon` entries → `independent` |
| Ballistica | Charged Shot | **25%** | Adding | `co_base_fraction: 0.25` |
| Ballistica Prime | Charged Shot | **50%** | Adding | `co_base_fraction: 0.50` |
| Ballistica Prime | Incarnon Mode | 100% | Multiplying | `independent` |
| Rakta Ballistica | Charged Shot | **25%** | Adding | `co_base_fraction: 0.25` |
| Miter | Charged Attack | **40%** | Adding | `co_base_fraction: 0.40` |
| Miter | Incarnon Mode | 100% | Multiplying | `independent` |
| Dread | Charged Attack | 50% | Adding | `co_base_fraction: 0.5` |
| Dread | Incarnon Mode | 100% | Multiplying | `independent` |
| Paris / Paris Prime | Charged Attack | 50% | Adding | `co_base_fraction: 0.5` |
| Mk1-Paris | Charged Attack | 50% | Adding | `co_base_fraction: 0.5` |
| Paris / Paris Prime | **Incarnon Mode** | 100% | Multiplying | `independent` — Mk1-Paris is NOT on this row |
| Latron / Latron Prime | Incarnon Mode | 100% | Multiplying | `independent` — Latron Wraith is NOT on this row |
| Felarx | Normal + Incarnon Mode | 100% | Multiplying | both `independent` |
| Kunai | Normal Attack | 100% | Adding | ordinary |
| Kunai / Mk1-Kunai | Incarnon Mode | 100% | Multiplying | `independent` — see the two notes below |
| Larkspur Prime | **Alt-fire** | 100% | Multiplying | `larkspur_prime_charged` → `independent` — the normal fire has NO row |
| Scourge | **Throw** (unmodded 150) | 100% | Multiplying | `scourge_thrown` → `independent` |
| Scourge Prime | **Throw** (unmodded 200) | 100% | Multiplying | `scourge_prime_thrown` → `independent` |
| Cernos Prime | Charged Attack | 50% | Adding | `co_base_fraction: 0.5` |
| Stug | **Blob Impact** | 0% | Does not apply | `co_behavior: inert` |
| Zylok / Zylok Prime | Incarnon Form Radial | 90% | Adding | derived per variant; the row MIXES them — see below |
| Braton family | Incarnon Form Radial | 95% | Adding | derived: 70 + 4 = 74, 70/74 |
| Burston / Burston Prime | Incarnon Form Radial | 24% | Adding | derived: 13 + 42 = 55, 13/55 |


### THE 2026-08-20 SWEEP — forty-four entries the catalog named and the roster contradicted

**A method error, and it is the useful part of this entry.** Every weapon yaml
written this month opened with *"NO row in the wiki's CO catalog (re-read
2026-08-20)"*. That check was run against **THIS FILE** — our own transcription,
which by construction carries only "rows the roster already has". Asking it
whether a NEW weapon has a row can only ever answer no. The check has to read
the WIKI PAGE, and when it finally did it found forty-four disagreements.

**Not all of them were new.** The Lanka has read Adding at 100% since it was
written and its row says 38%; both Laser Rifles, the whole Cernos family and the
Catabolyst are the same story. Condition Overload is on most builds, so each of
these was a wrong damage number rather than a wrong comment.

**And it took TWO passes, for a reason worth writing down.** The first
reconciliation matched a row to a form through a short list of attack NAMES —
"Normal Attack", "Alt-fire", "Charged Attack". The catalog names an attack the
way that WEAPON's page does, so "Projectile Impact", "Direct Hit", "Lock-On
Mode", "Slug Impact", "Burst Mode" and "Reload From Empty Impact" matched
nothing and were skipped in silence. A narrow vocabulary does not fail, it
under-reports.

| our entry | behaviour | co_base_fraction |
| --- | --- | --- |
| `acceltra` | additive_with_base_damage | 74.3% |
| `aegrit` | independent | 100% |
| `aeolak` | independent | 100% |
| `aeolak_alt` | independent | 100% |
| `alternox` | independent | 100% |
| `alternox_prime` | independent | 100% |
| `basmu` | independent | 100% |
| `battacor` | independent | 100% |
| `buzlok` | independent | 100% |
| `buzlok_beacon` | independent | 100% |
| `catabolyst` | independent | 100% |
| `cernos` | additive_with_base_damage | 50% |
| `cinta` | independent | 100% |
| `cinta_charged` | independent | 100% |
| `cyanex` | independent | 100% |
| `cyanex_burst` | independent | 100% |
| `daikyu_prime` | additive_with_base_damage | 50% |
| `drakgoon` | additive_with_base_damage | 57% |
| `epitaph` | independent | 100% |
| `epitaph_uncharged` | independent | 100% |
| `evensong` | additive_with_base_damage | 65% |
| `exergis` | independent | 100% |
| `fulmin_semi` | independent | 100% |
| `harpak_harpoon` | independent | 100% |
| `javlok` | independent | 100% |
| `lanka` | additive_with_base_damage | 38% |
| `laser_rifle` | independent | 100% |
| `mutalist_cernos` | additive_with_base_damage | 50% |
| `mutalist_cernos_uncharged` | independent | 100% |
| `nataruk_perfect` | independent | 100% |
| `paracyst_harpoon` | independent | 100% |
| `prime_laser_rifle` | independent | 100% |
| `quellor_alt` | independent | 100% |
| `rakta_cernos` | additive_with_base_damage | 50% |
| `seer` | independent | 100% |
| `sepulcrum` | independent | 100% |
| `sepulcrum_lockon` | independent | 100% |
| `sonicor` | inert | 100% |
| `stahlta` | independent | 100% |
| `stahlta_charged` | independent | 100% |
| `steflos` | independent | 100% |
| `tenet_diplos_lock_on` | independent | 100% |
| `tenet_envoy` | independent | 100% |
| `trumna_grenade` | independent | 100% |

**Seven AoE PARTS** were reading `takes_condition_overload: false` where the
catalog gives them their own row — not a fraction being off, the WHOLE term
missing from an explosion that is most of the weapon: the Ambassador's radial
(75%), both Ferroxes (350% / 333%), both Opticors (250% / 200%), the Trumna's
main fire (164%), and the Mutalist Cernos's charged cloud at **4100%**, which is
the most extreme relative column in the catalog. The per-part fraction is still
not expressible — `co_base_fraction` is one number per ENTRY — and each says so
on its card, which is the call the Pox has carried since its own 250% row.

### "CO-bonus does not use base damage increase Evolution" — eleven rows

This is the `co_base_excludes_this_evolution` flag, and it is a PERK's, not a
weapon's: the catalog names the tier and often the perk number, and lists only
DISCREPANT cases, so a tier-mate that also raises base damage feeds the CO term
in full unless it is named too.

Every row, with the perk the printed number identifies:

| weapon | row prints | perk | our fraction |
| --- | --- | --- | --- |
| Atomos | 100 or 124 (Evolution II) | both tier-2 options, each +24 | 0.8065 (catalog 81%) |
| Cestra (Normal) | 26 or 36 (Evolution II) | both, each +10 | — |
| Cestra (Incarnon) | 50 or 60 (Evolution II) | both, each +10 | 0.8333 (catalog 83.3%) |
| Despair | 60 or 120 (Evolution II **Perk 2**) | Stalker's Vendetta (+60); Fatal Affliction's +50 is NOT excluded | 0.5000 (catalog 50%) |
| Dual Toxocyst | 75 or 135 (Evolution II Perk 1) | Carnage Reign **and Fevered Frenzy** — see below | 0.5556 (catalog 56%) |
| Furis | 100 or 128 (Evolution II) | Haven Foray + Stormburst | 0.7812 (catalog 78%) |
| Lato Vandal | 152 or 174 (Evolution II **Perk 1**) | Haven Foray (+22) | 0.7755 — the ONE that does not reproduce, see that file |
| Lex Prime | 1200 or 1220 (Evolution II) | both, each +20 | 0.9836 (catalog 98%) |
| Vasto Prime | 420 or 564 (Evolution II **Perk 2**) | Deathtrap Trigger (+24 a pellet ×6 = the 144 printed) — see below | 0.7447 (catalog 74%) |
| Bronco Prime | 238 or 448 (Evolution II **Perk 1**) | Speeding Bullet, +30 a pellet x7 = the 210 printed | 0.5312 (catalog 53%) |
| Zylok Prime | 500 or 530 (Evolution II) | both, each +30 | 0.9434 (catalog 94%) |

**EXCLUDING AN EVOLUTION'S FLAT DAMAGE IS NOW THE DEFAULT**. This section used to say the opposite, and the
paragraph below is what it said. The reason it turned around is in the table's
own columns rather than in any one measurement:

**THE ELEVEN ROWS ABOVE ARE THE ONLY ONES THAT MEASURED AN EVOLVED WEAPON.**
They are the rows whose damage column prints a DOUBLE value — "100 or 124 (with
Evolution II)". Every other row prints one number, and that number is the
UNEVOLVED base, so a row reading `Torid | Main-fire | 100 | 100%` says the CO
bonus equals the base of a Torid with no evolution installed. That is true by
construction. It is not a statement that an evolution would feed the term, and
this repo read it as one.

On the question actually asked the score is **15 to 0**:

| | count | verdict |
|---|---|---|
| catalog rows that measured the evolved weapon | 11 | all EXCLUDED |
| owner measurements (Dual Toxocyst x2, Torid x2) | 4 | all EXCLUDED |
| anything, anywhere, measuring an evolved weapon and finding it INCLUDED | **0** | — |

Three of the four owner measurements are on perks the catalog does not list, so
its silence has been tested three times and meant "unmeasured" every time.

**SO THE DEFAULT FLIPPED — FOR `Adding` ENTRIES ONLY**. An
undeclared perk on an Adding entry keeps its flat damage out of the CO term. 238
weapon+perk pairs moved, by 37% on average at two Galvanized stacks against two
status types.

**`Multiplying` READS THE FULL EVOLVED BASE — MEASURED, and it is the OPPOSITE
answer** (MEASUREMENTS M51). The owner drew the line himself on the version of
this change that covered both classes, then ran the reading that settles it: the
Torid's base form, `Multiplying` where its Incarnon form is `Adding`, the same
two tier-2 perks. The CO multiplier came back 1.40 and 1.80 under BOTH the +51
and the +31 — identical, where a term reading the unevolved base prints 1.265
and 1.305. So "the rule may well be the same on both sides, since which base the
term reads sits upstream of how it combines" — which this file used to say — is
wrong. **The class decides which base the term reads.**

| class | the term reads | evidence |
|---|---|---|
| `Adding` | the UNEVOLVED base | 11 catalog rows + 4 owner readings, 15 to 0 |
| `Multiplying` | the FULL evolved base | M51, two attack parts x two perks |

**AND IT IS GENERALISED TO ALL 26 ENTRIES ON THAT ONE WEAPON'S READING**, deliberately ahead of this table: the wiki prints a fraction for a
minority of attacks, the rule beats the table, and a measurement that
contradicts it edits ONE weapon's yaml rather than the rule. The class now
answers BEFORE a perk's declaration on a `Multiplying` entry, which is what stops
a reading taken off an `Adding` form from reaching across a transform group and
diluting one. `no_evolution_dilutes_a_multiplying_co_base` asserts the property
roster-wide instead of the 26 numbers, so it holds for a weapon nobody has
entered yet; the reserved per-entry slot is `co_base_fraction:` in the weapon
yaml, 1.0 everywhere today.

**A DECLARATION STAYS THE PERK'S, AND IS SCOPED TO THE FORM IT WAS MEASURED ON.**
The catalog names perks, and a perk reaches both forms of its transform group —
whose CO classes can differ, the Torid's base form being `Multiplying` where its
Incarnon form is `Adding`. `co_base_excludes_only_form` is how a reading off one
entry is recorded without asserting the other.

**WHAT WOULD REVERSE IT** is one measurement finding an evolution that DOES feed
the term. `the_eleven_evolution_exclusion_rows_reproduce_their_own_percentages`
holds five unlisted perks asserting the new default, and that is the loop such a
measurement would edit. Until then the old paragraph stands as the record of
what was believed and why:

**INCLUDING an evolution's flat damage is the DEFAULT**;
the exclusion is opt-in per perk.

**AND THE DEFAULT FOLLOWS THE WIKI; A MEASUREMENT RE-CERTIFIES**. Flipping that default the other way was considered and refused:
it would have touched 107 perks across 65 weapons on the strength of ONE
measured weapon, and the repo's own rule forbids exactly that ("the catalog is
authoritative and absence means ORDINARY", "a row is transcribed for the entry
it names rather than generalised to a class"). What the Burston measurement
(M48) actually established is narrower and is now applied:

> **A fraction the catalog DERIVES belongs to the PERK, so it reaches every
> attack part the perk's damage landed on — not only the part the row names.**

The catalog has exactly three derived-fraction rows and they are now treated
alike: the Zylok family was already flagged, the Burston family was flagged on
the measurement, and the **Braton family** was flagged with them — its Daring
Reverie is the +4 the row's `70 + 4 = 74` names, and Munitions Grit is its
tier-2 twin. That last one needed no new measurement, only consistency: two of
the three were already done.

**AND THE ADOPTED RULE IS ON THE PAGE.** Every weapon's panel states which CO
rule it is computed under — behaviour, which attack parts, and the fraction
with its number — whether or not a CO source is equipped
(`scripts/check_gunco_stated.mjs`). The rules are per-weapon and transcribed by
hand, and the Burston's was wrong for months; putting it where a player who
owns the gun can read it is what makes the next one findable before it is
published rather than after. So every row above has to be flagged on the
perk it names, or that weapon computes its CO term on a base the game does not
use — for the Despair and the Bronco Prime, on twice it.

### The Kunai's two notes, neither of which the engine can say yet

> CO-bonus **DOES** use base damage increase Evolution; **does not factor +200%
> (3x) bonus vs <50% hp targets, effectively additive with it**

The first is the ordinary default here, so it needs nothing. The second does not
fit any bracket the engine has: Swift Conclusion's +200% below half health joins
the base-damage bucket, and under `Multiplying` the engine hands CO a base that
includes it — where the row says the two should ADD. Live since the perk was
implemented.

### A radial's fraction is DERIVED, not declared

The three radial rows print a "relative" figure that is not a discount at all:
it is an evolution raising the explosion's DAMAGE without raising the base its
CO term multiplies, which the engine already does (`RadialBase::co_base_fraction`,
set in `evolutions_data::apply`). Nothing is transcribed for these, and the
numbers come out:

| row | prints | engine |
| --- | --- | --- |
| Burston Prime | 55 / 13 / 24% | 13 + 42 = 55, fraction 13/55 = 23.6% |
| Braton Prime | 74 / 70 / 95% | 70 + 4 = 74, fraction 70/74 = 94.6% |

**The Zylok's row mixes its two variants**, which is why it is the one figure
here the engine does not reproduce and should not. It reads `776 || 700 || 90%`
with the note "Listed Values for Zylok Prime": 700 IS the Prime's radial, but
the +76 that makes 776 is the BASE Zylok's Precision's Payoff — the evolution
table prints that value per variant as X = 76 (Zylok) and X = 30 (Zylok Prime).
So 700/776 is one weapon's explosion under the other's perk. Each variant is
self-consistent here (Prime 700/730 = 95.9%, Zylok 600/676 = 88.8%) and the
per-variant evolution table is the more specific source. Pinned by
`the_zyloks_two_variants_each_carry_their_own_radial_co_base`.

**An AoE part needs its own row to take CO at all.** CO is a direct-hit bonus
everywhere else, which is why the engine's radial path refuses it by default and
the Torid's cloud is the declared exception. The Shedu's explosion has NO row,
so it takes none.

**The Attack Name cell scopes the row to ONE firing mode.** The spearguns' rows
read `Throw`, so they belong to the alt-fire entries and the primary-fire ones
have no row at all — and the Unmodded Damage cell is what tells the throw from
the explosion beside it (150 = 105 + 22.5 + 22.5, 200 = 140 + 30 + 30, neither
of them the 55 blast). Same shape as the Larkspur Prime's `Alt-fire` row. Absence
is still ordinary: `scourge` and `scourge_prime` are `additive_with_base_damage`.

**A weapon's two FORMS can differ.** The Torid's Incarnon form is `Adding` where
its base form is `Multiplying`, which is exactly the shape a refactor flattens —
both forms still "have CO". Pinned by
`the_torid_carries_both_of_its_co_catalog_rows`.

### The Notes column carries a MULTISHOT rule — `takes_multishot`

The CO table's last column is where the wiki records "AoE does not scale off
multishot", and it is the only place that rule is published at all. It is a
CO table, so nothing guarantees it lists every weapon the rule applies to — but
the same catalog discipline holds: **a row is transcribed for the entry it
names, and absence means ordinary** (a radial rides its projectile, so two
projectiles detonate twice, which is the engine's default).

Every row that carries it, and the reason they are the ones that do:

| row (weapon / attack) | in the roster | our entry |
| --- | --- | --- |
| Braton / Mk1 / Prime / Vandal — Incarnon Form Radial Attack | yes (×4) | `takes_multishot: false` — **MEASURED**, M41 |
| Burston / Prime — Incarnon Form Radial Attack | yes (×2) | `takes_multishot: false` |
| Zylok / Zylok Prime — Incarnon Form Radial Attack | yes (×2) | `takes_multishot: false` |
| Ambassador — Alt-fire **Hitscan** Radial Attack | no | — |
| Ferrox / Tenet Ferrox — **Hitscan** AoE Direct | no | — |
| Glaxion Vandal — Normal Attack **Hitscan** AoE | no | — |
| Opticor / Opticor Vandal — Charged **Hitscan** Radial Attack | no | — |
| Trumna — Main-fire **Hitscan** Radial Attack | no | — |
| Mausolon — Main-fire / Alt-fire Radial Attack ("Based on hitscan damage") | no | — |

The sentence means what it says, and it is about the GAME rather than about this
table's arithmetic: the Braton Prime's Incarnon form fires one explosion per
TRIGGER PULL at multishot 2.5, measured (M41). The arsenal is the half that
lies — it multiplies the AoE and the game does not deliver it.

**The discriminator is HITSCAN, not "beam".** Six of those rows say so in the
attack NAME, one says it in the note, and the three Incarnon families are all
forms the weapon page lists as Hit-Scan (Braton 5.67/s auto, Burston 20/s auto,
Zylok 0.6 s charge). Every AoE row WITHOUT the note belongs to an attack the
table's Projectile Type column calls Projectile — Laetum, Torid, Kompressa,
Latron, Miter, Phantasma, Larkspur, Shedu. The mechanism the split implies: a
hitscan shot has no projectile to carry an explosion, so the weapon spawns one
radial per trigger pull at the traced impact point and extra multishot traces
add nothing; a projectile weapon's extra rounds are separate objects that each
fly and detonate.

Note it is NOT the same rule as the beam one, which lives on the Multishot page
and is about a different thing: "Multishot has no effect on the spherical blast
radius of continuous weapons such as Ignis (Wraith), Glaxion Vandal, Gaze
Primary, Embolist, Catabolyst, and Cortege" — that is `BeamSpec::
radius_takes_multishot`, the sphere around a beam's contact point, and the extra
beams themselves still deal damage.

Two roster entries are hitscan with a radial and NO row, so they stay ordinary
until measured: the Stug's Incarnon blob (which the table calls a *Projectile*
under `Blob Impact`, and whose yaml carries a `projectile_speed_mps` — the
`shot_type: hit_scan` on that entry looks like the odd field, not the rule), and
nothing else.

### The damage column is a FREE CROSS-CHECK of the whole roster

"Attack Unmodded Damage" is the whole SHOT — every pellet of it — while a weapon
yaml carries the per-projectile damage and the pellet count separately. So
`base_vector.total() x base_multishot` has to reproduce it, and
`every_catalog_row_reproduces_our_shot_damage` asserts that for all 38 rows the
roster carries, with `every_catalog_radial_row_reproduces_our_explosion` for the
four radial ones.

That is not a CO check and it found a CO-unrelated bug on the first run: **both Bronco Incarnon entries had `multishot: 1.0` where the base
forms had 7**, so the Incarnon Bronco dealt ONE SEVENTH of its shot — 22 against
154, and 34 against 238 on the Prime. A lost pellet count is invisible
everywhere else: the damage per projectile stays right and the panel stays
plausible.

Every other multi-pellet entry was checked the same way and is correct — the
Boar pair, all four Struns, the Ballistica Prime and the Felarx have genuinely
single-projectile Incarnon forms, confirmed against each weapon's infobox.

### The exclusion rows check their OWN arithmetic

Each of the eleven prints two damage figures and a percentage — "100 or 124
(with Evolution II)" against "100% or 81%" — and the second percentage is
`unmodded/evolved`, which is exactly what `co_base_fraction` becomes when the
named perk is applied. So the row checks itself, and
`the_eleven_evolution_exclusion_rows_reproduce_their_own_percentages` fails if
the flag is missing, on the wrong perk, or on a perk whose flat damage does not
match. Six negative controls assert the unnamed tier-mates still feed CO in
full.

It found the **Vasto Prime** still missing its flag, which the
earlier sweep of eight had left behind.

### The Vasto Prime row: the damage column cannot pick the perk, and the row still can

This one is worth writing out, because the usual check does not decide it and
reading it as if it did is how the row was filed **UNRESOLVED** for a day.

Both EVO2 options give the Vasto Prime **+24**, and — checked against the raw
wikitext, not a summary of it — **both** carry the note *"Base Damage increase is
applied per pellet in Incarnon Form"*. The Incarnon form has 6 base multishot,
so either perk takes 420 to 420 + 6×24 = **564**. The damage column identifies
neither.

What identifies one is the row's own words, `(with Evolution II **Perk 2**)`, and
the page's ORDER: `Vasto_Incarnon_Genesis` lists **Lone Gun first and Deathtrap
Trigger second**, so Perk 2 is Deathtrap Trigger. That is the same convention the
Despair's row uses when it names Perk 2 and spells out that the tier-mate's +50
is *not* excluded — the catalog does distinguish tier-mates, and rows that mean
both say `(Evolution II)` with no number (Atomos, Cestra, Lex Prime, Zylok
Prime).

**It is worth a measurement anyway**, because the consequence is visible: on the
board's #1 Vasto Prime build (Galvanized Shot equipped) picking Lone Gun scores
**72.5** against Deathtrap Trigger's **64.8**, and swapping Galvanized Shot out
makes the two identical to the last digit. The whole 12% is this one flag. If a
player measures the pair in game and they match, the row means both perks and
`vasto_prime_lone_gun` needs the flag too — which is the negative control in
`the_eleven_evolution_exclusion_rows_reproduce_their_own_percentages`, so the
change is one line and a test that already names it.

---

### The MATH section's bullets are summaries of catalog rows, NOT laws — SETTLED

The page's **Math** section lists, of Additive-stacking CO:

> Damage multipliers or effects that are ignored with Additive Stacking CO-like
> bonuses: … Final Damage Multipliers … **Base Damage increases from Incarnon
> Genesis Evolutions.** … *Some* Melee Stance Multipliers … *Some* natural
> weapon stats modifiers, such as **Bow charging**.

Read as a law, the Incarnon bullet would mean every Adding weapon with a
flat-damage evolution excludes it — **107** (entry, perk) pairs in this roster,
against the eleven the catalog actually names.

**It is not a law**. Same ruling as the one this file
already states: the catalog is authoritative and absence means ORDINARY.

**And the page argues the owner's side.** Two of the four bullets are hedged
with "Some" outright, and the unhedged "Bow charging" one is enumerated by ~15
catalog rows that DISAGREE WITH EACH OTHER — Paris/Dread/Cernos at 50%, Miter at
40%, Lanka at 38%, Drakgoon at 57%, Evensong at 65%, the Ballisticas at 25% and
50% — and, decisively, **the class contains counter-examples**: the Cinta and
the Nataruk are charged bows at 100% Multiplying, and the Balefire Charger is
0%, "Does not apply". A bullet whose own named class holds exceptions in both
directions is describing what was observed, not stating a rule.

So the Incarnon bullet is the summary of the eleven rows, exactly as the bow
bullet is the summary of the charged-attack rows. Nothing in the engine changes,
and the negative controls in
`the_eleven_evolution_exclusion_rows_reproduce_their_own_percentages` are what
hold the line: a tier-mate the catalog does not name feeds CO in FULL even when
it raises base damage by the same number as its named sibling.

---

### OPEN QUESTIONS on this catalog — not decided, and worth a measurement

#### 1. The Lato Vandal exclusion row contradicts the catalog's own convention

Every other multi-pellet exclusion row is PER PELLET: the Bronco Prime's
238 -> 448 is exactly 7 x 30 and the Vasto Prime's 420 -> 564 is exactly 6 x 24.
The Lato Vandal's is 152 -> 174, i.e. its +22 landing ONCE on a 2-pellet shot;
per pellet it would be 196.

The engine treats a flat base-damage evolution as a BASE DAMAGE stat, which a
multishot weapon lists per projectile — so per pellet, which agrees with the
catalog on both other rows, and two cards say it outright ("Base Damage increase
is applied per pellet in Incarnon Form" — the Vasto's Lone Gun, the Soma's Fresh
Havoc). The row is treated as the outlier and the test carries our number with
the reasoning beside it.

#### 2. The wiki's SOURCES list is missing the Burston

The page's "Sources of Condition Overload-Style Bonuses" names nine Genesis
perks; the roster carries **sixteen** evolution CO grants and every one was
checked against a Genesis page. The two the list omits are
`burston_fatal_affliction` and `burston_prime_fatal_affliction`, and the Burston
Genesis page carries the row: *"+40% Direct Damage per Status Type affecting the
target."* Ours is right and the list is incomplete — recorded so the next audit
does not "fix" them away.

---

### Where this has already gone wrong

- **Zylok / Zylok Prime, 2026-08-11.** Both radials were filed as "no CO catalog
  row for this weapon" and so took the engine's defaults — CO off, multishot on
  — and BOTH were wrong. There is a row, and the reason it was missed is worth
  the line: the catalog keys it under `{{Weapon|Zylok}}/{{Weapon|Zylok Prime}}`,
  one row for two variants, so a search for either name alone finds a row that
  looks like it belongs to the other. The Burston and Braton rows are keyed the
  same way and were read correctly, which is what makes this a lookup habit
  rather than a one-off. Found while answering "why doesn't the Burston Prime's
  radial take multishot" — the question that made the column worth reading as a
  column.
- **Shedu, 2026-08-10.** Filed as `additive_with_base_damage` on the reasoning
  that it had no row. It has one, and it says Multiplying. The mistake was
  asserting an absence without opening the page — and the page's own Bugs
  section ("Galvanized Aptitude is multiplicative to base damage sources on
  direct hits") was describing the same behaviour the catalog classifies, since
  Galvanized Aptitude IS the CO bonus. Worth +41% on a status build.

---

## 2. Primary Compression — radius traded for damage

Wiki: [Primary Compression](https://wiki.warframe.com/w/Primary_Compression).
The arcane reads a weapon's **modded** blast radius, shrinks it to a fifth while
aiming, and pays for every metre lost.

```
radius_lost  = radius_modded × (1 − 0.2)      # continuous, NOT per whole metre
damage_bonus = damage_per_metre(rank) × radius_lost
ammo_eff     = eff_per_metre(rank)    × radius_lost
```

Rank ramp (wiki rank table == WFCD `levelStats`, both linear):

| rank | damage per metre | ammo efficiency per metre |
| --- | --- | --- |
| 0 | +50% | +3.0% |
| 5 | +100% | +5.5% |

*"Despite the description stating 'per meter lost,' the bonuses smoothly scale
between whole number radius values … a loss of 6.5 meters of radius gives +650%
Damage and +35.75% Ammo Efficiency."*

### Columns and LEGEND, verbatim

> Weapon | Attack Name | Compression Effectiveness | Stacking Behavior with
> Damage Bonuses | Radius Calculation | Base Radius | Max Damage Bonus @ Base
> Radius | Max Damage Bonus w/ Primed Firestorm | Notes | Class

The page's own legend, and it settles two columns that are easy to read wrong:

> **Compression Effectiveness:** Shows how much bigger/smaller the radius
> Compression considers compared to how much it should be considering. 100%
> means "intended" radius/damage calculation. >100% means better than expected.
> <100% means worse than expected.
>
> **Radius Calculation:** On weapons with multiple firing modes that have AoEs
> attached to them, Compression has to determine which radius to use on aiming:
> **Snapshot** = Uses the ads state when fired, not when AoE occurs.
> **Stolen** = Uses another firing mode's radius for the Compression bonus.
> **Doesn't Work** = Compression doesn't apply to this AoE.

Two things it settles, both easy to read wrong:

- **Effectiveness is about WHICH RADIUS is considered**, not how much of the
  bonus is paid. The arithmetic lands in the same place — the bonus scales with
  the radius given up — but the Vectis pair's 4% is not a discount on their
  damage, it is the arcane reading a 0.1 m embed radial instead of the headshot
  explosion, and the Trumna alt-fire's 127% is not a bonus multiplier, it is a
  radius counted twice over ("Merged").
- **`Radius Calculation` is a COLUMN, not a note.** `Snapshot` is its ordinary
  value, and the table also uses `Constant Check` (the Battacor), which the
  legend does not list.

And one column is a place to put an EXPLANATION rather than a number: `Max
Damage Bonus w/ Primed Firestorm` holds *"Primary Fire AoE not affected by
Firestorm"* on the Shedu and *"Shotguns cannot equip mod"* on every shotgun —
i.e. it says why that cell has no figure. The last column is the wiki's own
class taxonomy, which is why the Shedu reads `Arm-Cannon` rather than `Rifle`.

The Shedu's row in full, since it is the one the roster leans on:

| Weapon | Attack | Eff | Stacking | Radius calc | Base radius | Max @ base | w/ Primed Firestorm | Notes | Class |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Shedu | Primary Fire + AoE | 100% | Multiplies | Snapshot | 6.6 m | +528% | *Primary Fire AoE not affected by Firestorm* | *Cannot use reload pulse radial.* | Arm-Cannon |

### RE-READ 2026-08-19, when the catalog moved

`fetch_catalogs.mjs --force` reported Primary Compression as MOVED, which is the
one event that invalidates every row here, so all 28 transcribed weapons were
compared against the live table.

**The mechanics are intact.** The rank ramp is unchanged (0: +50% / +3.0%, 5:
+100% / +5.5%), every weapon this roster carries is still listed, and every
`effectiveness` value still matches. What changed on the page was its
ACQUISITION section — the arcane is now bought from Hunhow at Pontis Tower for
Emerald and Crimson Talent — which is nothing this repo reads.

**Two things came out of the re-read anyway**, and both are about the STACKING
column rather than about the numbers:

- Five rows carry `Doesn't Work` there (Arbucep, Cortege ×2, Kuva Ayanga, Torid
  Incarnon) and the schema holds only `multiplies` / `adds`. Those files' yaml
  said `multiplies`, which reads as a transcription and is not one. At 0%
  effectiveness the field is inert, so no number moves — but each now says in
  as many words that it is a placeholder and what the column really reads.
- **The Vectis pair's column reads `N/A`** and had never been transcribed at
  all. That one is NOT inert: its effectiveness is 4%, so `multiplies` against
  `adds` is a real difference, and `multiplies` is this repo's assumption rather
  than the catalog's answer. Both files now say so.

The lesson is the one the MOVED check exists for, and it is narrower than
"re-read everything": what drifts is not the numbers a formula reads, it is the
columns nobody is acting on — which is exactly why the rule says to transcribe
those too.

### SIX AXES, not one

This table is why the arcane is not a formula with a weapon stat in it. Every
one of these varies independently, and the first four cannot be derived from
anything the weapon's own data says:

1. **Stacking** — `Multiplies` is the common case, but a real minority `Adds`
   (Ambassador, Battacor, Ferrox, Opticor, Trumna, and every Braton/Burston
   Incarnon), and the Trumna's alt-fire is `Both`.
2. **Effectiveness**, i.e. which radius gets considered — mostly 100% or 0%,
   but the Vectis pair are **4%** and the Trumna's alt-fire is **127%**.
3. **Radius Calculation** — `Snapshot` / `Stolen` / `Doesn't Work`, plus the
   Battacor's `Constant Check`. It only bites on weapons with more than one
   AoE-bearing firing mode, which is most of the ones that matter.
4. **Which radius is even reduced.** Several weapons collect the bonus while
   paying nothing: the Torid's *"cloud radius is not reduced"*, the Alternox's
   pulse, Penta's napalm, the Simulor's singularity, Ferrox's pull.
5. **Radius mods may not apply at all** — the Shedu and both Trumnas.
6. **Multishot can change it.** The Simulor's effectiveness *decreases* with
   multishot (83%, 71%, 63%, 56%) and recovers with Primed Firestorm.

#### "Shotguns cannot equip" is about a MOD, not this arcane — CHECKED

The rendered table drops a word. The wikitext says **"Shotguns cannot equip
mod"**, and the row's last column is the weapon CLASS:

```
| {{Weapon|Corinth}} … || Alt-Fire + AoE || 100% || Multiplies || Snapshot
| 9.4 m (9.8 m)
| +752% (+784%)
| Shotguns cannot equip mod|| || Shotgun
```

The mod is the blast-radius one. Firestorm and Primed Firestorm are RIFLE mods
and Fulmination and Primed Fulmination are PISTOL mods; **there is no shotgun
blast-radius mod at all**. So a shotgun is stuck at its base radius and can
never reach the table's Primed Firestorm column — which is also quiet evidence
that the arcane reads the MODDED radius, since otherwise the note would say
nothing.

It is not an equip rule and there is nothing to implement: the engine already
answers it by construction, because a shotgun's pools are `[primary, shotgun]`
and the mod is in neither. Verified against `/api/meta`:

| weapon | class | blast-radius mods offered |
| --- | --- | --- |
| Phantasma Prime | Shotgun | — |
| Strun | Shotgun | — |
| Torid | Launcher | firestorm, primed_firestorm |
| Shedu | Rifle | firestorm, primed_firestorm |

*"Archguns cannot equip"* and *"Exalted weapon cannot equip Arcanes"* are the
same shape and are already true here for the same structural reason.

**The real per-weapon exception is the other one.** The Shedu IS offered
Firestorm — it is a rifle — and the mod does nothing to its explosion anyway
("Primary Fire AoE not affected by Firestorm", shared with both Trumnas). That
one the engine gets WRONG today, and it is recorded on the weapon's radial.

### Rows the ROSTER carries

Only ours, so this stays diffable. The full table lives on the wiki, and
`scripts/audit_weapon_stats.py` is not what checks it —
`the_roster_reproduces_primary_compressions_published_column` is, by
re-deriving the wiki's own Max Damage Bonus column from each entry's radius.

RE-READ 2026-08-20, when the roster finished its primary/secondary intake.
The published table named **fifty-nine** more of our attacks than we carried,
and an attack with no `compression:` pays the arcane NOTHING — so every one
of them was silently worth zero to a build holding Primary Compression. Half
the additions are a tested **0%** ("Archguns cannot equip", the beam
exclusion), which is a ROW and not an omission: saying so is the difference
between "checked" and "nobody looked".

103 rows.

| our entry | eff | base radius | max bonus | stacking | radius calc |
| --- | --- | --- | --- | --- | --- |
| `acceltra` | 100% | 4 m | +320% | Multiplies | snapshot |
| `acceltra_prime` | 100% | 5 m | +400% | Multiplies | snapshot |
| `aeolak_alt` | 100% | 7 m | +560% | Multiplies | snapshot |
| `afentis` | 100% | 3 m | +240% | Multiplies | snapshot |
| `afentis_prime` | 100% | 5.5 m | +440% | Multiplies | snapshot |
| `alternox_alt` | 100% | 6 m | +480% | Multiplies | snapshot |
| `alternox_prime_alt` | 100% | 6 m | +480% | Multiplies | snapshot |
| `ambassador_charged` | 100% | 6 m | +480% | Adds | snapshot |
| `arbucep` | 0% | 4 m | — | Doesn't Work | doesnt_work |
| `astilla` | 100% | 2.4 m | +192% | Multiplies | snapshot |
| `astilla_prime` | 100% | 2.4 m | +192% | Multiplies | snapshot |
| `basmu` | 100% | 1.7 m | +136% | Multiplies | snapshot |
| `battacor_charged` | 100% | 3.4 m | +272% | Adds | constant_check |
| `braton_incarnon` | 100% | 3 m | +240% | Adds | snapshot |
| `braton_prime_incarnon` | 100% | 3 m | +240% | Adds | snapshot |
| `braton_vandal_incarnon` | 100% | 3 m | +240% | Adds | snapshot |
| `bubonico_burst` | 100% | 7 m | +560% | Multiplies | snapshot |
| `burston_incarnon` | 100% | 2 m | +160% | Adds | snapshot |
| `burston_prime_incarnon` | 100% | 2 m | +160% | Adds | snapshot |
| `carmine_penta` | 100% | 4 m | +320% | Multiplies | snapshot |
| `cedo_alt` | 100% | 6 m | +480% | Multiplies | snapshot |
| `cedo_prime_alt` | 100% | 6 m | +480% | Multiplies | snapshot |
| `coda_bubonico_burst` | 100% | 7 m | +560% | Multiplies | snapshot |
| `coda_sporothrix` | 100% | 2 m | +160% | Multiplies | snapshot |
| `corinth_airburst` | 100% | 9.4 m | +752% | Multiplies | snapshot |
| `corinth_prime_airburst` | 100% | 9.8 m | +784% | Multiplies | snapshot |
| `cortege` | 0% | 0 m | — | Doesn't Work | doesnt_work |
| `cortege_alt` | 0% | 4 m | — | Doesn't Work | doesnt_work |
| `enkaus_alt` | 0% | 8 m | — | Doesn't Work | doesnt_work |
| `evensong` | 100% | 4 m | +320% | Multiplies | snapshot |
| `ferrox` | 100% | 3.6 m | +288% | Adds | snapshot |
| `ferrox_thrown` | 0% | 10 m | — | Doesn't Work | doesnt_work |
| `glaxion_vandal` | 0% | 2 m | — | Doesn't Work | doesnt_work |
| `gorgon_incarnon` | 100% | 5 m | +400% | Multiplies | snapshot |
| `gorgon_wraith_incarnon` | 100% | 5 m | +400% | Multiplies | snapshot |
| `grattler` | 0% | 9 m | — | Doesn't Work | doesnt_work |
| `ignis` | 0% | 3 m | — | Doesn't Work | doesnt_work |
| `ignis_wraith` | 0% | 3 m | — | Doesn't Work | doesnt_work |
| `javlok` | 100% | 2.4 m | +192% | Multiplies | snapshot |
| `javlok_throw` | 100% | 6 m | +480% | Multiplies | snapshot |
| `komorex` | 0% | 3.5 m | — | Doesn't Work | doesnt_work |
| `kuva_ayanga` | 0% | 6 m | — | Doesn't Work | doesnt_work |
| `kuva_bramma` | 100% | 8.3 m | +664% | Multiplies | snapshot |
| `kuva_chakkhurr` | 100% | 2.9 m | +232% | Multiplies | snapshot |
| `kuva_grattler` | 0% | 9 m | — | Doesn't Work | doesnt_work |
| `kuva_ogris` | 100% | 7.9 m | +632% | Multiplies | snapshot |
| `kuva_tonkor` | 100% | 7 m | +560% | Multiplies | snapshot |
| `kuva_zarr` | 100% | 7 m | +560% | Multiplies | snapshot |
| `larkspur_charged` | 0% | 9.6 m | — | Doesn't Work | doesnt_work |
| `larkspur_prime_charged` | 0% | 9.6 m | — | Doesn't Work | doesnt_work |
| `latron_incarnon` | 100% | 4 m | +320% | Multiplies | snapshot |
| `latron_prime_incarnon` | 100% | 4 m | +320% | Multiplies | snapshot |
| `lenz` | 100% | 7.2 m | +576% | Multiplies | snapshot |
| `mausolon` | 100% | 1.8 m | +144% | Adds | stolen |
| `mausolon_charged` | 100% | 8 m | +640% | Adds | snapshot |
| `miter_incarnon` | 100% | 3 m | +240% | Multiplies | snapshot |
| `mk1_braton_incarnon` | 100% | 3 m | +240% | Adds | snapshot |
| `morgha` | 0% | 3 m | — | Doesn't Work | doesnt_work |
| `morgha_alt` | 0% | 12 m | — | Doesn't Work | doesnt_work |
| `mutalist_cernos` | 0% | 2.5 m | — | Doesn't Work | doesnt_work |
| `mutalist_quanta_orb` | 100% | 4.4 m | +352% | Multiplies | snapshot |
| `ogris` | 100% | 7.1 m | +568% | Multiplies | snapshot |
| `opticor` | 100% | 6 m | +480% | Adds | snapshot |
| `opticor_quick` | 100% | 6 m | +480% | Adds | snapshot |
| `opticor_vandal` | 100% | 4.6 m | +368% | Adds | snapshot |
| `opticor_vandal_quick` | 100% | 4.6 m | +368% | Adds | snapshot |
| `panthera_prime` | 100% | 1.6 m | +128% | Multiplies | snapshot |
| `penta` | 100% | 4 m | +320% | Multiplies | snapshot |
| `phantasma_charged` | 100% | 4.8 m | +384% | Multiplies | snapshot |
| `phantasma_prime_charged` | 100% | 4.8 m | +384% | Multiplies | snapshot |
| `prisma_gorgon_incarnon` | 100% | 5 m | +400% | Multiplies | snapshot |
| `prisma_lenz` | 100% | 7.2 m | +576% | Multiplies | snapshot |
| `proboscis_cernos` | 100% | 7 m | +560% | Multiplies | snapshot |
| `quanta_cube` | 100% | 0.5 m | +40% | Multiplies | snapshot |
| `quanta_vandal_cube` | 100% | 0.5 m | +40% | Multiplies | snapshot |
| `scourge` | 100% | 1.7 m | +136% | Multiplies | snapshot |
| `scourge_prime` | 100% | 1.7 m | +136% | Multiplies | snapshot |
| `scourge_prime_thrown` | 100% | 7 m | +560% | Multiplies | snapshot |
| `scourge_thrown` | 100% | 7 m | +560% | Multiplies | snapshot |
| `secura_penta` | 100% | 6 m | +480% | Multiplies | snapshot |
| `shedu` | 100% | 6.6 m | +528% | Multiplies | snapshot |
| `simulor` | 100% | 5 m | +400% | Multiplies | snapshot |
| `sporothrix` | 100% | 1.7 m | +136% | Multiplies | snapshot |
| `stahlta_charged` | 0% | 7.2 m | — | Doesn't Work | doesnt_work |
| `strun_incarnon` | 100% | 4 m | +320% | Multiplies | snapshot |
| `strun_prime_incarnon` | 100% | 4 m | +320% | Multiplies | snapshot |
| `strun_wraith_incarnon` | 100% | 4 m | +320% | Multiplies | snapshot |
| `synoid_simulor` | 100% | 5 m | +400% | Multiplies | snapshot |
| `tenet_envoy` | 100% | 8 m | +640% | Multiplies | snapshot |
| `tenet_ferrox` | 100% | 4 m | +320% | Adds | snapshot |
| `tenet_ferrox_thrown` | 0% | 10 m | — | Doesn't Work | doesnt_work |
| `tenet_quanta_cube` | 100% | 0.5 m | +40% | Multiplies | snapshot |
| `tenet_tetra_grenade` | 100% | 8 m | +640% | Multiplies | snapshot |
| `tonkor` | 100% | 7 m | +560% | Multiplies | snapshot |
| `torid` | 100% | 3 m | +240% | Multiplies | snapshot |
| `torid_incarnon` | 0% | 2.3 m | — | Doesn't Work | doesnt_work |
| `trumna` | 100% | 1.6 m | +128% | Adds | snapshot |
| `trumna_prime` | 100% | 1.6 m | +128% | Adds | snapshot |
| `vadarya_prime` | 0% | 0 m | — | Doesn't Work | doesnt_work |
| `vectis_incarnon` | 4% | 6.7 m | +8% | Multiplies | snapshot |
| `vectis_prime_incarnon` | 4% | 6.7 m | +8% | Multiplies | snapshot |
| `zarr` | 100% | 4.9 m | +392% | Multiplies | snapshot |
| `zhuge_prime` | 100% | 2.6 m | +208% | Multiplies | snapshot |

### Status — MODELLED

The arcane brings two ramps PER METRE; the weapon brings the metres and the
bracket. They meet in one place and it is not the one you would guess:

- `loadout::resolve_for` computes `panel.compression` — how many metres THIS
  build gives up (modded radius × the row × 0.8), and whether the row `adds`.
  Aim-gated: the card says "on aim", so a Tenno who is not aiming gets `None`.
- `DummyParams::from_panel(panel, arena, arcane)` spends the arcane against
  those metres. It takes the arcane as an ARGUMENT rather than having it
  assigned afterwards, which is what makes the three sites agree — and it is one
  layer below `resolve_for` on purpose: **the optimizer resolves a panel once
  and pairs it with every arcane in the search**, so a panel that had already
  spent an arcane would have to be re-resolved per job.
- `adds` joins `arc_bd`, the live base-damage bracket — so Serration dilutes it.
  `multiplies` joins `arc_final` beside Secondary Surge — so Serration does not.
  `compression_pays_into_the_bracket_its_row_names` measures exactly that
  difference rather than either number on its own.

`the_roster_reproduces_primary_compressions_published_column` re-derives the
table's **Max Damage Bonus @ Base Radius** for all 26 rows from our own weapon
data. That column is not transcribed anywhere — it falls out of the radius, the
row and the rank ramp — so it is a cross-check: a radius typed wrong, an
effectiveness misread or an override invented breaks it.

What is still NOT modelled, and neither changes a number here:

- the radius REDUCTION itself. The panel keeps showing the full radius while
  the arcane is equipped, because this arena has no distance (docs/UNMODELLED.md)
  — every shot lands at point blank, so a fifth of a radius kills exactly as
  much as all of it.
- axis 6, MULTISHOT moving effectiveness (the Simulor). No roster weapon has
  that row.

---

## 3. Sniper Rifle — Minimum Combo and the zoom buffs

**Page:** [`Sniper Rifle`](https://wiki.warframe.com/w/Sniper_Rifle)
§"Zoom and Minimum Combo Stats". Cached as `vendor/wiki/sniper_rifle.wiki`.
**Fields:** `sniper_combo.min`, `sniper_combo.seconds`, `scope.magnification`,
`scope.headshot_damage`. The mechanic itself is MECHANICS §7 §"THE SNIPER
RIFLE"; this is the row.

A catalog by the same test the other two pass: the rule is one formula and the
numbers it needs exist nowhere in the weapon's own stats. Absence means the
weapon is **not a sniper rifle** rather than a sniper with default values —
there is no default Minimum Combo, and a gun with no scope has no zoom buff.

**The columns, verbatim:** Sniper Rifle | Zoom Level | Buff | Minimum Combo.
One weapon spans several Zoom Level rows and states Minimum Combo once.

**The rows the roster holds:**

| weapon | zoom levels and buffs | Minimum Combo |
|--------|-----------------------|---------------|
| Vectis | 3x +30% Headshot Damage · 4.5x +50% | 1 shot |
| Vectis Prime | 3.5x +40% Headshot Damage · 6x +60% | 5 shots |

Only the TOP level is declared (`ScopeSpec`): this arena has no field of view,
so nothing is traded for magnification and the highest level is free.

**What is NOT in the roster yet, and what it will need.** Three of the buff
kinds have no field. The table grants critical chance (Lanka 3x/5x/6x:
+20/+30/+40%) and critical multiplier as well as headshot damage, and the
mechanic page calls the Lanka's and the Komorex's out as exceptions to the
"additive with similar buffs from mods" rule. The Lanka also carries the only
non-2-second combo duration (6 s), which is why `sniper_combo.seconds` is a
field with a default rather than a constant. The Komorex's second zoom is not
a buff at all but a stat trade (+100% Damage, +3 m Explosion Radius, −75% Fire
Rate) — that one is not a `ScopeSpec` and should not be forced into it.

## Adding a catalog

A new one earns a section here when it has the same shape: a published table,
one row per weapon or per attack, saying something the weapon's stats do not.
Put the columns verbatim, the rows the roster actually carries, and — the part
that pays for itself — **where it has already gone wrong**.
