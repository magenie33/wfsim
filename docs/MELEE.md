# Melee — seven ways to swing one weapon

The roster's first melee weapon landed on 2026-08-28: the **Magistar**, as seven
entries. This is what melee is, what it cost, and what is still owed.

---

## 1. THE ONE DECISION EVERYTHING ELSE FOLLOWS FROM

> **Each way of swinging a melee weapon is an independent BUILD, and therefore
> an independent board row**.

A melee player picks one loop and runs it for the whole engagement. That is the
definition of a MODE in this repo — `WeaponPlayMode::sustainable` asks exactly
"can you be in it for three hundred seconds" — so the four stance combos, the
heavy attack, the slide and the heavy slam are seven `FormKind`s and seven
modes, not seven animations of one.

It was proposed here first as a new axis called a "rotation", on the reasoning
that a player mixes attacks inside one fight. The owner corrected it: they do
not. Melee therefore needed **no new build axis at all** — `mode` already means
this — which took the largest piece of speculative work off the plan.

The count is not a problem to be managed. Seven rows per weapon per ruler is
seven different builds being ranked, which is what the board is for.

| mode id | input | Crushing Ruin's name | the swings |
| --- | --- | --- | --- |
| `base` | stationary | Raging Whirlwind | 400% (forced Impact) -> 200% (360deg) -> 300% (360deg) -> 500% (forced Knockdown) |
| `forward` | forward | Tidal Force | 100% -> 100% (360deg) -> 100% (360deg) -> 200% -> 300% |
| `block` | block | Winding Temper | 300% -> 200% -> 400% |
| `block_forward` | block + forward | Shattered Village | 300% -> 2x50% (360deg) -> 300% -> 2x50% (360deg) -> 400% (360deg, Impact) -> 500% (Knockdown) |
| `heavy` | heavy | Crowd Fall | 600% (Lifted) -> 600% (Knockdown) |
| `slide` | slide | Hell's Wave | 200% (360deg, forced Impact) |
| `heavy_slam` | heavy, airborne | — | 630 Blast, 10 m sphere |

**A MODE'S NAME IS FIXED AND ITS STRENGTH IS NOT**. The id
is the INPUT — `neutral`, `block_forward` — and so is what a reader sees;
swapping the stance changes what `neutral` is WORTH and never what it is called.

That was briefly the other way round: the label was derived from the equipped
stance, so the same mode read "Raging Whirlwind" under Crushing Ruin and
"Falling Rock" under Shattering Storm. It is worse, and for the one question a
stance slot exists to answer — *which stance is best for the neutral combo* —
cannot be asked at all if the two builds call that mode different things. One
vocabulary, seven entries, and the numbers underneath them move.

The combo's own name stays in a comment above each block in the stance file,
where it is documentation for whoever transcribes the next stance rather than a
label the app draws.

---

## 2. THE MECHANICS, AND WHERE EACH ONE CAME FROM

Every line below is quoted from `wiki.warframe.com` and cross-checked against
WFCD's export where the export carries the field.

### The combo counter

- **Points, not hits.** *"Stance attacks add combo points, scaling with the
  attack's stance damage multiplier (100% stance damage multiplier = 1 point)"*
  — so a 400% swing is worth 4, and one number does both jobs.
- **Per body landed.** The wiki's own reading of the Rauta: *"generates 2 combo
  points per pellet landing on enemy (max 28 points across 14 pellets)"*.
- **The ladder** is `1 + floor(points / 20)` capped at 12: 2x at 20, one more
  every 20, 12x at 220. Venka Prime's 13x and Dex Nikana's shortened 110 are the
  two exceptions the page names, and neither is in the roster.
- **Five seconds**, refreshed by any landed swing.

### …AND IT DOES NOT MULTIPLY A NORMAL SWING

> *"**Melee Combo Multiplier does not multiply the damage of your normal
> attacks**. Instead, you can spend Melee Combo Count to perform Heavy Attacks,
> which deals between **2x** and **12x** damage."*

This is the single most counter-intuitive fact in melee and it was verified
twice, from the raw wikitext and from the rendered page. It is what makes the
seven modes genuinely different builds rather than seven damage numbers:

- in a **combo** mode the counter climbs and pays through **Blood Rush**
  (crit chance) and **Weeping Wounds** (status chance), both of which read it
  and never spend it;
- in a **heavy** mode the swing that reads the counter is the swing that empties
  it, so those two cards are worth almost nothing and **initial combo** is worth
  everything.

Both cards enter brackets this engine already had — the wiki writes the formula
that way itself:

```
Crit Chance   = Weapon Crit Chance   x [1 + Mod Crit Bonus   + BloodRush     x (Combo - 1)] + Static
Status Chance = Weapon Status Chance x [1 + Mod Status Bonus + WeepingWounds x (Combo - 1)]
```

### Initial combo is a FLOOR that refills

> *"Heavy attacks spend initial combo, which regenerates at a rate of 40 combo
> points per second."*

It is the whole of the pure-heavy build. The Magistar's Incarnon Form carries
**+30**, which is back inside **0.75 s** against a **0.8 s** wind-up (1.2 s
reduced by the form's own +50% wind-up speed) — so every heavy attack lands at
**2x** rather than 1x. `melee_combo_points` takes the higher of what was earned
and what has refilled, which is what makes "spend it and it comes back" and
"build on top of it" one number.

### Follow Through

> *"Proportion of weapon damage = FT^(n-1)"*, and *"Follow Through does not
> affect: (Heavy) Slam Attacks, Any attack that shoots projectiles or deals
> AoE."* A Hammer is **0.4**.

### Slams

- 2x a normal attack, **3x for a heavy slam**.
- *"linearly diminishing with distance from the point of impact to 50% (70% for
  heavy slam) at the edge of its radius."* WFCD's `attacks[]` array agrees
  exactly: `falloff {start: 0, end: 10, reduction: 0.3}`.
- The sphere is centred on the **wielder's own feet** — `BlastKind::Slam`, its
  own variant beside `Contact` and `Terminal` because it answers the same
  question (where is the epicentre) and a third variant keeps that a total
  function. It is why a slam ignores the 2.5 m reach that decides every other
  melee mode.

### Condition Overload (melee)

`Total = Base x [1 + Damage Mods + (CO x n)] x (1 + Elemental Mods)`, +80% per
type at rank 5, additive with Pressure Point — which is `co_behavior`'s existing
`additive_with_base_damage`, unchanged. Two of its own rules are **not modelled
yet** and are declared on the entries: it does not apply to slams or radial
attacks, and the status a hit applies does not amplify that same hit.

---

## 3. THE STANCE SLOT — the first mod that changes what a weapon FIRES

Every other card in this app changes what a weapon fires WITH. A stance
publishes a combo per FORM, and installing one replaces the weapon entry's own
script — so the same Magistar in the same mode is a different sequence of swings
under Crushing Ruin (Raging Whirlwind: 400/200/300/500 over 3.00 s) and under
Shattering Storm (Falling Rock: 400/300/400/200 over 3.03 s), measured in the
shipping build at 1,275 and 1,162 DPS.

**IT NEEDS NO FIELD OF ITS OWN ON THE WIRE**, and that is what made it cheap. A
stance mod is legal in the stance slot and NOWHERE else, so a flat mod list can
say which entry is the stance by looking at it. That is exactly what the exilus
slot could NOT do — an exilus-eligible mod is legal in a main slot too — which
is why THAT one travels in a field of its own (AGENTS.md) and this
one rides `mods`, appended. Nothing about the share link, the board record, the
worker's table or `builds::identity` had to change.

**A TENNOKAI HEAVY BREAKS THE CHAIN**, so the next light swing starts the combo
over. The wiki says nothing about a stance chain's position
— asked directly, and the page is silent on what advances it and what resets it
— so this is his answer and is recorded as one rather than derived.

It decides which swings ever happen, which is why it could not be left to
whichever behaviour fell out of the code. Raging Whirlwind is
`400 / 200 / 300 / 500`, and a chain that restarts on every window fires the
opener again and again. The sharp case is Discipline's Merit: it opens the
window every FOUR hits, which is exactly that combo's length, so the 500%
finisher is never reached at all. `swing_idx += 1` against `swing_idx = 0` was
the whole difference.

The slot is drawn on a melee weapon and on nothing else, the picker's filter
runs BOTH ways (a stance is refused from the eight, and only a stance is offered
in the tenth), and `builds::validate_with` does not count it against
`MAIN_SLOTS` — a melee build carrying one is `8 + 1` in the same list.

**THE COMBOS COME FROM `Module:Stances/data`**, the wiki's own Lua table, which
publishes `Dmg`, `Hits`, `Procs`, `Types`, `ImpactMultiplier` and `Duration` per
combo. It was found only after the first transcription, and it corrected three
things: a swing that lands TWICE (`Hits = { 1, 2 }`), a bonus to the Impact
component alone (`ImpactMultiplier = { 1.5 }`, which is a different thing from
the forced Knockback proc several of the same swings also carry), and the SLAM
three of Crushing Ruin's four combos end on. It also confirmed the durations
that had been DERIVED from the rendered table's two columns — 3.00 / 2.60 / 2.25
/ 4.25 — exactly, which is what makes the derivation trustworthy for Shattering
Storm, whose entry is past the point the module fetch truncates at.

The module's whole vocabulary, for whoever transcribes the next stance:

- `Types`: `360` (a spin, reaching everything in range), `Sweep`, `Thrust`,
  `Slam`, `Ranged`. Two are modelled — `360` and the slam — and `Sweep`,
  `Thrust` and the empty string all become the forward half-plane, which is the
  one invented number in the model and is declared.
- `Procs`: `Knockback` (Impact's own), `Bleed` (Slash's), `Puncture`,
  `Knockdown`, `Lifted`, plus `Ragdoll`, `Stun`, `Impair`, `Stagger`,
  `Finisher`, `Detonate` — all crowd control with no damage payload here.

---

## 4. WHAT THE ENGINE GREW

| piece | where |
| --- | --- |
| seven melee `FormKind`s, each its own mode | `weapons_data::FormKind`, `play_modes` |
| the combo script — a swing with its own multiplier, delay, wind-up, hit count, Impact bonus, 360deg flag, forced procs and trailing slam | `weapons_data::ComboHit` |
| a STANCE as a mod that supplies those scripts | `loadout::ModDef::stance`, `resolve` |
| the combo counter, its ladder and its refilling floor | `dummy::melee_combo_multiplier`, `melee_combo_points` |
| Blood Rush / Weeping Wounds | one `(combo - 1)` term in each of two existing brackets |
| Follow Through, `FT^(n-1)` over the bodies a swing reached | `spread_from_follow_through`, `Origin::FollowThrough` |
| a slam's epicentre at the wielder's own feet | `BlastKind::Slam` |
| the heavy wind-up as its own clock | `ComboHit::windup_seconds`, `ModEffect::HeavyWindUpSpeed` |
| Knockdown as a real status | `dummy::DebuffState::knockdown` |
| melee has no ammo, aims at nothing, puts nothing on a head | `scenario::Capability` |
| eleven mod effect kinds | crit/status per combo, combo duration as seconds and as a multiplier, initial combo, heavy efficiency, heavy damage, slam damage, melee reach in metres, combo count chance, wind-up speed, crit chance on a slide |
| six evolution effect kinds | relative base damage, initial combo, melee reach, follow through, slam radius, wind-up speed, proc conversion |

**Nothing else moved.** Every new field is empty or zero on a gun, and all 824
engine tests — the golden values among them — are unchanged.

---

## 5. THE ARCANE SLOT, AND ONE BUG THE AUDIT FOUND

A melee weapon seats a MELEE arcane, and `arcane_pools` already answered
`["melee"]` before there was a pool behind it. All twelve are in now. **Two of
them pay and ten declare**, which is an honest ratio for a family whose triggers
are a Warframe casting, a Warframe's shields breaking, a roll, a finisher and a
knockdown:

- **Melee Retaliation** reads a PLAYER stat — `+30% Melee Damage per 200 current
  Shields` — the way Primary Bulwark reads armour, through `tenno_scaled`. The
  neutral Tenno has zero shields, so it pays nothing and the panel says why:
  *"this Warframe has 0 shields — 0 whole steps of 200, so it pays 0%"*. That is
  the honest answer rather than a broken gate, the same reading Secondary
  Kinship gets in a solo fight.
- **Melee Duplicate** is the biggest declared gap in the pool: *"On Base
  Critical Hits: 100% chance for your attack to strike a second time"* — an
  extra hit in exactly the sense `docs/EXTRA_HIT.md` means, and worth a second
  copy of every critical swing. The engine's extra-hit machinery fires from a
  PERCENTAGE and an element; this one repeats the instance off a crit roll,
  which is a trigger nothing here has. Approximating it would be most of the
  weapon.
- **Melee Influence** is the meta card and it is entirely a CROWD effect: an
  Electricity status spreads this weapon's elemental statuses to everything
  within 20 m. Against one target it is worth nothing, which is why the arena
  that would show its value is the group ruler.

### …and the bug

**Condition Overload paid EXACTLY ZERO in all seven melee modes**, and the audit
caught it. Melee's Condition Overload is the ORIGINAL one and is unconditional —
no kill, no stacks, no clock, just the target's status count read on every swing
— and it was routed through the Galvanized family's path, which earns the same
PAYLOAD on a kill and therefore opens at zero stacks. It waited for a trigger it
does not have, for the whole fight, on the single most important card in the
melee pool.

`starts_full` is the fix, and it is NOT derived from `duration == NO_TIMEOUT`,
which would have been the cheap test and is wrong: locking a buff card writes
exactly that duration, and locking *"removes the expiry and nothing else — the
count still starts where the card sets it"*.

The three answers are now three different numbers, which is the assertion:
**x2.29** in the light combo, **x1.44** in the pure heavy, and **x1.0000** in
the heavy slam — the last one because *"this damage does not apply to Slams,
Heavy Slams, or Radial Attack explosions"*, which falls out of
`takes_condition_overload` defaulting to false on an explosion rather than being
arranged.

**And a crit card that says `(x2 for Heavy Attacks)` is doubled there and
nowhere else.** True Steel, Sacrificial Steel and Galvanized Steel all print it;
Blood Rush sits in the same bracket and prints nothing of the kind, so the CARD
carries the rule rather than the bucket. `20 x (1 + 1.20)` is 44% on a swing and
`20 x (1 + 2.40)` is 68% on a heavy, asserted as a pair.

---

## 6. WHAT IS STILL OWED

Each of these is on the page, in both languages, on the entry or the card it
applies to.

1. **Melee Duplicate**, and the eight other arcanes whose triggers this arena
   has not got — see §5.
2. **Melee rivens.** No pool has been surveyed; a melee card rolls Range, Attack
   Speed, Combo Duration and Heavy Attack Efficiency, none of which any existing
   riven pool contains.
3. **Power Spike's partial combo decay** — a Warframe passive, so the counter
   here drops to zero where a real build keeps most of it.
4. **A swing's own animation length.** The module publishes a combo's DURATION
   and not a per-swing split, so the swings share it evenly. It moves a status
   tick's start by fractions of a second and moves no total.
5. **`Sweep` and `Thrust` are one shape here** — the forward half-plane. A
   sweep is a wide arc and a thrust is not, and nothing published gives either
   an angle.
6. **A stance's capacity.** In game a stance GRANTS capacity and this engine's
   drain is a `u32`, so it is held at zero — the conservative direction: a build
   that fits here fits in game.
7. **Six cards that name a state this arena has not got**: Enduring Strike and
   Enduring Affliction want "the target is Lifted", Relentless Combination wants
   a combo point when a Slash DoT ticks, Spring-Loaded Blade wants a stacking
   reach buff, Galvanized Reflex wants stacking initial combo, and Shattering
   Impact wants a flat armour strip per Impact hit.

### …and two numbers that need measuring

- **`KNOCKDOWN_SECONDS` is a stand-in.** DE publishes no duration and the wiki
  flags its own table as under-researched. It stands at 1.0 s because `Lifted`
  does. Every slam forces a knockdown, so on a slam build this decides whether
  Condition Overload reads one more type between slams — 80% of a base-damage
  bucket either way.
- **`TENNOKAI_WINDUP_SPEED` is the other.** *"Performing a Heavy Attack or
  Heavy Slam during this flash increases its Wind Up Speed"*, and DE publishes
  no figure. It stands at +100% (half the charge) and is bounded either way: at
  0% the window is still worth a free heavy attack, and at +100% it is worth
  that plus a fifth of a second.
- **Where the stance multiplier sits relative to QUANTIZATION.** It is folded
  into the swing's base here, which makes the snap grid scale with it. That is
  the reading with an argument behind it (DE publishes a damage figure per combo
  attack) and it is the harmless one, since quantizing `kX` against `ks` is `k`
  times quantizing `X` against `s`. Unmeasured either way.

---

## 7. THE INCARNON, WHICH IS NOT A FORM

The Magistar's Genesis is **not** modelled as an Incarnon form and must not be:

> *"Reach **6x** Combo and then Heavy Attack to activate Incarnon Form"*,
> lasting **180 seconds**, persisting through holstering and removed only on
> death.

There is no gauge to spend, no way back, and the duration is the whole
engagement. Every evolution it grants is a stat — EVO1 is `+100% Melee Damage,
+30 Initial Combo, +50% Heavy Attack Wind Up Speed` — so it is a **triggered
buff**, the shape `on_reload_bd` and the Ocucor's tendrils already have. It is
NOT yet in the roster; when it lands it is a buff with an entry condition, and
the one thing to confirm in game first is that the form really does only change
numbers rather than swapping in a new set of attacks.

---

## 7b. HOW A MODE EXPLAINS ITSELF, AND WHY IT GOT SHORTER

Seven modes broke the mode-def block twice on the same day, in opposite
directions.

**It listed them all, and that was a wall.** The block was written to explain
every mode a weapon has, which was right at two or three
and is seven paragraphs above the build at seven — six of them about something
the reader did not pick. It draws the PICKED one now; comparison is what the
board does, one row per mode.

**Every line had to earn its place**. Three of the four
sentences a melee mode printed said nothing:

| line | why it went |
| --- | --- |
| "Swung as its Neutral Combo for the whole engagement." | restates the heading, which already reads *Neutral Combo* |
| "Nothing is spent to be in it, so it can be held forever." | true of all seven |
| "— a ruler ranks it." | true of all seven |

What replaced them is per-mode by construction: how many of its swings are
**spins** (a spin reaches the whole room and a sweep reaches one body — the one
spatial fact separating two combos of the same weapon), whether it **spends the
combo counter** (which makes it a different BUILD, since Blood Rush and Weeping
Wounds read the counter it empties), whether its damage is a **slam** the
weapon's reach does not bound, and what it **forces** whatever the roll says.

**The three numbers are in one unit, and two conversions get them there.** A
combo script's multipliers are relative to the ENTRY they are written in, and
the explosion is not in the script at all. `magistar_heavy_slam` states
`damage: { impact: 0.0 }` — the whole attack is its 630 Blast `radial:` — so a
summary counting swing multipliers reported **100% of base** for an attack that
deals **300%**, and naively adding the radial gave 400%. `swing_share`
(this entry's vector over the weapon's, 1.0 everywhere but the slam) and
`radial_share` are both on the FORM rather than in `combo_summary`, because
`stance_combos` has no weapon entry behind it: anything read off the entry would
vanish the moment a stance went in the slot.

---

## 8. WHAT MELEE COSTS FROM HERE

The Magistar paid for nearly all of the machinery. The second melee weapon is a
`data/weapons/melee/<id>.yaml` per mode — about 40 lines each, every number from
the wiki's infobox and its stance's published table — plus one line in
`assets.yaml` and one in `data/i18n/zh/names.yaml`.

**Two traps, both hit on the way in.** The export carries `Beginner /
Intermediate / Expert` internal tiers of the same mod under one display name:
joining by NAME put a phantom Pressure Point (+200%, rank 10) in front of this
work, and the bare path is the player's card. And WFCD's top-level `damage`
object has the Magistar's Puncture and Slash swapped, where its own
`damagePerShot` array, its own `attacks[]` block and the wiki all agree with
each other. Join by `uniqueName`, and let the wiki settle it.
