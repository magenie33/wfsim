# The per-weapon CATALOGS — tables the wiki publishes and the engine must carry entry by entry

Some mechanics are not a formula plus a weapon stat. They are a formula plus a
**published table with one row per weapon**, where the row can say a thing the
weapon's own numbers never would — that this weapon's bonus multiplies where
everyone else's adds, that this attack part is exempt, that this one does not
work at all.

Those rows are DATA, and the rule for them is the one the roster already
follows for Condition Overload (owner, 2026-07-30): **the catalog is
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

### THE RULE, stated once (owner, 2026-08-12)

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
   here. It is not a law (owner, 2026-08-12) — see the section below for why
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
| Dual Toxocyst | 75 or 135 (Evolution II Perk 1) | Carnage Reign | 0.5556 (catalog 56%) |
| Furis | 100 or 128 (Evolution II) | Haven Foray + Stormburst | 0.7812 (catalog 78%) |
| Lato Vandal | 152 or 174 (Evolution II **Perk 1**) | Haven Foray (+22) | 0.7755 — the ONE that does not reproduce, see that file |
| Lex Prime | 1200 or 1220 (Evolution II) | both, each +20 | 0.9836 (catalog 98%) |
| Vasto Prime | 420 or 564 (Evolution II **Perk 2**) | Deathtrap Trigger (+24 a pellet ×6 = the 144 printed) — see below | 0.7447 (catalog 74%) |
| Bronco Prime | 238 or 448 (Evolution II **Perk 1**) | Speeding Bullet, +30 a pellet x7 = the 210 printed | 0.5312 (catalog 53%) |
| Zylok Prime | 500 or 530 (Evolution II) | both, each +30 | 0.9434 (catalog 94%) |

**INCLUDING an evolution's flat damage is the DEFAULT** (owner, 2026-07-30);
the exclusion is opt-in per perk.

**AND THE DEFAULT FOLLOWS THE WIKI; A MEASUREMENT RE-CERTIFIES** (owner,
2026-08-16). Flipping that default the other way was considered and refused:
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
implemented (2026-08-12).

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

That is not a CO check and it found a CO-unrelated bug on the first run
(2026-08-12): **both Bronco Incarnon entries had `multishot: 1.0` where the base
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

It found the **Vasto Prime** still missing its flag (2026-08-12), which the
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

**It is not a law** (owner, 2026-08-12). Same ruling as the one this file
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
and the mod is in neither. Verified against `/api/meta` (2026-08-11):

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

Only ours, so this stays diffable. The full table lives on the wiki.

| our entry | eff | base radius | max bonus | stacking | note |
| --- | --- | --- | --- | --- | --- |
| Shedu | 100% | 6.6 m | +528% | Multiplies | AoE **not affected by Firestorm**; cannot use the reload pulse radial |
| Torid — Toxin Cloud | 100% | 3.0 m | +240% | Multiplies | cloud radius **not reduced** — pays nothing, collects everything |
| Torid — Incarnon | 0% | 2.3 m | — | Doesn't Work | the continuous-beam exclusion |
| Braton / Prime / Vandal / Mk1 — Incarnon | 100% | 3.0 m | +240% | **Adds** | |
| Burston / Prime — Incarnon | 100% | 2.0 m | +160% | **Adds** | |
| Gorgon / Prisma / Wraith — Incarnon | 100% | 5.0 m | +400% | Multiplies | |
| Latron / Prime — Incarnon | 100% | 4.0 m | +320% | Multiplies | |
| Miter — charged shot | 100% | 0.2 m | +16% | Multiplies | "wide projectile, not traditional AoE" |
| Miter — Incarnon | 100% | 3.0 m | +240% | Multiplies | |
| Strun / Prime / Wraith — Incarnon | 100% | 4.0 m | +320% | Multiplies | "Shotguns cannot equip" |
| Phantasma / Prime — Alt-Fire | 100% | 4.8 m | +384% | Multiplies | "Shotguns cannot equip"; bomblets do not benefit |
| Vectis / Prime — Incarnon | **4%** | 0.1 m | +8% | N/A | uses the embed radial, not the headshot explosion |
| Scourge (Prime) — Primary Fire + AoE | 100% | 1.7 m | +136% | Multiplies | Speargun |
| Scourge (Prime) — **Throw + AoE** | 100% | 7.0 m | +560% | Multiplies | Speargun; the biggest radius the pair has |
| Larkspur Prime | Untested / 0% | 9.6 m | — | — | "Archguns cannot equip" |

**The spearguns are TWO ROWS FOR ONE WEAPON**, split by firing mode the way §1's
CO rows are, and the Weapon cell reads `Scourge (Scourge Prime)` — one row that
names both variants, which is the opposite of the CO rule's usual bite and only
safe because the cell says so. Ordinary in every column, so the whole finding is
the pair of radii: 1.7 m on the primary fire against 7.0 m on the throw makes the
arcane worth **four times as much** on the alt-fire, on a weapon where both modes
are one build. The Primed Firestorm column is the same arithmetic at ×1.44
(2.448 m → +195.84%, 10.08 m → +806.4%), which is a second check on the radii.

**General exclusion, verbatim:** *"Does not work on Continuous Weapons or beam
attacks with an AoE component. For example, Ignis or Torid Incarnon Genesis."*

### Status — MODELLED (2026-08-11)

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
