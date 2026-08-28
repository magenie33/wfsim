# Melee — seven ways to swing one weapon

The roster's first melee weapon landed on 2026-08-28: the **Magistar**, as seven
entries. This is what melee is, what it cost, and what is still owed.

---

## 1. THE ONE DECISION EVERYTHING ELSE FOLLOWS FROM

> **Each way of swinging a melee weapon is an independent BUILD, and therefore
> an independent board row** (owner, 2026-08-28).

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

**A mode id is the INPUT, never the combo's name.** A stance names its own
combos and a different hammer stance names them differently, so a name here
would bake one stance into a durable id that every saved preset, share link and
board row carries. The input is what does not change; the NAME is derived and
filled in from the stance, which is `check_mode_def.mjs`'s existing machinery
and costs no translation.

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
is why THAT one travels in a field of its own (AGENTS.md, 2026-08-25) and this
one rides `mods`, appended. Nothing about the share link, the board record, the
worker's table or `builds::identity` had to change.

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

## 5. WHAT IS STILL OWED

Each of these is on the page, in both languages, on the entry or the card it
applies to.

1. **Tennokai.** 15% on a direct hit for a 2 s window in which a heavy attack is
   free and faster. Its seven cards ARE in the pool — they are the whole melee
   exilus slot — and each says the window is not modelled. Until it lands, the
   melee exilus slot pays nothing at all, which is why it is worth doing next:
   it is the only thing that would make that slot a decision.
2. **Crit chance mods double on heavy attacks** — True Steel, Sacrificial Steel
   and Galvanized Steel all say `(x2 for Heavy Attacks)` on their own cards.
   The two heavy modes read LOW until this lands.
3. **Condition Overload's slam exemption** — it does not apply to slams, heavy
   slams or radial attacks, and this engine does not exempt it, so the heavy
   slam mode reads HIGH with it equipped.
4. **Melee rivens.** No pool has been surveyed; a melee card rolls Range, Attack
   Speed, Combo Duration and Heavy Attack Efficiency, none of which any existing
   riven pool contains.
5. **Power Spike's partial combo decay** — a Warframe passive, so the counter
   here drops to zero where a real build keeps most of it.
6. **A swing's own animation length.** The module publishes a combo's DURATION
   and not a per-swing split, so the swings share it evenly. It moves a status
   tick's start by fractions of a second and moves no total.
7. **`Sweep` and `Thrust` are one shape here** — the forward half-plane. A
   sweep is a wide arc and a thrust is not, and nothing published gives either
   an angle.
8. **A stance's capacity.** In game a stance GRANTS capacity and this engine's
   drain is a `u32`, so it is held at zero — the conservative direction: a build
   that fits here fits in game.
9. **Six cards that name a state this arena has not got**: Enduring Strike and
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
- **Where the stance multiplier sits relative to QUANTIZATION.** It is folded
  into the swing's base here, which makes the snap grid scale with it. That is
  the reading with an argument behind it (DE publishes a damage figure per combo
  attack) and it is the harmless one, since quantizing `kX` against `ks` is `k`
  times quantizing `X` against `s`. Unmeasured either way.

---

## 6. THE INCARNON, WHICH IS NOT A FORM

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

## 7. WHAT MELEE COSTS FROM HERE

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
